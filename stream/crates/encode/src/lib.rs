//! Video encoders, chosen at run time from what the computer has: `VideoToolbox` on the Mac,
//! VA-API on Linux graphics that encode H.264, `OpenH264` elsewhere. Each encoder sharpens a
//! still picture on its own with passes that encode it again.

use std::{sync::Arc, time::Duration};

pub use stream::{Codec, Encoded, Format, FrameKind, Region};

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod nal;
#[cfg(not(target_os = "macos"))]
mod openh264;
#[cfg(not(target_os = "macos"))]
mod pixels;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod rate;
#[cfg(not(target_os = "macos"))]
mod refine;
#[cfg(target_os = "linux")]
mod vaapi;
#[cfg(target_os = "macos")]
mod videotoolbox;

const FRAME_RATE: u32 = 60;
/// Delay before the last picture is encoded again after the encoder or its consumer dropped one,
/// so viewers still get the latest state when the source goes idle.
const RETRY: Duration = Duration::from_millis(100);

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
    /// A `CVPixelBufferRef` in BT.709 video-range NV12 (`420v`), which the encoder takes as it is
    /// and holds until it has encoded it.
    #[cfg(target_os = "macos")]
    Nv12(*mut std::ffi::c_void),
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
    /// VA-API or `OpenH264`, both of which encode only H.264 and repair only with keyframes.
    /// `OpenH264`'s long-term references need camera mode, which halves the quality of scrolled
    /// text.
    #[cfg(not(target_os = "macos"))]
    Pixels(pixels::Encoder),
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
        return Self(Backend::Pixels(pixels::Encoder::start(move |shared| {
            #[cfg(target_os = "linux")]
            if vaapi::work(settings.bitrate, output.as_ref(), shared) {
                return;
            }
            openh264::work(settings.bitrate, output.as_ref(), shared);
        })));
    }

    /// An encoder that makes each picture a sharp keyframe, for stills that stand alone.
    #[cfg(target_os = "macos")]
    pub fn stills(format: Format, output: Arc<dyn Output>) -> Self {
        Self(Backend::VideoToolbox(videotoolbox::Encoder::stills(
            format, output,
        )))
    }

    /// Encodes a picture of `region` of the source, or of all of it, of which `changed` is the
    /// share that differs from the last picture.
    pub fn encode(&mut self, picture: Picture<'_>, region: Option<Region>, changed: f64) {
        match &mut self.0 {
            #[cfg(target_os = "macos")]
            Backend::VideoToolbox(encoder) => encoder.encode(picture, region, changed),
            #[cfg(not(target_os = "macos"))]
            Backend::Pixels(encoder) => encoder.encode(picture, region, changed),
        }
    }

    pub fn want_key(&mut self) {
        match &mut self.0 {
            #[cfg(target_os = "macos")]
            Backend::VideoToolbox(encoder) => encoder.want_key(),
            #[cfg(not(target_os = "macos"))]
            Backend::Pixels(encoder) => encoder.want_key(),
        }
    }

    /// Asks for an anchor that references only marks every viewer holds, or a keyframe when
    /// the stream has no anchors.
    pub fn recover(&mut self) {
        match &mut self.0 {
            #[cfg(target_os = "macos")]
            Backend::VideoToolbox(encoder) => encoder.recover(),
            #[cfg(not(target_os = "macos"))]
            Backend::Pixels(encoder) => encoder.want_key(),
        }
    }

    /// Marks every viewer acknowledged, which the next anchor may reference.
    pub fn held(&mut self, marks: &[u64]) {
        match &mut self.0 {
            #[cfg(target_os = "macos")]
            Backend::VideoToolbox(encoder) => encoder.held(marks),
            #[cfg(not(target_os = "macos"))]
            Backend::Pixels(_) => {
                let _ = marks;
            }
        }
    }

    pub fn set_bitrate(&mut self, bitrate: u32) {
        match &mut self.0 {
            #[cfg(target_os = "macos")]
            Backend::VideoToolbox(encoder) => encoder.set_bitrate(bitrate),
            #[cfg(not(target_os = "macos"))]
            Backend::Pixels(encoder) => encoder.set_bitrate(bitrate),
        }
    }

    /// Restarts the encoder with the new format; the next frame is a keyframe.
    pub fn set_format(&mut self, format: Format) {
        match &mut self.0 {
            #[cfg(target_os = "macos")]
            Backend::VideoToolbox(encoder) => encoder.set_format(format),
            #[cfg(not(target_os = "macos"))]
            Backend::Pixels(encoder) => {
                let _ = format;
                encoder.want_key();
            }
        }
    }
}
