use serde_json::{Map, Value, json};
use text::{Node, Selection};

use crate::{Change, ops::cut, schema};

pub(crate) fn map(place: &mut Value, change: &Change) {
    if let Change::Steps { steps } = change {
        Selection::map(place, schema(), steps);
    }
}

pub(crate) fn describe(doc: &Node, place: &Value) -> Value {
    let Some(selection) = Selection::read(place) else {
        return Value::Null;
    };
    let selection = selection.within(doc);
    let (first, column) = line_and_column(doc, selection.from);
    let (last, _) = line_and_column(doc, selection.to);
    let mut described = Map::new();
    let lines = if first == last {
        first.to_string()
    } else {
        format!("{first}-{last}")
    };
    described.insert("lines".to_owned(), json!(lines));
    described.insert("column".to_owned(), json!(column));
    let selected = selection.text(doc);
    if !selected.is_empty() {
        described.insert("selected".to_owned(), json!(cut(&selected)));
    }
    Value::Object(described)
}

/// Both count from 1, with columns in characters.
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
