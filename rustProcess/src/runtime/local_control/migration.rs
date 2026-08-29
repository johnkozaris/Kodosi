use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::{AppError, Result};
use directories::ProjectDirs;
use kodosi_domain::{
    ids::SessionId,
    session::{SessionState, SessionSummary},
    terminal::TerminalSize,
};
use serde::Deserialize;
use time::OffsetDateTime;

#[cfg_attr(test, allow(dead_code))]
const CLIENT_SERVER_CONTRACT_VERSION: usize = 1;

#[derive(Debug, Clone)]
pub(crate) struct CachedOwnedSession {
    pub(crate) summary: SessionSummary,
    pub(crate) cache_age: std::time::Duration,
}

pub(crate) fn load_cached_sessions(cache_root: Option<&Path>) -> Result<Vec<CachedOwnedSession>> {
    let Some(root) = cache_root else {
        return Ok(Vec::new());
    };
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(AppError::Io(error)),
    };

    let mut cached_sessions = Vec::new();
    for entry in entries.flatten() {
        let session_dir = entry.path();
        if !session_dir.is_dir() {
            continue;
        }
        match load_cached_session_from_dir(&session_dir) {
            Ok(Some(session)) => cached_sessions.push(session),
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(
                    session_dir = %session_dir.display(),
                    "failed to load cached session: {error}"
                );
            }
        }
    }
    cached_sessions.sort_by_key(|cached| cached.cache_age);
    Ok(cached_sessions)
}

pub(crate) fn delete_cached_session(cache_root: Option<&Path>, runtime_name: &str) -> Result<()> {
    validate_runtime_name(runtime_name)?;
    let Some(session_dir) = session_info_folder_for_session(cache_root, runtime_name)? else {
        return Ok(());
    };
    match fs::remove_dir_all(&session_dir) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppError::Io(error)),
    }
}

fn load_cached_session_from_dir(session_dir: &Path) -> Result<Option<CachedOwnedSession>> {
    let Some(runtime_name) = session_dir.file_name().and_then(|value| value.to_str()) else {
        return Ok(None);
    };
    let cache_path = session_dir.join("session.json");
    let document_text = match fs::read_to_string(&cache_path) {
        Ok(value) => value,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(AppError::Io(error)),
    };
    let cache_age = fs::metadata(&cache_path)
        .ok()
        .and_then(|metadata| metadata.created().ok().or_else(|| metadata.modified().ok()))
        .and_then(|timestamp| timestamp.elapsed().ok())
        .unwrap_or_default();
    parse_cached_session(&document_text, cache_age, Some(runtime_name))
}

fn parse_cached_session(
    document_text: &str,
    cache_age: std::time::Duration,
    trusted_runtime_name: Option<&str>,
) -> Result<Option<CachedOwnedSession>> {
    let document: CachedSessionDocument =
        serde_json::from_str(document_text).map_err(AppError::Json)?;
    document
        .into_cached_session(cache_age, trusted_runtime_name)
        .map(Some)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedSessionDocument {
    session_id: String,
    title: String,
    runtime_name: String,
    working_dir: Option<String>,
    room_name: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    last_update: OffsetDateTime,
    rows: u16,
    cols: u16,
}

impl CachedSessionDocument {
    fn into_cached_session(
        self,
        cache_age: std::time::Duration,
        trusted_runtime_name: Option<&str>,
    ) -> Result<CachedOwnedSession> {
        let id = SessionId::try_from(self.session_id.as_str()).map_err(|error| {
            AppError::InvalidBackendData {
                field: "session_cache.sessionId".to_owned(),
                reason: error.to_string(),
            }
        })?;
        let size = TerminalSize::new(self.rows, self.cols)?;
        let runtime_name = trusted_runtime_name.unwrap_or(&self.runtime_name);
        validate_runtime_name(runtime_name)?;
        let mut summary =
            SessionSummary::new_owned(id, self.title, runtime_name.to_owned(), size, None);
        summary.state = SessionState::Stopped;
        summary.working_dir = self.working_dir;
        summary.room_name = self.room_name;
        summary.created_at = self.created_at;
        summary.last_update = self.last_update;
        Ok(CachedOwnedSession { summary, cache_age })
    }
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn default_cache_root() -> Result<Option<PathBuf>> {
    if let Some(root) = crate::support::storage::paths::cache_root()? {
        return Ok(Some(
            root.join(format!("contract_version_{CLIENT_SERVER_CONTRACT_VERSION}"))
                .join("session_info"),
        ));
    }
    Ok(project_dirs().map(|project_dirs| {
        project_dirs
            .cache_dir()
            .join(format!("contract_version_{CLIENT_SERVER_CONTRACT_VERSION}"))
            .join("session_info")
    }))
}

fn session_info_folder_for_session(
    cache_root: Option<&Path>,
    runtime_name: &str,
) -> Result<Option<PathBuf>> {
    validate_runtime_name(runtime_name)?;
    Ok(cache_root.map(|root| root.join(runtime_name)))
}

fn validate_runtime_name(runtime_name: &str) -> Result<()> {
    let mut components = Path::new(runtime_name).components();
    let safe = !runtime_name.contains(['/', '\\'])
        && matches!(components.next(), Some(std::path::Component::Normal(_)))
        && components.next().is_none();
    if safe {
        Ok(())
    } else {
        Err(AppError::InvalidBackendData {
            field: "session_cache.runtimeName".to_owned(),
            reason: "runtime name must be one relative path component".to_owned(),
        })
    }
}

#[cfg_attr(test, allow(dead_code))]
fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("com", "kodosi", "kodosi")
}

#[cfg(test)]
mod tests {
    use super::{parse_cached_session, validate_runtime_name};

    #[test]
    fn decodes_retired_cached_session_documents() {
        let id = kodosi_domain::ids::SessionId::new();
        let document = serde_json::json!({
            "sessionId": id.to_string(),
            "title": "cached",
            "runtimeName": "runtime-cached",
            "workingDir": null,
            "roomName": null,
            "createdAt": "2026-08-17T00:00:00Z",
            "lastUpdate": "2026-08-17T00:00:01Z",
            "rows": 24,
            "cols": 80
        })
        .to_string();
        let cached = parse_cached_session(&document, std::time::Duration::default(), None)
            .unwrap_or_else(|error| panic!("cache should parse: {error}"))
            .unwrap_or_else(|| panic!("cache should exist"));

        assert_eq!(cached.summary.id, id);
        assert_eq!(cached.summary.title, "cached");
        assert_eq!(cached.summary.runtime_name, "runtime-cached");
        assert_eq!(cached.summary.size.rows(), 24);
        assert_eq!(cached.summary.size.cols(), 80);
        assert_eq!(
            cached.summary.state,
            kodosi_domain::session::SessionState::Stopped
        );
    }

    #[test]
    fn runtime_name_must_not_escape_cache_root() {
        for invalid in ["../Documents", "/Users/me", ".", "a/b", "a\\b"] {
            assert!(
                validate_runtime_name(invalid).is_err(),
                "{invalid:?} must be rejected"
            );
        }
        assert!(validate_runtime_name("runtime-abc123").is_ok());
    }
}
