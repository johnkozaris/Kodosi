use std::{collections::HashMap, time::Duration};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::AppError;
use kodosi_backend_client::session_relay::{
    RemoteRelayMode, SessionRelayClientHandle, SessionRelayCommand,
};
use kodosi_domain::{ids::SessionId, lifecycle::ConnectionState, permissions::ShareScope};

const VIEWER_RELAY_TEARDOWN_BUDGET: Duration = Duration::from_millis(500);
const PENDING_PERMISSION_ACTION_CAPACITY: usize = 128;

pub(crate) const NO_SESSION_RELAY_GENERATION: u64 = 0;

#[derive(Debug, Default)]
pub(crate) struct SessionRelayRegistry {
    entries: HashMap<SessionId, SessionRelayEntry>,
    last_generation: u64,
}

#[derive(Debug, Default)]
struct SessionRelayEntry {
    task: Option<SessionRelayTask>,
    generation: u64,
}

#[derive(Debug)]
pub(crate) struct DetachedSessionRelayShutdown {
    session_id: SessionId,
    task: SessionRelayTask,
}

impl DetachedSessionRelayShutdown {
    pub(crate) async fn run(self) {
        self.task.teardown_gracefully(self.session_id).await;
    }
}

#[derive(Debug)]
struct SessionRelayTask {
    cancellation: CancellationToken,
    command_tx: mpsc::Sender<SessionRelayCommand>,
    share_scope: ShareScope,
    relay_mode: RemoteRelayMode,
    status: ConnectionState,
    pending_permission_actions: HashMap<String, String>,
    join_handle: tokio::task::JoinHandle<()>,
}

impl SessionRelayRegistry {
    pub(crate) fn ids(&self) -> Vec<SessionId> {
        self.entries
            .iter()
            .filter_map(|(id, entry)| entry.task.as_ref().map(|_| *id))
            .collect()
    }

    pub(crate) fn contains(&self, id: SessionId) -> bool {
        self.entries
            .get(&id)
            .is_some_and(|entry| entry.task.is_some())
    }

    pub(crate) fn matches_live(
        &self,
        id: SessionId,
        share_scope: ShareScope,
        relay_mode: RemoteRelayMode,
    ) -> bool {
        self.entries
            .get(&id)
            .and_then(|entry| entry.task.as_ref())
            .is_some_and(|task| {
                !task.cancellation.is_cancelled()
                    && !task.join_handle.is_finished()
                    && task.share_scope == share_scope
                    && task.relay_mode == relay_mode
            })
    }

    pub(crate) fn reserve_generation(&mut self, id: SessionId) -> Result<u64, AppError> {
        let generation =
            self.last_generation
                .checked_add(1)
                .ok_or_else(|| AppError::Unsupported {
                    reason: "session relay generation space exhausted".to_owned(),
                })?;
        self.last_generation = generation;
        self.entries.entry(id).or_default().generation = generation;
        Ok(generation)
    }

    pub(crate) fn generation(&self, id: SessionId) -> u64 {
        self.entries
            .get(&id)
            .map_or(NO_SESSION_RELAY_GENERATION, |entry| entry.generation)
    }

    pub(crate) fn is_current_generation(&self, id: SessionId, generation: u64) -> bool {
        generation != NO_SESSION_RELAY_GENERATION && self.generation(id) == generation
    }

    pub(crate) fn attach(
        &mut self,
        id: SessionId,
        handle: SessionRelayClientHandle,
        share_scope: ShareScope,
        relay_mode: RemoteRelayMode,
        status: ConnectionState,
    ) {
        let entry = self.entries.entry(id).or_default();
        debug_assert_ne!(entry.generation, NO_SESSION_RELAY_GENERATION);
        if let Some(task) = entry.task.replace(SessionRelayTask {
            cancellation: handle.cancellation,
            command_tx: handle.command_tx,
            share_scope,
            relay_mode,
            status,
            pending_permission_actions: HashMap::new(),
            join_handle: handle.join_handle,
        }) {
            task.cancel_immediate();
        }
    }

    pub(crate) fn take_for_shutdown(
        &mut self,
        id: SessionId,
    ) -> Option<DetachedSessionRelayShutdown> {
        let mut entry = self.entries.remove(&id)?;
        entry.generation = NO_SESSION_RELAY_GENERATION;
        entry.task.take().map(|task| DetachedSessionRelayShutdown {
            session_id: id,
            task,
        })
    }

    #[tracing::instrument(skip_all, fields(session_id = %id))]
    pub(crate) async fn remove_gracefully(&mut self, id: SessionId) -> bool {
        let Some(shutdown) = self.take_for_shutdown(id) else {
            return false;
        };
        shutdown.run().await;
        true
    }

    pub(crate) fn remove_detached(&mut self, id: SessionId, generation: u64) -> bool {
        if !self.is_current_generation(id, generation) {
            return false;
        }
        self.entries
            .remove(&id)
            .is_some_and(|entry| entry.task.is_some())
    }

    pub(crate) fn cancel_immediate(&mut self, id: SessionId) -> bool {
        self.entries
            .remove(&id)
            .and_then(|entry| entry.task)
            .is_some_and(SessionRelayTask::cancel_immediate)
    }

    pub(crate) fn cancel_all_immediate(&mut self) {
        for (_, entry) in self.entries.drain() {
            if let Some(task) = entry.task {
                task.cancel_immediate();
            }
        }
    }

    pub(crate) fn set_status(
        &mut self,
        id: SessionId,
        generation: u64,
        status: ConnectionState,
    ) -> bool {
        if !self.is_current_generation(id, generation) {
            return false;
        }
        let Some(task) = self
            .entries
            .get_mut(&id)
            .and_then(|entry| entry.task.as_mut())
        else {
            return false;
        };

        task.status = status;
        true
    }

    pub(crate) fn any_status(&self, status: ConnectionState) -> bool {
        self.entries.values().any(|entry| {
            entry
                .task
                .as_ref()
                .is_some_and(|task| task.status == status)
        })
    }

    pub(crate) fn status(&self, id: SessionId) -> Option<ConnectionState> {
        self.entries.get(&id)?.task.as_ref().map(|task| task.status)
    }

    pub(crate) fn relay_mode(&self, id: SessionId) -> Option<RemoteRelayMode> {
        self.entries
            .get(&id)?
            .task
            .as_ref()
            .map(|task| task.relay_mode)
    }

    #[tracing::instrument(skip_all, fields(session_id = %id))]
    pub(crate) fn try_send(
        &self,
        id: SessionId,
        command: SessionRelayCommand,
    ) -> Option<Result<(), mpsc::error::TrySendError<SessionRelayCommand>>> {
        self.entries
            .get(&id)?
            .task
            .as_ref()
            .map(|task| task.command_tx.try_send(command))
    }

    pub(crate) fn try_send_permission_decision(
        &mut self,
        id: SessionId,
        action_id: &str,
        tool_use_id: &str,
        command: SessionRelayCommand,
    ) -> Option<Result<(), mpsc::error::TrySendError<SessionRelayCommand>>> {
        let task = self.entries.get_mut(&id)?.task.as_mut()?;
        if task.pending_permission_actions.contains_key(action_id) {
            return Some(Ok(()));
        }
        if task.pending_permission_actions.len() >= PENDING_PERMISSION_ACTION_CAPACITY {
            return Some(Err(mpsc::error::TrySendError::Full(command)));
        }
        match task.command_tx.try_send(command) {
            Ok(()) => {
                task.pending_permission_actions
                    .insert(action_id.to_owned(), tool_use_id.to_owned());
                Some(Ok(()))
            }
            Err(error) => Some(Err(error)),
        }
    }

    pub(crate) fn remove_permission_action(&mut self, id: SessionId, action_id: &str) {
        if let Some(task) = self
            .entries
            .get_mut(&id)
            .and_then(|entry| entry.task.as_mut())
        {
            task.pending_permission_actions.remove(action_id);
        }
    }

    pub(crate) fn take_permission_tool_use_id(
        &mut self,
        id: SessionId,
        action_id: &str,
    ) -> Option<String> {
        self.entries
            .get_mut(&id)?
            .task
            .as_mut()?
            .pending_permission_actions
            .remove(action_id)
    }

    #[cfg(test)]
    pub(crate) fn attach_for_test(
        &mut self,
        id: SessionId,
        cancellation: CancellationToken,
        command_tx: mpsc::Sender<SessionRelayCommand>,
        share_scope: ShareScope,
        relay_mode: RemoteRelayMode,
        status: ConnectionState,
        join_handle: tokio::task::JoinHandle<()>,
    ) -> u64 {
        let generation = self
            .reserve_generation(id)
            .expect("test relay generation space should not be exhausted");
        self.attach(
            id,
            SessionRelayClientHandle {
                cancellation,
                command_tx,
                join_handle,
            },
            share_scope,
            relay_mode,
            status,
        );
        generation
    }
}

impl Drop for SessionRelayRegistry {
    fn drop(&mut self) {
        self.cancel_all_immediate();
    }
}

impl SessionRelayTask {
    fn cancel_immediate(self) -> bool {
        self.cancellation.cancel();
        if !self.join_handle.is_finished() {
            self.join_handle.abort();
        }
        true
    }

    async fn teardown_gracefully(self, id: SessionId) {
        self.cancellation.cancel();
        if self.join_handle.is_finished() {
            return;
        }

        let abort_handle = self.join_handle.abort_handle();
        if tokio::time::timeout(VIEWER_RELAY_TEARDOWN_BUDGET, self.join_handle)
            .await
            .is_err()
        {
            abort_handle.abort();
            tracing::warn!(
                session_id = %id,
                "viewer relay did not exit within 500ms; aborted (KEM zeroization may be skipped)"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{NO_SESSION_RELAY_GENERATION, SessionRelayRegistry};
    use kodosi_backend_client::session_relay::{RemoteRelayMode, SessionRelayCommand};
    use kodosi_domain::{ids::SessionId, lifecycle::ConnectionState, permissions::ShareScope};
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    fn pending_task() -> tokio::task::JoinHandle<()> {
        tokio::spawn(async {
            std::future::pending::<()>().await;
        })
    }

    #[test]
    fn generations_are_process_wide_and_never_reused() {
        let mut registry = SessionRelayRegistry::default();
        let first = SessionId::new();
        let second = SessionId::new();

        let first_generation = registry
            .reserve_generation(first)
            .expect("first generation");
        let second_generation = registry
            .reserve_generation(second)
            .expect("second generation");
        let first_again = registry
            .reserve_generation(first)
            .expect("replacement generation");

        assert!(second_generation > first_generation);
        assert!(first_again > second_generation);
        assert_eq!(registry.generation(first), first_again);
        assert_eq!(registry.generation(second), second_generation);
    }

    #[test]
    fn generation_exhaustion_fails_without_reusing_or_claiming_a_generation() {
        let id = SessionId::new();
        let mut registry = SessionRelayRegistry::default();
        registry.last_generation = u64::MAX;

        let error = registry
            .reserve_generation(id)
            .expect_err("exhausted generation space must fail closed");

        std::assert_matches!(
            error,
            crate::AppError::Unsupported { reason }
                if reason == "session relay generation space exhausted"
        );
        assert_eq!(registry.last_generation, u64::MAX);
        assert_eq!(registry.generation(id), NO_SESSION_RELAY_GENERATION);
    }

    #[tokio::test]
    async fn dropping_registry_cancels_and_aborts_owned_relay() {
        let id = SessionId::new();
        let cancellation = CancellationToken::new();
        let (command_tx, _command_rx) = mpsc::channel(1);
        let task = pending_task();
        let abort = task.abort_handle();
        let mut registry = SessionRelayRegistry::default();
        registry.attach_for_test(
            id,
            cancellation.clone(),
            command_tx,
            ShareScope::Friends,
            RemoteRelayMode::SharedParticipant,
            ConnectionState::Connected,
            task,
        );

        drop(registry);

        assert!(cancellation.is_cancelled());
        tokio::task::yield_now().await;
        assert!(abort.is_finished());
    }

    #[tokio::test]
    async fn remove_gracefully_cancels_and_removes_wedged_relay() {
        let id = SessionId::new();
        let cancellation = CancellationToken::new();
        let (command_tx, _command_rx) = mpsc::channel(1);
        let mut registry = SessionRelayRegistry::default();
        registry.attach_for_test(
            id,
            cancellation.clone(),
            command_tx,
            ShareScope::Friends,
            RemoteRelayMode::SharedParticipant,
            ConnectionState::Connected,
            pending_task(),
        );

        assert!(registry.remove_gracefully(id).await);

        assert!(cancellation.is_cancelled());
        assert!(!registry.contains(id));
        assert_eq!(registry.generation(id), NO_SESSION_RELAY_GENERATION);
    }

    #[tokio::test]
    async fn old_exit_cannot_remove_replacement() {
        let id = SessionId::new();
        let (first_tx, _first_rx) = mpsc::channel(1);
        let (replacement_tx, _replacement_rx) = mpsc::channel(1);
        let mut registry = SessionRelayRegistry::default();
        let first_generation = registry.attach_for_test(
            id,
            CancellationToken::new(),
            first_tx,
            ShareScope::Friends,
            RemoteRelayMode::SharedParticipant,
            ConnectionState::Connected,
            pending_task(),
        );
        let replacement_generation = registry.attach_for_test(
            id,
            CancellationToken::new(),
            replacement_tx,
            ShareScope::Friends,
            RemoteRelayMode::SharedParticipant,
            ConnectionState::Connecting,
            pending_task(),
        );

        assert!(!registry.remove_detached(id, first_generation));
        assert!(registry.contains(id));
        assert_eq!(registry.generation(id), replacement_generation);
        assert_eq!(registry.status(id), Some(ConnectionState::Connecting));
    }

    #[tokio::test]
    async fn current_exit_removes_relay_without_cancelling_natural_exit() {
        let id = SessionId::new();
        let cancellation = CancellationToken::new();
        let (command_tx, _command_rx) = mpsc::channel(1);
        let mut registry = SessionRelayRegistry::default();
        let generation = registry.attach_for_test(
            id,
            cancellation.clone(),
            command_tx,
            ShareScope::Friends,
            RemoteRelayMode::SharedParticipant,
            ConnectionState::Connected,
            pending_task(),
        );

        assert!(registry.remove_detached(id, generation));

        assert!(!cancellation.is_cancelled());
        assert!(!registry.contains(id));
    }

    #[tokio::test]
    async fn try_send_reports_full_and_closed_channels() {
        let id = SessionId::new();
        let cancellation = CancellationToken::new();
        let (command_tx, command_rx) = mpsc::channel(1);
        let mut registry = SessionRelayRegistry::default();
        registry.attach_for_test(
            id,
            cancellation,
            command_tx,
            ShareScope::Friends,
            RemoteRelayMode::SharedParticipant,
            ConnectionState::Connected,
            tokio::spawn(async {}),
        );

        assert!(
            registry
                .try_send(
                    id,
                    SessionRelayCommand::Suggest {
                        action_id: "one".to_owned(),
                        body: "first".to_owned(),
                    },
                )
                .is_some_and(|result| result.is_ok())
        );
        std::assert_matches!(
            registry.try_send(
                id,
                SessionRelayCommand::Suggest {
                    action_id: "two".to_owned(),
                    body: "second".to_owned(),
                },
            ),
            Some(Err(mpsc::error::TrySendError::Full(_)))
        );

        drop(command_rx);
        let other_id = SessionId::new();
        let (closed_tx, closed_rx) = mpsc::channel(1);
        drop(closed_rx);
        registry.attach_for_test(
            other_id,
            CancellationToken::new(),
            closed_tx,
            ShareScope::Friends,
            RemoteRelayMode::SharedParticipant,
            ConnectionState::Connected,
            tokio::spawn(async {}),
        );
        std::assert_matches!(
            registry.try_send(
                other_id,
                SessionRelayCommand::Suggest {
                    action_id: "closed".to_owned(),
                    body: "closed".to_owned(),
                },
            ),
            Some(Err(mpsc::error::TrySendError::Closed(_)))
        );
    }

    #[tokio::test]
    async fn permission_action_result_correlates_back_to_tool_use_id() {
        let id = SessionId::new();
        let (command_tx, mut command_rx) = mpsc::channel(1);
        let mut registry = SessionRelayRegistry::default();
        registry.attach_for_test(
            id,
            CancellationToken::new(),
            command_tx,
            ShareScope::Friends,
            RemoteRelayMode::SharedParticipant,
            ConnectionState::Connected,
            pending_task(),
        );
        let action_id = "opaque-action";

        std::assert_matches!(
            registry.try_send_permission_decision(
                id,
                action_id,
                "tool-use-42",
                SessionRelayCommand::PermissionDecision {
                    action_id: action_id.to_owned(),
                    request_id: "tool-use-42".to_owned(),
                    request_generation: 7,
                    decision: "allow".to_owned(),
                },
            ),
            Some(Ok(()))
        );
        std::assert_matches!(
            command_rx.recv().await,
            Some(SessionRelayCommand::PermissionDecision { action_id, .. })
                if action_id == "opaque-action"
        );
        std::assert_matches!(
            registry.try_send_permission_decision(
                id,
                action_id,
                "tool-use-42",
                SessionRelayCommand::PermissionDecision {
                    action_id: action_id.to_owned(),
                    request_id: "tool-use-42".to_owned(),
                    request_generation: 7,
                    decision: "allow".to_owned(),
                },
            ),
            Some(Ok(()))
        );
        assert!(command_rx.try_recv().is_err());
        assert_eq!(
            registry.take_permission_tool_use_id(id, action_id),
            Some("tool-use-42".to_owned())
        );
        assert!(
            registry
                .take_permission_tool_use_id(id, action_id)
                .is_none()
        );
    }
}
