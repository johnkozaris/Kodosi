#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64", target_env = "gnu")
)))]
compile_error!("Kodosi supports arm64 macOS and x86_64 Linux GNU");

#[cfg(feature = "cli")]
pub mod cli;
mod config;
pub mod identity;
pub mod local_host;
pub mod network;
pub mod protocol;
pub mod provider;
mod runtime;
pub mod terminal;

pub use config::{Config, HostKind};
pub use protocol::{Command, CommandEnvelope, Event, EventBody};
pub use runtime::{RuntimeHandle, start};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("The runtime is busy. Try again.")]
    Busy,
    #[error("The runtime has stopped.")]
    Stopped,
    #[error("The session is not available.")]
    NotFound,
    #[error("The request belongs to an earlier session or connection.")]
    Stale,
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    HostBusy(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

impl From<kodosi_pty::KodosiError> for Error {
    fn from(error: kodosi_pty::KodosiError) -> Self {
        match error {
            kodosi_pty::KodosiError::Io(error) => Self::Io(error),
            kodosi_pty::KodosiError::Backpressure(_) => Self::Busy,
            other => Self::Other(other.to_string()),
        }
    }
}
impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Invalid(error.to_string())
    }
}
impl From<network::Error> for Error {
    fn from(error: network::Error) -> Self {
        match error {
            network::Error::Busy => Self::Busy,
            network::Error::Stale => Self::Stale,
            network::Error::Trust(message) => Self::Invalid(message),
            error @ (network::Error::SignedOut
            | network::Error::EnrollmentRequired
            | network::Error::Backend {
                status: 401 | 403 | 404,
                ..
            }) => Self::Invalid(error.to_string()),
            other => Self::Other(other.to_string()),
        }
    }
}
