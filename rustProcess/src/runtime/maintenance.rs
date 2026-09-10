use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

use crate::{
    AppError,
    session_runtime::{
        events::{DiscoverySurface, RuntimeSessionEvent},
        project::discover as discover_project,
    },
};
use kodosi_domain::{ids::SessionId, session::SessionState};

use super::{
    BackendReconciliationState, ProjectDiscoveryRequest, Runtime, pending_work::BackendScopeRestore,
};

const BACKEND_RECONCILIATION_BUDGET: Duration = Duration::from_secs(1);

fn pin_refresh_is_retryable(error: &AppError) -> bool {
    match error {
        AppError::Io(_) | AppError::Http(_) | AppError::WebSocket(_) | AppError::Unauthorized => {
            true
        }
        AppError::HttpProblem { status, .. } => {
            matches!(*status, 408 | 409 | 425 | 429 | 500..=599)
        }
        AppError::Unsupported { reason } => reason.starts_with("backend operation timed out:"),
        AppError::ConfigSource(_)
        | AppError::ConfigDeserialize(_)
        | AppError::Json(_)
        | AppError::Keychain { .. }
        | AppError::NotFound
        | AppError::AuthRejected { .. }
        | AppError::Join(_)
        | AppError::NoActiveSession
        | AppError::AccountEpochExhausted
        | AppError::ChannelClosed { .. }
        | AppError::ChannelFull { .. }
        | AppError::DeliveryUnknown { .. }
        | AppError::InvalidTerminalSize { .. }
        | AppError::InvalidUrl { .. }
        | AppError::MissingConfig { .. }
        | AppError::InvalidBackendData { .. }
        | AppError::RoomContentNotRecipient
        | AppError::IdentityRecoveryRequired { .. }
        | AppError::PeerIdentityChanged { .. } => false,
    }
}

#[cfg(test)]
mod pin_refresh_retry_tests {
    use std::io;

    use super::{AppError, pin_refresh_is_retryable};

    #[test]
    fn retries_only_transient_pin_refresh_failures() {
        assert!(pin_refresh_is_retryable(&AppError::Io(io::Error::new(
            io::ErrorKind::TimedOut,
            "temporary",
        ))));
        assert!(pin_refresh_is_retryable(&AppError::HttpProblem {
            status: 503,
            code: None,
            detail: "draining".to_owned(),
        }));
        assert!(pin_refresh_is_retryable(&AppError::Unsupported {
            reason: "backend operation timed out: identity fetch".to_owned(),
        }));

        assert!(!pin_refresh_is_retryable(&AppError::PeerIdentityChanged {
            user_id: "peer".to_owned(),
            detail: "key substitution".to_owned(),
        }));
        assert!(!pin_refresh_is_retryable(&AppError::InvalidBackendData {
            field: "identity.deviceList".to_owned(),
            reason: "malformed proof".to_owned(),
        }));
        assert!(!pin_refresh_is_retryable(&AppError::HttpProblem {
            status: 400,
            code: None,
            detail: "invalid request".to_owned(),
        }));
        assert!(!pin_refresh_is_retryable(&AppError::NotFound));
    }
}

impl Runtime {
    pub(crate) fn tick_interval(&self) -> Duration {
        Duration::from_millis(self.state.config.runtime.tick_interval_ms)
    }

    pub(crate) fn restore_cached_sessions(&mut self) -> crate::Result<()> {
        let cached = super::local_control::migration::load_cached_sessions(
            self.state.session_cache_root.as_deref(),
        )?;
        let discovered = self
            .local_catalog
            .discover(self.state.session_cache_root.as_deref(), cached);
        for entry in discovered {
            let id = entry.summary.id;
            if self.state.local.sessions.record(id).is_some() {
                continue;
            }
            if !entry.launch_committed {
                self.local_catalog.remove_from_list(id)?;
                self.state.record_log(format!(
                    "{} retired an uncommitted session launch reservation",
                    id.short()
                ));
                continue;
            }
            if let Some(working_dir) = entry.summary.working_dir.as_deref() {
                self.ensure_project_discovery(working_dir, false);
            }
            self.state.local.sessions.insert_discovered(
                entry.summary,
                entry.create_request_id,
                entry.local_incarnation_id,
                entry.recovery,
            );
            self.state
                .local
                .sessions
                .set_resume_source(id, entry.resume_source);
        }
        self.state
            .shelf
            .sync_owned_sessions(self.state.local.sessions.ids());
        Ok(())
    }

    #[cfg(feature = "cli")]
    pub(crate) async fn next_session_event(&mut self) -> Option<RuntimeSessionEvent> {
        self.session_events_rx.recv().await
    }

    pub(crate) async fn await_coordinator_mutation<T>(
        &mut self,
        mut completion: tokio::sync::oneshot::Receiver<crate::Result<T>>,
    ) -> crate::Result<T> {
        loop {
            tokio::select! {
                result = &mut completion => {
                    return result.unwrap_or_else(|_| Err(AppError::ChannelClosed {
                        session: "owned session coordinator".to_owned(),
                    }));
                }
                event = self.session_events_rx.recv() => {
                    let Some(event) = event else {
                        return Err(AppError::ChannelClosed {
                            session: "owned session event lane".to_owned(),
                        });
                    };
                    if self.handle_session_event(event) {
                        self.state.snapshot_refresh_pending = true;
                    }
                }
            }
        }
    }

    pub(crate) fn drain_pending_session_events(&mut self) -> bool {
        const MAX_EVENTS_PER_DRAIN: usize = 256;
        let mut force_snapshot = false;
        for _ in 0..MAX_EVENTS_PER_DRAIN {
            let Ok(event) = self.session_events_rx.try_recv() else {
                break;
            };
            force_snapshot |= self.handle_session_event(event);
        }
        force_snapshot
    }

    pub(crate) async fn flush_pending_session_events(&mut self) -> bool {
        let force_snapshot = self.drain_pending_session_events();
        self.drain_identity_lifecycle_work().await;
        self.drain_pin_reset_work().await;
        self.drain_pin_refresh_work().await;
        self.process_pending_discovery_refresh().await;
        force_snapshot
    }

    pub(crate) async fn drain_queued_pin_resets_before_self_pin_install(&mut self) {
        if self.drain_pending_session_events() {
            self.state.snapshot_refresh_pending = true;
        }
        self.drain_identity_lifecycle_work().await;
        self.drain_pin_reset_work().await;
    }

    const MAINTENANCE_MIN_INTERVAL: Duration = Duration::from_secs(1);
    const AUTH_REFRESH_CHECK_INTERVAL: Duration = Duration::from_secs(30);

    fn apply_coordinator_health_failures(
        &mut self,
        failures: Vec<(SessionId, uuid::Uuid, String)>,
    ) {
        for (id, local_incarnation_id, message) in failures {
            self.handle_session_event(RuntimeSessionEvent::Failed {
                origin: crate::session_runtime::events::LocalCoordinatorOrigin {
                    session_id: id,
                    local_incarnation_id,
                },
                message,
            });
        }
    }

    pub(crate) async fn run_periodic_maintenance_force(&mut self) {
        self.last_maintenance_ran = Some(Instant::now());
        self.state.agent_intel.purge_expired_now();

        let completed_coordinators = self.state.local.owned_session_runtimes.finished_ids();
        if !completed_coordinators.is_empty() && self.drain_pending_session_events() {
            self.state.snapshot_refresh_pending = true;
        }
        let deferred_mailbox_surfaces = mailbox_surfaces(&self.state.pending_discovery_surfaces);
        let deferred_room_mailbox_projections = self
            .state
            .pending_room_projection_refreshes
            .iter()
            .filter_map(|(room_id, surfaces)| {
                let surfaces = mailbox_surfaces(surfaces);
                (!surfaces.is_empty()).then(|| (room_id.clone(), surfaces))
            })
            .collect::<Vec<_>>();
        let health = self.state.check_task_health(&completed_coordinators);
        for message in health.messages {
            self.state.record_log(message);
        }
        self.apply_coordinator_health_failures(health.force_failed);
        for (id, agent_name) in health.restart_agent_intel {
            self.restart_finished_agent_intel(id, &agent_name);
        }

        let finalize_prior_login = std::mem::take(&mut self.state.pending_post_login_work);
        let completed_login = crate::runtime::auth::drain_device_flow_events(self).await;
        if completed_login {
            self.last_auth_refresh_check = Some(Instant::now());
        }
        if finalize_prior_login {
            crate::runtime::auth::finalize_login(self).await;
        }
        if !completed_login
            && self.state.identity.auth.is_authenticated()
            && self
                .last_auth_refresh_check
                .is_none_or(|last| last.elapsed() >= Self::AUTH_REFRESH_CHECK_INTERVAL)
        {
            self.last_auth_refresh_check = Some(Instant::now());
            match tokio::time::timeout(
                BACKEND_RECONCILIATION_BUDGET,
                crate::runtime::auth::ensure_backend_access(self),
            )
            .await
            {
                Ok(Ok(_)) => {}
                Ok(Err(error)) => {
                    self.state
                        .record_log(format!("proactive token refresh check failed: {error}"));
                }
                Err(_elapsed) => {
                    self.state.record_log(
                        "backend reconciliation timed out; local work remains available".to_owned(),
                    );
                }
            }
        }
        self.expire_share_transition_deadlines();
        self.retry_share_transition_settlements();
        self.pump_relay_prepare_worker().await;
        if self.has_verified_backend_compatibility() {
            self.process_collaboration_teardown_worker().await;
            self.process_share_transition_cleanup();
            self.finish_initial_collaboration_cleanup().await;
        }
        if self.state.identity.auth.is_authenticated() {
            self.state
                .queue_discovery_refresh(deferred_mailbox_surfaces);
            for (room_id, surfaces) in deferred_room_mailbox_projections {
                self.state.queue_room_projection_refresh(room_id, surfaces);
            }
        }
        if self.state.identity.auth.is_authenticated() {
            self.retire_inactive_session_access_mutations();
            self.pump_session_access_mutations().await;
        }
        if self.remote_surfaces_ready() {
            if let Err(error) = crate::runtime::remote_sessions::reconcile_relays(self).await {
                self.state
                    .record_log(format!("session relay reconcile failed: {error}"));
            }
            for id in self.state.remote.session_relays.ids() {
                crate::runtime::remote_sessions::replay_remote_permission_actions(self, id);
            }
            self.pump_semantic_receipt_mailbox().await;
            self.process_hosted_sharing_work().await;
        }
        self.refresh_clipboard_support().await;
        self.drain_ready_deletes();
        self.process_pending_discovery_refresh().await;
    }

    fn restart_finished_agent_intel(&mut self, id: SessionId, agent_name: &str) {
        let incarnation_id = crate::agent_intel::lifecycle::AgentIntelLifecycleCtx {
            intel_state: &mut self.state.agent_intel,
            local_sessions: &self.state.local,
            session_events: &self.session_events_tx,
            outbox: &mut self.state.runtime_outbox,
        }
        .restart_task(id);
        if let Some(incarnation_id) = incarnation_id {
            self.publish_pending_permissions(id, incarnation_id);
        }
        crate::agent_intel::lifecycle::AgentIntelLifecycleCtx {
            intel_state: &mut self.state.agent_intel,
            local_sessions: &self.state.local,
            session_events: &self.session_events_tx,
            outbox: &mut self.state.runtime_outbox,
        }
        .spawn(id, agent_name);
    }

    fn retire_inactive_session_access_mutations(&mut self) {
        let Some(account) = self.state.identity.auth.subject_string() else {
            return;
        };
        let sessions = match self.access_mutations.entries(&account) {
            Ok(entries) => entries
                .into_iter()
                .filter(|entry| entry.state.holds_local_authority())
                .filter_map(|entry| {
                    let live = self.access_mutation_session_live(&entry);
                    (!live).then_some(entry.runtime_session_id)
                })
                .collect::<std::collections::HashSet<_>>(),
            Err(error) => {
                self.state.record_log(error.to_string());
                return;
            }
        };
        for session_id in sessions {
            self.retire_access_mutations_for_session(
                session_id,
                "local session authority ended before the access change settled",
            );
        }
    }

    pub(crate) async fn pump_session_access_mutations(&mut self) {
        use crate::runtime::access_mutation_worker::SessionAccessMutationWorkerWork;

        if self
            .access_mutation_task
            .as_ref()
            .is_some_and(tokio::task::JoinHandle::is_finished)
        {
            let in_flight = self.access_mutation_in_flight.take();
            let Some(task) = self.access_mutation_task.take() else {
                return;
            };
            match task.await {
                Ok(completion) => self.apply_access_mutation_completion(completion).await,
                Err(error) if error.is_cancelled() => {}
                Err(error) => {
                    if let Some(identity) = in_flight {
                        self.access_mutation_retry_until
                            .insert(identity, Instant::now() + Duration::from_secs(5));
                    }
                    self.state
                        .record_log(format!("session access mutation worker failed: {error}"));
                }
            }
        }
        if self.access_mutation_task.is_some() || !self.remote_surfaces_ready() {
            return;
        }

        let Some(account) = self.state.identity.auth.subject_string() else {
            return;
        };
        let account_epoch = self.state.identity.account_epoch().value();
        let now = Instant::now();
        self.access_mutation_retry_until
            .retain(|_, retry_at| now < *retry_at);

        let selection = match self.select_access_mutation_work(&account) {
            Ok(selection) => selection,
            Err(error) => {
                self.state.record_log(error.to_string());
                return;
            }
        };
        let Some((identity, prepared, mode)) = selection else {
            return;
        };
        if self.share_transition_active_for_session(
            &prepared.account_user_id,
            prepared.runtime_session_id,
        ) {
            self.access_mutation_retry_until.insert(
                (prepared.account_user_id.clone(), prepared.mutation_id),
                Instant::now() + Duration::from_secs(1),
            );
            return;
        }

        let backend = self.backend.clone();
        let notify = std::sync::Arc::clone(&self.access_mutation_notify);
        self.access_mutation_in_flight = Some(identity);
        self.access_mutation_task = Some(tokio::spawn(async move {
            let completion = crate::runtime::access_mutation_worker::execute(
                backend,
                SessionAccessMutationWorkerWork {
                    prepared,
                    account_epoch,
                    mode,
                },
            )
            .await;
            notify.notify_one();
            completion
        }));
    }

    fn select_access_mutation_work(
        &mut self,
        account: &str,
    ) -> crate::Result<
        Option<(
            (String, uuid::Uuid),
            crate::runtime::access_mutations::PreparedSessionAccessMutation,
            crate::runtime::access_mutation_worker::SessionAccessMutationWorkerMode,
        )>,
    > {
        use crate::runtime::{
            access_mutation_worker::SessionAccessMutationWorkerMode,
            access_mutations::PreparedSessionAccessMutationState,
        };

        while let Some(index) = self
            .access_mutation_dispatch_queue
            .iter()
            .position(|(queued_account, _)| queued_account == account)
        {
            let Some(identity) = self.access_mutation_dispatch_queue.remove(index) else {
                continue;
            };
            let Some(prepared) = self.access_mutations.get(&identity.0, identity.1)?.cloned()
            else {
                continue;
            };
            if matches!(
                prepared.state,
                PreparedSessionAccessMutationState::Prepared
                    | PreparedSessionAccessMutationState::Attempting
            ) && self.access_mutation_session_live(&prepared)
            {
                return Ok(Some((
                    identity,
                    prepared,
                    SessionAccessMutationWorkerMode::Dispatch,
                )));
            }
        }
        let prepared = self
            .access_mutations
            .entries(account)?
            .into_iter()
            .find(|entry| {
                !matches!(entry.state, PreparedSessionAccessMutationState::Terminal(_))
                    && (matches!(entry.state, PreparedSessionAccessMutationState::Retiring)
                        || self.access_mutation_session_live(entry))
                    && !self.share_transition_active_for_session(
                        &entry.account_user_id,
                        entry.runtime_session_id,
                    )
                    && !self
                        .access_mutation_retry_until
                        .contains_key(&(entry.account_user_id.clone(), entry.mutation_id))
            });
        Ok(prepared.map(|prepared| {
            let mode = if matches!(prepared.state, PreparedSessionAccessMutationState::Prepared) {
                SessionAccessMutationWorkerMode::Dispatch
            } else {
                SessionAccessMutationWorkerMode::Reconcile
            };
            ((account.to_owned(), prepared.mutation_id), prepared, mode)
        }))
    }

    #[expect(
        clippy::too_many_lines,
        reason = "phase-aware preparation snapshots exact durable, audience, key, and service authority before detachment"
    )]
    fn start_access_effect_worker(
        &mut self,
        prepared: crate::runtime::access_mutations::PreparedSessionAccessMutation,
        mode: crate::runtime::access_mutation_worker::SessionAccessMutationWorkerMode,
        access_snapshot: Option<crate::Result<kodosi_backend_client::api::BackendAccessGrants>>,
    ) -> crate::Result<()> {
        use crate::runtime::{
            access_effect_worker::{
                AccessEffectAudience, AccessEffectKind, AccessEffectServices, AccessEffectWork,
            },
            access_mutations::SessionAccessMutationTarget,
        };

        if self.access_effect_task.is_some() {
            return Err(AppError::Unsupported {
                reason: "another session access key effect is already in flight".to_owned(),
            });
        }
        let shared = self
            .state
            .sharing
            .shared_sessions
            .get_mut(prepared.runtime_session_id)
            .ok_or(AppError::NoActiveSession)?;
        if shared.backend_session_id() != prepared.backend_session_id
            || *shared.backend_incarnation_id() != prepared.backend_incarnation_id
        {
            return Err(AppError::NoActiveSession);
        }
        let source_key_generation = shared.session_key_generation();
        let kind = if let crate::runtime::access_mutations::PreparedSessionAccessMutationState::RelayPending {
            key_generation,
        } = prepared.state
        {
            if source_key_generation != Some(key_generation) {
                return Err(AppError::Unsupported {
                    reason: "relay-pending key generation no longer matches local authority"
                        .to_owned(),
                });
            }
            AccessEffectKind::LoadRelayIdentity {
                expected_host_device_id: shared
                    .host_device_id()
                    .ok_or_else(|| AppError::Unsupported {
                        reason: "shared session is missing its host device identity".to_owned(),
                    })?
                    .to_owned(),
            }
        } else {
            match &prepared.target {
            SessionAccessMutationTarget::Grant {
                actor_user_id,
                access_level,
                expires_at_unix_ms,
            } => {
                let expires_at = time::OffsetDateTime::from_unix_timestamp_nanos(
                    i128::from(*expires_at_unix_ms) * 1_000_000,
                )
                .map_err(|error| AppError::InvalidBackendData {
                    field: "pendingSessionAccessMutations.expiresAt".to_owned(),
                    reason: error.to_string(),
                })?;
                shared.grant_user_at(actor_user_id.to_string(), *access_level, expires_at);
                AccessEffectKind::Redistribute {
                    session_key: *shared.session_key().ok_or_else(|| AppError::Unsupported {
                        reason: "cannot redistribute a missing session key".to_owned(),
                    })?,
                    key_generation: source_key_generation.ok_or_else(|| AppError::Unsupported {
                        reason: "shared session is missing current key generation".to_owned(),
                    })?,
                }
            }
            SessionAccessMutationTarget::Revoke { actor_user_id } => {
                shared.revoke_user(&actor_user_id.to_string());
                AccessEffectKind::Rotate {
                    previous_generation: source_key_generation.unwrap_or(0),
                }
            }
            SessionAccessMutationTarget::Leave => {
                return Err(AppError::Unsupported {
                    reason: "leave does not use the key-effect worker".to_owned(),
                });
            }
        }
        };
        let audience = AccessEffectAudience {
            scope: shared.scope(),
            room_id: shared.room().map(|room| room.id.clone()),
            explicit_grants: shared.explicit_grantee_access().clone(),
            owner_user_id: prepared.account_user_id.clone(),
        };
        if matches!(prepared.target, SessionAccessMutationTarget::Revoke { .. }) {
            self.state
                .sharing
                .host_relays
                .cancel(prepared.runtime_session_id);
        }
        let services = AccessEffectServices {
            backend: self.backend.clone(),
            auth: self.state.identity.auth.clone(),
            device_key_store: self.device_key_store.clone(),
            pin_store: self.pin_store.clone(),
            roster_pins_path: self.room_roster_pins_path.clone(),
        };
        let account_epoch = self.state.identity.account_epoch().value();
        self.access_effect_in_flight = Some((
            prepared.account_user_id.clone(),
            prepared.mutation_id,
            prepared.runtime_session_id,
        ));
        let notify = std::sync::Arc::clone(&self.access_effect_notify);
        self.access_effect_task = Some(tokio::spawn(async move {
            let completion = crate::runtime::access_effect_worker::execute(AccessEffectWork {
                prepared,
                account_epoch,
                source_key_generation,
                mode,
                access_snapshot,
                audience,
                services,
                kind,
            })
            .await;
            notify.notify_one();
            completion
        }));
        Ok(())
    }

    fn start_access_relay_prepare_worker(
        &mut self,
        prepared: crate::runtime::access_mutations::PreparedSessionAccessMutation,
        mode: crate::runtime::access_mutation_worker::SessionAccessMutationWorkerMode,
        access_snapshot: Option<crate::Result<kodosi_backend_client::api::BackendAccessGrants>>,
        source_key_generation: Option<u32>,
        sender_signing_pkcs8: zeroize::Zeroizing<Vec<u8>>,
    ) -> crate::Result<()> {
        self.start_relay_prepare_worker(
            crate::runtime::relay_prepare_worker::RelayPrepareOwner::Access {
                prepared,
                mode,
                access_snapshot,
            },
            source_key_generation,
            sender_signing_pkcs8,
        )
    }

    pub(crate) fn start_maintenance_relay_prepare_worker(
        &mut self,
        session_id: SessionId,
    ) -> crate::Result<()> {
        if self.state.sharing.host_relays.active(session_id) {
            return Ok(());
        }
        if self.relay_prepare_tasks.contains_key(&session_id) {
            return Ok(());
        }
        let account_user_id = self
            .state
            .identity
            .auth
            .subject_string()
            .ok_or(AppError::Unauthorized)?;
        if !self.remote_surfaces_ready()
            || self.share_transition_active_for_session(&account_user_id, session_id)
            || self
                .access_mutations
                .entries(&account_user_id)?
                .iter()
                .any(|entry| {
                    entry.runtime_session_id == session_id && entry.state.holds_local_authority()
                })
        {
            return Err(AppError::Unsupported {
                reason: "collaboration change prevents host relay restart".to_owned(),
            });
        }
        let runtime_incarnation_id = self
            .state
            .local
            .sessions
            .record(session_id)
            .ok_or(AppError::NoActiveSession)?
            .local_incarnation_id;
        let shared = self
            .state
            .sharing
            .shared_sessions
            .get(session_id)
            .ok_or(AppError::NoActiveSession)?;
        let backend_session_id = shared.backend_session_id().to_owned();
        let backend_incarnation_id = *shared.backend_incarnation_id();
        let source_key_generation = shared.session_key_generation();
        let expected_host_device_id = shared
            .host_device_id()
            .ok_or_else(|| AppError::Unsupported {
                reason: "shared session is missing its host device identity".to_owned(),
            })?
            .to_owned();
        let services = crate::runtime::access_effect_worker::AccessEffectServices {
            backend: self.backend.clone(),
            auth: self.state.identity.auth.clone(),
            device_key_store: self.device_key_store.clone(),
            pin_store: self.pin_store.clone(),
            roster_pins_path: self.room_roster_pins_path.clone(),
        };
        self.start_relay_prepare_worker_with_identity(
            crate::runtime::relay_prepare_worker::RelayPrepareOwner::Maintenance {
                account_user_id,
                runtime_session_id: session_id,
                runtime_incarnation_id,
                backend_session_id,
                backend_incarnation_id,
            },
            source_key_generation,
            crate::runtime::relay_prepare_worker::RelaySigningIdentity::Load {
                services: Box::new(services),
                expected_host_device_id,
            },
        )
    }

    pub(crate) fn start_relay_prepare_worker(
        &mut self,
        owner: crate::runtime::relay_prepare_worker::RelayPrepareOwner,
        source_key_generation: Option<u32>,
        sender_signing_pkcs8: zeroize::Zeroizing<Vec<u8>>,
    ) -> crate::Result<()> {
        self.start_relay_prepare_worker_with_identity(
            owner,
            source_key_generation,
            crate::runtime::relay_prepare_worker::RelaySigningIdentity::Prepared(
                sender_signing_pkcs8,
            ),
        )
    }

    #[expect(
        clippy::too_many_lines,
        reason = "actor preparation keeps every provisional relay resource and exact fence in one auditable transaction"
    )]
    fn start_relay_prepare_worker_with_identity(
        &mut self,
        owner: crate::runtime::relay_prepare_worker::RelayPrepareOwner,
        source_key_generation: Option<u32>,
        signing_identity: crate::runtime::relay_prepare_worker::RelaySigningIdentity,
    ) -> crate::Result<()> {
        use crate::{
            sharing::host_relay::adapters::{KodosiHostRelayPort, RuntimeHostRelayEventSink},
            terminal_transport::{TerminalCapability, TerminalSurface},
        };
        use kodosi_backend_client::relay::{self, HostRelaySpec};
        use std::sync::Arc;
        use tokio::sync::{mpsc, watch};
        use zeroize::Zeroizing;

        if self
            .relay_prepare_tasks
            .contains_key(&owner.runtime_session_id())
        {
            return Err(AppError::Unsupported {
                reason: "another host relay handshake is already in flight for this session"
                    .to_owned(),
            });
        }
        let id = owner.runtime_session_id();
        let shared = self
            .state
            .sharing
            .shared_sessions
            .get(id)
            .ok_or(AppError::NoActiveSession)?;
        let session_key =
            Zeroizing::new(*shared.session_key().ok_or_else(|| AppError::Unsupported {
                reason: "shared encrypted session is missing its session key".to_owned(),
            })?);
        let frame_key_generation =
            shared
                .session_key_generation()
                .ok_or_else(|| AppError::Unsupported {
                    reason: "shared encrypted session is missing current key generation".to_owned(),
                })?;
        let backend_session_id = shared.backend_session_id().to_owned();
        let backend_incarnation_id = *shared.backend_incarnation_id();
        let owner_secret = Zeroizing::new(shared.owner_secret().to_owned());
        let control_trust = shared.control_trust();
        let host_device_id = shared
            .host_device_id()
            .ok_or_else(|| AppError::Unsupported {
                reason: "shared session is missing its host device identity".to_owned(),
            })?
            .to_owned();
        let runtime = self
            .state
            .local
            .owned_session_runtimes
            .runtime_handle(id)
            .ok_or(AppError::NoActiveSession)?;
        let size_authority = self
            .state
            .local
            .owned_session_runtimes
            .size_authority(id)
            .unwrap_or_default();
        let port = Arc::new(KodosiHostRelayPort::new(runtime, size_authority));
        let relay_cancel = self
            .state
            .local
            .owned_session_runtimes
            .child_cancellation_token(id)?;
        let (frame_revision_block, frame_nonce_block) =
            crate::runtime::sharing::reserve_host_relay_ranges_or_dispose(
                self,
                id,
                frame_key_generation,
            )?;
        let relay_generation = owner
            .relay_generation()
            .map_or_else(|| self.state.sharing.host_relays.allocate_generation(), Ok)?;
        let account_origin = self
            .state
            .identity
            .current_account_event_origin()
            .ok_or(AppError::Unauthorized)?;
        let owner_user_id = account_origin.account_user_id.clone();
        let event_sink = Arc::new(RuntimeHostRelayEventSink::new(
            self.session_events_tx.clone(),
            id,
            relay_generation,
            account_origin,
        ));
        let initial_pending_permissions = self
            .state
            .agent_intel
            .permission_decisions
            .snapshot_for_publish(id, owner.runtime_incarnation_id())
            .ok_or_else(|| AppError::Unsupported {
                reason: "pending-permission snapshot generation exhausted".to_owned(),
            })?;
        let initial_pending_plaintext =
            serde_json::to_vec(&initial_pending_permissions).map_err(AppError::Json)?;
        let (pending_permissions_tx, pending_permissions_rx) =
            watch::channel(Some(relay::HostRelayPendingPermissionsSnapshot {
                generation: initial_pending_permissions.generation,
                incarnation_id: owner.runtime_incarnation_id(),
                plaintext: initial_pending_plaintext,
            }));
        let (semantic_receipt_tx, semantic_receipt_rx) =
            mpsc::channel(crate::runtime::sharing::SEMANTIC_RECEIPT_CAPACITY);
        let (action_result_tx, action_result_rx) =
            mpsc::channel(crate::runtime::sharing::ACTION_RESULT_CAPACITY);
        let (fence_completion_tx, fence_completion_rx) =
            mpsc::channel(crate::runtime::sharing::FENCE_COMPLETION_CAPACITY);
        let hub_handle = self
            .terminal_hub
            .register(id, TerminalSurface::RelayHost, TerminalCapability::ReadOnly)
            .ok_or(AppError::NoActiveSession)?;
        let (relay_event_tx, relay_event_rx) = mpsc::channel::<relay::HostRelayTerminalEvent>(
            crate::runtime::sharing::RELAY_BRIDGE_CAPACITY,
        );
        let connection_id = hub_handle.connection_id;
        let bridge_cancel = relay_cancel.child_token();
        tokio::spawn(crate::runtime::sharing::run_relay_hub_bridge(
            self.terminal_hub.clone(),
            id,
            hub_handle,
            relay_event_tx,
            bridge_cancel.clone(),
        ));
        let bridge = crate::runtime::sharing::ProvisionalRelayBridge {
            hub: self.terminal_hub.clone(),
            session_id: id,
            connection_id,
            cancellation: bridge_cancel,
            armed: true,
        };
        let spec = HostRelaySpec {
            id,
            backend_session_id,
            backend_incarnation_id,
            owner_secret,
            port,
            frame_revision_start: frame_revision_block.start,
            frame_revision_end_exclusive: frame_revision_block.end_exclusive,
            frame_key_generation,
            frame_nonce_start: frame_nonce_block.start,
            frame_nonce_end_exclusive: frame_nonce_block.end_exclusive,
            session_key,
            owner_user_id,
            control_trust,
            host_device_id,
            host_signing_pkcs8: Zeroizing::new(Vec::new()),
        };
        let owner_kind = owner.kind();
        let deadline = match &owner {
            crate::runtime::relay_prepare_worker::RelayPrepareOwner::Share { prepared } => Some(
                self.share_transition_deadlines
                    .get(&prepared.transition_id)
                    .copied()
                    .ok_or_else(|| AppError::Unsupported {
                        reason: "share transition execution deadline is unavailable".to_owned(),
                    })?,
            ),
            crate::runtime::relay_prepare_worker::RelayPrepareOwner::Access { .. }
            | crate::runtime::relay_prepare_worker::RelayPrepareOwner::Maintenance { .. } => None,
        };
        let work = crate::runtime::relay_prepare_worker::RelayPrepareWork {
            owner,
            account_epoch: self.state.identity.account_epoch().value(),
            source_key_generation,
            relay_generation,
            deadline,
            signing_identity,
            spec,
            terminal_rx: relay_event_rx,
            host_ws: self.host_ws.clone(),
            auth_provider: crate::runtime::auth::backend_auth_provider(self),
            event_sink,
            pending_permissions_rx,
            semantic_receipt_rx,
            action_result_rx,
            fence_completion_rx,
            cancellation: relay_cancel,
            bridge,
            pending_permissions_tx,
            semantic_receipt_tx,
            action_result_tx,
            fence_completion_tx,
        };
        let fallback_owner = match &work.owner {
            crate::runtime::relay_prepare_worker::RelayPrepareOwner::Access {
                prepared,
                mode,
                access_snapshot,
            } => crate::runtime::relay_prepare_worker::RelayPrepareOwner::Access {
                prepared: prepared.clone(),
                mode: *mode,
                access_snapshot: access_snapshot.as_ref().map(|snapshot| match snapshot {
                    Ok(value) => Ok(value.clone()),
                    Err(error) => Err(AppError::Unsupported {
                        reason: error.to_string(),
                    }),
                }),
            },
            crate::runtime::relay_prepare_worker::RelayPrepareOwner::Share { prepared } => {
                crate::runtime::relay_prepare_worker::RelayPrepareOwner::Share {
                    prepared: prepared.clone(),
                }
            }
            crate::runtime::relay_prepare_worker::RelayPrepareOwner::Maintenance {
                account_user_id,
                runtime_session_id,
                runtime_incarnation_id,
                backend_session_id,
                backend_incarnation_id,
            } => crate::runtime::relay_prepare_worker::RelayPrepareOwner::Maintenance {
                account_user_id: account_user_id.clone(),
                runtime_session_id: *runtime_session_id,
                runtime_incarnation_id: *runtime_incarnation_id,
                backend_session_id: backend_session_id.clone(),
                backend_incarnation_id: *backend_incarnation_id,
            },
        };
        let fallback_epoch = work.account_epoch;
        let fallback_key_generation = work.source_key_generation;
        let fallback_relay_generation = work.relay_generation;
        let notify = Arc::clone(&self.relay_prepare_notify);
        self.relay_prepare_owners.insert(id, owner_kind);
        self.relay_prepare_tasks.insert(
            id,
            tokio::spawn(async move {
                use futures_util::FutureExt as _;
                let completion = std::panic::AssertUnwindSafe(
                    crate::runtime::relay_prepare_worker::execute(work),
                )
                .catch_unwind()
                .await
                .unwrap_or_else(|_| {
                    crate::runtime::relay_prepare_worker::RelayPrepareCompletion {
                        owner: fallback_owner,
                        account_epoch: fallback_epoch,
                        source_key_generation: fallback_key_generation,
                        relay_generation: fallback_relay_generation,
                        outcome: Err(AppError::Unsupported {
                            reason: "host relay preparation worker panicked".to_owned(),
                        }),
                    }
                });
                notify.notify_one();
                completion
            }),
        );
        Ok(())
    }

    pub(crate) fn invalidate_hosted_session_keys(&mut self) {
        let hosted = self.state.sharing.shared_sessions.ids().collect::<Vec<_>>();
        self.cancel_relay_preparations_for_sessions(hosted);
        self.state.invalidate_hosted_session_keys();
    }

    pub(crate) fn rotate_room_scoped_session_keys(&mut self, room_id: &str) {
        let affected = self
            .state
            .sharing
            .shared_sessions
            .ids_scoped_to_room(room_id);
        self.cancel_relay_preparations_for_sessions(affected);
        self.state.rotate_room_scoped_session_keys(room_id);
    }

    pub(crate) fn cancel_relay_preparations_for_sessions(
        &mut self,
        session_ids: impl IntoIterator<Item = SessionId>,
    ) {
        for session_id in session_ids {
            self.cancel_relay_prepare_for_session(session_id);
        }
    }

    pub(crate) fn cancel_relay_prepare_for_session(&mut self, session_id: SessionId) -> bool {
        let cancelled = self
            .relay_prepare_tasks
            .remove(&session_id)
            .is_some_and(|task| {
                task.abort();
                true
            });
        self.relay_prepare_owners.remove(&session_id);
        cancelled
    }

    pub(crate) async fn pump_relay_prepare_worker(&mut self) {
        let finished = self
            .relay_prepare_tasks
            .iter()
            .filter_map(|(id, task)| task.is_finished().then_some(*id))
            .collect::<Vec<_>>();
        for id in finished {
            let Some(task) = self.relay_prepare_tasks.remove(&id) else {
                continue;
            };
            self.relay_prepare_owners.remove(&id);
            let completion = match task.await {
                Ok(completion) => completion,
                Err(error) if error.is_cancelled() => continue,
                Err(error) => {
                    self.state
                        .record_log(format!("host relay preparation worker failed: {error}"));
                    continue;
                }
            };
            self.apply_relay_prepare_completion(completion);
        }
    }

    fn apply_relay_prepare_completion(
        &mut self,
        completion: crate::runtime::relay_prepare_worker::RelayPrepareCompletion,
    ) {
        let fence = completion.fence();
        let crate::runtime::relay_prepare_worker::RelayPrepareCompletion {
            owner,
            account_epoch,
            source_key_generation,
            relay_generation,
            outcome,
        } = completion;
        match owner {
            crate::runtime::relay_prepare_worker::RelayPrepareOwner::Access {
                prepared,
                mode,
                access_snapshot,
            } => {
                self.apply_access_relay_prepare_completion(
                    &prepared,
                    mode,
                    access_snapshot,
                    account_epoch,
                    source_key_generation,
                    relay_generation,
                    fence.as_ref(),
                    outcome,
                );
            }
            crate::runtime::relay_prepare_worker::RelayPrepareOwner::Share { prepared } => {
                self.apply_share_relay_prepare_completion(
                    &prepared,
                    account_epoch,
                    source_key_generation,
                    relay_generation,
                    fence.as_ref(),
                    outcome,
                );
            }
            crate::runtime::relay_prepare_worker::RelayPrepareOwner::Maintenance {
                account_user_id,
                runtime_session_id,
                runtime_incarnation_id,
                backend_session_id,
                backend_incarnation_id,
            } => {
                self.apply_maintenance_relay_prepare_completion(
                    &account_user_id,
                    runtime_session_id,
                    runtime_incarnation_id,
                    &backend_session_id,
                    backend_incarnation_id,
                    account_epoch,
                    source_key_generation,
                    relay_generation,
                    fence.as_ref(),
                    outcome,
                );
            }
        }
    }

    fn access_mutation_session_live(
        &self,
        prepared: &crate::runtime::access_mutations::PreparedSessionAccessMutation,
    ) -> bool {
        if prepared.target.is_leave() {
            return prepared.backend_session_id == prepared.runtime_session_id.to_string()
                && prepared.backend_incarnation_id == prepared.expected_runtime_incarnation_id;
        }
        self.state
            .local
            .sessions
            .record(prepared.runtime_session_id)
            .is_some_and(|record| {
                record.local_incarnation_id == prepared.expected_runtime_incarnation_id
                    && !matches!(
                        record.summary.state,
                        SessionState::Stopping | SessionState::Stopped | SessionState::Failed
                    )
            })
    }

    fn relay_commit_session_live(&self, session_id: SessionId) -> bool {
        self.state
            .local
            .sessions
            .record(session_id)
            .is_some_and(|record| {
                !matches!(
                    record.summary.state,
                    SessionState::Stopping | SessionState::Stopped | SessionState::Failed
                )
            })
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "maintenance relay completion carries every snapshotted and live authority fence explicitly"
    )]
    fn apply_maintenance_relay_prepare_completion(
        &mut self,
        account_user_id: &str,
        runtime_session_id: SessionId,
        runtime_incarnation_id: uuid::Uuid,
        backend_session_id: &str,
        backend_incarnation_id: uuid::Uuid,
        account_epoch: u64,
        source_key_generation: Option<u32>,
        relay_generation: u64,
        fence: Option<&crate::runtime::relay_prepare_worker::RelayPrepareFence>,
        outcome: crate::Result<crate::runtime::relay_prepare_worker::RelayPrepareCommit>,
    ) {
        let shared = self.state.sharing.shared_sessions.get(runtime_session_id);
        let fences_match = self.relay_commit_session_live(runtime_session_id)
            && fence.is_some_and(|fence| {
            fence.account_user_id == account_user_id
                && fence.runtime_incarnation_id == runtime_incarnation_id
                && fence.backend_session_id == backend_session_id
                && fence.backend_incarnation_id == backend_incarnation_id
                && crate::runtime::relay_prepare_worker::exact_fence_matches(
                    fence,
                    self.state.identity.auth.subject_string().as_deref(),
                    self.state.identity.account_epoch().value(),
                    self.state
                        .local
                        .sessions
                        .record(runtime_session_id)
                        .map(|record| record.local_incarnation_id),
                    shared.map(
                        crate::sharing::shared_session_registry::SharedSessionState::backend_session_id,
                    ),
                    shared.map(|state| *state.backend_incarnation_id()),
                    shared.and_then(
                        crate::sharing::shared_session_registry::SharedSessionState::session_key_generation,
                    ),
                    self.state
                        .sharing
                        .host_relays
                        .generation(runtime_session_id),
                )
                && account_epoch == self.state.identity.account_epoch().value()
                && source_key_generation
                    == shared.and_then(
                        crate::sharing::shared_session_registry::SharedSessionState::session_key_generation,
                    )
        });
        if !fences_match {
            self.state.record_log(format!(
                "ignored stale maintenance relay preparation for {}",
                runtime_session_id.short()
            ));
            return;
        }
        let commit = match outcome {
            Ok(commit) => commit,
            Err(error) => {
                self.state.record_log(format!(
                    "relay preparation for {} remains pending: {error}",
                    runtime_session_id.short()
                ));
                return;
            }
        };
        if let Err(error) =
            self.activate_prepared_host_relay(runtime_session_id, relay_generation, commit)
        {
            self.state.record_log(format!(
                "relay activation for {} remains pending: {error}",
                runtime_session_id.short()
            ));
            return;
        }
        self.state
            .publish_session_if_host_relay_active(runtime_session_id);
        self.state.record_log(format!(
            "sharing relay connected for {}",
            runtime_session_id.short()
        ));
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "access relay completion carries every durable and live commit fence explicitly"
    )]
    fn apply_access_relay_prepare_completion(
        &mut self,
        prepared: &crate::runtime::access_mutations::PreparedSessionAccessMutation,
        mode: crate::runtime::access_mutation_worker::SessionAccessMutationWorkerMode,
        access_snapshot: Option<crate::Result<kodosi_backend_client::api::BackendAccessGrants>>,
        account_epoch: u64,
        source_key_generation: Option<u32>,
        relay_generation: u64,
        fence: Option<&crate::runtime::relay_prepare_worker::RelayPrepareFence>,
        outcome: crate::Result<crate::runtime::relay_prepare_worker::RelayPrepareCommit>,
    ) {
        use crate::runtime::access_mutations::PreparedSessionAccessMutationState;

        let ledger_matches = self
            .access_mutations
            .get(&prepared.account_user_id, prepared.mutation_id)
            .ok()
            .flatten()
            .is_some_and(|current| {
                current.fingerprint == prepared.fingerprint
                    && account_epoch == prepared.originating_account_epoch
                    && matches!(
                        current.state,
                        PreparedSessionAccessMutationState::RelayPending { key_generation }
                            if Some(key_generation) == source_key_generation
                    )
            });
        let shared = self
            .state
            .sharing
            .shared_sessions
            .get(prepared.runtime_session_id);
        let fences_match = self.relay_commit_session_live(prepared.runtime_session_id)
            && fence.is_some_and(|fence| {
            crate::runtime::relay_prepare_worker::exact_fence_matches(
                fence,
                self.state.identity.auth.subject_string().as_deref(),
                self.state.identity.account_epoch().value(),
                self.state
                    .local
                    .sessions
                    .record(prepared.runtime_session_id)
                    .map(|record| record.local_incarnation_id),
                shared.map(
                    crate::sharing::shared_session_registry::SharedSessionState::backend_session_id,
                ),
                shared.map(|state| *state.backend_incarnation_id()),
                shared.and_then(
                    crate::sharing::shared_session_registry::SharedSessionState::session_key_generation,
                ),
                self.state
                    .sharing
                    .host_relays
                    .generation(prepared.runtime_session_id),
            )
        });
        if !ledger_matches || !fences_match {
            self.state.record_log(format!(
                "ignored stale host relay preparation completion {}",
                prepared.mutation_id
            ));
            return;
        }
        let commit = match outcome {
            Ok(commit) => commit,
            Err(error) => {
                self.state.record_log(format!(
                    "session access mutation {} relay restart remains pending: {error}",
                    prepared.mutation_id
                ));
                self.access_mutation_retry_until.insert(
                    (prepared.account_user_id.clone(), prepared.mutation_id),
                    Instant::now() + Duration::from_secs(5),
                );
                return;
            }
        };
        if let Err(error) =
            self.activate_prepared_host_relay(prepared.runtime_session_id, relay_generation, commit)
        {
            self.state.record_log(format!(
                "session access mutation {} relay activation remains pending: {error}",
                prepared.mutation_id
            ));
            self.access_mutation_retry_until.insert(
                (prepared.account_user_id.clone(), prepared.mutation_id),
                Instant::now() + Duration::from_secs(5),
            );
            return;
        }
        self.finish_access_mutation_effect(prepared, mode, access_snapshot);
    }

    pub(crate) fn activate_prepared_host_relay(
        &mut self,
        session_id: SessionId,
        relay_generation: u64,
        mut commit: crate::runtime::relay_prepare_worker::RelayPrepareCommit,
    ) -> crate::Result<()> {
        self.state
            .sharing
            .host_relays
            .claim_generation(session_id, relay_generation)?;
        let relay_handle = commit.relay.activate();
        self.state.attach_host_relay(
            session_id,
            relay_handle.cancellation,
            commit.pending_permissions_tx,
            commit.semantic_receipt_tx,
            commit.action_result_tx,
            commit.fence_completion_tx,
            relay_handle.join_handle,
        );
        commit.bridge.disarm();
        self.drain_semantic_relay_receipts();
        self.drain_owner_action_results();
        Ok(())
    }

    fn finish_access_mutation_effect(
        &mut self,
        prepared: &crate::runtime::access_mutations::PreparedSessionAccessMutation,
        mode: crate::runtime::access_mutation_worker::SessionAccessMutationWorkerMode,
        access_snapshot: Option<crate::Result<kodosi_backend_client::api::BackendAccessGrants>>,
    ) {
        if let Err(error) = crate::runtime::sharing::mark_access_mutation_terminal(
            self,
            prepared,
            crate::runtime::access_mutations::SessionAccessMutationTerminalStatus::Applied,
            None,
        ) {
            self.state.record_log(error.to_string());
            return;
        }
        self.queue_access_mutation_outcome(
            prepared,
            mode,
            crate::host_protocol::SessionAccessMutationOutcome::Applied,
            None,
        );
        if matches!(
            prepared.target,
            crate::runtime::access_mutations::SessionAccessMutationTarget::Leave
        ) {
            self.state.snapshot_refresh_pending = true;
        }
        self.queue_access_snapshot_from_completion(prepared, access_snapshot);
    }

    pub(crate) fn retire_access_mutations_for_session(
        &mut self,
        session_id: SessionId,
        reason: &str,
    ) {
        use crate::runtime::access_mutations::{
            PreparedSessionAccessMutationState, SessionAccessMutationTerminal,
            SessionAccessMutationTerminalStatus,
        };

        let Some(account) = self.state.identity.auth.subject_string() else {
            return;
        };
        let entries = match self.access_mutations.entries(&account) {
            Ok(entries) => entries,
            Err(error) => {
                self.state.record_log(error.to_string());
                return;
            }
        };
        let retiring = entries
            .into_iter()
            .filter(|entry| {
                entry.runtime_session_id == session_id && entry.state.holds_local_authority()
            })
            .collect::<Vec<_>>();
        for mut entry in retiring {
            let previous_state = entry.state.clone();
            entry.state = match previous_state {
                PreparedSessionAccessMutationState::Prepared => {
                    PreparedSessionAccessMutationState::Terminal(
                        SessionAccessMutationTerminal::new(
                            SessionAccessMutationTerminalStatus::Rejected,
                            Some(reason.to_owned()),
                        ),
                    )
                }
                PreparedSessionAccessMutationState::Attempting
                | PreparedSessionAccessMutationState::OutcomeUnknown => {
                    PreparedSessionAccessMutationState::Retiring
                }
                PreparedSessionAccessMutationState::ReceiptConfirmed
                | PreparedSessionAccessMutationState::EffectPending
                | PreparedSessionAccessMutationState::RelayPending { .. } => {
                    PreparedSessionAccessMutationState::Terminal(
                        SessionAccessMutationTerminal::new(
                            SessionAccessMutationTerminalStatus::Applied,
                            Some(
                                "backend access changed before the local session ended".to_owned(),
                            ),
                        ),
                    )
                }
                PreparedSessionAccessMutationState::Retiring
                | PreparedSessionAccessMutationState::Terminal(_) => continue,
            };
            if let Err(error) = self.access_mutations.put(entry.clone()) {
                self.state.record_log(format!(
                    "could not retire session access mutation {}: {error}",
                    entry.mutation_id
                ));
                continue;
            }
            let identity = (entry.account_user_id.clone(), entry.mutation_id);
            self.access_mutation_dispatch_queue
                .retain(|queued| queued != &identity);
            self.access_mutation_retry_until.remove(&identity);
            if matches!(entry.state, PreparedSessionAccessMutationState::Retiring) {
                self.last_maintenance_ran = None;
            } else {
                self.state.runtime_outbox.queue_session(
                    crate::runtime::sharing::recovered_access_mutation_event(entry),
                );
            }
        }
        if self
            .access_mutation_in_flight
            .as_ref()
            .is_some_and(|(worker_account, mutation_id)| {
                self.access_mutations
                    .get(worker_account, *mutation_id)
                    .ok()
                    .flatten()
                    .is_some_and(|entry| entry.runtime_session_id == session_id)
            })
        {
            if let Some(task) = self.access_mutation_task.take() {
                task.abort();
            }
            self.access_mutation_in_flight = None;
        }
        if self
            .access_effect_in_flight
            .as_ref()
            .is_some_and(|(_, _, worker_session_id)| *worker_session_id == session_id)
        {
            if let Some(task) = self.access_effect_task.take() {
                task.abort();
            }
            self.access_effect_in_flight = None;
        }
        if self.relay_prepare_owners.get(&session_id)
            == Some(&crate::runtime::relay_prepare_worker::RelayPrepareOwnerKind::Access)
        {
            self.cancel_relay_prepare_for_session(session_id);
        }
    }

    pub(crate) async fn pump_access_effect_worker(&mut self) {
        if !self
            .access_effect_task
            .as_ref()
            .is_some_and(tokio::task::JoinHandle::is_finished)
        {
            return;
        }
        let Some(task) = self.access_effect_task.take() else {
            return;
        };
        self.access_effect_in_flight = None;
        match task.await {
            Ok(completion) => self.apply_access_effect_completion(completion),
            Err(error) if error.is_cancelled() => {}
            Err(error) => self
                .state
                .record_log(format!("session access key-effect worker failed: {error}")),
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "key-effect completion keeps durable, account, incarnation, backend, and source-key fences ordered"
    )]
    fn apply_access_effect_completion(
        &mut self,
        completion: crate::runtime::access_effect_worker::AccessEffectCompletion,
    ) {
        use crate::runtime::access_mutations::PreparedSessionAccessMutationState;

        let prepared = completion.prepared;
        let ledger_matches = self
            .access_mutations
            .get(&prepared.account_user_id, prepared.mutation_id)
            .ok()
            .flatten()
            .is_some_and(|current| {
                current.fingerprint == prepared.fingerprint
                    && matches!(
                        current.state,
                        PreparedSessionAccessMutationState::EffectPending
                    )
            });
        let context_matches = self.state.identity.auth.subject_string().as_deref()
            == Some(prepared.account_user_id.as_str())
            && self.state.identity.account_epoch().value() == completion.account_epoch
            && self.access_mutation_session_live(&prepared);
        let shared_matches = self
            .state
            .sharing
            .shared_sessions
            .get(prepared.runtime_session_id)
            .is_some_and(|shared| {
                shared.backend_session_id() == prepared.backend_session_id
                    && *shared.backend_incarnation_id() == prepared.backend_incarnation_id
                    && shared.session_key_generation() == completion.source_key_generation
            });
        if !ledger_matches || !context_matches || !shared_matches {
            self.state.record_log(format!(
                "ignored stale session access key-effect completion {}",
                prepared.mutation_id
            ));
            return;
        }
        let commit = match completion.outcome {
            Ok(commit) => commit,
            Err(error) => {
                self.state.record_log(format!(
                    "session access mutation {} key effects remain pending: {error}",
                    prepared.mutation_id
                ));
                self.access_mutation_retry_until.insert(
                    (prepared.account_user_id.clone(), prepared.mutation_id),
                    Instant::now() + Duration::from_secs(5),
                );
                return;
            }
        };
        let (
            session_key,
            key_generation,
            control_keys,
            sender_device_id,
            sender_signing_pkcs8,
            distributed,
        ) = match commit {
            crate::runtime::access_effect_worker::AccessEffectCommit::RelayIdentity {
                sender_signing_pkcs8,
            } => {
                if let Err(error) = self.start_access_relay_prepare_worker(
                    prepared.clone(),
                    completion.mode,
                    completion.access_snapshot,
                    completion.source_key_generation,
                    sender_signing_pkcs8,
                ) {
                    self.state.record_log(format!(
                        "session access mutation {} relay restart remains pending: {error}",
                        prepared.mutation_id
                    ));
                    self.access_mutation_retry_until.insert(
                        (prepared.account_user_id.clone(), prepared.mutation_id),
                        Instant::now() + Duration::from_secs(5),
                    );
                }
                return;
            }
            crate::runtime::access_effect_worker::AccessEffectCommit::Distributed {
                session_key,
                key_generation,
                control_keys,
                sender_device_id,
                sender_signing_pkcs8,
                distributed,
            } => (
                session_key,
                key_generation,
                control_keys,
                sender_device_id,
                sender_signing_pkcs8,
                distributed,
            ),
        };
        if !distributed {
            self.state.record_log(format!(
                "session access mutation {} is waiting for recipient device keys",
                prepared.mutation_id
            ));
            self.access_mutation_retry_until.insert(
                (prepared.account_user_id.clone(), prepared.mutation_id),
                Instant::now() + Duration::from_secs(5),
            );
            return;
        }
        let registry = &mut self.state.sharing.shared_sessions;
        if !registry.set_session_key(
            prepared.runtime_session_id,
            Some(session_key),
            Some(key_generation),
        ) || !registry
            .replace_control_trust_for_backend(&prepared.backend_session_id, control_keys)
            || !registry.set_host_device_for_backend(&prepared.backend_session_id, sender_device_id)
        {
            self.state.record_log(format!(
                "ignored session access key-effect completion {} after authority changed",
                prepared.mutation_id
            ));
            return;
        }
        if matches!(
            prepared.target,
            crate::runtime::access_mutations::SessionAccessMutationTarget::Revoke { .. }
        ) {
            let relay_pending = match crate::runtime::sharing::persist_access_mutation_state(
                self,
                &prepared,
                PreparedSessionAccessMutationState::RelayPending { key_generation },
            ) {
                Ok(relay_pending) => relay_pending,
                Err(error) => {
                    self.state.record_log(error.to_string());
                    return;
                }
            };
            if let Err(error) = self.start_access_relay_prepare_worker(
                relay_pending,
                completion.mode,
                completion.access_snapshot,
                Some(key_generation),
                sender_signing_pkcs8,
            ) {
                self.state.record_log(format!(
                    "session access mutation {} relay restart remains pending: {error}",
                    prepared.mutation_id
                ));
                self.access_mutation_retry_until.insert(
                    (prepared.account_user_id.clone(), prepared.mutation_id),
                    Instant::now() + Duration::from_secs(5),
                );
            }
            return;
        }
        self.finish_access_mutation_effect(&prepared, completion.mode, completion.access_snapshot);
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one actor-side completion state machine keeps durable transitions, exact fences, effects, and events ordered"
    )]
    async fn apply_access_mutation_completion(
        &mut self,
        completion: crate::runtime::access_mutation_worker::SessionAccessMutationWorkerCompletion,
    ) {
        use crate::runtime::{
            access_mutation_worker::{
                SessionAccessMutationWorkerMode, SessionAccessMutationWorkerOutcome,
            },
            access_mutations::{
                PreparedSessionAccessMutationState, SessionAccessMutationTerminalStatus,
            },
        };

        let prepared = completion.prepared;
        let account_context_matches = self.state.identity.auth.subject_string().as_deref()
            == Some(prepared.account_user_id.as_str())
            && self.state.identity.account_epoch().value() == completion.account_epoch;
        let current = self
            .access_mutations
            .get(&prepared.account_user_id, prepared.mutation_id)
            .ok()
            .flatten()
            .cloned();
        let exact_ledger_match = current.as_ref().is_some_and(|current| {
            current.fingerprint == prepared.fingerprint
                && current.runtime_session_id == prepared.runtime_session_id
                && current.expected_runtime_incarnation_id
                    == prepared.expected_runtime_incarnation_id
                && !matches!(
                    current.state,
                    PreparedSessionAccessMutationState::Terminal(_)
                )
        });
        if !account_context_matches || !exact_ledger_match {
            self.state.record_log(format!(
                "ignored stale session access mutation completion {}",
                prepared.mutation_id
            ));
            return;
        }
        if current.as_ref().is_some_and(|current| {
            matches!(current.state, PreparedSessionAccessMutationState::Retiring)
        }) {
            self.apply_retiring_access_mutation_completion(
                &prepared,
                completion.mode,
                completion.outcome,
            );
            return;
        }
        if !self.access_mutation_session_live(&prepared) {
            self.state.record_log(format!(
                "ignored session access mutation completion {} after local session retirement",
                prepared.mutation_id
            ));
            return;
        }

        match completion.outcome {
            SessionAccessMutationWorkerOutcome::Applied => {
                let Some(current) = current else {
                    return;
                };
                let effect_pending = match current.state {
                    PreparedSessionAccessMutationState::RelayPending { .. }
                    | PreparedSessionAccessMutationState::EffectPending => current,
                    PreparedSessionAccessMutationState::Prepared
                    | PreparedSessionAccessMutationState::Attempting
                    | PreparedSessionAccessMutationState::OutcomeUnknown
                    | PreparedSessionAccessMutationState::Retiring
                    | PreparedSessionAccessMutationState::ReceiptConfirmed => {
                        let confirmed = match crate::runtime::sharing::persist_access_mutation_state(
                            self,
                            &prepared,
                            PreparedSessionAccessMutationState::ReceiptConfirmed,
                        ) {
                            Ok(confirmed) => confirmed,
                            Err(error) => {
                                self.state.record_log(error.to_string());
                                return;
                            }
                        };
                        match crate::runtime::sharing::persist_access_mutation_state(
                            self,
                            &confirmed,
                            PreparedSessionAccessMutationState::EffectPending,
                        ) {
                            Ok(effect_pending) => effect_pending,
                            Err(error) => {
                                self.state.record_log(error.to_string());
                                return;
                            }
                        }
                    }
                    PreparedSessionAccessMutationState::Terminal(_) => return,
                };
                if matches!(
                    effect_pending.target,
                    crate::runtime::access_mutations::SessionAccessMutationTarget::Leave
                ) {
                    if let Err(error) = crate::runtime::sharing::resume_access_mutation_effects(
                        self,
                        &effect_pending,
                    )
                    .await
                    {
                        self.state.record_log(format!(
                            "session access mutation {} local effects remain pending: {error}",
                            prepared.mutation_id
                        ));
                        self.access_mutation_retry_until.insert(
                            (prepared.account_user_id.clone(), prepared.mutation_id),
                            Instant::now() + Duration::from_secs(5),
                        );
                        return;
                    }
                    self.finish_access_mutation_effect(
                        &effect_pending,
                        completion.mode,
                        completion.access_snapshot,
                    );
                    return;
                }
                if let Err(error) = self.start_access_effect_worker(
                    effect_pending.clone(),
                    completion.mode,
                    completion.access_snapshot,
                ) {
                    self.state.record_log(format!(
                        "session access mutation {} local effects remain pending: {error}",
                        prepared.mutation_id
                    ));
                    self.access_mutation_retry_until.insert(
                        (prepared.account_user_id.clone(), prepared.mutation_id),
                        Instant::now() + Duration::from_secs(5),
                    );
                }
            }
            SessionAccessMutationWorkerOutcome::Unknown(message) => {
                if let Err(error) = crate::runtime::sharing::persist_access_mutation_state(
                    self,
                    &prepared,
                    PreparedSessionAccessMutationState::OutcomeUnknown,
                ) {
                    self.state.record_log(error.to_string());
                    return;
                }
                self.access_mutation_retry_until.insert(
                    (prepared.account_user_id.clone(), prepared.mutation_id),
                    Instant::now() + Duration::from_secs(5),
                );
                if matches!(completion.mode, SessionAccessMutationWorkerMode::Dispatch) {
                    self.queue_access_mutation_outcome(
                        &prepared,
                        completion.mode,
                        crate::host_protocol::SessionAccessMutationOutcome::Unknown,
                        Some(message),
                    );
                }
            }
            SessionAccessMutationWorkerOutcome::Rejected(error)
                if matches!(
                    error,
                    AppError::Unauthorized | AppError::AuthRejected { .. }
                ) =>
            {
                let message = error.to_string();
                if let Err(expiry_error) =
                    crate::runtime::auth::mark_expired_from_backend(self, message.clone())
                {
                    self.state.record_log(expiry_error.to_string());
                }
                self.state.record_log(format!(
                    "session access mutation {} retained after authentication rejection: {message}",
                    prepared.mutation_id
                ));
            }
            SessionAccessMutationWorkerOutcome::Rejected(error) => {
                let message = error.to_string();
                if let Err(persist_error) = crate::runtime::sharing::mark_access_mutation_terminal(
                    self,
                    &prepared,
                    SessionAccessMutationTerminalStatus::Rejected,
                    Some(message.clone()),
                ) {
                    self.state.record_log(persist_error.to_string());
                    return;
                }
                self.queue_access_mutation_outcome(
                    &prepared,
                    completion.mode,
                    crate::host_protocol::SessionAccessMutationOutcome::Rejected,
                    Some(message),
                );
            }
            SessionAccessMutationWorkerOutcome::Deferred(error) => {
                self.state.record_log(format!(
                    "session access mutation {} remains pending: {error}",
                    prepared.mutation_id
                ));
                self.access_mutation_retry_until.insert(
                    (prepared.account_user_id.clone(), prepared.mutation_id),
                    Instant::now() + Duration::from_secs(5),
                );
            }
        }
    }

    fn apply_retiring_access_mutation_completion(
        &mut self,
        prepared: &crate::runtime::access_mutations::PreparedSessionAccessMutation,
        mode: crate::runtime::access_mutation_worker::SessionAccessMutationWorkerMode,
        outcome: crate::runtime::access_mutation_worker::SessionAccessMutationWorkerOutcome,
    ) {
        use crate::runtime::{
            access_mutation_worker::SessionAccessMutationWorkerOutcome,
            access_mutations::SessionAccessMutationTerminalStatus,
        };

        match outcome {
            SessionAccessMutationWorkerOutcome::Applied => {
                if let Err(error) = crate::runtime::sharing::mark_access_mutation_terminal(
                    self,
                    prepared,
                    SessionAccessMutationTerminalStatus::Applied,
                    Some("session ended before local access effects could apply".to_owned()),
                ) {
                    self.state.record_log(error.to_string());
                    return;
                }
                self.queue_access_mutation_outcome(
                    prepared,
                    mode,
                    crate::host_protocol::SessionAccessMutationOutcome::Applied,
                    None,
                );
            }
            SessionAccessMutationWorkerOutcome::Unknown(message) => {
                self.access_mutation_retry_until.insert(
                    (prepared.account_user_id.clone(), prepared.mutation_id),
                    Instant::now() + Duration::from_secs(5),
                );
                if matches!(
                    mode,
                    crate::runtime::access_mutation_worker::SessionAccessMutationWorkerMode::Dispatch
                ) {
                    self.queue_access_mutation_outcome(
                        prepared,
                        mode,
                        crate::host_protocol::SessionAccessMutationOutcome::Unknown,
                        Some(message),
                    );
                }
            }
            SessionAccessMutationWorkerOutcome::Rejected(error) => {
                let message = error.to_string();
                if let Err(persist_error) = crate::runtime::sharing::mark_access_mutation_terminal(
                    self,
                    prepared,
                    SessionAccessMutationTerminalStatus::Rejected,
                    Some(message.clone()),
                ) {
                    self.state.record_log(persist_error.to_string());
                    return;
                }
                self.queue_access_mutation_outcome(
                    prepared,
                    mode,
                    crate::host_protocol::SessionAccessMutationOutcome::Rejected,
                    Some(message),
                );
            }
            SessionAccessMutationWorkerOutcome::Deferred(error) => {
                self.state.record_log(format!(
                    "retiring session access mutation {} remains pending: {error}",
                    prepared.mutation_id
                ));
                self.access_mutation_retry_until.insert(
                    (prepared.account_user_id.clone(), prepared.mutation_id),
                    Instant::now() + Duration::from_secs(5),
                );
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn access_mutation_worker_finished_for_test(&self) -> bool {
        self.access_mutation_task
            .as_ref()
            .is_some_and(tokio::task::JoinHandle::is_finished)
    }

    #[cfg(test)]
    pub(crate) async fn apply_access_mutation_completion_for_test(
        &mut self,
        completion: crate::runtime::access_mutation_worker::SessionAccessMutationWorkerCompletion,
    ) {
        self.apply_access_mutation_completion(completion).await;
    }

    fn queue_access_mutation_outcome(
        &mut self,
        prepared: &crate::runtime::access_mutations::PreparedSessionAccessMutation,
        mode: crate::runtime::access_mutation_worker::SessionAccessMutationWorkerMode,
        outcome: crate::host_protocol::SessionAccessMutationOutcome,
        message: Option<String>,
    ) {
        if matches!(
            mode,
            crate::runtime::access_mutation_worker::SessionAccessMutationWorkerMode::Dispatch
        ) {
            self.state
                .runtime_outbox
                .queue_session(crate::SessionEvent::AccessMutationResult {
                    mutation_id: prepared.mutation_id.to_string(),
                    session_id: prepared.runtime_session_id.to_string(),
                    expected_runtime_incarnation_id: prepared
                        .expected_runtime_incarnation_id
                        .to_string(),
                    kind: prepared.target.kind(),
                    actor_user_id: prepared.target.actor_user_id(),
                    access_level: prepared.target.access_level(),
                    expires_at: prepared.target.expires_at(),
                    outcome,
                    message,
                });
        }
        if let Ok(Some(terminal)) = self
            .access_mutations
            .get(&prepared.account_user_id, prepared.mutation_id)
            .map(clone_access_mutation)
        {
            self.state.runtime_outbox.queue_session(
                crate::runtime::sharing::recovered_access_mutation_event(terminal),
            );
        }
    }

    fn queue_access_snapshot_from_completion(
        &mut self,
        prepared: &crate::runtime::access_mutations::PreparedSessionAccessMutation,
        snapshot: Option<crate::Result<kodosi_backend_client::api::BackendAccessGrants>>,
    ) {
        let Some(snapshot) = snapshot else { return };
        match snapshot {
            Ok(snapshot) if snapshot.incarnation_id == prepared.backend_incarnation_id => {
                let grants = snapshot
                    .grants
                    .into_iter()
                    .map(crate::runtime::sharing::access_grant_entry_from_dto)
                    .collect();
                self.state
                    .runtime_outbox
                    .queue_session(crate::SessionEvent::AccessGrants {
                        session_id: prepared.runtime_session_id.to_string(),
                        runtime_incarnation_id: prepared
                            .expected_runtime_incarnation_id
                            .to_string(),
                        account_user_id: prepared.account_user_id.clone(),
                        grants,
                    });
            }
            Ok(_) => self.state.record_log(format!(
                "session access snapshot for {} named a different incarnation",
                prepared.mutation_id
            )),
            Err(error) => {
                self.state.record_log(format!(
                    "session access mutation {} applied but projection refresh failed: {error}",
                    prepared.mutation_id
                ));
                self.state.snapshot_refresh_pending = true;
            }
        }
    }

    pub(crate) async fn run_periodic_maintenance_throttled(&mut self) {
        let due = self
            .last_maintenance_ran
            .is_none_or(|last| last.elapsed() >= Self::MAINTENANCE_MIN_INTERVAL);
        if due {
            self.run_periodic_maintenance_force().await;
        }
    }

    pub(crate) fn collaboration_cleanup_settled(&self) -> bool {
        if self.collaboration_teardown_task.is_some()
            || self.collaboration_teardown_retry_after.is_some()
            || !self.collaboration_teardown_retry_until.is_empty()
            || !self.collaboration_teardown_deferred_until.is_empty()
        {
            return false;
        }
        self.collaboration_teardown
            .cleanup_health(|record| {
                self.collaboration_obligation_is_not_pending_for_current_account(record)
            })
            .is_ok_and(|health| {
                health.pending_count == 0
                    && health.durable_state
                        == crate::sharing::collaboration_teardown_obligations::DurableStoreState::Healthy
            })
    }

    pub(crate) async fn finish_initial_collaboration_cleanup(&mut self) {
        if self.backend_reconciliation != BackendReconciliationState::CleanupPending {
            return;
        }
        if !self.collaboration_cleanup_settled() {
            return;
        }

        let now = Instant::now();
        if self
            .device_enrollment_retry_after
            .is_some_and(|retry_after| now < retry_after)
        {
            return;
        }

        let enrollment_result = if self.device_enrollment_satisfied {
            Ok(())
        } else {
            (crate::runtime::identity::device_keys::DeviceKeysCtx {
                auth: &self.state.identity.auth,
                backend: &self.backend,
                pin_store: &self.pin_store,
                device_key_store: &self.device_key_store,
                outbox: &mut self.state.runtime_outbox,
                logs: &mut self.state.logs,
            })
            .ensure_registered()
            .await
        };
        if let Err(error) = enrollment_result {
            self.state
                .record_log(format!("device enrollment deferred: {error}"));
            self.device_enrollment_retry_after = Some(Instant::now() + Duration::from_secs(30));
            return;
        }
        self.device_enrollment_satisfied = true;
        self.device_enrollment_retry_after = None;

        self.backend_reconciliation = BackendReconciliationState::Idle;
        crate::runtime::auth::ensure_user_events_stream(self);
        self.state
            .queue_discovery_refresh([DiscoverySurface::Friends, DiscoverySurface::OwnSessions]);
    }

    async fn refresh_clipboard_support(&mut self) {
        let supported = self.clipboard.is_available()
            && self.state.config.permissions.allow_terminal_clipboard_write;
        let ids = self.state.local.sessions.ids().to_vec();
        for message in self
            .state
            .local
            .owned_session_runtimes
            .sync_clipboard_support(&ids, supported)
            .await
        {
            self.state.record_log(message);
        }
    }

    pub(crate) async fn process_hosted_sharing_work(&mut self) {
        self.process_hosted_sharing_work_with(
            &crate::runtime::sharing::LiveSharingMaintenanceBackend,
        )
        .await;
    }

    pub(crate) async fn process_hosted_sharing_work_with<
        B: crate::runtime::sharing::SharingMaintenanceBackend,
    >(
        &mut self,
        backend: &B,
    ) {
        self.process_backend_scope_restore_work(backend).await;
        self.process_host_key_rotation_work(backend).await;
        self.process_key_redistribution_work().await;
        self.retry_stalled_host_relays();
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one actor-side pump keeps worker completion, retry policy, durable selection, and single-flight spawn ordered"
    )]
    pub(crate) async fn process_collaboration_teardown_worker(&mut self) {
        use crate::runtime::collaboration_teardown_worker::CollaborationTeardownTaskResult;

        if self
            .collaboration_teardown_task
            .as_ref()
            .is_some_and(tokio::task::JoinHandle::is_finished)
        {
            if self.state.catalog_refresh == crate::runtime::state::CatalogRefreshState::Idle {
                self.state.catalog_refresh =
                    crate::runtime::state::CatalogRefreshState::CheckPending;
            }
            let Some(task) = self.collaboration_teardown_task.take() else {
                return;
            };
            let result = task.await;
            match result {
                Ok(completion) => {
                    let create_idempotency_id = completion.create_idempotency_id;
                    match completion.result {
                        CollaborationTeardownTaskResult::Settled(message)
                        | CollaborationTeardownTaskResult::Quarantined(message) => {
                            self.state.record_log(message);
                            self.collaboration_teardown_retry_until
                                .remove(&create_idempotency_id);
                            self.collaboration_teardown_deferred_until
                                .remove(&create_idempotency_id);
                            self.collaboration_teardown_retry_after = None;
                        }
                        CollaborationTeardownTaskResult::Deferred {
                            create_idempotency_id,
                            message,
                        } => {
                            self.state.record_log(message);
                            self.collaboration_teardown_deferred_until.insert(
                                create_idempotency_id,
                                Instant::now() + Duration::from_secs(30),
                            );
                            self.collaboration_teardown_retry_after = None;
                        }
                        CollaborationTeardownTaskResult::Retry => {
                            self.collaboration_teardown_retry_until.insert(
                                create_idempotency_id,
                                Instant::now() + Duration::from_secs(5),
                            );
                            self.collaboration_teardown_retry_after = None;
                        }
                        CollaborationTeardownTaskResult::Pause => {
                            let refresh =
                                crate::runtime::auth::refresh_access_token_after_rejection(
                                    self,
                                    completion
                                        .rejected_access_token
                                        .as_deref()
                                        .map(String::as_str),
                                )
                                .await;
                            match refresh {
                                Ok(crate::identity_core::stored_auth::RefreshStoredAuthResult::Refreshed(_)) => {
                                    self.collaboration_teardown_retry_until
                                        .remove(&create_idempotency_id);
                                }
                                Ok(crate::identity_core::stored_auth::RefreshStoredAuthResult::RequiresLogin(reason)) => {
                                    if let Err(error) = crate::runtime::auth::mark_expired_from_backend(
                                        self,
                                        reason.to_string(),
                                    ) {
                                        self.state.record_log(error.to_string());
                                    }
                                }
                                Ok(crate::identity_core::stored_auth::RefreshStoredAuthResult::TemporarilyUnavailable(reason)) => {
                                    crate::runtime::auth::note_reconnecting(self, reason.to_string());
                                    self.collaboration_teardown_retry_until.insert(
                                        create_idempotency_id,
                                        Instant::now() + Duration::from_secs(30),
                                    );
                                }
                                Err(error) => {
                                    self.state.record_log(format!(
                                        "collaboration cleanup token refresh failed: {error}"
                                    ));
                                    self.collaboration_teardown_retry_until.insert(
                                        create_idempotency_id,
                                        Instant::now() + Duration::from_secs(30),
                                    );
                                }
                            }
                            self.collaboration_teardown_retry_after = None;
                        }
                        CollaborationTeardownTaskResult::StoreError(error) => {
                            self.state.record_log(format!(
                                "collaboration cleanup persistence failed: {error}"
                            ));
                            self.collaboration_teardown_retry_after =
                                Some(Instant::now() + Duration::from_secs(30));
                        }
                    }
                }
                Err(error) => {
                    self.state
                        .record_log(format!("collaboration cleanup task failed: {error}"));
                    self.collaboration_teardown_retry_after =
                        Some(Instant::now() + Duration::from_secs(5));
                }
            }
        }
        if self.collaboration_teardown_task.is_some() {
            return;
        }
        if self
            .collaboration_teardown_retry_after
            .is_some_and(|retry_after| Instant::now() < retry_after)
        {
            return;
        }
        self.collaboration_teardown_retry_after = None;
        let now = Instant::now();
        self.collaboration_teardown_retry_until
            .retain(|_, retry_after| now < *retry_after);
        self.collaboration_teardown_deferred_until
            .retain(|_, retry_after| now < *retry_after);

        let Some(account_subject) = self.state.identity.auth.subject_string() else {
            return;
        };
        let Some(backend_origin) = self.backend.backend_origin().cloned() else {
            return;
        };
        let obligations = match self
            .collaboration_teardown
            .list_for_account(&backend_origin, &account_subject)
        {
            Ok(obligations) => obligations,
            Err(error) => {
                self.state.record_log(format!(
                    "collaboration cleanup store could not be read: {error}"
                ));
                self.collaboration_teardown_retry_after =
                    Some(Instant::now() + Duration::from_secs(30));
                return;
            }
        };
        let Some(obligation) = obligations.into_iter().find(|obligation| {
            !self
                .collaboration_teardown_retry_until
                .contains_key(&obligation.create_idempotency_id)
                && !self
                    .collaboration_teardown_deferred_until
                    .contains_key(&obligation.create_idempotency_id)
                && !obligation
                    .backend_incarnation_id
                    .is_some_and(|incarnation_id| {
                        self.state
                            .sharing
                            .shared_sessions
                            .contains_exact_backend_incarnation(
                                &obligation.backend_session_id,
                                incarnation_id,
                            )
                    })
                && !self.share_transition_holds_cleanup(obligation.create_idempotency_id)
        }) else {
            return;
        };

        let backend = self.backend.clone();
        let store = self.collaboration_teardown.clone();
        self.collaboration_teardown_task = Some(tokio::spawn(async move {
            crate::runtime::collaboration_teardown_worker::run(backend, store, obligation).await
        }));
    }

    async fn process_backend_scope_restore_work<
        B: crate::runtime::sharing::SharingMaintenanceBackend,
    >(
        &mut self,
        backend: &B,
    ) {
        for repair in self.state.pending_work.backend_scope_restores() {
            let id = repair.session_id;
            let Some(record) = self.state.local.sessions.record(id) else {
                if self
                    .state
                    .pending_work
                    .clear_backend_scope_restore_if_same(&repair)
                {
                    crate::runtime::sharing::retire_stale_backend_scope_repair(self, &repair);
                }
                continue;
            };
            if session_state_is_terminal(record.summary.state) {
                queue_terminal_scope_cleanup(self, &repair);
                self.state.record_log(format!(
                    "{} skipped terminal scope repair and queued backend cleanup",
                    id.short()
                ));
                continue;
            }
            let restore_result = backend.restore_scope(self, &repair).await;
            if self
                .state
                .local
                .sessions
                .record(id)
                .is_none_or(|record| session_state_is_terminal(record.summary.state))
            {
                queue_terminal_scope_cleanup(self, &repair);
                self.state.record_log(format!(
                    "{} terminalized during scope repair; queued backend cleanup",
                    id.short()
                ));
                continue;
            }
            match restore_result {
                Ok(()) => {
                    if self
                        .state
                        .pending_work
                        .clear_backend_scope_restore_if_same(&repair)
                    {
                        crate::runtime::sharing::resume_after_backend_scope_restore(self, &repair);
                    }
                }
                Err(AppError::NotFound) => {
                    if self
                        .state
                        .pending_work
                        .clear_backend_scope_restore_if_same(&repair)
                    {
                        crate::runtime::sharing::retire_stale_backend_scope_repair(self, &repair);
                    }
                }
                Err(error) => {
                    if self.state.pending_work.backend_scope_restore(id).is_none() {
                        self.state
                            .pending_work
                            .queue_backend_scope_restore(repair.clone());
                    }
                    self.state
                        .local
                        .sessions
                        .update_state(id, SessionState::Reconnecting);
                    self.state.record_log(format!(
                        "{} backend sharing scope repair failed; retrying: {error}",
                        id.short()
                    ));
                    self.state.sync_host_relay_status();
                }
            }
        }
    }

    async fn process_host_key_rotation_work<
        B: crate::runtime::sharing::SharingMaintenanceBackend,
    >(
        &mut self,
        backend: &B,
    ) {
        let now = Instant::now();
        let retry_cooldown = Duration::from_secs(30);
        for id in self.state.pending_work.drain_host_key_rotations() {
            if self.state.pending_work.backend_scope_restore(id).is_some() {
                self.state.pending_work.queue_host_key_rotation(id);
                continue;
            }
            if !self
                .state
                .sharing
                .host_relays
                .retry_due(id, now, retry_cooldown)
            {
                self.state.pending_work.queue_host_key_rotation(id);
                continue;
            }
            self.state.sharing.host_relays.mark_retry(id, now);
            if let Err(error) = backend.rotate_key(self, id).await {
                self.state.record_log(format!(
                    "{} key rotation after revoke failed: {error}",
                    id.short()
                ));
                self.state.pending_work.queue_host_key_rotation(id);
            }
        }
    }

    async fn process_key_redistribution_work(&mut self) {
        for work in self.state.pending_work.drain_key_redistributions() {
            let id = work.session_id;
            if self.state.pending_work.backend_scope_restore(id).is_some() {
                self.state.pending_work.requeue_key_redistribution(work);
                continue;
            }
            match crate::runtime::sharing::redistribute_existing_session_key(self, id).await {
                Ok(crate::sharing::scope::SessionKeyDistributionResult::Distributed) => {
                    for (index, fence_id) in work.fence_ids.iter().enumerate() {
                        if let Err(error) = self
                            .state
                            .sharing
                            .host_relays
                            .send_fence_completion(id, fence_id.clone())
                        {
                            self.state.record_log(format!(
                                "{} key redistribution fence completion deferred: {error}",
                                id.short()
                            ));
                            self.state.pending_work.requeue_key_redistribution(
                                crate::runtime::pending_work::KeyRedistributionWork {
                                    session_id: id,
                                    fence_ids: work.fence_ids[index..].to_vec(),
                                },
                            );
                            break;
                        }
                    }
                }
                Ok(crate::sharing::scope::SessionKeyDistributionResult::PendingRecipients) => {
                    self.state.pending_work.requeue_key_redistribution(work);
                }
                Err(error) => {
                    self.state.record_log(format!(
                        "{} key redistribution for newly registered device failed: {error}",
                        id.short()
                    ));
                    self.state.pending_work.requeue_key_redistribution(work);
                }
            }
        }
    }

    fn retry_stalled_host_relays(&mut self) {
        let now = Instant::now();
        let stalled: Vec<SessionId> = self
            .state
            .local
            .sessions
            .ids()
            .iter()
            .copied()
            .filter(|&id| {
                self.state.local.sessions.record(id).is_some_and(|r| {
                    self.state.sharing.shared_sessions.contains(id)
                        && self.state.pending_work.backend_scope_restore(id).is_none()
                        && !self.state.sharing.host_relays.active(id)
                        && !matches!(
                            r.summary.state,
                            SessionState::Stopping | SessionState::Stopped | SessionState::Failed
                        )
                        && self.state.sharing.host_relays.retry_due(
                            id,
                            now,
                            Duration::from_secs(30),
                        )
                })
            })
            .collect();

        for id in stalled {
            self.state.sharing.host_relays.mark_retry(id, now);
            if let Err(error) = self.start_maintenance_relay_prepare_worker(id) {
                tracing::debug!(%error, session_id = %id, "relay retry deferred");
            }
        }
    }

    pub(crate) async fn drain_session_events(&mut self) -> bool {
        let force_snapshot = self.flush_pending_session_events().await;
        self.run_periodic_maintenance_throttled().await;
        self.flush_pending_session_events().await | force_snapshot
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one refresh pass preserves cross-surface ordering and shared retry state"
    )]
    async fn process_pending_discovery_refresh(&mut self) {
        let mut surfaces = self.state.take_pending_discovery_refresh();
        let room_projection_refreshes = self.state.take_pending_room_projection_refreshes();
        if surfaces.is_empty() && room_projection_refreshes.is_empty() {
            return;
        }
        if !self.remote_surfaces_ready() {
            self.state.queue_discovery_refresh(surfaces);
            for (room_id, room_surfaces) in room_projection_refreshes {
                self.state
                    .queue_room_projection_refresh(room_id, room_surfaces);
            }
            self.room_mailbox_retry.reset();
            return;
        }

        let requested_mailbox_surfaces = mailbox_surfaces(&surfaces);
        let mailbox_due = self.room_mailbox_retry.is_due(Instant::now());
        if !requested_mailbox_surfaces.is_empty() && !mailbox_due {
            self.state
                .queue_discovery_refresh(requested_mailbox_surfaces.iter().copied());
            surfaces.retain(|surface| !is_mailbox_surface(*surface));
        }

        let mut failed_room_projection_surfaces = BTreeSet::new();
        for (room_id, mut room_surfaces) in room_projection_refreshes {
            if !mailbox_due {
                let deferred = mailbox_surfaces(&room_surfaces);
                if !deferred.is_empty() {
                    self.state
                        .queue_room_projection_refresh(room_id.clone(), deferred);
                    room_surfaces.retain(|surface| !is_mailbox_surface(*surface));
                }
            }
            if room_surfaces.is_empty() {
                continue;
            }
            if let Err(error) = crate::runtime::room_mailbox::refresh_room_projections(
                self,
                &room_id,
                &room_surfaces,
            )
            .await
            {
                failed_room_projection_surfaces.extend(room_surfaces.iter().copied());
                self.state
                    .queue_room_projection_refresh(room_id.clone(), room_surfaces);
                self.state.record_log(format!(
                    "room {room_id} projection refresh failed; stale data retained: {error}"
                ));
            }
        }
        surfaces.retain(|surface| !failed_room_projection_surfaces.contains(surface));
        if surfaces.is_empty() {
            return;
        }

        if surfaces.contains(&DiscoverySurface::Friends)
            && self.state.identity.auth.is_authenticated()
        {
            self.friends_ctx()
                .refresh_snapshot("discovery.invalidated")
                .await;
        }

        if surfaces.contains(&DiscoverySurface::RoomCatalog)
            && self.state.identity.auth.is_authenticated()
            && let Err(error) = refresh_room_catalog_events(self).await
        {
            self.state
                .queue_discovery_refresh([DiscoverySurface::RoomCatalog]);
            self.state.record_log(format!(
                "room catalog projection refresh failed; stale data retained: {error}"
            ));
        }

        if (surfaces.contains(&DiscoverySurface::RoomChat)
            || surfaces.contains(&DiscoverySurface::RoomTasks))
            && self.state.identity.auth.is_authenticated()
        {
            match crate::runtime::room_mailbox::sync_invalidated(self, &surfaces).await {
                Ok(outcome) => {
                    self.room_mailbox_retry.reset();
                    if outcome.chat_more {
                        self.state
                            .queue_discovery_refresh([DiscoverySurface::RoomChat]);
                    }
                    if outcome.tasks_more {
                        self.state
                            .queue_discovery_refresh([DiscoverySurface::RoomTasks]);
                    }
                }
                Err(error) => {
                    if let Some(delay) = schedule_room_mailbox_retry(
                        &mut self.state.pending_discovery_surfaces,
                        &mut self.room_mailbox_retry,
                        &requested_mailbox_surfaces,
                        &error,
                        Instant::now(),
                    ) {
                        self.state.record_log(format!(
                            "room mailbox delivery failed; retrying in {} ms: {error}",
                            delay.as_millis()
                        ));
                    } else {
                        self.room_mailbox_retry.reset();
                        self.state
                            .record_log(format!("room mailbox delivery failed: {error}"));
                    }
                }
            }
        }

        crate::runtime::discovery::refresh(self).await;
        if self.state.catalog_refresh == crate::runtime::state::CatalogRefreshState::Idle {
            self.state.catalog_refresh = crate::runtime::state::CatalogRefreshState::CheckPending;
        }
    }

    async fn drain_identity_lifecycle_work(&mut self) {
        use crate::identity_core::device_list_pin_store::IdentityLifecycleApplyOutcome;

        while let Some(work) = self.state.pending_work.pop_identity_lifecycle() {
            match self
                .pin_store
                .apply_identity_lifecycle(&work.user_id, work.identity_revision, work.state)
                .await
            {
                Ok(IdentityLifecycleApplyOutcome::Stale) => {}
                Ok(IdentityLifecycleApplyOutcome::Recorded) => {
                    if matches!(
                        work.state,
                        kodosi_domain::user::IdentityLifecycleState::Enrolled { .. }
                    ) {
                        self.state.pending_work.queue_pin_refresh(&work.user_id);
                    }
                }
                Ok(IdentityLifecycleApplyOutcome::PinCleared) => {
                    if matches!(
                        work.state,
                        kodosi_domain::user::IdentityLifecycleState::Enrolled { .. }
                    ) {
                        self.state.record_log(format!(
                            "{} identity changed; explicit verification is required",
                            work.user_id
                        ));
                    }
                }
                Err(error) => {
                    self.state.record_log(format!(
                        "identity lifecycle persistence failed for {}: {error}",
                        work.user_id
                    ));
                    self.state.pending_work.queue_identity_lifecycle(work);
                    return;
                }
            }
        }
        if self.state.pending_work.take_identity_lifecycle_overflow() {
            crate::runtime::auth::stop_user_events_stream(self);
            crate::runtime::auth::ensure_user_events_stream(self);
            self.last_maintenance_ran = None;
            self.state.record_log(
                "identity lifecycle snapshot exceeded the local queue; reconnecting to resume"
                    .to_owned(),
            );
        }
    }

    async fn drain_pin_reset_work(&mut self) {
        if self.state.pending_work.take_pin_reset_all() {
            if let Err(error) = self.pin_store.reset_all().await {
                tracing::warn!(%error, "failed to apply coalesced device-list pin reset");
                self.state.pending_work.queue_pin_reset_all();
            }
            return;
        }
        for user_id in self.state.pending_work.drain_pin_resets() {
            if let Err(error) = self.pin_store.reset(&user_id).await {
                tracing::warn!(
                    user_id = %user_id,
                    %error,
                    "failed to drop pin on identity-withdrawn broadcast"
                );
                self.state.pending_work.queue_pin_reset(&user_id);
            }
        }
    }

    async fn drain_pin_refresh_work(&mut self) {
        use crate::identity_core::device_list_pin_store::{PinContext, PinVerdict};

        if !self.remote_surfaces_ready() {
            return;
        }

        for work in self.state.pending_work.drain_pin_refreshes() {
            let user_id = work.user_id.clone();
            let is_self = self.state.identity.auth.subject_string().as_deref() == Some(&user_id);
            let result = if is_self {
                self.device_list_ctx().refresh().await
            } else {
                async {
                    let bundle = self.backend.fetch_user_identity(&user_id).await?;
                    let view =
                        crate::runtime::identity::backend_adapters::identity_bundle_view(&bundle)?;
                    match self
                        .pin_store
                        .verify_or_pin(view, PinContext::BackgroundFetch)
                        .await?
                    {
                        PinVerdict::FirstShare
                        | PinVerdict::AcceptUpdate
                        | PinVerdict::AlreadyPinned => Ok(()),
                        PinVerdict::Reject { reason } => Err(AppError::PeerIdentityChanged {
                            user_id: user_id.clone(),
                            detail: format!("{reason:?}"),
                        }),
                    }
                }
                .await
            };
            if let Err(error) = result {
                tracing::warn!(user_id = %user_id, %error, "device-list pin refresh failed");
                if is_self {
                    self.state.runtime_outbox.queue_devices(
                        crate::host_protocol::DeviceEvent::Error {
                            user_code: None,
                            operation: "refresh".to_owned(),
                            message: error.to_string(),
                        },
                    );
                }
                if pin_refresh_is_retryable(&error) {
                    self.state.pending_work.requeue_pin_refresh(work);
                }
            }
        }
    }

    pub(crate) fn drain_ready_deletes(&mut self) {
        let now = Instant::now();
        for work in self.state.pending_work.drain_ready_deletes(now) {
            let session_id = work.session_id;
            let result = crate::runtime::local_sessions::lifecycle(self).delete(session_id);
            if let Err(error) = result {
                self.state.pending_work.requeue_ready_delete(work, now);
                self.state.record_log(format!(
                    "{} deferred delete failed: {error}",
                    session_id.short()
                ));
            }
        }
    }

    pub(crate) fn ensure_project_discovery(
        &mut self,
        working_dir: &str,
        allow_stale_refresh: bool,
    ) {
        ensure_project_discovery(
            &mut self.state,
            &self.session_events_tx,
            &self.shutdown,
            working_dir,
            allow_stale_refresh,
        );
    }

    pub(crate) fn spawn_project_discovery(&self, request: ProjectDiscoveryRequest) {
        spawn_project_discovery(
            request,
            self.session_events_tx.clone(),
            self.shutdown.clone(),
        );
    }
}

async fn refresh_room_catalog_events(app: &mut Runtime) -> crate::Result<()> {
    crate::runtime::runtime_loop::push_room_snapshot(app).await?;
    crate::runtime::runtime_loop::push_invitations(app).await
}

fn is_mailbox_surface(surface: DiscoverySurface) -> bool {
    matches!(
        surface,
        DiscoverySurface::RoomChat | DiscoverySurface::RoomTasks
    )
}

fn session_state_is_terminal(state: SessionState) -> bool {
    matches!(
        state,
        SessionState::Stopping | SessionState::Stopped | SessionState::Failed
    )
}

fn queue_terminal_scope_cleanup(app: &mut Runtime, repair: &BackendScopeRestore) {
    app.state
        .pending_work
        .clear_backend_scope_restore_if_same(repair);
    app.last_maintenance_ran = None;
}

fn clone_access_mutation(
    entry: Option<&crate::runtime::access_mutations::PreparedSessionAccessMutation>,
) -> Option<crate::runtime::access_mutations::PreparedSessionAccessMutation> {
    entry.cloned()
}

fn mailbox_surfaces(surfaces: &BTreeSet<DiscoverySurface>) -> BTreeSet<DiscoverySurface> {
    surfaces
        .iter()
        .copied()
        .filter(|surface| is_mailbox_surface(*surface))
        .collect()
}

fn schedule_room_mailbox_retry(
    pending: &mut BTreeSet<DiscoverySurface>,
    retry: &mut crate::runtime::room_mailbox::RoomMailboxRetryState,
    requested: &BTreeSet<DiscoverySurface>,
    error: &AppError,
    now: Instant,
) -> Option<Duration> {
    if !is_transient_room_mailbox_error(error) {
        return None;
    }
    pending.extend(requested);
    Some(retry.record_failure(now))
}

fn is_transient_room_mailbox_error(error: &AppError) -> bool {
    match error {
        AppError::Io(error) => matches!(
            error.kind(),
            std::io::ErrorKind::Interrupted
                | std::io::ErrorKind::WouldBlock
                | std::io::ErrorKind::TimedOut
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::NotConnected
                | std::io::ErrorKind::BrokenPipe
        ),
        AppError::Http(_)
        | AppError::WebSocket(_)
        | AppError::Keychain { .. }
        | AppError::Unauthorized
        | AppError::NotFound => true,
        AppError::HttpProblem { status, .. } => {
            matches!(*status, 408 | 425 | 429) || *status >= 500
        }
        AppError::Unsupported { reason } => reason.starts_with("backend operation timed out:"),
        _ => false,
    }
}

pub(crate) fn ensure_project_discovery(
    state: &mut crate::runtime::AppState,
    session_events_tx: &tokio::sync::mpsc::Sender<RuntimeSessionEvent>,
    shutdown: &tokio_util::sync::CancellationToken,
    working_dir: &str,
    allow_stale_refresh: bool,
) {
    if let Some(request) = state.queue_project_discovery(working_dir, allow_stale_refresh) {
        spawn_project_discovery(request, session_events_tx.clone(), shutdown.clone());
    }
}

fn spawn_project_discovery(
    request: ProjectDiscoveryRequest,
    session_events: tokio::sync::mpsc::Sender<RuntimeSessionEvent>,
    shutdown: tokio_util::sync::CancellationToken,
) {
    tokio::spawn(async move {
        let working_dir = request.working_dir;
        let join_result = tokio::task::spawn_blocking({
            let working_dir = working_dir.clone();
            move || discover_project(&working_dir)
        })
        .await;

        if shutdown.is_cancelled() {
            return;
        }

        let event = match join_result {
            Ok(discovery) => RuntimeSessionEvent::ProjectDiscoveryReady {
                working_dir,
                discovery,
            },
            Err(error) => RuntimeSessionEvent::ProjectDiscoveryFailed {
                working_dir,
                message: format!("background project discovery panicked: {error}"),
            },
        };

        drop(session_events.send(event).await);
    });
}

#[cfg(test)]
mod access_mutation_tests {
    use kodosi_domain::{
        auth::AuthState,
        ids::{SessionId, UserId},
    };
    use tokio_util::sync::CancellationToken;
    use uuid::Uuid;

    use super::Runtime;
    use crate::{
        config::AppConfig,
        runtime::{
            access_mutation_worker::{
                SessionAccessMutationWorkerCompletion, SessionAccessMutationWorkerMode,
                SessionAccessMutationWorkerOutcome,
            },
            access_mutations::{
                PreparedSessionAccessMutation, PreparedSessionAccessMutationState,
                SessionAccessMutationTarget,
            },
        },
    };

    #[tokio::test]
    async fn runtime_drop_aborts_hanging_access_effect_and_relay_prepare_workers() {
        struct DropSignal(std::sync::Arc<std::sync::atomic::AtomicUsize>);
        impl Drop for DropSignal {
            fn drop(&mut self) {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }

        let mut config = AppConfig::default();
        config.auth.keyring_service = format!("kodosi.test.{}", Uuid::now_v7());
        let mut app = Runtime::with_dependencies(
            config.clone(),
            CancellationToken::new(),
            crate::runtime::RuntimeDependencies::isolated(&config)
                .expect("isolated runtime dependencies"),
        )
        .unwrap();
        let dropped = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let access_guard = DropSignal(std::sync::Arc::clone(&dropped));
        app.access_effect_task = Some(tokio::spawn(async move {
            let _guard = access_guard;
            std::future::pending().await
        }));
        let relay_guard = DropSignal(std::sync::Arc::clone(&dropped));
        app.relay_prepare_tasks.insert(
            SessionId::new(),
            tokio::spawn(async move {
                let _guard = relay_guard;
                std::future::pending().await
            }),
        );

        drop(app);
        for _ in 0..10 {
            if dropped.load(std::sync::atomic::Ordering::SeqCst) == 2 {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(dropped.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn recovered_prepared_mutation_dispatches_instead_of_polling_absent_receipt() {
        let mut config = AppConfig::default();
        config.auth.keyring_service = format!("kodosi.test.{}", Uuid::now_v7());
        let mut app = Runtime::with_dependencies(
            config.clone(),
            CancellationToken::new(),
            crate::runtime::RuntimeDependencies::isolated(&config)
                .expect("isolated runtime dependencies"),
        )
        .unwrap();
        let account = Uuid::now_v7().to_string();
        app.state.identity.auth = AuthState::Authenticated {
            subject: Some(UserId::try_from(account.as_str()).unwrap()),
            expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
        };
        app.state
            .identity
            .set_account_epoch_for_test(crate::session_runtime::events::AccountEpoch::for_test(7));
        let mut recovered = prepared(&account, 7);
        recovered.state = PreparedSessionAccessMutationState::Prepared;
        app.access_mutations.put(recovered).unwrap();

        let selected = app.select_access_mutation_work(&account).unwrap();
        assert!(matches!(
            selected,
            Some((_, _, SessionAccessMutationWorkerMode::Dispatch))
        ));
    }

    #[tokio::test]
    async fn stale_account_completion_cannot_advance_durable_mutation() {
        let mut config = AppConfig::default();
        config.auth.keyring_service = format!("kodosi.test.{}", Uuid::now_v7());
        let mut app = Runtime::with_dependencies(
            config.clone(),
            CancellationToken::new(),
            crate::runtime::RuntimeDependencies::isolated(&config)
                .expect("isolated runtime dependencies"),
        )
        .unwrap();
        let account = Uuid::now_v7().to_string();
        app.state.identity.auth = AuthState::Authenticated {
            subject: Some(UserId::try_from(account.as_str()).unwrap()),
            expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
        };
        app.state
            .identity
            .set_account_epoch_for_test(crate::session_runtime::events::AccountEpoch::for_test(8));
        let prepared = prepared(&account, 7);
        app.access_mutations.put(prepared.clone()).unwrap();

        app.apply_access_mutation_completion(SessionAccessMutationWorkerCompletion {
            prepared: prepared.clone(),
            account_epoch: 7,
            mode: SessionAccessMutationWorkerMode::Reconcile,
            outcome: SessionAccessMutationWorkerOutcome::Applied,
            access_snapshot: None,
        })
        .await;

        assert_eq!(
            app.access_mutations
                .get(&account, prepared.mutation_id)
                .unwrap()
                .unwrap()
                .state,
            PreparedSessionAccessMutationState::Attempting
        );
        assert!(app.state.runtime_outbox.drain_sessions().is_empty());
    }

    #[tokio::test]
    async fn oversized_rejection_message_cannot_stall_terminal_persistence() {
        let mut config = AppConfig::default();
        config.auth.keyring_service = format!("kodosi.test.{}", Uuid::now_v7());
        let mut app = Runtime::with_dependencies(
            config.clone(),
            CancellationToken::new(),
            crate::runtime::RuntimeDependencies::isolated(&config)
                .expect("isolated runtime dependencies"),
        )
        .unwrap();
        let account = Uuid::now_v7().to_string();
        app.state.identity.auth = AuthState::Authenticated {
            subject: Some(UserId::try_from(account.as_str()).unwrap()),
            expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
        };
        app.state
            .identity
            .set_account_epoch_for_test(crate::session_runtime::events::AccountEpoch::for_test(7));
        let prepared = prepared(&account, 7);
        app.access_mutations.put(prepared.clone()).unwrap();

        app.apply_access_mutation_completion(SessionAccessMutationWorkerCompletion {
            prepared: prepared.clone(),
            account_epoch: 7,
            mode: SessionAccessMutationWorkerMode::Dispatch,
            outcome: SessionAccessMutationWorkerOutcome::Rejected(
                crate::AppError::InvalidBackendData {
                    field: "problem".to_owned(),
                    reason: "é"
                        .repeat(crate::runtime::access_mutations::MAX_TERMINAL_MESSAGE_BYTES),
                },
            ),
            access_snapshot: None,
        })
        .await;

        let terminal = match &app
            .access_mutations
            .get(&account, prepared.mutation_id)
            .unwrap()
            .unwrap()
            .state
        {
            PreparedSessionAccessMutationState::Terminal(terminal) => terminal,
            state => panic!("expected terminal mutation, got {state:?}"),
        };
        let message = terminal.message.as_ref().unwrap();
        assert!(message.len() <= crate::runtime::access_mutations::MAX_TERMINAL_MESSAGE_BYTES);
        assert!(message.ends_with('é'));
        assert!(message.is_char_boundary(message.len()));
    }

    fn prepared(account: &str, epoch: u64) -> PreparedSessionAccessMutation {
        let session = Uuid::now_v7();
        let incarnation = Uuid::now_v7();
        let mut prepared = PreparedSessionAccessMutation::new(
            Uuid::now_v7(),
            account.to_owned(),
            epoch,
            SessionId::parse_field(&session.to_string(), "sessionId").unwrap(),
            incarnation,
            session.to_string(),
            incarnation,
            SessionAccessMutationTarget::Leave,
        )
        .unwrap();
        prepared.state = PreparedSessionAccessMutationState::Attempting;
        prepared
    }
}

#[cfg(test)]
mod collaboration_teardown_tests {
    use super::{BackendReconciliationState, Runtime};
    use crate::identity_core::token_store::TokenStore as _;
    use crate::runtime::collaboration_teardown_worker::{DispatchOutcome, classify};
    use crate::{
        config::AppConfig,
        sharing::{
            collaboration_teardown_obligations::QuarantineReason,
            shared_session_registry::SharedSessionState,
        },
    };
    use kodosi_backend_client::BackendClientError;
    use kodosi_domain::{
        auth::AuthState,
        ids::{SessionId, UserId},
        permissions::ShareScope,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_util::sync::CancellationToken;

    async fn delete_server() -> (String, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut request = [0_u8; 4096];
            let read = socket.read(&mut request).await.expect("read request");
            socket
                .write_all(
                    b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await
                .expect("write response");
            String::from_utf8_lossy(&request[..read]).into_owned()
        });
        (format!("http://{address}/"), server)
    }

    async fn hanging_delete_server() -> (String, tokio::sync::oneshot::Receiver<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let (accepted_tx, accepted_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut request = [0_u8; 4096];
            let read = socket.read(&mut request).await.expect("read request");
            assert!(read > 0, "worker should send an HTTP request");
            accepted_tx.send(()).ok();
            std::future::pending::<()>().await;
        });
        (format!("http://{address}/"), accepted_rx)
    }

    async fn profile_server(user_id: &str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let body = format!(
            r#"{{"id":"{user_id}","handle":"account-b","displayName":"Account B","email":null,"avatarUrl":null}}"#
        );
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut request = [0_u8; 4096];
            let read = socket.read(&mut request).await.expect("read request");
            assert!(read > 0);
            socket
                .write_all(json_response(&body).as_bytes())
                .await
                .expect("write profile");
        });
        format!("http://{address}/")
    }

    async fn recorded_profile_server(user_id: &str) -> (String, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let body = format!(
            r#"{{"id":"{user_id}","handle":"profile","displayName":"Profile","email":null,"avatarUrl":null}}"#
        );
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut request = [0_u8; 4096];
            let read = socket.read(&mut request).await.expect("read request");
            socket
                .write_all(json_response(&body).as_bytes())
                .await
                .expect("write profile");
            String::from_utf8_lossy(&request[..read]).into_owned()
        });
        (format!("http://{address}/"), server)
    }

    async fn token_replacing_profile_server(
        user_id: &str,
        token_store: crate::identity_core::token_store::PlatformTokenStore,
        replacement_user_id: &str,
    ) -> (String, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let origin = format!("http://{address}/");
        let replacement_origin = origin
            .parse::<kodosi_backend_client::BackendOrigin>()
            .expect("backend origin")
            .to_string();
        let replacement_user_id = replacement_user_id.to_owned();
        let body = format!(
            r#"{{"id":"{user_id}","handle":"profile","displayName":"Profile","email":null,"avatarUrl":null}}"#
        );
        let server = tokio::spawn(async move {
            use crate::identity_core::token_store::TokenStore as _;

            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut request = [0_u8; 4096];
            let read = socket.read(&mut request).await.expect("read request");
            token_store
                .save(
                    crate::runtime::DEFAULT_TOKEN_SUBJECT,
                    &crate::identity_core::token_store::StoredTokens::new(
                        "replacement-access-token".to_owned(),
                        None,
                        time::OffsetDateTime::now_utc() + time::Duration::hours(1),
                    )
                    .bind_backend_account(replacement_origin, replacement_user_id),
                )
                .expect("replace stored token");
            socket
                .write_all(json_response(&body).as_bytes())
                .await
                .expect("write profile");
            String::from_utf8_lossy(&request[..read]).into_owned()
        });
        (origin, server)
    }

    async fn rejected_token_refresh_server() -> (String, tokio::task::JoinHandle<Vec<String>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let origin = format!("http://{address}/");
        let token_endpoint = format!("{origin}token");
        let server = tokio::spawn(async move {
            let mut requests = Vec::new();
            let mut delete_count = 0;
            while requests.len() < 4 {
                let (mut socket, _) = listener.accept().await.expect("accept");
                let mut request = vec![0_u8; 16 * 1024];
                let read = socket.read(&mut request).await.expect("read request");
                let request = String::from_utf8_lossy(&request[..read]).into_owned();
                let first_line = request.lines().next().unwrap_or_default();
                let response = if first_line.starts_with("DELETE ") {
                    delete_count += 1;
                    if delete_count == 1 {
                        empty_response("401 Unauthorized")
                    } else {
                        empty_response("204 No Content")
                    }
                } else if first_line.starts_with("GET /.well-known/openid-configuration ") {
                    json_response(&format!(r#"{{"token_endpoint":"{token_endpoint}"}}"#))
                } else if first_line.starts_with("POST /token ") {
                    json_response(
                        r#"{"access_token":"fresh-access-token","refresh_token":"fresh-refresh-token","expires_in":3600,"token_type":"Bearer"}"#,
                    )
                } else {
                    empty_response("404 Not Found")
                };
                requests.push(request);
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write response");
            }
            requests
        });
        (origin, server)
    }

    async fn response_sequence_server(
        responses: Vec<String>,
    ) -> (String, tokio::task::JoinHandle<Vec<String>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let mut requests = Vec::new();
            for response in responses {
                let (mut socket, _) = listener.accept().await.expect("accept");
                let mut request = [0_u8; 4096];
                let read = socket.read(&mut request).await.expect("read request");
                requests.push(String::from_utf8_lossy(&request[..read]).into_owned());
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write response");
            }
            requests
        });
        (format!("http://{address}/"), server)
    }

    fn empty_response(status: &str) -> String {
        format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
    }

    fn json_response(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn authenticated_runtime(api: String) -> Runtime {
        let mut config = AppConfig::default();
        config.backend.api = Some(api);
        config.auth.keyring_service = format!("kodosi.teardown.test.{}", uuid::Uuid::now_v7());
        let mut app = Runtime::with_dependencies(
            config.clone(),
            CancellationToken::new(),
            crate::runtime::RuntimeDependencies::isolated(&config)
                .expect("isolated runtime dependencies"),
        )
        .expect("runtime");
        app.state.identity.auth = AuthState::Authenticated {
            subject: Some(
                UserId::try_from("01900000-0000-7000-8000-000000000001").expect("user ID"),
            ),
            expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
        };
        app.device_enrollment_satisfied = true;
        app
    }

    fn provision_bound(app: &Runtime, session_id: &str) -> (uuid::Uuid, uuid::Uuid, uuid::Uuid) {
        let create_id = uuid::Uuid::now_v7();
        let incarnation_id = uuid::Uuid::now_v7();
        let mutation_id = uuid::Uuid::now_v7();
        app.collaboration_teardown
            .provision(
                app.backend.backend_origin().expect("origin"),
                &app.state.identity.auth.subject_string().expect("subject"),
                session_id,
                create_id,
                mutation_id,
                1,
            )
            .expect("provision");
        app.collaboration_teardown
            .bind_incarnation(create_id, session_id, incarnation_id)
            .expect("bind");
        (create_id, incarnation_id, mutation_id)
    }

    async fn wait_for_cleanup_worker(app: &Runtime) {
        for _ in 0..100 {
            if app
                .collaboration_teardown_task
                .as_ref()
                .is_some_and(tokio::task::JoinHandle::is_finished)
            {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("cleanup worker did not finish");
    }

    async fn process_cleanup_until_idle(app: &mut Runtime) {
        for _ in 0..20 {
            app.process_collaboration_teardown_worker().await;
            if app.collaboration_teardown_task.is_none() {
                return;
            }
            wait_for_cleanup_worker(app).await;
        }
        panic!("cleanup worker did not become idle");
    }

    #[test]
    fn cleanup_settlement_is_scoped_to_current_backend_account() {
        let app = authenticated_runtime("https://api.example.test/".to_owned());
        let other_account = "22222222-2222-2222-2222-222222222222";
        app.collaboration_teardown
            .provision(
                app.backend.backend_origin().expect("origin"),
                other_account,
                "other-session",
                uuid::Uuid::now_v7(),
                uuid::Uuid::now_v7(),
                1,
            )
            .expect("provision other account cleanup");

        assert!(app.collaboration_cleanup_settled());
    }

    #[tokio::test]
    async fn profile_reconciliation_uses_the_exact_stored_token_snapshot() {
        use crate::identity_core::token_store::TokenStore as _;

        let user_id = "22222222-2222-2222-2222-222222222222";
        let (api, request) = recorded_profile_server(user_id).await;
        let mut app = authenticated_runtime(api);
        app.backend
            .set_access_token(Some("stale-runtime-token".to_owned().into()));
        app.token_store
            .save(
                crate::runtime::DEFAULT_TOKEN_SUBJECT,
                &crate::identity_core::token_store::StoredTokens::new(
                    "stored-snapshot-token".to_owned(),
                    None,
                    time::OffsetDateTime::now_utc() + time::Duration::hours(1),
                ),
            )
            .expect("stored snapshot");

        crate::runtime::auth::sync_current_user(&mut app)
            .await
            .expect("profile reconciliation");

        let request = request.await.expect("profile request");
        assert!(request.contains("authorization: Bearer stored-snapshot-token"));
        assert!(!request.contains("stale-runtime-token"));
        assert_eq!(
            app.token_store
                .peek_backend_account(crate::runtime::DEFAULT_TOKEN_SUBJECT)
                .expect("binding")
                .expect("bound account")
                .backend_user_id,
            user_id
        );
    }

    #[tokio::test]
    async fn profile_reconciliation_rejects_a_token_replaced_while_request_is_in_flight() {
        use crate::identity_core::token_store::TokenStore as _;

        let mut app = authenticated_runtime("http://127.0.0.1:9/".to_owned());
        let original_subject = app.state.identity.auth.subject();
        let replacement_user_id = "33333333-3333-3333-3333-333333333333";
        let (api, request) = token_replacing_profile_server(
            "22222222-2222-2222-2222-222222222222",
            app.token_store.clone(),
            replacement_user_id,
        )
        .await;
        app.backend = kodosi_backend_client::http_client::BackendHttpClient::new(
            &kodosi_backend_client::config::BackendClientConfig {
                api: Some(api),
                ..kodosi_backend_client::config::BackendClientConfig::default()
            },
        )
        .expect("profile backend");
        app.backend
            .set_access_token(Some("stale-runtime-token".to_owned().into()));
        app.token_store
            .save(
                crate::runtime::DEFAULT_TOKEN_SUBJECT,
                &crate::identity_core::token_store::StoredTokens::new(
                    "request-token".to_owned(),
                    None,
                    time::OffsetDateTime::now_utc() + time::Duration::hours(1),
                ),
            )
            .expect("request token");

        let error = crate::runtime::auth::sync_current_user(&mut app)
            .await
            .expect_err("replaced generation must reject profile binding");

        let request = request.await.expect("profile request");
        assert!(request.contains("authorization: Bearer request-token"));
        assert_eq!(app.state.identity.auth.subject(), original_subject);
        assert!(
            error
                .to_string()
                .contains("stored credential generation changed")
        );
        assert_eq!(
            app.token_store
                .peek_backend_account(crate::runtime::DEFAULT_TOKEN_SUBJECT)
                .expect("binding")
                .expect("replacement account")
                .backend_user_id,
            replacement_user_id
        );
    }

    #[tokio::test]
    async fn profile_driven_account_switch_aborts_old_cleanup_without_losing_obligation() {
        let (api_a, accepted) = hanging_delete_server().await;
        let mut app = authenticated_runtime(api_a);
        app.backend_compatibility_verified = true;
        app.backend_reconciliation = BackendReconciliationState::CleanupPending;
        let session_id = SessionId::new().to_string();
        provision_bound(&app, &session_id);
        app.process_collaboration_teardown_worker().await;
        tokio::time::timeout(std::time::Duration::from_secs(1), accepted)
            .await
            .expect("A cleanup dispatched")
            .expect("A server accepted");
        assert!(app.collaboration_teardown_task.is_some());

        let user_b = "22222222-2222-2222-2222-222222222222";
        let api_b = profile_server(user_b).await;
        app.backend = kodosi_backend_client::http_client::BackendHttpClient::new(
            &kodosi_backend_client::config::BackendClientConfig {
                api: Some(api_b),
                ..kodosi_backend_client::config::BackendClientConfig::default()
            },
        )
        .expect("B backend");
        app.backend
            .set_access_token(Some("stored-access-token".to_owned().into()));
        app.token_store
            .save(
                crate::runtime::DEFAULT_TOKEN_SUBJECT,
                &crate::identity_core::token_store::StoredTokens::new(
                    "stored-access-token".to_owned(),
                    None,
                    time::OffsetDateTime::now_utc() + time::Duration::hours(1),
                ),
            )
            .expect("stored credential generation");

        crate::runtime::auth::sync_current_user(&mut app)
            .await
            .expect("profile-driven account switch");

        assert_eq!(
            app.state.identity.auth.subject_string().as_deref(),
            Some(user_b)
        );
        assert!(app.collaboration_teardown_task.is_none());
        assert!(app.collaboration_teardown_retry_after.is_none());
        assert!(app.collaboration_teardown_deferred_until.is_empty());
        assert_eq!(
            app.collaboration_teardown
                .validate_and_health()
                .expect("health")
                .active_count,
            1,
            "aborting A transport must retain A's durable obligation"
        );
    }

    #[tokio::test]
    async fn periodic_maintenance_uses_worker_and_opens_remote_only_after_settlement() {
        use crate::identity_core::token_store::TokenStore as _;

        let (api, server) = delete_server().await;
        let mut app = authenticated_runtime(api);
        app.backend_compatibility_verified = true;
        app.backend_reconciliation = BackendReconciliationState::CleanupPending;
        app.last_auth_refresh_check = Some(std::time::Instant::now());
        app.token_store
            .save(
                crate::runtime::DEFAULT_TOKEN_SUBJECT,
                &crate::identity_core::token_store::StoredTokens::new(
                    "stored-access-token".to_owned(),
                    None,
                    time::OffsetDateTime::now_utc() + time::Duration::hours(1),
                ),
            )
            .expect("stored token");
        let session_id = SessionId::new().to_string();
        provision_bound(&app, &session_id);

        app.run_periodic_maintenance_force().await;
        assert!(app.collaboration_teardown_task.is_some());
        assert!(!app.remote_surfaces_ready());
        server.await.expect("server");
        wait_for_cleanup_worker(&app).await;

        app.run_periodic_maintenance_force().await;

        assert!(app.remote_surfaces_ready());
        assert_eq!(
            app.collaboration_teardown
                .validate_and_health()
                .expect("health")
                .active_count,
            0
        );
    }

    #[tokio::test]
    async fn expired_retry_marker_is_consumed_when_no_cleanup_is_eligible() {
        let mut app = authenticated_runtime("http://127.0.0.1:9/".to_owned());
        app.backend_compatibility_verified = true;
        app.backend_reconciliation = BackendReconciliationState::CleanupPending;
        let local_id = SessionId::new();
        let session_id = local_id.to_string();
        let (_, incarnation_id, _) = provision_bound(&app, &session_id);
        app.state.sharing.shared_sessions.insert(
            local_id,
            SharedSessionState::new(
                session_id,
                incarnation_id,
                "secret".to_owned(),
                ShareScope::MyDevices,
                None,
                None,
                None,
            ),
        );
        app.collaboration_teardown_retry_after =
            Some(std::time::Instant::now() - std::time::Duration::from_millis(1));

        app.process_collaboration_teardown_worker().await;
        app.finish_initial_collaboration_cleanup().await;

        assert!(app.collaboration_teardown_retry_after.is_none());
        assert!(app.remote_surfaces_ready());
    }

    #[tokio::test]
    async fn full_quarantine_defers_bad_head_and_settles_later_obligation() {
        let (api, server) = response_sequence_server(vec![
            empty_response("400 Bad Request"),
            empty_response("204 No Content"),
        ])
        .await;
        let mut app = authenticated_runtime(api);
        app.backend_compatibility_verified = true;
        app.backend_reconciliation = BackendReconciliationState::CleanupPending;
        let root = tempfile::tempdir().expect("store root");
        app.collaboration_teardown = crate::sharing::collaboration_teardown_obligations::CollaborationTeardownObligationStore::with_test_capacities(
            root.path().join("collaboration-teardown-obligations.json"),
            4,
            4,
            1,
        )
        .expect("bounded store");
        let seed_id = "seed";
        let good_id = "z-good";
        let (seed_create, seed_incarnation, seed_end) = provision_bound(&app, seed_id);
        let (_, _, _) = provision_bound(&app, good_id);
        app.collaboration_teardown
            .quarantine(
                seed_create,
                Some(seed_incarnation),
                seed_end,
                QuarantineReason::OperatorIntervention,
            )
            .expect("fill quarantine");
        let bad_id = "a-bad";
        let (_, _, _) = provision_bound(&app, bad_id);

        process_cleanup_until_idle(&mut app).await;
        server.await.expect("server");
        app.finish_initial_collaboration_cleanup().await;

        let requests = app
            .collaboration_teardown
            .list_for_account(
                app.backend.backend_origin().expect("origin"),
                &app.state.identity.auth.subject_string().expect("subject"),
            )
            .expect("remaining records");
        assert!(
            requests
                .iter()
                .any(|record| record.backend_session_id == bad_id)
        );
        assert!(
            !requests
                .iter()
                .any(|record| record.backend_session_id == good_id)
        );
        assert!(
            app.collaboration_teardown_deferred_until
                .values()
                .any(|retry_after| *retry_after > std::time::Instant::now()),
            "quarantine-capacity deferral must remain a readiness barrier"
        );
        assert_eq!(
            app.backend_reconciliation,
            BackendReconciliationState::CleanupPending
        );
        assert!(!app.remote_surfaces_ready());
    }

    #[tokio::test]
    async fn malformed_current_schema_startup_keeps_local_work_available_and_remote_fenced() {
        let mut app = authenticated_runtime("http://127.0.0.1:9/".to_owned());
        app.backend_compatibility_verified = true;
        let root = tempfile::tempdir().expect("store root");
        let store_path = root.path().join("collaboration-teardown-obligations.json");
        let malformed = br#"{
          "version": 1,
          "records": [{"backendOrigin":"https://example.test/"}],
          "quarantinedRecords": []
        }"#;
        std::fs::write(&store_path, malformed).expect("malformed current schema");
        app.collaboration_teardown = crate::sharing::collaboration_teardown_obligations::CollaborationTeardownObligationStore::at(
            store_path,
            8,
        )
        .expect("startup store");
        app.backend_reconciliation = BackendReconciliationState::CleanupQuarantined;

        let effects = crate::host_protocol::sessions_command::apply_session_command(
            &mut app,
            crate::SessionCommand::SnapshotRefresh,
        )
        .await
        .expect("local snapshot remains available");

        assert!(effects.defer_catalog_replay);
        assert!(!app.remote_surfaces_ready());
        assert_eq!(
            app.collaboration_cleanup_health().state,
            crate::CollaborationCleanupState::Quarantined
        );
        assert!(
            crate::runtime::auth::ensure_remote_operation_ready(&mut app)
                .await
                .is_err()
        );
        let sidecar = std::fs::read_dir(root.path())
            .expect("store root")
            .filter_map(std::result::Result::ok)
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("collaboration-teardown-obligations.corrupt-")
            })
            .expect("corruption sidecar");
        assert_eq!(
            std::fs::read(sidecar.path()).expect("sidecar bytes"),
            malformed
        );

        let restarted = crate::sharing::collaboration_teardown_obligations::CollaborationTeardownObligationStore::at(
            root.path().join("collaboration-teardown-obligations.json"),
            8,
        )
        .expect("restart store");
        app.collaboration_teardown = restarted;
        assert!(
            !app.remote_surfaces_ready(),
            "restart must preserve the fence"
        );

        assert_eq!(
            app.reset_collaboration_cleanup_quarantine()
                .expect("operator reset"),
            1
        );
        assert!(app.collaboration_cleanup_settled());
        assert_eq!(
            app.collaboration_cleanup_health().state,
            crate::CollaborationCleanupState::Healthy
        );
        assert!(
            std::fs::read_dir(root.path())
                .expect("store root after reset")
                .filter_map(std::result::Result::ok)
                .any(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("collaboration-teardown-obligations.resolved-"))
        );
    }

    #[tokio::test]
    async fn corruption_quarantine_keeps_remote_surfaces_fenced() {
        let mut app = authenticated_runtime("http://127.0.0.1:9/".to_owned());
        app.backend_compatibility_verified = true;
        app.backend_reconciliation = BackendReconciliationState::CleanupPending;
        let root = tempfile::tempdir().expect("store root");
        let store_path = root.path().join("collaboration-teardown-obligations.json");
        std::fs::write(
            root.path()
                .join("collaboration-teardown-obligations.corrupt-1.json"),
            b"preserved corrupt bytes",
        )
        .expect("corruption sidecar");
        app.collaboration_teardown =
            crate::sharing::collaboration_teardown_obligations::CollaborationTeardownObligationStore::at(
                store_path,
                8,
            )
            .expect("store");

        app.finish_initial_collaboration_cleanup().await;

        assert!(!app.remote_surfaces_ready());
        assert_eq!(
            app.collaboration_cleanup_health().message.as_deref(),
            Some(
                "Collaboration cleanup obligation store is quarantined; remote operations are disabled until explicit reset."
            )
        );
    }

    #[tokio::test]
    async fn failed_pin_refresh_is_requeued_for_retry() {
        let mut app = authenticated_runtime("http://127.0.0.1:9/".to_owned());
        app.backend_compatibility_verified = true;
        app.backend_reconciliation = BackendReconciliationState::Idle;
        app.state.pending_work.queue_pin_refresh("peer-user");

        app.drain_pin_refresh_work().await;

        assert_eq!(app.state.pending_work.pin_work_count(), 1);
        assert!(app.state.pending_work.drain_pin_refreshes().is_empty());
    }

    #[tokio::test]
    async fn cleanup_pending_keeps_pin_refresh_queued_without_backend_read() {
        let mut app = authenticated_runtime("http://127.0.0.1:9/".to_owned());
        app.backend_compatibility_verified = true;
        app.backend_reconciliation = BackendReconciliationState::CleanupPending;
        app.state.pending_work.queue_pin_refresh("peer-user");

        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            app.flush_pending_session_events(),
        )
        .await
        .expect("cleanup fence must avoid backend I/O");

        let queued = app.state.pending_work.drain_pin_refreshes();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].user_id, "peer-user");
    }

    #[tokio::test]
    async fn cleanup_unauthorized_forces_exact_rejected_token_refresh_before_retry() {
        use crate::identity_core::token_store::{StoredTokens, TokenStore as _};

        let (origin, server) = rejected_token_refresh_server().await;
        let mut app = authenticated_runtime(origin.clone());
        app.backend_compatibility_verified = true;
        app.backend_reconciliation = BackendReconciliationState::CleanupPending;
        let mut auth_config = crate::config::AuthConfig::default();
        auth_config.issuer = Some(origin);
        auth_config.client_id = "kodosi".to_owned();
        let client = reqwest::Client::builder()
            .build()
            .expect("HTTP test client");
        app.state.identity.device_flow = crate::runtime::identity::DeviceFlowRuntime::new(
            crate::identity_core::device_flow::DeviceFlowClient::new_for_test(&auth_config, client)
                .expect("test device flow"),
        );
        app.token_store
            .save(
                crate::runtime::DEFAULT_TOKEN_SUBJECT,
                &StoredTokens::new(
                    "rejected-access-token".to_owned(),
                    Some("refresh-token".to_owned()),
                    time::OffsetDateTime::now_utc() + time::Duration::hours(1),
                ),
            )
            .expect("store old token");
        app.backend
            .set_access_token(Some("rejected-access-token".to_owned().into()));
        let session_id = SessionId::new().to_string();
        provision_bound(&app, &session_id);

        app.process_collaboration_teardown_worker().await;
        wait_for_cleanup_worker(&app).await;
        app.process_collaboration_teardown_worker().await;
        assert!(app.collaboration_teardown_task.is_some());
        assert!(!app.remote_surfaces_ready());
        wait_for_cleanup_worker(&app).await;
        app.process_collaboration_teardown_worker().await;
        app.finish_initial_collaboration_cleanup().await;

        let requests = server.await.expect("server");
        assert_eq!(requests.len(), 4);
        assert!(requests[0].contains("authorization: Bearer rejected-access-token"));
        assert!(requests[1].starts_with("GET /.well-known/openid-configuration "));
        assert!(requests[2].starts_with("POST /token "));
        assert!(requests[2].contains("grant_type=refresh_token"));
        assert!(requests[3].contains("authorization: Bearer fresh-access-token"));
        assert!(app.remote_surfaces_ready());
        assert_eq!(
            app.collaboration_teardown
                .validate_and_health()
                .expect("health")
                .active_count,
            0
        );
        assert!(
            app.state.logs.iter().all(|message| {
                !message.contains("rejected-access-token")
                    && !message.contains("fresh-access-token")
                    && !message.contains("refresh-token")
            }),
            "bearer and refresh tokens must never enter runtime logs"
        );
    }

    #[tokio::test]
    async fn transient_retry_of_first_obligation_does_not_starve_later_cleanup() {
        let (api, server) = response_sequence_server(vec![
            empty_response("503 Service Unavailable"),
            empty_response("204 No Content"),
        ])
        .await;
        let mut app = authenticated_runtime(api);
        app.backend_compatibility_verified = true;
        app.backend_reconciliation = BackendReconciliationState::CleanupPending;
        let first_id = "a-retry";
        let later_id = "z-settle";
        let (first_create_id, _, _) = provision_bound(&app, first_id);
        provision_bound(&app, later_id);

        app.process_collaboration_teardown_worker().await;
        wait_for_cleanup_worker(&app).await;
        app.process_collaboration_teardown_worker().await;
        assert!(
            app.collaboration_teardown_retry_until
                .contains_key(&first_create_id)
        );
        assert!(app.collaboration_teardown_task.is_some());

        wait_for_cleanup_worker(&app).await;
        app.process_collaboration_teardown_worker().await;
        server.await.expect("server");
        app.finish_initial_collaboration_cleanup().await;

        let remaining = app
            .collaboration_teardown
            .list_for_account(
                app.backend.backend_origin().expect("origin"),
                &app.state.identity.auth.subject_string().expect("subject"),
            )
            .expect("remaining obligations");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].backend_session_id, first_id);
        assert!(!app.remote_surfaces_ready());
    }

    #[tokio::test]
    async fn transient_cleanup_keeps_remote_surfaces_fenced_until_retry_settles() {
        let (api, server) = response_sequence_server(vec![
            empty_response("503 Service Unavailable"),
            empty_response("204 No Content"),
        ])
        .await;
        let mut app = authenticated_runtime(api);
        app.backend_compatibility_verified = true;
        app.backend_reconciliation = BackendReconciliationState::CleanupPending;
        let session_id = SessionId::new().to_string();
        let (create_id, _, _) = provision_bound(&app, &session_id);

        app.process_collaboration_teardown_worker().await;
        wait_for_cleanup_worker(&app).await;
        app.process_collaboration_teardown_worker().await;
        app.finish_initial_collaboration_cleanup().await;

        assert!(app.collaboration_teardown_task.is_none());
        assert!(app.collaboration_teardown_retry_after.is_none());
        assert_eq!(app.collaboration_teardown_retry_until.len(), 1);
        assert!(!app.remote_surfaces_ready());
        assert_eq!(
            app.collaboration_teardown
                .validate_and_health()
                .expect("health")
                .active_count,
            1
        );

        app.collaboration_teardown_retry_until.insert(
            create_id,
            std::time::Instant::now() - std::time::Duration::from_millis(1),
        );
        app.process_collaboration_teardown_worker().await;
        server.await.expect("server");
        wait_for_cleanup_worker(&app).await;
        app.process_collaboration_teardown_worker().await;
        app.finish_initial_collaboration_cleanup().await;

        assert!(app.remote_surfaces_ready());
        assert_eq!(
            app.collaboration_teardown
                .validate_and_health()
                .expect("health")
                .active_count,
            0
        );
    }

    #[tokio::test]
    async fn production_worker_releases_remote_surfaces_after_durable_settlement() {
        let (api, server) = delete_server().await;
        let mut app = authenticated_runtime(api);
        app.backend_compatibility_verified = true;
        app.backend_reconciliation = BackendReconciliationState::CleanupPending;
        let session_id = SessionId::new().to_string();
        provision_bound(&app, &session_id);

        app.process_collaboration_teardown_worker().await;
        assert!(!app.remote_surfaces_ready());
        server.await.expect("server");
        for _ in 0..20 {
            if app
                .collaboration_teardown_task
                .as_ref()
                .is_some_and(tokio::task::JoinHandle::is_finished)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(
            app.collaboration_teardown_task
                .as_ref()
                .is_some_and(tokio::task::JoinHandle::is_finished),
            "cleanup worker should finish after the exact 204 response"
        );

        app.process_collaboration_teardown_worker().await;
        app.finish_initial_collaboration_cleanup().await;

        assert!(app.remote_surfaces_ready());
        assert_eq!(
            app.collaboration_teardown
                .validate_and_health()
                .expect("health")
                .active_count,
            0
        );
        assert!(
            app.state
                .pending_discovery_surfaces
                .contains(&crate::session_runtime::events::DiscoverySurface::Friends)
        );
        assert!(
            app.state
                .pending_discovery_surfaces
                .contains(&crate::session_runtime::events::DiscoverySurface::OwnSessions)
        );
    }

    #[tokio::test]
    async fn production_worker_keeps_local_commands_live_while_delete_hangs() {
        let (api, accepted) = hanging_delete_server().await;
        let mut app = authenticated_runtime(api);
        app.backend_compatibility_verified = true;
        app.backend_reconciliation = BackendReconciliationState::CleanupPending;
        let session_id = SessionId::new().to_string();
        provision_bound(&app, &session_id);

        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            app.process_collaboration_teardown_worker(),
        )
        .await
        .expect("actor-side cleanup pump must only spawn the worker");
        tokio::time::timeout(std::time::Duration::from_secs(1), accepted)
            .await
            .expect("worker should dispatch DELETE")
            .expect("server should observe DELETE");
        assert!(app.collaboration_teardown_task.is_some());
        assert!(!app.remote_surfaces_ready());

        let effects = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            crate::host_protocol::sessions_command::apply_session_command(
                &mut app,
                crate::host_protocol::SessionCommand::SnapshotRefresh,
            ),
        )
        .await
        .expect("a hanging cleanup backend must not block local commands")
        .expect("local snapshot command should succeed");
        assert!(effects.defer_catalog_replay);
        assert_eq!(
            app.collaboration_teardown
                .validate_and_health()
                .expect("health")
                .active_count,
            1,
            "the in-flight obligation remains durable until an exact response"
        );
    }

    #[tokio::test]
    async fn provisional_not_found_acknowledges_without_delete() {
        let (api, server) = response_sequence_server(vec![empty_response("404 Not Found")]).await;
        let mut app = authenticated_runtime(api);
        let session_id = SessionId::new().to_string();
        app.collaboration_teardown
            .provision(
                app.backend.backend_origin().expect("origin"),
                &app.state.identity.auth.subject_string().expect("subject"),
                &session_id,
                uuid::Uuid::now_v7(),
                uuid::Uuid::now_v7(),
                1,
            )
            .expect("provision");

        process_cleanup_until_idle(&mut app).await;

        assert_eq!(
            app.collaboration_teardown
                .validate_and_health()
                .expect("health")
                .active_count,
            0
        );
        let requests = server.await.expect("server");
        assert_eq!(requests.len(), 1);
        assert!(requests[0].starts_with("GET "));
    }

    #[tokio::test]
    async fn provisional_exact_receipt_binds_then_deletes_in_same_pass() {
        let session_id = SessionId::new().to_string();
        let create_id = uuid::Uuid::now_v7();
        let incarnation_id = uuid::Uuid::now_v7();
        let mutation_id = uuid::Uuid::now_v7();
        let receipt = format!(
            r#"{{"sessionId":"{session_id}","createIdempotencyKey":"{create_id}","incarnationId":"{incarnation_id}","generation":1,"protocolVersion":2}}"#
        );
        let (api, server) = response_sequence_server(vec![
            json_response(&receipt),
            empty_response("204 No Content"),
        ])
        .await;
        let mut app = authenticated_runtime(api);
        app.collaboration_teardown
            .provision(
                app.backend.backend_origin().expect("origin"),
                &app.state.identity.auth.subject_string().expect("subject"),
                &session_id,
                create_id,
                mutation_id,
                1,
            )
            .expect("provision");

        process_cleanup_until_idle(&mut app).await;

        assert_eq!(
            app.collaboration_teardown
                .validate_and_health()
                .expect("health")
                .active_count,
            0
        );
        let requests = server.await.expect("server");
        assert_eq!(requests.len(), 2);
        assert!(requests[0].starts_with("GET "));
        assert!(requests[1].starts_with("DELETE "));
        assert!(
            requests[1]
                .to_ascii_lowercase()
                .contains(&format!("\r\nidempotency-key: {mutation_id}\r\n"))
        );
    }

    #[tokio::test]
    async fn bound_inactive_obligation_dispatches_exact_headers_and_acks() {
        let (api, server) = delete_server().await;
        let mut app = authenticated_runtime(api);
        let session_id = SessionId::new().to_string();
        let (_, incarnation_id, mutation_id) = provision_bound(&app, &session_id);

        process_cleanup_until_idle(&mut app).await;

        assert_eq!(
            app.collaboration_teardown
                .validate_and_health()
                .expect("health")
                .active_count,
            0
        );
        let request = server.await.expect("server").to_ascii_lowercase();
        assert!(request.starts_with(&format!(
            "delete /api/sessions/{session_id}?incarnationid={incarnation_id} "
        )));
        assert!(request.contains(&format!("\r\nidempotency-key: {mutation_id}\r\n")));
        assert!(request.contains("\r\nkodosi-attempt-id: "));
    }

    #[tokio::test]
    async fn exact_active_share_suppresses_creation_time_obligation() {
        let mut app = authenticated_runtime("http://127.0.0.1:9/".to_owned());
        let local_id = SessionId::new();
        let session_id = local_id.to_string();
        let (_, incarnation_id, _) = provision_bound(&app, &session_id);
        app.state.sharing.shared_sessions.insert(
            local_id,
            SharedSessionState::new(
                session_id,
                incarnation_id,
                "secret".to_owned(),
                ShareScope::MyDevices,
                None,
                None,
                None,
            ),
        );

        process_cleanup_until_idle(&mut app).await;

        assert_eq!(
            app.collaboration_teardown
                .validate_and_health()
                .expect("health")
                .active_count,
            1
        );
    }

    #[test]
    fn teardown_dispatch_classifier_is_contract_specific() {
        assert!(matches!(classify(Ok(())), DispatchOutcome::Acknowledge));
        assert!(matches!(
            classify(Err(BackendClientError::NotFound)),
            DispatchOutcome::Acknowledge
        ));
        assert!(matches!(
            classify(Err(BackendClientError::Unauthorized)),
            DispatchOutcome::Pause
        ));
        assert!(matches!(
            classify(Err(BackendClientError::Timeout {
                operation: "session end"
            })),
            DispatchOutcome::Retry
        ));
        assert!(matches!(
            classify(Err(BackendClientError::HttpProblem {
                status: 503,
                code: None,
                detail: "draining".to_owned(),
            })),
            DispatchOutcome::Retry
        ));
        assert!(matches!(
            classify(Err(BackendClientError::HttpProblem {
                status: 409,
                code: Some("SESSION_END_MUTATION_TARGET_CONFLICT".to_owned()),
                detail: "mutation mismatch".to_owned(),
            })),
            DispatchOutcome::Quarantine(QuarantineReason::MutationConflict)
        ));
        assert!(matches!(
            classify(Err(BackendClientError::HttpProblem {
                status: 409,
                code: Some("CONCURRENT_MODIFICATION".to_owned()),
                detail: "retry".to_owned(),
            })),
            DispatchOutcome::Retry
        ));
        assert!(matches!(
            classify(Err(BackendClientError::HttpProblem {
                status: 409,
                code: None,
                detail: "unknown conflict".to_owned(),
            })),
            DispatchOutcome::Retry
        ));
        assert!(matches!(
            classify(Err(BackendClientError::HttpProblem {
                status: 400,
                code: Some("INVALID_PARAMETER".to_owned()),
                detail: "bad request".to_owned(),
            })),
            DispatchOutcome::Quarantine(QuarantineReason::NonRetryableClientError)
        ));
    }
}

#[cfg(test)]
mod room_mailbox_maintenance_tests {
    use super::{Runtime, mailbox_surfaces, schedule_room_mailbox_retry};
    use crate::{AppError, config::AppConfig, session_runtime::events::DiscoverySurface};
    use std::{
        collections::BTreeSet,
        io,
        time::{Duration, Instant},
    };
    use tokio_util::sync::CancellationToken;

    #[test]
    fn transient_mailbox_failure_requeues_with_bounded_backoff() {
        let requested = BTreeSet::from([DiscoverySurface::RoomChat, DiscoverySurface::RoomTasks]);
        let mut pending = BTreeSet::new();
        let mut retry = crate::runtime::room_mailbox::RoomMailboxRetryState::default();
        let now = Instant::now();
        let transient = AppError::Io(io::Error::new(io::ErrorKind::TimedOut, "temporary"));

        let first =
            schedule_room_mailbox_retry(&mut pending, &mut retry, &requested, &transient, now)
                .expect("transient errors should retry");
        assert_eq!(pending, requested);
        assert!(!retry.is_due(now));
        assert!(retry.is_due(now + first));

        let mut largest = first;
        for attempt in 1..16 {
            largest = schedule_room_mailbox_retry(
                &mut pending,
                &mut retry,
                &requested,
                &transient,
                now + Duration::from_secs(attempt),
            )
            .expect("transient errors should keep retrying");
        }
        assert_eq!(largest, Duration::from_secs(30));
    }

    #[test]
    fn mailbox_retries_auth_transport_and_key_availability_failures() {
        let requested = BTreeSet::from([DiscoverySurface::RoomTasks]);
        let errors = [
            AppError::Unauthorized,
            AppError::NotFound,
            AppError::Keychain {
                reason: "temporarily locked".to_owned(),
            },
            AppError::Unsupported {
                reason: "backend operation timed out: room tasks".to_owned(),
            },
        ];

        for error in errors {
            let mut pending = BTreeSet::new();
            let mut retry = crate::runtime::room_mailbox::RoomMailboxRetryState::default();
            assert!(
                schedule_room_mailbox_retry(
                    &mut pending,
                    &mut retry,
                    &requested,
                    &error,
                    Instant::now(),
                )
                .is_some(),
                "{error} should retry"
            );
            assert_eq!(pending, requested);
        }
    }

    #[tokio::test]
    async fn timer_maintenance_preserves_mailbox_until_compatibility_is_verified() {
        let mut config = AppConfig::default();
        config.auth.keyring_service = "kodosi.test".to_owned();
        let mut app = Runtime::with_dependencies(
            config.clone(),
            CancellationToken::new(),
            crate::runtime::RuntimeDependencies::isolated(&config)
                .expect("isolated runtime dependencies"),
        )
        .expect("test runtime should construct");
        app.state
            .queue_discovery_refresh([DiscoverySurface::RoomChat]);

        app.run_periodic_maintenance_force().await;

        assert!(
            mailbox_surfaces(&app.state.pending_discovery_surfaces)
                .contains(&DiscoverySurface::RoomChat),
            "mailbox work must remain pending while backend compatibility is unknown"
        );
    }
}
