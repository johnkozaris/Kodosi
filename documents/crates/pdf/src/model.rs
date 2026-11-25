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

/// `rect` is `[left, bottom, right, top]` in PDF points from the bottom-left origin.
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

/// Note and field edits are key-addressed (last write wins). Page operations (`Work`) run on the
/// host and broadcast `Worked` with a new PDF asset hash for views to reload.
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
    /// External write replaced the underlying PDF pages.
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
    /// Baseline PDF bytes as of the last page operation; incremental saves append to this buffer.
    pub(crate) file: Arc<Vec<u8>>,
    pub(crate) asset: String,
    pub(crate) pages: Vec<Page>,
    pub(crate) notes: BTreeMap<String, Note>,
    pub(crate) fields: BTreeMap<String, String>,
    /// Notes and fields already stored in `file`, diffed on save to emit minimal incremental
    /// updates.
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
