#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"),
    all(target_os = "ios", target_arch = "aarch64"),
    all(
        target_os = "windows",
        target_env = "msvc",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )
)))]
compile_error!(
    "Kodosi supports arm64 macOS, x86_64 Linux GNU, x86_64 or arm64 Windows MSVC and arm64 iOS"
);

#[cfg(all(target_os = "ios", feature = "host"))]
compile_error!("iOS views terminals hosted elsewhere; build it with --no-default-features");

#[macro_use]
pub mod text;
pub mod agents;
pub mod canvas;
#[cfg(feature = "cli")]
pub mod cli;
mod config;
pub mod diagnostics;
#[cfg(feature = "host")]
mod documents;
pub mod dosi;
#[cfg(feature = "host")]
mod folders;
pub mod identity;
#[cfg(feature = "host")]
pub mod local_host;
pub mod network;
pub mod protocol;
pub mod provider;
pub mod rooms;
mod runtime;
pub mod terminal;
mod worktree;

pub use config::{Config, HostKind};
pub use protocol::{Command, CommandEnvelope, Event, EventBody};
pub use runtime::{RuntimeHandle, start};

pub const PRODUCT_VERSION: &str = env!("KODOSI_VERSION");

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Busy,
    Stopped,
    NotFound,
    Stale,
    Invalid(String),
    Refused(text::Text),
    Text(text::Text),
    HostBusy(text::Text),
    HostOutdated {
        kind: HostKind,
        pid: u32,
        terminals: usize,
    },
    Io(std::io::Error),
    Other(String),
}

impl Error {
    pub fn text(&self) -> text::Text {
        match self {
            Self::Busy => text!("Kodosi is busy. Try again."),
            Self::Stopped => text!("Kodosi has stopped."),
            Self::NotFound => text!("This terminal is not available."),
            Self::Stale => text!("The terminal or the account changed. Try again."),
            Self::Invalid(words) | Self::Other(words) => text::Text::plain(words.clone()),
            Self::Refused(text) | Self::Text(text) | Self::HostBusy(text) => text.clone(),
            Self::HostOutdated { .. } => {
                text!("Kodosi was updated, but its host still runs the earlier version.")
            }
            Self::Io(error) => text::Text::plain(error.to_string()),
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
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<text::Text> for Error {
    fn from(text: text::Text) -> Self {
        Self::Text(text)
    }
}

#[cfg(feature = "host")]
impl From<kodosi_pty::KodosiError> for Error {
    fn from(error: kodosi_pty::KodosiError) -> Self {
        match error {
            kodosi_pty::KodosiError::Io(error) => Self::Io(error),
            error @ kodosi_pty::KodosiError::Spawn(_) => Self::Other(error.to_string()),
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
            refusal @ (network::Error::Trust(_)
            | network::Error::SignedOut
            | network::Error::EnrollmentRequired
            | network::Error::Backend {
                status: 403 | 404, ..
            }) => Self::Refused(refusal.text()),
            other => Self::Text(other.text()),
        }
    }
}
