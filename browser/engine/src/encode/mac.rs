use std::{
    ffi::c_void,
    ptr,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Instant,
};

use bytes::Bytes;
use shared_browser::link::{EncodedFrame, FrameKind, FromEngine};

use super::{Convergence, FRAME_RATE, REDRAW_AFTER_MS, frame_budget, sps};
use crate::{
    cf::{
        CFArrayGetCount, CFArrayGetValueAtIndex, CFDictionaryGetValue, CFNumberGetValue, CFRelease,
        IOSurfaceGetHeight, IOSurfaceGetWidth, NUMBER_I64, Owned, Ref, array, dictionary,
        kCFBooleanFalse, kCFBooleanTrue, number_f64, number_i64,
    },
    engine,
    link::Link,
};

/// Maximum in-flight frames allowed in the encoder. Excess frames are dropped and a redraw is
/// scheduled once capacity frees up so viewers still receive the latest page state.
const MOST_WAITING: usize = 2;
const START_CODE: [u8; 4] = [0, 0, 0, 1];
const SEQUENCE_PARAMETERS: u8 = 7;

type OSStatus = i32;

#[repr(C)]
#[derive(Clone, Copy)]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}

const TIME_VALID: u32 = 1;
const NO_TIME: CMTime = CMTime {
    value: 0,
    timescale: 0,
    flags: 0,
    epoch: 0,
};
const H264: u32 = u32::from_be_bytes(*b"avc1");
const NV12: u32 = u32::from_be_bytes(*b"420v");
const BGRA: u32 = u32::from_be_bytes(*b"BGRA");
const MICROSECONDS: i32 = 1_000_000;
const KEY_INTERVAL: i64 = 3600;
/// Multiplier over average bitrate allowed for 1-second bursts in `DataRateLimits`.
const BURST: f64 = 1.5;
const FRAME_DROPPED: u32 = 1 << 1;
const REFINEMENT: usize = 1;
const RECOVERY: usize = 2;

type OutputCallback = unsafe extern "C" fn(*mut c_void, *mut c_void, OSStatus, u32, *mut c_void);

#[link(name = "VideoToolbox", kind = "framework")]
unsafe extern "C" {
    fn VTCompressionSessionCreate(
        allocator: Ref,
        width: i32,
        height: i32,
        codec: u32,
        encoder_specification: Ref,
        source_attributes: Ref,
        data_allocator: Ref,
        callback: Option<OutputCallback>,
        refcon: *mut c_void,
        session: *mut *mut c_void,
    ) -> OSStatus;
    fn VTCompressionSessionEncodeFrame(
        session: *mut c_void,
        image: *mut c_void,
        time: CMTime,
        duration: CMTime,
        properties: Ref,
        frame_refcon: *mut c_void,
        flags: *mut u32,
    ) -> OSStatus;
    fn VTCompressionSessionCompleteFrames(session: *mut c_void, until: CMTime) -> OSStatus;
    fn VTCompressionSessionInvalidate(session: *mut c_void);
    fn VTCompressionSessionGetPixelBufferPool(session: *mut c_void) -> *mut c_void;
    fn VTSessionSetProperty(session: *mut c_void, key: Ref, value: Ref) -> OSStatus;
    fn VTPixelTransferSessionCreate(allocator: Ref, session: *mut *mut c_void) -> OSStatus;
    fn VTPixelTransferSessionTransferImage(
        session: *mut c_void,
        from: *mut c_void,
        to: *mut c_void,
    ) -> OSStatus;
    fn VTPixelTransferSessionInvalidate(session: *mut c_void);
    static kVTCompressionPropertyKey_RealTime: Ref;
    static kVTCompressionPropertyKey_ProfileLevel: Ref;
    static kVTCompressionPropertyKey_AllowFrameReordering: Ref;
    static kVTCompressionPropertyKey_AverageBitRate: Ref;
    static kVTCompressionPropertyKey_DataRateLimits: Ref;
    static kVTCompressionPropertyKey_ExpectedFrameRate: Ref;
    static kVTCompressionPropertyKey_MaxKeyFrameInterval: Ref;
    static kVTProfileLevel_H264_ConstrainedBaseline_AutoLevel: Ref;
    static kVTEncodeFrameOptionKey_ForceKeyFrame: Ref;
    static kVTVideoEncoderSpecification_EnableLowLatencyRateControl: Ref;
    static kVTCompressionPropertyKey_EnableLTR: Ref;
    static kVTEncodeFrameOptionKey_AcknowledgedLTRTokens: Ref;
    static kVTEncodeFrameOptionKey_ForceLTRRefresh: Ref;
    static kVTSampleAttachmentKey_RequireLTRAcknowledgementToken: Ref;
}

#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    fn CVPixelBufferCreateWithIOSurface(
        allocator: Ref,
        surface: *mut c_void,
        attributes: Ref,
        buffer: *mut *mut c_void,
    ) -> i32;
    fn CVPixelBufferCreate(
        allocator: Ref,
        width: usize,
        height: usize,
        format: u32,
        attributes: Ref,
        buffer: *mut *mut c_void,
    ) -> i32;
    fn CVPixelBufferPoolCreatePixelBuffer(
        allocator: Ref,
        pool: *mut c_void,
        buffer: *mut *mut c_void,
    ) -> i32;
    fn CVPixelBufferLockBaseAddress(buffer: *mut c_void, flags: u64) -> i32;
    fn CVPixelBufferUnlockBaseAddress(buffer: *mut c_void, flags: u64) -> i32;
    fn CVPixelBufferGetBaseAddress(buffer: *mut c_void) -> *mut u8;
    fn CVPixelBufferGetBytesPerRow(buffer: *mut c_void) -> usize;
    static kCVPixelBufferPixelFormatTypeKey: Ref;
    static kCVPixelBufferWidthKey: Ref;
    static kCVPixelBufferHeightKey: Ref;
    static kCVPixelBufferIOSurfacePropertiesKey: Ref;
}

#[link(name = "CoreMedia", kind = "framework")]
unsafe extern "C" {
    fn CMSampleBufferGetDataBuffer(sample: *mut c_void) -> *mut c_void;
    fn CMSampleBufferGetFormatDescription(sample: *mut c_void) -> *mut c_void;
    fn CMSampleBufferGetSampleAttachmentsArray(sample: *mut c_void, create: u8) -> Ref;
    fn CMBlockBufferGetDataLength(buffer: *mut c_void) -> usize;
    fn CMBlockBufferCopyDataBytes(
        buffer: *mut c_void,
        offset: usize,
        length: usize,
        into: *mut c_void,
    ) -> OSStatus;
    fn CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
        description: *mut c_void,
        index: usize,
        set: *mut *const u8,
        size: *mut usize,
        count: *mut usize,
        header_length: *mut i32,
    ) -> OSStatus;
    static kCMSampleAttachmentKey_NotSync: Ref;
}

/// Shared state passed to the `VideoToolbox` callback. Scoped per session so an ending session
/// takes its in-flight `waiting` count with it.
struct Output {
    window: i64,
    link: Link,
    still: bool,
    waiting: AtomicUsize,
    failed: AtomicBool,
    want_key: AtomicBool,
    want_anchor: AtomicBool,
    missed: AtomicBool,
    clean: AtomicBool,
    sharpening: Mutex<Convergence>,
    budget: AtomicUsize,
}

struct Session {
    raw: *mut c_void,
    output: Arc<Output>,
    width: usize,
    height: usize,
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: `self.raw` is valid; completing frames flushes all pending callbacks before the
        // session and its `Output` refcon are released.
        unsafe {
            VTCompressionSessionCompleteFrames(self.raw, NO_TIME);
            VTCompressionSessionInvalidate(self.raw);
            CFRelease(self.raw);
            drop(Arc::from_raw(Arc::as_ptr(&self.output)));
        }
    }
}

pub(crate) struct Encoder {
    window: i64,
    link: Link,
    bitrate: u32,
    /// When true, encodes standalone still snapshots for the layer; failures are ignored
    /// because views keep the previous still.
    still: bool,
    want_key: bool,
    recover: bool,
    /// LTR tokens acknowledged by all viewers, attached to the next frame.
    held: Vec<u64>,
    started: Instant,
    session: Option<Session>,
    transfer: *mut c_void,
    last: Option<Owned>,
}

impl Encoder {
    pub(crate) fn new(window: i64, bitrate: u32, link: Link) -> Self {
        let mut transfer = ptr::null_mut();
        // SAFETY: the out pointer is valid; a failure leaves it null.
        unsafe { VTPixelTransferSessionCreate(ptr::null(), &raw mut transfer) };
        Self {
            window,
            link,
            bitrate,
            still: false,
            want_key: true,
            recover: false,
            held: Vec::new(),
            started: Instant::now(),
            session: None,
            transfer,
            last: None,
        }
    }

    pub(crate) fn stills(window: i64, bitrate: u32, link: Link) -> Self {
        let mut encoder = Self::new(window, bitrate, link);
        encoder.still = true;
        encoder
    }

    pub(crate) fn want_key(&mut self) {
        self.want_key = true;
    }

    /// Requests an LTR refresh frame referencing an acknowledged mark, falling back to a
    /// keyframe if no mark is held by all viewers.
    pub(crate) fn recover(&mut self) {
        self.recover = true;
    }

    pub(crate) fn held(&mut self, marks: &[u64]) {
        self.held.extend_from_slice(marks);
    }

    pub(crate) fn set_bitrate(&mut self, bitrate: u32) {
        self.bitrate = bitrate;
        if let Some(session) = &self.session {
            set_bitrate(session.raw, bitrate);
            session
                .output
                .budget
                .store(frame_budget(bitrate), Ordering::Release);
        }
    }

    pub(crate) fn refine(&mut self) -> bool {
        let (Some(session), Some(last)) = (&self.session, &self.last) else {
            return false;
        };
        if session.output.clean.load(Ordering::Acquire) {
            return true;
        }
        if session.output.waiting.load(Ordering::Acquire) >= MOST_WAITING {
            return false;
        }
        let last = last.mutable();
        self.submit(last, true)
    }

    /// Copies CEF's GPU `IOSurface` into the encoder's buffer pool before encoding, since CEF
    /// reclaims the surface once the paint callback returns.
    pub(crate) fn encode_surface(&mut self, surface: *mut c_void) {
        // SAFETY: CEF provides a valid IOSurface for the duration of the paint callback.
        let (width, height) = unsafe { (IOSurfaceGetWidth(surface), IOSurfaceGetHeight(surface)) };
        let mut source = ptr::null_mut();
        // SAFETY: the surface is live, and the buffer retains it.
        unsafe {
            CVPixelBufferCreateWithIOSurface(ptr::null(), surface, ptr::null(), &raw mut source)
        };
        let Some(source) = Owned::new(source) else {
            return;
        };
        self.encode_buffer(&source, width, height);
    }

    pub(crate) fn encode_pixels(&mut self, pixels: &[u8], width: usize, height: usize) {
        let mut source = ptr::null_mut();
        // SAFETY: the out pointer is valid; a failure leaves it null.
        unsafe {
            CVPixelBufferCreate(
                ptr::null(),
                width,
                height,
                BGRA,
                ptr::null(),
                &raw mut source,
            )
        };
        let Some(source) = Owned::new(source) else {
            return;
        };
        // SAFETY: the buffer is locked while its rows are written, and each row is at most
        // `bytes_per_row` long.
        unsafe {
            CVPixelBufferLockBaseAddress(source.mutable(), 0);
            let base = CVPixelBufferGetBaseAddress(source.mutable());
            let stride = CVPixelBufferGetBytesPerRow(source.mutable());
            let row = width * 4;
            if !base.is_null() && stride >= row && pixels.len() >= row * height {
                for line in 0..height {
                    ptr::copy_nonoverlapping(
                        pixels.as_ptr().add(line * row),
                        base.add(line * stride),
                        row,
                    );
                }
            }
            CVPixelBufferUnlockBaseAddress(source.mutable(), 0);
        }
        self.encode_buffer(&source, width, height);
    }

    fn encode_buffer(&mut self, source: &Owned, width: usize, height: usize) {
        if width < 2 || height < 2 || self.transfer.is_null() {
            return;
        }
        let width = width & !1;
        let height = height & !1;
        if self
            .session
            .as_ref()
            .is_none_or(|session| (session.width, session.height) != (width, height))
        {
            self.session = None;
            self.last = None;
            self.held.clear();
            self.session = self.start(width, height);
            self.want_key = true;
        }
        let Some(session) = &self.session else {
            return;
        };
        let output = &session.output;
        let wanted = self.want_key
            || self.recover
            || output.want_key.load(Ordering::Acquire)
            || output.want_anchor.load(Ordering::Acquire);
        if output.waiting.load(Ordering::Acquire) >= MOST_WAITING && !wanted {
            output.missed.store(true, Ordering::Release);
            if output.waiting.load(Ordering::Acquire) < MOST_WAITING
                && output.missed.swap(false, Ordering::AcqRel)
            {
                engine::redraw(self.window, 0);
            }
            return;
        }
        let mut target = ptr::null_mut();
        // SAFETY: the session is valid and its pool returns buffers matching its size and format.
        unsafe {
            let pool = VTCompressionSessionGetPixelBufferPool(session.raw);
            if !pool.is_null() {
                CVPixelBufferPoolCreatePixelBuffer(ptr::null(), pool, &raw mut target);
            }
        }
        let Some(target) = Owned::new(target) else {
            return;
        };
        // SAFETY: both buffers are valid and the pixel transfer completes synchronously.
        let copied = unsafe {
            VTPixelTransferSessionTransferImage(self.transfer, source.mutable(), target.mutable())
        };
        if copied != 0 {
            return;
        }
        if self.submit(target.mutable(), false) {
            self.last = Some(target);
            if let Some(session) = &self.session {
                session.output.clean.store(false, Ordering::Release);
                *session
                    .output
                    .sharpening
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = Convergence::default();
            }
        }
    }

    /// Tags `frame_refcon` with refinement and recovery flags so the callback can skip redraws
    /// for dropped refinements, emit `FrameKind::Anchor`, and retry dropped recovery frames.
    fn submit(&mut self, target: *mut c_void, refinement: bool) -> bool {
        let Some(session) = &self.session else {
            return false;
        };
        let output = &session.output;
        let key =
            std::mem::take(&mut self.want_key) | output.want_key.swap(false, Ordering::AcqRel);
        let recovery = !key
            && (std::mem::take(&mut self.recover)
                | output.want_anchor.swap(false, Ordering::AcqRel));
        let tokens: Vec<Owned> = std::mem::take(&mut self.held)
            .into_iter()
            .filter_map(|mark| number_i64(mark.cast_signed()))
            .collect();
        let acknowledged = if tokens.is_empty() {
            None
        } else {
            array(&tokens.iter().map(|token| token.0).collect::<Vec<_>>())
        };
        let mut pairs = Vec::new();
        // SAFETY: these framework extern statics are valid Core Foundation constants.
        unsafe {
            if key {
                pairs.push((kVTEncodeFrameOptionKey_ForceKeyFrame, kCFBooleanTrue));
            }
            if recovery {
                pairs.push((kVTEncodeFrameOptionKey_ForceLTRRefresh, kCFBooleanTrue));
            }
            if let Some(acknowledged) = &acknowledged {
                pairs.push((
                    kVTEncodeFrameOptionKey_AcknowledgedLTRTokens,
                    acknowledged.0,
                ));
            }
        }
        let options = if pairs.is_empty() {
            None
        } else {
            dictionary(&pairs)
        };
        let marker = if refinement { REFINEMENT } else { 0 } | if recovery { RECOVERY } else { 0 };
        let time = CMTime {
            value: i64::try_from(self.started.elapsed().as_micros()).unwrap_or(i64::MAX),
            timescale: MICROSECONDS,
            flags: TIME_VALID,
            epoch: 0,
        };
        output.waiting.fetch_add(1, Ordering::AcqRel);
        // SAFETY: the session and pixel buffer are valid, and `VideoToolbox` retains the buffer
        // until encoding completes.
        let status = unsafe {
            VTCompressionSessionEncodeFrame(
                session.raw,
                target,
                time,
                NO_TIME,
                options.as_ref().map_or(ptr::null(), |options| options.0),
                ptr::without_provenance_mut(marker),
                ptr::null_mut(),
            )
        };
        if status != 0 {
            output.waiting.fetch_sub(1, Ordering::AcqRel);
            self.want_key = key;
            self.recover |= recovery;
            self.fail(&format!("the encoder refused a picture: {status}"));
        }
        status == 0
    }

    /// Allocates all CF parameters before creating the session so early `?` returns cannot
    /// leak the raw session or refcon.
    fn start(&self, width: usize, height: usize) -> Option<Session> {
        let (session_width, session_height) =
            (i32::try_from(width).ok()?, i32::try_from(height).ok()?);
        let rate = number_f64(f64::from(FRAME_RATE))?;
        let interval = number_i64(KEY_INTERVAL)?;
        let format = number_i64(i64::from(NV12))?;
        let wide = number_i64(i64::try_from(width).ok()?)?;
        let high = number_i64(i64::try_from(height).ok()?)?;
        let surface = dictionary(&[])?;
        // SAFETY: the keys are framework CF constants and the values are live CF objects.
        let attributes = dictionary(&unsafe {
            [
                (kCVPixelBufferPixelFormatTypeKey, format.0),
                (kCVPixelBufferWidthKey, wide.0),
                (kCVPixelBufferHeightKey, high.0),
                (kCVPixelBufferIOSurfacePropertiesKey, surface.0),
            ]
        })?;
        // SAFETY: as above.
        let specification = dictionary(&[unsafe {
            (
                kVTVideoEncoderSpecification_EnableLowLatencyRateControl,
                kCFBooleanTrue,
            )
        }])?;
        let output = Arc::new(Output {
            window: self.window,
            link: self.link.clone(),
            still: self.still,
            waiting: AtomicUsize::new(0),
            failed: AtomicBool::new(false),
            want_key: AtomicBool::new(false),
            want_anchor: AtomicBool::new(false),
            missed: AtomicBool::new(false),
            clean: AtomicBool::new(false),
            sharpening: Mutex::default(),
            budget: AtomicUsize::new(frame_budget(self.bitrate)),
        });
        let refcon = Arc::into_raw(Arc::clone(&output))
            .cast_mut()
            .cast::<c_void>();
        let mut raw = ptr::null_mut();
        // SAFETY: all pointers are live; on success the session calls `encoded` with `refcon`,
        // which stays valid until the session's drop releases it.
        let status = unsafe {
            VTCompressionSessionCreate(
                ptr::null(),
                session_width,
                session_height,
                H264,
                specification.0,
                attributes.0,
                ptr::null(),
                Some(encoded),
                refcon,
                &raw mut raw,
            )
        };
        if status != 0 || raw.is_null() {
            // SAFETY: the session did not take the reference.
            unsafe { drop(Arc::from_raw(refcon.cast::<Output>())) };
            self.fail(&format!("the encoder did not start: {status}"));
            return None;
        }
        // SAFETY: `raw` is a valid session and each value matches the CF type expected by its key.
        unsafe {
            VTSessionSetProperty(raw, kVTCompressionPropertyKey_RealTime, kCFBooleanTrue);
            VTSessionSetProperty(
                raw,
                kVTCompressionPropertyKey_ProfileLevel,
                kVTProfileLevel_H264_ConstrainedBaseline_AutoLevel,
            );
            VTSessionSetProperty(
                raw,
                kVTCompressionPropertyKey_AllowFrameReordering,
                kCFBooleanFalse,
            );
            VTSessionSetProperty(raw, kVTCompressionPropertyKey_ExpectedFrameRate, rate.0);
            VTSessionSetProperty(
                raw,
                kVTCompressionPropertyKey_MaxKeyFrameInterval,
                interval.0,
            );
            VTSessionSetProperty(raw, kVTCompressionPropertyKey_EnableLTR, kCFBooleanTrue);
        }
        set_bitrate(raw, self.bitrate);
        Some(Session {
            raw,
            output,
            width,
            height,
        })
    }

    fn fail(&self, problem: &str) {
        if !self.still {
            self.link.send(&FromEngine::Failed {
                window: self.window,
                problem: problem.to_owned(),
            });
        }
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        self.session = None;
        if !self.transfer.is_null() {
            // SAFETY: the transfer session is live and this encoder owns it.
            unsafe {
                VTPixelTransferSessionInvalidate(self.transfer);
                CFRelease(self.transfer);
            }
        }
    }
}

fn set_bitrate(session: *mut c_void, bitrate: u32) {
    let (Some(average), Some(bytes), Some(second)) = (
        number_i64(i64::from(bitrate)),
        number_f64(f64::from(bitrate) / 8.0 * BURST),
        number_f64(1.0),
    ) else {
        return;
    };
    let Some(limits) = array(&[bytes.0, second.0]) else {
        return;
    };
    // SAFETY: `session` is valid and each value matches the CF type expected by its key.
    unsafe {
        VTSessionSetProperty(session, kVTCompressionPropertyKey_AverageBitRate, average.0);
        VTSessionSetProperty(session, kVTCompressionPropertyKey_DataRateLimits, limits.0);
    }
}

/// `VideoToolbox` output callback (runs on a `VideoToolbox` thread). Requests a CEF redraw when
/// a frame was skipped or dropped so a static page still delivers its latest state. We redraw
/// instead of re-encoding locally because this thread cannot access `Encoder`, and frames
/// skipped under backpressure were never copied from CEF's transient `IOSurface`.
unsafe extern "C" fn encoded(
    refcon: *mut c_void,
    marker: *mut c_void,
    status: OSStatus,
    flags: u32,
    sample: *mut c_void,
) {
    // SAFETY: `refcon` points to the session's `Output`, kept alive until
    // `VTCompressionSessionCompleteFrames` finishes in `Session::drop`.
    let output = unsafe { &*refcon.cast::<Output>() };
    let refinement = marker.addr() & REFINEMENT != 0;
    let recovery = marker.addr() & RECOVERY != 0;
    output.waiting.fetch_sub(1, Ordering::AcqRel);
    if output.missed.swap(false, Ordering::AcqRel) {
        engine::redraw(output.window, 0);
    }
    if status != 0 || sample.is_null() {
        if status != 0 && !output.still && !output.failed.swap(true, Ordering::AcqRel) {
            output.link.send(&FromEngine::Failed {
                window: output.window,
                problem: format!("the encoder failed: {status}"),
            });
        } else if flags & FRAME_DROPPED != 0 && (recovery || !refinement) {
            output.want_anchor.fetch_or(recovery, Ordering::AcqRel);
            engine::redraw(output.window, REDRAW_AFTER_MS);
        }
        return;
    }
    // SAFETY: the sample is live during the callback.
    let Some(coded) = (unsafe { annex_b(sample) }) else {
        return;
    };
    let size = coded.data.len();
    let lost = output.link.send_frame(&EncodedFrame {
        window: output.window,
        kind: if coded.key {
            FrameKind::Key
        } else if recovery {
            FrameKind::Anchor
        } else {
            FrameKind::Delta
        },
        mark: coded.mark,
        still: output.still,
        data: Bytes::from(coded.data),
    });
    if refinement
        && output
            .sharpening
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .refined(size, output.budget.load(Ordering::Acquire))
    {
        output.clean.store(true, Ordering::Release);
    }
    if lost {
        output.want_key.store(true, Ordering::Release);
        engine::redraw(output.window, REDRAW_AFTER_MS);
    }
}

struct Coded {
    key: bool,
    mark: Option<u64>,
    data: Vec<u8>,
}

/// Converts a compressed `CMSampleBuffer` to Annex B (prepending parameter sets on keyframes)
/// and extracts its LTR acknowledgement mark, if any.
///
/// # Safety
///
/// `sample` must be a valid `CMSampleBufferRef`.
unsafe fn annex_b(sample: *mut c_void) -> Option<Coded> {
    // SAFETY: the sample is live; the attachments and their values are borrowed from it.
    let (key, mark) = unsafe {
        let attachments = CMSampleBufferGetSampleAttachmentsArray(sample, 0);
        if attachments.is_null() || CFArrayGetCount(attachments) == 0 {
            (true, None)
        } else {
            let first = CFArrayGetValueAtIndex(attachments, 0);
            let not_sync = CFDictionaryGetValue(first, kCMSampleAttachmentKey_NotSync);
            let token =
                CFDictionaryGetValue(first, kVTSampleAttachmentKey_RequireLTRAcknowledgementToken);
            let mut value = 0_i64;
            let mark = (!token.is_null()
                && CFNumberGetValue(token, NUMBER_I64, (&raw mut value).cast()) != 0)
                .then(|| value.cast_unsigned());
            (not_sync.is_null() || not_sync == kCFBooleanFalse, mark)
        }
    };
    // SAFETY: the sample is live; its description and data are borrowed from it.
    let (description, block) = unsafe {
        (
            CMSampleBufferGetFormatDescription(sample),
            CMSampleBufferGetDataBuffer(sample),
        )
    };
    if description.is_null() || block.is_null() {
        return None;
    }
    let mut out = Vec::new();
    let mut count = 0;
    let mut length_size = 4;
    // SAFETY: `description` is valid; querying index 0 populates `count` and `length_size`.
    unsafe {
        CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
            description,
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            &raw mut count,
            &raw mut length_size,
        )
    };
    if key {
        for index in 0..count {
            let mut set = ptr::null();
            let mut size = 0;
            // SAFETY: the index is below the count, and the set is borrowed from the description.
            let status = unsafe {
                CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
                    description,
                    index,
                    &raw mut set,
                    &raw mut size,
                    ptr::null_mut(),
                    ptr::null_mut(),
                )
            };
            if status == 0 && !set.is_null() {
                // SAFETY: the set has `size` bytes.
                let set = unsafe { std::slice::from_raw_parts(set, size) };
                let allowing = set
                    .first()
                    .filter(|header| *header & 0x1f == SEQUENCE_PARAMETERS)
                    .and_then(|_| sps::allowing_gaps(set));
                out.extend_from_slice(&START_CODE);
                out.extend_from_slice(allowing.as_deref().unwrap_or(set));
            }
        }
    }
    // SAFETY: the block is live and `data` has room for its length.
    let data = unsafe {
        let length = CMBlockBufferGetDataLength(block);
        let mut data = vec![0_u8; length];
        (CMBlockBufferCopyDataBytes(block, 0, length, data.as_mut_ptr().cast()) == 0)
            .then_some(data)
    }?;
    annex_b_units(&data, usize::try_from(length_size).ok()?, &mut out)?;
    Some(Coded {
        key,
        mark,
        data: out,
    })
}

fn annex_b_units(data: &[u8], length_size: usize, out: &mut Vec<u8>) -> Option<()> {
    let mut rest = data;
    while !rest.is_empty() {
        let (length, after) = rest.split_at_checked(length_size)?;
        let length = length
            .iter()
            .fold(0_usize, |sum, byte| (sum << 8) | usize::from(*byte));
        let (unit, after) = after.split_at_checked(length)?;
        out.extend_from_slice(&START_CODE);
        out.extend_from_slice(unit);
        rest = after;
    }
    Some(())
}
