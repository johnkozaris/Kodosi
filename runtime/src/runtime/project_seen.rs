use std::{collections::BTreeMap, path::Path};

use super::saver::Saver;

const MOST_BYTES: usize = 1024 * 1024;

/// All marks stay in memory, so a save reads nothing.
pub(super) struct ProjectSeen {
    accounts: BTreeMap<String, BTreeMap<String, u64>>,
    saver: Saver,
}

impl ProjectSeen {
    pub(super) fn load(data_root: &Path) -> Self {
        let file = data_root.join("network").join("room-seen.json");
        let accounts = std::fs::read(&file)
            .ok()
            .filter(|bytes| bytes.len() <= MOST_BYTES)
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            accounts,
            saver: Saver::file(file, "which project messages were seen"),
        }
    }

    pub(super) fn of(&mut self, user: String) -> &mut BTreeMap<String, u64> {
        self.accounts.entry(user).or_default()
    }

    pub(super) fn save(&self) {
        match serde_json::to_vec(&self.accounts) {
            Ok(bytes) => self.saver.save(bytes),
            Err(error) => tracing::warn!(%error, "could not save which project messages were seen"),
        }
    }

    pub(super) async fn saved(&self) {
        self.saver.saved().await;
    }
}
