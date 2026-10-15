#[cfg(feature = "host")]
mod emulator;
mod remote;
#[cfg(feature = "host")]
mod session;
mod status;
mod subscribers;
mod types;
mod validation;

use uuid::Uuid;

pub(crate) use remote::RemoteTerminal;
#[cfg(feature = "host")]
pub(crate) use session::LocalSession;
pub use status::{BlockedKind, ProgramState, ProgramStatus};
pub use subscribers::{ControlFrame, DataFrame, Subscription};
pub use types::{
    Checkpoint, MAX_TERMINAL_COLS, MAX_TERMINAL_ROWS, TERMINAL_CHECKPOINT_SCHEMA_VERSION,
    TERMINAL_INPUT_MAX_BYTES, TERMINAL_SNAPSHOT_MAX_BYTES, TerminalPixelGeometry, TerminalSize,
};

pub use types::TerminalMetadata;

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
    Launched {
        program: String,
        command: String,
        folder: Option<String>,
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
