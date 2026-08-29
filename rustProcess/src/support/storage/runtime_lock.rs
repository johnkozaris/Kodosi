use std::{fs, io, path::Path};

use crate::{AppError, Result, support::platform::fs as support_fs};

#[derive(Debug)]
pub(crate) struct RuntimeAuthorityLock {
    _file: fs::File,
}

impl RuntimeAuthorityLock {
    pub(crate) fn acquire() -> Result<Self> {
        Self::try_acquire()?.ok_or_else(|| {
            AppError::Io(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "another Kodosi runtime owns this data root",
            ))
        })
    }

    pub(crate) fn try_acquire() -> Result<Option<Self>> {
        let path = crate::support::storage::paths::runtime_authority_lock_path()?;
        Self::try_at(&path)
    }

    #[cfg(test)]
    pub(crate) fn at(path: &Path) -> Result<Self> {
        Self::try_at(path)?.ok_or_else(|| {
            AppError::Io(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "another Kodosi runtime owns this data root",
            ))
        })
    }

    fn try_at(path: &Path) -> Result<Option<Self>> {
        let parent = path.parent().ok_or_else(|| AppError::Unsupported {
            reason: "runtime authority lock path has no parent".to_owned(),
        })?;
        support_fs::ensure_dir(parent)?;
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(fs::TryLockError::WouldBlock) => return Ok(None),
            Err(fs::TryLockError::Error(error)) => return Err(AppError::Io(error)),
        }
        support_fs::set_file_permissions(path)?;
        Ok(Some(Self { _file: file }))
    }
}

#[cfg(test)]
mod tests {
    use super::RuntimeAuthorityLock;

    #[test]
    fn one_data_root_has_one_runtime_authority() {
        let root = tempfile::tempdir().expect("root");
        let path = root.path().join("runtime-authority.lock");
        let first = RuntimeAuthorityLock::at(&path).expect("first authority");

        let error = RuntimeAuthorityLock::at(&path).expect_err("second authority denied");
        std::assert_matches!(
            error,
            crate::AppError::Io(error) if error.kind() == std::io::ErrorKind::AlreadyExists
        );

        drop(first);
        RuntimeAuthorityLock::at(&path).expect("authority released");
    }
}
