use bytes::{Buf, BufMut, Bytes, BytesMut};
use serde::{Deserialize, Serialize};

const JSON: u8 = 0;
const FRAME: u8 = 1;
const FRAME_HEADER: usize = 1 + 4 + 1 + 8;

/// Who looks at a window. The host decides this, and the window trusts it. `id` names one view,
/// and each view of one person has the same `person`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Viewer {
    pub id: String,
    pub person: String,
    pub name: String,
}

/// The view of a viewer, in CSS pixels and the scale of its screen.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Size {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
}

/// What a viewer sends to its window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ToWindow {
    Watch { size: Size },
    Ack { frame: u32 },
    Keyframe,
    Size { size: Size },
    Input { input: Input },
}

/// A person's step in a window, in the CSS pixels of its page.
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
    Key {
        action: KeyAction,
        key: String,
        code: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        choice: Option<u32>,
    },
    Dialog {
        accept: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    Files {
        paths: Vec<String>,
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

/// All that a viewer shows around the picture.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub tabs: Vec<Tab>,
    pub width: u32,
    pub height: u32,
    pub shape: String,
    pub pointers: Vec<Pointer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu: Option<Menu>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialog: Option<Dialog>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chooser: Option<Chooser>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copied: Option<String>,
    pub downloads: Vec<Download>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tab {
    pub id: String,
    pub title: String,
    pub address: String,
    pub active: bool,
    pub loading: bool,
}

/// Where a viewer's pointer is in the page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pointer {
    pub viewer: String,
    pub person: String,
    pub name: String,
    pub x: f64,
    pub y: f64,
}

/// A list that a page opened, which the viewer that asked draws at its pointer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Menu {
    pub id: u32,
    pub viewer: String,
    pub items: Vec<MenuItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuItem {
    pub label: String,
    pub enabled: bool,
    pub chosen: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DialogKind {
    Alert,
    Confirm,
    Prompt,
    Beforeunload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dialog {
    pub kind: DialogKind,
    pub message: String,
    pub text: String,
}

/// A page waits for a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chooser {
    pub several: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    pub name: String,
    pub path: String,
    pub done: bool,
}

/// One encoded picture. The data is H.264 in Annex B form, and it depends on the frames since
/// the last key frame of this viewer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub id: u32,
    pub key: bool,
    pub timestamp: u64,
    pub data: Bytes,
}

/// What a window sends to a viewer.
#[derive(Debug, Clone, PartialEq)]
pub enum ToViewer {
    Frame(Frame),
    State(Box<State>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Malformed;

impl std::fmt::Display for Malformed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("The message of the browser is not valid.")
    }
}

impl std::error::Error for Malformed {}

impl ToWindow {
    pub fn encode(&self) -> Bytes {
        json_message(self)
    }

    pub fn decode(message: &[u8]) -> Result<Self, Malformed> {
        match message.split_first() {
            Some((&JSON, body)) => serde_json::from_slice(body).map_err(|_| Malformed),
            _ => Err(Malformed),
        }
    }
}

impl ToViewer {
    pub fn encode(&self) -> Bytes {
        match self {
            Self::Frame(frame) => frame.encode(),
            Self::State(state) => json_message(state),
        }
    }

    pub fn decode(message: &[u8]) -> Result<Self, Malformed> {
        match message.split_first() {
            Some((&JSON, body)) => serde_json::from_slice(body)
                .map(Self::State)
                .map_err(|_| Malformed),
            Some((&FRAME, _)) => Frame::decode(message).map(Self::Frame),
            _ => Err(Malformed),
        }
    }
}

impl Frame {
    fn encode(&self) -> Bytes {
        let mut message = BytesMut::with_capacity(FRAME_HEADER + self.data.len());
        message.put_u8(FRAME);
        message.put_u32(self.id);
        message.put_u8(u8::from(self.key));
        message.put_u64(self.timestamp);
        message.put_slice(&self.data);
        message.freeze()
    }

    fn decode(mut message: &[u8]) -> Result<Self, Malformed> {
        if message.len() < FRAME_HEADER {
            return Err(Malformed);
        }
        message.advance(1);
        let id = message.get_u32();
        let key = message.get_u8() != 0;
        let timestamp = message.get_u64();
        Ok(Self {
            id,
            key,
            timestamp,
            data: Bytes::copy_from_slice(message),
        })
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
    fn a_frame_keeps_its_bytes() {
        let frame = Frame {
            id: 7,
            key: true,
            timestamp: 90_000,
            data: Bytes::from_static(&[0, 0, 0, 1, 0x65]),
        };
        let message = ToViewer::Frame(frame.clone()).encode();
        assert_eq!(ToViewer::decode(&message), Ok(ToViewer::Frame(frame)));
    }

    #[test]
    fn a_short_frame_is_refused() {
        assert_eq!(ToViewer::decode(&[FRAME, 0, 1]), Err(Malformed));
    }

    #[test]
    fn input_uses_the_contract_names() {
        let message = ToWindow::Input {
            input: Input::Mouse {
                action: MouseAction::Press,
                x: 4.0,
                y: 8.0,
                button: MouseButton::Left,
                clicks: 1,
                delta_x: 0.0,
                delta_y: 0.0,
                modifiers: 0,
            },
        };
        let encoded = message.encode();
        let text = std::str::from_utf8(&encoded[1..]).unwrap_or_default();
        assert!(text.contains(r#""type":"input""#));
        assert!(text.contains(r#""kind":"mouse""#));
        assert_eq!(ToWindow::decode(&encoded), Ok(message));
    }
}
