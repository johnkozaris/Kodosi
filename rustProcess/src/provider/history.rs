use std::{fs, path::Path, time::SystemTime};

use super::{ConversationListPage, ConversationRef, Provider, storage};

pub(super) fn discover(
    state_root: &Path,
    provider: Provider,
    cursor: Option<&str>,
    limit: Option<usize>,
    max_bytes: Option<usize>,
) -> Result<ConversationListPage, String> {
    let limit = storage::bounded("limit", limit.unwrap_or(30), 100)?;
    let budget = storage::bounded("maxBytes", max_bytes.unwrap_or(128 * 1024), 512 * 1024)?;
    match provider {
        Provider::Copilot => copilot(state_root, cursor, limit, budget),
        Provider::Claude => claude(state_root, cursor, limit, budget),
    }
}

fn empty() -> ConversationListPage {
    ConversationListPage {
        items: vec![],
        next_cursor: None,
        has_more: false,
        response_bytes: 0,
    }
}

fn cursor_id<'a>(cursor: Option<&'a str>, prefix: &str) -> Result<Option<&'a str>, String> {
    cursor
        .map(|cursor| {
            let id = cursor
                .strip_prefix(prefix)
                .ok_or("Invalid history cursor")?;
            if prefix == "all-copilot:" {
                if id.is_empty() || id.len() > 116 || id.chars().any(char::is_control) {
                    return Err("Invalid history cursor".into());
                }
            } else {
                storage::conversation_id(id)?;
            }
            Ok(id)
        })
        .transpose()
}

fn copilot(
    state_root: &Path,
    cursor: Option<&str>,
    limit: usize,
    budget: usize,
) -> Result<ConversationListPage, String> {
    if !state_root
        .join("session-store.db")
        .try_exists()
        .map_err(|error| error.to_string())?
    {
        return Ok(empty());
    }
    let connection = storage::copilot_database(state_root)?;
    let cursor = cursor_id(cursor, "all-copilot:")?;
    let anchor = if let Some(id) = cursor {
        Some(
            connection
                .query_row(
                    "SELECT COALESCE(updated_at,created_at,'') FROM sessions WHERE id = ?",
                    [id],
                    |row| row.get::<_, String>(0),
                )
                .map_err(|_| "History changed; reload the list".to_owned())?,
        )
    } else {
        None
    };
    let mut statement = connection.prepare("SELECT id, substr(cwd,1,4096), substr(summary,1,1024), substr(created_at,1,128), substr(updated_at,1,128) FROM sessions WHERE cwd IS NOT NULL AND (?1 IS NULL OR COALESCE(updated_at,created_at,'') < ?1 OR (COALESCE(updated_at,created_at,'') = ?1 AND id > ?2)) ORDER BY COALESCE(updated_at,created_at,'') DESC, id ASC LIMIT ?3").map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(
            rusqlite::params![
                anchor,
                cursor,
                i64::try_from(limit + 1).map_err(|error| error.to_string())?
            ],
            |row| {
                Ok(ConversationRef {
                    provider: Provider::Copilot,
                    native_conversation_id: row.get(0)?,
                    working_directory: row.get(1)?,
                    title: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            },
        )
        .map_err(|error| error.to_string())?;
    let mut page = empty();
    let mut last_id = None;
    for (consumed, row) in rows.enumerate() {
        if consumed == limit {
            page.has_more = true;
            break;
        }
        let item = row.map_err(|error| error.to_string())?;
        let bytes = serde_json::to_vec(&item)
            .map_err(|error| error.to_string())?
            .len();
        if page.response_bytes + bytes > budget {
            if consumed == 0 {
                return Err("Conversation metadata exceeds maxBytes".into());
            }
            page.has_more = true;
            break;
        }
        if storage::conversation_id(&item.native_conversation_id).is_err() {
            if item.native_conversation_id.is_empty()
                || item.native_conversation_id.len() > 116
                || item.native_conversation_id.chars().any(char::is_control)
            {
                return Err("Conversation index contains an invalid identifier".into());
            }
            last_id = Some(item.native_conversation_id);
            continue;
        }
        last_id = Some(item.native_conversation_id.clone());
        page.response_bytes += bytes;
        page.items.push(item);
    }
    if page.has_more {
        page.next_cursor = last_id.map(|id| format!("all-copilot:{id}"));
    }
    Ok(page)
}

fn claude(
    state_root: &Path,
    cursor: Option<&str>,
    limit: usize,
    budget: usize,
) -> Result<ConversationListPage, String> {
    let root = storage::transcript_root(state_root, Provider::Claude);
    let projects = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(empty()),
        Err(error) => return Err(error.to_string()),
    };
    let mut files = Vec::new();
    for (index, project) in projects.enumerate() {
        if index >= storage::MAX_DIRECTORY_ENTRIES {
            return Err("Too many history projects; choose a folder".into());
        }
        let project = project.map_err(|error| error.to_string())?;
        if !project
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            continue;
        }
        let name = project.file_name();
        let Some(project_name) = name.to_str() else {
            continue;
        };
        for (index, entry) in fs::read_dir(project.path())
            .map_err(|error| error.to_string())?
            .enumerate()
        {
            if index >= storage::MAX_DIRECTORY_ENTRIES || files.len() >= 20_000 {
                return Err("Too many history files; choose a folder".into());
            }
            let entry = entry.map_err(|error| error.to_string())?;
            if !entry
                .file_type()
                .map_err(|error| error.to_string())?
                .is_file()
            {
                continue;
            }
            let name = entry.file_name();
            let Some(id) = name.to_str().and_then(|name| name.strip_suffix(".jsonl")) else {
                continue;
            };
            if storage::conversation_id(id).is_err() {
                continue;
            }
            let metadata = entry.metadata().map_err(|error| error.to_string())?;
            files.push((
                metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                project_name.to_owned(),
                id.to_owned(),
                metadata.len(),
                metadata.created().ok(),
            ));
        }
    }
    files.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.2.cmp(&right.2)));
    let start = match cursor_id(cursor, "all-claude:")? {
        None => 0,
        Some(id) => files
            .iter()
            .position(|file| file.2 == id)
            .map(|index| index + 1)
            .ok_or("History changed; reload the list")?,
    };
    let mut page = empty();
    let mut consumed = start;
    for (modified, project, id, len, created) in files.iter().skip(start).take(128) {
        if let Some(item) = claude_item(&root, project, id, *len, *created, *modified)? {
            let bytes = serde_json::to_vec(&item)
                .map_err(|error| error.to_string())?
                .len();
            if page.response_bytes + bytes > budget {
                if consumed == start {
                    return Err("Conversation metadata exceeds maxBytes".into());
                }
                break;
            }
            page.response_bytes += bytes;
            page.items.push(item);
        }
        consumed += 1;
        if page.items.len() >= limit {
            break;
        }
    }
    page.has_more = consumed < files.len();
    if page.has_more {
        page.next_cursor = Some(format!("all-claude:{}", files[consumed - 1].2));
    }
    Ok(page)
}

fn claude_item(
    root: &Path,
    project: &str,
    id: &str,
    len: u64,
    created: Option<SystemTime>,
    modified: SystemTime,
) -> Result<Option<ConversationRef>, String> {
    let handle = storage::open_directory(root, project)?;
    let mut file = storage::open_file(&handle, &format!("{id}.jsonl"))?;
    let Some(directory) =
        storage::claude_directory(&mut file, len.min(storage::MAX_IDENTITY_BYTES))
            .ok()
            .flatten()
    else {
        return Ok(None);
    };
    Ok(Some(ConversationRef {
        provider: Provider::Claude,
        native_conversation_id: id.to_owned(),
        working_directory: directory.to_string_lossy().into_owned(),
        title: preview_title(&mut file),
        created_at: created.and_then(timestamp),
        updated_at: timestamp(modified),
    }))
}

fn timestamp(value: SystemTime) -> Option<String> {
    time::OffsetDateTime::from(value)
        .format(&time::format_description::well_known::Rfc3339)
        .ok()
}

pub(super) fn preview_title(file: &mut fs::File) -> Option<String> {
    use std::io::{Read as _, Seek as _};
    file.rewind().ok()?;
    let mut bytes = Vec::new();
    file.take(storage::MAX_IDENTITY_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    bytes
        .split(|byte| *byte == b'\n')
        .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
        .find_map(|value| {
            let text = value
                .get("customTitle")
                .and_then(serde_json::Value::as_str)
                .or_else(|| value.get("summary").and_then(serde_json::Value::as_str))
                .or_else(|| {
                    if value.get("type").and_then(serde_json::Value::as_str) == Some("user") {
                        value
                            .pointer("/message/content")
                            .and_then(serde_json::Value::as_str)
                    } else {
                        None
                    }
                })?;
            let title = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if title.is_empty() {
                None
            } else {
                Some(title.chars().take(160).collect())
            }
        })
}
