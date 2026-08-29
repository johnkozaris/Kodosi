use std::path::Path;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    AppError, Result,
    runtime::{AppState, MAX_SESSION_TITLE_LEN, local_control::migration},
    session_runtime::{
        events::RuntimeSessionEvent,
        handles::{LaunchKind, OwnedSessionLaunch, OwnedSessionSpec},
        session_coordinator,
    },
};
use kodosi_domain::{
    ids::SessionId,
    provider_conversation::{ProviderConversationIdentity, ProviderConversationProvider},
    session::{SessionState, SessionSummary},
};

pub(crate) struct LocalSessionLifecycleCtx<'a> {
    pub(crate) state: &'a mut AppState,
    pub(crate) catalog: &'a crate::runtime::local_control::LocalSessionCatalog,
    pub(crate) session_events_tx: &'a mpsc::Sender<RuntimeSessionEvent>,
    pub(crate) shutdown: &'a CancellationToken,
    pub(crate) clipboard_available: bool,
    pub(crate) allow_terminal_clipboard_write: bool,
    pub(crate) terminal_hub: &'a crate::terminal_transport::hub::SessionHub,
}

impl LocalSessionLifecycleCtx<'_> {
    #[expect(
        clippy::too_many_lines,
        reason = "session creation composes launch, cache, and discovery ownership"
    )]
    pub(crate) async fn create(
        &mut self,
        requested_title: Option<String>,
        initial_working_dir: Option<String>,
        create_request_id: Option<String>,
        resume_source: Option<ProviderConversationIdentity>,
    ) -> Result<bool> {
        let mut initial_working_dir = normalize_working_dir(initial_working_dir);
        if let Some(request_id) = create_request_id.as_deref()
            && let Some(existing) = self
                .state
                .local
                .sessions
                .ids()
                .iter()
                .filter_map(|id| self.state.local.sessions.record(*id))
                .find(|record| record.create_request_id.as_deref() == Some(request_id))
        {
            let requested_title = requested_title
                .as_deref()
                .map(normalize_session_title)
                .ok_or_else(|| AppError::Unsupported {
                    reason: "session.create retry is missing its original name".to_owned(),
                })?;
            if existing.summary.title == requested_title
                && existing.resume_source == resume_source
                && existing.summary.working_dir == initial_working_dir
            {
                let id = existing.summary.id;
                self.state.activate_owned_session(id);
                self.state.record_log(format!(
                    "replayed duplicate session.create request {request_id} for {}",
                    id.short()
                ));
                return Ok(true);
            }
        }
        if resume_source.is_some() {
            let working_directory =
                initial_working_dir
                    .as_deref()
                    .ok_or_else(|| AppError::Unsupported {
                        reason: "provider resume requires a working directory".to_owned(),
                    })?;
            initial_working_dir = Some(
                ::agent_intel::ops::path_safety::canonicalize_user_dir(
                    "workingDirectory",
                    working_directory,
                )
                .map_err(|reason| AppError::Unsupported {
                    reason: format!("invalid workingDirectory: {reason}"),
                })?
                .to_str()
                .ok_or_else(|| AppError::Unsupported {
                    reason: "workingDirectory must be valid UTF-8".to_owned(),
                })?
                .to_owned(),
            );
        }
        if let Some(request_id) = create_request_id.as_deref()
            && let Some(existing) = self
                .state
                .local
                .sessions
                .ids()
                .iter()
                .filter_map(|id| self.state.local.sessions.record(*id))
                .find(|record| record.create_request_id.as_deref() == Some(request_id))
        {
            let requested_title = requested_title
                .as_deref()
                .map(normalize_session_title)
                .ok_or_else(|| AppError::Unsupported {
                    reason: "session.create retry is missing its original name".to_owned(),
                })?;
            if existing.summary.title != requested_title
                || existing.resume_source != resume_source
                || existing.summary.working_dir != initial_working_dir
            {
                return Err(AppError::Unsupported {
                    reason: "session.create request ID was reused with a different target"
                        .to_owned(),
                });
            }
            let id = existing.summary.id;
            self.state.activate_owned_session(id);
            self.state.record_log(format!(
                "replayed duplicate session.create request {request_id} for {}",
                id.short()
            ));
            return Ok(true);
        }
        if let Some(source) = &resume_source {
            let provider = match source.provider {
                ProviderConversationProvider::Claude => ::agent_intel::AgentKind::Claude,
                ProviderConversationProvider::Copilot => ::agent_intel::AgentKind::Copilot,
            };
            let validated_directory =
                initial_working_dir
                    .as_deref()
                    .ok_or_else(|| AppError::Unsupported {
                        reason: "validated resume directory disappeared".to_owned(),
                    })?;
            ::agent_intel::ops::provider_conversations::validate_resume_target(
                &::agent_intel::runtime::paths::home_dir(),
                provider,
                validated_directory,
                &source.native_conversation_id,
            )
            .map_err(|reason| AppError::Unsupported { reason })?;
            let external_provider = match source.provider {
                ProviderConversationProvider::Claude => {
                    ::agent_intel::ops::external_sessions::ExternalAgent::Claude
                }
                ProviderConversationProvider::Copilot => {
                    ::agent_intel::ops::external_sessions::ExternalAgent::Copilot
                }
            };
            let validated_directory =
                initial_working_dir
                    .as_deref()
                    .ok_or_else(|| AppError::Unsupported {
                        reason: "validated resume directory disappeared".to_owned(),
                    })?;
            ::agent_intel::ops::external_sessions::ensure_resume_target_inactive(
                &::agent_intel::runtime::paths::home_dir(),
                external_provider,
                &source.native_conversation_id,
                Path::new(validated_directory),
            )
            .await
            .map_err(|reason| AppError::Unsupported { reason })?;
        }
        if let Some(source) = &resume_source
            && self.state.local.sessions.ids().iter().any(|id| {
                self.state.local.sessions.record(*id).is_some_and(|record| {
                    record.resume_source.as_ref() == Some(source)
                        && self.state.local.owned_session_runtimes.contains(*id)
                })
            })
        {
            return Err(AppError::Unsupported {
                reason: "this provider conversation is already open in Kodosi".to_owned(),
            });
        }
        let title = next_session_title(self.state, requested_title);

        let id = SessionId::new();
        let runtime_name =
            build_runtime_session_name(&self.state.config.runtime.session_prefix, id);
        let mut summary = SessionSummary::new_owned(
            id,
            title,
            runtime_name,
            self.state.local.last_terminal_size,
            self.state.identity.auth.subject(),
        );
        if let Some(working_dir) = initial_working_dir.as_ref() {
            summary.working_dir = Some(working_dir.clone());
            crate::runtime::maintenance::ensure_project_discovery(
                self.state,
                self.session_events_tx,
                self.shutdown,
                working_dir,
                false,
            );
        }
        if let Some(room_name) = initial_working_dir
            .as_deref()
            .and_then(room_name_from_working_dir)
        {
            summary.room_name = Some(room_name);
        }
        let local_incarnation_id = uuid::Uuid::now_v7();
        if !self
            .terminal_hub
            .open_local_incarnation(id, local_incarnation_id, 0)
        {
            return Err(AppError::Unsupported {
                reason: format!("terminal session {id} already has conflicting live state"),
            });
        }
        if let Err(error) = self.catalog.persist_new(
            &summary,
            create_request_id.clone(),
            resume_source.clone(),
            false,
            local_incarnation_id,
        ) {
            self.terminal_hub.clone().end_session(
                id,
                &crate::terminal_transport::TerminalCloseReason::IoError(
                    "session descriptor persistence failed".to_owned(),
                ),
            );
            return Err(error);
        }
        let mut handle = session_coordinator::spawn(
            OwnedSessionSpec {
                id,
                size: self.state.local.last_terminal_size,
                initial_terminal_sequence: self.terminal_hub.next_sequence(id).unwrap_or(0),
                initial_theme_dark: self.state.local.owned_session_runtimes.host_theme_dark(),
                launch: OwnedSessionLaunch {
                    kind: if resume_source.is_some() {
                        LaunchKind::Resume
                    } else {
                        LaunchKind::Create
                    },
                    initial_working_dir,
                    initial_shell: self.state.config.runtime.initial_shell.clone(),
                    resume_source: resume_source.clone(),
                },
            },
            session_coordinator::OwnedSessionServices {
                session_events: self.session_events_tx.clone(),
                local_incarnation_id,
                cancellation: self.shutdown.child_token(),
                supports_osc52_clipboard: self.clipboard_available
                    && self.allow_terminal_clipboard_write,
                terminal_history: kodosi_session::TerminalHistoryPolicy::default(),
                terminal_hub: self.terminal_hub.clone(),
            },
        )
        .await
        .inspect_err(|_| {
            drop(self.catalog.remove_from_list(id));
            self.terminal_hub.clone().end_session(
                id,
                &crate::terminal_transport::TerminalCloseReason::IoError(
                    "session coordinator spawn failed".to_owned(),
                ),
            );
        })?;

        if let Err(error) = self.catalog.persist_new(
            &summary,
            create_request_id.clone(),
            resume_source.clone(),
            true,
            local_incarnation_id,
        ) {
            handle.cancellation.cancel();
            if tokio::time::timeout(std::time::Duration::from_secs(35), &mut handle.join_handle)
                .await
                .is_err()
            {
                handle.join_handle.abort();
                drop(handle.join_handle.await);
            }
            drop(self.catalog.remove_from_list(id));
            self.terminal_hub.clone().end_session(
                id,
                &crate::terminal_transport::TerminalCloseReason::IoError(
                    "session launch commitment failed".to_owned(),
                ),
            );
            return Err(error);
        }

        self.state.register_owned_session(
            summary.clone(),
            handle,
            create_request_id.clone(),
            local_incarnation_id,
        );
        self.state
            .local
            .sessions
            .set_resume_source(id, resume_source.clone());
        if self.state.local.sessions.record(id).is_none() {
            return Err(AppError::NoActiveSession);
        }
        tracing::info!(
            session_id = %summary.id,
            runtime_name = %summary.runtime_name,
            working_dir = ?summary.working_dir,
            size_rows = summary.size.rows(),
            size_cols = summary.size.cols(),
            "owned session registered"
        );
        self.state.record_log(format!(
            "started {} ({})",
            summary.title, summary.runtime_name
        ));
        Ok(false)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "session reopen composes terminal, cache, and identity ownership"
    )]
    pub(crate) async fn reopen(&mut self, id: SessionId) -> Result<bool> {
        if self.reopen_is_already_active(id)? {
            return Ok(false);
        }

        let record = self
            .state
            .local
            .sessions
            .record(id)
            .ok_or(AppError::NoActiveSession)?;
        let create_request_id = record.create_request_id.clone();
        let working_dir = record.summary.working_dir.clone();
        let title = record.summary.title.clone();
        let runtime_name = record.summary.runtime_name.clone();
        let size = record.summary.size;
        let owner_id = record.summary.owner_id;
        let room_name = record.summary.room_name.clone();
        let prior_close_reason = self
            .terminal_hub
            .close_reason(id)
            .unwrap_or(crate::terminal_transport::TerminalCloseReason::SessionEnded);

        let initial_terminal_sequence = 0;
        let local_incarnation_id = uuid::Uuid::now_v7();
        if !self.terminal_hub.open_local_incarnation(
            id,
            local_incarnation_id,
            initial_terminal_sequence,
        ) {
            return Err(AppError::Unsupported {
                reason: format!("terminal session {id} already has conflicting live state"),
            });
        }
        let handle = session_coordinator::spawn(
            OwnedSessionSpec {
                id,
                size,
                initial_terminal_sequence,
                initial_theme_dark: self.state.local.owned_session_runtimes.host_theme_dark(),
                launch: OwnedSessionLaunch {
                    kind: LaunchKind::Reopen,
                    initial_working_dir: working_dir.clone(),
                    initial_shell: self.state.config.runtime.initial_shell.clone(),
                    resume_source: None,
                },
            },
            session_coordinator::OwnedSessionServices {
                session_events: self.session_events_tx.clone(),
                local_incarnation_id,
                cancellation: self.shutdown.child_token(),
                supports_osc52_clipboard: self.clipboard_available
                    && self.allow_terminal_clipboard_write,
                terminal_history: kodosi_session::TerminalHistoryPolicy::default(),
                terminal_hub: self.terminal_hub.clone(),
            },
        )
        .await
        .inspect_err(|_| {
            self.terminal_hub
                .clone()
                .end_session(id, &prior_close_reason);
        })?;

        if self.state.local.sessions.record(id).is_none() {
            handle.cancellation.cancel();
            handle.join_handle.abort();
            self.terminal_hub
                .clone()
                .end_session(id, &prior_close_reason);
            return Err(AppError::NoActiveSession);
        }
        let mut summary =
            SessionSummary::new_owned(id, title.clone(), runtime_name, size, owner_id);
        summary.working_dir.clone_from(&working_dir);
        summary.room_name = room_name;

        if let Err(error) = self.catalog.persist_reopened(
            &summary,
            create_request_id,
            None,
            true,
            local_incarnation_id,
        ) {
            handle.cancellation.cancel();
            handle.join_handle.abort();
            self.terminal_hub
                .clone()
                .end_session(id, &prior_close_reason);
            return Err(error);
        }

        self.state.clear_shared_session(id);
        if self
            .state
            .replace_reopened_owned_session(summary, handle, local_incarnation_id)
        {
            self.state.record_log(format!(
                "{} dropped stale pending delete on reopen",
                id.short()
            ));
        }
        self.state.local.sessions.set_resume_source(id, None);
        if let Some(working_dir) = working_dir.as_deref() {
            crate::runtime::maintenance::ensure_project_discovery(
                self.state,
                self.session_events_tx,
                self.shutdown,
                working_dir,
                false,
            );
        }
        self.state
            .record_log(format!("resurrected {} ({})", id.short(), title));
        Ok(true)
    }

    fn reopen_is_already_active(&mut self, id: SessionId) -> Result<bool> {
        if self.state.local.owned_session_runtimes.contains(id) {
            self.state.activate_owned_session(id);
            self.state.record_log(format!(
                "{} ignored session.reopen while coordinator is still registered",
                id.short()
            ));
            return Ok(true);
        }
        let state = self
            .state
            .local
            .sessions
            .record(id)
            .map(|record| record.summary.state)
            .ok_or(AppError::NoActiveSession)?;
        Ok(match state {
            SessionState::Running
            | SessionState::Published
            | SessionState::Reconnecting
            | SessionState::Stopping
            | SessionState::Starting => {
                self.state.activate_owned_session(id);
                true
            }
            SessionState::Stopped | SessionState::Failed => false,
        })
    }

    pub(crate) fn delete(&mut self, id: SessionId) -> Result<()> {
        let record = self
            .state
            .local
            .sessions
            .record(id)
            .ok_or(AppError::NoActiveSession)?;
        let runtime_name = record.summary.runtime_name.clone();
        if self.state.local.owned_session_runtimes.contains(id) {
            if record.summary.state == SessionState::Stopping {
                self.state.queue_delete_after_stop(id);
                self.state
                    .record_log(format!("{} queued delete until stop completes", id.short()));
                return Ok(());
            }

            return Err(AppError::Unsupported {
                reason: format!(
                    "session {id} is still active; stop it before deleting resurrectable state"
                ),
            });
        }

        self.catalog.remove_from_list(id)?;
        if self.state.delete_resurrectable_session_state(id) {
            if let Err(error) = migration::delete_cached_session(
                self.state.session_cache_root.as_deref(),
                &runtime_name,
            ) {
                self.state.record_log(format!(
                    "{} failed to delete resurrection cache: {error}",
                    id.short()
                ));
            }
            self.state.record_log(format!(
                "{} deleted resurrectable session state",
                id.short()
            ));
        }
        Ok(())
    }
}

fn normalize_session_title(title: &str) -> String {
    if title.len() > MAX_SESSION_TITLE_LEN {
        title[..title.floor_char_boundary(MAX_SESSION_TITLE_LEN)].to_owned()
    } else {
        title.to_owned()
    }
}

fn next_session_title(state: &mut AppState, requested_title: Option<String>) -> String {
    requested_title.map_or_else(
        || {
            let number = state.local.next_session_number;
            state.local.next_session_number += 1;
            format!("Owned Session {number}")
        },
        |title| normalize_session_title(&title),
    )
}

fn normalize_working_dir(working_dir: Option<String>) -> Option<String> {
    let normalized = working_dir?.trim().to_owned();
    (!normalized.is_empty()).then_some(normalized)
}

fn build_runtime_session_name(prefix: &str, id: SessionId) -> String {
    let cleaned_prefix = prefix
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    format!("{}-{}", cleaned_prefix, id.simple())
}

fn room_name_from_working_dir(working_dir: &str) -> Option<String> {
    Path::new(working_dir)
        .file_name()
        .and_then(|value| value.to_str())
        .map(str::to_owned)
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::normalize_session_title;

    #[test]
    fn session_create_target_uses_first_admission_title_normalization() {
        let oversized = "a".repeat(crate::runtime::MAX_SESSION_TITLE_LEN + 1);
        assert_eq!(
            normalize_session_title(&oversized).len(),
            crate::runtime::MAX_SESSION_TITLE_LEN
        );
    }
}
