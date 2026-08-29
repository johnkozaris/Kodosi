use time::OffsetDateTime;

use kodosi_domain::{ids::SessionId, session::SessionMode};

use super::state::AgentIntelState;
use crate::{
    AgentIntelEvent, local_sessions::state::LocalSessionsState,
    runtime::runtime_event_outbox::RuntimeEventOutbox,
};

pub(crate) struct AgentIntelModeCtx<'a> {
    pub(crate) intel_state: &'a mut AgentIntelState,
    pub(crate) local_sessions: &'a mut LocalSessionsState,
    pub(crate) outbox: &'a mut RuntimeEventOutbox,
}

impl AgentIntelModeCtx<'_> {
    pub(crate) fn handle_snapshot(&mut self, id: SessionId, payload: &serde_json::Value) {
        let Some(session_incarnation_id) = self
            .local_sessions
            .sessions
            .record(id)
            .map(|record| record.local_incarnation_id.to_string())
        else {
            tracing::debug!(session_id = %id, "ignored agent snapshot without a current incarnation");
            return;
        };
        let agent_session_id = payload
            .get("identity")
            .and_then(|identity| identity.get("vendorSessionId"))
            .and_then(serde_json::Value::as_str);
        self.intel_state
            .registry
            .set_agent_session_id(id, agent_session_id);
        self.outbox.queue_agent_intel(AgentIntelEvent::Snapshot {
            session_id: id.to_string(),
            session_incarnation_id,
            payload: payload.clone(),
        });

        if let Some(title) = payload
            .get("identity")
            .and_then(|identity| identity.get("title"))
            .and_then(serde_json::Value::as_str)
            && let Some(record) = self.local_sessions.sessions.record_mut(id)
            && !title.is_empty()
            && record.summary.title != title
        {
            title.clone_into(&mut record.summary.title);
        }
    }

    pub(crate) fn apply_mode_update(&mut self, id: SessionId, mode: SessionMode) {
        let Some(record) = self.local_sessions.sessions.record_mut(id) else {
            return;
        };

        if record.summary.mode == mode {
            return;
        }

        record.summary.mode = mode;
        record.summary.last_update = OffsetDateTime::now_utc();
    }
}
