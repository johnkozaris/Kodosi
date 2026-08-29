use std::collections::{HashSet, VecDeque};

use crate::{Result, host_ws::HostWebSocketStream};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use kodosi_domain::{
    permissions::SessionCapabilities,
    terminal::{TerminalPixelGeometry, TerminalSize},
};
use serde::de::DeserializeOwned;

use super::{
    HostRelayEvent, HostRelayEventSink, HostRelaySpec, SemanticAdmissionReply,
    emit::{
        emit_host_access_revoked, emit_host_demand, emit_host_key_distribution_requested,
        emit_host_key_rotation_required, emit_info,
    },
    port::ClientFocus,
    transport::LiveStreamState,
    wire::{
        HOST_RESIZE_MAX_COLS, HOST_RESIZE_MAX_ROWS, HostAccessRevokedMessage, HostEnvelope,
        HostFocusChangedMessage, HostInputMessage, HostInterruptMessage,
        HostKeyDistributionRequestedMessage, HostParticipantChangedMessage,
        HostParticipantDisconnectedMessage, HostPermissionDecisionMessage, HostResizeMessage,
        HostSemanticCancelMessage, HostSemanticSendMessage, HostStopMessage,
        HostStreamDemandMessage, HostSuggestionMessage, send_fence_ack,
    },
};

#[derive(Debug, Clone, Copy)]
struct ControlSender<'a> {
    user_id: &'a str,
    device_id: &'a str,
    signature_b64: &'a str,
}

const HOST_FENCE_DEDUPE_CAPACITY: usize = 1_024;

#[derive(Debug, Default)]
pub(super) struct HostFenceDedupe {
    completed: HashSet<String>,
    pending: HashSet<String>,
    order: VecDeque<String>,
}

impl HostFenceDedupe {
    fn contains(&self, fence_id: &str) -> bool {
        self.completed.contains(fence_id)
    }

    fn is_pending(&self, fence_id: &str) -> bool {
        self.pending.contains(fence_id)
    }

    fn begin(&mut self, fence_id: &str) -> bool {
        if self.contains(fence_id) || self.is_pending(fence_id) {
            return false;
        }
        self.pending.insert(fence_id.to_owned())
    }

    pub(super) fn complete(&mut self, fence_id: &str) {
        self.pending.remove(fence_id);
        self.remember(fence_id);
    }

    fn remember(&mut self, fence_id: &str) {
        if self.contains(fence_id) {
            return;
        }
        if self.order.len() >= HOST_FENCE_DEDUPE_CAPACITY
            && let Some(oldest) = self.order.pop_front()
        {
            self.completed.remove(&oldest);
        }
        let owned = fence_id.to_owned();
        self.order.push_back(owned.clone());
        self.completed.insert(owned);
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum OwnerMessageOutcome {
    Continue,
    ParticipantActionRejected { reason: String },
    ProtocolDrift { reason: String },
}

#[tracing::instrument(skip_all, fields(session_id = %spec.id, backend_session_id = %spec.backend_session_id), err)]
pub(super) async fn handle_owner_message(
    spec: &HostRelaySpec,
    stream: &mut HostWebSocketStream,
    message: &str,
    live_stream: &mut LiveStreamState,
    pending_render: &mut bool,
    fence_dedupe: &mut HostFenceDedupe,
    event_sink: &dyn HostRelayEventSink,
) -> Result<OwnerMessageOutcome> {
    let outcome = route_owner_message(
        spec,
        stream,
        message,
        live_stream,
        pending_render,
        fence_dedupe,
        event_sink,
    )
    .await;

    drain_control_rotation_demand(spec, event_sink).await;
    outcome
}

async fn drain_control_rotation_demand(spec: &HostRelaySpec, event_sink: &dyn HostRelayEventSink) {
    let Some(reason) = spec.control_trust.take_rotation_demand() else {
        return;
    };
    tracing::warn!(
        session_id = %spec.id,
        backend_session_id = %spec.backend_session_id,
        reason = reason.as_str(),
        fail_closed = reason.is_fail_closed(),
        "control replay window demands session key rotation",
    );
    emit_host_key_rotation_required(
        event_sink,
        spec.id,
        reason.as_str().to_owned(),
        reason.is_fail_closed(),
    )
    .await;
}

async fn route_owner_message(
    spec: &HostRelaySpec,
    stream: &mut HostWebSocketStream,
    message: &str,
    live_stream: &mut LiveStreamState,
    pending_render: &mut bool,
    fence_dedupe: &mut HostFenceDedupe,
    event_sink: &dyn HostRelayEventSink,
) -> Result<OwnerMessageOutcome> {
    let envelope: HostEnvelope = match serde_json::from_str(message) {
        Ok(envelope) => envelope,
        Err(error) => {
            tracing::warn!(%error, "malformed host relay envelope");
            return Ok(protocol_drift(format!(
                "malformed host relay envelope: {error}"
            )));
        }
    };
    tracing::trace!(
        message_type = %envelope.message_type,
        session_id = %spec.id,
        backend_session_id = %spec.backend_session_id,
        "host relay message received"
    );
    let Some(message_type) = super::HostIncomingMessageType::parse(&envelope.message_type) else {
        tracing::warn!(
            message_type = envelope.message_type,
            "unknown host relay message type"
        );
        return Ok(protocol_drift(format!(
            "unknown host relay message type `{}`",
            envelope.message_type
        )));
    };
    match message_type {
        super::HostIncomingMessageType::StreamDemand => {
            route_stream_demand(spec, message, live_stream, pending_render, event_sink).await
        }
        super::HostIncomingMessageType::SemanticSend => {
            route_semantic_send(spec, message, event_sink).await
        }
        super::HostIncomingMessageType::SemanticCancel => {
            route_semantic_cancel(spec, message, event_sink).await
        }
        super::HostIncomingMessageType::Input => route_input(spec, message, event_sink).await,
        super::HostIncomingMessageType::Resize => {
            route_resize(spec, message, live_stream, pending_render, event_sink).await
        }
        super::HostIncomingMessageType::FocusChanged => {
            route_focus_changed(spec, message, event_sink).await
        }
        super::HostIncomingMessageType::ParticipantDisconnected => {
            route_participant_disconnected(spec, message, event_sink).await
        }
        super::HostIncomingMessageType::Stop => route_stop(spec, message).await,
        super::HostIncomingMessageType::Interrupt => route_interrupt(spec, message).await,
        super::HostIncomingMessageType::Suggestion => {
            route_suggestion(spec, message, event_sink).await
        }
        super::HostIncomingMessageType::PermissionDecision => {
            route_permission_decision(spec, stream, message, event_sink).await
        }
        super::HostIncomingMessageType::ParticipantChanged => {
            route_participant_changed(spec, stream, message, fence_dedupe, event_sink).await
        }
        super::HostIncomingMessageType::AccessRevoked => {
            route_access_revoked(spec, stream, message, fence_dedupe, event_sink).await
        }
        super::HostIncomingMessageType::KeyDistributionRequested => {
            route_key_distribution_requested(spec, stream, message, fence_dedupe, event_sink).await
        }
        super::HostIncomingMessageType::Accepted
        | super::HostIncomingMessageType::ActionResultAck
        | super::HostIncomingMessageType::SemanticReceiptAck => Ok(protocol_drift(format!(
            "host relay message type `{}` reached the wrong lifecycle phase",
            envelope.message_type
        ))),
    }
}

fn protocol_drift(reason: String) -> OwnerMessageOutcome {
    OwnerMessageOutcome::ProtocolDrift { reason }
}

fn participant_action_rejected(
    message_type: &str,
    action_id: Option<&str>,
    reason: impl Into<String>,
) -> OwnerMessageOutcome {
    let reason = reason.into();
    tracing::warn!(
        message_type,
        action_id = action_id.unwrap_or("<unknown>"),
        reason,
        "rejecting invalid participant action without stopping the host relay",
    );
    OwnerMessageOutcome::ParticipantActionRejected { reason }
}

fn parse_owner_message<T>(
    message_type: &str,
    message: &str,
) -> std::result::Result<T, OwnerMessageOutcome>
where
    T: DeserializeOwned,
{
    serde_json::from_str::<T>(message).map_err(|error| {
        tracing::warn!(%error, message_type, "malformed host relay message");
        protocol_drift(format!(
            "malformed host relay message `{message_type}`: {error}"
        ))
    })
}

fn parse_participant_action<T>(
    message_type: &str,
    message: &str,
) -> std::result::Result<T, OwnerMessageOutcome>
where
    T: DeserializeOwned,
{
    serde_json::from_str::<T>(message).map_err(|error| {
        let action_id = action_id_from_message(message);
        participant_action_rejected(
            message_type,
            action_id.as_deref(),
            format!("malformed participant action `{message_type}`: {error}"),
        )
    })
}

fn action_id_from_message(message: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(message).ok()?;
    value
        .get("commandId")
        .or_else(|| value.get("suggestionId"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

fn ensure_current_session(
    message_type: &str,
    session_id: &str,
    backend_session_id: &str,
) -> std::result::Result<(), OwnerMessageOutcome> {
    if session_id == backend_session_id {
        return Ok(());
    }
    tracing::warn!(
        message_type,
        session_id,
        backend_session_id,
        "host relay message targeted another session"
    );
    Err(protocol_drift(format!(
        "host relay message `{message_type}` targeted session `{session_id}` while connected to `{backend_session_id}`",
    )))
}

async fn route_stream_demand(
    spec: &HostRelaySpec,
    message: &str,
    live_stream: &mut LiveStreamState,
    pending_render: &mut bool,
    event_sink: &dyn HostRelayEventSink,
) -> Result<OwnerMessageOutcome> {
    let demand: HostStreamDemandMessage = match parse_owner_message("host.streamDemand", message) {
        Ok(demand) => demand,
        Err(outcome) => return Ok(outcome),
    };
    if let Err(outcome) = ensure_current_session(
        "host.streamDemand",
        demand.session_id.as_str(),
        spec.backend_session_id.as_str(),
    ) {
        return Ok(outcome);
    }
    handle_stream_demand(spec, demand, live_stream, pending_render, event_sink).await
}

async fn route_semantic_send(
    spec: &HostRelaySpec,
    message: &str,
    event_sink: &dyn HostRelayEventSink,
) -> Result<OwnerMessageOutcome> {
    let request: HostSemanticSendMessage =
        match parse_participant_action("host.semanticSend", message) {
            Ok(request) => request,
            Err(outcome) => return Ok(outcome),
        };
    if let Err(outcome) = ensure_current_session(
        "host.semanticSend",
        &request.session_id,
        &spec.backend_session_id,
    ) {
        return Ok(outcome);
    }
    let text = match decrypt_semantic_send(spec, &request) {
        Ok(text) => text,
        Err(reason) => {
            return Ok(participant_action_rejected(
                "host.semanticSend",
                Some(&request.request_id.to_string()),
                reason,
            ));
        }
    };
    let (reply, admitted) = SemanticAdmissionReply::channel();
    event_sink
        .emit(HostRelayEvent::SemanticSend {
            id: spec.id,
            request_id: request.request_id,
            incarnation_id: request.incarnation_id,
            mode: request.mode,
            payload_sha256: request.payload_sha256.clone(),
            text,
            requester_user_id: request.sender_user_id.clone(),
            requester_device_id: request.sender_device_id.clone(),
            reply,
        })
        .await;
    match tokio::time::timeout(std::time::Duration::from_secs(10), admitted).await {
        Ok(Ok(true)) => Ok(OwnerMessageOutcome::Continue),
        Ok(Ok(false) | Err(_)) | Err(_) => Ok(participant_action_rejected(
            "host.semanticSend",
            Some(&request.request_id.to_string()),
            "runtime semantic admission rejected the request",
        )),
    }
}

async fn route_semantic_cancel(
    spec: &HostRelaySpec,
    message: &str,
    event_sink: &dyn HostRelayEventSink,
) -> Result<OwnerMessageOutcome> {
    let request: HostSemanticCancelMessage =
        match parse_participant_action("host.semanticCancel", message) {
            Ok(request) => request,
            Err(outcome) => return Ok(outcome),
        };
    if let Err(outcome) = ensure_current_session(
        "host.semanticCancel",
        &request.session_id,
        &spec.backend_session_id,
    ) {
        return Ok(outcome);
    }
    if let Err(reason) = verify_semantic_cancel(spec, &request) {
        return Ok(participant_action_rejected(
            "host.semanticCancel",
            Some(&request.request_id.to_string()),
            reason,
        ));
    }
    let (reply, admitted) = SemanticAdmissionReply::channel();
    event_sink
        .emit(HostRelayEvent::SemanticCancel {
            id: spec.id,
            request_id: request.request_id,
            incarnation_id: request.incarnation_id,
            mode: request.mode,
            payload_sha256: request.payload_sha256.clone(),
            requester_user_id: request.requester_user_id.clone(),
            requester_device_id: request.requester_device_id.clone(),
            reply,
        })
        .await;
    match tokio::time::timeout(std::time::Duration::from_secs(10), admitted).await {
        Ok(Ok(true)) => Ok(OwnerMessageOutcome::Continue),
        Ok(Ok(false) | Err(_)) | Err(_) => Ok(participant_action_rejected(
            "host.semanticCancel",
            Some(&request.request_id.to_string()),
            "runtime semantic cancellation rejected the request",
        )),
    }
}

fn decrypt_semantic_send(
    spec: &HostRelaySpec,
    request: &HostSemanticSendMessage,
) -> std::result::Result<String, String> {
    use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
    if request.sender_user_id != spec.owner_user_id
        || request.incarnation_id != spec.backend_incarnation_id
    {
        return Err("semantic request is not from this owner/incarnation".to_owned());
    }
    let session_id = uuid::Uuid::parse_str(&request.session_id)
        .map_err(|_| "semantic session ID is invalid".to_owned())?;
    let requester_user_id = uuid::Uuid::parse_str(&request.sender_user_id)
        .map_err(|_| "semantic requester user ID is invalid".to_owned())?;
    let nonce = BASE64
        .decode(&request.nonce)
        .map_err(|error| format!("semantic nonce is not valid base64: {error}"))?;
    let ciphertext = BASE64
        .decode(&request.ciphertext)
        .map_err(|error| format!("semantic ciphertext is not valid base64: {error}"))?;
    let signature = BASE64
        .decode(&request.signature)
        .map_err(|error| format!("semantic signature is not valid base64: {error}"))?;
    let mode = request.mode.as_str();
    let preimage = crate::crypto::semantic_request_signature_preimage(
        &session_id,
        &request.incarnation_id,
        &request.request_id,
        mode,
        &request.payload_sha256,
        &requester_user_id,
        &request.sender_device_id,
        &nonce,
        &ciphertext,
    )
    .map_err(|error| error.to_string())?;
    match spec.control_trust.authorize(
        &request.sender_user_id,
        &request.sender_device_id,
        &preimage,
        &signature,
        SessionCapabilities::STOP,
    ) {
        crate::control::ControlAuthorization::Authorized => {}
        _ => return Err("semantic request signature or owner capability is invalid".to_owned()),
    }
    let aad = crate::crypto::semantic_request_associated_data(
        &session_id,
        &request.incarnation_id,
        &request.request_id,
        mode,
        &request.payload_sha256,
        &requester_user_id,
        &request.sender_device_id,
    )
    .map_err(|error| error.to_string())?;
    let plaintext =
        crate::crypto::decrypt_semantic_request(&spec.session_key, &aad, &nonce, &ciphertext)
            .map_err(|error| error.to_string())?;
    let text = String::from_utf8(plaintext)
        .map_err(|error| format!("semantic plaintext is not UTF-8: {error}"))?;
    if crate::crypto::sha256_hex(text.as_bytes()) != request.payload_sha256 {
        return Err("semantic plaintext fingerprint mismatch".to_owned());
    }
    Ok(text)
}

fn verify_semantic_cancel(
    spec: &HostRelaySpec,
    request: &HostSemanticCancelMessage,
) -> std::result::Result<(), String> {
    use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
    if request.requester_user_id != spec.owner_user_id
        || request.incarnation_id != spec.backend_incarnation_id
    {
        return Err("semantic cancellation is not from this owner/incarnation".to_owned());
    }
    let session_id = uuid::Uuid::parse_str(&request.session_id)
        .map_err(|_| "semantic session ID is invalid".to_owned())?;
    let requester_user_id = uuid::Uuid::parse_str(&request.requester_user_id)
        .map_err(|_| "semantic requester user ID is invalid".to_owned())?;
    let signature = BASE64
        .decode(&request.signature)
        .map_err(|error| format!("semantic cancel signature is not valid base64: {error}"))?;
    let preimage = crate::crypto::semantic_cancel_preimage(
        &session_id,
        &request.incarnation_id,
        &request.request_id,
        request.mode.as_str(),
        &request.payload_sha256,
        &requester_user_id,
        &request.requester_device_id,
    )
    .map_err(|error| error.to_string())?;
    match spec.control_trust.authorize(
        &request.requester_user_id,
        &request.requester_device_id,
        &preimage,
        &signature,
        SessionCapabilities::STOP,
    ) {
        crate::control::ControlAuthorization::Authorized => Ok(()),
        _ => Err("semantic cancellation signature or owner capability is invalid".to_owned()),
    }
}

async fn route_input(
    spec: &HostRelaySpec,
    message: &str,
    event_sink: &dyn HostRelayEventSink,
) -> Result<OwnerMessageOutcome> {
    let input: HostInputMessage = match parse_participant_action("host.input", message) {
        Ok(input) => input,
        Err(outcome) => return Ok(outcome),
    };
    if let Err(outcome) = ensure_current_session(
        "host.input",
        input.session_id.as_str(),
        spec.backend_session_id.as_str(),
    ) {
        return Ok(outcome);
    }
    let payload = match decrypt_control_bytes(
        spec,
        "inject",
        &input.command_id,
        &input.nonce,
        &input.ciphertext,
        ControlSender {
            user_id: &input.sender_user_id,
            device_id: &input.sender_device_id,
            signature_b64: &input.signature,
        },
        SessionCapabilities::SEND_INPUT,
    ) {
        Ok(payload) => payload,
        Err(reason) => {
            return Ok(participant_action_rejected(
                "host.input",
                Some(&input.command_id),
                reason,
            ));
        }
    };
    handle_input(spec, input, &payload, event_sink).await
}

async fn emit_resize_result(
    spec: &HostRelaySpec,
    resize: &HostResizeMessage,
    accepted: bool,
    event_sink: &dyn HostRelayEventSink,
) {
    event_sink
        .emit(HostRelayEvent::ActionCompleted {
            id: spec.id,
            incarnation_id: spec.backend_incarnation_id,
            action_id: resize.command_id.clone(),
            request_id: resize.command_id.clone(),
            requester_user_id: resize.sender_user_id.clone(),
            requester_device_id: resize.sender_device_id.clone(),
            accepted,
        })
        .await;
}

async fn reject_resize(
    spec: &HostRelaySpec,
    resize: &HostResizeMessage,
    reason: String,
    event_sink: &dyn HostRelayEventSink,
) -> Result<OwnerMessageOutcome> {
    emit_resize_result(spec, resize, false, event_sink).await;
    Ok(participant_action_rejected(
        "host.resize",
        Some(&resize.command_id),
        reason,
    ))
}

#[expect(
    clippy::too_many_lines,
    reason = "one authenticated resize transaction validates dimensions, geometry, signature, and capability before dispatch"
)]
async fn route_resize(
    spec: &HostRelaySpec,
    message: &str,
    live_stream: &LiveStreamState,
    pending_render: &mut bool,
    event_sink: &dyn HostRelayEventSink,
) -> Result<OwnerMessageOutcome> {
    let resize: HostResizeMessage = match parse_participant_action("host.resize", message) {
        Ok(resize) => resize,
        Err(outcome) => return Ok(outcome),
    };
    if let Err(outcome) = ensure_current_session(
        "host.resize",
        resize.session_id.as_str(),
        spec.backend_session_id.as_str(),
    ) {
        return Ok(outcome);
    }

    if resize.rows == 0
        || resize.cols == 0
        || resize.rows > HOST_RESIZE_MAX_ROWS
        || resize.cols > HOST_RESIZE_MAX_COLS
    {
        tracing::warn!(
            session_id = %resize.session_id,
            command_id = %resize.command_id,
            rows = resize.rows,
            cols = resize.cols,
            max_rows = HOST_RESIZE_MAX_ROWS,
            max_cols = HOST_RESIZE_MAX_COLS,
            "host relay rejecting resize outside supported range",
        );
        return reject_resize(
            spec,
            &resize,
            format!(
                "host.resize dimensions {}x{} outside [1, {}]x[1, {}]",
                resize.rows, resize.cols, HOST_RESIZE_MAX_ROWS, HOST_RESIZE_MAX_COLS,
            ),
            event_sink,
        )
        .await;
    }
    let size = match TerminalSize::new(resize.rows, resize.cols) {
        Ok(size) => size,
        Err(error) => {
            tracing::warn!(
                session_id = %resize.session_id,
                command_id = %resize.command_id,
                error = %error,
                "host relay received invalid resize dimensions",
            );
            return reject_resize(
                spec,
                &resize,
                format!("host.resize invalid dimensions: {error}"),
                event_sink,
            )
            .await;
        }
    };
    let pixel_geometry = match (
        resize.width_pixels,
        resize.height_pixels,
        resize.cell_width_pixels,
        resize.cell_height_pixels,
    ) {
        (None, None, None, None) => None,
        (Some(width), Some(height), Some(cell_width), Some(cell_height)) => {
            match TerminalPixelGeometry::new(width, height, cell_width, cell_height)
                .and_then(|geometry| geometry.validate_for_size(size))
            {
                Ok(geometry) => Some(geometry),
                Err(error) => {
                    return reject_resize(
                        spec,
                        &resize,
                        format!("host.resize invalid pixel geometry: {error}"),
                        event_sink,
                    )
                    .await;
                }
            }
        }
        _ => {
            return reject_resize(
                spec,
                &resize,
                "host.resize pixel geometry fields must be supplied together".to_owned(),
                event_sink,
            )
            .await;
        }
    };
    let geometry_tag = pixel_geometry.map_or_else(
        || "none".to_owned(),
        |geometry| {
            format!(
                "{}:{}:{}:{}",
                geometry.width_pixels(),
                geometry.height_pixels(),
                geometry.cell_width_pixels(),
                geometry.cell_height_pixels()
            )
        },
    );
    let kind = format!(
        "resize:{}:{}:{}:{geometry_tag}",
        resize.rows, resize.cols, resize.claim
    );
    if resize.sender_user_id != spec.owner_user_id {
        return reject_resize(
            spec,
            &resize,
            "host.resize is owner-only".to_owned(),
            event_sink,
        )
        .await;
    }
    if let Err(reason) = decrypt_control_text(
        spec,
        &kind,
        &resize.command_id,
        &resize.nonce,
        &resize.ciphertext,
        ControlSender {
            user_id: &resize.sender_user_id,
            device_id: &resize.sender_device_id,
            signature_b64: &resize.signature,
        },
        SessionCapabilities::OWNER,
    ) {
        return reject_resize(spec, &resize, reason, event_sink).await;
    }

    handle_resize(
        spec,
        &resize,
        size,
        pixel_geometry,
        resize.sender_user_id == spec.owner_user_id,
        resize.claim,
        live_stream,
        pending_render,
        event_sink,
    )
    .await
}

async fn route_focus_changed(
    spec: &HostRelaySpec,
    message: &str,
    event_sink: &dyn HostRelayEventSink,
) -> Result<OwnerMessageOutcome> {
    let focus_changed: HostFocusChangedMessage =
        match parse_participant_action("host.focusChanged", message) {
            Ok(focus_changed) => focus_changed,
            Err(outcome) => return Ok(outcome),
        };
    if let Err(outcome) = ensure_current_session(
        "host.focusChanged",
        focus_changed.session_id.as_str(),
        spec.backend_session_id.as_str(),
    ) {
        return Ok(outcome);
    }

    if focus_changed.client_id.is_empty() {
        tracing::warn!(
            session_id = %focus_changed.session_id,
            command_id = %focus_changed.command_id,
            "host.focusChanged with empty clientId; refusing to forward",
        );
        return Ok(protocol_drift(
            "host.focusChanged clientId must be non-empty".to_owned(),
        ));
    }
    let kind = format!("focus:{}", focus_changed.focused);
    if let Err(reason) = decrypt_control_text(
        spec,
        &kind,
        &focus_changed.command_id,
        &focus_changed.nonce,
        &focus_changed.ciphertext,
        ControlSender {
            user_id: &focus_changed.sender_user_id,
            device_id: &focus_changed.sender_device_id,
            signature_b64: &focus_changed.signature,
        },
        SessionCapabilities::FOCUS,
    ) {
        return Ok(participant_action_rejected(
            "host.focusChanged",
            Some(&focus_changed.command_id),
            reason,
        ));
    }

    handle_focus_changed(
        spec,
        focus_changed.command_id,
        focus_changed.client_id,
        ClientFocus::from_bool(focus_changed.focused),
        event_sink,
    )
    .await
}

async fn route_participant_disconnected(
    spec: &HostRelaySpec,
    message: &str,
    event_sink: &dyn HostRelayEventSink,
) -> Result<OwnerMessageOutcome> {
    let disconnected: HostParticipantDisconnectedMessage =
        match parse_owner_message("host.participantDisconnected", message) {
            Ok(disconnected) => disconnected,
            Err(outcome) => return Ok(outcome),
        };
    if let Err(outcome) = ensure_current_session(
        "host.participantDisconnected",
        disconnected.session_id.as_str(),
        spec.backend_session_id.as_str(),
    ) {
        return Ok(outcome);
    }
    if disconnected.client_id.is_empty() {
        tracing::warn!(
            session_id = %disconnected.session_id,
            "host.participantDisconnected with empty clientId; refusing to forward",
        );
        return Ok(protocol_drift(
            "host.participantDisconnected clientId must be non-empty".to_owned(),
        ));
    }

    handle_focus_changed(
        spec,
        format!("disconnect:{}", disconnected.client_id),
        disconnected.client_id,
        ClientFocus::Blurred,
        event_sink,
    )
    .await
}

async fn route_stop(spec: &HostRelaySpec, message: &str) -> Result<OwnerMessageOutcome> {
    let stop: HostStopMessage = match parse_participant_action("host.stop", message) {
        Ok(stop) => stop,
        Err(outcome) => return Ok(outcome),
    };
    if let Err(outcome) = ensure_current_session(
        "host.stop",
        stop.session_id.as_str(),
        spec.backend_session_id.as_str(),
    ) {
        return Ok(outcome);
    }
    if stop.sender_user_id != spec.owner_user_id {
        return Ok(participant_action_rejected(
            "host.stop",
            Some(&stop.command_id),
            "host.stop was not signed by the session owner",
        ));
    }
    if let Err(reason) = decrypt_control_text(
        spec,
        "stop",
        &stop.command_id,
        &stop.nonce,
        &stop.ciphertext,
        ControlSender {
            user_id: &stop.sender_user_id,
            device_id: &stop.sender_device_id,
            signature_b64: &stop.signature,
        },
        SessionCapabilities::STOP,
    ) {
        return Ok(participant_action_rejected(
            "host.stop",
            Some(&stop.command_id),
            reason,
        ));
    }
    handle_stop(spec, stop).await
}

async fn route_interrupt(spec: &HostRelaySpec, message: &str) -> Result<OwnerMessageOutcome> {
    let interrupt: HostInterruptMessage = match parse_participant_action("host.interrupt", message)
    {
        Ok(interrupt) => interrupt,
        Err(outcome) => return Ok(outcome),
    };
    if let Err(outcome) = ensure_current_session(
        "host.interrupt",
        interrupt.session_id.as_str(),
        spec.backend_session_id.as_str(),
    ) {
        return Ok(outcome);
    }
    if interrupt.sender_user_id != spec.owner_user_id {
        return Ok(participant_action_rejected(
            "host.interrupt",
            Some(&interrupt.command_id),
            "host.interrupt was not signed by the session owner",
        ));
    }
    if let Err(reason) = decrypt_control_text(
        spec,
        "interrupt",
        &interrupt.command_id,
        &interrupt.nonce,
        &interrupt.ciphertext,
        ControlSender {
            user_id: &interrupt.sender_user_id,
            device_id: &interrupt.sender_device_id,
            signature_b64: &interrupt.signature,
        },
        SessionCapabilities::STOP,
    ) {
        return Ok(participant_action_rejected(
            "host.interrupt",
            Some(&interrupt.command_id),
            reason,
        ));
    }
    handle_interrupt(spec, interrupt).await
}

async fn route_suggestion(
    spec: &HostRelaySpec,
    message: &str,
    event_sink: &dyn HostRelayEventSink,
) -> Result<OwnerMessageOutcome> {
    let suggestion: HostSuggestionMessage =
        match parse_participant_action("host.suggestion", message) {
            Ok(suggestion) => suggestion,
            Err(outcome) => return Ok(outcome),
        };
    if let Err(outcome) = ensure_current_session(
        "host.suggestion",
        suggestion.session_id.as_str(),
        spec.backend_session_id.as_str(),
    ) {
        return Ok(outcome);
    }
    let body = match decrypt_control_text(
        spec,
        "suggest",
        &suggestion.suggestion_id,
        &suggestion.nonce,
        &suggestion.ciphertext,
        ControlSender {
            user_id: &suggestion.sender_user_id,
            device_id: &suggestion.sender_device_id,
            signature_b64: &suggestion.signature,
        },
        SessionCapabilities::SUGGEST,
    ) {
        Ok(body) => body,
        Err(reason) => {
            return Ok(participant_action_rejected(
                "host.suggestion",
                Some(&suggestion.suggestion_id),
                reason,
            ));
        }
    };
    emit_info(event_sink, spec.id, format!("remote suggestion: {body}")).await;
    Ok(OwnerMessageOutcome::Continue)
}

async fn route_permission_decision(
    spec: &HostRelaySpec,
    _stream: &mut HostWebSocketStream,
    message: &str,
    event_sink: &dyn HostRelayEventSink,
) -> Result<OwnerMessageOutcome> {
    let decision: HostPermissionDecisionMessage =
        match parse_participant_action("host.permissionDecision", message) {
            Ok(decision) => decision,
            Err(outcome) => return Ok(outcome),
        };
    if let Err(outcome) = ensure_current_session(
        "host.permissionDecision",
        decision.session_id.as_str(),
        spec.backend_session_id.as_str(),
    ) {
        return Ok(outcome);
    }

    let kind = format!(
        "permission:{}:{}:{}",
        decision.decision, decision.request_id, decision.request_generation
    );
    if !matches!(decision.decision.as_str(), "allow" | "deny") {
        return Ok(participant_action_rejected(
            "host.permissionDecision",
            Some(&decision.command_id),
            "permission decision must be allow or deny",
        ));
    }
    let _plaintext = match decrypt_control_text(
        spec,
        &kind,
        &decision.command_id,
        &decision.nonce,
        &decision.ciphertext,
        ControlSender {
            user_id: &decision.decider_user_id,
            device_id: &decision.decider_device_id,
            signature_b64: &decision.signature,
        },
        SessionCapabilities::APPROVE_DENY,
    ) {
        Ok(payload) => payload,
        Err(reason) => {
            return Ok(participant_action_rejected(
                "host.permissionDecision",
                Some(&decision.command_id),
                reason,
            ));
        }
    };
    let (reply, applied) = SemanticAdmissionReply::channel();
    event_sink
        .emit(HostRelayEvent::PermissionDecision {
            id: spec.id,
            incarnation_id: spec.backend_incarnation_id,
            action_id: decision.command_id.clone(),
            request_id: decision.request_id.clone(),
            request_generation: decision.request_generation,
            decision: decision.decision,
            decider_user_id: decision.decider_user_id.clone(),
            decider_device_id: Some(decision.decider_device_id.clone()),
            reply,
        })
        .await;
    let _accepted = matches!(
        tokio::time::timeout(std::time::Duration::from_secs(10), applied).await,
        Ok(Ok(true))
    );
    Ok(OwnerMessageOutcome::Continue)
}

async fn route_participant_changed(
    spec: &HostRelaySpec,
    stream: &mut HostWebSocketStream,
    message: &str,
    fence_dedupe: &mut HostFenceDedupe,
    event_sink: &dyn HostRelayEventSink,
) -> Result<OwnerMessageOutcome> {
    let changed: HostParticipantChangedMessage =
        match parse_owner_message("host.participantChanged", message) {
            Ok(changed) => changed,
            Err(outcome) => return Ok(outcome),
        };
    if let Err(outcome) = ensure_current_session(
        "host.participantChanged",
        changed.session_id.as_str(),
        spec.backend_session_id.as_str(),
    ) {
        return Ok(outcome);
    }
    let fence_id = changed.fence_id.clone();
    if !fence_dedupe.contains(&fence_id) {
        handle_participant_changed(spec, changed, event_sink).await?;
        fence_dedupe.remember(&fence_id);
    }
    send_fence_ack(stream, &spec.backend_session_id, &fence_id).await?;
    Ok(OwnerMessageOutcome::Continue)
}

async fn route_access_revoked(
    spec: &HostRelaySpec,
    stream: &mut HostWebSocketStream,
    message: &str,
    fence_dedupe: &mut HostFenceDedupe,
    event_sink: &dyn HostRelayEventSink,
) -> Result<OwnerMessageOutcome> {
    let revoked: HostAccessRevokedMessage = match parse_owner_message("host.accessRevoked", message)
    {
        Ok(revoked) => revoked,
        Err(outcome) => return Ok(outcome),
    };
    if let Err(outcome) = ensure_current_session(
        "host.accessRevoked",
        revoked.session_id.as_str(),
        spec.backend_session_id.as_str(),
    ) {
        return Ok(outcome);
    }
    if !fence_dedupe.contains(&revoked.fence_id) {
        tracing::info!(
            session_id = %revoked.session_id,
            revoked_user_id = %revoked.revoked_user_id,
            "participant access revoked on host relay",
        );
        emit_host_access_revoked(event_sink, spec.id, revoked.revoked_user_id).await;
        fence_dedupe.remember(&revoked.fence_id);
    }
    send_fence_ack(stream, &spec.backend_session_id, &revoked.fence_id).await?;
    Ok(OwnerMessageOutcome::Continue)
}

async fn route_key_distribution_requested(
    spec: &HostRelaySpec,
    stream: &mut HostWebSocketStream,
    message: &str,
    fence_dedupe: &mut HostFenceDedupe,
    event_sink: &dyn HostRelayEventSink,
) -> Result<OwnerMessageOutcome> {
    let request: HostKeyDistributionRequestedMessage =
        match parse_owner_message("host.keyDistributionRequested", message) {
            Ok(request) => request,
            Err(outcome) => return Ok(outcome),
        };
    if let Err(outcome) = ensure_current_session(
        "host.keyDistributionRequested",
        request.session_id.as_str(),
        spec.backend_session_id.as_str(),
    ) {
        return Ok(outcome);
    }
    if fence_dedupe.begin(&request.fence_id) {
        tracing::info!(
            session_id = %request.session_id,
            "host relay received key redistribution request",
        );
        emit_host_key_distribution_requested(event_sink, spec.id, request.fence_id.clone()).await;
        return Ok(OwnerMessageOutcome::Continue);
    }
    if fence_dedupe.is_pending(&request.fence_id) {
        return Ok(OwnerMessageOutcome::Continue);
    }
    send_fence_ack(stream, &spec.backend_session_id, &request.fence_id).await?;
    Ok(OwnerMessageOutcome::Continue)
}

async fn handle_stream_demand(
    spec: &HostRelaySpec,
    demand: HostStreamDemandMessage,
    live_stream: &mut LiveStreamState,
    pending_render: &mut bool,
    event_sink: &dyn HostRelayEventSink,
) -> Result<OwnerMessageOutcome> {
    let was_required = live_stream.required;
    let Some(participant_count) = demand.participant_count() else {
        return Ok(protocol_drift(
            "host.streamDemand participant counts overflowed usize".to_owned(),
        ));
    };

    live_stream.required = demand.required;
    emit_host_demand(
        event_sink,
        spec.id,
        demand.required,
        participant_count,
        demand.reason,
    )
    .await;

    if let Some(force) = pending_render_after_demand(was_required, demand.required) {
        *pending_render = force;
    }

    Ok(OwnerMessageOutcome::Continue)
}

async fn handle_input(
    spec: &HostRelaySpec,
    input: HostInputMessage,
    payload: &[u8],
    event_sink: &dyn HostRelayEventSink,
) -> Result<OwnerMessageOutcome> {
    match spec
        .port
        .dispatch_input_payload(payload, input.sender_user_id == spec.owner_user_id)
        .await
    {
        Ok(()) => {
            tracing::info!(
                session_id = %spec.backend_session_id,
                command_id = %input.command_id,
                outcome = "dispatched",
                "owner input dispatched",
            );
            Ok(OwnerMessageOutcome::Continue)
        }

        Err(error) => {
            tracing::info!(
                session_id = %spec.backend_session_id,
                command_id = %input.command_id,
                outcome = "rejected",
                error = %error,
                "owner input rejected",
            );
            emit_info(
                event_sink,
                spec.id,
                format!("remote inject failed: {error}"),
            )
            .await;
            Ok(OwnerMessageOutcome::Continue)
        }
    }
}

fn decrypt_control_bytes(
    spec: &HostRelaySpec,
    kind: &str,
    action_id: &str,
    nonce_b64: &str,
    ciphertext_b64: &str,
    sender: ControlSender<'_>,
    required_capability: u16,
) -> std::result::Result<Vec<u8>, String> {
    let ControlSender {
        user_id: sender_user_id,
        device_id: sender_device_id,
        signature_b64,
    } = sender;
    let session_key: &crate::crypto::SessionKey = &spec.session_key;
    let nonce = BASE64
        .decode(nonce_b64)
        .map_err(|error| format!("control nonce is not valid base64: {error}"))?;
    let ciphertext = BASE64
        .decode(ciphertext_b64)
        .map_err(|error| format!("control ciphertext is not valid base64: {error}"))?;
    let signature = BASE64
        .decode(signature_b64)
        .map_err(|error| format!("control signature is not valid base64: {error}"))?;
    let preimage = crate::crypto::control_message_preimage(
        kind,
        &spec.backend_session_id,
        action_id,
        sender_user_id,
        sender_device_id,
        &nonce,
        &ciphertext,
    )
    .map_err(|error| error.to_string())?;
    match spec.control_trust.authorize(
        sender_user_id,
        sender_device_id,
        &preimage,
        &signature,
        required_capability,
    ) {
        crate::control::ControlAuthorization::Authorized => {}
        crate::control::ControlAuthorization::Untrusted => {
            return Err(format!(
                "control signature from {sender_user_id}/{sender_device_id} is not trusted"
            ));
        }
        crate::control::ControlAuthorization::Forbidden { granted } => {
            tracing::warn!(
                session_id = %spec.backend_session_id,
                sender_user_id,
                sender_device_id,
                kind,
                required_capability,
                granted = granted.0,
                "backend relayed a control frame the owner never authorized",
            );
            return Err(format!(
                "control {kind} from {sender_user_id}/{sender_device_id} exceeds owner-granted capability"
            ));
        }
    }
    let aad = crate::crypto::control_associated_data(kind, &spec.backend_session_id, action_id)
        .map_err(|error| error.to_string())?;
    let plaintext = crate::crypto::decrypt_control_payload(session_key, &aad, &nonce, &ciphertext)
        .map_err(|error| error.to_string())?;
    match spec
        .control_trust
        .record_control_once(sender_user_id, sender_device_id, kind, action_id)
    {
        crate::control::ControlReplayDecision::Accepted => {}
        crate::control::ControlReplayDecision::Replay => {
            return Err(format!(
                "replayed control {kind}/{action_id} from {sender_user_id}/{sender_device_id}"
            ));
        }
        crate::control::ControlReplayDecision::Exhausted => {
            return Err(
                "control replay protection is exhausted; rotate the session key before accepting more remote controls"
                    .to_owned(),
            );
        }
    }
    Ok(plaintext)
}

fn decrypt_control_text(
    spec: &HostRelaySpec,
    kind: &str,
    action_id: &str,
    nonce_b64: &str,
    ciphertext_b64: &str,
    sender: ControlSender<'_>,
    required_capability: u16,
) -> std::result::Result<String, String> {
    let plaintext = decrypt_control_bytes(
        spec,
        kind,
        action_id,
        nonce_b64,
        ciphertext_b64,
        sender,
        required_capability,
    )?;
    String::from_utf8(plaintext)
        .map_err(|error| format!("control plaintext is not valid UTF-8: {error}"))
}

#[expect(
    clippy::too_many_arguments,
    reason = "the handler receives the already authenticated resize tuple plus the actor-owned render state"
)]
async fn handle_resize(
    spec: &HostRelaySpec,
    resize: &HostResizeMessage,
    size: TerminalSize,
    pixel_geometry: Option<TerminalPixelGeometry>,
    owner_origin: bool,
    claim: bool,
    live_stream: &LiveStreamState,
    pending_render: &mut bool,
    event_sink: &dyn HostRelayEventSink,
) -> Result<OwnerMessageOutcome> {
    match spec
        .port
        .resize(size, pixel_geometry, owner_origin, claim)
        .await
    {
        Ok(()) => {
            tracing::info!(
                session_id = %spec.backend_session_id,
                command_id = %resize.command_id,
                rows = size.rows(),
                cols = size.cols(),
                outcome = "dispatched",
                "owner resize dispatched",
            );

            if live_stream.required {
                *pending_render = true;
            }
            emit_resize_result(spec, resize, true, event_sink).await;
            Ok(OwnerMessageOutcome::Continue)
        }
        Err(error) => {
            tracing::info!(
                session_id = %spec.backend_session_id,
                command_id = %resize.command_id,
                rows = size.rows(),
                cols = size.cols(),
                outcome = "rejected",
                error = %error,
                "owner resize rejected",
            );
            emit_info(
                event_sink,
                spec.id,
                format!("remote resize failed: {error}"),
            )
            .await;
            emit_resize_result(spec, resize, false, event_sink).await;
            Ok(OwnerMessageOutcome::Continue)
        }
    }
}

async fn handle_focus_changed(
    spec: &HostRelaySpec,
    command_id: String,
    client_id: String,
    focus: ClientFocus,
    event_sink: &dyn HostRelayEventSink,
) -> Result<OwnerMessageOutcome> {
    let focus_label = match focus {
        ClientFocus::Focused => "focus",
        ClientFocus::Blurred => "blur",
    };
    match spec.port.set_focus(client_id.clone(), focus).await {
        Ok(()) => {
            tracing::trace!(
                session_id = %spec.backend_session_id,
                command_id = %command_id,
                client_id = %client_id,
                focus = focus_label,
                outcome = "dispatched",
                "owner focus change dispatched",
            );
            Ok(OwnerMessageOutcome::Continue)
        }
        Err(error) => {
            tracing::info!(
                session_id = %spec.backend_session_id,
                command_id = %command_id,
                client_id = %client_id,
                focus = focus_label,
                outcome = "rejected",
                error = %error,
                "owner focus change rejected",
            );
            emit_info(
                event_sink,
                spec.id,
                format!("remote {focus_label} failed: {error}"),
            )
            .await;
            Ok(OwnerMessageOutcome::Continue)
        }
    }
}

async fn handle_stop(spec: &HostRelaySpec, stop: HostStopMessage) -> Result<OwnerMessageOutcome> {
    tracing::info!(
        session_id = %stop.session_id,
        "host relay received remote stop request",
    );

    spec.port.stop().await?;
    Ok(OwnerMessageOutcome::Continue)
}

async fn handle_interrupt(
    spec: &HostRelaySpec,
    interrupt: HostInterruptMessage,
) -> Result<OwnerMessageOutcome> {
    tracing::info!(
        session_id = %interrupt.session_id,
        "host relay received remote interrupt request",
    );
    spec.port.interrupt().await?;
    Ok(OwnerMessageOutcome::Continue)
}

async fn handle_participant_changed(
    spec: &HostRelaySpec,
    changed: HostParticipantChangedMessage,
    event_sink: &dyn HostRelayEventSink,
) -> Result<OwnerMessageOutcome> {
    tracing::info!(
        session_id = %changed.session_id,
        participant_user_id = %changed.participant_user_id,
        participant_count = changed.participant_count,
        action = %changed.action,
        "participant changed on host relay",
    );
    emit_info(
        event_sink,
        spec.id,
        format!(
            "participant {} {}: {} participant(s) now",
            changed.participant_user_id, changed.action, changed.participant_count,
        ),
    )
    .await;
    Ok(OwnerMessageOutcome::Continue)
}

const fn pending_render_after_demand(was_required: bool, now_required: bool) -> Option<bool> {
    match (was_required, now_required) {
        (false, true) => Some(true),
        (true, false) => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use aws_lc_rs::signature::{KeyPair, ML_DSA_65_SIGNING, PqdsaKeyPair};
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    use kodosi_domain::{
        permissions::{AccessLevel, SessionCapabilities},
        terminal::{TerminalPixelGeometry, TerminalSize},
    };

    use super::{
        OwnerMessageOutcome, parse_owner_message, pending_render_after_demand, route_focus_changed,
        route_input, route_participant_disconnected, route_resize, route_stream_demand,
    };
    use crate::{
        control::ControlTrustStore,
        relay::{
            ClientFocus, HostRelayEvent, HostRelayEventFuture, HostRelayEventSink, HostRelayFuture,
            HostRelayPort, HostRelaySpec, HostRelayTerminalCheckpoint,
            HostRelayTerminalPresentation, transport::LiveStreamState,
            wire::HostStreamDemandMessage,
        },
    };

    #[derive(Debug, Default)]
    struct CountingPort {
        dispatched_inputs: AtomicUsize,
        input_packets: std::sync::Mutex<Vec<(Vec<u8>, bool)>>,
        focus_calls: std::sync::Mutex<Vec<(String, ClientFocus)>>,
        resize_calls: std::sync::Mutex<Vec<(TerminalSize, bool, bool)>>,
    }

    impl HostRelayPort for CountingPort {
        fn dispatch_input_payload<'a>(
            &'a self,
            payload: &'a [u8],
            owner_origin: bool,
        ) -> HostRelayFuture<'a, ()> {
            self.dispatched_inputs.fetch_add(1, Ordering::Relaxed);
            if let Ok(mut packets) = self.input_packets.lock() {
                packets.push((payload.to_vec(), owner_origin));
            }
            Box::pin(async { Ok(()) })
        }

        fn stop(&self) -> HostRelayFuture<'_, ()> {
            Box::pin(async { Ok(()) })
        }

        fn interrupt(&self) -> HostRelayFuture<'_, ()> {
            Box::pin(async { Ok(()) })
        }

        fn resize(
            &self,
            size: TerminalSize,
            _pixel_geometry: Option<TerminalPixelGeometry>,
            owner_origin: bool,
            claim: bool,
        ) -> HostRelayFuture<'_, ()> {
            if let Ok(mut calls) = self.resize_calls.lock() {
                calls.push((size, owner_origin, claim));
            }
            Box::pin(async { Ok(()) })
        }

        fn set_focus(&self, client_id: String, focus: ClientFocus) -> HostRelayFuture<'_, ()> {
            if let Ok(mut calls) = self.focus_calls.lock() {
                calls.push((client_id, focus));
            }
            Box::pin(async { Ok(()) })
        }

        fn capture_terminal_checkpoint(&self) -> HostRelayFuture<'_, HostRelayTerminalCheckpoint> {
            Box::pin(async {
                Ok(HostRelayTerminalCheckpoint {
                    checkpoint: kodosi_domain::terminal::TerminalCheckpointV2::new(
                        TerminalSize::new(80, 24).unwrap_or_else(|error| {
                            panic!("test terminal size should be valid: {error}")
                        }),
                        kodosi_domain::terminal::TerminalScreen::Primary,
                        b"checkpoint".to_vec(),
                        0,
                        0,
                        false,
                    )
                    .unwrap_or_else(|error| panic!("test checkpoint should be valid: {error}")),
                    next_sequence: 0,
                })
            })
        }

        fn capture_terminal_presentation(
            &self,
        ) -> HostRelayFuture<'_, HostRelayTerminalPresentation> {
            Box::pin(async {
                Ok(HostRelayTerminalPresentation {
                    presentation: kodosi_domain::terminal::TerminalPresentationV2::new(
                        TerminalSize::new(80, 24).unwrap_or_else(|error| {
                            panic!("test terminal size should be valid: {error}")
                        }),
                        kodosi_domain::terminal::TerminalScreen::Primary,
                        Vec::new(),
                        0,
                        0,
                        false,
                    )
                    .unwrap_or_else(|error| panic!("test presentation should be valid: {error}")),
                })
            })
        }
    }

    #[derive(Debug, Default)]
    struct RecordingEventSink {
        events: std::sync::Mutex<Vec<HostRelayEvent>>,
    }

    impl HostRelayEventSink for RecordingEventSink {
        fn emit(&self, event: HostRelayEvent) -> HostRelayEventFuture<'_> {
            Box::pin(async move {
                self.events
                    .lock()
                    .unwrap_or_else(|error| panic!("event sink lock poisoned: {error}"))
                    .push(event);
            })
        }
    }

    #[derive(Debug)]
    struct NoopEventSink;

    impl HostRelayEventSink for NoopEventSink {
        fn emit(&self, _event: HostRelayEvent) -> HostRelayEventFuture<'_> {
            Box::pin(async {})
        }
    }

    fn test_spec(port: Arc<CountingPort>) -> HostRelaySpec {
        HostRelaySpec {
            id: kodosi_domain::ids::SessionId::new(),
            backend_session_id: "backend-session".to_owned(),
            backend_incarnation_id: uuid::Uuid::from_u128(2),
            owner_secret: zeroize::Zeroizing::new("owner-secret".to_owned()),
            port,
            frame_revision_start: 1,
            frame_revision_end_exclusive: 100,
            frame_key_generation: 1,
            frame_nonce_start: 1,
            frame_nonce_end_exclusive: 100,
            session_key: zeroize::Zeroizing::new([7; 32]),
            owner_user_id: "owner".to_owned(),
            control_trust: ControlTrustStore::default(),
            host_device_id: "host-device".to_owned(),
            host_signing_pkcs8: zeroize::Zeroizing::new(Vec::new()),
        }
    }

    fn assert_zeroizes_on_drop<T: zeroize::ZeroizeOnDrop>(_: &T) {}

    #[test]
    fn relay_spec_debug_redacts_retained_secrets() {
        let spec = test_spec(Arc::new(CountingPort::default()));
        assert_zeroizes_on_drop(&spec.owner_secret);
        let debug = format!("{spec:?}");

        assert!(!debug.contains("owner-secret"));
        assert!(!debug.contains("[7, 7, 7"));
        assert!(debug.contains("owner_secret: \"<redacted>\""));
        assert!(debug.contains("session_key: \"<redacted>\""));
    }

    #[test]
    fn demand_rising_edge_forces_snapshot() {
        assert_eq!(pending_render_after_demand(false, true), Some(true));
    }

    #[test]
    fn demand_falling_edge_clears_pending() {
        assert_eq!(pending_render_after_demand(true, false), Some(false));
    }

    #[test]
    fn demand_unchanged_leaves_pending_untouched() {
        assert_eq!(pending_render_after_demand(true, true), None);
        assert_eq!(pending_render_after_demand(false, false), None);
    }

    fn signed_input_message(
        spec: &HostRelaySpec,
        keypair: &PqdsaKeyPair,
        command_id: &str,
        payload: &[u8],
        sender_user_id: &str,
    ) -> String {
        let session_key = *spec.session_key;
        let aad =
            crate::crypto::control_associated_data("inject", &spec.backend_session_id, command_id)
                .unwrap_or_else(|error| panic!("aad should build: {error}"));
        let sealed = crate::crypto::encrypt_control_payload(&session_key, &aad, payload)
            .unwrap_or_else(|error| panic!("control payload should seal: {error}"));
        let preimage = crate::crypto::control_message_preimage(
            "inject",
            &spec.backend_session_id,
            command_id,
            sender_user_id,
            "viewer-device",
            &sealed.nonce,
            &sealed.ciphertext,
        )
        .unwrap_or_else(|error| panic!("preimage should build: {error}"));
        let pkcs8 = keypair
            .to_pkcs8v1()
            .unwrap_or_else(|error| panic!("key should export: {error}"));
        let signature = crate::crypto::sign_control_message(pkcs8.as_ref(), &preimage)
            .unwrap_or_else(|error| panic!("signing should succeed: {error}"));
        format!(
            r#"{{"type":"host.input","sessionId":"{}","commandId":"{}","nonce":"{}","ciphertext":"{}","senderUserId":"{}","senderDeviceId":"viewer-device","signature":"{}"}}"#,
            spec.backend_session_id,
            command_id,
            BASE64.encode(sealed.nonce),
            BASE64.encode(&sealed.ciphertext),
            sender_user_id,
            BASE64.encode(&signature),
        )
    }

    fn signed_focus_message(
        spec: &HostRelaySpec,
        keypair: &PqdsaKeyPair,
        command_id: &str,
        focused: bool,
    ) -> String {
        let session_key = *spec.session_key;
        let kind = format!("focus:{focused}");
        let aad =
            crate::crypto::control_associated_data(&kind, &spec.backend_session_id, command_id)
                .unwrap_or_else(|error| panic!("aad should build: {error}"));
        let sealed = crate::crypto::encrypt_control_payload(&session_key, &aad, b"focus")
            .unwrap_or_else(|error| panic!("control payload should seal: {error}"));
        let preimage = crate::crypto::control_message_preimage(
            &kind,
            &spec.backend_session_id,
            command_id,
            "viewer",
            "viewer-device",
            &sealed.nonce,
            &sealed.ciphertext,
        )
        .unwrap_or_else(|error| panic!("preimage should build: {error}"));
        let pkcs8 = keypair
            .to_pkcs8v1()
            .unwrap_or_else(|error| panic!("key should export: {error}"));
        let signature = crate::crypto::sign_control_message(pkcs8.as_ref(), &preimage)
            .unwrap_or_else(|error| panic!("signing should succeed: {error}"));
        format!(
            r#"{{"type":"host.focusChanged","sessionId":"{}","commandId":"{}","clientId":"viewer-7","focused":{},"nonce":"{}","ciphertext":"{}","senderUserId":"viewer","senderDeviceId":"viewer-device","signature":"{}"}}"#,
            spec.backend_session_id,
            command_id,
            focused,
            BASE64.encode(sealed.nonce),
            BASE64.encode(&sealed.ciphertext),
            BASE64.encode(&signature),
        )
    }

    fn signed_resize_message(
        spec: &HostRelaySpec,
        keypair: &PqdsaKeyPair,
        command_id: &str,
        sender_user_id: &str,
        rows: u16,
        cols: u16,
        claim: bool,
    ) -> String {
        let session_key = *spec.session_key;
        let kind = format!("resize:{rows}:{cols}:{claim}:none");
        let aad =
            crate::crypto::control_associated_data(&kind, &spec.backend_session_id, command_id)
                .unwrap_or_else(|error| panic!("aad should build: {error}"));
        let sealed = crate::crypto::encrypt_control_payload(&session_key, &aad, b"resize")
            .unwrap_or_else(|error| panic!("control payload should seal: {error}"));
        let preimage = crate::crypto::control_message_preimage(
            &kind,
            &spec.backend_session_id,
            command_id,
            sender_user_id,
            "viewer-device",
            &sealed.nonce,
            &sealed.ciphertext,
        )
        .unwrap_or_else(|error| panic!("preimage should build: {error}"));
        let pkcs8 = keypair
            .to_pkcs8v1()
            .unwrap_or_else(|error| panic!("key should export: {error}"));
        let signature = crate::crypto::sign_control_message(pkcs8.as_ref(), &preimage)
            .unwrap_or_else(|error| panic!("signing should succeed: {error}"));
        format!(
            r#"{{"type":"host.resize","sessionId":"{}","commandId":"{}","rows":{},"cols":{},"claim":{},"nonce":"{}","ciphertext":"{}","senderUserId":"{}","senderDeviceId":"viewer-device","signature":"{}"}}"#,
            spec.backend_session_id,
            command_id,
            rows,
            cols,
            claim,
            BASE64.encode(sealed.nonce),
            BASE64.encode(&sealed.ciphertext),
            sender_user_id,
            BASE64.encode(&signature),
        )
    }

    #[tokio::test]
    async fn resize_rejects_a_valid_inject_participant_before_dispatch() {
        let port = Arc::new(CountingPort::default());
        let spec = test_spec(Arc::clone(&port));
        let keypair = trust_viewer_at(&spec, AccessLevel::Inject);
        let message =
            signed_resize_message(&spec, &keypair, "cmd-resize-1", "viewer", 24, 80, false);
        let live_stream = LiveStreamState::default();
        let mut pending_render = false;
        let events = RecordingEventSink::default();

        let outcome = route_resize(&spec, &message, &live_stream, &mut pending_render, &events)
            .await
            .unwrap_or_else(|error| panic!("participant resize must be contained: {error}"));

        std::assert_matches!(
            outcome,
            OwnerMessageOutcome::ParticipantActionRejected { ref reason }
                if reason.contains("owner-only")
        );
        assert!(port.resize_calls.lock().is_ok_and(|calls| calls.is_empty()));
        let recorded = events
            .events
            .lock()
            .unwrap_or_else(|error| panic!("event sink lock poisoned: {error}"));
        std::assert_matches!(
            recorded.as_slice(),
            [HostRelayEvent::ActionCompleted {
                action_id,
                request_id,
                requester_user_id,
                requester_device_id,
                accepted: false,
                ..
            }] if action_id == "cmd-resize-1"
                && request_id == "cmd-resize-1"
                && requester_user_id == "viewer"
                && requester_device_id == "viewer-device"
        );
    }

    #[tokio::test]
    async fn resize_accepts_an_authenticated_owner_claim() {
        let port = Arc::new(CountingPort::default());
        let spec = test_spec(Arc::clone(&port));
        let keypair = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING)
            .unwrap_or_else(|error| panic!("test key should generate: {error}"));
        spec.control_trust
            .replace(std::collections::HashMap::from([(
                ("owner".to_owned(), "viewer-device".to_owned()),
                crate::control::ControlTrustEntry::new(
                    keypair.public_key().as_ref().to_vec(),
                    SessionCapabilities::from_access(AccessLevel::Approve, true),
                ),
            )]));
        let message =
            signed_resize_message(&spec, &keypair, "cmd-resize-2", "owner", 30, 100, true);
        let live_stream = LiveStreamState::default();
        let mut pending_render = false;
        let events = RecordingEventSink::default();

        let outcome = route_resize(&spec, &message, &live_stream, &mut pending_render, &events)
            .await
            .unwrap_or_else(|error| panic!("owner resize should dispatch: {error}"));

        assert_eq!(outcome, OwnerMessageOutcome::Continue);
        let calls = port
            .resize_calls
            .lock()
            .unwrap_or_else(|error| panic!("resize calls lock poisoned: {error}"))
            .clone();
        assert_eq!(
            calls,
            [(TerminalSize::new(30, 100).expect("size"), true, true)]
        );
        let recorded = events
            .events
            .lock()
            .unwrap_or_else(|error| panic!("event sink lock poisoned: {error}"));
        std::assert_matches!(
            recorded.as_slice(),
            [HostRelayEvent::ActionCompleted {
                action_id,
                request_id,
                requester_user_id,
                requester_device_id,
                accepted: true,
                ..
            }] if action_id == "cmd-resize-2"
                && request_id == "cmd-resize-2"
                && requester_user_id == "owner"
                && requester_device_id == "viewer-device"
        );
    }

    #[tokio::test]
    async fn focus_requires_the_owner_granted_focus_capability() {
        let port = Arc::new(CountingPort::default());
        let spec = test_spec(Arc::clone(&port));

        let keypair = trust_viewer_at(&spec, AccessLevel::View);
        let message = signed_focus_message(&spec, &keypair, "cmd-focus-1", true);

        let outcome = route_focus_changed(&spec, &message, &NoopEventSink)
            .await
            .unwrap_or_else(|error| panic!("unauthorized focus must be contained: {error}"));

        std::assert_matches!(
            outcome,
            OwnerMessageOutcome::ParticipantActionRejected { ref reason }
                if reason.contains("exceeds owner-granted capability")
        );
        assert!(
            port.focus_calls.lock().is_ok_and(|calls| calls.is_empty()),
            "unauthorized focus must never reach the terminal"
        );
    }

    #[tokio::test]
    async fn authorized_participant_may_assert_focus() {
        let port = Arc::new(CountingPort::default());
        let spec = test_spec(Arc::clone(&port));
        let keypair = trust_viewer_at(&spec, AccessLevel::Inject);
        let message = signed_focus_message(&spec, &keypair, "cmd-focus-2", true);

        let outcome = route_focus_changed(&spec, &message, &NoopEventSink)
            .await
            .unwrap_or_else(|error| panic!("authorized focus should dispatch: {error}"));

        assert_eq!(outcome, OwnerMessageOutcome::Continue);
        let calls = port
            .focus_calls
            .lock()
            .unwrap_or_else(|error| panic!("focus calls lock poisoned: {error}"))
            .clone();
        assert_eq!(
            calls.as_slice(),
            [("viewer-7".to_owned(), ClientFocus::Focused)]
        );
    }

    #[tokio::test]
    async fn unauthenticated_focus_changed_is_rejected() {
        let port = Arc::new(CountingPort::default());
        let spec = test_spec(Arc::clone(&port));

        let outcome = route_focus_changed(
            &spec,
            r#"{"type":"host.focusChanged","sessionId":"backend-session","commandId":"cmd-1","clientId":"viewer-7","focused":false}"#,
            &NoopEventSink,
        )
        .await
        .unwrap_or_else(|error| panic!("unauthenticated focus must be contained: {error}"));

        std::assert_matches!(
            outcome,
            OwnerMessageOutcome::ParticipantActionRejected { .. }
        );
        assert!(
            port.focus_calls.lock().is_ok_and(|calls| calls.is_empty()),
            "unauthenticated focus must never reach the terminal"
        );
    }

    #[tokio::test]
    async fn participant_disconnected_releases_focus_without_authentication() {
        let port = Arc::new(CountingPort::default());
        let spec = test_spec(Arc::clone(&port));

        let outcome = route_participant_disconnected(
            &spec,
            r#"{"type":"host.participantDisconnected","sessionId":"backend-session","clientId":"viewer-7"}"#,
            &NoopEventSink,
        )
        .await
        .unwrap_or_else(|error| panic!("disconnect cleanup must succeed: {error}"));

        assert_eq!(outcome, OwnerMessageOutcome::Continue);
        let calls = port
            .focus_calls
            .lock()
            .unwrap_or_else(|error| panic!("focus calls lock poisoned: {error}"))
            .clone();
        assert_eq!(
            calls.as_slice(),
            [("viewer-7".to_owned(), ClientFocus::Blurred)]
        );
    }

    #[tokio::test]
    async fn participant_disconnected_cannot_assert_focus() {
        let port = Arc::new(CountingPort::default());
        let spec = test_spec(Arc::clone(&port));

        let outcome = route_participant_disconnected(
            &spec,
            r#"{"type":"host.participantDisconnected","sessionId":"backend-session","clientId":"viewer-7","focused":true}"#,
            &NoopEventSink,
        )
        .await
        .unwrap_or_else(|error| panic!("disconnect cleanup must succeed: {error}"));

        assert_eq!(outcome, OwnerMessageOutcome::Continue);
        let calls = port
            .focus_calls
            .lock()
            .unwrap_or_else(|error| panic!("focus calls lock poisoned: {error}"))
            .clone();
        assert_eq!(
            calls.as_slice(),
            [("viewer-7".to_owned(), ClientFocus::Blurred)]
        );
    }

    #[tokio::test]
    async fn participant_disconnected_for_another_session_is_protocol_drift() {
        let port = Arc::new(CountingPort::default());
        let spec = test_spec(Arc::clone(&port));

        let outcome = route_participant_disconnected(
            &spec,
            r#"{"type":"host.participantDisconnected","sessionId":"other-session","clientId":"viewer-7"}"#,
            &NoopEventSink,
        )
        .await
        .unwrap_or_else(|error| panic!("cross-session notice must be contained: {error}"));

        std::assert_matches!(outcome, OwnerMessageOutcome::ProtocolDrift { .. });
    }

    fn trust_sender_at(
        spec: &HostRelaySpec,
        sender_user_id: &str,
        access: AccessLevel,
    ) -> PqdsaKeyPair {
        let keypair = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING)
            .unwrap_or_else(|error| panic!("test key should generate: {error}"));
        spec.control_trust
            .replace(std::collections::HashMap::from([(
                (sender_user_id.to_owned(), "viewer-device".to_owned()),
                crate::control::ControlTrustEntry::new(
                    keypair.public_key().as_ref().to_vec(),
                    SessionCapabilities::from_access(access, false),
                ),
            )]));
        keypair
    }

    fn trust_viewer_at(spec: &HostRelaySpec, access: AccessLevel) -> PqdsaKeyPair {
        trust_sender_at(spec, "viewer", access)
    }

    #[tokio::test]
    async fn view_only_participant_cannot_inject_even_with_a_valid_signature() {
        let port = Arc::new(CountingPort::default());
        let spec = test_spec(Arc::clone(&port));
        let keypair = trust_viewer_at(&spec, AccessLevel::View);
        let message = signed_input_message(&spec, &keypair, "cmd-1", b"denied input", "viewer");

        let outcome = route_input(&spec, &message, &NoopEventSink)
            .await
            .unwrap_or_else(|error| panic!("unauthorized input must be contained: {error}"));

        std::assert_matches!(
            outcome,
            OwnerMessageOutcome::ParticipantActionRejected { ref reason }
                if reason.contains("exceeds owner-granted capability"),
        );
        assert_eq!(
            port.dispatched_inputs.load(Ordering::Relaxed),
            0,
            "a view-only participant must never reach the terminal",
        );
    }

    #[tokio::test]
    async fn participant_and_remote_owner_inputs_preserve_every_byte_and_origin() {
        for (sender_user_id, owner_origin) in [("viewer", false), ("owner", true)] {
            let port = Arc::new(CountingPort::default());
            let spec = test_spec(Arc::clone(&port));
            let keypair = trust_sender_at(&spec, sender_user_id, AccessLevel::Inject);
            let payload = (0_u8..=u8::MAX).collect::<Vec<_>>();
            let message = signed_input_message(
                &spec,
                &keypair,
                &format!("cmd-{sender_user_id}"),
                &payload,
                sender_user_id,
            );

            let outcome = route_input(&spec, &message, &NoopEventSink)
                .await
                .unwrap_or_else(|error| panic!("authorized input should dispatch: {error}"));

            assert_eq!(outcome, OwnerMessageOutcome::Continue);
            assert_eq!(port.dispatched_inputs.load(Ordering::Relaxed), 1);
            let packet = port
                .input_packets
                .lock()
                .unwrap_or_else(|error| panic!("input packets lock poisoned: {error}"))
                .first()
                .cloned();
            assert_eq!(packet, Some((payload, owner_origin)));
        }
    }

    #[tokio::test]
    async fn repeated_malformed_and_forged_participant_actions_keep_relay_usable() {
        let port = Arc::new(CountingPort::default());
        let spec = test_spec(Arc::clone(&port));
        let sink = NoopEventSink;

        for index in 0..128 {
            let message = if index % 2 == 0 {
                format!(
                    r#"{{"type":"host.input","sessionId":"backend-session","commandId":"bad-{index}","nonce":"AA==","senderUserId":"attacker","senderDeviceId":"device","signature":"AA=="}}"#
                )
            } else {
                format!(
                    r#"{{"type":"host.input","sessionId":"backend-session","commandId":"forged-{index}","nonce":"AA==","ciphertext":"AA==","senderUserId":"attacker","senderDeviceId":"device","signature":"AA=="}}"#
                )
            };
            let outcome = route_input(&spec, &message, &sink)
                .await
                .unwrap_or_else(|error| {
                    panic!("bad participant action must be contained: {error}")
                });
            std::assert_matches!(
                outcome,
                OwnerMessageOutcome::ParticipantActionRejected { .. },
                "bad participant action must not become backend protocol drift"
            );
        }

        assert_eq!(port.dispatched_inputs.load(Ordering::Relaxed), 0);

        let mut live_stream = LiveStreamState::default();
        let mut pending_render = false;
        let outcome = route_stream_demand(
            &spec,
            r#"{"type":"host.streamDemand","sessionId":"backend-session","required":true,"sharedParticipantCount":1,"ownerParticipantCount":0,"reason":"participant joined"}"#,
            &mut live_stream,
            &mut pending_render,
            &sink,
        )
        .await
        .unwrap_or_else(|error| panic!("backend frame after bad actions should work: {error}"));
        assert_eq!(outcome, OwnerMessageOutcome::Continue);
        assert!(live_stream.required);
        assert!(pending_render);
    }

    #[test]
    fn malformed_backend_control_remains_protocol_drift() {
        let outcome = parse_owner_message::<HostStreamDemandMessage>(
            "host.streamDemand",
            r#"{"type":"host.streamDemand","sessionId":"backend-session"}"#,
        );

        std::assert_matches!(outcome, Err(OwnerMessageOutcome::ProtocolDrift { .. }));
    }

    #[test]
    fn host_fence_is_deduplicated_only_after_successful_processing() {
        let mut dedupe = super::HostFenceDedupe::default();

        assert!(dedupe.begin("fence-1"));
        assert!(!dedupe.contains("fence-1"));
        assert!(dedupe.is_pending("fence-1"));
        assert!(!dedupe.begin("fence-1"));

        dedupe.complete("fence-1");
        assert!(dedupe.contains("fence-1"));
        assert!(!dedupe.is_pending("fence-1"));
        assert!(!dedupe.begin("fence-1"));
        assert_eq!(dedupe.order.len(), 1);
    }
}
