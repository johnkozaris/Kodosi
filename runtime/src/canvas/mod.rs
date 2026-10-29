use serde::{Deserialize, Serialize};

#[cfg(feature = "host")]
mod actor;
#[cfg(feature = "host")]
mod browser;
#[cfg(feature = "host")]
mod files;
#[cfg(feature = "host")]
mod page;
#[cfg(all(test, feature = "host"))]
mod tests;

#[cfg(feature = "host")]
pub(crate) use actor::{CanvasHandle, close_left, keep_profiles};

pub const MOST_ITEMS: usize = 20;
pub const MOST_FILE_BYTES: u64 = 32 * 1024 * 1024;
pub const PART_BYTES: usize = 256 * 1024;
pub const MOST_PATHS: usize = MOST_ITEMS;
pub const MOST_ARGUMENTS: usize = 512;
const MOST_TABS: usize = 10;
const MOST_TITLE: usize = 512;
const MOST_URL: usize = 512;
const MOST_KEY: usize = 1024;
const MOST_NAME: usize = 512;
const MOST_EVENTS: usize = 64;
const MOST_TYPED: usize = 16 * 1024;
const MOST_PROBLEM: usize = 1024;
const MOST_PICTURE: usize = 400 * 1024;
const MOST_SIDE: u32 = 4096;
const MOST_TEXT: usize = 1024 * 1024;
const MOST_POSITION: i32 = 100_000;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Canvas {
    pub sequence: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<CanvasItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser: Option<CanvasBrowser>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanvasItem {
    pub key: String,
    pub name: String,
    pub kind: CanvasKind,
    pub revision: u64,
    pub bytes: u64,
    pub changed: u64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub missing: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum CanvasKind {
    Page,
    Picture,
    Pdf,
    Office,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanvasBrowser {
    pub state: BrowserState,
    pub changed: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<BrowserTab>,
    pub width: u32,
    pub height: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doing: Option<BrowserDoing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<BrowserAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum BrowserState {
    Starting,
    Awake,
    Asleep,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserTab {
    pub id: String,
    pub title: String,
    pub url: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserAction {
    pub agent: String,
    pub words: String,
}

/// What the agent does in the browser while its command runs, and the middle of the element
/// that it acts on, in the browser's CSS pixels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserDoing {
    pub agent: String,
    pub kind: BrowserDoingKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point: Option<BrowserPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum BrowserDoingKind {
    Open,
    Click,
    Type,
    Scroll,
    Read,
    Wait,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub struct BrowserPoint {
    pub x: u32,
    pub y: u32,
}

/// What a device that shows a terminal asks of its canvas, on this computer or across the network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CanvasRequest {
    Read {
        key: String,
        revision: u64,
        offset: u64,
    },
    Remove {
        #[serde(default)]
        keys: Vec<String>,
    },
    Picture {
        after: u64,
    },
    Input {
        events: Vec<BrowserInput>,
        width: u32,
        height: u32,
    },
    Act {
        action: BrowserAct,
    },
}

/// A person's pointer, key or text in the shared browser, in the browser's own pixels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum BrowserInput {
    Mouse {
        event_type: MouseEvent,
        x: i32,
        y: i32,
        #[serde(default)]
        button: MouseButton,
        #[serde(default)]
        click_count: u8,
        #[serde(default)]
        delta_x: i32,
        #[serde(default)]
        delta_y: i32,
        #[serde(default)]
        modifiers: u8,
    },
    Key {
        event_type: KeyEvent,
        key: String,
        code: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default)]
        key_code: u32,
        #[serde(default)]
        modifiers: u8,
    },
    Touch {
        event_type: TouchEvent,
        points: Vec<TouchPoint>,
        #[serde(default)]
        modifiers: u8,
    },
    Text {
        text: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum MouseEvent {
    MousePressed,
    MouseReleased,
    MouseMoved,
    MouseWheel,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    #[default]
    None,
    Left,
    Middle,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum KeyEvent {
    KeyDown,
    KeyUp,
    RawKeyDown,
    Char,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum TouchEvent {
    TouchStart,
    TouchMove,
    TouchEnd,
    TouchCancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub struct TouchPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(
    tag = "do",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum BrowserAct {
    Open { url: String },
    Back,
    Forward,
    Reload,
    Tab { id: String },
    NewTab,
    CloseTab { id: String },
    Wake { width: u32, height: u32 },
}

/// What the canvas answers. A device checks an answer from another computer before it uses it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(
    tag = "answer",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CanvasAnswer {
    Done,
    Text {
        text: String,
        code: i32,
    },
    Part {
        key: String,
        name: String,
        kind: CanvasKind,
        revision: u64,
        offset: u64,
        total: u64,
        data: String,
    },
    Stale {
        key: String,
        revision: u64,
    },
    Picture {
        sequence: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        frame: Option<BrowserFrame>,
    },
}

/// One JPEG picture of the browser, in base64.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserFrame {
    pub width: u32,
    pub height: u32,
    pub data: String,
}

impl Canvas {
    #[cfg(feature = "host")]
    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty() && self.browser.is_none()
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.items.len() <= MOST_ITEMS
            && self.items.iter().all(CanvasItem::is_valid)
            && self.browser.as_ref().is_none_or(CanvasBrowser::is_valid)
    }
}

impl CanvasItem {
    fn is_valid(&self) -> bool {
        bounded(&self.key, MOST_KEY) && bounded(&self.name, MOST_NAME) && !self.name.is_empty()
    }
}

impl CanvasBrowser {
    fn is_valid(&self) -> bool {
        self.tabs.len() <= MOST_TABS
            && self.tabs.iter().all(|tab| {
                bounded(&tab.id, 64)
                    && bounded(&tab.title, MOST_TITLE)
                    && bounded(&tab.url, MOST_URL)
            })
            && self.doing.as_ref().is_none_or(|doing| {
                bounded(&doing.agent, 64)
                    && doing
                        .point
                        .is_none_or(|point| point.x <= MOST_SIDE && point.y <= MOST_SIDE)
            })
            && self
                .last
                .as_ref()
                .is_none_or(|last| bounded(&last.agent, 64) && bounded(&last.words, 256))
            && self
                .problem
                .as_deref()
                .is_none_or(|problem| bounded(problem, MOST_PROBLEM))
    }
}

impl CanvasRequest {
    pub(crate) fn is_valid(&self) -> bool {
        match self {
            Self::Read { key, .. } => bounded(key, MOST_KEY),
            Self::Remove { keys } => {
                keys.len() <= MOST_ITEMS && keys.iter().all(|key| bounded(key, MOST_KEY))
            }
            Self::Picture { .. } => true,
            Self::Input { events, .. } => {
                !events.is_empty()
                    && events.len() <= MOST_EVENTS
                    && events.iter().all(BrowserInput::is_valid)
            }
            Self::Act { action } => match action {
                BrowserAct::Open { url } => !url.trim().is_empty() && bounded(url, 4096),
                BrowserAct::Tab { id } | BrowserAct::CloseTab { id } => {
                    !id.is_empty() && bounded(id, 64)
                }
                BrowserAct::Back
                | BrowserAct::Forward
                | BrowserAct::Reload
                | BrowserAct::NewTab
                | BrowserAct::Wake { .. } => true,
            },
        }
    }
}

impl BrowserInput {
    fn is_valid(&self) -> bool {
        let near = |value: &i32| value.unsigned_abs() <= MOST_POSITION.unsigned_abs();
        match self {
            Self::Mouse {
                x,
                y,
                delta_x,
                delta_y,
                ..
            } => [x, y, delta_x, delta_y].into_iter().all(near),
            Self::Key {
                key, code, text, ..
            } => {
                bounded(key, 32)
                    && bounded(code, 32)
                    && text.as_deref().is_none_or(|text| bounded(text, 16))
            }
            Self::Touch { points, .. } => {
                points.len() <= 10 && points.iter().all(|point| near(&point.x) && near(&point.y))
            }
            Self::Text { text } => !text.is_empty() && bounded(text, MOST_TYPED),
        }
    }
}

impl CanvasAnswer {
    pub(crate) fn is_valid(&self) -> bool {
        match self {
            Self::Done => true,
            Self::Text { text, .. } => text.len() <= MOST_TEXT,
            Self::Part {
                key,
                name,
                offset,
                total,
                data,
                ..
            } => {
                bounded(key, MOST_KEY)
                    && bounded(name, MOST_NAME)
                    && *total <= MOST_FILE_BYTES * 4
                    && offset <= total
                    && data.len() <= PART_BYTES.div_ceil(3) * 4
            }
            Self::Stale { key, .. } => bounded(key, MOST_KEY),
            Self::Picture { frame, .. } => frame.as_ref().is_none_or(|frame| {
                frame.width <= 8192 && frame.height <= 8192 && frame.data.len() <= MOST_PICTURE
            }),
        }
    }
}

fn bounded(text: &str, most: usize) -> bool {
    text.len() <= most && !text.contains('\0')
}

/// What a file shows as on the canvas, by its extension.
pub fn kind_of(path: &std::path::Path) -> Option<CanvasKind> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "html" | "htm" | "md" | "markdown" | "txt" | "log" | "csv" | "tsv" | "json" | "yaml"
        | "yml" | "toml" | "xml" | "diff" | "patch" => CanvasKind::Page,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" => CanvasKind::Picture,
        "pdf" => CanvasKind::Pdf,
        "docx" | "doc" | "xlsx" | "xls" | "pptx" | "ppt" | "odt" | "ods" | "odp" | "rtf"
        | "pages" | "numbers" | "key" => CanvasKind::Office,
        _ => return None,
    })
}
