use crate::{
    AppError, Result,
    runtime::Runtime,
    session_runtime::commands::SessionInput,
    sessions_common::{normalized_session_title, session_input_command_name},
};
use kodosi_domain::{ids::SessionId, session::SessionMode};

impl Runtime {
    pub(crate) async fn rename_session(
        &mut self,
        id: SessionId,
        requested_title: &str,
    ) -> Result<()> {
        let title = normalized_session_title(requested_title)?;
        if self.state.local.sessions.record(id).is_some() {
            crate::runtime::local_sessions::rename(self, id, title).await
        } else if crate::runtime::remote_sessions::owned_remote_record(self, id).is_some() {
            crate::runtime::remote_sessions::rename(self, id, title).await
        } else {
            crate::runtime::local_sessions::rename(self, id, title).await
        }
    }

    pub(crate) fn set_session_mode(
        &self,
        id: SessionId,
        expected_runtime_incarnation_id: uuid::Uuid,
        _mode: SessionMode,
    ) -> Result<()> {
        if self.session_incarnation_id(id) != Some(expected_runtime_incarnation_id) {
            return Err(AppError::NoActiveSession);
        }
        Err(AppError::Unsupported {
            reason: "Change mode in the agent terminal. This provider does not expose a confirmed mode-control API.".to_owned(),
        })
    }

    pub(crate) async fn send_input_to_session(
        &mut self,
        id: SessionId,
        expected_runtime_incarnation_id: uuid::Uuid,
        input: SessionInput,
    ) -> Result<()> {
        if self.session_incarnation_id(id) != Some(expected_runtime_incarnation_id) {
            return Err(AppError::NoActiveSession);
        }
        let command_name = session_input_command_name(&input);
        if self.state.local.sessions.record(id).is_some() {
            self.state
                .local
                .owned_session_runtimes
                .input_router()
                .send_input(id, Some(expected_runtime_incarnation_id), input)
                .await
                .ok_or(AppError::NoActiveSession)?
        } else if crate::runtime::remote_sessions::owned_remote_record(self, id).is_some() {
            crate::runtime::remote_sessions::send_input(self, id, &input, command_name)
        } else if self.state.discovery.session(id).is_some() {
            crate::runtime::remote_sessions::send_participant_input(self, id, &input, command_name)
        } else {
            Err(AppError::NoActiveSession)
        }
    }

    pub(crate) async fn stop_session(&mut self, id: SessionId) -> Result<()> {
        if self.state.local.sessions.record(id).is_some() {
            self.state.local.stop(id, &mut self.state.logs).await?;
            self.cancel_relay_prepare_for_session(id);
            self.cancel_share_transition_for_session(id, "session stopped during share transition");
            self.retire_access_mutations_for_session(
                id,
                "session stopped before the access change settled",
            );
            Ok(())
        } else if crate::runtime::remote_sessions::owned_remote_record(self, id).is_some() {
            crate::runtime::remote_sessions::stop(self, id)
        } else if self.state.discovery.session(id).is_some() {
            Err(AppError::Unsupported {
                reason: format!(
                    "session {id} is remote and this device is not its owner; stop it from the owner runtime"
                ),
            })
        } else {
            self.state.local.stop(id, &mut self.state.logs).await
        }
    }

    pub(crate) async fn interrupt_session(&mut self, id: SessionId) -> Result<()> {
        if self.state.local.sessions.record(id).is_some() {
            self.state.local.interrupt(id, &mut self.state.logs).await
        } else if crate::runtime::remote_sessions::owned_remote_record(self, id).is_some() {
            crate::runtime::remote_sessions::interrupt(self, id)
        } else if self.state.discovery.session(id).is_some() {
            Err(AppError::Unsupported {
                reason: format!(
                    "session {id} is remote and this device is not its owner; interrupt it from the owner runtime"
                ),
            })
        } else {
            self.state.local.interrupt(id, &mut self.state.logs).await
        }
    }
}
