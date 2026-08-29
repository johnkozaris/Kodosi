use std::{fs, path::Path};

use crate::{AppError, Result};

#[path = "fs_unix.rs"]
mod imp;

pub(crate) fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(AppError::Io)?;
    set_dir_permissions(path)?;
    Ok(())
}

pub(crate) fn set_file_permissions(path: &Path) -> Result<()> {
    imp::set_file_permissions(path)
}

pub(crate) fn set_dir_permissions(path: &Path) -> Result<()> {
    imp::set_dir_permissions(path)
}
