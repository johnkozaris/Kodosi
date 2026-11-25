use bytes::Bytes;

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encoded {
    pub codec: Codec,
    pub kind: FrameKind,
    pub mark: Option<u64>,
    pub data: Bytes,
}
