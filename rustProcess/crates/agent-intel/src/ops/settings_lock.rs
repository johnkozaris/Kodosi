use std::path::Path;

pub(crate) fn with_lock<T>(
    settings_path: &Path,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let parent = settings_path
        .parent()
        .ok_or_else(|| "settings path has no parent".to_owned())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("mkdir {}: {error}", parent.display()))?;
    let file_name = settings_path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("settings");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(parent.join(format!("{file_name}.kodosi.lock")))
        .map_err(|error| format!("open settings lock: {error}"))?;
    lock.lock()
        .map_err(|error| format!("lock settings: {error}"))?;
    let result = operation();
    drop(lock.unlock());
    result
}
