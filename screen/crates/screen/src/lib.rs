//! A computer's screens as video streams for its owner's other devices: the host side that the
//! runtime uses, and the messages to the "Kodosi Screen" helper that captures them.

pub mod link;

#[cfg(feature = "host")]
mod host;
#[cfg(feature = "host")]
mod live;
#[cfg(feature = "host")]
mod wire;

#[cfg(feature = "host")]
pub use host::{Error, Pipe, Screens, Settings};
pub use link::Notice;
#[cfg(feature = "host")]
pub use live::Wanted;
pub use stream::viewer::Viewer;
