use std::io::{Read as _, Write as _};

use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use ts_rs::TS;

/// Views and the CLI with another number cannot join.
pub const CONTRACT: u32 = 1;

const JSON_FRAME: u8 = 0;
const GZIP_FRAME: u8 = 1;
const ASSET_FRAME: u8 = 2;
const HASH_BYTES: usize = 32;
const GZIP_FROM: usize = 16 * 1024;
const MOST_UNPACKED: u64 = 256 * 1024 * 1024;

pub type Rev = u64;
pub type Seq = u64;

/// A person or an agent, as the host names them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Viewer {
    pub id: String,
    pub name: String,
    pub can_edit: bool,
}

/// What a client sends to its document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ToDocument {
    Join {
        contract: u32,
        client: String,
        #[ts(type = "number")]
        epoch: u64,
        #[ts(type = "number")]
        rev: Rev,
    },
    Change {
        #[ts(type = "number")]
        seq: Seq,
        #[ts(type = "number")]
        base: Rev,
        body: Value,
    },
    Place {
        #[ts(type = "number")]
        rev: Rev,
        body: Value,
    },
    Query {
        #[ts(type = "number")]
        id: u64,
        body: Value,
    },
}

/// One change in the order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Changed {
    #[ts(type = "number")]
    pub rev: Rev,
    pub author: String,
    pub client: String,
    #[ts(type = "number")]
    pub seq: Seq,
    pub body: Value,
}

/// Someone who is here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub place: Value,
}

/// What a client needs after it joins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum Catchup {
    Snapshot { snapshot: Value },
    Missed { changes: Vec<Changed> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RefusedReason {
    /// Move the change across what came first and send it again, or read again.
    Behind,
    /// Load again.
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ClosedReason {
    Contract,
    Removed,
    Failed,
}

/// What a document sends to a client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ToClient {
    Joined {
        id: String,
        name: String,
        can_edit: bool,
        #[ts(type = "number")]
        epoch: u64,
        #[ts(type = "number")]
        rev: Rev,
        #[ts(type = "number")]
        seq: Seq,
        peers: Vec<Peer>,
        catchup: Catchup,
    },
    Changed(Changed),
    Refused {
        #[ts(type = "number")]
        seq: Seq,
        reason: RefusedReason,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        detail: Option<String>,
    },
    Answer {
        #[ts(type = "number")]
        id: u64,
        #[ts(type = "number")]
        rev: Rev,
        body: Value,
    },
    Saved {
        #[ts(type = "number")]
        rev: Rev,
        waits: bool,
    },
    Peer(Peer),
    Left {
        id: String,
    },
    Closed {
        reason: ClosedReason,
    },
}

/// The hash that names an asset.
pub type Hash = [u8; HASH_BYTES];

/// One frame of a pipe: JSON, gzip JSON when it is large, or the raw bytes of an asset.
/// An asset frame with no bytes asks for that asset.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame<T> {
    Message(T),
    Asset { hash: Hash, bytes: Vec<u8> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("The frame is not valid.")]
pub struct BadFrame;

pub fn encode_message(message: &impl Serialize) -> Vec<u8> {
    let json = serde_json::to_vec(message).unwrap_or_default();
    if json.len() >= GZIP_FROM {
        let mut encoder = GzEncoder::new(vec![GZIP_FRAME], Compression::fast());
        if encoder.write_all(&json).is_ok()
            && let Ok(frame) = encoder.finish()
        {
            return frame;
        }
    }
    let mut frame = Vec::with_capacity(json.len() + 1);
    frame.push(JSON_FRAME);
    frame.extend_from_slice(&json);
    frame
}

pub fn encode_asset(hash: &Hash, bytes: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(1 + HASH_BYTES + bytes.len());
    frame.push(ASSET_FRAME);
    frame.extend_from_slice(hash);
    frame.extend_from_slice(bytes);
    frame
}

pub fn decode<T: DeserializeOwned>(frame: &[u8]) -> Result<Frame<T>, BadFrame> {
    let (&kind, body) = frame.split_first().ok_or(BadFrame)?;
    match kind {
        JSON_FRAME => serde_json::from_slice(body)
            .map(Frame::Message)
            .map_err(|_| BadFrame),
        GZIP_FRAME => {
            let mut json = Vec::new();
            GzDecoder::new(body)
                .take(MOST_UNPACKED)
                .read_to_end(&mut json)
                .map_err(|_| BadFrame)?;
            serde_json::from_slice(&json)
                .map(Frame::Message)
                .map_err(|_| BadFrame)
        }
        ASSET_FRAME => {
            let (hash, bytes) = body.split_at_checked(HASH_BYTES).ok_or(BadFrame)?;
            Ok(Frame::Asset {
                hash: hash.try_into().map_err(|_| BadFrame)?,
                bytes: bytes.to_vec(),
            })
        }
        _ => Err(BadFrame),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_small_message_stays_plain_json() {
        let message = ToDocument::Query {
            id: 3,
            body: json!({ "cell": "A1" }),
        };
        let frame = encode_message(&message);
        assert_eq!(frame[0], JSON_FRAME);
        assert_eq!(decode(&frame), Ok(Frame::Message(message)));
    }

    #[test]
    fn a_large_message_travels_as_gzip() {
        let message = ToDocument::Query {
            id: 3,
            body: json!("x".repeat(GZIP_FROM)),
        };
        let frame = encode_message(&message);
        assert_eq!(frame[0], GZIP_FRAME);
        assert!(frame.len() < GZIP_FROM / 10);
        assert_eq!(decode(&frame), Ok(Frame::Message(message)));
    }

    #[test]
    fn an_asset_keeps_its_hash_and_bytes() {
        let frame = encode_asset(&[7; HASH_BYTES], b"picture");
        assert_eq!(
            decode::<ToDocument>(&frame),
            Ok(Frame::Asset {
                hash: [7; HASH_BYTES],
                bytes: b"picture".to_vec(),
            })
        );
    }

    #[test]
    fn broken_frames_are_refused() {
        assert_eq!(decode::<ToDocument>(&[]), Err(BadFrame));
        assert_eq!(decode::<ToDocument>(&[9, 1]), Err(BadFrame));
        assert_eq!(decode::<ToDocument>(&[ASSET_FRAME, 1, 2]), Err(BadFrame));
        assert_eq!(decode::<ToDocument>(&[JSON_FRAME, b'{']), Err(BadFrame));
    }
}
