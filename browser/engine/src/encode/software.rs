use std::{
    os::raw::c_int,
    sync::{Arc, Condvar, Mutex, PoisonError},
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
    formats::{BgraSliceU8, YUVBuffer},
};
use openh264_sys2::{
    ENCODER_OPTION, ENCODER_OPTION_BITRATE, ENCODER_OPTION_MAX_BITRATE, LAYER_NUM, SBitrateInfo,
    SPATIAL_LAYER_0, SPATIAL_LAYER_ALL, cmResultSuccess,
};
use shared_browser::link::{EncodedFrame, FromEngine};

use super::{FRAME_RATE, REDRAW_AFTER_MS};
use crate::{engine, link::Link};

struct Picture {
    pixels: Vec<u8>,
    width: usize,
    height: usize,
    time: u64,
}

#[derive(Default)]
struct Slot {
    picture: Option<Picture>,
    /// The buffer of the last encoded picture, for the next one.
    spare: Vec<u8>,
    key: bool,
    bitrate: Option<u32>,
    refine: bool,
    stop: bool,
}

type Shared = Arc<(Mutex<Slot>, Condvar)>;

/// `OpenH264` on a thread of its own, so a slow picture never holds up the browser. The newest
/// picture replaces one that still waits, and the two picture buffers go round.
pub(crate) struct Encoder {
    shared: Shared,
    started: Instant,
    worker: Option<JoinHandle<()>>,
}

impl Encoder {
    pub(crate) fn new(window: i64, bitrate: u32, link: Link) -> Self {
        let shared: Shared = Arc::default();
        let working = Arc::clone(&shared);
        let started = Instant::now();
        let worker = std::thread::Builder::new()
            .name("kodosi-encoder".to_owned())
            .spawn(move || work(window, bitrate, started, &link, &working))
            .ok();
        Self {
            shared,
            started,
            worker,
        }
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

    pub(crate) fn set_bitrate(&mut self, bitrate: u32) {
        self.change(|slot| slot.bitrate = Some(bitrate));
    }

    /// Codes the last picture again, unless a newer one waits.
    pub(crate) fn refine(&mut self) {
        self.change(|slot| slot.refine = true);
    }

    /// Takes BGRA rows; an odd width or height loses its last column or row. The copy happens
    /// outside the lock, so the worker never waits for it.
    pub(crate) fn encode_pixels(&mut self, pixels: &[u8], width: usize, height: usize) {
        let (even_width, even_height) = (width & !1, height & !1);
        if even_width < 2 || even_height < 2 || pixels.len() < width * height * 4 {
            return;
        }
        let time = micros_since(self.started);
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
                time,
            });
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
        // A range that reaches 0 becomes 26 to 35 for screen content, so a still page never got
        // sharper than 43 dB. A key frame is never coarser than 40 either.
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

/// `OpenH264` refuses a bitrate above the highest of its layer, which starts as the first bitrate,
/// so a rise moves the highest first and a cut moves it last.
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

fn micros_since(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

fn work(window: i64, mut bitrate: u32, started: Instant, link: &Link, shared: &Shared) {
    let (slot, ready) = &**shared;
    let mut running: Option<Running> = None;
    let mut lost = false;
    loop {
        let (picture, asked, changed, refine) = {
            let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
            while slot.picture.is_none() && !slot.refine && !slot.stop {
                slot = ready.wait(slot).unwrap_or_else(PoisonError::into_inner);
            }
            if slot.stop {
                return;
            }
            (
                slot.picture.take(),
                std::mem::take(&mut slot.key),
                slot.bitrate.take(),
                std::mem::take(&mut slot.refine),
            )
        };
        let previous = bitrate;
        if let Some(changed) = changed {
            bitrate = changed;
        }
        let mut key = asked || lost;
        let time = if let Some(picture) = picture {
            let fits = running.as_mut().is_some_and(|running| {
                (running.width, running.height) == (picture.width, picture.height)
                    && (bitrate == previous || set_bitrate(&mut running.encoder, previous, bitrate))
            });
            if !fits {
                running = start(picture.width, picture.height, bitrate);
                key = true;
                if running.is_none() {
                    link.send(&FromEngine::Failed {
                        window,
                        problem: "the encoder did not start".to_owned(),
                    });
                    return;
                }
            }
            let Some(running) = running.as_mut() else {
                continue;
            };
            running.picture.read_bgra8(BgraSliceU8::new(
                &picture.pixels,
                (picture.width, picture.height),
            ));
            slot.lock().unwrap_or_else(PoisonError::into_inner).spare = picture.pixels;
            Some(picture.time)
        } else {
            None
        };
        let Some(running) = running.as_mut().filter(|_| time.is_some() || refine) else {
            continue;
        };
        if time.is_none()
            && bitrate != previous
            && !set_bitrate(&mut running.encoder, previous, bitrate)
        {
            slot.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .bitrate
                .get_or_insert(bitrate);
            bitrate = previous;
        }
        if key {
            running.encoder.force_intra_frame();
        }
        let stream = match running.encoder.encode(&running.picture) {
            Ok(stream) => stream,
            Err(problem) => {
                link.send(&FromEngine::Failed {
                    window,
                    problem: format!("the encoder refused a picture: {problem}"),
                });
                return;
            }
        };
        let data = stream.to_vec();
        // A refinement that the rate control skipped leaves the viewers with the picture that
        // they have, so the page need not draw again.
        if data.is_empty() {
            lost = key;
            if key || time.is_some() {
                engine::redraw(window, REDRAW_AFTER_MS);
            }
            continue;
        }
        lost = link.send_frame(&EncodedFrame {
            window,
            key: matches!(stream.frame_type(), FrameType::IDR | FrameType::I),
            timestamp: time.unwrap_or_else(|| micros_since(started)),
            data: Bytes::from(data),
        });
        if lost {
            engine::redraw(window, REDRAW_AFTER_MS);
        }
    }
}
