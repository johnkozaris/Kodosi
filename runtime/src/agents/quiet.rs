use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    fs::{File, OpenOptions, TryLockError},
    io::{self, Read as _},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{MapAccess, Visitor},
};
use serde_json::{Value, value::RawValue};
use uuid::Uuid;

use super::{Agent, Output};

const FILE: &str = "quiet-settings.json";
const SETTINGS: &str = "settings.json";
/// Copilot CLI writes its settings only while it holds a lock on this file.
const SETTINGS_LOCK: &str = "settings.json.lock";
const MOST_BYTES: usize = 1024 * 1024;
const LOCK_WAIT: Duration = Duration::from_secs(5);
const LOCK_RETRY: Duration = Duration::from_millis(50);

/// Copilot CLI has no flag or variable for these, and reads them only from its saved settings.
fn quiet_values() -> [(&'static str, Value); 4] {
    [
        ("showReasoning", Value::Bool(false)),
        ("showTimestamps", Value::Bool(false)),
        ("showToolDurations", Value::Bool(false)),
        ("banner", Value::from("never")),
    ]
}

pub(super) fn settings_file(agent: Agent, output: Output, folder: &Path) -> Option<PathBuf> {
    (agent == Agent::Copilot && output == Output::Quiet && folder.is_absolute())
        .then(|| folder.join(SETTINGS))
}

pub(crate) struct QuietSettings {
    file: PathBuf,
    records: Mutex<BTreeMap<PathBuf, Record>>,
    writing: Mutex<()>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    terminals: BTreeSet<Uuid>,
    keys: BTreeMap<String, Kept>,
    #[serde(default)]
    created: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Kept {
    original: Option<Value>,
    written: Value,
}

impl Record {
    fn join(&mut self, terminal: Uuid) -> Option<Self> {
        self.terminals.insert(terminal).then(|| self.clone())
    }
}

impl QuietSettings {
    /// The terminals of an earlier host ended with it, so none of them holds its settings now.
    pub(super) fn load(data_root: &Path) -> Self {
        let file = data_root.join(FILE);
        let records = std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let quiet = Self {
            file,
            records: Mutex::new(records),
            writing: Mutex::new(()),
        };
        quiet.release_all();
        quiet
    }

    pub(crate) fn held_by(&self, terminal: Uuid) -> Option<PathBuf> {
        self.records()
            .iter()
            .find(|(_, record)| record.terminals.contains(&terminal))
            .map(|(settings, _)| settings.clone())
    }

    pub(super) fn hold(&self, terminal: Uuid, settings: &Path) -> bool {
        let _writing = lock(&self.writing);
        let joined = self
            .records()
            .entry(settings.to_owned())
            .or_default()
            .join(terminal);
        let Some(mut record) = joined else {
            return false;
        };
        if let Err(error) = apply(settings, &mut record) {
            tracing::warn!(%error, settings = %settings.display(), "Copilot CLI starts without its quiet settings");
        }
        if let Some(held) = self.records().get_mut(settings) {
            held.keys = record.keys;
            held.created = record.created;
        }
        self.save();
        true
    }

    pub(super) fn release(&self, terminal: Uuid) {
        self.release_where(|_, terminals| terminals.remove(&terminal));
    }

    pub(super) fn release_hold(&self, terminal: Uuid, settings: &Path) {
        self.release_where(|held, terminals| held == settings && terminals.remove(&terminal));
    }

    pub(super) fn release_all(&self) {
        self.release_where(|_, terminals| {
            terminals.clear();
            true
        });
    }

    fn release_where(&self, mut leaves: impl FnMut(&Path, &mut BTreeSet<Uuid>) -> bool) {
        let _writing = lock(&self.writing);
        let mut changed = false;
        let emptied = self
            .records()
            .iter_mut()
            .filter_map(|(settings, record)| {
                let left = leaves(settings, &mut record.terminals);
                changed |= left;
                (left && record.terminals.is_empty()).then(|| (settings.clone(), record.clone()))
            })
            .collect::<Vec<_>>();
        if !changed {
            return;
        }
        for (settings, record) in emptied {
            match restore(&settings, &record) {
                Ok(()) => {
                    self.records().remove(&settings);
                }
                Err(error) => {
                    tracing::warn!(%error, settings = %settings.display(), "the settings of Copilot CLI stay quiet until Kodosi starts again");
                }
            }
        }
        self.save();
    }

    fn records(&self) -> MutexGuard<'_, BTreeMap<PathBuf, Record>> {
        lock(&self.records)
    }

    fn save(&self) {
        let bytes = serde_json::to_vec(&*self.records());
        let saved = bytes
            .map_err(io::Error::from)
            .and_then(|bytes| kodosi_os::fs::write_private(&self.file, &bytes));
        if let Err(error) = saved {
            tracing::warn!(%error, "what Kodosi changed in the settings of Copilot CLI was not saved");
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn apply(settings: &Path, record: &mut Record) -> io::Result<()> {
    let _lock = lock_settings(settings)?;
    let (mut members, created) =
        read(settings)?.map_or_else(|| (Members::default(), true), |members| (members, false));
    let mut kept = BTreeMap::new();
    for (key, quiet) in quiet_values() {
        let current = members.get(key);
        if current.as_ref() != Some(&quiet) {
            members.set(key, &quiet)?;
            kept.insert(
                key.to_owned(),
                Kept {
                    original: current,
                    written: quiet,
                },
            );
        }
    }
    if !kept.is_empty() {
        write(settings, &members)?;
        record.keys.extend(kept);
        record.created |= created;
    }
    Ok(())
}

/// A key that no longer has what Kodosi wrote was changed by the person, so it stays.
fn restore(settings: &Path, record: &Record) -> io::Result<()> {
    if record.keys.is_empty() {
        return Ok(());
    }
    let _lock = match lock_settings(settings) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        lock => lock?,
    };
    let mut members = match read(settings) {
        Ok(Some(members)) => members,
        Ok(None) => return Ok(()),
        Err(error) if error.kind() == io::ErrorKind::InvalidData => return Ok(()),
        Err(error) => return Err(error),
    };
    let mut changed = false;
    for (key, kept) in &record.keys {
        if members.get(key).as_ref() == Some(&kept.written) {
            match &kept.original {
                Some(original) => members.set(key, original)?,
                None => members.remove(key),
            }
            changed = true;
        }
    }
    if !changed {
        Ok(())
    } else if record.created && members.0.is_empty() {
        std::fs::remove_file(settings)
    } else {
        write(settings, &members)
    }
}

fn lock_settings(settings: &Path) -> io::Result<File> {
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(settings.with_file_name(SETTINGS_LOCK))?;
    let asked = Instant::now();
    loop {
        match lock.try_lock() {
            Ok(()) => return Ok(lock),
            Err(TryLockError::WouldBlock) if asked.elapsed() < LOCK_WAIT => {
                std::thread::sleep(LOCK_RETRY);
            }
            Err(TryLockError::WouldBlock) => return Err(io::ErrorKind::WouldBlock.into()),
            Err(TryLockError::Error(error)) => return Err(error),
        }
    }
}

fn read(settings: &Path) -> io::Result<Option<Members>> {
    match std::fs::symlink_metadata(settings) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Err(invalid("the settings are not a file")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    }
    let mut bytes = Vec::new();
    File::open(settings)?
        .take(MOST_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MOST_BYTES {
        return Err(invalid("the settings are too long"));
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| invalid(error.to_string()))
}

fn write(settings: &Path, members: &Members) -> io::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(members)?;
    bytes.push(b'\n');
    kodosi_os::fs::write_private(settings, &bytes)
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The settings in their own order, with each value as it was written, so that a change of a
/// few keys leaves the rest of the file as it was.
#[derive(Default)]
struct Members(Vec<(String, Box<RawValue>)>);

impl Members {
    fn get(&self, key: &str) -> Option<Value> {
        let (_, value) = self.0.iter().rev().find(|(name, _)| name == key)?;
        serde_json::from_str(value.get()).ok()
    }

    fn set(&mut self, key: &str, value: &Value) -> serde_json::Result<()> {
        let value = serde_json::value::to_raw_value(value)?;
        let mut found = false;
        for (_, held) in self.0.iter_mut().filter(|(name, _)| name == key) {
            held.clone_from(&value);
            found = true;
        }
        if !found {
            self.0.push((key.to_owned(), value));
        }
        Ok(())
    }

    fn remove(&mut self, key: &str) {
        self.0.retain(|(name, _)| name != key);
    }
}

impl<'de> Deserialize<'de> for Members {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Object;

        impl<'de> Visitor<'de> for Object {
            type Value = Members;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Members, A::Error> {
                let mut members = Vec::new();
                while let Some(member) = map.next_entry()? {
                    members.push(member);
                }
                Ok(Members(members))
            }
        }

        deserializer.deserialize_map(Object)
    }
}

impl Serialize for Members {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_map(self.0.iter().map(|(key, value)| (key, value)))
    }
}

#[cfg(test)]
#[path = "quiet_tests.rs"]
mod tests;
