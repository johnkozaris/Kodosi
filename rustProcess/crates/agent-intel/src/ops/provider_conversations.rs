use std::{fs, path::Path};

use crate::{
    AgentKind, claude::ClaudeCodeProvider, copilot::session_store_db::CopilotSessionStore,
};

use super::{
    dto::{ProviderConversationPage, ProviderConversationRef},
    io::format_rfc3339,
    path_safety,
};

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 100;
const DEFAULT_RESPONSE_BYTES: usize = 128 * 1024;
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 4_096;
const MAX_CURSOR_BYTES: usize = 512;
const MAX_TITLE_BYTES: usize = 1_024;
const MAX_CLAUDE_CWD_RECORD_BYTES: usize = 16 * 1024;
const MAX_CLAUDE_SCAN_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedResumeTarget {
    pub canonical_working_directory: String,
}

pub async fn discover(
    home: &Path,
    provider: AgentKind,
    working_directory: &str,
    cursor: Option<&str>,
    limit: Option<usize>,
    max_bytes: Option<usize>,
) -> Result<ProviderConversationPage, String> {
    let canonical_working_directory =
        path_safety::canonicalize_user_dir("workingDirectory", working_directory)
            .map_err(|error| format!("invalid workingDirectory: {error}"))?;
    let working_directory = canonical_working_directory
        .to_str()
        .ok_or_else(|| "workingDirectory must be valid UTF-8".to_owned())?
        .to_owned();
    let limit = bounded_value("limit", limit.unwrap_or(DEFAULT_LIMIT), MAX_LIMIT)?;
    let max_bytes = bounded_value(
        "maxBytes",
        max_bytes.unwrap_or(DEFAULT_RESPONSE_BYTES),
        MAX_RESPONSE_BYTES,
    )?;
    if cursor.is_some_and(|value| value.len() > MAX_CURSOR_BYTES) {
        return Err(format!("cursor exceeds {MAX_CURSOR_BYTES} bytes"));
    }

    let home = home.to_owned();
    let cursor = cursor.map(ToOwned::to_owned);
    tokio::task::spawn_blocking(move || {
        let mut items = match provider {
            AgentKind::Claude => discover_claude(&home, &working_directory)?,
            AgentKind::Copilot => discover_copilot(&home, &working_directory)?,
        };
        items.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| right.created_at.cmp(&left.created_at))
                .then_with(|| {
                    left.native_conversation_id
                        .cmp(&right.native_conversation_id)
                })
        });
        paginate(&items, provider, cursor.as_deref(), limit, max_bytes)
    })
    .await
    .map_err(|error| format!("provider conversation discovery task failed: {error}"))?
}

pub fn validate_resume_target(
    home: &Path,
    provider: AgentKind,
    working_directory: &str,
    native_conversation_id: &str,
) -> Result<ValidatedResumeTarget, String> {
    path_safety::validate_uuid("nativeConversationId", native_conversation_id)
        .map_err(|error| error.to_string())?;
    let working_directory =
        path_safety::canonicalize_user_dir("workingDirectory", working_directory)
            .map_err(|error| format!("invalid workingDirectory: {error}"))?;
    let canonical_working_directory = working_directory
        .to_str()
        .ok_or_else(|| "workingDirectory must be valid UTF-8".to_owned())?
        .to_owned();
    match provider {
        AgentKind::Claude => {
            let root = home.join(".claude").join("projects");
            let project = root.join(ClaudeCodeProvider::encode_project_path(
                &working_directory.to_string_lossy(),
            ));
            let transcript = project.join(format!("{native_conversation_id}.jsonl"));
            validate_transcript(&root, &transcript)?;
            let mut budget = MAX_CLAUDE_CWD_RECORD_BYTES;
            if read_claude_cwd(&transcript, &mut budget).as_deref()
                != Some(canonical_working_directory.as_str())
            {
                return Err("Claude conversation does not belong to workingDirectory".to_owned());
            }
        }
        AgentKind::Copilot => {
            let root = home.join(".copilot").join("session-state");
            validate_transcript(
                &root,
                &root.join(native_conversation_id).join("events.jsonl"),
            )?;
            let database = CopilotSessionStore::new(home.join(".copilot/session-store.db"));
            let stored = database
                .session_directory(native_conversation_id)
                .ok_or_else(|| "Copilot conversation is not indexed locally".to_owned())?;
            let stored = path_safety::canonicalize_user_dir("storedWorkingDirectory", &stored)
                .map_err(|error| format!("invalid Copilot working directory: {error}"))?;
            if stored != working_directory {
                return Err("Copilot conversation does not belong to workingDirectory".to_owned());
            }
        }
    }
    Ok(ValidatedResumeTarget {
        canonical_working_directory,
    })
}

fn discover_claude(
    home: &Path,
    working_directory: &str,
) -> Result<Vec<ProviderConversationRef>, String> {
    let root = home.join(".claude").join("projects");
    let project = root.join(ClaudeCodeProvider::encode_project_path(working_directory));
    let entries = bounded_entries(&project, "Claude project conversations")?;
    let mut items = Vec::new();
    let mut scan_budget = MAX_CLAUDE_SCAN_BYTES;
    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
            continue;
        }
        match entry.file_type() {
            Ok(file_type) if file_type.is_file() && !file_type.is_symlink() => {}
            _ => continue,
        }
        let Some(id) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if path_safety::validate_uuid("nativeConversationId", id).is_err()
            || validate_transcript(&root, &path).is_err()
        {
            continue;
        }
        if scan_budget == 0 {
            return Err(format!(
                "Claude conversation scan exceeds {MAX_CLAUDE_SCAN_BYTES} bytes"
            ));
        }
        if read_claude_cwd(&path, &mut scan_budget).as_deref() != Some(working_directory) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        items.push(ProviderConversationRef {
            provider: "claude".to_owned(),
            native_conversation_id: id.to_owned(),
            working_directory: working_directory.to_owned(),
            title: None,
            created_at: metadata.created().ok().and_then(format_rfc3339),
            updated_at: metadata.modified().ok().and_then(format_rfc3339),
        });
    }
    Ok(items)
}

fn discover_copilot(
    home: &Path,
    working_directory: &str,
) -> Result<Vec<ProviderConversationRef>, String> {
    let database = home.join(".copilot").join("session-store.db");
    if !database.exists() {
        return Ok(Vec::new());
    }
    let sessions = CopilotSessionStore::new(database)
        .list_sessions_for_directory(working_directory)
        .ok_or_else(|| "Copilot session store is unavailable or incompatible".to_owned())?;
    let root = home.join(".copilot").join("session-state");
    Ok(sessions
        .into_iter()
        .filter(|session| {
            path_safety::validate_uuid("nativeConversationId", &session.id).is_ok()
                && validate_transcript(&root, &root.join(&session.id).join("events.jsonl")).is_ok()
        })
        .map(|session| ProviderConversationRef {
            provider: "copilot".to_owned(),
            native_conversation_id: session.id,
            working_directory: working_directory.to_owned(),
            title: session
                .summary
                .map(|summary| truncate_utf8(summary, MAX_TITLE_BYTES)),
            created_at: session.created_at,
            updated_at: session.updated_at,
        })
        .collect())
}

fn paginate(
    items: &[ProviderConversationRef],
    provider: AgentKind,
    cursor: Option<&str>,
    limit: usize,
    max_bytes: usize,
) -> Result<ProviderConversationPage, String> {
    let start = match cursor {
        None => 0,
        Some(cursor) => {
            let id = decode_cursor(cursor, provider)?;
            items
                .iter()
                .position(|item| item.native_conversation_id == id)
                .map(|index| index + 1)
                .ok_or_else(|| "cursor no longer exists; restart discovery".to_owned())?
        }
    };
    let mut page = Vec::new();
    let mut response_bytes = 0_usize;
    for item in items.iter().skip(start).take(limit) {
        let item_bytes = serde_json::to_vec(item)
            .map_err(|error| format!("encode conversation metadata: {error}"))?
            .len();
        if response_bytes.saturating_add(item_bytes) > max_bytes {
            if page.is_empty() {
                return Err("one conversation record exceeds maxBytes".to_owned());
            }
            break;
        }
        response_bytes += item_bytes;
        page.push(item.clone());
    }
    let consumed = start + page.len();
    let has_more = consumed < items.len();
    let next_cursor = if has_more {
        page.last()
            .map(|item| encode_cursor(provider, &item.native_conversation_id))
    } else {
        None
    };
    Ok(ProviderConversationPage {
        items: page,
        next_cursor,
        has_more,
        response_bytes,
    })
}

fn encode_cursor(provider: AgentKind, id: &str) -> String {
    format!("v1:{}:{id}", provider.canonical())
}

fn decode_cursor(cursor: &str, provider: AgentKind) -> Result<&str, String> {
    let prefix = format!("v1:{}:", provider.canonical());
    let id = cursor
        .strip_prefix(&prefix)
        .ok_or_else(|| "cursor does not match this provider".to_owned())?;
    path_safety::validate_uuid("cursor", id).map_err(|error| error.to_string())?;
    Ok(id)
}

fn validate_transcript(root: &Path, path: &Path) -> Result<(), String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("transcript unavailable: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("transcript must be a regular non-symlink file".to_owned());
    }
    if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
        return Err("transcript must be JSONL".to_owned());
    }
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("resolve transcript root: {error}"))?;
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("resolve transcript: {error}"))?;
    if !canonical.starts_with(canonical_root) {
        return Err("transcript escapes provider root".to_owned());
    }
    Ok(())
}

fn bounded_entries(directory: &Path, label: &str) -> Result<Vec<fs::DirEntry>, String> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("read {label}: {error}")),
    };
    let mut bounded = Vec::with_capacity(MAX_DIRECTORY_ENTRIES.min(256));
    for entry in entries {
        if bounded.len() == MAX_DIRECTORY_ENTRIES {
            return Err(format!("{label} exceeds {MAX_DIRECTORY_ENTRIES} entries"));
        }
        bounded.push(entry.map_err(|error| format!("read {label}: {error}"))?);
    }
    Ok(bounded)
}

fn read_claude_cwd(path: &Path, remaining_budget: &mut usize) -> Option<String> {
    use std::io::Read;

    if *remaining_budget == 0 {
        return None;
    }
    let read_limit = (*remaining_budget).min(MAX_CLAUDE_CWD_RECORD_BYTES);
    let mut bytes = Vec::with_capacity(read_limit);
    fs::File::open(path)
        .ok()?
        .take(u64::try_from(read_limit).ok()?)
        .read_to_end(&mut bytes)
        .ok()?;
    *remaining_budget = (*remaining_budget).saturating_sub(bytes.len());
    for line in bytes.split(|byte| *byte == b'\n').take(32) {
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(cwd) = value.get("cwd").and_then(serde_json::Value::as_str) {
            return path_safety::canonicalize_user_dir("cwd", cwd)
                .ok()
                .and_then(|path| path.to_str().map(ToOwned::to_owned));
        }
    }
    None
}

fn bounded_value(label: &str, value: usize, maximum: usize) -> Result<usize, String> {
    if value == 0 || value > maximum {
        return Err(format!("{label} must be between 1 and {maximum}"));
    }
    Ok(value)
}

fn truncate_utf8(mut value: String, maximum_bytes: usize) -> String {
    if value.len() > maximum_bytes {
        value.truncate(value.floor_char_boundary(maximum_bytes));
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn uuid(index: u128) -> String {
        uuid::Uuid::from_u128(index).to_string()
    }

    #[test]
    fn pagination_keeps_provider_identity_and_cursor_exact() {
        let items: Vec<ProviderConversationRef> = (1..=3)
            .map(|index| ProviderConversationRef {
                provider: "claude".to_owned(),
                native_conversation_id: uuid(index),
                working_directory: "/repo".to_owned(),
                title: None,
                created_at: None,
                updated_at: Some(format!("{index}")),
            })
            .collect();
        let first = paginate(&items, AgentKind::Claude, None, 2, 64 * 1024).expect("first page");
        assert_eq!(first.items.len(), 2);
        assert!(first.has_more);
        let second = paginate(
            &items,
            AgentKind::Claude,
            first.next_cursor.as_deref(),
            2,
            64 * 1024,
        )
        .expect("second page");
        assert_eq!(second.items.len(), 1);
        assert!(!second.has_more);
    }

    #[test]
    fn cursor_is_provider_scoped() {
        assert!(
            decode_cursor(
                &encode_cursor(AgentKind::Claude, &uuid(1)),
                AgentKind::Copilot,
            )
            .is_err()
        );
    }

    #[test]
    fn response_bounds_are_rejected_not_silently_clamped() {
        assert!(bounded_value("limit", 0, MAX_LIMIT).is_err());
        assert!(bounded_value("limit", MAX_LIMIT + 1, MAX_LIMIT).is_err());
        assert!(bounded_value("limit", MAX_LIMIT, MAX_LIMIT).is_ok());
    }

    #[tokio::test]
    async fn claude_discovery_is_directory_scoped_and_read_only() {
        let home = tempfile::tempdir().expect("home");
        let working = home.path().join("repo");
        fs::create_dir_all(&working).expect("working directory");
        let working = working.canonicalize().expect("canonical working directory");
        let project =
            home.path()
                .join(".claude/projects")
                .join(ClaudeCodeProvider::encode_project_path(
                    &working.to_string_lossy(),
                ));
        fs::create_dir_all(&project).expect("Claude project");
        let id = uuid(1);
        let transcript = project.join(format!("{id}.jsonl"));
        let bytes = format!(
            "{{\"cwd\":{},\"type\":\"user\",\"message\":\"hello\"}}\n",
            serde_json::to_string(&working.to_string_lossy()).expect("cwd")
        );
        fs::write(&transcript, bytes.as_bytes()).expect("transcript");

        let page = discover(
            home.path(),
            AgentKind::Claude,
            &working.to_string_lossy(),
            None,
            Some(10),
            Some(64 * 1024),
        )
        .await
        .expect("discover");

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].provider, "claude");
        assert_eq!(page.items[0].native_conversation_id, id);
        assert_eq!(
            validate_resume_target(
                home.path(),
                AgentKind::Claude,
                &working.to_string_lossy(),
                &id,
            )
            .expect("validated target")
            .canonical_working_directory,
            working.to_string_lossy()
        );
        let other = home.path().join("other");
        fs::create_dir_all(&other).expect("other directory");
        assert!(
            validate_resume_target(
                home.path(),
                AgentKind::Claude,
                &other.to_string_lossy(),
                &id,
            )
            .is_err()
        );
        assert_eq!(
            fs::read(&transcript).expect("read transcript"),
            bytes.as_bytes()
        );
    }

    #[tokio::test]
    async fn copilot_discovery_requires_complete_local_session_state() {
        let home = tempfile::tempdir().expect("home");
        let working = home.path().join("repo");
        fs::create_dir_all(&working).expect("working directory");
        let working = working.canonicalize().expect("canonical working directory");
        let copilot = home.path().join(".copilot");
        fs::create_dir_all(&copilot).expect("Copilot root");
        let database = copilot.join("session-store.db");
        let connection = Connection::open(&database).expect("database");
        connection
            .execute_batch(
                "CREATE TABLE schema_version (version INTEGER NOT NULL);
                 INSERT INTO schema_version(version) VALUES (3);
                 CREATE TABLE sessions (
                   id TEXT PRIMARY KEY, cwd TEXT, repository TEXT, branch TEXT,
                   summary TEXT, created_at TEXT, updated_at TEXT, host_type TEXT
                 );",
            )
            .expect("schema");
        let complete = uuid(2);
        let incomplete = uuid(3);
        for id in [&complete, &incomplete] {
            connection
                .execute(
                    "INSERT INTO sessions
                     (id, cwd, repository, summary, created_at, updated_at, host_type)
                     VALUES (?, ?, 'owner/repo', 'Work', '2026-08-01', '2026-08-02', 'cli')",
                    rusqlite::params![id, working.to_string_lossy()],
                )
                .expect("session");
        }
        let transcript = copilot
            .join("session-state")
            .join(&complete)
            .join("events.jsonl");
        fs::create_dir_all(transcript.parent().expect("state parent")).expect("state");
        let bytes = b"{\"type\":\"user.message\",\"data\":{\"content\":\"hello\"}}\n";
        fs::write(&transcript, bytes).expect("transcript");

        let page = discover(
            home.path(),
            AgentKind::Copilot,
            &working.to_string_lossy(),
            None,
            Some(10),
            Some(64 * 1024),
        )
        .await
        .expect("discover");

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].native_conversation_id, complete);
        assert!(
            validate_resume_target(
                home.path(),
                AgentKind::Copilot,
                &working.to_string_lossy(),
                &complete,
            )
            .is_ok()
        );
        let other = home.path().join("other");
        fs::create_dir_all(&other).expect("other directory");
        assert!(
            validate_resume_target(
                home.path(),
                AgentKind::Copilot,
                &other.to_string_lossy(),
                &complete,
            )
            .is_err()
        );
        assert_eq!(fs::read(&transcript).expect("read transcript"), bytes);
    }
}
