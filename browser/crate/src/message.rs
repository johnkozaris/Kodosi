use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[cfg(feature = "host")]
mod wire;

#[cfg(feature = "host")]
pub(crate) use wire::{Act, Input, ToViewer, ToWindow};

pub use stream::viewer::Viewer;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub tabs: Vec<Tab>,
    pub width: u32,
    pub height: u32,
    pub shape: String,
    pub pointers: Vec<Pointer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub menu: Option<Menu>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dialog: Option<Dialog>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chooser: Option<Chooser>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copied: Option<String>,
    pub downloads: Vec<Download>,
    /// Newly updated tab favicons (base64 PNG) not yet sent to this viewer; empty string clears.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub icons: BTreeMap<String, String>,
    /// Core Animation context ID, sent only to a local view that hosts the layer, since any
    /// process in the user's session that knows it can show the page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tab {
    pub id: String,
    pub title: String,
    pub address: String,
    pub active: bool,
    pub loading: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pointer {
    pub viewer: String,
    pub person: String,
    pub name: String,
    pub x: f64,
    pub y: f64,
}

/// Popup `<select>` list or right-click context menu, positioned in page CSS pixels.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Menu {
    pub id: u32,
    pub viewer: String,
    pub x: f64,
    pub y: f64,
    pub items: Vec<MenuItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<MenuContext>,
}

/// Right-click target metadata. Clients build localized menu items for it: choice 0 copies the
/// selection and choice 1 copies the link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuContext {
    /// Truncated start of the selection, long enough to ask about it.
    pub selection: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Chooser {
    pub several: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    pub name: String,
    pub path: String,
    pub done: bool,
}
