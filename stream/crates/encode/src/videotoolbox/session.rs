//! One `VideoToolbox` compression session for a picture size and stream format.

use std::{
    ffi::{c_int, c_void},
    ptr::{self, NonNull},
    sync::Arc,
    time::Duration,
};

use bytes::Bytes;
use objc2_core_foundation::{CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType};
use objc2_core_media::{
    CMFormatDescription, CMSampleBuffer, CMTime, CMTimeFlags,
    CMVideoFormatDescriptionGetH264ParameterSetAtIndex,
    CMVideoFormatDescriptionGetHEVCParameterSetAtIndex,
    kCMFormatDescriptionColorPrimaries_ITU_R_709_2,
    kCMFormatDescriptionTransferFunction_ITU_R_709_2, kCMFormatDescriptionYCbCrMatrix_ITU_R_709_2,
    kCMSampleAttachmentKey_NotSync, kCMVideoCodecType_H264, kCMVideoCodecType_HEVC,
};
use objc2_core_video::CVPixelBuffer;
use objc2_video_toolbox::{
    VTCompressionSession, VTEncodeInfoFlags, VTSessionCopyProperty, VTSessionSetProperty,
    kVTCompressionPropertyKey_AllowFrameReordering, kVTCompressionPropertyKey_ColorPrimaries,
    kVTCompressionPropertyKey_EnableLTR, kVTCompressionPropertyKey_ExpectedFrameRate,
    kVTCompressionPropertyKey_MaxKeyFrameInterval, kVTCompressionPropertyKey_ProfileLevel,
    kVTCompressionPropertyKey_RealTime, kVTCompressionPropertyKey_SupportsBaseFrameQP,
    kVTCompressionPropertyKey_TransferFunction, kVTCompressionPropertyKey_YCbCrMatrix,
    kVTEncodeFrameOptionKey_AcknowledgedLTRTokens, kVTEncodeFrameOptionKey_BaseFrameQP,
    kVTEncodeFrameOptionKey_ForceKeyFrame, kVTEncodeFrameOptionKey_ForceLTRRefresh,
    kVTProfileLevel_H264_ConstrainedHigh_AutoLevel, kVTProfileLevel_HEVC_Main_AutoLevel,
    kVTSampleAttachmentKey_RequireLTRAcknowledgementToken,
    kVTVideoEncoderSpecification_EnableLowLatencyRateControl,
};
use stream::{Codec, Format};

use super::Gate;
use crate::FRAME_RATE;

pub(super) const RECOVERY: usize = 1;
const START_CODE: [u8; 4] = [0, 0, 0, 1];
/// Low-latency rate control never asks for keyframes on its own; this only bounds a stream that
/// nobody asked to repair.
const KEY_INTERVAL: i64 = 3600;
const PROBE: (i32, i32) = (640, 360);
const INVALID_TIME: CMTime = CMTime {
    value: 0,
    timescale: 0,
    flags: CMTimeFlags(0),
    epoch: 0,
};

/// A session's picture size and stream format.
pub(super) type Shape = (usize, usize, Format);

pub(super) struct Coded {
    pub(super) key: bool,
    pub(super) mark: Option<u64>,
    pub(super) data: Bytes,
}

struct Context {
    gate: Arc<Gate>,
    codec: Codec,
}

pub(super) struct Session {
    raw: CFRetained<VTCompressionSession>,
    context: *const Context,
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) format: Format,
}

#[expect(
    clippy::non_send_fields_in_send_ty,
    reason = "Core Foundation types are thread safe, but objc2 marks them all as !Send"
)]
// SAFETY: `VideoToolbox` sessions may be used from any thread, and `context` is only read by the
// session's callback.
unsafe impl Send for Session {}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: completing the frames runs every pending callback before the context they
        // read is released.
        unsafe {
            self.raw.complete_frames(INVALID_TIME);
            self.raw.invalidate();
            drop(Arc::from_raw(self.context));
        }
    }
}

const fn codec_type(codec: Codec) -> u32 {
    match codec {
        Codec::H264 => kCMVideoCodecType_H264,
        Codec::Hevc => kCMVideoCodecType_HEVC,
    }
}

fn low_latency() -> CFRetained<CFDictionary<CFString, CFType>> {
    // SAFETY: a VideoToolbox constant key.
    let key = unsafe { kVTVideoEncoderSpecification_EnableLowLatencyRateControl };
    let value: &CFType = CFBoolean::new(true);
    CFDictionary::from_slices(&[key], &[value])
}

fn set(session: &VTCompressionSession, key: &CFString, value: &CFType) -> bool {
    // SAFETY: each caller passes a value of the type the key expects.
    unsafe { VTSessionSetProperty(session, key, Some(value)) == 0 }
}

impl Session {
    /// Whether this computer encodes `codec` in hardware with low-latency rate control.
    pub(super) fn supports(codec: Codec) -> bool {
        let specification = low_latency();
        let mut raw = ptr::null_mut();
        // SAFETY: the specification is valid and the out pointer is valid; no frames are
        // submitted, so the missing callback is never called.
        let status = unsafe {
            VTCompressionSession::create(
                None,
                PROBE.0,
                PROBE.1,
                codec_type(codec),
                Some(specification.as_opaque()),
                None,
                None,
                None,
                ptr::null_mut(),
                NonNull::from(&mut raw),
            )
        };
        let Some(raw) = NonNull::new(raw) else {
            return false;
        };
        // SAFETY: the session was just created with a +1 retain count.
        let session = unsafe { CFRetained::from_raw(raw) };
        // SAFETY: the session is valid and has no pending frames.
        unsafe { session.invalidate() };
        status == 0
    }

    pub(super) fn start(
        width: usize,
        height: usize,
        format: Format,
        gate: &Arc<Gate>,
    ) -> Result<Self, String> {
        let (wide, high) = (
            i32::try_from(width).map_err(|_| "the picture is too wide")?,
            i32::try_from(height).map_err(|_| "the picture is too high")?,
        );
        let context = Arc::into_raw(Arc::new(Context {
            gate: Arc::clone(gate),
            codec: format.codec,
        }));
        let specification = low_latency();
        let mut raw = ptr::null_mut();
        // SAFETY: the pointers are valid; on success the session calls `encoded` with `context`,
        // which stays alive until the session's drop releases it.
        let status = unsafe {
            VTCompressionSession::create(
                None,
                wide,
                high,
                codec_type(format.codec),
                Some(specification.as_opaque()),
                None,
                None,
                Some(encoded),
                context.cast_mut().cast(),
                NonNull::from(&mut raw),
            )
        };
        let Some(raw) = NonNull::new(raw).filter(|_| status == 0) else {
            // SAFETY: the session did not take the context.
            unsafe { drop(Arc::from_raw(context)) };
            return Err(format!("the encoder did not start: {status}"));
        };
        // SAFETY: the session was just created with a +1 retain count.
        let raw = unsafe { CFRetained::from_raw(raw) };
        let session = Self {
            raw,
            context,
            width,
            height,
            format,
        };
        session.configure()?;
        if !session.takes_quantizers() {
            return Err("the encoder takes no quantizer for each frame".to_owned());
        }
        // SAFETY: the session is valid. Preparing ahead saves the first frame's setup time; a
        // session that refuses prepares with that frame instead.
        unsafe { session.raw.prepare_to_encode_frames() };
        Ok(session)
    }

    pub(super) const fn shape(&self) -> Shape {
        (self.width, self.height, self.format)
    }

    fn configure(&self) -> Result<(), String> {
        let raw = &*self.raw;
        let yes: &CFType = CFBoolean::new(true);
        let no: &CFType = CFBoolean::new(false);
        let rate = CFNumber::new_f64(f64::from(FRAME_RATE));
        let interval = CFNumber::new_i64(KEY_INTERVAL);
        // SAFETY: these are VideoToolbox and Core Media constants, each paired with the value
        // type its key expects.
        unsafe {
            let profile: &CFType = match self.format.codec {
                Codec::H264 => kVTProfileLevel_H264_ConstrainedHigh_AutoLevel,
                Codec::Hevc => kVTProfileLevel_HEVC_Main_AutoLevel,
            };
            for (key, value) in [
                (kVTCompressionPropertyKey_RealTime, yes),
                (kVTCompressionPropertyKey_ProfileLevel, profile),
                (kVTCompressionPropertyKey_AllowFrameReordering, no),
            ] {
                if !set(raw, key, value) {
                    return Err(format!("the encoder refused {key}"));
                }
            }
            // The stream still works without these, so an encoder that refuses them is used as
            // it is.
            for (key, value) in [
                (
                    kVTCompressionPropertyKey_ExpectedFrameRate,
                    &*rate as &CFType,
                ),
                (kVTCompressionPropertyKey_MaxKeyFrameInterval, &*interval),
                (
                    kVTCompressionPropertyKey_ColorPrimaries,
                    kCMFormatDescriptionColorPrimaries_ITU_R_709_2,
                ),
                (
                    kVTCompressionPropertyKey_TransferFunction,
                    kCMFormatDescriptionTransferFunction_ITU_R_709_2,
                ),
                (
                    kVTCompressionPropertyKey_YCbCrMatrix,
                    kCMFormatDescriptionYCbCrMatrix_ITU_R_709_2,
                ),
                (
                    kVTCompressionPropertyKey_EnableLTR,
                    if self.format.anchors { yes } else { no },
                ),
            ] {
                set(raw, key, value);
            }
        }
        Ok(())
    }

    /// Low-latency rate control takes a quantizer for each frame, which replaces its own rate
    /// control and its frame drops.
    fn takes_quantizers(&self) -> bool {
        let mut value: *const c_void = ptr::null();
        // SAFETY: a VideoToolbox constant key, and the out pointer is valid.
        let status = unsafe {
            VTSessionCopyProperty(
                &self.raw,
                kVTCompressionPropertyKey_SupportsBaseFrameQP,
                None,
                (&raw mut value).cast(),
            )
        };
        let Some(value) = NonNull::new(value.cast_mut()).filter(|_| status == 0) else {
            return false;
        };
        let takes = ptr::eq(value.as_ptr().cast::<CFBoolean>(), CFBoolean::new(true));
        // SAFETY: the copy returned the value with a +1 retain count.
        drop(unsafe { CFRetained::from_raw(value.cast::<CFType>()) });
        takes
    }

    /// Returns the encoder's status; on success the callback reports the frame later.
    pub(super) fn encode(
        &self,
        picture: &CVPixelBuffer,
        since: Duration,
        key: bool,
        recovery: bool,
        held: &[u64],
        quantizer: u8,
        marker: usize,
    ) -> i32 {
        let yes: &CFType = CFBoolean::new(true);
        let quantizer = CFNumber::new_i32(i32::from(quantizer));
        let tokens: Vec<CFRetained<CFNumber>> = held
            .iter()
            .map(|mark| CFNumber::new_i64(mark.cast_signed()))
            .collect();
        let acknowledged = (!tokens.is_empty())
            .then(|| objc2_core_foundation::CFArray::from_retained_objects(&tokens));
        // SAFETY: a VideoToolbox constant key with the number it expects.
        let mut keys: Vec<&CFString> = vec![unsafe { kVTEncodeFrameOptionKey_BaseFrameQP }];
        let mut values: Vec<&CFType> = vec![&quantizer];
        // SAFETY: VideoToolbox constant keys with the values they expect.
        unsafe {
            if key {
                keys.push(kVTEncodeFrameOptionKey_ForceKeyFrame);
                values.push(yes);
            }
            if recovery {
                keys.push(kVTEncodeFrameOptionKey_ForceLTRRefresh);
                values.push(yes);
            }
            if let Some(acknowledged) = &acknowledged {
                keys.push(kVTEncodeFrameOptionKey_AcknowledgedLTRTokens);
                values.push(acknowledged);
            }
        }
        let options = CFDictionary::from_slices(&keys, &values);
        let time = CMTime {
            value: i64::try_from(since.as_micros()).unwrap_or(i64::MAX),
            timescale: 1_000_000,
            flags: CMTimeFlags::Valid,
            epoch: 0,
        };
        // SAFETY: the session and picture are valid, and VideoToolbox retains the picture until
        // it is encoded.
        unsafe {
            self.raw.encode_frame(
                picture,
                time,
                INVALID_TIME,
                Some(options.as_opaque()),
                ptr::without_provenance_mut(marker),
                ptr::null_mut(),
            )
        }
    }
}

/// `VideoToolbox` calls this on one of its threads for each picture.
unsafe extern "C-unwind" fn encoded(
    refcon: *mut c_void,
    marker: *mut c_void,
    status: i32,
    flags: VTEncodeInfoFlags,
    sample: *mut CMSampleBuffer,
) {
    // SAFETY: `refcon` is the session's context, alive until the session's drop completes all
    // frames.
    let context = unsafe { &*refcon.cast::<Context>() };
    let coded = NonNull::new(sample)
        .filter(|_| status == 0)
        // SAFETY: the sample is valid during the callback.
        .and_then(|sample| unsafe { coded(sample.as_ref(), context.codec) });
    context.gate.encoded(
        marker.addr(),
        status,
        flags.contains(VTEncodeInfoFlags::FrameDropped),
        coded,
        context.codec,
    );
}

/// Converts a compressed sample to Annex B, with the parameter sets in front of a keyframe.
///
/// # Safety
///
/// `sample` must be a valid compressed `CMSampleBuffer`.
unsafe fn coded(sample: &CMSampleBuffer, codec: Codec) -> Option<Coded> {
    // SAFETY: the caller guarantees a valid sample.
    let (key, mark) = unsafe { sync_and_mark(sample) };
    // SAFETY: as above.
    let (description, block) = unsafe { (sample.format_description()?, sample.data_buffer()?) };
    let mut out = Vec::new();
    // SAFETY: the description belongs to the sample.
    let length_size = unsafe { parameter_sets(&description, codec, key, &mut out)? };
    // SAFETY: the block belongs to the sample, and `data` has room for its length.
    let data = unsafe {
        let length = block.data_length();
        let mut data = vec![0_u8; length];
        let status = block.copy_data_bytes(0, length, NonNull::new(data.as_mut_ptr().cast())?);
        (status == 0).then_some(data)?
    };
    annex_b_units(&data, length_size, &mut out)?;
    Some(Coded {
        key,
        mark,
        data: Bytes::from(out),
    })
}

/// # Safety
///
/// `sample` must be a valid `CMSampleBuffer`.
unsafe fn sync_and_mark(sample: &CMSampleBuffer) -> (bool, Option<u64>) {
    // SAFETY: the caller guarantees a valid sample; the attachments belong to it.
    let Some(attachments) = (unsafe { sample.sample_attachments_array(false) }) else {
        return (true, None);
    };
    if attachments.count() == 0 {
        return (true, None);
    }
    // SAFETY: index 0 exists, and each sample's attachments are a dictionary.
    let first = unsafe {
        attachments
            .value_at_index(0)
            .cast::<CFDictionary>()
            .as_ref()
    };
    let Some(first) = first else {
        return (true, None);
    };
    // SAFETY: the keys are Core Media and VideoToolbox constants; the values belong to the
    // dictionary.
    let (not_sync, token) = unsafe {
        (
            first.value(ptr::from_ref(kCMSampleAttachmentKey_NotSync).cast()),
            first
                .value(ptr::from_ref(kVTSampleAttachmentKey_RequireLTRAcknowledgementToken).cast())
                .cast::<CFNumber>()
                .as_ref(),
        )
    };
    let mark = token.and_then(CFNumber::as_i64).map(i64::cast_unsigned);
    let sync = not_sync.is_null() || ptr::eq(not_sync.cast(), CFBoolean::new(false));
    (sync, mark)
}

/// Writes the parameter sets in front of a keyframe and returns the stream's NAL length size.
///
/// # Safety
///
/// `description` must be a valid video format description of `codec`.
unsafe fn parameter_sets(
    description: &CMFormatDescription,
    codec: Codec,
    key: bool,
    out: &mut Vec<u8>,
) -> Option<usize> {
    let read = |index: usize,
                set: *mut *const u8,
                size: *mut usize,
                count: *mut usize,
                header: *mut c_int| {
        // SAFETY: the description is valid and the out pointers are valid or null.
        unsafe {
            match codec {
                Codec::H264 => CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
                    description,
                    index,
                    set,
                    size,
                    count,
                    header,
                ),
                Codec::Hevc => CMVideoFormatDescriptionGetHEVCParameterSetAtIndex(
                    description,
                    index,
                    set,
                    size,
                    count,
                    header,
                ),
            }
        }
    };
    let (mut count, mut length_size) = (0, 4);
    read(
        0,
        ptr::null_mut(),
        ptr::null_mut(),
        &raw mut count,
        &raw mut length_size,
    );
    if key {
        for index in 0..count {
            let (mut set, mut size) = (ptr::null(), 0);
            if read(
                index,
                &raw mut set,
                &raw mut size,
                ptr::null_mut(),
                ptr::null_mut(),
            ) != 0
                || set.is_null()
            {
                continue;
            }
            // SAFETY: the set has `size` bytes owned by the description.
            let set = unsafe { std::slice::from_raw_parts(set, size) };
            out.extend_from_slice(&START_CODE);
            out.extend_from_slice(set);
        }
    }
    usize::try_from(length_size).ok()
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
