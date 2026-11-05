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
    key: bool,
    bitrate: Option<u32>,
    stop: bool,
}

type Shared = Arc<(Mutex<Slot>, Condvar)>;

/// OpenH264 on a thread of its own, so a slow picture never holds up the browser. The newest
/// picture replaces one that still waits.
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

    fn change(&self, change: impl FnOnce(&mut Slot)) {
        let (slot, ready) = &*self.shared;
        change(&mut slot.lock().unwrap_or_else(PoisonError::into_inner));
        ready.notify_one();
    }

    pub(crate) fn want_key(&mut self) {
        self.change(|slot| slot.key = true);
    }

    pub(crate) fn set_bitrate(&mut self, bitrate: u32) {
        self.change(|slot| slot.bitrate = Some(bitrate));
    }

    /// Takes BGRA rows; an odd width or height loses its last column or row.
    pub(crate) fn encode_pixels(&mut self, pixels: &[u8], width: usize, height: usize) {
        let (even_width, even_height) = (width & !1, height & !1);
        if even_width < 2 || even_height < 2 || pixels.len() < width * height * 4 {
            return;
        }
        let time = u64::try_from(self.started.elapsed().as_micros()).unwrap_or(u64::MAX);
        self.change(|slot| {
            let mut kept = slot
                .picture
                .take()
                .map(|picture| picture.pixels)
                .unwrap_or_default();
            kept.clear();
            for row in pixels.chunks_exact(width * 4).take(even_height) {
                kept.extend_from_slice(&row[..even_width * 4]);
            }
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

fn start(width: usize, height: usize, bitrate: u32) -> Option<(H264, usize, usize)> {
    let config = EncoderConfig::new()
        .bitrate(BitRate::from_bps(bitrate))
        .max_frame_rate(FrameRate::from_hz(f32::from(
            u16::try_from(FRAME_RATE).ok()?,
        )))
        .usage_type(UsageType::ScreenContentRealTime)
        .rate_control_mode(RateControlMode::Bitrate)
        .skip_frames(true);
    let encoder = H264::with_api_config(OpenH264API::from_source(), config).ok()?;
    Some((encoder, width, height))
}

fn work(window: i64, mut bitrate: u32, link: &Link, shared: &Shared) {
    let (slot, ready) = &**shared;
    let mut encoder: Option<(H264, usize, usize)> = None;
    loop {
        let (picture, key, changed) = {
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
        let mut key = key;
        if encoder
            .as_ref()
            .is_none_or(|(_, width, height)| (*width, *height) != (picture.width, picture.height))
        {
            encoder = start(picture.width, picture.height, bitrate);
            key = true;
            if encoder.is_none() {
                link.send(&FromEngine::Failed {
                    window,
                    problem: "the encoder did not start".to_owned(),
                });
                return;
            }
        }
        let Some((encoder, _, _)) = encoder.as_mut() else {
            continue;
        };
        if changed.is_some() {
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
        if key {
            encoder.force_intra_frame();
        }
        let source = YUVBuffer::from_rgb_source(BgraSliceU8::new(
            &picture.pixels,
            (picture.width, picture.height),
        ));
        let Ok(stream) = encoder.encode(&source) else {
            continue;
        };
        let data = stream.to_vec();
        if data.is_empty() {
            continue;
        }
        link.send_frame(&EncodedFrame {
            window,
            key: matches!(stream.frame_type(), FrameType::IDR | FrameType::I),
            timestamp: picture.time,
            data: Bytes::from(data),
        });
    }
}
