use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, PoisonError},
};

use document::Seq;
use serde::{Deserialize, Serialize};

use crate::journal::{private_folder, write_replacing};

const FILE: &str = "taken.json";
/// Evict a client's sequence entry after this many clean closes with no new changes from it.
const MOST_QUIET_CLOSES: u32 = 32;
const MOST_CLIENTS: usize = 256;
const MOST_DOCUMENTS: usize = 256;

/// `quiet` counts consecutive clean closes with no new batches from this client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Remembered {
    seq: Seq,
    quiet: u32,
}

pub(crate) fn remembered(
    before: &HashMap<String, Remembered>,
    seqs: HashMap<String, Seq>,
) -> HashMap<String, Remembered> {
    let mut clients: Vec<(String, Remembered)> = seqs
        .into_iter()
        .filter_map(|(client, seq)| {
            let quiet = before
                .get(&client)
                .filter(|old| old.seq == seq)
                .map_or(0, |old| old.quiet + 1);
            (quiet <= MOST_QUIET_CLOSES).then_some((client, Remembered { seq, quiet }))
        })
        .collect();
    clients.sort_unstable_by_key(|(_, remembered)| remembered.quiet);
    clients.truncate(MOST_CLIENTS);
    clients.into_iter().collect()
}

pub(crate) fn seqs_of(clients: &HashMap<String, Remembered>) -> HashMap<String, Seq> {
    clients
        .iter()
        .map(|(client, remembered)| (client.clone(), remembered.seq))
        .collect()
}

#[derive(Default, Serialize, Deserialize)]
struct Closed {
    /// Ordered from oldest to most recently closed (LRU eviction drains from the front).
    documents: Vec<ClosedDocument>,
}

#[derive(Serialize, Deserialize)]
struct ClosedDocument {
    path: PathBuf,
    clients: HashMap<String, Remembered>,
}

/// Persists the last accepted client sequence numbers across clean closes so reconnecting clients
/// can reconcile in-flight batches. Keeps the most recently closed documents in a user-only file.
pub(crate) struct Memory {
    folder: PathBuf,
    lock: Mutex<()>,
}

impl Memory {
    pub(crate) fn new(folder: PathBuf) -> Self {
        Self {
            folder,
            lock: Mutex::default(),
        }
    }

    pub(crate) fn recall(&self, path: &Path) -> HashMap<String, Remembered> {
        let _held = self.held();
        self.read()
            .documents
            .into_iter()
            .find(|closed| closed.path == path)
            .map_or_default(|closed| closed.clients)
    }

    /// Removes the closed-state record once a document opens; while open, the journal is the
    /// source of truth and stale data here would mislead recovery.
    pub(crate) fn forget(&self, path: &Path) -> io::Result<()> {
        let _held = self.held();
        let mut closed = self.read();
        let known = closed.documents.len();
        closed.documents.retain(|closed| closed.path != path);
        if closed.documents.len() < known {
            self.write(&closed)?;
        }
        Ok(())
    }

    pub(crate) fn keep(&self, path: &Path, clients: HashMap<String, Remembered>) -> io::Result<()> {
        let _held = self.held();
        let mut closed = self.read();
        let known = closed.documents.len();
        closed.documents.retain(|closed| closed.path != path);
        if clients.is_empty() && closed.documents.len() == known {
            return Ok(());
        }
        if !clients.is_empty() {
            closed.documents.push(ClosedDocument {
                path: path.to_owned(),
                clients,
            });
        }
        let forgotten = closed.documents.len().saturating_sub(MOST_DOCUMENTS);
        closed.documents.drain(..forgotten);
        self.write(&closed)
    }

    fn held(&self) -> MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn read(&self) -> Closed {
        std::fs::read(self.folder.join(FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn write(&self, closed: &Closed) -> io::Result<()> {
        let bytes = serde_json::to_vec(closed)?;
        private_folder(&self.folder)?;
        write_replacing(&self.folder.join(FILE), &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_documents_that_closed_last_are_remembered() {
        let state = tempfile::tempdir().unwrap();
        let memory = Memory::new(state.path().to_owned());
        let path = |number: usize| PathBuf::from(format!("/files/{number}.docx"));
        let ana = |seq: Seq| HashMap::from([("ana".to_owned(), Remembered { seq, quiet: 0 })]);
        for number in 0..=MOST_DOCUMENTS {
            memory.keep(&path(number), ana(1)).unwrap();
        }
        memory.keep(&path(1), ana(2)).unwrap();
        assert!(memory.recall(&path(0)).is_empty());
        assert_eq!(memory.recall(&path(1)), ana(2));
        memory.keep(&path(MOST_DOCUMENTS + 1), ana(1)).unwrap();
        assert_eq!(memory.recall(&path(1)), ana(2));
        assert!(memory.recall(&path(2)).is_empty());
        memory.forget(&path(1)).unwrap();
        assert!(memory.recall(&path(1)).is_empty());
    }
}
