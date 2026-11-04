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

/// The .docx format.
pub struct Docx;

/// A top-level block as it came from the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Source {
    pub node: Node,
    pub xml: String,
}

/// The live copy of a .docx file.
#[derive(Debug, Clone)]
pub struct Model {
    pub(crate) doc: Node,
    pub(crate) sources: HashMap<String, Source>,
    /// Pieces of the original XML that the tree does not hold, by the hash in its attributes.
    pub(crate) raws: HashMap<String, String>,
    /// Styles, numbering and the page, which a view needs in order to draw.
    pub(crate) view: Value,
}

/// One batch of edits. A view sends steps. The CLI sends named ops, and the server turns them
/// into steps, so a view never sees an op.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Change {
    Steps {
        steps: Vec<Value>,
        /// The ids of the top-level blocks that the steps touch, which the server fills in.
        #[serde(default)]
        touched: Vec<String>,
        /// Pieces of XML that came with a merge, so a replay of the journal has them too.
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
    /// What a compare found in a write of another program.
    Merge {
        ops: Vec<Op>,
        keep: BTreeMap<String, String>,
        sources: BTreeMap<String, String>,
    },
}

/// What read and check ask.
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
