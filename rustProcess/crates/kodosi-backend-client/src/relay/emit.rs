use kodosi_domain::{ids::SessionId, session::SessionState};

use super::{HostRelayEvent, HostRelayEventSink};

pub async fn emit_info(event_sink: &dyn HostRelayEventSink, id: SessionId, message: String) {
    event_sink.emit(HostRelayEvent::Info { id, message }).await;
}

pub async fn emit_log_error(event_sink: &dyn HostRelayEventSink, id: SessionId, message: String) {
    event_sink
        .emit(HostRelayEvent::LogError { id, message })
        .await;
}

pub async fn emit_backend_access_invalid(event_sink: &dyn HostRelayEventSink, reason: String) {
    event_sink
        .emit(HostRelayEvent::BackendAccessInvalid { reason })
        .await;
}

pub async fn emit_host_demand(
    event_sink: &dyn HostRelayEventSink,
    id: SessionId,
    required: bool,
    participant_count: usize,
    reason: String,
) {
    event_sink
        .emit(HostRelayEvent::HostDemand {
            id,
            required,
            participant_count,
            reason,
        })
        .await;
}

pub async fn emit_state(event_sink: &dyn HostRelayEventSink, id: SessionId, state: SessionState) {
    event_sink
        .emit(HostRelayEvent::StateChanged { id, state })
        .await;
}

pub async fn emit_host_access_revoked(
    event_sink: &dyn HostRelayEventSink,
    id: SessionId,
    revoked_user_id: String,
) {
    event_sink
        .emit(HostRelayEvent::HostAccessRevoked {
            id,
            revoked_user_id,
        })
        .await;
}

pub async fn emit_host_frame_reservation_exhausted(
    event_sink: &dyn HostRelayEventSink,
    id: SessionId,
) {
    event_sink
        .emit(HostRelayEvent::HostFrameReservationExhausted { id })
        .await;
}

pub async fn emit_host_key_distribution_requested(
    event_sink: &dyn HostRelayEventSink,
    id: SessionId,
    fence_id: String,
) {
    event_sink
        .emit(HostRelayEvent::HostKeyDistributionRequested { id, fence_id })
        .await;
}

pub async fn emit_host_key_rotation_required(
    event_sink: &dyn HostRelayEventSink,
    id: SessionId,
    reason: String,
    fail_closed: bool,
) {
    event_sink
        .emit(HostRelayEvent::HostKeyRotationRequired {
            id,
            reason,
            fail_closed,
        })
        .await;
}
