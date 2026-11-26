//! The headers of each frame, which the encoder writes rather than the driver: the parameter
//! sets that start each keyframe describe the colours and tell decoders that frames come in the
//! order they show, so they show each frame as soon as it arrives. Mesa takes the slice's unit
//! type only from a slice header written for it.

use crate::{FRAME_RATE, nal::escape};

/// `frame_num` counts frames modulo `1 << LOG2_FRAME_NUMBERS`.
pub(super) const LOG2_FRAME_NUMBERS: u32 = 8;
/// The quantizer a slice's delta starts from.
pub(super) const INITIAL_QUANTIZER: u8 = 26;
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
pub(super) const SLICE_P: u8 = 0;
pub(super) const SLICE_I: u8 = 2;
const BLOCK: usize = 16;

/// The size of the pictures that a stream carries.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Shape {
    pub(super) width: usize,
    pub(super) height: usize,
}

impl Shape {
    pub(super) fn width_in_blocks(self) -> usize {
        self.width.div_ceil(BLOCK)
    }

    pub(super) fn height_in_blocks(self) -> usize {
        self.height.div_ceil(BLOCK)
    }

    pub(super) fn blocks(self) -> usize {
        self.width_in_blocks() * self.height_in_blocks()
    }

    /// The size that the encoder codes, in whole macroblocks.
    pub(super) fn coded(self) -> (usize, usize) {
        (
            self.width_in_blocks() * BLOCK,
            self.height_in_blocks() * BLOCK,
        )
    }

    /// What decoders crop off the right and bottom of the coded size, in pairs of pixels.
    pub(super) fn crop(self) -> (usize, usize) {
        let (width, height) = self.coded();
        ((width - self.width) / 2, (height - self.height) / 2)
    }

    /// The lowest level whose decoders take this size at the frame rate.
    pub(super) fn level(self) -> u8 {
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
        let blocks = self.blocks();
        let side = self.width_in_blocks().max(self.height_in_blocks());
        LEVELS
            .into_iter()
            .find(|(_, per_second, per_frame)| {
                blocks <= *per_frame
                    && blocks * FRAME_RATE as usize <= *per_second
                    && side * side <= 8 * per_frame
            })
            .map_or(62, |(level, ..)| level)
    }
}

/// Writes a payload's bits, the most significant first.
#[derive(Default)]
struct Bits {
    bytes: Vec<u8>,
    count: usize,
}

impl Bits {
    fn put(&mut self, value: u32, width: u32) {
        for shift in (0..width).rev() {
            if self.count.is_multiple_of(8) {
                self.bytes.push(0);
            }
            if (value >> shift) & 1 == 1
                && let Some(last) = self.bytes.last_mut()
            {
                *last |= 0x80 >> (self.count % 8);
            }
            self.count += 1;
        }
    }

    fn flag(&mut self, on: bool) {
        self.put(u32::from(on), 1);
    }

    /// An unsigned Exp-Golomb code, `ue(v)`. Zero is also the code of a signed zero, `se(v)`.
    fn number(&mut self, value: usize) {
        let coded = u32::try_from(value + 1).unwrap_or(u32::MAX);
        let width = coded.bit_width();
        self.put(0, width - 1);
        self.put(coded, width);
    }

    fn signed(&mut self, value: i8) {
        let magnitude = usize::from(value.unsigned_abs());
        self.number(if value > 0 {
            2 * magnitude - 1
        } else {
            2 * magnitude
        });
    }

    /// A unit after a start code, of `header` and the payload so far, and its length in bits.
    fn unit(&self, header: u8) -> (Vec<u8>, usize) {
        let mut unit = vec![0, 0, 0, 1];
        unit.extend(escape(header, &self.bytes));
        let unused = (8 - self.count % 8) % 8;
        let length = unit.len() * 8 - unused;
        (unit, length)
    }

    /// Ends the payload with its stop bit and makes it a unit.
    fn ended(mut self, header: u8) -> Vec<u8> {
        self.put(1, 1);
        self.unit(header).0
    }
}

/// The sequence and picture parameter sets, one unit after the other.
pub(super) fn parameter_sets(shape: Shape) -> Vec<u8> {
    let mut sets = sequence_set(shape);
    sets.extend(picture_set());
    sets
}

fn sequence_set(shape: Shape) -> Vec<u8> {
    let mut bits = Bits::default();
    bits.put(HIGH, 8);
    bits.put(CONSTRAINED, 8);
    bits.put(u32::from(shape.level()), 8);
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
    bits.number(shape.width_in_blocks() - 1);
    bits.number(shape.height_in_blocks() - 1);
    // frame_mbs_only_flag, direct_8x8_inference_flag.
    bits.flag(true);
    bits.flag(true);
    let (right, bottom) = shape.crop();
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
    bits.ended(SEQUENCE_SET)
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
    bits.ended(PICTURE_SET)
}

/// The header of a frame's one slice, whose data the driver writes after it, and its length in
/// bits: an IDR, or a P slice on the frame before, at `quantizer`.
pub(super) fn slice_header(
    key: bool,
    frame_number: u32,
    idr: u16,
    quantizer: u8,
) -> (Vec<u8>, usize) {
    let mut bits = Bits::default();
    // first_mb_in_slice, slice_type, pic_parameter_set_id, frame_num.
    bits.number(0);
    bits.number(usize::from(if key { SLICE_I } else { SLICE_P }));
    bits.number(0);
    bits.put(frame_number, LOG2_FRAME_NUMBERS);
    if key {
        // idr_pic_id, then dec_ref_pic_marking: no_output_of_prior_pics_flag and
        // long_term_reference_flag.
        bits.number(usize::from(idr));
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
    bits.signed(quantizer.cast_signed() - INITIAL_QUANTIZER.cast_signed());
    // disable_deblocking_filter_idc, slice_alpha_c0_offset_div2, slice_beta_offset_div2.
    bits.number(0);
    bits.number(0);
    bits.number(0);
    bits.unit(if key { IDR_SLICE } else { SLICE })
}
