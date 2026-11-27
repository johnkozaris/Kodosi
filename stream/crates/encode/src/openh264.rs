//! `OpenH264` in software, for computers whose GPU does not encode. Its worker sharpens a
//! picture that stays with passes that encode it again. Its own rate control gives each frame
//! the bitrate's share for the frame rate that it is told, so it is told the stream's pace.

use std::{mem::MaybeUninit, os::raw::c_int, time::Instant};

use bytes::Bytes;
use openh264::{
    OpenH264API,
    encoder::{
        BitRate, Encoder as H264, EncoderConfig, FrameRate, FrameType, QpRange, RateControlMode,
        UsageType,
    },
    formats::YUVSlices,
};
use openh264_sys2::{
    ENCODER_OPTION, ENCODER_OPTION_BITRATE, ENCODER_OPTION_FRAME_RATE, ENCODER_OPTION_MAX_BITRATE,
    ENCODER_OPTION_SVC_ENCODE_PARAM_EXT, LAYER_NUM, SBitrateInfo, SEncParamExt, SPATIAL_LAYER_0,
    SPATIAL_LAYER_ALL, cmResultSuccess,
};
use stream::{Codec, Encoded, FrameKind, Region};

use crate::{
    FRAME_RATE, Output, Pace, RETRY, due,
    pixels::{self, Pixels, Shared},
    refine::{Cleanup, Convergence, PASS_EVERY, STILL, frame_budget},
    retry_after,
};

/// H.264 colour description codes for BT.709, and "unspecified" for the video format.
const BT709: u8 = 1;
const UNSPECIFIED_FORMAT: u8 = 5;
const HALF: i32 = 1 << 15;

/// Planes of a BT.709 limited-range I420 picture.
struct Planes {
    luma: Vec<u8>,
    blue: Vec<u8>,
    red: Vec<u8>,
    width: usize,
    height: usize,
}

impl Planes {
    fn new(width: usize, height: usize) -> Self {
        let chroma = (width / 2) * (height / 2);
        Self {
            luma: vec![16; width * height],
            blue: vec![128; chroma],
            red: vec![128; chroma],
            width,
            height,
        }
    }

    /// BT.709 limited range in 16-bit fixed point: `Y` in 16..=235, `Cb` and `Cr` in 16..=240,
    /// chroma averaged over each 2x2 block.
    fn read(&mut self, bgra: &[u8]) {
        let width = self.width;
        for row in (0..self.height).step_by(2) {
            for column in (0..width).step_by(2) {
                let (mut red, mut green, mut blue) = (0_i32, 0_i32, 0_i32);
                for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                    let at = (row + dy) * width + column + dx;
                    let pixel = &bgra[at * 4..at * 4 + 3];
                    let (b, g, r) = (
                        i32::from(pixel[0]),
                        i32::from(pixel[1]),
                        i32::from(pixel[2]),
                    );
                    self.luma[at] = clamp(16 + ((11966 * r + 40254 * g + 4064 * b + HALF) >> 16));
                    red += r;
                    green += g;
                    blue += b;
                }
                let (r, g, b) = ((red + 2) >> 2, (green + 2) >> 2, (blue + 2) >> 2);
                let at = (row / 2) * (width / 2) + column / 2;
                self.blue[at] = clamp(128 + ((-6596 * r - 22189 * g + 28785 * b + HALF) >> 16));
                self.red[at] = clamp(128 + ((28785 * r - 26145 * g - 2639 * b + HALF) >> 16));
            }
        }
    }

    fn slices(&self) -> YUVSlices<'_> {
        YUVSlices::new(
            (&self.luma, &self.blue, &self.red),
            (self.width, self.height),
            (self.width, self.width / 2, self.width / 2),
        )
    }
}

fn clamp(value: i32) -> u8 {
    u8::try_from(value.clamp(0, 255)).unwrap_or_default()
}

struct Running {
    encoder: H264,
    planes: Planes,
    /// The pace whose frame rate the encoder was told.
    pace: Pace,
}

fn hertz(frame_rate: u32) -> f32 {
    f32::from(u16::try_from(frame_rate).unwrap_or(u16::MAX))
}

/// Starts at the highest frame rate, which sets the stream's level, and is told the pace later.
fn start(width: usize, height: usize, bitrate: u32) -> Option<Running> {
    let config = EncoderConfig::new()
        .bitrate(BitRate::from_bps(bitrate))
        .max_frame_rate(FrameRate::from_hz(hertz(FRAME_RATE)))
        .usage_type(UsageType::ScreenContentRealTime)
        .rate_control_mode(RateControlMode::Bitrate)
        // In screen-content mode, OpenH264 overrides min QP 0 to 26..35 (capping static text
        // around 43 dB). 12..40 lets refinements sharpen further while bounding keyframe QP.
        .qp(QpRange::new(12, 40))
        .skip_frames(true);
    let mut encoder = H264::with_api_config(OpenH264API::from_source(), config).ok()?;
    describe_colours(&mut encoder);
    Some(Running {
        encoder,
        planes: Planes::new(width, height),
        pace: Pace::Acting,
    })
}

/// Writes BT.709 limited range into the stream, so decoders convert the colours back the way
/// `Planes::read` made them.
fn describe_colours(encoder: &mut H264) -> bool {
    // SAFETY: `SEncParamExt` is a plain C struct for which all zeroes is a valid value.
    let mut params: SEncParamExt = unsafe { MaybeUninit::zeroed().assume_init() };
    // SAFETY: both options take an `SEncParamExt`, which lives for the calls.
    unsafe {
        let api = encoder.raw_api();
        if api.get_option(
            ENCODER_OPTION_SVC_ENCODE_PARAM_EXT,
            (&raw mut params).cast(),
        ) != cmResultSuccess
        {
            return false;
        }
        let layer = &mut params.sSpatialLayers[0];
        layer.bVideoSignalTypePresent = true;
        layer.uiVideoFormat = UNSPECIFIED_FORMAT;
        layer.bFullRange = false;
        layer.bColorDescriptionPresent = true;
        layer.uiColorPrimaries = BT709;
        layer.uiTransferCharacteristics = BT709;
        layer.uiColorMatrix = BT709;
        api.set_option(
            ENCODER_OPTION_SVC_ENCODE_PARAM_EXT,
            (&raw mut params).cast(),
        ) == cmResultSuccess
    }
}

/// `OpenH264` rejects a target bitrate above the layer's max bitrate (initialized to the starting
/// bitrate), so raise the max before the target and lower the target before the max.
fn set_bitrate(encoder: &mut H264, from: u32, to: u32) -> bool {
    let highest = (ENCODER_OPTION_MAX_BITRATE, SPATIAL_LAYER_0);
    let target = (ENCODER_OPTION_BITRATE, SPATIAL_LAYER_ALL);
    let order = if to > from {
        [highest, target]
    } else {
        [target, highest]
    };
    order
        .into_iter()
        .all(|(option, layer)| set_option(encoder, option, layer, to))
}

fn set_frame_rate(encoder: &mut H264, pace: Pace) -> bool {
    let mut rate = hertz(pace.frame_rate());
    // SAFETY: the option takes a float, which lives for the call.
    unsafe {
        encoder
            .raw_api()
            .set_option(ENCODER_OPTION_FRAME_RATE, (&raw mut rate).cast())
            == cmResultSuccess
    }
}

fn set_option(encoder: &mut H264, option: ENCODER_OPTION, layer: LAYER_NUM, bitrate: u32) -> bool {
    let mut info = SBitrateInfo {
        iLayer: layer,
        iBitrate: c_int::try_from(bitrate).unwrap_or(c_int::MAX),
    };
    // SAFETY: both options take an `SBitrateInfo`, which lives for the call.
    unsafe { encoder.raw_api().set_option(option, (&raw mut info).cast()) == cmResultSuccess }
}

pub(crate) fn work(bitrate: u32, pace: Pace, output: &dyn Output, shared: &Shared) {
    let mut worker = Worker {
        output,
        shared,
        bitrate,
        pace,
        pictured_at: None,
        running: None,
        region: None,
        lost: false,
        missing: false,
        again: None,
        refusals: 0,
        cleanup: Cleanup::default(),
        pass_at: None,
        sharpening: Convergence::default(),
    };
    while let Some(work) = worker.next() {
        if !worker.code(work) {
            return;
        }
    }
}

struct Work {
    picture: Option<Pixels>,
    changed: f64,
    asked: bool,
    bitrate: Option<u32>,
    /// Encodes the last picture again: to retry it, or as a pass.
    again: bool,
    pass: bool,
}

struct Worker<'a> {
    output: &'a dyn Output,
    shared: &'a Shared,
    bitrate: u32,
    pace: Pace,
    /// When the encoder last took a new picture.
    pictured_at: Option<Instant>,
    running: Option<Running>,
    /// What the picture in the encoder shows of the source.
    region: Option<Region>,
    lost: bool,
    /// Set until the latest picture has been sent and taken.
    missing: bool,
    again: Option<Instant>,
    refusals: u32,
    cleanup: Cleanup,
    pass_at: Option<Instant>,
    sharpening: Convergence,
}

impl Worker<'_> {
    /// Waits for a new picture at its frame time, or while none waits, for a keyframe asked for
    /// while the picture is still, a retry or a pass.
    fn next(&mut self) -> Option<Work> {
        let shared = self.shared;
        let mut slot = pixels::lock(shared);
        loop {
            if slot.stop {
                return None;
            }
            if let Some(pace) = slot.pace.take() {
                self.pace = pace;
            }
            let now = Instant::now();
            let wake = if slot.picture.is_some() {
                match due(self.pace, self.pictured_at, now) {
                    Ok(()) => break,
                    Err(at) => Some(at),
                }
            } else {
                if slot.key && self.running.is_some() {
                    break;
                }
                let wake = self.again.into_iter().chain(self.pass_at).min();
                if wake.is_some_and(|at| at <= now) {
                    break;
                }
                wake
            };
            slot = pixels::wait(shared, slot, wake);
        }
        let now = Instant::now();
        let picture = slot.picture.take();
        if picture.is_some() {
            self.pictured_at = Some(now);
        }
        let again = self.again.is_some_and(|at| at <= now);
        if again || picture.is_some() {
            self.again = None;
        }
        let pass = picture.is_none() && !again && self.pass_at.is_some_and(|at| at <= now);
        if pass {
            self.pass_at = None;
        }
        Some(Work {
            picture,
            changed: std::mem::take(&mut slot.changed),
            asked: std::mem::take(&mut slot.key),
            bitrate: slot.bitrate.take(),
            again,
            pass,
        })
    }

    fn code(&mut self, work: Work) -> bool {
        let previous = self.bitrate;
        if let Some(bitrate) = work.bitrate {
            self.bitrate = bitrate;
        }
        let mut key = work.asked || self.lost;
        match work.picture {
            Some(picture) => {
                let Some(started) = self.take(picture, work.changed, previous) else {
                    return false;
                };
                key |= started;
            }
            None if work.again || work.pass || work.asked => self.keep_bitrate(previous),
            None => return true,
        }
        let Some(running) = self.running.as_mut() else {
            return true;
        };
        // The encoder starts with its first picture and takes options only after it, so a new
        // one codes that picture at the highest frame rate.
        if running.pace != self.pace && set_frame_rate(&mut running.encoder, self.pace) {
            running.pace = self.pace;
        }
        if key {
            running.encoder.force_intra_frame();
        }
        let stream = match running.encoder.encode(&running.planes.slices()) {
            Ok(stream) => stream,
            Err(problem) => {
                self.output
                    .failed(format!("the encoder refused a picture: {problem}"));
                return false;
            }
        };
        let data = stream.to_vec();
        let key_frame = matches!(stream.frame_type(), FrameType::IDR | FrameType::I);
        self.send(data, key_frame, key, work.pass);
        true
    }

    fn take(&mut self, picture: Pixels, changed: f64, previous: u32) -> Option<bool> {
        let bitrate = self.bitrate;
        let fits = self.running.as_mut().is_some_and(|running| {
            (running.planes.width, running.planes.height) == (picture.width, picture.height)
                && (bitrate == previous || set_bitrate(&mut running.encoder, previous, bitrate))
        });
        if !fits {
            self.running = start(picture.width, picture.height, bitrate);
            if self.running.is_none() {
                self.output.failed("the encoder did not start".to_owned());
                return None;
            }
        }
        self.running.as_mut()?.planes.read(&picture.bgra);
        self.region = picture.region;
        pixels::lock(self.shared).spare = picture.bgra;
        self.sharpening = Convergence::default();
        self.pass_at = self.cleanup.drawn(changed).then(|| Instant::now() + STILL);
        self.missing = true;
        Some(!fits)
    }

    fn keep_bitrate(&mut self, previous: u32) {
        let bitrate = self.bitrate;
        let Some(running) = self.running.as_mut().filter(|_| bitrate != previous) else {
            return;
        };
        if !set_bitrate(&mut running.encoder, previous, bitrate) {
            pixels::lock(self.shared).bitrate.get_or_insert(bitrate);
            self.bitrate = previous;
        }
    }

    /// Retries skipped or dropped frames here rather than asking the source for a new picture,
    /// which would look like a change and start needless passes.
    fn send(&mut self, data: Vec<u8>, key_frame: bool, key: bool, pass: bool) {
        let retry = Instant::now() + RETRY;
        if data.is_empty() {
            self.lost = key;
            if self.lost || self.missing {
                self.again = Some(retry);
            }
            if pass {
                self.passed(false);
            }
            return;
        }
        let size = data.len();
        let taken = self.output.frame(Encoded {
            codec: Codec::H264,
            kind: if key_frame {
                FrameKind::Key
            } else {
                FrameKind::Delta
            },
            mark: None,
            region: self.region,
            data: Bytes::from(data),
        });
        self.lost = !taken;
        if self.lost {
            self.refusals += 1;
            self.again = Some(Instant::now() + retry_after(self.refusals));
        } else {
            self.refusals = 0;
            self.missing = false;
        }
        if pass {
            let clean = self
                .sharpening
                .refined(size, frame_budget(self.bitrate, self.pace));
            self.passed(clean);
        }
    }

    fn passed(&mut self, clean: bool) {
        self.pass_at = self
            .cleanup
            .refined(clean)
            .then(|| Instant::now() + PASS_EVERY);
    }
}

#[cfg(test)]
mod tests {
    use super::Planes;

    fn converted(blue: u8, green: u8, red: u8) -> (u8, u8, u8) {
        let mut planes = Planes::new(2, 2);
        planes.read(&[blue, green, red, 255].repeat(4));
        (planes.luma[0], planes.blue[0], planes.red[0])
    }

    #[test]
    fn colours_land_on_the_bt709_limited_range_values_that_decoders_expect() {
        assert_eq!(converted(0, 0, 0), (16, 128, 128));
        assert_eq!(converted(255, 255, 255), (235, 128, 128));
        assert_eq!(converted(0, 0, 255), (63, 102, 240));
        assert_eq!(converted(0, 255, 0), (173, 42, 26));
        assert_eq!(converted(255, 0, 0), (32, 240, 118));
    }
}
