use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use kodosi_domain::terminal::{MAX_TERMINAL_COLS, MAX_TERMINAL_ROWS};
use serde::{Deserialize, Serialize};
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

use crate::{
    BackendClientError, Result,
    host_ws::{HostWebSocketStream, HostWsClient},
    relay_wire::KeyRotationMessage,
    session_relay_authority::{relay_protocol_uses_exact_match, relay_protocol_version},
};

use super::HostRelaySpec;

const HOST_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
const HOST_PRIME_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

pub(super) const HOST_RESIZE_MAX_ROWS: u16 = MAX_TERMINAL_ROWS;

pub(super) const HOST_RESIZE_MAX_COLS: u16 = MAX_TERMINAL_COLS;

#[derive(Debug, Deserialize)]
pub(super) struct HostEnvelope {
    #[serde(rename = "type")]
    pub message_type: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HostFenceAckMessage<'a> {
    #[serde(rename = "type")]
    message_type: &'static str,
    session_id: &'a str,
    fence_id: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HostHelloMessage<'a> {
    #[serde(rename = "type")]
    message_type: &'static str,
    session_id: &'a str,
    session_secret: &'a str,
    device_id: &'a str,
    expected_incarnation_id: &'a uuid::Uuid,
    relay_protocol_version: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HostHeartbeatMessage<'a> {
    #[serde(rename = "type")]
    message_type: &'static str,
    session_id: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HostEndMessage<'a> {
    #[serde(rename = "type")]
    message_type: &'static str,
    session_id: &'a str,
    reason: &'a str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HostAcceptedMessage {
    pub session_id: String,
    pub relay_epoch: String,
    pub incarnation_id: uuid::Uuid,
    pub incarnation_generation: u64,
    pub relay_protocol_version: u32,
}

pub(super) struct PrimedHostConnection {
    pub stream: HostWebSocketStream,
    pub accepted: HostAcceptedMessage,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HostActionResultMessage<'a> {
    #[serde(rename = "type")]
    message_type: &'static str,
    session_id: &'a str,
    incarnation_id: &'a uuid::Uuid,
    action_id: &'a str,
    request_id: &'a str,
    request_generation: u64,
    requester_user_id: &'a str,
    requester_device_id: &'a str,
    status: &'a str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HostStreamDemandMessage {
    pub session_id: String,
    pub required: bool,
    pub shared_participant_count: usize,
    pub owner_participant_count: usize,
    pub reason: String,
}

impl HostStreamDemandMessage {
    pub(super) const fn participant_count(&self) -> Option<usize> {
        self.shared_participant_count
            .checked_add(self.owner_participant_count)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HostSemanticReceiptMessage<'a> {
    #[serde(rename = "type")]
    message_type: &'static str,
    session_id: &'a str,
    incarnation_id: &'a uuid::Uuid,
    request_id: &'a uuid::Uuid,
    mode: crate::session_relay::wire::RelaySemanticMode,
    payload_sha256: &'a str,
    outcome: crate::session_relay::wire::RelaySemanticOutcome,
    requester_user_id: &'a str,
    requester_device_id: &'a str,
    owner_user_id: &'a str,
    owner_device_id: &'a str,
    signature: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HostSemanticSendMessage {
    pub session_id: String,
    pub request_id: uuid::Uuid,
    pub incarnation_id: uuid::Uuid,
    pub mode: crate::session_relay::wire::RelaySemanticMode,
    pub payload_sha256: String,
    pub nonce: String,
    pub ciphertext: String,
    pub sender_user_id: String,
    pub sender_device_id: String,
    pub signature: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HostSemanticCancelMessage {
    pub session_id: String,
    pub request_id: uuid::Uuid,
    pub incarnation_id: uuid::Uuid,
    pub mode: crate::session_relay::wire::RelaySemanticMode,
    pub payload_sha256: String,
    pub requester_user_id: String,
    pub requester_device_id: String,
    pub signature: String,
}

#[expect(
    clippy::struct_field_names,
    reason = "field names mirror the exact host.actionResultAck wire contract"
)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HostActionResultAckMessage {
    pub session_id: String,
    pub incarnation_id: uuid::Uuid,
    pub action_id: String,
    pub requester_user_id: String,
}

#[expect(
    clippy::struct_field_names,
    reason = "field names mirror the exact host.semanticReceiptAck wire contract"
)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HostSemanticReceiptAckMessage {
    pub session_id: String,
    pub incarnation_id: uuid::Uuid,
    pub request_id: uuid::Uuid,
    pub requester_user_id: String,
    pub requester_device_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HostInputMessage {
    pub session_id: String,
    #[serde(rename = "commandId")]
    pub command_id: String,
    pub nonce: String,
    pub ciphertext: String,
    pub sender_user_id: String,
    pub sender_device_id: String,
    pub signature: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HostPermissionDecisionMessage {
    pub session_id: String,
    #[serde(rename = "commandId")]
    pub command_id: String,
    #[serde(rename = "requestId")]
    pub request_id: String,
    #[serde(rename = "requestGeneration")]
    pub request_generation: u64,
    pub decision: String,
    #[serde(rename = "deciderUserId")]
    pub decider_user_id: String,
    #[serde(rename = "deciderDeviceId")]
    pub decider_device_id: String,
    pub nonce: String,
    pub ciphertext: String,
    pub signature: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HostResizeMessage {
    pub session_id: String,
    #[serde(rename = "commandId")]
    pub command_id: String,
    pub rows: u16,
    pub cols: u16,
    pub width_pixels: Option<u32>,
    pub height_pixels: Option<u32>,
    pub cell_width_pixels: Option<u32>,
    pub cell_height_pixels: Option<u32>,

    #[serde(default)]
    pub claim: bool,
    pub nonce: String,
    pub ciphertext: String,
    pub sender_user_id: String,
    pub sender_device_id: String,
    pub signature: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HostFocusChangedMessage {
    pub session_id: String,
    #[serde(rename = "commandId")]
    pub command_id: String,
    pub client_id: String,
    pub focused: bool,
    pub nonce: String,
    pub ciphertext: String,
    pub sender_user_id: String,
    pub sender_device_id: String,
    pub signature: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HostParticipantDisconnectedMessage {
    pub session_id: String,
    pub client_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HostStopMessage {
    pub session_id: String,
    pub command_id: String,
    pub nonce: String,
    pub ciphertext: String,
    pub sender_user_id: String,
    pub sender_device_id: String,
    pub signature: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HostInterruptMessage {
    pub session_id: String,
    pub command_id: String,
    pub nonce: String,
    pub ciphertext: String,
    pub sender_user_id: String,
    pub sender_device_id: String,
    pub signature: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HostSuggestionMessage {
    pub session_id: String,
    pub suggestion_id: String,
    pub nonce: String,
    pub ciphertext: String,
    pub sender_user_id: String,
    pub sender_device_id: String,
    pub signature: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HostParticipantChangedMessage {
    pub session_id: String,
    pub fence_id: String,
    pub participant_user_id: String,
    pub participant_count: usize,
    pub action: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::struct_field_names,
    reason = "field names intentionally mirror the host wire contract"
)]
pub(super) struct HostAccessRevokedMessage {
    pub session_id: String,
    pub fence_id: String,
    pub revoked_user_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HostKeyDistributionRequestedMessage {
    pub session_id: String,
    pub fence_id: String,
}

pub(super) async fn connect_and_prime(
    spec: &HostRelaySpec,
    host_ws: &HostWsClient,
    access_token: &str,
    cancellation: &CancellationToken,
) -> Result<PrimedHostConnection> {
    let mut stream = Box::pin(super::await_relay_operation(
        "host relay websocket connect",
        HOST_CONNECT_TIMEOUT,
        cancellation,
        host_ws.connect(
            &spec.backend_session_id,
            access_token,
            &spec.owner_user_id,
            &spec.host_device_id,
            spec.host_signing_pkcs8.as_ref(),
            &spec.backend_incarnation_id,
        ),
    ))
    .await??;

    let accepted = super::await_relay_operation(
        "host relay acceptance",
        HOST_PRIME_TIMEOUT,
        cancellation,
        async {
            send_json(
                &mut stream,
                &HostHelloMessage {
                    message_type: "host.hello",
                    session_id: &spec.backend_session_id,
                    session_secret: spec.owner_secret.as_str(),
                    device_id: &spec.host_device_id,
                    expected_incarnation_id: &spec.backend_incarnation_id,
                    relay_protocol_version: relay_protocol_version(),
                },
            )
            .await?;
            wait_for_host_accepted(
                &mut stream,
                &spec.backend_session_id,
                &spec.backend_incarnation_id,
            )
            .await
        },
    )
    .await??;
    send_json(
        &mut stream,
        &KeyRotationMessage::borrowed(&spec.backend_session_id, spec.frame_key_generation),
    )
    .await?;
    Ok(PrimedHostConnection { stream, accepted })
}

pub(super) async fn send_heartbeat(
    stream: &mut HostWebSocketStream,
    backend_session_id: &str,
) -> Result<()> {
    send_json(
        stream,
        &HostHeartbeatMessage {
            message_type: "host.heartbeat",
            session_id: backend_session_id,
        },
    )
    .await
}

pub(super) async fn send_host_end(
    stream: &mut HostWebSocketStream,
    backend_session_id: &str,
) -> Result<()> {
    send_json(
        stream,
        &HostEndMessage {
            message_type: "host.end",
            session_id: backend_session_id,
            reason: "host_stopped",
        },
    )
    .await?;
    stream
        .send(Message::Close(None))
        .await
        .map_err(BackendClientError::WebSocket)
}

pub(super) async fn send_host_action_result(
    stream: &mut HostWebSocketStream,
    backend_session_id: &str,
    result: &super::HostRelayActionResult,
) -> Result<()> {
    send_json(
        stream,
        &HostActionResultMessage {
            message_type: "host.actionResult",
            session_id: backend_session_id,
            incarnation_id: &result.incarnation_id,
            action_id: &result.action_id,
            request_id: &result.request_id,
            request_generation: result.request_generation,
            requester_user_id: &result.requester_user_id,
            requester_device_id: &result.requester_device_id,
            status: if result.accepted {
                "accepted"
            } else {
                "rejected"
            },
        },
    )
    .await
}

pub(super) fn build_semantic_receipt_dto(
    backend_session_id: &str,
    receipt_signing_pkcs8: &[u8],
    receipt: &super::HostRelaySemanticReceipt,
) -> Result<crate::dto::semantic_receipts::SemanticReceiptDto> {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    let session_id =
        uuid::Uuid::parse_str(backend_session_id).map_err(|_| BackendClientError::Protocol {
            reason: "host semantic receipt session ID is invalid".to_owned(),
        })?;
    let requester_user_id = uuid::Uuid::parse_str(&receipt.requester_user_id).map_err(|_| {
        BackendClientError::Protocol {
            reason: "host semantic receipt requester user ID is invalid".to_owned(),
        }
    })?;
    let owner_user_id = uuid::Uuid::parse_str(&receipt.owner_user_id).map_err(|_| {
        BackendClientError::Protocol {
            reason: "host semantic receipt owner user ID is invalid".to_owned(),
        }
    })?;
    let preimage = crate::crypto::semantic_receipt_preimage(
        &session_id,
        &receipt.incarnation_id,
        &receipt.request_id,
        receipt.mode.as_str(),
        &receipt.payload_sha256,
        receipt.outcome.as_str(),
        &requester_user_id,
        &receipt.requester_device_id,
        &owner_user_id,
        &receipt.owner_device_id,
    )?;
    let signature = if let Some(signature) = receipt.signature.clone() {
        signature
    } else {
        BASE64.encode(crate::crypto::sign_control_message(
            receipt_signing_pkcs8,
            &preimage,
        )?)
    };
    Ok(crate::dto::semantic_receipts::SemanticReceiptDto {
        session_id,
        incarnation_id: receipt.incarnation_id,
        request_id: receipt.request_id,
        mode: receipt.mode.as_str().to_owned(),
        payload_sha256: receipt.payload_sha256.clone(),
        outcome: receipt.outcome.as_str().to_owned(),
        requester_user_id,
        requester_device_id: receipt.requester_device_id.clone(),
        owner_user_id,
        owner_device_id: receipt.owner_device_id.clone(),
        signature,
    })
}

pub(super) async fn send_semantic_receipt(
    stream: &mut HostWebSocketStream,
    backend_session_id: &str,
    receipt_signing_pkcs8: &[u8],
    receipt: &super::HostRelaySemanticReceipt,
) -> Result<()> {
    let envelope = build_semantic_receipt_dto(backend_session_id, receipt_signing_pkcs8, receipt)?;
    send_json(
        stream,
        &HostSemanticReceiptMessage {
            message_type: "host.semanticReceipt",
            session_id: backend_session_id,
            incarnation_id: &envelope.incarnation_id,
            request_id: &envelope.request_id,
            mode: receipt.mode,
            payload_sha256: &envelope.payload_sha256,
            outcome: receipt.outcome,
            requester_user_id: &receipt.requester_user_id,
            requester_device_id: &envelope.requester_device_id,
            owner_user_id: &receipt.owner_user_id,
            owner_device_id: &envelope.owner_device_id,
            signature: envelope.signature,
        },
    )
    .await
}

pub(super) async fn send_fence_ack(
    stream: &mut HostWebSocketStream,
    backend_session_id: &str,
    fence_id: &str,
) -> Result<()> {
    send_json(
        stream,
        &HostFenceAckMessage {
            message_type: "host.fenceAck",
            session_id: backend_session_id,
            fence_id,
        },
    )
    .await
}

pub(super) async fn send_json<T>(stream: &mut HostWebSocketStream, message: &T) -> Result<()>
where
    T: Serialize + Sync,
{
    let payload = serde_json::to_string(message)?;
    stream
        .send(Message::Text(payload.into()))
        .await
        .map_err(BackendClientError::WebSocket)
}

pub(super) async fn send_binary(stream: &mut HostWebSocketStream, data: Bytes) -> Result<()> {
    stream
        .send(Message::Binary(data))
        .await
        .map_err(BackendClientError::WebSocket)
}

async fn wait_for_host_accepted(
    stream: &mut HostWebSocketStream,
    backend_session_id: &str,
    expected_incarnation_id: &uuid::Uuid,
) -> Result<HostAcceptedMessage> {
    while let Some(message) = stream.next().await {
        match message.map_err(BackendClientError::WebSocket)? {
            Message::Text(text) => {
                let envelope: HostEnvelope = serde_json::from_str(text.as_ref())?;
                if envelope.message_type == "host.accepted" {
                    return validate_host_accepted(
                        text.as_ref(),
                        backend_session_id,
                        expected_incarnation_id,
                    );
                }

                return Err(BackendClientError::Protocol {
                    reason: format!(
                        "backend sent `{}` before host.accepted",
                        envelope.message_type
                    ),
                });
            }
            Message::Close(_) => {
                return Err(BackendClientError::Protocol {
                    reason: "backend closed the host relay before host.accepted".to_owned(),
                });
            }
            Message::Ping(_) | Message::Pong(_) => {}
            Message::Binary(_) | Message::Frame(_) => {
                return Err(BackendClientError::Protocol {
                    reason: "backend sent non-text data before host.accepted".to_owned(),
                });
            }
        }
    }

    Err(BackendClientError::Protocol {
        reason: "backend closed the host relay before host.accepted".to_owned(),
    })
}

fn validate_host_accepted(
    message: &str,
    backend_session_id: &str,
    expected_incarnation_id: &uuid::Uuid,
) -> Result<HostAcceptedMessage> {
    let accepted: HostAcceptedMessage = serde_json::from_str(message)?;
    if accepted.session_id.eq_ignore_ascii_case(backend_session_id)
        && accepted.incarnation_id == *expected_incarnation_id
        && accepted.incarnation_generation > 0
        && relay_protocol_uses_exact_match()
        && accepted.relay_protocol_version == relay_protocol_version()
        && !accepted.relay_epoch.trim().is_empty()
    {
        return Ok(accepted);
    }

    Err(BackendClientError::Protocol {
        reason: "backend accepted the wrong host relay session".to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        HOST_RESIZE_MAX_COLS, HOST_RESIZE_MAX_ROWS, HostFocusChangedMessage, HostHelloMessage,
        HostInputMessage, HostParticipantDisconnectedMessage, HostResizeMessage,
        HostStreamDemandMessage, HostSuggestionMessage, validate_host_accepted,
    };
    use crate::session_relay_authority::relay_protocol_version;

    #[test]
    fn host_input_requires_session_id() {
        let error = serde_json::from_str::<HostInputMessage>(
            r#"{"type":"host.input","commandId":"action-1","payload":"pwd"}"#,
        )
        .expect_err("host.input without sessionId must fail closed");

        assert!(error.to_string().contains("sessionId"));
    }

    #[test]
    fn host_stream_demand_rejects_participant_count_overflow() {
        let demand = HostStreamDemandMessage {
            session_id: "session-1".to_owned(),
            required: true,
            shared_participant_count: usize::MAX,
            owner_participant_count: 1,
            reason: "owner_participant_joined".to_owned(),
        };

        assert_eq!(demand.participant_count(), None);
    }

    #[test]
    fn host_resize_requires_session_id() {
        let error = serde_json::from_str::<HostResizeMessage>(
            r#"{"type":"host.resize","commandId":"action-1","rows":24,"cols":80}"#,
        )
        .expect_err("host.resize without sessionId must fail closed");

        assert!(error.to_string().contains("sessionId"));
    }

    #[test]
    fn host_resize_decodes_dimensions_into_u16() {
        let resize: HostResizeMessage = serde_json::from_str(
            r#"{"type":"host.resize","sessionId":"session-1","commandId":"cmd-1","rows":24,"cols":80,"claim":false,"nonce":"nonce","ciphertext":"ciphertext","senderUserId":"user-1","senderDeviceId":"device-1","signature":"signature"}"#,
        )
        .expect("valid host.resize should deserialize");

        assert_eq!(resize.session_id, "session-1");
        assert_eq!(resize.command_id, "cmd-1");
        assert_eq!(resize.rows, 24);
        assert_eq!(resize.cols, 80);
    }

    #[test]
    fn host_resize_caps_define_a_safe_terminal_grid() {
        const FIVE_K_WIDTH_PX: u16 = 5120;
        const FIVE_K_HEIGHT_PX: u16 = 2880;
        const MIN_CELL_WIDTH_PX: u16 = 5;
        const MIN_CELL_HEIGHT_PX: u16 = 16;
        const _: () = assert!(HOST_RESIZE_MAX_ROWS >= FIVE_K_HEIGHT_PX / MIN_CELL_HEIGHT_PX);
        const _: () = assert!(HOST_RESIZE_MAX_COLS >= FIVE_K_WIDTH_PX / MIN_CELL_WIDTH_PX);

        const _: () = assert!(HOST_RESIZE_MAX_ROWS <= 4096);
        const _: () = assert!(HOST_RESIZE_MAX_COLS <= 4096);
    }

    #[test]
    fn host_suggestion_requires_session_id() {
        let error = serde_json::from_str::<HostSuggestionMessage>(
            r#"{"type":"host.suggestion","suggestionId":"action-1","body":"run tests"}"#,
        )
        .expect_err("host.suggestion without sessionId must fail closed");

        assert!(error.to_string().contains("sessionId"));
    }

    #[test]
    fn host_focus_changed_requires_session_id() {
        let error = serde_json::from_str::<HostFocusChangedMessage>(
            r#"{"type":"host.focusChanged","commandId":"cmd-1","clientId":"client-1","focused":true}"#,
        )
        .expect_err("host.focusChanged without sessionId must fail closed");

        assert!(error.to_string().contains("sessionId"));
    }

    #[test]
    fn host_focus_changed_requires_client_id() {
        let error = serde_json::from_str::<HostFocusChangedMessage>(
            r#"{"type":"host.focusChanged","sessionId":"session-1","commandId":"cmd-1","focused":true}"#,
        )
        .expect_err("host.focusChanged without clientId must fail closed");

        assert!(error.to_string().contains("clientId"));
    }

    #[test]
    fn host_focus_changed_decodes_focused_as_bool() {
        let payload: HostFocusChangedMessage = serde_json::from_str(
            r#"{"type":"host.focusChanged","sessionId":"session-1","commandId":"cmd-1","clientId":"client-7","focused":true,"nonce":"n","ciphertext":"c","senderUserId":"user-1","senderDeviceId":"device-1","signature":"sig"}"#,
        )
        .expect("valid host.focusChanged should deserialize");

        assert_eq!(payload.session_id, "session-1");
        assert_eq!(payload.command_id, "cmd-1");
        assert_eq!(payload.client_id, "client-7");
        assert!(payload.focused);
    }

    #[test]
    fn host_focus_changed_requires_device_authentication() {
        let error = serde_json::from_str::<HostFocusChangedMessage>(
            r#"{"type":"host.focusChanged","sessionId":"session-1","commandId":"cmd-1","clientId":"client-7","focused":false}"#,
        )
        .expect_err("unauthenticated host.focusChanged must fail closed");

        assert!(error.to_string().contains("nonce"));
    }

    #[test]
    fn host_participant_disconnected_decodes_without_authentication() {
        let payload: HostParticipantDisconnectedMessage = serde_json::from_str(
            r#"{"type":"host.participantDisconnected","sessionId":"session-1","clientId":"client-7"}"#,
        )
        .expect("valid host.participantDisconnected should deserialize");

        assert_eq!(payload.session_id, "session-1");
        assert_eq!(payload.client_id, "client-7");
    }

    #[test]
    fn host_participant_disconnected_ignores_a_focused_claim() {
        let payload: HostParticipantDisconnectedMessage = serde_json::from_str(
            r#"{"type":"host.participantDisconnected","sessionId":"session-1","clientId":"client-7","focused":true}"#,
        )
        .expect("unknown fields are ignored on the disconnect notice");

        assert_eq!(payload.client_id, "client-7");
    }

    #[test]
    fn host_participant_disconnected_requires_client_id() {
        let error = serde_json::from_str::<HostParticipantDisconnectedMessage>(
            r#"{"type":"host.participantDisconnected","sessionId":"session-1"}"#,
        )
        .expect_err("host.participantDisconnected without clientId must fail closed");

        assert!(error.to_string().contains("clientId"));
    }

    #[test]
    fn host_hello_advertises_the_relay_protocol_version() {
        let incarnation_id = uuid::Uuid::from_u128(2);
        let hello = HostHelloMessage {
            message_type: "host.hello",
            session_id: "session-1",
            session_secret: "secret",
            device_id: "device-1",
            expected_incarnation_id: &incarnation_id,
            relay_protocol_version: relay_protocol_version(),
        };
        let encoded =
            serde_json::to_string(&hello).expect("host.hello should serialize for the handshake");

        assert!(
            encoded.contains(&format!(
                r#""relayProtocolVersion":{}"#,
                relay_protocol_version()
            )),
            "host.hello must carry the version the backend gates sends on: {encoded}"
        );
        assert!(
            encoded.contains(&format!(r#""expectedIncarnationId":"{incarnation_id}""#)),
            "host.hello must bind the durable incarnation: {encoded}"
        );
    }

    #[test]
    fn host_focus_changed_rejects_missing_focused() {
        let error = serde_json::from_str::<HostFocusChangedMessage>(
            r#"{"type":"host.focusChanged","sessionId":"session-1","commandId":"cmd-1","clientId":"client-7"}"#,
        )
        .expect_err("host.focusChanged without focused must fail closed");

        assert!(error.to_string().contains("focused"));
    }

    #[test]
    fn host_accepted_requires_matching_session_id() {
        let incarnation_id = uuid::Uuid::from_u128(2);
        let accepted = format!(
            r#"{{"type":"host.accepted","sessionId":"session-123","relayEpoch":"epoch-1","incarnationId":"{incarnation_id}","incarnationGeneration":2,"relayProtocolVersion":{}}}"#,
            relay_protocol_version()
        );
        validate_host_accepted(&accepted, "session-123", &incarnation_id)
            .expect("matching host.accepted should be accepted");

        validate_host_accepted(
            r#"{"type":"host.accepted"}"#,
            "session-123",
            &incarnation_id,
        )
        .expect_err("host.accepted without sessionId must fail closed");

        validate_host_accepted(
            &accepted.replace("session-123", "other-session"),
            "session-123",
            &incarnation_id,
        )
        .expect_err("wrong-session host.accepted must fail closed");

        validate_host_accepted(
            &accepted.replace("epoch-1", ""),
            "session-123",
            &incarnation_id,
        )
        .expect_err("host.accepted without a relay epoch must fail closed");

        validate_host_accepted(&accepted, "session-123", &uuid::Uuid::from_u128(3))
            .expect_err("host.accepted for a stale incarnation must fail closed");
    }
}
