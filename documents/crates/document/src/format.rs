use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::{Digest as _, Sha256};

use crate::contract::{Hash, Rev, Seq};

/// Pictures and other large parts. They never travel inside a change, and a change names them
/// by hash.
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

    /// Takes an asset that a client sent, when its bytes fit its name.
    pub fn take(&mut self, hash: Hash, bytes: &[u8]) -> bool {
        let fits = <[u8; 32]>::from(Sha256::digest(bytes)) == hash;
        if fits {
            self.held.entry(hash).or_insert_with(|| Arc::from(bytes));
        }
        fits
    }

    pub fn get(&self, hash: &Hash) -> Option<&Arc<[u8]>> {
        self.held.get(hash)
    }

    pub fn bytes(&self) -> usize {
        self.held.values().map(|bytes| bytes.len()).sum()
    }
}

/// One change in the order, with who made it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ordered<C> {
    pub rev: Rev,
    pub author: String,
    pub client: String,
    pub seq: Seq,
    pub change: C,
}

/// All that a view needs to show a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot(pub Value);

/// What a query answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer(pub Value);

/// Why a change does not go in the order.
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

/// One op that an agent can send, with its fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideOp {
    pub name: String,
    pub does: String,
    pub fields: Vec<GuideField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

/// Each op of a format, for agents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Guide {
    pub format: String,
    pub ops: Vec<GuideOp>,
}

/// One kind of file. Each function takes values and gives values, so a format has no network,
/// disk or clock, and a test can run it with no server.
pub trait Format: 'static {
    type Model: Send;
    type Change: Serialize + DeserializeOwned + Clone + Send + 'static;
    type Query: DeserializeOwned;

    /// Whether a file opens live. A file that grows past this while it is open stays live, so
    /// the writes of other programs still merge.
    fn opens(_file: &[u8]) -> Result<(), Unreadable> {
        Ok(())
    }

    /// Reads a file. Parts that views load by hash go in `assets`.
    fn read(file: &[u8], assets: &mut Assets) -> Result<Self::Model, Unreadable>;

    fn snapshot(model: &Self::Model) -> Snapshot;

    /// Puts a change in the model, or changes nothing and refuses. `since` holds what came after
    /// the sender's base. The result is the change as views must see it: a named op becomes
    /// steps or fields.
    fn apply(
        model: &mut Self::Model,
        change: Self::Change,
        since: &[Ordered<Self::Change>],
        assets: &mut Assets,
    ) -> Result<Self::Change, Refused>;

    /// What turns one state into another, in a form that holds on any later state.
    fn compare(from: &Self::Model, to: &Self::Model) -> Vec<Self::Change>;

    /// How far apart the two states lie that `compare` found these changes between. An outside
    /// write merges from the nearest saved state, which is the one that its writer read.
    fn distance(changes: &[Self::Change]) -> usize {
        changes.len()
    }

    /// Gives the bytes of the file. `file` is the file as it is on disk, and only what someone
    /// edited differs from it.
    fn write(model: &Self::Model, file: &[u8], assets: &Assets) -> Result<Vec<u8>, Unwritable>;

    fn query(model: &Self::Model, query: Self::Query) -> Answer;

    fn guide() -> Guide;

    /// Whether this change must leave no trace behind, such as a redaction. The file is then
    /// saved at once, and a new journal starts.
    fn seals(_change: &Self::Change) -> bool {
        false
    }

    /// Moves a view's place across a change that came after it, as the change moves the content.
    /// A place that names its parts by id stays as it is.
    fn map_place(_place: &mut Value, _change: &Self::Change) {}

    /// A place in the words of `read`, for agents.
    fn describe_place(_model: &Self::Model, place: &Value) -> Value {
        place.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_asset_with_the_wrong_name_is_refused() {
        let mut assets = Assets::default();
        let hash = assets.put(b"picture");
        let mut other = Assets::default();
        assert!(!other.take([0; 32], b"picture"));
        assert!(other.take(hash, b"picture"));
        assert_eq!(other.get(&hash).map(|bytes| bytes.len()), Some(7));
    }
}
