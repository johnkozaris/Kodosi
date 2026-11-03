use std::collections::{BTreeMap, HashMap, HashSet};

use document::{Ordered, Refused};
use serde_json::{Map, Value};
use text::{Node, Step};

use crate::{
    model::{Change, Model, Source, schema},
    ops::{self, Op},
};

/// Blocks in the middle of two files that a full compare still handles.
const MOST_COMPARED: usize = 2_000_000;

fn invalid(error: impl std::fmt::Display) -> Refused {
    Refused::Invalid(error.to_string())
}

pub(crate) fn block_id(node: &Node) -> Option<&str> {
    node.attrs().get("id").and_then(Value::as_str)
}

/// The top-level blocks that a range of a document touches.
fn touched_blocks(doc: &Node, from: usize, to: usize) -> Vec<String> {
    let Ok(first) = doc.resolve(from) else {
        return Vec::new();
    };
    let Ok(last) = doc.resolve(to) else {
        return Vec::new();
    };
    (first.index(0)..=last.index(0))
        .filter_map(|index| doc.maybe_child(index))
        .filter_map(block_id)
        .map(str::to_owned)
        .collect()
}

/// Runs steps on a copy, so a failed step changes nothing.
pub(crate) fn run_steps(doc: &Node, steps: &[Value]) -> Result<(Node, Vec<String>), text::Failed> {
    let mut doc = doc.clone();
    let mut touched = HashSet::new();
    for json in steps {
        let step = Step::from_json(schema(), json)?;
        if let Some((from, to)) = step.range() {
            touched.extend(touched_blocks(&doc, from, to));
        }
        doc = step.apply(schema(), &doc)?;
    }
    Ok((doc, touched.into_iter().collect()))
}

pub(crate) fn apply(
    model: &mut Model,
    change: Change,
    since: &[Ordered<Change>],
) -> Result<Change, Refused> {
    match change {
        Change::Steps {
            steps,
            keep,
            sources,
            ..
        } => {
            let moved = since
                .iter()
                .any(|ordered| matches!(ordered.change, Change::Steps { .. }));
            if moved {
                return Err(Refused::Behind(
                    "The text moved. Move the steps and send them again.".to_owned(),
                ));
            }
            let (doc, touched) = run_steps(&model.doc, &steps).map_err(invalid)?;
            model.doc = doc;
            model.raws.extend(keep.clone());
            add_sources(model, &sources);
            Ok(Change::Steps {
                steps,
                touched,
                keep,
                sources,
            })
        }
        Change::Ops { ops, track, author } => {
            for ordered in since {
                let Change::Steps { touched, .. } = &ordered.change else {
                    continue;
                };
                let conflict = ops
                    .iter()
                    .flat_map(Op::targets)
                    .find(|target| touched.iter().any(|block| block == target));
                if let Some(block) = conflict {
                    return Err(Refused::Behind(format!(
                        "{} changed block {block} after you read it. Read again.",
                        ordered.author
                    )));
                }
            }
            let steps =
                ops::to_steps(&model.doc, &ops, track, &author).map_err(Refused::Invalid)?;
            let (doc, touched) = run_steps(&model.doc, &steps).map_err(invalid)?;
            model.doc = doc;
            Ok(Change::Steps {
                steps,
                touched,
                keep: BTreeMap::new(),
                sources: BTreeMap::new(),
            })
        }
        Change::Merge { ops, keep, sources } => {
            let steps = ops::merge_steps(&model.doc, &ops).map_err(Refused::Invalid)?;
            let (doc, touched) = run_steps(&model.doc, &steps).map_err(invalid)?;
            model.doc = doc;
            model.raws.extend(keep.clone());
            add_sources(model, &sources);
            Ok(Change::Steps {
                steps,
                touched,
                keep,
                sources,
            })
        }
    }
}

fn add_sources(model: &mut Model, sources: &BTreeMap<String, String>) {
    if sources.is_empty() {
        return;
    }
    let found: HashMap<&str, &Node> = model
        .doc
        .content()
        .children()
        .iter()
        .filter_map(|block| Some((block_id(block)?, block)))
        .collect();
    let mut added = Vec::new();
    for (id, xml) in sources {
        if let Some(node) = found.get(id.as_str()) {
            added.push((
                id.clone(),
                Source {
                    node: (*node).clone(),
                    xml: xml.clone(),
                },
            ));
        }
    }
    model.sources.extend(added);
}

/// The JSON of a block without its id, because the same block can have two ids.
fn content_key(node: &Node) -> String {
    let mut json = node.to_json(schema());
    if let Some(attrs) = json.get_mut("attrs").and_then(Value::as_object_mut) {
        attrs.remove("id");
    }
    json.to_string()
}

/// The blocks that both lists keep, as pairs of indexes.
fn common_blocks(from: &[String], to: &[String]) -> Vec<(usize, usize)> {
    let mut start = 0;
    while start < from.len() && start < to.len() && from[start] == to[start] {
        start += 1;
    }
    let mut tail = 0;
    while from.len() - tail > start
        && to.len() - tail > start
        && from[from.len() - tail - 1] == to[to.len() - tail - 1]
    {
        tail += 1;
    }
    let (from_end, to_end) = (from.len() - tail, to.len() - tail);
    let mut pairs: Vec<(usize, usize)> = (0..start).map(|index| (index, index)).collect();
    let (rows, columns) = (from_end - start, to_end - start);
    if rows.saturating_mul(columns) <= MOST_COMPARED && rows > 0 && columns > 0 {
        let mut table = vec![vec![0_u32; columns + 1]; rows + 1];
        for row in (0..rows).rev() {
            for column in (0..columns).rev() {
                table[row][column] = if from[start + row] == to[start + column] {
                    table[row + 1][column + 1] + 1
                } else {
                    table[row + 1][column].max(table[row][column + 1])
                };
            }
        }
        let (mut row, mut column) = (0, 0);
        while row < rows && column < columns {
            if from[start + row] == to[start + column] {
                pairs.push((start + row, start + column));
                row += 1;
                column += 1;
            } else if table[row + 1][column] >= table[row][column + 1] {
                row += 1;
            } else {
                column += 1;
            }
        }
    }
    pairs.extend((0..tail).map(|offset| (from_end + offset, to_end + offset)));
    pairs
}

/// What turns one file's blocks into another's, named by block, so it holds on any later
/// state.
pub(crate) fn compare(from: &Model, to: &Model) -> Vec<Change> {
    let keys = |model: &Model| -> Vec<String> {
        model
            .doc
            .content()
            .children()
            .iter()
            .map(content_key)
            .collect()
    };
    let (from_keys, to_keys) = (keys(from), keys(to));
    if from_keys == to_keys {
        return Vec::new();
    }
    let kept = common_blocks(&from_keys, &to_keys);
    let from_blocks = from.doc.content().children();
    let to_blocks = to.doc.content().children();
    let mut ops = Vec::new();
    let mut keep = BTreeMap::new();
    let mut sources = BTreeMap::new();
    let mut add_piece = |node: &Node, ops_node: Value| {
        collect_raws(&ops_node, to, &mut keep);
        if let Some(id) = block_id(node)
            && let Some(source) = to.sources.get(id)
        {
            sources.insert(id.to_owned(), source.xml.clone());
        }
        ops_node
    };
    let (mut from_at, mut to_at) = (0_usize, 0_usize);
    for (from_kept, to_kept) in kept
        .iter()
        .copied()
        .chain([(from_blocks.len(), to_blocks.len())])
    {
        let mut anchor = from_at
            .checked_sub(1)
            .and_then(|index| from_blocks.get(index))
            .and_then(block_id)
            .map(str::to_owned);
        while from_at < from_kept && to_at < to_kept {
            let node = &to_blocks[to_at];
            let id = block_id(&from_blocks[from_at])
                .unwrap_or_default()
                .to_owned();
            anchor = Some(id.clone());
            ops.push(Op::ReplaceBlock {
                block: id,
                node: add_piece(node, node.to_json(schema())),
            });
            from_at += 1;
            to_at += 1;
        }
        while from_at < from_kept {
            ops.push(Op::DeleteBlock {
                block: block_id(&from_blocks[from_at])
                    .unwrap_or_default()
                    .to_owned(),
            });
            from_at += 1;
        }
        while to_at < to_kept {
            let node = &to_blocks[to_at];
            ops.push(Op::InsertBlock {
                after: anchor.clone(),
                node: add_piece(node, node.to_json(schema())),
            });
            anchor = block_id(node).map(str::to_owned);
            to_at += 1;
        }
        from_at = from_kept + 1;
        to_at = to_kept + 1;
    }
    vec![Change::Merge { ops, keep, sources }]
}

/// Finds the pieces of original XML that a block's attributes name.
fn collect_raws(json: &Value, model: &Model, keep: &mut BTreeMap<String, String>) {
    match json {
        Value::Object(fields) => {
            add_raws(fields, model, keep);
            for value in fields.values() {
                collect_raws(value, model, keep);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_raws(value, model, keep);
            }
        }
        _ => {}
    }
}

fn add_raws(fields: &Map<String, Value>, model: &Model, keep: &mut BTreeMap<String, String>) {
    for name in ["props", "raw"] {
        if let Some(raw) = fields.get(name).and_then(Value::as_str)
            && let Some(xml) = model.raws.get(raw)
        {
            keep.insert(raw.to_owned(), xml.clone());
        }
    }
}
