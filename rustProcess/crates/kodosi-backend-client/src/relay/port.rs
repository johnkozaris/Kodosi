use std::{error::Error, fmt, future::Future, pin::Pin};

use kodosi_domain::terminal::{
    TerminalCheckpointV2, TerminalPixelGeometry, TerminalPresentationV2, TerminalSize,
};

pub type HostRelayFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, HostRelayPortError>> + Send + 'a>>;

#[derive(Debug)]
pub struct HostRelayPortError {
    source: Box<dyn Error + Send + Sync + 'static>,
}

impl HostRelayPortError {
    pub fn from_error(error: impl Error + Send + Sync + 'static) -> Self {
        Self {
            source: Box::new(error),
        }
    }

    pub fn into_source(self) -> Box<dyn Error + Send + Sync + 'static> {
        self.source
    }
}

impl fmt::Display for HostRelayPortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "host relay port failed: {}", self.source)
    }
}

impl Error for HostRelayPortError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientFocus {
    Focused,
    Blurred,
}

impl ClientFocus {
    pub(crate) const fn from_bool(focused: bool) -> Self {
        if focused {
            Self::Focused
        } else {
            Self::Blurred
        }
    }
}

pub trait HostRelayPort: fmt::Debug + Send + Sync {
    fn dispatch_input_payload<'a>(
        &'a self,
        payload: &'a [u8],
        owner_origin: bool,
    ) -> HostRelayFuture<'a, ()>;

    fn stop(&self) -> HostRelayFuture<'_, ()>;

    fn interrupt(&self) -> HostRelayFuture<'_, ()>;

    fn resize(
        &self,
        size: TerminalSize,
        pixel_geometry: Option<TerminalPixelGeometry>,
        owner_origin: bool,
        claim: bool,
    ) -> HostRelayFuture<'_, ()>;

    fn set_focus(&self, client_id: String, focus: ClientFocus) -> HostRelayFuture<'_, ()>;

    fn capture_terminal_checkpoint(&self) -> HostRelayFuture<'_, HostRelayTerminalCheckpoint>;

    fn capture_terminal_presentation(&self) -> HostRelayFuture<'_, HostRelayTerminalPresentation>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRelayTerminalCheckpoint {
    pub checkpoint: TerminalCheckpointV2,

    pub next_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRelayTerminalPresentation {
    pub presentation: TerminalPresentationV2,
}
