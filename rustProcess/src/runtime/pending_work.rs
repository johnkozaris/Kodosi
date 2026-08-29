use std::{
    collections::{HashSet, VecDeque},
    time::{Duration, Instant},
};

use kodosi_domain::{ids::SessionId, permissions::ShareScope, user::IdentityLifecycleState};
use uuid::Uuid;

use crate::sharing::scope::SelectedRoom;

const PENDING_PIN_WORK_CAPACITY: usize = 256;
const PIN_REFRESH_RETRY_BASE: Duration = Duration::from_secs(1);
const PIN_REFRESH_RETRY_MAX: Duration = Duration::from_mins(1);
const READY_DELETE_RETRY_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackendScopeRestore {
    pub(crate) session_id: SessionId,
    pub(crate) backend_session_id: String,
    pub(crate) incarnation_id: Uuid,
    pub(crate) scope: ShareScope,
    pub(crate) room: Option<SelectedRoom>,
    pub(crate) requires_key_rotation: bool,
}

#[derive(Debug, Default)]
pub(crate) struct PendingWorkQueue {
    entries: VecDeque<PendingWork>,
    keys: HashSet<PendingWorkKey>,
    pin_work_count: usize,
    identity_lifecycle_count: usize,
    identity_lifecycle_overflowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IdentityLifecycleWork {
    pub(crate) user_id: String,
    pub(crate) identity_revision: u64,
    pub(crate) state: IdentityLifecycleState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KeyRedistributionWork {
    pub(crate) session_id: SessionId,
    pub(crate) fence_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PinRefreshWork {
    pub(crate) user_id: String,
    attempts: u8,
    not_before: Instant,
}

impl PinRefreshWork {
    fn initial(user_id: &str) -> Self {
        Self {
            user_id: user_id.to_owned(),
            attempts: 0,
            not_before: Instant::now(),
        }
    }

    pub(crate) fn retry(mut self) -> Self {
        self.attempts = self.attempts.saturating_add(1);
        let exponent = u32::from(self.attempts.saturating_sub(1).min(6));
        let delay = PIN_REFRESH_RETRY_BASE
            .checked_mul(2_u32.pow(exponent))
            .unwrap_or(PIN_REFRESH_RETRY_MAX)
            .min(PIN_REFRESH_RETRY_MAX);
        self.not_before = Instant::now() + delay;
        self
    }

    fn ready(&self, now: Instant) -> bool {
        now >= self.not_before
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReadyDeleteWork {
    pub(crate) session_id: SessionId,
    not_before: Instant,
}

impl ReadyDeleteWork {
    fn initial(session_id: SessionId) -> Self {
        Self {
            session_id,
            not_before: Instant::now(),
        }
    }

    pub(crate) fn retry(mut self, now: Instant) -> Self {
        self.not_before = now + READY_DELETE_RETRY_DELAY;
        self
    }

    fn ready(&self, now: Instant) -> bool {
        now >= self.not_before
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingWork {
    PinResetAll,
    IdentityLifecycle(IdentityLifecycleWork),
    PinReset { user_id: String },
    PinRefresh(PinRefreshWork),
    BackendScopeRestore(BackendScopeRestore),
    HostKeyRotation(SessionId),
    KeyRedistribution(KeyRedistributionWork),
    DeleteAfterStop(SessionId),
    ReadyDelete(ReadyDeleteWork),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum PendingWorkKey {
    PinResetAll,
    IdentityLifecycle(String),
    PinReset(String),
    PinRefresh(String),
    BackendScopeRestore(SessionId),
    HostKeyRotation(SessionId),
    KeyRedistribution(SessionId),
    DeleteAfterStop(SessionId),
    ReadyDelete(SessionId),
}

impl PendingWork {
    fn key(&self) -> PendingWorkKey {
        match self {
            Self::PinResetAll => PendingWorkKey::PinResetAll,
            Self::IdentityLifecycle(work) => {
                PendingWorkKey::IdentityLifecycle(work.user_id.clone())
            }
            Self::PinReset { user_id } => PendingWorkKey::PinReset(user_id.clone()),
            Self::PinRefresh(work) => PendingWorkKey::PinRefresh(work.user_id.clone()),
            Self::BackendScopeRestore(work) => PendingWorkKey::BackendScopeRestore(work.session_id),
            Self::HostKeyRotation(id) => PendingWorkKey::HostKeyRotation(*id),
            Self::KeyRedistribution(work) => PendingWorkKey::KeyRedistribution(work.session_id),
            Self::DeleteAfterStop(id) => PendingWorkKey::DeleteAfterStop(*id),
            Self::ReadyDelete(work) => PendingWorkKey::ReadyDelete(work.session_id),
        }
    }
}

impl PendingWorkQueue {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn queue_pin_reset_all(&mut self) {
        self.coalesce_pin_work_to_reset_all();
    }

    pub(crate) fn queue_identity_lifecycle(&mut self, work: IdentityLifecycleWork) -> bool {
        let key = PendingWorkKey::IdentityLifecycle(work.user_id.clone());
        if let Some(existing) = self.entries.iter_mut().find_map(|entry| match entry {
            PendingWork::IdentityLifecycle(existing) if existing.user_id == work.user_id => {
                Some(existing)
            }
            _ => None,
        }) {
            if work.identity_revision > existing.identity_revision {
                *existing = work;
            }
            return true;
        }
        if self.identity_lifecycle_count >= PENDING_PIN_WORK_CAPACITY {
            self.identity_lifecycle_overflowed = true;
            tracing::warn!(
                capacity = PENDING_PIN_WORK_CAPACITY,
                "pending identity lifecycle queue is full; reconnect required"
            );
            return false;
        }
        debug_assert!(!self.keys.contains(&key));
        self.queue(PendingWork::IdentityLifecycle(work))
    }

    pub(crate) fn take_identity_lifecycle_overflow(&mut self) -> bool {
        std::mem::take(&mut self.identity_lifecycle_overflowed)
    }

    pub(crate) fn pop_identity_lifecycle(&mut self) -> Option<IdentityLifecycleWork> {
        let key = self.entries.iter().find_map(|work| match work {
            PendingWork::IdentityLifecycle(work) => {
                Some(PendingWorkKey::IdentityLifecycle(work.user_id.clone()))
            }
            _ => None,
        })?;
        if let Some(PendingWork::IdentityLifecycle(work)) = self.remove_key(&key) {
            Some(work)
        } else {
            debug_assert!(
                false,
                "identity lifecycle key resolved to another work type"
            );
            None
        }
    }

    pub(crate) fn queue_pin_reset(&mut self, user_id: &str) {
        if self.keys.contains(&PendingWorkKey::PinResetAll) {
            return;
        }
        let key = PendingWorkKey::PinReset(user_id.to_owned());
        if self.keys.contains(&key) {
            return;
        }
        self.remove_key(&PendingWorkKey::PinRefresh(user_id.to_owned()));

        if self.pin_work_count >= PENDING_PIN_WORK_CAPACITY {
            if self.drop_oldest_pin_refresh() {
                tracing::warn!(
                    capacity = PENDING_PIN_WORK_CAPACITY,
                    "pending pin reset queue full; evicted a refresh to preserve reset work"
                );
            } else {
                self.coalesce_pin_work_to_reset_all();
                return;
            }
        }

        self.queue(PendingWork::PinReset {
            user_id: user_id.to_owned(),
        });
    }

    pub(crate) fn queue_pin_refresh(&mut self, user_id: &str) {
        if self.keys.contains(&PendingWorkKey::PinResetAll)
            || self
                .keys
                .contains(&PendingWorkKey::PinReset(user_id.to_owned()))
        {
            return;
        }
        self.queue_pin_refresh_work(PinRefreshWork::initial(user_id));
    }

    pub(crate) fn requeue_pin_refresh(&mut self, work: PinRefreshWork) {
        self.queue_pin_refresh_work(work.retry());
    }

    fn queue_pin_refresh_work(&mut self, work: PinRefreshWork) {
        if self.keys.contains(&PendingWorkKey::PinResetAll)
            || self
                .keys
                .contains(&PendingWorkKey::PinReset(work.user_id.clone()))
        {
            return;
        }
        let key = PendingWorkKey::PinRefresh(work.user_id.clone());
        if self.keys.contains(&key) {
            return;
        }
        if self.pin_work_count >= PENDING_PIN_WORK_CAPACITY && !self.drop_oldest_pin_refresh() {
            tracing::warn!(
                capacity = PENDING_PIN_WORK_CAPACITY,
                "pending pin refresh queue full of reset work; dropped incoming refresh"
            );
            return;
        }
        self.queue(PendingWork::PinRefresh(work));
    }

    pub(crate) fn drain_pin_resets(&mut self) -> Vec<String> {
        let mut drained = Vec::new();
        let mut kept = VecDeque::with_capacity(self.entries.len());

        while let Some(work) = self.entries.pop_front() {
            match work {
                PendingWork::PinReset { user_id } => {
                    self.keys.remove(&PendingWorkKey::PinReset(user_id.clone()));
                    self.pin_work_count = self.pin_work_count.saturating_sub(1);
                    drained.push(user_id);
                }
                other => kept.push_back(other),
            }
        }

        self.entries = kept;
        drained
    }

    pub(crate) fn take_pin_reset_all(&mut self) -> bool {
        self.remove_key(&PendingWorkKey::PinResetAll).is_some()
    }

    pub(crate) fn drain_pin_refreshes(&mut self) -> Vec<PinRefreshWork> {
        self.drain_pin_refreshes_at(Instant::now())
    }

    fn drain_pin_refreshes_at(&mut self, now: Instant) -> Vec<PinRefreshWork> {
        let mut drained = Vec::new();
        let mut kept = VecDeque::with_capacity(self.entries.len());
        while let Some(work) = self.entries.pop_front() {
            match work {
                PendingWork::PinRefresh(work) if work.ready(now) => {
                    self.keys
                        .remove(&PendingWorkKey::PinRefresh(work.user_id.clone()));
                    self.pin_work_count = self.pin_work_count.saturating_sub(1);
                    drained.push(work);
                }
                other => kept.push_back(other),
            }
        }
        self.entries = kept;
        drained
    }

    pub(crate) fn clear_pin_work(&mut self) {
        self.entries.retain(|work| {
            !matches!(
                work,
                PendingWork::PinResetAll
                    | PendingWork::IdentityLifecycle(_)
                    | PendingWork::PinReset { .. }
                    | PendingWork::PinRefresh(_)
            )
        });
        self.keys.retain(|key| {
            !matches!(
                key,
                PendingWorkKey::PinResetAll
                    | PendingWorkKey::IdentityLifecycle(_)
                    | PendingWorkKey::PinReset(_)
                    | PendingWorkKey::PinRefresh(_)
            )
        });
        self.pin_work_count = 0;
        self.identity_lifecycle_count = 0;
        self.identity_lifecycle_overflowed = false;
    }

    #[cfg(test)]
    pub(crate) fn pin_work_count(&self) -> usize {
        self.pin_work_count
    }

    pub(crate) fn clear_account_epoch(&mut self) {
        self.entries.retain(|work| {
            matches!(
                work,
                PendingWork::PinResetAll
                    | PendingWork::IdentityLifecycle(_)
                    | PendingWork::PinReset { .. }
                    | PendingWork::PinRefresh(_)
                    | PendingWork::DeleteAfterStop(_)
                    | PendingWork::ReadyDelete(_)
            )
        });
        self.keys = self.entries.iter().map(PendingWork::key).collect();
        self.pin_work_count = self
            .entries
            .iter()
            .filter(|work| {
                matches!(
                    work,
                    PendingWork::PinResetAll
                        | PendingWork::PinReset { .. }
                        | PendingWork::PinRefresh(_)
                )
            })
            .count();
        self.identity_lifecycle_count = self
            .entries
            .iter()
            .filter(|work| matches!(work, PendingWork::IdentityLifecycle(_)))
            .count();
        self.identity_lifecycle_overflowed = false;
    }

    pub(crate) fn queue_host_key_rotation(&mut self, id: SessionId) {
        self.queue(PendingWork::HostKeyRotation(id));
    }

    pub(crate) fn queue_backend_scope_restore(&mut self, work: BackendScopeRestore) {
        let key = PendingWorkKey::BackendScopeRestore(work.session_id);
        if let Some(existing) = self.entries.iter_mut().find_map(|entry| match entry {
            PendingWork::BackendScopeRestore(existing)
                if existing.session_id == work.session_id =>
            {
                Some(existing)
            }
            _ => None,
        }) {
            existing.scope = work.scope;
            existing.room = work.room;
            existing.backend_session_id = work.backend_session_id;
            existing.incarnation_id = work.incarnation_id;
            existing.requires_key_rotation |= work.requires_key_rotation;
            return;
        }
        debug_assert!(!self.keys.contains(&key));
        self.queue(PendingWork::BackendScopeRestore(work));
    }

    pub(crate) fn backend_scope_restore(&self, id: SessionId) -> Option<&BackendScopeRestore> {
        self.entries.iter().find_map(|work| match work {
            PendingWork::BackendScopeRestore(repair) if repair.session_id == id => Some(repair),
            _ => None,
        })
    }

    pub(crate) fn backend_scope_restores(&self) -> Vec<BackendScopeRestore> {
        self.entries
            .iter()
            .filter_map(|work| match work {
                PendingWork::BackendScopeRestore(repair) => Some(repair.clone()),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn take_backend_scope_restore(
        &mut self,
        id: SessionId,
    ) -> Option<BackendScopeRestore> {
        let PendingWork::BackendScopeRestore(work) =
            self.remove_key(&PendingWorkKey::BackendScopeRestore(id))?
        else {
            debug_assert!(false, "scope restore key resolved to different work");
            return None;
        };
        Some(work)
    }

    pub(crate) fn clear_backend_scope_restore(&mut self, id: SessionId) -> bool {
        self.remove_key(&PendingWorkKey::BackendScopeRestore(id))
            .is_some()
    }

    pub(crate) fn clear_backend_scope_restore_if_same(
        &mut self,
        expected: &BackendScopeRestore,
    ) -> bool {
        if self.backend_scope_restore(expected.session_id) != Some(expected) {
            return false;
        }
        self.clear_backend_scope_restore(expected.session_id)
    }

    pub(crate) fn drain_host_key_rotations(&mut self) -> Vec<SessionId> {
        let mut drained = Vec::new();
        let mut kept = VecDeque::with_capacity(self.entries.len());

        while let Some(work) = self.entries.pop_front() {
            match work {
                PendingWork::HostKeyRotation(id) => {
                    self.keys.remove(&PendingWorkKey::HostKeyRotation(id));
                    drained.push(id);
                }
                other => kept.push_back(other),
            }
        }

        self.entries = kept;
        drained
    }

    pub(crate) fn clear_host_key_rotation(&mut self, id: SessionId) -> bool {
        self.remove_key(&PendingWorkKey::HostKeyRotation(id))
            .is_some()
    }

    pub(crate) fn queue_key_redistribution(&mut self, id: SessionId) {
        self.queue_key_redistribution_fence(id, None);
    }

    pub(crate) fn queue_key_redistribution_fence(
        &mut self,
        id: SessionId,
        fence_id: Option<String>,
    ) {
        if let Some(PendingWork::KeyRedistribution(work)) = self.entries.iter_mut().find(
            |entry| matches!(entry, PendingWork::KeyRedistribution(work) if work.session_id == id),
        ) {
            if let Some(fence_id) = fence_id
                && !work.fence_ids.contains(&fence_id)
            {
                work.fence_ids.push(fence_id);
            }
            return;
        }
        self.queue(PendingWork::KeyRedistribution(KeyRedistributionWork {
            session_id: id,
            fence_ids: fence_id.into_iter().collect(),
        }));
    }

    pub(crate) fn requeue_key_redistribution(&mut self, work: KeyRedistributionWork) {
        for fence_id in work.fence_ids {
            self.queue_key_redistribution_fence(work.session_id, Some(fence_id));
        }
        if !self
            .keys
            .contains(&PendingWorkKey::KeyRedistribution(work.session_id))
        {
            self.queue_key_redistribution(work.session_id);
        }
    }

    pub(crate) fn drain_key_redistributions(&mut self) -> Vec<KeyRedistributionWork> {
        let mut drained = Vec::new();
        let mut kept = VecDeque::with_capacity(self.entries.len());

        while let Some(work) = self.entries.pop_front() {
            match work {
                PendingWork::KeyRedistribution(work) => {
                    self.keys
                        .remove(&PendingWorkKey::KeyRedistribution(work.session_id));
                    drained.push(work);
                }
                other => kept.push_back(other),
            }
        }

        self.entries = kept;
        drained
    }

    pub(crate) fn clear_key_redistribution(&mut self, id: SessionId) -> bool {
        self.remove_key(&PendingWorkKey::KeyRedistribution(id))
            .is_some()
    }

    pub(crate) fn queue_delete_after_stop(&mut self, id: SessionId) {
        self.queue(PendingWork::DeleteAfterStop(id));
    }

    pub(crate) fn promote_delete_after_stop(&mut self, id: SessionId) -> bool {
        if self
            .remove_key(&PendingWorkKey::DeleteAfterStop(id))
            .is_none()
        {
            return false;
        }

        self.queue(PendingWork::ReadyDelete(ReadyDeleteWork::initial(id)));
        true
    }

    pub(crate) fn queue_ready_delete(&mut self, id: SessionId) {
        self.queue(PendingWork::ReadyDelete(ReadyDeleteWork::initial(id)));
    }

    pub(crate) fn requeue_ready_delete(&mut self, work: ReadyDeleteWork, now: Instant) {
        self.queue(PendingWork::ReadyDelete(work.retry(now)));
    }

    pub(crate) fn drain_ready_deletes(&mut self, now: Instant) -> Vec<ReadyDeleteWork> {
        let mut drained = Vec::new();
        let mut kept = VecDeque::with_capacity(self.entries.len());

        while let Some(work) = self.entries.pop_front() {
            match work {
                PendingWork::ReadyDelete(work) if work.ready(now) => {
                    self.keys
                        .remove(&PendingWorkKey::ReadyDelete(work.session_id));
                    drained.push(work);
                }
                other => kept.push_back(other),
            }
        }

        self.entries = kept;
        drained
    }

    pub(crate) fn clear_delete(&mut self, id: SessionId) -> bool {
        let removed_after_stop = self
            .remove_key(&PendingWorkKey::DeleteAfterStop(id))
            .is_some();
        let removed_ready = self.remove_key(&PendingWorkKey::ReadyDelete(id)).is_some();
        removed_after_stop || removed_ready
    }

    fn queue(&mut self, work: PendingWork) -> bool {
        let key = work.key();
        if !self.keys.insert(key) {
            return false;
        }

        if matches!(work, PendingWork::IdentityLifecycle(_)) {
            self.identity_lifecycle_count += 1;
        } else if matches!(
            work,
            PendingWork::PinResetAll | PendingWork::PinReset { .. } | PendingWork::PinRefresh(_)
        ) {
            self.pin_work_count += 1;
        }
        self.entries.push_back(work);
        true
    }

    fn drop_oldest_pin_refresh(&mut self) -> bool {
        let Some(key) = self.entries.iter().find_map(|work| match work {
            PendingWork::PinRefresh(work) => Some(PendingWorkKey::PinRefresh(work.user_id.clone())),
            _ => None,
        }) else {
            return false;
        };

        self.remove_key(&key).is_some()
    }

    fn coalesce_pin_work_to_reset_all(&mut self) {
        let pin_keys = self
            .keys
            .iter()
            .filter(|key| {
                matches!(
                    key,
                    PendingWorkKey::PinReset(_) | PendingWorkKey::PinRefresh(_)
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        for key in pin_keys {
            self.remove_key(&key);
        }
        self.queue(PendingWork::PinResetAll);
        tracing::warn!(
            capacity = PENDING_PIN_WORK_CAPACITY,
            "pending pin reset queue coalesced to reset-all"
        );
    }

    fn remove_key(&mut self, key: &PendingWorkKey) -> Option<PendingWork> {
        if !self.keys.contains(key) {
            return None;
        }

        let Some(index) = self.entries.iter().position(|work| work.key() == *key) else {
            debug_assert!(false, "pending work key without queue entry");
            self.keys.remove(key);
            return None;
        };

        let Some(work) = self.entries.remove(index) else {
            debug_assert!(false, "pending work index disappeared");
            self.keys.remove(key);
            return None;
        };

        self.keys.remove(key);
        if matches!(work, PendingWork::IdentityLifecycle(_)) {
            self.identity_lifecycle_count = self.identity_lifecycle_count.saturating_sub(1);
        } else if matches!(
            work,
            PendingWork::PinResetAll | PendingWork::PinReset { .. } | PendingWork::PinRefresh(_)
        ) {
            self.pin_work_count = self.pin_work_count.saturating_sub(1);
        }
        Some(work)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BackendScopeRestore, IdentityLifecycleWork, KeyRedistributionWork,
        PENDING_PIN_WORK_CAPACITY, PendingWorkKey, PendingWorkQueue, READY_DELETE_RETRY_DELAY,
    };
    use kodosi_domain::{ids::SessionId, permissions::ShareScope};
    use std::time::Instant;

    #[test]
    fn failed_lifecycle_retry_preserves_unattempted_work() {
        let mut queue = PendingWorkQueue::new();
        let first = IdentityLifecycleWork {
            user_id: "first".to_owned(),
            identity_revision: 2,
            state: kodosi_domain::user::IdentityLifecycleState::Withdrawn,
        };
        let second = IdentityLifecycleWork {
            user_id: "second".to_owned(),
            identity_revision: 3,
            state: kodosi_domain::user::IdentityLifecycleState::Enrolled {
                incarnation_id: uuid::Uuid::now_v7(),
            },
        };
        assert!(queue.queue_identity_lifecycle(first.clone()));
        assert!(queue.queue_identity_lifecycle(second.clone()));

        let failed = queue
            .pop_identity_lifecycle()
            .expect("first lifecycle attempt");
        assert_eq!(failed, first);
        assert!(queue.queue_identity_lifecycle(failed));

        assert_eq!(queue.pop_identity_lifecycle(), Some(second));
        assert_eq!(queue.pop_identity_lifecycle(), Some(first));
        assert_eq!(queue.pop_identity_lifecycle(), None);
    }

    #[test]
    fn pin_resets_coalesce_to_bounded_reset_all() {
        let mut queue = PendingWorkQueue::new();

        for index in 0..=PENDING_PIN_WORK_CAPACITY {
            queue.queue_pin_reset(&format!("user-{index}"));
        }

        assert!(queue.take_pin_reset_all());
        assert!(queue.drain_pin_resets().is_empty());
        assert_eq!(queue.pin_work_count, 0);
    }

    #[test]
    fn reset_evicts_refresh_but_refresh_never_evicts_reset() {
        let mut queue = PendingWorkQueue::new();
        queue.queue_pin_refresh("refresh-first");
        for index in 0..PENDING_PIN_WORK_CAPACITY {
            queue.queue_pin_reset(&format!("reset-{index}"));
        }
        assert!(queue.drain_pin_refreshes().is_empty());

        queue.queue_pin_refresh("refresh-dropped");
        assert!(queue.drain_pin_refreshes().is_empty());
        assert_eq!(queue.drain_pin_resets().len(), PENDING_PIN_WORK_CAPACITY);
    }

    #[test]
    fn pin_resets_and_refreshes_are_deduplicated_and_drained_independently() {
        let mut queue = PendingWorkQueue::new();

        queue.queue_pin_reset("reset-user");
        queue.queue_pin_reset("reset-user");
        queue.queue_pin_refresh("refresh-user");
        queue.queue_pin_refresh("refresh-user");

        assert_eq!(queue.pin_work_count, 2);
        assert_eq!(
            queue
                .drain_pin_refreshes()
                .into_iter()
                .map(|work| work.user_id)
                .collect::<Vec<_>>(),
            vec!["refresh-user"]
        );
        assert_eq!(queue.pin_work_count, 1);
        assert_eq!(queue.drain_pin_resets(), vec!["reset-user"]);
        assert_eq!(queue.pin_work_count, 0);
    }

    #[test]
    fn failed_pin_refresh_retries_with_bounded_backoff() {
        let mut queue = PendingWorkQueue::new();
        queue.queue_pin_refresh("peer");
        let first = queue
            .drain_pin_refreshes()
            .into_iter()
            .next()
            .expect("initial refresh is ready");
        let first_deadline = std::time::Instant::now();
        queue.requeue_pin_refresh(first);

        assert!(queue.drain_pin_refreshes_at(first_deadline).is_empty());
        let second = queue
            .drain_pin_refreshes_at(first_deadline + std::time::Duration::from_secs(2))
            .into_iter()
            .next()
            .expect("first retry becomes ready");
        queue.requeue_pin_refresh(second);
        assert!(
            queue
                .drain_pin_refreshes_at(first_deadline + std::time::Duration::from_secs(2))
                .is_empty()
        );
        assert_eq!(queue.pin_work_count, 1);
    }

    #[test]
    fn pin_reset_supersedes_refresh_retry() {
        let mut queue = PendingWorkQueue::new();
        queue.queue_pin_refresh("peer");
        let failed = queue
            .drain_pin_refreshes()
            .into_iter()
            .next()
            .expect("refresh ready");
        queue.queue_pin_reset("peer");
        queue.requeue_pin_refresh(failed);

        assert!(queue.drain_pin_refreshes().is_empty());
        assert_eq!(queue.drain_pin_resets(), vec!["peer"]);
        assert_eq!(queue.pin_work_count, 0);
    }

    #[test]
    fn key_redistributions_are_deduplicated() {
        let mut queue = PendingWorkQueue::new();
        let id = SessionId::new();

        queue.queue_key_redistribution(id);
        queue.queue_key_redistribution(id);
        queue.queue_key_redistribution_fence(id, Some("fence-1".to_owned()));
        queue.queue_key_redistribution_fence(id, Some("fence-2".to_owned()));
        queue.queue_key_redistribution_fence(id, Some("fence-1".to_owned()));

        assert!(queue.keys.contains(&PendingWorkKey::KeyRedistribution(id)));
        assert_eq!(
            queue.drain_key_redistributions(),
            vec![KeyRedistributionWork {
                session_id: id,
                fence_ids: vec!["fence-1".to_owned(), "fence-2".to_owned()],
            }]
        );
        assert!(!queue.keys.contains(&PendingWorkKey::KeyRedistribution(id)));
    }

    #[test]
    fn backend_scope_restore_is_deduplicated_without_losing_rotation_requirement() {
        let mut queue = PendingWorkQueue::new();
        let id = SessionId::new();

        queue.queue_backend_scope_restore(BackendScopeRestore {
            session_id: id,
            backend_session_id: "backend-1".to_owned(),
            incarnation_id: uuid::Uuid::from_u128(1),
            scope: ShareScope::Room,
            room: None,
            requires_key_rotation: true,
        });
        queue.queue_backend_scope_restore(BackendScopeRestore {
            session_id: id,
            backend_session_id: "backend-2".to_owned(),
            incarnation_id: uuid::Uuid::from_u128(2),
            scope: ShareScope::MyDevices,
            room: None,
            requires_key_rotation: false,
        });

        let repair = queue
            .backend_scope_restore(id)
            .expect("repair remains queued")
            .clone();
        assert_eq!(repair.scope, ShareScope::MyDevices);
        assert!(repair.requires_key_rotation);
        assert_eq!(queue.take_backend_scope_restore(id), Some(repair));
        assert!(queue.backend_scope_restore(id).is_none());
    }

    #[test]
    fn delete_after_stop_promotes_to_ready_delete_once() {
        let mut queue = PendingWorkQueue::new();
        let id = SessionId::new();

        queue.queue_delete_after_stop(id);
        queue.queue_delete_after_stop(id);

        assert!(queue.keys.contains(&PendingWorkKey::DeleteAfterStop(id)));
        assert!(queue.promote_delete_after_stop(id));
        assert!(!queue.keys.contains(&PendingWorkKey::DeleteAfterStop(id)));
        assert!(!queue.promote_delete_after_stop(id));
        let now = Instant::now();
        let work = queue.drain_ready_deletes(now).pop().expect("ready delete");
        assert_eq!(work.session_id, id);
        queue.requeue_ready_delete(work, now);
        assert!(queue.drain_ready_deletes(now).is_empty());
        let retried = queue
            .drain_ready_deletes(Instant::now() + READY_DELETE_RETRY_DELAY)
            .pop()
            .expect("retried delete becomes ready");
        assert_eq!(retried.session_id, id);
    }
}
