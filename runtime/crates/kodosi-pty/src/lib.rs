#![expect(
    unsafe_code,
    unused_qualifications,
    reason = "Kodosi PTY wraps platform file descriptors and process calls behind safe APIs"
)]
#![cfg_attr(
    unix,
    expect(
        let_underscore_drop,
        reason = "Unix PTY close paths intentionally ignore best-effort cleanup results"
    )
)]
#![expect(
    clippy::cast_possible_wrap,
    clippy::match_same_arms,
    clippy::needless_continue,
    clippy::option_if_let_else,
    reason = "Kodosi PTY keeps platform control flow explicit"
)]
#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::unwrap_used,
        reason = "tests assert preconditions via panic-on-None/Err for failure clarity"
    )
)]

pub mod pty;

pub type Result<T> = std::result::Result<T, KodosiError>;

#[derive(Debug, thiserror::Error)]
pub enum KodosiError {
    #[error("I/O failure: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to spawn child process: {0}")]
    Spawn(String),
    #[error("unsupported terminal operation: {0}")]
    Unsupported(String),
    #[error("terminal backpressure: {0}")]
    Backpressure(String),
}

pub use pty::{KodosiPty, RawFdAsyncReader, ShutdownStage, WaitOutcome};
