use kodosi_domain::ids::SessionId;
use tokio::sync::mpsc;

use super::state::AgentIntelState;
use super::task;
use crate::host_protocol::AgentIntelEvent;
use crate::local_sessions::state::LocalSessionsState;
use crate::runtime::runtime_event_outbox::RuntimeEventOutbox;
use crate::session_runtime::events::RuntimeSessionEvent;

pub(crate) struct AgentIntelLifecycleCtx<'a> {
    pub(crate) intel_state: &'a mut AgentIntelState,
    pub(crate) local_sessions: &'a LocalSessionsState,
    pub(crate) session_events: &'a mpsc::Sender<RuntimeSessionEvent>,
    pub(crate) outbox: &'a mut RuntimeEventOutbox,
}

impl AgentIntelLifecycleCtx<'_> {
    pub(crate) fn spawn(&mut self, session_id: SessionId, agent_name: &str) {
        let Ok(cancellation) = self
            .local_sessions
            .owned_session_runtimes
            .child_cancellation_token(session_id)
        else {
            return;
        };

        let Some(local_incarnation_id) = self
            .local_sessions
            .sessions
            .record(session_id)
            .map(|record| record.local_incarnation_id)
        else {
            return;
        };
        let cwd = self
            .local_sessions
            .sessions
            .record(session_id)
            .and_then(|r| r.summary.working_dir.clone());

        let generation = self.intel_state.registry.reserve_generation(session_id);
        if let Some(handle) = task::try_spawn(
            session_id,
            local_incarnation_id,
            generation,
            agent_name,
            cwd.as_deref(),
            self.session_events.clone(),
            &cancellation,
            self.intel_state.permission_decisions.clone(),
            self.intel_state.permission_timeout,
        ) {
            self.intel_state.registry.attach(session_id, handle);
        } else {
            self.intel_state
                .registry
                .retire_generation(session_id, generation);
        }
    }

    pub(crate) fn restart_task(&mut self, session_id: SessionId) -> Option<uuid::Uuid> {
        self.teardown_task(session_id, false)
    }

    pub(crate) fn teardown_session(&mut self, session_id: SessionId) {
        self.teardown_task(session_id, true);
    }

    fn teardown_task(&mut self, session_id: SessionId, retire_session: bool) -> Option<uuid::Uuid> {
        let local_incarnation_id = self
            .local_sessions
            .sessions
            .record(session_id)
            .map(|record| record.local_incarnation_id);
        let session_incarnation_id = local_incarnation_id.map(|value| value.to_string());
        if retire_session {
            self.intel_state.registry.detach(session_id);
        } else {
            self.intel_state.registry.detach_task(session_id);
        }

        if let Some(local_incarnation_id) = local_incarnation_id {
            let mutation = if retire_session {
                self.intel_state
                    .permission_decisions
                    .retire_incarnation(session_id, local_incarnation_id)
            } else {
                self.intel_state
                    .permission_decisions
                    .clear_pending_for_incarnation(session_id, local_incarnation_id)
            };
            if !retire_session || mutation.changed() {
                self.outbox.queue_pending_permissions_snapshot(
                    self.intel_state.permission_decisions.snapshot(),
                );
            }
        }
        tracing::info!(
            session_id = %session_id,
            retire_session,
            "tore down agent intelligence task"
        );
        if let Some(session_incarnation_id) = session_incarnation_id {
            self.outbox.queue_agent_intel(AgentIntelEvent::Cleared {
                session_id: session_id.to_string(),
                session_incarnation_id: session_incarnation_id.clone(),
            });
            if let Ok(incarnation_id) = uuid::Uuid::parse_str(&session_incarnation_id)
                && let Some(live_set) = self
                    .intel_state
                    .live_authority
                    .clear(session_id, incarnation_id)
            {
                self.outbox.queue_agent_intel(live_set);
            }
        }
        local_incarnation_id
    }
}
