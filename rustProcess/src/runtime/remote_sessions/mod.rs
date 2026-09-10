use std::{collections::BTreeSet, sync::Arc};

use time::OffsetDateTime;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::identity::{DeviceKeyAccess, IdentitySessionKeyTrust};
use crate::{
    AppError, Result,
    discovery::ShelfItem,
    identity_core::device_keys::DeviceKeys,
    remote_sessions::{focus::FocusTransition, relay::events::spawn_session_relay_event_bridge},
    session_runtime::commands::SessionInput,
    sessions_common::session_input_bytes,
};
use kodosi_backend_client::session_relay::{
    self, RemoteRelayMode, SessionRelayClientSpec, SessionRelayCommand, cursor::ReplayCursor,
    events::SessionRelayEvent,
};
use kodosi_domain::{
    ids::SessionId,
    lifecycle::{ConnectionState, RemoteSessionAccessState},
    permissions::ShareScope,
    session::{SessionProvenance, SessionRole, SessionState},
    terminal::TerminalSize,
};

use super::{Runtime, SessionRelayCommandDispatchOutcome};
use crate::discovery::state::RemoteSessionRecord;

mod commands;
mod dispatch;
mod lifecycle;
mod permissions;
mod semantics;

pub(crate) use commands::{
    blur, blur_participant, claim_size, focus, focus_participant, interrupt, owned_remote_record,
    owned_remote_record_mut, reassert_focus, rename, send_input, send_participant_input, stop,
};
pub(crate) use dispatch::ensure_authenticated_remote_account;
pub(crate) use lifecycle::{hide, leave, open, reconcile_relays, unhide};
pub(crate) use permissions::{permission_decision, replay_remote_permission_actions};
pub(crate) use semantics::{
    query_semantics, replay_remote_semantics, semantic_cancel, semantic_send,
};
