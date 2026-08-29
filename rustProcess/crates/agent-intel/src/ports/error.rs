use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentIntelError {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, AgentIntelError>;
