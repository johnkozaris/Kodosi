//! Local WebSocket protocol between the host and browser engine.
//!
//! The host writes its port and one-time token for the engine to connect once. Control messages
//! (`ToEngine`, `FromEngine`) use JSON text frames; encoded video frames use binary messages.

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use stream::{Codec, Encoded, Format, FrameKind, Size};

use crate::message::{Dialog, MenuItem};

/// Engine exit code when launched as root on Linux, where Chromium's sandbox requires a non-root
/// user.
pub const AS_ROOT: i32 = 73;

/// Maximum base64 favicon size accepted from a page; larger icons are dropped.
pub const MOST_ICON: usize = 12 * 1024;

/// Wire protocol version between host and engine; both sides must match.
///
/// Bump on any breaking change to `ToEngine`, `FromEngine`, or the binary frame header.
pub const VERSION: u32 = 3;

/// Binary frame header layout: window (8 bytes), flags (1 byte), codec (1 byte), mark (8 bytes).
const FRAME_HEADER: usize = 8 + 1 + 1 + 8;
const KEY: u8 = 1;
const ANCHOR: u8 = 2;
const MARKED: u8 = 4;
const STILL: u8 = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Address {
    pub port: u16,
    pub token: String,
}

impl Address {
    /// Includes `VERSION` in the WebSocket path so mismatched engine versions are rejected.
    pub fn path(&self) -> String {
        format!("/{}/{VERSION}", self.token)
    }
}

/// Host-to-engine commands. Window and host-page IDs are host-assigned; tab IDs are
/// engine-assigned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ToEngine {
    OpenWindow {
        window: i64,
        address: String,
        size: Size,
    },
    Resize {
        window: i64,
        size: Size,
    },
    Capture {
        window: i64,
        tab: i64,
        bitrate: u32,
        format: Format,
    },
    /// Restarts the window's encoder in a format that every viewer decodes.
    Format {
        window: i64,
        format: Format,
    },
    Stop {
        window: i64,
    },
    Keyframe {
        window: i64,
    },
    /// Requests a recovery frame: an anchor referencing a held mark if available, else a keyframe.
    Recover {
        window: i64,
    },
    /// Reference frame marks acknowledged by all viewers and safe for anchors to reference.
    Held {
        window: i64,
        marks: Vec<u64>,
    },
    Bitrate {
        window: i64,
        bitrate: u32,
    },
    /// Tab rendered into the window's Core Animation layer, or `None` to hold the last frame.
    Layer {
        window: i64,
        tab: Option<i64>,
    },
    /// Requests a keyframe still for a local layer-hosting view's fallback snapshot.
    Still {
        window: i64,
    },
    Activate {
        tab: i64,
    },
    /// When `ask` is set, the engine replies with `TabOpened` once the target ID is known.
    NewTab {
        window: i64,
        address: String,
        ask: Option<i64>,
    },
    CloseTab {
        tab: i64,
    },
    /// Answers a JavaScript dialog; the engine suppresses native dialogs.
    Dialog {
        tab: i64,
        accept: bool,
        text: Option<String>,
    },
    CloseWindow {
        window: i64,
    },
    Pick {
        tab: i64,
        frame: i64,
        menu: u32,
        choice: Option<u32>,
    },
    OpenPage {
        page: i64,
        width: u32,
        height: u32,
    },
    /// Resizes a host page's viewport so screenshots (which clip to the view) can cover it.
    ResizePage {
        page: i64,
        width: u32,
        height: u32,
    },
    ClosePage {
        page: i64,
    },
    Quit,
}

/// Tab metadata reported by the engine; popups opened by a tab appear as tabs in the same window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineTab {
    pub id: i64,
    pub window: i64,
    pub target: Option<String>,
    pub title: String,
    pub address: String,
    pub active: bool,
    pub loading: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FromEngine {
    /// The engine's first report: the codecs this computer encodes in real time.
    Encodes {
        codecs: Vec<Codec>,
    },
    Tabs {
        tabs: Vec<EngineTab>,
    },
    /// CSS cursor name.
    Shape {
        tab: i64,
        shape: String,
    },
    Menu {
        tab: i64,
        frame: i64,
        menu: u32,
        items: Vec<MenuItem>,
    },
    Context {
        tab: i64,
        selection: String,
        link: Option<String>,
    },
    Copied {
        tab: i64,
        text: String,
    },
    /// Base64-encoded PNG favicon, or empty to clear.
    Icon {
        tab: i64,
        png: String,
    },
    Failed {
        window: i64,
        problem: String,
    },
    /// Core Animation context ID for the window's shared layer, hostable by local apps.
    Layer {
        window: i64,
        context: u32,
    },
    /// Layer hosting is unavailable for this window; local views fall back to video.
    NoLayer {
        window: i64,
        problem: String,
    },
    PageOpened {
        page: i64,
        target: String,
    },
    TabOpened {
        ask: i64,
        target: String,
    },
    Dialog {
        tab: i64,
        dialog: Dialog,
    },
    /// Dialog dismissed without a user response (e.g., the page navigated or closed).
    DialogClosed {
        tab: i64,
    },
}

/// A picture from a window's encoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedFrame {
    pub window: i64,
    /// Standalone keyframe snapshot for layer-hosting views to display while the layer is
    /// unavailable; not part of the video stream.
    pub still: bool,
    pub picture: Encoded,
}

impl EncodedFrame {
    pub fn to_message(&self) -> Vec<u8> {
        let picture = &self.picture;
        let mut flags = match picture.kind {
            FrameKind::Delta => 0,
            FrameKind::Key => KEY,
            FrameKind::Anchor => ANCHOR,
        };
        if picture.mark.is_some() {
            flags |= MARKED;
        }
        if self.still {
            flags |= STILL;
        }
        let mut message = Vec::with_capacity(FRAME_HEADER + picture.data.len());
        message.extend_from_slice(&self.window.to_be_bytes());
        message.push(flags);
        message.push(picture.codec.wire());
        message.extend_from_slice(&picture.mark.unwrap_or_default().to_be_bytes());
        message.extend_from_slice(&picture.data);
        message
    }

    pub fn from_message(message: &Bytes) -> Option<Self> {
        let header = message.get(..FRAME_HEADER)?;
        let flags = header[8];
        let kind = if flags & KEY != 0 {
            FrameKind::Key
        } else if flags & ANCHOR != 0 {
            FrameKind::Anchor
        } else {
            FrameKind::Delta
        };
        Some(Self {
            window: i64::from_be_bytes(header[..8].try_into().ok()?),
            still: flags & STILL != 0,
            picture: Encoded {
                codec: Codec::from_wire(header[9])?,
                kind,
                mark: (flags & MARKED != 0)
                    .then(|| header[10..].try_into().ok().map(u64::from_be_bytes))
                    .flatten(),
                data: message.slice(FRAME_HEADER..),
            },
        })
    }
}
