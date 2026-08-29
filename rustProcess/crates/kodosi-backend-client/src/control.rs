use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, RwLock},
};

use aws_lc_rs::signature::{ML_DSA_65, VerificationAlgorithm};
use kodosi_domain::permissions::SessionCapabilities;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlTrustEntry {
    pub signing_key: Vec<u8>,
    pub capabilities: SessionCapabilities,
}

impl ControlTrustEntry {
    #[must_use]
    pub const fn new(signing_key: Vec<u8>, capabilities: SessionCapabilities) -> Self {
        Self {
            signing_key,
            capabilities,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlAuthorization {
    Authorized,
    Untrusted,
    Forbidden { granted: SessionCapabilities },
}

#[derive(Debug, Clone, Default)]
pub struct ControlTrustStore {
    inner: Arc<RwLock<HashMap<(String, String), ControlTrustEntry>>>,
    replay: Arc<Mutex<ControlReplayWindow>>,
}

const MAX_CONTROL_REPLAY_KEYS: usize = 65_536;

const CONTROL_REPLAY_ROTATION_WATERMARK: usize = 49_152;

const _: () = assert!(CONTROL_REPLAY_ROTATION_WATERMARK < MAX_CONTROL_REPLAY_KEYS);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlReplayDecision {
    Accepted,
    Replay,
    Exhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ControlReplayRotationReason {
    Watermark,

    Exhausted,
}

impl ControlReplayRotationReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Watermark => "control replay window approaching capacity",
            Self::Exhausted => "control replay window exhausted",
        }
    }

    #[must_use]
    pub const fn is_fail_closed(self) -> bool {
        matches!(self, Self::Exhausted)
    }
}

#[derive(Debug, Default)]
struct ControlReplayWindow {
    seen: HashSet<ControlReplayKey>,
    rotation: Option<ControlReplayRotationReason>,
}

impl ControlReplayWindow {
    fn demand_rotation(&mut self, reason: ControlReplayRotationReason) {
        if self.rotation.is_none_or(|current| current < reason) {
            self.rotation = Some(reason);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ControlReplayKey {
    user_id: String,
    device_id: String,
    kind: String,
    action_id: String,
}

impl ControlTrustStore {
    pub fn replace(&self, keys: HashMap<(String, String), ControlTrustEntry>) {
        match self.inner.write() {
            Ok(mut guard) => *guard = keys,
            Err(poisoned) => *poisoned.into_inner() = keys,
        }
    }

    fn entry(&self, user_id: &str, device_id: &str) -> Option<ControlTrustEntry> {
        let key = (user_id.to_owned(), device_id.to_owned());
        match self.inner.read() {
            Ok(guard) => guard.get(&key).cloned(),
            Err(poisoned) => poisoned.into_inner().get(&key).cloned(),
        }
    }

    #[must_use]
    pub fn capabilities(&self, user_id: &str, device_id: &str) -> Option<SessionCapabilities> {
        self.entry(user_id, device_id)
            .map(|entry| entry.capabilities)
    }

    pub fn authorize(
        &self,
        user_id: &str,
        device_id: &str,
        message: &[u8],
        signature: &[u8],
        required: u16,
    ) -> ControlAuthorization {
        let Some(entry) = self.entry(user_id, device_id) else {
            return ControlAuthorization::Untrusted;
        };
        if ML_DSA_65
            .verify_sig(&entry.signing_key, message, signature)
            .is_err()
        {
            return ControlAuthorization::Untrusted;
        }
        if !entry.capabilities.allows(required) {
            return ControlAuthorization::Forbidden {
                granted: entry.capabilities,
            };
        }
        ControlAuthorization::Authorized
    }

    pub fn record_control_once(
        &self,
        user_id: &str,
        device_id: &str,
        kind: &str,
        action_id: &str,
    ) -> ControlReplayDecision {
        let key = ControlReplayKey {
            user_id: user_id.to_owned(),
            device_id: device_id.to_owned(),
            kind: kind.to_owned(),
            action_id: action_id.to_owned(),
        };
        let mut replay = match self.replay.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if replay.seen.contains(&key) {
            return ControlReplayDecision::Replay;
        }
        if replay.seen.len() >= MAX_CONTROL_REPLAY_KEYS {
            replay.demand_rotation(ControlReplayRotationReason::Exhausted);
            return ControlReplayDecision::Exhausted;
        }
        replay.seen.insert(key);
        if replay.seen.len() >= CONTROL_REPLAY_ROTATION_WATERMARK {
            replay.demand_rotation(ControlReplayRotationReason::Watermark);
        }
        ControlReplayDecision::Accepted
    }

    pub fn take_rotation_demand(&self) -> Option<ControlReplayRotationReason> {
        let mut replay = match self.replay.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        replay.rotation.take()
    }

    pub fn reset_replay(&self) {
        let mut replay = match self.replay.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        replay.seen.clear();

        replay.rotation = None;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use aws_lc_rs::signature::{KeyPair, ML_DSA_65_SIGNING, PqdsaKeyPair};
    use kodosi_domain::permissions::{AccessLevel, SessionCapabilities};

    use super::{
        CONTROL_REPLAY_ROTATION_WATERMARK, ControlAuthorization, ControlReplayDecision,
        ControlReplayRotationReason, ControlTrustEntry, ControlTrustStore, MAX_CONTROL_REPLAY_KEYS,
    };

    struct TestDevice {
        keypair: PqdsaKeyPair,
    }

    impl TestDevice {
        fn new() -> Self {
            Self {
                keypair: PqdsaKeyPair::generate(&ML_DSA_65_SIGNING)
                    .unwrap_or_else(|error| panic!("test signing key should generate: {error}")),
            }
        }

        fn public_key(&self) -> Vec<u8> {
            self.keypair.public_key().as_ref().to_vec()
        }

        fn sign(&self, message: &[u8]) -> Vec<u8> {
            let pkcs8 = self
                .keypair
                .to_pkcs8v1()
                .unwrap_or_else(|error| panic!("test key should export: {error}"));
            crate::crypto::sign_control_message(pkcs8.as_ref(), message)
                .unwrap_or_else(|error| panic!("test signing should succeed: {error}"))
        }
    }

    fn store_with(access: AccessLevel, device: &TestDevice) -> ControlTrustStore {
        let store = ControlTrustStore::default();
        store.replace(HashMap::from([(
            ("viewer".to_owned(), "viewer-device".to_owned()),
            ControlTrustEntry::new(
                device.public_key(),
                SessionCapabilities::from_access(access, false),
            ),
        )]));
        store
    }

    fn fill(store: &ControlTrustStore, range: std::ops::Range<usize>) {
        for index in range {
            assert_eq!(
                store.record_control_once("user", "device", "inject", &format!("action-{index}")),
                ControlReplayDecision::Accepted
            );
        }
    }

    #[test]
    fn control_replay_window_rejects_duplicate_sender_action() {
        let store = ControlTrustStore::default();
        assert_eq!(
            store.record_control_once("user", "device", "inject", "action"),
            ControlReplayDecision::Accepted
        );
        assert_eq!(
            store.record_control_once("user", "device", "inject", "action"),
            ControlReplayDecision::Replay
        );
        assert_eq!(
            store.record_control_once("user", "device", "resize", "action"),
            ControlReplayDecision::Accepted
        );
        assert_eq!(
            store.record_control_once("user", "other-device", "inject", "action"),
            ControlReplayDecision::Accepted
        );
    }

    #[test]
    fn replay_store_fails_closed_at_exact_capacity() {
        let store = ControlTrustStore::default();
        for index in 0..MAX_CONTROL_REPLAY_KEYS {
            assert_eq!(
                store.record_control_once("user", "device", "inject", &format!("action-{index}")),
                ControlReplayDecision::Accepted
            );
        }
        assert_eq!(
            store.record_control_once("user", "device", "inject", "overflow"),
            ControlReplayDecision::Exhausted
        );
        assert_eq!(
            store.record_control_once("user", "device", "inject", "action-0"),
            ControlReplayDecision::Replay
        );
    }

    #[test]
    fn replay_store_resets_only_on_explicit_key_epoch_change() {
        let store = ControlTrustStore::default();
        assert_eq!(
            store.record_control_once("user", "device", "inject", "action"),
            ControlReplayDecision::Accepted
        );
        store.reset_replay();
        assert_eq!(
            store.record_control_once("user", "device", "inject", "action"),
            ControlReplayDecision::Accepted
        );
    }

    #[test]
    fn fresh_window_demands_no_rotation() {
        let store = ControlTrustStore::default();
        fill(&store, 0..8);
        assert_eq!(store.take_rotation_demand(), None);
        assert_eq!(store.take_rotation_demand(), None);
    }

    #[test]
    fn rotation_is_demanded_at_the_watermark_before_controls_are_refused() {
        let store = ControlTrustStore::default();
        fill(&store, 0..CONTROL_REPLAY_ROTATION_WATERMARK - 1);
        assert_eq!(
            store.take_rotation_demand(),
            None,
            "one key below the watermark must not demand rotation"
        );
        fill(
            &store,
            CONTROL_REPLAY_ROTATION_WATERMARK - 1..CONTROL_REPLAY_ROTATION_WATERMARK,
        );
        assert_eq!(
            store.take_rotation_demand(),
            Some(ControlReplayRotationReason::Watermark)
        );

        assert!(!ControlReplayRotationReason::Watermark.is_fail_closed());
        assert_eq!(
            store.record_control_once("user", "device", "inject", "still-admitted"),
            ControlReplayDecision::Accepted
        );
    }

    #[test]
    fn exhaustion_latches_a_fail_closed_rotation_demand() {
        let store = ControlTrustStore::default();
        fill(&store, 0..MAX_CONTROL_REPLAY_KEYS);
        assert_eq!(
            store.record_control_once("user", "device", "inject", "overflow"),
            ControlReplayDecision::Exhausted
        );
        let demand = store.take_rotation_demand().expect("rotation demanded");
        assert_eq!(demand, ControlReplayRotationReason::Exhausted);
        assert!(demand.is_fail_closed());
    }

    #[test]
    fn exhaustion_upgrades_a_pending_watermark_demand() {
        let store = ControlTrustStore::default();
        fill(&store, 0..MAX_CONTROL_REPLAY_KEYS);

        assert_eq!(
            store.record_control_once("user", "device", "inject", "overflow"),
            ControlReplayDecision::Exhausted
        );
        assert_eq!(
            store.take_rotation_demand(),
            Some(ControlReplayRotationReason::Exhausted),
            "severity must escalate, never downgrade"
        );
    }

    #[test]
    fn a_drained_demand_is_not_reported_twice() {
        let store = ControlTrustStore::default();
        fill(&store, 0..CONTROL_REPLAY_ROTATION_WATERMARK);
        assert_eq!(
            store.take_rotation_demand(),
            Some(ControlReplayRotationReason::Watermark)
        );
        assert_eq!(
            store.take_rotation_demand(),
            None,
            "one latch must not spam the rotation queue"
        );
    }

    #[test]
    fn every_refusal_relatches_so_a_failed_rotation_cannot_strand_the_session() {
        let store = ControlTrustStore::default();
        fill(&store, 0..MAX_CONTROL_REPLAY_KEYS);
        assert!(store.take_rotation_demand().is_some());
        for attempt in 0..4 {
            assert_eq!(
                store.record_control_once("user", "device", "inject", &format!("flood-{attempt}")),
                ControlReplayDecision::Exhausted
            );
            assert_eq!(
                store.take_rotation_demand(),
                Some(ControlReplayRotationReason::Exhausted)
            );
        }
    }

    #[test]
    fn replays_below_the_watermark_do_not_force_rotation() {
        let store = ControlTrustStore::default();
        fill(&store, 0..16);
        for _ in 0..1_000 {
            assert_eq!(
                store.record_control_once("user", "device", "inject", "action-0"),
                ControlReplayDecision::Replay
            );
        }
        assert_eq!(store.take_rotation_demand(), None);
    }

    #[test]
    fn key_epoch_change_clears_both_the_window_and_the_demand() {
        let store = ControlTrustStore::default();
        fill(&store, 0..MAX_CONTROL_REPLAY_KEYS);
        assert_eq!(
            store.record_control_once("user", "device", "inject", "overflow"),
            ControlReplayDecision::Exhausted
        );
        store.reset_replay();
        assert_eq!(
            store.take_rotation_demand(),
            None,
            "the new epoch satisfies the demand raised under the old one"
        );
        assert_eq!(
            store.record_control_once("user", "device", "inject", "overflow"),
            ControlReplayDecision::Accepted,
            "availability is restored by rotation"
        );
    }

    #[test]
    fn valid_signature_from_view_only_device_cannot_send_input() {
        let device = TestDevice::new();
        let store = store_with(AccessLevel::View, &device);
        let message = b"control-preimage";
        let signature = device.sign(message);

        assert_eq!(
            store.authorize(
                "viewer",
                "viewer-device",
                message,
                &signature,
                SessionCapabilities::VIEW,
            ),
            ControlAuthorization::Authorized,
            "a view-only device may still perform view-level control",
        );
        assert_eq!(
            store.authorize(
                "viewer",
                "viewer-device",
                message,
                &signature,
                SessionCapabilities::SEND_INPUT,
            ),
            ControlAuthorization::Forbidden {
                granted: SessionCapabilities::from_access(AccessLevel::View, false),
            },
            "host must reject terminal input from a view-only participant even with a good signature",
        );
    }

    #[test]
    fn suggest_access_is_limited_to_suggestions() {
        let device = TestDevice::new();
        let store = store_with(AccessLevel::Suggest, &device);
        let message = b"control-preimage";
        let signature = device.sign(message);

        assert_eq!(
            store.authorize(
                "viewer",
                "viewer-device",
                message,
                &signature,
                SessionCapabilities::SUGGEST
            ),
            ControlAuthorization::Authorized
        );
        for forbidden in [
            SessionCapabilities::SEND_INPUT,
            SessionCapabilities::RESIZE,
            SessionCapabilities::STOP,
            SessionCapabilities::APPROVE_DENY,
        ] {
            assert!(
                matches!(
                    store.authorize("viewer", "viewer-device", message, &signature, forbidden),
                    ControlAuthorization::Forbidden { .. }
                ),
                "suggest access must not carry capability {forbidden}",
            );
        }
    }

    #[test]
    fn approve_access_resolves_requests_but_cannot_inject() {
        let device = TestDevice::new();
        let store = store_with(AccessLevel::Approve, &device);
        let message = b"control-preimage";
        let signature = device.sign(message);

        assert_eq!(
            store.authorize(
                "viewer",
                "viewer-device",
                message,
                &signature,
                SessionCapabilities::APPROVE_DENY,
            ),
            ControlAuthorization::Authorized
        );
        assert!(matches!(
            store.authorize(
                "viewer",
                "viewer-device",
                message,
                &signature,
                SessionCapabilities::SEND_INPUT
            ),
            ControlAuthorization::Forbidden { .. }
        ));
    }

    #[test]
    fn owner_devices_receive_every_owner_capability() {
        let owner_device = TestDevice::new();
        let store = ControlTrustStore::default();
        store.replace(HashMap::from([
            (
                ("owner".to_owned(), "owner-device".to_owned()),
                ControlTrustEntry::new(
                    owner_device.public_key(),
                    SessionCapabilities::from_access(AccessLevel::Approve, true),
                ),
            ),
            (
                ("viewer".to_owned(), "viewer-device".to_owned()),
                ControlTrustEntry::new(
                    owner_device.public_key(),
                    SessionCapabilities::from_access(AccessLevel::Approve, false),
                ),
            ),
        ]));
        let message = b"control-preimage";
        let signature = owner_device.sign(message);

        for capability in [
            SessionCapabilities::OWNER,
            SessionCapabilities::VIEW,
            SessionCapabilities::SUGGEST,
            SessionCapabilities::SEND_INPUT,
            SessionCapabilities::APPROVE_DENY,
            SessionCapabilities::RESIZE,
            SessionCapabilities::FOCUS,
            SessionCapabilities::STOP,
        ] {
            assert_eq!(
                store.authorize("owner", "owner-device", message, &signature, capability),
                ControlAuthorization::Authorized,
                "owner device must hold capability {capability}",
            );
        }

        for forbidden in [SessionCapabilities::SEND_INPUT, SessionCapabilities::STOP] {
            assert!(
                matches!(
                    store.authorize("viewer", "viewer-device", message, &signature, forbidden),
                    ControlAuthorization::Forbidden { .. }
                ),
                "non-owner Approve must not carry capability {forbidden}",
            );
        }
    }

    #[test]
    fn forged_signature_and_unknown_device_are_untrusted() {
        let device = TestDevice::new();
        let attacker = TestDevice::new();
        let store = store_with(AccessLevel::Inject, &device);
        let message = b"control-preimage";

        assert_eq!(
            store.authorize(
                "viewer",
                "viewer-device",
                message,
                &attacker.sign(message),
                SessionCapabilities::SEND_INPUT,
            ),
            ControlAuthorization::Untrusted,
            "a signature from another key must not authorize a trusted identity",
        );
        assert_eq!(
            store.authorize(
                "attacker",
                "attacker-device",
                message,
                &attacker.sign(message),
                SessionCapabilities::VIEW,
            ),
            ControlAuthorization::Untrusted,
        );
    }

    #[test]
    fn empty_capability_requirement_is_rejected() {
        assert!(!SessionCapabilities::from_access(AccessLevel::View, false).allows(0));
        assert!(!SessionCapabilities(SessionCapabilities::OWNER).allows(0));
    }

    #[test]
    fn replacing_trust_set_drops_revoked_devices() {
        let device = TestDevice::new();
        let store = store_with(AccessLevel::Inject, &device);
        assert!(store.capabilities("viewer", "viewer-device").is_some());
        store.replace(HashMap::new());
        assert!(store.capabilities("viewer", "viewer-device").is_none());
        assert_eq!(
            store.authorize(
                "viewer",
                "viewer-device",
                b"control-preimage",
                &device.sign(b"control-preimage"),
                SessionCapabilities::VIEW,
            ),
            ControlAuthorization::Untrusted,
        );
    }

    #[test]
    fn capability_checks_do_not_disturb_the_replay_window() {
        let device = TestDevice::new();
        let store = store_with(AccessLevel::View, &device);
        let message = b"control-preimage";
        let signature = device.sign(message);

        for _ in 0..1_000 {
            assert!(matches!(
                store.authorize(
                    "viewer",
                    "viewer-device",
                    message,
                    &signature,
                    SessionCapabilities::SEND_INPUT
                ),
                ControlAuthorization::Forbidden { .. }
            ));
        }
        assert_eq!(store.take_rotation_demand(), None);
    }
}
