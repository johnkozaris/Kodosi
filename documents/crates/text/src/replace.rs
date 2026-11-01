use crate::{Failed, Fragment, Node, ResolvedPos, Schema, Slice, failed};

pub(crate) fn replace(
    schema: &Schema,
    from: &ResolvedPos,
    to: &ResolvedPos,
    slice: &Slice,
) -> Result<Node, Failed> {
    if slice.open_start > from.depth() {
        return failed("Inserted content deeper than insertion position");
    }
    if from.depth() + slice.open_end != to.depth() + slice.open_start {
        return failed("Inconsistent open depths");
    }
    replace_outer(schema, from, to, slice, 0)
}

fn replace_outer(
    schema: &Schema,
    from: &ResolvedPos,
    to: &ResolvedPos,
    slice: &Slice,
    depth: usize,
) -> Result<Node, Failed> {
    let index = from.index(depth);
    let node = from.node(depth);
    if index == to.index(depth) && depth + slice.open_start < from.depth() {
        let inner = replace_outer(schema, from, to, slice, depth + 1)?;
        Ok(node.copy(node.content().replace_child(index, inner)?))
    } else if slice.content.size() == 0 {
        close(schema, node, replace_two_way(schema, from, to, depth)?)
    } else if slice.open_start == 0
        && slice.open_end == 0
        && from.depth() == depth
        && to.depth() == depth
    {
        let parent = from.parent();
        let content = parent.content();
        close(
            schema,
            parent,
            content
                .cut(0, from.parent_offset())?
                .append(&slice.content)
                .append(&content.cut_end(to.parent_offset())?),
        )
    } else {
        let (start, end) = prepare_slice(slice, from)?;
        close(
            schema,
            node,
            replace_three_way(schema, from, &start, &end, to, depth)?,
        )
    }
}

fn check_join(schema: &Schema, main: &Node, sub: &Node) -> Result<(), Failed> {
    if schema.compatible_content(sub.kind(), main.kind()) {
        Ok(())
    } else {
        failed(format!(
            "Cannot join {} onto {}",
            schema.node_name(sub.kind()),
            schema.node_name(main.kind())
        ))
    }
}

fn joinable(
    schema: &Schema,
    before: &ResolvedPos,
    after: &ResolvedPos,
    depth: usize,
) -> Result<Node, Failed> {
    let node = before.node(depth);
    check_join(schema, node, after.node(depth))?;
    Ok(node.clone())
}

fn add_node(child: Node, target: &mut Vec<Node>) {
    match target.last_mut() {
        Some(last) if child.is_text() && child.same_markup(last) => {
            *last = child.copy_text_after(last);
        }
        _ => target.push(child),
    }
}

impl Node {
    fn copy_text_after(&self, before: &Self) -> Self {
        before.joined_text(self)
    }
}

fn add_range(
    start: Option<&ResolvedPos>,
    end: Option<&ResolvedPos>,
    depth: usize,
    target: &mut Vec<Node>,
) -> Result<(), Failed> {
    let Some(any) = end.or(start) else {
        return Ok(());
    };
    let node = any.node(depth);
    let mut start_index = 0;
    let end_index = end.map_or_else(|| node.child_count(), |end| end.index(depth));
    if let Some(start) = start {
        start_index = start.index(depth);
        if start.depth() > depth {
            start_index += 1;
        } else if start.text_offset() != 0 {
            if let Some(after) = start.node_after()? {
                add_node(after, target);
            }
            start_index += 1;
        }
    }
    for index in start_index..end_index {
        add_node(node.child(index)?.clone(), target);
    }
    if let Some(end) = end
        && end.depth() == depth
        && end.text_offset() != 0
        && let Some(before) = end.node_before()?
    {
        add_node(before, target);
    }
    Ok(())
}

fn close(schema: &Schema, node: &Node, content: Fragment) -> Result<Node, Failed> {
    if !schema.valid_content(node.kind(), &content) {
        return failed(format!(
            "Invalid content for node {}",
            schema.node_name(node.kind())
        ));
    }
    Ok(node.copy(content))
}

fn replace_three_way(
    schema: &Schema,
    from: &ResolvedPos,
    start: &ResolvedPos,
    end: &ResolvedPos,
    to: &ResolvedPos,
    depth: usize,
) -> Result<Fragment, Failed> {
    let open_start = if from.depth() > depth {
        Some(joinable(schema, from, start, depth + 1)?)
    } else {
        None
    };
    let open_end = if to.depth() > depth {
        Some(joinable(schema, end, to, depth + 1)?)
    } else {
        None
    };
    let mut content = Vec::new();
    add_range(None, Some(from), depth, &mut content)?;
    match (&open_start, &open_end) {
        (Some(first), Some(last)) if start.index(depth) == end.index(depth) => {
            check_join(schema, first, last)?;
            let inner = replace_three_way(schema, from, start, end, to, depth + 1)?;
            add_node(close(schema, first, inner)?, &mut content);
        }
        _ => {
            if let Some(first) = &open_start {
                let inner = replace_two_way(schema, from, start, depth + 1)?;
                add_node(close(schema, first, inner)?, &mut content);
            }
            add_range(Some(start), Some(end), depth, &mut content)?;
            if let Some(last) = &open_end {
                let inner = replace_two_way(schema, end, to, depth + 1)?;
                add_node(close(schema, last, inner)?, &mut content);
            }
        }
    }
    add_range(Some(to), None, depth, &mut content)?;
    Ok(Fragment::new(content))
}

fn replace_two_way(
    schema: &Schema,
    from: &ResolvedPos,
    to: &ResolvedPos,
    depth: usize,
) -> Result<Fragment, Failed> {
    let mut content = Vec::new();
    add_range(None, Some(from), depth, &mut content)?;
    if from.depth() > depth {
        let kind = joinable(schema, from, to, depth + 1)?;
        let inner = replace_two_way(schema, from, to, depth + 1)?;
        add_node(close(schema, &kind, inner)?, &mut content);
    }
    add_range(Some(to), None, depth, &mut content)?;
    Ok(Fragment::new(content))
}

fn prepare_slice(slice: &Slice, along: &ResolvedPos) -> Result<(ResolvedPos, ResolvedPos), Failed> {
    let extra = along.depth() - slice.open_start;
    let mut node = along.node(extra).copy(slice.content.clone());
    for depth in (0..extra).rev() {
        node = along.node(depth).copy(Fragment::new(vec![node]));
    }
    let start = node.resolve(slice.open_start + extra)?;
    let Some(end) = node.content().size().checked_sub(slice.open_end + extra) else {
        return failed("The slice is smaller than its open sides");
    };
    Ok((start, node.resolve(end)?))
}
