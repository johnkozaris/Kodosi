use std::{collections::HashMap, ops::RangeInclusive};

use roxmltree::Document;
use serde_json::Value;
use text::Node;

use crate::{
    model::{Model, Shape, ShapeFields, Slide, schema},
    read::names_of,
};

// DrawingML schema bounds: ST_CoordinateUnqualified, ST_PositiveCoordinate, and ST_Angle.
const PLACES: RangeInclusive<i64> = -27_273_042_329_600..=27_273_042_316_900;
const SIZES: RangeInclusive<i64> = 0..=27_273_042_316_900;
const TURNS: RangeInclusive<i64> = -2_147_483_648..=2_147_483_647;

pub(crate) fn writable_fill(color: &str) -> Result<(), String> {
    let hex = color.len() == 6 && color.bytes().all(|byte| byte.is_ascii_hexdigit());
    let scheme = color.strip_prefix("scheme:").is_some_and(|name| {
        !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_alphanumeric())
    });
    if hex || scheme {
        return Ok(());
    }
    Err(format!(
        "A fill is six hex digits, such as FF0000, or scheme: and a color of the theme, and {color} is not."
    ))
}

/// Rejects field edits the writer cannot serialize (clearing fills, or changing lines or stacking
/// order).
pub(crate) fn sendable_fields(fields: &ShapeFields) -> Result<(), String> {
    if fields.line.is_some() || fields.order.is_some() {
        return Err("The file takes no change of a line or of the stacking order.".to_owned());
    }
    match &fields.fill {
        Some(None) => Err("The file takes no fill away.".to_owned()),
        Some(Some(color)) => writable_fill(color),
        None => Ok(()),
    }
}

/// Rejects changes that would leave the presentation unserializable.
pub(crate) fn writable_change(before: &Model, after: &Model) -> Result<(), String> {
    for slide in &after.slides {
        let old = before.slide(&slide.id);
        for shape in &slide.shapes {
            let kept = old.and_then(|old| old.shapes.iter().find(|known| known.id == shape.id));
            if kept != Some(shape) {
                writable_frame(kept, shape)?;
                writable_shape(slide, shape, &after.raws)?;
            }
        }
    }
    Ok(())
}

fn writable_shape(
    slide: &Slide,
    shape: &Shape,
    raws: &HashMap<String, String>,
) -> Result<(), String> {
    if shape.kind == "graphic" && shape.fill.is_some() {
        return Err(format!(
            "The shape {} is a table, a chart or another graphic, and the file holds no fill for it.",
            shape.id
        ));
    }
    if let Some(source) = &shape.source {
        piece(raws, "shape", Some(source))?;
        if !slide.sources.contains_key(source) {
            return Err(format!(
                "The piece {source} of the file is a shape of another slide than {}.",
                slide.id
            ));
        }
        if raws.get(source).and_then(|piece| own_id(piece)).as_ref() != Some(&shape.id) {
            return Err(format!(
                "The piece {source} of the file is another shape than {}.",
                shape.id
            ));
        }
    } else if !is_box(shape) {
        return Err(format!(
            "The shape {} is not one that the file held, and a new shape can only be a box with text, without a picture, a placeholder, a link or a line.",
            shape.id
        ));
    }
    shape
        .text
        .as_ref()
        .map_or(Ok(()), |tree| writable_text(tree, raws))
}

/// True if the writer can synthesize this shape from scratch without an original XML fragment.
pub(crate) fn is_box(shape: &Shape) -> bool {
    shape.kind == "shape"
        && shape
            .geometry
            .as_deref()
            .is_none_or(|geometry| geometry == "rect")
        && shape.placeholder.is_none()
        && shape.asset.is_none()
        && shape.link.is_none()
        && shape.line.is_none()
}

/// Existing out-of-range coordinates from the source file are preserved; only values modified by
/// a change must stay within `DrawingML` bounds.
fn writable_frame(old: Option<&Shape>, shape: &Shape) -> Result<(), String> {
    let fits = |value: fn(&Shape) -> i64, range: RangeInclusive<i64>| {
        old.is_some_and(|old| value(old) == value(shape)) || range.contains(&value(shape))
    };
    if fits(|shape| shape.x, PLACES)
        && fits(|shape| shape.y, PLACES)
        && fits(|shape| shape.width, SIZES)
        && fits(|shape| shape.height, SIZES)
        && fits(|shape| shape.rotation, TURNS)
    {
        return Ok(());
    }
    Err(format!(
        "The shape {} takes a place, a size or a turn that the file cannot hold, such as a size below zero.",
        shape.id
    ))
}

fn own_id(piece: &str) -> Option<String> {
    let parsed = Document::parse(piece).ok()?;
    let id = names_of(parsed.root_element())?.attribute("id")?;
    Some(id.to_owned())
}

fn writable_text(node: &Node, raws: &HashMap<String, String>) -> Result<(), String> {
    let attrs = node.attrs();
    let name = schema().node_name(node.kind());
    match name {
        "paragraph" => {
            properties(raws, name, attrs.get("props"))?;
            properties(raws, name, attrs.get("end"))?;
        }
        "inlineOpaque" => piece(raws, name, attrs.get("raw").and_then(Value::as_str))?,
        _ => {}
    }
    for mark in node.marks() {
        if schema().mark_name(mark.kind()) == "run" {
            properties(raws, "run", mark.attrs().get("props"))?;
        }
    }
    for child in node.content().children() {
        writable_text(child, raws)?;
    }
    Ok(())
}

fn piece(raws: &HashMap<String, String>, owner: &str, key: Option<&str>) -> Result<(), String> {
    match key {
        Some(key) if raws.contains_key(key) => Ok(()),
        Some(key) => Err(format!(
            "The {owner} names the piece {key} of the file, which the presentation does not hold."
        )),
        None => Err(format!(
            "The {owner} needs the piece of the file that it stands for."
        )),
    }
}

fn properties(
    raws: &HashMap<String, String>,
    owner: &str,
    key: Option<&Value>,
) -> Result<(), String> {
    match key {
        None | Some(Value::Null) => Ok(()),
        Some(key) => piece(raws, owner, key.as_str()),
    }
}
