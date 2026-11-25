use std::{collections::HashMap, sync::Arc};

use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::{Digest as _, Sha256};

use crate::contract::{HASH_BYTES, Hash, Rev, Seq};

pub fn short_hash(text: &str) -> String {
    crate::contract::hex(&Sha256::digest(text.as_bytes())[..HASH_BYTES / 4])
}

/// Content-addressed store for binary assets referenced by hash in snapshots and changes.
#[derive(Debug, Clone, Default)]
pub struct Assets {
    held: HashMap<Hash, Arc<[u8]>>,
}

impl Assets {
    pub fn put(&mut self, bytes: &[u8]) -> Hash {
        let hash: Hash = Sha256::digest(bytes).into();
        self.held.entry(hash).or_insert_with(|| Arc::from(bytes));
        hash
    }

    pub fn get(&self, hash: &Hash) -> Option<&Arc<[u8]>> {
        self.held.get(hash)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ordered<C> {
    pub rev: Rev,
    pub author: String,
    pub client: String,
    pub seq: Seq,
    pub change: C,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot(pub Value);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer(pub Value);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refused {
    #[error("Someone changed this first: {0}")]
    Behind(String),
    #[error("The change is not valid: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("The file cannot be read: {0}")]
pub struct Unreadable(pub String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("The file cannot be written: {0}")]
pub struct Unwritable(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideOp {
    pub name: String,
    pub does: String,
    pub fields: Vec<GuideField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideField {
    pub name: String,
    pub kind: String,
    pub required: bool,
    pub does: String,
}

#[must_use]
pub fn guide_op(name: &str, does: &str, fields: Vec<GuideField>) -> GuideOp {
    GuideOp {
        name: name.to_owned(),
        does: does.to_owned(),
        fields,
    }
}

#[must_use]
pub fn guide_field(name: &str, kind: &str, required: bool, does: &str) -> GuideField {
    GuideField {
        name: name.to_owned(),
        kind: kind.to_owned(),
        required,
        does: does.to_owned(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guide {
    pub format: String,
    pub ops: Vec<GuideOp>,
}

/// Pure format implementation with no I/O, clock, or threading dependencies.
pub trait Format: 'static {
    type Model: Send;
    type Change: Serialize + DeserializeOwned + Clone + Send + 'static;
    type Query: DeserializeOwned;

    /// Validates whether a file may be opened for live editing. Checked only on open so files that
    /// grow past limits while open keep merging external writes.
    fn opens(_file: &[u8]) -> Result<(), Unreadable> {
        Ok(())
    }

    /// Parses `file` into a model, storing binary blobs fetched lazily by views in `assets`.
    fn read(file: &[u8], assets: &mut Assets) -> Result<Self::Model, Unreadable>;

    fn snapshot(model: &Self::Model) -> Snapshot;

    /// Rejects changes that only the host may produce (such as external-write merges) so clients
    /// cannot inject arbitrary raw content.
    fn sendable(_change: &Self::Change) -> Result<(), String> {
        Ok(())
    }

    /// Atomically applies `change` given intervening changes `since` the sender's base revision.
    /// Returns the normalized change broadcast to views (e.g. expanding CLI ops into steps/fields).
    fn apply(
        model: &mut Self::Model,
        change: Self::Change,
        since: &[Ordered<Self::Change>],
        assets: &mut Assets,
    ) -> Result<Self::Change, Refused>;

    /// Computes changes from `from` to `to` that can be applied on top of later states.
    fn compare(from: &Self::Model, to: &Self::Model) -> Vec<Self::Change>;

    /// Edit distance for changes returned by `compare`, used to pick the closest saved state as
    /// the merge base for an external write.
    fn distance(changes: &[Self::Change]) -> usize {
        changes.len()
    }

    /// Serializes `model` by patching `file` (the current bytes on disk) so untouched parts stay
    /// intact.
    fn write(model: &Self::Model, file: &[u8], assets: &Assets) -> Result<Vec<u8>, Unwritable>;

    fn query(model: &Self::Model, query: Self::Query) -> Answer;

    fn guide() -> Guide;

    /// Returns true if `change` must not remain in history (e.g. a redaction), triggering an
    /// immediate save and fresh journal.
    fn seals(_change: &Self::Change) -> bool {
        false
    }

    /// Transforms a selection (`place`) past a subsequent change; formats addressing by ID can
    /// leave it unchanged.
    fn map_place(_place: &mut Value, _change: &Self::Change) {}

    /// Formats a view's selection in the coordinate space returned by `read` queries.
    fn describe_place(_model: &Self::Model, place: &Value) -> Value {
        place.clone()
    }
}
