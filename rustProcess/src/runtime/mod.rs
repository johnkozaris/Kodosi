pub(crate) mod access_effect_worker;
pub(crate) mod access_mutation_ledger;
pub(crate) mod access_mutation_worker;
pub(crate) mod access_mutations;
pub(crate) mod action_results;
pub(crate) mod auth;
mod collaboration_teardown_worker;
pub(crate) mod discovery;
pub(crate) mod identity;
pub(crate) mod identity_reset;
pub(crate) mod local_control;
pub(crate) mod local_sessions;
pub(crate) mod maintenance;
#[cfg(feature = "cli")]
pub(crate) mod one_shot;
mod pending_work;
mod permission_actions;
pub(crate) mod relay_prepare_worker;
mod remote_semantics;
pub(crate) mod remote_sessions;
mod room_mailbox;
pub(crate) mod room_mutation_ledger;
pub(crate) mod room_mutations;
pub(crate) mod rooms;
pub(crate) mod runtime_event_outbox;
pub(crate) mod runtime_loop;
mod semantic_mailbox;
pub(crate) mod session_catalog;
mod session_events;
pub(crate) mod share_transition_ledger;
mod share_transition_ledger_file;
mod share_transition_runtime;
pub(crate) mod share_transition_worker;
pub(crate) mod share_transitions;
pub(crate) mod sharing;
pub(crate) mod state;
pub(crate) mod steering;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    Result,
    config::AppConfig,
    host_protocol::RoomListEntry,
    identity_core::{
        device_flow::DeviceFlowClient, device_keys::DeviceKeyStore,
        device_list_pin_store::DeviceListPinStoreHandle, token_store::PlatformTokenStore,
    },
    session_runtime::events::RuntimeSessionEvent,
    support::ui::clipboard::SystemClipboardBridge,
    terminal_transport::hub::SessionHub,
};
use kodosi_backend_client::{
    config::BackendClientConfig, host_ws::HostWsClient, http_client::BackendHttpClient,
    session_relay::ws::SessionRelayWsClient, user_events_ws::UserEventsWsClient,
};

pub(crate) use self::state::{
    AppState, FailedSessionRuntimeDisposition, ProjectDiscoveryRequest, SessionEventEffects,
};

use self::identity::DeviceFlowRuntime;
use self::identity::device_link::DeviceLinkCtx;
use self::identity::device_list::DeviceListCtx;
use crate::discovery::RemoteSessionRecord;
use crate::remote_sessions::{focus::ClientFocusTracker, terminal_cache::RemoteTerminalCache};

pub(crate) const DEFAULT_TOKEN_SUBJECT: &str = "default";
pub(crate) const SESSION_EVENT_CAPACITY: usize = 64;
pub(crate) const MAX_SESSION_TITLE_LEN: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackendReconciliationState {
    Idle,
    IdentityPending,
    CleanupPending,
    CleanupQuarantined,
}

#[derive(Debug)]
pub(crate) struct Runtime {
    pub(crate) state: AppState,
    pub(crate) terminal_hub: SessionHub,
    pub(crate) remote_terminal: RemoteTerminalCache,

    pub(crate) client_focus: ClientFocusTracker,
    pub(crate) backend: BackendHttpClient,
    pub(crate) collaboration_teardown:
        crate::sharing::collaboration_teardown_obligations::CollaborationTeardownObligationStore,
    _roots: RuntimeRoots,
    backend_compatibility_verified: bool,
    backend_reconciliation: BackendReconciliationState,
    pub(crate) token_store: PlatformTokenStore,
    pub(crate) device_key_store: DeviceKeyStore,
    pub(crate) pin_store: DeviceListPinStoreHandle,
    pub(crate) hidden_session_store: crate::discovery::hidden_sessions::HiddenSessionStore,
    pub(crate) identity_reset_intent: identity_reset::IdentityResetIntentStore,
    pub(crate) room_roster_pins_path: std::path::PathBuf,
    pub(crate) host_ws: HostWsClient,
    pub(crate) session_relay_ws: SessionRelayWsClient,
    pub(crate) user_events_ws: UserEventsWsClient,
    session_events_rx: mpsc::Receiver<RuntimeSessionEvent>,
    pub(crate) session_events_tx: mpsc::Sender<RuntimeSessionEvent>,
    pub(crate) shutdown: CancellationToken,
    pub(crate) clipboard: SystemClipboardBridge,
    pub(crate) last_maintenance_ran: Option<std::time::Instant>,

    pub(crate) last_auth_refresh_check: Option<std::time::Instant>,
    collaboration_teardown_task: Option<
        tokio::task::JoinHandle<collaboration_teardown_worker::CollaborationTeardownTaskCompletion>,
    >,
    collaboration_teardown_retry_after: Option<std::time::Instant>,
    collaboration_teardown_retry_until: std::collections::BTreeMap<uuid::Uuid, std::time::Instant>,
    collaboration_teardown_deferred_until:
        std::collections::BTreeMap<uuid::Uuid, std::time::Instant>,
    device_enrollment_retry_after: Option<std::time::Instant>,
    device_enrollment_satisfied: bool,
    room_mailbox_retry: room_mailbox::RoomMailboxRetryState,
    semantic_receipt_cursor: Option<String>,
    semantic_mailbox_task:
        Option<tokio::task::JoinHandle<semantic_mailbox::SemanticMailboxCompletion>>,
    share_transition_tasks: std::collections::HashMap<uuid::Uuid, tokio::task::JoinHandle<()>>,
    share_transition_in_flight:
        std::collections::HashMap<kodosi_domain::ids::SessionId, uuid::Uuid>,
    share_transition_deadlines: std::collections::HashMap<uuid::Uuid, std::time::Instant>,
    share_transition_settlement_retry:
        std::collections::HashMap<uuid::Uuid, share_transitions::DeferredShareSettlement>,
    share_transition_completion_tx:
        mpsc::Sender<share_transition_worker::ShareTransitionCompletion>,
    pub(crate) share_transition_completion_rx:
        mpsc::Receiver<share_transition_worker::ShareTransitionCompletion>,
    relay_prepare_tasks: std::collections::HashMap<
        kodosi_domain::ids::SessionId,
        tokio::task::JoinHandle<relay_prepare_worker::RelayPrepareCompletion>,
    >,
    relay_prepare_owners: std::collections::HashMap<
        kodosi_domain::ids::SessionId,
        relay_prepare_worker::RelayPrepareOwnerKind,
    >,
    relay_prepare_notify: std::sync::Arc<tokio::sync::Notify>,
    access_effect_task:
        Option<tokio::task::JoinHandle<access_effect_worker::AccessEffectCompletion>>,
    access_effect_in_flight: Option<(String, uuid::Uuid, kodosi_domain::ids::SessionId)>,
    access_effect_notify: std::sync::Arc<tokio::sync::Notify>,
    access_mutation_task: Option<
        tokio::task::JoinHandle<access_mutation_worker::SessionAccessMutationWorkerCompletion>,
    >,
    access_mutation_notify: std::sync::Arc<tokio::sync::Notify>,
    access_mutation_in_flight: Option<(String, uuid::Uuid)>,
    access_mutation_dispatch_queue: std::collections::VecDeque<(String, uuid::Uuid)>,
    access_mutation_retry_until:
        std::collections::HashMap<(String, uuid::Uuid), std::time::Instant>,
    semantic_receipts_in_flight:
        std::collections::HashSet<(kodosi_domain::ids::SessionId, uuid::Uuid, u64)>,
    action_results_in_flight:
        std::collections::HashSet<(kodosi_domain::ids::SessionId, String, String, u64)>,
    owner_action_results: action_results::OwnerActionResultStore,
    pending_remote_resizes:
        std::collections::HashMap<(kodosi_domain::ids::SessionId, String), PendingRemoteResize>,
    remote_permission_actions: permission_actions::RemotePermissionActionStore,
    remote_semantics: remote_semantics::RemoteSemanticStore,
    pub(crate) local_catalog: local_control::LocalSessionCatalog,
    pub(crate) share_transitions: share_transition_ledger::ShareTransitionLedger,
    pub(crate) room_mutations: room_mutation_ledger::RoomMutationLedger,
    pub(crate) access_mutations: access_mutation_ledger::SessionAccessMutationLedger,
}

impl Drop for Runtime {
    fn drop(&mut self) {
        if let Some(task) = self.collaboration_teardown_task.take() {
            task.abort();
        }
        if let Some(task) = self.semantic_mailbox_task.take() {
            task.abort();
        }
        for (_, task) in self.share_transition_tasks.drain() {
            task.abort();
        }
        self.share_transition_completion_rx.close();
        self.share_transition_deadlines.clear();
        self.share_transition_settlement_retry.clear();
        for (_, task) in self.relay_prepare_tasks.drain() {
            task.abort();
        }
        self.relay_prepare_owners.clear();
        if let Some(task) = self.access_effect_task.take() {
            task.abort();
        }
        self.access_effect_in_flight = None;
        if let Some(task) = self.access_mutation_task.take() {
            task.abort();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionRelayCommandDispatchOutcome {
    Enqueued,
    Rejected,
    Busy,
}

#[derive(Debug)]
pub(crate) struct RuntimeRoots {
    isolated: Option<std::sync::Arc<tempfile::TempDir>>,
}

impl RuntimeRoots {
    fn production() -> Self {
        Self { isolated: None }
    }

    #[cfg(test)]
    pub(crate) fn isolated() -> Result<Self> {
        Ok(Self {
            isolated: Some(std::sync::Arc::new(
                tempfile::tempdir().map_err(crate::AppError::Io)?,
            )),
        })
    }

    fn isolated_path(&self) -> Option<&std::path::Path> {
        self.isolated.as_ref().map(|directory| directory.path())
    }
}

#[derive(Debug)]
pub(crate) struct RuntimeDependencies {
    roots: RuntimeRoots,
    backend: BackendHttpClient,
    pin_store: Option<DeviceListPinStoreHandle>,
    clipboard: SystemClipboardBridge,
}

impl RuntimeDependencies {
    fn production(config: &AppConfig) -> Result<Self> {
        let backend_config = BackendClientConfig::new(
            config.backend.api.clone(),
            config.backend.host_relay.clone(),
            config.backend.viewer_relay.clone(),
            config.backend.user_events.clone(),
        );
        Ok(Self {
            roots: RuntimeRoots::production(),
            backend: BackendHttpClient::new(&backend_config)?,
            pin_store: None,
            clipboard: SystemClipboardBridge::new(),
        })
    }

    #[cfg(test)]
    pub(crate) fn isolated(config: &AppConfig) -> Result<Self> {
        let backend_config = BackendClientConfig::new(
            config.backend.api.clone(),
            config.backend.host_relay.clone(),
            config.backend.viewer_relay.clone(),
            config.backend.user_events.clone(),
        );
        Ok(Self {
            roots: RuntimeRoots::isolated()?,
            backend: BackendHttpClient::new(&backend_config)?,
            pin_store: None,
            clipboard: SystemClipboardBridge::new(),
        })
    }

    #[cfg(test)]
    pub(crate) fn with_pin_store(mut self, pin_store: DeviceListPinStoreHandle) -> Self {
        self.pin_store = Some(pin_store);
        self
    }

    #[cfg(test)]
    pub(crate) fn with_clipboard(mut self, clipboard: SystemClipboardBridge) -> Self {
        self.clipboard = clipboard;
        self
    }
}

#[derive(Debug)]
pub(crate) struct PendingRemoteResize {
    pub(crate) session_id: kodosi_domain::ids::SessionId,
    pub(crate) relay_generation: u64,
    pub(crate) identity: crate::host_protocol::TerminalResizeIdentity,
}

#[derive(Debug)]
pub(crate) struct DiscoveryRefreshOutcome {
    pub(crate) remote_sessions: Vec<RemoteSessionRecord>,
    pub(crate) preservation: crate::discovery::state::DiscoveryPreservation,
    pub(crate) available_rooms: Option<Vec<RoomListEntry>>,
}

impl Runtime {
    pub(crate) fn session_incarnation_id(
        &self,
        session_id: kodosi_domain::ids::SessionId,
    ) -> Option<uuid::Uuid> {
        self.state
            .local
            .sessions
            .record(session_id)
            .map(|record| record.local_incarnation_id)
            .or_else(|| {
                self.state
                    .discovery
                    .session(session_id)
                    .and_then(|record| record.incarnation_id)
            })
    }

    pub(crate) fn friends_ctx(&mut self) -> crate::discovery::friends::FriendsCtx<'_> {
        crate::discovery::friends::FriendsCtx {
            backend: &self.backend,
            outbox: &mut self.state.runtime_outbox,
        }
    }

    pub(crate) fn device_list_ctx(&mut self) -> DeviceListCtx<'_> {
        DeviceListCtx {
            auth: &self.state.identity.auth,
            backend: &self.backend,
            device_key_store: &self.device_key_store,
            pin_store: &self.pin_store,
            outbox: &mut self.state.runtime_outbox,
        }
    }

    pub(crate) fn device_link_ctx(&mut self) -> DeviceLinkCtx<'_> {
        let account_origin = self.state.identity.current_account_event_origin();
        DeviceLinkCtx {
            auth: &self.state.identity.auth,
            account_origin,
            backend: &self.backend,
            device_key_store: &self.device_key_store,
            pin_store: &self.pin_store,
            account_runtimes: &mut self.state.identity.account_runtimes,
            device_flow: &mut self.state.identity.device_flow,
            pending_discovery_surfaces: &mut self.state.pending_discovery_surfaces,
            session_events_tx: &self.session_events_tx,
            logs: &mut self.state.logs,
        }
    }

    pub(crate) fn new(config: AppConfig, shutdown: CancellationToken) -> Result<Self> {
        let dependencies = RuntimeDependencies::production(&config)?;
        Self::with_dependencies(config, shutdown, dependencies)
    }

    #[expect(
        clippy::needless_pass_by_value,
        clippy::too_many_lines,
        reason = "construction consumes and validates every runtime authority before publishing a usable Runtime"
    )]
    pub(crate) fn with_dependencies(
        config: AppConfig,
        shutdown: CancellationToken,
        dependencies: RuntimeDependencies,
    ) -> Result<Self> {
        crate::support::storage::paths::validate_storage_contract()?;
        let isolated_root = dependencies.roots.isolated_path();
        let backend_config = BackendClientConfig::new(
            config.backend.api.clone(),
            config.backend.host_relay.clone(),
            config.backend.viewer_relay.clone(),
            config.backend.user_events.clone(),
        );
        let backend = dependencies.backend;
        let collaboration_teardown = isolated_root.map_or_else(
            crate::sharing::collaboration_teardown_obligations::CollaborationTeardownObligationStore::load_default,
            |root| {
                crate::sharing::collaboration_teardown_obligations::CollaborationTeardownObligationStore::at(
                    root.join("collaboration-teardown-obligations.json"),
                    1_024,
                )
            },
        )?;
        let collaboration_health = collaboration_teardown.validate_and_health();
        let initial_backend_reconciliation = match &collaboration_health {
            Ok(health)
                if health.durable_state
                    != crate::sharing::collaboration_teardown_obligations::DurableStoreState::Healthy =>
            {
                BackendReconciliationState::CleanupQuarantined
            }
            Err(_) => BackendReconciliationState::CleanupQuarantined,
            Ok(_) => BackendReconciliationState::Idle,
        };
        let host_ws = HostWsClient::new(backend_config.host_relay.as_deref())?;
        let session_relay_ws = SessionRelayWsClient::new(backend_config.viewer_relay.as_deref())?;
        let user_events_ws = UserEventsWsClient::new(backend_config.user_events.as_deref())?;
        let auth_client = DeviceFlowClient::new(&config.auth)?;
        let token_store =
            isolated_root.map_or_else(PlatformTokenStore::default_location, |root| {
                Ok(PlatformTokenStore::at(
                    root.join("tokens"),
                    &config.auth.keyring_service,
                ))
            })?;
        let device_key_store = isolated_root.map_or_else(
            || DeviceKeyStore::new(&config.auth.keyring_service),
            |root| {
                Ok(DeviceKeyStore::at(
                    root.join("device-keys"),
                    &config.auth.keyring_service,
                ))
            },
        )?;
        let pin_store = if let Some(pin_store) = dependencies.pin_store {
            pin_store
        } else if let Some(root) = isolated_root {
            DeviceListPinStoreHandle::from_store(
                crate::identity_core::device_list_pin_store::DeviceListPinStore::load_from(
                    root.join("device-list-pins.json"),
                )?,
            )?
        } else {
            DeviceListPinStoreHandle::load_default()?
        };
        let hidden_session_store = isolated_root.map_or_else(
            crate::discovery::hidden_sessions::HiddenSessionStore::load_default,
            |root| {
                Ok(crate::discovery::hidden_sessions::HiddenSessionStore::at(
                    root.join("hidden-sessions.json"),
                ))
            },
        )?;
        let identity_reset_intent = isolated_root.map_or_else(
            identity_reset::IdentityResetIntentStore::load_default,
            |root| {
                Ok(identity_reset::IdentityResetIntentStore::at(
                    root.join("identity-reset-intent.json"),
                ))
            },
        )?;
        let room_roster_pins_path = isolated_root.map_or_else(
            crate::support::storage::paths::room_roster_pins_path,
            |root| Ok(root.join("room-roster-pins.json")),
        )?;
        let (session_events_tx, session_events_rx) = mpsc::channel(SESSION_EVENT_CAPACITY);
        let device_flow = DeviceFlowRuntime::new(auth_client);
        let steering = isolated_root
            .map_or_else(steering::SteeringState::load_default, |root| {
                steering::SteeringState::load_at(root.join("pending-steers.json"))
            })?;
        let local_catalog = isolated_root
            .map_or_else(local_control::LocalSessionCatalog::load_default, |root| {
                local_control::LocalSessionCatalog::at(root.join("session-catalog"))
            })?;
        let remote_permission_actions = isolated_root.map_or_else(
            permission_actions::RemotePermissionActionStore::load_default,
            |root| {
                permission_actions::RemotePermissionActionStore::load_at(
                    root.join("pending-remote-permission-actions.json"),
                )
            },
        )?;
        let remote_semantics = isolated_root.map_or_else(
            remote_semantics::RemoteSemanticStore::load_default,
            |root| {
                remote_semantics::RemoteSemanticStore::load_at(
                    &root.join("pending-remote-semantics.json"),
                )
            },
        )?;
        let owner_action_results = isolated_root.map_or_else(
            action_results::OwnerActionResultStore::load_default,
            |root| {
                action_results::OwnerActionResultStore::load_at(
                    root.join("pending-owner-action-results.json"),
                )
            },
        )?;

        let session_cache_root = isolated_root
            .map_or_else(local_control::migration::default_cache_root, |root| {
                Ok(Some(root.join("session-cache")))
            })?;

        let mut app_state = AppState::new(
            config.app_state_config(),
            device_flow,
            steering,
            session_cache_root,
        );
        match collaboration_health {
            Ok(health)
                if health.quarantined_count > 0
                    || health.durable_state
                        != crate::sharing::collaboration_teardown_obligations::DurableStoreState::Healthy =>
            {
                app_state.record_log(match health.durable_state {
                    crate::sharing::collaboration_teardown_obligations::DurableStoreState::Healthy => format!(
                        "{} collaboration cleanup quarantine(s) require operator attention",
                        health.quarantined_count
                    ),
                    crate::sharing::collaboration_teardown_obligations::DurableStoreState::CorruptionQuarantined =>
                        "collaboration cleanup obligation store is corruption-quarantined; remote operations are disabled until explicit reset"
                            .to_owned(),
                    crate::sharing::collaboration_teardown_obligations::DurableStoreState::Unavailable =>
                        "collaboration cleanup obligation store is unavailable; remote operations are disabled"
                            .to_owned(),
                });
            }
            Err(error) => {
                app_state.record_log(format!(
                    "collaboration cleanup store is unavailable; remote operations are disabled: {error}"
                ));
            }
            Ok(_) => {}
        }

        let share_transitions = isolated_root.map_or_else(
            share_transition_ledger::ShareTransitionLedger::load_default,
            |root| {
                share_transition_ledger::ShareTransitionLedger::load_at(
                    root.join("share-transitions.json"),
                )
            },
        )?;
        if let Err(error) = share_transitions.ensure_available() {
            app_state.record_log(error.to_string());
        }
        let room_mutations = isolated_root.map_or_else(
            room_mutation_ledger::RoomMutationLedger::load_default,
            |root| {
                room_mutation_ledger::RoomMutationLedger::load_at(root.join("room-mutations.json"))
            },
        )?;
        if let Err(error) = room_mutations.ensure_available() {
            app_state.record_log(error.to_string());
        }
        let access_mutations = isolated_root.map_or_else(
            access_mutation_ledger::SessionAccessMutationLedger::load_default,
            |root| {
                access_mutation_ledger::SessionAccessMutationLedger::load_at(
                    root.join("pending-session-access-mutations.json"),
                )
            },
        )?;
        if let Err(error) = access_mutations.ensure_available() {
            app_state.record_log(error.to_string());
        }

        let (share_transition_completion_tx, share_transition_completion_rx) = mpsc::channel(32);
        let app = Self {
            state: app_state,
            terminal_hub: SessionHub::new(),
            remote_terminal: RemoteTerminalCache::new(),
            client_focus: ClientFocusTracker::new(),
            backend,
            collaboration_teardown,
            _roots: dependencies.roots,
            backend_compatibility_verified: false,
            backend_reconciliation: initial_backend_reconciliation,
            token_store,
            device_key_store,
            pin_store,
            hidden_session_store,
            identity_reset_intent,
            room_roster_pins_path,
            host_ws,
            session_relay_ws,
            user_events_ws,
            session_events_rx,
            session_events_tx,
            shutdown,
            clipboard: dependencies.clipboard,
            last_maintenance_ran: None,
            last_auth_refresh_check: None,
            collaboration_teardown_task: None,
            collaboration_teardown_retry_after: None,
            collaboration_teardown_retry_until: std::collections::BTreeMap::new(),
            collaboration_teardown_deferred_until: std::collections::BTreeMap::new(),
            device_enrollment_retry_after: None,
            device_enrollment_satisfied: false,
            room_mailbox_retry: room_mailbox::RoomMailboxRetryState::default(),
            semantic_receipt_cursor: None,
            semantic_mailbox_task: None,
            share_transition_tasks: std::collections::HashMap::new(),
            share_transition_in_flight: std::collections::HashMap::new(),
            share_transition_deadlines: std::collections::HashMap::new(),
            share_transition_settlement_retry: std::collections::HashMap::new(),
            share_transition_completion_tx,
            share_transition_completion_rx,
            relay_prepare_tasks: std::collections::HashMap::new(),
            relay_prepare_owners: std::collections::HashMap::new(),
            relay_prepare_notify: std::sync::Arc::new(tokio::sync::Notify::new()),
            access_effect_task: None,
            access_effect_in_flight: None,
            access_effect_notify: std::sync::Arc::new(tokio::sync::Notify::new()),
            access_mutation_task: None,
            access_mutation_notify: std::sync::Arc::new(tokio::sync::Notify::new()),
            access_mutation_in_flight: None,
            access_mutation_dispatch_queue: std::collections::VecDeque::new(),
            access_mutation_retry_until: std::collections::HashMap::new(),
            semantic_receipts_in_flight: std::collections::HashSet::new(),
            action_results_in_flight: std::collections::HashSet::new(),
            owner_action_results,
            pending_remote_resizes: std::collections::HashMap::new(),
            remote_permission_actions,
            remote_semantics,
            local_catalog,
            share_transitions,
            room_mutations,
            access_mutations,
        };

        Ok(app)
    }

    pub(crate) async fn initialize(&mut self) -> Result<()> {
        if !self.state.identity.auth.is_authenticated() {
            discovery::clear_remote_catalog(self, discovery::CatalogClearReason::AccountTeardown);
        }
        let reset_resolution = identity_reset::resolve_pending(self).await?;
        let reset_pending = matches!(
            reset_resolution,
            identity_reset::IdentityResetResolution::Pending(_)
        );
        if self.drain_pending_session_events() {
            self.state.snapshot_refresh_pending = true;
        }
        self.restore_cached_sessions()?;
        if !reset_pending {
            auth::restore_auth(self).await?;
            self.recover_share_transitions();
        }
        let account_user_id = self
            .state
            .identity
            .auth
            .subject_string()
            .unwrap_or_else(|| "local".to_owned());
        for mut entry in self
            .state
            .steering
            .all_entries()
            .into_iter()
            .filter(|entry| entry.account_user_id == account_user_id)
        {
            if entry.delivery_state == crate::SteerDeliveryState::Preparing
                && let Ok(session_id) =
                    kodosi_domain::ids::SessionId::parse_field(&entry.session_id, "sessionId")
                && self
                    .state
                    .local
                    .sessions
                    .record(session_id)
                    .is_some_and(|record| {
                        record.local_incarnation_id.to_string() == entry.session_incarnation_id
                    })
                && let Err(error) =
                    self.state
                        .steering
                        .acknowledge_queued(&entry.steer_id)
                        .map(|queued| {
                            if let Some(queued) = queued {
                                entry = queued;
                            }
                        })
            {
                self.state.record_log(format!(
                    "semantic send {} could not resume durable queue intent: {error}",
                    entry.steer_id
                ));
            }
            let transition = match entry.delivery_state {
                crate::SteerDeliveryState::Preparing => crate::SteerTransition::Sending,
                crate::SteerDeliveryState::Queued => crate::SteerTransition::Queued,
                crate::SteerDeliveryState::DeliveryUnknown => {
                    crate::SteerTransition::DeliveryUnknown
                }
                crate::SteerDeliveryState::Injected => crate::SteerTransition::Injected,
                crate::SteerDeliveryState::Cancelled => crate::SteerTransition::Cancelled,
            };
            self.state
                .runtime_outbox
                .queue_agent_intel(crate::AgentIntelEvent::SteerState {
                    entry,
                    transition,
                    message: None,
                });
        }
        auth::stop_user_events_stream(self);
        Ok(())
    }

    pub(crate) fn collaboration_obligation_is_not_pending_for_current_account(
        &self,
        record: &crate::sharing::collaboration_teardown_obligations::TeardownObligation,
    ) -> bool {
        let belongs_to_current_account = self.backend.backend_origin()
            == Some(&record.backend_origin)
            && self.state.identity.auth.subject_string().as_deref()
                == Some(record.account_subject.as_str());
        if !belongs_to_current_account {
            return true;
        }
        record.backend_incarnation_id.is_some_and(|incarnation_id| {
            self.state
                .sharing
                .shared_sessions
                .contains_exact_backend_incarnation(&record.backend_session_id, incarnation_id)
        }) || self.share_transition_holds_cleanup(record.create_idempotency_id)
    }

    pub(crate) fn collaboration_cleanup_health(&self) -> crate::CollaborationCleanupHealth {
        use crate::sharing::collaboration_teardown_obligations::DurableStoreState;

        self.collaboration_teardown
            .cleanup_health(|record| {
                self.collaboration_obligation_is_not_pending_for_current_account(record)
            })
            .map_or_else(
                |_| crate::CollaborationCleanupHealth {
                    state: crate::CollaborationCleanupState::Unavailable,
                    pending_count: 0,
                    quarantined_count: 0,
                    message: Some(
                        "Collaboration cleanup status is unavailable; remote operations are disabled."
                            .to_owned(),
                    ),
                },
                |health| {
                    let (state, message) = match health.durable_state {
                        DurableStoreState::Healthy if health.quarantined_count == 0 => {
                            (crate::CollaborationCleanupState::Healthy, None)
                        }
                        DurableStoreState::Healthy => (
                            crate::CollaborationCleanupState::Quarantined,
                            Some("Collaboration cleanup needs operator attention.".to_owned()),
                        ),
                        DurableStoreState::CorruptionQuarantined => (
                            crate::CollaborationCleanupState::Quarantined,
                            Some(
                                "Collaboration cleanup obligation store is quarantined; remote operations are disabled until explicit reset."
                                    .to_owned(),
                            ),
                        ),
                        DurableStoreState::Unavailable => (
                            crate::CollaborationCleanupState::Unavailable,
                            Some(
                                "Collaboration cleanup status is unavailable; remote operations are disabled."
                                    .to_owned(),
                            ),
                        ),
                    };
                    crate::CollaborationCleanupHealth {
                        state,
                        pending_count: health.pending_count,
                        quarantined_count: health.quarantined_count,
                        message,
                    }
                },
            )
    }

    #[cfg(test)]
    pub(crate) fn set_remote_surfaces_ready_for_test(&mut self) {
        self.backend_compatibility_verified = true;
        self.backend_reconciliation = BackendReconciliationState::Idle;
        self.last_auth_refresh_check = Some(std::time::Instant::now());
    }

    pub(crate) fn remote_surfaces_ready(&self) -> bool {
        self.state.identity.auth.is_authenticated()
            && self.backend_compatibility_verified
            && self.backend_reconciliation == BackendReconciliationState::Idle
            && self.share_transitions.ensure_available().is_ok()
            && self.access_mutations.ensure_available().is_ok()
            && self.collaboration_teardown.durable_state()
                == crate::sharing::collaboration_teardown_obligations::DurableStoreState::Healthy
    }

    #[cfg(feature = "cli")]
    pub(crate) fn reset_share_transition_ledger(&mut self) -> Result<Vec<std::path::PathBuf>> {
        self.share_transitions
            .reset_unavailable_preserving_evidence()
    }

    #[cfg(any(test, feature = "cli"))]
    pub(crate) fn reset_collaboration_cleanup_quarantine(&mut self) -> Result<usize> {
        let reset_count = self.collaboration_teardown.reset_corruption_quarantine()?;
        self.backend_reconciliation = if self.state.identity.auth.is_authenticated() {
            BackendReconciliationState::CleanupPending
        } else {
            BackendReconciliationState::Idle
        };
        self.last_maintenance_ran = None;
        self.state.record_log(format!(
            "operator reset collaboration cleanup corruption quarantine; preserved {reset_count} evidence file(s)"
        ));
        Ok(reset_count)
    }

    pub(crate) const fn has_verified_backend_compatibility(&self) -> bool {
        self.backend_compatibility_verified
    }
}

#[cfg(test)]
mod tests;
