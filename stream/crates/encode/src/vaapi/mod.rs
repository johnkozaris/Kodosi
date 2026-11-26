//! H.264 on Intel and AMD graphics through VA-API. The worker chooses each frame's quantizer for
//! the link, and sharpens a picture that settles with passes that encode it again. When the GPU
//! fails, software takes over.

mod headers;
mod session;
mod va;

use std::{ffi::c_int, path::PathBuf, rc::Rc, sync::OnceLock, time::Instant};

use bytes::Bytes;
use stream::{Codec, Encoded, FrameKind, Region};

use self::{session::Session, va::Display};
use crate::{
    Output, RETRY,
    pixels::{self, Pixels, Shared, Slot},
    rate::{self, Choice, Rate, SETTLED},
};

/// The first GPU that encodes H.264 as the encoder needs, and the entrypoint to use.
fn gpu() -> Option<&'static (PathBuf, c_int)> {
    static GPU: OnceLock<Option<(PathBuf, c_int)>> = OnceLock::new();
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
            let entrypoint = session::entrypoint(&Display::open(&node)?)?;
            Some((node, entrypoint))
        })
    })
    .as_ref()
}

/// Encodes the pictures that come into `shared` until the encoder stops. Returns false when the
/// GPU cannot encode them, with the picture it could not encode back in the slot.
pub(crate) fn work(bitrate: u32, output: &dyn Output, shared: &Shared) -> bool {
    let Some((display, entrypoint)) =
        gpu().and_then(|(node, entrypoint)| Some((Display::open(node)?, *entrypoint)))
    else {
        return false;
    };
    let now = Instant::now();
    let mut worker = Worker {
        output,
        shared,
        display: Rc::new(display),
        entrypoint,
        session: None,
        bitrate,
        rate: Rate::new(bitrate, now),
        region: None,
        drawn_at: now,
        repair: false,
        retry_at: None,
    };
    while let Some(job) = worker.next() {
        if let Err(picture) = worker.code(job) {
            give_back(shared, picture, worker.bitrate);
            return false;
        }
    }
    true
}

/// Leaves the slot as software needs it to take over: with the newest picture, as all new, and
/// with the bitrate.
fn give_back(shared: &Shared, picture: Option<Pixels>, bitrate: u32) {
    let mut slot = pixels::lock(shared);
    if let Some(picture) = picture
        && slot.picture.is_none()
    {
        slot.picture = Some(picture);
    }
    slot.changed = 1.0;
    slot.bitrate.get_or_insert(bitrate);
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
    display: Rc<Display>,
    entrypoint: c_int,
    session: Option<Session>,
    bitrate: u32,
    rate: Rate,
    /// What the picture in the session shows of the source.
    region: Option<Region>,
    drawn_at: Instant,
    /// Set when the next frame must be a keyframe.
    repair: bool,
    retry_at: Option<Instant>,
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
            self.repair |= std::mem::take(&mut slot.key);
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
        let shown = self.session.as_ref().map(Session::size);
        let (size, changed) = match (&slot.picture, shown) {
            (Some(picture), _) => ((picture.width, picture.height), Some(slot.changed)),
            (None, Some(size)) if repair_due => (size, Some(1.0)),
            (None, Some(size)) if self.retry_at.is_none() => (size, None),
            _ => return Err(self.retry_at.filter(|at| *at > now)),
        };
        let key = self.repair || shown != Some(size);
        let pixels = rate::pixels(size.0, size.1);
        let choice = if let Some(changed) = changed {
            self.rate.picture(changed, key, pixels, now).map_err(Some)?
        } else {
            let settled = self.drawn_at + SETTLED;
            if self.rate.sharp() {
                return Err(None);
            }
            if now < settled {
                return Err(Some(settled));
            }
            self.rate.pass(pixels, now).ok_or(None)?.map_err(Some)?
        };
        let picture = slot.picture.take();
        if picture.is_some() {
            slot.changed = 0.0;
        }
        if retry_due {
            self.retry_at = None;
        }
        self.repair = false;
        Ok(Job {
            picture,
            key,
            choice,
        })
    }

    /// Encodes a frame; on failure, gives back the new picture that it could not encode.
    fn code(&mut self, job: Job) -> Result<(), Option<Pixels>> {
        let Job {
            picture,
            key,
            choice,
        } = job;
        if let Some(new) = &picture
            && self.load(new).is_err()
        {
            return Err(picture);
        }
        let Some(Ok(data)) = self
            .session
            .as_mut()
            .map(|session| session.encode(key, choice.quantizer))
        else {
            return Err(picture);
        };
        if let Some(picture) = picture {
            self.region = picture.region;
            self.drawn_at = Instant::now();
            pixels::lock(self.shared).spare = picture.bgra;
        }
        let size = data.len();
        let taken = self.output.frame(Encoded {
            codec: Codec::H264,
            kind: if key {
                FrameKind::Key
            } else {
                FrameKind::Delta
            },
            mark: None,
            region: self.region,
            data: Bytes::from(data),
        });
        self.rate.coded(choice, if taken { size } else { 0 });
        if !taken {
            self.repair = true;
            self.retry_at = Some(Instant::now() + RETRY);
        }
        Ok(())
    }

    /// Puts a picture on the GPU, in a new session when its size changed.
    fn load(&mut self, picture: &Pixels) -> Result<(), va::Status> {
        let size = (picture.width, picture.height);
        if self
            .session
            .as_ref()
            .is_none_or(|session| session.size() != size)
        {
            self.session = None;
            self.session = Some(Session::start(&self.display, self.entrypoint, size)?);
        }
        self.session
            .as_ref()
            .map_or(Ok(()), |session| session.load(&picture.bgra))
    }
}
