mod context;
#[cfg(feature = "host")]
mod emulator;
mod remote;
#[cfg(feature = "host")]
mod session;
mod status;
mod subscribers;
mod title;
mod types;
mod validation;

use std::time::Duration;

use uuid::Uuid;

pub(crate) const TYPING: Duration = Duration::from_secs(3);
pub(crate) const SOMEONE_TYPING: &str =
    "Someone is typing in this terminal. Try again in a moment.";

pub use context::{ContextPlace, PlaceKind, TerminalContext};
pub(crate) use remote::RemoteTerminal;
#[cfg(feature = "host")]
pub(crate) use session::TerminalSession;
pub use status::{BlockedKind, ProgramState, ProgramStatus};
pub use subscribers::{ControlFrame, DataFrame, Frame, Subscription};
pub(crate) use title::title_spins;
pub use types::{
    CellPixels, Checkpoint, TERMINAL_CHECKPOINT_SCHEMA_VERSION, TERMINAL_IMAGE_STORAGE_MAX_BYTES,
    TERMINAL_INPUT_MAX_BYTES, TERMINAL_REMOTE_IMAGE_DATA_MAX_BYTES,
    TERMINAL_REMOTE_IMAGE_DATA_MIN_BYTES, TERMINAL_SNAPSHOT_MAX_BYTES,
    TERMINAL_SNAPSHOT_TEXT_MAX_BYTES, TerminalMetadata, TerminalPixelGeometry, TerminalSize,
};

#[cfg(test)]
pub(crate) use types::snapshot_limits;

pub(crate) enum SessionChange {
    RemoteEnded {
        id: Uuid,
        instance_id: Uuid,
    },
    #[cfg(feature = "host")]
    Presence {
        id: Uuid,
        users: Vec<String>,
    },
    #[cfg(feature = "host")]
    Cwd {
        id: Uuid,
        path: std::path::PathBuf,
        repository: bool,
    },
    #[cfg(feature = "host")]
    Program {
        id: Uuid,
        program: Option<String>,
    },
    #[cfg(feature = "host")]
    Title {
        id: Uuid,
        title: String,
    },
    #[cfg(feature = "host")]
    Bell {
        id: Uuid,
    },
    #[cfg(feature = "host")]
    Status {
        id: Uuid,
        status: Option<ProgramStatus>,
    },
    #[cfg(feature = "host")]
    Context {
        id: Uuid,
        context: Option<TerminalContext>,
    },
    #[cfg(feature = "host")]
    Notification {
        id: Uuid,
        title: String,
        body: String,
    },
    #[cfg(feature = "host")]
    Prompt {
        id: Uuid,
        prompt: bool,
    },
    #[cfg(feature = "host")]
    Ended {
        id: Uuid,
        reason: String,
    },
    Metadata {
        id: Uuid,
        incarnation: Uuid,
        instance_id: Uuid,
        metadata: TerminalMetadata,
    },
    RemoteInterrupted {
        id: Uuid,
        incarnation: Uuid,
        instance_id: Uuid,
        reason: String,
    },
    RemoteClosed {
        id: Uuid,
        incarnation: Uuid,
        instance_id: Uuid,
        reason: String,
    },
}
