use std::collections::VecDeque;

use crate::{
    AppError, Result,
    session_runtime::handles::{SessionPtyInstruction, SessionScreenInstruction},
};
use kodosi_domain::{
    ids::SessionId,
    session::{SessionProvenance, SessionState},
};

use super::state::LocalSessionsState;

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
