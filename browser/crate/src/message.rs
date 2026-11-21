use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[cfg(feature = "host")]
mod wire;

#[cfg(feature = "host")]
pub(crate) use wire::{Frame, Input, KeyAction, MouseAction, MouseButton, ToViewer, ToWindow};

/// The host decides who looks, and the window trusts it. `id` names one view, and each view of
/// one person has the same `person`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Viewer {
    pub id: String,
    pub person: String,
    pub name: String,
    /// The view runs on the computer of the browser, with no link between them.
    pub local: bool,
}

/// In CSS pixels, with the scale of the viewer's screen.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Size {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
}

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
    /// The icons of tabs, as PNG in base64, that this viewer has not had yet. An empty one
    /// takes a tab's icon away.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub icons: BTreeMap<String, String>,
    /// The Core Animation context that shows the page, only for a view on the browser's own
    /// computer that hosts it: another process of the person's session that knows it can show
    /// the page.
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

/// A page's list or the menu of a right-click, which opens where the viewer last pointed, in the
/// CSS pixels of the page.
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

/// What a right-click was on. The apps give its menu their own items in their own words: choice
/// 0 copies the selection and choice 1 the link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuContext {
    /// The start of the selection, enough to ask about it.
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
