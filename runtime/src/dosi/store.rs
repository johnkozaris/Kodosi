use std::{
    collections::VecDeque,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{DosiEntry, DosiLog, DosiRecord, conversation};
#[cfg(feature = "host")]
use crate::agents::{Agent, AgentChoice};

#[cfg(feature = "host")]
const STATE: &str = "state.json";
const LOG: &str = "log.jsonl";
const SHARED: &str = "shared";
#[cfg(feature = "host")]
const INSTRUCTIONS: &str = "instructions.md";
const MAX_LOG_BYTES: u64 = 8 * 1024 * 1024;
const COMPACT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_MARKS: usize = 1024;
const MAX_HELD_MARKS: usize = 4 * MAX_MARKS;
const MAX_MARKS_BYTES: u64 = 256 * 1024;

/// What this device keeps of the person's conversation when the server loses or sends again
/// its records: the newest conversation, the turns that started or stopped, the messages that
/// the person took back, and the answers that Dosi took here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Marks {
    floor: Option<String>,
    settled: Done,
    withdrawn: VecDeque<String>,
    answered: Done,
}

/// Time-ordered ids that this device is done with: the newest ones, and every id up to the
/// newest one that it let go, which stays below the id that still waits.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Done {
    kept: VecDeque<String>,
    through: Option<String>,
}

impl Done {
    fn has(&self, id: &str) -> bool {
        self.through.as_deref().is_some_and(|through| id <= through)
            || self.kept.iter().any(|kept| kept == id)
    }

    fn add(&mut self, id: &str) -> bool {
        if self.has(id) {
            return false;
        }
        self.kept.push_back(id.to_owned());
        true
    }

    fn trim(&mut self, waiting: Option<&str>) {
        let held = |kept: &str| waiting.is_some_and(|waiting| kept >= waiting);
        while self.kept.len() > MAX_MARKS {
            if self.kept.len() <= MAX_HELD_MARKS
                && self.kept.front().is_some_and(|front| held(front))
            {
                break;
            }
            let Some(oldest) = self.kept.pop_front() else {
                break;
            };
            let past = conversation::time_of(&oldest).is_some_and(|at| at <= conversation::now());
            if past
                && !held(&oldest)
                && self
                    .through
                    .as_deref()
                    .is_none_or(|through| oldest.as_str() > through)
            {
                self.through = Some(oldest);
            }
        }
    }
}

impl Marks {
    pub(super) fn floor(&self) -> Option<&str> {
        self.floor.as_deref()
    }

    pub(super) fn below(&self, conversation: &str) -> bool {
        self.floor
            .as_deref()
            .is_some_and(|floor| conversation < floor)
    }

    pub(super) fn raise(&mut self, conversation: &str) -> bool {
        if self
            .floor
            .as_deref()
            .is_some_and(|floor| floor >= conversation)
        {
            return false;
        }
        self.floor = Some(conversation.to_owned());
        true
    }

    pub(super) fn settled(&self, turn: &str) -> bool {
        self.settled.has(turn)
    }

    pub(super) fn settle(&mut self, turn: &str) -> bool {
        self.settled.add(turn)
    }

    pub(super) fn withdrawn(&self, turn: &str) -> bool {
        self.withdrawn.iter().any(|known| known == turn)
    }

    pub(super) fn withdraw(&mut self, turn: &str) -> bool {
        remember(&mut self.withdrawn, turn)
    }

    pub(super) fn answered(&self, item: &str) -> bool {
        self.answered.has(item)
    }

    pub(super) fn answer(&mut self, item: &str) -> bool {
        self.answered.add(item)
    }

    /// Lets the oldest ids go, but not past the turn of the message of `waiting`, which still
    /// waits here.
    pub(super) fn trim(&mut self, waiting: Option<&str>) {
        self.settled.trim(waiting);
        self.answered.trim(None);
        while self.withdrawn.len() > MAX_MARKS {
            self.withdrawn.pop_front();
        }
    }
}

fn remember(known: &mut VecDeque<String>, id: &str) -> bool {
    if known.iter().any(|kept| kept == id) {
        return false;
    }
    known.push_back(id.to_owned());
    true
}

#[cfg(feature = "host")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProviderSession {
    pub conversation: String,
    pub agent: Agent,
    pub account: String,
    pub session: Option<String>,
    pub seen: u64,
}

#[cfg(feature = "host")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Saved {
    pub setup: AgentChoice,
    pub sessions: Vec<ProviderSession>,
    /// Whether Dosi runs on this computer.
    pub on: bool,
}

#[cfg(feature = "host")]
impl Default for Saved {
    fn default() -> Self {
        Self {
            setup: AgentChoice::default(),
            sessions: Vec::new(),
            on: true,
        }
    }
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

    #[cfg(feature = "host")]
    pub(super) fn load(&self) -> Saved {
        std::fs::read(self.root.join(STATE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    #[cfg(feature = "host")]
    pub(super) fn save(&self, saved: &Saved) {
        let written = serde_json::to_vec(saved)
            .map_err(std::io::Error::from)
            .and_then(|bytes| kodosi_os::fs::write_private(&self.root.join(STATE), &bytes));
        if let Err(error) = written {
            tracing::warn!(%error, "Dosi's state was not saved");
        }
    }

    fn marks_path(&self, user: Uuid) -> PathBuf {
        self.root.join(SHARED).join(format!("{user}.marks.json"))
    }

    pub(super) fn marks(&self, user: Uuid) -> Marks {
        let mut bytes = Vec::new();
        std::fs::File::open(self.marks_path(user))
            .and_then(|file| file.take(MAX_MARKS_BYTES).read_to_end(&mut bytes))
            .ok()
            .and_then(|_| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub(super) fn set_marks(&self, user: Uuid, marks: &Marks) {
        let written = serde_json::to_vec(marks)
            .map_err(std::io::Error::from)
            .and_then(|bytes| kodosi_os::fs::write_private(&self.marks_path(user), &bytes));
        if let Err(error) = written {
            tracing::warn!(%error, "Dosi's marks of the conversation were not saved");
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
                    .saturating_sub(conversation::MAX_ITEMS * 2),
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

    #[cfg(feature = "host")]
    pub(super) fn instructions(&self) -> Option<String> {
        std::fs::read_to_string(self.root.join(INSTRUCTIONS))
            .ok()
            .filter(|text| !text.trim().is_empty())
    }

    #[cfg(feature = "host")]
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
