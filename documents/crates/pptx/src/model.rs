use std::{
    collections::{BTreeMap, HashMap},
    ops::RangeInclusive,
    sync::LazyLock,
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use text::{Node, Schema, SchemaSpec};

use crate::{Op, read::Frame};

static SCHEMA: LazyLock<Schema> = LazyLock::new(|| {
    let spec: SchemaSpec = serde_json::from_str(include_str!("../schema.json"))
        .unwrap_or_else(|error| unreachable!("the schema is part of this crate: {error}"));
    Schema::new(&spec)
        .unwrap_or_else(|error| unreachable!("the schema is part of this crate: {error}"))
});

pub(crate) fn schema() -> &'static Schema {
    &SCHEMA
}

/// Valid OOXML `ST_SlideId` range.
pub(crate) const SLIDE_IDS: RangeInclusive<u64> = 256..=2_147_483_647;
/// Valid OOXML `ST_DrawingElementId` range.
pub(crate) const SHAPE_IDS: RangeInclusive<u64> = 0..=4_294_967_295;
/// Shape ID reserved for the root `p:spTree` group on newly created slides.
pub(crate) const ROOT_SHAPE: u64 = 1;

pub(crate) fn number_in(id: &str, range: &RangeInclusive<u64>) -> Option<u64> {
    if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    id.parse().ok().filter(|number| range.contains(number))
}

pub struct Pptx;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shape {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub placeholder: Option<String>,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    pub rotation: i64,
    pub geometry: Option<String>,
    pub fill: Option<String>,
    pub line: Option<String>,
    pub asset: Option<String>,
    pub link: Option<Value>,
    pub text: Option<Node>,
    /// Inherited per-level text styles resolved from master, layout, and shape.
    pub look: Value,
    /// Stacking order on the slide (higher renders in front).
    pub order: u64,
    /// Hash key of the original shape XML in `Model::raws` (allows undo to restore the full XML);
    /// `None` for newly created shapes.
    pub source: Option<String>,
}

/// Original frame and fill read from a shape's XML fragment, used to patch only subsequent edits
/// if the shape is restored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sourced {
    pub(crate) frame: Frame,
    pub(crate) fill: Option<String>,
}

impl Sourced {
    pub(crate) fn of(shape: &Shape) -> Self {
        Self {
            frame: Frame {
                x: shape.x,
                y: shape.y,
                width: shape.width,
                height: shape.height,
                rotation: shape.rotation,
            },
            fill: shape.fill.clone(),
        }
    }

    pub(crate) fn by_piece(shapes: &[Shape]) -> HashMap<String, Self> {
        shapes
            .iter()
            .filter_map(|shape| Some((shape.source.clone()?, Self::of(shape))))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Slide {
    pub id: String,
    pub part: Option<String>,
    /// Fractional sort key for slide ordering so moves update a single number.
    pub position: f64,
    pub shapes: Vec<Shape>,
    /// Read-only background shapes inherited from the slide master and layout.
    pub behind: Vec<Shape>,
    pub background: Option<String>,
    /// Default text style from the slide master, applied to newly added text shapes.
    pub text_look: Value,
    /// Highest shape ID seen on this slide (including inside groups) to avoid collisions when
    /// allocating new IDs.
    pub last_shape: u64,
    /// Original shape XML metadata scoped to this slide; shape XML references per-slide
    /// relationships and cannot be restored onto a different slide.
    pub sources: HashMap<String, Sourced>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Model {
    pub(crate) width: i64,
    pub(crate) height: i64,
    pub(crate) slides: Vec<Slide>,
    pub(crate) raws: HashMap<String, String>,
    pub(crate) theme: Value,
}

/// Partial shape update: omitted fields stay unchanged; explicit `null` clears `fill` or `line`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShapeFields {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<i64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "named"
    )]
    pub fill: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "named"
    )]
    pub line: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<u64>,
}

/// Deserializes a present field as `Some(T)` so explicit `null` becomes `Some(None)` rather than
/// `None` (absent).
fn named<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// Shape field and order edits are key-addressed (last write wins); `Text` steps are
/// position-addressed within a single shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Change {
    Shape {
        slide: String,
        shape: String,
        fields: ShapeFields,
    },
    Text {
        slide: String,
        shape: String,
        steps: Vec<Value>,
        /// Raw XML fragments from an external-write merge, carried so journal replay has them.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        keep: BTreeMap<String, String>,
    },
    MoveSlide {
        slide: String,
        position: f64,
    },
    AddSlide {
        slide: String,
        position: f64,
    },
    RemoveSlide {
        slide: String,
    },
    AddShape {
        slide: String,
        shape: Value,
        /// Raw XML fragments from an external-write merge, carried so journal replay has them.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        keep: BTreeMap<String, String>,
    },
    RemoveShape {
        slide: String,
        shape: String,
    },
    Ops {
        ops: Vec<Op>,
    },
    Group {
        changes: Vec<Self>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Query {
    Read {
        #[serde(default)]
        slide: Option<String>,
    },
    Check,
}

impl Shape {
    pub(crate) fn snapshot(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "kind": self.kind,
            "placeholder": self.placeholder,
            "x": self.x,
            "y": self.y,
            "width": self.width,
            "height": self.height,
            "rotation": self.rotation,
            "geometry": self.geometry,
            "fill": self.fill,
            "line": self.line,
            "asset": self.asset,
            "link": self.link,
            "text": self.text.as_ref().map(|text| text.to_json(schema())),
            "look": self.look,
            "order": self.order,
            "source": self.source,
        })
    }

    pub(crate) fn from_snapshot(json: &Value) -> Result<Self, String> {
        let number = |name: &str| json[name].as_i64().unwrap_or(0);
        let text = |name: &str| json[name].as_str().map(str::to_owned);
        let tree = match json.get("text").filter(|text| !text.is_null()) {
            Some(tree) => Some(Node::from_json(schema(), tree).map_err(|failed| failed.0)?),
            None => None,
        };
        Ok(Self {
            id: text("id").ok_or("a shape needs an id")?,
            name: text("name").unwrap_or_default(),
            kind: text("kind").unwrap_or_else(|| "shape".to_owned()),
            placeholder: text("placeholder"),
            x: number("x"),
            y: number("y"),
            width: number("width"),
            height: number("height"),
            rotation: number("rotation"),
            geometry: text("geometry"),
            fill: text("fill"),
            line: text("line"),
            asset: text("asset"),
            link: json.get("link").filter(|link| !link.is_null()).cloned(),
            text: tree,
            look: json.get("look").cloned().unwrap_or_default(),
            order: json["order"].as_u64().unwrap_or(0),
            source: text("source"),
        })
    }
}

impl Model {
    pub(crate) fn slide(&self, id: &str) -> Option<&Slide> {
        self.slides.iter().find(|slide| slide.id == id)
    }

    pub(crate) fn slide_mut(&mut self, id: &str) -> Option<&mut Slide> {
        self.slides.iter_mut().find(|slide| slide.id == id)
    }

    pub(crate) fn sort_slides(&mut self) {
        self.slides
            .sort_by(|left, right| left.position.total_cmp(&right.position));
    }

    pub(crate) fn snapshot(&self) -> Value {
        let slides: Vec<Value> = self
            .slides
            .iter()
            .map(|slide| {
                let shapes: Vec<Value> = slide.shapes.iter().map(Shape::snapshot).collect();
                let behind: Vec<Value> = slide.behind.iter().map(Shape::snapshot).collect();
                json!({
                    "id": slide.id,
                    "position": slide.position,
                    "shapes": shapes,
                    "behind": behind,
                    "background": slide.background,
                })
            })
            .collect();
        json!({
            "width": self.width,
            "height": self.height,
            "slides": slides,
            "theme": self.theme,
        })
    }
}
