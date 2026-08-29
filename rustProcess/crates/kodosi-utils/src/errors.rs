use std::io;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, KodosiError>;

#[derive(Debug, Error)]
pub enum KodosiError {
    #[error("I/O failure")]
    Io(#[from] io::Error),
    #[error("failed to spawn child process: {0}")]
    Spawn(String),
    #[error("unsupported Kodosi operation: {0}")]
    Unsupported(String),
    #[error("Kodosi channel backpressure: {0}")]
    Backpressure(String),
}
