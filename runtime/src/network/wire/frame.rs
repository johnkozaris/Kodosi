use bytes::Bytes;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    METADATA_LIMIT, RAW_BATCH_LIMIT, RAW_CHUNK_LIMIT, Result, TerminalControl, decode_control,
    decode_metadata, encode_control, encode_metadata, invalid, packing,
};
use crate::terminal::TerminalMetadata;

pub(crate) const FRAME_LIMIT: usize = 1024 * 1024;
pub(crate) const KEYFRAME_PART: usize = 32 * 1024;
pub(crate) const OUTPUT_FRAME: usize = 32 * 1024;
const HEADER: usize = 7;
const PACK_FROM: usize = 256;
const OUTPUT_LEVEL: u32 = 3;
const UNPACKED_OUTPUT_LIMIT: usize = RAW_BATCH_LIMIT * (4 + RAW_CHUNK_LIMIT);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Hello {
    pub protocol_version: u32,
    pub session_id: Uuid,
    pub incarnation_id: Uuid,
    pub user_id: String,
    pub device_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Accept {
    pub protocol_version: u32,
    pub host_device_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Refuse {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct End {
    pub final_sequence: u64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ControlResult {
    pub request_id: Uuid,
    pub accepted: bool,
    pub message: String,
}

#[derive(Debug, Clone)]
pub(crate) enum Frame {
    Hello(Hello),
    Accept(Accept),
    Refuse(Refuse),
    Keyframe {
        next_sequence: u64,
        more: bool,
        part: Bytes,
    },
    Output {
        first_sequence: u64,
        chunks: Vec<Bytes>,
    },
    Resize {
        rows: u16,
        cols: u16,
        at_sequence: u64,
    },
    Metadata(TerminalMetadata),
    Heartbeat {
        number: u32,
        next_sequence: u64,
    },
    End(End),
    Ack {
        received: u64,
    },
    Input {
        offset: u64,
        heartbeat: u32,
        bytes: Bytes,
    },
    Control {
        request_id: Uuid,
        control: TerminalControl,
    },
    Refresh,
    InputAck {
        offset: u64,
    },
    ControlResult(ControlResult),
}

impl Frame {
    pub(crate) const fn carries_output(&self) -> bool {
        matches!(self, Self::Keyframe { .. } | Self::Output { .. })
    }

    pub(crate) fn encode(&self) -> Result<Vec<u8>> {
        let (kind, body) = match self {
            Self::Hello(hello) => (1, serde_json::to_vec(hello)?),
            Self::Accept(accept) => (2, serde_json::to_vec(accept)?),
            Self::Refuse(refuse) => (3, serde_json::to_vec(refuse)?),
            Self::Keyframe {
                next_sequence,
                more,
                part,
            } => {
                if part.len() > KEYFRAME_PART {
                    return Err(invalid("Terminal snapshot part exceeds its bound."));
                }
                let mut body = Vec::with_capacity(9 + part.len());
                body.extend_from_slice(&next_sequence.to_be_bytes());
                body.push(u8::from(*more));
                body.extend_from_slice(part);
                (4, body)
            }
            Self::Output {
                first_sequence,
                chunks,
            } => (5, encode_output(*first_sequence, chunks)?),
            Self::Resize {
                rows,
                cols,
                at_sequence,
            } => {
                let mut body = Vec::with_capacity(12);
                body.extend_from_slice(&rows.to_be_bytes());
                body.extend_from_slice(&cols.to_be_bytes());
                body.extend_from_slice(&at_sequence.to_be_bytes());
                (6, body)
            }
            Self::Metadata(metadata) => (7, encode_metadata(metadata)?),
            Self::Heartbeat {
                number,
                next_sequence,
            } => {
                let mut body = Vec::with_capacity(12);
                body.extend_from_slice(&number.to_be_bytes());
                body.extend_from_slice(&next_sequence.to_be_bytes());
                (8, body)
            }
            Self::End(end) => (9, serde_json::to_vec(end)?),
            Self::Ack { received } => (10, received.to_be_bytes().to_vec()),
            Self::Input {
                offset,
                heartbeat,
                bytes,
            } => {
                if bytes.is_empty() || bytes.len() > super::INPUT_LIMIT {
                    return Err(invalid("Terminal input exceeds its chunk limit."));
                }
                let mut body = Vec::with_capacity(12 + bytes.len());
                body.extend_from_slice(&offset.to_be_bytes());
                body.extend_from_slice(&heartbeat.to_be_bytes());
                body.extend_from_slice(bytes);
                (11, body)
            }
            Self::Control {
                request_id,
                control,
            } => {
                if matches!(control, TerminalControl::Input { .. }) {
                    return Err(invalid("Terminal input uses its own frame."));
                }
                let mut body = request_id.as_bytes().to_vec();
                body.extend_from_slice(&encode_control(control)?);
                (12, body)
            }
            Self::Refresh => (13, Vec::new()),
            Self::InputAck { offset } => (14, offset.to_be_bytes().to_vec()),
            Self::ControlResult(result) => (15, serde_json::to_vec(result)?),
        };
        let length = padded(HEADER + body.len());
        if length > FRAME_LIMIT {
            return Err(invalid("Terminal frame exceeds its bound."));
        }
        let mut frame = Vec::with_capacity(length);
        frame.extend_from_slice(&u24(length - 3)?);
        frame.push(kind);
        frame.extend_from_slice(&u24(body.len())?);
        frame.extend_from_slice(&body);
        frame.resize(length, 0);
        Ok(frame)
    }

    pub(crate) fn take(buffer: &mut Vec<u8>) -> Result<Option<(Self, usize)>> {
        if buffer.len() < 3 {
            return Ok(None);
        }
        let length = 3 + read24(buffer, 0);
        if !(HEADER..=FRAME_LIMIT).contains(&length) {
            return Err(invalid("Terminal frame has an invalid length."));
        }
        if buffer.len() < length {
            return Ok(None);
        }
        let kind = buffer[3];
        let size = read24(buffer, 4);
        if HEADER + size > length || buffer[HEADER + size..length].iter().any(|byte| *byte != 0) {
            return Err(invalid("Terminal frame has an invalid body."));
        }
        let frame = Self::decode(kind, &buffer[HEADER..HEADER + size])?;
        buffer.drain(..length);
        Ok(Some((frame, length)))
    }

    fn decode(kind: u8, body: &[u8]) -> Result<Self> {
        let json = |limit: usize| {
            if body.len() > limit {
                Err(invalid("Terminal frame exceeds its bound."))
            } else {
                Ok(body)
            }
        };
        Ok(match kind {
            1 => Self::Hello(serde_json::from_slice(json(4096)?)?),
            2 => Self::Accept(serde_json::from_slice(json(4096)?)?),
            3 => Self::Refuse(serde_json::from_slice(json(4096)?)?),
            4 => {
                if body.len() < 9 || body.len() > 9 + KEYFRAME_PART || body[8] > 1 {
                    return Err(invalid("Terminal snapshot part is invalid."));
                }
                Self::Keyframe {
                    next_sequence: read64(body, 0),
                    more: body[8] == 1,
                    part: Bytes::copy_from_slice(&body[9..]),
                }
            }
            5 => decode_output(body)?,
            6 => {
                exact(body, 12)?;
                Self::Resize {
                    rows: u16::from_be_bytes([body[0], body[1]]),
                    cols: u16::from_be_bytes([body[2], body[3]]),
                    at_sequence: read64(body, 4),
                }
            }
            7 => Self::Metadata(decode_metadata(json(METADATA_LIMIT)?)?),
            8 => {
                exact(body, 12)?;
                Self::Heartbeat {
                    number: read32(body, 0),
                    next_sequence: read64(body, 4),
                }
            }
            9 => Self::End(serde_json::from_slice(json(4096)?)?),
            10 => {
                exact(body, 8)?;
                Self::Ack {
                    received: read64(body, 0),
                }
            }
            11 => {
                if body.len() <= 12 || body.len() > 12 + super::INPUT_LIMIT {
                    return Err(invalid("Terminal input frame is invalid."));
                }
                Self::Input {
                    offset: read64(body, 0),
                    heartbeat: read32(body, 8),
                    bytes: Bytes::copy_from_slice(&body[12..]),
                }
            }
            12 => {
                if body.len() < 16 || body.len() > 16 + 4096 {
                    return Err(invalid("Terminal control frame is invalid."));
                }
                let request_id = Uuid::from_slice(&body[..16])
                    .map_err(|_| invalid("Terminal control frame is invalid."))?;
                let control = decode_control(&body[16..], request_id)?;
                if matches!(control, TerminalControl::Input { .. }) {
                    return Err(invalid("Terminal input uses its own frame."));
                }
                Self::Control {
                    request_id,
                    control,
                }
            }
            13 => {
                exact(body, 0)?;
                Self::Refresh
            }
            14 => {
                exact(body, 8)?;
                Self::InputAck {
                    offset: read64(body, 0),
                }
            }
            15 => Self::ControlResult(serde_json::from_slice(json(4096)?)?),
            _ => return Err(invalid("Unsupported terminal frame.")),
        })
    }
}

fn encode_output(first_sequence: u64, chunks: &[Bytes]) -> Result<Vec<u8>> {
    if chunks.is_empty()
        || chunks.len() > RAW_BATCH_LIMIT
        || chunks.iter().any(|chunk| chunk.len() > RAW_CHUNK_LIMIT)
    {
        return Err(invalid("Terminal output batch exceeds its bound."));
    }
    let size = chunks.iter().map(|chunk| 4 + chunk.len()).sum::<usize>();
    let mut plain = Vec::with_capacity(size);
    for chunk in chunks {
        plain.extend_from_slice(
            &u32::try_from(chunk.len())
                .map_err(|_| invalid("Terminal output chunk is too large."))?
                .to_be_bytes(),
        );
        plain.extend_from_slice(chunk);
    }
    let mut body = Vec::with_capacity(9 + plain.len());
    body.extend_from_slice(&first_sequence.to_be_bytes());
    let packed = (plain.len() >= PACK_FROM)
        .then(|| packing::pack(&plain, OUTPUT_LEVEL))
        .transpose()?
        .filter(|packed| packed.len() < plain.len());
    if let Some(packed) = packed {
        body.push(1);
        body.extend_from_slice(&packed);
    } else {
        body.push(0);
        body.extend_from_slice(&plain);
    }
    Ok(body)
}

fn decode_output(body: &[u8]) -> Result<Frame> {
    if body.len() < 9 || body[8] > 1 {
        return Err(invalid("Terminal output frame is invalid."));
    }
    let plain = if body[8] == 1 {
        packing::unpack(&body[9..], UNPACKED_OUTPUT_LIMIT)?
    } else {
        body[9..].to_vec()
    };
    let plain = Bytes::from(plain);
    let mut chunks = Vec::new();
    let mut at = 0;
    while at < plain.len() {
        if plain.len() - at < 4 || chunks.len() == RAW_BATCH_LIMIT {
            return Err(invalid("Terminal output frame is invalid."));
        }
        let size = read32(&plain, at) as usize;
        at += 4;
        if size > RAW_CHUNK_LIMIT || plain.len() - at < size {
            return Err(invalid("Terminal output frame is invalid."));
        }
        chunks.push(plain.slice(at..at + size));
        at += size;
    }
    if chunks.is_empty() {
        return Err(invalid("Terminal output frame is empty."));
    }
    Ok(Frame::Output {
        first_sequence: read64(body, 0),
        chunks,
    })
}

pub(crate) fn padded(length: usize) -> usize {
    if length <= 1024 {
        return length.div_ceil(64) * 64;
    }
    let exponent = usize::BITS - 1 - length.leading_zeros();
    let bits = u32::BITS - exponent.leading_zeros();
    let mask = (1usize << (exponent - bits)) - 1;
    (length + mask) & !mask
}

fn u24(value: usize) -> Result<[u8; 3]> {
    let bytes = u32::try_from(value)
        .ok()
        .filter(|value| *value < 1 << 24)
        .ok_or_else(|| invalid("Terminal frame exceeds its bound."))?
        .to_be_bytes();
    Ok([bytes[1], bytes[2], bytes[3]])
}

fn read24(bytes: &[u8], at: usize) -> usize {
    usize::from(bytes[at]) << 16 | usize::from(bytes[at + 1]) << 8 | usize::from(bytes[at + 2])
}

fn read32(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn read64(bytes: &[u8], at: usize) -> u64 {
    let mut value = [0; 8];
    value.copy_from_slice(&bytes[at..at + 8]);
    u64::from_be_bytes(value)
}

fn exact(body: &[u8], length: usize) -> Result<()> {
    if body.len() == length {
        Ok(())
    } else {
        Err(invalid("Terminal frame has an invalid body."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(frame: &Frame) -> Frame {
        let mut encoded = frame.encode().unwrap();
        let length = encoded.len();
        encoded.extend_from_slice(b"next");
        let (decoded, used) = Frame::take(&mut encoded).unwrap().unwrap();
        assert_eq!((used, encoded.as_slice()), (length, &b"next"[..]));
        decoded
    }

    #[test]
    fn frames_keep_their_fields_and_leave_the_next_frame_in_the_buffer() {
        let output = Frame::Output {
            first_sequence: 41,
            chunks: vec![
                Bytes::from_static(b"one"),
                Bytes::new(),
                Bytes::from(vec![b'x'; 5000]),
            ],
        };
        let Frame::Output {
            first_sequence,
            chunks,
        } = round_trip(&output)
        else {
            panic!("output frame");
        };
        assert_eq!((first_sequence, chunks.len()), (41, 3));
        assert_eq!(
            (&chunks[0][..], chunks[1].len(), chunks[2].len()),
            (&b"one"[..], 0, 5000)
        );
        assert!(output.encode().unwrap().len() < 1024);

        let Frame::Input {
            offset,
            heartbeat,
            bytes,
        } = round_trip(&Frame::Input {
            offset: 9,
            heartbeat: 3,
            bytes: Bytes::from_static(b"ls\r"),
        })
        else {
            panic!("input frame");
        };
        assert_eq!((offset, heartbeat, &bytes[..]), (9, 3, &b"ls\r"[..]));

        let request_id = Uuid::now_v7();
        assert!(matches!(
            round_trip(&Frame::Control { request_id, control: TerminalControl::Interrupt }),
            Frame::Control { request_id: id, control: TerminalControl::Interrupt } if id == request_id
        ));
        assert!(matches!(round_trip(&Frame::Refresh), Frame::Refresh));
        assert!(matches!(
            round_trip(&Frame::Heartbeat {
                number: 7,
                next_sequence: 12
            }),
            Frame::Heartbeat {
                number: 7,
                next_sequence: 12
            }
        ));
    }

    #[test]
    fn a_frame_is_complete_only_with_all_its_bytes_and_zero_padding() {
        let mut encoded = Frame::Ack { received: 5 }.encode().unwrap();
        assert_eq!(encoded.len(), 64);
        let mut partial = encoded[..40].to_vec();
        assert!(Frame::take(&mut partial).unwrap().is_none());
        assert_eq!(partial.len(), 40);
        encoded[63] = 1;
        assert!(Frame::take(&mut encoded).is_err());
        assert!(Frame::take(&mut vec![0, 0, 1, 10]).is_err());
        assert!(
            Frame::Control {
                request_id: Uuid::now_v7(),
                control: TerminalControl::Input { bytes: vec![1] },
            }
            .encode()
            .is_err()
        );
    }

    #[test]
    fn padding_hides_small_differences_and_adds_at_most_an_eighth() {
        assert_eq!(
            (padded(1), padded(64), padded(65), padded(1024)),
            (64, 64, 128, 1024)
        );
        for length in [1025, 1500, 4096, 4097, 33_000, 262_153, 1_000_000] {
            let padded = padded(length);
            assert!(
                padded >= length && padded - length <= length / 8,
                "{length}"
            );
        }
        assert_eq!(padded(1025), padded(1030));
    }
}
