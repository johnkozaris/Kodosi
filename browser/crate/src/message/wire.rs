use bytes::Bytes;
use serde::Deserialize;
use stream::{
    Decoder, Size,
    viewer::{self, Frame},
};

use super::State;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum ToWindow {
    /// Initial handshake from a viewer:
    /// - `decoders`: the codecs the viewer decodes, and which of them in hardware.
    /// - `anchors`: decoder supports anchor-frame recovery after dropped frames.
    /// - `opened`: user explicitly opened the view (false on reconnect or visibility resume).
    /// - `layer`: client can host the Core Animation layer instead of video when local.
    Watch {
        size: Size,
        decoders: Vec<Decoder>,
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
    /// The bitrate that the transport carries to the viewer, from the host's runtime while the
    /// viewer's frames go as datagrams.
    Rate {
        bitrate: u64,
    },
    /// A frame that went to the viewer as datagrams and that it cannot have, from the host's
    /// runtime.
    Lost {
        frame: u32,
    },
}

/// The pointer and keys that every stream takes, or what a viewer does to the browser.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub(crate) enum Input {
    Device(viewer::Input),
    Browser(Act),
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum Act {
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

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ToViewer {
    Frame(Frame),
    State(Box<State>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Malformed;

impl ToWindow {
    pub(crate) fn decode(message: &[u8]) -> Result<Self, Malformed> {
        viewer::from_json(message).ok_or(Malformed)
    }
}

impl ToViewer {
    pub(crate) fn encode(&self) -> Bytes {
        match self {
            Self::Frame(frame) => frame.encode(),
            Self::State(state) => viewer::json(state),
        }
    }
}
