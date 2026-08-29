use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use futures_util::StreamExt;
use tokio::{sync::mpsc, time};
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;
use zeroize::{Zeroize, Zeroizing};

use crate::{
    BackendClientError, Result,
    close_reasons::{self, CloseBehavior},
    crypto,
    session_key_access::WAITING_FOR_SESSION_KEY_REASON,
};
use kodosi_domain::{
    lifecycle::{ConnectionState, RemoteActionStatus, RemoteSessionAccessState},
    session::SessionState,
};

use super::{
    RemoteRelayMode, SessionRelayClientSpec, SessionRelayCommand,
    connect::ConnectedRemoteSession,
    cursor::ReplayCursor,
    events::SessionRelayEventSink,
    incoming::{
        MessageOutcome, handle_encrypted_frame, handle_message_for_incarnation_with_acceptance,
    },
    ws::{
        SessionRelayWebSocketStream, participant_focus_changed_message,
        participant_heartbeat_message, participant_inject_message, participant_interrupt_message,
        participant_permission_decision_message, participant_resize_message,
        participant_semantic_cancel_message, participant_semantic_receipt_ack_message,
        participant_semantic_send_message, participant_stop_message, participant_suggest_message,
        send_session_relay_message,
    },
};

const PARTICIPANT_HEARTBEAT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
const PARTICIPANT_ACCEPTANCE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const MISSING_SESSION_KEY_RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(10);
const TERMINAL_INPUT_CHUNK_BYTES: usize = 1024 * 1024;

pub(super) enum ConnectedSessionOutcome {
    Reconnect {
        cursor: ReplayCursor,
        force_refresh_token: Option<Zeroizing<String>>,
    },
    TerminalReplayGap {
        cursor: ReplayCursor,
    },
    Exit,
}

pub(super) async fn run_connected_session(
    spec: &SessionRelayClientSpec,
    connected: ConnectedRemoteSession,
    events: &SessionRelayEventSink,
    command_rx: &mut mpsc::Receiver<SessionRelayCommand>,
    cancellation: &CancellationToken,
    cursor: ReplayCursor,
) -> ConnectedSessionOutcome {
    let ConnectedRemoteSession {
        mut stream,
        mut fetched_key,
        trusted_signer,
        retry_missing_session_key,
        owner_pin_established: _,
        refreshed,
        access_token,
    } = connected;

    if refreshed {
        events
            .info("session relay refreshed sign-in and reconnected".to_owned())
            .await;
    }

    let session_key = fetched_key.as_ref().or(spec.session_key.as_ref());
    let outcome = match receive_loop(
        spec,
        &mut stream,
        events,
        command_rx,
        cancellation,
        cursor,
        session_key,
        trusted_signer.as_ref(),
        retry_missing_session_key,
    )
    .await
    {
        Ok(ReceiveLoopOutcome::Reconnect {
            cursor: next_cursor,
            force_refresh,
        }) => ConnectedSessionOutcome::Reconnect {
            cursor: next_cursor,
            force_refresh_token: force_refresh.then_some(access_token),
        },
        Ok(ReceiveLoopOutcome::AccessRevoked) => {
            events.info("remote access revoked".to_owned()).await;
            ConnectedSessionOutcome::Exit
        }
        Ok(ReceiveLoopOutcome::Ended { reason }) => {
            events.state(SessionState::Stopped).await;
            events.info(format!("remote session ended: {reason}")).await;
            ConnectedSessionOutcome::Exit
        }
        Ok(ReceiveLoopOutcome::TerminalReplayGap { cursor }) => {
            ConnectedSessionOutcome::TerminalReplayGap { cursor }
        }
        Ok(ReceiveLoopOutcome::ProtocolDrift { reason }) => {
            events.log_error(reason.clone()).await;
            events
                .access_state(RemoteSessionAccessState::Failed, Some(reason.clone()), None)
                .await;
            events
                .connection(ConnectionState::Offline, Some(reason))
                .await;
            ConnectedSessionOutcome::Exit
        }
        Err(error) => {
            events
                .log_error(format!("session relay disconnected: {error}"))
                .await;
            ConnectedSessionOutcome::Reconnect {
                cursor,
                force_refresh_token: None,
            }
        }
    };

    reject_queued_commands(command_rx, events).await;

    fetched_key.zeroize();
    outcome
}

#[derive(Debug)]
enum ReceiveLoopOutcome {
    Reconnect {
        cursor: ReplayCursor,
        force_refresh: bool,
    },
    AccessRevoked,
    Ended {
        reason: String,
    },
    TerminalReplayGap {
        cursor: ReplayCursor,
    },
    ProtocolDrift {
        reason: String,
    },
}

struct EncryptedFrameState {
    encrypted_frames_without_key: u32,
    key_miss_logged: bool,
    retry_missing_session_key: bool,
}

#[expect(
    clippy::too_many_arguments,
    reason = "the receive loop keeps each bounded channel and trust input explicit"
)]
async fn receive_loop(
    spec: &SessionRelayClientSpec,
    stream: &mut SessionRelayWebSocketStream,
    events: &SessionRelayEventSink,
    command_rx: &mut mpsc::Receiver<SessionRelayCommand>,
    cancellation: &CancellationToken,
    initial_cursor: ReplayCursor,
    session_key: Option<&crypto::SessionKey>,
    trusted_signer: Option<&crate::session_key_service::SessionKeyTrustedSigner>,
    retry_missing_session_key: bool,
) -> Result<ReceiveLoopOutcome> {
    let mut cursor = initial_cursor;
    cursor.reset_encrypted_frame_admission();
    let mut encrypted_frame_state = EncryptedFrameState {
        encrypted_frames_without_key: 0,
        key_miss_logged: false,
        retry_missing_session_key: retry_missing_session_key && session_key.is_none(),
    };
    let mut participant_accepted = false;
    let acceptance_timeout = time::sleep(PARTICIPANT_ACCEPTANCE_TIMEOUT);
    let missing_session_key_retry = time::sleep(MISSING_SESSION_KEY_RETRY_DELAY);
    let mut heartbeat = time::interval(PARTICIPANT_HEARTBEAT_INTERVAL);
    tokio::pin!(acceptance_timeout);
    tokio::pin!(missing_session_key_retry);

    loop {
        tokio::select! {
            () = cancellation.cancelled() => {
                return Ok(ReceiveLoopOutcome::Reconnect {
                    cursor,
                    force_refresh: false,
                });
            }
            () = &mut acceptance_timeout, if !participant_accepted => {
                return Ok(protocol_drift_outcome(
                    "backend relay protocol drift: participant acceptance timed out",
                ));
            }
            () = &mut missing_session_key_retry, if participant_accepted && encrypted_frame_state.retry_missing_session_key => {
                if !encrypted_frame_state.key_miss_logged {
                    tracing::warn!(
                        session_id = %spec.id,
                        "session key still unavailable after reconnect wait — retrying participant connection",
                    );
                }
                return Ok(ReceiveLoopOutcome::Reconnect {
                    cursor,
                    force_refresh: false,
                });
            }
            _ = heartbeat.tick(), if participant_accepted => {
                send_session_relay_message(stream, participant_heartbeat_message()).await?;
            }
            Some(command) = command_rx.recv(), if participant_accepted => {
                handle_remote_command(
                    stream,
                    events,
                    command,
                    spec,
                    session_key,
                ).await?;
            }
            message = stream.next() => {
                if let Some(outcome) = handle_incoming_message(
                    message,
                    stream,
                    spec,
                    session_key,
                    trusted_signer,
                    events,
                    &mut cursor,
                    &mut encrypted_frame_state,
                    &mut participant_accepted,
                ).await? {
                    return Ok(outcome);
                }
            }
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "message dispatch keeps transport cursor and replay state explicit"
)]
async fn handle_incoming_message(
    message: Option<std::result::Result<Message, tokio_tungstenite::tungstenite::Error>>,
    stream: &mut SessionRelayWebSocketStream,
    spec: &SessionRelayClientSpec,
    session_key: Option<&crypto::SessionKey>,
    trusted_signer: Option<&crate::session_key_service::SessionKeyTrustedSigner>,
    events: &SessionRelayEventSink,
    cursor: &mut ReplayCursor,
    encrypted_frame_state: &mut EncryptedFrameState,
    participant_accepted: &mut bool,
) -> Result<Option<ReceiveLoopOutcome>> {
    match message {
        Some(Ok(Message::Text(text))) => {
            match handle_text_message_with_receipts(
                spec,
                stream,
                trusted_signer,
                spec.relay_mode,
                &spec.backend_incarnation_id,
                text.as_ref(),
                events,
                cursor,
                participant_accepted,
            )
            .await
            {
                Ok(outcome) => Ok(outcome),
                Err(error) => Ok(Some(protocol_drift_outcome(format!(
                    "backend relay protocol drift: {error}"
                )))),
            }
        }
        Some(Ok(Message::Binary(data))) => {
            handle_incoming_binary(
                data,
                spec,
                session_key,
                events,
                cursor,
                encrypted_frame_state,
                *participant_accepted,
            )
            .await
        }
        Some(Ok(Message::Close(frame))) => Ok(Some(receive_outcome_for_close(
            close_reasons::interpret(frame.as_ref()),
            *cursor,
        ))),
        None => Ok(Some(ReceiveLoopOutcome::Reconnect {
            cursor: *cursor,
            force_refresh: false,
        })),
        Some(Ok(Message::Ping(_) | Message::Pong(_))) => Ok(None),
        Some(Ok(Message::Frame(_))) => Ok(Some(protocol_drift_outcome(
            "backend relay protocol drift: unsupported non-text frame",
        ))),
        Some(Err(error)) => Err(BackendClientError::WebSocket(error)),
    }
}

fn protocol_drift_outcome(reason: impl Into<String>) -> ReceiveLoopOutcome {
    ReceiveLoopOutcome::ProtocolDrift {
        reason: reason.into(),
    }
}

fn receive_outcome_for_close(behavior: CloseBehavior, cursor: ReplayCursor) -> ReceiveLoopOutcome {
    match behavior {
        CloseBehavior::AuthRefresh => ReceiveLoopOutcome::Reconnect {
            cursor,
            force_refresh: true,
        },
        CloseBehavior::TerminalReplayGap => ReceiveLoopOutcome::TerminalReplayGap { cursor },
        CloseBehavior::Terminal { reason } => ReceiveLoopOutcome::Ended { reason },
        CloseBehavior::Retryable => ReceiveLoopOutcome::Reconnect {
            cursor,
            force_refresh: false,
        },
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "authenticated receipt handling binds every transport and replay input"
)]
async fn handle_text_message_with_receipts(
    spec: &SessionRelayClientSpec,
    stream: &mut SessionRelayWebSocketStream,
    trusted_signer: Option<&crate::session_key_service::SessionKeyTrustedSigner>,
    relay_mode: RemoteRelayMode,
    expected_incarnation_id: &uuid::Uuid,
    message: &str,
    events: &SessionRelayEventSink,
    cursor: &mut ReplayCursor,
    participant_accepted: &mut bool,
) -> Result<Option<ReceiveLoopOutcome>> {
    let outcome = handle_message_for_incarnation_with_acceptance(
        relay_mode,
        expected_incarnation_id,
        message,
        events,
        cursor,
        *participant_accepted,
    )
    .await?;
    match outcome {
        MessageOutcome::SemanticReceipt(receipt) => {
            verify_semantic_receipt(spec, trusted_signer, &receipt)?;
            if !events.semantic_receipt(receipt.clone()).await {
                return Err(BackendClientError::Protocol {
                    reason: "verified semantic receipt was not durably applied".to_owned(),
                });
            }
            send_semantic_receipt_ack(spec, stream, &receipt).await?;
            Ok(None)
        }
        ordinary => apply_text_outcome(ordinary, events, cursor, participant_accepted).await,
    }
}

#[cfg(test)]
async fn handle_text_message(
    relay_mode: RemoteRelayMode,
    expected_incarnation_id: &uuid::Uuid,
    message: &str,
    events: &SessionRelayEventSink,
    cursor: &mut ReplayCursor,
    participant_accepted: &mut bool,
) -> Result<Option<ReceiveLoopOutcome>> {
    let outcome = handle_message_for_incarnation_with_acceptance(
        relay_mode,
        expected_incarnation_id,
        message,
        events,
        cursor,
        *participant_accepted,
    )
    .await?;
    if matches!(outcome, MessageOutcome::SemanticReceipt(_)) {
        return Err(BackendClientError::Protocol {
            reason: "semantic receipt requires authenticated transport context".to_owned(),
        });
    }
    apply_text_outcome(outcome, events, cursor, participant_accepted).await
}

async fn apply_text_outcome(
    outcome: MessageOutcome,
    events: &SessionRelayEventSink,
    cursor: &ReplayCursor,
    participant_accepted: &mut bool,
) -> Result<Option<ReceiveLoopOutcome>> {
    Ok(match outcome {
        MessageOutcome::ParticipantAccepted => {
            if *participant_accepted {
                return Err(BackendClientError::Protocol {
                    reason: "duplicate participant acceptance".to_owned(),
                });
            }
            *participant_accepted = true;
            events.connection(ConnectionState::Connected, None).await;
            None
        }
        MessageOutcome::Continue => None,
        MessageOutcome::SemanticReceipt(_) => {
            return Err(BackendClientError::Protocol {
                reason: "semantic receipt escaped authenticated handler".to_owned(),
            });
        }
        MessageOutcome::Reconnect => Some(ReceiveLoopOutcome::Reconnect {
            cursor: *cursor,
            force_refresh: false,
        }),
        MessageOutcome::AccessRevoked => Some(ReceiveLoopOutcome::AccessRevoked),
        MessageOutcome::Ended { reason } => Some(ReceiveLoopOutcome::Ended { reason }),
    })
}

fn verify_semantic_receipt(
    spec: &SessionRelayClientSpec,
    trusted_signer: Option<&crate::session_key_service::SessionKeyTrustedSigner>,
    receipt: &crate::session_relay::wire::ParticipantSemanticReceiptMessage,
) -> Result<()> {
    use aws_lc_rs::signature::VerificationAlgorithm;
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    let requester_user_id =
        spec.viewer_user_id
            .as_deref()
            .ok_or_else(|| BackendClientError::Protocol {
                reason: "semantic receipt requires requester user identity".to_owned(),
            })?;
    let requester_device_id =
        spec.viewer_device_id
            .as_deref()
            .ok_or_else(|| BackendClientError::Protocol {
                reason: "semantic receipt requires requester device identity".to_owned(),
            })?;
    receipt.validate_target(
        &spec.backend_session_id,
        &spec.backend_incarnation_id,
        requester_user_id,
        requester_device_id,
    )?;
    let signer = trusted_signer.ok_or_else(|| BackendClientError::Protocol {
        reason: "semantic receipt arrived without pinned owner signer".to_owned(),
    })?;
    #[expect(
        clippy::suspicious_operation_groupings,
        reason = "the trusted key-blob sender is the pinned owner device for receipt signatures"
    )]
    if receipt.owner_user_id != signer.owner_user_id
        || receipt.owner_device_id != signer.sender_device_id
        || spec.owner_user_id.as_deref() != Some(signer.owner_user_id.as_str())
    {
        return Err(BackendClientError::Protocol {
            reason: "semantic receipt owner does not match pinned signer".to_owned(),
        });
    }
    let session_id =
        uuid::Uuid::parse_str(&receipt.session_id).map_err(|_| BackendClientError::Protocol {
            reason: "semantic receipt session ID is invalid".to_owned(),
        })?;
    let requester_user_uuid = uuid::Uuid::parse_str(&receipt.requester_user_id).map_err(|_| {
        BackendClientError::Protocol {
            reason: "semantic receipt requester user ID is invalid".to_owned(),
        }
    })?;
    let owner_user_uuid = uuid::Uuid::parse_str(&receipt.owner_user_id).map_err(|_| {
        BackendClientError::Protocol {
            reason: "semantic receipt owner user ID is invalid".to_owned(),
        }
    })?;
    let preimage = crypto::semantic_receipt_preimage(
        &session_id,
        &receipt.incarnation_id,
        &receipt.request_id,
        receipt.mode.as_str(),
        &receipt.payload_sha256,
        receipt.outcome.as_str(),
        &requester_user_uuid,
        &receipt.requester_device_id,
        &owner_user_uuid,
        &receipt.owner_device_id,
    )?;
    let signature =
        BASE64
            .decode(&receipt.signature)
            .map_err(|_| BackendClientError::Protocol {
                reason: "semantic receipt signature is not valid base64".to_owned(),
            })?;
    aws_lc_rs::signature::ML_DSA_65
        .verify_sig(&signer.public_key, &preimage, &signature)
        .map_err(|_| BackendClientError::Crypto {
            reason: "semantic receipt owner signature verification failed".to_owned(),
        })
}

async fn send_semantic_receipt_ack(
    spec: &SessionRelayClientSpec,
    stream: &mut SessionRelayWebSocketStream,
    receipt: &crate::session_relay::wire::ParticipantSemanticReceiptMessage,
) -> Result<()> {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    let signing_pkcs8 =
        spec.viewer_signing_pkcs8
            .as_deref()
            .ok_or_else(|| BackendClientError::Protocol {
                reason: "semantic receipt acknowledgement requires requester signing key"
                    .to_owned(),
            })?;
    let session_id =
        uuid::Uuid::parse_str(&receipt.session_id).map_err(|_| BackendClientError::Protocol {
            reason: "semantic receipt acknowledgement session ID is invalid".to_owned(),
        })?;
    let requester_user_id = uuid::Uuid::parse_str(&receipt.requester_user_id).map_err(|_| {
        BackendClientError::Protocol {
            reason: "semantic receipt acknowledgement requester ID is invalid".to_owned(),
        }
    })?;
    let preimage = crypto::semantic_receipt_ack_preimage(
        &session_id,
        &receipt.incarnation_id,
        &receipt.request_id,
        &requester_user_id,
        &receipt.requester_device_id,
    )?;
    let signature = BASE64.encode(crypto::sign_control_message(signing_pkcs8, &preimage)?);
    send_session_relay_message(
        stream,
        participant_semantic_receipt_ack_message(
            &receipt.session_id,
            &receipt.incarnation_id,
            &receipt.request_id,
            &receipt.requester_user_id,
            &receipt.requester_device_id,
            &signature,
        ),
    )
    .await
}

async fn handle_incoming_binary(
    data: bytes::Bytes,
    spec: &SessionRelayClientSpec,
    session_key: Option<&crypto::SessionKey>,
    events: &SessionRelayEventSink,
    cursor: &mut ReplayCursor,
    encrypted_frame_state: &mut EncryptedFrameState,
    participant_accepted: bool,
) -> Result<Option<ReceiveLoopOutcome>> {
    if !participant_accepted {
        return Ok(Some(protocol_drift_outcome(
            "backend relay protocol drift: encrypted frame arrived before participant acceptance",
        )));
    }
    handle_binary_message(
        spec,
        &data,
        session_key,
        events,
        cursor,
        encrypted_frame_state,
    )
    .await
}

async fn handle_binary_message(
    spec: &SessionRelayClientSpec,
    data: &[u8],
    session_key: Option<&crypto::SessionKey>,
    events: &SessionRelayEventSink,
    cursor: &mut ReplayCursor,
    encrypted_frame_state: &mut EncryptedFrameState,
) -> Result<Option<ReceiveLoopOutcome>> {
    if let Some(key) = session_key {
        return match handle_encrypted_frame(data, key, spec.backend_incarnation_id, events, cursor)
            .await
        {
            Ok(()) => {
                encrypted_frame_state.encrypted_frames_without_key = 0;
                Ok(None)
            }
            Err(error) => Ok(Some(protocol_drift_outcome(format!(
                "viewer encrypted-session failure: {error}"
            )))),
        };
    }

    encrypted_frame_state.encrypted_frames_without_key += 1;
    if !encrypted_frame_state.key_miss_logged {
        encrypted_frame_state.key_miss_logged = true;
        if encrypted_frame_state.retry_missing_session_key {
            events
                .access_state(
                    RemoteSessionAccessState::AwaitingKey,
                    Some(WAITING_FOR_SESSION_KEY_REASON.to_owned()),
                    None,
                )
                .await;
            tracing::warn!(
                session_id = %spec.id,
                "encrypted frames received but no session key — content not decryptable. \
                 Key will be fetched on next reconnect."
            );
        } else {
            return Ok(Some(protocol_drift_outcome(
                "encrypted relay frame arrived before a usable session key was available",
            )));
        }
    }

    if encrypted_frame_state.retry_missing_session_key
        && encrypted_frame_state.encrypted_frames_without_key == 5
    {
        return Ok(Some(ReceiveLoopOutcome::Reconnect {
            cursor: *cursor,
            force_refresh: false,
        }));
    }

    Ok(None)
}

async fn handle_remote_command(
    stream: &mut SessionRelayWebSocketStream,
    events: &SessionRelayEventSink,
    command: SessionRelayCommand,
    spec: &SessionRelayClientSpec,
    session_key: Option<&crypto::SessionKey>,
) -> Result<()> {
    if matches!(
        command,
        SessionRelayCommand::SemanticSend { .. } | SessionRelayCommand::SemanticCancel { .. }
    ) {
        let payload = build_semantic_command_payload(&command, spec, session_key)?;
        validate_outbound_command_size(&payload)?;
        return send_session_relay_message(stream, payload).await;
    }
    let reported_action = reported_action(&command);
    if let SessionRelayCommand::OwnerInject { payload } = &command {
        return send_input_chunks(stream, events, spec, session_key, payload).await;
    }

    let built = build_remote_command_payload(&command, spec, session_key);
    let payload = match built {
        Ok(payload) => payload,
        Err(error) => {
            if let Some(action) = reported_action {
                events
                    .action_result(
                        action.action_id,
                        action.request_id,
                        action.request_generation,
                        RemoteActionStatus::Rejected,
                    )
                    .await;
            }
            return Err(error);
        }
    };
    validate_outbound_command_size(&payload).map_err(|error| {
        if reported_action.is_some() {
            tracing::warn!(%error, "rejecting oversized remote command");
        }
        error
    })?;

    send_remote_command_payload(stream, events, reported_action, payload).await
}

async fn send_input_chunks(
    stream: &mut SessionRelayWebSocketStream,
    events: &SessionRelayEventSink,
    spec: &SessionRelayClientSpec,
    session_key: Option<&crypto::SessionKey>,
    payload: &[u8],
) -> Result<()> {
    for wire in build_input_chunk_payloads(spec, session_key, payload)? {
        send_remote_command_payload(stream, events, None, wire).await?;
    }
    Ok(())
}

fn build_input_chunk_payloads(
    spec: &SessionRelayClientSpec,
    session_key: Option<&crypto::SessionKey>,
    payload: &[u8],
) -> Result<Vec<serde_json::Value>> {
    let mut messages = Vec::with_capacity(payload.len().div_ceil(TERMINAL_INPUT_CHUNK_BYTES));
    for chunk in payload.chunks(TERMINAL_INPUT_CHUNK_BYTES) {
        let chunk_action_id = uuid::Uuid::now_v7().simple().to_string();
        let sealed = seal_control(session_key, spec, "inject", &chunk_action_id, chunk)?;
        let wire = participant_inject_message(&chunk_action_id, &sealed.0, &sealed.1, &sealed.2);
        validate_outbound_command_size(&wire)?;
        messages.push(wire);
    }
    Ok(messages)
}

fn validate_outbound_command_size(payload: &serde_json::Value) -> Result<()> {
    let message_type = payload["type"]
        .as_str()
        .ok_or_else(|| BackendClientError::Protocol {
            reason: "remote command payload has no message type".to_owned(),
        })?;
    let max_bytes =
        crate::session_relay_authority::participant_outbound_message_limit(message_type)
            .ok_or_else(|| BackendClientError::Protocol {
                reason: format!("remote command `{message_type}` has no outbound size authority"),
            })?;
    let bytes = serde_json::to_vec(payload)?;
    if bytes.len() > max_bytes {
        return Err(BackendClientError::Protocol {
            reason: format!(
                "remote command `{message_type}` is {} bytes, exceeding the {max_bytes}-byte limit",
                bytes.len(),
            ),
        });
    }
    Ok(())
}

#[derive(Debug)]
struct ReportedAction {
    action_id: String,
    request_id: Option<String>,
    request_generation: Option<u64>,
}

fn reported_action(command: &SessionRelayCommand) -> Option<ReportedAction> {
    match command {
        SessionRelayCommand::Suggest { action_id, .. }
        | SessionRelayCommand::Inject { action_id, .. } => Some(ReportedAction {
            action_id: action_id.clone(),
            request_id: None,
            request_generation: None,
        }),
        SessionRelayCommand::PermissionDecision {
            action_id,
            request_id,
            request_generation,
            ..
        } => Some(ReportedAction {
            action_id: action_id.clone(),
            request_id: Some(request_id.clone()),
            request_generation: Some(*request_generation),
        }),
        SessionRelayCommand::OwnerResize { action_id, .. } => Some(ReportedAction {
            action_id: action_id.clone(),
            request_id: Some(action_id.clone()),
            request_generation: None,
        }),
        SessionRelayCommand::SemanticSend { .. }
        | SessionRelayCommand::SemanticCancel { .. }
        | SessionRelayCommand::OwnerInject { .. }
        | SessionRelayCommand::OwnerFocusChanged { .. }
        | SessionRelayCommand::OwnerStop
        | SessionRelayCommand::OwnerInterrupt => None,
    }
}

fn build_semantic_command_payload(
    command: &SessionRelayCommand,
    spec: &SessionRelayClientSpec,
    session_key: Option<&crypto::SessionKey>,
) -> Result<serde_json::Value> {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    let session_key = session_key.ok_or_else(|| BackendClientError::Protocol {
        reason: "remote semantic send requires an encrypted session key".to_owned(),
    })?;
    let session_id = uuid::Uuid::parse_str(&spec.backend_session_id).map_err(|_| {
        BackendClientError::Protocol {
            reason: "remote semantic session ID is invalid".to_owned(),
        }
    })?;
    let requester_user_id =
        spec.viewer_user_id
            .as_deref()
            .ok_or_else(|| BackendClientError::Protocol {
                reason: "remote semantic send requires requester user ID".to_owned(),
            })?;
    let requester_user_uuid =
        uuid::Uuid::parse_str(requester_user_id).map_err(|_| BackendClientError::Protocol {
            reason: "remote semantic requester user ID is invalid".to_owned(),
        })?;
    let requester_device_id =
        spec.viewer_device_id
            .as_deref()
            .ok_or_else(|| BackendClientError::Protocol {
                reason: "remote semantic send requires requester device ID".to_owned(),
            })?;
    let signing_pkcs8 =
        spec.viewer_signing_pkcs8
            .as_deref()
            .ok_or_else(|| BackendClientError::Protocol {
                reason: "remote semantic send requires requester signing key".to_owned(),
            })?;
    match command {
        SessionRelayCommand::SemanticSend {
            request_id,
            incarnation_id,
            mode,
            payload_sha256,
            text,
        } => {
            let aad = crypto::semantic_request_associated_data(
                &session_id,
                incarnation_id,
                request_id,
                mode.as_str(),
                payload_sha256,
                &requester_user_uuid,
                requester_device_id,
            )?;
            let sealed = crypto::encrypt_semantic_request(session_key, &aad, text.as_bytes())?;
            let preimage = crypto::semantic_request_signature_preimage(
                &session_id,
                incarnation_id,
                request_id,
                mode.as_str(),
                payload_sha256,
                &requester_user_uuid,
                requester_device_id,
                &sealed.nonce,
                &sealed.ciphertext,
            )?;
            let signature = BASE64.encode(crypto::sign_control_message(signing_pkcs8, &preimage)?);
            Ok(participant_semantic_send_message(
                request_id,
                incarnation_id,
                *mode,
                payload_sha256,
                &BASE64.encode(sealed.nonce),
                &BASE64.encode(sealed.ciphertext),
                &signature,
            ))
        }
        SessionRelayCommand::SemanticCancel {
            request_id,
            incarnation_id,
            mode,
            payload_sha256,
        } => {
            let preimage = crypto::semantic_cancel_preimage(
                &session_id,
                incarnation_id,
                request_id,
                mode.as_str(),
                payload_sha256,
                &requester_user_uuid,
                requester_device_id,
            )?;
            let signature = BASE64.encode(crypto::sign_control_message(signing_pkcs8, &preimage)?);
            Ok(participant_semantic_cancel_message(
                request_id,
                incarnation_id,
                *mode,
                payload_sha256,
                &signature,
            ))
        }
        _ => Err(BackendClientError::Protocol {
            reason: "non-semantic command reached semantic payload builder".to_owned(),
        }),
    }
}

fn build_owner_resize_payload(
    spec: &SessionRelayClientSpec,
    session_key: Option<&crypto::SessionKey>,
    action_id: &str,
    rows: u16,
    cols: u16,
    pixel_geometry: Option<kodosi_domain::terminal::TerminalPixelGeometry>,
    claim: bool,
) -> Result<serde_json::Value> {
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
    let kind = format!("resize:{rows}:{cols}:{claim}:{geometry_tag}");
    let sealed = seal_control(session_key, spec, &kind, action_id, &[])?;
    Ok(participant_resize_message(
        action_id,
        rows,
        cols,
        pixel_geometry,
        claim,
        &sealed.0,
        &sealed.1,
        &sealed.2,
    ))
}

fn build_remote_command_payload(
    command: &SessionRelayCommand,
    spec: &SessionRelayClientSpec,
    session_key: Option<&crypto::SessionKey>,
) -> Result<serde_json::Value> {
    Ok(match command {
        SessionRelayCommand::SemanticSend { .. } | SessionRelayCommand::SemanticCancel { .. } => {
            return Err(BackendClientError::Protocol {
                reason: "semantic command must use semantic payload builder".to_owned(),
            });
        }
        SessionRelayCommand::Suggest { action_id, body } => {
            let sealed = seal_control(session_key, spec, "suggest", action_id, body.as_bytes())?;
            participant_suggest_message(action_id, &sealed.0, &sealed.1, &sealed.2)
        }
        SessionRelayCommand::Inject { action_id, payload } => {
            let sealed = seal_control(session_key, spec, "inject", action_id, payload)?;
            participant_inject_message(action_id, &sealed.0, &sealed.1, &sealed.2)
        }
        SessionRelayCommand::PermissionDecision {
            action_id,
            request_id,
            request_generation,
            decision,
        } => {
            let kind = format!("permission:{decision}:{request_id}:{request_generation}");
            let sealed = seal_control(session_key, spec, &kind, action_id, &[])?;
            participant_permission_decision_message(
                action_id,
                request_id,
                *request_generation,
                decision,
                &sealed.0,
                &sealed.1,
                &sealed.2,
            )
        }
        SessionRelayCommand::OwnerInject { .. } => {
            return Err(BackendClientError::Protocol {
                reason: "owner input must use the bounded chunk sender".to_owned(),
            });
        }

        SessionRelayCommand::OwnerResize {
            action_id,
            rows,
            cols,
            pixel_geometry,
            claim,
        } => build_owner_resize_payload(
            spec,
            session_key,
            action_id,
            *rows,
            *cols,
            *pixel_geometry,
            *claim,
        )?,
        SessionRelayCommand::OwnerFocusChanged { focused } => {
            let action_id = uuid::Uuid::now_v7().simple().to_string();
            let kind = format!("focus:{focused}");
            let sealed = seal_control(session_key, spec, &kind, &action_id, &[])?;
            participant_focus_changed_message(&action_id, *focused, &sealed.0, &sealed.1, &sealed.2)
        }
        SessionRelayCommand::OwnerStop => {
            let action_id = uuid::Uuid::now_v7().simple().to_string();
            let sealed = seal_control(session_key, spec, "stop", &action_id, &[])?;
            participant_stop_message(&action_id, &sealed.0, &sealed.1, &sealed.2)
        }
        SessionRelayCommand::OwnerInterrupt => {
            let action_id = uuid::Uuid::now_v7().simple().to_string();
            let sealed = seal_control(session_key, spec, "interrupt", &action_id, &[])?;
            participant_interrupt_message(&action_id, &sealed.0, &sealed.1, &sealed.2)
        }
    })
}

async fn send_remote_command_payload(
    stream: &mut SessionRelayWebSocketStream,
    events: &SessionRelayEventSink,
    reported_action: Option<ReportedAction>,
    payload: serde_json::Value,
) -> Result<()> {
    if let Err(error) = send_session_relay_message(stream, payload).await {
        if let Some(action) = reported_action {
            events
                .action_result(
                    action.action_id,
                    action.request_id,
                    action.request_generation,
                    RemoteActionStatus::Busy,
                )
                .await;
        }
        return Err(error);
    }

    Ok(())
}

fn seal_control(
    session_key: Option<&crypto::SessionKey>,
    spec: &SessionRelayClientSpec,
    kind: &str,
    action_id: &str,
    plaintext: &[u8],
) -> Result<(String, String, String)> {
    let session_key = session_key.ok_or_else(|| BackendClientError::Protocol {
        reason: "remote control requires an encrypted session key".to_owned(),
    })?;
    let user_id = spec
        .viewer_user_id
        .as_deref()
        .ok_or_else(|| BackendClientError::Protocol {
            reason: "remote control requires a sender user id".to_owned(),
        })?;
    let device_id =
        spec.viewer_device_id
            .as_deref()
            .ok_or_else(|| BackendClientError::Protocol {
                reason: "remote control requires a sender device id".to_owned(),
            })?;
    let signing_pkcs8 =
        spec.viewer_signing_pkcs8
            .as_deref()
            .ok_or_else(|| BackendClientError::Protocol {
                reason: "remote control requires a device signing key".to_owned(),
            })?;
    let aad = crypto::control_associated_data(kind, &spec.backend_session_id, action_id)?;
    let sealed = crypto::encrypt_control_payload(session_key, &aad, plaintext)?;
    let preimage = crypto::control_message_preimage(
        kind,
        &spec.backend_session_id,
        action_id,
        user_id,
        device_id,
        &sealed.nonce,
        &sealed.ciphertext,
    )?;
    let signature = crypto::sign_control_message(signing_pkcs8, &preimage)?;
    Ok((
        BASE64.encode(sealed.nonce),
        BASE64.encode(sealed.ciphertext),
        BASE64.encode(signature),
    ))
}

pub(super) async fn reject_queued_commands(
    command_rx: &mut mpsc::Receiver<SessionRelayCommand>,
    events: &SessionRelayEventSink,
) {
    while let Ok(command) = command_rx.try_recv() {
        if let Some(action) = reported_action(&command) {
            events
                .action_result(
                    action.action_id,
                    action.request_id,
                    action.request_generation,
                    RemoteActionStatus::Rejected,
                )
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use aws_lc_rs::signature::{ML_DSA_65_SIGNING, PqdsaKeyPair};
    use tokio::sync::mpsc;

    use super::*;
    use crate::session_relay::events::SessionRelayEvent;
    use kodosi_domain::ids::SessionId;

    #[test]
    fn semantic_receipt_verifies_only_pinned_owner_signature_and_exact_tuple() {
        use aws_lc_rs::signature::KeyPair;
        let owner = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).expect("owner key");
        let public = owner.public_key();
        let session_id = uuid::Uuid::now_v7();
        let incarnation_id = uuid::Uuid::now_v7();
        let request_id = uuid::Uuid::now_v7();
        let requester_id = uuid::Uuid::now_v7();
        let fingerprint = "a".repeat(64);
        let preimage = crypto::semantic_receipt_preimage(
            &session_id,
            &incarnation_id,
            &request_id,
            "queue",
            &fingerprint,
            "injected",
            &requester_id,
            "requester-device",
            &requester_id,
            "owner-device",
        )
        .expect("receipt preimage");
        let signature = crypto::sign_control_message(
            owner.to_pkcs8v1().expect("owner pkcs8").as_ref(),
            &preimage,
        )
        .expect("sign");
        let spec = SessionRelayClientSpec {
            id: SessionId::new(),
            backend_session_id: session_id.to_string(),
            backend_incarnation_id: incarnation_id,
            cursor: ReplayCursor::default(),
            relay_mode: RemoteRelayMode::OwnerParticipant,
            session_key: Some([7; 32]),
            viewer_kem_secret_bytes: None,
            viewer_device_id: Some("requester-device".to_owned()),
            viewer_user_id: Some(requester_id.to_string()),
            viewer_signing_pkcs8: None,
            owner_user_id: Some(requester_id.to_string()),
        };
        let signer = crate::session_key_service::SessionKeyTrustedSigner {
            public_key: public.as_ref().to_vec(),
            owner_user_id: requester_id.to_string(),
            sender_device_id: "owner-device".to_owned(),
            device_list_generation: 1,
            identity_fingerprint: [1; 32],
        };
        let mut receipt = crate::session_relay::wire::ParticipantSemanticReceiptMessage {
            session_id: session_id.to_string(),
            incarnation_id,
            request_id,
            mode: crate::session_relay::wire::RelaySemanticMode::Queue,
            payload_sha256: fingerprint,
            outcome: crate::session_relay::wire::RelaySemanticOutcome::Injected,
            requester_user_id: requester_id.to_string(),
            requester_device_id: "requester-device".to_owned(),
            owner_user_id: requester_id.to_string(),
            owner_device_id: "owner-device".to_owned(),
            signature: BASE64.encode(signature),
        };
        verify_semantic_receipt(&spec, Some(&signer), &receipt).expect("valid receipt");
        receipt.outcome = crate::session_relay::wire::RelaySemanticOutcome::Cancelled;
        assert!(verify_semantic_receipt(&spec, Some(&signer), &receipt).is_err());
    }

    #[test]
    fn semantic_send_uses_exact_tuple_signature_and_separate_encryption_key() {
        use aws_lc_rs::signature::{KeyPair, VerificationAlgorithm};
        let keypair = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).expect("keypair");
        let pkcs8 = keypair.to_pkcs8v1().expect("pkcs8");
        let public = keypair.public_key();
        let session_key = [7_u8; 32];
        let session_id = uuid::Uuid::now_v7();
        let incarnation_id = uuid::Uuid::now_v7();
        let request_id = uuid::Uuid::now_v7();
        let requester_id = uuid::Uuid::now_v7();
        let text = "continue after the tool";
        let fingerprint = crypto::sha256_hex(text.as_bytes());
        let spec = SessionRelayClientSpec {
            id: SessionId::new(),
            backend_session_id: session_id.to_string(),
            backend_incarnation_id: incarnation_id,
            cursor: ReplayCursor::default(),
            relay_mode: RemoteRelayMode::OwnerParticipant,
            session_key: Some(session_key),
            viewer_kem_secret_bytes: None,
            viewer_device_id: Some("requester-device".to_owned()),
            viewer_user_id: Some(requester_id.to_string()),
            viewer_signing_pkcs8: Some(pkcs8.as_ref().to_vec()),
            owner_user_id: Some(requester_id.to_string()),
        };
        let command = SessionRelayCommand::SemanticSend {
            request_id,
            incarnation_id,
            mode: crate::session_relay::wire::RelaySemanticMode::Steer,
            payload_sha256: fingerprint.clone(),
            text: text.to_owned(),
        };

        let wire = build_semantic_command_payload(&command, &spec, Some(&session_key))
            .expect("semantic payload");
        let nonce = BASE64
            .decode(wire["nonce"].as_str().expect("nonce"))
            .expect("nonce b64");
        let ciphertext = BASE64
            .decode(wire["ciphertext"].as_str().expect("ciphertext"))
            .expect("ciphertext b64");
        let signature = BASE64
            .decode(wire["signature"].as_str().expect("signature"))
            .expect("signature b64");
        let aad = crypto::semantic_request_associated_data(
            &session_id,
            &incarnation_id,
            &request_id,
            "steer",
            &fingerprint,
            &requester_id,
            "requester-device",
        )
        .expect("aad");
        assert_eq!(
            crypto::decrypt_semantic_request(&session_key, &aad, &nonce, &ciphertext)
                .expect("semantic decrypt"),
            text.as_bytes()
        );
        assert!(crypto::decrypt_control_payload(&session_key, &aad, &nonce, &ciphertext,).is_err());
        let preimage = crypto::semantic_request_signature_preimage(
            &session_id,
            &incarnation_id,
            &request_id,
            "steer",
            &fingerprint,
            &requester_id,
            "requester-device",
            &nonce,
            &ciphertext,
        )
        .expect("preimage");
        aws_lc_rs::signature::ML_DSA_65
            .verify_sig(public.as_ref(), &preimage, &signature)
            .expect("signature verifies");
    }

    #[test]
    fn maximum_semantic_plaintext_fits_the_normative_envelope() {
        let keypair = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).expect("keypair");
        let pkcs8 = keypair.to_pkcs8v1().expect("pkcs8");
        let session_key = [7_u8; 32];
        let session_id = uuid::Uuid::now_v7();
        let incarnation_id = uuid::Uuid::now_v7();
        let request_id = uuid::Uuid::now_v7();
        let requester_id = uuid::Uuid::now_v7();
        let text = "x".repeat(kodosi_domain::session::REMOTE_SEMANTIC_TEXT_MAX_BYTES);
        let spec = SessionRelayClientSpec {
            id: SessionId::new(),
            backend_session_id: session_id.to_string(),
            backend_incarnation_id: incarnation_id,
            cursor: ReplayCursor::default(),
            relay_mode: RemoteRelayMode::OwnerParticipant,
            session_key: Some(session_key),
            viewer_kem_secret_bytes: None,
            viewer_device_id: Some("d".repeat(256)),
            viewer_user_id: Some(requester_id.to_string()),
            viewer_signing_pkcs8: Some(pkcs8.as_ref().to_vec()),
            owner_user_id: Some(requester_id.to_string()),
        };
        let command = SessionRelayCommand::SemanticSend {
            request_id,
            incarnation_id,
            mode: crate::session_relay::wire::RelaySemanticMode::StopAndSend,
            payload_sha256: crypto::sha256_hex(text.as_bytes()),
            text,
        };

        let wire = build_semantic_command_payload(&command, &spec, Some(&session_key))
            .expect("maximum semantic payload should seal");
        validate_outbound_command_size(&wire)
            .expect("maximum semantic plaintext must fit relay authority");
    }

    #[test]
    fn one_mebibyte_input_chunk_fits_the_normative_encrypted_envelope() {
        let keypair = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING)
            .unwrap_or_else(|error| panic!("test key should generate: {error}"));
        let pkcs8 = keypair
            .to_pkcs8v1()
            .unwrap_or_else(|error| panic!("test key should export: {error}"));
        let session_key = [7_u8; 32];
        let spec = SessionRelayClientSpec {
            id: SessionId::new(),
            backend_session_id: "backend-session".to_owned(),
            backend_incarnation_id: uuid::Uuid::nil(),
            cursor: ReplayCursor::default(),
            relay_mode: RemoteRelayMode::OwnerParticipant,
            session_key: Some(session_key),
            viewer_kem_secret_bytes: None,
            viewer_device_id: Some("viewer-device".to_owned()),
            viewer_user_id: Some("viewer".to_owned()),
            viewer_signing_pkcs8: Some(pkcs8.as_ref().to_vec()),
            owner_user_id: Some("owner".to_owned()),
        };
        let action_id = uuid::Uuid::nil().simple().to_string();
        let plaintext = vec![0xff; TERMINAL_INPUT_CHUNK_BYTES];
        let sealed = seal_control(Some(&session_key), &spec, "inject", &action_id, &plaintext)
            .expect("maximum input chunk should seal");
        let wire = participant_inject_message(&action_id, &sealed.0, &sealed.1, &sealed.2);

        validate_outbound_command_size(&wire)
            .expect("maximum input chunk should fit participant.inject authority");
        let serialized = serde_json::to_vec(&wire).expect("wire should serialize");
        assert!(serialized.len() <= 2 * 1024 * 1024);
    }

    #[test]
    fn oversized_outbound_control_is_rejected_before_websocket_send() {
        let wire = serde_json::json!({
            "type": "participant.suggest",
            "padding": "x".repeat(16 * 1024),
        });
        let error = validate_outbound_command_size(&wire)
            .expect_err("control larger than its manifest cap must fail locally");
        assert!(error.to_string().contains("exceeding the 16384-byte limit"));
    }

    #[test]
    fn remote_owner_input_chunks_preserve_large_binary_payload_in_order() {
        let keypair = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING)
            .unwrap_or_else(|error| panic!("test key should generate: {error}"));
        let pkcs8 = keypair
            .to_pkcs8v1()
            .unwrap_or_else(|error| panic!("test key should export: {error}"));
        let session_key = [7_u8; 32];
        let spec = SessionRelayClientSpec {
            id: SessionId::new(),
            backend_session_id: "backend-session".to_owned(),
            backend_incarnation_id: uuid::Uuid::nil(),
            cursor: ReplayCursor::default(),
            relay_mode: RemoteRelayMode::OwnerParticipant,
            session_key: Some(session_key),
            viewer_kem_secret_bytes: None,
            viewer_device_id: Some("viewer-device".to_owned()),
            viewer_user_id: Some("owner".to_owned()),
            viewer_signing_pkcs8: Some(pkcs8.as_ref().to_vec()),
            owner_user_id: Some("owner".to_owned()),
        };
        let payload = (0..(TERMINAL_INPUT_CHUNK_BYTES * 2 + 17))
            .map(|index| index.to_le_bytes()[0])
            .collect::<Vec<_>>();

        let messages = build_input_chunk_payloads(&spec, Some(&session_key), &payload)
            .expect("large input should split into bounded messages");
        assert_eq!(messages.len(), 3);
        let mut plaintext = Vec::new();
        for wire in messages {
            validate_outbound_command_size(&wire).expect("chunk must fit manifest cap");
            let action_id = wire["actionId"].as_str().expect("chunk action id");
            let nonce = BASE64
                .decode(wire["nonce"].as_str().expect("chunk nonce"))
                .expect("nonce base64");
            let ciphertext = BASE64
                .decode(wire["ciphertext"].as_str().expect("chunk ciphertext"))
                .expect("ciphertext base64");
            let aad =
                crypto::control_associated_data("inject", &spec.backend_session_id, action_id)
                    .expect("chunk AAD");
            plaintext.extend(
                crypto::decrypt_control_payload(&session_key, &aad, &nonce, &ciphertext)
                    .expect("chunk decrypt"),
            );
        }
        assert_eq!(plaintext, payload);
    }

    #[test]
    fn participant_command_seals_every_byte_without_text_conversion() {
        let keypair = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING)
            .unwrap_or_else(|error| panic!("test key should generate: {error}"));
        let pkcs8 = keypair
            .to_pkcs8v1()
            .unwrap_or_else(|error| panic!("test key should export: {error}"));
        let session_key = [7_u8; 32];
        let spec = SessionRelayClientSpec {
            id: SessionId::new(),
            backend_session_id: "backend-session".to_owned(),
            backend_incarnation_id: uuid::Uuid::nil(),
            cursor: ReplayCursor::default(),
            relay_mode: RemoteRelayMode::SharedParticipant,
            session_key: Some(session_key),
            viewer_kem_secret_bytes: None,
            viewer_device_id: Some("viewer-device".to_owned()),
            viewer_user_id: Some("viewer".to_owned()),
            viewer_signing_pkcs8: Some(pkcs8.as_ref().to_vec()),
            owner_user_id: Some("owner".to_owned()),
        };
        let payload = (0_u8..=u8::MAX).collect::<Vec<_>>();
        let command = SessionRelayCommand::Inject {
            action_id: "action-binary".to_owned(),
            payload: payload.clone(),
        };

        let wire = build_remote_command_payload(&command, &spec, Some(&session_key))
            .unwrap_or_else(|error| panic!("binary input should seal: {error}"));
        let action_id = wire["actionId"]
            .as_str()
            .expect("wire input should carry an action id");
        let nonce = wire["nonce"]
            .as_str()
            .and_then(|value| BASE64.decode(value).ok())
            .expect("wire nonce should be base64");
        let ciphertext = wire["ciphertext"]
            .as_str()
            .and_then(|value| BASE64.decode(value).ok())
            .expect("wire ciphertext should be base64");
        let aad = crypto::control_associated_data("inject", &spec.backend_session_id, action_id)
            .expect("input AAD should build");
        let plaintext = crypto::decrypt_control_payload(&session_key, &aad, &nonce, &ciphertext)
            .expect("wire payload should decrypt");

        assert_eq!(wire["type"], "participant.inject");
        assert_eq!(plaintext, payload);
    }

    #[tokio::test]
    async fn participant_acceptance_is_required_before_application_messages() {
        let session_id = SessionId::new();
        let (session_events_tx, mut session_events_rx) = mpsc::channel(2);
        let events =
            SessionRelayEventSink::new(session_events_tx, session_id, CancellationToken::new());
        let mut cursor = ReplayCursor::default();
        let mut participant_accepted = false;
        let status =
            format!(r#"{{"type":"session.status","sessionId":"{session_id}","status":"Live"}}"#);

        let error = handle_text_message(
            RemoteRelayMode::SharedParticipant,
            &uuid::Uuid::nil(),
            &status,
            &events,
            &mut cursor,
            &mut participant_accepted,
        )
        .await
        .expect_err("application data before participant.accepted must fail closed");

        assert!(error.to_string().contains("before participant acceptance"));
        assert!(!participant_accepted);
        assert!(session_events_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn validated_participant_acceptance_opens_the_receive_gate_once() {
        let session_id = SessionId::new();
        let (session_events_tx, mut session_events_rx) = mpsc::channel(3);
        let events =
            SessionRelayEventSink::new(session_events_tx, session_id, CancellationToken::new());
        let mut cursor = ReplayCursor::default();
        let mut participant_accepted = false;
        let accepted = format!(
            r#"{{"type":"participant.accepted","sessionId":"{session_id}","access":"View","capabilities":1,"incarnationId":"00000000-0000-0000-0000-000000000000","incarnationGeneration":1,"relayProtocolVersion":10}}"#
        );

        let outcome = handle_text_message(
            RemoteRelayMode::SharedParticipant,
            &uuid::Uuid::nil(),
            &accepted,
            &events,
            &mut cursor,
            &mut participant_accepted,
        )
        .await
        .expect("valid acceptance should open the receive gate");

        assert!(outcome.is_none());
        assert!(participant_accepted);
        assert!(participant_accepted);
        std::assert_matches!(
            session_events_rx.recv().await,
            Some(SessionRelayEvent::RemoteAccessChanged { id, .. }) if id == session_id
        );
        std::assert_matches!(
            session_events_rx.recv().await,
            Some(SessionRelayEvent::RemoteSessionConnectionChanged {
                id,
                status: ConnectionState::Connected,
                reason: None,
            }) if id == session_id
        );

        let error = handle_text_message(
            RemoteRelayMode::SharedParticipant,
            &uuid::Uuid::nil(),
            &accepted,
            &events,
            &mut cursor,
            &mut participant_accepted,
        )
        .await
        .expect_err("duplicate participant.accepted must fail closed");
        assert!(
            error
                .to_string()
                .contains("duplicate participant acceptance")
        );
        assert!(session_events_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn encrypted_frame_before_participant_acceptance_is_protocol_drift() {
        let session_id = SessionId::new();
        let (session_events_tx, _session_events_rx) = mpsc::channel(1);
        let events =
            SessionRelayEventSink::new(session_events_tx, session_id, CancellationToken::new());
        let spec = SessionRelayClientSpec {
            id: session_id,
            backend_session_id: "backend-session".to_owned(),
            backend_incarnation_id: uuid::Uuid::nil(),
            cursor: ReplayCursor::default(),
            relay_mode: RemoteRelayMode::SharedParticipant,
            session_key: None,
            viewer_kem_secret_bytes: None,
            viewer_device_id: None,
            viewer_user_id: None,
            viewer_signing_pkcs8: None,
            owner_user_id: None,
        };
        let mut cursor = ReplayCursor::default();
        let mut encrypted_frame_state = EncryptedFrameState {
            encrypted_frames_without_key: 0,
            key_miss_logged: false,
            retry_missing_session_key: false,
        };
        let participant_accepted = false;

        let outcome = handle_incoming_binary(
            vec![0x01].into(),
            &spec,
            None,
            &events,
            &mut cursor,
            &mut encrypted_frame_state,
            participant_accepted,
        )
        .await
        .expect("pre-accept frame should map to a terminal outcome");

        std::assert_matches!(
            outcome,
            Some(ReceiveLoopOutcome::ProtocolDrift { reason })
                if reason.contains("before participant acceptance")
        );
    }

    #[tokio::test]
    async fn encrypted_frame_without_non_retryable_session_key_is_protocol_drift() {
        let session_id = SessionId::new();
        let (session_events_tx, _session_events_rx) = mpsc::channel(1);
        let events =
            SessionRelayEventSink::new(session_events_tx, session_id, CancellationToken::new());
        let spec = SessionRelayClientSpec {
            id: session_id,
            backend_session_id: "backend-session".to_owned(),
            backend_incarnation_id: uuid::Uuid::from_u128(2),
            cursor: ReplayCursor::default(),
            relay_mode: RemoteRelayMode::SharedParticipant,
            session_key: None,
            viewer_kem_secret_bytes: None,
            viewer_device_id: None,
            viewer_user_id: None,
            viewer_signing_pkcs8: None,
            owner_user_id: None,
        };
        let mut cursor = ReplayCursor::default();
        let mut encrypted_frame_state = EncryptedFrameState {
            encrypted_frames_without_key: 0,
            key_miss_logged: false,
            retry_missing_session_key: false,
        };

        let outcome = handle_binary_message(
            &spec,
            &[0x01],
            None,
            &events,
            &mut cursor,
            &mut encrypted_frame_state,
        )
        .await
        .expect("non-retryable key absence should be a terminal relay outcome");

        std::assert_matches!(
            outcome,
            Some(ReceiveLoopOutcome::ProtocolDrift { reason })
                if reason.contains("usable session key")
        );
    }

    #[tokio::test]
    async fn malformed_known_encrypted_frame_is_terminal_protocol_drift() {
        let session_id = SessionId::new();
        let (session_events_tx, _session_events_rx) = mpsc::channel(1);
        let events =
            SessionRelayEventSink::new(session_events_tx, session_id, CancellationToken::new());
        let spec = SessionRelayClientSpec {
            id: session_id,
            backend_session_id: "backend-session".to_owned(),
            backend_incarnation_id: uuid::Uuid::from_u128(2),
            cursor: ReplayCursor::default(),
            relay_mode: RemoteRelayMode::SharedParticipant,
            session_key: None,
            viewer_kem_secret_bytes: None,
            viewer_device_id: None,
            viewer_user_id: None,
            viewer_signing_pkcs8: None,
            owner_user_id: None,
        };
        let mut cursor = ReplayCursor::default();
        let mut encrypted_frame_state = EncryptedFrameState {
            encrypted_frames_without_key: 0,
            key_miss_logged: false,
            retry_missing_session_key: false,
        };
        let session_key = [3_u8; 32];

        let outcome = handle_binary_message(
            &spec,
            &[0x03],
            Some(&session_key),
            &events,
            &mut cursor,
            &mut encrypted_frame_state,
        )
        .await
        .expect("malformed known frame maps to an explicit outcome");

        std::assert_matches!(
            outcome,
            Some(ReceiveLoopOutcome::ProtocolDrift { reason })
                if reason.contains("too short")
        );
    }

    #[test]
    fn runtime_recovery_close_reconnects_with_cursor_intact() {
        let cursor = ReplayCursor {
            checkpoint_revision: 7,
            presentation_revision: 9,
            next_sequence: Some(12),
            key_generation: 3,
            ..ReplayCursor::default()
        };

        std::assert_matches!(
            receive_outcome_for_close(CloseBehavior::Retryable, cursor),
            ReceiveLoopOutcome::Reconnect {
                cursor: next_cursor,
                force_refresh: false,
            } if next_cursor == cursor
        );
    }

    #[test]
    fn terminal_replay_gap_preserves_cursor_for_one_shot_reset() {
        let cursor = ReplayCursor {
            checkpoint_revision: 7,
            presentation_revision: 9,
            next_sequence: Some(12),
            key_generation: 3,
            ..ReplayCursor::default()
        };

        std::assert_matches!(
            receive_outcome_for_close(CloseBehavior::TerminalReplayGap, cursor),
            ReceiveLoopOutcome::TerminalReplayGap { cursor: preserved }
                if preserved == cursor
        );
    }

    #[test]
    fn retryable_closing_close_preserves_cursor_for_reconnect() {
        use tokio_tungstenite::tungstenite::protocol::{CloseFrame, frame::coding::CloseCode};

        let cursor = ReplayCursor {
            key_generation: 3,
            ..ReplayCursor::default()
        };
        let close = CloseFrame {
            code: CloseCode::Normal,
            reason: "closing".into(),
        };
        std::assert_matches!(
            receive_outcome_for_close(close_reasons::interpret(Some(&close)), cursor),
            ReceiveLoopOutcome::Reconnect {
                cursor: reconnected,
                force_refresh: false,
            } if reconnected == cursor
        );
    }

    #[test]
    fn reported_permission_action_retains_tool_request_id() {
        let action = reported_action(&SessionRelayCommand::PermissionDecision {
            action_id: "action-1".to_owned(),
            request_id: "tool-use-1".to_owned(),
            request_generation: 7,
            decision: "allow".to_owned(),
        })
        .expect("permission decisions report transport outcomes");

        assert_eq!(action.action_id, "action-1");
        assert_eq!(action.request_id.as_deref(), Some("tool-use-1"));
        assert_eq!(action.request_generation, Some(7));
    }
}
