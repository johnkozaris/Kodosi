//! H.264's side of a session: the parameter sets and slice headers that the encoder writes
//! itself, and the parameter buffers that describe each frame to the driver. The parameter sets
//! describe the colours and tell decoders that frames come in the order they show, so they show
//! each frame as soon as it arrives; Mesa takes a slice's unit type only from a slice header
//! written for it.

use std::ffi::c_int;

use super::{
    bits::Bits,
    session::{Frame, PACKED_SETS, PACKED_SLICE, Shape, packed},
    va::{
        self, Display, Id, NO_ID, OUT_OF_RANGE, PictureH264, PictureParametersH264, SequenceH264,
        SliceH264, Status,
    },
};
use crate::FRAME_RATE;

/// Macroblocks are 16 pixels square.
pub(super) const BLOCK: usize = 16;
const BLOCKS: (usize, usize) = (BLOCK, BLOCK);
/// `frame_num` counts frames modulo `1 << LOG2_FRAME_NUMBERS`.
const LOG2_FRAME_NUMBERS: u32 = 8;
const FRAME_NUMBERS: u32 = 1 << LOG2_FRAME_NUMBERS;
/// The quantizer a slice's delta starts from.
const INITIAL_QUANTIZER: u8 = 26;
/// Keyframes come only when asked for, but drivers want a period, and some divide by it.
const INTRA_PERIOD: u32 = 1 << 15;
const HIGH: u32 = 100;
/// `constraint_set4_flag` and `constraint_set5_flag`, which make High into Constrained High:
/// progressive, and without B slices.
const CONSTRAINED: u32 = 0b0000_1100;
const UNSPECIFIED_FORMAT: u32 = 5;
const BT709: u32 = 1;
/// The longest motion vectors that H.264 allows, as the log2 of their length.
const LONGEST_MOTION: usize = 15;
const SEQUENCE_SET: u8 = 0x67;
const PICTURE_SET: u8 = 0x68;
const IDR_SLICE: u8 = 0x65;
const SLICE: u8 = 0x61;
const SLICE_P: u8 = 0;
const SLICE_I: u8 = 2;
/// `seq_fields`: 4:2:0, progressive, 8x8 direct inference, `frame_num`'s length, and
/// `pic_order_cnt_type` 2.
const SEQUENCE_FIELDS: u32 = 1 | (1 << 2) | (1 << 5) | ((LOG2_FRAME_NUMBERS - 4) << 6) | (2 << 10);
/// `vui_fields`: bitstream restrictions with the longest motion vectors, which may cross the
/// picture's edges.
const USABILITY_FIELDS: u32 = (1 << 2) | (15 << 3) | (15 << 8) | (1 << 15);
/// `pic_fields` beside `idr_pic_flag`: a reference picture, CABAC, 8x8 transforms, and
/// deblocking that slices control.
const PICTURE_FIELDS: u32 = (1 << 1) | (1 << 3) | (1 << 8) | (1 << 9);

/// The entrypoint for H.264 that a display offers, if it takes all that a session gives it. Low
/// power comes first: Intel graphics encode faster in it, and newer ones only in it.
pub(super) fn entrypoint(display: &Display) -> Option<c_int> {
    let offered = display.entrypoints(va::H264_HIGH);
    [va::ENCODE_LOW_POWER, va::ENCODE]
        .into_iter()
        .find(|entrypoint| offered.contains(entrypoint))
        .filter(|entrypoint| packs_headers(display, va::H264_HIGH, *entrypoint))
}

/// Whether an entrypoint encodes 4:2:0 at set quantizers with the headers that sessions write.
pub(super) fn packs_headers(display: &Display, profile: c_int, entrypoint: c_int) -> bool {
    let headers = va::PACKED_SEQUENCE | va::PACKED_SLICE;
    display
        .attributes(
            profile,
            entrypoint,
            [
                va::ATTRIBUTE_FORMAT,
                va::ATTRIBUTE_RATE_CONTROL,
                va::ATTRIBUTE_PACKED_HEADERS,
            ],
        )
        .is_some_and(|[format, rate_control, packed]| {
            format & va::FORMAT_YUV420 != 0
                && rate_control & va::CONSTANT_QUANTIZER != 0
                && packed & headers == headers
        })
}

pub(super) struct Syntax {
    shape: Shape,
    sequence: Box<SequenceH264>,
    parameter_sets: Vec<u8>,
}

impl Syntax {
    pub(super) fn new(shape: Shape) -> Self {
        Self {
            shape,
            sequence: Box::new(sequence(shape)),
            parameter_sets: [sequence_set(shape), picture_set()].concat(),
        }
    }

    /// Makes the buffers that describe a frame.
    pub(super) fn buffers(
        &self,
        display: &Display,
        context: Id,
        frame: &Frame,
        buffers: &mut Vec<Id>,
    ) -> Result<(), Status> {
        let current = picture(frame.number, frame.current, 0);
        let previous = (!frame.key).then(|| {
            picture(
                frame.number.wrapping_sub(1),
                frame.previous,
                va::SHORT_TERM_REFERENCE,
            )
        });
        if frame.key {
            buffers.push(display.create_parameters(context, va::SEQUENCE, &*self.sequence)?);
        }
        let mut parameters = PictureParametersH264 {
            current,
            references: [PictureH264::NONE; 16],
            coded: frame.coded,
            frame_number: u16::try_from(current.frame_index).map_err(|_| OUT_OF_RANGE)?,
            initial_quantizer: INITIAL_QUANTIZER,
            fields: u32::from(frame.key) | PICTURE_FIELDS,
            ..PictureParametersH264::default()
        };
        if let Some(previous) = previous {
            parameters.references[0] = previous;
        }
        buffers.push(display.create_parameters(context, va::PICTURE, &parameters)?);
        if frame.key {
            let length = self.parameter_sets.len() * 8;
            packed(
                display,
                context,
                PACKED_SETS,
                &self.parameter_sets,
                length,
                buffers,
            )?;
        }
        let (header, length) = slice_header(frame, current.frame_index);
        packed(display, context, PACKED_SLICE, &header, length, buffers)?;
        let mut slice = SliceH264 {
            blocks: u32::try_from(blocks(self.shape)).map_err(|_| OUT_OF_RANGE)?,
            block_info: NO_ID,
            slice_type: if frame.key { SLICE_I } else { SLICE_P },
            idr_id: frame.idr,
            list0: [PictureH264::NONE; 32],
            list1: [PictureH264::NONE; 32],
            quantizer_delta: quantizer_delta(frame.quantizer),
            ..SliceH264::default()
        };
        if let Some(previous) = previous {
            slice.list0[0] = previous;
        }
        buffers.push(display.create_parameters(context, va::SLICE, &slice)?);
        Ok(())
    }
}

/// The `number`th frame since the IDR, reconstructed into `surface`.
fn picture(number: u32, surface: Id, flags: u32) -> PictureH264 {
    let order = number.wrapping_mul(2).cast_signed();
    PictureH264 {
        surface,
        frame_index: number % FRAME_NUMBERS,
        flags,
        top_order: order,
        bottom_order: order,
        reserved: [0; 4],
    }
}

fn blocks(shape: Shape) -> usize {
    let (width, height) = shape.coded(BLOCKS);
    (width / BLOCK) * (height / BLOCK)
}

fn quantizer_delta(quantizer: u8) -> i8 {
    quantizer.cast_signed() - INITIAL_QUANTIZER.cast_signed()
}

fn sequence(shape: Shape) -> SequenceH264 {
    let (right, bottom) = shape.crop(BLOCKS);
    let (width, height) = shape.coded(BLOCKS);
    SequenceH264 {
        level: level(shape),
        intra_period: INTRA_PERIOD,
        idr_period: INTRA_PERIOD,
        ip_period: 1,
        reference_frames: 1,
        width_in_blocks: u16::try_from(width / BLOCK).unwrap_or(u16::MAX),
        height_in_blocks: u16::try_from(height / BLOCK).unwrap_or(u16::MAX),
        fields: SEQUENCE_FIELDS,
        cropping: u8::from(right > 0 || bottom > 0),
        crop_right: u32::try_from(right).unwrap_or_default(),
        crop_bottom: u32::try_from(bottom).unwrap_or_default(),
        vui_present: 1,
        vui_fields: USABILITY_FIELDS,
        ..SequenceH264::default()
    }
}

/// The lowest level whose decoders take this size at the frame rate.
fn level(shape: Shape) -> u8 {
    const LEVELS: [(u8, usize, usize); 10] = [
        (31, 108_000, 3_600),
        (32, 216_000, 5_120),
        (40, 245_760, 8_192),
        (42, 522_240, 8_704),
        (50, 589_824, 22_080),
        (51, 983_040, 36_864),
        (52, 2_073_600, 36_864),
        (60, 4_177_920, 139_264),
        (61, 8_355_840, 139_264),
        (62, 16_711_680, 139_264),
    ];
    let blocks = blocks(shape);
    let (width, height) = shape.coded(BLOCKS);
    let side = width.max(height) / BLOCK;
    LEVELS
        .into_iter()
        .find(|(_, per_second, per_frame)| {
            blocks <= *per_frame
                && blocks * FRAME_RATE as usize <= *per_second
                && side * side <= 8 * per_frame
        })
        .map_or(62, |(level, ..)| level)
}

fn sequence_set(shape: Shape) -> Vec<u8> {
    let mut bits = Bits::default();
    bits.put(HIGH, 8);
    bits.put(CONSTRAINED, 8);
    bits.put(u32::from(level(shape)), 8);
    // seq_parameter_set_id, then 4:2:0 at 8 bits, without lossless blocks or scaling matrices.
    bits.number(0);
    bits.number(1);
    bits.number(0);
    bits.number(0);
    bits.flag(false);
    bits.flag(false);
    bits.number(LOG2_FRAME_NUMBERS as usize - 4);
    // pic_order_cnt_type 2: frames show in the order they come.
    bits.number(2);
    // max_num_ref_frames, then gaps_in_frame_num_value_allowed_flag.
    bits.number(1);
    bits.flag(false);
    let (width, height) = shape.coded(BLOCKS);
    bits.number(width / BLOCK - 1);
    bits.number(height / BLOCK - 1);
    // frame_mbs_only_flag, direct_8x8_inference_flag.
    bits.flag(true);
    bits.flag(true);
    let (right, bottom) = shape.crop(BLOCKS);
    bits.flag(right > 0 || bottom > 0);
    if right > 0 || bottom > 0 {
        bits.number(0);
        bits.number(right);
        bits.number(0);
        bits.number(bottom);
    }
    // vui_parameters_present_flag
    bits.flag(true);
    write_usability(&mut bits);
    bits.ended(&[SEQUENCE_SET])
}

/// Video usability information: BT.709 limited range, and no frame reordering.
fn write_usability(bits: &mut Bits) {
    // aspect_ratio_info_present_flag, overscan_info_present_flag.
    bits.flag(false);
    bits.flag(false);
    // video_signal_type_present_flag, video_format, video_full_range_flag, then colour
    // primaries, transfer characteristics and matrix coefficients.
    bits.flag(true);
    bits.put(UNSPECIFIED_FORMAT, 3);
    bits.flag(false);
    bits.flag(true);
    bits.put(BT709, 8);
    bits.put(BT709, 8);
    bits.put(BT709, 8);
    // chroma_loc_info, timing_info, NAL and VCL HRD parameters, pic_struct.
    for _ in 0..5 {
        bits.flag(false);
    }
    // bitstream_restriction_flag, motion_vectors_over_pic_boundaries_flag,
    // max_bytes_per_pic_denom and max_bits_per_mb_denom without limits, the longest motion
    // vectors, then max_num_reorder_frames 0 and max_dec_frame_buffering 1.
    bits.flag(true);
    bits.flag(true);
    bits.number(0);
    bits.number(0);
    bits.number(LONGEST_MOTION);
    bits.number(LONGEST_MOTION);
    bits.number(0);
    bits.number(1);
}

fn picture_set() -> Vec<u8> {
    let mut bits = Bits::default();
    // pic_parameter_set_id, seq_parameter_set_id, CABAC, no field order, one slice group.
    bits.number(0);
    bits.number(0);
    bits.flag(true);
    bits.flag(false);
    bits.number(0);
    // One reference in each list by default, without weighted prediction.
    bits.number(0);
    bits.number(0);
    bits.flag(false);
    bits.put(0, 2);
    // pic_init_qp_minus26 for `INITIAL_QUANTIZER`, pic_init_qs_minus26, chroma_qp_index_offset.
    bits.number(0);
    bits.number(0);
    bits.number(0);
    // deblocking_filter_control_present_flag, constrained_intra_pred_flag,
    // redundant_pic_cnt_present_flag, transform_8x8_mode_flag, pic_scaling_matrix_present_flag,
    // second_chroma_qp_index_offset.
    bits.flag(true);
    bits.flag(false);
    bits.flag(false);
    bits.flag(true);
    bits.flag(false);
    bits.number(0);
    bits.ended(&[PICTURE_SET])
}

/// The header of a frame's one slice, whose data the driver writes after it, and its length in
/// bits.
fn slice_header(frame: &Frame, frame_number: u32) -> (Vec<u8>, usize) {
    let mut bits = Bits::default();
    // first_mb_in_slice, slice_type, pic_parameter_set_id, frame_num.
    bits.number(0);
    bits.number(usize::from(if frame.key { SLICE_I } else { SLICE_P }));
    bits.number(0);
    bits.put(frame_number, LOG2_FRAME_NUMBERS);
    if frame.key {
        // idr_pic_id, then dec_ref_pic_marking: no_output_of_prior_pics_flag and
        // long_term_reference_flag.
        bits.number(usize::from(frame.idr));
        bits.flag(false);
        bits.flag(false);
    } else {
        // num_ref_idx_active_override_flag, ref_pic_list_modification_flag_l0, then
        // dec_ref_pic_marking: adaptive_ref_pic_marking_mode_flag; then cabac_init_idc.
        bits.flag(false);
        bits.flag(false);
        bits.flag(false);
        bits.number(0);
    }
    bits.signed(quantizer_delta(frame.quantizer));
    // disable_deblocking_filter_idc, slice_alpha_c0_offset_div2, slice_beta_offset_div2.
    bits.number(0);
    bits.number(0);
    bits.number(0);
    bits.unit(&[if frame.key { IDR_SLICE } else { SLICE }])
}
