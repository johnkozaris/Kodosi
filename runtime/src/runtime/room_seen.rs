use std::{collections::BTreeMap, path::Path};

use super::saver::Saver;

const MOST_BYTES: usize = 1024 * 1024;

/// Which room messages each account of this device saw. All of them stay in memory, so a save
/// reads nothing, and the saver writes them away from the runtime's task.
pub(super) struct RoomSeen {
    accounts: BTreeMap<String, BTreeMap<String, u64>>,
    saver: Saver,
}

impl RoomSeen {
    pub(super) fn load(data_root: &Path) -> Self {
        let file = data_root.join("network").join("room-seen.json");
        let accounts = std::fs::read(&file)
            .ok()
            .filter(|bytes| bytes.len() <= MOST_BYTES)
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            accounts,
            saver: Saver::file(file, "which room messages were seen"),
        }
    }

    /// The newest message that an account saw in each room.
    pub(super) fn of(&mut self, user: String) -> &mut BTreeMap<String, u64> {
        self.accounts.entry(user).or_default()
    }

    pub(super) fn save(&self) {
        match serde_json::to_vec(&self.accounts) {
            Ok(bytes) => self.saver.save(bytes),
            Err(error) => tracing::warn!(%error, "could not save which room messages were seen"),
        }
    }

    /// Returns when the newest marks are on disk.
    pub(super) async fn saved(&self) {
        self.saver.saved().await;
    }
}
