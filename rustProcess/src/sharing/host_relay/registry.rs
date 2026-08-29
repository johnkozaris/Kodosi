use std::{collections::HashMap, time::Instant};

use kodosi_backend_client::relay::{
    HostRelayActionResultDelivery, HostRelayPendingPermissionsSnapshot,
    HostRelaySemanticReceiptDelivery,
};
use kodosi_domain::ids::SessionId;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::{AppError, Result};

const HOST_END_FLUSH_BUDGET: std::time::Duration = std::time::Duration::from_millis(500);

pub(crate) const NO_HOST_RELAY_GENERATION: u64 = 0;

#[derive(Debug, Default)]
pub(crate) struct HostRelayRegistry {
    tasks: HashMap<SessionId, HostRelayTask>,
    last_generation: u64,
}

#[derive(Debug, Default)]
struct HostRelayTask {
    cancellation: Option<CancellationToken>,
    pending_permissions_tx: Option<watch::Sender<Option<HostRelayPendingPermissionsSnapshot>>>,
    semantic_receipt_tx: Option<mpsc::Sender<HostRelaySemanticReceiptDelivery>>,
    action_result_tx: Option<mpsc::Sender<HostRelayActionResultDelivery>>,
    fence_completion_tx:
        Option<mpsc::Sender<kodosi_backend_client::relay::HostRelayFenceCompletion>>,
    join_handle: Option<tokio::task::JoinHandle<()>>,
    last_retry: Option<Instant>,
    generation: u64,
}

impl HostRelayRegistry {
    pub(crate) fn active(&self, id: SessionId) -> bool {
        self.tasks.get(&id).is_some_and(HostRelayTask::active)
    }

    pub(crate) fn allocate_generation(&mut self) -> Result<u64> {
        let generation =
            self.last_generation
                .checked_add(1)
                .ok_or_else(|| AppError::Unsupported {
                    reason: "host relay generation space exhausted".to_owned(),
                })?;
        self.last_generation = generation;
        Ok(generation)
    }

    pub(crate) fn claim_generation(&mut self, id: SessionId, generation: u64) -> Result<()> {
        if generation == NO_HOST_RELAY_GENERATION || generation > self.last_generation {
            return Err(AppError::Unsupported {
                reason: "host relay generation was not allocated by this registry".to_owned(),
            });
        }
        self.tasks.entry(id).or_default().generation = generation;
        Ok(())
    }

    pub(crate) fn generation(&self, id: SessionId) -> u64 {
        self.tasks
            .get(&id)
            .map_or(NO_HOST_RELAY_GENERATION, |task| task.generation)
    }

    pub(crate) fn is_current_generation(&self, id: SessionId, generation: u64) -> bool {
        generation != NO_HOST_RELAY_GENERATION && self.generation(id) == generation
    }

    pub(crate) fn attach(
        &mut self,
        id: SessionId,
        cancellation: CancellationToken,
        pending_permissions_tx: watch::Sender<Option<HostRelayPendingPermissionsSnapshot>>,
        semantic_receipt_tx: mpsc::Sender<HostRelaySemanticReceiptDelivery>,
        action_result_tx: mpsc::Sender<HostRelayActionResultDelivery>,
        fence_completion_tx: mpsc::Sender<kodosi_backend_client::relay::HostRelayFenceCompletion>,
        join_handle: tokio::task::JoinHandle<()>,
    ) {
        let task = self.tasks.entry(id).or_default();
        if let Some(existing) = task.cancellation.replace(cancellation) {
            existing.cancel();
        }
        task.pending_permissions_tx = Some(pending_permissions_tx);
        task.semantic_receipt_tx = Some(semantic_receipt_tx);
        task.action_result_tx = Some(action_result_tx);
        task.fence_completion_tx = Some(fence_completion_tx);
        if let Some(existing) = task.join_handle.replace(join_handle)
            && !existing.is_finished()
        {
            existing.abort();
        }
    }

    pub(crate) fn publish_pending_permissions(
        &self,
        id: SessionId,
        snapshot: HostRelayPendingPermissionsSnapshot,
    ) -> Result<()> {
        let sender = self
            .tasks
            .get(&id)
            .and_then(|task| task.pending_permissions_tx.as_ref())
            .ok_or_else(|| AppError::Unsupported {
                reason: format!("session {} host relay is not active", id.short()),
            })?;
        if sender.is_closed() {
            return Err(AppError::ChannelClosed {
                session: id.to_string(),
            });
        }
        sender.send_replace(Some(snapshot));
        Ok(())
    }

    pub(crate) fn send_fence_completion(&self, id: SessionId, fence_id: String) -> Result<()> {
        self.send(
            id,
            |task| task.fence_completion_tx.as_ref(),
            kodosi_backend_client::relay::HostRelayFenceCompletion { fence_id },
        )
    }

    pub(crate) fn send_action_result(
        &self,
        id: SessionId,
        delivery: HostRelayActionResultDelivery,
    ) -> Result<()> {
        self.send(id, |task| task.action_result_tx.as_ref(), delivery)
    }

    pub(crate) fn send_semantic_receipt(
        &self,
        id: SessionId,
        delivery: HostRelaySemanticReceiptDelivery,
    ) -> Result<()> {
        self.send(id, |task| task.semantic_receipt_tx.as_ref(), delivery)
    }

    fn send<T>(
        &self,
        id: SessionId,
        channel: impl FnOnce(&HostRelayTask) -> Option<&mpsc::Sender<T>>,
        value: T,
    ) -> Result<()> {
        let sender =
            self.tasks
                .get(&id)
                .and_then(channel)
                .ok_or_else(|| AppError::Unsupported {
                    reason: format!("session {} host relay is not active", id.short()),
                })?;
        sender.try_send(value).map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => AppError::ChannelFull {
                session: id.to_string(),
            },
            mpsc::error::TrySendError::Closed(_) => AppError::ChannelClosed {
                session: id.to_string(),
            },
        })
    }

    pub(crate) fn cancel(&mut self, id: SessionId) -> bool {
        self.tasks
            .get_mut(&id)
            .is_some_and(HostRelayTask::cancel_immediate)
    }

    pub(crate) fn cancel_all_immediate(&mut self) -> usize {
        self.tasks
            .values_mut()
            .map(HostRelayTask::cancel_immediate)
            .map(usize::from)
            .sum()
    }

    pub(crate) fn retire_for_immediate_restart(&mut self, id: SessionId) -> bool {
        let Some(task) = self.tasks.get_mut(&id) else {
            return false;
        };
        let retired = task.teardown();
        task.last_retry = None;
        retired
    }

    pub(crate) fn teardown(&mut self, id: SessionId) -> bool {
        self.tasks.get_mut(&id).is_some_and(HostRelayTask::teardown)
    }

    pub(crate) fn remove(&mut self, id: SessionId) -> bool {
        self.tasks
            .remove(&id)
            .is_some_and(|mut task| task.teardown())
    }

    pub(crate) fn is_finished(&self, id: SessionId) -> bool {
        self.tasks
            .get(&id)
            .and_then(|task| task.join_handle.as_ref())
            .is_some_and(tokio::task::JoinHandle::is_finished)
    }

    pub(crate) fn retry_due(
        &self,
        id: SessionId,
        now: Instant,
        cooldown: std::time::Duration,
    ) -> bool {
        self.tasks.get(&id).is_none_or(|task| {
            task.last_retry
                .is_none_or(|last_retry| now.duration_since(last_retry) >= cooldown)
        })
    }

    pub(crate) fn mark_retry(&mut self, id: SessionId, now: Instant) {
        self.tasks.entry(id).or_default().last_retry = Some(now);
    }
}

impl Drop for HostRelayRegistry {
    fn drop(&mut self) {
        self.cancel_all_immediate();
    }
}

impl HostRelayTask {
    fn active(&self) -> bool {
        self.cancellation
            .as_ref()
            .is_some_and(|token| !token.is_cancelled())
            && self
                .join_handle
                .as_ref()
                .is_some_and(|handle| !handle.is_finished())
    }

    fn cancel_immediate(&mut self) -> bool {
        self.generation = NO_HOST_RELAY_GENERATION;
        let cancelled_token = self.cancellation.take().is_some_and(|cancellation| {
            cancellation.cancel();
            true
        });
        self.clear_channels();
        let cancelled_task = self.join_handle.take().is_some_and(|handle| {
            if !handle.is_finished() {
                handle.abort();
            }
            true
        });
        cancelled_token || cancelled_task
    }

    fn teardown(&mut self) -> bool {
        self.generation = NO_HOST_RELAY_GENERATION;
        let cancelled_token = self.cancellation.take().is_some_and(|cancellation| {
            cancellation.cancel();
            true
        });
        self.clear_channels();
        let cancelled_task = self.join_handle.take().is_some_and(|handle| {
            if !handle.is_finished() {
                let abort_handle = handle.abort_handle();
                tokio::spawn(async move {
                    if tokio::time::timeout(HOST_END_FLUSH_BUDGET, handle)
                        .await
                        .is_err()
                    {
                        abort_handle.abort();
                    }
                });
            }
            true
        });
        cancelled_token || cancelled_task
    }

    fn clear_channels(&mut self) {
        self.pending_permissions_tx = None;
        self.semantic_receipt_tx = None;
        self.action_result_tx = None;
        self.fence_completion_tx = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn latest_pending_snapshot_replaces_prior_value() {
        let id = SessionId::new();
        let mut registry = HostRelayRegistry::default();
        let generation = registry.allocate_generation().expect("generation");
        registry.claim_generation(id, generation).expect("claim");
        let cancellation = CancellationToken::new();
        let (pending_tx, mut pending_rx) = watch::channel(None);
        registry.attach(
            id,
            cancellation,
            pending_tx,
            mpsc::channel(1).0,
            mpsc::channel(1).0,
            mpsc::channel(1).0,
            tokio::spawn(std::future::pending()),
        );
        for snapshot_generation in [1, 2] {
            registry
                .publish_pending_permissions(
                    id,
                    HostRelayPendingPermissionsSnapshot {
                        generation: snapshot_generation,
                        incarnation_id: uuid::Uuid::now_v7(),
                        plaintext: vec![u8::try_from(snapshot_generation).expect("small")],
                    },
                )
                .expect("publish");
        }
        pending_rx.changed().await.expect("changed");
        assert_eq!(
            pending_rx
                .borrow_and_update()
                .as_ref()
                .map(|snapshot| snapshot.generation),
            Some(2)
        );
    }

    #[test]
    fn relay_generations_are_never_reused() {
        let mut registry = HostRelayRegistry::default();
        let first = registry.allocate_generation().expect("first");
        let second = registry.allocate_generation().expect("second");
        assert!(second > first);
    }
}
