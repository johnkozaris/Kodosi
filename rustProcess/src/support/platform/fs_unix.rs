use std::{fs, os::unix::fs::PermissionsExt, path::Path};

use crate::{AppError, Result};

pub(super) fn set_file_permissions(path: &Path) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(AppError::Io)
}

pub(super) fn set_dir_permissions(path: &Path) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(AppError::Io)
}
