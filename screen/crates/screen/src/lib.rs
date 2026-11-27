//! A computer's screens as video streams for its owner's other devices: the host side that the
//! runtime uses, and the messages to the "Kodosi Screen" helper that captures them.

#[cfg(target_os = "macos")]
pub mod handover;
pub mod link;
#[cfg(target_os = "macos")]
pub mod signature;

#[cfg(feature = "host")]
mod host;
#[cfg(feature = "host")]
mod live;
#[cfg(feature = "host")]
mod wire;

#[cfg(feature = "host")]
pub use host::{Error, Pipe, Screens, Settings};
pub use link::{Notice, Question};
#[cfg(feature = "host")]
pub use live::Wanted;
pub use stream::viewer::Viewer;

/// Whether the computer's desktop runs on X11, where Kodosi Screen captures it through the X
/// server. Elsewhere on Linux, the desktop's portal shares it, as on Wayland.
#[cfg(target_os = "linux")]
pub fn on_x11() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_none() && std::env::var_os("DISPLAY").is_some()
}
