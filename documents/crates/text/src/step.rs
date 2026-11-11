use serde_json::{Value, json};

use crate::{Failed, Fragment, Mark, Node, Schema, Slice, StepMap, failed};

/// Each step has the same result as the step of the same name in `ProseMirror`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Replace {
        from: usize,
        to: usize,
        slice: Slice,
        structure: bool,
    },
    ReplaceAround {
        from: usize,
        to: usize,
        gap_from: usize,
        gap_to: usize,
        slice: Slice,
        insert: usize,
        structure: bool,
    },
    AddMark {
        from: usize,
        to: usize,
        mark: Mark,
    },
    RemoveMark {
        from: usize,
        to: usize,
        mark: Mark,
    },
    AddNodeMark {
        pos: usize,
        mark: Mark,
    },
    RemoveNodeMark {
        pos: usize,
        mark: Mark,
    },
    Attr {
        pos: usize,
        attr: String,
        value: Value,
    },
    DocAttr {
        attr: String,
        value: Value,
    },
}

fn number(json: &Value, name: &str) -> Result<usize, Failed> {
    json[name]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| Failed(format!("Invalid {name} in a step")))
}

fn text(json: &Value, name: &str) -> Result<String, Failed> {
    json[name]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| Failed(format!("Invalid {name} in a step")))
}

impl Step {
    pub fn from_json(schema: &Schema, json: &Value) -> Result<Self, Failed> {
        let mark = || Mark::from_json(schema, &json["mark"]);
        let slice = || Slice::from_json(schema, json.get("slice"));
        let structure = json["structure"].as_bool().unwrap_or(false);
        Ok(match json["stepType"].as_str().unwrap_or_default() {
            "replace" => Self::Replace {
                from: number(json, "from")?,
                to: number(json, "to")?,
                slice: slice()?,
                structure,
            },
            "replaceAround" => Self::ReplaceAround {
                from: number(json, "from")?,
                to: number(json, "to")?,
                gap_from: number(json, "gapFrom")?,
                gap_to: number(json, "gapTo")?,
                slice: slice()?,
                insert: number(json, "insert")?,
                structure,
            },
            "addMark" => Self::AddMark {
                from: number(json, "from")?,
                to: number(json, "to")?,
                mark: mark()?,
            },
            "removeMark" => Self::RemoveMark {
                from: number(json, "from")?,
                to: number(json, "to")?,
                mark: mark()?,
            },
            "addNodeMark" => Self::AddNodeMark {
                pos: number(json, "pos")?,
                mark: mark()?,
            },
            "removeNodeMark" => Self::RemoveNodeMark {
                pos: number(json, "pos")?,
                mark: mark()?,
            },
            "attr" => Self::Attr {
                pos: number(json, "pos")?,
                attr: text(json, "attr")?,
                value: json.get("value").cloned().unwrap_or(Value::Null),
            },
            "docAttr" => Self::DocAttr {
                attr: text(json, "attr")?,
                value: json.get("value").cloned().unwrap_or(Value::Null),
            },
            other => return failed(format!("No step type {other} defined")),
        })
    }

    pub fn to_json(&self, schema: &Schema) -> Value {
        let with_slice = |mut json: Value, slice: &Slice, structure: bool| {
            if slice.size() > 0 {
                json["slice"] = slice.to_json(schema);
            }
            if structure {
                json["structure"] = json!(true);
            }
            json
        };
        match self {
            Self::Replace {
                from,
                to,
                slice,
                structure,
            } => with_slice(
                json!({ "stepType": "replace", "from": from, "to": to }),
                slice,
                *structure,
            ),
            Self::ReplaceAround {
                from,
                to,
                gap_from,
                gap_to,
                slice,
                insert,
                structure,
            } => with_slice(
                json!({
                    "stepType": "replaceAround",
                    "from": from,
                    "to": to,
                    "gapFrom": gap_from,
                    "gapTo": gap_to,
                    "insert": insert,
                }),
                slice,
                *structure,
            ),
            Self::AddMark { from, to, mark } => json!({
                "stepType": "addMark",
                "mark": mark.to_json(schema),
                "from": from,
                "to": to,
            }),
            Self::RemoveMark { from, to, mark } => json!({
                "stepType": "removeMark",
                "mark": mark.to_json(schema),
                "from": from,
                "to": to,
            }),
            Self::AddNodeMark { pos, mark } => json!({
                "stepType": "addNodeMark",
                "pos": pos,
                "mark": mark.to_json(schema),
            }),
            Self::RemoveNodeMark { pos, mark } => json!({
                "stepType": "removeNodeMark",
                "pos": pos,
                "mark": mark.to_json(schema),
            }),
            Self::Attr { pos, attr, value } => json!({
                "stepType": "attr",
                "pos": pos,
                "attr": attr,
                "value": value,
            }),
            Self::DocAttr { attr, value } => json!({
                "stepType": "docAttr",
                "attr": attr,
                "value": value,
            }),
        }
    }

    pub fn apply(&self, schema: &Schema, doc: &Node) -> Result<Node, Failed> {
        match self {
            Self::Replace {
                from,
                to,
                slice,
                structure,
            } => {
                if *structure && content_between(doc, *from, *to)? {
                    return failed("Structure replace would overwrite content");
                }
                doc.replace(schema, *from, *to, slice)
            }
            Self::ReplaceAround {
                from,
                to,
                gap_from,
                gap_to,
                slice,
                insert,
                structure,
            } => {
                if *structure
                    && (content_between(doc, *from, *gap_from)?
                        || content_between(doc, *gap_to, *to)?)
                {
                    return failed("Structure gap-replace would overwrite content");
                }
                let gap = doc.slice(*gap_from, *gap_to)?;
                if gap.open_start != 0 || gap.open_end != 0 {
                    return failed("Gap is not a flat range");
                }
                let Some(inserted) = slice.insert_at(schema, *insert, &gap.content)? else {
                    return failed("Content does not fit in gap");
                };
                doc.replace(schema, *from, *to, &inserted)
            }
            Self::AddMark { from, to, mark } => add_mark(schema, doc, *from, *to, mark),
            Self::RemoveMark { from, to, mark } => {
                let old = doc.slice(*from, *to)?;
                let content = map_fragment(
                    schema,
                    &old.content,
                    &|node: &Node, _parent: &Node| {
                        node.with_marks(mark.remove_from_set(node.marks()))
                    },
                    doc,
                )?;
                let slice = Slice::new(content, old.open_start, old.open_end);
                doc.replace(schema, *from, *to, &slice)
            }
            Self::AddNodeMark { pos, mark } => {
                let Some(node) = doc.node_at(*pos)? else {
                    return failed("No node at mark step's position");
                };
                update_node(
                    schema,
                    doc,
                    *pos,
                    &node,
                    mark.add_to_set(schema, node.marks()),
                )
            }
            Self::RemoveNodeMark { pos, mark } => {
                let Some(node) = doc.node_at(*pos)? else {
                    return failed("No node at mark step's position");
                };
                update_node(schema, doc, *pos, &node, mark.remove_from_set(node.marks()))
            }
            Self::Attr { pos, attr, value } => set_attr(schema, doc, *pos, attr, value),
            Self::DocAttr { attr, value } => {
                let mut attrs = doc.attrs().as_ref().clone();
                attrs.insert(attr.clone(), value.clone());
                doc.with_doc_attrs(schema, &Value::Object(attrs))
            }
        }
    }

    pub fn range(&self) -> Option<(usize, usize)> {
        match self {
            Self::Replace { from, to, .. }
            | Self::ReplaceAround { from, to, .. }
            | Self::AddMark { from, to, .. }
            | Self::RemoveMark { from, to, .. } => Some((*from, *to)),
            Self::AddNodeMark { pos, .. }
            | Self::RemoveNodeMark { pos, .. }
            | Self::Attr { pos, .. } => Some((*pos, *pos)),
            Self::DocAttr { .. } => None,
        }
    }

    pub fn map(&self) -> StepMap {
        match self {
            Self::Replace {
                from, to, slice, ..
            } => StepMap::new(vec![*from, to - from, slice.size()]),
            Self::ReplaceAround {
                from,
                to,
                gap_from,
                gap_to,
                slice,
                insert,
                ..
            } => StepMap::new(vec![
                *from,
                gap_from - from,
                *insert,
                *gap_to,
                to - gap_to,
                slice.size().saturating_sub(*insert),
            ]),
            _ => StepMap::default(),
        }
    }
}

fn add_mark(
    schema: &Schema,
    doc: &Node,
    from: usize,
    to: usize,
    mark: &Mark,
) -> Result<Node, Failed> {
    let old = doc.slice(from, to)?;
    let start = doc.resolve(from)?;
    let parent = start.node(start.shared_depth(to)).clone();
    let content = map_fragment(
        schema,
        &old.content,
        &|node: &Node, parent: &Node| {
            if !schema.is_atom(node.kind()) || !schema.allows_mark_type(parent.kind(), mark.kind())
            {
                return node.clone();
            }
            node.with_marks(mark.add_to_set(schema, node.marks()))
        },
        &parent,
    )?;
    let slice = Slice::new(content, old.open_start, old.open_end);
    doc.replace(schema, from, to, &slice)
}

fn set_attr(
    schema: &Schema,
    doc: &Node,
    pos: usize,
    attr: &str,
    value: &Value,
) -> Result<Node, Failed> {
    let Some(node) = doc.node_at(pos)? else {
        return failed("No node at attribute step's position");
    };
    let mut attrs = node.attrs().as_ref().clone();
    attrs.insert(attr.to_owned(), value.clone());
    let updated = node.with_attrs(schema, &Value::Object(attrs))?;
    let slice = Slice::new(
        Fragment::new(vec![updated]),
        0,
        usize::from(!node.is_leaf()),
    );
    doc.replace(schema, pos, pos + 1, &slice)
}

fn update_node(
    schema: &Schema,
    doc: &Node,
    pos: usize,
    node: &Node,
    marks: Vec<Mark>,
) -> Result<Node, Failed> {
    let updated = Node::new(
        schema,
        node.kind(),
        Some(&Value::Object(node.attrs().as_ref().clone())),
        Fragment::default(),
        marks,
    )?;
    let slice = Slice::new(
        Fragment::new(vec![updated]),
        0,
        usize::from(!node.is_leaf()),
    );
    doc.replace(schema, pos, pos + 1, &slice)
}

fn map_fragment(
    schema: &Schema,
    fragment: &Fragment,
    change: &dyn Fn(&Node, &Node) -> Node,
    parent: &Node,
) -> Result<Fragment, Failed> {
    let mut mapped = Vec::with_capacity(fragment.child_count());
    for child in fragment.children() {
        let mut child = child.clone();
        if child.content().size() > 0 {
            let inner = map_fragment(schema, child.content(), change, &child)?;
            child = child.copy(inner);
        }
        if schema.is_inline(child.kind()) {
            child = change(&child, parent);
        }
        mapped.push(child);
    }
    Ok(Fragment::from_array(mapped))
}

fn content_between(doc: &Node, from: usize, to: usize) -> Result<bool, Failed> {
    let start = doc.resolve(from)?;
    let Some(mut distance) = to.checked_sub(from) else {
        return failed("The step ends before it starts");
    };
    let mut depth = start.depth();
    while distance > 0 && depth > 0 && start.index_after(depth) == start.node(depth).child_count() {
        depth -= 1;
        distance -= 1;
    }
    if distance > 0 {
        let mut next = start
            .node(depth)
            .maybe_child(start.index_after(depth))
            .cloned();
        while distance > 0 {
            let Some(node) = next.filter(|node| !node.is_leaf()) else {
                return Ok(true);
            };
            next = node.maybe_child(0).cloned();
            distance -= 1;
        }
    }
    Ok(false)
}
