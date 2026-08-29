use bytes::{BufMut as _, Bytes, BytesMut};
use kodosi_domain::terminal::{
    TERMINAL_CHECKPOINT_SCHEMA_VERSION, TERMINAL_LOCAL_CHECKPOINT_FRAME_MAX_BYTES,
    TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES, TerminalCheckpointV2, TerminalScreen, TerminalSize,
};
use serde::{Deserialize, Serialize};

use crate::{AppError, Result};

const FRAME_JSON_CONTROL: u8 = 0x01;
const FRAME_LOCAL_CHECKPOINT: u8 = 0x02;
const FRAME_DATA: u8 = 0x03;
const FRAME_INPUT: u8 = 0x04;
const CHECKPOINT_HEADER_BYTES: usize = 1 + 4 + 2 + 2 + 1 + 2 + 2 + 1 + 8 + 4;
const DATA_HEADER_BYTES: usize = 1 + 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TerminalLaneServerFrame {
    LocalCheckpoint {
        checkpoint: TerminalCheckpointV2,
        next_sequence: u64,
    },
    Data {
        sequence: u64,
        bytes: Bytes,
    },
    Resize {
        rows: u16,
        cols: u16,
        at_sequence: u64,
    },
    Closed {
        reason: String,
        final_sequence: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TerminalLaneClientFrame {
    Input { bytes: Bytes },
    Resize { rows: u16, cols: u16 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum ServerControl {
    Resize {
        rows: u16,
        cols: u16,
        at_sequence: u64,
    },
    Closed {
        reason: String,
        final_sequence: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum ClientControl {
    Resize { rows: u16, cols: u16 },
}

impl TerminalLaneServerFrame {
    pub(crate) fn encode(self) -> Result<Bytes> {
        match self {
            Self::LocalCheckpoint {
                checkpoint,
                next_sequence,
            } => encode_checkpoint(&checkpoint, next_sequence),
            Self::Data { sequence, bytes } => {
                let mut frame = BytesMut::with_capacity(DATA_HEADER_BYTES + bytes.len());
                frame.put_u8(FRAME_DATA);
                frame.put_u64(sequence);
                frame.extend_from_slice(&bytes);
                Ok(frame.freeze())
            }
            Self::Resize {
                rows,
                cols,
                at_sequence,
            } => encode_json_control(&ServerControl::Resize {
                rows,
                cols,
                at_sequence,
            }),
            Self::Closed {
                reason,
                final_sequence,
            } => encode_json_control(&ServerControl::Closed {
                reason,
                final_sequence,
            }),
        }
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self> {
        let (&tag, body) = bytes
            .split_first()
            .ok_or_else(|| protocol_error("empty terminal frame"))?;
        match tag {
            FRAME_LOCAL_CHECKPOINT => decode_checkpoint(body),
            FRAME_DATA => {
                let sequence = read_u64(body, 0, "data sequence")?;
                let payload = body
                    .get(8..)
                    .ok_or_else(|| protocol_error("truncated data frame"))?;
                Ok(Self::Data {
                    sequence,
                    bytes: Bytes::copy_from_slice(payload),
                })
            }
            FRAME_JSON_CONTROL => {
                let control: ServerControl =
                    serde_json::from_slice(body).map_err(AppError::Json)?;
                Ok(match control {
                    ServerControl::Resize {
                        rows,
                        cols,
                        at_sequence,
                    } => Self::Resize {
                        rows,
                        cols,
                        at_sequence,
                    },
                    ServerControl::Closed {
                        reason,
                        final_sequence,
                    } => Self::Closed {
                        reason,
                        final_sequence,
                    },
                })
            }
            other => Err(protocol_error(format!(
                "unknown terminal server frame tag 0x{other:02x}"
            ))),
        }
    }
}

impl TerminalLaneClientFrame {
    pub(crate) fn encode(self) -> Result<Bytes> {
        match self {
            Self::Input { bytes } => {
                let mut frame = BytesMut::with_capacity(1 + bytes.len());
                frame.put_u8(FRAME_INPUT);
                frame.extend_from_slice(&bytes);
                Ok(frame.freeze())
            }
            Self::Resize { rows, cols } => {
                encode_json_control(&ClientControl::Resize { rows, cols })
            }
        }
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self> {
        let (&tag, body) = bytes
            .split_first()
            .ok_or_else(|| protocol_error("empty terminal frame"))?;
        match tag {
            FRAME_INPUT => Ok(Self::Input {
                bytes: Bytes::copy_from_slice(body),
            }),
            FRAME_JSON_CONTROL => {
                let control: ClientControl =
                    serde_json::from_slice(body).map_err(AppError::Json)?;
                Ok(match control {
                    ClientControl::Resize { rows, cols } => Self::Resize { rows, cols },
                })
            }
            other => Err(protocol_error(format!(
                "unknown terminal client frame tag 0x{other:02x}"
            ))),
        }
    }
}

fn encode_checkpoint(checkpoint: &TerminalCheckpointV2, next_sequence: u64) -> Result<Bytes> {
    if checkpoint.semantic_checkpoint.len() > TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES {
        return Err(protocol_error("semantic checkpoint exceeds raw byte cap"));
    }
    let body_len = u32::try_from(checkpoint.semantic_checkpoint.len())
        .map_err(|_| protocol_error("semantic checkpoint length is not representable"))?;
    let mut frame = BytesMut::with_capacity(CHECKPOINT_HEADER_BYTES + body_len as usize);
    frame.put_u8(FRAME_LOCAL_CHECKPOINT);
    frame.put_u32(checkpoint.schema_version());
    frame.put_u16(checkpoint.rows());
    frame.put_u16(checkpoint.cols());
    frame.put_u8(match checkpoint.active_screen {
        TerminalScreen::Primary => 0,
        TerminalScreen::Alternate => 1,
    });
    frame.put_u16(checkpoint.cursor_x());
    frame.put_u16(checkpoint.cursor_y());
    frame.put_u8(u8::from(checkpoint.cursor_hidden()));
    frame.put_u64(next_sequence);
    frame.put_u32(body_len);
    frame.extend_from_slice(&checkpoint.semantic_checkpoint);
    if frame.len() > TERMINAL_LOCAL_CHECKPOINT_FRAME_MAX_BYTES {
        return Err(protocol_error("local checkpoint frame exceeds byte cap"));
    }
    Ok(frame.freeze())
}

fn decode_checkpoint(body: &[u8]) -> Result<TerminalLaneServerFrame> {
    let header_len = CHECKPOINT_HEADER_BYTES - 1;
    if body.len() < header_len {
        return Err(protocol_error("truncated local checkpoint header"));
    }
    let schema = read_u32(body, 0, "checkpoint schema")?;
    if schema != TERMINAL_CHECKPOINT_SCHEMA_VERSION {
        return Err(protocol_error(format!(
            "unsupported checkpoint schema {schema}"
        )));
    }
    let rows = read_u16(body, 4, "checkpoint rows")?;
    let cols = read_u16(body, 6, "checkpoint columns")?;
    let active_screen = match body[8] {
        0 => TerminalScreen::Primary,
        1 => TerminalScreen::Alternate,
        other => return Err(protocol_error(format!("unknown active screen {other}"))),
    };
    let cursor_x = read_u16(body, 9, "checkpoint cursor x")?;
    let cursor_y = read_u16(body, 11, "checkpoint cursor y")?;
    let cursor_hidden = match body[13] {
        0 => false,
        1 => true,
        other => {
            return Err(protocol_error(format!(
                "invalid cursor-hidden value {other}"
            )));
        }
    };
    let next_sequence = read_u64(body, 14, "checkpoint sequence")?;
    let checkpoint_len = usize::try_from(read_u32(body, 22, "checkpoint length")?)
        .map_err(|_| protocol_error("checkpoint length is not representable"))?;
    if checkpoint_len == 0 || checkpoint_len > TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES {
        return Err(protocol_error(
            "semantic checkpoint length is outside bounds",
        ));
    }
    let checkpoint_bytes = body
        .get(header_len..)
        .ok_or_else(|| protocol_error("truncated checkpoint body"))?;
    if checkpoint_bytes.len() != checkpoint_len {
        return Err(protocol_error("checkpoint body length mismatch"));
    }
    let checkpoint = TerminalCheckpointV2::new(
        TerminalSize::new(rows, cols).map_err(|error| protocol_error(error.to_string()))?,
        active_screen,
        checkpoint_bytes.to_vec(),
        cursor_x,
        cursor_y,
        cursor_hidden,
    )
    .map_err(|error| protocol_error(error.to_string()))?;
    Ok(TerminalLaneServerFrame::LocalCheckpoint {
        checkpoint,
        next_sequence,
    })
}

fn encode_json_control<T: Serialize>(control: &T) -> Result<Bytes> {
    let json = serde_json::to_vec(control).map_err(AppError::Json)?;
    if json.len() > 64 * 1024 {
        return Err(protocol_error("terminal JSON control exceeds byte cap"));
    }
    let mut frame = BytesMut::with_capacity(1 + json.len());
    frame.put_u8(FRAME_JSON_CONTROL);
    frame.extend_from_slice(&json);
    Ok(frame.freeze())
}

fn read_u16(bytes: &[u8], offset: usize, name: &str) -> Result<u16> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| protocol_error(format!("truncated {name}")))?;
    Ok(u16::from_be_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize, name: &str) -> Result<u32> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| protocol_error(format!("truncated {name}")))?;
    Ok(u32::from_be_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_u64(bytes: &[u8], offset: usize, name: &str) -> Result<u64> {
    let value = bytes
        .get(offset..offset + 8)
        .ok_or_else(|| protocol_error(format!("truncated {name}")))?;
    Ok(u64::from_be_bytes([
        value[0], value[1], value[2], value[3], value[4], value[5], value[6], value[7],
    ]))
}

fn protocol_error(reason: impl Into<String>) -> AppError {
    AppError::Unsupported {
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
    fn raw_checkpoint_round_trips_without_base64() {
        let raw = vec![0, 0xff, b'{', b'}'];
        let encoded = TerminalLaneServerFrame::LocalCheckpoint {
            checkpoint: checkpoint(raw.clone()),
            next_sequence: 9,
        }
        .encode()
        .expect("encode");
        assert!(encoded.len() < raw.len() + CHECKPOINT_HEADER_BYTES + 1);
        let decoded = TerminalLaneServerFrame::decode(&encoded).expect("decode");
        std::assert_matches!(decoded, TerminalLaneServerFrame::LocalCheckpoint { checkpoint, next_sequence: 9 }
            if checkpoint.semantic_checkpoint == raw);
    }

    #[test]
    fn raw_data_and_input_round_trip_invalid_utf8_and_nul() {
        let bytes = Bytes::from_static(&[0, 0xff, b'x']);
        let data = TerminalLaneServerFrame::Data {
            sequence: 7,
            bytes: bytes.clone(),
        };
        let encoded_data = data.clone().encode().expect("encode");
        assert_eq!(
            TerminalLaneServerFrame::decode(&encoded_data).expect("decode"),
            data
        );
        let input = TerminalLaneClientFrame::Input { bytes };
        let encoded_input = input.clone().encode().expect("encode");
        assert_eq!(
            TerminalLaneClientFrame::decode(&encoded_input).expect("decode"),
            input
        );
    }

    #[test]
    fn headless_input_is_raw_bytes_and_rejects_semantic_paste_or_mouse_controls() {
        let raw = Bytes::from_static(b"\x1b[200~line\n\x1b[201~");
        let encoded = TerminalLaneClientFrame::Input { bytes: raw.clone() }
            .encode()
            .expect("encode raw input");
        std::assert_matches!(
            TerminalLaneClientFrame::decode(&encoded),
            Ok(TerminalLaneClientFrame::Input { bytes }) if bytes == raw
        );

        for control in [
            br#"{"type":"paste","text":"line"}"#.as_slice(),
            br#"{"type":"mouse","row":1,"col":1}"#.as_slice(),
        ] {
            let mut frame = vec![FRAME_JSON_CONTROL];
            frame.extend_from_slice(control);
            assert!(TerminalLaneClientFrame::decode(&frame).is_err());
        }
    }

    #[test]
    fn checkpoint_rejects_unknown_tag_truncation_and_length_mismatch() {
        assert!(TerminalLaneServerFrame::decode(&[0xff]).is_err());
        assert!(TerminalLaneServerFrame::decode(&[FRAME_LOCAL_CHECKPOINT]).is_err());
        let mut encoded = TerminalLaneServerFrame::LocalCheckpoint {
            checkpoint: checkpoint(b"checkpoint".to_vec()),
            next_sequence: 1,
        }
        .encode()
        .expect("encode")
        .to_vec();
        encoded.pop();
        assert!(TerminalLaneServerFrame::decode(&encoded).is_err());
    }

    #[test]
    fn controls_reject_unknown_fields_and_boundary_free_close() {
        let mut unknown = vec![FRAME_JSON_CONTROL];
        unknown.extend_from_slice(
            br#"{"type":"resize","rows":2,"cols":3,"at_sequence":4,"extra":true}"#,
        );
        assert!(TerminalLaneServerFrame::decode(&unknown).is_err());
        let mut close = vec![FRAME_JSON_CONTROL];
        close.extend_from_slice(br#"{"type":"closed","reason":"done"}"#);
        assert!(TerminalLaneServerFrame::decode(&close).is_err());
    }
}
