use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use kodosi_domain::ids::SessionId;
use serde::{Deserialize, Serialize};

use crate::{
    AppError, Result,
    support::{
        platform::fs as support_fs,
        storage::atomic_file::{FileMode, atomic_write_json},
    },
};

const FILE_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HiddenSessionsFile {
    version: u32,
    users: BTreeMap<String, BTreeSet<String>>,
}

impl Default for HiddenSessionsFile {
    fn default() -> Self {
        Self {
            version: FILE_VERSION,
            users: BTreeMap::new(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct HiddenSessionStore {
    path: PathBuf,
}

impl HiddenSessionStore {
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn load_default() -> Result<Self> {
        Ok(Self {
            path: crate::support::storage::paths::hidden_sessions_path()?,
        })
    }

    pub(crate) fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub(crate) fn load(&self, user_id: &str) -> Result<BTreeSet<SessionId>> {
        load_file(&self.path)?
            .users
            .remove(user_id)
            .unwrap_or_default()
            .into_iter()
            .map(|id| {
                SessionId::parse_field(&id, "hiddenSessions.sessionId").map_err(|error| {
                    AppError::InvalidBackendData {
                        field: "hiddenSessions.sessionId".to_owned(),
                        reason: error.to_string(),
                    }
                })
            })
            .collect()
    }

    pub(crate) fn set_hidden(
        &self,
        user_id: &str,
        session_id: SessionId,
        hidden: bool,
    ) -> Result<()> {
        self.update(|file| {
            let sessions = file.users.entry(user_id.to_owned()).or_default();
            if hidden {
                sessions.insert(session_id.to_string());
            } else {
                sessions.remove(&session_id.to_string());
            }
            if sessions.is_empty() {
                file.users.remove(user_id);
            }
        })
    }

    pub(crate) fn clear_user(&self, user_id: &str) -> Result<()> {
        self.update(|file| {
            file.users.remove(user_id);
        })
    }

    fn update(&self, mutate: impl FnOnce(&mut HiddenSessionsFile)) -> Result<()> {
        let parent = self.path.parent().ok_or_else(|| AppError::Unsupported {
            reason: "hidden-session store path has no parent".to_owned(),
        })?;
        support_fs::ensure_dir(parent)?;
        let lock_path = parent.join("hidden-sessions.lock");
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)?;
        support_fs::set_file_permissions(&lock_path)?;
        lock.lock().map_err(AppError::Io)?;

        let mut file = load_file(&self.path)?;
        mutate(&mut file);
        atomic_write_json(&self.path, &file, true, FileMode::UserPrivate)
    }
}

fn load_file(path: &Path) -> Result<HiddenSessionsFile> {
    let payload = match fs::read_to_string(path) {
        Ok(payload) => payload,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(HiddenSessionsFile::default());
        }
        Err(error) => return Err(AppError::Io(error)),
    };
    let file: HiddenSessionsFile = serde_json::from_str(&payload)?;
    if file.version != FILE_VERSION {
        return Err(AppError::InvalidBackendData {
            field: "hiddenSessions.version".to_owned(),
            reason: format!("unsupported hidden-session file version {}", file.version),
        });
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_sessions_are_scoped_by_authenticated_user() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = HiddenSessionStore::at(dir.path().join("hidden-sessions.json"));
        let alice_session = SessionId::new();
        let bob_session = SessionId::new();

        store
            .set_hidden("alice", alice_session, true)
            .expect("hide for alice");
        store
            .set_hidden("bob", bob_session, true)
            .expect("hide for bob");
        assert_eq!(
            store.load("alice").expect("alice hidden sessions"),
            BTreeSet::from([alice_session])
        );
        assert_eq!(
            store.load("bob").expect("bob hidden sessions"),
            BTreeSet::from([bob_session])
        );

        store.clear_user("alice").expect("clear alice");
        assert!(store.load("alice").expect("alice cleared").is_empty());
        assert_eq!(
            store.load("bob").expect("bob retained"),
            BTreeSet::from([bob_session])
        );
    }
}
