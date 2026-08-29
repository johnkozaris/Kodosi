use serde::{Deserialize, Serialize};

use kodosi_domain::ids::SessionId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "camelCase")]
pub(crate) enum ShelfItem {
    Owned(SessionId),
    Remote(SessionId),
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SessionShelf {
    visible_sessions: Vec<ShelfItem>,
    active_item: Option<ShelfItem>,
}

impl SessionShelf {
    pub(crate) fn visible_sessions(&self) -> &[ShelfItem] {
        &self.visible_sessions
    }

    pub(crate) fn active_session(&self) -> Option<ShelfItem> {
        self.active_item
            .filter(|item| self.visible_sessions.contains(item))
            .or_else(|| self.visible_sessions.first().copied())
    }

    pub(crate) fn activate(&mut self, item: ShelfItem) {
        if !self.visible_sessions.contains(&item) {
            self.visible_sessions.push(item);
        }
        self.active_item = Some(item);
    }

    pub(crate) fn sync_owned_sessions(&mut self, session_ids: &[SessionId]) {
        self.visible_sessions.retain(|item| match item {
            ShelfItem::Owned(id) => session_ids.contains(id),
            ShelfItem::Remote(_) => true,
        });
        self.active_item = self.active_session();
    }

    pub(crate) fn sync_remote_sessions(&mut self, session_ids: &[SessionId]) {
        self.visible_sessions.retain(|item| match item {
            ShelfItem::Owned(_) => true,
            ShelfItem::Remote(id) => session_ids.contains(id),
        });
        self.active_item = self.active_session();
    }
}
