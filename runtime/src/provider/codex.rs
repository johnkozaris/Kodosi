use std::{
    collections::HashSet,
    io::{BufRead as _, BufReader, Read as _},
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde_json::Value;

use super::storage;

const SESSIONS: &str = "sessions";
const WRITER_LOCKS: &str = "thread-writer-locks";
const MAX_RECORD_BYTES: u64 = 1024 * 1024;

struct Thread {
    id: String,
    cwd: Option<String>,
    written: SystemTime,
}

/// The Codex conversation that the processes of the group `group` show, from the files that
/// they hold open.
pub(super) fn conversation(root: &Path, group: u32, folder: Option<&Path>) -> Option<String> {
    let root = kodosi_os::fs::canonicalize(root).ok()?;
    shown(
        &root,
        kodosi_pty::open_files(group).unwrap_or_default(),
        folder,
    )
}

/// Of the rollouts in `held`, the one written last. When `held` has none of the files of
/// Codex's threads, as on Windows or with Codex's shared server, the one live conversation
/// of `folder`.
pub(super) fn shown(root: &Path, held: Vec<PathBuf>, folder: Option<&Path>) -> Option<String> {
    let (sessions, locks) = (root.join(SESSIONS), root.join(WRITER_LOCKS));
    let held = held
        .into_iter()
        .filter(|file| file.starts_with(&sessions) || file.starts_with(&locks))
        .collect::<Vec<_>>();
    if held.is_empty() {
        return live_conversation(root, folder?);
    }
    held.iter()
        .filter_map(|file| thread(file))
        .max_by_key(|thread| thread.written)
        .map(|thread| thread.id)
}

/// The one conversation of `folder` that a process writes now, by the lock that Codex keeps
/// for each thread that a process writes.
fn live_conversation(root: &Path, folder: &Path) -> Option<String> {
    let folder = kodosi_os::fs::canonicalize(folder).ok()?;
    let live = std::fs::read_dir(root.join(WRITER_LOCKS))
        .ok()?
        .take(storage::MAX_DIRECTORY_ENTRIES)
        .filter_map(Result::ok)
        .filter_map(|lock| {
            storage::conversation_id(lock.file_name().to_str()?.strip_suffix(".lock")?).ok()
        })
        .collect::<HashSet<_>>();
    let mut shown = rollouts(&root.join(SESSIONS), live)
        .iter()
        .filter_map(|file| thread(file))
        .filter(|thread| {
            thread
                .cwd
                .as_deref()
                .and_then(|cwd| kodosi_os::fs::canonicalize(Path::new(cwd)).ok())
                .is_some_and(|cwd| cwd == folder)
        })
        .map(|thread| thread.id)
        .collect::<HashSet<_>>()
        .into_iter();
    let id = shown.next()?;
    shown.next().is_none().then_some(id)
}

/// The rollouts of the threads `ids`, from the newest day of `sessions` back.
fn rollouts(sessions: &Path, mut ids: HashSet<String>) -> Vec<PathBuf> {
    let days = folders(sessions)
        .into_iter()
        .flat_map(|year| folders(&year))
        .flat_map(|month| folders(&month));
    let mut found = Vec::new();
    for day in days {
        if ids.is_empty() {
            break;
        }
        let rollouts = std::fs::read_dir(day)
            .into_iter()
            .flatten()
            .take(storage::MAX_DIRECTORY_ENTRIES)
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| rollout_thread(path).is_some_and(|id| ids.contains(id)))
            .collect::<Vec<_>>();
        for rollout in rollouts {
            if let Some(id) = rollout_thread(&rollout) {
                ids.remove(id);
            }
            found.push(rollout);
        }
    }
    found
}

fn folders(parent: &Path) -> Vec<PathBuf> {
    let mut folders = std::fs::read_dir(parent)
        .into_iter()
        .flatten()
        .take(storage::MAX_DIRECTORY_ENTRIES)
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    folders.sort_unstable_by(|left, right| right.cmp(left));
    folders
}

/// The thread of a rollout `rollout-<time>-<thread>.jsonl`, which a revert names
/// `rollout-<time>-<thread>_<rollout>.jsonl`.
fn rollout_thread(path: &Path) -> Option<&str> {
    let name = path
        .file_name()?
        .to_str()?
        .strip_prefix("rollout-")?
        .strip_suffix(".jsonl")?;
    let ids = name.get(19..)?.strip_prefix('-')?;
    Some(ids.split_once('_').map_or(ids, |(thread, _)| thread))
}

/// A conversation of its own, not a sub-agent's, from the first record of its rollout.
fn thread(path: &Path) -> Option<Thread> {
    rollout_thread(path)?;
    let folder = kodosi_os::fs::open_dir(path.parent()?).ok()?;
    let file = storage::open_file(&folder, path.file_name()?.to_str()?).ok()?;
    let written = file.metadata().ok()?.modified().ok()?;
    let mut first = Vec::new();
    BufReader::new(file.take(MAX_RECORD_BYTES))
        .read_until(b'\n', &mut first)
        .ok()?;
    let record = serde_json::from_slice::<Value>(&first).ok()?;
    let meta = record
        .get("payload")
        .filter(|_| record.get("type").and_then(Value::as_str) == Some("session_meta"))?;
    if meta
        .get("parent_thread_id")
        .is_some_and(|parent| !parent.is_null())
        || meta.get("source").is_some_and(|source| !source.is_string())
    {
        return None;
    }
    Some(Thread {
        id: storage::conversation_id(meta.get("id")?.as_str()?).ok()?,
        cwd: meta.get("cwd").and_then(Value::as_str).map(str::to_owned),
        written,
    })
}
