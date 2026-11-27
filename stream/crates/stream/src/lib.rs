//! Live video streams: codecs, encoded frames, the messages between a stream and its viewers, the
//! flow control that decides which viewer takes which frame, and the pace that follows what the
//! viewers do.

#![forbid(unsafe_code)]

mod codec;
mod flow;
mod frame;
pub mod media;
mod pace;
pub mod viewer;

pub use codec::{Codec, Decoder, Format, Viewing, choose};
pub use flow::{Ask, Delivery, Flow, HOPELESS};
pub use frame::{Encoded, FrameKind, Region};
pub use pace::{Pace, Pacer};
pub use viewer::Size;
