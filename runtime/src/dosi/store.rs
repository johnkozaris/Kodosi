use std::{
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{DosiEntry, DosiLog, DosiRecord};
use crate::agents::{Agent, AgentChoice};

const STATE: &str = "state.json";
const LOG: &str = "log.jsonl";
const SHARED: &str = "shared";
const INSTRUCTIONS: &str = "instructions.md";
const MAX_LOG_BYTES: u64 = 8 * 1024 * 1024;
const COMPACT_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProviderSession {
    pub conversation: String,
    pub agent: Agent,
    pub account: String,
    pub session: Option<String>,
    pub seen: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Saved {
    pub setup: AgentChoice,
    pub sessions: Vec<ProviderSession>,
}

#[derive(Clone, Copy)]
pub(super) enum Book {
    Local,
    Shared(Uuid),
}

pub(super) struct Store {
    root: PathBuf,
}

impl Store {
    pub(super) fn open(data_root: &Path) -> Self {
        let root = data_root.join("dosi");
        for folder in [root.clone(), root.join(SHARED)] {
            if let Err(error) = kodosi_os::fs::create_private_dir(&folder) {
                tracing::warn!(%error, "Dosi's storage was not made");
            }
        }
        Self { root }
    }

    pub(super) fn load(&self) -> Saved {
        std::fs::read(self.root.join(STATE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub(super) fn save(&self, saved: &Saved) {
        let written = serde_json::to_vec(saved)
            .map_err(std::io::Error::from)
            .and_then(|bytes| kodosi_os::fs::write_private(&self.root.join(STATE), &bytes));
        if let Err(error) = written {
            tracing::warn!(%error, "Dosi's state was not saved");
        }
    }

    fn path(&self, book: Book) -> PathBuf {
        match book {
            Book::Local => self.root.join(LOG),
            Book::Shared(user) => self.root.join(SHARED).join(format!("{user}.jsonl")),
        }
    }

    pub(super) fn log(&self, book: Book) -> DosiLog {
        let mut text = String::new();
        let read = std::fs::File::open(self.path(book))
            .and_then(|file| file.take(MAX_LOG_BYTES).read_to_string(&mut text));
        if read.is_err() {
            return DosiLog::default();
        }
        let entries = text
            .lines()
            .filter_map(|line| serde_json::from_str::<DosiEntry>(line).ok())
            .collect::<Vec<_>>();
        DosiLog {
            sequence: entries.last().map_or(0, |entry| entry.sequence),
            entries,
            ..DosiLog::default()
        }
    }

    pub(super) fn append(&self, book: Book, entries: &[DosiEntry]) {
        if entries.is_empty() {
            return;
        }
        let path = self.path(book);
        let mut bytes = Vec::new();
        for entry in entries {
            if serde_json::to_writer(&mut bytes, entry).is_ok() {
                bytes.push(b'\n');
            }
        }
        let appended =
            kodosi_os::fs::append_private(&path).and_then(|mut file| file.write_all(&bytes));
        if let Err(error) = appended {
            tracing::warn!(%error, "Dosi's conversation was not saved");
        }
        if std::fs::metadata(&path).is_ok_and(|metadata| metadata.len() > COMPACT_BYTES) {
            self.compact(book);
        }
    }

    fn compact(&self, book: Book) {
        let log = self.log(book);
        let start = log
            .entries
            .iter()
            .rposition(|entry| matches!(entry.record, DosiRecord::Begin { .. }))
            .unwrap_or(0)
            .max(
                log.entries
                    .len()
                    .saturating_sub(super::conversation::MAX_ITEMS * 2),
            );
        let mut bytes = Vec::new();
        for entry in &log.entries[start..] {
            if serde_json::to_writer(&mut bytes, entry).is_ok() {
                bytes.push(b'\n');
            }
        }
        if let Err(error) = kodosi_os::fs::write_private(&self.path(book), &bytes) {
            tracing::warn!(%error, "Dosi's conversation was not compacted");
        }
    }

    pub(super) fn clear(&self, book: Book) {
        if let Err(error) = kodosi_os::fs::write_private(&self.path(book), &[]) {
            tracing::warn!(%error, "Dosi's saved conversation was not cleared");
        }
    }

    pub(super) fn instructions(&self) -> Option<String> {
        std::fs::read_to_string(self.root.join(INSTRUCTIONS))
            .ok()
            .filter(|text| !text.trim().is_empty())
    }

    pub(super) fn set_instructions(&self, text: Option<&str>) -> std::io::Result<()> {
        let path = self.root.join(INSTRUCTIONS);
        if let Some(text) = text {
            return kodosi_os::fs::write_private(&path, text.as_bytes());
        }
        match std::fs::remove_file(&path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }
}
