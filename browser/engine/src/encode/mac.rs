use std::{
    ffi::c_void,
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Instant,
};

use bytes::Bytes;
use shared_browser::link::{EncodedFrame, FromEngine};

use super::FRAME_RATE;
use crate::link::Link;

/// The most pictures that wait in the encoder. A newer picture replaces a waiting one, so the
/// viewer sees the page as it is now.
const MOST_WAITING: usize = 2;
const START_CODE: [u8; 4] = [0, 0, 0, 1];

type OSStatus = i32;
type Ref = *const c_void;

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
const NUMBER_I64: isize = 4;
const NUMBER_F64: isize = 13;
const MICROSECONDS: i32 = 1_000_000;
const KEY_INTERVAL: i64 = 3600;
/// A second of bursts may go half again above the average.
const BURST: f64 = 1.5;

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
    fn CMSampleBufferGetPresentationTimeStamp(sample: *mut c_void) -> CMTime;
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

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(object: Ref);
    fn CFDictionaryCreate(
        allocator: Ref,
        keys: *const Ref,
        values: *const Ref,
        count: isize,
        key_callbacks: Ref,
        value_callbacks: Ref,
    ) -> Ref;
    fn CFDictionaryGetValue(dictionary: Ref, key: Ref) -> Ref;
    fn CFArrayCreate(allocator: Ref, values: *const Ref, count: isize, callbacks: Ref) -> Ref;
    fn CFArrayGetCount(array: Ref) -> isize;
    fn CFArrayGetValueAtIndex(array: Ref, index: isize) -> Ref;
    fn CFNumberCreate(allocator: Ref, kind: isize, value: *const c_void) -> Ref;
    static kCFBooleanTrue: Ref;
    static kCFBooleanFalse: Ref;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
    static kCFTypeArrayCallBacks: c_void;
}

#[link(name = "IOSurface", kind = "framework")]
unsafe extern "C" {
    fn IOSurfaceGetWidth(surface: *mut c_void) -> usize;
    fn IOSurfaceGetHeight(surface: *mut c_void) -> usize;
}

/// A Core Foundation object that this code owns.
struct Owned(Ref);

impl Owned {
    fn new(object: Ref) -> Option<Self> {
        (!object.is_null()).then_some(Self(object))
    }

    fn mutable(&self) -> *mut c_void {
        self.0.cast_mut()
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: the object is not null and this code holds one reference to it.
        unsafe { CFRelease(self.0) };
    }
}

fn number_i64(value: i64) -> Option<Owned> {
    // SAFETY: the value lives for the call, and CFNumberCreate copies it.
    Owned::new(unsafe { CFNumberCreate(ptr::null(), NUMBER_I64, (&raw const value).cast()) })
}

fn number_f64(value: f64) -> Option<Owned> {
    // SAFETY: as in `number_i64`.
    Owned::new(unsafe { CFNumberCreate(ptr::null(), NUMBER_F64, (&raw const value).cast()) })
}

fn dictionary(pairs: &[(Ref, Ref)]) -> Option<Owned> {
    let keys: Vec<Ref> = pairs.iter().map(|(key, _)| *key).collect();
    let values: Vec<Ref> = pairs.iter().map(|(_, value)| *value).collect();
    // SAFETY: the keys and values are live CF objects, and the dictionary retains them.
    Owned::new(unsafe {
        CFDictionaryCreate(
            ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            isize::try_from(pairs.len()).ok()?,
            (&raw const kCFTypeDictionaryKeyCallBacks).cast(),
            (&raw const kCFTypeDictionaryValueCallBacks).cast(),
        )
    })
}

/// What the encoder's callback needs. Each session has its own, so a session that ends takes
/// its count of waiting pictures with it.
struct Output {
    window: i64,
    link: Link,
    waiting: AtomicUsize,
    failed: AtomicBool,
    /// The link lost a picture, so the next one is a key frame.
    want_key: AtomicBool,
}

struct Session {
    raw: *mut c_void,
    output: Arc<Output>,
    width: usize,
    height: usize,
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: the session is live; completing its frames runs the callbacks that are due
        // before the session and its output go.
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
    want_key: bool,
    started: Instant,
    session: Option<Session>,
    transfer: *mut c_void,
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
            want_key: true,
            started: Instant::now(),
            session: None,
            transfer,
        }
    }

    pub(crate) fn want_key(&mut self) {
        self.want_key = true;
    }

    pub(crate) fn set_bitrate(&mut self, bitrate: u32) {
        self.bitrate = bitrate;
        if let Some(session) = &self.session {
            set_bitrate(session.raw, bitrate);
        }
    }

    /// Encodes a picture that CEF drew on the GPU. The surface is CEF's only during this call,
    /// so it is copied into one of the encoder's buffers first.
    pub(crate) fn encode_surface(&mut self, surface: *mut c_void) {
        if surface.is_null() {
            return;
        }
        // SAFETY: CEF gives a live IOSurface for the length of the paint call.
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

    /// Encodes a picture that CEF drew in memory, as BGRA rows.
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
            self.session = self.start(width, height);
            self.want_key = true;
        }
        let Some(session) = &self.session else {
            return;
        };
        let output = &session.output;
        let want_key = self.want_key || output.want_key.load(Ordering::Acquire);
        if output.waiting.load(Ordering::Acquire) >= MOST_WAITING && !want_key {
            return;
        }
        let mut target = ptr::null_mut();
        // SAFETY: the session is live, and its pool gives buffers of its size and format.
        unsafe {
            let pool = VTCompressionSessionGetPixelBufferPool(session.raw);
            if !pool.is_null() {
                CVPixelBufferPoolCreatePixelBuffer(ptr::null(), pool, &raw mut target);
            }
        }
        let Some(target) = Owned::new(target) else {
            return;
        };
        // SAFETY: both buffers are live; the copy ends before this call returns.
        let copied = unsafe {
            VTPixelTransferSessionTransferImage(self.transfer, source.mutable(), target.mutable())
        };
        if copied != 0 {
            return;
        }
        let key =
            std::mem::take(&mut self.want_key) | output.want_key.swap(false, Ordering::AcqRel);
        let options = if key {
            // SAFETY: the key and the boolean are constants of the frameworks.
            dictionary(&[unsafe { (kVTEncodeFrameOptionKey_ForceKeyFrame, kCFBooleanTrue) }])
        } else {
            None
        };
        let time = CMTime {
            value: i64::try_from(self.started.elapsed().as_micros()).unwrap_or(i64::MAX),
            timescale: MICROSECONDS,
            flags: TIME_VALID,
            epoch: 0,
        };
        output.waiting.fetch_add(1, Ordering::AcqRel);
        // SAFETY: the session and the buffer are live, and the session retains the buffer
        // until it encoded it.
        let status = unsafe {
            VTCompressionSessionEncodeFrame(
                session.raw,
                target.mutable(),
                time,
                NO_TIME,
                options.as_ref().map_or(ptr::null(), |options| options.0),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        if status != 0 {
            output.waiting.fetch_sub(1, Ordering::AcqRel);
            self.want_key = key;
            self.fail(&format!("the encoder refused a picture: {status}"));
        }
    }

    /// Makes every value first, so that no failure leaves a session behind.
    fn start(&self, width: usize, height: usize) -> Option<Session> {
        let (session_width, session_height) =
            (i32::try_from(width).ok()?, i32::try_from(height).ok()?);
        let rate = number_f64(f64::from(FRAME_RATE))?;
        let interval = number_i64(KEY_INTERVAL)?;
        let format = number_i64(i64::from(NV12))?;
        let wide = number_i64(i64::try_from(width).ok()?)?;
        let high = number_i64(i64::try_from(height).ok()?)?;
        let surface = dictionary(&[])?;
        // SAFETY: the keys are constants of the frameworks and the values are live.
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
            waiting: AtomicUsize::new(0),
            failed: AtomicBool::new(false),
            want_key: AtomicBool::new(false),
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
        // SAFETY: the session is live and the values are live CF objects of the kinds that the
        // keys take.
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
        self.link.send(&FromEngine::Failed {
            window: self.window,
            problem: problem.to_owned(),
        });
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
    let pair = [bytes.0, second.0];
    // SAFETY: the values are live CF numbers, and the array retains them.
    let Some(limits) = Owned::new(unsafe {
        CFArrayCreate(
            ptr::null(),
            pair.as_ptr(),
            2,
            (&raw const kCFTypeArrayCallBacks).cast(),
        )
    }) else {
        return;
    };
    // SAFETY: the session is live and the values have the kinds that the keys take.
    unsafe {
        VTSessionSetProperty(session, kVTCompressionPropertyKey_AverageBitRate, average.0);
        VTSessionSetProperty(session, kVTCompressionPropertyKey_DataRateLimits, limits.0);
    }
}

/// The encoder's callback, on a thread of `VideoToolbox`.
unsafe extern "C" fn encoded(
    refcon: *mut c_void,
    _frame: *mut c_void,
    status: OSStatus,
    _flags: u32,
    sample: *mut c_void,
) {
    // SAFETY: `refcon` is the session's `Output`, which lives until the session completed its
    // frames.
    let output = unsafe { &*refcon.cast::<Output>() };
    output.waiting.fetch_sub(1, Ordering::AcqRel);
    if status != 0 || sample.is_null() {
        if status != 0 && !output.failed.swap(true, Ordering::AcqRel) {
            output.link.send(&FromEngine::Failed {
                window: output.window,
                problem: format!("the encoder failed: {status}"),
            });
        }
        return;
    }
    // SAFETY: the sample is live during the callback.
    let Some((key, data)) = (unsafe { annex_b(sample) }) else {
        return;
    };
    // SAFETY: as above.
    let time = unsafe { CMSampleBufferGetPresentationTimeStamp(sample) };
    let lost = output.link.send_frame(&EncodedFrame {
        window: output.window,
        key,
        timestamp: u64::try_from(time.value).unwrap_or(0),
        data: Bytes::from(data),
    });
    if lost {
        output.want_key.store(true, Ordering::Release);
    }
}

/// The picture as Annex B, with the parameter sets before a key frame.
///
/// # Safety
///
/// `sample` is a live compressed sample buffer.
unsafe fn annex_b(sample: *mut c_void) -> Option<(bool, Vec<u8>)> {
    // SAFETY: the sample is live; the attachments are borrowed from it.
    let key = unsafe {
        let attachments = CMSampleBufferGetSampleAttachmentsArray(sample, 0);
        attachments.is_null() || CFArrayGetCount(attachments) == 0 || {
            let first = CFArrayGetValueAtIndex(attachments, 0);
            let not_sync = CFDictionaryGetValue(first, kCMSampleAttachmentKey_NotSync);
            not_sync.is_null() || not_sync == kCFBooleanFalse
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
    // SAFETY: the description is live; asking for set 0 gives the count and the length size.
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
                out.extend_from_slice(&START_CODE);
                // SAFETY: the set has `size` bytes.
                out.extend_from_slice(unsafe { std::slice::from_raw_parts(set, size) });
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
    Some((key, out))
}

/// Rewrites length-prefixed NAL units as Annex B units with start codes.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_prefixed_units_get_start_codes() {
        let mut out = Vec::new();
        annex_b_units(&[0, 0, 0, 2, 0x65, 0xAA, 0, 0, 0, 1, 0x41], 4, &mut out).unwrap();
        assert_eq!(out, [0, 0, 0, 1, 0x65, 0xAA, 0, 0, 0, 1, 0x41]);
    }

    #[test]
    fn a_cut_unit_is_refused() {
        assert!(annex_b_units(&[0, 0, 0, 9, 0x65], 4, &mut Vec::new()).is_none());
    }
}
