use std::path::Path;

use crate::copilot::session_store_db::{CopilotSessionStore, RepoSessionRow, RepositoryRow};

const MAX_REPOSITORY_NAME_LEN: usize = 512;

pub fn validate_repository_arg(value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err("repository must not be empty".to_owned());
    }
    if value.len() > MAX_REPOSITORY_NAME_LEN {
        return Err(format!(
            "repository name too long (> {MAX_REPOSITORY_NAME_LEN} bytes)"
        ));
    }
    if value.chars().any(char::is_control) {
        return Err("repository name contains control characters".to_owned());
    }
    Ok(())
}

pub async fn list_repositories(home: &Path) -> Result<Vec<RepositoryRow>, String> {
    let db_path = home.join(".copilot").join("session-store.db");
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    tokio::task::spawn_blocking(move || {
        CopilotSessionStore::new(db_path)
            .list_repositories()
            .ok_or_else(|| "session-store unavailable or schema mismatch".to_owned())
    })
    .await
    .map_err(|e| format!("blocking task failed: {e}"))?
}

pub async fn list_repo_sessions(
    home: &Path,
    repository: &str,
) -> Result<Vec<RepoSessionRow>, String> {
    validate_repository_arg(repository)?;
    let db_path = home.join(".copilot").join("session-store.db");
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    list_repo_sessions_at(&db_path, repository).await
}

pub async fn list_repo_sessions_at(
    db_path: &Path,
    repository: &str,
) -> Result<Vec<RepoSessionRow>, String> {
    validate_repository_arg(repository)?;
    let db_path = db_path.to_owned();
    let repo_owned = repository.to_owned();
    tokio::task::spawn_blocking(move || {
        CopilotSessionStore::new(db_path)
            .list_sessions_for_repository(&repo_owned)
            .ok_or_else(|| "session-store unavailable or schema mismatch".to_owned())
    })
    .await
    .map_err(|e| format!("blocking task failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty() {
        assert!(validate_repository_arg("").is_err());
    }

    #[test]
    fn rejects_oversized() {
        let big = "a".repeat(MAX_REPOSITORY_NAME_LEN + 1);
        assert!(validate_repository_arg(&big).is_err());
    }

    #[test]
    fn rejects_control_chars() {
        assert!(validate_repository_arg("owner/repo\x00").is_err());
        assert!(validate_repository_arg("owner/repo\nfake").is_err());
    }

    #[test]
    fn accepts_typical_names() {
        assert!(validate_repository_arg("owner/repo").is_ok());
        assert!(validate_repository_arg("github/kodosi-copilot-cli").is_ok());
    }
}
