use std::{collections::BTreeMap, sync::Arc};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::Op;

pub struct Pdf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub id: String,
    pub width: f64,
    pub height: f64,
    pub rotation: i64,
}

/// In PDF points from the bottom left.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub page: String,
    pub kind: String,
    pub rect: [f64; 4],
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default)]
    pub author: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PageWork {
    Order { pages: Vec<String> },
    Rotate { page: String, degrees: i64 },
    Remove { page: String },
    Redact { page: String, rects: Vec<[f64; 4]> },
}

/// Notes and form values name a key, so the last one wins. Page work names a page, the server
/// does it, and views load the file again from the asset that the change names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Change {
    Note {
        id: String,
        note: Option<Note>,
    },
    Field {
        name: String,
        value: Option<String>,
    },
    Work {
        work: PageWork,
    },
    Worked {
        work: PageWork,
        asset: String,
        pages: Vec<Page>,
    },
    /// Another program changed the pages.
    Base {
        asset: String,
    },
    Ops {
        ops: Vec<Op>,
    },
    Group {
        changes: Vec<Self>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Query {
    Read {
        #[serde(default)]
        page: Option<u32>,
    },
    Check,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    /// The file as the last page work left it. A save appends to these bytes.
    pub(crate) file: Arc<Vec<u8>>,
    pub(crate) asset: String,
    pub(crate) pages: Vec<Page>,
    pub(crate) notes: BTreeMap<String, Note>,
    pub(crate) fields: BTreeMap<String, String>,
    /// What `file` holds, to find what a save must add.
    pub(crate) file_notes: BTreeMap<String, Note>,
    pub(crate) file_fields: BTreeMap<String, String>,
}

impl Model {
    pub(crate) fn snapshot(&self) -> Value {
        json!({
            "asset": self.asset,
            "pages": self.pages,
            "notes": self.notes,
            "fields": self.fields,
        })
    }
}
