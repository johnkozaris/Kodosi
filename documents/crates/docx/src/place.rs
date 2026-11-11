use serde_json::{Map, Value, json};
use text::{Node, Selection};

use crate::{
    apply::block_id,
    model::{Change, schema},
};

pub(crate) fn map(place: &mut Value, change: &Change) {
    if let Change::Steps { steps, .. } = change {
        Selection::map(place, schema(), steps);
    }
}

pub(crate) fn describe(doc: &Node, place: &Value) -> Value {
    let Some(selection) = Selection::read(place) else {
        return Value::Null;
    };
    let selection = selection.within(doc);
    let mut described = Map::new();
    let block = doc
        .resolve(selection.from)
        .ok()
        .and_then(|at| doc.maybe_child(at.index(0)))
        .and_then(block_id);
    if let Some(block) = block {
        described.insert("block".to_owned(), json!(block));
    }
    let words = selection.text(doc);
    if !words.is_empty() {
        described.insert("selected".to_owned(), json!(words));
    }
    Value::Object(described)
}
