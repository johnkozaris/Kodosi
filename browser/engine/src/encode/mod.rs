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
