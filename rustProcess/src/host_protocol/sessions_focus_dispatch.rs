use crate::{AppError, Result, remote_sessions::focus::FocusTransition, runtime::Runtime};
use kodosi_domain::ids::SessionId;

impl Runtime {
    pub(crate) async fn focus_session(&mut self, id: SessionId, client_id: String) -> Result<()> {
        if self.state.local.sessions.record(id).is_some() {
            self.apply_local_focus(id, client_id, true).await
        } else if crate::runtime::remote_sessions::owned_remote_record(self, id).is_some() {
            crate::runtime::remote_sessions::focus(self, id, &client_id)
        } else if self.state.discovery.session(id).is_some() {
            crate::runtime::remote_sessions::focus_participant(self, id, &client_id)
        } else {
            Err(AppError::NoActiveSession)
        }
    }

    pub(crate) async fn blur_session(&mut self, id: SessionId, client_id: String) -> Result<()> {
        if self.state.local.sessions.record(id).is_some() {
            self.apply_local_focus(id, client_id, false).await
        } else if crate::runtime::remote_sessions::owned_remote_record(self, id).is_some() {
            crate::runtime::remote_sessions::blur(self, id, &client_id)
        } else if self.state.discovery.session(id).is_some() {
            crate::runtime::remote_sessions::blur_participant(self, id, &client_id)
        } else {
            Err(AppError::NoActiveSession)
        }
    }

    async fn apply_local_focus(
        &mut self,
        id: SessionId,
        client_id: String,
        focused: bool,
    ) -> Result<()> {
        let transition = if focused {
            self.client_focus.note_focus(id, client_id.clone())
        } else {
            self.client_focus.note_blur(id, &client_id)
        };

        let result = if focused {
            self.state
                .local
                .focus(id, client_id.clone(), &mut self.state.logs)
                .await
        } else {
            self.state
                .local
                .blur(id, client_id.clone(), &mut self.state.logs)
                .await
        };

        if result.is_err() && matches!(transition, FocusTransition::Send { .. }) {
            if focused {
                let _ = self.client_focus.note_blur(id, &client_id);
            } else {
                let _ = self.client_focus.note_focus(id, client_id);
            }
        }
        result
    }

    pub(crate) async fn release_focus_if_disconnected(&mut self, id: SessionId) {
        if self.terminal_hub.connection_count(id) > 0 {
            return;
        }
        for client_id in self.client_focus.clients(id) {
            if let Err(error) = self.blur_session(id, client_id.clone()).await {
                tracing::debug!(
                    session_id = %id,
                    %client_id,
                    %error,
                    "could not release focus held by a disconnected client",
                );

                let _ = self.client_focus.note_blur(id, &client_id);
            }
        }
    }
}
