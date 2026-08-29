use std::{io::Write, path::Path};

use serde::Serialize;
use tempfile::NamedTempFile;

use crate::{AppError, Result, support::platform::fs as support_fs};

#[path = "atomic_file_unix.rs"]
mod imp;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum FileMode {
    #[default]
    Default,

    UserPrivate,
}

#[derive(Debug)]
pub(crate) enum AtomicWriteFailure {
    NotReplaced(AppError),
    ReplacedDurabilityUncertain(AppError),
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8], mode: FileMode) -> Result<()> {
    let dir = path.parent().ok_or_else(|| AppError::Unsupported {
        reason: "atomic_write path has no parent directory".to_owned(),
    })?;

    let tmp = write_tempfile(dir, bytes, mode)?;
    persist_replace(tmp, path)?;
    sync_parent_dir(dir)?;

    Ok(())
}

pub(crate) fn atomic_write_commit_aware(
    path: &Path,
    bytes: &[u8],
    mode: FileMode,
) -> std::result::Result<(), AtomicWriteFailure> {
    let dir = path.parent().ok_or_else(|| {
        AtomicWriteFailure::NotReplaced(AppError::Unsupported {
            reason: "atomic_write path has no parent directory".to_owned(),
        })
    })?;
    let tmp = write_tempfile(dir, bytes, mode).map_err(AtomicWriteFailure::NotReplaced)?;
    persist_replace(tmp, path).map_err(AtomicWriteFailure::NotReplaced)?;
    sync_parent_dir(dir).map_err(AtomicWriteFailure::ReplacedDurabilityUncertain)
}

pub(crate) fn atomic_write_json_commit_aware<T: Serialize>(
    path: &Path,
    value: &T,
    pretty: bool,
    mode: FileMode,
) -> std::result::Result<(), AtomicWriteFailure> {
    let payload = if pretty {
        serde_json::to_vec_pretty(value)
    } else {
        serde_json::to_vec(value)
    }
    .map_err(AppError::Json)
    .map_err(AtomicWriteFailure::NotReplaced)?;
    atomic_write_commit_aware(path, &payload, mode)
}

pub(crate) fn retry_sync_parent(path: &Path) -> Result<()> {
    let dir = path.parent().ok_or_else(|| AppError::Unsupported {
        reason: "atomic_write path has no parent directory".to_owned(),
    })?;
    sync_parent_dir(dir)
}

#[cfg(any(test, feature = "cli"))]
pub(crate) fn atomic_write_new(path: &Path, bytes: &[u8], mode: FileMode) -> Result<bool> {
    let dir = path.parent().ok_or_else(|| AppError::Unsupported {
        reason: "atomic_write_new path has no parent directory".to_owned(),
    })?;

    let tmp = write_tempfile(dir, bytes, mode)?;
    let created = persist_no_clobber(tmp, path)?;
    if created {
        sync_parent_dir(dir)?;
    }
    Ok(created)
}

pub(crate) fn atomic_write_json<T: Serialize>(
    path: &Path,
    value: &T,
    pretty: bool,
    mode: FileMode,
) -> Result<()> {
    let payload = if pretty {
        serde_json::to_vec_pretty(value)
    } else {
        serde_json::to_vec(value)
    }
    .map_err(AppError::Json)?;

    atomic_write(path, &payload, mode)
}

fn write_tempfile(dir: &Path, bytes: &[u8], mode: FileMode) -> Result<NamedTempFile> {
    let mut tmp = NamedTempFile::new_in(dir).map_err(AppError::Io)?;

    if mode == FileMode::UserPrivate {
        support_fs::set_file_permissions(tmp.path())?;
    }

    tmp.write_all(bytes).map_err(AppError::Io)?;
    tmp.as_file().sync_all().map_err(AppError::Io)?;
    Ok(tmp)
}

fn persist_replace(tmp: NamedTempFile, path: &Path) -> Result<()> {
    imp::persist_replace(tmp, path)
}

#[cfg(any(test, feature = "cli"))]
fn persist_no_clobber(tmp: NamedTempFile, path: &Path) -> Result<bool> {
    imp::persist_no_clobber(tmp, path)
}

fn sync_parent_dir(dir: &Path) -> Result<()> {
    sync_directory(dir).map_err(AppError::Io)
}

pub(crate) fn sync_directory(dir: &Path) -> std::io::Result<()> {
    std::fs::File::open(dir)?.sync_all()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{FileMode, atomic_write, atomic_write_json};

    #[test]
    fn round_trips_bytes_with_default_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hello.txt");

        atomic_write(&path, b"hello world", FileMode::Default).unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"hello world");
    }

    #[test]
    fn round_trips_json_pretty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");

        atomic_write_json(
            &path,
            &serde_json::json!({ "key": "value" }),
            true,
            FileMode::Default,
        )
        .unwrap();

        let read = fs::read_to_string(&path).unwrap();
        assert!(
            read.contains("\n  \"key\""),
            "pretty JSON should indent keys, got {read:?}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn user_private_mode_applies_0600() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret.txt");

        atomic_write(&path, b"secret", FileMode::UserPrivate).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "UserPrivate must apply 0600");
    }

    #[test]
    fn successive_writes_replace_content_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.txt");

        atomic_write(&path, b"first", FileMode::Default).unwrap();
        atomic_write(&path, b"second", FileMode::Default).unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"second");

        let entries: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        assert_eq!(
            entries.len(),
            1,
            "no leftover tempfiles; found {:?}",
            entries
                .iter()
                .map(fs::DirEntry::file_name)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn panic_in_serializer_leaves_original_file_intact() {
        use serde::{Serialize, Serializer};

        struct Exploder;

        impl Serialize for Exploder {
            fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("simulated serializer failure"))
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.txt");

        atomic_write(&path, b"original", FileMode::Default).unwrap();

        let result = atomic_write_json(&path, &Exploder, true, FileMode::Default);
        assert!(result.is_err(), "explosive serializer should fail");

        assert_eq!(fs::read(&path).unwrap(), b"original");
    }

    #[test]
    fn errors_when_parent_directory_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let bad_path = dir.path().join("does/not/exist/file.txt");

        let result = atomic_write(&bad_path, b"x", FileMode::Default);
        assert!(
            result.is_err(),
            "writing into a missing parent must not silently succeed"
        );
    }

    #[test]
    fn atomic_write_new_does_not_overwrite_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.txt");

        assert!(super::atomic_write_new(&path, b"first", FileMode::Default).unwrap());
        assert!(!super::atomic_write_new(&path, b"second", FileMode::Default).unwrap());

        assert_eq!(fs::read(&path).unwrap(), b"first");
    }
}
