//! Messages between the host and the "Kodosi Screen" helper over a local socket.
//!
//! Each message is a 4-byte big-endian length, then a tag byte: a JSON control message, or an
//! encoded frame with a 10-byte header (flags, codec, mark), then the region it shows as four
//! 32-bit floats when its flags say so, then its Annex B data.

use std::io;

use bytes::{BufMut, Bytes, BytesMut};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use stream::{Codec, Encoded, Format, FrameKind, Pace, Region, viewer::Input};
use tokio::io::{AsyncRead, AsyncReadExt as _};

/// The helper ships inside the same app as the host, so both sides must match.
pub const VERSION: u32 = 3;
/// The largest message either side takes.
const MOST_MESSAGE: usize = 16 * 1024 * 1024;
/// The most clipboard text that goes between a viewer and the computer.
pub const MOST_CLIPBOARD: usize = 1024 * 1024;
const JSON: u8 = 0;
const FRAME: u8 = 1;
const FRAME_HEADER: usize = 1 + 1 + 1 + 8;
const REGION_BYTES: usize = 4 * 4;
const KEY: u8 = 1;
const ANCHOR: u8 = 2;
const MARKED: u8 = 4;
const REGION: u8 = 8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ToHelper {
    /// Starts or changes the capture: `region` of `source` in its points, scaled to
    /// `width` × `height` pixels.
    Capture {
        source: Source,
        region: Option<Region>,
        width: u32,
        height: u32,
        format: Format,
        bitrate: u32,
        pace: Pace,
    },
    Stop,
    Keyframe,
    Recover,
    Held {
        marks: Vec<u64>,
    },
    Bitrate {
        bitrate: u32,
    },
    Pace {
        pace: Pace,
    },
    /// Pointer coordinates are in the shown source's points.
    Input {
        input: Input,
    },
    /// What the computer shows while people watch, or `None` once nobody does.
    Notice {
        notice: Option<Notice>,
    },
    /// Asks the system for what the helper may not do yet, which may show the person at the
    /// computer its prompts.
    Ask,
    /// Puts a viewer's text on the computer's clipboard, before the viewer pastes it.
    Clipboard {
        text: String,
    },
    Quit,
}

/// The notice on the computer, in the language of its owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    /// Who watches, as a sentence.
    pub watching: String,
    /// The label of the button that ends the sharing.
    pub stop: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FromHelper {
    /// The helper's first message.
    Hello {
        token: String,
        version: u32,
        encodable: Vec<Codec>,
        /// The largest picture, long side by short side, that the helper encodes in real time.
        largest: (u32, u32),
        access: Access,
        away: Option<Away>,
    },
    Sources {
        sources: Sources,
    },
    Access {
        access: Access,
    },
    /// Why nobody can see the screen now, or `None` once it shows again.
    Away {
        away: Option<Away>,
    },
    /// Where the pointer is in the shown source's points, or `None` when it is elsewhere.
    Pointer {
        pointer: Option<Pointer>,
    },
    /// A pointer image that `Pointer` refers to by `id`, sent once.
    Shape {
        shape: Shape,
    },
    /// Text copied on the computer while it is captured.
    Copied {
        text: String,
    },
    /// The person at the computer stopped the sharing.
    Stopped,
    Failed {
        problem: String,
    },
}

/// Why the screen shows nothing for a while, which ends by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Away {
    Locked,
    /// The display sleeps and does not wake, as when a laptop's lid is closed.
    Asleep,
    /// Someone else's account is on the computer's display.
    AnotherAccount,
}

/// What the system lets the helper do.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Access {
    pub screen: bool,
    pub input: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sources {
    /// The main display comes first.
    pub displays: Vec<Display>,
    pub windows: Vec<Window>,
}

impl Sources {
    /// The size in points and the pixels per point of a source.
    pub fn size(&self, source: Source) -> Option<(f64, f64, f64)> {
        match source {
            Source::Display { id } => self
                .displays
                .iter()
                .find(|display| display.id == id)
                .map(|display| (display.width, display.height, display.scale)),
            Source::Window { id } => self
                .windows
                .iter()
                .find(|window| window.id == id)
                .map(|window| (window.width, window.height, window.scale)),
        }
    }

    pub fn main(&self) -> Option<Source> {
        self.displays
            .first()
            .map(|display| Source::Display { id: display.id })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Display {
    pub id: u32,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    pub id: u32,
    pub app: String,
    pub title: String,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Source {
    Display { id: u32 },
    Window { id: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pointer {
    pub x: f64,
    pub y: f64,
    pub shape: u64,
}

/// A pointer image with its hotspot, both in points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shape {
    pub id: u64,
    /// Base64 PNG.
    pub png: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Message<T> {
    Said(T),
    Frame(Encoded),
}

pub fn json(value: &impl Serialize) -> Bytes {
    let body = serde_json::to_vec(value).unwrap_or_default();
    let mut message = BytesMut::with_capacity(5 + body.len());
    message.put_u32(u32::try_from(1 + body.len()).unwrap_or(u32::MAX));
    message.put_u8(JSON);
    message.put_slice(&body);
    message.freeze()
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "a region's points fit a 32-bit float"
)]
pub fn frame(picture: &Encoded) -> Bytes {
    let mut flags = match picture.kind {
        FrameKind::Delta => 0,
        FrameKind::Key => KEY,
        FrameKind::Anchor => ANCHOR,
    };
    if picture.mark.is_some() {
        flags |= MARKED;
    }
    let mut length = FRAME_HEADER + picture.data.len();
    if picture.region.is_some() {
        flags |= REGION;
        length += REGION_BYTES;
    }
    let mut message = BytesMut::with_capacity(4 + length);
    message.put_u32(u32::try_from(length).unwrap_or(u32::MAX));
    message.put_u8(FRAME);
    message.put_u8(flags);
    message.put_u8(picture.codec.wire());
    message.put_u64(picture.mark.unwrap_or_default());
    if let Some(region) = picture.region {
        for value in [region.x, region.y, region.width, region.height] {
            message.put_f32(value as f32);
        }
    }
    message.put_slice(&picture.data);
    message.freeze()
}

/// Reads the next message, or `None` once the other side closed the socket.
pub async fn read<T: DeserializeOwned>(
    from: &mut (impl AsyncRead + Unpin),
) -> io::Result<Option<Message<T>>> {
    let length = match from.read_u32().await {
        Ok(length) => usize::try_from(length).unwrap_or(usize::MAX),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    };
    if length == 0 || length > MOST_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "a message of the wrong size",
        ));
    }
    let mut body = vec![0; length];
    from.read_exact(&mut body).await?;
    let body = Bytes::from(body);
    let message = match body[0] {
        JSON => serde_json::from_slice(&body[1..]).ok().map(Message::Said),
        FRAME if body.len() >= FRAME_HEADER => {
            let flags = body[1];
            let kind = if flags & KEY != 0 {
                FrameKind::Key
            } else if flags & ANCHOR != 0 {
                FrameKind::Anchor
            } else {
                FrameKind::Delta
            };
            let mark = u64::from_be_bytes(body[3..FRAME_HEADER].try_into().unwrap_or_default());
            let (region, data) = if flags & REGION == 0 {
                (None, FRAME_HEADER)
            } else {
                (region(&body[FRAME_HEADER..]), FRAME_HEADER + REGION_BYTES)
            };
            Codec::from_wire(body[2])
                .filter(|_| data <= body.len() && (flags & REGION == 0 || region.is_some()))
                .map(|codec| {
                    Message::Frame(Encoded {
                        codec,
                        kind,
                        mark: (flags & MARKED != 0).then_some(mark),
                        region,
                        data: body.slice(data..),
                    })
                })
        }
        _ => None,
    };
    message.map_or_else(
        || {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "a message that does not read",
            ))
        },
        |message| Ok(Some(message)),
    )
}

fn region(bytes: &[u8]) -> Option<Region> {
    let mut values = bytes
        .get(..REGION_BYTES)?
        .as_chunks::<4>()
        .0
        .iter()
        .map(|value| f64::from(f32::from_be_bytes(*value)));
    Some(Region {
        x: values.next()?,
        y: values.next()?,
        width: values.next()?,
        height: values.next()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn messages_and_frames_come_back_as_they_went() {
        let said = FromHelper::Pointer {
            pointer: Some(Pointer {
                x: 4.5,
                y: 9.0,
                shape: 7,
            }),
        };
        let picture = Encoded {
            codec: Codec::Hevc,
            kind: FrameKind::Anchor,
            mark: Some(3),
            region: None,
            data: Bytes::from_static(&[0, 0, 0, 1, 0x26]),
        };
        let zoomed = Encoded {
            codec: Codec::H264,
            kind: FrameKind::Delta,
            mark: None,
            region: Some(Region {
                x: 10.5,
                y: 20.0,
                width: 300.25,
                height: 200.0,
            }),
            data: Bytes::from_static(&[0, 0, 0, 1, 0x41]),
        };
        let mut written = json(&said).to_vec();
        written.extend_from_slice(&frame(&picture));
        written.extend_from_slice(&frame(&zoomed));
        let mut reading = written.as_slice();
        assert_eq!(
            read::<FromHelper>(&mut reading).await.unwrap(),
            Some(Message::Said(said))
        );
        assert_eq!(
            read::<FromHelper>(&mut reading).await.unwrap(),
            Some(Message::Frame(picture))
        );
        assert_eq!(
            read::<FromHelper>(&mut reading).await.unwrap(),
            Some(Message::Frame(zoomed))
        );
        assert_eq!(read::<FromHelper>(&mut reading).await.unwrap(), None);
    }
}
