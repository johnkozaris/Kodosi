#[cfg(unix)]
mod unix;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownStage {
    Interrupt,
    Hangup,
    Terminate,
    Force,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitOutcome {
    Reaped(Option<i32>),
    TimedOut,
}

#[cfg(unix)]
pub use unix::{KodosiPty, RawFdAsyncReader};
