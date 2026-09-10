use kodosi_domain::ids::SessionId;

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
    pub(crate) fn handle_snapshot(
        &mut self,
        id: SessionId,
        payload: &agent_intel::AgentIntelSnapshot,
    ) {
        let Some(session_incarnation_id) = self
            .local_sessions
            .sessions
            .record(id)
            .map(|record| record.local_incarnation_id)
        else {
            tracing::debug!(session_id = %id, "ignored agent snapshot without a current incarnation");
            return;
        };
        let agent_session_id = payload.identity.vendor_session_id.as_deref();
        self.intel_state
            .registry
            .set_agent_session_id(id, agent_session_id);
        let Ok(legacy_payload) = serde_json::to_value(payload) else {
            tracing::warn!(session_id = %id, "could not serialize typed agent snapshot");
            return;
        };
        self.outbox.queue_agent_intel(AgentIntelEvent::Snapshot {
            session_id: id.to_string(),
            session_incarnation_id: session_incarnation_id.to_string(),
            payload: legacy_payload,
        });
        if let Some(live_set) =
            self.intel_state
                .live_authority
                .upsert(id, session_incarnation_id, payload.clone())
        {
            self.outbox.queue_agent_intel(live_set);
        }

        if let Some(title) = payload.identity.title.as_deref()
            && let Some(record) = self.local_sessions.sessions.record_mut(id)
            && !title.is_empty()
            && record.summary.title != title
        {
            title.clone_into(&mut record.summary.title);
        }
    }
}
