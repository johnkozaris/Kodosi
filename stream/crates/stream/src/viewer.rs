//! Messages between a stream and a viewer.
//!
//! Each message starts with a tag byte: JSON control messages, or a frame with a 7-byte header
//! (tag, id, flags, codec), then the region it shows as four 32-bit floats when its flags say so,
//! then its Annex B data.

use bytes::{BufMut, Bytes, BytesMut};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{Codec, FrameKind, Region};

pub const JSON: u8 = 0;
pub const FRAME: u8 = 1;
const FRAME_HEADER: usize = 1 + 4 + 1 + 1;
pub(crate) const KEY: u8 = 1;
pub(crate) const ANCHOR: u8 = 2;
const REGION: u8 = 4;
const REGION_BYTES: usize = 4 * 4;

/// A view that the host let in. `id` names one view; the views of one person share `person`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Viewer {
    pub id: String,
    pub person: String,
    pub name: String,
    /// True when the view runs on the computer that the stream comes from.
    pub local: bool,
}

/// Viewport size in CSS pixels and the device's scale factor.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Size {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
}

/// A picture of the stream. A delta frame depends on the one before it, so a viewer that missed
/// one resumes at the next keyframe or anchor.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub id: u32,
    pub codec: Codec,
    pub kind: FrameKind,
    /// The part of the source that the picture shows, when it shows only a part.
    pub region: Option<Region>,
    pub data: Bytes,
}

impl Frame {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a region's points fit a 32-bit float"
    )]
    pub fn encode(&self) -> Bytes {
        let mut message = BytesMut::with_capacity(FRAME_HEADER + REGION_BYTES + self.data.len());
        message.put_u8(FRAME);
        message.put_u32(self.id);
        let mut flags = match self.kind {
            FrameKind::Delta => 0,
            FrameKind::Key => KEY,
            FrameKind::Anchor => ANCHOR,
        };
        if self.region.is_some() {
            flags |= REGION;
        }
        message.put_u8(flags);
        message.put_u8(self.codec.wire());
        if let Some(region) = self.region {
            for value in [region.x, region.y, region.width, region.height] {
                message.put_f32(value as f32);
            }
        }
        message.put_slice(&self.data);
        message.freeze()
    }
}

pub fn json(value: &impl Serialize) -> Bytes {
    let mut message = vec![JSON];
    if serde_json::to_writer(&mut message, value).is_err() {
        message.truncate(1);
    }
    message.into()
}

pub fn from_json<T: DeserializeOwned>(message: &[u8]) -> Option<T> {
    match message.split_first() {
        Some((&JSON, body)) => serde_json::from_slice(body).ok(),
        _ => None,
    }
}

/// Pointer coordinates are in the stream's CSS pixels. Modifiers: alt 1, control 2, meta 4,
/// shift 8.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Input {
    Mouse {
        action: MouseAction,
        x: f64,
        y: f64,
        #[serde(default)]
        button: MouseButton,
        #[serde(default)]
        clicks: u8,
        #[serde(default)]
        delta_x: f64,
        #[serde(default)]
        delta_y: f64,
        #[serde(default)]
        modifiers: u8,
    },
    /// `key` and `code` are DOM `KeyboardEvent` values; `key_code` is the Windows virtual key.
    Key {
        action: KeyAction,
        key: String,
        code: String,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        key_code: u32,
        #[serde(default)]
        modifiers: u8,
    },
    Text {
        text: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MouseAction {
    Press,
    Release,
    Move,
    Wheel,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MouseButton {
    #[default]
    None,
    Left,
    Middle,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyAction {
    Down,
    Up,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_has_the_header_that_viewers_read() {
        for (kind, codec, flags, byte) in [
            (FrameKind::Key, Codec::H264, 1, 0),
            (FrameKind::Anchor, Codec::H264, 2, 0),
            (FrameKind::Delta, Codec::Hevc, 0, 1),
        ] {
            let frame = Frame {
                id: 7,
                codec,
                kind,
                region: None,
                data: Bytes::from_static(&[0, 0, 0, 1, 0x65]),
            };
            assert_eq!(
                &frame.encode()[..],
                &[FRAME, 0, 0, 0, 7, flags, byte, 0, 0, 0, 1, 0x65][..]
            );
        }
        let zoomed = Frame {
            id: 7,
            codec: Codec::H264,
            kind: FrameKind::Delta,
            region: Some(Region {
                x: 1.0,
                y: 2.0,
                width: 0.5,
                height: -2.0,
            }),
            data: Bytes::from_static(&[0, 0, 0, 1, 0x41]),
        };
        assert_eq!(
            &zoomed.encode()[..],
            &[
                FRAME, 0, 0, 0, 7, 4, 0, 0x3f, 0x80, 0, 0, 0x40, 0, 0, 0, 0x3f, 0, 0, 0, 0xc0, 0,
                0, 0, 0, 0, 0, 1, 0x41
            ][..]
        );
    }
}
