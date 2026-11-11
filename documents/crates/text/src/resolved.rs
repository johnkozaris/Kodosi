use crate::{Failed, Node, failed};

#[derive(Debug, Clone)]
struct Level {
    node: Node,
    index: usize,
    offset: usize,
}

#[derive(Debug, Clone)]
pub struct ResolvedPos {
    pos: usize,
    path: Vec<Level>,
    parent_offset: usize,
}

impl ResolvedPos {
    pub(crate) fn resolve(doc: &Node, pos: usize) -> Result<Self, Failed> {
        if pos > doc.content().size() {
            return failed(format!("Position {pos} out of range"));
        }
        let mut path = Vec::new();
        let mut start = 0;
        let mut parent_offset = pos;
        let mut node = doc.clone();
        loop {
            let (index, offset) = node.content().find_index(parent_offset)?;
            let rest = parent_offset - offset;
            path.push(Level {
                node: node.clone(),
                index,
                offset: start + offset,
            });
            if rest == 0 {
                break;
            }
            let child = node.child(index)?.clone();
            if child.is_text() {
                break;
            }
            node = child;
            parent_offset = rest - 1;
            start += offset + 1;
        }
        Ok(Self {
            pos,
            path,
            parent_offset,
        })
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn depth(&self) -> usize {
        self.path.len() - 1
    }

    pub fn parent_offset(&self) -> usize {
        self.parent_offset
    }

    pub fn parent(&self) -> &Node {
        self.node(self.depth())
    }

    pub fn node(&self, depth: usize) -> &Node {
        &self.path[depth].node
    }

    pub fn index(&self, depth: usize) -> usize {
        self.path[depth].index
    }

    pub fn index_after(&self, depth: usize) -> usize {
        self.index(depth) + usize::from(depth != self.depth() || self.text_offset() != 0)
    }

    pub fn start(&self, depth: usize) -> usize {
        if depth == 0 {
            0
        } else {
            self.path[depth - 1].offset + 1
        }
    }

    pub fn end(&self, depth: usize) -> usize {
        self.start(depth) + self.node(depth).content().size()
    }

    pub fn text_offset(&self) -> usize {
        self.pos - self.path[self.path.len() - 1].offset
    }

    pub fn node_after(&self) -> Result<Option<Node>, Failed> {
        let parent = self.parent();
        let index = self.index(self.depth());
        if index == parent.child_count() {
            return Ok(None);
        }
        let offset = self.text_offset();
        let child = parent.child(index)?;
        if offset == 0 {
            return Ok(Some(child.clone()));
        }
        child.cut(offset, child.node_size()).map(Some)
    }

    pub fn node_before(&self) -> Result<Option<Node>, Failed> {
        let index = self.index(self.depth());
        let offset = self.text_offset();
        if offset != 0 {
            return self.parent().child(index)?.cut(0, offset).map(Some);
        }
        if index == 0 {
            return Ok(None);
        }
        self.parent().child(index - 1).cloned().map(Some)
    }

    pub fn shared_depth(&self, pos: usize) -> usize {
        (1..=self.depth())
            .rev()
            .find(|depth| self.start(*depth) <= pos && self.end(*depth) >= pos)
            .unwrap_or(0)
    }
}
