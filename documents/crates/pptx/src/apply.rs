use std::collections::{BTreeMap, HashMap, HashSet};

use document::{Ordered, Refused};
use serde_json::{Value, json};
use text::{Node, Step};

use crate::{
    model::{
        Change, Model, ROOT_SHAPE, SHAPE_IDS, SLIDE_IDS, Shape, ShapeFields, Slide, Sourced,
        number_in, schema,
    },
    ops,
    writable::{is_box, writable_change},
};

fn invalid(error: impl std::fmt::Display) -> Refused {
    Refused::Invalid(error.to_string())
}

fn no_slide(id: &str) -> Refused {
    Refused::Invalid(format!("There is no slide {id}."))
}

fn no_shape(id: &str) -> Refused {
    Refused::Invalid(format!("There is no shape {id}."))
}

fn holds_text_of(change: &Change, slide: &str, shape: &str) -> bool {
    match change {
        Change::Text {
            slide: other_slide,
            shape: other_shape,
            ..
        } => other_slide == slide && other_shape == shape,
        Change::Group { changes } => changes
            .iter()
            .any(|change| holds_text_of(change, slide, shape)),
        _ => false,
    }
}

fn is_behind(change: &Change, since: &[Ordered<Change>]) -> bool {
    match change {
        Change::Text { slide, shape, .. } => since
            .iter()
            .any(|ordered| holds_text_of(&ordered.change, slide, shape)),
        Change::Group { changes } => changes.iter().any(|change| is_behind(change, since)),
        _ => false,
    }
}

pub(crate) fn apply(
    model: &mut Model,
    change: Change,
    since: &[Ordered<Change>],
) -> Result<Change, Refused> {
    if is_behind(&change, since) {
        return Err(Refused::Behind(
            "The text moved. Move the steps and send them again.".to_owned(),
        ));
    }
    let change = if let Change::Ops { ops } = change {
        ops::check_conflicts(&ops, since)?;
        Change::Group {
            changes: ops::to_changes(model, &ops).map_err(Refused::Invalid)?,
        }
    } else {
        change
    };
    let change = past_removals(model, change, since);
    let change = returning(model, change);
    let mut copy = model.clone();
    apply_one(&mut copy, &change)?;
    writable_change(model, &copy).map_err(Refused::Invalid)?;
    *model = copy;
    Ok(change)
}

type Removed<'s> = (HashSet<&'s str>, HashSet<(&'s str, &'s str)>);

fn removed_by<'s>(change: &'s Change, removed: &mut Removed<'s>) {
    match change {
        Change::RemoveSlide { slide } => {
            removed.0.insert(slide);
        }
        Change::RemoveShape { slide, shape } => {
            removed.1.insert((slide, shape));
        }
        Change::Group { changes } => {
            for change in changes {
                removed_by(change, removed);
            }
        }
        _ => {}
    }
}

/// Drops sub-changes targeting slides or shapes concurrently removed in `since` while keeping the
/// rest of the batch. References to IDs that never existed are left intact so `apply_one` rejects
/// them.
fn past_removals(model: &Model, change: Change, since: &[Ordered<Change>]) -> Change {
    let mut removed = (HashSet::new(), HashSet::new());
    for ordered in since {
        removed_by(&ordered.change, &mut removed);
    }
    if removed.0.is_empty() && removed.1.is_empty() {
        return change;
    }
    without_removed(model, change, &removed).unwrap_or(Change::Group {
        changes: Vec::new(),
    })
}

fn without_removed(model: &Model, change: Change, removed: &Removed<'_>) -> Option<Change> {
    if let Change::Group { changes } = change {
        return Some(Change::Group {
            changes: changes
                .into_iter()
                .filter_map(|change| without_removed(model, change, removed))
                .collect(),
        });
    }
    let (slides, shapes) = removed;
    let gone = match &change {
        Change::Shape { slide, shape, .. }
        | Change::Text { slide, shape, .. }
        | Change::RemoveShape { slide, shape } => {
            let there = model
                .slide(slide)
                .is_some_and(|known| known.shapes.iter().any(|known| known.id == *shape));
            !there
                && (slides.contains(slide.as_str())
                    || shapes.contains(&(slide.as_str(), shape.as_str())))
        }
        Change::AddShape { slide, .. }
        | Change::MoveSlide { slide, .. }
        | Change::RemoveSlide { slide } => {
            model.slide(slide).is_none() && slides.contains(slide.as_str())
        }
        Change::AddSlide { .. } | Change::Ops { .. } | Change::Group { .. } => false,
    };
    (!gone).then_some(change)
}

/// If an undo re-adds a text box whose original XML fragment was purged (e.g. after a save and
/// reopen), strip missing raw XML references so it recreates cleanly as a new box with its text
/// and styling intact.
fn returning(model: &Model, change: Change) -> Change {
    match change {
        Change::AddShape {
            slide,
            mut shape,
            keep,
        } if keep.is_empty() => {
            let held = shape
                .get("source")
                .and_then(Value::as_str)
                .is_none_or(|source| {
                    model
                        .slide(&slide)
                        .is_some_and(|known| known.sources.contains_key(source))
                });
            if !held && Shape::from_snapshot(&shape).is_ok_and(|parsed| is_box(&parsed)) {
                shape["source"] = Value::Null;
                if let Some(text) = shape.get_mut("text") {
                    without_gone_pieces(text, &model.raws);
                }
            }
            Change::AddShape { slide, shape, keep }
        }
        Change::Group { changes } => Change::Group {
            changes: changes
                .into_iter()
                .map(|change| returning(model, change))
                .collect(),
        },
        change => change,
    }
}

fn without_gone_pieces(node: &mut Value, raws: &HashMap<String, String>) {
    let gone = |key: &Value| key.as_str().is_some_and(|key| !raws.contains_key(key));
    let attrs = node.get_mut("attrs").and_then(Value::as_object_mut);
    for (name, key) in attrs.into_iter().flatten() {
        if matches!(name.as_str(), "props" | "end") && gone(key) {
            *key = Value::Null;
        }
    }
    let marks = node.get_mut("marks").and_then(Value::as_array_mut);
    for mark in marks.into_iter().flatten() {
        if let Some(key) = mark.pointer_mut("/attrs/props")
            && gone(key)
        {
            *key = Value::Null;
        }
    }
    let Some(content) = node.get_mut("content").and_then(Value::as_array_mut) else {
        return;
    };
    let marks_of = |node: &Value| {
        node.get("marks")
            .filter(|marks| marks.as_array().is_some_and(|marks| !marks.is_empty()))
            .cloned()
    };
    let mut kept: Vec<Value> = Vec::with_capacity(content.len());
    for mut child in content.drain(..) {
        if child["type"] == "inlineOpaque" && child.pointer("/attrs/raw").is_some_and(gone) {
            let text = child.pointer("/attrs/text").and_then(Value::as_str);
            let Some(text) = text.filter(|text| !text.is_empty()) else {
                continue;
            };
            let mut shown = json!({ "type": "text", "text": text });
            if let Some(marks) = marks_of(&child) {
                shown["marks"] = marks;
            }
            child = shown;
        }
        without_gone_pieces(&mut child, raws);
        match kept.last_mut() {
            Some(last)
                if last["type"] == "text"
                    && child["type"] == "text"
                    && marks_of(last) == marks_of(&child) =>
            {
                let joined = format!(
                    "{}{}",
                    last["text"].as_str().unwrap_or_default(),
                    child["text"].as_str().unwrap_or_default()
                );
                last["text"] = Value::String(joined);
            }
            _ => kept.push(child),
        }
    }
    *content = kept;
}

fn shape_mut<'m>(model: &'m mut Model, slide: &str, shape: &str) -> Result<&'m mut Shape, Refused> {
    model
        .slide_mut(slide)
        .ok_or_else(|| no_slide(slide))?
        .shapes
        .iter_mut()
        .find(|known| known.id == shape)
        .ok_or_else(|| no_shape(shape))
}

pub(crate) fn run_steps(tree: &Node, steps: &[Value]) -> Result<Node, text::Failed> {
    let mut tree = tree.clone();
    for step in steps {
        tree = Step::from_json(schema(), step)?.apply(schema(), &tree)?;
    }
    Ok(tree)
}

fn set_fields(shape: &mut Shape, fields: &ShapeFields) {
    let ShapeFields {
        x,
        y,
        width,
        height,
        rotation,
        fill,
        line,
        order,
    } = fields;
    shape.x = x.unwrap_or(shape.x);
    shape.y = y.unwrap_or(shape.y);
    shape.width = width.unwrap_or(shape.width);
    shape.height = height.unwrap_or(shape.height);
    shape.rotation = rotation.unwrap_or(shape.rotation);
    shape.order = order.unwrap_or(shape.order);
    if let Some(fill) = fill {
        shape.fill.clone_from(fill);
    }
    if let Some(line) = line {
        shape.line.clone_from(line);
    }
}

/// Registers merged raw XML (`keep`) in `model.raws` and `slide.sources` so the shape can
/// round-trip through later saves.
fn add_shape(
    model: &mut Model,
    slide: &str,
    shape: &Value,
    keep: &BTreeMap<String, String>,
) -> Result<(), Refused> {
    model.raws.extend(keep.clone());
    let mut shape = Shape::from_snapshot(shape).map_err(Refused::Invalid)?;
    let number = number_in(&shape.id, &SHAPE_IDS).ok_or_else(|| {
        Refused::Invalid(format!(
            "A shape id is a number from 0 to 4294967295, and {} is not.",
            shape.id
        ))
    })?;
    let target = model.slide_mut(slide).ok_or_else(|| no_slide(slide))?;
    if target.shapes.iter().any(|known| known.id == shape.id) {
        return Err(Refused::Invalid(format!(
            "The shape {} is there already.",
            shape.id
        )));
    }
    if shape.text.is_some() && shape.look.is_null() {
        shape.look.clone_from(&target.text_look);
    }
    if let Some(source) = &shape.source
        && keep.contains_key(source)
    {
        target.sources.insert(source.clone(), Sourced::of(&shape));
    }
    target.last_shape = target.last_shape.max(number);
    target.shapes.push(shape);
    Ok(())
}

fn apply_one(model: &mut Model, change: &Change) -> Result<(), Refused> {
    match change {
        Change::Shape {
            slide,
            shape,
            fields,
        } => set_fields(shape_mut(model, slide, shape)?, fields),
        Change::Text {
            slide,
            shape,
            steps,
            keep,
        } => {
            let target = shape_mut(model, slide, shape)?;
            let tree = target
                .text
                .as_ref()
                .ok_or_else(|| Refused::Invalid(format!("The shape {shape} holds no text.")))?;
            target.text = Some(run_steps(tree, steps).map_err(invalid)?);
            model.raws.extend(keep.clone());
        }
        Change::MoveSlide { slide, position } => {
            model
                .slide_mut(slide)
                .ok_or_else(|| no_slide(slide))?
                .position = *position;
            model.sort_slides();
        }
        Change::AddSlide { slide, position } => {
            if number_in(slide, &SLIDE_IDS).is_none() {
                return Err(Refused::Invalid(format!(
                    "A slide id is a number from 256 to 2147483647, and {slide} is not."
                )));
            }
            if model.slide(slide).is_some() {
                return Err(Refused::Invalid(format!(
                    "The slide {slide} is there already."
                )));
            }
            let text_look = model
                .slides
                .first()
                .map_or(Value::Null, |first| first.text_look.clone());
            model.slides.push(Slide {
                id: slide.clone(),
                part: None,
                position: *position,
                shapes: Vec::new(),
                behind: Vec::new(),
                background: None,
                text_look,
                last_shape: ROOT_SHAPE,
                sources: HashMap::new(),
            });
            model.sort_slides();
        }
        Change::RemoveSlide { slide } => {
            if model.slides.len() == 1 {
                return Err(Refused::Invalid(
                    "A presentation keeps its last slide.".to_owned(),
                ));
            }
            model.slides.retain(|known| known.id != *slide);
        }
        Change::AddShape { slide, shape, keep } => add_shape(model, slide, shape, keep)?,
        Change::RemoveShape { slide, shape } => {
            model
                .slide_mut(slide)
                .ok_or_else(|| no_slide(slide))?
                .shapes
                .retain(|known| known.id != *shape);
        }
        Change::Group { changes } => {
            for change in changes {
                apply_one(model, change)?;
            }
        }
        Change::Ops { .. } => return Err(Refused::Invalid("Ops cannot hold ops.".to_owned())),
    }
    Ok(())
}

/// Returns true if an external write preserved the shape's identity under the same ID so changes
/// can be expressed as field/text patches instead of a remove and re-add.
fn same_shape(old: &Shape, new: &Shape) -> bool {
    old.name == new.name
        && old.kind == new.kind
        && old.placeholder == new.placeholder
        && old.geometry == new.geometry
        && old.asset == new.asset
        && old.link == new.link
        && old.look == new.look
        && old.text.is_some() == new.text.is_some()
}

fn stack<'s>(slide: &'s Slide, other: &Slide) -> Vec<&'s str> {
    let mut shapes: Vec<&Shape> = slide
        .shapes
        .iter()
        .filter(|shape| other.shapes.iter().any(|known| known.id == shape.id))
        .collect();
    shapes.sort_by_key(|shape| shape.order);
    shapes.iter().map(|shape| shape.id.as_str()).collect()
}

/// True if an external write reordered or added shapes, requiring z-order updates; pure deletions
/// leave remaining shapes' relative order intact.
fn stacked_anew(old: &Slide, new: &Slide) -> bool {
    stack(old, new) != stack(new, old)
        || new.shapes.iter().any(|shape| {
            old.shapes
                .iter()
                .find(|known| known.id == shape.id)
                .is_none_or(|known| !same_shape(known, shape))
        })
}

/// Collects raw XML fragments referenced by `json` into `keep` for merge changes.
fn pieces_in(json: &Value, raws: &HashMap<String, String>, keep: &mut BTreeMap<String, String>) {
    match json {
        Value::Object(fields) => {
            for (name, value) in fields {
                if let ("props" | "end" | "raw" | "source", Some(key)) =
                    (name.as_str(), value.as_str())
                    && let Some(xml) = raws.get(key)
                {
                    keep.insert(key.to_owned(), xml.clone());
                }
                pieces_in(value, raws, keep);
            }
        }
        Value::Array(values) => {
            for value in values {
                pieces_in(value, raws, keep);
            }
        }
        _ => {}
    }
}

fn added(slide: &str, shape: &Shape, raws: &HashMap<String, String>) -> Change {
    let shape = shape.snapshot();
    let mut keep = BTreeMap::new();
    pieces_in(&shape, raws, &mut keep);
    Change::AddShape {
        slide: slide.to_owned(),
        shape,
        keep,
    }
}

fn shape_changes(
    slide: &str,
    old: &Shape,
    new: &Shape,
    stacked: bool,
    raws: &HashMap<String, String>,
    out: &mut Vec<Change>,
) {
    if !same_shape(old, new) {
        out.push(Change::RemoveShape {
            slide: slide.to_owned(),
            shape: old.id.clone(),
        });
        out.push(added(slide, new, raws));
        return;
    }
    let mut fields = ShapeFields::default();
    if (old.x, old.y) != (new.x, new.y) {
        fields.x = Some(new.x);
        fields.y = Some(new.y);
    }
    if (old.width, old.height) != (new.width, new.height) {
        fields.width = Some(new.width);
        fields.height = Some(new.height);
    }
    if old.rotation != new.rotation {
        fields.rotation = Some(new.rotation);
    }
    if old.fill != new.fill {
        fields.fill = Some(new.fill.clone());
    }
    if old.line != new.line {
        fields.line = Some(new.line.clone());
    }
    if stacked && old.order != new.order {
        fields.order = Some(new.order);
    }
    if fields != ShapeFields::default() {
        out.push(Change::Shape {
            slide: slide.to_owned(),
            shape: new.id.clone(),
            fields,
        });
    }
    if let (true, Some(old_text), Some(new_text)) = (old.text != new.text, &old.text, &new.text) {
        let steps = vec![
            Step::Replace {
                from: 0,
                to: old_text.content().size(),
                slice: text::Slice::new(new_text.content().clone(), 0, 0),
                structure: false,
            }
            .to_json(schema()),
        ];
        let mut keep = BTreeMap::new();
        pieces_in(&Value::from(steps.clone()), raws, &mut keep);
        out.push(Change::Text {
            slide: slide.to_owned(),
            shape: new.id.clone(),
            steps,
            keep,
        });
    }
}

pub(crate) fn compare(from: &Model, to: &Model) -> Vec<Change> {
    let mut changes = Vec::new();
    for slide in &to.slides {
        let Some(old) = from.slide(&slide.id) else {
            changes.push(Change::AddSlide {
                slide: slide.id.clone(),
                position: slide.position,
            });
            for shape in &slide.shapes {
                changes.push(added(&slide.id, shape, &to.raws));
            }
            continue;
        };
        if old.position.total_cmp(&slide.position).is_ne() {
            changes.push(Change::MoveSlide {
                slide: slide.id.clone(),
                position: slide.position,
            });
        }
        let stacked = stacked_anew(old, slide);
        for shape in &slide.shapes {
            match old.shapes.iter().find(|known| known.id == shape.id) {
                Some(known) => {
                    shape_changes(&slide.id, known, shape, stacked, &to.raws, &mut changes);
                }
                None => changes.push(added(&slide.id, shape, &to.raws)),
            }
        }
        for shape in &old.shapes {
            if !slide.shapes.iter().any(|known| known.id == shape.id) {
                changes.push(Change::RemoveShape {
                    slide: slide.id.clone(),
                    shape: shape.id.clone(),
                });
            }
        }
    }
    for slide in &from.slides {
        if to.slide(&slide.id).is_none() {
            changes.push(Change::RemoveSlide {
                slide: slide.id.clone(),
            });
        }
    }
    changes
}
