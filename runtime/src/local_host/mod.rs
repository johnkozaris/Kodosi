use futures_util::StreamExt as _;
use std::{
    fs::{File, OpenOptions},
    io,
    os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::UnixStream,
    sync::{Semaphore, broadcast, watch},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    CommandEnvelope, Error, Event, EventBody, HostKind, Result, RuntimeHandle,
    protocol::SessionKind, terminal,
};

const VERSION: u32 = 18;
const STOP_WAIT: Duration = Duration::from_secs(5);
const MAX_FRAME: usize = 8 * 1024 * 1024 + 64 * 1024;
const MAX_CONNECTIONS: usize = 64;
const HELLO_TIMEOUT: Duration = Duration::from_secs(3);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Hello {
    version: u32,
    root: String,
    session_id: Option<Uuid>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Welcome {
    version: u32,
    error: Option<String>,
    events: Vec<Value>,
    incarnation_id: Option<Uuid>,
    host: Option<HostDescription>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum ClientRequest {
    Command { command: Value },
    Snapshot,
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

fn local_sessions_in(events: &[Value]) -> usize {
    events
        .iter()
        .rev()
        .find(|event| event.get("type").and_then(Value::as_str) == Some("sessions.snapshot"))
        .and_then(|event| event.get("sessions").and_then(Value::as_array))
        .map_or(0, |sessions| {
            sessions
                .iter()
                .filter(|session| session.get("kind").and_then(Value::as_str) == Some("local"))
                .count()
        })
}

fn stop_permission(
    kind: HostKind,
    force: bool,
    local_sessions: usize,
) -> std::result::Result<(), String> {
    if kind == HostKind::App {
        return Err("Kodosi is running as the app. Quit the app to stop this host.".into());
    }
    if force {
        return Ok(());
    }
    if kind == HostKind::Foreground {
        return Err("This host was started with `kodosi host`. Stop it from its terminal.".into());
    }
    if local_sessions > 0 {
        return Err(format!(
            "This host still runs {local_sessions} terminal(s). Close them first."
        ));
    }
    Ok(())
}

mod client;
mod framing;
mod server;

pub use client::{Client, TerminalClient, host_status, stop_other_host};
use framing::{
    DATA, INPUT, INPUT_ACK, RESIZE, checkpoint_frame, control_frame, read_frame, read_json,
    write_frame, write_json,
};
pub use framing::{FrameReader, TerminalFrame, read_terminal, write_input, write_resize};
pub use server::{HostServer, serve};
use server::{existing_owned_file, lock_root, root_identity, socket_dir};

#[cfg(test)]
mod tests;
