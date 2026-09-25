mod emulator;
mod remote;
mod session;
mod subscribers;
mod types;
mod validation;

pub use emulator::TerminalHistoryPolicy;
pub(crate) use remote::RemoteTerminal;
pub(crate) use session::{LocalSession, SessionChange};
pub use subscribers::{ControlFrame, DataFrame, Subscription};
pub use types::{
    Checkpoint, MAX_TERMINAL_COLS, MAX_TERMINAL_ROWS, TERMINAL_CHECKPOINT_SCHEMA_VERSION,
    TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES, TerminalPixelGeometry, TerminalScreen, TerminalSize,
};
pub use validation::validate_terminal_checkpoint;

pub use types::TerminalMetadata;
