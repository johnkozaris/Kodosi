use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;

use crate::{
    config::AppStateConfig,
    host_protocol::RoomListEntry,
    session_runtime::{
        events::DiscoverySurface,
        handles::OwnedSessionHandle,
        project::{CachedProjectDiscovery, ProjectDiscovery},
        registry::TaskHealthReport,
    },
};
use kodosi_backend_client::session_relay::{RemoteRelayMode, SessionRelayClientHandle};
use kodosi_domain::{
    ids::SessionId,
    lifecycle::ConnectionState,
    permissions::ShareScope,
    session::{SessionState, SessionSummary},
};

use super::identity::{DeviceFlowRuntime, IdentityState};
use super::{pending_work::PendingWorkQueue, runtime_event_outbox::RuntimeEventOutbox};
use crate::{
    agent_intel::AgentIntelState,
    discovery::{DiscoveryState, SessionShelf},
    local_sessions::state::LocalSessionsState,
    remote_sessions::state::RemoteSessionsState,
    sharing::state::SharingState,
};

mod account_events;
mod event_reducer;
mod owned_events;
mod remote_events;

pub(crate) const MAX_LOG_LINES: usize = 10;

pub(crate) fn push_log(logs: &mut VecDeque<String>, message: String) {
    if logs.len() == MAX_LOG_LINES {
        logs.pop_front();
    }
    logs.push_back(message);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CatalogRefreshState {
    Idle,
    CheckPending,
    ReplayPending,
}

#[derive(Debug)]
pub(crate) struct AppState {
    pub(crate) config: AppStateConfig,
    pub(crate) identity: IdentityState,
    pub(crate) local: LocalSessionsState,
    pub(crate) remote: RemoteSessionsState,
    pub(crate) sharing: SharingState,
    pub(crate) agent_intel: AgentIntelState,
    pub(crate) session_cache_root: Option<std::path::PathBuf>,
    pub(crate) shelf: SessionShelf,
    pub(crate) discovery: DiscoveryState,
    pub(crate) available_rooms: Vec<RoomListEntry>,
    pub(crate) room_catalog_loaded: bool,
    pub(crate) pending_work: PendingWorkQueue,
    pub(crate) steering: crate::runtime::steering::SteeringState,
    pub(crate) runtime_outbox: RuntimeEventOutbox,
    pub(crate) project_discovery: HashMap<String, CachedProjectDiscovery>,
    project_discovery_in_flight: HashSet<String>,
    pub(crate) logs: VecDeque<String>,
    pub(crate) snapshot_refresh_pending: bool,
    pub(crate) catalog_refresh: CatalogRefreshState,
    pub(crate) backend_status: ConnectionState,
    pub(crate) host_ws_status: ConnectionState,
    pub(crate) viewer_ws_status: ConnectionState,
    pub(crate) pending_discovery_surfaces: BTreeSet<DiscoverySurface>,
    pub(crate) pending_room_projection_refreshes: BTreeMap<String, BTreeSet<DiscoverySurface>>,
    pub(crate) pending_post_login_work: bool,
}

#[derive(Debug)]
pub(crate) struct ProjectDiscoveryRequest {
    pub(crate) working_dir: String,
}

#[derive(Debug, Default)]
pub(crate) struct SessionEventEffects {
    pub(crate) force_snapshot: bool,
    pub(crate) project_discovery: Option<ProjectDiscoveryRequest>,
    pub(crate) agent_intel_spawn: Option<(SessionId, String)>,
    pub(crate) agent_intel_teardown: Option<SessionId>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum FailedSessionRuntimeDisposition {
    AwaitStoppedEvent,
    DropNow,
}

impl AppState {
    pub(crate) const MAX_PROJECT_DISCOVERY_ENTRIES: usize = 256;

    pub(crate) fn new(
        config: AppStateConfig,
        device_flow: DeviceFlowRuntime,
        steering: crate::runtime::steering::SteeringState,
        session_cache_root: Option<std::path::PathBuf>,
    ) -> Self {
        let agent_intel =
            AgentIntelState::with_permission_timeout(config.permissions.decision_timeout());
        Self {
            config,
            identity: IdentityState::new(device_flow),
            local: LocalSessionsState::default(),
            remote: RemoteSessionsState::default(),
            sharing: SharingState::default(),
            agent_intel,
            session_cache_root,
            shelf: SessionShelf::default(),
            discovery: DiscoveryState::empty(),
            available_rooms: Vec::new(),
            room_catalog_loaded: false,
            pending_work: PendingWorkQueue::new(),
            steering,
            runtime_outbox: RuntimeEventOutbox::default(),
            project_discovery: HashMap::new(),
            project_discovery_in_flight: HashSet::new(),
            logs: VecDeque::new(),
            snapshot_refresh_pending: false,
            catalog_refresh: CatalogRefreshState::Idle,
            backend_status: ConnectionState::Offline,
            host_ws_status: ConnectionState::Offline,
            viewer_ws_status: ConnectionState::Offline,
            pending_discovery_surfaces: BTreeSet::new(),
            pending_room_projection_refreshes: BTreeMap::new(),
            pending_post_login_work: false,
        }
    }

    pub(crate) fn apply_session_scope_locally(&mut self, id: SessionId, scope: ShareScope) {
        if let Some(record) = self.local.sessions.record_mut(id) {
            record.summary.scope = scope;
            record.summary.last_update = OffsetDateTime::now_utc();
        }
    }

    pub(crate) fn mark_session_stopped(&mut self, id: SessionId) {
        self.local.sessions.mark_stopped(id);
        self.local.owned_session_runtimes.remove(id);
        self.sharing.shared_sessions.clear(id);
    }

    pub(crate) fn register_owned_session(
        &mut self,
        summary: SessionSummary,
        handle: OwnedSessionHandle,
        create_request_id: Option<String>,
        local_incarnation_id: uuid::Uuid,
    ) {
        let id = summary.id;
        self.local.sessions.insert(summary);
        if let Some(record) = self.local.sessions.record_mut(id) {
            record.create_request_id = create_request_id;
            record.local_incarnation_id = local_incarnation_id;
        }
        self.local
            .owned_session_runtimes
            .attach(id, local_incarnation_id, handle);
        self.local.sessions.update_state(id, SessionState::Running);
        self.shelf.sync_owned_sessions(self.local.sessions.ids());
        self.shelf.activate(crate::discovery::ShelfItem::Owned(id));
    }

    pub(crate) fn replace_reopened_owned_session(
        &mut self,
        summary: SessionSummary,
        handle: OwnedSessionHandle,
        local_incarnation_id: uuid::Uuid,
    ) -> bool {
        let id = summary.id;
        self.local
            .owned_session_runtimes
            .attach(id, local_incarnation_id, handle);
        self.sharing.host_relays.teardown(id);
        self.agent_intel.registry.prepare_for_restart(id);
        let cleared_stale_delete = self.pending_work.clear_delete(id);

        if let Some(record) = self.local.sessions.record_mut(id) {
            record.summary = summary;
            record.summary.last_update = OffsetDateTime::now_utc();
            record.local_incarnation_id = local_incarnation_id;
        }
        self.local.sessions.update_state(id, SessionState::Running);
        self.shelf.sync_owned_sessions(self.local.sessions.ids());
        self.shelf.activate(crate::discovery::ShelfItem::Owned(id));
        cleared_stale_delete
    }

    pub(crate) fn queue_delete_after_stop(&mut self, id: SessionId) {
        self.pending_work.queue_delete_after_stop(id);
    }

    pub(crate) fn activate_owned_session(&mut self, id: SessionId) {
        self.shelf.activate(crate::discovery::ShelfItem::Owned(id));
    }

    pub(crate) fn clear_shared_session(&mut self, id: SessionId) -> bool {
        let cleared = self.sharing.shared_sessions.clear(id);
        if cleared {
            self.sync_host_relay_status();
        }
        cleared
    }

    pub(crate) fn publish_session_if_host_relay_active(&mut self, id: SessionId) {
        if self.sharing.host_relays.active(id) {
            self.local
                .sessions
                .update_state(id, SessionState::Published);
        }
    }

    pub(crate) fn unshare_session_locally(&mut self, id: SessionId) {
        self.sharing.host_relays.cancel(id);
        self.pending_work.clear_backend_scope_restore(id);
        self.pending_work.clear_host_key_rotation(id);
        self.pending_work.clear_key_redistribution(id);

        self.local
            .owned_session_runtimes
            .release_remote_size_authority(id);
        self.sharing.shared_sessions.clear(id);
        self.apply_session_scope_locally(id, ShareScope::JustMe);
        self.restore_unrelayed_session_state(id);
        self.sync_host_relay_status();
    }

    pub(crate) fn consume_terminal_scope_restore(&mut self, id: SessionId) -> bool {
        if self.pending_work.take_backend_scope_restore(id).is_none() {
            return false;
        }
        self.record_log(format!(
            "{} terminalized with pending scope repair; durable backend cleanup is eligible",
            id.short()
        ));
        true
    }

    fn restore_unrelayed_session_state(&mut self, id: SessionId) {
        let relayed_state = self.local.sessions.record(id).is_some_and(|record| {
            matches!(
                record.summary.state,
                SessionState::Running | SessionState::Published | SessionState::Reconnecting
            )
        });
        if relayed_state {
            self.local.sessions.update_state(id, SessionState::Running);
        }
    }

    pub(crate) fn delete_resurrectable_session_state(&mut self, id: SessionId) -> bool {
        if !self.local.sessions.delete(id) {
            return false;
        }

        self.consume_terminal_scope_restore(id);
        self.local.owned_session_runtimes.remove(id);
        self.sharing.shared_sessions.clear(id);
        self.sharing.host_relays.remove(id);
        self.agent_intel.registry.detach(id);
        if let Ok(store) = crate::rooms::mailbox_store::AgentRoomStore::open()
            && let Err(error) = store.remove_profile(id)
        {
            tracing::debug!(session_id = %id, %error, "could not retire Agent Room profile");
        }
        self.pending_work.clear_delete(id);
        self.shelf.sync_owned_sessions(self.local.sessions.ids());
        true
    }

    pub(crate) fn attach_host_relay(
        &mut self,
        id: SessionId,
        cancellation: CancellationToken,
        pending_permissions_tx: tokio::sync::watch::Sender<
            Option<kodosi_backend_client::relay::HostRelayPendingPermissionsSnapshot>,
        >,
        semantic_receipt_tx: tokio::sync::mpsc::Sender<
            kodosi_backend_client::relay::HostRelaySemanticReceiptDelivery,
        >,
        action_result_tx: tokio::sync::mpsc::Sender<
            kodosi_backend_client::relay::HostRelayActionResultDelivery,
        >,
        fence_completion_tx: tokio::sync::mpsc::Sender<
            kodosi_backend_client::relay::HostRelayFenceCompletion,
        >,
        join_handle: tokio::task::JoinHandle<()>,
    ) {
        self.sharing.host_relays.attach(
            id,
            cancellation,
            pending_permissions_tx,
            semantic_receipt_tx,
            action_result_tx,
            fence_completion_tx,
            join_handle,
        );
        self.sync_host_relay_status();
    }

    pub(crate) fn attach_session_relay(
        &mut self,
        id: SessionId,
        handle: SessionRelayClientHandle,
        share_scope: ShareScope,
        relay_mode: RemoteRelayMode,
    ) {
        self.remote.session_relays.attach(
            id,
            handle,
            share_scope,
            relay_mode,
            ConnectionState::Connecting,
        );
        self.sync_session_relay_status();
    }

    pub(crate) fn set_session_relay_status(
        &mut self,
        id: SessionId,
        relay_generation: u64,
        status: ConnectionState,
    ) -> bool {
        let changed = self
            .remote
            .session_relays
            .set_status(id, relay_generation, status);
        if changed {
            self.sync_session_relay_status();
        }
        changed
    }

    pub(crate) fn remove_detached_session_relay(
        &mut self,
        id: SessionId,
        relay_generation: u64,
    ) -> bool {
        let removed = self
            .remote
            .session_relays
            .remove_detached(id, relay_generation);
        self.sync_session_relay_status();
        removed
    }

    pub(crate) fn take_session_relay_for_shutdown(
        &mut self,
        id: SessionId,
    ) -> Option<crate::remote_sessions::relay::registry::DetachedSessionRelayShutdown> {
        let shutdown = self.remote.session_relays.take_for_shutdown(id);
        self.sync_session_relay_status();
        shutdown
    }

    pub(crate) async fn remove_session_relay_gracefully(&mut self, id: SessionId) -> bool {
        let removed = self.remote.session_relays.remove_gracefully(id).await;
        self.sync_session_relay_status();
        removed
    }

    pub(crate) fn cancel_session_relay_immediate(&mut self, id: SessionId) -> bool {
        let removed = self.remote.session_relays.cancel_immediate(id);
        self.sync_session_relay_status();
        removed
    }

    pub(crate) fn cancel_all_session_relays_immediate(&mut self) {
        self.remote.session_relays.cancel_all_immediate();
        self.sync_session_relay_status();
    }

    fn mark_session_failed(&mut self, id: SessionId, disposition: FailedSessionRuntimeDisposition) {
        match disposition {
            FailedSessionRuntimeDisposition::AwaitStoppedEvent => {
                self.local.sessions.update_state_with_recovery(
                    id,
                    SessionState::Failed,
                    kodosi_domain::session::LocalSessionRecoveryState::Recoverable,
                );
            }
            FailedSessionRuntimeDisposition::DropNow => {
                self.local.sessions.mark_failed(id);
                self.local.owned_session_runtimes.remove(id);
            }
        }
    }

    fn has_reconnecting_shared_sessions(&self) -> bool {
        self.sharing.shared_sessions.ids().any(|id| {
            self.local
                .sessions
                .record(id)
                .is_some_and(|record| record.summary.state == SessionState::Reconnecting)
        })
    }

    pub(crate) fn set_available_rooms(&mut self, rooms: Vec<RoomListEntry>) {
        self.available_rooms = rooms;
        self.room_catalog_loaded = true;
    }

    pub(crate) fn clear_room_catalog(&mut self) {
        self.available_rooms.clear();
        self.room_catalog_loaded = false;
    }

    pub(crate) fn queue_discovery_refresh(
        &mut self,
        surfaces: impl IntoIterator<Item = DiscoverySurface>,
    ) {
        self.pending_discovery_surfaces.extend(surfaces);
    }

    pub(crate) fn take_pending_discovery_refresh(&mut self) -> BTreeSet<DiscoverySurface> {
        std::mem::take(&mut self.pending_discovery_surfaces)
    }

    pub(crate) fn queue_room_projection_refresh(
        &mut self,
        room_id: String,
        surfaces: impl IntoIterator<Item = DiscoverySurface>,
    ) {
        self.pending_room_projection_refreshes
            .entry(room_id)
            .or_default()
            .extend(surfaces);
    }

    pub(crate) fn take_pending_room_projection_refreshes(
        &mut self,
    ) -> BTreeMap<String, BTreeSet<DiscoverySurface>> {
        std::mem::take(&mut self.pending_room_projection_refreshes)
    }

    pub(crate) fn clear_pending_discovery_refresh(&mut self) {
        self.pending_discovery_surfaces.clear();
        self.pending_room_projection_refreshes.clear();
    }

    pub(crate) fn queue_project_discovery(
        &mut self,
        working_dir: &str,
        allow_stale_refresh: bool,
    ) -> Option<ProjectDiscoveryRequest> {
        let normalized = working_dir.trim();
        if normalized.is_empty() {
            return None;
        }

        let needs_refresh = self
            .project_discovery
            .get(normalized)
            .is_none_or(|entry| allow_stale_refresh && entry.is_stale());
        if !needs_refresh {
            return None;
        }
        if !self
            .project_discovery_in_flight
            .insert(normalized.to_owned())
        {
            return None;
        }

        Some(ProjectDiscoveryRequest {
            working_dir: normalized.to_owned(),
        })
    }

    pub(crate) fn project_discovery(&self, working_dir: Option<&str>) -> Option<&ProjectDiscovery> {
        let normalized = working_dir?.trim();
        if normalized.is_empty() {
            return None;
        }

        self.project_discovery
            .get(normalized)
            .map(|entry| &entry.data)
    }

    pub(crate) fn record_log(&mut self, message: String) {
        push_log(&mut self.logs, message);
    }

    pub(crate) fn check_task_health(
        &mut self,
        completed_coordinators: &[SessionId],
    ) -> TaskHealthReport {
        let mut report = self.local.sessions.promote_stopping_timeouts();

        for (id, _, _) in &report.force_failed {
            self.local.owned_session_runtimes.remove(*id);
            self.sharing.host_relays.teardown(*id);
            self.agent_intel.registry.detach(*id);
        }

        for &id in completed_coordinators {
            let Some(record) = self.local.sessions.record(id) else {
                continue;
            };
            if record.summary.state == SessionState::Stopped {
                continue;
            }
            let local_incarnation_id = record.local_incarnation_id;

            self.mark_session_failed(id, FailedSessionRuntimeDisposition::DropNow);
            self.sharing.host_relays.teardown(id);
            self.agent_intel.registry.detach(id);
            report.messages.push(format!(
                "{} coordinator task exited unexpectedly",
                id.short()
            ));
            report.force_failed.push((
                id,
                local_incarnation_id,
                "coordinator task exited unexpectedly".to_owned(),
            ));
        }

        for id in self.agent_intel.registry.reap_finished() {
            report.messages.push(format!(
                "{} agent intel task exited unexpectedly",
                id.short()
            ));
            if let Some(agent_name) = self
                .local
                .sessions
                .record(id)
                .and_then(|record| record.summary.detected_agent.clone())
                && !matches!(
                    self.local
                        .sessions
                        .record(id)
                        .map(|record| record.summary.state),
                    Some(SessionState::Stopping | SessionState::Stopped | SessionState::Failed)
                )
            {
                report.restart_agent_intel.push((id, agent_name));
            }
        }

        for id in self.local.sessions.ids().to_vec() {
            let should_reconnect = self.sharing.host_relays.is_finished(id)
                && self.sharing.shared_sessions.contains(id)
                && self.local.sessions.record(id).is_some_and(|record| {
                    !matches!(
                        record.summary.state,
                        SessionState::Failed | SessionState::Stopped | SessionState::Stopping
                    )
                });
            if !should_reconnect {
                continue;
            }

            if let Some(record) = self.local.sessions.record_mut(id) {
                record.summary.state = SessionState::Reconnecting;
                record.summary.last_update = OffsetDateTime::now_utc();
            }
            self.sharing.host_relays.teardown(id);
            report.messages.push(format!(
                "{} host relay task exited unexpectedly",
                id.short()
            ));
        }

        report
    }

    pub(crate) fn sync_host_relay_status(&mut self) {
        self.host_ws_status = if self.has_reconnecting_shared_sessions() {
            ConnectionState::Reconnecting
        } else if self.sharing.shared_sessions.ids().any(|id| {
            self.local.sessions.record(id).is_some_and(|record| {
                !matches!(
                    record.summary.state,
                    SessionState::Stopped | SessionState::Failed
                )
            })
        }) {
            ConnectionState::Connected
        } else {
            ConnectionState::Offline
        };
    }

    pub(crate) fn sync_session_relay_status(&mut self) {
        self.viewer_ws_status = if self
            .remote
            .session_relays
            .any_status(ConnectionState::Reconnecting)
        {
            ConnectionState::Reconnecting
        } else if self
            .remote
            .session_relays
            .any_status(ConnectionState::Connecting)
        {
            ConnectionState::Connecting
        } else if self
            .remote
            .session_relays
            .any_status(ConnectionState::Connected)
        {
            ConnectionState::Connected
        } else {
            ConnectionState::Offline
        };
    }
}
