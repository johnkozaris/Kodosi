use crate::{
    BackendClientError, RelayReservationKind, Result, crypto, host_ws::HostWebSocketStream,
    terminal_wire,
};
use bytes::{Bytes, BytesMut};
use kodosi_domain::terminal::{
    TERMINAL_PRESENTATION_ENCRYPTED_FRAME_MAX_BYTES, TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES,
    TerminalCheckpointV2, TerminalPresentationV2,
};

use super::{HostRelayPendingPermissionsSnapshot, HostRelaySpec, wire::send_binary};

const CHECKPOINT_FRAME_TYPE: u8 = 0x03;
const RAW_BATCH_FRAME_TYPE: u8 = 0x04;
const PRESENTATION_FRAME_TYPE: u8 = 0x05;
pub const PENDING_PERMISSIONS_FRAME_TYPE: u8 = 0x06;
const CHECKPOINT_HEADER_BYTES: usize = 1 + 4 + 8 + 8 + 8;
const RAW_BATCH_HEADER_BYTES: usize = 1 + 4 + 8 + 8 + 8;
const PRESENTATION_HEADER_BYTES: usize = 1 + 4 + 8 + 8;
const PENDING_PERMISSIONS_HEADER_BYTES: usize = 1 + 4 + 8 + 8 + 16 + 16;
const AEAD_TAG_BYTES: usize = 16;
pub const PENDING_PERMISSIONS_ENCRYPTED_FRAME_MAX_BYTES: usize = 2 * 1024 * 1024;
pub const CHECKPOINT_ENCRYPTED_FRAME_MAX_BYTES: usize = CHECKPOINT_HEADER_BYTES
    + terminal_wire::CHECKPOINT_PLAINTEXT_HEADER_BYTES
    + TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES
    + AEAD_TAG_BYTES;
pub fn raw_batch_encrypted_frame_max_bytes() -> usize {
    terminal_wire::terminal_raw_batch_max_plaintext_bytes()
        + RAW_BATCH_HEADER_BYTES
        + AEAD_TAG_BYTES
}

#[derive(Debug, Default)]
pub(super) struct LiveStreamState {
    pub required: bool,
}

impl LiveStreamState {
    pub(super) fn disable(&mut self) {
        self.required = false;
    }
}

pub struct EncryptionState {
    pub checkpoint_key: zeroize::Zeroizing<crypto::SessionKey>,
    pub terminal_raw_key: zeroize::Zeroizing<crypto::SessionKey>,
    pub terminal_presentation_key: zeroize::Zeroizing<crypto::SessionKey>,
    pub pending_permissions_key: zeroize::Zeroizing<crypto::SessionKey>,
    pub key_generation: u32,
    pub checkpoint_counter: u64,
    pub raw_counter: u64,
    pub presentation_counter: u64,
    pub pending_permissions_counter: u64,
    pub nonce_end_exclusive: u64,
}

impl EncryptionState {
    pub fn derive(
        session_key: &crypto::SessionKey,
        key_generation: u32,
        nonce_counter: u64,
        nonce_end_exclusive: u64,
    ) -> Result<Self> {
        Ok(Self {
            checkpoint_key: zeroize::Zeroizing::new(crypto::derive_stream_key(
                session_key,
                crypto::RelayStream::Checkpoint,
            )?),
            terminal_raw_key: zeroize::Zeroizing::new(crypto::derive_stream_key(
                session_key,
                crypto::RelayStream::TerminalRaw,
            )?),
            terminal_presentation_key: zeroize::Zeroizing::new(crypto::derive_stream_key(
                session_key,
                crypto::RelayStream::TerminalPresentation,
            )?),
            pending_permissions_key: zeroize::Zeroizing::new(crypto::derive_stream_key(
                session_key,
                crypto::RelayStream::PendingPermissions,
            )?),
            key_generation,
            checkpoint_counter: nonce_counter,
            raw_counter: nonce_counter,
            presentation_counter: nonce_counter,
            pending_permissions_counter: nonce_counter,
            nonce_end_exclusive,
        })
    }
}

pub(super) async fn send_checkpoint(
    stream: &mut HostWebSocketStream,
    spec: &HostRelaySpec,
    checkpoint_revision: &mut u64,
    encryption: &mut EncryptionState,
) -> Result<u64> {
    let capture = spec.port.capture_terminal_checkpoint().await?;
    let revision = take_next_revision(
        checkpoint_revision,
        spec.frame_revision_end_exclusive,
        "checkpoint_revision_block",
    )?;
    let frame = build_checkpoint_frame(
        encryption,
        revision,
        capture.next_sequence,
        &capture.checkpoint,
    )?;
    send_binary(stream, frame).await?;
    Ok(capture.next_sequence)
}

pub(super) async fn send_presentation(
    stream: &mut HostWebSocketStream,
    spec: &HostRelaySpec,
    presentation_revision: &mut u64,
    encryption: &mut EncryptionState,
) -> Result<()> {
    let capture = spec.port.capture_terminal_presentation().await?;
    let revision = take_next_revision(
        presentation_revision,
        spec.frame_revision_end_exclusive,
        "presentation_revision_block",
    )?;
    let frame = build_presentation_frame(encryption, revision, &capture.presentation)?;
    send_binary(stream, frame).await
}

pub fn build_checkpoint_frame(
    enc: &mut EncryptionState,
    checkpoint_revision: u64,
    next_sequence: u64,
    checkpoint: &TerminalCheckpointV2,
) -> Result<Bytes> {
    let plaintext = terminal_wire::encode_checkpoint(checkpoint)?;
    ensure_counter_in_reserved_range(enc.checkpoint_counter, enc.nonce_end_exclusive)?;
    let mut header = BytesMut::with_capacity(CHECKPOINT_HEADER_BYTES);
    header.extend_from_slice(&[CHECKPOINT_FRAME_TYPE]);
    header.extend_from_slice(&enc.key_generation.to_be_bytes());
    header.extend_from_slice(&enc.checkpoint_counter.to_be_bytes());
    header.extend_from_slice(&checkpoint_revision.to_be_bytes());
    header.extend_from_slice(&next_sequence.to_be_bytes());
    let ciphertext = crypto::encrypt_frame(
        &enc.checkpoint_key,
        enc.key_generation,
        enc.checkpoint_counter,
        &header,
        &plaintext,
    )?;
    let frame = finish_frame(
        &header,
        &ciphertext,
        CHECKPOINT_ENCRYPTED_FRAME_MAX_BYTES,
        "terminal_checkpoint_encrypted_frame",
    )?;
    enc.checkpoint_counter = increment_counter(enc.checkpoint_counter)?;
    Ok(frame)
}

pub fn build_raw_batch_frame(
    enc: &mut EncryptionState,
    first_sequence: u64,
    chunks: &[Bytes],
) -> Result<Bytes> {
    let chunk_count = u64::try_from(chunks.len()).map_err(|_| BackendClientError::Capacity {
        resource: "terminal_raw_batch",
        reason: "terminal raw batch chunk count is not representable".to_owned(),
    })?;
    let next_sequence =
        first_sequence
            .checked_add(chunk_count)
            .ok_or_else(|| BackendClientError::Capacity {
                resource: "terminal_sequence_space",
                reason: "terminal raw batch sequence range overflowed".to_owned(),
            })?;
    let plaintext = terminal_wire::encode_raw_chunks(chunks)?;
    ensure_counter_in_reserved_range(enc.raw_counter, enc.nonce_end_exclusive)?;
    let mut header = BytesMut::with_capacity(RAW_BATCH_HEADER_BYTES);
    header.extend_from_slice(&[RAW_BATCH_FRAME_TYPE]);
    header.extend_from_slice(&enc.key_generation.to_be_bytes());
    header.extend_from_slice(&enc.raw_counter.to_be_bytes());
    header.extend_from_slice(&first_sequence.to_be_bytes());
    header.extend_from_slice(&next_sequence.to_be_bytes());
    let ciphertext = crypto::encrypt_frame(
        &enc.terminal_raw_key,
        enc.key_generation,
        enc.raw_counter,
        &header,
        &plaintext,
    )?;
    let frame = finish_frame(
        &header,
        &ciphertext,
        raw_batch_encrypted_frame_max_bytes(),
        "terminal_raw_encrypted_frame",
    )?;
    enc.raw_counter = increment_counter(enc.raw_counter)?;
    Ok(frame)
}

pub fn build_presentation_frame(
    enc: &mut EncryptionState,
    presentation_revision: u64,
    presentation: &TerminalPresentationV2,
) -> Result<Bytes> {
    let plaintext = terminal_wire::encode_presentation(presentation)?;
    ensure_counter_in_reserved_range(enc.presentation_counter, enc.nonce_end_exclusive)?;
    let mut header = BytesMut::with_capacity(PRESENTATION_HEADER_BYTES);
    header.extend_from_slice(&[PRESENTATION_FRAME_TYPE]);
    header.extend_from_slice(&enc.key_generation.to_be_bytes());
    header.extend_from_slice(&enc.presentation_counter.to_be_bytes());
    header.extend_from_slice(&presentation_revision.to_be_bytes());
    let ciphertext = crypto::encrypt_frame(
        &enc.terminal_presentation_key,
        enc.key_generation,
        enc.presentation_counter,
        &header,
        &plaintext,
    )?;
    let frame = finish_frame(
        &header,
        &ciphertext,
        TERMINAL_PRESENTATION_ENCRYPTED_FRAME_MAX_BYTES,
        "terminal_presentation_encrypted_frame",
    )?;
    enc.presentation_counter = increment_counter(enc.presentation_counter)?;
    Ok(frame)
}

fn finish_frame(
    header: &[u8],
    ciphertext: &[u8],
    maximum: usize,
    resource: &'static str,
) -> Result<Bytes> {
    let mut frame = BytesMut::with_capacity(header.len() + ciphertext.len());
    frame.extend_from_slice(header);
    frame.extend_from_slice(ciphertext);
    if frame.len() > maximum {
        return Err(BackendClientError::Capacity {
            resource,
            reason: format!(
                "encrypted frame is {} bytes; maximum is {maximum}",
                frame.len()
            ),
        });
    }
    Ok(frame.freeze())
}

fn ensure_counter_in_reserved_range(counter: u64, end_exclusive: u64) -> Result<()> {
    if counter >= end_exclusive {
        return Err(BackendClientError::RelayReservationExhausted {
            kind: RelayReservationKind::Nonce,
        });
    }
    Ok(())
}

fn increment_counter(counter: u64) -> Result<u64> {
    counter
        .checked_add(1)
        .ok_or(BackendClientError::RelayReservationExhausted {
            kind: RelayReservationKind::Nonce,
        })
}

pub fn build_pending_permissions_frame(
    enc: &mut EncryptionState,
    session_id: kodosi_domain::ids::SessionId,
    snapshot: &HostRelayPendingPermissionsSnapshot,
) -> Result<Bytes> {
    ensure_counter_in_reserved_range(enc.pending_permissions_counter, enc.nonce_end_exclusive)?;
    let session_uuid = uuid::Uuid::parse_str(&session_id.to_string()).map_err(|error| {
        BackendClientError::Protocol {
            reason: format!("pending-permissions session ID is invalid: {error}"),
        }
    })?;
    let mut header = BytesMut::with_capacity(PENDING_PERMISSIONS_HEADER_BYTES);
    header.extend_from_slice(&[PENDING_PERMISSIONS_FRAME_TYPE]);
    header.extend_from_slice(&enc.key_generation.to_be_bytes());
    header.extend_from_slice(&enc.pending_permissions_counter.to_be_bytes());
    header.extend_from_slice(&snapshot.generation.to_be_bytes());
    header.extend_from_slice(session_uuid.as_bytes());
    header.extend_from_slice(snapshot.incarnation_id.as_bytes());
    let ciphertext = crypto::encrypt_frame(
        &enc.pending_permissions_key,
        enc.key_generation,
        enc.pending_permissions_counter,
        &header,
        &snapshot.plaintext,
    )?;
    let frame = finish_frame(
        &header,
        &ciphertext,
        PENDING_PERMISSIONS_ENCRYPTED_FRAME_MAX_BYTES,
        "pending_permissions_encrypted_frame",
    )?;
    enc.pending_permissions_counter = increment_counter(enc.pending_permissions_counter)?;
    Ok(frame)
}

fn take_next_revision(
    next_revision: &mut u64,
    end_exclusive: u64,
    _resource: &'static str,
) -> Result<u64> {
    if *next_revision >= end_exclusive {
        return Err(BackendClientError::RelayReservationExhausted {
            kind: RelayReservationKind::Revision,
        });
    }
    let revision = *next_revision;
    *next_revision =
        next_revision
            .checked_add(1)
            .ok_or(BackendClientError::RelayReservationExhausted {
                kind: RelayReservationKind::Revision,
            })?;
    Ok(revision)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodosi_domain::terminal::{TerminalScreen, TerminalSize};

    fn checkpoint() -> TerminalCheckpointV2 {
        TerminalCheckpointV2::new(
            TerminalSize::new(24, 80).expect("size"),
            TerminalScreen::Primary,
            b"checkpoint".to_vec(),
            0,
            0,
            false,
        )
        .expect("checkpoint")
    }

    #[test]
    fn v5_frames_use_exact_headers_and_independent_counters() {
        let mut enc = EncryptionState::derive(&[7; 32], 3, 10, 20).expect("derive");
        let checkpoint = build_checkpoint_frame(&mut enc, 4, 7, &checkpoint()).expect("checkpoint");
        let raw = build_raw_batch_frame(
            &mut enc,
            7,
            &[Bytes::from_static(b"a"), Bytes::from_static(b"b")],
        )
        .expect("raw");
        let presentation = TerminalPresentationV2::new(
            TerminalSize::new(24, 80).expect("size"),
            TerminalScreen::Primary,
            vec!["line".to_owned(); 24],
            0,
            0,
            false,
        )
        .expect("presentation");
        let presentation =
            build_presentation_frame(&mut enc, 9, &presentation).expect("presentation");

        assert_eq!(checkpoint[0], 0x03);
        assert_eq!(
            u64::from_be_bytes(checkpoint[13..21].try_into().expect("revision")),
            4
        );
        assert_eq!(
            u64::from_be_bytes(checkpoint[21..29].try_into().expect("sequence")),
            7
        );
        assert_eq!(raw[0], 0x04);
        assert_eq!(
            u64::from_be_bytes(raw[13..21].try_into().expect("first")),
            7
        );
        assert_eq!(u64::from_be_bytes(raw[21..29].try_into().expect("next")), 9);
        assert_eq!(presentation[0], 0x05);
        assert_eq!(
            u64::from_be_bytes(presentation[13..21].try_into().expect("revision")),
            9
        );
        assert_eq!(
            (
                enc.checkpoint_counter,
                enc.raw_counter,
                enc.presentation_counter
            ),
            (11, 11, 11)
        );
    }

    #[test]
    fn invalid_raw_batch_does_not_consume_counter() {
        let mut enc = EncryptionState::derive(&[7; 32], 3, 10, 20).expect("derive");
        assert!(build_raw_batch_frame(&mut enc, 7, &[]).is_err());
        assert_eq!(enc.raw_counter, 10);
    }

    #[test]
    fn exhausted_frame_ranges_return_typed_error_without_advancing() {
        let mut nonce = EncryptionState::derive(&[7; 32], 3, 10, 11).expect("derive");
        let session_id = kodosi_domain::ids::SessionId::new();
        let snapshot = HostRelayPendingPermissionsSnapshot {
            generation: 1,
            incarnation_id: uuid::Uuid::now_v7(),
            plaintext: br#"{"generation":1,"requests":[]}"#.to_vec(),
        };
        build_pending_permissions_frame(&mut nonce, session_id, &snapshot)
            .expect("final reserved nonce");
        let error = build_pending_permissions_frame(&mut nonce, session_id, &snapshot)
            .expect_err("next nonce must require a fresh reservation");
        assert!(matches!(
            error,
            BackendClientError::RelayReservationExhausted {
                kind: RelayReservationKind::Nonce
            }
        ));
        assert_eq!(nonce.pending_permissions_counter, 11);

        let mut revision = 7;
        assert_eq!(
            take_next_revision(&mut revision, 8, "test").expect("final revision"),
            7
        );
        let error = take_next_revision(&mut revision, 8, "test")
            .expect_err("next revision must require a fresh reservation");
        assert!(matches!(
            error,
            BackendClientError::RelayReservationExhausted {
                kind: RelayReservationKind::Revision
            }
        ));
        assert_eq!(revision, 8);
    }

    #[test]
    fn pending_snapshot_retry_uses_a_fresh_counter() {
        let mut enc = EncryptionState::derive(&[7; 32], 3, 10, 20).expect("derive");
        let session_id = kodosi_domain::ids::SessionId::new();
        let snapshot = HostRelayPendingPermissionsSnapshot {
            generation: 42,
            incarnation_id: uuid::Uuid::now_v7(),
            plaintext: br#"{"generation":42,"requests":[]}"#.to_vec(),
        };
        let first =
            build_pending_permissions_frame(&mut enc, session_id, &snapshot).expect("first");
        let retry =
            build_pending_permissions_frame(&mut enc, session_id, &snapshot).expect("retry");
        assert_eq!(
            u64::from_be_bytes(first[5..13].try_into().expect("counter")),
            10
        );
        assert_eq!(
            u64::from_be_bytes(retry[5..13].try_into().expect("counter")),
            11
        );
        assert_ne!(first, retry);
    }
}
