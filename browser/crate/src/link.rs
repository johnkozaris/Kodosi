//! What a host and its browser engine say on their one local WebSocket.
//!
//! The host listens, writes the port and a token for the engine, and the engine connects once
//! with that token. Commands and reports are JSON text; each encoded picture is a binary
//! message.

use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::message::{Dialog, MenuItem, Size};

/// The exit code of an engine that does not start as root on Linux, where Chromium's sandbox
/// needs a normal user.
pub const AS_ROOT: i32 = 73;

/// The host bounds what a page says, so a bigger icon shows as none.
pub const MOST_ICON: usize = 12 * 1024;

/// The bytes before a picture's data: its window, its kind, its time and its mark.
const FRAME_HEADER: usize = 8 + 1 + 8 + 8;
const KEY: u8 = 1;
const ANCHOR: u8 = 2;
const MARKED: u8 = 4;

/// The host writes the address into the engine's profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Address {
    pub port: u16,
    pub token: String,
}

impl Address {
    pub fn path(&self) -> String {
        format!("/{}", self.token)
    }
}

/// A command of the host. Windows and host pages have the ids that the host gives them; tabs
/// have the engine's ids.
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
    },
    Stop {
        window: i64,
    },
    Keyframe {
        window: i64,
    },
    /// A frame that each viewer can take after frames that it missed: an anchor when the encoder
    /// has marks that every viewer holds, or else a key frame.
    Recover {
        window: i64,
    },
    /// Marks of frames that every viewer took, which an anchor may refer to.
    Held {
        window: i64,
        marks: Vec<u64>,
    },
    Bitrate {
        window: i64,
        bitrate: u32,
    },
    Activate {
        tab: i64,
    },
    /// With an ask, the engine names the tab's target once it knows it.
    NewTab {
        window: i64,
        address: String,
        ask: Option<i64>,
    },
    CloseTab {
        tab: i64,
    },
    /// The engine shows no dialog itself.
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
    /// A picture of a page reaches no farther than its view, so a larger picture needs a larger
    /// view.
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

/// The pages that a tab opens are tabs of its window too.
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
    Tabs {
        tabs: Vec<EngineTab>,
    },
    /// A CSS cursor name.
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
    /// The icon that a tab's page names, as PNG in base64, or nothing.
    Icon {
        tab: i64,
        png: String,
    },
    Failed {
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
    /// A dialog that went without an answer, such as when its page left.
    DialogClosed {
        tab: i64,
    },
}

/// What a frame needs from the frames before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    /// The frame before it.
    Delta,
    /// Nothing.
    Key,
    /// Only frames whose marks the host said every viewer holds, so a viewer that missed the
    /// frames before it can go on from here.
    Anchor,
}

/// H.264 in Annex B form. The encoder gives a mark to a frame that a later anchor may refer to,
/// once every viewer holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedFrame {
    pub window: i64,
    pub kind: FrameKind,
    /// Microseconds.
    pub timestamp: u64,
    pub mark: Option<u64>,
    pub data: Bytes,
}

impl EncodedFrame {
    pub fn to_message(&self) -> Vec<u8> {
        let mut flags = match self.kind {
            FrameKind::Delta => 0,
            FrameKind::Key => KEY,
            FrameKind::Anchor => ANCHOR,
        };
        if self.mark.is_some() {
            flags |= MARKED;
        }
        let mut message = Vec::with_capacity(FRAME_HEADER + self.data.len());
        message.extend_from_slice(&self.window.to_be_bytes());
        message.push(flags);
        message.extend_from_slice(&self.timestamp.to_be_bytes());
        message.extend_from_slice(&self.mark.unwrap_or_default().to_be_bytes());
        message.extend_from_slice(&self.data);
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
            kind,
            timestamp: u64::from_be_bytes(header[9..17].try_into().ok()?),
            mark: (flags & MARKED != 0)
                .then(|| header[17..].try_into().ok().map(u64::from_be_bytes))
                .flatten(),
            data: message.slice(FRAME_HEADER..),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_keeps_its_window_kind_time_mark_and_data() {
        for (kind, mark) in [
            (FrameKind::Key, None),
            (FrameKind::Anchor, Some(0)),
            (FrameKind::Delta, Some(u64::MAX)),
        ] {
            let frame = EncodedFrame {
                window: -7,
                kind,
                timestamp: 1_234_567,
                mark,
                data: Bytes::from_static(b"\0\0\0\x01picture"),
            };
            let message = Bytes::from(frame.to_message());
            assert_eq!(EncodedFrame::from_message(&message), Some(frame));
        }
    }

    #[test]
    fn a_short_message_is_no_frame() {
        assert_eq!(
            EncodedFrame::from_message(&Bytes::from_static(b"short")),
            None
        );
    }

    #[test]
    fn commands_and_reports_have_their_wire_form() {
        let command = ToEngine::Capture {
            window: 3,
            tab: 9,
            bitrate: 4_000_000,
        };
        assert_eq!(
            serde_json::to_value(&command).unwrap(),
            serde_json::json!({"type": "capture", "window": 3, "tab": 9, "bitrate": 4_000_000})
        );
        let report: FromEngine = serde_json::from_value(serde_json::json!({
            "type": "pageOpened", "page": 2, "target": "T"
        }))
        .unwrap();
        assert_eq!(
            report,
            FromEngine::PageOpened {
                page: 2,
                target: "T".into()
            }
        );
    }
}
