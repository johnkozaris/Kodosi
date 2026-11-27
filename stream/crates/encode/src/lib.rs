//! Video encoders, chosen at run time from what the computer has: `VideoToolbox` on the Mac,
//! VA-API on Linux graphics that encode H.264, `OpenH264` elsewhere. Each encoder codes new
//! pictures at the stream's pace and sharpens a still picture on its own with passes that encode
//! it again.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub use stream::{Codec, Encoded, Format, FrameKind, Pace, Region};

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

/// The highest pace, which the capture keeps and which decoders are told to expect.
const FRAME_RATE: u32 = Pace::Acting.frame_rate();
/// Delay before the last picture is encoded again after the encoder or its consumer dropped one,
/// so viewers still get the latest state when the source goes idle.
const RETRY: Duration = Duration::from_millis(100);

/// Delay before the next try after the consumer refused `refusals` frames in a row. A consumer
/// that stays full, as on a busy computer, gets fewer of the keyframes it cannot take.
fn retry_after(refusals: u32) -> Duration {
    RETRY * 2_u32.pow(refusals.clamp(1, 4) - 1)
}

/// Whether a new picture may be coded now, `last` being when the last new one was, or else when
/// the one that waits is due. A capture keeps the highest pace, and its pictures come up to half
/// of its frame time early or late: one that little early passes at once, and one that came
/// sooner waits until that little after its frame time, so that a newer picture replaces it
/// first. At a lower pace, every second picture of the capture passes as it comes, never every
/// third.
fn due(pace: Pace, last: Option<Instant>, now: Instant) -> Result<(), Instant> {
    let Some(last) = last else {
        return Ok(());
    };
    let slack = Pace::Acting.frame_time() / 2;
    let frame = last + pace.frame_time();
    if now + slack >= frame {
        Ok(())
    } else {
        Err(frame + slack)
    }
}

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
    pub pace: Pace,
}

pub struct Encoder(Backend);

enum Backend {
    #[cfg(target_os = "macos")]
    VideoToolbox(videotoolbox::Encoder),
    /// VA-API or `OpenH264`, both of which repair only with keyframes. `OpenH264` encodes only
    /// H.264, and its long-term references need camera mode, which halves the quality of
    /// scrolled text.
    #[cfg(not(target_os = "macos"))]
    Pixels(pixels::Encoder),
}

/// The largest picture, long side by short side, that graphics encode in real time.
#[cfg(any(target_os = "macos", target_os = "linux"))]
const LARGEST_ON_GRAPHICS: (u32, u32) = (4096, 2304);
/// The largest picture that the processor encodes in real time without taking over the computer.
#[cfg(not(target_os = "macos"))]
const LARGEST_ON_PROCESSOR: (u32, u32) = (1920, 1080);

impl Encoder {
    /// The largest picture, long side by short side, that this computer encodes in real time.
    pub fn largest() -> (u32, u32) {
        #[cfg(target_os = "macos")]
        return LARGEST_ON_GRAPHICS;
        #[cfg(target_os = "linux")]
        return if vaapi::encodable().is_some() {
            LARGEST_ON_GRAPHICS
        } else {
            LARGEST_ON_PROCESSOR
        };
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        return LARGEST_ON_PROCESSOR;
    }

    /// Codecs this computer encodes in real time.
    pub fn encodable() -> Vec<Codec> {
        #[cfg(target_os = "macos")]
        return videotoolbox::encodable();
        #[cfg(target_os = "linux")]
        return vaapi::encodable().unwrap_or_else(|| vec![Codec::H264]);
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
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
            if vaapi::work(settings, output.as_ref(), shared) {
                return;
            }
            openh264::work(settings.bitrate, settings.pace, output.as_ref(), shared);
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

    pub fn set_pace(&mut self, pace: Pace) {
        match &mut self.0 {
            #[cfg(target_os = "macos")]
            Backend::VideoToolbox(encoder) => encoder.set_pace(pace),
            #[cfg(not(target_os = "macos"))]
            Backend::Pixels(encoder) => encoder.set_pace(pace),
        }
    }

    /// Restarts the encoder with the new format; the next frame is a keyframe.
    pub fn set_format(&mut self, format: Format) {
        match &mut self.0 {
            #[cfg(target_os = "macos")]
            Backend::VideoToolbox(encoder) => encoder.set_format(format),
            #[cfg(not(target_os = "macos"))]
            Backend::Pixels(encoder) => encoder.set_format(format),
        }
    }
}
