use serde_json::{Value, json};

use crate::{Node, Schema, Step};

/// The `{from, to}` positions that a view sends for what a person selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub from: usize,
    pub to: usize,
}

impl Selection {
    pub fn read(place: &Value) -> Option<Self> {
        let position = |key: &str| usize::try_from(place.get(key)?.as_u64()?).ok();
        Some(Self {
            from: position("from")?,
            to: position("to")?,
        })
    }

    #[must_use]
    pub fn within(self, doc: &Node) -> Self {
        let size = doc.content().size();
        let from = self.from.min(size);
        Self {
            from,
            to: self.to.clamp(from, size),
        }
    }

    pub fn text(self, doc: &Node) -> String {
        doc.slice(self.from, self.to).map_or_else(
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
        )
    }

    /// A step that is not valid leaves the selection where it is.
    pub fn map(place: &mut Value, schema: &Schema, steps: &[Value]) {
        let Some(Self { mut from, mut to }) = Self::read(place) else {
            return;
        };
        for json in steps {
            let Ok(step) = Step::from_json(schema, json) else {
                return;
            };
            let map = step.map();
            from = map.map(from, 1);
            to = map.map(to, 1);
        }
        place["from"] = json!(from);
        place["to"] = json!(to.max(from));
    }
}
