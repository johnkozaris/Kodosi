use std::collections::VecDeque;

use crate::{
    AppError, Result,
    agent_intel::{mode::AgentIntelModeCtx, state::AgentIntelState},
    runtime::{runtime_event_outbox::RuntimeEventOutbox, state::push_log},
    session_runtime::{
        commands::SessionInput,
        handles::{SessionPtyInstruction, SessionScreenInstruction},
    },
};
use kodosi_domain::{
    ids::SessionId,
    session::{SessionMode, SessionProvenance, SessionState},
};

use super::state::LocalSessionsState;
use crate::sessions_common::{shift_tab_bytes, shift_tab_count_for_agent};

pub(crate) fn reject_non_kodosi_local(
    id: SessionId,
    provenance: SessionProvenance,
    operation: &str,
) -> Result<()> {
    if provenance == SessionProvenance::Kodosi {
        return Ok(());
    }
    Err(AppError::Unsupported {
        reason: format!("session {} is remote; {operation}", id.short()),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalResizeOutcome {
    Applied,
    RejectedByAuthority,
    RejectedRemoteOwnerOnly,
    PendingRemote { relay_generation: u64 },
}

impl LocalSessionsState {
    pub(crate) async fn focus(
        &self,
        id: SessionId,
        client_id: String,
        _logs: &mut VecDeque<String>,
    ) -> Result<()> {
        self.owned_session_runtimes
            .send_to_screen(id, SessionScreenInstruction::Focus { client_id })
            .await
    }

    pub(crate) async fn blur(
        &self,
        id: SessionId,
        client_id: String,
        _logs: &mut VecDeque<String>,
    ) -> Result<()> {
        self.owned_session_runtimes
            .send_to_screen(id, SessionScreenInstruction::Blur { client_id })
            .await
    }

    pub(crate) async fn stop(&mut self, id: SessionId, _logs: &mut VecDeque<String>) -> Result<()> {
        if !self.owned_session_runtimes.contains(id) {
            if self.sessions.record(id).is_some_and(|record| {
                matches!(
                    record.summary.state,
                    SessionState::Stopped | SessionState::Failed
                )
            }) {
                return Ok(());
            }
            return Err(AppError::NoActiveSession);
        }

        self.owned_session_runtimes
            .send_to_pty(id, SessionPtyInstruction::Kill)
            .await?;

        self.sessions.update_state(id, SessionState::Stopping);
        Ok(())
    }

    pub(crate) async fn interrupt(
        &self,
        id: SessionId,
        _logs: &mut VecDeque<String>,
    ) -> Result<()> {
        self.owned_session_runtimes
            .send_to_pty(id, SessionPtyInstruction::Interrupt)
            .await
    }
}

pub(crate) struct LocalSessionModeCtx<'a> {
    pub(crate) local: &'a mut LocalSessionsState,
    pub(crate) intel_state: &'a mut AgentIntelState,
    pub(crate) outbox: &'a mut RuntimeEventOutbox,
    pub(crate) logs: &'a mut VecDeque<String>,
}

impl LocalSessionModeCtx<'_> {
    pub(crate) async fn set_mode(
        &mut self,
        id: SessionId,
        expected_runtime_incarnation_id: uuid::Uuid,
        mode: SessionMode,
    ) -> Result<()> {
        let record = self
            .local
            .sessions
            .record(id)
            .filter(|record| record.local_incarnation_id == expected_runtime_incarnation_id)
            .ok_or(AppError::NoActiveSession)?;

        reject_non_kodosi_local(
            id,
            record.summary.provenance,
            "desktop mode control only applies to local sessions",
        )?;

        let current_mode = record.summary.mode;
        if current_mode == mode {
            return Ok(());
        }

        let detected_agent = record.summary.detected_agent.clone();
        let shift_tab_count =
            shift_tab_count_for_agent(detected_agent.as_deref(), current_mode, mode, id)?;

        self.local
            .owned_session_runtimes
            .send_to_screen(
                id,
                SessionScreenInstruction::Input(SessionInput::new(shift_tab_bytes(
                    shift_tab_count,
                ))),
            )
            .await?;

        let still_current =
            self.local.sessions.record(id).is_some_and(|record| {
                record.local_incarnation_id == expected_runtime_incarnation_id
            });
        if !still_current {
            return Err(AppError::NoActiveSession);
        }

        AgentIntelModeCtx {
            intel_state: self.intel_state,
            local_sessions: self.local,
            outbox: self.outbox,
        }
        .apply_mode_update(id, mode);

        push_log(
            self.logs,
            format!(
                "{} switched mode from {} to {}",
                id.short(),
                crate::sessions_common::session_mode_label(current_mode),
                crate::sessions_common::session_mode_label(mode)
            ),
        );
        Ok(())
    }
}
