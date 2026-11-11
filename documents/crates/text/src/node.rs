use std::sync::Arc;

use serde_json::{Map, Value, json};

use crate::{Failed, MarkId, NodeId, ResolvedPos, Schema, failed, replace};

const MOST_DEPTH: usize = 128;

pub type Attrs = Arc<Map<String, Value>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    kind: MarkId,
    attrs: Attrs,
}

impl Mark {
    pub fn new(schema: &Schema, kind: MarkId, attrs: Option<&Value>) -> Result<Self, Failed> {
        Ok(Self {
            kind,
            attrs: schema.mark_attrs(kind, attrs)?,
        })
    }

    pub fn kind(&self) -> MarkId {
        self.kind
    }

    pub fn attrs(&self) -> &Attrs {
        &self.attrs
    }

    pub fn from_json(schema: &Schema, json: &Value) -> Result<Self, Failed> {
        let name = json["type"].as_str().unwrap_or_default();
        let Some(kind) = schema.mark_id(name) else {
            return failed(format!("There is no mark type {name} in this schema"));
        };
        Self::new(schema, kind, json.get("attrs"))
    }

    pub fn to_json(&self, schema: &Schema) -> Value {
        let mut json = json!({ "type": schema.mark_name(self.kind) });
        if schema.mark_has_attrs(self.kind) {
            json["attrs"] = Value::Object(self.attrs.as_ref().clone());
        }
        json
    }

    pub(crate) fn add_to_set(&self, schema: &Schema, set: &[Self]) -> Vec<Self> {
        let mut copy: Option<Vec<Self>> = None;
        let mut placed = false;
        for (index, other) in set.iter().enumerate() {
            if self == other {
                return set.to_vec();
            }
            if schema.excludes(self.kind, other.kind) {
                copy.get_or_insert_with(|| set[..index].to_vec());
            } else if schema.excludes(other.kind, self.kind) {
                return set.to_vec();
            } else {
                if !placed && other.kind > self.kind {
                    copy.get_or_insert_with(|| set[..index].to_vec())
                        .push(self.clone());
                    placed = true;
                }
                if let Some(copy) = copy.as_mut() {
                    copy.push(other.clone());
                }
            }
        }
        let mut copy = copy.unwrap_or_else(|| set.to_vec());
        if !placed {
            copy.push(self.clone());
        }
        copy
    }

    pub(crate) fn remove_from_set(&self, set: &[Self]) -> Vec<Self> {
        set.iter().position(|mark| mark == self).map_or_else(
            || set.to_vec(),
            |index| [&set[..index], &set[index + 1..]].concat(),
        )
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Inner {
    kind: NodeId,
    attrs: Attrs,
    content: Fragment,
    marks: Arc<[Mark]>,
    text: Option<Arc<[u16]>>,
    leaf: bool,
}

/// Nodes are shared and never change: an edit makes new nodes along one path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node(Arc<Inner>);

impl Node {
    pub fn new(
        schema: &Schema,
        kind: NodeId,
        attrs: Option<&Value>,
        content: Fragment,
        marks: Vec<Mark>,
    ) -> Result<Self, Failed> {
        let mut marks = marks;
        marks.sort_by_key(Mark::kind);
        Ok(Self(Arc::new(Inner {
            kind,
            attrs: schema.node_attrs(kind, attrs)?,
            content,
            marks: marks.into(),
            text: None,
            leaf: schema.is_leaf(kind),
        })))
    }

    pub fn text(schema: &Schema, text: &str, marks: Vec<Mark>) -> Result<Self, Failed> {
        let units: Vec<u16> = text.encode_utf16().collect();
        Self::text_units(schema, units.into(), marks)
    }

    fn text_units(schema: &Schema, text: Arc<[u16]>, marks: Vec<Mark>) -> Result<Self, Failed> {
        if text.is_empty() {
            return failed("Empty text nodes are not allowed");
        }
        let mut marks = marks;
        marks.sort_by_key(Mark::kind);
        Ok(Self(Arc::new(Inner {
            kind: schema.text_node(),
            attrs: Attrs::default(),
            content: Fragment::default(),
            marks: marks.into(),
            text: Some(text),
            leaf: true,
        })))
    }

    pub fn kind(&self) -> NodeId {
        self.0.kind
    }

    pub fn attrs(&self) -> &Attrs {
        &self.0.attrs
    }

    pub fn content(&self) -> &Fragment {
        &self.0.content
    }

    pub fn marks(&self) -> &[Mark] {
        &self.0.marks
    }

    pub fn is_text(&self) -> bool {
        self.0.text.is_some()
    }

    pub fn is_leaf(&self) -> bool {
        self.0.leaf
    }

    /// In UTF-16 units.
    pub(crate) fn units(&self) -> Option<&[u16]> {
        self.0.text.as_deref()
    }

    pub fn text_of(&self) -> Option<String> {
        self.units().map(String::from_utf16_lossy)
    }

    pub fn text_content(&self) -> String {
        self.text_of().unwrap_or_else(|| {
            self.content()
                .children()
                .iter()
                .map(Self::text_content)
                .collect()
        })
    }

    pub fn node_size(&self) -> usize {
        match &self.0.text {
            Some(text) => text.len(),
            None if self.0.leaf => 1,
            None => 2 + self.0.content.size(),
        }
    }

    pub fn child_count(&self) -> usize {
        self.0.content.child_count()
    }

    pub fn child(&self, index: usize) -> Result<&Self, Failed> {
        self.0.content.child(index)
    }

    pub fn maybe_child(&self, index: usize) -> Option<&Self> {
        self.0.content.maybe_child(index)
    }

    pub(crate) fn same_markup(&self, other: &Self) -> bool {
        self.0.kind == other.0.kind
            && self.0.attrs == other.0.attrs
            && self.0.marks == other.0.marks
    }

    pub(crate) fn copy(&self, content: Fragment) -> Self {
        Self(Arc::new(Inner {
            kind: self.0.kind,
            attrs: Arc::clone(&self.0.attrs),
            content,
            marks: Arc::clone(&self.0.marks),
            text: self.0.text.clone(),
            leaf: self.0.leaf,
        }))
    }

    pub(crate) fn with_marks(&self, marks: Vec<Mark>) -> Self {
        Self(Arc::new(Inner {
            kind: self.0.kind,
            attrs: Arc::clone(&self.0.attrs),
            content: self.0.content.clone(),
            marks: marks.into(),
            text: self.0.text.clone(),
            leaf: self.0.leaf,
        }))
    }

    pub(crate) fn with_attrs(&self, schema: &Schema, attrs: &Value) -> Result<Self, Failed> {
        Ok(Self(Arc::new(Inner {
            kind: self.0.kind,
            attrs: schema.node_attrs(self.0.kind, Some(attrs))?,
            content: Fragment::default(),
            marks: Arc::clone(&self.0.marks),
            text: None,
            leaf: self.0.leaf,
        })))
    }

    pub(crate) fn with_doc_attrs(&self, schema: &Schema, attrs: &Value) -> Result<Self, Failed> {
        Ok(Self(Arc::new(Inner {
            kind: self.0.kind,
            attrs: schema.node_attrs(self.0.kind, Some(attrs))?,
            content: self.0.content.clone(),
            marks: Arc::clone(&self.0.marks),
            text: None,
            leaf: self.0.leaf,
        })))
    }

    fn with_units(&self, text: Arc<[u16]>) -> Self {
        Self(Arc::new(Inner {
            kind: self.0.kind,
            attrs: Arc::clone(&self.0.attrs),
            content: Fragment::default(),
            marks: Arc::clone(&self.0.marks),
            text: Some(text),
            leaf: true,
        }))
    }

    pub(crate) fn joined_text(&self, other: &Self) -> Self {
        let left = self.units().unwrap_or_default();
        let right = other.units().unwrap_or_default();
        self.with_units([left, right].concat().into())
    }

    pub(crate) fn cut(&self, from: usize, to: usize) -> Result<Self, Failed> {
        if let Some(text) = self.units() {
            if from == 0 && to == text.len() {
                return Ok(self.clone());
            }
            let Some(part) = text.get(from..to) else {
                return failed("Cut outside of text");
            };
            return Ok(self.with_units(part.into()));
        }
        if from == 0 && to == self.content().size() {
            return Ok(self.clone());
        }
        Ok(self.copy(self.content().cut(from, to)?))
    }

    pub fn slice(&self, from: usize, to: usize) -> Result<Slice, Failed> {
        if from == to {
            return Ok(Slice::empty());
        }
        let start = self.resolve(from)?;
        let end = self.resolve(to)?;
        let depth = start.shared_depth(to);
        let node_start = start.start(depth);
        let node = start.node(depth);
        let content = node
            .content()
            .cut(start.pos() - node_start, end.pos() - node_start)?;
        Ok(Slice::new(
            content,
            start.depth() - depth,
            end.depth() - depth,
        ))
    }

    pub fn replace(
        &self,
        schema: &Schema,
        from: usize,
        to: usize,
        slice: &Slice,
    ) -> Result<Self, Failed> {
        replace::replace(schema, &self.resolve(from)?, &self.resolve(to)?, slice)
    }

    pub fn node_at(&self, mut position: usize) -> Result<Option<Self>, Failed> {
        let mut node = self.clone();
        loop {
            let (index, offset) = node.content().find_index(position)?;
            let Some(child) = node.maybe_child(index).cloned() else {
                return Ok(None);
            };
            if offset == position || child.is_text() {
                return Ok(Some(child));
            }
            position -= offset + 1;
            node = child;
        }
    }

    pub fn resolve(&self, position: usize) -> Result<ResolvedPos, Failed> {
        ResolvedPos::resolve(self, position)
    }

    pub fn to_json(&self, schema: &Schema) -> Value {
        let mut json = json!({ "type": schema.node_name(self.kind()) });
        if schema.node_has_attrs(self.kind()) {
            json["attrs"] = Value::Object(self.attrs().as_ref().clone());
        }
        if self.content().size() > 0 {
            json["content"] = self.content().to_json(schema);
        }
        if !self.marks().is_empty() {
            json["marks"] = self
                .marks()
                .iter()
                .map(|mark| mark.to_json(schema))
                .collect();
        }
        if let Some(text) = self.text_of() {
            json["text"] = Value::String(text);
        }
        json
    }

    pub fn from_json(schema: &Schema, json: &Value) -> Result<Self, Failed> {
        Self::from_json_at(schema, json, 0)
    }

    fn from_json_at(schema: &Schema, json: &Value, depth: usize) -> Result<Self, Failed> {
        if depth > MOST_DEPTH {
            return failed("The document is nested too deep");
        }
        let marks = match json.get("marks") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(marks)) => marks
                .iter()
                .map(|mark| Mark::from_json(schema, mark))
                .collect::<Result<_, _>>()?,
            Some(_) => return failed("Invalid mark data for Node.fromJSON"),
        };
        let name = json["type"].as_str().unwrap_or_default();
        if name == "text" {
            let Some(text) = json["text"].as_str() else {
                return failed("Invalid text node in JSON");
            };
            return Self::text(schema, text, marks);
        }
        let Some(kind) = schema.node_id(name) else {
            return failed(format!("Unknown node type: {name}"));
        };
        let content = Fragment::from_json_at(schema, json.get("content"), depth + 1)?;
        Self::new(schema, kind, json.get("attrs"), content, marks)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fragment {
    content: Arc<[Node]>,
    size: usize,
}

impl Fragment {
    pub(crate) fn new(content: Vec<Node>) -> Self {
        let size = content.iter().map(Node::node_size).sum();
        Self {
            content: content.into(),
            size,
        }
    }

    /// Joins adjacent text with the same marks.
    pub fn from_array(nodes: Vec<Node>) -> Self {
        let mut joined: Vec<Node> = Vec::with_capacity(nodes.len());
        for node in nodes {
            match joined.last_mut() {
                Some(last) if node.is_text() && last.same_markup(&node) => {
                    *last = last.joined_text(&node);
                }
                _ => joined.push(node),
            }
        }
        Self::new(joined)
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn children(&self) -> &[Node] {
        &self.content
    }

    pub fn child_count(&self) -> usize {
        self.content.len()
    }

    pub fn child(&self, index: usize) -> Result<&Node, Failed> {
        self.content
            .get(index)
            .ok_or_else(|| Failed(format!("Index {index} out of range")))
    }

    pub fn maybe_child(&self, index: usize) -> Option<&Node> {
        self.content.get(index)
    }

    pub(crate) fn append(&self, other: &Self) -> Self {
        if other.size == 0 {
            return self.clone();
        }
        if self.size == 0 {
            return other.clone();
        }
        let mut content = self.content.to_vec();
        let mut rest = other.content.iter();
        if let (Some(last), Some(first)) = (content.last_mut(), other.content.first())
            && last.is_text()
            && last.same_markup(first)
        {
            *last = last.joined_text(first);
            rest.next();
        }
        content.extend(rest.cloned());
        Self {
            content: content.into(),
            size: self.size + other.size,
        }
    }

    pub(crate) fn cut(&self, from: usize, to: usize) -> Result<Self, Failed> {
        if from == 0 && to == self.size {
            return Ok(self.clone());
        }
        let mut result = Vec::new();
        let mut size = 0;
        if to > from {
            let mut position = 0;
            for child in self.content.iter() {
                if position >= to {
                    break;
                }
                let end = position + child.node_size();
                if end > from {
                    let child = if position < from || end > to {
                        if let Some(text) = child.units() {
                            child
                                .cut(from.saturating_sub(position), text.len().min(to - position))?
                        } else {
                            child.cut(
                                from.saturating_sub(position + 1),
                                child.content().size().min(to.saturating_sub(position + 1)),
                            )?
                        }
                    } else {
                        child.clone()
                    };
                    size += child.node_size();
                    result.push(child);
                }
                position = end;
            }
            if position < to {
                return failed("Cut outside of the fragment");
            }
        }
        Ok(Self {
            content: result.into(),
            size,
        })
    }

    pub(crate) fn cut_end(&self, from: usize) -> Result<Self, Failed> {
        self.cut(from, self.size)
    }

    pub(crate) fn replace_child(&self, index: usize, node: Node) -> Result<Self, Failed> {
        let Some(current) = self.content.get(index) else {
            return failed(format!("Index {index} out of range"));
        };
        let size = self.size + node.node_size() - current.node_size();
        let mut copy = self.content.to_vec();
        copy[index] = node;
        Ok(Self {
            content: copy.into(),
            size,
        })
    }

    pub(crate) fn find_index(&self, position: usize) -> Result<(usize, usize), Failed> {
        if position == 0 {
            return Ok((0, position));
        }
        if position == self.size {
            return Ok((self.content.len(), position));
        }
        if position > self.size {
            return failed(format!("Position {position} outside of fragment"));
        }
        let mut current = 0;
        for (index, child) in self.content.iter().enumerate() {
            let end = current + child.node_size();
            if end >= position {
                return Ok(if end == position {
                    (index + 1, end)
                } else {
                    (index, current)
                });
            }
            current = end;
        }
        failed(format!("Position {position} outside of fragment"))
    }

    pub fn to_json(&self, schema: &Schema) -> Value {
        if self.content.is_empty() {
            return Value::Null;
        }
        self.content
            .iter()
            .map(|node| node.to_json(schema))
            .collect()
    }

    pub fn from_json(schema: &Schema, json: Option<&Value>) -> Result<Self, Failed> {
        Self::from_json_at(schema, json, 0)
    }

    fn from_json_at(schema: &Schema, json: Option<&Value>, depth: usize) -> Result<Self, Failed> {
        match json {
            None | Some(Value::Null) => Ok(Self::default()),
            Some(Value::Array(nodes)) => Ok(Self::from_array(
                nodes
                    .iter()
                    .map(|node| Node::from_json_at(schema, node, depth))
                    .collect::<Result<_, _>>()?,
            )),
            Some(_) => failed("Invalid input for Fragment.fromJSON"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Slice {
    pub content: Fragment,
    pub open_start: usize,
    pub open_end: usize,
}

impl Slice {
    pub fn new(content: Fragment, open_start: usize, open_end: usize) -> Self {
        Self {
            content,
            open_start,
            open_end,
        }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn size(&self) -> usize {
        self.content
            .size()
            .saturating_sub(self.open_start + self.open_end)
    }

    pub(crate) fn insert_at(
        &self,
        schema: &Schema,
        position: usize,
        fragment: &Fragment,
    ) -> Result<Option<Self>, Failed> {
        let content = insert_into(
            schema,
            &self.content,
            position + self.open_start,
            fragment,
            as_signed(self.open_start) + 1,
            as_signed(self.open_end) + 1,
            None,
        )?;
        Ok(content.map(|content| Self::new(content, self.open_start, self.open_end)))
    }

    pub fn to_json(&self, schema: &Schema) -> Value {
        if self.content.size() == 0 {
            return Value::Null;
        }
        let mut json = json!({ "content": self.content.to_json(schema) });
        if self.open_start > 0 {
            json["openStart"] = json!(self.open_start);
        }
        if self.open_end > 0 {
            json["openEnd"] = json!(self.open_end);
        }
        json
    }

    pub fn from_json(schema: &Schema, json: Option<&Value>) -> Result<Self, Failed> {
        let Some(json) = json.filter(|json| !json.is_null()) else {
            return Ok(Self::empty());
        };
        let open = |name: &str| {
            json.get(name).map_or(Ok(0), |value| {
                value
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| Failed("Invalid input for Slice.fromJSON".to_owned()))
            })
        };
        Ok(Self::new(
            Fragment::from_json(schema, json.get("content"))?,
            open("openStart")?,
            open("openEnd")?,
        ))
    }
}

fn as_signed(value: usize) -> isize {
    isize::try_from(value).unwrap_or(isize::MAX - 1)
}

fn insert_into(
    schema: &Schema,
    content: &Fragment,
    distance: usize,
    insert: &Fragment,
    open_start: isize,
    open_end: isize,
    parent: Option<&Node>,
) -> Result<Option<Fragment>, Failed> {
    let (index, offset) = content.find_index(distance)?;
    let child = content.maybe_child(index);
    if offset == distance || child.is_some_and(Node::is_text) {
        if let Some(parent) = parent
            && open_start <= 0
            && open_end <= 0
            && !schema.can_replace(parent, index, index, insert)
        {
            return Ok(None);
        }
        return Ok(Some(
            content
                .cut(0, distance)?
                .append(insert)
                .append(&content.cut_end(distance)?),
        ));
    }
    let Some(child) = child else {
        return failed("No node at the position");
    };
    let inner = insert_into(
        schema,
        child.content(),
        distance - offset - 1,
        insert,
        if index == 0 { open_start - 1 } else { 0 },
        if index + 1 == content.child_count() {
            open_end - 1
        } else {
            0
        },
        Some(child),
    )?;
    inner
        .map(|inner| content.replace_child(index, child.copy(inner)))
        .transpose()
}
