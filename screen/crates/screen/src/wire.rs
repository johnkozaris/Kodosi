//! What a viewer and the screen say to each other on the viewer's pipe, after a tag byte as in
//! `stream::viewer`.

use serde::{Deserialize, Serialize};
use stream::{Decoder, Region, Size, viewer::Input};

use crate::link::{Access, Away, Pointer, Shape, Source, Sources};

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
        /// The viewer's keyboard has a Command key, which takes Control's place in the shortcuts
        /// of computers that are not Macs.
        #[serde(default)]
        command_key: bool,
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
    /// The part of `source` that the viewer shows, or `None` for all of it, so that the
    /// capture gives it the most sharpness. A look at another source than the shown one came
    /// before the source changed, and the screen leaves it.
    Look {
        source: Source,
        region: Option<Region>,
    },
    /// The bitrate that the transport carries to the viewer, from the host's runtime while the
    /// viewer's frames go as datagrams.
    Rate {
        bitrate: u64,
    },
    /// A frame that went to the viewer as datagrams and that it cannot have, from the host's
    /// runtime.
    Lost {
        frame: u32,
    },
    /// The viewer's clipboard, which the computer's clipboard takes before the viewer pastes.
    Clipboard {
        text: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) away: Option<Away>,
    /// True for the viewer whose view the capture's size follows.
    pub(crate) control: bool,
    /// Text copied on the computer, sent once to the viewer in control.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) copied: Option<String>,
}

impl State {
    /// Keeps what an unread earlier state carried once, so the viewer still gets it.
    pub(crate) fn after(mut self, earlier: Self) -> Self {
        if self.sources.is_none() {
            self.sources = earlier.sources;
        }
        if self.copied.is_none() {
            self.copied = earlier.copied;
        }
        let mut shapes = earlier.shapes;
        shapes.retain(|shape| self.shapes.iter().all(|newer| newer.id != shape.id));
        shapes.append(&mut self.shapes);
        self.shapes = shapes;
        self
    }
}
