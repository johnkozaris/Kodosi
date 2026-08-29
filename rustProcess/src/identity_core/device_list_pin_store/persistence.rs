use std::{fs, fs::OpenOptions, path::Path};

use crate::{
    AppError, Result,
    support::{
        platform::fs as support_fs,
        storage::atomic_file::{FileMode, atomic_write_json},
    },
};

use super::schema::{LegacyPinFileV1, LegacyPinFileV2, PIN_FILE_SCHEMA_VERSION, PinFile};

fn parse_pin_file(payload: &str) -> Result<(PinFile, bool)> {
    let value: serde_json::Value = serde_json::from_str(payload).map_err(AppError::Json)?;
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| AppError::InvalidBackendData {
            field: "pin_file.version".to_owned(),
            reason: "pin file is missing a version".to_owned(),
        })?;
    match version {
        1 => {
            let legacy: LegacyPinFileV1 = serde_json::from_value(value).map_err(AppError::Json)?;
            Ok((legacy.into(), true))
        }
        2 => {
            let legacy: LegacyPinFileV2 = serde_json::from_value(value).map_err(AppError::Json)?;
            Ok((legacy.into(), true))
        }
        3 => {
            let file: PinFile = serde_json::from_value(value).map_err(AppError::Json)?;
            Ok((file, true))
        }
        current if current == u64::from(PIN_FILE_SCHEMA_VERSION) => {
            let file: PinFile = serde_json::from_value(value).map_err(AppError::Json)?;
            Ok((file, false))
        }
        _ => Err(AppError::InvalidBackendData {
            field: "pin_file.version".to_owned(),
            reason: format!(
                "pin file version {version} is not the expected {PIN_FILE_SCHEMA_VERSION}"
            ),
        }),
    }
}

fn read_pin_file(path: &Path) -> Result<(PinFile, bool)> {
    match fs::read_to_string(path) {
        Ok(payload) => parse_pin_file(&payload),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok((PinFile::default(), false))
        }
        Err(error) => Err(AppError::Io(error)),
    }
}

pub(super) fn load_pin_file(path: &Path) -> Result<PinFile> {
    if let Some(parent) = path.parent() {
        support_fs::ensure_dir(parent)?;
    }
    let lock_path = path.with_extension("json.lock");
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    lock.lock()?;

    let (mut file, migrated) = read_pin_file(path)?;
    if migrated {
        file.version = PIN_FILE_SCHEMA_VERSION;
        file.revision =
            file.revision
                .checked_add(1)
                .ok_or_else(|| AppError::InvalidBackendData {
                    field: "pin_file.revision".to_owned(),
                    reason: "pin-store revision exhausted".to_owned(),
                })?;
        persist_pin_file(path, &file)?;
    }
    Ok(file)
}

pub(super) fn persist_pin_file(path: &Path, file: &PinFile) -> Result<()> {
    if let Some(parent) = path.parent() {
        support_fs::ensure_dir(parent)?;
    }
    atomic_write_json(path, file, true, FileMode::UserPrivate)
}

pub(super) fn update_pin_file<T>(
    path: &Path,
    update: impl FnOnce(&PinFile) -> Result<(PinFile, T)>,
) -> Result<(PinFile, T)> {
    if let Some(parent) = path.parent() {
        support_fs::ensure_dir(parent)?;
    }
    let lock_path = path.with_extension("json.lock");
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    lock.lock()?;

    let (current, _) = read_pin_file(path)?;
    let (mut pending, output) = update(&current)?;
    pending.version = PIN_FILE_SCHEMA_VERSION;
    pending.revision =
        current
            .revision
            .checked_add(1)
            .ok_or_else(|| AppError::InvalidBackendData {
                field: "pin_file.revision".to_owned(),
                reason: "pin-store revision exhausted".to_owned(),
            })?;
    persist_pin_file(path, &pending)?;
    Ok((pending, output))
}

pub(super) fn default_clock_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}
