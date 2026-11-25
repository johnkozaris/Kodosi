//! Live video streams: codecs, encoded frames, the messages between a stream and its viewers, and
//! the flow control that decides which viewer takes which frame.

#![forbid(unsafe_code)]

mod codec;
mod flow;
mod frame;
pub mod viewer;

pub use codec::{Codec, Decoder, Format, Viewing, choose};
pub use flow::{Ask, Delivery, Flow, HOPELESS};
pub use frame::{Encoded, FrameKind, Region};
pub use viewer::Size;
