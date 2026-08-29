use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
};

use uuid::Uuid;

use bytes::Bytes;
use kodosi_domain::ids::SessionId;
use tokio::sync::mpsc;

use super::{
    TerminalCapability, TerminalCloseReason, TerminalConnectionId, TerminalControlFrame,
    TerminalDataFrame, TerminalSurface,
};

const DATA_CHANNEL_CAPACITY: usize = 256;

const CONTROL_CHANNEL_CAPACITY: usize = 16;
const CLOSE_CONTROL_RESERVE: usize = 1;

const TOMBSTONE_TTL: std::time::Duration = std::time::Duration::from_mins(1);

#[derive(Debug)]
struct ConnectionEntry {
    bootstrap_pending: bool,
    data_tx: mpsc::Sender<TerminalDataFrame>,
    control_tx: mpsc::Sender<TerminalControlFrame>,
}

impl ConnectionEntry {
    fn try_send_control(&mut self, frame: TerminalControlFrame) -> bool {
        if self.bootstrap_pending {
            if !matches!(frame, TerminalControlFrame::SemanticCheckpoint { .. }) {
                return true;
            }
            let delivered = self.control_tx.capacity() > CLOSE_CONTROL_RESERVE
                && self.control_tx.try_send(frame).is_ok();
            if delivered {
                self.bootstrap_pending = false;
            }
            return delivered;
        }
        self.control_tx.capacity() > CLOSE_CONTROL_RESERVE
            && self.control_tx.try_send(frame).is_ok()
    }

    fn try_send_close(&self, frame: TerminalControlFrame) -> bool {
        self.control_tx.try_send(frame).is_ok()
    }
}

#[derive(Debug)]
enum Lifecycle {
    Live,
    Ended {
        at: std::time::Instant,
        reason: TerminalCloseReason,
    },
}

#[derive(Debug)]
struct SessionState {
    lifecycle: Lifecycle,
    local_incarnation_id: Option<Uuid>,
    next_sequence: u64,
    connections: HashMap<TerminalConnectionId, ConnectionEntry>,
}

impl SessionState {
    fn new() -> Self {
        Self {
            lifecycle: Lifecycle::Live,
            local_incarnation_id: None,
            next_sequence: 0,
            connections: HashMap::new(),
        }
    }

    const fn ended_reason(&self) -> Option<&TerminalCloseReason> {
        match &self.lifecycle {
            Lifecycle::Live => None,
            Lifecycle::Ended { reason, .. } => Some(reason),
        }
    }

    fn take_seq(&mut self) -> Option<u64> {
        let seq = self.next_sequence;
        self.next_sequence = self.next_sequence.checked_add(1)?;
        Some(seq)
    }
}

pub struct SubscriberHandle {
    pub connection_id: TerminalConnectionId,
    pub runtime_incarnation_id: Option<Uuid>,
    pub surface: TerminalSurface,
    pub capability: TerminalCapability,
    pub data_rx: mpsc::Receiver<TerminalDataFrame>,
    pub control_rx: mpsc::Receiver<TerminalControlFrame>,
}

#[derive(Clone, Debug)]
pub struct SessionHub {
    sessions: Arc<Mutex<HashMap<SessionId, SessionState>>>,
}

impl SessionHub {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<SessionId, SessionState>> {
        self.sessions.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("terminal hub lock was poisoned; recovering");
            poisoned.into_inner()
        })
    }

    pub fn register(
        &mut self,
        session_id: SessionId,
        surface: TerminalSurface,
        capability: TerminalCapability,
    ) -> Option<SubscriberHandle> {
        self.register_with_bootstrap_state(session_id, surface, capability, false)
    }

    pub fn register_bootstrap_pending(
        &mut self,
        session_id: SessionId,
        surface: TerminalSurface,
        capability: TerminalCapability,
    ) -> Option<SubscriberHandle> {
        self.register_with_bootstrap_state(session_id, surface, capability, true)
    }

    fn register_with_bootstrap_state(
        &self,
        session_id: SessionId,
        surface: TerminalSurface,
        capability: TerminalCapability,
        bootstrap_pending: bool,
    ) -> Option<SubscriberHandle> {
        let connection_id = TerminalConnectionId::new();
        let (data_tx, data_rx) = mpsc::channel(DATA_CHANNEL_CAPACITY);
        let (control_tx, control_rx) =
            mpsc::channel(CONTROL_CHANNEL_CAPACITY + CLOSE_CONTROL_RESERVE);

        let mut sessions = self.lock();
        Self::expire_tombstones(&mut sessions);
        let state = sessions.get_mut(&session_id)?;
        if state.ended_reason().is_some() {
            drop(sessions);
            return None;
        }
        state.connections.insert(
            connection_id,
            ConnectionEntry {
                bootstrap_pending,
                data_tx,
                control_tx,
            },
        );
        drop(sessions);

        Some(SubscriberHandle {
            connection_id,
            runtime_incarnation_id: None,
            surface,
            capability,
            data_rx,
            control_rx,
        })
    }

    pub fn unregister(&mut self, session_id: SessionId, connection_id: TerminalConnectionId) {
        if let Some(state) = self.lock().get_mut(&session_id) {
            state.connections.remove(&connection_id);
        }
    }

    fn publish_live_state(
        session_id: SessionId,
        state: &mut SessionState,
        bytes: Bytes,
    ) -> Option<u64> {
        let Some(seq) = state.take_seq() else {
            let reason = TerminalCloseReason::IoError("terminal sequence exhausted".to_owned());
            Self::end_live_state(state, &reason);
            tracing::error!(%session_id, "terminal sequence exhausted; session closed");
            return None;
        };
        let frame = TerminalDataFrame::new(seq, bytes);
        let mut failed = Vec::new();
        for (connection_id, entry) in &state.connections {
            if !entry.bootstrap_pending && entry.data_tx.try_send(frame.clone()).is_err() {
                failed.push(*connection_id);
            }
        }
        for connection_id in failed {
            tracing::warn!(
                %session_id,
                connection = ?connection_id,
                "terminal hub evicted lagging subscriber"
            );
            state.connections.remove(&connection_id);
        }
        Some(seq)
    }

    fn end_live_state(state: &mut SessionState, reason: &TerminalCloseReason) {
        state.lifecycle = Lifecycle::Ended {
            at: std::time::Instant::now(),
            reason: reason.clone(),
        };
        let final_sequence = state.next_sequence;
        for (_, entry) in std::mem::take(&mut state.connections) {
            let _ = entry.try_send_close(TerminalControlFrame::Closed {
                reason: reason.clone(),
                final_sequence,
            });
        }
    }

    pub fn publish(&mut self, session_id: SessionId, bytes: Bytes) -> Option<u64> {
        let mut sessions = self.lock();
        Self::expire_tombstones(&mut sessions);
        let state = sessions.get_mut(&session_id)?;
        if state.ended_reason().is_some() {
            drop(sessions);
            tracing::debug!(%session_id, "dropped terminal publish for an ended session");
            return None;
        }
        Self::publish_live_state(session_id, state, bytes)
    }

    pub fn publish_at_sequence(
        &mut self,
        session_id: SessionId,
        sequence: u64,
        bytes: Bytes,
    ) -> bool {
        let mut sessions = self.lock();
        Self::expire_tombstones(&mut sessions);
        let Some(state) = sessions.get_mut(&session_id) else {
            return false;
        };
        if state.ended_reason().is_some() {
            return false;
        }
        if sequence < state.next_sequence {
            return true;
        }
        if sequence != state.next_sequence {
            tracing::error!(
                %session_id,
                expected = state.next_sequence,
                actual = sequence,
                "remote terminal sequence gap"
            );
            return false;
        }
        let Some(next_sequence) = sequence.checked_add(1) else {
            return false;
        };
        state.next_sequence = next_sequence;
        let frame = TerminalDataFrame::new(sequence, bytes);
        let mut failed = Vec::new();
        for (connection_id, entry) in &state.connections {
            if !entry.bootstrap_pending && entry.data_tx.try_send(frame.clone()).is_err() {
                failed.push(*connection_id);
            }
        }
        for connection_id in failed {
            state.connections.remove(&connection_id);
        }
        drop(sessions);
        true
    }

    pub fn send_control(
        &mut self,
        session_id: SessionId,
        connection_id: TerminalConnectionId,
        frame: TerminalControlFrame,
    ) -> bool {
        let mut sessions = self.lock();
        let Some(state) = sessions.get_mut(&session_id) else {
            return false;
        };
        let Some(entry) = state.connections.get_mut(&connection_id) else {
            return false;
        };
        let delivered = if entry.try_send_control(frame) {
            true
        } else {
            state.connections.remove(&connection_id);
            false
        };
        drop(sessions);
        delivered
    }

    pub fn broadcast_control(&mut self, session_id: SessionId, frame: &TerminalControlFrame) {
        let mut sessions = self.lock();
        let Some(state) = sessions.get_mut(&session_id) else {
            return;
        };
        let mut failed: Vec<TerminalConnectionId> = Vec::new();
        for (conn_id, entry) in &mut state.connections {
            if !entry.try_send_control(frame.clone()) {
                failed.push(*conn_id);
            }
        }
        for conn_id in failed {
            state.connections.remove(&conn_id);
        }
        drop(sessions);
    }

    #[must_use]
    pub fn next_sequence(&self, session_id: SessionId) -> Option<u64> {
        let sessions = self.lock();
        let next = sessions.get(&session_id).and_then(|state| {
            if state.ended_reason().is_some() {
                None
            } else {
                Some(state.next_sequence)
            }
        });
        drop(sessions);
        next
    }

    #[must_use]
    pub fn connection_count(&self, session_id: SessionId) -> usize {
        self.lock()
            .get(&session_id)
            .map_or(0, |s| s.connections.len())
    }

    #[must_use]
    pub fn has_connection(
        &self,
        session_id: SessionId,
        connection_id: TerminalConnectionId,
    ) -> bool {
        self.lock()
            .get(&session_id)
            .is_some_and(|state| state.connections.contains_key(&connection_id))
    }

    pub fn end_session(&mut self, session_id: SessionId, reason: &TerminalCloseReason) {
        let mut sessions = self.lock();
        Self::expire_tombstones(&mut sessions);
        let Some(state) = sessions.get_mut(&session_id) else {
            sessions.insert(
                session_id,
                SessionState {
                    lifecycle: Lifecycle::Ended {
                        at: std::time::Instant::now(),
                        reason: reason.clone(),
                    },
                    local_incarnation_id: None,
                    next_sequence: 0,
                    connections: HashMap::new(),
                },
            );
            drop(sessions);
            return;
        };
        if state.ended_reason().is_some() {
            drop(sessions);
            return;
        }
        Self::end_live_state(state, reason);
        drop(sessions);
    }

    pub fn install_semantic_checkpoint(
        &mut self,
        session_id: SessionId,
        checkpoint: kodosi_domain::terminal::TerminalCheckpointV2,
        next_sequence: u64,
    ) -> bool {
        let mut sessions = self.lock();
        Self::expire_tombstones(&mut sessions);
        let state = sessions.entry(session_id).or_insert_with(SessionState::new);
        if next_sequence < state.next_sequence {
            drop(sessions);
            return false;
        }
        if state.ended_reason().is_some() {
            state.lifecycle = Lifecycle::Live;
            state.local_incarnation_id = None;
            state.connections.clear();
        }
        state.next_sequence = next_sequence;
        let frame = TerminalControlFrame::SemanticCheckpoint {
            checkpoint,
            next_sequence,
        };
        let mut failed = Vec::new();
        for (connection_id, entry) in &mut state.connections {
            if !entry.try_send_control(frame.clone()) {
                failed.push(*connection_id);
            }
        }
        for connection_id in failed {
            state.connections.remove(&connection_id);
        }
        drop(sessions);
        true
    }

    fn open_at_sequence(
        &self,
        session_id: SessionId,
        local_incarnation_id: Option<Uuid>,
        next_sequence: u64,
    ) -> bool {
        let mut sessions = self.lock();
        Self::expire_tombstones(&mut sessions);
        match sessions.entry(session_id) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(SessionState {
                    lifecycle: Lifecycle::Live,
                    local_incarnation_id,
                    next_sequence,
                    connections: HashMap::new(),
                });
                true
            }
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                let state = entry.get_mut();
                if state.next_sequence != next_sequence {
                    return false;
                }
                if state.ended_reason().is_some() {
                    state.lifecycle = Lifecycle::Live;
                    state.local_incarnation_id = local_incarnation_id;
                    state.connections.clear();
                    return true;
                }
                match (local_incarnation_id, state.local_incarnation_id) {
                    (Some(requested), Some(current)) => requested == current,
                    (Some(requested), None) => {
                        state.local_incarnation_id = Some(requested);
                        true
                    }
                    (None, _) => true,
                }
            }
        }
    }

    pub(crate) fn open_local_incarnation(
        &self,
        session_id: SessionId,
        local_incarnation_id: Uuid,
        next_sequence: u64,
    ) -> bool {
        self.open_at_sequence(session_id, Some(local_incarnation_id), next_sequence)
    }

    pub(crate) fn publish_local(
        &self,
        origin: crate::session_runtime::events::LocalCoordinatorOrigin,
        bytes: Bytes,
    ) -> Option<u64> {
        let mut sessions = self.lock();
        Self::expire_tombstones(&mut sessions);
        let state = sessions.get_mut(&origin.session_id)?;
        if state.ended_reason().is_some()
            || state.local_incarnation_id != Some(origin.local_incarnation_id)
        {
            return None;
        }
        let sequence = Self::publish_live_state(origin.session_id, state, bytes);
        drop(sessions);
        sequence
    }

    pub(crate) fn end_local_incarnation(
        &self,
        origin: crate::session_runtime::events::LocalCoordinatorOrigin,
        reason: &TerminalCloseReason,
    ) -> bool {
        let mut sessions = self.lock();
        Self::expire_tombstones(&mut sessions);
        let Some(state) = sessions.get_mut(&origin.session_id) else {
            return false;
        };
        if state.ended_reason().is_some()
            || state.local_incarnation_id != Some(origin.local_incarnation_id)
        {
            return false;
        }
        Self::end_live_state(state, reason);
        drop(sessions);
        true
    }

    fn expire_tombstones(sessions: &mut HashMap<SessionId, SessionState>) {
        let now = std::time::Instant::now();
        sessions.retain(|_, state| match &state.lifecycle {
            Lifecycle::Live => true,
            Lifecycle::Ended { at, .. } => now.duration_since(*at) < TOMBSTONE_TTL,
        });
    }

    #[must_use]
    pub fn close_reason(&self, session_id: SessionId) -> Option<TerminalCloseReason> {
        self.lock().get(&session_id)?.ended_reason().cloned()
    }
}

impl Default for SessionHub {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
