use futures_util::StreamExt as _;
use std::{
    fs::File,
    io,
    path::{Path, PathBuf},
    time::Duration,
};

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    sync::{Semaphore, broadcast, watch},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    CommandEnvelope, Error, Event, EventBody, HostKind, Result, RuntimeHandle,
    protocol::SessionKind, terminal,
};

const VERSION: u32 = 25;
const STOP_WAIT: Duration = Duration::from_secs(5);
const MAX_FRAME: usize = 8 * 1024 * 1024 + 64 * 1024;
const MAX_CONNECTIONS: usize = 64;
const HELLO_TIMEOUT: Duration = Duration::from_secs(3);
const ACCOUNT_TIMEOUT: Duration = Duration::from_secs(30);
const REFRESH_RETRY: Duration = Duration::from_secs(1);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Hello {
    version: u32,
    root: String,
    session_id: Option<Uuid>,
}
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Welcome {
    version: u32,
    error: Option<String>,
    #[serde(default)]
    events: Vec<Value>,
    #[serde(default)]
    host: Option<HostDescription>,
    #[serde(default)]
    terminals: usize,
    #[serde(default)]
    terminal: Option<TerminalWelcome>,
    #[serde(default)]
    refusal: Option<Refusal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Refusal {
    Busy,
    Stopped,
    NotFound,
    Stale,
}
impl Refusal {
    const fn of(error: &Error) -> Option<Self> {
        match error {
            Error::Busy => Some(Self::Busy),
            Error::Stopped => Some(Self::Stopped),
            Error::NotFound => Some(Self::NotFound),
            Error::Stale => Some(Self::Stale),
            _ => None,
        }
    }
    const fn error(self) -> Error {
        match self {
            Self::Busy => Error::Busy,
            Self::Stopped => Error::Stopped,
            Self::NotFound => Error::NotFound,
            Self::Stale => Error::Stale,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalWelcome {
    pub incarnation_id: Uuid,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum ClientRequest {
    Command { command: Value },
    Stop { force: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostDescription {
    pub pid: u32,
    pub kind: HostKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostStatus {
    pub host: HostDescription,
    pub local_sessions: usize,
}

pub(crate) fn local_sessions(events: &[Event]) -> usize {
    events
        .iter()
        .rev()
        .find_map(|event| match &event.event {
            EventBody::SessionsSnapshot { sessions } => Some(
                sessions
                    .iter()
                    .filter(|session| session.kind == SessionKind::Local)
                    .count(),
            ),
            _ => None,
        })
        .unwrap_or(0)
}

fn stop_permission(
    kind: HostKind,
    force: bool,
    local_sessions: usize,
) -> std::result::Result<(), String> {
    if kind == HostKind::Foreground {
        return Err("This host was started with `kodosi host`. Stop it from its terminal.".into());
    }
    if force {
        return Ok(());
    }
    if local_sessions > 0 {
        return Err(format!(
            "This host still runs {local_sessions} terminal(s). Close them first."
        ));
    }
    Ok(())
}

mod app;
mod client;
mod framing;
mod server;

pub use app::{
    AppClaim, Forwarded, MAX_ACTIVATION_BYTES, RunningApp, claim_app, forward_activation,
    serve_activations,
};
pub use client::{
    Client, Commands, Events, TerminalClient, connect_or_start, host_status, stop_other_host,
};
use framing::{
    CONTROL, DATA, INPUT, INPUT_ACK, REFRESH, RESIZE, TERMINAL_FRAME_MAX, checkpoint_frame,
    control_frame, read_frame, read_json, resize_frame, write_frame, write_json,
};
pub use framing::{
    FrameReader, TerminalFrame, read_terminal, write_control, write_input, write_refresh,
    write_resize,
};
pub use server::{LocalServer, serve};
use server::{endpoint, local_error, lock_root, role_endpoint, root_identity};

#[cfg(test)]
mod tests;
