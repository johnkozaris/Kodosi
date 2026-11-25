//! What a viewer and the screen say to each other on the viewer's pipe, after a tag byte as in
//! `stream::viewer`.

use serde::{Deserialize, Serialize};
use stream::{Decoder, Region, Size, viewer::Input};

use crate::link::{Access, Pointer, Shape, Source, Sources};

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum ToScreen {
    /// `opened` is set when the person opened the screen on this device, which then takes
    /// control of the capture's size.
    Watch {
        size: Size,
        decoders: Vec<Decoder>,
        #[serde(default)]
        anchors: bool,
        #[serde(default)]
        opened: bool,
    },
    Ack {
        frame: u32,
    },
    Keyframe,
    Size {
        size: Size,
    },
    /// Pointer coordinates are in the shown source's points.
    Input {
        input: Input,
    },
    Show {
        source: Source,
    },
    /// The part of the source that the viewer shows, or `None` for all of it, so that the
    /// capture gives it the most sharpness.
    Look {
        region: Option<Region>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct State {
    /// Sent only when they changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) sources: Option<Sources>,
    pub(crate) source: Option<Source>,
    /// The shown source's size in points.
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) pointer: Option<Pointer>,
    /// Pointer images the viewer has not had yet.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) shapes: Vec<Shape>,
    pub(crate) access: Access,
    /// True for the viewer whose view the capture's size follows.
    pub(crate) control: bool,
}

impl State {
    /// Keeps what an unread earlier state carried once, so the viewer still gets it.
    pub(crate) fn after(mut self, earlier: Self) -> Self {
        if self.sources.is_none() {
            self.sources = earlier.sources;
        }
        let mut shapes = earlier.shapes;
        shapes.retain(|shape| self.shapes.iter().all(|newer| newer.id != shape.id));
        shapes.append(&mut self.shapes);
        self.shapes = shapes;
        self
    }
}
