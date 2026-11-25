use bytes::{BufMut, Bytes, BytesMut};
use serde::{Deserialize, Serialize};

use super::{Size, State};
use crate::link::FrameKind;

const JSON: u8 = 0;
const FRAME: u8 = 1;
const FRAME_HEADER: usize = 1 + 4 + 1;
const KEY: u8 = 1;
const ANCHOR: u8 = 2;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum ToWindow {
    /// Initial handshake from a viewer:
    /// - `anchors`: decoder supports anchor-frame recovery after dropped frames.
    /// - `opened`: user explicitly opened the view (false on reconnect or visibility resume).
    /// - `layer`: client can host the Core Animation layer instead of video when local.
    Watch {
        size: Size,
        #[serde(default)]
        anchors: bool,
        #[serde(default)]
        opened: bool,
        #[serde(default)]
        layer: bool,
    },
    Ack {
        frame: u32,
    },
    Keyframe,
    Size {
        size: Size,
    },
    Input {
        input: Input,
    },
}

/// Pointer coordinates are in page CSS pixels.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum Input {
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
    Open {
        address: String,
    },
    Back,
    Forward,
    Reload,
    Tab {
        id: String,
    },
    NewTab,
    CloseTab {
        id: String,
    },
    Choose {
        menu: u32,
        #[serde(default)]
        choice: Option<u32>,
    },
    Dialog {
        accept: bool,
        #[serde(default)]
        text: Option<String>,
    },
    Files {
        paths: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum MouseAction {
    Press,
    Release,
    Move,
    Wheel,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum MouseButton {
    #[default]
    None,
    Left,
    Middle,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum KeyAction {
    Down,
    Up,
}

/// Annex B H.264 video frame. A delta frame depends on the one before it, so a viewer that
/// missed one resumes at the next keyframe or anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Frame {
    pub(crate) id: u32,
    pub(crate) kind: FrameKind,
    pub(crate) data: Bytes,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ToViewer {
    Frame(Frame),
    State(Box<State>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Malformed;

impl ToWindow {
    pub(crate) fn decode(message: &[u8]) -> Result<Self, Malformed> {
        match message.split_first() {
            Some((&JSON, body)) => serde_json::from_slice(body).map_err(|_| Malformed),
            _ => Err(Malformed),
        }
    }
}

impl ToViewer {
    pub(crate) fn encode(&self) -> Bytes {
        match self {
            Self::Frame(frame) => frame.encode(),
            Self::State(state) => json_message(state),
        }
    }
}

impl Frame {
    fn encode(&self) -> Bytes {
        let mut message = BytesMut::with_capacity(FRAME_HEADER + self.data.len());
        message.put_u8(FRAME);
        message.put_u32(self.id);
        message.put_u8(match self.kind {
            FrameKind::Delta => 0,
            FrameKind::Key => KEY,
            FrameKind::Anchor => ANCHOR,
        });
        message.put_slice(&self.data);
        message.freeze()
    }
}

fn json_message(value: &impl Serialize) -> Bytes {
    let mut message = vec![JSON];
    if serde_json::to_writer(&mut message, value).is_err() {
        message.truncate(1);
    }
    message.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_has_the_header_that_viewers_read() {
        for (kind, flags) in [
            (FrameKind::Key, 1),
            (FrameKind::Anchor, 2),
            (FrameKind::Delta, 0),
        ] {
            let frame = Frame {
                id: 7,
                kind,
                data: Bytes::from_static(&[0, 0, 0, 1, 0x65]),
            };
            assert_eq!(
                &ToViewer::Frame(frame).encode()[..],
                &[FRAME, 0, 0, 0, 7, flags, 0, 0, 0, 1, 0x65][..]
            );
        }
    }
}
