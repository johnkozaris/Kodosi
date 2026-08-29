use std::str::FromStr as _;

use aws_lc_rs::digest;
use kodosi_backend_client::BackendOrigin;
use kodosi_domain::{ids::SessionId, permissions::ShareScope};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AppError, Result};

const FINGERPRINT_DOMAIN: &[u8] = b"kodosi:share-transition:v2:";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ShareAudience {
    pub(crate) scope: ShareScope,
    pub(crate) room_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ShareTransitionTerminalStatus {
    Applied,
    RolledBack,
    Cancelled,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ShareTransitionTerminal {
    pub(crate) status: ShareTransitionTerminalStatus,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ShareTransitionCleanupIdentity {
    pub(crate) backend_origin: String,
    pub(crate) create_idempotency_id: Uuid,
    pub(crate) end_mutation_id: Uuid,
    pub(crate) created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ShareTransitionState {
    Prepared,
    ApplyingTarget,
    TargetObserved,
    KeyPreparing,
    GenerationClaiming,
    BlobsPublishing {
        key_generation: u32,
    },
    RelayPending {
        key_generation: u32,
        relay_generation: u64,
    },
    CommitReady,
    CleanupPending(ShareTransitionTerminal),
    Terminal(ShareTransitionTerminal),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PreparedShareTransition {
    pub(crate) transition_id: Uuid,
    pub(crate) transition_epoch: u64,
    pub(crate) account_user_id: String,
    pub(crate) originating_account_epoch: u64,
    pub(crate) runtime_session_id: SessionId,
    pub(crate) expected_runtime_incarnation_id: Uuid,
    pub(crate) previous: ShareAudience,
    pub(crate) target: ShareAudience,
    pub(crate) backend_session_id: String,
    pub(crate) backend_incarnation_id: Option<Uuid>,
    pub(crate) source_key_generation: Option<u32>,
    pub(crate) claimed_key_generation: Option<u32>,
    pub(crate) relay_generation: Option<u64>,
    pub(crate) cleanup: Option<ShareTransitionCleanupIdentity>,
    pub(crate) fingerprint: String,
    pub(crate) state: ShareTransitionState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeferredShareVerdict {
    ScopeChanged,
    Error,
    None,
}

#[derive(Debug, Clone)]
pub(crate) struct DeferredShareSettlement {
    pub(crate) entry: PreparedShareTransition,
    pub(crate) verdict: DeferredShareVerdict,
}

impl PreparedShareTransition {
    #[expect(
        clippy::too_many_arguments,
        reason = "transition provenance stores every exact commit fence explicitly"
    )]
    pub(crate) fn new(
        transition_id: Uuid,
        transition_epoch: u64,
        account_user_id: String,
        originating_account_epoch: u64,
        runtime_session_id: SessionId,
        expected_runtime_incarnation_id: Uuid,
        previous: ShareAudience,
        target: ShareAudience,
        backend_session_id: String,
        backend_incarnation_id: Option<Uuid>,
        source_key_generation: Option<u32>,
        cleanup: Option<ShareTransitionCleanupIdentity>,
    ) -> Result<Self> {
        let fingerprint = fingerprint(
            transition_id,
            transition_epoch,
            &account_user_id,
            originating_account_epoch,
            runtime_session_id,
            expected_runtime_incarnation_id,
            &previous,
            &target,
            &backend_session_id,
            backend_incarnation_id,
            source_key_generation,
            cleanup.as_ref(),
        )?;
        let value = Self {
            transition_id,
            transition_epoch,
            account_user_id,
            originating_account_epoch,
            runtime_session_id,
            expected_runtime_incarnation_id,
            previous,
            target,
            backend_session_id,
            backend_incarnation_id,
            source_key_generation,
            claimed_key_generation: None,
            relay_generation: None,
            cleanup,
            fingerprint,
            state: ShareTransitionState::Prepared,
        };
        value.validate()?;
        Ok(value)
    }

    pub(crate) const fn is_initial_share(&self) -> bool {
        matches!(self.previous.scope, ShareScope::JustMe)
            && !matches!(self.target.scope, ShareScope::JustMe)
    }

    pub(crate) fn bind_backend_incarnation(&mut self, incarnation_id: Uuid) -> Result<()> {
        match self.backend_incarnation_id {
            None if self.is_initial_share() => self.backend_incarnation_id = Some(incarnation_id),
            Some(existing) if existing == incarnation_id => {}
            _ => {
                return Err(AppError::Unsupported {
                    reason: "share transition backend incarnation binding changed".to_owned(),
                });
            }
        }
        Ok(())
    }

    pub(crate) fn bind_source_key_generation(&mut self, generation: u32) -> Result<()> {
        match self.source_key_generation {
            None if self.is_initial_share() => self.source_key_generation = Some(generation),
            Some(existing) if existing == generation => {}
            _ => {
                return Err(AppError::Unsupported {
                    reason: "share transition source key generation binding changed".to_owned(),
                });
            }
        }
        Ok(())
    }

    pub(crate) fn bind_claimed_key_generation(&mut self, generation: u32) -> Result<()> {
        if generation == 0 {
            return invalid("shareTransitions.claimedKeyGeneration", "must be positive");
        }
        match self.claimed_key_generation {
            None => self.claimed_key_generation = Some(generation),
            Some(existing) if existing == generation => {}
            Some(_) => {
                return Err(AppError::Unsupported {
                    reason: "share transition claimed key generation binding changed".to_owned(),
                });
            }
        }
        self.validate()
    }

    pub(crate) fn bind_relay_generation(&mut self, generation: u64) -> Result<()> {
        if generation == 0 {
            return invalid("shareTransitions.relayGeneration", "must be positive");
        }
        match self.relay_generation {
            None => self.relay_generation = Some(generation),
            Some(existing) if existing == generation => {}
            Some(_) => {
                return Err(AppError::Unsupported {
                    reason: "share transition relay generation binding changed".to_owned(),
                });
            }
        }
        Ok(())
    }

    pub(crate) fn validate(&self) -> Result<()> {
        let backend_session_valid = Uuid::parse_str(&self.backend_session_id)
            .is_ok_and(|id| id.to_string() == self.backend_session_id.to_lowercase());
        let initial_share = self.is_initial_share();
        let local_noop = self.previous.scope == ShareScope::JustMe
            && self.target.scope == ShareScope::JustMe
            && self.cleanup.is_none();
        let source_binding_valid = initial_share
            || local_noop
            || (self.backend_incarnation_id.is_some() && self.source_key_generation.is_some());
        let cleanup_valid = self.cleanup.as_ref().is_some_and(|cleanup| {
            cleanup.create_idempotency_id.get_version_num() == 7
                && cleanup.end_mutation_id.get_version_num() == 7
                && cleanup.created_at_ms >= 0
                && BackendOrigin::from_str(&cleanup.backend_origin).is_ok()
        });
        if self.transition_id.get_version_num() != 7
            || self.transition_epoch == 0
            || self.account_user_id.trim().is_empty()
            || self.account_user_id.len() > 1_024
            || self.originating_account_epoch == 0
            || self.expected_runtime_incarnation_id.is_nil()
            || !backend_session_valid
            || !source_binding_valid
            || self.backend_incarnation_id.is_some_and(|id| id.is_nil())
            || self.previous.scope == ShareScope::Friends
            || self.target.scope == ShareScope::Friends
            || (self.previous.scope == ShareScope::Room) != self.previous.room_id.is_some()
            || (self.target.scope == ShareScope::Room) != self.target.room_id.is_some()
            || (!cleanup_valid && !local_noop)
        {
            return invalid(
                "shareTransitions.identity",
                "share transition identity, audience, cleanup, or incarnation fence is invalid",
            );
        }
        let expected = fingerprint(
            self.transition_id,
            self.transition_epoch,
            &self.account_user_id,
            self.originating_account_epoch,
            self.runtime_session_id,
            self.expected_runtime_incarnation_id,
            &self.previous,
            &self.target,
            &self.backend_session_id,
            if initial_share {
                None
            } else {
                self.backend_incarnation_id
            },
            if initial_share {
                None
            } else {
                self.source_key_generation
            },
            self.cleanup.as_ref(),
        )?;
        if self.fingerprint != expected || !is_sha256_hex(&self.fingerprint) {
            return invalid(
                "shareTransitions.fingerprint",
                "share transition fingerprint does not match exact intent",
            );
        }
        match &self.state {
            ShareTransitionState::BlobsPublishing { key_generation } => {
                if *key_generation == 0
                    || self.claimed_key_generation != Some(*key_generation)
                    || self.relay_generation.is_some()
                {
                    return invalid(
                        "shareTransitions.blobsPublishing",
                        "blob publication must match the write-once claimed generation",
                    );
                }
            }
            ShareTransitionState::RelayPending {
                key_generation,
                relay_generation,
            } => {
                if *key_generation == 0
                    || *relay_generation == 0
                    || self.claimed_key_generation != Some(*key_generation)
                    || self.relay_generation != Some(*relay_generation)
                {
                    return invalid(
                        "shareTransitions.relayPending",
                        "relay phase must match exact key and relay generation bindings",
                    );
                }
            }
            ShareTransitionState::CleanupPending(terminal)
            | ShareTransitionState::Terminal(terminal) => validate_terminal(terminal)?,
            ShareTransitionState::Prepared
            | ShareTransitionState::ApplyingTarget
            | ShareTransitionState::TargetObserved
            | ShareTransitionState::KeyPreparing
            | ShareTransitionState::GenerationClaiming
            | ShareTransitionState::CommitReady => {}
        }
        Ok(())
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "fingerprint covers every immutable transition commit fence explicitly"
)]
fn fingerprint(
    transition_id: Uuid,
    transition_epoch: u64,
    account_user_id: &str,
    originating_account_epoch: u64,
    runtime_session_id: SessionId,
    runtime_incarnation_id: Uuid,
    previous: &ShareAudience,
    target: &ShareAudience,
    backend_session_id: &str,
    backend_incarnation_id: Option<Uuid>,
    source_key_generation: Option<u32>,
    cleanup: Option<&ShareTransitionCleanupIdentity>,
) -> Result<String> {
    let mut bytes = FINGERPRINT_DOMAIN.to_vec();
    push(&mut bytes, transition_id.as_bytes())?;
    push(&mut bytes, &transition_epoch.to_be_bytes())?;
    push(&mut bytes, account_user_id.as_bytes())?;
    push(&mut bytes, &originating_account_epoch.to_be_bytes())?;
    let runtime_session_id = runtime_session_id.to_string();
    push(&mut bytes, runtime_session_id.as_bytes())?;
    push(&mut bytes, runtime_incarnation_id.as_bytes())?;
    push_audience(&mut bytes, previous)?;
    push_audience(&mut bytes, target)?;
    push(&mut bytes, backend_session_id.as_bytes())?;
    push_optional(
        &mut bytes,
        backend_incarnation_id
            .as_ref()
            .map(|value| value.as_bytes().as_slice()),
    )?;
    push_optional(
        &mut bytes,
        source_key_generation
            .as_ref()
            .map(|value| value.to_be_bytes())
            .as_ref()
            .map(<[u8; 4]>::as_slice),
    )?;
    push_optional(
        &mut bytes,
        cleanup.map(|value| value.backend_origin.as_bytes()),
    )?;
    push_optional(
        &mut bytes,
        cleanup.map(|value| value.create_idempotency_id.as_bytes().as_slice()),
    )?;
    push_optional(
        &mut bytes,
        cleanup.map(|value| value.end_mutation_id.as_bytes().as_slice()),
    )?;
    push_optional(
        &mut bytes,
        cleanup
            .map(|value| value.created_at_ms.to_be_bytes())
            .as_ref()
            .map(<[u8; 8]>::as_slice),
    )?;
    Ok(hex(digest::digest(&digest::SHA256, &bytes).as_ref()))
}

fn push_audience(bytes: &mut Vec<u8>, audience: &ShareAudience) -> Result<()> {
    push(
        bytes,
        match audience.scope {
            ShareScope::JustMe => b"justMe",
            ShareScope::MyDevices => b"myDevices",
            ShareScope::Friends => b"friends",
            ShareScope::Room => b"room",
        },
    )?;
    push_optional(bytes, audience.room_id.as_deref().map(str::as_bytes))
}

fn push(bytes: &mut Vec<u8>, value: &[u8]) -> Result<()> {
    let length = u32::try_from(value.len()).map_err(|_| AppError::Unsupported {
        reason: "share transition fingerprint field is too large".to_owned(),
    })?;
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(value);
    Ok(())
}

fn push_optional(bytes: &mut Vec<u8>, value: Option<&[u8]>) -> Result<()> {
    bytes.push(u8::from(value.is_some()));
    if let Some(value) = value {
        push(bytes, value)?;
    }
    Ok(())
}

fn validate_terminal(terminal: &ShareTransitionTerminal) -> Result<()> {
    if terminal
        .message
        .as_ref()
        .is_some_and(|message| message.len() > 16 * 1_024)
    {
        return invalid(
            "shareTransitions.terminal",
            "terminal message must be bounded",
        );
    }
    Ok(())
}

fn invalid<T>(field: &str, reason: &str) -> Result<T> {
    Err(AppError::InvalidBackendData {
        field: field.to_owned(),
        reason: reason.to_owned(),
    })
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cleanup() -> ShareTransitionCleanupIdentity {
        ShareTransitionCleanupIdentity {
            backend_origin: "https://example.com:443/".to_owned(),
            create_idempotency_id: Uuid::now_v7(),
            end_mutation_id: Uuid::now_v7(),
            created_at_ms: 1,
        }
    }

    fn transition() -> PreparedShareTransition {
        PreparedShareTransition::new(
            Uuid::now_v7(),
            1,
            "account-a".to_owned(),
            7,
            SessionId::new(),
            Uuid::now_v7(),
            ShareAudience {
                scope: ShareScope::JustMe,
                room_id: None,
            },
            ShareAudience {
                scope: ShareScope::MyDevices,
                room_id: None,
            },
            Uuid::now_v7().to_string(),
            None,
            None,
            Some(cleanup()),
        )
        .unwrap()
    }

    #[test]
    fn exact_intent_fingerprint_is_tamper_evident() {
        let mut value = transition();
        assert!(value.validate().is_ok());
        value.target.scope = ShareScope::Room;
        value.target.room_id = Some("room-a".to_owned());
        assert!(value.validate().is_err());
    }

    #[test]
    fn initial_share_allows_write_once_result_bindings() {
        let mut value = transition();
        let incarnation = Uuid::now_v7();
        value.bind_backend_incarnation(incarnation).unwrap();
        value.bind_backend_incarnation(incarnation).unwrap();
        value.bind_source_key_generation(0).unwrap();
        assert!(value.bind_backend_incarnation(Uuid::now_v7()).is_err());
        assert!(value.bind_source_key_generation(1).is_err());
    }

    #[test]
    fn relay_pending_requires_exact_positive_bindings() {
        let mut value = transition();
        value.bind_claimed_key_generation(7).unwrap();
        value.bind_relay_generation(9).unwrap();
        value.state = ShareTransitionState::RelayPending {
            key_generation: 7,
            relay_generation: 9,
        };
        assert!(value.validate().is_ok());
        value.state = ShareTransitionState::RelayPending {
            key_generation: 7,
            relay_generation: 8,
        };
        assert!(value.validate().is_err());
    }
}
