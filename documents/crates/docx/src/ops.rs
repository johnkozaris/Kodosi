use document::{Guide, GuideField, GuideOp};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use text::{Fragment, Mark, Node, Slice, Step, StepMap};

use crate::{
    apply::run_steps,
    model::{Model, Query, schema},
};

/// What an agent means. The server turns each op into normal steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Op {
    ReplaceText {
        find: String,
        with: String,
        #[serde(default)]
        block: Option<String>,
        #[serde(default)]
        all: bool,
    },
    InsertParagraph {
        text: String,
        #[serde(default)]
        after: Option<String>,
        #[serde(default)]
        style: Option<String>,
    },
    DeleteBlock {
        block: String,
    },
    SetStyle {
        block: String,
        #[serde(default)]
        style: Option<String>,
    },
    FormatText {
        find: String,
        #[serde(default)]
        block: Option<String>,
        #[serde(default)]
        bold: bool,
        #[serde(default)]
        italic: bool,
        #[serde(default)]
        underline: bool,
        #[serde(default)]
        color: Option<String>,
    },
    InsertTable {
        rows: Vec<Vec<String>>,
        #[serde(default)]
        after: Option<String>,
    },
    SetCell {
        table: String,
        row: usize,
        column: usize,
        text: String,
    },
    AcceptChanges {
        #[serde(default)]
        block: Option<String>,
    },
    RejectChanges {
        #[serde(default)]
        block: Option<String>,
    },
    /// Only for merges of outside writes.
    ReplaceBlock {
        block: String,
        node: Value,
    },
    /// Only for merges of outside writes.
    InsertBlock {
        #[serde(default)]
        after: Option<String>,
        node: Value,
    },
}

impl Op {
    /// The blocks that this op acts on.
    pub(crate) fn targets(&self) -> Vec<String> {
        let block = match self {
            Self::ReplaceText { block, .. }
            | Self::FormatText { block, .. }
            | Self::AcceptChanges { block }
            | Self::RejectChanges { block } => block.clone(),
            Self::InsertParagraph { after, .. }
            | Self::InsertTable { after, .. }
            | Self::InsertBlock { after, .. } => after.clone(),
            Self::DeleteBlock { block }
            | Self::ReplaceBlock { block, .. }
            | Self::SetStyle { block, .. } => Some(block.clone()),
            Self::SetCell { table, .. } => Some(table.clone()),
        };
        block.into_iter().collect()
    }
}

fn field(name: &str, kind: &str, required: bool, does: &str) -> GuideField {
    GuideField {
        name: name.to_owned(),
        kind: kind.to_owned(),
        required,
        does: does.to_owned(),
    }
}

fn op(name: &str, does: &str, fields: Vec<GuideField>) -> GuideOp {
    GuideOp {
        name: name.to_owned(),
        does: does.to_owned(),
        fields,
    }
}

pub(crate) fn guide() -> Guide {
    let in_block = || field("block", "block id", false, "Looks only in this block");
    let after = || {
        field(
            "after",
            "block id",
            false,
            "Goes after this block. Without it, at the end",
        )
    };
    Guide {
        format: "docx".to_owned(),
        ops: vec![
            op(
                "replaceText",
                "Replaces text and keeps its look",
                vec![
                    field("find", "text", true, "The exact text"),
                    field("with", "text", true, "The new text"),
                    in_block(),
                    field(
                        "all",
                        "boolean",
                        false,
                        "Replaces each place, not only the first",
                    ),
                ],
            ),
            op(
                "insertParagraph",
                "Adds a paragraph",
                vec![
                    field("text", "text", true, "Its text"),
                    after(),
                    field("style", "style id", false, "Its style, such as Heading1"),
                ],
            ),
            op(
                "deleteBlock",
                "Removes a paragraph or a table",
                vec![field("block", "block id", true, "The block")],
            ),
            op(
                "setStyle",
                "Sets a paragraph's style",
                vec![
                    field("block", "block id", true, "The paragraph"),
                    field(
                        "style",
                        "style id",
                        false,
                        "The style. Without it, the default",
                    ),
                ],
            ),
            op(
                "formatText",
                "Adds a look to text",
                vec![
                    field("find", "text", true, "The exact text"),
                    in_block(),
                    field("bold", "boolean", false, "Makes it bold"),
                    field("italic", "boolean", false, "Makes it italic"),
                    field("underline", "boolean", false, "Underlines it"),
                    field("color", "hex color", false, "Its color, such as FF0000"),
                ],
            ),
            op(
                "insertTable",
                "Adds a table",
                vec![
                    field("rows", "rows of text", true, "The cells of each row"),
                    after(),
                ],
            ),
            op(
                "setCell",
                "Replaces the text of a table cell",
                vec![
                    field("table", "block id", true, "The table"),
                    field("row", "number", true, "Its row, from 0"),
                    field("column", "number", true, "Its column, from 0"),
                    field("text", "text", true, "The new text"),
                ],
            ),
            op(
                "acceptChanges",
                "Accepts tracked changes: insertions stay and deletions go",
                vec![field("block", "block id", false, "Only in this block")],
            ),
            op(
                "rejectChanges",
                "Rejects tracked changes: insertions go and deletions stay",
                vec![field("block", "block id", false, "Only in this block")],
            ),
        ],
    }
}

fn node_kind(name: &str) -> text::NodeId {
    schema().node_id(name).unwrap_or_default()
}

fn mark_of(name: &str, attrs: Option<&Value>) -> Result<Mark, String> {
    let kind = schema()
        .mark_id(name)
        .ok_or_else(|| format!("There is no mark {name}."))?;
    Mark::new(schema(), kind, attrs).map_err(|failed| failed.0)
}

/// Who makes a tracked change. A change of an agent has no date.
fn by(author: &str) -> Value {
    json!({ "author": author, "date": "" })
}

/// A new paragraph. A tracked one is an insertion: its words and the mark that ends it.
fn paragraph_of(
    text: &str,
    style: Option<&str>,
    id: Option<&str>,
    by: Option<&Value>,
) -> Result<Node, String> {
    let marks = match by {
        Some(by) => vec![mark_of("insertion", Some(by))?],
        None => Vec::new(),
    };
    let content = if text.is_empty() {
        Fragment::default()
    } else {
        Fragment::from_array(vec![
            Node::text(schema(), text, marks).map_err(|failed| failed.0)?,
        ])
    };
    Node::new(
        schema(),
        node_kind("paragraph"),
        Some(&json!({ "style": style, "id": id, "inserted": by })),
        content,
        Vec::new(),
    )
    .map_err(|failed| failed.0)
}

fn table_of(rows: &[Vec<String>], id: &str) -> Result<Node, String> {
    let mut built = Vec::with_capacity(rows.len());
    for row in rows {
        let mut cells = Vec::with_capacity(row.len());
        for cell in row {
            cells.push(
                Node::new(
                    schema(),
                    node_kind("tableCell"),
                    None,
                    Fragment::from_array(vec![paragraph_of(cell, None, None, None)?]),
                    Vec::new(),
                )
                .map_err(|failed| failed.0)?,
            );
        }
        built.push(
            Node::new(
                schema(),
                node_kind("tableRow"),
                None,
                Fragment::from_array(cells),
                Vec::new(),
            )
            .map_err(|failed| failed.0)?,
        );
    }
    Node::new(
        schema(),
        node_kind("table"),
        Some(&json!({ "id": id })),
        Fragment::from_array(built),
        Vec::new(),
    )
    .map_err(|failed| failed.0)
}

fn block_start(doc: &Node, block: &str) -> Option<(usize, Node)> {
    let mut position = 0;
    for child in doc.content().children() {
        if child.attrs().get("id").and_then(Value::as_str) == Some(block) {
            return Some((position, child.clone()));
        }
        position += child.node_size();
    }
    None
}

fn end_after(doc: &Node, after: Option<&str>) -> Result<usize, String> {
    let Some(after) = after else {
        return Ok(doc.content().size());
    };
    let (start, node) =
        block_start(doc, after).ok_or_else(|| format!("There is no block {after}."))?;
    Ok(start + node.node_size())
}

/// An id that no block has yet.
fn new_id(doc: &Node, taken: usize) -> String {
    let highest = doc
        .content()
        .children()
        .iter()
        .filter_map(|child| child.attrs().get("id").and_then(Value::as_str))
        .filter_map(|id| id.strip_prefix("new-"))
        .filter_map(|number| number.parse::<usize>().ok())
        .max()
        .unwrap_or(0);
    format!("new-{}", highest + taken + 1)
}

fn insert_step(position: usize, node: Node) -> Step {
    Step::Replace {
        from: position,
        to: position,
        slice: Slice::new(Fragment::from_array(vec![node]), 0, 0),
        structure: false,
    }
}

/// A stretch of text in one textblock, with the position of each UTF-16 unit.
struct Stretch {
    units: Vec<u16>,
    positions: Vec<usize>,
}

fn stretches_in(node: &Node, start: usize, out: &mut Vec<Stretch>) {
    if schema().is_textblock(node.kind()) {
        let mut stretch = Stretch {
            units: Vec::new(),
            positions: Vec::new(),
        };
        let mut position = start;
        for child in node.content().children() {
            if let Some(text) = child.text_of() {
                for unit in text.encode_utf16() {
                    stretch.units.push(unit);
                    stretch.positions.push(position);
                    position += 1;
                }
            } else {
                if !stretch.units.is_empty() {
                    out.push(std::mem::replace(
                        &mut stretch,
                        Stretch {
                            units: Vec::new(),
                            positions: Vec::new(),
                        },
                    ));
                }
                position += child.node_size();
            }
        }
        if !stretch.units.is_empty() {
            out.push(stretch);
        }
        return;
    }
    let mut position = start;
    for child in node.content().children() {
        stretches_in(child, position + 1, out);
        position += child.node_size();
    }
}

/// Each place of the text, as ranges of positions, from last to first.
fn find_places(doc: &Node, block: Option<&str>, find: &str) -> Result<Vec<(usize, usize)>, String> {
    let needle: Vec<u16> = find.encode_utf16().collect();
    if needle.is_empty() {
        return Err("The text to find is empty.".to_owned());
    }
    let mut stretches = Vec::new();
    if let Some(block) = block {
        let (start, node) =
            block_start(doc, block).ok_or_else(|| format!("There is no block {block}."))?;
        stretches_in(&node, start + 1, &mut stretches);
    } else {
        let mut position = 0;
        for child in doc.content().children() {
            stretches_in(child, position + 1, &mut stretches);
            position += child.node_size();
        }
    }
    let mut places = Vec::new();
    for stretch in &stretches {
        let mut index = 0;
        while index + needle.len() <= stretch.units.len() {
            if stretch.units[index..index + needle.len()] == needle[..] {
                let from = stretch.positions[index];
                let to = stretch.positions[index + needle.len() - 1] + 1;
                places.push((from, to));
                index += needle.len();
            } else {
                index += 1;
            }
        }
    }
    places.reverse();
    Ok(places)
}

fn marks_at(doc: &Node, position: usize) -> Vec<Mark> {
    doc.node_at(position)
        .ok()
        .flatten()
        .map(|node| node.marks().to_vec())
        .unwrap_or_default()
}

fn replace_text_steps(
    doc: &Node,
    (from, to): (usize, usize),
    with: &str,
    by: Option<&Value>,
) -> Result<Vec<Step>, String> {
    let mut marks = marks_at(doc, from);
    let Some(by) = by else {
        let slice = if with.is_empty() {
            Slice::empty()
        } else {
            let node = Node::text(schema(), with, marks).map_err(|failed| failed.0)?;
            Slice::new(Fragment::from_array(vec![node]), 0, 0)
        };
        return Ok(vec![Step::Replace {
            from,
            to,
            slice,
            structure: false,
        }]);
    };
    let mut steps = vec![Step::AddMark {
        from,
        to,
        mark: mark_of("deletion", Some(by))?,
    }];
    if !with.is_empty() {
        marks.push(mark_of("insertion", Some(by))?);
        let node = Node::text(schema(), with, marks).map_err(|failed| failed.0)?;
        steps.push(insert_step(to, node));
    }
    Ok(steps)
}

/// Whether the tree keeps this node as XML of its own, such as a bookmark or a field.
fn is_opaque(node: &Node) -> bool {
    schema().node_name(node.kind()) == "inlineOpaque"
}

/// Deletion marks on the words, breaks and pictures of a textblock whose content starts at
/// `offset`, where no deletion is yet. What the tree keeps as XML of its own stays as it is.
fn word_deletions(node: &Node, offset: usize, deletion: &Mark) -> Vec<Step> {
    let mut steps = Vec::new();
    let mut position = offset;
    for child in node.content().children() {
        let end = position + child.node_size();
        let deleted = child
            .marks()
            .iter()
            .any(|mark| schema().mark_name(mark.kind()) == "deletion");
        if !deleted && !is_opaque(child) {
            steps.push(Step::AddMark {
                from: position,
                to: end,
                mark: deletion.clone(),
            });
        }
        position = end;
    }
    steps
}

fn is_paragraph(node: &Node) -> bool {
    schema().node_name(node.kind()) == "paragraph"
}

fn has_change(node: &Node, name: &str) -> bool {
    node.attrs().get(name).is_some_and(|value| !value.is_null())
}

/// A tracked deletion of a paragraph at `start`: its words and the mark that ends it.
fn deletion_steps(start: usize, node: &Node, by: &Value) -> Result<Vec<Step>, String> {
    let mut steps = word_deletions(node, start + 1, &mark_of("deletion", Some(by))?);
    if !has_change(node, "deleted") {
        steps.push(Step::Attr {
            pos: start,
            attr: "deleted".to_owned(),
            value: by.clone(),
        });
    }
    Ok(steps)
}

/// A tracked change of the text of a cell at `position`: its words and the marks that end its
/// paragraphs but the last are deletions, and the new text follows them as an insertion.
fn cell_steps(position: usize, cell: &Node, text: &str, by: &Value) -> Result<Vec<Step>, String> {
    let deletion = mark_of("deletion", Some(by))?;
    let blocks = cell.content().children();
    let mut steps = Vec::new();
    let mut start = position + 1;
    let mut end = start;
    for (index, block) in blocks.iter().enumerate() {
        steps.extend(word_deletions(block, start + 1, &deletion));
        if index + 1 < blocks.len() && !has_change(block, "deleted") {
            steps.push(Step::Attr {
                pos: start,
                attr: "deleted".to_owned(),
                value: by.clone(),
            });
        }
        end = start + block.node_size() - 1;
        start += block.node_size();
    }
    if !text.is_empty() {
        let insertion = mark_of("insertion", Some(by))?;
        let node = Node::text(schema(), text, vec![insertion]).map_err(|failed| failed.0)?;
        steps.push(insert_step(end, node));
    }
    Ok(steps)
}

fn format_marks(
    bold: bool,
    italic: bool,
    underline: bool,
    color: Option<&str>,
) -> Result<Vec<Mark>, String> {
    let mut marks = Vec::new();
    for (on, name) in [(bold, "bold"), (italic, "italic"), (underline, "underline")] {
        if on {
            marks.push(mark_of(name, None)?);
        }
    }
    if let Some(color) = color {
        marks.push(mark_of("textStyle", Some(&json!({ "color": color })))?);
    }
    Ok(marks)
}

fn cell_position(
    doc: &Node,
    table: &str,
    row: usize,
    column: usize,
) -> Result<(usize, Node), String> {
    let (start, node) =
        block_start(doc, table).ok_or_else(|| format!("There is no table {table}."))?;
    let mut position = start + 1;
    let target = node
        .maybe_child(row)
        .ok_or_else(|| format!("There is no row {row}."))?;
    for earlier in &node.content().children()[..row] {
        position += earlier.node_size();
    }
    position += 1;
    let cell = target
        .maybe_child(column)
        .ok_or_else(|| format!("There is no column {column}."))?;
    for earlier in &target.content().children()[..column] {
        position += earlier.node_size();
    }
    Ok((position, cell.clone()))
}

/// The steps of an op. With `by`, text and paragraphs change as tracked changes, and what cannot
/// be one, such as a look or a table, changes directly.
#[expect(clippy::too_many_lines, reason = "one arm for each op")]
fn op_steps(doc: &Node, op: &Op, by: Option<&Value>, taken: usize) -> Result<Vec<Step>, String> {
    match op {
        Op::ReplaceText {
            find,
            with,
            block,
            all,
        } => {
            let places = find_places(doc, block.as_deref(), find)?;
            if places.is_empty() {
                return Err(format!("The text \"{find}\" is not there."));
            }
            let chosen: Vec<_> = if *all {
                places
            } else {
                places.into_iter().next_back().into_iter().collect()
            };
            let mut steps = Vec::new();
            for place in chosen {
                steps.extend(replace_text_steps(doc, place, with, by)?);
            }
            Ok(steps)
        }
        Op::InsertParagraph { text, after, style } => {
            let position = end_after(doc, after.as_deref())?;
            let id = new_id(doc, taken);
            let node = paragraph_of(text, style.as_deref(), Some(&id), by)?;
            Ok(vec![insert_step(position, node)])
        }
        Op::DeleteBlock { block } => {
            let (start, node) =
                block_start(doc, block).ok_or_else(|| format!("There is no block {block}."))?;
            if let Some(by) = by.filter(|_| is_paragraph(&node)) {
                return deletion_steps(start, &node, by);
            }
            if doc.child_count() == 1 {
                let replacement = paragraph_of("", None, Some(&new_id(doc, taken)), None)?;
                return Ok(vec![Step::Replace {
                    from: start,
                    to: start + node.node_size(),
                    slice: Slice::new(Fragment::from_array(vec![replacement]), 0, 0),
                    structure: false,
                }]);
            }
            Ok(vec![Step::Replace {
                from: start,
                to: start + node.node_size(),
                slice: Slice::empty(),
                structure: false,
            }])
        }
        Op::SetStyle { block, style } => {
            let (start, _) =
                block_start(doc, block).ok_or_else(|| format!("There is no block {block}."))?;
            Ok(vec![Step::Attr {
                pos: start,
                attr: "style".to_owned(),
                value: json!(style),
            }])
        }
        Op::FormatText {
            find,
            block,
            bold,
            italic,
            underline,
            color,
        } => {
            let marks = format_marks(*bold, *italic, *underline, color.as_deref())?;
            let places = find_places(doc, block.as_deref(), find)?;
            let (from, to) = places
                .last()
                .copied()
                .ok_or_else(|| format!("The text \"{find}\" is not there."))?;
            Ok(marks
                .into_iter()
                .map(|mark| Step::AddMark { from, to, mark })
                .collect())
        }
        Op::InsertTable { rows, after } => {
            if rows.is_empty() || rows.iter().any(Vec::is_empty) {
                return Err("A table needs cells.".to_owned());
            }
            let position = end_after(doc, after.as_deref())?;
            Ok(vec![insert_step(
                position,
                table_of(rows, &new_id(doc, taken))?,
            )])
        }
        Op::SetCell {
            table,
            row,
            column,
            text,
        } => {
            let (position, cell) = cell_position(doc, table, *row, *column)?;
            let paragraphs = cell.content().children().iter().all(is_paragraph);
            if let Some(by) = by.filter(|_| paragraphs) {
                return cell_steps(position, &cell, text, by);
            }
            let slice = Slice::new(
                Fragment::from_array(vec![paragraph_of(text, None, None, None)?]),
                0,
                0,
            );
            Ok(vec![Step::Replace {
                from: position + 1,
                to: position + cell.node_size() - 1,
                slice,
                structure: false,
            }])
        }
        Op::ReplaceBlock { block, node } => {
            let (start, current) =
                block_start(doc, block).ok_or_else(|| format!("There is no block {block}."))?;
            let node = Node::from_json(schema(), node).map_err(|failed| failed.0)?;
            Ok(vec![Step::Replace {
                from: start,
                to: start + current.node_size(),
                slice: Slice::new(Fragment::from_array(vec![node]), 0, 0),
                structure: false,
            }])
        }
        Op::InsertBlock { after, node } => {
            let position = end_after(doc, after.as_deref())?;
            let node = Node::from_json(schema(), node).map_err(|failed| failed.0)?;
            Ok(vec![insert_step(position, node)])
        }
        Op::AcceptChanges { block } => decide_steps(doc, block.as_deref(), true),
        Op::RejectChanges { block } => decide_steps(doc, block.as_deref(), false),
    }
}

/// The attributes of a paragraph that say whether the mark that ends it is a tracked change.
const ENDS: [&str; 2] = ["inserted", "deleted"];

/// The tracked changes of a part of a document: words and pictures with each mark, and the
/// paragraphs whose ending mark is a change, with the name of its attribute.
#[derive(Default)]
struct Changes {
    words: Vec<(usize, usize, Mark)>,
    ends: Vec<(usize, &'static str)>,
}

impl Changes {
    fn add_ends(&mut self, node: &Node, position: usize) {
        for name in ENDS {
            if has_change(node, name) {
                self.ends.push((position, name));
            }
        }
    }

    /// Adds the changes under a node whose content starts at `offset`.
    fn add_under(&mut self, node: &Node, offset: usize) {
        let mut position = offset;
        for child in node.content().children() {
            let end = position + child.node_size();
            if child.is_text() || child.is_leaf() {
                self.words.extend(
                    child
                        .marks()
                        .iter()
                        .filter(|mark| is_change(mark))
                        .map(|mark| (position, end, mark.clone())),
                );
            } else {
                self.add_ends(child, position);
                self.add_under(child, position + 1);
            }
            position = end;
        }
    }
}

fn is_change(mark: &Mark) -> bool {
    matches!(schema().mark_name(mark.kind()), "insertion" | "deletion")
}

/// Accepts or rejects the tracked changes of the body or of one block. An accepted insertion
/// and a rejected deletion lose their mark. The text of the others goes, from the end, and then
/// the marks that end paragraphs.
fn decide_steps(doc: &Node, block: Option<&str>, accept: bool) -> Result<Vec<Step>, String> {
    let mut changes = Changes::default();
    match block {
        Some(block) => {
            let (start, node) =
                block_start(doc, block).ok_or_else(|| format!("There is no block {block}."))?;
            changes.add_ends(&node, start);
            changes.add_under(&node, start + 1);
        }
        None => changes.add_under(doc, 0),
    }
    if changes.words.is_empty() && changes.ends.is_empty() {
        return Err("There are no tracked changes there.".to_owned());
    }
    let stays = |name: &str| matches!(name, "insertion" | "inserted") == accept;
    let mut steps: Vec<Step> = changes
        .words
        .iter()
        .filter(|(_, _, mark)| stays(schema().mark_name(mark.kind())))
        .map(|(from, to, mark)| Step::RemoveMark {
            from: *from,
            to: *to,
            mark: mark.clone(),
        })
        .collect();
    steps.extend(
        changes
            .ends
            .iter()
            .filter(|(_, name)| stays(name))
            .map(|(pos, name)| Step::Attr {
                pos: *pos,
                attr: (*name).to_owned(),
                value: Value::Null,
            }),
    );
    let mut gone: Vec<(usize, usize)> = changes
        .words
        .iter()
        .filter(|(_, _, mark)| !stays(schema().mark_name(mark.kind())))
        .map(|(from, to, _)| (*from, *to))
        .collect();
    gone.dedup();
    steps.extend(gone.into_iter().rev().map(|(from, to)| Step::Replace {
        from,
        to,
        slice: Slice::empty(),
        structure: false,
    }));
    let failed = |failed: text::Failed| failed.0;
    let maps: Vec<StepMap> = steps.iter().map(Step::map).collect();
    let mut current = doc.clone();
    for step in &steps {
        current = step.apply(schema(), &current).map_err(failed)?;
    }
    for (pos, name) in changes.ends.into_iter().rev() {
        if stays(name) {
            continue;
        }
        let at = maps.iter().fold(pos, |pos, map| map.map(pos, 1));
        for step in end_removal(&current, at, name)? {
            current = step.apply(schema(), &current).map_err(failed)?;
            steps.push(step);
        }
    }
    Ok(steps)
}

/// Takes away the mark that ends the paragraph at `at`. The paragraph goes when nothing but XML of
/// its own is left of it, as a deleted block does, and else its content goes to the start of the
/// paragraph after it, whose mark stays and keeps its look. The last block of the body or of a
/// cell, and a paragraph before a table, keep their mark and lose only its change.
fn end_removal(doc: &Node, at: usize, name: &str) -> Result<Vec<Step>, String> {
    let failed = |failed: text::Failed| failed.0;
    let node = doc
        .node_at(at)
        .map_err(failed)?
        .ok_or_else(|| format!("There is no paragraph at {at}."))?;
    let end = at + node.node_size();
    let alone = doc.resolve(at).map_err(failed)?.parent().child_count() == 1;
    let next = doc
        .resolve(end)
        .map_err(failed)?
        .node_after()
        .map_err(failed)?;
    let words = node
        .content()
        .children()
        .iter()
        .any(|child| !is_opaque(child));
    let removed = Step::Replace {
        from: at,
        to: end,
        slice: Slice::empty(),
        structure: false,
    };
    if !words && !alone {
        return Ok(vec![removed]);
    }
    if words && next.as_ref().is_some_and(is_paragraph) {
        return Ok(vec![
            removed,
            Step::Replace {
                from: at + 1,
                to: at + 1,
                slice: Slice::new(node.content().clone(), 0, 0),
                structure: false,
            },
        ]);
    }
    Ok(vec![Step::Attr {
        pos: at,
        attr: name.to_owned(),
        value: Value::Null,
    }])
}

/// Turns ops into steps, each one on the document as the ops before it left it, all or none.
pub(crate) fn to_steps(
    doc: &Node,
    ops: &[Op],
    track: bool,
    author: &str,
) -> Result<Vec<Value>, String> {
    let by = track.then(|| by(author));
    let mut doc = doc.clone();
    let mut all = Vec::new();
    for (taken, op) in ops.iter().enumerate() {
        for step in op_steps(&doc, op, by.as_ref(), taken)? {
            let json = step.to_json(schema());
            let (next, _) =
                run_steps(&doc, std::slice::from_ref(&json)).map_err(|failed| failed.0)?;
            doc = next;
            all.push(json);
        }
    }
    Ok(all)
}

/// As [`to_steps`], for a merge: a block that a person removed meanwhile stays removed, and an
/// insertion after it goes to the end.
pub(crate) fn merge_steps(doc: &Node, ops: &[Op]) -> Result<Vec<Value>, String> {
    let mut doc = doc.clone();
    let mut all = Vec::new();
    for (taken, op) in ops.iter().enumerate() {
        let steps = match op_steps(&doc, op, None, taken) {
            Ok(steps) => steps,
            Err(_) => match op {
                Op::InsertBlock { node, .. } => op_steps(
                    &doc,
                    &Op::InsertBlock {
                        after: None,
                        node: node.clone(),
                    },
                    None,
                    taken,
                )?,
                _ => continue,
            },
        };
        for step in steps {
            let json = step.to_json(schema());
            let (next, _) =
                run_steps(&doc, std::slice::from_ref(&json)).map_err(|failed| failed.0)?;
            doc = next;
            all.push(json);
        }
    }
    Ok(all)
}

fn block_json(node: &Node) -> Value {
    let kind = schema().node_name(node.kind());
    let id = node.attrs().get("id").cloned().unwrap_or(Value::Null);
    match kind {
        "table" => {
            let rows: Vec<Vec<String>> = node
                .content()
                .children()
                .iter()
                .map(|row| {
                    row.content()
                        .children()
                        .iter()
                        .map(Node::text_content)
                        .collect()
                })
                .collect();
            json!({ "id": id, "kind": kind, "rows": rows })
        }
        _ => json!({
            "id": id,
            "kind": kind,
            "style": node.attrs().get("style"),
            "text": node.text_content(),
        }),
    }
}

fn tracked_changes(node: &Node) -> usize {
    let own = node.marks().iter().filter(|mark| is_change(mark)).count()
        + ENDS.iter().filter(|name| has_change(node, name)).count();
    own + node
        .content()
        .children()
        .iter()
        .map(tracked_changes)
        .sum::<usize>()
}

pub(crate) fn answer(model: &Model, query: &Query) -> Value {
    match query {
        Query::Read { block } => {
            let blocks: Vec<Value> = model
                .doc
                .content()
                .children()
                .iter()
                .filter(|node| {
                    block.as_deref().is_none_or(|block| {
                        node.attrs().get("id").and_then(Value::as_str) == Some(block)
                    })
                })
                .map(block_json)
                .collect();
            json!({ "blocks": blocks })
        }
        Query::Check => json!({ "trackedChanges": tracked_changes(&model.doc) }),
    }
}
