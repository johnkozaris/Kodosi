use std::path::Path;

use tokio::sync::watch;

use super::saver::Saver;
use crate::terminal::TerminalLook;

/// A host without an app keeps the look that the app gave it last.
pub(super) struct HostLook {
    look: watch::Sender<TerminalLook>,
    saver: Saver,
}

impl HostLook {
    pub(super) fn load(data_root: &Path) -> Self {
        let file = data_root.join("terminal-look.json");
        let look = std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            look: watch::channel(look).0,
            saver: Saver::file(file, "the look of the terminals"),
        }
    }

    pub(super) fn now(&self) -> TerminalLook {
        *self.look.borrow()
    }

    pub(super) fn watch(&self) -> watch::Receiver<TerminalLook> {
        self.look.subscribe()
    }

    pub(super) fn change(&self, look: TerminalLook) -> bool {
        let changed = self
            .look
            .send_if_modified(|now| std::mem::replace(now, look) != look);
        if changed {
            match serde_json::to_vec(&look) {
                Ok(bytes) => self.saver.save(bytes),
                Err(error) => tracing::warn!(%error, "could not save the look of the terminals"),
            }
        }
        changed
    }

    pub(super) async fn saved(&self) {
        self.saver.saved().await;
    }
}
