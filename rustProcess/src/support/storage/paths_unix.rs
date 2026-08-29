use std::path::PathBuf;

use directories::BaseDirs;

use crate::{AppError, Result};

const UNIX_APP_DIR: &str = "kodosi";

pub(super) fn data_root() -> Result<PathBuf> {
    base_config_dir()
        .ok_or_else(|| AppError::Unsupported {
            reason: "cannot determine config directory".to_owned(),
        })
        .map(|base| base.join(UNIX_APP_DIR))
}

pub(super) fn tokens_dir() -> Result<PathBuf> {
    data_root()
}

pub(super) fn secrets_dir() -> Result<PathBuf> {
    data_root()
}

fn base_config_dir() -> Option<PathBuf> {
    BaseDirs::new().map(|dirs| dirs.config_dir().to_path_buf())
}
