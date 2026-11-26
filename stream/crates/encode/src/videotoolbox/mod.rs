//! `VideoToolbox` on Apple silicon's media engine. Metal converts each picture to NV12 on the
//! caller's thread; a worker thread feeds the encoder, because low-latency mode blocks while it
//! encodes. The worker chooses each frame's quantizer, and sharpens a picture that settles with
//! passes that encode it again.

mod convert;
mod session;
mod sps;

use std::{
    collections::VecDeque,
    ptr::NonNull,
    sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError},
    thread::JoinHandle,
    time::Instant,
};

use objc2_core_foundation::CFRetained;
use objc2_core_video::{CVPixelBuffer, CVPixelBufferGetHeight, CVPixelBufferGetWidth};
use objc2_io_surface::IOSurfaceRef;
use stream::{Codec, Encoded, Format, FrameKind, Region};

use self::{
    convert::Converter,
    session::{Coded, RECOVERY, Session, Shape},
};
use crate::{
    Output, Picture, RETRY, Settings,
    rate::{self, Choice, Rate, SETTLED, SHARP},
    retry_after,
};

/// Pictures in the encoder at once; more only add latency.
const MOST_WAITING: usize = 2;
/// A marker's low bit holds `RECOVERY`; the bits above number the picture.
const NUMBER_SHIFT: u32 = 1;

/// An NV12 picture and the part of the source that it shows.
#[derive(Clone)]
struct Buffer {
    picture: CFRetained<CVPixelBuffer>,
    region: Option<Region>,
}

#[expect(
    clippy::non_send_fields_in_send_ty,
    reason = "Core Foundation types are thread safe, but objc2 marks them all as !Send"
)]
// SAFETY: Core Video retains and releases pixel buffers atomically, and nothing writes to a
// picture once the encoder has it.
unsafe impl Send for Buffer {}

pub(crate) fn encodable() -> Vec<Codec> {
    static CODECS: OnceLock<Vec<Codec>> = OnceLock::new();
    CODECS
        .get_or_init(|| {
            let mut codecs = vec![Codec::H264];
            if Session::supports(Codec::Hevc) {
                codecs.push(Codec::Hevc);
            }
            codecs
        })
        .clone()
}

/// How the next picture repairs the stream; a keyframe repairs more than an anchor.
#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Repair {
    #[default]
    None,
    Recover,
    Key,
}

/// What the encoder was asked to make of a picture.
struct Asked {
    number: usize,
    region: Option<Region>,
    choice: Option<Choice>,
}

struct Slot {
    picture: Option<Buffer>,
    /// The share of the source that changed since the worker last took a picture.
    changed: f64,
    /// When the last picture came.
    drawn_at: Instant,
    /// What the encoder was asked for each picture in it, in order.
    asked: VecDeque<Asked>,
    repair: Repair,
    held: Vec<u64>,
    format: Format,
    rate: Rate,
    waiting: usize,
    retry_at: Option<Instant>,
    refusals: u32,
    failed: bool,
    stop: bool,
}

/// What the caller, the worker and the encoder's callback share.
struct Gate {
    slot: Mutex<Slot>,
    ready: Condvar,
    output: Arc<dyn Output>,
    /// Makes each picture a sharp keyframe, with no passes.
    stills: bool,
}

impl Gate {
    fn lock(&self) -> MutexGuard<'_, Slot> {
        self.slot.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn change<R>(&self, change: impl FnOnce(&mut Slot) -> R) -> R {
        let result = change(&mut self.lock());
        self.ready.notify_one();
        result
    }

    fn fail(&self, problem: String) {
        let first = !std::mem::replace(&mut self.lock().failed, true);
        if first {
            self.output.failed(problem);
        }
    }

    /// Runs on the encoder's thread for each submitted picture.
    fn encoded(
        &self,
        marker: usize,
        status: i32,
        dropped: bool,
        sample: Option<Coded>,
        codec: Codec,
    ) {
        let recovery = marker & RECOVERY != 0;
        let (region, choice) = self.answered(marker >> NUMBER_SHIFT);
        let sent = sample.map(|coded| {
            let size = coded.data.len();
            let kind = if coded.key {
                FrameKind::Key
            } else if recovery {
                FrameKind::Anchor
            } else {
                FrameKind::Delta
            };
            let taken = self.output.frame(Encoded {
                codec,
                kind,
                mark: coded.mark,
                region,
                data: coded.data,
            });
            (taken, size)
        });
        let mut slot = self.lock();
        slot.waiting = slot.waiting.saturating_sub(1);
        if let Some(choice) = choice {
            let carried = sent.filter(|(taken, _)| *taken).map_or(0, |(_, size)| size);
            slot.rate.coded(choice, carried);
        }
        match sent {
            Some((false, _)) => {
                slot.refusals += 1;
                slot.repair = Repair::Key;
                slot.retry_at = Some(Instant::now() + retry_after(slot.refusals));
            }
            None if status != 0 => {
                drop(slot);
                self.fail(format!("the encoder failed: {status}"));
                self.ready.notify_one();
                return;
            }
            None if dropped => {
                if recovery {
                    slot.repair = slot.repair.max(Repair::Recover);
                }
                slot.retry_at = Some(Instant::now() + RETRY);
            }
            Some((true, _)) => slot.refusals = 0,
            None => {}
        }
        drop(slot);
        self.ready.notify_one();
    }

    /// Takes what the encoder was asked for a picture; the pictures before it, which the encoder
    /// skipped, cost nothing.
    fn answered(&self, number: usize) -> (Option<Region>, Option<Choice>) {
        let mut guard = self.lock();
        let slot = &mut *guard;
        let Some(at) = slot.asked.iter().position(|asked| asked.number == number) else {
            return (None, None);
        };
        for skipped in slot.asked.drain(..at) {
            if let Some(choice) = skipped.choice {
                slot.rate.coded(choice, 0);
            }
        }
        let asked = slot.asked.pop_front();
        drop(guard);
        asked.map_or((None, None), |asked| (asked.region, asked.choice))
    }
}

pub(crate) struct Encoder {
    gate: Arc<Gate>,
    /// Made with the first BGRA picture, since NV12 pictures need none.
    converter: Option<Converter>,
    worker: Option<JoinHandle<()>>,
}

impl Encoder {
    pub(crate) fn new(settings: Settings, output: Arc<dyn Output>) -> Self {
        Self::start(settings.format, settings.bitrate, output, false)
    }

    pub(crate) fn stills(format: Format, output: Arc<dyn Output>) -> Self {
        Self::start(format, 0, output, true)
    }

    fn start(format: Format, bitrate: u32, output: Arc<dyn Output>, stills: bool) -> Self {
        let now = Instant::now();
        let gate = Arc::new(Gate {
            slot: Mutex::new(Slot {
                picture: None,
                changed: 0.0,
                drawn_at: now,
                asked: VecDeque::new(),
                repair: Repair::Key,
                held: Vec::new(),
                format,
                rate: Rate::new(bitrate, now),
                waiting: 0,
                retry_at: None,
                refusals: 0,
                failed: false,
                stop: false,
            }),
            ready: Condvar::new(),
            output,
            stills,
        });
        let working = Arc::clone(&gate);
        let worker = std::thread::Builder::new()
            .name("kodosi-encoder".to_owned())
            .spawn(move || work(&working))
            .ok();
        Self {
            gate,
            converter: None,
            worker,
        }
    }

    pub(crate) fn encode(&mut self, picture: Picture<'_>, region: Option<Region>, changed: f64) {
        let nv12 = match picture {
            Picture::Nv12(buffer) => {
                // SAFETY: the caller passes a valid `CVPixelBufferRef`, which this retains.
                let Some(buffer) = NonNull::new(buffer.cast::<CVPixelBuffer>())
                    .map(|buffer| unsafe { CFRetained::retain(buffer) })
                else {
                    return;
                };
                Ok(buffer)
            }
            Picture::Surface(surface) => {
                let Some(converter) = self.converter() else {
                    return;
                };
                // SAFETY: the caller passes an `IOSurfaceRef` that is valid during this call.
                let Some(surface) = (unsafe { surface.cast::<IOSurfaceRef>().as_ref() }) else {
                    return;
                };
                let (width, height) = (surface.width() & !1, surface.height() & !1);
                if width < 2 || height < 2 {
                    return;
                }
                converter.surface(surface, width, height)
            }
            Picture::Pixels {
                bgra,
                width,
                height,
            } => {
                let (even_width, even_height) = (width & !1, height & !1);
                if even_width < 2 || even_height < 2 {
                    return;
                }
                let Some(converter) = self.converter() else {
                    return;
                };
                converter.pixels(bgra, even_width, even_height, width * 4)
            }
        };
        match nv12 {
            Ok(picture) => self.gate.change(|slot| {
                slot.picture = Some(Buffer { picture, region });
                slot.changed = (slot.changed + changed).min(1.0);
                slot.drawn_at = Instant::now();
                slot.retry_at = None;
            }),
            Err(problem) => self.gate.fail(problem),
        }
    }

    fn converter(&mut self) -> Option<&mut Converter> {
        if self.converter.is_none() {
            match Converter::new() {
                Ok(converter) => self.converter = Some(converter),
                Err(problem) => self.gate.fail(problem),
            }
        }
        self.converter.as_mut()
    }

    pub(crate) fn want_key(&self) {
        self.gate.change(|slot| slot.repair = Repair::Key);
    }

    pub(crate) fn recover(&self) {
        self.gate
            .change(|slot| slot.repair = slot.repair.max(Repair::Recover));
    }

    pub(crate) fn held(&self, marks: &[u64]) {
        self.gate.change(|slot| slot.held.extend_from_slice(marks));
    }

    pub(crate) fn set_bitrate(&self, bitrate: u32) {
        self.gate
            .change(|slot| slot.rate.set_bitrate(bitrate, Instant::now()));
    }

    pub(crate) fn set_format(&self, format: Format) {
        self.gate.change(|slot| {
            slot.format = format;
            slot.repair = Repair::Key;
            slot.held.clear();
        });
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        self.gate.change(|slot| slot.stop = true);
        if let Some(worker) = self.worker.take() {
            drop(worker.join());
        }
    }
}

enum Job {
    /// A new picture, which becomes the last one.
    Encode(Buffer),
    /// The last picture again, to repair the stream or to sharpen it.
    Again(Buffer),
}

struct Submission {
    job: Job,
    /// The picture's size or the stream's format differs from the session's.
    restart: bool,
    key: bool,
    recover: bool,
    held: Vec<u64>,
    format: Format,
    quantizer: u8,
    choice: Option<Choice>,
}

fn next(gate: &Gate, last: Option<&Buffer>, shape: Option<Shape>) -> Option<Submission> {
    let mut slot = gate.lock();
    loop {
        if slot.stop {
            return None;
        }
        let now = Instant::now();
        let wake = if slot.waiting < MOST_WAITING {
            match plan(&mut slot, gate.stills, last, shape, now) {
                Ok(submission) => return Some(submission),
                Err(wake) => wake,
            }
        } else {
            None
        };
        slot = match wake {
            Some(at) => {
                gate.ready
                    .wait_timeout(slot, at.saturating_duration_since(now))
                    .unwrap_or_else(PoisonError::into_inner)
                    .0
            }
            None => gate
                .ready
                .wait(slot)
                .unwrap_or_else(PoisonError::into_inner),
        };
    }
}

/// Plans the next frame: a new picture when the link has room for it, else the last picture
/// again to repair the stream, else a pass once the picture settles. Otherwise tells when to
/// look again, or `None` to wait for a change.
fn plan(
    slot: &mut Slot,
    stills: bool,
    last: Option<&Buffer>,
    shape: Option<Shape>,
    now: Instant,
) -> Result<Submission, Option<Instant>> {
    let retry_due = slot.retry_at.is_some_and(|at| at <= now);
    let repair_due = retry_due || (slot.retry_at.is_none() && slot.repair != Repair::None);
    let (buffer, changed) = match (&slot.picture, last) {
        (Some(picture), _) => (picture.clone(), Some(slot.changed)),
        (None, Some(last)) if repair_due => (last.clone(), Some(1.0)),
        (None, Some(last)) if !stills && slot.retry_at.is_none() => (last.clone(), None),
        _ => return Err(slot.retry_at.filter(|at| *at > now)),
    };
    let (width, height) = (
        CVPixelBufferGetWidth(&buffer.picture),
        CVPixelBufferGetHeight(&buffer.picture),
    );
    let restart = shape != Some((width, height, slot.format));
    let repair = slot.repair;
    let key = stills
        || restart
        || repair == Repair::Key
        || (repair == Repair::Recover && !slot.format.anchors);
    let recover = !key && repair == Repair::Recover;
    let pixels = rate::pixels(width, height);
    let choice = match changed {
        _ if stills => None,
        Some(changed) => Some(
            slot.rate
                .picture(changed, key || recover, pixels, now)
                .map_err(Some)?,
        ),
        None => {
            let settled = slot.drawn_at + SETTLED;
            if slot.rate.sharp() {
                return Err(None);
            }
            if now < settled {
                return Err(Some(settled));
            }
            Some(slot.rate.pass(pixels, now).ok_or(None)?.map_err(Some)?)
        }
    };
    let job = if slot.picture.take().is_some() {
        slot.changed = 0.0;
        Job::Encode(buffer)
    } else {
        Job::Again(buffer)
    };
    if retry_due {
        slot.retry_at = None;
    }
    slot.repair = Repair::None;
    Ok(Submission {
        job,
        restart,
        key,
        recover,
        held: std::mem::take(&mut slot.held),
        format: slot.format,
        quantizer: choice.map_or(SHARP, |choice| choice.quantizer),
        choice,
    })
}

fn work(gate: &Arc<Gate>) {
    let started = Instant::now();
    let mut session: Option<Session> = None;
    let mut last: Option<Buffer> = None;
    let mut submitted: usize = 0;
    while let Some(submission) = next(gate, last.as_ref(), session.as_ref().map(Session::shape)) {
        let (Job::Encode(buffer) | Job::Again(buffer)) = &submission.job;
        let picture = &buffer.picture;
        if submission.restart {
            session = None;
            let (width, height) = (
                CVPixelBufferGetWidth(picture),
                CVPixelBufferGetHeight(picture),
            );
            match Session::start(width, height, submission.format, gate) {
                Ok(started) => session = Some(started),
                Err(problem) => {
                    if let Some(choice) = submission.choice {
                        gate.change(|slot| slot.rate.coded(choice, 0));
                    }
                    gate.fail(problem);
                    continue;
                }
            }
        }
        let Some(session) = &session else {
            continue;
        };
        submitted = submitted.wrapping_add(1) & (usize::MAX >> NUMBER_SHIFT);
        let number = submitted;
        let marker = number << NUMBER_SHIFT | if submission.recover { RECOVERY } else { 0 };
        gate.change(|slot| {
            slot.waiting += 1;
            slot.asked.push_back(Asked {
                number,
                region: buffer.region,
                choice: submission.choice,
            });
        });
        let status = session.encode(
            picture,
            started.elapsed(),
            submission.key,
            submission.recover,
            &submission.held,
            submission.quantizer,
            marker,
        );
        if status != 0 {
            gate.change(|slot| {
                slot.waiting = slot.waiting.saturating_sub(1);
                if let Some(at) = slot.asked.iter().position(|asked| asked.number == number)
                    && let Some(Asked {
                        choice: Some(choice),
                        ..
                    }) = slot.asked.remove(at)
                {
                    slot.rate.coded(choice, 0);
                }
                let undone = if submission.key {
                    Repair::Key
                } else if submission.recover {
                    Repair::Recover
                } else {
                    Repair::None
                };
                slot.repair = slot.repair.max(undone);
            });
            gate.fail(format!("the encoder refused a picture: {status}"));
        }
        if let Job::Encode(picture) = submission.job {
            last = Some(picture);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        thread,
        time::{Duration, Instant},
    };

    use objc2_core_foundation::CFRetained;
    use objc2_core_video::{CVPixelBufferCreate, kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange};
    use stream::{Codec, Encoded, Format, FrameKind, Region};

    use super::Encoder;
    use crate::{Output, Picture, Settings};

    const WIDTH: usize = 320;
    const HEIGHT: usize = 240;

    #[derive(Default)]
    struct Frames(Mutex<Vec<Encoded>>);

    impl Output for Frames {
        fn frame(&self, frame: Encoded) -> bool {
            self.0.lock().unwrap().push(frame);
            true
        }

        fn failed(&self, problem: String) {
            panic!("{problem}");
        }
    }

    impl Frames {
        fn count(&self) -> usize {
            self.0.lock().unwrap().len()
        }
    }

    fn picture(shift: usize) -> Vec<u8> {
        (0..WIDTH * HEIGHT)
            .flat_map(|at| {
                let (x, y) = (
                    (at % WIDTH + shift).to_le_bytes()[0],
                    (at / WIDTH).to_le_bytes()[0],
                );
                [x, y, x ^ y, 255]
            })
            .collect()
    }

    fn wait_for(frames: &Frames, count: usize) {
        let until = Instant::now() + Duration::from_secs(3);
        while frames.count() < count && Instant::now() < until {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(frames.count() >= count, "the encoder sent no frame in time");
    }

    fn units(data: &[u8], codec: Codec) -> Vec<u8> {
        data.windows(5)
            .filter(|window| window[..4] == [0, 0, 0, 1])
            .map(|window| match codec {
                Codec::H264 => window[4] & 0x1f,
                Codec::Hevc => (window[4] >> 1) & 0x3f,
            })
            .collect()
    }

    fn start(format: Format, bitrate: u32) -> (Arc<Frames>, Encoder) {
        let frames = Arc::new(Frames::default());
        let encoder = Encoder::new(
            Settings { format, bitrate },
            Arc::clone(&frames) as Arc<dyn Output>,
        );
        (frames, encoder)
    }

    fn draw(encoder: &mut Encoder, frames: &Frames, pictures: usize) {
        for index in 0..pictures {
            encoder.encode(
                Picture::Pixels {
                    bgra: &picture(index * 8),
                    width: WIDTH,
                    height: HEIGHT,
                },
                None,
                1.0,
            );
            wait_for(frames, frames.count() + 1);
        }
    }

    /// Waits until the encoder has sent nothing for a while.
    fn quiet(frames: &Frames) {
        let until = Instant::now() + Duration::from_secs(5);
        let mut seen = frames.count();
        loop {
            thread::sleep(Duration::from_millis(300));
            let now = frames.count();
            if now == seen || Instant::now() > until {
                return;
            }
            seen = now;
        }
    }

    fn encode(format: Format, pictures: usize) -> Vec<Encoded> {
        let (frames, mut encoder) = start(format, 4_000_000);
        draw(&mut encoder, &frames, pictures);
        drop(encoder);
        std::mem::take(&mut *frames.0.lock().unwrap())
    }

    #[test]
    fn a_captured_nv12_picture_is_encoded_as_it_is_with_the_region_it_shows() {
        let mut raw = std::ptr::null_mut();
        // SAFETY: the out pointer is valid.
        let status = unsafe {
            CVPixelBufferCreate(
                None,
                WIDTH,
                HEIGHT,
                kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
                None,
                std::ptr::NonNull::from(&mut raw),
            )
        };
        assert_eq!(status, 0);
        let buffer = std::ptr::NonNull::new(raw).unwrap();
        // SAFETY: the buffer was just created with a +1 retain count.
        let buffer = unsafe { CFRetained::from_raw(buffer) };
        let frames = Arc::new(Frames::default());
        let mut encoder = Encoder::new(
            Settings {
                format: Format::default(),
                bitrate: 4_000_000,
            },
            Arc::clone(&frames) as Arc<dyn Output>,
        );
        let region = Region {
            x: 100.0,
            y: 50.0,
            width: 160.0,
            height: 120.0,
        };
        encoder.encode(
            Picture::Nv12(CFRetained::as_ptr(&buffer).as_ptr().cast()),
            Some(region),
            1.0,
        );
        wait_for(&frames, 1);
        let first = frames.0.lock().unwrap()[0].clone();
        assert_eq!(first.kind, FrameKind::Key);
        assert!(units(&first.data, Codec::H264).contains(&7));
        assert_eq!(first.region, Some(region));
    }

    #[test]
    fn each_codec_starts_with_a_keyframe_that_carries_its_parameter_sets() {
        for (codec, sets, key, delta) in [
            (Codec::H264, &[7, 8][..], 5, 1),
            (Codec::Hevc, &[32, 33, 34][..], 19, 1),
        ] {
            let format = Format {
                codec,
                anchors: false,
            };
            let frames = encode(format, 3);
            let first = units(&frames[0].data, codec);
            assert_eq!(frames[0].kind, FrameKind::Key, "{codec:?}");
            assert!(
                sets.iter().all(|set| first.contains(set)),
                "{codec:?}: {first:?}"
            );
            assert!(
                first.contains(&key) || first.contains(&20),
                "{codec:?}: {first:?}"
            );
            for frame in &frames[1..] {
                assert_eq!(frame.kind, FrameKind::Delta, "{codec:?}");
                assert!(units(&frame.data, codec).contains(&delta), "{codec:?}");
            }
        }
    }

    #[test]
    fn a_viewer_that_missed_frames_recovers_at_once_from_an_anchor_on_a_held_mark() {
        let format = Format {
            codec: Codec::H264,
            anchors: true,
        };
        let (frames, mut encoder) = start(format, 4_000_000);
        draw(&mut encoder, &frames, 3);
        quiet(&frames);
        let sent = frames.count();
        let marks: Vec<u64> = frames
            .0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|frame| frame.mark)
            .collect();
        assert!(!marks.is_empty(), "the stream marks no references");
        encoder.held(&marks);
        encoder.recover();
        wait_for(&frames, sent + 1);
        assert_eq!(frames.0.lock().unwrap()[sent].kind, FrameKind::Anchor);
    }

    #[test]
    fn a_keyframe_asked_for_while_the_picture_is_still_comes_at_once() {
        let (frames, mut encoder) = start(Format::default(), 20_000_000);
        draw(&mut encoder, &frames, 1);
        quiet(&frames);
        let sent = frames.count();
        encoder.want_key();
        wait_for(&frames, sent + 1);
        assert_eq!(frames.0.lock().unwrap()[sent].kind, FrameKind::Key);
    }

    /// Refuses the first frame, as a full link does.
    #[derive(Default)]
    struct Refusing(Mutex<Vec<FrameKind>>);

    impl Output for Refusing {
        fn frame(&self, frame: Encoded) -> bool {
            let mut kinds = self.0.lock().unwrap();
            kinds.push(frame.kind);
            kinds.len() > 1
        }

        fn failed(&self, problem: String) {
            panic!("{problem}");
        }
    }

    #[test]
    fn a_still_that_the_link_refused_comes_again() {
        let refusing = Arc::new(Refusing::default());
        let mut encoder =
            Encoder::stills(Format::default(), Arc::clone(&refusing) as Arc<dyn Output>);
        encoder.encode(
            Picture::Pixels {
                bgra: &picture(0),
                width: WIDTH,
                height: HEIGHT,
            },
            None,
            1.0,
        );
        let until = Instant::now() + Duration::from_secs(3);
        while refusing.0.lock().unwrap().len() < 2 && Instant::now() < until {
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            *refusing.0.lock().unwrap(),
            [FrameKind::Key, FrameKind::Key]
        );
    }

    /// Refuses every frame, as a link that stays full does.
    #[derive(Default)]
    struct Full(Mutex<Vec<Instant>>);

    impl Output for Full {
        fn frame(&self, _: Encoded) -> bool {
            self.0.lock().unwrap().push(Instant::now());
            false
        }

        fn failed(&self, problem: String) {
            panic!("{problem}");
        }
    }

    #[test]
    fn a_link_that_stays_full_gets_keyframes_less_and_less_often() {
        let full = Arc::new(Full::default());
        let mut encoder = Encoder::stills(Format::default(), Arc::clone(&full) as Arc<dyn Output>);
        encoder.encode(
            Picture::Pixels {
                bgra: &picture(0),
                width: WIDTH,
                height: HEIGHT,
            },
            None,
            1.0,
        );
        let until = Instant::now() + Duration::from_secs(3);
        while full.0.lock().unwrap().len() < 4 && Instant::now() < until {
            thread::sleep(Duration::from_millis(5));
        }
        let tries = full.0.lock().unwrap().clone();
        assert!(tries.len() >= 4, "the encoder tries again");
        let (first, third) = (tries[1] - tries[0], tries[3] - tries[2]);
        assert!(third >= first * 2, "{first:?} then {third:?}");
    }

    #[test]
    fn passes_sharpen_a_still_picture_and_stop_once_it_is_sharp() {
        let (frames, mut encoder) = start(Format::default(), 300_000);
        draw(&mut encoder, &frames, 1);
        quiet(&frames);
        assert!(frames.count() > 1, "no passes");
        assert!(encoder.gate.lock().rate.sharp());
        let kinds: Vec<FrameKind> = frames
            .0
            .lock()
            .unwrap()
            .iter()
            .map(|frame| frame.kind)
            .collect();
        assert!(
            kinds[1..].iter().all(|kind| *kind == FrameKind::Delta),
            "{kinds:?}"
        );
    }
}
