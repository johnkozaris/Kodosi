use std::ops::Range;

use text::{Failed, Fragment, Node, Slice, Step};

use crate::schema;

/// More edits join into this many steps, so that a large change stays quick on the server and in
/// each view.
const MOST_STEPS: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Edit {
    pub(crate) from: usize,
    pub(crate) to: usize,
    pub(crate) text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Text {
    pub(crate) joined: String,
    starts: Vec<usize>,
}

impl Text {
    pub(crate) fn new(doc: &Node) -> Self {
        let mut joined = String::new();
        let mut starts = Vec::with_capacity(doc.child_count());
        for (index, line) in doc.content().children().iter().enumerate() {
            if index > 0 {
                joined.push('\n');
            }
            starts.push(joined.len());
            joined.push_str(&line.text_content());
        }
        Self { joined, starts }
    }

    pub(crate) fn count(&self) -> usize {
        self.starts.len()
    }

    pub(crate) fn line(&self, line: usize) -> &str {
        &self.joined[self.start(line)..self.end(line)]
    }

    pub(crate) fn lines(&self) -> Vec<&str> {
        (0..self.count()).map(|line| self.line(line)).collect()
    }

    pub(crate) fn start(&self, line: usize) -> usize {
        self.starts[line]
    }

    pub(crate) fn end(&self, line: usize) -> usize {
        self.starts
            .get(line + 1)
            .map_or(self.joined.len(), |next| next - 1)
    }

    pub(crate) fn line_at(&self, at: usize) -> usize {
        self.starts.partition_point(|start| *start <= at) - 1
    }

    fn position(&self, at: usize, positions: &[usize]) -> usize {
        let line = self.line_at(at);
        let column: usize = self.joined[self.start(line)..at]
            .chars()
            .map(char::len_utf16)
            .sum();
        positions[line] + 1 + column
    }

    pub(crate) fn lines_edit(&self, lines: Range<usize>, new: &[&str]) -> Option<Edit> {
        let end = self.joined.len();
        let (from, to, text) = match (lines.is_empty(), new.is_empty()) {
            (true, true) => return None,
            (false, false) => (
                self.start(lines.start),
                self.end(lines.end - 1),
                new.join("\n"),
            ),
            (true, false) if lines.start < self.count() => {
                let at = self.start(lines.start);
                (at, at, new.join("\n") + "\n")
            }
            (true, false) => (end, end, "\n".to_owned() + &new.join("\n")),
            (false, true) if lines.end < self.count() => (
                self.start(lines.start),
                self.start(lines.end),
                String::new(),
            ),
            (false, true) if lines.start > 0 => (self.end(lines.start - 1), end, String::new()),
            (false, true) => (0, end, String::new()),
        };
        Some(Edit { from, to, text })
    }

    /// The edits must be in order and must not overlap.
    pub(crate) fn steps(&self, doc: &Node, edits: Vec<Edit>) -> Result<Vec<Step>, Failed> {
        let edits = if edits.len() > MOST_STEPS {
            self.joined_edits(&edits)
        } else {
            edits
        };
        let positions = line_positions(doc);
        edits
            .iter()
            .rev()
            .map(|edit| {
                let parts: Vec<&str> = edit.text.split('\n').collect();
                let slice = match parts.as_slice() {
                    [""] => Slice::empty(),
                    [part] => Slice::new(Fragment::from_array(vec![text_node(part)?]), 0, 0),
                    parts => Slice::new(
                        Fragment::from_array(
                            parts
                                .iter()
                                .map(|part| line_node(part))
                                .collect::<Result<_, _>>()?,
                        ),
                        1,
                        1,
                    ),
                };
                Ok(Step::Replace {
                    from: self.position(edit.from, &positions),
                    to: self.position(edit.to, &positions),
                    slice,
                    structure: false,
                })
            })
            .collect()
    }

    fn joined_edits(&self, edits: &[Edit]) -> Vec<Edit> {
        edits
            .chunks(edits.len().div_ceil(MOST_STEPS))
            .filter_map(|chunk| {
                let (first, last) = (chunk.first()?, chunk.last()?);
                let mut text = String::new();
                let mut at = first.from;
                for edit in chunk {
                    text.push_str(&self.joined[at..edit.from]);
                    text.push_str(&edit.text);
                    at = edit.to;
                }
                Some(Edit {
                    from: first.from,
                    to: last.to,
                    text,
                })
            })
            .collect()
    }
}

pub(crate) fn line_positions(doc: &Node) -> Vec<usize> {
    let mut positions = Vec::with_capacity(doc.child_count() + 1);
    let mut position = 0;
    positions.push(position);
    for line in doc.content().children() {
        position += line.node_size();
        positions.push(position);
    }
    positions
}

pub(crate) fn lines_of(text: &str) -> Vec<&str> {
    let text = text.strip_suffix('\n').unwrap_or(text);
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect()
}

fn text_node(text: &str) -> Result<Node, Failed> {
    Node::text(schema(), text, Vec::new())
}

pub(crate) fn line_node(text: &str) -> Result<Node, Failed> {
    let content = if text.is_empty() {
        Vec::new()
    } else {
        vec![text_node(text)?]
    };
    let line = schema()
        .node_id("line")
        .ok_or_else(|| Failed("The schema has no line".to_owned()))?;
    Node::new(
        schema(),
        line,
        None,
        Fragment::from_array(content),
        Vec::new(),
    )
}
