use std::{collections::BTreeSet, path::PathBuf};

use bytes::Bytes;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;

mod client;
pub mod crypto;
mod http;
mod relay;
mod wire;

#[cfg(test)]
mod tests;

pub use client::Network;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{reason}")]
    Invalid { reason: String },
    #[error("{0}")]
    Trust(String),
    #[error("Sign in to continue.")]
    SignedOut,
    #[error("Approve this device from an existing device to continue.")]
    EnrollmentRequired,
    #[error("This request belongs to an earlier account connection.")]
    Stale,
    #[error("The connection is busy; try again.")]
    Busy,
    #[error("The connection has closed.")]
    Closed,
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Http(reqwest::Error),
    #[error("The server address could not be resolved. Check your connection and try again.")]
    Dns,
    #[error("The server did not respond in time. The operation was not confirmed.")]
    Timeout,
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("Backend request failed ({status}): {message}")]
    Backend { status: u16, message: String },
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        if error.is_dns() {
            Self::Dns
        } else if error.is_timeout() {
            Self::Timeout
        } else {
            Self::Http(error)
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn invalid(reason: impl Into<String>) -> Error {
    Error::Invalid {
        reason: reason.into(),
    }
}

#[derive(Clone)]
pub struct NetworkConfig {
    pub api_url: Url,
    pub issuer: String,
    pub client_id: String,
    pub scopes: Vec<String>,
    pub audience: Option<String>,
    pub data_root: PathBuf,
    pub secret_service: String,
    pub isolated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub user_id: String,
    pub device_id: String,
    pub display_name: String,
    pub enrolled: bool,
}

#[derive(Debug, Clone)]
pub struct NetworkEvent {
    pub generation: u64,
    pub user_id: Option<String>,
    pub event: Value,
}

#[derive(Debug)]
pub struct NetworkReply {
    pub generation: u64,
    pub user_id: Option<String>,
    pub events: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPublication {
    pub session_id: Uuid,
    pub incarnation_id: Uuid,
    pub name: String,
    pub room_id: Option<Uuid>,
    pub shared_with: BTreeSet<String>,
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
    pub room_id: Option<Uuid>,
    pub room_name: Option<String>,
    pub shared_with: Vec<String>,
    pub online: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TerminalControl {
    Input {
        bytes: Vec<u8>,
    },
    Resize {
        #[serde(rename = "requestId")]
        request_id: String,
        rows: u16,
        cols: u16,
        #[serde(rename = "widthPixels")]
        width_pixels: u32,
        #[serde(rename = "heightPixels")]
        height_pixels: u32,
        #[serde(rename = "cellWidthPixels")]
        cell_width_pixels: u32,
        #[serde(rename = "cellHeightPixels")]
        cell_height_pixels: u32,
        claim: bool,
    },
    Focus {
        focused: bool,
    },
    Interrupt,
    Close,
}

pub struct CheckpointCut {
    pub checkpoint: crate::terminal::Checkpoint,
    pub next_sequence: u64,
}

pub enum HostRequest {
    ResetPresence,
    Connected {
        connection_id: Uuid,
        user_id: String,
    },
    Bootstrap {
        request_id: Uuid,
        reply: oneshot::Sender<std::result::Result<CheckpointCut, String>>,
    },
    Control {
        sender_user_id: String,
        sender_device_id: String,
        connection_id: Uuid,
        control: TerminalControl,
        authorization: tokio_util::sync::CancellationToken,
        reply: oneshot::Sender<std::result::Result<Value, String>>,
    },
    Disconnected {
        connection_id: Uuid,
    },
}

#[derive(Clone)]
pub enum PublishedFrame {
    MetadataChanged,
    BootstrapBarrier {
        request_id: Uuid,
    },
    Raw {
        sequence: u64,
        bytes: Bytes,
    },
    Checkpoint {
        checkpoint: crate::terminal::Checkpoint,
        next_sequence: u64,
    },
    Closed {
        reason: String,
        final_sequence: u64,
    },
}

pub enum RemoteUpdate {
    Ended {
        final_sequence: u64,
    },
    Checkpoint {
        checkpoint: crate::terminal::Checkpoint,
        next_sequence: u64,
        fresh: bool,
    },
    Raw {
        sequence: u64,
        bytes: Bytes,
    },
    Closed {
        reason: String,
    },
}

pub struct RemoteConnection {
    pub session: RemoteSession,
    pub updates: mpsc::Receiver<RemoteUpdate>,
    commands: mpsc::Sender<relay::RemoteRequest>,
    cancellation: tokio_util::sync::CancellationToken,
}

#[derive(Clone)]
pub struct RemoteControl {
    commands: mpsc::Sender<relay::RemoteRequest>,
    cancellation: tokio_util::sync::CancellationToken,
}

impl RemoteConnection {
    pub fn control_handle(&self) -> RemoteControl {
        RemoteControl {
            commands: self.commands.clone(),
            cancellation: self.cancellation.clone(),
        }
    }
    pub async fn send_control(&self, control: TerminalControl) -> Result<Value> {
        self.control_handle().send_control(control).await
    }
    pub async fn request_checkpoint(&self) -> Result<()> {
        self.control_handle().request_checkpoint().await
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
            .try_send(relay::RemoteRequest::Control { control, reply })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => Error::Busy,
                mpsc::error::TrySendError::Closed(_) => Error::Closed,
            })?;
        tokio::select! {
            biased;
            result=tokio::time::timeout(std::time::Duration::from_secs(10), response)=>
                result.map_err(|_|invalid("The host did not confirm the operation; it was not retried."))?
                    .map_err(|_|invalid("The connection closed before confirming the operation; it was not retried."))?,
            ()=self.cancellation.cancelled()=>Err(invalid("The connection closed before confirming the operation; it was not retried.")),
        }
    }

    #[expect(
        clippy::unused_async,
        reason = "shares the remote command handle's async API without waiting on transport capacity"
    )]
    pub async fn request_checkpoint(&self) -> Result<()> {
        if self.cancellation.is_cancelled() {
            return Err(Error::Closed);
        }
        self.commands
            .try_send(relay::RemoteRequest::Checkpoint)
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => Error::Busy,
                mpsc::error::TrySendError::Closed(_) => Error::Closed,
            })
    }

    pub fn disconnect(&self) {
        self.cancellation.cancel();
    }
}

impl Drop for RemoteConnection {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

pub type PublicationOutput = broadcast::Receiver<PublishedFrame>;

#[cfg(test)]
pub(crate) use relay::RemoteRequest as TestRemoteRequest;

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
