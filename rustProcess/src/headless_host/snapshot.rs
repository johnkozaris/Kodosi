use kodosi_domain::ids::UserId;
use serde::Serialize;

use crate::{
    AppError, AuthEvent, AuthRequiredReason, HostEvent, Result, RoomListEntry, SessionEvent,
    SessionListEntry, SystemEvent,
};

#[derive(Debug, Clone, Serialize, Default)]
pub(crate) struct HeadlessHostState {
    pub(crate) auth: HeadlessHostAuthState,
    #[serde(skip_serializing)]
    pub(crate) auth_user_id: Option<UserId>,
    pub(crate) sessions: Vec<SessionListEntry>,
    pub(crate) rooms: Vec<RoomListEntry>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(tag = "state", rename_all = "camelCase")]
pub(crate) enum HeadlessHostAuthState {
    Ready,
    RequiresLogin {
        reason: AuthRequiredReason,
    },
    WaitingForApproval {
        user_code: String,
        verification_uri: String,
    },
    #[default]
    Unknown,
}

const SNAPSHOT_SESSION_OPERATIONS: &[&str] = &["session.list"];

fn session_error_is_the_snapshots_own(
    operation: &str,
    session_id: Option<&str>,
    request_id: Option<&str>,
) -> bool {
    SNAPSHOT_SESSION_OPERATIONS.contains(&operation) && session_id.is_none() && request_id.is_none()
}

pub(in crate::headless_host) fn apply_snapshot_message(
    snapshot: &mut HeadlessHostState,
    message: HostEvent,
) -> Result<()> {
    match message {
        HostEvent::Auth(AuthEvent::Ready { user_id, .. }) => {
            snapshot.auth = HeadlessHostAuthState::Ready;
            snapshot.auth_user_id = user_id
                .as_deref()
                .map(UserId::try_from)
                .transpose()
                .map_err(|error| AppError::InvalidBackendData {
                    field: "auth.ready.userId".to_owned(),
                    reason: error.to_string(),
                })?;
        }
        HostEvent::Auth(AuthEvent::Required { reason, .. }) => {
            snapshot.auth = HeadlessHostAuthState::RequiresLogin { reason };
            snapshot.auth_user_id = None;
        }
        HostEvent::Auth(AuthEvent::DeviceCode {
            user_code,
            verification_uri,
        }) => {
            snapshot.auth = HeadlessHostAuthState::WaitingForApproval {
                user_code,
                verification_uri,
            };
            snapshot.auth_user_id = None;
        }
        HostEvent::Session(SessionEvent::List { sessions }) => snapshot.sessions = sessions,
        HostEvent::Session(SessionEvent::Upsert { session }) => {
            let session = *session;
            let session_id = session.id().to_owned();
            if let Some(existing) = snapshot
                .sessions
                .iter_mut()
                .find(|candidate| candidate.id() == session_id)
            {
                *existing = session;
            } else {
                snapshot.sessions.push(session);
            }
        }
        HostEvent::Session(SessionEvent::Removed { session_id }) => {
            snapshot
                .sessions
                .retain(|session| session.id() != session_id);
        }
        HostEvent::Session(SessionEvent::RoomList { rooms }) => {
            snapshot.rooms = rooms;
        }
        HostEvent::Session(SessionEvent::Error {
            operation,
            session_id,
            request_id,
            message,
        }) => {
            if !session_error_is_the_snapshots_own(
                &operation,
                session_id.as_deref(),
                request_id.as_deref(),
            ) {
                tracing::debug!(
                    operation,
                    ?session_id,
                    ?request_id,
                    message,
                    "ignoring a session error raised for another client's command"
                );
                return Ok(());
            }
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

        HostEvent::System(SystemEvent::Error {
            message, context, ..
        }) => {
            tracing::debug!(
                message,
                ?context,
                "ignoring a runtime error the snapshot did not cause"
            );
        }
        _ => {}
    }

    Ok(())
}
