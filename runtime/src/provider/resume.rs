use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use super::{Provider, storage};

const MAX_CLAUDE_STATE_BYTES: u64 = 256 * 1024;
const MAX_COPILOT_HEAD_BYTES: u64 = 64 * 1024;
const MAX_COPILOT_TAIL_BYTES: u64 = 1024 * 1024;

pub(super) fn ensure_inactive(
    state_root: &Path,
    provider: Provider,
    directory: &Path,
    id: &str,
    file: File,
) -> Result<(), String> {
    match provider {
        Provider::Claude => claude_inactive(state_root, directory, id),
        Provider::Copilot => {
            let session =
                storage::open_directory(&storage::transcript_root(state_root, provider), id)?;
            copilot_inactive(directory, file, &session)
        }
    }
}

fn claude_inactive(state_root: &Path, directory: &Path, id: &str) -> Result<(), String> {
    let root = state_root.join("sessions");
    let entries = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "Could not inspect Claude session activity: {error}"
            ));
        }
    };
    let root = kodosi_os::fs::open_dir(&root).map_err(|error| error.to_string())?;
    for (index, entry) in entries.enumerate() {
        if index >= storage::MAX_DIRECTORY_ENTRIES {
            return Err("Claude session activity exceeds its entry limit".to_owned());
        }
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        let Some(name) = name.to_str().filter(|name| is_record(name)) else {
            continue;
        };
        let value = claude_record(&root, name)?;
        if record_conversation(&value) != Some(id) {
            continue;
        }
        let cwd = value
            .get("cwd")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "Claude activity record has no working directory".to_owned())?;
        if storage::canonical_directory(cwd)? != directory {
            return Err("Claude activity record belongs to another working directory".to_owned());
        }
        let pid = value
            .get("pid")
            .and_then(serde_json::Value::as_u64)
            .and_then(|pid| u32::try_from(pid).ok())
            .filter(|pid| *pid != 0)
            .ok_or_else(|| "Claude conversation activity is unknown".to_owned())?;
        if kodosi_os::process::is_running(pid) {
            return Err(
                "Claude conversation may already be open; close it before resuming".to_owned(),
            );
        }
    }
    Ok(())
}

pub(super) fn claude_conversation(state_root: &Path, group: u32) -> Option<String> {
    let root = state_root.join("sessions");
    let entries = std::fs::read_dir(&root).ok()?;
    let directory = kodosi_os::fs::open_dir(&root).ok()?;
    entries
        .take(storage::MAX_DIRECTORY_ENTRIES)
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| is_record(name))
        .filter_map(|name| claude_record(&directory, &name).ok())
        .find_map(|record| {
            let pid = record
                .get("pid")
                .and_then(serde_json::Value::as_u64)
                .and_then(|pid| u32::try_from(pid).ok())
                .filter(|pid| *pid != 0)?;
            (pid == group || kodosi_pty::process_group(pid) == Some(group))
                .then(|| record_conversation(&record))
                .flatten()
                .and_then(|id| storage::conversation_id(id).ok())
        })
}

fn is_record(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

fn claude_record(root: &File, name: &str) -> Result<serde_json::Value, String> {
    let mut file = storage::open_file(root, name)?;
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_CLAUDE_STATE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_CLAUDE_STATE_BYTES {
        return Err("Claude activity record exceeds its byte limit".to_owned());
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("Could not read Claude session activity: {error}"))
}

fn record_conversation(record: &serde_json::Value) -> Option<&str> {
    record
        .get("sessionId")
        .or_else(|| record.get("session_id"))
        .and_then(serde_json::Value::as_str)
}

fn copilot_inactive(directory: &Path, mut file: File, session: &File) -> Result<(), String> {
    let mut head = Vec::new();
    (&mut file)
        .take(MAX_COPILOT_HEAD_BYTES)
        .read_to_end(&mut head)
        .map_err(|error| error.to_string())?;
    let cwd = head
        .split(|byte| *byte == b'\n')
        .take(50)
        .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
        .find(|value| {
            value.get("type").and_then(serde_json::Value::as_str) == Some("session.start")
        })
        .and_then(|value| {
            value
                .pointer("/data/context/cwd")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .or_else(|| copilot_workspace_directory(session))
        .ok_or_else(|| "Copilot conversation activity has no working directory".to_owned())?;
    if storage::canonical_directory(&cwd)? != directory {
        return Err("Copilot activity record belongs to another working directory".to_owned());
    }
    let length = file.metadata().map_err(|error| error.to_string())?.len();
    let start = length.saturating_sub(MAX_COPILOT_TAIL_BYTES);
    file.seek(SeekFrom::Start(start))
        .map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    file.take(MAX_COPILOT_TAIL_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    let aligned = if start == 0 {
        bytes.as_slice()
    } else if let Some(newline) = bytes.iter().position(|byte| *byte == b'\n') {
        &bytes[newline + 1..]
    } else {
        return Err("Copilot activity record exceeds its byte limit".to_owned());
    };
    let mut shutdown = false;
    for line in aligned.split(|byte| *byte == b'\n') {
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(line) else {
            shutdown = false;
            continue;
        };
        shutdown =
            value.get("type").and_then(serde_json::Value::as_str) == Some("session.shutdown");
    }
    if !shutdown {
        return Err("Copilot conversation may still be open; close it before resuming".to_owned());
    }
    Ok(())
}

fn copilot_workspace_directory(session: &File) -> Option<String> {
    let file = storage::open_file(session, "workspace.yaml").ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_COPILOT_HEAD_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_COPILOT_HEAD_BYTES {
        return None;
    }
    let text = std::str::from_utf8(&bytes).ok()?;
    text.lines().take(40).find_map(|line| {
        let cwd = line
            .trim()
            .strip_prefix("cwd:")?
            .trim()
            .trim_matches(['\"', '\'']);
        (!cwd.is_empty()).then(|| cwd.to_owned())
    })
}
