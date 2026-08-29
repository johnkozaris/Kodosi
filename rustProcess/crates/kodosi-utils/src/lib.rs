#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        reason = "tests assert preconditions via panic-on-Err for failure clarity"
    )
)]

pub mod errors;

pub use errors::{KodosiError, Result};
