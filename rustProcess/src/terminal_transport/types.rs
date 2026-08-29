use std::fmt;

use bytes::Bytes;
use kodosi_domain::{ids::SessionId, terminal::TerminalCheckpointV2};
use tokio::sync::oneshot;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TerminalConnectionId(Uuid);

impl TerminalConnectionId {
    #[must_use]
    #[expect(
        clippy::new_without_default,
        reason = "a terminal connection identity must be minted explicitly"
    )]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn parse(value: &str) -> Result<Self, uuid::Error> {
        Uuid::parse_str(value).map(Self)
    }
}

impl fmt::Display for TerminalConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TerminalSurface {
    Desktop,

    Cli,
    HeadlessCapture,
    RelayHost,
}

impl fmt::Display for TerminalSurface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Desktop => f.write_str("desktop"),
            Self::Cli => f.write_str("cli"),
            Self::HeadlessCapture => f.write_str("headless-capture"),
            Self::RelayHost => f.write_str("relay-host"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TerminalCapability {
    Write,
    ReadOnly,
}

impl TerminalCapability {
    #[must_use]
    pub fn can_write(self) -> bool {
        matches!(self, Self::Write)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalDataFrame {
    pub sequence: u64,
    pub bytes: Bytes,
}

impl TerminalDataFrame {
    #[must_use]
    pub fn new(sequence: u64, bytes: Bytes) -> Self {
        Self { sequence, bytes }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalControlFrame {
    SemanticCheckpoint {
        checkpoint: TerminalCheckpointV2,
        next_sequence: u64,
    },
    Resize {
        rows: u16,
        cols: u16,
        at_sequence: u64,
    },
    Closed {
        reason: TerminalCloseReason,

        final_sequence: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalCloseReason {
    SessionEnded,
    Detached,
    RelayDisconnected,
    AuthRevoked,
    IoError(String),
}

impl fmt::Display for TerminalCloseReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SessionEnded => f.write_str("session ended"),
            Self::Detached => f.write_str("detached"),
            Self::RelayDisconnected => f.write_str("relay disconnected"),
            Self::AuthRevoked => f.write_str("auth revoked"),
            Self::IoError(msg) => write!(f, "I/O error: {msg}"),
        }
    }
}

pub struct TerminalHubRequest {
    pub session_id: SessionId,
    pub surface: TerminalSurface,
    pub capability: TerminalCapability,
    pub reply: oneshot::Sender<Option<super::hub::SubscriberHandle>>,
}

pub struct TerminalHubUnsubscribeRequest {
    pub session_id: SessionId,
    pub connection_id: TerminalConnectionId,
    pub reply: oneshot::Sender<()>,
}

pub enum TerminalHubCommand {
    Subscribe(TerminalHubRequest),
    Unsubscribe(TerminalHubUnsubscribeRequest),
}
