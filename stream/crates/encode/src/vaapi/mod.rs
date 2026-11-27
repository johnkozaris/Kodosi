//! H.264 and HEVC on Intel and AMD graphics through VA-API. The worker chooses each frame's
//! quantizer for the link, and sharpens a picture that settles with passes that encode it
//! again. When the GPU fails, software takes over.

mod bits;
mod h264;
mod hevc;
mod session;
mod va;

use std::{ffi::c_int, path::PathBuf, rc::Rc, sync::OnceLock, time::Instant};

use bytes::Bytes;
use stream::{Codec, Encoded, FrameKind};

use self::{session::Session, va::Display};
use crate::{
    Output, Pace, Settings,
    pixels::{self, Pixels, Shared, Slot},
    rate::{self, Choice, Rate, SETTLED},
    retry_after,
};

/// A GPU that encodes H.264 as the encoder needs, and HEVC too when its driver says how.
struct Gpu {
    node: PathBuf,
    h264: c_int,
    hevc: Option<hevc::Capabilities>,
}

impl Gpu {
    /// The codec that the GPU encodes for a stream that asks for `codec`.
    fn encodes(&self, codec: Codec) -> Codec {
        match codec {
            Codec::Hevc if self.hevc.is_some() => Codec::Hevc,
            _ => Codec::H264,
        }
    }
}

/// The first GPU that encodes H.264 as the encoder needs.
fn gpu() -> Option<&'static Gpu> {
    static GPU: OnceLock<Option<Gpu>> = OnceLock::new();
    GPU.get_or_init(|| {
        let mut nodes: Vec<PathBuf> = std::fs::read_dir("/dev/dri")
            .ok()?
            .filter_map(|entry| Some(entry.ok()?.path()))
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("renderD"))
            })
            .collect();
        nodes.sort();
        nodes.into_iter().find_map(|node| {
            let display = Display::open(&node)?;
            let converts = display
                .entrypoints(va::NO_PROFILE)
                .contains(&va::VIDEO_PROCESSING);
            let h264 = h264::entrypoint(&display).filter(|_| converts)?;
            let hevc = hevc::capabilities(&display);
            Some(Gpu { node, h264, hevc })
        })
    })
    .as_ref()
}

/// The codecs that the GPU encodes, when there is one.
pub(crate) fn encodable() -> Option<Vec<Codec>> {
    let gpu = gpu()?;
    let mut codecs = vec![Codec::H264];
    if gpu.hevc.is_some() {
        codecs.push(Codec::Hevc);
    }
    Some(codecs)
}

/// Encodes the pictures that come into `shared` until the encoder stops. Returns false when the
/// GPU cannot encode them, with the newest picture back in the slot.
pub(crate) fn work(settings: Settings, output: &dyn Output, shared: &Shared) -> bool {
    let Some((gpu, display)) = gpu().and_then(|gpu| Some((gpu, Display::open(&gpu.node)?))) else {
        return false;
    };
    let now = Instant::now();
    let mut worker = Worker {
        output,
        shared,
        gpu,
        display: Rc::new(display),
        codec: gpu.encodes(settings.format.codec),
        session: None,
        last: None,
        bitrate: settings.bitrate,
        pace: settings.pace,
        rate: Rate::new(settings.bitrate, settings.pace, now),
        drawn_at: now,
        repair: false,
        retry_at: None,
        refusals: 0,
    };
    while let Some(job) = worker.next() {
        if worker.code(job).is_err() {
            give_back(shared, worker.last.take(), worker.bitrate, worker.pace);
            return false;
        }
    }
    true
}

/// Leaves the slot as software needs it to take over: with the newest picture, as all new, and
/// with the bitrate and the pace.
fn give_back(shared: &Shared, picture: Option<Pixels>, bitrate: u32, pace: Pace) {
    let mut slot = pixels::lock(shared);
    if let Some(picture) = picture
        && slot.picture.is_none()
    {
        slot.picture = Some(picture);
    }
    slot.changed = 1.0;
    slot.bitrate.get_or_insert(bitrate);
    slot.pace.get_or_insert(pace);
}

struct Job {
    /// A new picture, or none to encode the last one again.
    picture: Option<Pixels>,
    key: bool,
    choice: Choice,
}

struct Worker<'a> {
    output: &'a dyn Output,
    shared: &'a Shared,
    gpu: &'static Gpu,
    display: Rc<Display>,
    /// The codec of the stream's next keyframe.
    codec: Codec,
    session: Option<Session>,
    /// The last picture, which the session encodes again for passes and repairs, and loads
    /// again when it restarts in another codec.
    last: Option<Pixels>,
    bitrate: u32,
    pace: Pace,
    rate: Rate,
    drawn_at: Instant,
    /// Set when the next frame must be a keyframe.
    repair: bool,
    retry_at: Option<Instant>,
    refusals: u32,
}

impl Worker<'_> {
    fn next(&mut self) -> Option<Job> {
        let shared = self.shared;
        let mut slot = pixels::lock(shared);
        loop {
            if slot.stop {
                return None;
            }
            let now = Instant::now();
            if let Some(bitrate) = slot.bitrate.take() {
                self.bitrate = bitrate;
                self.rate.set_bitrate(bitrate, now);
            }
            if let Some(pace) = slot.pace.take() {
                self.pace = pace;
                self.rate.set_pace(pace);
            }
            self.repair |= std::mem::take(&mut slot.key);
            if let Some(format) = slot.format.take() {
                self.codec = self.gpu.encodes(format.codec);
            }
            match self.plan(&mut slot, now) {
                Ok(job) => return Some(job),
                Err(wake) => slot = pixels::wait(shared, slot, wake),
            }
        }
    }

    /// Plans the next frame: a new picture when the link has room for it, else the last picture
    /// again to repair the stream, else a pass once the picture settles. Otherwise tells when to
    /// look again, or `None` to wait for a change.
    fn plan(&mut self, slot: &mut Slot, now: Instant) -> Result<Job, Option<Instant>> {
        let retry_due = self.retry_at.is_some_and(|at| at <= now);
        let repair_due = retry_due || (self.retry_at.is_none() && self.repair);
        let Some(shown) = slot.picture.as_ref().or(self.last.as_ref()) else {
            return Err(None);
        };
        let size = (shown.width, shown.height);
        let current = self
            .session
            .as_ref()
            .map(|session| (session.size(), session.codec()));
        let key = self.repair || current != Some((size, self.codec));
        let pixels = rate::pixels(size.0, size.1);
        let choice = if slot.picture.is_some() {
            self.rate.picture(slot.changed, key, pixels, now)
        } else if repair_due {
            self.rate.repair(true, pixels, now)
        } else if self.retry_at.is_some() {
            return Err(self.retry_at);
        } else {
            let settled = self.drawn_at + SETTLED;
            if self.rate.sharp() {
                return Err(None);
            }
            if now < settled {
                return Err(Some(settled));
            }
            self.rate.pass(pixels, now).ok_or(None)?
        }
        .map_err(Some)?;
        let picture = slot.picture.take();
        if picture.is_some() {
            slot.changed = 0.0;
        }
        // A new picture repairs the stream as well as a retry would.
        if retry_due || picture.is_some() {
            self.retry_at = None;
        }
        self.repair = false;
        Ok(Job {
            picture,
            key,
            choice,
        })
    }

    /// Encodes a frame of the new picture, or of the last one again.
    fn code(&mut self, job: Job) -> Result<(), va::Status> {
        let fresh = job.picture.is_some();
        if let Some(picture) = job.picture {
            if let Some(shown) = self.last.replace(picture) {
                pixels::lock(self.shared).spare = shown.bgra;
            }
            self.drawn_at = Instant::now();
        }
        let Some(last) = &self.last else {
            return Ok(());
        };
        let size = (last.width, last.height);
        let region = last.region;
        let stale = self
            .session
            .as_ref()
            .is_none_or(|session| (session.size(), session.codec()) != (size, self.codec));
        if stale {
            self.session = None;
            self.session = Some(Session::start(&self.display, self.gpu, self.codec, size)?);
        }
        let (Some(session), Some(last)) = (&mut self.session, &self.last) else {
            return Ok(());
        };
        if fresh || stale {
            session.load(&last.bgra)?;
        }
        let codec = session.codec();
        let data = session.encode(job.key, job.choice.quantizer)?;
        let size = data.len();
        let taken = self.output.frame(Encoded {
            codec,
            kind: if job.key {
                FrameKind::Key
            } else {
                FrameKind::Delta
            },
            mark: None,
            region,
            data: Bytes::from(data),
        });
        self.rate.coded(job.choice, if taken { size } else { 0 });
        if taken {
            self.refusals = 0;
        } else {
            self.refusals += 1;
            self.repair = true;
            self.retry_at = Some(Instant::now() + retry_after(self.refusals));
        }
        Ok(())
    }
}
