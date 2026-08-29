use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use kodosi_domain::ids::SessionId;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::host_protocol::{
    ActivePendingPermission, PendingPermissionDecisionPhase, PendingPermissionsSnapshot,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum PermissionDecision {
    Allow,
    Deny {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

#[derive(Debug)]
pub(crate) struct ResolvedDecision {
    pub decision: PermissionDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(
    clippy::struct_field_names,
    reason = "the exact protocol tuple uses three distinct domain IDs"
)]
pub(crate) struct PendingKey {
    pub session_id: SessionId,
    pub session_incarnation_id: uuid::Uuid,
    pub tool_use_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemoteResolveOutcome {
    Resolved,
    Rejected,
    PersistenceFailed { delivered: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegistryMutation {
    Changed,
    Unchanged,
}

impl RegistryMutation {
    pub(crate) const fn changed(self) -> bool {
        matches!(self, Self::Changed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingPermissionMetadata {
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub deadline_at_ms: u64,
    pub risk: crate::agent_intel::risk::ApprovalRisk,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PermissionDecisionRegistry {
    inner: Arc<Mutex<RegistryState>>,
}

#[derive(Debug, Default)]
struct RegistryState {
    generation: u64,
    pending: HashMap<PendingKey, PendingDecision>,
    incarnations: HashMap<IncarnationKey, IncarnationState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct IncarnationKey {
    session_id: SessionId,
    incarnation_id: uuid::Uuid,
}

#[derive(Debug, Default)]
struct IncarnationState {
    request_counter: u64,
    outbound_snapshot: u64,
    inbound_snapshot: u64,
}

#[derive(Debug)]
struct PendingDecision {
    sender: Option<oneshot::Sender<ResolvedDecision>>,
    staged_metadata: Option<PendingPermissionMetadata>,
    visible: Option<ActivePendingPermission>,
}

impl PermissionDecisionRegistry {
    pub(crate) fn park(&self, key: PendingKey) -> Option<oneshot::Receiver<ResolvedDecision>> {
        let (tx, rx) = oneshot::channel();
        let Ok(mut state) = self.inner.lock() else {
            return None;
        };
        if state.pending.contains_key(&key) {
            return None;
        }
        state.pending.insert(
            key,
            PendingDecision {
                sender: Some(tx),
                staged_metadata: None,
                visible: None,
            },
        );
        Some(rx)
    }

    pub(crate) fn stage_metadata(
        &self,
        key: &PendingKey,
        metadata: PendingPermissionMetadata,
    ) -> bool {
        let Ok(mut state) = self.inner.lock() else {
            return false;
        };
        let Some(pending) = state.pending.get_mut(key) else {
            return false;
        };
        pending.staged_metadata = Some(metadata);
        true
    }

    pub(crate) fn activate_local(&self, key: &PendingKey, created_at_ms: u64) -> Option<u64> {
        let Ok(mut state) = self.inner.lock() else {
            return None;
        };
        if let Some(generation) = state
            .pending
            .get(key)
            .and_then(|pending| pending.visible.as_ref())
            .map(|visible| visible.request_generation)
        {
            return Some(generation);
        }
        let incarnation_key = IncarnationKey::from(key);
        let request_generation = state
            .incarnations
            .entry(incarnation_key)
            .or_default()
            .request_counter
            .checked_add(1)?;
        let metadata = state
            .pending
            .get(key)
            .and_then(|pending| pending.staged_metadata.clone())?;
        state
            .incarnations
            .entry(incarnation_key)
            .or_default()
            .request_counter = request_generation;
        let pending = state.pending.get_mut(key)?;
        let visible = active_permission(key, request_generation, created_at_ms, &metadata);
        if pending.visible.as_ref() == Some(&visible) {
            return Some(request_generation);
        }
        pending.visible = Some(visible);
        let _ = advance_generation(&mut state);
        Some(request_generation)
    }

    pub(crate) fn replace_remote_snapshot(
        &self,
        session_id: SessionId,
        incarnation_id: uuid::Uuid,
        snapshot: PendingPermissionsSnapshot,
    ) -> RegistryMutation {
        if snapshot.generation == 0
            || snapshot.requests.iter().any(|request| {
                request.session_id != session_id.to_string()
                    || request.session_incarnation_id != incarnation_id.to_string()
                    || request.request_generation == 0
            })
        {
            return RegistryMutation::Unchanged;
        }
        let Ok(mut state) = self.inner.lock() else {
            return RegistryMutation::Unchanged;
        };
        let incarnation_key = IncarnationKey {
            session_id,
            incarnation_id,
        };
        let accepted = state
            .incarnations
            .entry(incarnation_key)
            .or_default()
            .inbound_snapshot;
        if snapshot.generation <= accepted {
            return RegistryMutation::Unchanged;
        }
        state.pending.retain(|key, pending| {
            key.session_id != session_id
                || key.session_incarnation_id != incarnation_id
                || pending.sender.is_some()
        });
        for visible in snapshot.requests {
            let key = PendingKey {
                session_id,
                session_incarnation_id: incarnation_id,
                tool_use_id: visible.tool_use_id.clone(),
            };
            state.pending.insert(
                key,
                PendingDecision {
                    sender: None,
                    staged_metadata: None,
                    visible: Some(visible),
                },
            );
        }
        state
            .incarnations
            .entry(incarnation_key)
            .or_default()
            .inbound_snapshot = snapshot.generation;
        advance_generation(&mut state)
    }

    #[cfg(test)]
    pub(crate) fn activate_remote(
        &self,
        key: PendingKey,
        request_generation: u64,
        created_at_ms: u64,
        metadata: PendingPermissionMetadata,
    ) -> RegistryMutation {
        self.replace_remote_snapshot(
            key.session_id,
            key.session_incarnation_id,
            PendingPermissionsSnapshot {
                generation: request_generation,
                requests: vec![active_permission(
                    &key,
                    request_generation,
                    created_at_ms,
                    &metadata,
                )],
            },
        )
    }

    pub(crate) fn mark_sending(
        &self,
        key: &PendingKey,
        request_generation: u64,
    ) -> RegistryMutation {
        let Ok(mut state) = self.inner.lock() else {
            return RegistryMutation::Unchanged;
        };
        let Some(visible) = state
            .pending
            .get_mut(key)
            .and_then(|pending| pending.visible.as_mut())
        else {
            return RegistryMutation::Unchanged;
        };
        if visible.request_generation != request_generation
            || visible.decision_phase == PendingPermissionDecisionPhase::Sending
        {
            return RegistryMutation::Unchanged;
        }
        visible.decision_phase = PendingPermissionDecisionPhase::Sending;
        advance_generation(&mut state)
    }

    pub(crate) fn mark_actionable(
        &self,
        key: &PendingKey,
        request_generation: u64,
    ) -> RegistryMutation {
        let Ok(mut state) = self.inner.lock() else {
            return RegistryMutation::Unchanged;
        };
        let Some(visible) = state
            .pending
            .get_mut(key)
            .and_then(|pending| pending.visible.as_mut())
        else {
            return RegistryMutation::Unchanged;
        };
        if visible.request_generation != request_generation {
            return RegistryMutation::Unchanged;
        }
        visible.decision_phase = PendingPermissionDecisionPhase::Actionable;
        advance_generation(&mut state)
    }

    pub(crate) fn resolve_exact(
        &self,
        key: &PendingKey,
        request_generation: u64,
        decision: PermissionDecision,
    ) -> bool {
        let Ok(mut state) = self.inner.lock() else {
            return false;
        };
        let Some(pending) = state.pending.get_mut(key) else {
            return false;
        };
        if pending
            .visible
            .as_ref()
            .is_none_or(|visible| visible.request_generation != request_generation)
        {
            return false;
        }
        let Some(sender) = pending.sender.take() else {
            return false;
        };
        let delivered = sender.send(ResolvedDecision { decision }).is_ok();
        if delivered
            && let Some(visible) = pending.visible.as_mut()
            && visible.decision_phase != PendingPermissionDecisionPhase::Sending
        {
            visible.decision_phase = PendingPermissionDecisionPhase::Sending;
            let _ = advance_generation(&mut state);
        }
        delivered
    }

    pub(crate) fn resolve_remote_exact<F>(
        &self,
        key: &PendingKey,
        request_generation: u64,
        decision: PermissionDecision,
        mut record_delivery: F,
    ) -> RemoteResolveOutcome
    where
        F: FnMut(bool) -> bool,
    {
        let Ok(mut state) = self.inner.lock() else {
            return RemoteResolveOutcome::Rejected;
        };
        let Some(pending) = state.pending.get_mut(key) else {
            return RemoteResolveOutcome::Rejected;
        };
        if pending
            .visible
            .as_ref()
            .is_none_or(|visible| visible.request_generation != request_generation)
        {
            return RemoteResolveOutcome::Rejected;
        }
        let Some(sender) = pending.sender.take() else {
            return RemoteResolveOutcome::Rejected;
        };
        let delivered = sender.send(ResolvedDecision { decision }).is_ok();
        if !record_delivery(delivered) {
            return RemoteResolveOutcome::PersistenceFailed { delivered };
        }
        if delivered {
            if let Some(visible) = pending.visible.as_mut()
                && visible.decision_phase != PendingPermissionDecisionPhase::Sending
            {
                visible.decision_phase = PendingPermissionDecisionPhase::Sending;
                let _ = advance_generation(&mut state);
            }
            RemoteResolveOutcome::Resolved
        } else {
            RemoteResolveOutcome::Rejected
        }
    }

    pub(crate) fn discard_unpublished(&self, key: &PendingKey) {
        if let Ok(mut state) = self.inner.lock()
            && state
                .pending
                .get(key)
                .is_some_and(|pending| pending.visible.is_none())
        {
            state.pending.remove(key);
        }
    }

    pub(crate) fn seal_sender(&self, key: &PendingKey) {
        if let Ok(mut state) = self.inner.lock()
            && let Some(pending) = state.pending.get_mut(key)
        {
            pending.sender.take();
        }
    }

    pub(crate) fn complete(&self, key: &PendingKey, request_generation: u64) -> RegistryMutation {
        let Ok(mut state) = self.inner.lock() else {
            return RegistryMutation::Unchanged;
        };
        let matches = state.pending.get(key).is_some_and(|pending| {
            pending
                .visible
                .as_ref()
                .is_some_and(|visible| visible.request_generation == request_generation)
        });
        if !matches {
            return RegistryMutation::Unchanged;
        }
        state.pending.remove(key);
        advance_generation(&mut state)
    }

    pub(crate) fn request_generation(&self, key: &PendingKey) -> Option<u64> {
        self.inner.lock().ok().and_then(|state| {
            state.pending.get(key).and_then(|pending| {
                pending
                    .visible
                    .as_ref()
                    .map(|visible| visible.request_generation)
            })
        })
    }

    pub(crate) fn visible_identity(&self, key: &PendingKey, request_generation: u64) -> bool {
        self.request_generation(key) == Some(request_generation)
    }

    pub(crate) fn actionable_identity(&self, key: &PendingKey, request_generation: u64) -> bool {
        self.inner.lock().is_ok_and(|state| {
            state.pending.get(key).is_some_and(|pending| {
                pending.visible.as_ref().is_some_and(|visible| {
                    visible.request_generation == request_generation
                        && visible.decision_phase == PendingPermissionDecisionPhase::Actionable
                })
            })
        })
    }

    pub(crate) fn snapshot(&self) -> PendingPermissionsSnapshot {
        let Ok(state) = self.inner.lock() else {
            return empty_snapshot();
        };
        snapshot_from_state(&state)
    }

    pub(crate) fn snapshot_for_query(&self) -> Option<PendingPermissionsSnapshot> {
        let Ok(mut state) = self.inner.lock() else {
            return None;
        };
        advance_generation(&mut state)
            .changed()
            .then(|| snapshot_from_state(&state))
    }

    pub(crate) fn snapshot_for_publish(
        &self,
        session_id: SessionId,
        incarnation_id: uuid::Uuid,
    ) -> Option<PendingPermissionsSnapshot> {
        let Ok(mut state) = self.inner.lock() else {
            return None;
        };
        let incarnation = state
            .incarnations
            .entry(IncarnationKey {
                session_id,
                incarnation_id,
            })
            .or_default();
        incarnation.outbound_snapshot = incarnation.outbound_snapshot.checked_add(1)?;
        let generation = incarnation.outbound_snapshot;
        let mut requests = state
            .pending
            .iter()
            .filter(|(key, _)| {
                key.session_id == session_id && key.session_incarnation_id == incarnation_id
            })
            .filter_map(|(_, pending)| pending.visible.clone())
            .collect::<Vec<_>>();
        requests.sort_by(|left, right| {
            (&left.tool_use_id, left.request_generation)
                .cmp(&(&right.tool_use_id, right.request_generation))
        });
        Some(PendingPermissionsSnapshot {
            generation,
            requests,
        })
    }

    pub(crate) fn clear_pending_for_incarnation(
        &self,
        session_id: SessionId,
        session_incarnation_id: uuid::Uuid,
    ) -> RegistryMutation {
        let Ok(mut state) = self.inner.lock() else {
            return RegistryMutation::Unchanged;
        };
        let had_visible = state.pending.iter().any(|(key, pending)| {
            key.session_id == session_id
                && key.session_incarnation_id == session_incarnation_id
                && pending.visible.is_some()
        });
        state.pending.retain(|key, _| {
            key.session_id != session_id || key.session_incarnation_id != session_incarnation_id
        });
        if had_visible {
            advance_generation(&mut state)
        } else {
            RegistryMutation::Unchanged
        }
    }

    pub(crate) fn clear_remote_for_reconnect(
        &self,
        session_id: SessionId,
        session_incarnation_id: uuid::Uuid,
    ) -> RegistryMutation {
        let Ok(mut state) = self.inner.lock() else {
            return RegistryMutation::Unchanged;
        };
        let had_visible = state.pending.iter().any(|(key, pending)| {
            key.session_id == session_id
                && key.session_incarnation_id == session_incarnation_id
                && pending.visible.is_some()
        });
        state.pending.retain(|key, _| {
            key.session_id != session_id || key.session_incarnation_id != session_incarnation_id
        });
        state
            .incarnations
            .entry(IncarnationKey {
                session_id,
                incarnation_id: session_incarnation_id,
            })
            .or_default()
            .inbound_snapshot = 0;
        if had_visible {
            advance_generation(&mut state)
        } else {
            RegistryMutation::Unchanged
        }
    }

    pub(crate) fn retire_incarnation(
        &self,
        session_id: SessionId,
        session_incarnation_id: uuid::Uuid,
    ) -> RegistryMutation {
        let Ok(mut state) = self.inner.lock() else {
            return RegistryMutation::Unchanged;
        };
        let had_visible = state.pending.iter().any(|(key, pending)| {
            key.session_id == session_id
                && key.session_incarnation_id == session_incarnation_id
                && pending.visible.is_some()
        });
        state.pending.retain(|key, _| {
            key.session_id != session_id || key.session_incarnation_id != session_incarnation_id
        });
        state.incarnations.remove(&IncarnationKey {
            session_id,
            incarnation_id: session_incarnation_id,
        });
        if had_visible {
            advance_generation(&mut state)
        } else {
            RegistryMutation::Unchanged
        }
    }
}

impl From<&PendingKey> for IncarnationKey {
    fn from(key: &PendingKey) -> Self {
        Self {
            session_id: key.session_id,
            incarnation_id: key.session_incarnation_id,
        }
    }
}

fn empty_snapshot() -> PendingPermissionsSnapshot {
    PendingPermissionsSnapshot {
        generation: 0,
        requests: Vec::new(),
    }
}

fn active_permission(
    key: &PendingKey,
    request_generation: u64,
    created_at_ms: u64,
    metadata: &PendingPermissionMetadata,
) -> ActivePendingPermission {
    ActivePendingPermission {
        session_id: key.session_id.to_string(),
        session_incarnation_id: key.session_incarnation_id.to_string(),
        request_generation,
        tool_use_id: key.tool_use_id.clone(),
        tool_name: metadata.tool_name.clone(),
        tool_input: metadata.tool_input.clone(),
        created_at_ms,
        deadline_at_ms: metadata.deadline_at_ms,
        risk: metadata.risk,
        decision_phase: PendingPermissionDecisionPhase::Actionable,
    }
}

fn snapshot_from_state(state: &RegistryState) -> PendingPermissionsSnapshot {
    let mut requests = state
        .pending
        .values()
        .filter_map(|pending| pending.visible.clone())
        .collect::<Vec<_>>();
    requests.sort_by(|left, right| {
        (&left.session_id, &left.tool_use_id, left.request_generation).cmp(&(
            &right.session_id,
            &right.tool_use_id,
            right.request_generation,
        ))
    });
    PendingPermissionsSnapshot {
        generation: state.generation,
        requests,
    }
}

fn advance_generation(state: &mut RegistryState) -> RegistryMutation {
    let Some(next) = state.generation.checked_add(1) else {
        tracing::error!("pending-permission snapshot generation exhausted; refusing mutation");
        return RegistryMutation::Unchanged;
    };
    state.generation = next;
    RegistryMutation::Changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_intel::risk::ApprovalRisk;

    fn key() -> PendingKey {
        PendingKey {
            session_id: SessionId::new(),
            session_incarnation_id: uuid::Uuid::now_v7(),
            tool_use_id: "toolu_test".to_owned(),
        }
    }

    fn metadata() -> PendingPermissionMetadata {
        PendingPermissionMetadata {
            tool_name: "Bash".to_owned(),
            tool_input: serde_json::json!({"command": "ls"}),
            deadline_at_ms: 200,
            risk: ApprovalRisk::Destructive,
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn exact_visible_decision_is_dispatched_and_remains_sending() {
        let registry = PermissionDecisionRegistry::default();
        let key = key();
        let receiver = registry.park(key.clone()).expect("park");
        assert!(registry.stage_metadata(&key, metadata()));
        assert_eq!(registry.activate_local(&key, 100), Some(1));
        assert!(registry.resolve_exact(&key, 1, PermissionDecision::Allow));
        assert_eq!(receiver.await.unwrap().decision, PermissionDecision::Allow);
        let snapshot = registry.snapshot();
        assert_eq!(snapshot.requests.len(), 1);
        assert_eq!(
            snapshot.requests[0].decision_phase,
            PendingPermissionDecisionPhase::Sending
        );
        assert!(registry.complete(&key, 1).changed());
        assert!(registry.snapshot().requests.is_empty());
    }

    #[test]
    fn duplicate_active_tuple_is_rejected_without_displacing_first() {
        let registry = PermissionDecisionRegistry::default();
        let key = key();
        let first = registry.park(key.clone());
        assert!(first.is_some());
        assert!(registry.stage_metadata(&key, metadata()));
        assert_eq!(registry.activate_local(&key, 100), Some(1));
        assert!(registry.park(key).is_none());
        let snapshot = registry.snapshot();
        assert_eq!(snapshot.requests.len(), 1);
        assert_eq!(snapshot.requests[0].request_generation, 1);
    }

    #[test]
    fn wrong_generation_and_incarnation_cannot_resolve() {
        let registry = PermissionDecisionRegistry::default();
        let key = key();
        let _receiver = registry.park(key.clone()).expect("park");
        assert!(registry.stage_metadata(&key, metadata()));
        assert_eq!(registry.activate_local(&key, 100), Some(1));
        assert!(!registry.resolve_exact(&key, 2, PermissionDecision::Allow));
        let mut replacement = key.clone();
        replacement.session_incarnation_id = uuid::Uuid::now_v7();
        assert!(!registry.resolve_exact(&replacement, 1, PermissionDecision::Allow));
    }

    #[test]
    fn failed_remote_delivery_restores_only_exact_generation() {
        let registry = PermissionDecisionRegistry::default();
        let key = key();
        assert!(registry.park(key.clone()).is_some());
        assert!(registry.stage_metadata(&key, metadata()));
        assert_eq!(registry.activate_local(&key, 100), Some(1));
        assert!(registry.mark_sending(&key, 1).changed());
        let sending_generation = registry.snapshot().generation;

        assert!(!registry.mark_actionable(&key, 2).changed());
        assert_eq!(registry.snapshot().generation, sending_generation);
        assert!(registry.mark_actionable(&key, 1).changed());
        let snapshot = registry.snapshot();
        assert!(snapshot.generation > sending_generation);
        assert_eq!(
            snapshot.requests[0].decision_phase,
            PendingPermissionDecisionPhase::Actionable
        );
        let actionable_generation = snapshot.generation;
        assert!(registry.mark_actionable(&key, 1).changed());
        assert!(registry.snapshot().generation > actionable_generation);
    }

    #[test]
    fn snapshot_queries_advance_generation_without_changing_membership() {
        let registry = PermissionDecisionRegistry::default();
        let key = key();
        let _receiver = registry.park(key.clone()).expect("park");
        assert!(registry.stage_metadata(&key, metadata()));
        assert_eq!(registry.activate_local(&key, 100), Some(1));
        let before = registry.snapshot();

        let queried = registry.snapshot_for_query().expect("query snapshot");

        assert!(queried.generation > before.generation);
        assert_eq!(queried.requests, before.requests);
        assert_eq!(registry.snapshot(), queried);
    }

    #[test]
    fn visible_decision_phase_mutations_advance_generation() {
        let registry = PermissionDecisionRegistry::default();
        let key = key();
        let _receiver = registry.park(key.clone()).expect("park");
        assert!(registry.stage_metadata(&key, metadata()));
        assert_eq!(registry.activate_local(&key, 100), Some(1));
        let first = registry.snapshot().generation;
        assert_eq!(registry.activate_local(&key, 100), Some(1));
        assert_eq!(registry.snapshot().generation, first);
        assert!(registry.mark_sending(&key, 1).changed());
        assert!(registry.snapshot().generation > first);
    }

    #[test]
    fn teardown_only_removes_matching_incarnation() {
        let registry = PermissionDecisionRegistry::default();
        let first = key();
        let mut second = first.clone();
        second.session_incarnation_id = uuid::Uuid::now_v7();
        for key in [&first, &second] {
            let _receiver = registry.park(key.clone()).expect("park");
            assert!(registry.stage_metadata(key, metadata()));
            assert_eq!(registry.activate_local(key, 100), Some(1));
        }
        assert!(
            registry
                .retire_incarnation(first.session_id, first.session_incarnation_id)
                .changed()
        );
        let snapshot = registry.snapshot();
        assert_eq!(snapshot.requests.len(), 1);
        assert_eq!(
            snapshot.requests[0].session_incarnation_id,
            second.session_incarnation_id.to_string()
        );
    }

    #[test]
    fn request_generation_is_registry_owned_and_monotonic_per_incarnation() {
        let registry = PermissionDecisionRegistry::default();
        let first = key();
        let mut second = first.clone();
        second.tool_use_id = "toolu_second".to_owned();
        for (key, expected) in [(&first, 1), (&second, 2)] {
            let _receiver = registry.park(key.clone()).expect("park");
            assert!(registry.stage_metadata(key, metadata()));
            assert_eq!(registry.activate_local(key, 100), Some(expected));
        }
        let mut replacement_incarnation = first.clone();
        replacement_incarnation.session_incarnation_id = uuid::Uuid::now_v7();
        replacement_incarnation.tool_use_id = "toolu_replacement".to_owned();
        let _receiver = registry
            .park(replacement_incarnation.clone())
            .expect("park");
        assert!(registry.stage_metadata(&replacement_incarnation, metadata()));
        assert_eq!(
            registry.activate_local(&replacement_incarnation, 100),
            Some(1)
        );
    }

    #[test]
    fn task_restart_preserves_snapshot_and_request_generations() {
        let registry = PermissionDecisionRegistry::default();
        let first = key();
        let _receiver = registry.park(first.clone()).expect("park first");
        assert!(registry.stage_metadata(&first, metadata()));
        assert_eq!(registry.activate_local(&first, 100), Some(1));
        let before_restart = registry
            .snapshot_for_publish(first.session_id, first.session_incarnation_id)
            .expect("first publication");

        assert!(
            registry
                .clear_pending_for_incarnation(first.session_id, first.session_incarnation_id)
                .changed()
        );
        let cleared = registry
            .snapshot_for_publish(first.session_id, first.session_incarnation_id)
            .expect("clear publication");
        assert!(cleared.requests.is_empty());
        assert!(cleared.generation > before_restart.generation);

        let mut after_restart = first;
        after_restart.tool_use_id = "toolu_after_restart".to_owned();
        let _receiver = registry
            .park(after_restart.clone())
            .expect("park after restart");
        assert!(registry.stage_metadata(&after_restart, metadata()));
        let request_generation = registry
            .activate_local(&after_restart, 200)
            .expect("activate after restart");
        assert!(request_generation > 1);
    }

    #[test]
    fn remote_reconnect_reset_preserves_outbound_and_request_counters() {
        let registry = PermissionDecisionRegistry::default();
        let first = key();
        let _receiver = registry.park(first.clone()).expect("park first");
        assert!(registry.stage_metadata(&first, metadata()));
        assert_eq!(registry.activate_local(&first, 100), Some(1));
        let first_publication = registry
            .snapshot_for_publish(first.session_id, first.session_incarnation_id)
            .expect("first publication");

        assert!(
            registry
                .clear_remote_for_reconnect(first.session_id, first.session_incarnation_id)
                .changed()
        );
        let mut second = first;
        second.tool_use_id = "toolu_after_reconnect".to_owned();
        let _receiver = registry.park(second.clone()).expect("park second");
        assert!(registry.stage_metadata(&second, metadata()));
        assert_eq!(registry.activate_local(&second, 200), Some(2));
        let second_publication = registry
            .snapshot_for_publish(second.session_id, second.session_incarnation_id)
            .expect("second publication");
        assert!(second_publication.generation > first_publication.generation);
    }

    #[test]
    fn remote_snapshot_atomically_replaces_and_empty_clears_current_incarnation() {
        let registry = PermissionDecisionRegistry::default();
        let key = key();
        let active = active_permission(&key, 7, 100, &metadata());
        assert!(
            registry
                .replace_remote_snapshot(
                    key.session_id,
                    key.session_incarnation_id,
                    PendingPermissionsSnapshot {
                        generation: 3,
                        requests: vec![active],
                    },
                )
                .changed()
        );
        assert_eq!(registry.snapshot().requests.len(), 1);
        assert!(
            !registry
                .replace_remote_snapshot(
                    key.session_id,
                    key.session_incarnation_id,
                    PendingPermissionsSnapshot {
                        generation: 2,
                        requests: Vec::new(),
                    },
                )
                .changed()
        );
        assert_eq!(registry.snapshot().requests.len(), 1);
        assert!(
            registry
                .replace_remote_snapshot(
                    key.session_id,
                    key.session_incarnation_id,
                    PendingPermissionsSnapshot {
                        generation: 4,
                        requests: Vec::new(),
                    },
                )
                .changed()
        );
        assert!(registry.snapshot().requests.is_empty());
    }
}
