use document::{Ordered, Refused};
use serde_json::Value;
use text::{Node, Step};

use crate::{
    model::{Change, Model, Shape, ShapeFields, Slide, schema},
    ops,
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
    let mut copy = model.clone();
    apply_one(&mut copy, &change)?;
    *model = copy;
    Ok(change)
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
        } => {
            let target = shape_mut(model, slide, shape)?;
            let tree = target
                .text
                .as_ref()
                .ok_or_else(|| Refused::Invalid(format!("The shape {shape} holds no text.")))?;
            target.text = Some(run_steps(tree, steps).map_err(invalid)?);
        }
        Change::MoveSlide { slide, position } => {
            model
                .slide_mut(slide)
                .ok_or_else(|| no_slide(slide))?
                .position = *position;
            model.sort_slides();
        }
        Change::AddSlide { slide, position } => {
            if model.slide(slide).is_some() {
                return Err(Refused::Invalid(format!(
                    "The slide {slide} is there already."
                )));
            }
            model.slides.push(Slide {
                id: slide.clone(),
                part: None,
                position: *position,
                shapes: Vec::new(),
                behind: Vec::new(),
                background: None,
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
        Change::AddShape { slide, shape } => {
            let shape = Shape::from_snapshot(shape).map_err(Refused::Invalid)?;
            let target = model.slide_mut(slide).ok_or_else(|| no_slide(slide))?;
            if target.shapes.iter().any(|known| known.id == shape.id) {
                return Err(Refused::Invalid(format!(
                    "The shape {} is there already.",
                    shape.id
                )));
            }
            target.shapes.push(shape);
        }
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

/// Whether a write kept the kind of shape under its id, so that fields and text steps carry what
/// changed. Another kind, name, geometry, picture, link or look of its text makes it a shape of
/// its own.
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

fn shape_changes(slide: &str, old: &Shape, new: &Shape, out: &mut Vec<Change>) {
    if !same_shape(old, new) {
        out.push(Change::RemoveShape {
            slide: slide.to_owned(),
            shape: old.id.clone(),
        });
        out.push(Change::AddShape {
            slide: slide.to_owned(),
            shape: new.snapshot(),
        });
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
    if old.order != new.order {
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
        out.push(Change::Text {
            slide: slide.to_owned(),
            shape: new.id.clone(),
            steps: vec![
                Step::Replace {
                    from: 0,
                    to: old_text.content().size(),
                    slice: text::Slice::new(new_text.content().clone(), 0, 0),
                    structure: false,
                }
                .to_json(schema()),
            ],
        });
    }
}

/// What turns one state of the deck into another, by the ids of slides and shapes.
pub(crate) fn compare(from: &Model, to: &Model) -> Vec<Change> {
    let mut changes = Vec::new();
    for slide in &to.slides {
        let Some(old) = from.slide(&slide.id) else {
            changes.push(Change::AddSlide {
                slide: slide.id.clone(),
                position: slide.position,
            });
            for shape in &slide.shapes {
                changes.push(Change::AddShape {
                    slide: slide.id.clone(),
                    shape: shape.snapshot(),
                });
            }
            continue;
        };
        if old.position.total_cmp(&slide.position).is_ne() {
            changes.push(Change::MoveSlide {
                slide: slide.id.clone(),
                position: slide.position,
            });
        }
        for shape in &slide.shapes {
            match old.shapes.iter().find(|known| known.id == shape.id) {
                Some(known) => shape_changes(&slide.id, known, shape, &mut changes),
                None => changes.push(Change::AddShape {
                    slide: slide.id.clone(),
                    shape: shape.snapshot(),
                }),
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
