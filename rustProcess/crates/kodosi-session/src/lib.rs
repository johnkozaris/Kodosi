#![deny(unsafe_code)]

pub mod agent_kind;
mod checkpoint_validation;
pub mod process;
pub mod terminal;

pub use agent_kind::AgentKind;
pub use checkpoint_validation::validate_terminal_checkpoint;
pub use kodosi_pty::{
    KodosiPty, ProcessSnapshot, ProcessTarget, RawFdAsyncReader, ShutdownStage, WaitOutcome,
};
pub use process::{ProcessInspection, inspect_process_from_snapshot};
pub use terminal::{
    CheckpointWithSequence, ClientFocus, PresentationWithSequence, ProcessedTerminalOutput,
    SessionTerminalHandle, TerminalDirection, TerminalEffect, TerminalHistoryPolicy, TerminalInput,
};
