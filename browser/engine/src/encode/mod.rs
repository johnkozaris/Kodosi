//! Encodes the pictures of the watched tab as H.264 in Annex B form, for the host's viewers.

#[cfg(target_os = "macos")]
mod mac;
#[cfg(not(target_os = "macos"))]
mod software;

#[cfg(target_os = "macos")]
pub(crate) use mac::Encoder;
#[cfg(not(target_os = "macos"))]
pub(crate) use software::Encoder;

const FRAME_RATE: u32 = 60;
/// After a picture that the encoder or the link dropped, the page draws again this much later,
/// so the viewers get its newest picture even when the page then stays still.
const REDRAW_AFTER_MS: i64 = 100;
