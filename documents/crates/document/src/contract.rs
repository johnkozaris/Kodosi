use std::io::{Read as _, Write as _};

use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use ts_rs::TS;

/// Views and the CLI with another number cannot join.
pub const CONTRACT: u32 = 3;

const JSON_FRAME: u8 = 0;
const GZIP_FRAME: u8 = 1;
const ASSET_FRAME: u8 = 2;
pub(crate) const HASH_BYTES: usize = 32;
const GZIP_FROM: usize = 16 * 1024;
const MOST_UNPACKED: u64 = 256 * 1024 * 1024;

pub type Rev = u64;
pub type Seq = u64;

/// Each view of one person has the same `id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Viewer {
    pub id: String,
    pub name: String,
    pub can_edit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ToDocument {
    Join {
        contract: u32,
        client: String,
        epoch: u64,
        rev: Rev,
        /// The client sent no batch yet and has none on its way, so a document that does not
        /// know it took nothing from it.
        fresh: bool,
    },
    Change {
        seq: Seq,
        base: Rev,
        body: Value,
    },
    /// Where the person is, as the document was at `rev`.
    Place {
        rev: Rev,
        body: Value,
    },
    Query {
        id: u64,
        body: Value,
    },
    /// Who else is here and where, in the words of `read`. The answer is a list of names and
    /// places.
    People {
        id: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Changed {
    pub rev: Rev,
    pub author: String,
    pub client: String,
    pub seq: Seq,
    pub body: Value,
}

/// `person` is the viewer's id, the same in each view of them, and `place` is where they are as
/// the document is now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    pub id: String,
    pub person: String,
    pub name: String,
    pub place: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Catchup {
    Snapshot { snapshot: Value },
    Missed { changes: Vec<Changed> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum RefusedReason {
    /// Move the change across what came first and send it again, or read again.
    Behind,
    /// Load again.
    Invalid,
}

/// Views of another contract read why they could not join, so these names stay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ClosedReason {
    /// Update this app.
    ClientOutdated,
    /// Update the app on the computer with the file.
    HostOutdated,
    Removed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ToClient {
    Joined {
        id: String,
        name: String,
        can_edit: bool,
        epoch: u64,
        rev: Rev,
        /// The last batch of this client that the document took, and none when the document does
        /// not know the client, so that it cannot tell whether it took the batch on its way.
        seq: Option<Seq>,
        peers: Vec<Peer>,
        catchup: Catchup,
    },
    Changed(Changed),
    Refused {
        seq: Seq,
        reason: RefusedReason,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        detail: Option<String>,
    },
    Answer {
        id: u64,
        rev: Rev,
        body: Value,
    },
    Saved {
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

pub type Hash = [u8; HASH_BYTES];

/// The bytes in lowercase hexadecimal, as an asset or a short hash goes in a snapshot.
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 0xf)]));
    }
    text
}

/// The hash that [`hex`] wrote.
pub fn hash_from_hex(text: &str) -> Option<Hash> {
    let (pairs, []) = text.as_bytes().as_chunks::<2>() else {
        return None;
    };
    let pairs: &[[u8; 2]; HASH_BYTES] = pairs.try_into().ok()?;
    let mut hash = [0; HASH_BYTES];
    for (byte, [high, low]) in hash.iter_mut().zip(pairs) {
        let digit = |digit: u8| char::from(digit).to_digit(16);
        *byte = u8::try_from(digit(*high)? << 4 | digit(*low)?).ok()?;
    }
    Some(hash)
}

/// One frame of a pipe: JSON, gzip JSON when it is large, or the raw bytes of an asset.
/// An asset frame with no bytes asks for that asset.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    use std::{
        collections::BTreeMap,
        path::{Path, PathBuf},
    };

    use serde_json::json;

    use super::*;

    const GENERATED: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../views/contract/src/generated"
    );

    fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut found = BTreeMap::new();
        let mut folders = vec![root.to_path_buf()];
        while let Some(folder) = folders.pop() {
            for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    folders.push(path);
                } else {
                    let name = path.strip_prefix(root).unwrap().to_path_buf();
                    found.insert(name, std::fs::read(&path).unwrap());
                }
            }
        }
        found
    }

    /// The views build with the committed files, so a stale one is written again here and
    /// fails the check, for the same commit to take it.
    #[test]
    fn the_views_have_the_contract_and_its_number_from_here() {
        let fresh = std::env::temp_dir().join(format!("document-contract-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&fresh));
        let config = ts_rs::Config::new()
            .with_large_int("number")
            .with_out_dir(&fresh);
        ToDocument::export_all(&config).unwrap();
        ToClient::export_all(&config).unwrap();
        std::fs::write(
            fresh.join("CONTRACT.ts"),
            format!(
                "// This file was generated from documents/crates/document/src/contract.rs. Do not edit this file manually.\n\nexport const CONTRACT = {CONTRACT};\n"
            ),
        )
        .unwrap();
        let wanted = files(&fresh);
        drop(std::fs::remove_dir_all(&fresh));
        let generated = Path::new(GENERATED);
        if files(generated) == wanted {
            return;
        }
        drop(std::fs::remove_dir_all(generated));
        for (name, bytes) in &wanted {
            let path = generated.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        panic!(
            "the views had an old contract; it is written again in {GENERATED}, for this commit"
        );
    }

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
