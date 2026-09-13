use std::{
    fs::File,
    io::{Read, Seek},
    path::{Component, Path, PathBuf},
};

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use rustix::fs::{Mode, OFlags, openat};

use super::Provider;

pub(super) const MAX_DIRECTORY_ENTRIES: usize = 4_096;
pub(super) const MAX_IDENTITY_BYTES: u64 = 16 * 1024;

pub(super) fn canonical_directory(value: &str) -> Result<PathBuf, String> {
    let path = Path::new(value);
    if value.is_empty()
        || value.len() > 4_096
        || value.chars().any(char::is_control)
        || !path.is_absolute()
        || path.components().any(|part| part == Component::ParentDir)
    {
        return Err(
            "Working directory must be a bounded absolute path without parent traversal".to_owned(),
        );
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("Working directory unavailable: {error}"))?;
    if !canonical.is_dir() {
        return Err("Working directory is not a directory".to_owned());
    }
    Ok(canonical)
}

pub(super) fn conversation_id(value: &str) -> Result<String, String> {
    let id = uuid::Uuid::parse_str(value)
        .map_err(|_| "Conversation ID must be a canonical UUID".to_owned())?;
    if id.is_nil() || id.hyphenated().to_string() != value {
        return Err("Conversation ID must be a canonical nonzero UUID".to_owned());
    }
    Ok(value.to_owned())
}

pub(super) fn encoded_claude_directory(directory: &Path) -> Result<String, String> {
    let value = directory
        .to_str()
        .ok_or_else(|| "Working directory must be UTF-8".to_owned())?;
    Ok(value.replace(['/', '\\', ':'], "-"))
}

pub(super) fn transcript_root(home: &Path, provider: Provider) -> PathBuf {
    match provider {
        Provider::Claude => home.join(".claude/projects"),
        Provider::Copilot => home.join(".copilot/session-state"),
    }
}

pub(super) fn open_directory(root: &Path, name: &str) -> Result<File, String> {
    if name.is_empty() || name.contains(['/', '\\']) || matches!(name, "." | "..") {
        return Err("Invalid provider directory component".to_owned());
    }
    let root =
        File::open(root).map_err(|error| format!("Provider history unavailable: {error}"))?;
    let fd = openat(
        &root,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| format!("Provider history directory unavailable: {error}"))?;
    Ok(File::from(fd))
}

pub(super) fn open_file(directory: &File, name: &str) -> Result<File, String> {
    if name.is_empty() || name.contains(['/', '\\']) || matches!(name, "." | "..") {
        return Err("Invalid provider file component".to_owned());
    }
    let fd = openat(
        directory,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map_err(|error| format!("Provider file unavailable: {error}"))?;
    let file = File::from(fd);
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("Provider file must be a regular file".to_owned());
    }
    Ok(file)
}

pub(super) fn conversation_file(
    home: &Path,
    provider: Provider,
    directory: &Path,
    id: &str,
) -> Result<File, String> {
    let root = transcript_root(home, provider);
    match provider {
        Provider::Claude => {
            let project = open_directory(&root, &encoded_claude_directory(directory)?)?;
            let mut file = open_file(&project, &format!("{id}.jsonl"))?;
            let cwd = claude_directory(&mut file, MAX_IDENTITY_BYTES)?;
            if cwd.as_deref() != Some(directory) {
                return Err(
                    "Claude conversation does not belong to this working directory".to_owned(),
                );
            }
            file.rewind().map_err(|error| error.to_string())?;
            Ok(file)
        }
        Provider::Copilot => {
            let session = open_directory(&root, id)?;
            let stored = copilot_directory(home, id)?;
            if canonical_directory(&stored)? != directory {
                return Err(
                    "Copilot conversation does not belong to this working directory".to_owned(),
                );
            }
            open_file(&session, "events.jsonl")
        }
    }
}

pub(super) fn claude_directory(file: &mut File, maximum: u64) -> Result<Option<PathBuf>, String> {
    let mut bytes = Vec::new();
    file.take(maximum)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read conversation identity: {error}"))?;
    for line in bytes.split(|byte| *byte == b'\n').take(32) {
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(cwd) = value.get("cwd").and_then(serde_json::Value::as_str) {
            return canonical_directory(cwd).map(Some);
        }
    }
    Ok(None)
}

pub(super) fn copilot_database(home: &Path) -> Result<Connection, String> {
    let path = home.join(".copilot/session-store.db");
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|error| format!("Copilot index unavailable: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Copilot index must be a regular non-symlink file".to_owned());
    }
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_PRIVATE_CACHE,
    )
    .map_err(|error| format!("Could not open Copilot index: {error}"))?;
    connection
        .busy_timeout(std::time::Duration::from_millis(250))
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch("PRAGMA query_only = ON; PRAGMA trusted_schema = OFF;")
        .map_err(|error| error.to_string())?;
    let columns = connection
        .prepare("SELECT name FROM pragma_table_info('sessions')")
        .and_then(|mut statement| {
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(|error| format!("Could not inspect Copilot index: {error}"))?;
    if !["id", "cwd", "summary", "created_at", "updated_at"]
        .iter()
        .all(|required| columns.iter().any(|column| column == required))
    {
        return Err("Copilot index is missing required conversation columns".to_owned());
    }
    Ok(connection)
}

fn copilot_directory(home: &Path, id: &str) -> Result<String, String> {
    let connection = copilot_database(home)?;
    let directory: Option<String> = connection.query_row(
        "SELECT CASE WHEN length(CAST(cwd AS BLOB)) BETWEEN 1 AND 4096 THEN cwd ELSE NULL END FROM sessions WHERE id = ? LIMIT 1",
        [id], |row| row.get(0),
    ).optional().map_err(|error| format!("Could not read Copilot conversation index: {error}"))?.flatten();
    directory.ok_or_else(|| {
        "Copilot conversation is not indexed with a valid working directory".to_owned()
    })
}

pub(super) fn bounded(label: &str, value: usize, maximum: usize) -> Result<usize, String> {
    if value == 0 || value > maximum {
        return Err(format!("{label} must be between 1 and {maximum}"));
    }
    Ok(value)
}
