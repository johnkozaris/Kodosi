use std::{fs, path::Path, time::SystemTime};

use super::{ConversationListPage, ConversationRef, Provider, storage};

const MAX_SCAN_BYTES: u64 = 8 * 1024 * 1024;
const MAX_PAGE_BYTES: usize = 512 * 1024;
const MAX_TITLE_BYTES: usize = 1_024;

pub(super) fn discover(
    state_root: &Path,
    provider: Provider,
    directory: &Path,
    cursor: Option<&str>,
    limit: Option<usize>,
    max_bytes: Option<usize>,
) -> Result<ConversationListPage, String> {
    let limit = storage::bounded("limit", limit.unwrap_or(50), 100)?;
    let max_bytes = storage::bounded("maxBytes", max_bytes.unwrap_or(128 * 1024), MAX_PAGE_BYTES)?;
    let mut items = match provider {
        Provider::Claude => claude(state_root, directory)?,
        Provider::Copilot => copilot(state_root, directory)?,
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
    paginate(&items, provider, cursor, limit, max_bytes)
}

fn claude(state_root: &Path, directory: &Path) -> Result<Vec<ConversationRef>, String> {
    let root = storage::transcript_root(state_root, Provider::Claude);
    let project_name = storage::encoded_claude_directory(directory)?;
    let project_path = root.join(&project_name);
    let entries = match fs::read_dir(&project_path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("Could not list Claude conversations: {error}")),
    };
    let project = storage::open_directory(&root, &project_name)?;
    let mut items = Vec::new();
    let mut scan_bytes = 0;
    for (index, entry) in entries.enumerate() {
        if index >= storage::MAX_DIRECTORY_ENTRIES {
            return Err("Claude conversation directory exceeds its entry limit".to_owned());
        }
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(id) = name.strip_suffix(".jsonl") else {
            continue;
        };
        if storage::conversation_id(id).is_err() {
            continue;
        }
        let Ok(mut file) = storage::open_file(&project, name) else {
            continue;
        };
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        let read_bytes = metadata.len().min(storage::MAX_IDENTITY_BYTES);
        scan_bytes += read_bytes;
        if scan_bytes > MAX_SCAN_BYTES {
            return Err("Claude conversation scan exceeds its byte limit".to_owned());
        }
        if storage::claude_directory(&mut file, read_bytes)
            .ok()
            .flatten()
            .as_deref()
            != Some(directory)
        {
            continue;
        }
        items.push(ConversationRef {
            provider: Provider::Claude,
            native_conversation_id: id.to_owned(),
            working_directory: directory.to_string_lossy().into_owned(),
            title: super::history::preview_title(&mut file),
            created_at: metadata.created().ok().and_then(timestamp),
            updated_at: metadata.modified().ok().and_then(timestamp),
        });
    }
    Ok(items)
}

fn copilot(state_root: &Path, directory: &Path) -> Result<Vec<ConversationRef>, String> {
    if !state_root
        .join("session-store.db")
        .try_exists()
        .map_err(|error| error.to_string())?
    {
        return Ok(Vec::new());
    }
    let connection = storage::copilot_database(state_root)?;
    let mut statement = connection.prepare(
        "SELECT substr(id, 1, 64), substr(summary, 1, 1024), substr(created_at, 1, 128), substr(updated_at, 1, 128)
         FROM sessions WHERE cwd = ? ORDER BY COALESCE(updated_at, created_at) DESC, id ASC LIMIT 4097",
    ).map_err(|error| format!("Could not query Copilot conversations: {error}"))?;
    let rows = statement
        .query_map([directory.to_string_lossy().as_ref()], |row| {
            Ok(ConversationRef {
                provider: Provider::Copilot,
                native_conversation_id: row.get(0)?,
                working_directory: directory.to_string_lossy().into_owned(),
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })
        .map_err(|error| format!("Could not read Copilot conversations: {error}"))?;
    let mut items = Vec::new();
    let root = storage::transcript_root(state_root, Provider::Copilot);
    for (index, row) in rows.enumerate() {
        if index >= storage::MAX_DIRECTORY_ENTRIES {
            return Err("Copilot conversation index exceeds its entry limit".to_owned());
        }
        let mut item = row.map_err(|error| error.to_string())?;
        if storage::conversation_id(&item.native_conversation_id).is_err() {
            continue;
        }
        let Ok(session) = storage::open_directory(&root, &item.native_conversation_id) else {
            continue;
        };
        if storage::open_file(&session, "events.jsonl").is_err() {
            continue;
        }
        if let Some(title) = &mut item.title {
            title.truncate(title.floor_char_boundary(MAX_TITLE_BYTES));
        }
        items.push(item);
    }
    Ok(items)
}

fn timestamp(value: SystemTime) -> Option<String> {
    time::OffsetDateTime::from(value)
        .format(&time::format_description::well_known::Rfc3339)
        .ok()
}

fn paginate(
    items: &[ConversationRef],
    provider: Provider,
    cursor: Option<&str>,
    limit: usize,
    max_bytes: usize,
) -> Result<ConversationListPage, String> {
    let start = match cursor {
        None => 0,
        Some(cursor) => {
            if cursor.len() > 128 {
                return Err("Conversation cursor exceeds its limit".to_owned());
            }
            let prefix = format!("{}:", provider.executable());
            let id = cursor
                .strip_prefix(&prefix)
                .ok_or_else(|| "Cursor belongs to a different provider".to_owned())?;
            storage::conversation_id(id)?;
            items
                .iter()
                .position(|item| item.native_conversation_id == id)
                .map(|index| index + 1)
                .ok_or_else(|| {
                    "Conversation cursor no longer exists; refresh the list".to_owned()
                })?
        }
    };
    let mut page = ConversationListPage {
        items: Vec::new(),
        next_cursor: None,
        has_more: false,
        response_bytes: 0,
    };
    for item in items.iter().skip(start).take(limit) {
        let size = serde_json::to_vec(item)
            .map_err(|error| error.to_string())?
            .len();
        if page.response_bytes.saturating_add(size) > max_bytes {
            if page.items.is_empty() {
                return Err("Conversation metadata exceeds maxBytes".to_owned());
            }
            break;
        }
        page.response_bytes += size;
        page.items.push(item.clone());
    }
    page.has_more = start + page.items.len() < items.len();
    if page.has_more {
        page.next_cursor = page
            .items
            .last()
            .map(|item| format!("{}:{}", provider.executable(), item.native_conversation_id));
    }
    Ok(page)
}
