use std::{collections::HashMap, sync::LazyLock};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use text::{Node, Schema, SchemaSpec};

use crate::Op;

static SCHEMA: LazyLock<Schema> = LazyLock::new(|| {
    let spec: SchemaSpec = serde_json::from_str(include_str!("../schema.json"))
        .unwrap_or_else(|error| unreachable!("the schema is part of this crate: {error}"));
    Schema::new(&spec)
        .unwrap_or_else(|error| unreachable!("the schema is part of this crate: {error}"))
});

pub(crate) fn schema() -> &'static Schema {
    &SCHEMA
}

/// The `PowerPoint` format.
pub struct PowerPoint;

/// One shape on a slide.
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
    pub text: Option<Node>,
    /// The look that its text takes from the master, the layout and the shape, by level.
    pub look: Value,
    /// Where it stands among the shapes: higher is in front.
    pub order: u64,
}

/// One slide.
#[derive(Debug, Clone, PartialEq)]
pub struct Slide {
    pub id: String,
    pub part: Option<String>,
    /// Where it stands among the slides, so a move names one number.
    pub position: f64,
    pub shapes: Vec<Shape>,
    /// What its layout and master draw behind it, which is not edited here.
    pub behind: Vec<Shape>,
    pub background: Option<String>,
}

/// The live copy of a `PowerPoint` file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Model {
    pub(crate) width: i64,
    pub(crate) height: i64,
    pub(crate) slides: Vec<Slide>,
    pub(crate) raws: HashMap<String, String>,
    pub(crate) theme: Value,
}

/// The fields of a shape that a key names. What a change does not name stays.
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<u64>,
}

/// One change. Fields and orders name a key, so the last one wins. Text names positions in one
/// shape's tree.
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

/// What read and check ask.
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
            "text": self.text.as_ref().map(|text| text.to_json(schema())),
            "look": self.look,
            "order": self.order,
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
            text: tree,
            look: json.get("look").cloned().unwrap_or(Value::Null),
            order: json["order"].as_u64().unwrap_or(0),
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
