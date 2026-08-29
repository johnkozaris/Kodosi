use std::path::Path;

use tempfile::NamedTempFile;

use crate::{AppError, Result};

pub(super) fn persist_replace(tmp: NamedTempFile, path: &Path) -> Result<()> {
    tmp.persist(path)
        .map_err(|error| AppError::Io(error.error))?;
    Ok(())
}

#[cfg(any(test, feature = "cli"))]
pub(super) fn persist_no_clobber(tmp: NamedTempFile, path: &Path) -> Result<bool> {
    match tmp.persist_noclobber(path) {
        Ok(_) => Ok(true),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(AppError::Io(error.error)),
    }
}
