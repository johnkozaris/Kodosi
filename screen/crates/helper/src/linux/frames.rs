//! The encoder of the shown source's pictures, which the capture's thread feeds and the link to
//! the host steers, whichever way the helper shares the screen.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bytes::Bytes;
use screen::link;
use stream::{Encoded, Region};
use stream_encode::{Crop, Dmabuf, Encoder, Imported, Output, Picture, Settings};
use tokio::sync::mpsc;

use super::Told;

/// The encoder, which the capture's thread feeds and the link steers.
pub(super) struct Frames {
    encoding: Mutex<Encoding>,
}

struct Encoding {
    encoder: Encoder,
    settings: Settings,
}

impl Frames {
    pub(super) fn new<E: Told>(
        settings: Settings,
        say: mpsc::Sender<Bytes>,
        events: mpsc::UnboundedSender<E>,
    ) -> Arc<Self> {
        Arc::new(Self {
            encoding: Mutex::new(Encoding {
                encoder: Encoder::new(settings, Arc::new(Sent { say, events })),
                settings,
            }),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Encoding> {
        self.encoding.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn encode_as(&self, settings: Settings) {
        let mut encoding = self.lock();
        if settings.format != encoding.settings.format {
            encoding.encoder.set_format(settings.format);
        }
        if settings.bitrate != encoding.settings.bitrate {
            encoding.encoder.set_bitrate(settings.bitrate);
        }
        if settings.pace != encoding.settings.pace {
            encoding.encoder.set_pace(settings.pace);
        }
        encoding.settings = settings;
    }

    pub(super) fn encoder(&self, work: impl FnOnce(&mut Encoder)) {
        work(&mut self.lock().encoder);
    }

    pub(super) fn encode(&self, picture: Picture<'_>, region: Option<Region>, changed: f64) {
        self.lock().encoder.encode(picture, region, changed);
    }

    pub(super) fn imports(&self) -> bool {
        self.lock().encoder.imports()
    }

    pub(super) fn import(&self, buffer: &Dmabuf<'_>) -> Option<Imported> {
        self.lock().encoder.import(buffer)
    }

    pub(super) fn encode_imported(
        &self,
        frame: &Imported,
        crop: Crop,
        size: (usize, usize),
        region: Option<Region>,
        changed: f64,
    ) -> bool {
        self.lock()
            .encoder
            .encode_imported(frame, crop, size, region, changed)
    }
}

/// Sends encoded frames to the host. A full link drops a frame, which the encoder repairs.
struct Sent<E> {
    say: mpsc::Sender<Bytes>,
    events: mpsc::UnboundedSender<E>,
}

impl<E: Told> Output for Sent<E> {
    fn frame(&self, frame: Encoded) -> bool {
        self.say.try_send(link::frame(&frame)).is_ok()
    }

    fn failed(&self, problem: String) {
        drop(self.events.send(E::failed(problem)));
    }

    fn redraw(&self) {
        drop(self.events.send(E::redraw()));
    }
}
