//! The rules of a live document: its order, its log, who is here and the merge of outside
//! writes. Nothing here touches a network, a disk, a clock or a thread.

pub mod contract;
mod format;
mod live;

pub use contract::{
    BadFrame, CONTRACT, Catchup, Changed, ClosedReason, Frame, Hash, Peer, RefusedReason, Rev, Seq,
    ToClient, ToDocument, Viewer,
};
pub use format::{
    Answer, Assets, Format, Guide, GuideField, GuideOp, Ordered, Refused, Snapshot, Unreadable,
    Unwritable,
};
pub use live::{Document, Effect, Input, PipeId, Restored};

#[cfg(test)]
mod tests;
