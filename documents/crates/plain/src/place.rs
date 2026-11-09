//! Where a person is in a text: the positions `from` and `to` of their selection.

use serde_json::{Map, Value, json};
use text::{Node, Step};

use crate::{Change, ops::cut, schema};

fn position(place: &Value, key: &str) -> Option<usize> {
    usize::try_from(place.get(key)?.as_u64()?).ok()
}

/// Moves a place across the steps of a change, as a selection moves.
pub(crate) fn map(place: &mut Value, change: &Change) {
    let Change::Steps { steps } = change else {
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

/// The lines of a place as a read names them, the column where it starts, and the text that it
/// selects.
pub(crate) fn describe(doc: &Node, place: &Value) -> Value {
    let (Some(from), Some(to)) = (position(place, "from"), position(place, "to")) else {
        return Value::Null;
    };
    let size = doc.content().size();
    let from = from.min(size);
    let to = to.clamp(from, size);
    let (first, column) = line_and_column(doc, from);
    let (last, _) = line_and_column(doc, to);
    let mut described = Map::new();
    let lines = if first == last {
        first.to_string()
    } else {
        format!("{first}-{last}")
    };
    described.insert("lines".to_owned(), json!(lines));
    described.insert("column".to_owned(), json!(column));
    let selected = doc.slice(from, to).map_or_else(
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
    if !selected.is_empty() {
        described.insert("selected".to_owned(), json!(cut(&selected)));
    }
    Value::Object(described)
}

/// The line and the column of a position, both from 1, with columns in characters.
fn line_and_column(doc: &Node, position: usize) -> (usize, usize) {
    let Ok(at) = doc.resolve(position) else {
        return (1, 1);
    };
    let line = at.index(0).min(doc.child_count().saturating_sub(1));
    if at.depth() == 0 {
        return (line + 1, 1);
    }
    let mut units = 0;
    let column = at
        .parent()
        .text_content()
        .chars()
        .take_while(|letter| {
            units += letter.len_utf16();
            units <= at.parent_offset()
        })
        .count();
    (line + 1, column + 1)
}
