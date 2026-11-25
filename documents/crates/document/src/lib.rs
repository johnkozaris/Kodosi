//! Sans-I/O live document state machine: no networking, disk I/O, clocks, or threads.

pub mod contract;
mod format;
mod live;

pub use contract::{
    BadFrame, CONTRACT, Catchup, Changed, ClosedReason, Frame, Hash, Peer, RefusedReason, Rev, Seq,
    ToClient, ToDocument, Viewer, hash_from_hex, hex,
};
pub use format::{
    Answer, Assets, Format, Guide, GuideField, GuideOp, Ordered, Refused, Snapshot, Unreadable,
    Unwritable, guide_field, guide_op, short_hash,
};
pub use live::{Document, Effect, Input, PipeId, Restored};

#[cfg(test)]
mod tests;
