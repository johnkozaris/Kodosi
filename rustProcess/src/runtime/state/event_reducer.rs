use crate::{
    runtime::{AppState, SessionEventEffects},
    session_runtime::events::RuntimeSessionEvent,
};

impl AppState {
    #[expect(
        clippy::too_many_lines,
        reason = "single exhaustive reducer keeps runtime event ownership auditable"
    )]
    pub(crate) fn apply_session_event(
        &mut self,
        event: RuntimeSessionEvent,
    ) -> SessionEventEffects {
        if let Some(origin) = event.account_event_origin()
            && !self.identity.accepts_account_event(origin)
        {
            tracing::debug!(
                event_account_user_id = %origin.account_user_id,
                event_account_epoch = ?origin.epoch,
                current_account_epoch = ?self.identity.account_epoch(),
                "reducer ignored event from a prior authenticated account epoch"
            );
            return SessionEventEffects::default();
        }

        if let Some(origin) = event.host_relay_event_origin()
            && !self
                .sharing
                .host_relays
                .is_current_generation(origin.session_id, origin.relay_generation)
        {
            return SessionEventEffects::default();
        }

        let mut effects = SessionEventEffects::default();
        match event {
            RuntimeSessionEvent::TerminalOutputObserved { origin, .. } => {
                self.apply_terminal_output_observed(origin.session_id);
            }
            RuntimeSessionEvent::CaptureMetadata {
                origin,
                size,
                working_dir,
                refresh_working_dir,
            } => self.apply_capture_metadata(
                origin.session_id,
                size,
                working_dir,
                refresh_working_dir,
                &mut effects,
            ),
            RuntimeSessionEvent::WorkingDirChanged {
                origin,
                working_dir,
            } => {
                self.apply_working_dir_changed(origin.session_id, working_dir, &mut effects);
            }
            RuntimeSessionEvent::RuntimeMetadata {
                origin,
                working_dir,
                running_command,
                detected_agent,
            } => self.apply_runtime_metadata(
                origin.session_id,
                working_dir,
                running_command,
                detected_agent.as_deref(),
                &mut effects,
            ),
            RuntimeSessionEvent::ProjectDiscoveryReady {
                working_dir,
                discovery,
            } => self.apply_project_discovery_ready(working_dir, discovery),
            RuntimeSessionEvent::ProjectDiscoveryFailed {
                working_dir,
                message,
            } => self.apply_project_discovery_failed(&working_dir, &message),
            RuntimeSessionEvent::StateChanged { origin, state } => {
                self.apply_owned_state_changed(origin.session_id, state, origin.relay_generation);
            }
            RuntimeSessionEvent::HostRelayInfo { origin, message } => {
                self.apply_info(origin.session_id, &message);
            }
            RuntimeSessionEvent::HostRelayLogError { origin, message } => {
                self.apply_log_error(origin.session_id, &message);
            }
            RuntimeSessionEvent::RemoteInfo {
                id,
                message,
                relay_generation,
            } => {
                if self
                    .remote
                    .session_relays
                    .is_current_generation(id, relay_generation)
                {
                    self.apply_info(id, &message);
                }
            }
            RuntimeSessionEvent::RemoteLogError {
                id,
                message,
                relay_generation,
            } => {
                if self
                    .remote
                    .session_relays
                    .is_current_generation(id, relay_generation)
                {
                    self.apply_log_error(id, &message);
                }
            }
            RuntimeSessionEvent::DiscoveryInvalidated {
                surfaces, room_id, ..
            } => {
                self.apply_discovery_invalidated(surfaces, room_id);
            }
            RuntimeSessionEvent::UserIdentityLifecycleChanged {
                user_id,
                identity_revision,
                state,
                ..
            } => {
                self.apply_user_identity_lifecycle_changed(&user_id, identity_revision, state);
            }
            RuntimeSessionEvent::UserDeviceListChanged {
                user_id,
                generation,
                ..
            } => self.apply_user_device_list_changed(&user_id, generation),
            RuntimeSessionEvent::DeviceLinkSnapshot { requests, .. } => {
                self.apply_device_link_snapshot(requests);
            }
            RuntimeSessionEvent::DeviceLinkRequested {
                user_code,
                device_label,
                expires_at,
                ..
            } => self.apply_device_link_requested(user_code, device_label, expires_at),
            RuntimeSessionEvent::DeviceLinkResolved {
                user_code, outcome, ..
            } => {
                self.apply_device_link_resolved(user_code, outcome);
            }
            RuntimeSessionEvent::DeviceLinkSelfPending {
                user_code,
                expires_at,
                ..
            } => self.apply_device_link_self_pending(user_code, expires_at),
            RuntimeSessionEvent::DeviceLinkSelfResolved { outcome, .. } => {
                self.apply_device_link_self_resolved(outcome);
            }
            RuntimeSessionEvent::HostDemand {
                origin,
                required,
                participant_count,
                reason,
            } => self.apply_host_demand(origin.session_id, required, participant_count, &reason),
            RuntimeSessionEvent::RemoteStateChanged {
                id,
                state,
                relay_generation,
            } => {
                if self
                    .remote
                    .session_relays
                    .is_current_generation(id, relay_generation)
                {
                    self.apply_remote_state_changed(id, state);
                }
            }
            RuntimeSessionEvent::RemoteAccessChanged {
                id,
                access,
                relay_generation,
            } => {
                if self
                    .remote
                    .session_relays
                    .is_current_generation(id, relay_generation)
                {
                    self.apply_remote_access_changed(id, access);
                }
            }
            RuntimeSessionEvent::RemoteActionResult {
                id,
                action_id,
                request_id,
                request_generation,
                status,
                relay_generation,
            } => {
                if self
                    .remote
                    .session_relays
                    .is_current_generation(id, relay_generation)
                {
                    self.apply_remote_action_result(
                        id,
                        action_id,
                        request_id,
                        request_generation,
                        status,
                    );
                }
            }
            RuntimeSessionEvent::RemoteSessionConnectionChanged {
                id,
                status,
                reason,
                relay_generation,
            } => {
                if self
                    .remote
                    .session_relays
                    .is_current_generation(id, relay_generation)
                {
                    self.apply_remote_session_connection_changed(id, status, reason);
                }
            }
            RuntimeSessionEvent::RemoteAccessStateChanged {
                id,
                state,
                reason,
                issue,
                relay_generation,
            } => {
                if self
                    .remote
                    .session_relays
                    .is_current_generation(id, relay_generation)
                {
                    self.apply_remote_access_state_changed(id, state, reason, issue);
                }
            }
            RuntimeSessionEvent::RemoteAccessRevoked {
                id,
                relay_generation,
            } => {
                if self
                    .remote
                    .session_relays
                    .is_current_generation(id, relay_generation)
                {
                    self.apply_remote_access_revoked(id);
                }
            }
            RuntimeSessionEvent::HostAccessRevoked {
                origin,
                revoked_user_id,
            } => {
                self.apply_host_access_revoked(origin.session_id, &revoked_user_id);
            }
            RuntimeSessionEvent::HostRelayBackendRestarted { origin } => {
                self.apply_host_relay_backend_restarted(origin.session_id);
            }
            RuntimeSessionEvent::HostFrameReservationExhausted { origin } => {
                self.apply_host_frame_reservation_exhausted(origin.session_id);
            }
            RuntimeSessionEvent::HostKeyDistributionRequested { origin, fence_id } => {
                self.apply_host_key_distribution_requested(origin.session_id, fence_id);
            }
            RuntimeSessionEvent::HostKeyRotationRequired {
                origin,
                reason,
                fail_closed,
            } => self.apply_host_key_rotation_required(origin.session_id, &reason, fail_closed),
            RuntimeSessionEvent::Stopped { origin, reason } => {
                self.apply_owned_stopped(origin.session_id, reason);
            }
            RuntimeSessionEvent::Failed { origin, message } => {
                self.apply_owned_failed(origin.session_id, &message);
            }
            RuntimeSessionEvent::BackendAccessInvalid { .. }
            | RuntimeSessionEvent::HostRelayBackendAccessInvalid { .. }
            | RuntimeSessionEvent::RemoteCheckpoint { .. }
            | RuntimeSessionEvent::RemoteRawBatch { .. }
            | RuntimeSessionEvent::RemotePlainPresentation { .. }
            | RuntimeSessionEvent::RemotePendingPermissionsSnapshot { .. }
            | RuntimeSessionEvent::RemoteSemanticReceipt { .. }
            | RuntimeSessionEvent::RemoteSessionRelayExited { .. }
            | RuntimeSessionEvent::RemoteActionCompleted { .. }
            | RuntimeSessionEvent::HostActionResultMailboxAck { .. }
            | RuntimeSessionEvent::HostSemanticReceiptMailboxAck { .. }
            | RuntimeSessionEvent::HostSemanticSend { .. }
            | RuntimeSessionEvent::HostSemanticCancel { .. }
            | RuntimeSessionEvent::RemoteControlTrustEstablished { .. }
            | RuntimeSessionEvent::PendingPermissionRequest { .. }
            | RuntimeSessionEvent::PermissionResolved { .. }
            | RuntimeSessionEvent::RemotePermissionDecision { .. }
            | RuntimeSessionEvent::ClipboardUpdate { .. }
            | RuntimeSessionEvent::TerminalBell { .. }
            | RuntimeSessionEvent::TerminalTitleChanged { .. }
            | RuntimeSessionEvent::TerminalNotification { .. }
            | RuntimeSessionEvent::AgentIntelSnapshot { .. }
            | RuntimeSessionEvent::RoomAgentDeliveryState { .. }
            | RuntimeSessionEvent::AgentBoundary { .. }
            | RuntimeSessionEvent::AgentTurnStateChanged { .. } => {}
        }
        effects
    }
}
