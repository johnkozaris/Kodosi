//! `OpenH264` in software, on a worker thread so encoding never blocks the source.

use std::{
    mem::MaybeUninit,
    os::raw::c_int,
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError},
    thread::JoinHandle,
    time::Instant,
};

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
    ENCODER_OPTION, ENCODER_OPTION_BITRATE, ENCODER_OPTION_MAX_BITRATE,
    ENCODER_OPTION_SVC_ENCODE_PARAM_EXT, LAYER_NUM, SBitrateInfo, SEncParamExt, SPATIAL_LAYER_0,
    SPATIAL_LAYER_ALL, cmResultSuccess,
};
use stream::{Codec, Encoded, Format, FrameKind};

use crate::{
    Output, Picture, Settings,
    refine::{Convergence, FRAME_RATE, RETRY, frame_budget},
};

/// H.264 colour description codes for BT.709, and "unspecified" for the video format.
const BT709: u8 = 1;
const UNSPECIFIED_FORMAT: u8 = 5;
const HALF: i32 = 1 << 15;

struct Pixels {
    bgra: Vec<u8>,
    width: usize,
    height: usize,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Refinement {
    #[default]
    Idle,
    Asked,
    Clean,
}

#[derive(Default)]
struct Slot {
    picture: Option<Pixels>,
    spare: Vec<u8>,
    key: bool,
    bitrate: Option<u32>,
    refinement: Refinement,
    stop: bool,
}

type Shared = Arc<(Mutex<Slot>, Condvar)>;

/// New pictures replace a picture still waiting for the worker; two buffers are reused between
/// the threads.
pub(crate) struct Encoder {
    shared: Shared,
    worker: Option<JoinHandle<()>>,
}

impl Encoder {
    pub(crate) fn new(settings: Settings, output: Arc<dyn Output>) -> Self {
        let shared: Shared = Arc::default();
        let working = Arc::clone(&shared);
        let worker = std::thread::Builder::new()
            .name("kodosi-encoder".to_owned())
            .spawn(move || work(settings.bitrate, output.as_ref(), &working))
            .ok();
        Self { shared, worker }
    }

    fn change<R>(&self, change: impl FnOnce(&mut Slot) -> R) -> R {
        let (slot, ready) = &*self.shared;
        let result = change(&mut slot.lock().unwrap_or_else(PoisonError::into_inner));
        ready.notify_one();
        result
    }

    pub(crate) fn want_key(&self) {
        self.change(|slot| slot.key = true);
    }

    pub(crate) fn set_bitrate(&self, bitrate: u32) {
        self.change(|slot| slot.bitrate = Some(bitrate));
    }

    /// `OpenH264` encodes only H.264, and repairs only with keyframes: its long-term references
    /// need camera mode, which halves the quality of scrolled text.
    pub(crate) fn set_format(&self, _format: Format) {
        self.want_key();
    }

    pub(crate) fn refine(&self) -> bool {
        self.change(|slot| match slot.refinement {
            Refinement::Clean => true,
            Refinement::Asked => false,
            Refinement::Idle => {
                slot.refinement = Refinement::Asked;
                slot.picture.is_none()
            }
        })
    }

    /// Copies outside the lock so the worker is not blocked while the rows are cloned.
    pub(crate) fn encode(&self, picture: Picture<'_>) {
        let Picture::Pixels {
            bgra,
            width,
            height,
        } = picture;
        let (even_width, even_height) = (width & !1, height & !1);
        if even_width < 2 || even_height < 2 || bgra.len() < width * height * 4 {
            return;
        }
        let (slot, _) = &*self.shared;
        let mut kept = {
            let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
            match slot.picture.take() {
                Some(waiting) => waiting.bgra,
                None => std::mem::take(&mut slot.spare),
            }
        };
        kept.clear();
        for row in bgra.chunks_exact(width * 4).take(even_height) {
            kept.extend_from_slice(&row[..even_width * 4]);
        }
        self.change(|slot| {
            slot.picture = Some(Pixels {
                bgra: kept,
                width: even_width,
                height: even_height,
            });
            slot.refinement = Refinement::Idle;
        });
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        self.change(|slot| slot.stop = true);
        if let Some(worker) = self.worker.take() {
            drop(worker.join());
        }
    }
}

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
}

fn start(width: usize, height: usize, bitrate: u32) -> Option<Running> {
    let config = EncoderConfig::new()
        .bitrate(BitRate::from_bps(bitrate))
        .max_frame_rate(FrameRate::from_hz(f32::from(
            u16::try_from(FRAME_RATE).ok()?,
        )))
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

fn set_option(encoder: &mut H264, option: ENCODER_OPTION, layer: LAYER_NUM, bitrate: u32) -> bool {
    let mut info = SBitrateInfo {
        iLayer: layer,
        iBitrate: c_int::try_from(bitrate).unwrap_or(c_int::MAX),
    };
    // SAFETY: both options take an `SBitrateInfo`, which lives for the call.
    unsafe { encoder.raw_api().set_option(option, (&raw mut info).cast()) == cmResultSuccess }
}

fn lock(shared: &Shared) -> MutexGuard<'_, Slot> {
    shared.0.lock().unwrap_or_else(PoisonError::into_inner)
}

fn work(bitrate: u32, output: &dyn Output, shared: &Shared) {
    let mut worker = Worker {
        output,
        shared,
        bitrate,
        running: None,
        lost: false,
        missing: false,
        again: None,
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
    asked: bool,
    bitrate: Option<u32>,
    refine: bool,
}

struct Worker<'a> {
    output: &'a dyn Output,
    shared: &'a Shared,
    bitrate: u32,
    running: Option<Running>,
    lost: bool,
    /// Set until the latest picture has been sent and taken.
    missing: bool,
    again: Option<Instant>,
    sharpening: Convergence,
}

impl Worker<'_> {
    fn next(&mut self) -> Option<Work> {
        let shared = self.shared;
        let mut slot = lock(shared);
        while slot.picture.is_none() && slot.refinement != Refinement::Asked && !slot.stop {
            let Some(wait) = self
                .again
                .map(|at| at.saturating_duration_since(Instant::now()))
            else {
                slot = shared.1.wait(slot).unwrap_or_else(PoisonError::into_inner);
                continue;
            };
            if wait.is_zero() {
                break;
            }
            slot = shared
                .1
                .wait_timeout(slot, wait)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        if slot.stop {
            return None;
        }
        let asked = slot.refinement == Refinement::Asked;
        if asked {
            slot.refinement = Refinement::Idle;
        }
        Some(Work {
            picture: slot.picture.take(),
            asked: std::mem::take(&mut slot.key),
            bitrate: slot.bitrate.take(),
            refine: self.again.take().is_some() || asked,
        })
    }

    fn code(&mut self, work: Work) -> bool {
        let previous = self.bitrate;
        if let Some(bitrate) = work.bitrate {
            self.bitrate = bitrate;
        }
        let mut key = work.asked || self.lost;
        let repeated = match work.picture {
            Some(picture) => {
                let Some(started) = self.take(picture, previous) else {
                    return false;
                };
                key |= started;
                false
            }
            None if work.refine => {
                self.keep_bitrate(previous);
                true
            }
            None => return true,
        };
        let Some(running) = self.running.as_mut() else {
            return true;
        };
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
        self.send(data, key_frame, key, repeated);
        true
    }

    fn take(&mut self, picture: Pixels, previous: u32) -> Option<bool> {
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
        {
            let mut slot = lock(self.shared);
            slot.spare = picture.bgra;
            if slot.refinement == Refinement::Clean {
                slot.refinement = Refinement::Idle;
            }
        }
        self.sharpening = Convergence::default();
        self.missing = true;
        Some(!fits)
    }

    fn keep_bitrate(&mut self, previous: u32) {
        let bitrate = self.bitrate;
        let Some(running) = self.running.as_mut().filter(|_| bitrate != previous) else {
            return;
        };
        if !set_bitrate(&mut running.encoder, previous, bitrate) {
            lock(self.shared).bitrate.get_or_insert(bitrate);
            self.bitrate = previous;
        }
    }

    /// Retries skipped or dropped frames here rather than asking the source for a new picture,
    /// which would look like a change and start needless refinement passes.
    fn send(&mut self, data: Vec<u8>, key_frame: bool, key: bool, repeated: bool) {
        let retry = Instant::now() + RETRY;
        if data.is_empty() {
            self.lost = key;
            if self.lost || self.missing {
                self.again = Some(retry);
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
            data: Bytes::from(data),
        });
        self.lost = !taken;
        if self.lost {
            self.again = Some(retry);
        } else {
            self.missing = false;
        }
        if repeated && self.sharpening.refined(size, frame_budget(self.bitrate)) {
            let mut slot = lock(self.shared);
            if slot.picture.is_none() {
                slot.refinement = Refinement::Clean;
            }
        }
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
