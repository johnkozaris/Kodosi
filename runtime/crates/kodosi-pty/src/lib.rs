#![cfg_attr(
    unix,
    expect(
        unsafe_code,
        unused_qualifications,
        reason = "Kodosi PTY wraps platform file descriptors and process calls behind safe APIs"
    )
)]
#![cfg_attr(
    windows,
    expect(
        unsafe_code,
        reason = "Kodosi PTY wraps ConPTY, pipes and job objects behind safe APIs"
    )
)]
#![cfg_attr(
    unix,
    expect(
        let_underscore_drop,
        reason = "Unix PTY close paths intentionally ignore best-effort cleanup results"
    )
)]
#![cfg_attr(
    unix,
    expect(
        clippy::cast_possible_wrap,
        clippy::match_same_arms,
        clippy::needless_continue,
        clippy::option_if_let_else,
        reason = "Kodosi PTY keeps platform control flow explicit"
    )
)]
#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::unwrap_used,
        reason = "tests assert preconditions via panic-on-None/Err for failure clarity"
    )
)]

#[cfg(target_os = "ios")]
compile_error!("iOS views terminals hosted elsewhere; build it with --no-default-features");

mod pty;

type Result<T> = std::result::Result<T, KodosiError>;

#[derive(Debug, thiserror::Error)]
pub enum KodosiError {
    #[error("I/O failure: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to spawn child process: {0}")]
    Spawn(String),
}

pub use pty::{CONSOLE_HOST, KodosiPty, PtyReader, ShutdownStage, Transfer, WaitOutcome};
