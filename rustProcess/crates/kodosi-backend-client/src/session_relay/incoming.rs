use crate::{
    BackendClientError, Result, crypto,
    relay::{
        CHECKPOINT_ENCRYPTED_FRAME_MAX_BYTES, PENDING_PERMISSIONS_ENCRYPTED_FRAME_MAX_BYTES,
        PENDING_PERMISSIONS_FRAME_TYPE, raw_batch_encrypted_frame_max_bytes,
    },
    relay_wire::{KEY_ROTATION_TYPE, KeyRotationMessage},
    session_relay_authority::{
        participant_text_envelope_max_bytes, participant_text_message_limit,
    },
    terminal_wire,
};
use kodosi_domain::{
    lifecycle::{ConnectionState, RemoteSessionAccessState},
    terminal::TERMINAL_PRESENTATION_ENCRYPTED_FRAME_MAX_BYTES,
};

use super::{
    RemoteRelayMode,
    cursor::ReplayCursor,
    events::SessionRelayEventSink,
    wire::{
        ActionResultMessage, ParticipantAcceptedMessage, ParticipantSemanticReceiptMessage,
        SessionAccessRevokedMessage, SessionEndedMessage, SessionRelayEnvelope,
        SessionStatusMessage,
    },
};

const ACCESS_REVOKED_REASON: &str = "Access revoked";
const SESSION_KEY_ROTATION_REASON: &str = "Refreshing session key";
pub(super) const PARTICIPANT_ACCEPTED_TYPE: &str = "participant.accepted";
const ACTION_RESULT_TYPE: &str = "action.result";
const SEMANTIC_RECEIPT_TYPE: &str = "participant.semanticReceipt";
const SESSION_STATUS_TYPE: &str = "session.status";
const SESSION_ENDED_TYPE: &str = "session.ended";
const ACCESS_REVOKED_TYPE: &str = "session.accessRevoked";

#[derive(Debug)]
pub enum MessageOutcome {
    ParticipantAccepted,
    Continue,
    SemanticReceipt(ParticipantSemanticReceiptMessage),
    Reconnect,
    AccessRevoked,
    Ended { reason: String },
}

#[derive(Debug, thiserror::Error)]
pub enum EncryptedFrameError {
    #[error("Encrypted viewer frame is too short.")]
    FrameTooShort,
    #[error("Encrypted viewer frame exceeds the {maximum} byte limit ({actual} bytes).")]
    FrameTooLarge { actual: usize, maximum: usize },
    #[error("Encrypted frame key generation was not accepted by key.rotation.")]
    KeyGenerationMismatch,
    #[error("Encrypted frame counter is duplicate or regressive.")]
    CounterNotMonotonic,
    #[error("Encrypted terminal frame revision is duplicate or regressive.")]
    RevisionNotMonotonic,
    #[error("Encrypted terminal sequence has a gap, duplicate, or overlap.")]
    TerminalSequenceMismatch,
    #[error(
        "Encrypted session frame type 0x{frame_type:02x} is unsupported by the current relay protocol."
    )]
    UnsupportedFrameType { frame_type: u8 },
    #[error("Unable to decrypt the encrypted session stream.")]
    DecryptFailed {
        #[source]
        source: BackendClientError,
    },
    #[error("Unable to decode the encrypted session stream.")]
    DecodeFailed { reason: String },
    #[error("Encrypted terminal checkpoint failed runtime application.")]
    CheckpointApplicationRejected,
    #[error("Encrypted pending-permissions snapshot targets another session incarnation.")]
    PendingPermissionsTargetMismatch,
}

fn validate_participant_text_message_size(message: &str) -> Result<()> {
    let envelope_limit = participant_text_envelope_max_bytes();
    if message.len() > envelope_limit {
        return Err(BackendClientError::Protocol {
            reason: format!(
                "participant relay text message exceeds envelope limit ({}/{envelope_limit} bytes)",
                message.len()
            ),
        });
    }
    let envelope: SessionRelayEnvelope = serde_json::from_str(message)?;
    let maximum = participant_text_message_limit(&envelope.message_type).ok_or_else(|| {
        BackendClientError::Protocol {
            reason: format!(
                "unknown session relay message type `{}`",
                envelope.message_type
            ),
        }
    })?;
    if message.len() > maximum {
        return Err(BackendClientError::Protocol {
            reason: format!(
                "session relay message `{}` exceeds its {maximum} byte limit ({} bytes)",
                envelope.message_type,
                message.len()
            ),
        });
    }
    Ok(())
}

pub(super) async fn handle_message_for_incarnation_with_acceptance(
    relay_mode: RemoteRelayMode,
    expected_incarnation_id: &uuid::Uuid,
    message: &str,
    events: &SessionRelayEventSink,
    cursor: &mut ReplayCursor,
    participant_accepted: bool,
) -> Result<MessageOutcome> {
    validate_participant_text_message_size(message)?;
    let envelope: SessionRelayEnvelope = serde_json::from_str(message)?;
    match (participant_accepted, envelope.message_type.as_str()) {
        (false, PARTICIPANT_ACCEPTED_TYPE) | (true, _) => {}
        (false, _) => {
            return Err(BackendClientError::Protocol {
                reason: "application message arrived before participant acceptance".to_owned(),
            });
        }
    }
    if participant_accepted && envelope.message_type == PARTICIPANT_ACCEPTED_TYPE {
        return Err(BackendClientError::Protocol {
            reason: "duplicate participant acceptance".to_owned(),
        });
    }
    handle_message_for_incarnation(relay_mode, expected_incarnation_id, message, events, cursor)
        .await
}

async fn handle_message_for_incarnation(
    relay_mode: RemoteRelayMode,
    expected_incarnation_id: &uuid::Uuid,
    message: &str,
    events: &SessionRelayEventSink,
    cursor: &mut ReplayCursor,
) -> Result<MessageOutcome> {
    let envelope: SessionRelayEnvelope = match serde_json::from_str(message) {
        Ok(envelope) => envelope,
        Err(error) => {
            tracing::warn!(%error, "malformed session relay envelope");
            return Err(error.into());
        }
    };
    tracing::trace!(
        message_type = %envelope.message_type,
        relay_mode = ?relay_mode,
        "session relay message received"
    );
    match envelope.message_type.as_str() {
        PARTICIPANT_ACCEPTED_TYPE => {
            let accepted: ParticipantAcceptedMessage = serde_json::from_str(message)?;
            ensure_current_session(PARTICIPANT_ACCEPTED_TYPE, &accepted.session_id, events)?;
            accepted.validate_incarnation(expected_incarnation_id)?;
            if relay_mode == RemoteRelayMode::SharedParticipant {
                events.access(accepted.validated_access()?).await;
            }
            Ok(MessageOutcome::ParticipantAccepted)
        }
        ACTION_RESULT_TYPE => {
            let result: ActionResultMessage = serde_json::from_str(message)?;
            ensure_current_session(ACTION_RESULT_TYPE, &result.session_id, events)?;
            let status = result.remote_status();
            events
                .action_result(
                    result.action_id,
                    result.request_id,
                    result.request_generation,
                    status,
                )
                .await;
            Ok(MessageOutcome::Continue)
        }
        SEMANTIC_RECEIPT_TYPE => {
            let receipt: ParticipantSemanticReceiptMessage = serde_json::from_str(message)?;
            ensure_current_session(SEMANTIC_RECEIPT_TYPE, &receipt.session_id, events)?;
            if receipt.incarnation_id != *expected_incarnation_id {
                return Err(BackendClientError::Protocol {
                    reason: "semantic receipt targeted a different session incarnation".to_owned(),
                });
            }
            Ok(MessageOutcome::SemanticReceipt(receipt))
        }
        SESSION_STATUS_TYPE => {
            let status: SessionStatusMessage = serde_json::from_str(message)?;
            ensure_current_session(SESSION_STATUS_TYPE, &status.session_id, events)?;
            events.state(status.status.into_session_state()).await;
            Ok(MessageOutcome::Continue)
        }
        KEY_ROTATION_TYPE => handle_key_rotation_payload(message, events, cursor).await,
        ACCESS_REVOKED_TYPE => {
            let revoked: SessionAccessRevokedMessage = serde_json::from_str(message)?;
            handle_access_revoked_message(revoked, events).await
        }
        SESSION_ENDED_TYPE => {
            let ended: SessionEndedMessage = serde_json::from_str(message)?;
            ensure_current_session(SESSION_ENDED_TYPE, &ended.session_id, events)?;
            Ok(MessageOutcome::Ended {
                reason: ended.reason,
            })
        }
        unknown_type => {
            tracing::warn!(
                message_type = unknown_type,
                "unknown session relay message type"
            );
            Err(BackendClientError::Protocol {
                reason: format!("unknown session relay message type `{unknown_type}`"),
            })
        }
    }
}

async fn handle_key_rotation_payload(
    message: &str,
    events: &SessionRelayEventSink,
    cursor: &mut ReplayCursor,
) -> Result<MessageOutcome> {
    let rotation: KeyRotationMessage<'_> = serde_json::from_str(message)?;
    ensure_current_session(KEY_ROTATION_TYPE, rotation.session_id.as_ref(), events)?;
    handle_key_rotation_message(rotation.key_generation, events, cursor).await
}

async fn handle_key_rotation_message(
    key_generation: u32,
    events: &SessionRelayEventSink,
    cursor: &mut ReplayCursor,
) -> Result<MessageOutcome> {
    if !cursor.observe_key_generation(key_generation) {
        return Err(BackendClientError::Protocol {
            reason: format!(
                "key.rotation generation {key_generation} does not advance accepted generation {}",
                cursor.key_generation
            ),
        });
    }
    tracing::info!(
        session_id = events.id().to_string(),
        key_generation,
        "session key rotated — triggering reconnect to fetch new key",
    );
    events
        .access_state(
            RemoteSessionAccessState::AwaitingKey,
            Some(SESSION_KEY_ROTATION_REASON.to_owned()),
            None,
        )
        .await;
    events
        .connection(
            ConnectionState::Reconnecting,
            Some(SESSION_KEY_ROTATION_REASON.to_owned()),
        )
        .await;
    Ok(MessageOutcome::Reconnect)
}

async fn handle_access_revoked_message(
    revoked: SessionAccessRevokedMessage,
    events: &SessionRelayEventSink,
) -> Result<MessageOutcome> {
    ensure_current_session(ACCESS_REVOKED_TYPE, &revoked.session_id, events)?;
    events.access_revoked().await;
    events
        .access_state(
            RemoteSessionAccessState::AccessDenied,
            Some(ACCESS_REVOKED_REASON.to_owned()),
            None,
        )
        .await;
    events
        .connection(
            ConnectionState::Offline,
            Some(ACCESS_REVOKED_REASON.to_owned()),
        )
        .await;
    Ok(MessageOutcome::AccessRevoked)
}

fn ensure_current_session(
    message_type: &str,
    session_id: &str,
    events: &SessionRelayEventSink,
) -> Result<()> {
    if session_id.eq_ignore_ascii_case(&events.id().to_string()) {
        return Ok(());
    }

    Err(BackendClientError::Protocol {
        reason: format!("{message_type} targeted a different session"),
    })
}

pub async fn handle_encrypted_frame(
    data: &[u8],
    session_key: &crypto::SessionKey,
    expected_incarnation_id: uuid::Uuid,
    events: &SessionRelayEventSink,
    cursor: &mut ReplayCursor,
) -> std::result::Result<(), EncryptedFrameError> {
    let maximum = match data.first() {
        Some(0x03) => CHECKPOINT_ENCRYPTED_FRAME_MAX_BYTES,
        Some(0x04) => raw_batch_encrypted_frame_max_bytes(),
        Some(0x05) => TERMINAL_PRESENTATION_ENCRYPTED_FRAME_MAX_BYTES,
        Some(&PENDING_PERMISSIONS_FRAME_TYPE) => PENDING_PERMISSIONS_ENCRYPTED_FRAME_MAX_BYTES,
        Some(frame_type) => {
            return Err(EncryptedFrameError::UnsupportedFrameType {
                frame_type: *frame_type,
            });
        }
        None => return Err(EncryptedFrameError::FrameTooShort),
    };
    if data.len() > maximum {
        return Err(EncryptedFrameError::FrameTooLarge {
            actual: data.len(),
            maximum,
        });
    }
    match data[0] {
        0x03 => handle_checkpoint_frame(data, session_key, events, cursor).await,
        0x04 => handle_raw_batch_frame(data, session_key, events, cursor).await,
        0x05 => handle_presentation_frame(data, session_key, events, cursor).await,
        PENDING_PERMISSIONS_FRAME_TYPE => {
            handle_pending_permissions_frame(
                data,
                session_key,
                expected_incarnation_id,
                events,
                cursor,
            )
            .await
        }
        _ => unreachable!("frame type was validated above"),
    }
}

async fn handle_checkpoint_frame(
    data: &[u8],
    session_key: &crypto::SessionKey,
    events: &SessionRelayEventSink,
    cursor: &mut ReplayCursor,
) -> std::result::Result<(), EncryptedFrameError> {
    if data.len() < 29 {
        return Err(EncryptedFrameError::FrameTooShort);
    }
    let key_generation = read_u32(data, 1);
    let counter = read_u64(data, 5);
    let checkpoint_revision = read_u64(data, 13);
    let next_sequence = read_u64(data, 21);
    if key_generation != cursor.key_generation {
        return Err(EncryptedFrameError::KeyGenerationMismatch);
    }
    if !cursor.accepts_checkpoint_frame(checkpoint_revision, next_sequence, key_generation, counter)
    {
        return Err(if checkpoint_revision <= cursor.checkpoint_revision {
            EncryptedFrameError::RevisionNotMonotonic
        } else if cursor
            .next_sequence
            .is_some_and(|accepted| next_sequence < accepted)
        {
            EncryptedFrameError::TerminalSequenceMismatch
        } else {
            EncryptedFrameError::CounterNotMonotonic
        });
    }
    let key = derive_frame_key(session_key, crypto::RelayStream::Checkpoint)?;
    let plaintext = decrypt_payload(&key, key_generation, counter, data, 29)?;
    let checkpoint = terminal_wire::decode_checkpoint(&plaintext).map_err(decode_error)?;
    if !events.checkpoint(next_sequence, checkpoint).await {
        return Err(EncryptedFrameError::CheckpointApplicationRejected);
    }
    cursor.observe_checkpoint_frame(checkpoint_revision, counter, next_sequence);
    Ok(())
}

async fn handle_raw_batch_frame(
    data: &[u8],
    session_key: &crypto::SessionKey,
    events: &SessionRelayEventSink,
    cursor: &mut ReplayCursor,
) -> std::result::Result<(), EncryptedFrameError> {
    if data.len() < 29 {
        return Err(EncryptedFrameError::FrameTooShort);
    }
    let key_generation = read_u32(data, 1);
    let counter = read_u64(data, 5);
    let first_sequence = read_u64(data, 13);
    let next_sequence = read_u64(data, 21);
    if key_generation != cursor.key_generation {
        return Err(EncryptedFrameError::KeyGenerationMismatch);
    }
    if !cursor.accepts_raw_batch(first_sequence, next_sequence, key_generation, counter) {
        return Err(
            if cursor
                .raw_counter
                .is_some_and(|accepted| counter <= accepted)
            {
                EncryptedFrameError::CounterNotMonotonic
            } else {
                EncryptedFrameError::TerminalSequenceMismatch
            },
        );
    }
    let key = derive_frame_key(session_key, crypto::RelayStream::TerminalRaw)?;
    let plaintext = decrypt_payload(&key, key_generation, counter, data, 29)?;
    let chunks = terminal_wire::decode_raw_chunks(&plaintext, first_sequence, next_sequence)
        .map_err(decode_error)?;
    cursor.observe_raw_batch(counter, next_sequence);
    events
        .raw_batch(first_sequence, next_sequence, chunks)
        .await;
    Ok(())
}

async fn handle_presentation_frame(
    data: &[u8],
    session_key: &crypto::SessionKey,
    events: &SessionRelayEventSink,
    cursor: &mut ReplayCursor,
) -> std::result::Result<(), EncryptedFrameError> {
    if data.len() < 21 {
        return Err(EncryptedFrameError::FrameTooShort);
    }
    let key_generation = read_u32(data, 1);
    let counter = read_u64(data, 5);
    let presentation_revision = read_u64(data, 13);
    if key_generation != cursor.key_generation {
        return Err(EncryptedFrameError::KeyGenerationMismatch);
    }
    if !cursor.accepts_presentation_frame(presentation_revision, key_generation, counter) {
        return Err(if presentation_revision <= cursor.presentation_revision {
            EncryptedFrameError::RevisionNotMonotonic
        } else {
            EncryptedFrameError::CounterNotMonotonic
        });
    }
    let key = derive_frame_key(session_key, crypto::RelayStream::TerminalPresentation)?;
    let plaintext = decrypt_payload(&key, key_generation, counter, data, 21)?;
    let presentation = terminal_wire::decode_presentation(&plaintext).map_err(decode_error)?;
    cursor.observe_presentation_frame(presentation_revision, counter);
    events.plain_presentation(presentation).await;
    Ok(())
}

fn derive_frame_key(
    session_key: &crypto::SessionKey,
    stream: crypto::RelayStream,
) -> std::result::Result<crypto::SessionKey, EncryptedFrameError> {
    crypto::derive_stream_key(session_key, stream)
        .map_err(|source| EncryptedFrameError::DecryptFailed { source })
}

fn decrypt_payload(
    key: &crypto::SessionKey,
    key_generation: u32,
    counter: u64,
    data: &[u8],
    header_len: usize,
) -> std::result::Result<Vec<u8>, EncryptedFrameError> {
    crypto::decrypt_frame(
        key,
        key_generation,
        counter,
        &data[..header_len],
        &data[header_len..],
    )
    .map_err(|source| EncryptedFrameError::DecryptFailed { source })
}

fn read_u32(data: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

fn read_u64(data: &[u8], offset: usize) -> u64 {
    u64::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
        data[offset + 4],
        data[offset + 5],
        data[offset + 6],
        data[offset + 7],
    ])
}

fn decode_error(error: impl std::fmt::Display) -> EncryptedFrameError {
    EncryptedFrameError::DecodeFailed {
        reason: error.to_string(),
    }
}

async fn handle_pending_permissions_frame(
    data: &[u8],
    session_key: &crypto::SessionKey,
    expected_incarnation_id: uuid::Uuid,
    events: &SessionRelayEventSink,
    cursor: &mut ReplayCursor,
) -> std::result::Result<(), EncryptedFrameError> {
    const HEADER_BYTES: usize = 53;
    if data.len() <= HEADER_BYTES {
        return Err(EncryptedFrameError::FrameTooShort);
    }
    let key_generation = read_u32(data, 1);
    let counter = read_u64(data, 5);
    let generation = read_u64(data, 13);
    if key_generation != cursor.key_generation {
        return Err(EncryptedFrameError::KeyGenerationMismatch);
    }
    if cursor
        .pending_permissions_counter
        .is_some_and(|accepted| counter <= accepted)
    {
        return Err(EncryptedFrameError::CounterNotMonotonic);
    }
    let session_id =
        uuid::Uuid::from_slice(&data[21..37]).map_err(|_| EncryptedFrameError::FrameTooShort)?;
    let incarnation_id =
        uuid::Uuid::from_slice(&data[37..53]).map_err(|_| EncryptedFrameError::FrameTooShort)?;
    let expected_session_id = uuid::Uuid::parse_str(&events.id().to_string())
        .map_err(|_| EncryptedFrameError::PendingPermissionsTargetMismatch)?;
    if session_id != expected_session_id || incarnation_id != expected_incarnation_id {
        return Err(EncryptedFrameError::PendingPermissionsTargetMismatch);
    }
    let key = derive_frame_key(session_key, crypto::RelayStream::PendingPermissions)?;
    let plaintext = decrypt_payload(&key, key_generation, counter, data, HEADER_BYTES)?;
    let snapshot: serde_json::Value =
        serde_json::from_slice(&plaintext).map_err(|source| EncryptedFrameError::DecodeFailed {
            reason: source.to_string(),
        })?;
    if snapshot
        .get("generation")
        .and_then(serde_json::Value::as_u64)
        != Some(generation)
    {
        return Err(EncryptedFrameError::DecodeFailed {
            reason: "pending-permissions payload generation does not match its envelope".to_owned(),
        });
    }
    if generation <= cursor.pending_permissions_generation {
        cursor.pending_permissions_counter = Some(counter);
        return Ok(());
    }
    events
        .pending_permissions_snapshot(incarnation_id, generation, snapshot)
        .await;
    cursor.observe_pending_permissions_frame(counter, generation);
    Ok(())
}

#[cfg(test)]
mod pending_permissions_tests {
    use super::*;
    use crate::relay::{
        EncryptionState, HostRelayPendingPermissionsSnapshot, build_pending_permissions_frame,
    };
    use crate::session_relay::events::SessionRelayEvent;
    use kodosi_domain::ids::SessionId;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    fn snapshot(
        generation: u64,
        incarnation_id: uuid::Uuid,
    ) -> HostRelayPendingPermissionsSnapshot {
        HostRelayPendingPermissionsSnapshot {
            generation,
            incarnation_id,
            plaintext: serde_json::to_vec(&serde_json::json!({
                "generation": generation,
                "requests": [],
            }))
            .expect("snapshot"),
        }
    }

    #[tokio::test]
    async fn pending_snapshot_is_incarnation_bound_and_strictly_newer() {
        let session_id = SessionId::new();
        let incarnation_id = uuid::Uuid::now_v7();
        let key = [7; 32];
        let (tx, mut rx) = mpsc::channel(4);
        let events = SessionRelayEventSink::new(tx, session_id, CancellationToken::new());
        let mut cursor = ReplayCursor {
            key_generation: 1,
            ..ReplayCursor::default()
        };
        let mut encryption = EncryptionState::derive(&key, 1, 10, 20).expect("encryption");
        let first = build_pending_permissions_frame(
            &mut encryption,
            session_id,
            &snapshot(1, incarnation_id),
        )
        .expect("frame");
        handle_encrypted_frame(&first, &key, incarnation_id, &events, &mut cursor)
            .await
            .expect("current snapshot");
        std::assert_matches!(
            rx.recv().await,
            Some(SessionRelayEvent::RemotePendingPermissionsSnapshot {
                generation: 1,
                incarnation_id: received_incarnation,
                ..
            }) if received_incarnation == incarnation_id
        );

        let duplicate_generation = build_pending_permissions_frame(
            &mut encryption,
            session_id,
            &snapshot(1, incarnation_id),
        )
        .expect("fresh-counter duplicate");
        handle_encrypted_frame(
            &duplicate_generation,
            &key,
            incarnation_id,
            &events,
            &mut cursor,
        )
        .await
        .expect("authenticated stale generation is ignored");
        assert!(rx.try_recv().is_err());

        let wrong_incarnation = uuid::Uuid::now_v7();
        let wrong = build_pending_permissions_frame(
            &mut encryption,
            session_id,
            &snapshot(2, wrong_incarnation),
        )
        .expect("wrong target frame");
        assert!(matches!(
            handle_encrypted_frame(&wrong, &key, incarnation_id, &events, &mut cursor).await,
            Err(EncryptedFrameError::PendingPermissionsTargetMismatch)
        ));
    }

    #[tokio::test]
    async fn key_rotation_accepts_republished_newer_snapshot_and_resets_only_counter() {
        let session_id = SessionId::new();
        let incarnation_id = uuid::Uuid::now_v7();
        let key = [9; 32];
        let (tx, mut rx) = mpsc::channel(2);
        let events = SessionRelayEventSink::new(tx, session_id, CancellationToken::new());
        let mut cursor = ReplayCursor {
            key_generation: 1,
            pending_permissions_generation: 4,
            pending_permissions_counter: Some(99),
            ..ReplayCursor::default()
        };
        assert!(cursor.observe_key_generation(2));
        assert_eq!(cursor.pending_permissions_generation, 4);
        assert!(cursor.pending_permissions_counter.is_none());
        let mut encryption = EncryptionState::derive(&key, 2, 1, 5).expect("encryption");
        let frame = build_pending_permissions_frame(
            &mut encryption,
            session_id,
            &snapshot(5, incarnation_id),
        )
        .expect("frame");
        handle_encrypted_frame(&frame, &key, incarnation_id, &events, &mut cursor)
            .await
            .expect("rotated snapshot");
        std::assert_matches!(
            rx.recv().await,
            Some(SessionRelayEvent::RemotePendingPermissionsSnapshot { generation: 5, .. })
        );
    }

    #[tokio::test]
    async fn retired_frame_type_is_rejected_without_renumbering_terminal_frames() {
        let session_id = SessionId::new();
        let incarnation_id = uuid::Uuid::now_v7();
        let (tx, _rx) = mpsc::channel(1);
        let events = SessionRelayEventSink::new(tx, session_id, CancellationToken::new());
        let mut cursor = ReplayCursor::default();
        let error = handle_encrypted_frame(&[0x02], &[0; 32], incarnation_id, &events, &mut cursor)
            .await
            .expect_err("retired frame type");
        assert!(matches!(
            &error,
            EncryptedFrameError::UnsupportedFrameType { frame_type: 0x02 }
        ));
        assert_eq!(
            error.to_string(),
            "Encrypted session frame type 0x02 is unsupported by the current relay protocol."
        );
        for frame_type in [0x03, 0x04, 0x05] {
            assert!(matches!(
                handle_encrypted_frame(
                    &[frame_type],
                    &[0; 32],
                    incarnation_id,
                    &events,
                    &mut cursor,
                )
                .await,
                Err(EncryptedFrameError::FrameTooShort)
            ));
        }
    }
}
