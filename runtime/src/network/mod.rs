use std::{collections::BTreeSet, path::PathBuf, time::Duration};

use bytes::Bytes;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;

use crate::text::Text;

mod backend_client;
pub mod crypto;
mod http;
mod terminal_connections;
mod wire;

#[cfg(test)]
mod tests;

pub use backend_client::BackendClient;
#[cfg(any(target_os = "ios", target_os = "android"))]
pub(crate) use backend_client::device_label;
#[cfg(feature = "host")]
pub(crate) use backend_client::host_label;
pub(crate) use backend_client::{DELETION_LIFE, SIGN_IN_LIFE};
#[cfg(feature = "host")]
pub(crate) use terminal_connections::Answering;

#[derive(Debug)]
pub enum Error {
    Invalid { reason: String },
    Text(Text),
    Trust(Text),
    SignedOut,
    AccountDeleted,
    AccountBanned,
    EnrollmentRequired,
    Stale,
    Busy,
    Closed,
    Io(std::io::Error),
    Http(reqwest::Error),
    Dns,
    Unreachable,
    Timeout,
    Dropped,
    Json(serde_json::Error),
    Backend { status: u16, message: Text },
    UpdateRequired,
    ServerOutdated,
    ChannelVersion { peer_newer: bool },
}

impl Error {
    pub fn text(&self) -> Text {
        match self {
            Self::Invalid { reason } => Text::plain(reason.clone()),
            Self::Text(text) | Self::Trust(text) | Self::Backend { message: text, .. } => {
                text.clone()
            }
            Self::SignedOut => text!("Sign in to continue."),
            Self::AccountDeleted => text!("This Kodosi account was deleted."),
            Self::AccountBanned => text!(
                "This account is banned because it tried too many user names. Ask the operator of this Kodosi server to remove the ban."
            ),
            Self::EnrollmentRequired => {
                text!("This device is not approved for your account. Approve it first.")
            }
            Self::Stale => Text::plain("This request belongs to an earlier account connection."),
            Self::Busy => text!("The connection is busy. Try again."),
            Self::Closed => Text::plain("The connection has closed."),
            Self::Io(error) => Text::plain(error.to_string()),
            Self::Http(error) => Text::plain(error.to_string()),
            Self::Json(error) => Text::plain(error.to_string()),
            Self::Dns => text!(
                "Kodosi could not find the server address. Check your connection and try again."
            ),
            Self::Unreachable => {
                text!("Kodosi cannot reach the server. Check your connection and try again.")
            }
            Self::Timeout => text!(
                "The server did not respond in time, so Kodosi cannot tell if the operation worked."
            ),
            Self::Dropped => text!(
                "The connection to the server dropped, so Kodosi cannot tell if the operation worked."
            ),
            Self::UpdateRequired => {
                text!("This version of Kodosi is too old for the server. Update Kodosi.")
            }
            Self::ServerOutdated => {
                text!("The server uses an older version of Kodosi. Its operator must update it.")
            }
            Self::ChannelVersion { peer_newer: true } => text!(
                "The other computer has a newer version of Kodosi. Update Kodosi to open this terminal."
            ),
            Self::ChannelVersion { peer_newer: false } => {
                text!("The other computer has an older version of Kodosi. Update Kodosi there.")
            }
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.text().fmt(formatter)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Http(error) => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<Text> for Error {
    fn from(text: Text) -> Self {
        Self::Text(text)
    }
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        if error.is_dns() {
            Self::Dns
        } else if error.is_connect() {
            Self::Unreachable
        } else if error.is_timeout() {
            Self::Timeout
        } else if error.is_request() || error.is_body() {
            Self::Dropped
        } else {
            Self::Http(error)
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn user_facing(&self) -> bool {
        !matches!(self, Self::Stale | Self::Closed)
    }

    pub(crate) fn unanswered(&self) -> bool {
        matches!(
            self,
            Self::Io(_)
                | Self::Http(_)
                | Self::Dns
                | Self::Unreachable
                | Self::Timeout
                | Self::Dropped
                | Self::Busy
        ) || matches!(self, Self::Backend { status, .. } if *status == 429 || *status >= 500)
    }
}

pub(crate) fn invalid(reason: impl Into<String>) -> Error {
    Error::Invalid {
        reason: reason.into(),
    }
}

fn unconfirmed() -> Error {
    Error::Text(text!(
        "The connection closed before the terminal's computer confirmed the operation, and Kodosi did not send it again."
    ))
}

pub(crate) fn untrusted(reason: impl Into<std::borrow::Cow<'static, str>>) -> Error {
    Error::Trust(Text::plain(reason))
}

pub(crate) fn stagger(delay: Duration) -> Duration {
    let mut random = [0; 4];
    if aws_lc_rs::rand::fill(&mut random).is_err() {
        return delay;
    }
    delay.mul_f64(0.5 + f64::from(u32::from_le_bytes(random)) / f64::from(u32::MAX) / 2.0)
}

const SHORTEST_SLEEP: Duration = Duration::from_secs(5);

/// The monotonic clock stands still while the computer sleeps, and the wall clock goes on.
#[derive(Clone, Copy)]
pub(crate) struct Moment {
    pub(crate) monotonic: tokio::time::Instant,
    pub(crate) wall: std::time::SystemTime,
}

impl Moment {
    pub(crate) fn now() -> Self {
        Self {
            monotonic: tokio::time::Instant::now(),
            wall: std::time::SystemTime::now(),
        }
    }

    pub(crate) fn slept_until(self, later: Self) -> bool {
        let awake = later.monotonic.saturating_duration_since(self.monotonic);
        let passed = later.wall.duration_since(self.wall).unwrap_or_default();
        passed > awake + SHORTEST_SLEEP
    }
}

#[derive(Clone)]
pub struct BackendConfig {
    pub api_url: Url,
    pub issuer: String,
    pub client_id: String,
    pub scopes: Vec<String>,
    pub data_root: PathBuf,
    pub secret_service: String,
    pub isolated: bool,
}

#[derive(Debug, Clone)]
pub struct Identity {
    pub user_id: String,
    pub device_id: String,
    pub display_name: String,
    pub enrolled: bool,
}

#[derive(Debug, Clone)]
pub struct BackendEvent {
    pub generation: u64,
    pub user_id: Option<String>,
    pub event: Value,
}

#[derive(Debug)]
pub struct BackendReply {
    pub generation: u64,
    pub user_id: Option<String>,
    pub events: Vec<Value>,
    pub answered_later: bool,
}

#[derive(Debug, Clone)]
pub struct LocalPublication {
    pub session_id: Uuid,
    pub incarnation_id: Uuid,
    pub name: String,
    pub mission_id: Option<Uuid>,
    pub room_sequence: u64,
    pub shared_with: BTreeSet<String>,
    pub restored_from: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSession {
    pub id: Uuid,
    pub incarnation_id: Uuid,
    pub name: String,
    pub owner_user_id: String,
    pub owner_name: String,
    pub host_device_id: String,
    pub host_name: String,
    pub mission_id: Option<Uuid>,
    pub mission_name: Option<String>,
    pub shared_with: Vec<String>,
    pub online: bool,
    pub status: Option<TerminalStatus>,
    pub hidden: bool,
    pub restored_from: Option<Uuid>,
}

/// The server reads it, so it never carries the program's text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<crate::terminal::ProgramState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<crate::terminal::BlockedKind>,
}

impl TerminalStatus {
    #[cfg(feature = "host")]
    pub(crate) fn of(metadata: &crate::terminal::TerminalMetadata) -> Self {
        Self {
            program: metadata.program.clone(),
            account: metadata.account.filter(|_| metadata.program.is_some()),
            state: metadata.status.as_ref().map(|status| status.state),
            kind: metadata.status.as_ref().and_then(|status| status.kind),
        }
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.program
            .as_deref()
            .is_none_or(crate::agents::is_agent_name)
            && (self.account.is_none() || self.program.is_some())
            && (self.kind.is_none() || self.state == Some(crate::terminal::ProgramState::Blocked))
    }

    pub(crate) fn program_status(&self) -> Option<crate::terminal::ProgramStatus> {
        Some(crate::terminal::ProgramStatus {
            state: self.state?,
            kind: self.kind,
            progress: None,
            title: None,
            message: None,
        })
    }
}

#[derive(Debug, Clone)]
pub enum TerminalControl {
    Input {
        bytes: Vec<u8>,
    },
    Message {
        bytes: Vec<u8>,
    },
    Resize {
        rows: u16,
        cols: u16,
        width_pixels: u32,
        height_pixels: u32,
        cell_width_pixels: u32,
        cell_height_pixels: u32,
        claim: bool,
    },
    Focus {
        focused: bool,
    },
    Interrupt,
    Close,
    Canvas {
        request: crate::canvas::CanvasRequest,
    },
    Agent {
        change: Box<crate::agents::AgentSettings>,
    },
}

#[derive(Debug)]
pub struct CheckpointCut {
    pub checkpoint: crate::terminal::Checkpoint,
    pub next_sequence: u64,
}

pub enum HostRequest {
    ResetPresence {
        owner: Option<String>,
    },
    Connected {
        connection_id: Uuid,
        user_id: String,
    },
    Bootstrap {
        request_id: Uuid,
        history: bool,
        image_data_bytes: u64,
        reply: oneshot::Sender<std::result::Result<CheckpointCut, Text>>,
    },
    Control {
        connection_id: Uuid,
        control: TerminalControl,
        authorization: tokio_util::sync::CancellationToken,
        reply: oneshot::Sender<std::result::Result<Value, Text>>,
    },
    Disconnected {
        connection_id: Uuid,
    },
    Document {
        connection_id: Uuid,
        key: String,
        name: String,
        reply: oneshot::Sender<std::result::Result<crate::canvas::PipeEnds, Text>>,
    },
    Picture {
        connection_id: Uuid,
        authorization: tokio_util::sync::CancellationToken,
        picture: HostPicture,
        reply: oneshot::Sender<std::result::Result<Value, Text>>,
    },
}

/// The host pastes a known picture from its file, if it still has it, and answers `missing` if
/// not.
pub enum HostPicture {
    Known {
        digest: [u8; 32],
        format: crate::terminal::PictureFormat,
    },
    Whole(crate::terminal::Picture),
}

#[derive(Clone)]
pub enum PublishedFrame {
    Metadata(crate::terminal::TerminalMetadata),
    BootstrapBarrier {
        request_id: Uuid,
    },
    Raw {
        sequence: u64,
        bytes: Bytes,
    },
    Resize {
        rows: u16,
        cols: u16,
        cell: crate::terminal::CellPixels,
        at_sequence: u64,
    },
    Closed {
        reason: Text,
        final_sequence: u64,
    },
}

pub enum RemoteUpdate {
    Ended,
    Checkpoint {
        checkpoint: crate::terminal::Checkpoint,
        next_sequence: u64,
    },
    Raw {
        sequence: u64,
        bytes: Bytes,
    },
    Metadata(crate::terminal::TerminalMetadata),
    Resize {
        rows: u16,
        cols: u16,
        cell: crate::terminal::CellPixels,
        at_sequence: u64,
    },
    Resync,
    Closed {
        reason: Text,
    },
    Picture {
        sent: u64,
        total: u64,
    },
}

pub struct RemoteConnection {
    pub session: RemoteSession,
    pub updates: mpsc::Receiver<RemoteUpdate>,
    commands: mpsc::Sender<terminal_connections::RemoteRequest>,
    cancellation: tokio_util::sync::CancellationToken,
}

#[derive(Clone)]
pub struct RemoteControl {
    commands: mpsc::Sender<terminal_connections::RemoteRequest>,
    cancellation: tokio_util::sync::CancellationToken,
}

impl RemoteConnection {
    pub fn control_handle(&self) -> RemoteControl {
        RemoteControl {
            commands: self.commands.clone(),
            cancellation: self.cancellation.clone(),
        }
    }

    pub fn disconnect(&self) {
        self.cancellation.cancel();
    }
}

impl RemoteControl {
    pub async fn send_control(&self, control: TerminalControl) -> Result<Value> {
        if self.cancellation.is_cancelled() {
            return Err(Error::Closed);
        }
        let (reply, response) = oneshot::channel();
        self.commands
            .try_send(terminal_connections::RemoteRequest::Control { control, reply })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => Error::Busy,
                mpsc::error::TrySendError::Closed(_) => Error::Closed,
            })?;
        tokio::select! {
            biased;
            result=tokio::time::timeout(CONTROL_TIMEOUT, response)=>
                result.map_err(|_|Error::Text(text!("The terminal's computer did not confirm the operation, and Kodosi did not send it again.")))?
                    .map_err(|_|unconfirmed())?,
            ()=self.cancellation.cancelled()=>Err(unconfirmed()),
        }
    }

    pub(crate) fn open_pipe(
        &self,
        pipe: Uuid,
        key: String,
        name: String,
    ) -> Result<crate::canvas::PipeEnds> {
        if self.cancellation.is_cancelled() {
            return Err(Error::Closed);
        }
        Ok(terminal_connections::open_pipe(
            self.commands.clone(),
            self.cancellation.clone(),
            (pipe, key, name),
        ))
    }

    pub fn send_picture(
        &self,
        picture: crate::terminal::Picture,
    ) -> impl Future<Output = Result<()>> + Send + use<> {
        const LOST: &str = "The connection to the other computer was lost, so the picture did not arrive. Paste it again.";
        let (reply, response) = oneshot::channel();
        let admission = if self.cancellation.is_cancelled() {
            Err(invalid(LOST))
        } else {
            self.commands
                .try_send(terminal_connections::RemoteRequest::Picture { picture, reply })
                .map_err(|error| match error {
                    mpsc::error::TrySendError::Full(_) => Error::Busy,
                    mpsc::error::TrySendError::Closed(_) => invalid(LOST),
                })
        };
        let cancellation = self.cancellation.clone();
        async move {
            admission?;
            tokio::select! {
                biased;
                result = response => result.map_err(|_| invalid(LOST))?,
                () = cancellation.cancelled() => Err(invalid(LOST)),
            }
        }
    }

    #[expect(
        clippy::unused_async,
        clippy::unused_async_trait_impl,
        reason = "shares the remote command handle's async API without waiting on transport capacity"
    )]
    pub async fn request_checkpoint(&self) -> Result<()> {
        if self.cancellation.is_cancelled() {
            return Err(Error::Closed);
        }
        self.commands
            .try_send(terminal_connections::RemoteRequest::Checkpoint)
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => Error::Busy,
                mpsc::error::TrySendError::Closed(_) => Error::Closed,
            })
    }
}

impl Drop for RemoteConnection {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

pub type PublicationOutput = broadcast::Receiver<PublishedFrame>;

#[cfg(test)]
pub(crate) use terminal_connections::RemoteRequest as TestRemoteRequest;
#[cfg(test)]
pub(crate) use terminal_connections::Wait;
pub(crate) use terminal_connections::{
    CONTROL_TIMEOUT, RECONNECT_WAIT, STABLE_CONNECTION, input_not_sent,
};

#[cfg(test)]
pub(crate) fn test_remote_connection(
    session: RemoteSession,
) -> (
    RemoteConnection,
    mpsc::Receiver<TestRemoteRequest>,
    mpsc::Sender<RemoteUpdate>,
) {
    let (commands, requests) = mpsc::channel(128);
    let (updates_tx, updates) = mpsc::channel(128);
    let connection = RemoteConnection {
        session,
        updates,
        commands,
        cancellation: tokio_util::sync::CancellationToken::new(),
    };
    (connection, requests, updates_tx)
}
