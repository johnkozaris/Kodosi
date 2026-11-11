use serde::{Deserialize, Serialize};

#[cfg(feature = "host")]
mod actor;
#[cfg(feature = "host")]
mod attach;
#[cfg(feature = "host")]
mod browser;
#[cfg(feature = "host")]
mod engine;
#[cfg(feature = "host")]
mod files;
#[cfg(feature = "host")]
mod page;
#[cfg(feature = "host")]
mod render;
#[cfg(all(test, feature = "host"))]
mod tests;

#[cfg(feature = "host")]
pub(crate) use actor::{CanvasHandle, CanvasServices, Show, close_left, keep_profiles};

pub const MOST_ITEMS: usize = 20;
pub const MOST_FILE_BYTES: u64 = 32 * 1024 * 1024;
pub const PART_BYTES: usize = 256 * 1024;
/// The key of the pipe that carries the live picture of a terminal's browser.
pub(crate) const BROWSER_PIPE: &str = "browser";
/// The frames that wait on each side of a document's pipe.
pub(crate) const PIPE_QUEUE: usize = 64;
pub(crate) const MOST_PIPE_FRAME: usize = 64 * 1024 * 1024;
pub const MOST_PATHS: usize = MOST_ITEMS;
pub const MOST_ARGUMENTS: usize = 512;
const MOST_TABS: usize = 10;
const MOST_TITLE: usize = 512;
const MOST_URL: usize = 512;
const MOST_KEY: usize = 1024;
const MOST_NAME: usize = 512;
const MOST_PROBLEM: usize = 1024;
const MOST_SIDE: u32 = 4096;
const MOST_TEXT: usize = 1024 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Canvas {
    pub sequence: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<CanvasItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser: Option<CanvasBrowser>,
    /// What the agent showed last, with `kodosi show` or in the browser.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<CanvasOpen>,
    /// What changed last: a file of the canvas, by anyone, or the browser by a command of the
    /// agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<CanvasOpen>,
}

/// An item of the canvas by its key, or the browser, and when the agent showed it or it changed.
///
/// `at` is in milliseconds since 1970 on the terminal's computer, later than each show and change
/// before. Each device acts on each one once, and only on one after the device's app started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanvasOpen {
    pub key: String,
    pub at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanvasItem {
    pub key: String,
    pub name: String,
    pub kind: CanvasKind,
    /// The live view of this file, when the file has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<DocumentView>,
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
    Document,
    Sheet,
    Presentation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum DocumentView {
    Docx,
    Xlsx,
    Pptx,
    Pdf,
    Text,
}

impl DocumentView {
    /// The name of the view in the view bundle.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Docx => "docx",
            Self::Xlsx => "xlsx",
            Self::Pptx => "pptx",
            Self::Pdf => "pdf",
            Self::Text => "text",
        }
    }
}

/// A device's two ends of a live document's pipe: frames go in and come out, and the pipe ends
/// when either side lets go.
#[derive(Debug)]
pub struct PipeEnds {
    pub sender: tokio::sync::mpsc::Sender<Vec<u8>>,
    pub receiver: tokio::sync::mpsc::Receiver<Vec<u8>>,
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
    /// The part of the first download of Kodosi Browser, from 0 to 100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub getting: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doing: Option<BrowserDoing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<BrowserAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<crate::text::Text>,
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
        keys: Vec<String>,
    },
    /// One part of a file that a person adds. The file goes into the folder of the terminal.
    Attach {
        upload: String,
        name: String,
        offset: u64,
        data: String,
        last: bool,
        /// A file for a page of the browser goes into the folder of the terminal, of any kind,
        /// and not on the canvas.
        page: bool,
    },
    Act {
        action: BrowserAct,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(
    tag = "do",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum BrowserAct {
    Wake,
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
    Attached {
        key: String,
    },
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
            && self
                .open
                .as_ref()
                .is_none_or(|open| bounded(&open.key, MOST_KEY))
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
                .as_ref()
                .is_none_or(|problem| bounded(&problem.to_string(), MOST_PROBLEM))
    }
}

impl CanvasRequest {
    pub(crate) fn is_valid(&self) -> bool {
        match self {
            Self::Read { key, .. } => bounded(key, MOST_KEY),
            Self::Remove { keys } => {
                keys.len() <= MOST_ITEMS && keys.iter().all(|key| bounded(key, MOST_KEY))
            }
            Self::Attach {
                upload, name, data, ..
            } => {
                uuid::Uuid::parse_str(upload).is_ok()
                    && !name.is_empty()
                    && bounded(name, MOST_NAME)
                    && !matches!(name.as_str(), "." | "..")
                    && !name.contains(['/', '\\', ':', '\0'])
                    && data.len() <= PART_BYTES.div_ceil(3) * 4
            }
            Self::Act {
                action: BrowserAct::Wake,
            } => true,
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
            Self::Stale { key, .. } | Self::Attached { key } => bounded(key, MOST_KEY),
        }
    }
}

fn bounded(text: &str, most: usize) -> bool {
    text.len() <= most && !text.contains('\0')
}

/// Whether `kodosi show` takes a word as an address for the shared browser.
pub fn is_address(word: &str) -> bool {
    let lower = word.get(..8).unwrap_or(word).to_ascii_lowercase();
    ["http://", "https://", "file://"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
        && bounded(word, MOST_URL)
        && !word.chars().any(char::is_control)
}

/// The live view of a file, by its extension.
pub fn view_of(path: &std::path::Path) -> Option<DocumentView> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "docx" => DocumentView::Docx,
        "xlsx" => DocumentView::Xlsx,
        "pptx" => DocumentView::Pptx,
        "pdf" => DocumentView::Pdf,
        "txt" | "json" | "yaml" | "yml" | "toml" | "xml" => DocumentView::Text,
        _ => return None,
    })
}

/// What a file shows as on the canvas, by its extension.
pub fn kind_of(path: &std::path::Path) -> Option<CanvasKind> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "html" | "htm" | "md" | "markdown" | "txt" | "log" | "csv" | "tsv" | "json" | "yaml"
        | "yml" | "toml" | "xml" | "diff" | "patch" => CanvasKind::Page,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" => CanvasKind::Picture,
        "pdf" => CanvasKind::Pdf,
        "docx" | "doc" | "odt" | "rtf" | "pages" => CanvasKind::Document,
        "xlsx" | "xls" | "ods" | "numbers" => CanvasKind::Sheet,
        "pptx" | "ppt" | "odp" | "key" => CanvasKind::Presentation,
        _ => return None,
    })
}
