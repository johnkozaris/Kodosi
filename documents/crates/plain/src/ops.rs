use document::{Guide, guide_field as field, guide_op as op};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    Query,
    lines::{Edit, Text, lines_of},
};

/// The most lines that one read gives.
const MOST_READ: usize = 2000;
/// The most characters of a line that a read gives.
const MOST_LINE: usize = 2000;

/// What an agent means. Line numbers count from 1 in the text as the agent read it, and each op
/// of one change counts in that text, so the ops of one change do not move each other's lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "op",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Op {
    ReplaceText {
        find: String,
        with: String,
        #[serde(default)]
        all: bool,
    },
    InsertLines {
        text: String,
        #[serde(default)]
        after: Option<usize>,
    },
    ReplaceLines {
        from: usize,
        #[serde(default)]
        to: Option<usize>,
        text: String,
    },
    DeleteLines {
        from: usize,
        #[serde(default)]
        to: Option<usize>,
    },
}

impl Op {
    /// Whether this op names lines by number, which a change after the read moves.
    pub(crate) fn counts_lines(&self) -> bool {
        match self {
            Self::ReplaceText { .. } | Self::InsertLines { after: None, .. } => false,
            Self::InsertLines { .. } | Self::ReplaceLines { .. } | Self::DeleteLines { .. } => true,
        }
    }

    fn edits(&self, text: &Text) -> Result<Vec<Edit>, String> {
        let count = text.count();
        match self {
            Self::ReplaceText { find, with, all } => {
                let find = lines_of(find).join("\n");
                if find.is_empty() {
                    return Err("The text to find is empty.".to_owned());
                }
                let found: Vec<usize> =
                    text.joined.match_indices(&find).map(|(at, _)| at).collect();
                match found.len() {
                    0 => return Err(format!("The file has no {find:?}.")),
                    1 => {}
                    places if !all => {
                        return Err(format!(
                            "{find:?} is in {places} places. Give more of the text around it, or set all."
                        ));
                    }
                    _ => {}
                }
                let with = lines_of(with).join("\n");
                Ok(found
                    .into_iter()
                    .map(|at| Edit {
                        from: at,
                        to: at + find.len(),
                        text: with.clone(),
                    })
                    .collect())
            }
            Self::InsertLines { text: new, after } => {
                let new = lines_of(new);
                let after = after.unwrap_or(count);
                if after > count {
                    return Err(format!("The file has {count} lines."));
                }
                let lines = if count == 1 && text.line(0).is_empty() {
                    0..1
                } else {
                    after..after
                };
                Ok(text.lines_edit(lines, &new).into_iter().collect())
            }
            Self::ReplaceLines {
                from,
                to,
                text: new,
            } => {
                let lines = lines_in(*from, *to, count)?;
                Ok(text.lines_edit(lines, &lines_of(new)).into_iter().collect())
            }
            Self::DeleteLines { from, to } => {
                let lines = lines_in(*from, *to, count)?;
                Ok(text.lines_edit(lines, &[]).into_iter().collect())
            }
        }
    }
}

/// The lines `from` to `to` that an op names, from 0.
fn lines_in(
    from: usize,
    to: Option<usize>,
    count: usize,
) -> Result<std::ops::Range<usize>, String> {
    let to = to.unwrap_or(from);
    if from == 0 {
        return Err("Lines count from 1.".to_owned());
    }
    if to < from {
        return Err(format!("Line {to} comes before line {from}."));
    }
    if to > count {
        return Err(format!("The file has {count} lines."));
    }
    Ok(from - 1..to)
}

/// The edits of ops, in order. Ops that change the same text are refused, since their order
/// would be a guess.
pub(crate) fn edits(text: &Text, ops: &[Op]) -> Result<Vec<Edit>, String> {
    if ops.is_empty() {
        return Err("There are no ops.".to_owned());
    }
    let mut edits = Vec::new();
    for op in ops {
        edits.extend(op.edits(text)?);
    }
    edits.sort_by_key(|edit| (edit.from, edit.to));
    if let Some(pair) = edits.windows(2).find(|pair| pair[0].to > pair[1].from) {
        return Err(format!(
            "Two ops change line {}. Send one change for each.",
            text.line_at(pair[1].from) + 1
        ));
    }
    Ok(edits)
}

pub(crate) fn answer(text: &Text, query: &Query) -> Value {
    match query {
        Query::Read { from, to } => {
            let count = text.count();
            let first = from.unwrap_or(1).max(1);
            let last = to
                .unwrap_or(count)
                .min(count)
                .min(first.saturating_add(MOST_READ - 1));
            let lines: Vec<Value> = (first..=last)
                .map(|line| json!({ "line": line, "text": cut(text.line(line - 1)) }))
                .collect();
            json!({ "count": count, "lines": lines })
        }
        Query::Check => json!([]),
    }
}

/// A line as a read gives it, cut at [`MOST_LINE`] characters.
pub(crate) fn cut(line: &str) -> String {
    match line.char_indices().nth(MOST_LINE) {
        Some((at, _)) => format!("{}…", &line[..at]),
        None => line.to_owned(),
    }
}

pub(crate) fn guide() -> Guide {
    let from = || {
        field(
            "from",
            "line number",
            true,
            "The first line, as the read numbered it",
        )
    };
    let to = || {
        field(
            "to",
            "line number",
            false,
            "The last line. Without it, only the first",
        )
    };
    Guide {
        format: "text".to_owned(),
        ops: vec![
            op(
                "replaceText",
                "Replaces text, which may span lines",
                vec![
                    field(
                        "find",
                        "text",
                        true,
                        "The exact text, found in one place unless all is set",
                    ),
                    field("with", "text", true, "The new text"),
                    field("all", "boolean", false, "Replaces each place"),
                ],
            ),
            op(
                "insertLines",
                "Adds lines",
                vec![
                    field("text", "text", true, "The lines, split at line feeds"),
                    field(
                        "after",
                        "line number",
                        false,
                        "Goes after this line, or at the top with 0. Without it, at the end",
                    ),
                ],
            ),
            op(
                "replaceLines",
                "Puts new lines in place of lines",
                vec![
                    from(),
                    to(),
                    field("text", "text", true, "The new lines, split at line feeds"),
                ],
            ),
            op("deleteLines", "Removes lines", vec![from(), to()]),
        ],
    }
}
