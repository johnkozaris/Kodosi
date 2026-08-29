use bytes::{BufMut as _, Bytes, BytesMut};
use kodosi_domain::terminal::{
    TERMINAL_CHECKPOINT_SCHEMA_VERSION, TERMINAL_PRESENTATION_MAX_SERIALIZED_BYTES,
    TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES, TerminalCheckpointV2, TerminalPresentationV2,
    TerminalScreen, TerminalSize,
};

use crate::{BackendClientError, Result};

pub(crate) const CHECKPOINT_PLAINTEXT_HEADER_BYTES: usize = 4 + 2 + 2 + 1 + 2 + 2 + 1 + 4;
pub(crate) const TERMINAL_RAW_BATCH_MAX_CHUNKS: usize = 1_024;
pub(crate) const TERMINAL_RAW_CHUNK_MAX_BYTES: usize = 1024 * 1024;
const TERMINAL_RAW_ENVELOPE_OVERHEAD_BYTES: usize = 1 + 4 + 8 + 8 + 8 + 16;

pub(crate) fn terminal_raw_batch_max_plaintext_bytes() -> usize {
    crate::session_relay_authority::relay_message_limit("term.rawBatch")
        .saturating_sub(TERMINAL_RAW_ENVELOPE_OVERHEAD_BYTES)
}

pub(crate) fn encode_checkpoint(checkpoint: &TerminalCheckpointV2) -> Result<Vec<u8>> {
    let semantic_len = checkpoint.semantic_checkpoint.len();
    if semantic_len == 0 || semantic_len > TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES {
        return Err(protocol_error(
            "semantic checkpoint length is outside bounds",
        ));
    }
    let semantic_len = u32::try_from(semantic_len)
        .map_err(|_| protocol_error("semantic checkpoint length is not representable as a u32"))?;
    let mut encoded =
        BytesMut::with_capacity(CHECKPOINT_PLAINTEXT_HEADER_BYTES + semantic_len as usize);
    encoded.put_u32(checkpoint.schema_version());
    encoded.put_u16(checkpoint.rows());
    encoded.put_u16(checkpoint.cols());
    encoded.put_u8(encode_screen(checkpoint.active_screen));
    encoded.put_u16(checkpoint.cursor_x());
    encoded.put_u16(checkpoint.cursor_y());
    encoded.put_u8(u8::from(checkpoint.cursor_hidden()));
    encoded.put_u32(semantic_len);
    encoded.extend_from_slice(&checkpoint.semantic_checkpoint);
    Ok(encoded.to_vec())
}

pub(crate) fn decode_checkpoint(encoded: &[u8]) -> Result<TerminalCheckpointV2> {
    if encoded.len() < CHECKPOINT_PLAINTEXT_HEADER_BYTES {
        return Err(protocol_error("truncated terminal checkpoint plaintext"));
    }
    let schema = read_u32(encoded, 0, "checkpoint schema")?;
    if schema != TERMINAL_CHECKPOINT_SCHEMA_VERSION {
        return Err(protocol_error(format!(
            "unsupported terminal checkpoint schema {schema}"
        )));
    }
    let rows = read_u16(encoded, 4, "checkpoint rows")?;
    let cols = read_u16(encoded, 6, "checkpoint columns")?;
    let active_screen = decode_screen(encoded[8])?;
    let cursor_x = read_u16(encoded, 9, "checkpoint cursor x")?;
    let cursor_y = read_u16(encoded, 11, "checkpoint cursor y")?;
    let cursor_hidden = decode_bool(encoded[13], "checkpoint cursor-hidden")?;
    let body_len = usize::try_from(read_u32(encoded, 14, "checkpoint body length")?)
        .map_err(|_| protocol_error("checkpoint body length is not representable"))?;
    if body_len == 0 || body_len > TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES {
        return Err(protocol_error(
            "semantic checkpoint length is outside bounds",
        ));
    }
    let body = encoded
        .get(CHECKPOINT_PLAINTEXT_HEADER_BYTES..)
        .ok_or_else(|| protocol_error("truncated terminal checkpoint body"))?;
    if body.len() != body_len {
        return Err(protocol_error("terminal checkpoint body length mismatch"));
    }
    let size = TerminalSize::new(rows, cols).map_err(|error| protocol_error(error.to_string()))?;
    TerminalCheckpointV2::new(
        size,
        active_screen,
        body.to_vec(),
        cursor_x,
        cursor_y,
        cursor_hidden,
    )
    .map_err(|error| protocol_error(error.to_string()))
}

pub(crate) fn encode_raw_chunks(chunks: &[Bytes]) -> Result<Vec<u8>> {
    if chunks.is_empty() || chunks.len() > TERMINAL_RAW_BATCH_MAX_CHUNKS {
        return Err(protocol_error(
            "terminal raw batch chunk count is outside bounds",
        ));
    }
    let mut encoded = BytesMut::new();
    for chunk in chunks {
        if chunk.len() > TERMINAL_RAW_CHUNK_MAX_BYTES {
            return Err(protocol_error("terminal raw chunk exceeds byte cap"));
        }
        let chunk_len = u32::try_from(chunk.len())
            .map_err(|_| protocol_error("terminal raw chunk length is not representable"))?;
        let next_len = encoded
            .len()
            .checked_add(4)
            .and_then(|len| len.checked_add(chunk.len()))
            .ok_or_else(|| protocol_error("terminal raw batch length overflow"))?;
        if next_len > terminal_raw_batch_max_plaintext_bytes() {
            return Err(protocol_error("terminal raw batch exceeds byte cap"));
        }
        encoded.put_u32(chunk_len);
        encoded.extend_from_slice(chunk);
    }
    Ok(encoded.to_vec())
}

pub(crate) fn decode_raw_chunks(
    encoded: &[u8],
    first_sequence: u64,
    next_sequence: u64,
) -> Result<Vec<Vec<u8>>> {
    if encoded.is_empty() || encoded.len() > terminal_raw_batch_max_plaintext_bytes() {
        return Err(protocol_error(
            "terminal raw batch plaintext is outside bounds",
        ));
    }
    let expected_count = next_sequence
        .checked_sub(first_sequence)
        .ok_or_else(|| protocol_error("terminal raw batch sequence range is regressive"))?;
    let expected_count = usize::try_from(expected_count)
        .map_err(|_| protocol_error("terminal raw batch sequence span is not representable"))?;
    if expected_count == 0 || expected_count > TERMINAL_RAW_BATCH_MAX_CHUNKS {
        return Err(protocol_error(
            "terminal raw batch sequence span is outside bounds",
        ));
    }

    let mut chunks = Vec::with_capacity(expected_count);
    let mut offset = 0usize;
    while offset < encoded.len() {
        if chunks.len() >= TERMINAL_RAW_BATCH_MAX_CHUNKS {
            return Err(protocol_error("terminal raw batch has too many chunks"));
        }
        let chunk_len = usize::try_from(read_u32(encoded, offset, "raw chunk length")?)
            .map_err(|_| protocol_error("terminal raw chunk length is not representable"))?;
        if chunk_len > TERMINAL_RAW_CHUNK_MAX_BYTES {
            return Err(protocol_error("terminal raw chunk exceeds byte cap"));
        }
        offset = offset
            .checked_add(4)
            .ok_or_else(|| protocol_error("terminal raw batch offset overflow"))?;
        let end = offset
            .checked_add(chunk_len)
            .ok_or_else(|| protocol_error("terminal raw chunk length overflow"))?;
        let chunk = encoded
            .get(offset..end)
            .ok_or_else(|| protocol_error("truncated terminal raw chunk"))?;
        chunks.push(chunk.to_vec());
        offset = end;
    }
    if chunks.len() != expected_count {
        return Err(protocol_error(
            "terminal raw batch chunk count does not match sequence range",
        ));
    }
    Ok(chunks)
}

pub(crate) fn encode_presentation(presentation: &TerminalPresentationV2) -> Result<Vec<u8>> {
    let plaintext = serde_json::to_vec(presentation)?;
    if plaintext.len() > TERMINAL_PRESENTATION_MAX_SERIALIZED_BYTES {
        return Err(BackendClientError::Capacity {
            resource: "terminal_presentation",
            reason: format!(
                "serialized presentation is {} bytes; maximum is {TERMINAL_PRESENTATION_MAX_SERIALIZED_BYTES}",
                plaintext.len()
            ),
        });
    }
    Ok(plaintext)
}

pub(crate) fn decode_presentation(encoded: &[u8]) -> Result<TerminalPresentationV2> {
    if encoded.len() > TERMINAL_PRESENTATION_MAX_SERIALIZED_BYTES {
        return Err(BackendClientError::Capacity {
            resource: "terminal_presentation",
            reason: format!(
                "serialized presentation is {} bytes; maximum is {TERMINAL_PRESENTATION_MAX_SERIALIZED_BYTES}",
                encoded.len()
            ),
        });
    }
    serde_json::from_slice(encoded).map_err(Into::into)
}

const fn encode_screen(screen: TerminalScreen) -> u8 {
    match screen {
        TerminalScreen::Primary => 0,
        TerminalScreen::Alternate => 1,
    }
}

fn decode_screen(value: u8) -> Result<TerminalScreen> {
    match value {
        0 => Ok(TerminalScreen::Primary),
        1 => Ok(TerminalScreen::Alternate),
        other => Err(protocol_error(format!(
            "unknown terminal active-screen tag {other}"
        ))),
    }
}

fn decode_bool(value: u8, field: &'static str) -> Result<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(protocol_error(format!("invalid {field} value {other}"))),
    }
}

fn read_u16(bytes: &[u8], offset: usize, field: &'static str) -> Result<u16> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| protocol_error(format!("truncated {field}")))?;
    Ok(u16::from_be_bytes(
        value
            .try_into()
            .map_err(|_| protocol_error(format!("invalid {field}")))?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize, field: &'static str) -> Result<u32> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| protocol_error(format!("truncated {field}")))?;
    Ok(u32::from_be_bytes(
        value
            .try_into()
            .map_err(|_| protocol_error(format!("invalid {field}")))?,
    ))
}

fn protocol_error(reason: impl Into<String>) -> BackendClientError {
    BackendClientError::Protocol {
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkpoint(bytes: Vec<u8>) -> TerminalCheckpointV2 {
        TerminalCheckpointV2::new(
            TerminalSize::new(24, 80).expect("size"),
            TerminalScreen::Alternate,
            bytes,
            79,
            23,
            true,
        )
        .expect("checkpoint")
    }

    #[test]
    fn checkpoint_binary_codec_preserves_raw_bytes_without_base64() {
        let raw = vec![0, 0xff, b'{', b'}'];
        let encoded = encode_checkpoint(&checkpoint(raw.clone())).expect("encode");
        assert_eq!(encoded.len(), CHECKPOINT_PLAINTEXT_HEADER_BYTES + raw.len());
        let decoded = decode_checkpoint(&encoded).expect("decode");
        assert_eq!(decoded.semantic_checkpoint, raw);
    }

    #[test]
    fn raw_batch_codec_preserves_chunk_boundaries_and_binary() {
        let chunks = [
            Bytes::from_static(&[0, 0xff]),
            Bytes::new(),
            Bytes::from_static(b"third"),
        ];
        let encoded = encode_raw_chunks(&chunks).expect("encode");
        assert_eq!(
            decode_raw_chunks(&encoded, 7, 10).expect("decode"),
            vec![vec![0, 0xff], Vec::new(), b"third".to_vec()]
        );
    }

    #[test]
    fn raw_batch_rejects_truncation_and_sequence_count_mismatch() {
        let encoded = encode_raw_chunks(&[Bytes::from_static(b"one")]).expect("encode");
        assert!(decode_raw_chunks(&encoded[..encoded.len() - 1], 1, 2).is_err());
        assert!(decode_raw_chunks(&encoded, 1, 3).is_err());
        assert!(decode_raw_chunks(&encoded, 2, 1).is_err());
    }
}
