//! Plain text files, such as .txt, .json and .yaml. The server keeps the text as a tree of lines,
//! so a view edits it with the same steps as a document, and a save keeps the file's byte order
//! mark, its line endings and whether it ends with one.

mod diff;
mod lines;
mod merge;
mod ops;
mod place;

use std::sync::LazyLock;

use document::{Answer, Assets, Format, Guide, Ordered, Refused, Snapshot, Unreadable, Unwritable};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use text::{Fragment, Node, Schema, SchemaSpec, Step};

use crate::lines::{Text, line_node};

pub use merge::Written;
pub use ops::Op;

static SCHEMA: LazyLock<Schema> = LazyLock::new(|| {
    let spec: SchemaSpec = serde_json::from_str(include_str!("../schema.json"))
        .unwrap_or_else(|error| unreachable!("the schema is part of this crate: {error}"));
    Schema::new(&spec)
        .unwrap_or_else(|error| unreachable!("the schema is part of this crate: {error}"))
});

pub(crate) fn schema() -> &'static Schema {
    &SCHEMA
}

const BOM: &[u8] = "\u{feff}".as_bytes();
/// The most lines of a file that opens live. Each edit of a view takes longer with each line,
/// so a longer file stays a page.
const MOST_LINES: usize = 10_000;
/// The most bytes of a file that opens live, for the files whose lines are very long.
const MOST_BYTES: usize = 512 * 1024;

/// The text of a file, without its byte order mark.
fn text_of(file: &[u8]) -> Result<&str, Unreadable> {
    std::str::from_utf8(file.strip_prefix(BOM).unwrap_or(file))
        .map_err(|error| Unreadable(format!("the file is not UTF-8 text: {error}")))
}

/// Plain text.
pub struct Plain;

/// The live copy of a text file: its lines.
#[derive(Debug, Clone)]
pub struct Model {
    doc: Node,
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
    },
    Ops {
        ops: Vec<Op>,
    },
    /// What a compare found in a write of another program. It becomes steps, and never travels.
    #[serde(skip)]
    Written(Written),
}

/// What read and check ask.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Query {
    /// The lines `from` to `to`, counted from 1.
    Read {
        #[serde(default)]
        from: Option<usize>,
        #[serde(default)]
        to: Option<usize>,
    },
    Check,
}

impl Format for Plain {
    type Model = Model;
    type Change = Change;
    type Query = Query;

    fn opens(file: &[u8]) -> Result<(), Unreadable> {
        let lines = file.split(|byte| *byte == b'\n').count() - usize::from(file.ends_with(b"\n"));
        if lines > MOST_LINES || file.len() > MOST_BYTES {
            return Err(Unreadable(format!(
                "it is too long to edit live: more than {MOST_LINES} lines or {} KB",
                MOST_BYTES / 1024
            )));
        }
        text_of(file).map(drop)
    }

    fn read(file: &[u8], _assets: &mut Assets) -> Result<Model, Unreadable> {
        let text = text_of(file)?;
        let lines: Vec<Node> = split(text)
            .map(line_node)
            .collect::<Result<_, _>>()
            .map_err(|failed| Unreadable(failed.0))?;
        let doc = Node::new(
            schema(),
            schema().top_node(),
            None,
            Fragment::from_array(lines),
            Vec::new(),
        )
        .map_err(|failed| Unreadable(failed.0))?;
        Ok(Model { doc })
    }

    fn snapshot(model: &Model) -> Snapshot {
        Snapshot(json!({ "doc": model.doc.to_json(schema()) }))
    }

    fn apply(
        model: &mut Model,
        change: Change,
        since: &[Ordered<Change>],
        _assets: &mut Assets,
    ) -> Result<Change, Refused> {
        let invalid = |failed: text::Failed| Refused::Invalid(failed.0);
        let steps = match change {
            Change::Steps { steps } => {
                if !since.is_empty() {
                    return Err(Refused::Behind(
                        "The text moved. Move the steps and send them again.".to_owned(),
                    ));
                }
                let mut doc = model.doc.clone();
                for json in &steps {
                    doc = Step::from_json(schema(), json)
                        .and_then(|step| step.apply(schema(), &doc))
                        .map_err(invalid)?;
                }
                model.doc = doc;
                return Ok(Change::Steps { steps });
            }
            Change::Ops { ops } => {
                if let Some(ordered) = since.first()
                    && ops.iter().any(Op::counts_lines)
                {
                    return Err(Refused::Behind(format!(
                        "{} changed the file after you read it. Read again.",
                        ordered.author
                    )));
                }
                let text = Text::new(&model.doc);
                let edits = ops::edits(&text, &ops).map_err(Refused::Invalid)?;
                text.steps(&model.doc, edits).map_err(invalid)?
            }
            Change::Written(written) => {
                let text = Text::new(&model.doc);
                let edits = merge::edits(&text, &written);
                if edits.is_empty() {
                    return Err(Refused::Invalid(
                        "The file has these lines already.".to_owned(),
                    ));
                }
                text.steps(&model.doc, edits).map_err(invalid)?
            }
        };
        let mut doc = model.doc.clone();
        for step in &steps {
            doc = step.apply(schema(), &doc).map_err(invalid)?;
        }
        model.doc = doc;
        Ok(Change::Steps {
            steps: steps.iter().map(|step| step.to_json(schema())).collect(),
        })
    }

    fn compare(from: &Model, to: &Model) -> Vec<Change> {
        merge::compare(from, to)
    }

    fn distance(changes: &[Change]) -> usize {
        changes
            .iter()
            .map(|change| match change {
                Change::Written(written) => written.distance(),
                Change::Steps { .. } | Change::Ops { .. } => 1,
            })
            .sum()
    }

    fn write(model: &Model, file: &[u8], _assets: &Assets) -> Result<Vec<u8>, Unwritable> {
        let (bom, file) = file
            .strip_prefix(BOM)
            .map_or((false, file), |rest| (true, rest));
        let ending = if is_crlf(file) { "\r\n" } else { "\n" };
        let text = Text::new(&model.doc);
        let mut written = Vec::with_capacity(text.joined.len() + text.count() + BOM.len());
        if bom {
            written.extend_from_slice(BOM);
        }
        if text.joined.is_empty() && text.count() == 1 {
            return Ok(written);
        }
        for (index, line) in text.lines().into_iter().enumerate() {
            if index > 0 {
                written.extend_from_slice(ending.as_bytes());
            }
            written.extend_from_slice(line.as_bytes());
        }
        if file.is_empty() || file.ends_with(b"\n") {
            written.extend_from_slice(ending.as_bytes());
        }
        Ok(written)
    }

    fn query(model: &Model, query: Query) -> Answer {
        Answer(ops::answer(&Text::new(&model.doc), &query))
    }

    fn guide() -> Guide {
        ops::guide()
    }

    fn map_place(place: &mut Value, change: &Change) {
        place::map(place, change);
    }

    fn describe_place(model: &Model, place: &Value) -> Value {
        place::describe(&model.doc, place)
    }
}

/// Whether a file has line feeds, and a carriage return comes before each.
fn is_crlf(file: &[u8]) -> bool {
    let mut ended = file.split(|byte| *byte == b'\n');
    ended.next_back();
    let mut ended = ended.peekable();
    ended.peek().is_some() && ended.all(|line| line.ends_with(b"\r"))
}

/// The lines of a file. A line ending at its end ends the last line, and does not start another.
fn split(text: &str) -> impl Iterator<Item = &str> {
    let ending = if is_crlf(text.as_bytes()) {
        "\r\n"
    } else {
        "\n"
    };
    let body = text.strip_suffix(ending).unwrap_or(text);
    body.split(ending)
}

#[cfg(test)]
mod tests;
