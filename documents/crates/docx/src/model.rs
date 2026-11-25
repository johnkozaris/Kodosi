use std::{
    collections::{BTreeMap, HashMap},
    sync::LazyLock,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use text::{Node, Schema, SchemaSpec};

use crate::Op;

static SCHEMA: LazyLock<Schema> = LazyLock::new(|| {
    let spec: SchemaSpec = serde_json::from_str(include_str!("../schema.json"))
        .unwrap_or_else(|error| unreachable!("the schema is part of this crate: {error}"));
    Schema::new(&spec)
        .unwrap_or_else(|error| unreachable!("the schema is part of this crate: {error}"))
});

pub(crate) fn schema() -> &'static Schema {
    &SCHEMA
}

pub struct Docx;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Source {
    pub node: Node,
    pub xml: String,
}

#[derive(Debug, Clone)]
pub struct Model {
    pub(crate) doc: Node,
    pub(crate) sources: HashMap<String, Source>,
    /// Original XML fragments omitted from the text tree, keyed by the hash stored in node attrs.
    pub(crate) raws: HashMap<String, String>,
    /// Styles, numbering, and page layout metadata sent to views for rendering.
    pub(crate) view: Value,
}

/// Views send `Steps`; the CLI sends `Ops`, which the host translates into `Steps` before
/// broadcasting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Change {
    Steps {
        steps: Vec<Value>,
        /// Top-level block IDs touched by `steps`, populated by the host for conflict detection.
        #[serde(default)]
        touched: Vec<String>,
        /// Raw XML fragments introduced by an external-write merge, carried so journal replay has
        /// them.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        keep: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        sources: BTreeMap<String, String>,
    },
    Ops {
        ops: Vec<Op>,
        #[serde(default)]
        track: bool,
        #[serde(default)]
        author: String,
    },
    Merge {
        ops: Vec<Op>,
        keep: BTreeMap<String, String>,
        sources: BTreeMap<String, String>,
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
        block: Option<String>,
    },
    Check,
}
