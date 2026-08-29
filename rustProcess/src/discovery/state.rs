use std::collections::{BTreeMap, BTreeSet};

use kodosi_domain::{
    ids::SessionId,
    lifecycle::{ConnectionState, RemoteSessionAccessIssue, RemoteSessionAccessState},
    session::{SessionState, SessionSummary},
};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub(crate) struct RemoteSessionRecord {
    pub(crate) summary: SessionSummary,
    pub(crate) incarnation_id: Option<Uuid>,
    pub(crate) room_id: Option<String>,
    pub(crate) connection_state: Option<ConnectionState>,
    pub(crate) connection_reason: Option<String>,
    pub(crate) access_state: Option<RemoteSessionAccessState>,
    pub(crate) access_reason: Option<String>,
    pub(crate) access_issue: Option<RemoteSessionAccessIssue>,
    pub(crate) viewer_blocked: bool,

    pub(crate) viewer_hidden: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DiscoveryPreservation {
    pub(crate) all: bool,
    pub(crate) owned: bool,
    pub(crate) rooms: bool,
    pub(crate) room_ids: BTreeSet<String>,
}

impl DiscoveryPreservation {
    pub(crate) fn is_empty(&self) -> bool {
        !self.all && !self.owned && !self.rooms && self.room_ids.is_empty()
    }

    fn retains(&self, record: &RemoteSessionRecord) -> bool {
        self.all
            || (self.owned && record.summary.role == kodosi_domain::session::SessionRole::Owner)
            || (self.rooms && record.room_id.is_some())
            || record
                .room_id
                .as_ref()
                .is_some_and(|room_id| self.room_ids.contains(room_id))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RetiredRemoteIncarnation {
    pub(crate) session_id: SessionId,
    pub(crate) incarnation_id: Uuid,
}

#[derive(Debug, Clone)]
pub(crate) struct DiscoveryState {
    remote_sessions: Vec<RemoteSessionRecord>,
    hidden_session_ids: BTreeSet<SessionId>,
}

impl DiscoveryState {
    pub(crate) fn empty() -> Self {
        Self {
            remote_sessions: Vec::new(),
            hidden_session_ids: BTreeSet::new(),
        }
    }

    pub(crate) fn drain_all(&mut self) {
        self.remote_sessions.clear();
        self.hidden_session_ids.clear();
    }

    pub(crate) fn set_hidden_session_ids(&mut self, ids: BTreeSet<SessionId>) {
        self.hidden_session_ids = ids;
        for record in &mut self.remote_sessions {
            record.viewer_hidden = self.hidden_session_ids.contains(&record.summary.id);
        }
    }

    pub(crate) fn replace_remote_sessions(
        &mut self,
        remote_sessions: Vec<RemoteSessionRecord>,
        preserve_missing: bool,
    ) -> Vec<RetiredRemoteIncarnation> {
        let preservation = if preserve_missing {
            DiscoveryPreservation {
                all: true,
                ..DiscoveryPreservation::default()
            }
        } else {
            DiscoveryPreservation::default()
        };
        self.replace_remote_sessions_with_policy(remote_sessions, &preservation)
    }

    pub(crate) fn replace_remote_sessions_with_policy(
        &mut self,
        remote_sessions: Vec<RemoteSessionRecord>,
        preservation: &DiscoveryPreservation,
    ) -> Vec<RetiredRemoteIncarnation> {
        let mut existing_by_id = self
            .remote_sessions
            .drain(..)
            .map(|record| (record.summary.id, record))
            .collect::<BTreeMap<_, _>>();
        let mut retired_incarnations = Vec::new();

        self.remote_sessions = remote_sessions
            .into_iter()
            .map(|mut record| {
                if let Some(existing) = existing_by_id.remove(&record.summary.id) {
                    let incarnation_changed = existing.incarnation_id.is_some()
                        && record.incarnation_id.is_some()
                        && existing.incarnation_id != record.incarnation_id;
                    if let (true, Some(incarnation_id)) =
                        (incarnation_changed, existing.incarnation_id)
                    {
                        retired_incarnations.push(RetiredRemoteIncarnation {
                            session_id: record.summary.id,
                            incarnation_id,
                        });
                    } else {
                        record.summary.pending_suggestion_count =
                            existing.summary.pending_suggestion_count;
                        record.summary.state = SessionState::merge_local_with_incoming(
                            existing.summary.state,
                            record.summary.state,
                        );
                        if !existing.viewer_blocked {
                            if let Some(state) = existing.connection_state {
                                record.connection_state = Some(state);
                            }
                            if let Some(reason) = existing.connection_reason {
                                record.connection_reason = Some(reason);
                            }
                            if let Some(state) = existing.access_state {
                                record.access_state = Some(state);
                            }
                            if let Some(reason) = existing.access_reason {
                                record.access_reason = Some(reason);
                            }
                            if let Some(issue) = existing.access_issue {
                                record.access_issue = Some(issue);
                            }
                        }
                    }
                }
                record.viewer_hidden = self.hidden_session_ids.contains(&record.summary.id);
                record
            })
            .collect();

        if !preservation.is_empty() {
            self.remote_sessions.extend(
                existing_by_id
                    .into_values()
                    .filter(|record| preservation.retains(record)),
            );
        }
        retired_incarnations
    }

    pub(crate) fn remote_session_ids(&self) -> Vec<SessionId> {
        self.remote_sessions
            .iter()
            .map(|session| session.summary.id)
            .collect()
    }

    pub(crate) fn non_hidden_remote_session_ids(&self) -> Vec<SessionId> {
        self.remote_sessions
            .iter()
            .filter(|session| !session.viewer_hidden)
            .map(|session| session.summary.id)
            .collect()
    }

    pub(crate) fn hidden_sessions(&self) -> impl Iterator<Item = &RemoteSessionRecord> {
        self.remote_sessions
            .iter()
            .filter(|session| session.viewer_hidden)
    }

    pub(crate) fn hide_session(&mut self, id: SessionId) -> bool {
        let Some(record) = self.session_mut(id) else {
            return false;
        };
        if record.viewer_hidden {
            return false;
        }
        record.viewer_hidden = true;
        self.hidden_session_ids.insert(id);
        true
    }

    pub(crate) fn unhide_session(&mut self, id: SessionId) -> bool {
        let Some(record) = self.session_mut(id) else {
            return false;
        };
        if !record.viewer_hidden {
            return false;
        }
        record.viewer_hidden = false;
        self.hidden_session_ids.remove(&id);
        true
    }

    pub(crate) fn remove_session(&mut self, id: SessionId) -> bool {
        let before = self.remote_sessions.len();
        self.remote_sessions
            .retain(|record| record.summary.id != id);
        self.hidden_session_ids.remove(&id);
        self.remote_sessions.len() != before
    }

    pub(crate) fn session(&self, id: SessionId) -> Option<&RemoteSessionRecord> {
        self.remote_sessions
            .iter()
            .find(|session| session.summary.id == id)
    }

    pub(crate) fn session_mut(&mut self, id: SessionId) -> Option<&mut RemoteSessionRecord> {
        self.remote_sessions
            .iter_mut()
            .find(|session| session.summary.id == id)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use kodosi_domain::{
        ids::SessionId,
        permissions::{AccessLevel, ShareScope},
        session::{SessionRole, SessionSummary},
        terminal::TerminalSize,
    };

    use super::{DiscoveryPreservation, DiscoveryState, RemoteSessionRecord};

    fn record(role: SessionRole, room_id: Option<&str>) -> RemoteSessionRecord {
        let mut summary = SessionSummary::new_remote(
            SessionId::new(),
            "Session".to_owned(),
            "Owner".to_owned(),
            None,
            room_id.map_or(ShareScope::Friends, |_| ShareScope::Room),
            AccessLevel::View,
            TerminalSize::default(),
        );
        summary.role = role;
        RemoteSessionRecord {
            summary,
            incarnation_id: None,
            room_id: room_id.map(str::to_owned),
            connection_state: None,
            connection_reason: None,
            access_state: None,
            access_reason: None,
            access_issue: None,
            viewer_blocked: false,
            viewer_hidden: false,
        }
    }

    #[test]
    fn partial_room_source_preserves_only_that_rooms_missing_records() {
        let owner = record(SessionRole::Owner, None);
        let room_one = record(SessionRole::Viewer, Some("room-1"));
        let room_two = record(SessionRole::Viewer, Some("room-2"));
        let room_one_id = room_one.summary.id;
        let mut state = DiscoveryState::empty();
        state.replace_remote_sessions(vec![owner, room_one, room_two], false);

        state.replace_remote_sessions_with_policy(
            Vec::new(),
            &DiscoveryPreservation {
                room_ids: BTreeSet::from(["room-1".to_owned()]),
                ..DiscoveryPreservation::default()
            },
        );

        assert_eq!(state.remote_session_ids(), vec![room_one_id]);
    }

    #[test]
    fn failed_own_source_does_not_preserve_unrelated_viewer_rows() {
        let owner = record(SessionRole::Owner, None);
        let owner_id = owner.summary.id;
        let viewer = record(SessionRole::Viewer, Some("room"));
        let mut state = DiscoveryState::empty();
        state.replace_remote_sessions(vec![owner, viewer], false);

        state.replace_remote_sessions_with_policy(
            Vec::new(),
            &DiscoveryPreservation {
                owned: true,
                ..DiscoveryPreservation::default()
            },
        );

        assert_eq!(state.remote_session_ids(), vec![owner_id]);
    }
}
