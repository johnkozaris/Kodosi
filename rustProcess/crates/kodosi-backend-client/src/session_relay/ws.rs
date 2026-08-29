use futures_util::SinkExt;
use reqwest::Url;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async_with_config, tungstenite::Message,
};

use crate::{
    BackendClientError, Result,
    endpoint::{
        build_ws_bearer_request, join_endpoint, parse_url, session_relay_ws_config, set_tcp_nodelay,
    },
    session_relay_authority::{relay_protocol_uses_exact_match, relay_protocol_version},
};

use super::cursor::ReplayCursor;

fn participant_relay_protocol_version() -> u32 {
    assert!(
        relay_protocol_uses_exact_match(),
        "embedded session relay authority must require exact matching"
    );
    relay_protocol_version()
}

pub type SessionRelayWebSocketStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Debug, Clone)]
pub struct SessionRelayWsClient {
    base_url: Option<Url>,
}

impl SessionRelayWsClient {
    pub fn new(base_url: Option<&str>) -> Result<Self> {
        Ok(Self {
            base_url: base_url.map(parse_url).transpose()?,
        })
    }

    pub async fn connect_participant(
        &self,
        session_id: &str,
        access_token: &str,
        cursor: ReplayCursor,
        device_id: &str,
        user_id: &str,
        signing_pkcs8: &[u8],
        expected_incarnation_id: &uuid::Uuid,
    ) -> Result<SessionRelayWebSocketStream> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!("participants/{session_id}"),
            "backend.viewer_relay",
        )?;
        let request = build_ws_bearer_request(&url, access_token)?;

        let (mut stream, _) =
            connect_async_with_config(request, Some(session_relay_ws_config()), false)
                .await
                .map_err(BackendClientError::WebSocket)?;
        set_tcp_nodelay(&stream)?;
        crate::host_ws::prove_device_connection(
            &mut stream,
            user_id,
            device_id,
            signing_pkcs8,
            "participant",
            Some(session_id),
            Some(expected_incarnation_id),
        )
        .await?;
        let join_message = participant_join_message(cursor, device_id, expected_incarnation_id);
        send_session_relay_message(&mut stream, join_message).await?;
        Ok(stream)
    }
}

pub async fn send_session_relay_message(
    stream: &mut SessionRelayWebSocketStream,
    message: serde_json::Value,
) -> Result<()> {
    stream
        .send(Message::Text(message.to_string().into()))
        .await
        .map_err(BackendClientError::WebSocket)
}

fn participant_join_message(
    cursor: ReplayCursor,
    device_id: &str,
    expected_incarnation_id: &uuid::Uuid,
) -> serde_json::Value {
    serde_json::json!({
        "type": "participant.join",
        "checkpointRevision": cursor.checkpoint_revision,
        "presentationRevision": cursor.presentation_revision,
        "nextSequence": cursor.next_sequence.unwrap_or(0),
        "lastSeenKeyGeneration": cursor.key_generation,
        "deviceId": device_id,
        "expectedIncarnationId": expected_incarnation_id,
        "relayProtocolVersion": participant_relay_protocol_version(),
    })
}

pub fn participant_heartbeat_message() -> serde_json::Value {
    serde_json::json!({
        "type": "participant.heartbeat",
    })
}

pub fn participant_semantic_send_message(
    request_id: &uuid::Uuid,
    incarnation_id: &uuid::Uuid,
    mode: super::wire::RelaySemanticMode,
    payload_sha256: &str,
    nonce: &str,
    ciphertext: &str,
    signature: &str,
) -> serde_json::Value {
    serde_json::json!({
        "type": "participant.semanticSend",
        "requestId": request_id,
        "incarnationId": incarnation_id,
        "mode": mode,
        "payloadSha256": payload_sha256,
        "nonce": nonce,
        "ciphertext": ciphertext,
        "signature": signature,
    })
}

pub fn participant_semantic_cancel_message(
    request_id: &uuid::Uuid,
    incarnation_id: &uuid::Uuid,
    mode: super::wire::RelaySemanticMode,
    payload_sha256: &str,
    signature: &str,
) -> serde_json::Value {
    serde_json::json!({
        "type": "participant.semanticCancel",
        "requestId": request_id,
        "incarnationId": incarnation_id,
        "mode": mode,
        "payloadSha256": payload_sha256,
        "signature": signature,
    })
}

pub fn participant_semantic_receipt_ack_message(
    session_id: &str,
    incarnation_id: &uuid::Uuid,
    request_id: &uuid::Uuid,
    requester_user_id: &str,
    requester_device_id: &str,
    signature: &str,
) -> serde_json::Value {
    serde_json::json!({
        "type": "participant.semanticReceiptAck",
        "sessionId": session_id,
        "incarnationId": incarnation_id,
        "requestId": request_id,
        "requesterUserId": requester_user_id,
        "requesterDeviceId": requester_device_id,
        "signature": signature,
    })
}

pub fn participant_suggest_message(
    action_id: &str,
    nonce: &str,
    ciphertext: &str,
    signature: &str,
) -> serde_json::Value {
    serde_json::json!({
        "type": "participant.suggest",
        "actionId": action_id,
        "nonce": nonce,
        "ciphertext": ciphertext,
        "signature": signature,
    })
}

pub fn participant_inject_message(
    action_id: &str,
    nonce: &str,
    ciphertext: &str,
    signature: &str,
) -> serde_json::Value {
    serde_json::json!({
        "type": "participant.inject",
        "actionId": action_id,
        "nonce": nonce,
        "ciphertext": ciphertext,
        "signature": signature,
    })
}

pub fn participant_permission_decision_message(
    action_id: &str,
    request_id: &str,
    request_generation: u64,
    decision: &str,
    nonce: &str,
    ciphertext: &str,
    signature: &str,
) -> serde_json::Value {
    let mut message = serde_json::json!({
        "type": "participant.permissionDecision",
        "actionId": action_id,
        "requestId": request_id,
        "requestGeneration": request_generation,
        "decision": decision,
    });
    message["nonce"] = serde_json::Value::String(nonce.to_owned());
    message["ciphertext"] = serde_json::Value::String(ciphertext.to_owned());
    message["signature"] = serde_json::Value::String(signature.to_owned());
    message
}

pub fn participant_resize_message(
    action_id: &str,
    rows: u16,
    cols: u16,
    pixel_geometry: Option<kodosi_domain::terminal::TerminalPixelGeometry>,
    claim: bool,
    nonce: &str,
    ciphertext: &str,
    signature: &str,
) -> serde_json::Value {
    let mut message = serde_json::json!({
        "type": "participant.resize",
        "actionId": action_id,
        "rows": rows,
        "cols": cols,
        "claim": claim,
        "nonce": nonce,
        "ciphertext": ciphertext,
        "signature": signature,
    });
    if let Some(geometry) = pixel_geometry {
        message["widthPixels"] = geometry.width_pixels().into();
        message["heightPixels"] = geometry.height_pixels().into();
        message["cellWidthPixels"] = geometry.cell_width_pixels().into();
        message["cellHeightPixels"] = geometry.cell_height_pixels().into();
    }
    message
}

pub fn participant_focus_changed_message(
    action_id: &str,
    focused: bool,
    nonce: &str,
    ciphertext: &str,
    signature: &str,
) -> serde_json::Value {
    serde_json::json!({
        "type": "participant.focusChanged",
        "actionId": action_id,
        "focused": focused,
        "nonce": nonce,
        "ciphertext": ciphertext,
        "signature": signature,
    })
}

pub fn participant_stop_message(
    action_id: &str,
    nonce: &str,
    ciphertext: &str,
    signature: &str,
) -> serde_json::Value {
    serde_json::json!({
        "type": "participant.stop",
        "actionId": action_id,
        "nonce": nonce,
        "ciphertext": ciphertext,
        "signature": signature,
    })
}

pub fn participant_interrupt_message(
    action_id: &str,
    nonce: &str,
    ciphertext: &str,
    signature: &str,
) -> serde_json::Value {
    serde_json::json!({
        "type": "participant.interrupt",
        "actionId": action_id,
        "nonce": nonce,
        "ciphertext": ciphertext,
        "signature": signature,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        participant_focus_changed_message, participant_heartbeat_message,
        participant_inject_message, participant_interrupt_message, participant_join_message,
        participant_resize_message, participant_stop_message, participant_suggest_message,
    };
    use crate::{
        session_relay::cursor::ReplayCursor, session_relay_authority::load_session_relay_authority,
    };

    #[test]
    fn participant_join_message_matches_authority_manifest() {
        let authority = load_session_relay_authority();

        let payload = participant_join_message(
            ReplayCursor {
                checkpoint_revision: 7,
                presentation_revision: 11,
                next_sequence: Some(13),
                key_generation: 3,
                ..ReplayCursor::default()
            },
            "device-1",
            &uuid::Uuid::from_u128(2),
        );
        let message_type = payload
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("participant join payload should include type"));
        let field_names = payload
            .as_object()
            .unwrap_or_else(|| panic!("participant join payload should serialize as object"))
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let mut actual_fields = field_names;
        actual_fields.sort();
        let join_authority = authority
            .messages
            .get(message_type)
            .unwrap_or_else(|| panic!("participant.join should be in the authority manifest"));
        let mut expected_fields = join_authority.required_fields.clone();
        expected_fields.extend(join_authority.optional_fields.clone());
        expected_fields.sort();

        assert_eq!(join_authority.direction, "participantToBackend");
        assert_eq!(actual_fields, expected_fields);
    }

    #[test]
    fn participant_control_messages_match_authority_vocabulary() {
        let authority = load_session_relay_authority();

        for payload in [
            participant_heartbeat_message(),
            participant_suggest_message("action-1", "nonce", "ciphertext", "signature"),
            participant_inject_message("action-2", "nonce", "ciphertext", "signature"),
            participant_resize_message(
                "action-3",
                24,
                80,
                None,
                false,
                "nonce",
                "ciphertext",
                "signature",
            ),
            participant_focus_changed_message("action-4", true, "nonce", "ciphertext", "signature"),
            participant_stop_message("action-5", "nonce", "ciphertext", "signature"),
            participant_interrupt_message("action-6", "nonce", "ciphertext", "signature"),
        ] {
            let message_type = payload
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_else(|| panic!("participant control payload should include type"));
            let spec = authority
                .messages
                .get(message_type)
                .unwrap_or_else(|| panic!("{message_type} should be in the authority manifest"));
            assert_eq!(spec.direction, "participantToBackend");

            let mut actual_fields = payload
                .as_object()
                .unwrap_or_else(|| panic!("{message_type} payload should serialize as object"))
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            actual_fields.sort();
            let mut expected_fields = spec.required_fields.clone();
            expected_fields.sort();
            assert_eq!(
                actual_fields, expected_fields,
                "{message_type} fields drift from authority manifest"
            );
        }
    }
}
