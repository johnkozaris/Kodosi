use std::{collections::VecDeque, time::Duration};

use tokio::time;
use uuid::Uuid;

use crate::{
    AppError, AuthCommand, HostEvent, RelayActionStatus, Result, RoomActionStatus, RoomCommand,
    RoomEvent, RuntimeSessionStatus, SessionCommand, SessionEvent, SessionListEntry, SystemCommand,
    TrustCommand, TrustEvent,
    support::io::framed_json::{self, FramedJsonStream},
};
use kodosi_domain::{permissions::ShareScope, session::SessionMode};

use super::{
    handshake::{ControlClientFrame, ControlServerFrame},
    local_endpoint::LocalStream,
    snapshot::{HeadlessHostState, apply_snapshot_message},
};

const SESSION_UPDATE_TIMEOUT: Duration = Duration::from_secs(15);
const ROOM_ACTION_SETTLEMENT_TIMEOUT_SECS: u64 = 75;
const ROOM_ACTION_SETTLEMENT_TIMEOUT: Duration =
    Duration::from_secs(ROOM_ACTION_SETTLEMENT_TIMEOUT_SECS);
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(15);
const PENDING_EVENT_CAPACITY: usize = 512;

const HOST_SILENCE_TIMEOUT_SECS: u64 = 15;
const HOST_SILENCE_TIMEOUT: Duration = Duration::from_secs(HOST_SILENCE_TIMEOUT_SECS);
pub(crate) const ROOM_ACTION_WAIT_BOUND_SECS: u64 =
    HOST_SILENCE_TIMEOUT_SECS + ROOM_ACTION_SETTLEMENT_TIMEOUT_SECS;

#[derive(Debug)]
pub(crate) struct HeadlessHostClient {
    pub(in crate::headless_host) framed: FramedJsonStream<LocalStream>,
    pub(in crate::headless_host) account_user_id: Option<kodosi_domain::ids::UserId>,
    pub(in crate::headless_host) account_epoch: u64,
    pub(in crate::headless_host) remote_operations_ready: bool,
    pub(in crate::headless_host) pending_events: VecDeque<HostEvent>,
}

impl HeadlessHostClient {
    pub(crate) const fn account_user_id(&self) -> Option<kodosi_domain::ids::UserId> {
        self.account_user_id
    }

    pub(crate) const fn remote_operations_ready(&self) -> bool {
        self.remote_operations_ready
    }

    async fn send_control_value(&mut self, command: serde_json::Value) -> Result<()> {
        framed_json::send_json(&mut self.framed, &ControlClientFrame::Command { command }).await
    }

    pub(crate) async fn send_system(&mut self, message: SystemCommand) -> Result<()> {
        self.send_control_value(serde_json::to_value(message)?)
            .await
    }

    pub(crate) async fn send_auth(&mut self, message: AuthCommand) -> Result<()> {
        self.send_control_value(serde_json::to_value(message)?)
            .await
    }

    pub(crate) async fn send_session(&mut self, message: SessionCommand) -> Result<()> {
        self.send_control_value(serde_json::to_value(message)?)
            .await
    }

    pub(crate) async fn send_room(&mut self, message: RoomCommand) -> Result<()> {
        self.send_control_value(serde_json::to_value(message)?)
            .await
    }

    pub(crate) async fn send_room_action(&mut self, command: RoomCommand) -> Result<()> {
        command.validate().map_err(|error| AppError::Unsupported {
            reason: format!("invalid room command: {error}"),
        })?;
        let request_id = command
            .request_id()
            .ok_or_else(|| AppError::Unsupported {
                reason: "room action requires a request ID".to_owned(),
            })?
            .to_owned();
        let operation = command.operation().to_owned();
        let room_id = command
            .room_id_for_error()
            .ok_or_else(|| AppError::Unsupported {
                reason: "room action requires a room ID".to_owned(),
            })?;
        self.send_room(command).await?;

        let mut wait = RoomActionWait::new(request_id, operation, room_id);
        loop {
            let remaining = wait
                .deadline()
                .saturating_duration_since(time::Instant::now());
            let Some(event) = time::timeout(remaining, self.recv())
                .await
                .map_err(|_| wait.gave_up())??
            else {
                return Err(AppError::Unsupported {
                    reason: format!("headless host closed before settling {}", wait.operation()),
                });
            };
            match wait.observe(&event) {
                RoomActionWaitStep::Settled => return Ok(()),
                RoomActionWaitStep::Failed(error) => return Err(error),
                RoomActionWaitStep::Pending => {}
            }
        }
    }

    pub(crate) async fn send_trust(&mut self, message: TrustCommand) -> Result<()> {
        self.send_control_value(serde_json::to_value(message)?)
            .await
    }

    pub(crate) async fn recv(&mut self) -> Result<Option<HostEvent>> {
        if let Some(event) = self.pending_events.pop_front() {
            return Ok(Some(event));
        }
        loop {
            let Some(frame) =
                framed_json::next_json::<_, ControlServerFrame>(&mut self.framed).await?
            else {
                return Ok(None);
            };
            if let ControlServerFrame::Event { event } = frame {
                return Ok(Some(*event));
            }
        }
    }

    pub(crate) async fn snapshot(&mut self) -> Result<HeadlessHostState> {
        let refresh_id = Uuid::now_v7();
        framed_json::send_json(
            &mut self.framed,
            &ControlClientFrame::Snapshot { refresh_id },
        )
        .await?;
        let deadline = time::Instant::now() + SNAPSHOT_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(time::Instant::now());
            let Some(frame) = time::timeout(
                remaining,
                framed_json::next_json::<_, ControlServerFrame>(&mut self.framed),
            )
            .await
            .map_err(|_| AppError::Unsupported {
                reason: "timed out waiting for correlated headless host state".to_owned(),
            })??
            else {
                return Err(AppError::Unsupported {
                    reason: "headless host closed before returning correlated state".to_owned(),
                });
            };
            match frame {
                ControlServerFrame::Snapshot {
                    refresh_id: response_id,
                    account_user_id,
                    account_epoch,
                    remote_operations_ready,
                    auth,
                    sessions,
                    rooms,
                } if response_id == refresh_id => {
                    let parsed_account = account_user_id
                        .as_deref()
                        .map(kodosi_domain::ids::UserId::try_from)
                        .transpose()
                        .map_err(|error| AppError::InvalidBackendData {
                            field: "snapshot.accountUserId".to_owned(),
                            reason: error.to_string(),
                        })?;
                    if account_epoch != self.account_epoch || parsed_account != self.account_user_id
                    {
                        return Err(AppError::Unsupported {
                            reason: "headless host account changed during snapshot".to_owned(),
                        });
                    }
                    self.remote_operations_ready = remote_operations_ready;
                    let mut snapshot = HeadlessHostState {
                        sessions,
                        rooms,
                        ..HeadlessHostState::default()
                    };
                    apply_snapshot_message(&mut snapshot, HostEvent::Auth(auth))?;
                    return Ok(snapshot);
                }
                ControlServerFrame::Event { event } => {
                    if self.pending_events.len() == PENDING_EVENT_CAPACITY {
                        return Err(AppError::Unsupported {
                            reason: "headless host event backlog filled during snapshot; reconnect to refresh state"
                                .to_owned(),
                        });
                    }
                    self.pending_events.push_back(*event);
                }
                ControlServerFrame::Snapshot { .. } => {}
            }
        }
    }

    pub(crate) async fn create_session(
        &mut self,
        name: String,
        working_dir: Option<String>,
    ) -> Result<SessionListEntry> {
        let request_id = Uuid::now_v7().simple().to_string();
        self.send_session(SessionCommand::Create {
            request_id: request_id.clone(),
            name,
            working_dir,
            resume: None,
        })
        .await?;
        self.wait_for_session_by_request_id(&request_id).await
    }

    pub(crate) async fn reset_trust(&mut self, user_id: &str) -> Result<bool> {
        let request_id = Uuid::now_v7().simple().to_string();
        self.send_trust(TrustCommand::Reset {
            request_id: request_id.clone(),
            user_id: user_id.to_owned(),
        })
        .await?;
        let deadline = time::Instant::now() + SESSION_UPDATE_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(time::Instant::now());
            let Some(message) = time::timeout(remaining, self.recv()).await.map_err(|_| {
                AppError::Unsupported {
                    reason: format!("timed out waiting for trust reset ({user_id})"),
                }
            })??
            else {
                return Err(AppError::Unsupported {
                    reason: "headless host closed the connection".to_owned(),
                });
            };
            if let Some(outcome) = Self::trust_reset_event_outcome(message, &request_id, user_id) {
                return outcome;
            }
        }
    }

    pub(crate) async fn update_session_mode(
        &mut self,
        session_id: &str,
        mode: SessionMode,
    ) -> Result<SessionListEntry> {
        let incarnation = self.exact_session_incarnation(session_id).await?;
        self.send_session(SessionCommand::SetMode {
            session_id: session_id.to_owned(),
            expected_runtime_incarnation_id: incarnation.to_string(),
            mode,
        })
        .await?;
        self.wait_for_session_condition(session_id, |session| session.mode() == mode)
            .await
    }

    pub(super) fn trust_reset_event_outcome(
        message: HostEvent,
        request_id: &str,
        user_id: &str,
    ) -> Option<Result<bool>> {
        match message {
            HostEvent::Trust(TrustEvent::Reset {
                request_id: result_request_id,
                user_id: reset_user_id,
                cleared,
            }) if result_request_id == request_id && reset_user_id == user_id => Some(Ok(cleared)),
            HostEvent::Trust(TrustEvent::Error {
                request_id: Some(error_request_id),
                user_id: Some(error_user_id),
                operation,
                message,
            }) if operation == "reset"
                && error_request_id == request_id
                && error_user_id == user_id =>
            {
                Some(Err(AppError::Unsupported { reason: message }))
            }
            _ => None,
        }
    }

    pub(crate) async fn rename_session(
        &mut self,
        session_id: &str,
        name: String,
    ) -> Result<SessionListEntry> {
        self.send_session(SessionCommand::Rename {
            session_id: session_id.to_owned(),
            name: name.clone(),
        })
        .await?;
        self.wait_for_session_condition(session_id, |session| session.name() == name)
            .await
    }

    pub(crate) async fn update_session_scope(
        &mut self,
        session_id: &str,
        scope: ShareScope,
        room_id: Option<String>,
    ) -> Result<SessionListEntry> {
        let request_id = Uuid::now_v7().simple().to_string();
        let expected_runtime_incarnation_id = self
            .snapshot()
            .await?
            .sessions
            .into_iter()
            .find(|session| session.id() == session_id)
            .and_then(|session| session.incarnation_id().map(ToOwned::to_owned))
            .ok_or_else(|| AppError::Unsupported {
                reason: format!(
                    "session {session_id} has no current incarnation; refresh before changing scope"
                ),
            })?;
        self.send_session(SessionCommand::SetShareScope {
            request_id: request_id.clone(),
            session_id: session_id.to_owned(),
            expected_runtime_incarnation_id,
            scope,
            room_id: room_id.clone(),
        })
        .await?;

        let mut wait = ScopeWait::new(request_id, session_id.to_owned());
        loop {
            let event = self.recv_before(&wait).await?;
            match wait.observe(&event) {
                ScopeWaitStep::Settled => break,
                ScopeWaitStep::Failed(error) => return Err(error),
                ScopeWaitStep::Pending => {}
            }
        }

        self.wait_for_session_condition(session_id, |session| {
            session.scope() == scope
                && (scope != ShareScope::Room || session.room_id() == room_id.as_deref())
        })
        .await
    }

    async fn recv_before(&mut self, wait: &ScopeWait) -> Result<HostEvent> {
        let remaining = wait
            .deadline()
            .saturating_duration_since(time::Instant::now());
        let Some(message) = time::timeout(remaining, self.recv())
            .await
            .map_err(|_| wait.gave_up())??
        else {
            return Err(AppError::Unsupported {
                reason: "headless host closed the connection".to_owned(),
            });
        };
        Ok(message)
    }

    async fn exact_session_incarnation(&mut self, session_id: &str) -> Result<Uuid> {
        let snapshot = self.snapshot().await?;
        let value = snapshot
            .sessions
            .iter()
            .find(|session| session.id() == session_id)
            .and_then(SessionListEntry::incarnation_id)
            .ok_or_else(|| AppError::Unsupported {
                reason: format!(
                    "session {session_id} has no current incarnation; refresh before acting"
                ),
            })?;
        Uuid::parse_str(value).map_err(|error| AppError::InvalidBackendData {
            field: "session.incarnationId".to_owned(),
            reason: error.to_string(),
        })
    }

    pub(crate) async fn stop_session(&mut self, session_id: &str) -> Result<()> {
        let incarnation = self.exact_session_incarnation(session_id).await?;
        let request_id = Uuid::now_v7().to_string();
        self.send_session(SessionCommand::Stop {
            request_id: request_id.clone(),
            session_id: session_id.to_owned(),
            expected_runtime_incarnation_id: incarnation.to_string(),
        })
        .await?;
        self.wait_for_session_stopped_or_removed(&request_id, session_id)
            .await
    }

    pub(crate) async fn interrupt_session(&mut self, session_id: &str) -> Result<()> {
        let incarnation = self.exact_session_incarnation(session_id).await?;
        let request_id = Uuid::now_v7().to_string();
        self.send_session(SessionCommand::Interrupt {
            request_id: request_id.clone(),
            session_id: session_id.to_owned(),
            expected_runtime_incarnation_id: incarnation.to_string(),
        })
        .await?;
        self.wait_for_interrupt_result(&request_id, session_id)
            .await
    }

    pub(crate) async fn reopen_session(&mut self, session_id: &str) -> Result<SessionListEntry> {
        let incarnation = self.exact_session_incarnation(session_id).await?;
        self.send_session(SessionCommand::Reopen {
            request_id: Uuid::now_v7().to_string(),
            session_id: session_id.to_owned(),
            expected_runtime_incarnation_id: incarnation.to_string(),
        })
        .await?;
        self.wait_for_session_condition(session_id, |session| {
            matches!(
                session.status(),
                RuntimeSessionStatus::Active
                    | RuntimeSessionStatus::Waiting
                    | RuntimeSessionStatus::Blocked
                    | RuntimeSessionStatus::Reconnecting
            )
        })
        .await
    }

    pub(crate) async fn delete_session(&mut self, session_id: &str) -> Result<()> {
        let incarnation = self.exact_session_incarnation(session_id).await?;
        self.send_session(SessionCommand::Delete {
            request_id: Uuid::now_v7().to_string(),
            session_id: session_id.to_owned(),
            expected_runtime_incarnation_id: incarnation.to_string(),
        })
        .await?;
        self.wait_for_session_removal(session_id, "deletion", "session.delete")
            .await
    }

    pub(crate) async fn leave_session(&mut self, session_id: &str) -> Result<()> {
        let snapshot = self.snapshot().await?;
        let incarnation_id = snapshot
            .sessions
            .iter()
            .find(|session| session.id() == session_id)
            .and_then(SessionListEntry::incarnation_id)
            .ok_or_else(|| AppError::Unsupported {
                reason: format!(
                    "session {session_id} has no current incarnation; refresh before leaving"
                ),
            })?;
        self.send_session(SessionCommand::Leave {
            mutation_id: Uuid::now_v7().to_string(),
            session_id: session_id.to_owned(),
            expected_runtime_incarnation_id: incarnation_id.to_owned(),
        })
        .await?;
        self.wait_for_session_removal(session_id, "leave", "session.leave")
            .await
    }

    async fn wait_for_interrupt_result(
        &mut self,
        request_id: &str,
        session_id: &str,
    ) -> Result<()> {
        let deadline = time::Instant::now() + SESSION_UPDATE_TIMEOUT;
        while time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(time::Instant::now());
            let Some(message) = time::timeout(remaining, self.recv()).await.map_err(|_| {
                AppError::Unsupported {
                    reason: format!("timed out waiting for session interrupt ({session_id})"),
                }
            })??
            else {
                return Err(AppError::Unsupported {
                    reason: "headless host closed before confirming the interrupt".to_owned(),
                });
            };
            match message {
                HostEvent::Session(SessionEvent::Interrupted {
                    request_id: completed,
                    session_id: completed_session,
                    ..
                }) if completed == request_id && completed_session == session_id => return Ok(()),
                HostEvent::Session(SessionEvent::Error {
                    request_id: Some(rejected),
                    message,
                    ..
                }) if rejected == request_id => {
                    return Err(AppError::Unsupported { reason: message });
                }
                _ => {}
            }
        }
        Err(AppError::Unsupported {
            reason: format!("timed out waiting for session interrupt ({session_id})"),
        })
    }

    async fn wait_for_session_removal(
        &mut self,
        session_id: &str,
        operation: &str,
        error_operation: &str,
    ) -> Result<()> {
        let deadline = time::Instant::now() + SESSION_UPDATE_TIMEOUT;
        while time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(time::Instant::now());
            let Some(message) = time::timeout(remaining, self.recv()).await.map_err(|_| {
                AppError::Unsupported {
                    reason: format!("timed out waiting for session {operation} ({session_id})"),
                }
            })??
            else {
                return Err(AppError::Unsupported {
                    reason: "headless host closed the connection".to_owned(),
                });
            };
            match message {
                HostEvent::Session(SessionEvent::Removed {
                    session_id: removed,
                }) if removed == session_id => return Ok(()),
                HostEvent::Session(SessionEvent::List { sessions })
                    if sessions.iter().all(|session| session.id() != session_id) =>
                {
                    return Ok(());
                }
                HostEvent::Session(SessionEvent::Error {
                    operation,
                    session_id: error_session_id,
                    message,
                    ..
                }) if operation == error_operation
                    && error_session_id.as_deref() == Some(session_id) =>
                {
                    return Err(AppError::Unsupported { reason: message });
                }
                _ => {}
            }
        }
        Err(AppError::Unsupported {
            reason: format!("timed out waiting for session {operation} ({session_id})"),
        })
    }

    async fn session_is_stopped_or_removed(&mut self, session_id: &str) -> Result<bool> {
        let snapshot = self.snapshot().await?;
        Ok(snapshot
            .sessions
            .iter()
            .find(|session| session.id() == session_id)
            .is_none_or(|session| matches!(session.status(), RuntimeSessionStatus::Stopped)))
    }

    async fn wait_for_session_stopped_or_removed(
        &mut self,
        request_id: &str,
        session_id: &str,
    ) -> Result<()> {
        if self.session_is_stopped_or_removed(session_id).await? {
            return Ok(());
        }

        let deadline = time::Instant::now() + SESSION_UPDATE_TIMEOUT;
        while time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(time::Instant::now());
            let Some(message) = time::timeout(remaining, self.recv()).await.map_err(|_| {
                AppError::Unsupported {
                    reason: format!("timed out waiting for session stop ({session_id})"),
                }
            })??
            else {
                return Err(AppError::Unsupported {
                    reason: "headless host closed the connection".to_owned(),
                });
            };

            match message {
                HostEvent::Session(SessionEvent::List { sessions })
                    if sessions
                        .iter()
                        .find(|session| session.id() == session_id)
                        .is_none_or(|session| {
                            matches!(session.status(), RuntimeSessionStatus::Stopped)
                        }) =>
                {
                    if self.session_is_stopped_or_removed(session_id).await? {
                        return Ok(());
                    }
                }
                HostEvent::Session(SessionEvent::Upsert { session })
                    if session.id() == session_id
                        && matches!(session.status(), RuntimeSessionStatus::Stopped) =>
                {
                    if self.session_is_stopped_or_removed(session_id).await? {
                        return Ok(());
                    }
                }
                HostEvent::Session(SessionEvent::Removed {
                    session_id: removed,
                }) if removed == session_id => {
                    if self.session_is_stopped_or_removed(session_id).await? {
                        return Ok(());
                    }
                }
                HostEvent::Session(SessionEvent::Error {
                    request_id: Some(rejected),
                    message,
                    ..
                }) if rejected == request_id => {
                    return Err(AppError::Unsupported { reason: message });
                }
                _ => {}
            }
        }

        Err(AppError::Unsupported {
            reason: format!("timed out waiting for session stop ({session_id})"),
        })
    }

    pub(crate) async fn open_remote_session(&mut self, session_id: &str) -> Result<()> {
        self.send_session(SessionCommand::OpenRemote {
            session_id: session_id.to_owned(),
        })
        .await?;
        let deadline = time::Instant::now() + SESSION_UPDATE_TIMEOUT;
        while time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(time::Instant::now());
            let Some(message) = time::timeout(remaining, self.recv()).await.map_err(|_| {
                AppError::Unsupported {
                    reason: format!(
                        "timed out waiting for session.opened confirmation ({session_id})"
                    ),
                }
            })??
            else {
                return Err(AppError::Unsupported {
                    reason: "headless host closed the connection while opening remote session"
                        .to_owned(),
                });
            };
            match message {
                HostEvent::Session(SessionEvent::Opened { session_id: opened })
                    if opened == session_id =>
                {
                    return Ok(());
                }
                HostEvent::Session(SessionEvent::Error {
                    operation,
                    session_id: error_session_id,
                    message,
                    ..
                }) if operation == "session.openRemote"
                    && error_session_id.as_deref() == Some(session_id) =>
                {
                    return Err(AppError::Unsupported {
                        reason: format!("host rejected session.openRemote: {message}"),
                    });
                }
                _ => {}
            }
        }
        Err(AppError::Unsupported {
            reason: format!("timed out waiting for session.opened ({session_id})"),
        })
    }

    pub(crate) async fn wait_for_session_by_request_id(
        &mut self,
        request_id: &str,
    ) -> Result<SessionListEntry> {
        self.wait_for_session_condition(request_id, |session| {
            session.create_request_id() == Some(request_id)
        })
        .await
    }

    pub(crate) async fn wait_for_session_condition<F>(
        &mut self,
        identifier: &str,
        predicate: F,
    ) -> Result<SessionListEntry>
    where
        F: Fn(&SessionListEntry) -> bool,
    {
        let deadline = time::Instant::now() + SESSION_UPDATE_TIMEOUT;
        while time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(time::Instant::now());
            let Some(message) = time::timeout(remaining, self.recv()).await.map_err(|_| {
                AppError::Unsupported {
                    reason: format!("timed out waiting for session update ({identifier})"),
                }
            })??
            else {
                return Err(AppError::Unsupported {
                    reason: "headless host closed the connection".to_owned(),
                });
            };

            match message {
                HostEvent::Session(SessionEvent::List { sessions }) => {
                    if let Some(session) = sessions.into_iter().find(|session| {
                        session_matches_identifier(session, identifier) && predicate(session)
                    }) {
                        return Ok(session);
                    }
                }
                HostEvent::Session(SessionEvent::Upsert { session })
                    if session_matches_identifier(&session, identifier) && predicate(&session) =>
                {
                    return Ok(*session);
                }
                HostEvent::Session(SessionEvent::ActionResult {
                    status: RelayActionStatus::Rejected | RelayActionStatus::Busy,
                    action_id,
                    session_id,
                }) if session_id == identifier => {
                    return Err(AppError::Unsupported {
                        reason: format!("host rejected remote action {action_id}"),
                    });
                }
                HostEvent::Session(SessionEvent::Error {
                    operation,
                    session_id,
                    request_id,
                    message,
                }) if session_error_matches_identifier(
                    session_id.as_deref(),
                    request_id.as_deref(),
                    identifier,
                ) =>
                {
                    let detail = [Some(operation), session_id, request_id]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(":");
                    let reason = if detail.is_empty() {
                        message
                    } else {
                        format!("{detail}: {message}")
                    };
                    return Err(AppError::Unsupported { reason });
                }
                _ => {}
            }
        }

        Err(AppError::Unsupported {
            reason: format!("timed out waiting for session update ({identifier})"),
        })
    }
}

pub(in crate::headless_host) fn session_error_matches_identifier(
    session_id: Option<&str>,
    request_id: Option<&str>,
    identifier: &str,
) -> bool {
    session_id == Some(identifier) || request_id == Some(identifier)
}

#[derive(Debug)]
pub(in crate::headless_host) enum RoomActionWaitStep {
    Pending,
    Settled,
    Failed(AppError),
}

#[derive(Debug)]
pub(in crate::headless_host) struct RoomActionWait {
    request_id: String,
    operation: String,
    room_id: String,
    accepted_fingerprint: Option<String>,
    deadline: time::Instant,
}

impl RoomActionWait {
    pub(in crate::headless_host) fn new(
        request_id: String,
        operation: String,
        room_id: String,
    ) -> Self {
        Self {
            request_id,
            operation,
            room_id,
            accepted_fingerprint: None,
            deadline: time::Instant::now() + HOST_SILENCE_TIMEOUT,
        }
    }

    pub(in crate::headless_host) const fn deadline(&self) -> time::Instant {
        self.deadline
    }

    pub(in crate::headless_host) fn operation(&self) -> &str {
        &self.operation
    }

    pub(in crate::headless_host) fn gave_up(&self) -> AppError {
        AppError::Unsupported {
            reason: if self.accepted_fingerprint.is_some() {
                format!(
                    "headless host did not settle {} after acceptance",
                    self.operation
                )
            } else {
                format!("headless host never accepted {}", self.operation)
            },
        }
    }

    pub(in crate::headless_host) fn observe(&mut self, event: &HostEvent) -> RoomActionWaitStep {
        match event {
            HostEvent::Room(RoomEvent::ActionAccepted {
                request_id,
                operation,
                room_id,
                fingerprint,
            }) if request_id == &self.request_id => {
                if operation != &self.operation || room_id != &self.room_id {
                    return RoomActionWaitStep::Failed(AppError::InvalidBackendData {
                        field: "room.action.accepted".to_owned(),
                        reason: "correlated room action contradicted operation or room".to_owned(),
                    });
                }
                if self
                    .accepted_fingerprint
                    .as_deref()
                    .is_some_and(|previous| previous != fingerprint)
                {
                    return RoomActionWaitStep::Failed(AppError::InvalidBackendData {
                        field: "room.action.accepted.fingerprint".to_owned(),
                        reason: "correlated room action changed fingerprint".to_owned(),
                    });
                }
                self.accepted_fingerprint = Some(fingerprint.clone());
                self.deadline = time::Instant::now() + ROOM_ACTION_SETTLEMENT_TIMEOUT;
            }
            HostEvent::Room(RoomEvent::ActionResult {
                request_id,
                operation,
                room_id,
                fingerprint,
                status,
                message,
                ..
            }) if request_id == &self.request_id => {
                if operation != &self.operation || room_id.as_deref() != Some(&self.room_id) {
                    return RoomActionWaitStep::Failed(AppError::InvalidBackendData {
                        field: "room.action.result".to_owned(),
                        reason: "correlated room action contradicted operation or room".to_owned(),
                    });
                }
                if self.accepted_fingerprint.as_deref() != fingerprint.as_deref() {
                    return RoomActionWaitStep::Failed(AppError::InvalidBackendData {
                        field: "room.action.result.fingerprint".to_owned(),
                        reason: "correlated room result did not reproduce accepted fingerprint"
                            .to_owned(),
                    });
                }
                return match status {
                    RoomActionStatus::Succeeded => RoomActionWaitStep::Settled,
                    RoomActionStatus::Unknown => {
                        RoomActionWaitStep::Failed(AppError::Unsupported {
                            reason: message.clone().unwrap_or_else(|| {
                                format!(
                                    "{} has an unknown durable outcome; do not retry blindly",
                                    self.operation
                                )
                            }),
                        })
                    }
                    RoomActionStatus::Failed | RoomActionStatus::Conflict => {
                        RoomActionWaitStep::Failed(AppError::Unsupported {
                            reason: message
                                .clone()
                                .unwrap_or_else(|| format!("{} failed", self.operation)),
                        })
                    }
                };
            }
            _ => {}
        }
        RoomActionWaitStep::Pending
    }
}

#[derive(Debug)]
pub(in crate::headless_host) enum ScopeWaitStep {
    Pending,

    Settled,

    Failed(AppError),
}

#[derive(Debug)]
pub(in crate::headless_host) struct ScopeWait {
    request_id: String,
    session_id: String,
    accepted: bool,
    budget: Duration,
    deadline: time::Instant,
}

impl ScopeWait {
    pub(in crate::headless_host) fn new(request_id: String, session_id: String) -> Self {
        Self {
            request_id,
            session_id,
            accepted: false,
            budget: Duration::ZERO,
            deadline: time::Instant::now() + HOST_SILENCE_TIMEOUT,
        }
    }

    pub(in crate::headless_host) const fn deadline(&self) -> time::Instant {
        self.deadline
    }

    pub(in crate::headless_host) fn gave_up(&self) -> AppError {
        let reason = if self.accepted {
            format!(
                "headless host did not settle the share scope change within its own {}s budget ({})",
                self.budget.as_secs(),
                self.session_id
            )
        } else {
            format!(
                "headless host never accepted share scope change ({})",
                self.session_id
            )
        };
        AppError::Unsupported { reason }
    }

    pub(in crate::headless_host) fn observe(&mut self, event: &HostEvent) -> ScopeWaitStep {
        if let Some(error) = self.failure(event) {
            return ScopeWaitStep::Failed(error);
        }
        match event {
            HostEvent::Session(SessionEvent::ScopeAccepted {
                request_id,
                budget_ms,
                ..
            }) if *request_id == self.request_id => {
                self.accepted = true;
                self.budget = Duration::from_millis(*budget_ms);
                self.deadline = time::Instant::now() + self.budget;
            }
            HostEvent::Session(SessionEvent::ScopeChanged { request_id, .. })
                if *request_id == self.request_id =>
            {
                return ScopeWaitStep::Settled;
            }
            _ => {}
        }
        ScopeWaitStep::Pending
    }

    fn failure(&self, event: &HostEvent) -> Option<AppError> {
        let HostEvent::Session(SessionEvent::Error {
            operation,
            session_id,
            request_id,
            message,
        }) = event
        else {
            return None;
        };
        let correlated = request_id.as_deref() == Some(self.request_id.as_str())
            || (request_id.is_none()
                && operation == "session.scope"
                && session_id.as_deref() == Some(self.session_id.as_str()));
        correlated.then(|| AppError::Unsupported {
            reason: format!("{operation}:{}: {message}", self.session_id),
        })
    }
}

pub(in crate::headless_host) fn session_matches_identifier(
    session: &SessionListEntry,
    identifier: &str,
) -> bool {
    session.id() == identifier || session.create_request_id() == Some(identifier)
}

#[cfg(test)]
mod room_action_wait_tests {
    use super::{RoomActionWait, RoomActionWaitStep};
    use crate::{HostEvent, RoomActionStatus, RoomEvent, SystemEvent};

    fn accepted(request_id: &str, operation: &str, room_id: &str, fingerprint: &str) -> HostEvent {
        HostEvent::Room(RoomEvent::ActionAccepted {
            request_id: request_id.to_owned(),
            operation: operation.to_owned(),
            room_id: room_id.to_owned(),
            fingerprint: fingerprint.to_owned(),
        })
    }

    fn result(
        request_id: &str,
        operation: &str,
        room_id: Option<&str>,
        fingerprint: Option<&str>,
        status: RoomActionStatus,
    ) -> HostEvent {
        HostEvent::Room(RoomEvent::ActionResult {
            request_id: request_id.to_owned(),
            operation: operation.to_owned(),
            room_id: room_id.map(ToOwned::to_owned),
            fingerprint: fingerprint.map(ToOwned::to_owned),
            status,
            entity_id: Some("task-1".to_owned()),
            message: None,
        })
    }

    #[tokio::test(start_paused = true)]
    async fn heartbeats_cannot_extend_room_action_admission() {
        let mut wait = RoomActionWait::new(
            "request-1".to_owned(),
            "tasks.assign".to_owned(),
            "room-1".to_owned(),
        );
        let admission_deadline = wait.deadline();

        for _ in 0..24 {
            tokio::time::advance(std::time::Duration::from_secs(5)).await;
            assert!(matches!(
                wait.observe(&HostEvent::System(SystemEvent::Heartbeat)),
                RoomActionWaitStep::Pending
            ));
            assert_eq!(wait.deadline(), admission_deadline);
        }

        assert!(tokio::time::Instant::now() >= wait.deadline());
    }

    #[test]
    fn exact_acceptance_and_result_settle_despite_unrelated_broadcasts() {
        let mut wait = RoomActionWait::new(
            "request-1".to_owned(),
            "tasks.assign".to_owned(),
            "room-1".to_owned(),
        );
        assert!(matches!(
            wait.observe(&accepted("request-2", "tasks.assign", "room-1", "other")),
            RoomActionWaitStep::Pending
        ));
        assert!(matches!(
            wait.observe(&accepted("request-1", "tasks.assign", "room-1", "hash")),
            RoomActionWaitStep::Pending
        ));
        assert!(matches!(
            wait.observe(&result(
                "request-2",
                "tasks.assign",
                Some("room-1"),
                Some("other"),
                RoomActionStatus::Succeeded,
            )),
            RoomActionWaitStep::Pending
        ));
        assert!(matches!(
            wait.observe(&result(
                "request-1",
                "tasks.assign",
                Some("room-1"),
                Some("hash"),
                RoomActionStatus::Succeeded,
            )),
            RoomActionWaitStep::Settled
        ));
    }

    #[test]
    fn correlated_identity_or_fingerprint_contradictions_fail_closed() {
        for event in [
            accepted("request-1", "tasks.transition", "room-1", "hash"),
            accepted("request-1", "tasks.assign", "room-2", "hash"),
        ] {
            let mut wait = RoomActionWait::new(
                "request-1".to_owned(),
                "tasks.assign".to_owned(),
                "room-1".to_owned(),
            );
            assert!(matches!(
                wait.observe(&event),
                RoomActionWaitStep::Failed(crate::AppError::InvalidBackendData { .. })
            ));
        }

        let mut wait = RoomActionWait::new(
            "request-1".to_owned(),
            "tasks.assign".to_owned(),
            "room-1".to_owned(),
        );
        assert!(matches!(
            wait.observe(&accepted("request-1", "tasks.assign", "room-1", "hash")),
            RoomActionWaitStep::Pending
        ));
        for event in [
            result(
                "request-1",
                "tasks.assign",
                Some("room-1"),
                None,
                RoomActionStatus::Succeeded,
            ),
            result(
                "request-1",
                "tasks.assign",
                Some("room-1"),
                Some("different"),
                RoomActionStatus::Succeeded,
            ),
        ] {
            assert!(matches!(
                wait.observe(&event),
                RoomActionWaitStep::Failed(crate::AppError::InvalidBackendData { .. })
            ));
        }
    }

    #[test]
    fn unknown_result_warns_against_blind_retry() {
        let mut wait = RoomActionWait::new(
            "request-1".to_owned(),
            "tasks.transition".to_owned(),
            "room-1".to_owned(),
        );
        let _ = wait.observe(&accepted("request-1", "tasks.transition", "room-1", "hash"));
        let RoomActionWaitStep::Failed(crate::AppError::Unsupported { reason }) =
            wait.observe(&result(
                "request-1",
                "tasks.transition",
                Some("room-1"),
                Some("hash"),
                RoomActionStatus::Unknown,
            ))
        else {
            panic!("unknown result must fail")
        };
        assert!(reason.contains("do not retry blindly"));
    }
}

#[cfg(test)]
mod trust_reset_tests {
    use super::HeadlessHostClient;
    use crate::{HostEvent, TrustEvent};

    #[test]
    fn concurrent_reset_events_match_exact_request_and_user() {
        let unrelated_error = HostEvent::Trust(TrustEvent::Error {
            request_id: Some("request-2".to_owned()),
            user_id: Some("user-1".to_owned()),
            operation: "reset".to_owned(),
            message: "wrong request".to_owned(),
        });
        assert!(
            HeadlessHostClient::trust_reset_event_outcome(unrelated_error, "request-1", "user-1")
                .is_none()
        );

        let unrelated_user = HostEvent::Trust(TrustEvent::Reset {
            request_id: "request-1".to_owned(),
            user_id: "user-2".to_owned(),
            cleared: true,
        });
        assert!(
            HeadlessHostClient::trust_reset_event_outcome(unrelated_user, "request-1", "user-1")
                .is_none()
        );

        let matching = HostEvent::Trust(TrustEvent::Reset {
            request_id: "request-1".to_owned(),
            user_id: "user-1".to_owned(),
            cleared: false,
        });
        assert!(
            !HeadlessHostClient::trust_reset_event_outcome(matching, "request-1", "user-1")
                .expect("matching completion")
                .expect("successful reset")
        );
    }
}
