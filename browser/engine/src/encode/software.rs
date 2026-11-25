use std::{
    os::raw::c_int,
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use bytes::Bytes;
use openh264::{
    OpenH264API,
    encoder::{
        BitRate, Encoder as H264, EncoderConfig, FrameRate, FrameType, QpRange, RateControlMode,
        UsageType,
    },
    formats::{BgraSliceU8, YUVBuffer},
};
use openh264_sys2::{
    ENCODER_OPTION, ENCODER_OPTION_BITRATE, ENCODER_OPTION_MAX_BITRATE, LAYER_NUM, SBitrateInfo,
    SPATIAL_LAYER_0, SPATIAL_LAYER_ALL, cmResultSuccess,
};
use shared_browser::link::{EncodedFrame, FrameKind, FromEngine};

use super::{Convergence, FRAME_RATE, REDRAW_AFTER_MS, frame_budget};
use crate::link::Link;

struct Picture {
    pixels: Vec<u8>,
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
    picture: Option<Picture>,
    spare: Vec<u8>,
    key: bool,
    bitrate: Option<u32>,
    refinement: Refinement,
    stop: bool,
}

type Shared = Arc<(Mutex<Slot>, Condvar)>;

/// Runs `OpenH264` on a dedicated worker thread so encoding never blocks the browser. Incoming
/// frames overwrite any unencoded pending frame, reusing two pixel buffers between threads.
pub(crate) struct Encoder {
    shared: Shared,
    worker: Option<JoinHandle<()>>,
}

impl Encoder {
    pub(crate) fn new(window: i64, bitrate: u32, link: Link) -> Self {
        let shared: Shared = Arc::default();
        let working = Arc::clone(&shared);
        let worker = std::thread::Builder::new()
            .name("kodosi-encoder".to_owned())
            .spawn(move || work(window, bitrate, &link, &working))
            .ok();
        Self { shared, worker }
    }

    fn change<R>(&self, change: impl FnOnce(&mut Slot) -> R) -> R {
        let (slot, ready) = &*self.shared;
        let result = change(&mut slot.lock().unwrap_or_else(PoisonError::into_inner));
        ready.notify_one();
        result
    }

    pub(crate) fn want_key(&mut self) {
        self.change(|slot| slot.key = true);
    }

    /// `OpenH264` only supports acknowledged LTR frames in camera mode, which halves quality on
    /// scrolled pages compared to screen mode, so recovery falls back to a full keyframe.
    pub(crate) fn recover(&mut self) {
        self.want_key();
    }

    pub(crate) fn set_bitrate(&mut self, bitrate: u32) {
        self.change(|slot| slot.bitrate = Some(bitrate));
    }

    pub(crate) fn refine(&mut self) -> bool {
        self.change(|slot| match slot.refinement {
            Refinement::Clean => true,
            Refinement::Asked => false,
            Refinement::Idle => {
                slot.refinement = Refinement::Asked;
                slot.picture.is_none()
            }
        })
    }

    /// Copies outside the mutex so the worker thread is not blocked while cloning BGRA rows.
    pub(crate) fn encode_pixels(&mut self, pixels: &[u8], width: usize, height: usize) {
        let (even_width, even_height) = (width & !1, height & !1);
        if even_width < 2 || even_height < 2 || pixels.len() < width * height * 4 {
            return;
        }
        let (slot, _) = &*self.shared;
        let mut kept = {
            let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
            match slot.picture.take() {
                Some(waiting) => waiting.pixels,
                None => std::mem::take(&mut slot.spare),
            }
        };
        kept.clear();
        for row in pixels.chunks_exact(width * 4).take(even_height) {
            kept.extend_from_slice(&row[..even_width * 4]);
        }
        self.change(|slot| {
            slot.picture = Some(Picture {
                pixels: kept,
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

struct Running {
    encoder: H264,
    picture: YUVBuffer,
    width: usize,
    height: usize,
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
    let encoder = H264::with_api_config(OpenH264API::from_source(), config).ok()?;
    Some(Running {
        encoder,
        picture: YUVBuffer::new(width, height),
        width,
        height,
    })
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

fn work(window: i64, bitrate: u32, link: &Link, shared: &Shared) {
    let mut worker = Worker {
        window,
        link,
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
    picture: Option<Picture>,
    asked: bool,
    bitrate: Option<u32>,
    refine: bool,
}

struct Worker<'a> {
    window: i64,
    link: &'a Link,
    shared: &'a Shared,
    bitrate: u32,
    running: Option<Running>,
    lost: bool,
    /// Set until the latest input frame has been emitted and accepted by the link.
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
        let stream = match running.encoder.encode(&running.picture) {
            Ok(stream) => stream,
            Err(problem) => {
                self.link.send(&FromEngine::Failed {
                    window: self.window,
                    problem: format!("the encoder refused a picture: {problem}"),
                });
                return false;
            }
        };
        let data = stream.to_vec();
        let key_frame = matches!(stream.frame_type(), FrameType::IDR | FrameType::I);
        self.send(data, key_frame, key, repeated);
        true
    }

    fn take(&mut self, picture: Picture, previous: u32) -> Option<bool> {
        let bitrate = self.bitrate;
        let fits = self.running.as_mut().is_some_and(|running| {
            (running.width, running.height) == (picture.width, picture.height)
                && (bitrate == previous || set_bitrate(&mut running.encoder, previous, bitrate))
        });
        if !fits {
            self.running = start(picture.width, picture.height, bitrate);
            if self.running.is_none() {
                self.link.send(&FromEngine::Failed {
                    window: self.window,
                    problem: "the encoder did not start".to_owned(),
                });
                return None;
            }
        }
        let running = self.running.as_mut()?;
        running.picture.read_bgra8(BgraSliceU8::new(
            &picture.pixels,
            (picture.width, picture.height),
        ));
        {
            let mut slot = lock(self.shared);
            slot.spare = picture.pixels;
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

    /// Retries skipped or dropped frames locally via `self.again` rather than requesting a CEF
    /// redraw, which would look like a page change and trigger unnecessary refinement passes.
    fn send(&mut self, data: Vec<u8>, key_frame: bool, key: bool, repeated: bool) {
        let retry = Instant::now() + Duration::from_millis(REDRAW_AFTER_MS.unsigned_abs());
        if data.is_empty() {
            self.lost = key;
            if self.lost || self.missing {
                self.again = Some(retry);
            }
            return;
        }
        let size = data.len();
        self.lost = self.link.send_frame(&EncodedFrame {
            window: self.window,
            kind: if key_frame {
                FrameKind::Key
            } else {
                FrameKind::Delta
            },
            mark: None,
            still: false,
            data: Bytes::from(data),
        });
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
