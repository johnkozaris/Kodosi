use std::collections::{BTreeSet, VecDeque};

use crate::host_protocol::{
    AgentIntelEvent, AuthEvent, DeviceEvent, FriendsEvent, RoomEvent, SessionEvent, TerminalEvent,
    TrustEvent,
};

pub(crate) const PENDING_TERMINAL_CONTROL_EVENT_CAPACITY: usize = 256;
pub(crate) const PENDING_FRIENDS_EVENT_CAPACITY: usize = 64;
pub(crate) const PENDING_DEVICES_EVENT_CAPACITY: usize = 64;
pub(crate) const PENDING_TRUST_EVENT_CAPACITY: usize = 32;
pub(crate) const PENDING_ROOM_EVENT_CAPACITY: usize = 32;
pub(crate) const PENDING_AUTH_EVENT_CAPACITY: usize = 64;
pub(crate) const PENDING_SESSION_EVENT_CAPACITY: usize = 256;
pub(crate) const PENDING_AGENT_INTEL_EVENT_CAPACITY: usize = 64;

#[derive(Debug)]
pub(crate) struct RuntimeEventOutbox {
    terminal_control_events: PendingDomainEvents<TerminalEvent>,
    friends_events: PendingDomainEvents<FriendsEvent>,
    devices_events: PendingDomainEvents<DeviceEvent>,
    trust_events: PendingDomainEvents<TrustEvent>,
    room_events: PendingDomainEvents<RoomEvent>,
    auth_events: PendingDomainEvents<AuthEvent>,
    session_events: PendingDomainEvents<SessionEvent>,
    agent_intel_events: PendingDomainEvents<AgentIntelEvent>,
    pending_permissions_snapshot: Option<AgentIntelEvent>,
}

impl Default for RuntimeEventOutbox {
    fn default() -> Self {
        Self {
            terminal_control_events: PendingDomainEvents::new(
                PENDING_TERMINAL_CONTROL_EVENT_CAPACITY,
                "pending terminal control event",
            ),
            friends_events: PendingDomainEvents::new(
                PENDING_FRIENDS_EVENT_CAPACITY,
                "pending friends event",
            ),
            devices_events: PendingDomainEvents::new(
                PENDING_DEVICES_EVENT_CAPACITY,
                "pending devices event",
            ),
            trust_events: PendingDomainEvents::new(
                PENDING_TRUST_EVENT_CAPACITY,
                "pending trust event",
            ),
            room_events: PendingDomainEvents::new(
                PENDING_ROOM_EVENT_CAPACITY,
                "pending room event",
            ),
            auth_events: PendingDomainEvents::new(
                PENDING_AUTH_EVENT_CAPACITY,
                "pending auth event",
            ),
            session_events: PendingDomainEvents::new(
                PENDING_SESSION_EVENT_CAPACITY,
                "pending session event",
            ),
            agent_intel_events: PendingDomainEvents::new(
                PENDING_AGENT_INTEL_EVENT_CAPACITY,
                "pending agent-intel event",
            ),
            pending_permissions_snapshot: None,
        }
    }
}

macro_rules! impl_outbox_lane {
    ($queue:ident, $drain:ident, $field:ident, $event:ty) => {
        pub(crate) fn $queue(&mut self, event: $event) {
            self.$field.push(event);
        }
        pub(crate) fn $drain(&mut self) -> Vec<$event> {
            self.$field.drain()
        }
    };
}

impl RuntimeEventOutbox {
    impl_outbox_lane!(
        queue_terminal_control,
        drain_terminal_control,
        terminal_control_events,
        TerminalEvent
    );
    impl_outbox_lane!(queue_friends, drain_friends, friends_events, FriendsEvent);
    impl_outbox_lane!(queue_devices, drain_devices, devices_events, DeviceEvent);
    impl_outbox_lane!(queue_trust, drain_trust, trust_events, TrustEvent);
    impl_outbox_lane!(queue_room, drain_room, room_events, RoomEvent);
    impl_outbox_lane!(queue_auth, drain_auth, auth_events, AuthEvent);
    impl_outbox_lane!(queue_session, drain_sessions, session_events, SessionEvent);
    pub(crate) fn queue_agent_intel(&mut self, event: AgentIntelEvent) {
        self.agent_intel_events.push(event);
    }

    pub(crate) fn queue_pending_permissions_snapshot(
        &mut self,
        snapshot: crate::host_protocol::PendingPermissionsSnapshot,
    ) {
        let event = AgentIntelEvent::PendingPermissionsSnapshot {
            generation: snapshot.generation,
            requests: snapshot.requests,
        };
        let replace = self
            .pending_permissions_snapshot
            .as_ref()
            .is_none_or(|current| {
                let AgentIntelEvent::PendingPermissionsSnapshot { generation, .. } = current else {
                    return true;
                };
                snapshot.generation >= *generation
            });
        if replace {
            self.pending_permissions_snapshot = Some(event);
        }
    }

    pub(crate) fn drain_agent_intel(&mut self) -> Vec<AgentIntelEvent> {
        let mut events = self.agent_intel_events.drain();
        if let Some(snapshot) = self.pending_permissions_snapshot.take() {
            events.push(snapshot);
        }
        events
    }

    pub(crate) fn retire_session_incarnation(&mut self, session_id: &str, incarnation_id: &str) {
        self.agent_intel_events.retain(|event| {
            !agent_intel_event_matches_incarnation(event, session_id, incarnation_id)
        });
        if self
            .pending_permissions_snapshot
            .as_ref()
            .is_some_and(|event| {
                agent_intel_event_matches_incarnation(event, session_id, incarnation_id)
            })
        {
            self.pending_permissions_snapshot = None;
        }
    }

    pub(crate) fn clear_account_epoch(
        &mut self,
        remote_session_ids: &BTreeSet<String>,
        retiring_account_user_id: Option<&str>,
    ) {
        self.friends_events.clear();
        self.devices_events.clear();
        self.trust_events.clear();
        self.room_events.clear();
        self.terminal_control_events
            .retain(|event| !remote_session_ids.contains(event.session_id()));
        self.session_events.retain(|event| match event {
            SessionEvent::List { .. }
            | SessionEvent::RoomList { .. }
            | SessionEvent::HiddenList { .. } => false,
            SessionEvent::Upsert { session } => !remote_session_ids.contains(session.id()),
            SessionEvent::Created { session_id, .. }
            | SessionEvent::Removed { session_id }
            | SessionEvent::Opened { session_id }
            | SessionEvent::Interrupted { session_id, .. }
            | SessionEvent::ActionResult { session_id, .. }
            | SessionEvent::AccessGrants { session_id, .. }
            | SessionEvent::AccessMutationAccepted { session_id, .. }
            | SessionEvent::AccessMutationResult { session_id, .. }
            | SessionEvent::AccessMutationRecovered { session_id, .. }
            | SessionEvent::ScopeAccepted { session_id, .. }
            | SessionEvent::ScopeChanged { session_id, .. } => {
                !remote_session_ids.contains(session_id)
            }
            SessionEvent::Error { session_id, .. } => session_id
                .as_ref()
                .is_none_or(|session_id| !remote_session_ids.contains(session_id)),
        });
        self.agent_intel_events.retain(|event| {
            !agent_intel_event_is_for(event, remote_session_ids)
                && !agent_intel_event_is_for_account(event, retiring_account_user_id)
        });
        self.pending_permissions_snapshot = None;
    }
}

fn agent_intel_event_matches_incarnation(
    event: &AgentIntelEvent,
    session_id: &str,
    incarnation_id: &str,
) -> bool {
    match event {
        AgentIntelEvent::Snapshot {
            session_id: event_session_id,
            session_incarnation_id,
            ..
        }
        | AgentIntelEvent::Cleared {
            session_id: event_session_id,
            session_incarnation_id,
        }
        | AgentIntelEvent::RemotePermissionDecisionState {
            session_id: event_session_id,
            session_incarnation_id,
            ..
        } => event_session_id == session_id && session_incarnation_id == incarnation_id,
        AgentIntelEvent::PendingPermissionsSnapshot { requests, .. } => {
            requests.iter().any(|request| {
                request.session_id == session_id && request.session_incarnation_id == incarnation_id
            })
        }
        AgentIntelEvent::SteerState { entry, .. } => {
            entry.session_id == session_id && entry.session_incarnation_id == incarnation_id
        }
        AgentIntelEvent::Reply { .. } | AgentIntelEvent::Error { .. } => false,
    }
}

fn agent_intel_event_is_for(
    event: &AgentIntelEvent,
    remote_session_ids: &BTreeSet<String>,
) -> bool {
    let session_id = match event {
        AgentIntelEvent::Snapshot { session_id, .. }
        | AgentIntelEvent::Cleared { session_id, .. }
        | AgentIntelEvent::RemotePermissionDecisionState { session_id, .. } => Some(session_id),
        AgentIntelEvent::Reply { .. } | AgentIntelEvent::Error { .. } => None,
        AgentIntelEvent::PendingPermissionsSnapshot { requests, .. } => {
            return requests
                .iter()
                .any(|request| remote_session_ids.contains(&request.session_id));
        }
        AgentIntelEvent::SteerState { entry, .. } => Some(&entry.session_id),
    };
    session_id.is_some_and(|session_id| remote_session_ids.contains(session_id))
}

fn agent_intel_event_is_for_account(
    event: &AgentIntelEvent,
    retiring_account_user_id: Option<&str>,
) -> bool {
    match event {
        AgentIntelEvent::SteerState { entry, .. } => retiring_account_user_id
            .is_none_or(|account_user_id| entry.account_user_id == account_user_id),
        _ => false,
    }
}

#[derive(Debug)]
struct PendingDomainEvents<T> {
    entries: VecDeque<T>,
    capacity: usize,
    label: &'static str,
}

impl<T> PendingDomainEvents<T> {
    fn new(capacity: usize, label: &'static str) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
            label,
        }
    }

    fn push(&mut self, event: T) {
        drop(self.push_reporting_eviction(event));
    }

    fn push_reporting_eviction(&mut self, event: T) -> Option<T> {
        let evicted = if self.entries.len() >= self.capacity {
            let evicted = self.entries.pop_front();
            tracing::warn!(
                capacity = self.capacity,
                "{} queue full; dropped oldest event",
                self.label
            );
            evicted
        } else {
            None
        };
        self.entries.push_back(event);
        evicted
    }

    fn drain(&mut self) -> Vec<T> {
        self.entries.drain(..).collect()
    }

    fn clear(&mut self) {
        self.entries.clear();
    }

    fn retain(&mut self, keep: impl FnMut(&T) -> bool) {
        self.entries.retain(keep);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PENDING_AUTH_EVENT_CAPACITY, PENDING_DEVICES_EVENT_CAPACITY,
        PENDING_FRIENDS_EVENT_CAPACITY, PENDING_TERMINAL_CONTROL_EVENT_CAPACITY,
        RuntimeEventOutbox,
    };
    use crate::host_protocol::{
        ActivePendingPermission, AgentIntelEvent, AuthEvent, DeviceEvent, FriendsEvent,
        PendingPermissionDecisionPhase, PendingPermissionsSnapshot, SemanticSendMode,
        SteerDeliveryState, SteerQueueEntry, SteerTransition, TerminalEvent,
    };

    fn steer_state(account_user_id: &str, text: &str) -> AgentIntelEvent {
        AgentIntelEvent::SteerState {
            entry: SteerQueueEntry {
                steer_id: format!("steer-{account_user_id}"),
                account_user_id: account_user_id.to_owned(),
                request_id: format!("request-{account_user_id}"),
                session_incarnation_id: "incarnation".to_owned(),
                mode: SemanticSendMode::Steer,
                session_id: "session-1".to_owned(),
                text: text.to_owned(),
                queued_at_ms: 1,
                delivery_state: SteerDeliveryState::Queued,
                at_tool_use_id: None,
            },
            transition: SteerTransition::Queued,
            message: None,
        }
    }

    fn pending_request(request_generation: u64) -> ActivePendingPermission {
        ActivePendingPermission {
            session_id: "session-1".to_owned(),
            session_incarnation_id: "incarnation-1".to_owned(),
            request_generation,
            tool_use_id: "tool-1".to_owned(),
            tool_name: "Bash".to_owned(),
            tool_input: serde_json::json!({"command": "ls"}),
            created_at_ms: 1,
            deadline_at_ms: 2,
            risk: crate::ApprovalRisk::Unknown,
            decision_phase: PendingPermissionDecisionPhase::Actionable,
        }
    }

    #[test]
    fn complete_pending_snapshot_coalesces_independently_of_detailed_overflow() {
        let mut outbox = RuntimeEventOutbox::default();
        outbox.queue_pending_permissions_snapshot(PendingPermissionsSnapshot {
            generation: 4,
            requests: vec![pending_request(3)],
        });
        for index in 0..256 {
            outbox.queue_agent_intel(AgentIntelEvent::Cleared {
                session_id: format!("noise-{index}"),
                session_incarnation_id: "incarnation".to_owned(),
            });
        }
        outbox.queue_pending_permissions_snapshot(PendingPermissionsSnapshot {
            generation: 5,
            requests: Vec::new(),
        });
        outbox.queue_pending_permissions_snapshot(PendingPermissionsSnapshot {
            generation: 4,
            requests: vec![pending_request(3)],
        });

        let snapshots = outbox
            .drain_agent_intel()
            .into_iter()
            .filter_map(|event| match event {
                AgentIntelEvent::PendingPermissionsSnapshot {
                    generation,
                    requests,
                } => Some((generation, requests)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].0, 5);
        assert!(snapshots[0].1.is_empty());
    }

    #[test]
    fn account_epoch_clear_drops_only_retiring_account_steer_plaintext() {
        let mut outbox = RuntimeEventOutbox::default();
        outbox.queue_agent_intel(steer_state("account-a", "secret-a"));
        outbox.queue_agent_intel(steer_state("account-b", "secret-b"));

        outbox.clear_account_epoch(&std::collections::BTreeSet::new(), Some("account-a"));

        std::assert_matches!(
            outbox.drain_agent_intel().as_slice(),
            [AgentIntelEvent::SteerState { entry, .. }]
                if entry.account_user_id == "account-b" && entry.text == "secret-b"
        );
    }

    #[test]
    fn unscoped_signed_out_clear_drops_all_steer_plaintext() {
        let mut outbox = RuntimeEventOutbox::default();
        outbox.queue_agent_intel(steer_state("account-a", "secret-a"));
        outbox.queue_agent_intel(AgentIntelEvent::Cleared {
            session_id: "local-session".to_owned(),
            session_incarnation_id: "local-incarnation".to_owned(),
        });

        outbox.clear_account_epoch(&std::collections::BTreeSet::new(), None);

        std::assert_matches!(
            outbox.drain_agent_intel().as_slice(),
            [AgentIntelEvent::Cleared { session_id, .. }] if session_id == "local-session"
        );
    }

    #[test]
    fn terminal_control_events_drop_oldest_on_overflow() {
        let mut outbox = RuntimeEventOutbox::default();

        for index in 0..=PENDING_TERMINAL_CONTROL_EVENT_CAPACITY {
            outbox.queue_terminal_control(TerminalEvent::Notification {
                session_id: format!("session-{index}"),
                title: Some(format!("title-{index}")),
                body: None,
            });
        }

        let messages = outbox.drain_terminal_control();
        assert_eq!(messages.len(), PENDING_TERMINAL_CONTROL_EVENT_CAPACITY);

        let retained = messages
            .iter()
            .map(|message| match message {
                TerminalEvent::Notification {
                    session_id, title, ..
                } => (session_id.clone(), title.clone()),
                other => panic!("unexpected terminal event: {other:?}"),
            })
            .collect::<Vec<_>>();
        let expected = (1..=PENDING_TERMINAL_CONTROL_EVENT_CAPACITY)
            .map(|index| (format!("session-{index}"), Some(format!("title-{index}"))))
            .collect::<Vec<_>>();
        assert_eq!(
            retained, expected,
            "overflow must drop only the oldest event and preserve queue order"
        );
    }

    #[test]
    fn friends_events_drop_oldest_on_overflow() {
        let mut outbox = RuntimeEventOutbox::default();

        for index in 0..=PENDING_FRIENDS_EVENT_CAPACITY {
            outbox.queue_friends(FriendsEvent::Error {
                operation: "refresh".to_owned(),
                message: format!("event-{index}"),
                request_id: None,
            });
        }

        let messages = outbox.drain_friends();
        assert_eq!(messages.len(), PENDING_FRIENDS_EVENT_CAPACITY);
        std::assert_matches!(
            messages.first(),
            Some(FriendsEvent::Error { message, .. }) if message == "event-1"
        );
        std::assert_matches!(
            messages.last(),
            Some(FriendsEvent::Error { message, .. })
                if message == &format!("event-{PENDING_FRIENDS_EVENT_CAPACITY}")
        );
    }

    #[test]
    fn devices_events_drop_oldest_on_overflow() {
        let mut outbox = RuntimeEventOutbox::default();

        for index in 0..=PENDING_DEVICES_EVENT_CAPACITY {
            outbox.queue_devices(DeviceEvent::Error {
                user_code: None,
                operation: "refresh".to_owned(),
                message: format!("event-{index}"),
            });
        }

        let messages = outbox.drain_devices();
        assert_eq!(messages.len(), PENDING_DEVICES_EVENT_CAPACITY);
        std::assert_matches!(
            messages.first(),
            Some(DeviceEvent::Error { message, .. }) if message == "event-1"
        );
        std::assert_matches!(
            messages.last(),
            Some(DeviceEvent::Error { message, .. })
                if message == &format!("event-{PENDING_DEVICES_EVENT_CAPACITY}")
        );
    }

    #[test]
    fn auth_events_drop_oldest_on_overflow() {
        let mut outbox = RuntimeEventOutbox::default();

        for index in 0..=PENDING_AUTH_EVENT_CAPACITY {
            outbox.queue_auth(AuthEvent::Error {
                operation: "login.start".to_owned(),
                message: format!("event-{index}"),
            });
        }

        let messages = outbox.drain_auth();
        assert_eq!(messages.len(), PENDING_AUTH_EVENT_CAPACITY);
        std::assert_matches!(
            messages.first(),
            Some(AuthEvent::Error { message, .. }) if message == "event-1"
        );
        std::assert_matches!(
            messages.last(),
            Some(AuthEvent::Error { message, .. })
                if message == &format!("event-{PENDING_AUTH_EVENT_CAPACITY}")
        );
    }
}
