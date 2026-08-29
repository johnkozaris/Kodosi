use std::{
    fs, io,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::{
    AppError, Result,
    support::{
        platform::fs as support_fs,
        storage::atomic_file::{FileMode, atomic_write_json},
    },
};

const HOST_STATE_FILE_NAME: &str = "headless-host.json";
pub(in crate::headless_host) const HOST_CONTRACT_VERSION: u8 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::headless_host) struct HostStateFile {
    pub(in crate::headless_host) version: u8,
    pub(in crate::headless_host) pid: u32,
    pub(in crate::headless_host) socket_path: String,
    pub(in crate::headless_host) token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::headless_host) config_identity: Option<String>,
    pub(in crate::headless_host) started_at: String,
}

fn host_state_path() -> Option<PathBuf> {
    host_runtime_dir().map(|dir| host_state_path_in(&dir))
}

pub(in crate::headless_host) fn host_state_path_in(runtime_dir: &Path) -> PathBuf {
    runtime_dir.join(HOST_STATE_FILE_NAME)
}

pub(in crate::headless_host) fn host_runtime_dir() -> Option<PathBuf> {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        return Some(Path::new(&runtime_dir).join("kodosi"));
    }
    project_dirs().map(|dirs| dirs.data_local_dir().join("runtime"))
}

fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("com", "kodosi", "kodosi")
}

pub(in crate::headless_host) fn write_host_state_file(state: &HostStateFile) -> Result<()> {
    let path = host_state_path().ok_or_else(|| AppError::Unsupported {
        reason: "headless host runtime directory is unavailable".to_owned(),
    })?;
    write_host_state_file_at(&path, state)
}

fn write_host_state_file_at(path: &Path, state: &HostStateFile) -> Result<()> {
    let parent = path.parent().ok_or_else(|| AppError::Unsupported {
        reason: "headless host state path has no parent directory".to_owned(),
    })?;
    support_fs::ensure_dir(parent)?;

    atomic_write_json(path, state, true, FileMode::UserPrivate)
}

pub(in crate::headless_host) fn load_host_state_file_in(
    runtime_dir: &Path,
) -> Result<Option<HostStateFile>> {
    let path = host_state_path_in(runtime_dir);
    let payload = match fs::read(&path) {
        Ok(payload) => payload,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(AppError::Io(error)),
    };
    let state = serde_json::from_slice::<HostStateFile>(&payload).map_err(AppError::Json)?;
    if state.version != HOST_CONTRACT_VERSION {
        cleanup_host_state_file_in(runtime_dir);
        return Ok(None);
    }
    Ok(Some(state))
}

pub(in crate::headless_host) fn cleanup_host_state_file_in(runtime_dir: &Path) {
    drop(fs::remove_file(host_state_path_in(runtime_dir)));
}

pub(in crate::headless_host) fn cleanup_host_state_file_if_matches(
    runtime_dir: &Path,
    state: &HostStateFile,
) {
    match load_host_state_file_in(runtime_dir) {
        Ok(Some(current)) if current.token == state.token && current.pid == state.pid => {
            cleanup_host_state_file_in(runtime_dir);
        }
        Ok(Some(_)) => {
            tracing::debug!("host state file was replaced while connecting; leaving it in place");
        }
        Ok(None) => {}
        Err(error) => {
            tracing::debug!(%error, "could not re-read the host state file before cleanup");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{HostStateFile, write_host_state_file_at};

    #[test]
    fn write_host_state_file_creates_runtime_dir() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let state_path = temp.path().join("runtime").join("headless-host.json");
        let state = HostStateFile {
            version: 2,
            pid: 123,
            socket_path: "/run/kodosi/headless-host.sock".to_owned(),
            token: "test-token".to_owned(),
            config_identity: Some("config-identity".to_owned()),
            started_at: "2026-04-27T18:00:00Z".to_owned(),
        };

        write_host_state_file_at(&state_path, &state).expect("write host state");

        assert!(state_path.exists());
    }
}
