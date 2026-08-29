use crate::{
    agent_intel::{lifecycle::AgentIntelLifecycleCtx, mode::AgentIntelModeCtx},
    host_protocol::TerminalEvent,
    session_runtime::events::RuntimeSessionEvent,
    terminal_transport::{TerminalCloseReason, TerminalControlFrame},
};
use kodosi_domain::{ids::SessionId, lifecycle::ConnectionState, session::SessionState};

fn relay_semantic_mode(
    mode: kodosi_backend_client::session_relay::wire::RelaySemanticMode,
) -> crate::host_protocol::SemanticSendMode {
    match mode {
        kodosi_backend_client::session_relay::wire::RelaySemanticMode::Queue => {
            crate::host_protocol::SemanticSendMode::Queue
        }
        kodosi_backend_client::session_relay::wire::RelaySemanticMode::Steer => {
            crate::host_protocol::SemanticSendMode::Steer
        }
        kodosi_backend_client::session_relay::wire::RelaySemanticMode::StopAndSend => {
            crate::host_protocol::SemanticSendMode::StopAndSend
        }
    }
}

fn remote_semantic_transition(
    outcome: kodosi_backend_client::session_relay::wire::RelaySemanticOutcome,
) -> crate::host_protocol::SteerTransition {
    match outcome {
        kodosi_backend_client::session_relay::wire::RelaySemanticOutcome::Injected => {
            crate::host_protocol::SteerTransition::Injected
        }
        kodosi_backend_client::session_relay::wire::RelaySemanticOutcome::Cancelled => {
            crate::host_protocol::SteerTransition::Cancelled
        }
        kodosi_backend_client::session_relay::wire::RelaySemanticOutcome::DeliveryUnknown => {
            crate::host_protocol::SteerTransition::DeliveryUnknown
        }
    }
}

use super::super::Runtime;

impl Runtime {
    #[expect(clippy::too_many_lines, reason = "wrapper composition by design")]
    pub(crate) fn apply_immediate_session_event_side_effects(
        &mut self,
        event: &RuntimeSessionEvent,
    ) -> Option<bool> {
        match event {
            RuntimeSessionEvent::BackendAccessInvalid { reason, .. }
            | RuntimeSessionEvent::HostRelayBackendAccessInvalid { reason, .. } => {
                self.handle_backend_access_invalid(reason);
                Some(false)
            }
            RuntimeSessionEvent::HostActionResultMailboxAck {
                origin,
                result,
                delivered,
            } => {
                self.action_results_in_flight.remove(&(
                    origin.session_id,
                    result.action_id.clone(),
                    result.requester_user_id.clone(),
                    origin.relay_generation,
                ));
                let acknowledged = *delivered
                    && self
                        .owner_action_results
                        .acknowledge(result)
                        .unwrap_or(false);
                Some(acknowledged)
            }
            RuntimeSessionEvent::HostSemanticReceiptMailboxAck {
                origin,
                request_id,
                incarnation_id,
                delivered,
                acknowledge_locally,
            } => {
                self.semantic_receipts_in_flight.remove(&(
                    origin.session_id,
                    *request_id,
                    origin.relay_generation,
                ));
                let acknowledged = *delivered
                    && (!*acknowledge_locally
                        || self
                            .state
                            .steering
                            .acknowledge_relay_receipt(
                                &origin.account_origin.account_user_id,
                                *request_id,
                                origin.session_id,
                                *incarnation_id,
                            )
                            .unwrap_or(false));
                Some(acknowledged)
            }
            RuntimeSessionEvent::HostSemanticSend {
                origin,
                request_id,
                incarnation_id,
                mode,
                payload_sha256,
                text,
                requester_user_id,
                requester_device_id,
                reply,
            } => {
                let local_incarnation_id = self
                    .state
                    .sharing
                    .shared_sessions
                    .get(origin.session_id)
                    .filter(|shared| *shared.backend_incarnation_id() == *incarnation_id)
                    .and_then(|_| {
                        self.state
                            .local
                            .sessions
                            .record(origin.session_id)
                            .map(|record| record.local_incarnation_id)
                    });
                let accepted = requester_user_id == &origin.account_origin.account_user_id
                    && crate::runtime::steering::payload_sha256(text) == *payload_sha256
                    && local_incarnation_id.is_some_and(|local_incarnation_id| {
                        match self.semantic_send_now_from_relay(
                            *request_id,
                            origin.session_id,
                            local_incarnation_id,
                            relay_semantic_mode(*mode),
                            text.clone(),
                            requester_device_id,
                        ) {
                            Ok(entry) => {
                                if !matches!(
                                    entry.delivery_state,
                                    crate::host_protocol::SteerDeliveryState::Preparing
                                        | crate::host_protocol::SteerDeliveryState::Queued
                                ) {
                                    let _ = self.resend_semantic_relay_receipt(
                                        requester_user_id,
                                        *request_id,
                                        origin.session_id,
                                        local_incarnation_id,
                                        relay_semantic_mode(*mode),
                                        payload_sha256,
                                        requester_device_id,
                                    );
                                }
                                true
                            }
                            Err(_) => false,
                        }
                    });
                reply.complete(accepted);
                Some(accepted)
            }
            RuntimeSessionEvent::HostSemanticCancel {
                origin,
                request_id,
                incarnation_id,
                mode,
                payload_sha256,
                requester_user_id,
                requester_device_id,
                reply,
            } => {
                let local_incarnation_id = self
                    .state
                    .sharing
                    .shared_sessions
                    .get(origin.session_id)
                    .filter(|shared| *shared.backend_incarnation_id() == *incarnation_id)
                    .and_then(|_| {
                        self.state
                            .local
                            .sessions
                            .record(origin.session_id)
                            .map(|record| record.local_incarnation_id)
                    });
                let existing = self
                    .state
                    .steering
                    .pending_request(requester_user_id, &request_id.to_string())
                    .cloned();
                let accepted = requester_user_id == &origin.account_origin.account_user_id
                    && self
                        .state
                        .steering
                        .relay_requester_device(requester_user_id, &request_id.to_string())
                        == Some(requester_device_id.as_str())
                    && local_incarnation_id.is_some_and(|local_incarnation_id| {
                        existing.as_ref().is_some_and(|entry| {
                            entry.session_id == origin.session_id.to_string()
                                && entry.session_incarnation_id == local_incarnation_id.to_string()
                                && entry.mode == relay_semantic_mode(*mode)
                                && crate::runtime::steering::payload_sha256(&entry.text)
                                    == *payload_sha256
                        })
                    })
                    && existing.as_ref().is_some_and(|entry| {
                        self.cancel_steer_with_origin(
                            origin.session_id,
                            &entry.steer_id,
                            crate::runtime::steering::SemanticSendOrigin::Relay,
                        )
                        .is_ok()
                    });
                reply.complete(accepted);
                Some(accepted)
            }
            RuntimeSessionEvent::TerminalOutputObserved { origin, data } => {
                self.forward_terminal_output_to_agent_intel(origin.session_id, data);
                None
            }
            RuntimeSessionEvent::RemoteSemanticReceipt {
                id,
                receipt,
                persistence,
                relay_generation,
            } => {
                if !self
                    .state
                    .remote
                    .session_relays
                    .is_current_generation(*id, *relay_generation)
                {
                    persistence.complete(false);
                    return Some(false);
                }
                let Some(account_user_id) = self.state.identity.auth.subject_string() else {
                    persistence.complete(false);
                    return Some(false);
                };
                match self.remote_semantics.complete(&account_user_id, receipt) {
                    Ok(Some(completed)) => {
                        let entry = crate::host_protocol::SteerQueueEntry {
                            steer_id: completed.request.request_id.to_string(),
                            account_user_id: completed.request.account_user_id,
                            request_id: completed.request.request_id.to_string(),
                            session_incarnation_id: completed.request.incarnation_id.to_string(),
                            mode: match completed.request.mode {
                                kodosi_backend_client::session_relay::wire::RelaySemanticMode::Queue => crate::host_protocol::SemanticSendMode::Queue,
                                kodosi_backend_client::session_relay::wire::RelaySemanticMode::Steer => crate::host_protocol::SemanticSendMode::Steer,
                                kodosi_backend_client::session_relay::wire::RelaySemanticMode::StopAndSend => crate::host_protocol::SemanticSendMode::StopAndSend,
                            },
                            session_id: completed.request.session_id,
                            text: completed.request.text,
                            queued_at_ms: 0,
                            delivery_state: match completed.outcome {
                                kodosi_backend_client::session_relay::wire::RelaySemanticOutcome::Injected => crate::host_protocol::SteerDeliveryState::Injected,
                                kodosi_backend_client::session_relay::wire::RelaySemanticOutcome::Cancelled => crate::host_protocol::SteerDeliveryState::Cancelled,
                                kodosi_backend_client::session_relay::wire::RelaySemanticOutcome::DeliveryUnknown => crate::host_protocol::SteerDeliveryState::DeliveryUnknown,
                            },
                            at_tool_use_id: None,
                        };
                        self.state.runtime_outbox.queue_agent_intel(
                            crate::AgentIntelEvent::SteerState {
                                entry,
                                transition: remote_semantic_transition(completed.outcome),
                                message: None,
                            },
                        );
                        persistence.complete(true);
                        Some(true)
                    }
                    Ok(None) => {
                        persistence.complete(false);
                        Some(false)
                    }
                    Err(error) => {
                        persistence.complete(false);
                        self.state.record_log(format!(
                            "{} verified remote semantic receipt could not persist: {error}",
                            id.short()
                        ));
                        Some(false)
                    }
                }
            }
            RuntimeSessionEvent::RemoteCheckpoint {
                id,
                next_sequence,
                checkpoint,
                application,
                relay_generation,
                ..
            } => {
                let accepted = self
                    .state
                    .remote
                    .session_relays
                    .is_current_generation(*id, *relay_generation)
                    && self.handle_remote_checkpoint(*id, checkpoint.clone(), *next_sequence);
                application.complete(accepted);
                None
            }
            RuntimeSessionEvent::RemoteRawBatch {
                id,
                first_sequence,
                next_sequence,
                chunks,
                relay_generation,
            } => {
                if self
                    .state
                    .remote
                    .session_relays
                    .is_current_generation(*id, *relay_generation)
                {
                    self.handle_remote_raw_batch(*id, *first_sequence, *next_sequence, chunks);
                }
                None
            }
            RuntimeSessionEvent::RemotePlainPresentation {
                id,
                presentation,
                relay_generation,
                ..
            } => {
                if self
                    .state
                    .remote
                    .session_relays
                    .is_current_generation(*id, *relay_generation)
                {
                    self.remote_terminal
                        .apply_presentation(*id, presentation.clone());
                }
                None
            }
            RuntimeSessionEvent::AgentIntelSnapshot {
                id,
                local_incarnation_id,
                generation,
                payload,
            } => {
                if self
                    .state
                    .local
                    .sessions
                    .record(*id)
                    .is_none_or(|record| record.local_incarnation_id != *local_incarnation_id)
                {
                    tracing::debug!(
                        session_id = %id,
                        incarnation_id = %local_incarnation_id,
                        "ignored snapshot from a retired agent intel task"
                    );
                    return None;
                }
                if !self
                    .state
                    .agent_intel
                    .registry
                    .is_current_generation(*id, *generation)
                {
                    tracing::debug!(
                        session_id = %id,
                        %generation,
                        "ignored snapshot from a replaced agent intel task"
                    );
                    return None;
                }
                AgentIntelModeCtx {
                    intel_state: &mut self.state.agent_intel,
                    local_sessions: &mut self.state.local,
                    outbox: &mut self.state.runtime_outbox,
                }
                .handle_snapshot(*id, payload);
                None
            }
            RuntimeSessionEvent::RoomAgentDeliveryState {
                id,
                local_incarnation_id,
                state,
                event_id,
                detail,
            } => {
                if self.state.identity.current_account_event_origin().is_none() {
                    tracing::debug!(
                        session_id = %id,
                        "ignored room agent delivery without current account authority"
                    );
                    return None;
                }
                if self
                    .state
                    .local
                    .sessions
                    .record(*id)
                    .is_some_and(|record| record.local_incarnation_id == *local_incarnation_id)
                {
                    self.state.runtime_outbox.queue_room(
                        crate::host_protocol::RoomEvent::AgentDelivery {
                            session_id: id.to_string(),
                            session_incarnation_id: local_incarnation_id.to_string(),
                            state: *state,
                            event_id: event_id.clone(),
                            detail: detail.clone(),
                        },
                    );
                }
                None
            }
            RuntimeSessionEvent::AgentBoundary {
                id,
                local_incarnation_id,
                kind,
                tool_use_id,
            } => {
                if self
                    .state
                    .local
                    .sessions
                    .record(*id)
                    .is_none_or(|record| record.local_incarnation_id != *local_incarnation_id)
                {
                    return None;
                }
                let scope = crate::runtime::steering::semantic_scope(
                    self.state.identity.auth.subject_string(),
                );
                let result = match kind {
                    crate::session_runtime::events::AgentBoundaryKind::Tool => self
                        .state
                        .steering
                        .note_tool_boundary(&scope, *id, tool_use_id.clone()),
                };
                if let Err(error) = result {
                    self.state.record_log(format!(
                        "{} could not persist an official agent boundary: {error}",
                        id.short()
                    ));
                }
                None
            }
            RuntimeSessionEvent::AgentTurnStateChanged {
                id,
                local_incarnation_id,
                state,
            } => {
                if self
                    .state
                    .local
                    .sessions
                    .record(*id)
                    .is_none_or(|record| record.local_incarnation_id != *local_incarnation_id)
                {
                    return None;
                }
                let scope = crate::runtime::steering::semantic_scope(
                    self.state.identity.auth.subject_string(),
                );
                let state = match state {
                    crate::session_runtime::events::AgentTurnState::Running => {
                        crate::runtime::steering::SemanticTurnState::Running
                    }
                    crate::session_runtime::events::AgentTurnState::Idle => {
                        crate::runtime::steering::SemanticTurnState::Idle
                    }
                };
                if let Err(error) = self.state.steering.note_semantic_turn_state(
                    &scope,
                    *id,
                    *local_incarnation_id,
                    state,
                ) {
                    self.state.record_log(format!(
                        "{} could not persist official agent turn state: {error}",
                        id.short()
                    ));
                }
                None
            }
            RuntimeSessionEvent::RemoteSessionConnectionChanged {
                id,
                status,
                relay_generation,
                ..
            } => {
                if self.track_remote_session_connection(*id, *relay_generation, *status) {
                    if *status == ConnectionState::Connected {
                        crate::runtime::remote_sessions::replay_remote_semantics(self, *id);
                        crate::runtime::remote_sessions::replay_remote_permission_actions(
                            self, *id,
                        );
                        crate::runtime::remote_sessions::reassert_focus(self, *id);
                    } else {
                        self.reject_pending_remote_resizes(
                            *id,
                            Some(*relay_generation),
                            "the remote terminal disconnected before applying this size",
                        );
                        if *status == ConnectionState::Offline {
                            if let Some(incarnation_id) = self.session_incarnation_id(*id) {
                                self.clear_remote_pending_permissions(
                                    *id,
                                    incarnation_id,
                                    RemotePendingClearMode::RetryableRelay,
                                );
                            }
                            self.detach_remote_session_relay(*id, *relay_generation);
                        }
                    }
                }
                None
            }
            RuntimeSessionEvent::RemoteAccessRevoked {
                id,
                relay_generation,
            } => {
                if self
                    .state
                    .remote
                    .session_relays
                    .is_current_generation(*id, *relay_generation)
                {
                    if let Some(incarnation_id) = self.session_incarnation_id(*id) {
                        self.clear_remote_pending_permissions(
                            *id,
                            incarnation_id,
                            RemotePendingClearMode::AccessRevoked,
                        );
                    }
                    self.state.apply_remote_access_revoked(*id);
                    self.reject_pending_remote_resizes(
                        *id,
                        Some(*relay_generation),
                        "remote terminal access ended before applying this size",
                    );
                    self.detach_remote_session_relay(*id, *relay_generation);
                }
                Some(false)
            }
            RuntimeSessionEvent::RemoteStateChanged {
                id,
                state,
                relay_generation,
            } => {
                if matches!(state, SessionState::Stopped | SessionState::Failed)
                    && self
                        .state
                        .remote
                        .session_relays
                        .is_current_generation(*id, *relay_generation)
                    && let Some(incarnation_id) = self.session_incarnation_id(*id)
                {
                    self.clear_remote_pending_permissions(
                        *id,
                        incarnation_id,
                        RemotePendingClearMode::RetireIncarnation,
                    );
                    self.state.apply_remote_state_changed(*id, *state);
                    self.reject_pending_remote_resizes(
                        *id,
                        Some(*relay_generation),
                        "the remote terminal ended before applying this size",
                    );
                    self.detach_remote_session_relay(*id, *relay_generation);
                }
                None
            }
            RuntimeSessionEvent::RemoteSessionRelayExited {
                id,
                relay_generation,
            } => {
                if self
                    .state
                    .remote
                    .session_relays
                    .is_current_generation(*id, *relay_generation)
                {
                    if let Some(incarnation_id) = self.session_incarnation_id(*id) {
                        self.clear_remote_pending_permissions(
                            *id,
                            incarnation_id,
                            RemotePendingClearMode::RetryableRelay,
                        );
                    }
                    self.reject_pending_remote_resizes(
                        *id,
                        Some(*relay_generation),
                        "the remote terminal closed before applying this size",
                    );
                    self.detach_remote_session_relay(*id, *relay_generation);
                }
                None
            }
            RuntimeSessionEvent::PendingPermissionRequest {
                id,
                local_incarnation_id,
                tool_use_id,
                ..
            } => {
                let key = crate::agent_intel::permission_decision_registry::PendingKey {
                    session_id: *id,
                    session_incarnation_id: *local_incarnation_id,
                    tool_use_id: tool_use_id.clone(),
                };
                let Some(_request_generation) = self
                    .state
                    .agent_intel
                    .permission_decisions
                    .activate_local(&key, current_epoch_ms())
                else {
                    self.state.record_log(format!(
                        "{} pending permission activation failed",
                        id.short()
                    ));
                    return Some(false);
                };
                self.state
                    .runtime_outbox
                    .queue_pending_permissions_snapshot(
                        self.state.agent_intel.permission_decisions.snapshot(),
                    );
                self.publish_pending_permissions(*id, *local_incarnation_id);
                Some(false)
            }
            RuntimeSessionEvent::RemoteControlTrustEstablished {
                id,
                incarnation_id,
                owner_user_id,
                signer_device_id,
                signer_public_key,
                device_list_generation,
                identity_fingerprint,
                relay_generation,
            } => {
                if !self
                    .state
                    .remote
                    .session_relays
                    .is_current_generation(*id, *relay_generation)
                {
                    return Some(false);
                }
                if let Err(error) = self.remote_semantics.record_signer_snapshot(
                    crate::runtime::remote_semantics::RemoteSemanticSignerSnapshot {
                        account_user_id: self
                            .state
                            .identity
                            .auth
                            .subject_string()
                            .unwrap_or_default(),
                        session_id: id.to_string(),
                        incarnation_id: *incarnation_id,
                        owner_user_id: owner_user_id.clone(),
                        owner_device_id: signer_device_id.clone(),
                        owner_signing_public_key: signer_public_key.clone(),
                        device_list_generation: *device_list_generation,
                        identity_fingerprint: *identity_fingerprint,
                    },
                ) {
                    self.state.record_log(format!(
                        "{} remote semantic signer snapshot could not persist: {error}",
                        id.short()
                    ));
                }
                Some(false)
            }
            RuntimeSessionEvent::RemotePendingPermissionsSnapshot {
                id,
                incarnation_id,
                generation,
                snapshot,
                relay_generation,
            } => {
                if !self.accepts_remote_pending_permissions_snapshot(
                    *id,
                    *incarnation_id,
                    *relay_generation,
                ) {
                    return Some(false);
                }
                let Ok(decoded) =
                    serde_json::from_value::<crate::PendingPermissionsSnapshot>(snapshot.clone())
                else {
                    return Some(false);
                };
                if decoded.generation != *generation {
                    return Some(false);
                }
                if self
                    .state
                    .agent_intel
                    .permission_decisions
                    .replace_remote_snapshot(*id, *incarnation_id, decoded)
                    .changed()
                {
                    self.state
                        .runtime_outbox
                        .queue_pending_permissions_snapshot(
                            self.state.agent_intel.permission_decisions.snapshot(),
                        );
                    self.reconcile_remote_permission_actions(*id, *incarnation_id);
                }
                Some(false)
            }
            RuntimeSessionEvent::PermissionResolved {
                id,
                local_incarnation_id,
                tool_use_id,
                ..
            } => {
                let key = crate::agent_intel::permission_decision_registry::PendingKey {
                    session_id: *id,
                    session_incarnation_id: *local_incarnation_id,
                    tool_use_id: tool_use_id.clone(),
                };
                if let Some(request_generation) = self
                    .state
                    .agent_intel
                    .permission_decisions
                    .request_generation(&key)
                    && self
                        .state
                        .agent_intel
                        .permission_decisions
                        .complete(&key, request_generation)
                        .changed()
                {
                    self.state
                        .runtime_outbox
                        .queue_pending_permissions_snapshot(
                            self.state.agent_intel.permission_decisions.snapshot(),
                        );
                    self.publish_pending_permissions(*id, *local_incarnation_id);
                }
                Some(false)
            }
            RuntimeSessionEvent::RemoteActionCompleted {
                origin,
                incarnation_id,
                action_id,
                request_id,
                requester_user_id,
                requester_device_id,
                accepted,
            } => {
                let result = crate::runtime::action_results::OwnerActionResult {
                    account_user_id: origin.account_origin.account_user_id.clone(),
                    session_id: origin.session_id.to_string(),
                    incarnation_id: *incarnation_id,
                    action_id: action_id.clone(),
                    request_id: request_id.clone(),
                    request_generation: 1,
                    requester_user_id: requester_user_id.clone(),
                    requester_device_id: requester_device_id.clone(),
                    accepted: Some(*accepted),
                };
                if let Err(error) = self.owner_action_results.admit(result) {
                    self.state.record_log(format!(
                        "{} remote action result admission failed: {error}",
                        origin.session_id.short()
                    ));
                    return Some(false);
                }
                self.drain_owner_action_results();
                Some(*accepted)
            }
            RuntimeSessionEvent::RemotePermissionDecision {
                origin,
                incarnation_id,
                action_id,
                request_id,
                request_generation,
                decision,
                decider_user_id,
                decider_device_id,
                reply,
            } => {
                let admission = crate::runtime::action_results::OwnerActionResult {
                    account_user_id: origin.account_origin.account_user_id.clone(),
                    session_id: origin.session_id.to_string(),
                    incarnation_id: *incarnation_id,
                    action_id: action_id.clone(),
                    request_id: request_id.clone(),
                    request_generation: *request_generation,
                    requester_user_id: decider_user_id.clone(),
                    requester_device_id: decider_device_id.clone().unwrap_or_default(),
                    accepted: None,
                };
                match self.owner_action_results.admit(admission) {
                    Ok(crate::runtime::action_results::ActionAdmission::Terminal(accepted)) => {
                        reply.complete(accepted);
                        self.drain_owner_action_results();
                        return Some(accepted);
                    }
                    Ok(
                        crate::runtime::action_results::ActionAdmission::Pending
                        | crate::runtime::action_results::ActionAdmission::New,
                    ) => {}
                    Err(error) => {
                        self.state.record_log(format!(
                            "{} remote permission action admission failed: {error}",
                            origin.session_id.short()
                        ));
                        reply.complete(false);
                        return Some(false);
                    }
                }
                let accepted =
                    self.resolve_remote_permission_decision(RemotePermissionDecisionInput {
                        session_id: origin.session_id,
                        session_incarnation_id: *incarnation_id,
                        action_id,
                        request_id,
                        request_generation: *request_generation,
                        decision,
                        decider_user_id,
                    });
                reply.complete(accepted);
                self.drain_owner_action_results();
                Some(accepted)
            }
            RuntimeSessionEvent::HostDemand {
                origin, required, ..
            } => {
                if *required
                    && let Some(incarnation_id) = self.session_incarnation_id(origin.session_id)
                {
                    self.publish_pending_permissions(origin.session_id, incarnation_id);
                }
                None
            }
            RuntimeSessionEvent::HostRelayBackendRestarted { origin } => {
                if let Some(incarnation_id) = self.session_incarnation_id(origin.session_id) {
                    self.publish_pending_permissions(origin.session_id, incarnation_id);
                }
                None
            }
            RuntimeSessionEvent::ClipboardUpdate { origin, text } => {
                self.update_system_clipboard(origin.session_id, text);
                None
            }
            RuntimeSessionEvent::TerminalBell { origin } => {
                self.queue_terminal_bell(origin.session_id);
                Some(false)
            }
            RuntimeSessionEvent::TerminalTitleChanged { origin, title } => {
                self.apply_terminal_title(origin.session_id, title.clone());
                Some(true)
            }
            RuntimeSessionEvent::TerminalNotification {
                origin,
                title,
                body,
            } => {
                self.queue_terminal_notification(origin.session_id, title.clone(), body.clone());
                None
            }
            RuntimeSessionEvent::Stopped { origin, .. } => {
                let id = origin.session_id;
                self.resolve_pending_steers_for_ended_session(id);
                AgentIntelLifecycleCtx {
                    intel_state: &mut self.state.agent_intel,
                    local_sessions: &self.state.local,
                    session_events: &self.session_events_tx,
                    outbox: &mut self.state.runtime_outbox,
                }
                .teardown_session(id);
                self.terminal_hub
                    .end_local_incarnation(*origin, &TerminalCloseReason::SessionEnded);

                self.client_focus.forget(id);
                None
            }
            RuntimeSessionEvent::Failed { origin, message } => {
                let id = origin.session_id;

                self.resolve_pending_steers_for_ended_session(id);
                self.terminal_hub
                    .end_local_incarnation(*origin, &TerminalCloseReason::IoError(message.clone()));
                self.client_focus.forget(id);
                None
            }
            RuntimeSessionEvent::CaptureMetadata { origin, size, .. } => {
                let id = origin.session_id;

                let at_sequence = self.terminal_hub.next_sequence(id).unwrap_or(0);
                self.terminal_hub.broadcast_control(
                    id,
                    &TerminalControlFrame::Resize {
                        rows: size.rows(),
                        cols: size.cols(),
                        at_sequence,
                    },
                );
                None
            }
            RuntimeSessionEvent::RemoteInfo { .. }
            | RuntimeSessionEvent::RemoteLogError { .. }
            | RuntimeSessionEvent::HostRelayInfo { .. }
            | RuntimeSessionEvent::HostRelayLogError { .. }
            | RuntimeSessionEvent::DiscoveryInvalidated { .. }
            | RuntimeSessionEvent::UserDeviceListChanged { .. }
            | RuntimeSessionEvent::UserIdentityLifecycleChanged { .. }
            | RuntimeSessionEvent::HostAccessRevoked { .. }
            | RuntimeSessionEvent::HostFrameReservationExhausted { .. }
            | RuntimeSessionEvent::HostKeyDistributionRequested { .. }
            | RuntimeSessionEvent::HostKeyRotationRequired { .. }
            | RuntimeSessionEvent::RuntimeMetadata { .. }
            | RuntimeSessionEvent::WorkingDirChanged { .. }
            | RuntimeSessionEvent::ProjectDiscoveryReady { .. }
            | RuntimeSessionEvent::ProjectDiscoveryFailed { .. }
            | RuntimeSessionEvent::StateChanged { .. }
            | RuntimeSessionEvent::RemoteAccessChanged { .. }
            | RuntimeSessionEvent::RemoteAccessStateChanged { .. }
            | RuntimeSessionEvent::RemoteActionResult { .. }
            | RuntimeSessionEvent::DeviceLinkSnapshot { .. }
            | RuntimeSessionEvent::DeviceLinkRequested { .. }
            | RuntimeSessionEvent::DeviceLinkResolved { .. }
            | RuntimeSessionEvent::DeviceLinkSelfPending { .. }
            | RuntimeSessionEvent::DeviceLinkSelfResolved { .. } => None,
        }
    }

    fn handle_backend_access_invalid(&mut self, reason: &str) {
        if let Err(error) = crate::runtime::auth::mark_expired_from_backend(self, reason.to_owned())
        {
            self.state.record_log(error.to_string());
        }
    }

    fn forward_terminal_output_to_agent_intel(&self, id: SessionId, data: &bytes::Bytes) {
        use crate::agent_intel::TerminalIntelEnqueueOutcome;
        match self.state.agent_intel.registry.try_send_terminal(id, data) {
            TerminalIntelEnqueueOutcome::Full => {
                tracing::warn!(session = %id, "agent intelligence terminal queue full; dropped secondary batch");
            }
            TerminalIntelEnqueueOutcome::Closed => {
                tracing::debug!(session = %id, "agent intelligence terminal queue closed");
            }
            TerminalIntelEnqueueOutcome::Enqueued
            | TerminalIntelEnqueueOutcome::Unattached
            | TerminalIntelEnqueueOutcome::FullAlreadyReported => {}
        }
    }

    fn track_remote_session_connection(
        &mut self,
        id: SessionId,
        relay_generation: u64,
        status: ConnectionState,
    ) -> bool {
        self.state
            .set_session_relay_status(id, relay_generation, status)
    }

    fn detach_remote_session_relay(&mut self, id: SessionId, relay_generation: u64) {
        if !self
            .state
            .remove_detached_session_relay(id, relay_generation)
        {
            return;
        }
        self.remote_terminal.forget(id);
        self.client_focus.forget(id);
        self.terminal_hub
            .end_session(id, &TerminalCloseReason::Detached);
    }

    fn handle_remote_checkpoint(
        &mut self,
        id: SessionId,
        checkpoint: kodosi_domain::terminal::TerminalCheckpointV2,
        next_sequence: u64,
    ) -> bool {
        if let Err(error) = kodosi_session::validate_terminal_checkpoint(
            &checkpoint,
            kodosi_session::TerminalHistoryPolicy::default(),
        ) {
            self.state.record_log(format!(
                "{} rejected remote semantic checkpoint before installation: {error}",
                id.short()
            ));
            return false;
        }
        if !self
            .terminal_hub
            .install_semantic_checkpoint(id, checkpoint.clone(), next_sequence)
        {
            return false;
        }
        self.remote_terminal
            .install_checkpoint(id, checkpoint, next_sequence);
        true
    }

    fn handle_remote_raw_batch(
        &mut self,
        id: SessionId,
        first_sequence: u64,
        next_sequence: u64,
        chunks: &[Vec<u8>],
    ) {
        let Some(expected_next) = first_sequence.checked_add(chunks.len() as u64) else {
            self.terminal_hub.end_session(
                id,
                &TerminalCloseReason::IoError("remote terminal sequence overflow".to_owned()),
            );
            return;
        };
        if expected_next != next_sequence {
            self.terminal_hub.end_session(
                id,
                &TerminalCloseReason::IoError("remote terminal batch boundary mismatch".to_owned()),
            );
            return;
        }
        for (offset, chunk) in chunks.iter().enumerate() {
            let Some(sequence) = first_sequence.checked_add(offset as u64) else {
                return;
            };
            if !self.terminal_hub.publish_at_sequence(
                id,
                sequence,
                bytes::Bytes::copy_from_slice(chunk),
            ) {
                self.terminal_hub.end_session(
                    id,
                    &TerminalCloseReason::IoError("remote terminal sequence gap".to_owned()),
                );
                return;
            }
        }
    }

    pub(crate) fn publish_pending_permissions(
        &mut self,
        id: SessionId,
        incarnation_id: uuid::Uuid,
    ) {
        if !self.state.sharing.host_relays.active(id) {
            return;
        }
        let Some(snapshot) = self
            .state
            .agent_intel
            .permission_decisions
            .snapshot_for_publish(id, incarnation_id)
        else {
            self.state.record_log(format!(
                "{} pending-permission generation exhausted",
                id.short()
            ));
            return;
        };
        let plaintext = match serde_json::to_vec(&snapshot) {
            Ok(plaintext) => plaintext,
            Err(error) => {
                self.state.record_log(format!(
                    "{} pending-permission snapshot encode failed: {error}",
                    id.short()
                ));
                return;
            }
        };
        if let Err(error) = self.state.sharing.host_relays.publish_pending_permissions(
            id,
            kodosi_backend_client::relay::HostRelayPendingPermissionsSnapshot {
                generation: snapshot.generation,
                incarnation_id,
                plaintext,
            },
        ) {
            self.state.record_log(format!(
                "{} pending-permission snapshot publish deferred: {error}",
                id.short()
            ));
        }
    }

    fn clear_remote_pending_permissions(
        &mut self,
        id: SessionId,
        incarnation_id: uuid::Uuid,
        mode: RemotePendingClearMode,
    ) {
        match mode {
            RemotePendingClearMode::AccessRevoked => {
                self.state
                    .agent_intel
                    .permission_decisions
                    .clear_pending_for_incarnation(id, incarnation_id);
            }
            RemotePendingClearMode::RetryableRelay => {
                self.state
                    .agent_intel
                    .permission_decisions
                    .clear_remote_for_reconnect(id, incarnation_id);
            }
            RemotePendingClearMode::RetireIncarnation => {
                self.state
                    .agent_intel
                    .permission_decisions
                    .retire_incarnation(id, incarnation_id);
            }
        }
        self.state
            .runtime_outbox
            .queue_pending_permissions_snapshot(
                self.state.agent_intel.permission_decisions.snapshot(),
            );

        if let Some(account_user_id) = self.state.identity.auth.subject_string()
            && let Err(error) = self.remote_permission_actions.clear_session_incarnation(
                &account_user_id,
                id,
                incarnation_id,
            )
        {
            self.state.record_log(format!(
                "{} could not clear pending remote permission actions: {error}",
                id.short()
            ));
        }
    }

    fn accepts_remote_pending_permissions_snapshot(
        &self,
        id: SessionId,
        incarnation_id: uuid::Uuid,
        relay_generation: u64,
    ) -> bool {
        self.state
            .remote
            .session_relays
            .is_current_generation(id, relay_generation)
            && self.state.discovery.session(id).is_some_and(|record| {
                record.incarnation_id == Some(incarnation_id)
                    && !matches!(
                        record.summary.state,
                        SessionState::Stopped | SessionState::Failed
                    )
                    && record.connection_state != Some(ConnectionState::Offline)
                    && !matches!(
                        record.access_state,
                        Some(
                            kodosi_domain::lifecycle::RemoteSessionAccessState::AccessDenied
                                | kodosi_domain::lifecycle::RemoteSessionAccessState::Failed
                        )
                    )
            })
    }

    fn reconcile_remote_permission_actions(&mut self, id: SessionId, incarnation_id: uuid::Uuid) {
        let Some(account_user_id) = self.state.identity.auth.subject_string() else {
            return;
        };
        let pending = self.remote_permission_actions.pending_for_incarnation(
            &account_user_id,
            id,
            incarnation_id,
        );
        for action in pending {
            let key = crate::agent_intel::permission_decision_registry::PendingKey {
                session_id: id,
                session_incarnation_id: incarnation_id,
                tool_use_id: action.request_id.clone(),
            };
            if self
                .state
                .agent_intel
                .permission_decisions
                .visible_identity(&key, action.request_generation)
            {
                continue;
            }
            if self
                .remote_permission_actions
                .complete_request_exact(
                    &account_user_id,
                    id,
                    incarnation_id,
                    &action.request_id,
                    action.request_generation,
                )
                .is_ok_and(|removed| removed.is_some())
            {
                self.state
                    .remote
                    .session_relays
                    .remove_permission_action(id, &action.action_id);
            }
        }
    }

    fn resolve_remote_permission_decision(
        &mut self,
        input: RemotePermissionDecisionInput<'_>,
    ) -> bool {
        let RemotePermissionDecisionInput {
            session_id: id,
            session_incarnation_id,
            action_id,
            request_id,
            request_generation,
            decision,
            decider_user_id,
        } = input;
        let permission_decision = match decision {
            "allow" => crate::agent_intel::permission_decision_registry::PermissionDecision::Allow,
            "deny" => crate::agent_intel::permission_decision_registry::PermissionDecision::Deny {
                reason: Some(format!("denied by {decider_user_id}")),
            },
            other => {
                self.state.record_log(format!(
                    "{} ignoring unknown remote decision '{other}'",
                    id.short()
                ));
                return false;
            }
        };

        let key = crate::agent_intel::permission_decision_registry::PendingKey {
            session_id: id,
            session_incarnation_id,
            tool_use_id: request_id.to_owned(),
        };

        let account_user_id = self
            .state
            .identity
            .auth
            .subject_string()
            .unwrap_or_default();
        let resolved = self
            .state
            .agent_intel
            .permission_decisions
            .resolve_remote_exact(&key, request_generation, permission_decision, |delivered| {
                self.owner_action_results
                    .complete_delivery_attempt(
                        &account_user_id,
                        id,
                        action_id,
                        decider_user_id,
                        delivered,
                    )
                    .unwrap_or(false)
            });
        match resolved {
            crate::agent_intel::permission_decision_registry::RemoteResolveOutcome::Resolved => {
                true
            }
            crate::agent_intel::permission_decision_registry::RemoteResolveOutcome::PersistenceFailed {
                delivered,
            } => {
                self.state.record_log(format!(
                    "{} remote decision for {request_id} was delivered={delivered} but its accepted result could not persist",
                    id.short()
                ));
                delivered
            }
            crate::agent_intel::permission_decision_registry::RemoteResolveOutcome::Rejected => {
                self.state.record_log(format!(
                    "{} remote decision for {request_id} had no eligible pending request",
                    id.short()
                ));
                false
            }
        }
    }

    fn update_system_clipboard(&mut self, id: SessionId, text: &str) {
        if !self.state.config.permissions.allow_terminal_clipboard_write {
            self.state.record_log(format!(
                "{} denied terminal-originated clipboard write by local policy",
                id.short()
            ));
            return;
        }
        if let Err(error) = self.clipboard.set_text(text) {
            self.state.record_log(format!(
                "{} failed to update system clipboard: {error}",
                id.short()
            ));
            let ids = self.state.local.sessions.ids().to_vec();
            for message in self
                .state
                .local
                .owned_session_runtimes
                .try_sync_clipboard_support(&ids, false)
            {
                self.state.record_log(message);
            }
        }
    }

    fn queue_terminal_bell(&mut self, id: SessionId) {
        self.state
            .runtime_outbox
            .queue_terminal_control(TerminalEvent::Bell {
                session_id: id.to_string(),
            });
    }

    fn apply_terminal_title(&mut self, id: SessionId, title: Option<String>) {
        let Some(record) = self.state.local.sessions.record_mut(id) else {
            return;
        };
        record.terminal_title.clone_from(&title);
        record.summary.last_update = time::OffsetDateTime::now_utc();
        self.state
            .runtime_outbox
            .queue_terminal_control(TerminalEvent::Title {
                session_id: id.to_string(),
                title,
            });
    }

    fn queue_terminal_notification(
        &mut self,
        id: SessionId,
        title: Option<String>,
        body: Option<String>,
    ) {
        self.state
            .runtime_outbox
            .queue_terminal_control(TerminalEvent::Notification {
                session_id: id.to_string(),
                title,
                body,
            });
    }
}

#[derive(Clone, Copy)]
struct RemotePermissionDecisionInput<'a> {
    session_id: SessionId,
    session_incarnation_id: uuid::Uuid,
    action_id: &'a str,
    request_id: &'a str,
    request_generation: u64,
    decision: &'a str,
    decider_user_id: &'a str,
}

#[derive(Clone, Copy)]
enum RemotePendingClearMode {
    AccessRevoked,
    RetryableRelay,
    RetireIncarnation,
}

fn current_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests;
