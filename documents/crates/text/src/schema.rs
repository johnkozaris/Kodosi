use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::{Attrs, Failed, Fragment, failed};

pub type NodeId = u16;
pub type MarkId = u16;
type MatchId = usize;

const EMPTY_MATCH: MatchId = 0;

/// A schema as data, with nodes and marks in their order, as `Schema.spec` holds them.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaSpec {
    pub nodes: Vec<NodeSpec>,
    #[serde(default)]
    pub marks: Vec<MarkSpec>,
    #[serde(default)]
    pub top_node: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NodeSpec {
    pub name: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub marks: Option<String>,
    #[serde(default)]
    pub inline: bool,
    #[serde(default)]
    pub atom: bool,
    #[serde(default)]
    pub attrs: Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MarkSpec {
    pub name: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub excludes: Option<String>,
    #[serde(default)]
    pub attrs: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("The schema is not valid: {0}")]
pub struct BadSchema(pub String);

#[derive(Debug, Clone)]
struct Attribute {
    name: String,
    default: Option<Value>,
}

#[derive(Debug, Clone)]
struct NodeType {
    name: String,
    groups: Vec<String>,
    attrs: Vec<Attribute>,
    is_block: bool,
    atom: bool,
    content_match: MatchId,
    inline_content: bool,
    mark_set: Option<Vec<MarkId>>,
}

#[derive(Debug, Clone)]
struct MarkType {
    name: String,
    groups: Vec<String>,
    attrs: Vec<Attribute>,
    excluded: Vec<MarkId>,
}

#[derive(Debug, Clone, Default)]
struct MatchState {
    valid_end: bool,
    next: Vec<(NodeId, MatchId)>,
}

#[derive(Debug, Clone)]
pub struct Schema {
    nodes: Vec<NodeType>,
    marks: Vec<MarkType>,
    node_ids: HashMap<String, NodeId>,
    mark_ids: HashMap<String, MarkId>,
    matches: Vec<MatchState>,
    top: NodeId,
    text: NodeId,
}

fn attributes(spec: &Map<String, Value>) -> Vec<Attribute> {
    spec.iter()
        .map(|(name, attribute)| Attribute {
            name: name.clone(),
            default: attribute.get("default").cloned(),
        })
        .collect()
}

fn words(text: Option<&String>) -> Vec<String> {
    text.map_or_else(Vec::new, |text| {
        text.split_whitespace().map(str::to_owned).collect()
    })
}

impl Schema {
    pub fn new(spec: &SchemaSpec) -> Result<Self, BadSchema> {
        let mut nodes = Vec::with_capacity(spec.nodes.len());
        let mut node_ids = HashMap::new();
        for (index, node) in spec.nodes.iter().enumerate() {
            let id = NodeId::try_from(index).map_err(|_| BadSchema("too many nodes".to_owned()))?;
            node_ids.insert(node.name.clone(), id);
            nodes.push(NodeType {
                name: node.name.clone(),
                groups: words(node.group.as_ref()),
                attrs: attributes(&node.attrs),
                is_block: !(node.inline || node.name == "text"),
                atom: node.atom,
                content_match: EMPTY_MATCH,
                inline_content: false,
                mark_set: None,
            });
        }
        let mut marks = Vec::with_capacity(spec.marks.len());
        let mut mark_ids = HashMap::new();
        for (index, mark) in spec.marks.iter().enumerate() {
            let id = MarkId::try_from(index).map_err(|_| BadSchema("too many marks".to_owned()))?;
            mark_ids.insert(mark.name.clone(), id);
            marks.push(MarkType {
                name: mark.name.clone(),
                groups: words(mark.group.as_ref()),
                attrs: attributes(&mark.attrs),
                excluded: Vec::new(),
            });
        }
        let top_name = spec.top_node.as_deref().unwrap_or("doc");
        let top = *node_ids
            .get(top_name)
            .ok_or_else(|| BadSchema(format!("no top node {top_name}")))?;
        let text = *node_ids
            .get("text")
            .ok_or_else(|| BadSchema("no text node".to_owned()))?;

        let mut matches = vec![MatchState {
            valid_end: true,
            next: Vec::new(),
        }];
        let mut compiled: HashMap<String, MatchId> = HashMap::new();
        for (index, node) in spec.nodes.iter().enumerate() {
            let expression = node.content.clone().unwrap_or_default();
            let known = compiled.get(&expression).copied();
            let content_match = if let Some(found) = known {
                found
            } else {
                let found = compile(&expression, &nodes, &node_ids, &mut matches)?;
                compiled.insert(expression, found);
                found
            };
            let inline_content = matches[content_match]
                .next
                .first()
                .is_some_and(|(first, _)| !nodes[*first as usize].is_block);
            let mark_set = match node.marks.as_deref() {
                Some("_") => None,
                Some("") => Some(Vec::new()),
                Some(names) => Some(gather_marks(&marks, &mark_ids, names)?),
                None if inline_content => None,
                None => Some(Vec::new()),
            };
            let target = &mut nodes[index];
            target.content_match = content_match;
            target.inline_content = inline_content;
            target.mark_set = mark_set;
        }
        for (index, mark) in spec.marks.iter().enumerate() {
            marks[index].excluded = match mark.excludes.as_deref() {
                None => vec![MarkId::try_from(index).unwrap_or_default()],
                Some("") => Vec::new(),
                Some(names) => gather_marks(&marks, &mark_ids, names)?,
            };
        }
        Ok(Self {
            nodes,
            marks,
            node_ids,
            mark_ids,
            matches,
            top,
            text,
        })
    }

    pub fn node_id(&self, name: &str) -> Option<NodeId> {
        self.node_ids.get(name).copied()
    }

    pub fn mark_id(&self, name: &str) -> Option<MarkId> {
        self.mark_ids.get(name).copied()
    }

    pub fn top_node(&self) -> NodeId {
        self.top
    }

    pub fn text_node(&self) -> NodeId {
        self.text
    }

    fn node(&self, id: NodeId) -> &NodeType {
        &self.nodes[id as usize]
    }

    pub fn node_name(&self, id: NodeId) -> &str {
        &self.node(id).name
    }

    pub fn mark_name(&self, id: MarkId) -> &str {
        &self.marks[id as usize].name
    }

    pub(crate) fn is_leaf(&self, id: NodeId) -> bool {
        self.node(id).content_match == EMPTY_MATCH
    }

    pub(crate) fn is_atom(&self, id: NodeId) -> bool {
        self.is_leaf(id) || self.node(id).atom
    }

    pub(crate) fn is_inline(&self, id: NodeId) -> bool {
        !self.node(id).is_block
    }

    pub fn is_textblock(&self, id: NodeId) -> bool {
        let node = self.node(id);
        node.is_block && node.inline_content
    }

    pub(crate) fn node_has_attrs(&self, id: NodeId) -> bool {
        !self.node(id).attrs.is_empty()
    }

    pub(crate) fn mark_has_attrs(&self, id: MarkId) -> bool {
        !self.marks[id as usize].attrs.is_empty()
    }

    pub(crate) fn node_attrs(&self, id: NodeId, given: Option<&Value>) -> Result<Attrs, Failed> {
        let node = self.node(id);
        compute_attrs(&node.attrs, given)
    }

    pub(crate) fn mark_attrs(&self, id: MarkId, given: Option<&Value>) -> Result<Attrs, Failed> {
        compute_attrs(&self.marks[id as usize].attrs, given)
    }

    pub(crate) fn excludes(&self, mark: MarkId, other: MarkId) -> bool {
        self.marks[mark as usize].excluded.contains(&other)
    }

    pub(crate) fn allows_mark_type(&self, node: NodeId, mark: MarkId) -> bool {
        self.node(node)
            .mark_set
            .as_ref()
            .is_none_or(|set| set.contains(&mark))
    }

    pub(crate) fn valid_content(&self, node: NodeId, content: &Fragment) -> bool {
        let Some(result) = self.match_fragment(self.node(node).content_match, content) else {
            return false;
        };
        self.matches[result].valid_end
            && content.children().iter().all(|child| {
                child
                    .marks()
                    .iter()
                    .all(|mark| self.allows_mark_type(node, mark.kind()))
            })
    }

    fn match_fragment(&self, start: MatchId, content: &Fragment) -> Option<MatchId> {
        let mut current = start;
        for child in content.children() {
            current = self.matches[current]
                .next
                .iter()
                .find(|(kind, _)| *kind == child.kind())?
                .1;
        }
        Some(current)
    }

    pub(crate) fn compatible_content(&self, node: NodeId, other: NodeId) -> bool {
        if node == other {
            return true;
        }
        let left = &self.matches[self.node(node).content_match].next;
        let right = &self.matches[self.node(other).content_match].next;
        left.iter()
            .any(|(kind, _)| right.iter().any(|(other, _)| other == kind))
    }

    pub(crate) fn can_replace(
        &self,
        parent: &crate::Node,
        from: usize,
        to: usize,
        content: &Fragment,
    ) -> bool {
        let kind = parent.kind();
        let Some(before) = self.match_children(
            self.node(kind).content_match,
            &parent.content().children()[..from],
        ) else {
            return false;
        };
        let Some(after) = self.match_children(before, content.children()) else {
            return false;
        };
        let Some(end) = self.match_children(after, &parent.content().children()[to..]) else {
            return false;
        };
        self.matches[end].valid_end
            && content.children().iter().all(|child| {
                child
                    .marks()
                    .iter()
                    .all(|mark| self.allows_mark_type(kind, mark.kind()))
            })
    }

    fn match_children(&self, start: MatchId, children: &[crate::Node]) -> Option<MatchId> {
        let mut current = start;
        for child in children {
            current = self.matches[current]
                .next
                .iter()
                .find(|(kind, _)| *kind == child.kind())?
                .1;
        }
        Some(current)
    }
}

fn compute_attrs(attrs: &[Attribute], given: Option<&Value>) -> Result<Attrs, Failed> {
    let mut built = Map::new();
    for attribute in attrs {
        let value = given.and_then(|given| given.get(&attribute.name));
        let value = match (value, &attribute.default) {
            (Some(value), _) | (None, Some(value)) => value.clone(),
            (None, None) => {
                return failed(format!(
                    "No value supplied for attribute {}",
                    attribute.name
                ));
            }
        };
        built.insert(attribute.name.clone(), value);
    }
    Ok(built.into())
}

fn gather_marks(
    marks: &[MarkType],
    ids: &HashMap<String, MarkId>,
    names: &str,
) -> Result<Vec<MarkId>, BadSchema> {
    let mut found = Vec::new();
    for name in names.split(' ') {
        if let Some(id) = ids.get(name) {
            found.push(*id);
            continue;
        }
        let mut known = false;
        for (index, mark) in marks.iter().enumerate() {
            if name == "_" || mark.groups.iter().any(|group| group == name) {
                found.push(MarkId::try_from(index).unwrap_or_default());
                known = true;
            }
        }
        if !known {
            return Err(BadSchema(format!("unknown mark type {name}")));
        }
    }
    Ok(found)
}

#[derive(Debug, Clone)]
enum Expr {
    Choice(Vec<Self>),
    Seq(Vec<Self>),
    Plus(Box<Self>),
    Star(Box<Self>),
    Opt(Box<Self>),
    Range(usize, Option<usize>, Box<Self>),
    Name(NodeId),
}

struct Tokens<'a> {
    words: Vec<String>,
    position: usize,
    inline: Option<bool>,
    nodes: &'a [NodeType],
    ids: &'a HashMap<String, NodeId>,
}

impl Tokens<'_> {
    fn next(&self) -> Option<&str> {
        self.words.get(self.position).map(String::as_str)
    }

    fn eat(&mut self, token: &str) -> bool {
        let found = self.next() == Some(token);
        if found {
            self.position += 1;
        }
        found
    }

    fn parse_expr(&mut self) -> Result<Expr, BadSchema> {
        let mut exprs = vec![self.parse_seq()?];
        while self.eat("|") {
            exprs.push(self.parse_seq()?);
        }
        Ok(if exprs.len() == 1 {
            exprs.remove(0)
        } else {
            Expr::Choice(exprs)
        })
    }

    fn parse_seq(&mut self) -> Result<Expr, BadSchema> {
        let mut exprs = vec![self.parse_subscript()?];
        while self.next().is_some_and(|next| next != ")" && next != "|") {
            exprs.push(self.parse_subscript()?);
        }
        Ok(if exprs.len() == 1 {
            exprs.remove(0)
        } else {
            Expr::Seq(exprs)
        })
    }

    fn parse_subscript(&mut self) -> Result<Expr, BadSchema> {
        let mut expr = self.parse_atom()?;
        loop {
            if self.eat("+") {
                expr = Expr::Plus(Box::new(expr));
            } else if self.eat("*") {
                expr = Expr::Star(Box::new(expr));
            } else if self.eat("?") {
                expr = Expr::Opt(Box::new(expr));
            } else if self.eat("{") {
                expr = self.parse_range(expr)?;
            } else {
                return Ok(expr);
            }
        }
    }

    fn parse_number(&mut self) -> Result<usize, BadSchema> {
        let number = self
            .next()
            .and_then(|next| next.parse().ok())
            .ok_or_else(|| BadSchema("expected a number".to_owned()))?;
        self.position += 1;
        Ok(number)
    }

    fn parse_range(&mut self, expr: Expr) -> Result<Expr, BadSchema> {
        let min = self.parse_number()?;
        let max = if !self.eat(",") {
            Some(min)
        } else if self.next() == Some("}") {
            None
        } else {
            Some(self.parse_number()?)
        };
        if !self.eat("}") {
            return Err(BadSchema("unclosed braced range".to_owned()));
        }
        Ok(Expr::Range(min, max, Box::new(expr)))
    }

    fn resolve_name(&self, name: &str) -> Result<Vec<NodeId>, BadSchema> {
        if let Some(id) = self.ids.get(name) {
            return Ok(vec![*id]);
        }
        let found: Vec<NodeId> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.groups.iter().any(|group| group == name))
            .filter_map(|(index, _)| NodeId::try_from(index).ok())
            .collect();
        if found.is_empty() {
            return Err(BadSchema(format!("no node type or group {name}")));
        }
        Ok(found)
    }

    fn parse_atom(&mut self) -> Result<Expr, BadSchema> {
        if self.eat("(") {
            let expr = self.parse_expr()?;
            if !self.eat(")") {
                return Err(BadSchema("missing closing paren".to_owned()));
            }
            return Ok(expr);
        }
        let Some(name) = self.next().map(str::to_owned) else {
            return Err(BadSchema("unexpected end".to_owned()));
        };
        if !name.chars().all(is_word) {
            return Err(BadSchema(format!("unexpected token {name}")));
        }
        let mut exprs = Vec::new();
        for id in self.resolve_name(&name)? {
            let inline = !self.nodes[id as usize].is_block;
            if self.inline.is_some_and(|known| known != inline) {
                return Err(BadSchema("mixing inline and block content".to_owned()));
            }
            self.inline = Some(inline);
            exprs.push(Expr::Name(id));
        }
        self.position += 1;
        Ok(if exprs.len() == 1 {
            exprs.remove(0)
        } else {
            Expr::Choice(exprs)
        })
    }
}

fn is_word(letter: char) -> bool {
    letter.is_ascii_alphanumeric() || letter == '_'
}

fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut word = String::new();
    for letter in text.chars() {
        if is_word(letter) {
            word.push(letter);
            continue;
        }
        if !word.is_empty() {
            tokens.push(std::mem::take(&mut word));
        }
        if !letter.is_whitespace() {
            tokens.push(letter.to_string());
        }
    }
    if !word.is_empty() {
        tokens.push(word);
    }
    tokens
}

#[derive(Debug, Clone, Copy)]
struct Edge {
    term: Option<NodeId>,
    to: Option<usize>,
}

#[derive(Default)]
struct Nfa {
    states: Vec<Vec<Edge>>,
}

type EdgeRef = (usize, usize);

impl Nfa {
    fn node(&mut self) -> usize {
        self.states.push(Vec::new());
        self.states.len() - 1
    }

    fn edge(&mut self, from: usize, to: Option<usize>, term: Option<NodeId>) -> EdgeRef {
        self.states[from].push(Edge { term, to });
        (from, self.states[from].len() - 1)
    }

    fn connect(&mut self, edges: &[EdgeRef], to: usize) {
        for (from, index) in edges {
            self.states[*from][*index].to = Some(to);
        }
    }

    fn compile(&mut self, expr: &Expr, from: usize) -> Vec<EdgeRef> {
        match expr {
            Expr::Choice(exprs) => exprs
                .iter()
                .flat_map(|expr| self.compile(expr, from))
                .collect(),
            Expr::Seq(exprs) => {
                let mut from = from;
                for (index, expr) in exprs.iter().enumerate() {
                    let next = self.compile(expr, from);
                    if index == exprs.len() - 1 {
                        return next;
                    }
                    from = self.node();
                    self.connect(&next, from);
                }
                Vec::new()
            }
            Expr::Star(expr) => {
                let looped = self.node();
                self.edge(from, Some(looped), None);
                let inner = self.compile(expr, looped);
                self.connect(&inner, looped);
                vec![self.edge(looped, None, None)]
            }
            Expr::Plus(expr) => {
                let looped = self.node();
                let first = self.compile(expr, from);
                self.connect(&first, looped);
                let again = self.compile(expr, looped);
                self.connect(&again, looped);
                vec![self.edge(looped, None, None)]
            }
            Expr::Opt(expr) => {
                let mut edges = vec![self.edge(from, None, None)];
                edges.extend(self.compile(expr, from));
                edges
            }
            Expr::Range(min, max, expr) => {
                let mut current = from;
                for _ in 0..*min {
                    let next = self.node();
                    let edges = self.compile(expr, current);
                    self.connect(&edges, next);
                    current = next;
                }
                match max {
                    None => {
                        let edges = self.compile(expr, current);
                        self.connect(&edges, current);
                    }
                    Some(max) => {
                        for _ in *min..*max {
                            let next = self.node();
                            self.edge(current, Some(next), None);
                            let edges = self.compile(expr, current);
                            self.connect(&edges, next);
                            current = next;
                        }
                    }
                }
                vec![self.edge(current, None, None)]
            }
            Expr::Name(id) => vec![self.edge(from, None, Some(*id))],
        }
    }

    fn null_from(&self, node: usize) -> Vec<usize> {
        let mut result = Vec::new();
        self.scan(node, &mut result);
        result.sort_unstable_by(|left, right| right.cmp(left));
        result
    }

    fn scan(&self, node: usize, result: &mut Vec<usize>) {
        let edges = &self.states[node];
        if let [
            Edge {
                term: None,
                to: Some(to),
            },
        ] = edges.as_slice()
        {
            return self.scan(*to, result);
        }
        result.push(node);
        for edge in edges {
            if edge.term.is_none()
                && let Some(to) = edge.to
                && !result.contains(&to)
            {
                self.scan(to, result);
            }
        }
    }
}

fn compile(
    expression: &str,
    nodes: &[NodeType],
    ids: &HashMap<String, NodeId>,
    matches: &mut Vec<MatchState>,
) -> Result<MatchId, BadSchema> {
    let mut tokens = Tokens {
        words: tokenize(expression),
        position: 0,
        inline: None,
        nodes,
        ids,
    };
    if tokens.next().is_none() {
        return Ok(EMPTY_MATCH);
    }
    let expr = tokens.parse_expr()?;
    if tokens.next().is_some() {
        return Err(BadSchema(format!("trailing text in {expression}")));
    }
    let mut nfa = Nfa::default();
    nfa.node();
    let edges = nfa.compile(&expr, 0);
    let end = nfa.node();
    nfa.connect(&edges, end);
    let mut labeled = HashMap::new();
    let start = nfa.null_from(0);
    Ok(explore(&nfa, start, &mut labeled, matches))
}

fn explore(
    nfa: &Nfa,
    states: Vec<usize>,
    labeled: &mut HashMap<Vec<usize>, MatchId>,
    matches: &mut Vec<MatchState>,
) -> MatchId {
    let mut out: Vec<(NodeId, Vec<usize>)> = Vec::new();
    for state in &states {
        for edge in &nfa.states[*state] {
            let (Some(term), Some(to)) = (edge.term, edge.to) else {
                continue;
            };
            let found = out.iter().position(|(kind, _)| *kind == term);
            let index = found.unwrap_or_else(|| {
                out.push((term, Vec::new()));
                out.len() - 1
            });
            for node in nfa.null_from(to) {
                if !out[index].1.contains(&node) {
                    out[index].1.push(node);
                }
            }
        }
    }
    let id = matches.len();
    matches.push(MatchState {
        valid_end: states.contains(&(nfa.states.len() - 1)),
        next: Vec::new(),
    });
    labeled.insert(states, id);
    for (kind, mut next) in out {
        next.sort_unstable_by(|left, right| right.cmp(left));
        let known = labeled.get(&next).copied();
        let target = known.unwrap_or_else(|| explore(nfa, next, labeled, matches));
        matches[id].next.push((kind, target));
    }
    id
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn schema() -> Schema {
        let spec: SchemaSpec = serde_json::from_value(json!({
            "nodes": [
                { "name": "doc", "content": "(paragraph | list)+" },
                { "name": "paragraph", "content": "inline*", "group": "block" },
                { "name": "list", "content": "item{1,3}", "group": "block" },
                { "name": "item", "content": "paragraph block*" },
                { "name": "text", "group": "inline" },
                { "name": "image", "group": "inline", "inline": true, "attrs": { "src": {} } },
            ],
            "marks": [
                { "name": "strong" },
                { "name": "link", "excludes": "", "attrs": { "href": {} } },
            ],
        }))
        .unwrap();
        Schema::new(&spec).unwrap()
    }

    #[test]
    fn content_rules_become_states() {
        let schema = schema();
        let doc = schema.node_id("doc").unwrap();
        let paragraph = schema.node_id("paragraph").unwrap();
        let image = schema.node_id("image").unwrap();
        assert!(schema.is_leaf(image));
        assert!(!schema.is_leaf(paragraph));
        assert!(schema.is_textblock(paragraph));
        assert!(!schema.is_textblock(doc));
        assert!(schema.allows_mark_type(paragraph, schema.mark_id("link").unwrap()));
        assert!(!schema.allows_mark_type(doc, schema.mark_id("link").unwrap()));
    }

    #[test]
    fn a_mark_excludes_itself_unless_it_says_otherwise() {
        let schema = schema();
        let strong = schema.mark_id("strong").unwrap();
        let link = schema.mark_id("link").unwrap();
        assert!(schema.excludes(strong, strong));
        assert!(!schema.excludes(link, link));
    }

    #[test]
    fn an_unknown_name_is_a_bad_schema() {
        let spec: SchemaSpec = serde_json::from_value(json!({
            "nodes": [{ "name": "doc", "content": "nothing+" }, { "name": "text" }],
        }))
        .unwrap();
        assert!(Schema::new(&spec).is_err());
    }

    #[test]
    fn tokens_split_as_prosemirror_splits_them() {
        assert_eq!(
            tokenize("(paragraph | list){2,}  block*"),
            [
                "(",
                "paragraph",
                "|",
                "list",
                ")",
                "{",
                "2",
                ",",
                "}",
                "block",
                "*"
            ]
        );
    }
}
