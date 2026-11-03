use document::{Guide, GuideField, GuideOp, Ordered, Refused};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use text::{Fragment, Node, Slice, Step};

use crate::model::{Change, Model, Query, Shape, ShapeFields, schema};

const EMU_PER_POINT: i64 = 12_700;
const SMALL_TEXT: usize = 8;

/// What an agent means. The server turns each op into normal changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Op {
    SetText {
        slide: String,
        shape: String,
        text: String,
    },
    ReplaceText {
        find: String,
        with: String,
        #[serde(default)]
        slide: Option<String>,
    },
    MoveShape {
        slide: String,
        shape: String,
        #[serde(default)]
        x: Option<f64>,
        #[serde(default)]
        y: Option<f64>,
        #[serde(default)]
        width: Option<f64>,
        #[serde(default)]
        height: Option<f64>,
    },
    SetFill {
        slide: String,
        shape: String,
        color: String,
    },
    AddTextBox {
        slide: String,
        text: String,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    RemoveShape {
        slide: String,
        shape: String,
    },
    AddSlide {
        #[serde(default)]
        after: Option<String>,
    },
    MoveSlide {
        slide: String,
        #[serde(default)]
        after: Option<String>,
    },
    RemoveSlide {
        slide: String,
    },
}

fn field(name: &str, kind: &str, required: bool, does: &str) -> GuideField {
    GuideField {
        name: name.to_owned(),
        kind: kind.to_owned(),
        required,
        does: does.to_owned(),
    }
}

fn op(name: &str, does: &str, fields: Vec<GuideField>) -> GuideOp {
    GuideOp {
        name: name.to_owned(),
        does: does.to_owned(),
        fields,
    }
}

pub(crate) fn guide() -> Guide {
    let slide = || field("slide", "slide id", true, "The slide");
    let shape = || field("shape", "shape id", true, "The shape");
    let at = |name: &str| field(name, "points", true, "In points from the top left");
    let maybe = |name: &str| field(name, "points", false, "The new value, in points");
    Guide {
        format: "pptx".to_owned(),
        ops: vec![
            op(
                "setText",
                "Replaces all the text of a shape",
                vec![
                    slide(),
                    shape(),
                    field("text", "text", true, "One line for each paragraph"),
                ],
            ),
            op(
                "replaceText",
                "Replaces text and keeps its look",
                vec![
                    field("find", "text", true, "The exact text"),
                    field("with", "text", true, "The new text"),
                    field("slide", "slide id", false, "Looks only on this slide"),
                ],
            ),
            op(
                "moveShape",
                "Moves or sizes a shape",
                vec![
                    slide(),
                    shape(),
                    maybe("x"),
                    maybe("y"),
                    maybe("width"),
                    maybe("height"),
                ],
            ),
            op(
                "setFill",
                "Sets the color of a shape",
                vec![
                    slide(),
                    shape(),
                    field("color", "hex color", true, "Such as FF0000"),
                ],
            ),
            op(
                "addTextBox",
                "Adds a text box",
                vec![
                    slide(),
                    field("text", "text", true, "Its text"),
                    at("x"),
                    at("y"),
                    at("width"),
                    at("height"),
                ],
            ),
            op("removeShape", "Removes a shape", vec![slide(), shape()]),
            op(
                "addSlide",
                "Adds an empty slide",
                vec![field(
                    "after",
                    "slide id",
                    false,
                    "Goes after this slide. Without it, at the end",
                )],
            ),
            op(
                "moveSlide",
                "Moves a slide",
                vec![
                    slide(),
                    field(
                        "after",
                        "slide id",
                        false,
                        "Goes after this slide. Without it, first",
                    ),
                ],
            ),
            op("removeSlide", "Removes a slide", vec![slide()]),
        ],
    }
}

/// The shapes that this op acts on.
fn targets(op: &Op) -> Option<(&str, &str)> {
    match op {
        Op::SetText { slide, shape, .. }
        | Op::MoveShape { slide, shape, .. }
        | Op::SetFill { slide, shape, .. }
        | Op::RemoveShape { slide, shape } => Some((slide, shape)),
        _ => None,
    }
}

fn touched(change: &Change, out: &mut Vec<(String, String)>) {
    match change {
        Change::Shape { slide, shape, .. }
        | Change::Text { slide, shape, .. }
        | Change::RemoveShape { slide, shape } => out.push((slide.clone(), shape.clone())),
        Change::Group { changes } => {
            for change in changes {
                touched(change, out);
            }
        }
        _ => {}
    }
}

pub(crate) fn check_conflicts(ops: &[Op], since: &[Ordered<Change>]) -> Result<(), Refused> {
    for ordered in since {
        let mut changed = Vec::new();
        touched(&ordered.change, &mut changed);
        for (slide, shape) in ops.iter().filter_map(targets) {
            if changed.iter().any(|(s, sh)| s == slide && sh == shape) {
                return Err(Refused::Behind(format!(
                    "{} changed shape {shape} on slide {slide} after you read it. Read again.",
                    ordered.author
                )));
            }
        }
    }
    Ok(())
}

fn emu(points: f64) -> i64 {
    #[expect(clippy::cast_possible_truncation, reason = "a slide is far smaller")]
    let emu = (points * 12_700.0).round() as i64;
    emu
}

fn plain_tree(text: &str) -> Result<Node, String> {
    let paragraph = schema().node_id("paragraph").unwrap_or_default();
    let mut paragraphs = Vec::new();
    for line in text.split('\n') {
        let content = if line.is_empty() {
            Fragment::default()
        } else {
            Fragment::from_array(vec![
                Node::text(schema(), line, Vec::new()).map_err(|failed| failed.0)?,
            ])
        };
        paragraphs.push(
            Node::new(schema(), paragraph, None, content, Vec::new()).map_err(|failed| failed.0)?,
        );
    }
    Node::new(
        schema(),
        schema().top_node(),
        None,
        Fragment::from_array(paragraphs),
        Vec::new(),
    )
    .map_err(|failed| failed.0)
}

fn shape_of<'m>(model: &'m Model, slide: &str, shape: &str) -> Result<&'m Shape, String> {
    model
        .slide(slide)
        .ok_or_else(|| format!("There is no slide {slide}."))?
        .shapes
        .iter()
        .find(|known| known.id == shape)
        .ok_or_else(|| format!("There is no shape {shape}."))
}

fn set_text(model: &Model, slide: &str, shape: &str, text: &str) -> Result<Change, String> {
    let target = shape_of(model, slide, shape)?;
    let tree = target
        .text
        .as_ref()
        .ok_or_else(|| format!("The shape {shape} holds no text."))?;
    let new = plain_tree(text)?;
    let step = Step::Replace {
        from: 0,
        to: tree.content().size(),
        slice: Slice::new(new.content().clone(), 0, 0),
        structure: false,
    };
    Ok(Change::Text {
        slide: slide.to_owned(),
        shape: shape.to_owned(),
        steps: vec![step.to_json(schema())],
    })
}

/// Each place of the text in one shape's tree, from last to first, so earlier positions hold.
fn places_in(tree: &Node, find: &str) -> Vec<(usize, usize)> {
    let needle: Vec<u16> = find.encode_utf16().collect();
    let mut places = Vec::new();
    let mut paragraph_start = 1;
    for paragraph in tree.content().children() {
        let mut position = paragraph_start;
        for child in paragraph.content().children() {
            if let Some(text) = child.text_of() {
                let units: Vec<u16> = text.encode_utf16().collect();
                let mut index = 0;
                while index + needle.len() <= units.len() {
                    if units[index..index + needle.len()] == needle[..] {
                        places.push((position + index, position + index + needle.len()));
                        index += needle.len();
                    } else {
                        index += 1;
                    }
                }
            }
            position += child.node_size();
        }
        paragraph_start += paragraph.node_size();
    }
    places.reverse();
    places
}

fn replace_in_tree(tree: &Node, find: &str, with: &str) -> Result<Vec<Value>, String> {
    let mut steps = Vec::new();
    for (from, to) in places_in(tree, find) {
        let marks = tree
            .node_at(from)
            .ok()
            .flatten()
            .map(|node| node.marks().to_vec())
            .unwrap_or_default();
        let slice = if with.is_empty() {
            Slice::empty()
        } else {
            let node = Node::text(schema(), with, marks).map_err(|failed| failed.0)?;
            Slice::new(Fragment::from_array(vec![node]), 0, 0)
        };
        let step = Step::Replace {
            from,
            to,
            slice,
            structure: false,
        };
        steps.push(step.to_json(schema()));
    }
    Ok(steps)
}

fn replace_everywhere(
    model: &Model,
    find: &str,
    with: &str,
    only: Option<&str>,
) -> Result<Vec<Change>, String> {
    let mut changes = Vec::new();
    for slide in &model.slides {
        if only.is_some_and(|only| only != slide.id) {
            continue;
        }
        for shape in &slide.shapes {
            let Some(tree) = &shape.text else {
                continue;
            };
            let steps = replace_in_tree(tree, find, with)?;
            if !steps.is_empty() {
                changes.push(Change::Text {
                    slide: slide.id.clone(),
                    shape: shape.id.clone(),
                    steps,
                });
            }
        }
    }
    if changes.is_empty() {
        return Err(format!("The text \"{find}\" is not there."));
    }
    Ok(changes)
}

fn next_position(model: &Model, after: Option<&str>) -> Result<f64, String> {
    let Some(after) = after else {
        return Ok(model
            .slides
            .last()
            .map_or(0.0, |slide| slide.position + 1.0));
    };
    let index = model
        .slides
        .iter()
        .position(|slide| slide.id == after)
        .ok_or_else(|| format!("There is no slide {after}."))?;
    let here = model.slides[index].position;
    Ok(model
        .slides
        .get(index + 1)
        .map_or(here + 1.0, |next| f64::midpoint(here, next.position)))
}

fn first_position(model: &Model) -> f64 {
    model
        .slides
        .first()
        .map_or(0.0, |slide| slide.position - 1.0)
}

fn new_shape_id(model: &Model, slide: &str, taken: u64) -> Result<String, String> {
    let highest = model
        .slide(slide)
        .ok_or_else(|| format!("There is no slide {slide}."))?
        .shapes
        .iter()
        .filter_map(|shape| shape.id.parse::<u64>().ok())
        .max()
        .unwrap_or(1);
    Ok((highest + taken + 1).to_string())
}

fn new_slide_id(model: &Model, taken: u64) -> String {
    let highest = model
        .slides
        .iter()
        .filter_map(|slide| slide.id.parse::<u64>().ok())
        .max()
        .unwrap_or(255);
    (highest + taken + 1).to_string()
}

#[expect(clippy::too_many_lines, reason = "one arm for each op")]
pub(crate) fn to_changes(model: &Model, ops: &[Op]) -> Result<Vec<Change>, String> {
    let mut changes = Vec::with_capacity(ops.len());
    let mut taken = 0;
    for op in ops {
        match op {
            Op::SetText { slide, shape, text } => {
                changes.push(set_text(model, slide, shape, text)?);
            }
            Op::ReplaceText { find, with, slide } => {
                changes.extend(replace_everywhere(model, find, with, slide.as_deref())?);
            }
            Op::MoveShape {
                slide,
                shape,
                x,
                y,
                width,
                height,
            } => {
                shape_of(model, slide, shape)?;
                changes.push(Change::Shape {
                    slide: slide.clone(),
                    shape: shape.clone(),
                    fields: ShapeFields {
                        x: x.map(emu),
                        y: y.map(emu),
                        width: width.map(emu),
                        height: height.map(emu),
                        ..ShapeFields::default()
                    },
                });
            }
            Op::SetFill {
                slide,
                shape,
                color,
            } => {
                shape_of(model, slide, shape)?;
                changes.push(Change::Shape {
                    slide: slide.clone(),
                    shape: shape.clone(),
                    fields: ShapeFields {
                        fill: Some(color.clone()),
                        ..ShapeFields::default()
                    },
                });
            }
            Op::AddTextBox {
                slide,
                text,
                x,
                y,
                width,
                height,
            } => {
                let id = new_shape_id(model, slide, taken)?;
                taken += 1;
                let order = model.slide(slide).map_or(0, |known| known.shapes.len());
                let shape = Shape {
                    name: format!("Text {id}"),
                    id,
                    kind: "shape".to_owned(),
                    placeholder: None,
                    x: emu(*x),
                    y: emu(*y),
                    width: emu(*width),
                    height: emu(*height),
                    rotation: 0,
                    geometry: Some("rect".to_owned()),
                    fill: None,
                    line: None,
                    asset: None,
                    text: Some(plain_tree(text)?),
                    look: Value::Null,
                    order: order as u64,
                };
                changes.push(Change::AddShape {
                    slide: slide.clone(),
                    shape: shape.snapshot(),
                });
            }
            Op::RemoveShape { slide, shape } => {
                shape_of(model, slide, shape)?;
                changes.push(Change::RemoveShape {
                    slide: slide.clone(),
                    shape: shape.clone(),
                });
            }
            Op::AddSlide { after } => {
                let slide = new_slide_id(model, taken);
                taken += 1;
                changes.push(Change::AddSlide {
                    slide,
                    position: next_position(model, after.as_deref())?,
                });
            }
            Op::MoveSlide { slide, after } => {
                changes.push(Change::MoveSlide {
                    slide: slide.clone(),
                    position: match after {
                        Some(after) => next_position(model, Some(after))?,
                        None => first_position(model),
                    },
                });
            }
            Op::RemoveSlide { slide } => {
                changes.push(Change::RemoveSlide {
                    slide: slide.clone(),
                });
            }
        }
    }
    Ok(changes)
}

fn shape_json(shape: &Shape) -> Value {
    json!({
        "id": shape.id,
        "name": shape.name,
        "kind": shape.kind,
        "placeholder": shape.placeholder,
        "x": shape.x / EMU_PER_POINT,
        "y": shape.y / EMU_PER_POINT,
        "width": shape.width / EMU_PER_POINT,
        "height": shape.height / EMU_PER_POINT,
        "text": shape.text.as_ref().map(|tree| {
            tree.content()
                .children()
                .iter()
                .map(Node::text_content)
                .collect::<Vec<_>>()
                .join("\n")
        }),
    })
}

pub(crate) fn answer(model: &Model, query: &Query) -> Value {
    match query {
        Query::Read { slide } => {
            let slides: Vec<Value> = model
                .slides
                .iter()
                .filter(|known| slide.as_deref().is_none_or(|slide| slide == known.id))
                .map(|slide| {
                    let shapes: Vec<Value> = slide.shapes.iter().map(shape_json).collect();
                    json!({ "id": slide.id, "shapes": shapes })
                })
                .collect();
            json!({
                "width": model.width / EMU_PER_POINT,
                "height": model.height / EMU_PER_POINT,
                "slides": slides,
            })
        }
        Query::Check => {
            let mut problems = Vec::new();
            for slide in &model.slides {
                for shape in &slide.shapes {
                    let outside = shape.x + shape.width > model.width
                        || shape.y + shape.height > model.height
                        || shape.x < 0
                        || shape.y < 0;
                    if outside {
                        problems.push(json!({
                            "slide": slide.id,
                            "shape": shape.id,
                            "problem": "The shape leaves the slide.",
                        }));
                    }
                    let empty = shape.text.as_ref().is_some_and(|tree| {
                        tree.text_content().len() < SMALL_TEXT
                            && tree.text_content().trim().is_empty()
                    });
                    if empty && shape.placeholder.is_some() {
                        problems.push(json!({
                            "slide": slide.id,
                            "shape": shape.id,
                            "problem": "The placeholder is empty.",
                        }));
                    }
                }
            }
            json!({ "problems": problems })
        }
    }
}
