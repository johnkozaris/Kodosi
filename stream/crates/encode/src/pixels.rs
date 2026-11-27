//! Encoders that take BGRA pixels on a worker thread, so encoding never blocks the source. New
//! pictures replace a picture still waiting for the worker; two buffers are reused between the
//! threads. On Linux, the newest picture may instead be one that the GPU converted already.

use std::{
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError},
    thread::JoinHandle,
    time::Instant,
};

use stream::{Format, Region};

use crate::{Pace, Picture};

pub(crate) struct Pixels {
    pub(crate) bgra: Vec<u8>,
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) region: Option<Region>,
}

#[derive(Default)]
pub(crate) struct Slot {
    pub(crate) picture: Option<Pixels>,
    /// The newest picture in place of `picture`, when the GPU converted it.
    #[cfg(target_os = "linux")]
    pub(crate) converted: Option<crate::vaapi::Converted>,
    /// The share of the source that changed since the worker last took a picture.
    pub(crate) changed: f64,
    pub(crate) spare: Vec<u8>,
    #[cfg(target_os = "linux")]
    pub(crate) spare_surface: Option<crate::vaapi::Surface>,
    /// The worker no longer encodes on the GPU, so pictures must come as pixels.
    #[cfg(target_os = "linux")]
    pub(crate) gpu_stopped: bool,
    pub(crate) key: bool,
    /// A new format for the stream, which `OpenH264` encodes as H.264 all the same.
    pub(crate) format: Option<Format>,
    pub(crate) bitrate: Option<u32>,
    pub(crate) pace: Option<Pace>,
    pub(crate) stop: bool,
}

pub(crate) type Shared = Arc<(Mutex<Slot>, Condvar)>;

pub(crate) fn lock(shared: &Shared) -> MutexGuard<'_, Slot> {
    shared.0.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Waits until the caller changes the slot, or until `until`.
pub(crate) fn wait<'a>(
    shared: &'a Shared,
    slot: MutexGuard<'a, Slot>,
    until: Option<Instant>,
) -> MutexGuard<'a, Slot> {
    let ready = &shared.1;
    match until {
        Some(at) => {
            ready
                .wait_timeout(slot, at.saturating_duration_since(Instant::now()))
                .unwrap_or_else(PoisonError::into_inner)
                .0
        }
        None => ready.wait(slot).unwrap_or_else(PoisonError::into_inner),
    }
}

pub(crate) struct Encoder {
    shared: Shared,
    worker: Option<JoinHandle<()>>,
}

impl Encoder {
    pub(crate) fn start(work: impl FnOnce(&Shared) + Send + 'static) -> Self {
        let shared: Shared = Arc::default();
        let working = Arc::clone(&shared);
        let worker = std::thread::Builder::new()
            .name("kodosi-encoder".to_owned())
            .spawn(move || work(&working))
            .ok();
        Self { shared, worker }
    }

    #[cfg(target_os = "linux")]
    pub(crate) const fn shared(&self) -> &Shared {
        &self.shared
    }

    fn change<R>(&self, change: impl FnOnce(&mut Slot) -> R) -> R {
        let result = change(&mut lock(&self.shared));
        self.shared.1.notify_one();
        result
    }

    pub(crate) fn want_key(&self) {
        self.change(|slot| slot.key = true);
    }

    /// Restarts the stream in the new format; the next frame is a keyframe.
    pub(crate) fn set_format(&self, format: Format) {
        self.change(|slot| {
            slot.format = Some(format);
            slot.key = true;
        });
    }

    pub(crate) fn set_bitrate(&self, bitrate: u32) {
        self.change(|slot| slot.bitrate = Some(bitrate));
    }

    pub(crate) fn set_pace(&self, pace: Pace) {
        self.change(|slot| slot.pace = Some(pace));
    }

    /// Copies outside the lock so the worker is not blocked while the rows are cloned.
    pub(crate) fn encode(&self, picture: Picture<'_>, region: Option<Region>, changed: f64) {
        let Picture::Pixels {
            bgra,
            width,
            height,
        } = picture;
        let (even_width, even_height) = (width & !1, height & !1);
        if even_width < 2 || even_height < 2 || bgra.len() < width * height * 4 {
            return;
        }
        let mut kept = {
            let mut slot = lock(&self.shared);
            #[cfg(target_os = "linux")]
            if let Some(waiting) = slot.converted.take() {
                slot.spare_surface = Some(waiting.surface);
            }
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
                region,
            });
            slot.changed = (slot.changed + changed).min(1.0);
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
