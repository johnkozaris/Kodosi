use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::Codec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    Delta,
    Key,
    /// References only frames whose marks every viewer acknowledged, so a viewer that missed
    /// frames recovers without a keyframe.
    Anchor,
}

/// An Annex B picture. `mark` names a long-term reference that later anchors may use once every
/// viewer acknowledged it.
#[derive(Debug, Clone, PartialEq)]
pub struct Encoded {
    pub codec: Codec,
    pub kind: FrameKind,
    pub mark: Option<u64>,
    /// The part of the source that the picture shows, when it shows only a part.
    pub region: Option<Region>,
    pub data: Bytes,
}

/// A rectangle of a stream's source, in its points.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Region {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
