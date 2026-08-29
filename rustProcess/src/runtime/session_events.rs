use crate::TerminalEvent;
use crate::agent_intel::lifecycle::AgentIntelLifecycleCtx;
use crate::session_runtime::events::RuntimeSessionEvent;
use kodosi_domain::ids::SessionId;

use super::Runtime;

mod side_effects;

impl Runtime {
    fn accepts_remote_permission_result(&mut self, event: &RuntimeSessionEvent) -> bool {
        let RuntimeSessionEvent::RemoteActionResult {
            id,
            action_id,
            request_id,
            request_generation,
            status,
            relay_generation,
        } = event
        else {
            return true;
        };
        let Some(account_user_id) = self.state.identity.auth.subject_string() else {
            return request_generation.is_none();
        };
        let is_permission =
            self.remote_permission_actions
                .contains_action(&account_user_id, *id, action_id);
        if !is_permission {
            return request_generation.is_none();
        }
        let (Some(request_id), Some(request_generation)) =
            (request_id.as_deref(), *request_generation)
        else {
            self.state.record_log(format!(
                "{} ignored permission result without its exact request tuple",
                id.short()
            ));
            return false;
        };
        if request_generation == 0
            || !self
                .state
                .remote
                .session_relays
                .is_current_generation(*id, *relay_generation)
        {
            return false;
        }
        let owner_confirmed = self.remote_permission_actions.is_owner_confirmed_exact(
            &account_user_id,
            *id,
            action_id,
            request_id,
            request_generation,
        );
        match status {
            kodosi_domain::lifecycle::RemoteActionStatus::Busy => {
                !owner_confirmed
                    && self.remote_permission_actions.matches_result(
                        &account_user_id,
                        *id,
                        action_id,
                        request_id,
                        request_generation,
                    )
            }
            kodosi_domain::lifecycle::RemoteActionStatus::Accepted
            | kodosi_domain::lifecycle::RemoteActionStatus::Duplicate => {
                match self.remote_permission_actions.mark_owner_confirmed_exact(
                    &account_user_id,
                    *id,
                    action_id,
                    request_id,
                    request_generation,
                ) {
                    Ok(marked) => marked,
                    Err(error) => {
                        self.state.record_log(format!(
                            "{} could not confirm remote permission action {}: {error}",
                            id.short(),
                            action_id
                        ));
                        false
                    }
                }
            }
            kodosi_domain::lifecycle::RemoteActionStatus::Rejected => {
                if owner_confirmed {
                    return false;
                }
                match self.remote_permission_actions.complete_exact(
                    &account_user_id,
                    *id,
                    action_id,
                    request_id,
                    request_generation,
                ) {
                    Ok(Some(_)) => true,
                    Ok(None) => false,
                    Err(error) => {
                        self.state.record_log(format!(
                            "{} could not retire remote permission action {}: {error}",
                            id.short(),
                            action_id
                        ));
                        false
                    }
                }
            }
        }
    }

    fn queue_remote_resize_rejection(&mut self, pending: super::PendingRemoteResize, reason: &str) {
        self.state
            .runtime_outbox
            .queue_terminal_control(TerminalEvent::ResizeRejected {
                session_id: pending.session_id.to_string(),
                identity: pending.identity,
                reason: reason.to_owned(),
            });
    }

    pub(crate) fn reject_pending_remote_resizes(
        &mut self,
        session_id: SessionId,
        relay_generation: Option<u64>,
        reason: &str,
    ) {
        let retired = self
            .pending_remote_resizes
            .extract_if(|(id, _), pending| {
                *id == session_id
                    && relay_generation
                        .is_none_or(|generation| pending.relay_generation == generation)
            })
            .collect::<Vec<_>>();
        for ((_, _request_id), pending) in retired {
            self.queue_remote_resize_rejection(pending, reason);
        }
    }

    pub(crate) fn clear_pending_remote_resizes(&mut self) {
        self.pending_remote_resizes.clear();
    }

    fn complete_pending_remote_resize(&mut self, event: &RuntimeSessionEvent) -> bool {
        let RuntimeSessionEvent::RemoteActionResult {
            id,
            action_id,
            request_id,
            request_generation: _,
            status,
            relay_generation,
        } = event
        else {
            return false;
        };
        let key = (*id, action_id.clone());
        let Some(pending) = self.pending_remote_resizes.get(&key) else {
            return request_id.as_deref() == Some(action_id.as_str());
        };
        if request_id.as_deref() != Some(action_id.as_str()) {
            self.state.record_log(format!(
                "{} ignored remote resize result with mismatched request correlation",
                id.short()
            ));
            return true;
        }
        if pending.relay_generation != *relay_generation {
            self.state.record_log(format!(
                "{} ignored remote resize result from relay generation {}",
                id.short(),
                relay_generation
            ));
            return true;
        }
        let Some(pending) = self.pending_remote_resizes.remove(&key) else {
            return true;
        };
        let event = match status {
            kodosi_domain::lifecycle::RemoteActionStatus::Accepted
            | kodosi_domain::lifecycle::RemoteActionStatus::Duplicate => {
                TerminalEvent::ResizeApplied {
                    session_id: pending.session_id.to_string(),
                    identity: pending.identity,
                }
            }
            kodosi_domain::lifecycle::RemoteActionStatus::Busy
            | kodosi_domain::lifecycle::RemoteActionStatus::Rejected => {
                self.queue_remote_resize_rejection(
                    pending,
                    "the owner could not apply this terminal size",
                );
                return true;
            }
        };
        self.state.runtime_outbox.queue_terminal_control(event);
        true
    }

    fn retire_self_device_link_result(&mut self, event: &RuntimeSessionEvent) {
        let RuntimeSessionEvent::DeviceLinkSelfResolved {
            user_code, outcome, ..
        } = event
        else {
            return;
        };
        if self
            .state
            .identity
            .account_runtimes
            .retire_self_device_link(user_code)
            && *outcome == kodosi_domain::device_link::SelfDeviceLinkOutcome::Approved
        {
            self.device_enrollment_satisfied = true;
            self.device_enrollment_retry_after = None;
            self.last_maintenance_ran = None;
        }
    }

    fn retire_stale_host_delivery_ack(&mut self, event: &RuntimeSessionEvent) -> bool {
        match event {
            RuntimeSessionEvent::HostActionResultMailboxAck {
                origin,
                result,
                delivered: false,
            } => {
                self.action_results_in_flight.remove(&(
                    origin.session_id,
                    result.action_id.clone(),
                    result.requester_user_id.clone(),
                    origin.relay_generation,
                ));
                true
            }
            RuntimeSessionEvent::HostSemanticReceiptMailboxAck {
                origin,
                request_id,
                delivered: false,
                ..
            } => {
                self.semantic_receipts_in_flight.remove(&(
                    origin.session_id,
                    *request_id,
                    origin.relay_generation,
                ));
                true
            }
            _ => false,
        }
    }

    fn fence_pre_reduction_lifecycle(&mut self, event: &RuntimeSessionEvent) {
        if let RuntimeSessionEvent::UserDeviceListChanged { user_id, .. } = event
            && self.state.identity.auth.subject_string().as_deref() == Some(user_id.as_str())
        {
            let hosted = self.state.sharing.shared_sessions.ids().collect::<Vec<_>>();
            self.cancel_relay_preparations_for_sessions(hosted);
        }
        if let RuntimeSessionEvent::Stopped { origin, .. }
        | RuntimeSessionEvent::Failed { origin, .. } = event
        {
            let id = origin.session_id;
            self.cancel_relay_prepare_for_session(id);
            self.cancel_share_transition_for_session(
                id,
                "local session ended during share transition",
            );
            self.retire_access_mutations_for_session(
                id,
                "local session ended before the access change settled",
            );
        }
    }

    fn accepts_local_coordinator_event(&self, event: &RuntimeSessionEvent) -> bool {
        let Some(origin) = event.local_coordinator_origin() else {
            return true;
        };
        let accepted = self
            .state
            .local
            .sessions
            .record(origin.session_id)
            .is_some_and(|record| record.local_incarnation_id == origin.local_incarnation_id);
        if !accepted {
            tracing::debug!(
                session_id = %origin.session_id,
                event_local_incarnation_id = %origin.local_incarnation_id,
                "ignored event from a retired local session coordinator"
            );
        }
        accepted
    }

    pub(crate) fn handle_session_event(&mut self, event: RuntimeSessionEvent) -> bool {
        if let Some(origin) = event.account_event_origin()
            && !self.state.identity.accepts_account_event(origin)
        {
            event.reject_reply();
            tracing::debug!(
                event_account_user_id = %origin.account_user_id,
                event_account_epoch = ?origin.epoch,
                current_account_epoch = ?self.state.identity.account_epoch(),
                "ignored event from a prior authenticated account epoch"
            );
            return false;
        }

        if !self.accepts_local_coordinator_event(&event) {
            return false;
        }

        self.retire_self_device_link_result(&event);
        self.fence_pre_reduction_lifecycle(&event);

        if let Some(origin) = event.host_relay_event_origin()
            && !self
                .state
                .sharing
                .host_relays
                .is_current_generation(origin.session_id, origin.relay_generation)
        {
            if self.retire_stale_host_delivery_ack(&event) {
                return false;
            }
            event.reject_reply();
            tracing::debug!(
                session_id = %origin.session_id,
                relay_generation = origin.relay_generation,
                "ignored event from a retired host relay generation"
            );
            return false;
        }

        if let Some((id, local_incarnation_id)) = event.local_hook_origin()
            && self
                .state
                .local
                .sessions
                .record(id)
                .is_none_or(|record| record.local_incarnation_id != local_incarnation_id)
        {
            event.reject_reply();
            tracing::debug!(
                session_id = %id,
                event_local_incarnation_id = %local_incarnation_id,
                "ignored hook event from a retired local session incarnation"
            );
            return false;
        }

        if !self.accepts_remote_permission_result(&event) {
            return false;
        }

        if self.complete_pending_remote_resize(&event) {
            return false;
        }

        if let Some(force_snapshot) = self.apply_immediate_session_event_side_effects(&event) {
            return force_snapshot;
        }

        let descriptor_candidate = event.local_descriptor_candidate();
        let effects = self.state.apply_session_event(event);
        if let Some(request) = effects.project_discovery {
            self.spawn_project_discovery(request);
        }
        if let Some(session_id) = effects.agent_intel_teardown {
            let incarnation_id = AgentIntelLifecycleCtx {
                intel_state: &mut self.state.agent_intel,
                local_sessions: &self.state.local,
                session_events: &self.session_events_tx,
                outbox: &mut self.state.runtime_outbox,
            }
            .restart_task(session_id);
            if let Some(incarnation_id) = incarnation_id {
                self.publish_pending_permissions(session_id, incarnation_id);
            }
        }
        if let Some((session_id, agent_name)) = effects.agent_intel_spawn {
            AgentIntelLifecycleCtx {
                intel_state: &mut self.state.agent_intel,
                local_sessions: &self.state.local,
                session_events: &self.session_events_tx,
                outbox: &mut self.state.runtime_outbox,
            }
            .spawn(session_id, &agent_name);
        }
        if let Some(id) = descriptor_candidate
            && let Some(record) = self.state.local.sessions.record(id)
            && let Err(error) = self.local_catalog.persist_record(record)
        {
            self.state.record_log(format!(
                "{} failed to persist local session descriptor: {error}",
                id.short()
            ));
        }
        self.drain_semantic_relay_receipts();
        self.drain_owner_action_results();
        self.drain_ready_deletes();
        effects.force_snapshot
    }
}
