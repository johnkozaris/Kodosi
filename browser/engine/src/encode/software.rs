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
        BitRate, Encoder as H264, EncoderConfig, FrameRate, FrameType, RateControlMode, UsageType,
    },
    formats::{BgraSliceU8, YUVBuffer},
};
use openh264_sys2::{ENCODER_OPTION_BITRATE, SBitrateInfo, SPATIAL_LAYER_ALL};
use shared_browser::link::{EncodedFrame, FromEngine};

use super::FRAME_RATE;
use crate::link::Link;

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
        let worker = std::thread::Builder::new()
            .name("kodosi-encoder".to_owned())
            .spawn(move || work(window, bitrate, &link, &working))
            .ok();
        Self {
            shared,
            started: Instant::now(),
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

    /// Takes BGRA rows; an odd width or height loses its last column or row. The copy happens
    /// outside the lock, so the worker never waits for it.
    pub(crate) fn encode_pixels(&mut self, pixels: &[u8], width: usize, height: usize) {
        let (even_width, even_height) = (width & !1, height & !1);
        if even_width < 2 || even_height < 2 || pixels.len() < width * height * 4 {
            return;
        }
        let time = u64::try_from(self.started.elapsed().as_micros()).unwrap_or(u64::MAX);
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
        .skip_frames(true);
    let encoder = H264::with_api_config(OpenH264API::from_source(), config).ok()?;
    Some(Running {
        encoder,
        picture: YUVBuffer::new(width, height),
        width,
        height,
    })
}

fn set_bitrate(encoder: &mut H264, bitrate: u32) {
    let mut info = SBitrateInfo {
        iLayer: SPATIAL_LAYER_ALL,
        iBitrate: c_int::try_from(bitrate).unwrap_or(c_int::MAX),
    };
    // SAFETY: the option takes an `SBitrateInfo`, which lives for the call.
    unsafe {
        encoder
            .raw_api()
            .set_option(ENCODER_OPTION_BITRATE, (&raw mut info).cast());
    }
}

fn work(window: i64, mut bitrate: u32, link: &Link, shared: &Shared) {
    let (slot, ready) = &**shared;
    let mut running: Option<Running> = None;
    let mut lost = false;
    loop {
        let (picture, asked, changed) = {
            let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
            while slot.picture.is_none() && !slot.stop {
                slot = ready.wait(slot).unwrap_or_else(PoisonError::into_inner);
            }
            if slot.stop {
                return;
            }
            (
                slot.picture.take(),
                std::mem::take(&mut slot.key),
                slot.bitrate.take(),
            )
        };
        let Some(picture) = picture else { continue };
        if let Some(changed) = changed {
            bitrate = changed;
        }
        let mut key = asked || lost;
        if running.as_ref().is_none_or(|running| {
            (running.width, running.height) != (picture.width, picture.height)
        }) {
            running = start(picture.width, picture.height, bitrate);
            key = true;
            if running.is_none() {
                link.send(&FromEngine::Failed {
                    window,
                    problem: "the encoder did not start".to_owned(),
                });
                return;
            }
        } else if changed.is_some()
            && let Some(running) = running.as_mut()
        {
            set_bitrate(&mut running.encoder, bitrate);
        }
        let Some(running) = running.as_mut() else {
            continue;
        };
        running.picture.read_bgra8(BgraSliceU8::new(
            &picture.pixels,
            (picture.width, picture.height),
        ));
        slot.lock().unwrap_or_else(PoisonError::into_inner).spare = picture.pixels;
        if key {
            running.encoder.force_intra_frame();
        }
        let Ok(stream) = running.encoder.encode(&running.picture) else {
            lost = key;
            continue;
        };
        let data = stream.to_vec();
        if data.is_empty() {
            lost = key;
            continue;
        }
        lost = link.send_frame(&EncodedFrame {
            window,
            key: matches!(stream.frame_type(), FrameType::IDR | FrameType::I),
            timestamp: picture.time,
            data: Bytes::from(data),
        });
    }
}
