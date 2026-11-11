use serde_json::{Map, Value, json};
use text::{Node, Step};

use crate::{
    apply::block_id,
    model::{Change, schema},
};

fn position(place: &Value, key: &str) -> Option<usize> {
    usize::try_from(place.get(key)?.as_u64()?).ok()
}

pub(crate) fn map(place: &mut Value, change: &Change) {
    let Change::Steps { steps, .. } = change else {
        return;
    };
    let (Some(mut from), Some(mut to)) = (position(place, "from"), position(place, "to")) else {
        return;
    };
    for json in steps {
        let Ok(step) = Step::from_json(schema(), json) else {
            return;
        };
        let map = step.map();
        from = map.map(from, 1);
        to = map.map(to, 1);
    }
    place["from"] = json!(from);
    place["to"] = json!(to.max(from));
}

pub(crate) fn describe(doc: &Node, place: &Value) -> Value {
    let (Some(from), Some(to)) = (position(place, "from"), position(place, "to")) else {
        return Value::Null;
    };
    let size = doc.content().size();
    let from = from.min(size);
    let to = to.clamp(from, size);
    let mut described = Map::new();
    let block = doc
        .resolve(from)
        .ok()
        .and_then(|at| doc.maybe_child(at.index(0)))
        .and_then(block_id);
    if let Some(block) = block {
        described.insert("block".to_owned(), json!(block));
    }
    let words = doc.slice(from, to).map_or_else(
        |_| String::new(),
        |slice| {
            slice
                .content
                .children()
                .iter()
                .map(Node::text_content)
                .collect::<Vec<_>>()
                .join("\n")
        },
    );
    if !words.is_empty() {
        described.insert("selected".to_owned(), json!(words));
    }
    Value::Object(described)
}
