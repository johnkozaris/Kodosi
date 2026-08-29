use time::OffsetDateTime;

use crate::{
    host_protocol::{RelayActionStatus as HostRelayActionStatus, SessionEvent as HostSessionEvent},
    runtime::AppState,
};
use kodosi_domain::{
    ids::SessionId,
    lifecycle::{
        ConnectionState, RemoteActionStatus, RemoteSessionAccessIssue, RemoteSessionAccessState,
    },
    permissions::AccessLevel,
    session::SessionState,
};

impl AppState {
    pub(crate) fn apply_host_demand(
        &mut self,
        id: SessionId,
        required: bool,
        participant_count: usize,
        reason: &str,
    ) {
        let transition = if let Some(record) = self.local.sessions.record_mut(id) {
            let previous_participant_count = record.summary.active_count;
            record.summary.active_count = participant_count;
            record.summary.last_update = OffsetDateTime::now_utc();
            Some((previous_participant_count, participant_count))
        } else {
            None
        };

        if let Some((previous_participant_count, current_participant_count)) = transition {
            if previous_participant_count == 0 && required {
                self.record_log(format!(
                    "{} live demand active ({current_participant_count} viewer{})",
                    id.short(),
                    if current_participant_count == 1 {
                        ""
                    } else {
                        "s"
                    }
                ));
            } else if previous_participant_count > 0 && !required {
                self.record_log(format!("{} live demand idle ({reason})", id.short()));
            }
        }
    }

    pub(crate) fn apply_remote_state_changed(&mut self, id: SessionId, state: SessionState) {
        if let Some(record) = self.discovery.session_mut(id) {
            record.summary.state = state;
            record.summary.last_update = OffsetDateTime::now_utc();
        }
    }

    pub(crate) fn apply_remote_access_changed(&mut self, id: SessionId, access: AccessLevel) {
        if let Some(record) = self.discovery.session_mut(id) {
            record.summary.access = access;
            record.summary.last_update = OffsetDateTime::now_utc();
        }
    }

    pub(crate) fn apply_remote_action_result(
        &mut self,
        id: SessionId,
        action_id: String,
        request_id: Option<String>,
        request_generation: Option<u64>,
        status: RemoteActionStatus,
    ) {
        let locally_correlated_tool_use_id = self
            .remote
            .session_relays
            .take_permission_tool_use_id(id, &action_id);
        let permission_tool_use_id = match (request_id, locally_correlated_tool_use_id) {
            (Some(request_id), Some(local_tool_use_id)) if request_id != local_tool_use_id => {
                self.record_log(format!(
                    "{} ignored mismatched local permission correlation: backend requestId={} local toolUseId={}",
                    id.short(),
                    request_id,
                    local_tool_use_id,
                ));
                Some(request_id)
            }
            (Some(request_id), _) => Some(request_id),
            (None, local_tool_use_id) => local_tool_use_id,
        };
        let status = host_relay_action_status(status);
        if let (Some(tool_use_id), Some(request_generation)) = (
            permission_tool_use_id.as_ref(),
            request_generation.filter(|value| *value > 0),
        ) {
            let incarnation_id = self
                .discovery
                .session(id)
                .and_then(|record| record.incarnation_id);
            let rejected = status == crate::host_protocol::RelayActionStatus::Rejected;
            if let Some(session_incarnation_id) = incarnation_id {
                let key = crate::agent_intel::permission_decision_registry::PendingKey {
                    session_id: id,
                    session_incarnation_id,
                    tool_use_id: tool_use_id.clone(),
                };
                let mutation = if rejected {
                    self.agent_intel
                        .permission_decisions
                        .mark_actionable(&key, request_generation)
                } else {
                    self.agent_intel
                        .permission_decisions
                        .mark_sending(&key, request_generation)
                };
                if mutation.changed() {
                    self.runtime_outbox.queue_pending_permissions_snapshot(
                        self.agent_intel.permission_decisions.snapshot(),
                    );
                }
            }
            let message = match status {
                crate::host_protocol::RelayActionStatus::Busy => Some(
                    "backend is busy; the remote permission decision remains queued for retry"
                        .to_owned(),
                ),
                crate::host_protocol::RelayActionStatus::Rejected => Some(
                    "backend rejected the remote permission decision before owner confirmation"
                        .to_owned(),
                ),
                crate::host_protocol::RelayActionStatus::Accepted
                | crate::host_protocol::RelayActionStatus::Duplicate => None,
            };
            self.runtime_outbox.queue_agent_intel(
                crate::AgentIntelEvent::RemotePermissionDecisionState {
                    session_id: id.to_string(),
                    session_incarnation_id: incarnation_id
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    tool_use_id: tool_use_id.clone(),
                    request_generation,
                    phase: if rejected {
                        crate::RemotePermissionDecisionPhase::Failed
                    } else {
                        crate::RemotePermissionDecisionPhase::Sending
                    },
                    status: Some(status),
                    message,
                },
            );
        }
        self.runtime_outbox
            .queue_session(HostSessionEvent::ActionResult {
                session_id: id.to_string(),
                action_id: permission_tool_use_id.unwrap_or(action_id),
                status,
            });
    }

    pub(crate) fn apply_remote_session_connection_changed(
        &mut self,
        id: SessionId,
        status: ConnectionState,
        reason: Option<String>,
    ) {
        if let Some(record) = self.discovery.session_mut(id) {
            record.connection_state = Some(status);
            record.summary.last_update = OffsetDateTime::now_utc();
            match status {
                ConnectionState::Connected => {
                    record.connection_reason = None;
                }
                ConnectionState::Offline => {
                    if let Some(reason) = reason {
                        record.connection_reason = Some(reason);
                    } else if !record.viewer_blocked {
                        record.connection_reason = None;
                    }
                }
                ConnectionState::Connecting | ConnectionState::Reconnecting => {
                    if let Some(reason) = reason {
                        record.connection_reason = Some(reason);
                    }
                }
            }
        }
    }

    pub(crate) fn apply_remote_access_state_changed(
        &mut self,
        id: SessionId,
        state: RemoteSessionAccessState,
        reason: Option<String>,
        issue: Option<RemoteSessionAccessIssue>,
    ) {
        if let Some(record) = self.discovery.session_mut(id) {
            record.access_state = Some(state);
            record.summary.last_update = OffsetDateTime::now_utc();
            match state {
                RemoteSessionAccessState::Ready => {
                    record.access_reason = None;
                    record.access_issue = None;
                    record.viewer_blocked = false;
                }
                RemoteSessionAccessState::AccessDenied => {
                    record.access_reason = reason;
                    record.access_issue = issue;
                    record.viewer_blocked = true;
                }
                RemoteSessionAccessState::RegisteringDevice
                | RemoteSessionAccessState::AwaitingKey
                | RemoteSessionAccessState::Failed => {
                    record.access_reason = reason;
                    record.access_issue = issue;
                }
            }
        }
    }

    pub(crate) fn apply_remote_access_revoked(&mut self, id: SessionId) {
        if let Some(record) = self.discovery.session_mut(id) {
            record.connection_state = Some(ConnectionState::Offline);
            record.connection_reason = None;
            record.access_state = Some(RemoteSessionAccessState::AccessDenied);
            record.access_reason = Some("Access revoked".to_owned());
            record.access_issue = None;
            record.viewer_blocked = true;
            record.summary.last_update = OffsetDateTime::now_utc();
        }
    }

    pub(crate) fn apply_host_access_revoked(&mut self, id: SessionId, revoked_user_id: &str) {
        if let Some(shared) = self.sharing.shared_sessions.get_mut(id) {
            shared.revoke_user(revoked_user_id);
        }
        self.pending_work.queue_host_key_rotation(id);
        self.record_log(format!(
            "{} revoked {} before rotating the session key",
            id.short(),
            revoked_user_id
        ));
    }

    pub(crate) fn apply_host_relay_backend_restarted(&mut self, id: SessionId) {
        self.pending_work.queue_host_key_rotation(id);
        self.record_log(format!(
            "{} backend relay restarted — reconciling with a fresh session key",
            id.short()
        ));
    }

    pub(crate) fn apply_host_key_distribution_requested(
        &mut self,
        id: SessionId,
        fence_id: String,
    ) {
        self.pending_work
            .queue_key_redistribution_fence(id, Some(fence_id));
        self.record_log(format!(
            "{} host relay requested session-key redistribution",
            id.short()
        ));
    }

    pub(crate) fn apply_host_key_rotation_required(
        &mut self,
        id: SessionId,
        reason: &str,
        fail_closed: bool,
    ) {
        self.pending_work.queue_host_key_rotation(id);
        let posture = if fail_closed {
            "remote controls refused until rotation"
        } else {
            "rotating ahead of exhaustion"
        };
        self.record_log(format!("{} {reason} — {posture}", id.short()));
    }
}

fn host_relay_action_status(status: RemoteActionStatus) -> HostRelayActionStatus {
    match status {
        RemoteActionStatus::Accepted => HostRelayActionStatus::Accepted,
        RemoteActionStatus::Duplicate => HostRelayActionStatus::Duplicate,
        RemoteActionStatus::Busy => HostRelayActionStatus::Busy,
        RemoteActionStatus::Rejected => HostRelayActionStatus::Rejected,
    }
}
