use crate::{
    host_protocol::{AuthEvent, DeviceEvent},
    runtime::AppState,
    session_runtime::events::DiscoverySurface,
};
use kodosi_domain::device_link::{DeviceLinkOutcome, SelfDeviceLinkOutcome};

impl AppState {
    pub(crate) fn apply_discovery_invalidated(
        &mut self,
        surfaces: impl IntoIterator<Item = DiscoverySurface>,
        room_id: Option<String>,
    ) {
        let surfaces = surfaces.into_iter().collect::<Vec<_>>();
        if let Some(room_id) = room_id {
            let room_surfaces = surfaces.iter().copied().filter(|surface| {
                matches!(
                    surface,
                    DiscoverySurface::RoomChat | DiscoverySurface::RoomTasks
                )
            });
            self.queue_room_projection_refresh(room_id, room_surfaces);
        }
        self.queue_discovery_refresh(surfaces);
    }

    pub(crate) fn apply_user_identity_lifecycle_changed(
        &mut self,
        user_id: &str,
        identity_revision: u64,
        state: kodosi_domain::user::IdentityLifecycleState,
    ) {
        tracing::info!(
            user_id,
            identity_revision,
            ?state,
            "received user identity lifecycle change"
        );
        let _queued = self.pending_work.queue_identity_lifecycle(
            crate::runtime::pending_work::IdentityLifecycleWork {
                user_id: user_id.to_owned(),
                identity_revision,
                state,
            },
        );
    }

    pub(crate) fn apply_user_device_list_changed(&mut self, user_id: &str, generation: u64) {
        tracing::info!(
            user_id = %user_id,
            generation,
            "received user.deviceListChanged event"
        );
        self.pending_work.queue_pin_refresh(user_id);
        if self.identity.auth.subject_string().as_deref() == Some(user_id) {
            self.invalidate_hosted_session_keys();
        }
    }

    pub(crate) fn invalidate_hosted_session_keys(&mut self) {
        let hosted_ids = self.sharing.shared_sessions.ids().collect::<Vec<_>>();
        for id in hosted_ids {
            self.sharing.host_relays.cancel(id);
            self.sharing.shared_sessions.set_session_key(id, None, None);
            self.pending_work.queue_host_key_rotation(id);
        }
    }

    pub(crate) fn rotate_room_scoped_session_keys(
        &mut self,
        room_id: &str,
    ) -> Vec<kodosi_domain::ids::SessionId> {
        let affected = self.sharing.shared_sessions.ids_scoped_to_room(room_id);
        for id in &affected {
            self.sharing.host_relays.cancel(*id);
            self.sharing
                .shared_sessions
                .set_session_key(*id, None, None);
            self.pending_work.queue_host_key_rotation(*id);
        }
        if !affected.is_empty() {
            tracing::info!(
                room_id = %room_id,
                sessions = affected.len(),
                "rotating room-scoped session keys after roster removal"
            );
        }
        affected
    }

    pub(crate) fn apply_device_link_snapshot(
        &mut self,
        requests: Vec<kodosi_backend_client::user_events::UserDeviceLinkSnapshotEntry>,
    ) {
        self.runtime_outbox
            .queue_devices(DeviceEvent::LinkSnapshot {
                requests: requests
                    .into_iter()
                    .map(|request| crate::host_protocol::DeviceLinkRequestEntry {
                        user_code: request.user_code,
                        device_label: request.device_label,
                        expires_at: request.expires_at,
                    })
                    .collect(),
            });
    }

    pub(crate) fn apply_device_link_requested(
        &mut self,
        user_code: String,
        device_label: String,
        expires_at: String,
    ) {
        self.runtime_outbox
            .queue_devices(DeviceEvent::LinkRequested {
                user_code,
                device_label,
                expires_at,
            });
    }

    pub(crate) fn apply_device_link_resolved(
        &mut self,
        user_code: String,
        outcome: DeviceLinkOutcome,
    ) {
        self.runtime_outbox
            .queue_devices(DeviceEvent::LinkResolved { user_code, outcome });
    }

    pub(crate) fn apply_device_link_self_pending(&mut self, user_code: String, expires_at: String) {
        self.runtime_outbox
            .queue_devices(DeviceEvent::LinkSelfPending {
                user_code,
                expires_at,
            });
    }

    pub(crate) fn apply_device_link_self_resolved(&mut self, outcome: SelfDeviceLinkOutcome) {
        if outcome == SelfDeviceLinkOutcome::Approved {
            self.runtime_outbox
                .queue_auth(AuthEvent::Notice { message: None });
        }
        self.runtime_outbox
            .queue_devices(DeviceEvent::LinkSelfResolved { outcome });
    }
}
