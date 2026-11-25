//! `VideoToolbox` on Apple silicon's media engine. Metal converts each picture to NV12 on the
//! caller's thread; a worker thread feeds the encoder, because low-latency mode blocks while it
//! encodes.

mod convert;
mod session;
mod sps;

use std::{
    ptr::NonNull,
    sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError},
    thread::JoinHandle,
    time::Instant,
};

use objc2_core_foundation::CFRetained;
use objc2_core_video::{CVPixelBuffer, CVPixelBufferGetHeight, CVPixelBufferGetWidth};
use objc2_io_surface::IOSurfaceRef;
use stream::{Codec, Encoded, Format, FrameKind};

use self::{
    convert::Converter,
    session::{Coded, RECOVERY, REFINEMENT, Session},
};
use crate::{
    Output, Picture, Settings,
    refine::{Convergence, RETRY, frame_budget},
};

/// Pictures in the encoder at once; more only add latency.
const MOST_WAITING: usize = 2;

/// An NV12 picture that Metal filled.
#[derive(Clone)]
struct Buffer(CFRetained<CVPixelBuffer>);

#[expect(
    clippy::non_send_fields_in_send_ty,
    reason = "Core Foundation types are thread safe, but objc2 marks them all as !Send"
)]
// SAFETY: Core Video retains and releases pixel buffers atomically, and nothing writes to a
// picture after Metal filled it.
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

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Refinement {
    #[default]
    Idle,
    Asked,
    Clean,
}

struct Slot {
    picture: Option<Buffer>,
    repair: Repair,
    held: Vec<u64>,
    format: Format,
    bitrate: u32,
    changed: bool,
    refinement: Refinement,
    sharpening: Convergence,
    waiting: usize,
    retry_at: Option<Instant>,
    failed: bool,
    stop: bool,
}

/// What the caller, the worker and the encoder's callback share.
struct Gate {
    slot: Mutex<Slot>,
    ready: Condvar,
    output: Arc<dyn Output>,
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
        let refinement = marker & REFINEMENT != 0;
        let recovery = marker & RECOVERY != 0;
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
                data: coded.data,
            });
            (taken, size)
        });
        let mut slot = self.lock();
        slot.waiting = slot.waiting.saturating_sub(1);
        match sent {
            Some((true, size)) => {
                let budget = frame_budget(slot.bitrate);
                if refinement && slot.sharpening.refined(size, budget) {
                    slot.refinement = Refinement::Clean;
                }
            }
            Some((false, _)) => {
                slot.repair = Repair::Key;
                slot.retry_at = Some(Instant::now() + RETRY);
            }
            None if status != 0 => {
                drop(slot);
                self.fail(format!("the encoder failed: {status}"));
                self.ready.notify_one();
                return;
            }
            None if dropped && (recovery || !refinement) => {
                if recovery {
                    slot.repair = slot.repair.max(Repair::Recover);
                }
                slot.retry_at = Some(Instant::now() + RETRY);
            }
            None => {}
        }
        drop(slot);
        self.ready.notify_one();
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
        let gate = Arc::new(Gate {
            slot: Mutex::new(Slot {
                picture: None,
                repair: Repair::Key,
                held: Vec::new(),
                format: settings.format,
                bitrate: settings.bitrate,
                changed: false,
                refinement: Refinement::Idle,
                sharpening: Convergence::default(),
                waiting: 0,
                retry_at: None,
                failed: false,
                stop: false,
            }),
            ready: Condvar::new(),
            output,
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

    pub(crate) fn encode(&mut self, picture: Picture<'_>) {
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
                slot.picture = Some(Buffer(picture));
                slot.retry_at = None;
                slot.refinement = Refinement::Idle;
                slot.sharpening = Convergence::default();
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
        self.gate.change(|slot| {
            slot.bitrate = bitrate;
            slot.changed = true;
        });
    }

    pub(crate) fn set_format(&self, format: Format) {
        self.gate.change(|slot| {
            slot.format = format;
            slot.changed = true;
            slot.repair = Repair::Key;
            slot.held.clear();
        });
    }

    pub(crate) fn refine(&self) -> bool {
        self.gate.change(|slot| match slot.refinement {
            Refinement::Clean => true,
            Refinement::Asked => false,
            Refinement::Idle => {
                slot.refinement = Refinement::Asked;
                slot.picture.is_none()
            }
        })
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
    Encode(Buffer),
    Again(Buffer),
    Refine(Buffer),
}

struct Submission {
    job: Job,
    key: bool,
    recover: bool,
    held: Vec<u64>,
    format: Format,
    bitrate: u32,
    changed: bool,
}

fn next(gate: &Gate, last: Option<&Buffer>) -> Option<Submission> {
    let mut slot = gate.lock();
    loop {
        if slot.stop {
            return None;
        }
        let now = Instant::now();
        let room = slot.waiting < MOST_WAITING;
        let due = slot.retry_at.is_some_and(|at| at <= now);
        let job = if !room {
            None
        } else if let Some(picture) = slot.picture.take() {
            Some(Job::Encode(picture))
        } else if let Some(last) = last.filter(|_| due) {
            slot.retry_at = None;
            Some(Job::Again(last.clone()))
        } else if let Some(last) = last.filter(|_| slot.refinement == Refinement::Asked) {
            slot.refinement = Refinement::Idle;
            Some(Job::Refine(last.clone()))
        } else {
            None
        };
        if let Some(job) = job {
            let repair = std::mem::take(&mut slot.repair);
            let anchors = slot.format.anchors;
            let key = repair == Repair::Key || (repair == Repair::Recover && !anchors);
            let recover = repair == Repair::Recover && anchors;
            return Some(Submission {
                job,
                key,
                recover,
                held: std::mem::take(&mut slot.held),
                format: slot.format,
                bitrate: slot.bitrate,
                changed: std::mem::take(&mut slot.changed),
            });
        }
        slot = match slot.retry_at.filter(|_| room) {
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

fn work(gate: &Arc<Gate>) {
    let started = Instant::now();
    let mut session: Option<Session> = None;
    let mut last: Option<Buffer> = None;
    while let Some(submission) = next(gate, last.as_ref()) {
        let (Buffer(picture), marker) = match &submission.job {
            Job::Encode(picture) | Job::Again(picture) => (picture, 0),
            Job::Refine(picture) => (picture, REFINEMENT),
        };
        let (width, height) = (
            CVPixelBufferGetWidth(picture),
            CVPixelBufferGetHeight(picture),
        );
        let mut key = submission.key;
        if session.as_ref().is_none_or(|session| {
            (session.width, session.height, session.format) != (width, height, submission.format)
        }) {
            session = None;
            match Session::start(width, height, submission.format, submission.bitrate, gate) {
                Ok(started) => session = Some(started),
                Err(problem) => {
                    gate.fail(problem);
                    continue;
                }
            }
            key = true;
        } else if submission.changed
            && let Some(session) = &session
        {
            session.set_bitrate(submission.bitrate);
        }
        let Some(session) = &session else {
            continue;
        };
        let recover = submission.recover && !key;
        let marker = marker | if recover { RECOVERY } else { 0 };
        gate.change(|slot| slot.waiting += 1);
        let status = session.encode(
            picture,
            started.elapsed(),
            key,
            recover,
            &submission.held,
            marker,
        );
        if status != 0 {
            gate.change(|slot| {
                slot.waiting = slot.waiting.saturating_sub(1);
                let undone = if key {
                    Repair::Key
                } else if recover {
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
    use stream::{Codec, Encoded, Format, FrameKind};

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
        assert_eq!(frames.count(), count, "the encoder sent no frame in time");
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

    fn encode(
        format: Format,
        pictures: usize,
        before: impl Fn(&Encoder, &[Encoded]),
    ) -> Vec<Encoded> {
        let frames = Arc::new(Frames::default());
        let mut encoder = Encoder::new(
            Settings {
                format,
                bitrate: 4_000_000,
            },
            Arc::clone(&frames) as Arc<dyn Output>,
        );
        for index in 0..pictures {
            before(&encoder, &frames.0.lock().unwrap());
            encoder.encode(Picture::Pixels {
                bgra: &picture(index * 8),
                width: WIDTH,
                height: HEIGHT,
            });
            wait_for(&frames, index + 1);
        }
        drop(encoder);
        std::mem::take(&mut *frames.0.lock().unwrap())
    }

    #[test]
    fn a_captured_nv12_picture_is_encoded_as_it_is() {
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
        encoder.encode(Picture::Nv12(CFRetained::as_ptr(&buffer).as_ptr().cast()));
        wait_for(&frames, 1);
        let first = frames.0.lock().unwrap()[0].clone();
        assert_eq!(first.kind, FrameKind::Key);
        assert!(units(&first.data, Codec::H264).contains(&7));
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
            let frames = encode(format, 3, |_, _| {});
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
    fn a_viewer_that_missed_frames_recovers_from_an_anchor_on_a_held_mark() {
        let format = Format {
            codec: Codec::H264,
            anchors: true,
        };
        let frames = encode(format, 4, |encoder, sent| {
            if sent.len() == 3 {
                let marks: Vec<u64> = sent.iter().filter_map(|frame| frame.mark).collect();
                assert!(!marks.is_empty(), "the stream marks no references");
                encoder.held(&marks);
                encoder.recover();
            }
        });
        assert_eq!(frames[3].kind, FrameKind::Anchor);
    }
}
