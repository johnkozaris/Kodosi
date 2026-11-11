//! Nothing here touches a network, a disk, a clock or a thread.

pub mod contract;
mod format;
mod live;

pub use contract::{
    BadFrame, CONTRACT, Catchup, Changed, ClosedReason, Frame, Hash, Peer, RefusedReason, Rev, Seq,
    ToClient, ToDocument, Viewer,
};
pub use format::{
    Answer, Assets, Format, Guide, GuideField, GuideOp, Ordered, Refused, Snapshot, Unreadable,
    Unwritable, guide_field, guide_op,
};
pub use live::{Document, Effect, Input, PipeId, Restored};

#[cfg(test)]
mod tests;
