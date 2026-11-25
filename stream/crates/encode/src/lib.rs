//! Video encoders, chosen at run time from what the computer has: `VideoToolbox` on the Mac,
//! `OpenH264` elsewhere.

use std::sync::Arc;

pub use stream::{Codec, Encoded, Format, FrameKind};

#[cfg(not(target_os = "macos"))]
mod openh264;
pub mod refine;
#[cfg(target_os = "macos")]
mod videotoolbox;

/// Where an encoder sends its frames.
pub trait Output: Send + Sync + 'static {
    /// Returns false when the frame could not be queued, so the encoder sends a keyframe next.
    fn frame(&self, frame: Encoded) -> bool;
    fn failed(&self, problem: String);
}

#[derive(Clone, Copy)]
pub enum Picture<'a> {
    /// BGRA rows without padding.
    Pixels {
        bgra: &'a [u8],
        width: usize,
        height: usize,
    },
    /// A BGRA `IOSurfaceRef` that stays valid only during the call.
    #[cfg(target_os = "macos")]
    Surface(*mut std::ffi::c_void),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub format: Format,
    pub bitrate: u32,
}

pub struct Encoder(Backend);

enum Backend {
    #[cfg(target_os = "macos")]
    VideoToolbox(videotoolbox::Encoder),
    #[cfg(not(target_os = "macos"))]
    OpenH264(openh264::Encoder),
}

impl Encoder {
    /// Codecs this computer encodes in real time.
    pub fn encodable() -> Vec<Codec> {
        #[cfg(target_os = "macos")]
        return videotoolbox::encodable();
        #[cfg(not(target_os = "macos"))]
        return vec![Codec::H264];
    }

    pub fn new(settings: Settings, output: Arc<dyn Output>) -> Self {
        #[cfg(target_os = "macos")]
        return Self(Backend::VideoToolbox(videotoolbox::Encoder::new(
            settings, output,
        )));
        #[cfg(not(target_os = "macos"))]
        return Self(Backend::OpenH264(openh264::Encoder::new(settings, output)));
    }

    pub fn encode(&mut self, picture: Picture<'_>) {
        match &mut self.0 {
            #[cfg(target_os = "macos")]
            Backend::VideoToolbox(encoder) => encoder.encode(picture),
            #[cfg(not(target_os = "macos"))]
            Backend::OpenH264(encoder) => encoder.encode(picture),
        }
    }

    pub fn want_key(&mut self) {
        match &mut self.0 {
            #[cfg(target_os = "macos")]
            Backend::VideoToolbox(encoder) => encoder.want_key(),
            #[cfg(not(target_os = "macos"))]
            Backend::OpenH264(encoder) => encoder.want_key(),
        }
    }

    /// Asks for an anchor that references only marks every viewer holds, or a keyframe when
    /// the stream has no anchors.
    pub fn recover(&mut self) {
        match &mut self.0 {
            #[cfg(target_os = "macos")]
            Backend::VideoToolbox(encoder) => encoder.recover(),
            #[cfg(not(target_os = "macos"))]
            Backend::OpenH264(encoder) => encoder.want_key(),
        }
    }

    /// Marks every viewer acknowledged, which the next anchor may reference.
    pub fn held(&mut self, marks: &[u64]) {
        match &mut self.0 {
            #[cfg(target_os = "macos")]
            Backend::VideoToolbox(encoder) => encoder.held(marks),
            #[cfg(not(target_os = "macos"))]
            Backend::OpenH264(_) => {
                let _ = marks;
            }
        }
    }

    pub fn set_bitrate(&mut self, bitrate: u32) {
        match &mut self.0 {
            #[cfg(target_os = "macos")]
            Backend::VideoToolbox(encoder) => encoder.set_bitrate(bitrate),
            #[cfg(not(target_os = "macos"))]
            Backend::OpenH264(encoder) => encoder.set_bitrate(bitrate),
        }
    }

    /// Restarts the encoder with the new format; the next frame is a keyframe.
    pub fn set_format(&mut self, format: Format) {
        match &mut self.0 {
            #[cfg(target_os = "macos")]
            Backend::VideoToolbox(encoder) => encoder.set_format(format),
            #[cfg(not(target_os = "macos"))]
            Backend::OpenH264(encoder) => encoder.set_format(format),
        }
    }

    /// Encodes the last picture again so rate control can sharpen it. Returns true when the pass
    /// counts: it is queued, or the picture is already as sharp as the bitrate allows.
    pub fn refine(&mut self) -> bool {
        match &mut self.0 {
            #[cfg(target_os = "macos")]
            Backend::VideoToolbox(encoder) => encoder.refine(),
            #[cfg(not(target_os = "macos"))]
            Backend::OpenH264(encoder) => encoder.refine(),
        }
    }
}
