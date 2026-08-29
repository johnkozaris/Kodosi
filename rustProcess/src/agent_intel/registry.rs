use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use kodosi_domain::ids::SessionId;

use super::task::AgentIntelHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalIntelEnqueueOutcome {
    Enqueued,
    Unattached,
    Full,
    FullAlreadyReported,
    Closed,
}

#[derive(Debug, Default)]
pub(crate) struct AgentIntelRegistry {
    tasks: HashMap<SessionId, AgentIntelTask>,
}

#[derive(Debug, Default)]
struct AgentIntelTask {
    generation: Option<uuid::Uuid>,
    agent_session_id: Option<String>,
    terminal_tx: Option<mpsc::Sender<bytes::Bytes>>,
    cancellation: Option<CancellationToken>,
    join_handle: Option<tokio::task::JoinHandle<()>>,
    terminal_drop_reported: AtomicBool,
}

impl AgentIntelRegistry {
    pub(crate) fn reserve_generation(&mut self, id: SessionId) -> uuid::Uuid {
        let generation = uuid::Uuid::now_v7();
        self.tasks.entry(id).or_default().generation = Some(generation);
        generation
    }

    pub(crate) fn retire_generation(&mut self, id: SessionId, generation: uuid::Uuid) {
        if let Some(task) = self.tasks.get_mut(&id)
            && task.generation == Some(generation)
        {
            task.generation = None;
        }
    }

    pub(crate) fn is_current_generation(&self, id: SessionId, generation: uuid::Uuid) -> bool {
        self.tasks
            .get(&id)
            .is_some_and(|task| task.generation == Some(generation))
    }

    pub(crate) fn active(&self, id: SessionId) -> bool {
        self.tasks.get(&id).is_some_and(|task| {
            task.cancellation
                .as_ref()
                .is_some_and(|token| !token.is_cancelled())
        })
    }

    pub(crate) fn attach(&mut self, id: SessionId, handle: AgentIntelHandle) {
        let task = self.tasks.entry(id).or_default();
        if let Some(existing) = task.cancellation.take() {
            existing.cancel();
        }
        task.terminal_tx = Some(handle.terminal_tx);
        task.cancellation = Some(handle.cancellation);
        task.join_handle = Some(handle.join_handle);
    }

    pub(crate) fn detach_task(&mut self, id: SessionId) {
        let Some(task) = self.tasks.get_mut(&id) else {
            return;
        };
        task.drop_channels();
    }

    pub(crate) fn detach(&mut self, id: SessionId) {
        if let Some(mut task) = self.tasks.remove(&id) {
            task.teardown();
        }
    }

    pub(crate) fn prepare_for_restart(&mut self, id: SessionId) {
        let task = self.tasks.entry(id).or_default();
        task.teardown();
    }

    pub(crate) fn try_send_terminal(
        &self,
        id: SessionId,
        data: &bytes::Bytes,
    ) -> TerminalIntelEnqueueOutcome {
        let Some(task) = self.tasks.get(&id) else {
            return TerminalIntelEnqueueOutcome::Unattached;
        };
        let Some(tx) = task.terminal_tx.as_ref() else {
            return TerminalIntelEnqueueOutcome::Unattached;
        };
        match tx.try_send(data.clone()) {
            Ok(()) => {
                task.terminal_drop_reported.store(false, Ordering::Release);
                TerminalIntelEnqueueOutcome::Enqueued
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                if task.terminal_drop_reported.swap(true, Ordering::AcqRel) {
                    TerminalIntelEnqueueOutcome::FullAlreadyReported
                } else {
                    TerminalIntelEnqueueOutcome::Full
                }
            }
            Err(mpsc::error::TrySendError::Closed(_)) => TerminalIntelEnqueueOutcome::Closed,
        }
    }

    pub(crate) fn agent_session_id(&self, id: SessionId) -> Option<&str> {
        self.tasks.get(&id)?.agent_session_id.as_deref()
    }

    pub(crate) fn set_agent_session_id(
        &mut self,
        session_id: SessionId,
        agent_session_id: Option<&str>,
    ) {
        self.tasks.entry(session_id).or_default().agent_session_id =
            agent_session_id.map(str::to_owned);
    }

    pub(crate) fn reap_finished(&mut self) -> Vec<SessionId> {
        let finished = self
            .tasks
            .iter()
            .filter_map(|(id, task)| {
                task.join_handle
                    .as_ref()
                    .is_some_and(tokio::task::JoinHandle::is_finished)
                    .then_some(*id)
            })
            .collect::<Vec<_>>();

        let mut exited = Vec::new();
        for id in finished {
            if let Some(task) = self.tasks.get_mut(&id) {
                task.drop_channels();
                exited.push(id);
            }
        }
        exited
    }

    #[cfg(test)]
    pub(crate) fn attach_finished_for_test(&mut self, id: SessionId) {
        let (terminal_tx, _terminal_rx) = mpsc::channel(1);
        self.reserve_generation(id);
        self.attach(
            id,
            AgentIntelHandle {
                terminal_tx,
                cancellation: CancellationToken::new(),
                join_handle: tokio::spawn(async {}),
            },
        );
    }
}

impl AgentIntelTask {
    fn drop_channels(&mut self) {
        if let Some(cancellation) = self.cancellation.take() {
            cancellation.cancel();
        }
        self.terminal_tx = None;
        self.join_handle = None;
        self.generation = None;
    }

    fn teardown(&mut self) {
        self.drop_channels();
        self.agent_session_id = None;
    }
}

#[cfg(test)]
mod tests {
    use kodosi_domain::ids::SessionId;

    use super::AgentIntelRegistry;
    use crate::agent_intel::task::AgentIntelHandle;

    #[tokio::test]
    async fn finished_task_is_reported_once_for_restart() {
        let id = SessionId::new();
        let mut registry = AgentIntelRegistry::default();
        let (terminal_tx, _) = tokio::sync::mpsc::channel(1);
        let mut join_handle = tokio::spawn(async {});
        (&mut join_handle).await.expect("task completes");
        registry.attach(
            id,
            AgentIntelHandle {
                terminal_tx,
                cancellation: tokio_util::sync::CancellationToken::new(),
                join_handle,
            },
        );

        assert_eq!(registry.reap_finished(), vec![id]);
        assert!(!registry.active(id));
        assert!(registry.reap_finished().is_empty());
    }

    #[test]
    fn agent_session_id_is_cleared_for_restart_and_teardown() {
        let id = SessionId::new();
        let mut registry = AgentIntelRegistry::default();

        registry.set_agent_session_id(id, Some("native-session"));
        registry.prepare_for_restart(id);
        assert_eq!(registry.agent_session_id(id), None);

        registry.set_agent_session_id(id, Some("replacement-session"));
        registry.detach(id);
        assert_eq!(registry.agent_session_id(id), None);
    }
}
