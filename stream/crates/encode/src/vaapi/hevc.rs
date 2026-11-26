//! HEVC's side of a session. As for H.264, the encoder writes the parameter sets and slice
//! headers itself. The sequence set turns on the coding tools and block sizes that the driver
//! reports, since the driver codes with them; drivers that report none keep to H.264. Intel's
//! encoders code inter frames only as B slices with the frame before in both lists.

use std::ffi::c_int;

use super::{
    bits::Bits,
    h264::packs_headers,
    session::{Frame, PACKED_SETS, PACKED_SLICE, Shape, packed},
    va::{
        self, Display, Id, OUT_OF_RANGE, PictureHevc, PictureParametersHevc, SequenceHevc,
        SliceHevc, Status,
    },
};
use crate::FRAME_RATE;

const INITIAL_QUANTIZER: u8 = 26;
/// Slices code the low bits of the picture order count.
const LOG2_ORDER_LSB: u32 = 8;
const INTRA_PERIOD: u32 = 1 << 15;
const MAIN: u8 = 1;
const UNSPECIFIED_FORMAT: u32 = 5;
const BT709: u32 = 1;
const VIDEO_SET: u8 = 32;
const SEQUENCE_SET: u8 = 33;
const PICTURE_SET: u8 = 34;
const IDR: u8 = 19;
const TRAILING: u8 = 1;
const SLICE_B: u8 = 0;
const SLICE_P: u8 = 1;
const SLICE_I: u8 = 2;
/// `coding_type` for I and P pictures; B slices that stand for P frames keep P's.
const CODING_I: u32 = 1;
const CODING_P: u32 = 2;
/// No reference, for `collocated_ref_pic_index` without temporal motion vector prediction.
const NO_COLLOCATED: u8 = 0xff;
/// The shifts of `VAConfigAttribValEncHEVCFeatures`'s two-bit fields.
const AMP: u32 = 4;
const SAO: u32 = 6;
const TEMPORAL_MOTION: u32 = 10;
const TRANSFORM_SKIP: u32 = 20;

/// What a display's HEVC encoder takes.
#[derive(Clone, Copy)]
pub(super) struct Capabilities {
    pub(super) entrypoint: c_int,
    /// `VAConfigAttribValEncHEVCFeatures` and `VAConfigAttribValEncHEVCBlockSizes`.
    features: u32,
    blocks: u32,
    both_lists: bool,
    /// What the coded width and height must be multiples of, beside the smallest block.
    alignment: (usize, usize),
}

pub(super) fn capabilities(display: &Display) -> Option<Capabilities> {
    let offered = display.entrypoints(va::HEVC_MAIN);
    let entrypoint = [va::ENCODE_LOW_POWER, va::ENCODE]
        .into_iter()
        .find(|entrypoint| offered.contains(entrypoint))
        .filter(|entrypoint| packs_headers(display, va::HEVC_MAIN, *entrypoint))?;
    let [prediction, features, blocks] = display.attributes(
        va::HEVC_MAIN,
        entrypoint,
        [
            va::ATTRIBUTE_PREDICTION,
            va::ATTRIBUTE_HEVC_FEATURES,
            va::ATTRIBUTE_HEVC_BLOCKS,
        ],
    )?;
    if features == va::NOT_SUPPORTED || blocks == va::NOT_SUPPORTED {
        return None;
    }
    Some(Capabilities {
        entrypoint,
        features,
        blocks,
        both_lists: prediction != va::NOT_SUPPORTED && prediction & va::BOTH_LISTS != 0,
        alignment: display.alignment(va::HEVC_MAIN, entrypoint),
    })
}

/// The coding tools and block sizes that a session's parameter sets turn on.
impl Capabilities {
    fn feature(self, shift: u32) -> bool {
        (self.features >> shift) & 0b11 != 0
    }

    fn block_field(self, shift: u32) -> u32 {
        (self.blocks >> shift) & 0b11
    }

    fn amp(self) -> bool {
        self.feature(AMP)
    }

    fn sao(self) -> bool {
        self.feature(SAO)
    }

    fn temporal_motion(self) -> bool {
        self.feature(TEMPORAL_MOTION)
    }

    fn transform_skip(self) -> bool {
        self.feature(TRANSFORM_SKIP)
    }

    fn log2_tree(self) -> u32 {
        self.block_field(0) + 3
    }

    fn log2_min_block(self) -> u32 {
        self.block_field(4) + 3
    }

    fn log2_max_transform(self) -> u32 {
        self.block_field(6) + 2
    }

    fn log2_min_transform(self) -> u32 {
        self.block_field(8) + 2
    }

    fn depth_inter(self) -> u32 {
        self.block_field(10)
    }

    fn depth_intra(self) -> u32 {
        self.block_field(14)
    }

    /// What the coded width and height are multiples of.
    fn block(self) -> (usize, usize) {
        let block = 1 << self.log2_min_block();
        (block.max(self.alignment.0), block.max(self.alignment.1))
    }
}

pub(super) struct Syntax {
    tools: Capabilities,
    trees: u32,
    sequence: SequenceHevc,
    parameter_sets: Vec<u8>,
}

impl Syntax {
    pub(super) fn new(shape: Shape, tools: Capabilities) -> Result<Self, Status> {
        let tree = 1 << tools.log2_tree();
        let (width, height) = shape.coded(tools.block());
        let trees = width.div_ceil(tree) * height.div_ceil(tree);
        Ok(Self {
            tools,
            trees: u32::try_from(trees).map_err(|_| OUT_OF_RANGE)?,
            sequence: sequence(shape, tools)?,
            parameter_sets: [
                video_set(shape),
                sequence_set(shape, tools),
                picture_set(tools),
            ]
            .concat(),
        })
    }

    /// What the coded width and height are multiples of.
    pub(super) fn alignment(&self) -> (usize, usize) {
        self.tools.block()
    }

    /// Makes the buffers that describe a frame.
    pub(super) fn buffers(
        &self,
        display: &Display,
        context: Id,
        frame: &Frame,
        buffers: &mut Vec<Id>,
    ) -> Result<(), Status> {
        let tools = self.tools;
        let order = frame.number.cast_signed();
        let current = picture(frame.current, order, 0);
        let previous = (!frame.key)
            .then(|| picture(frame.previous, order.wrapping_sub(1), va::CURRENT_BEFORE));
        if frame.key {
            buffers.push(display.create_parameters(context, va::SEQUENCE, &self.sequence)?);
        }
        let coding = if frame.key { CODING_I } else { CODING_P };
        let mut parameters = PictureParametersHevc {
            current,
            references: [PictureHevc::NONE; 15],
            coded: frame.coded,
            collocated_reference: if tools.temporal_motion() {
                0
            } else {
                NO_COLLOCATED
            },
            initial_quantizer: INITIAL_QUANTIZER,
            nal_unit_type: if frame.key { IDR } else { TRAILING },
            // idr_pic_flag, coding_type, reference_pic_flag, transform_skip_enabled_flag.
            fields: u32::from(frame.key)
                | (coding << 1)
                | (1 << 4)
                | (u32::from(tools.transform_skip()) << 8),
            ..PictureParametersHevc::default()
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
        let slice_type = match (frame.key, tools.both_lists) {
            (true, _) => SLICE_I,
            (false, true) => SLICE_B,
            (false, false) => SLICE_P,
        };
        let (header, length) = slice_header(frame, tools, slice_type);
        packed(display, context, PACKED_SLICE, &header, length, buffers)?;
        let temporal_motion = tools.temporal_motion() && !frame.key;
        let mut slice = SliceHevc {
            trees: self.trees,
            slice_type,
            list0: [PictureHevc::NONE; 15],
            list1: [PictureHevc::NONE; 15],
            merge_candidates: 5,
            quantizer_delta: quantizer_delta(frame.quantizer),
            // last_slice_of_pic_flag, slice_temporal_mvp_enabled_flag, slice_sao_luma_flag,
            // slice_sao_chroma_flag, collocated_from_l0_flag.
            fields: 1
                | (u32::from(temporal_motion) << 4)
                | (u32::from(tools.sao()) << 5)
                | (u32::from(tools.sao()) << 6)
                | (1 << 13),
            ..SliceHevc::default()
        };
        if let Some(previous) = previous {
            slice.list0[0] = previous;
            if slice_type == SLICE_B {
                slice.list1[0] = previous;
            }
        }
        buffers.push(display.create_parameters(context, va::SLICE, &slice)?);
        Ok(())
    }
}

fn picture(surface: Id, order: i32, flags: u32) -> PictureHevc {
    PictureHevc {
        surface,
        order,
        flags,
        reserved: [0; 4],
    }
}

fn quantizer_delta(quantizer: u8) -> i8 {
    quantizer.cast_signed() - INITIAL_QUANTIZER.cast_signed()
}

fn sequence(shape: Shape, tools: Capabilities) -> Result<SequenceHevc, Status> {
    let (width, height) = shape.coded(tools.block());
    Ok(SequenceHevc {
        profile: MAIN,
        level: level(shape),
        intra_period: INTRA_PERIOD,
        idr_period: INTRA_PERIOD,
        ip_period: 1,
        width: u16::try_from(width).map_err(|_| OUT_OF_RANGE)?,
        height: u16::try_from(height).map_err(|_| OUT_OF_RANGE)?,
        // 4:2:0, AMP, SAO, temporal motion vector prediction, a low-delay sequence.
        fields: 1
            | (u32::from(tools.amp()) << 11)
            | (u32::from(tools.sao()) << 12)
            | (u32::from(tools.temporal_motion()) << 15)
            | (1 << 16),
        log2_min_block_minus3: u8::try_from(tools.log2_min_block() - 3).unwrap_or_default(),
        log2_block_range: u8::try_from(tools.log2_tree() - tools.log2_min_block())
            .unwrap_or_default(),
        log2_min_transform_minus2: u8::try_from(tools.log2_min_transform() - 2).unwrap_or_default(),
        log2_transform_range: u8::try_from(tools.log2_max_transform() - tools.log2_min_transform())
            .unwrap_or_default(),
        depth_inter: u8::try_from(tools.depth_inter()).unwrap_or_default(),
        depth_intra: u8::try_from(tools.depth_intra()).unwrap_or_default(),
        vui_present: 1,
        ..SequenceHevc::default()
    })
}

/// The lowest level whose decoders take this size at the frame rate, times 30.
fn level(shape: Shape) -> u8 {
    const LEVELS: [(u8, usize, usize); 9] = [
        (93, 983_040, 33_177_600),
        (120, 2_228_224, 66_846_720),
        (123, 2_228_224, 133_693_440),
        (150, 8_912_896, 267_386_880),
        (153, 8_912_896, 534_773_760),
        (156, 8_912_896, 1_069_547_520),
        (180, 35_651_584, 1_069_547_520),
        (183, 35_651_584, 2_139_095_040),
        (186, 35_651_584, 4_278_190_080),
    ];
    let samples = shape.width * shape.height;
    let side = shape.width.max(shape.height);
    LEVELS
        .into_iter()
        .find(|(_, per_picture, per_second)| {
            samples <= *per_picture
                && samples * FRAME_RATE as usize <= *per_second
                && side * side <= 8 * per_picture
        })
        .map_or(186, |(level, ..)| level)
}

fn header(unit_type: u8) -> [u8; 2] {
    [unit_type << 1, 1]
}

/// Main profile, main tier, progressive frames only.
fn write_profile(bits: &mut Bits, shape: Shape) {
    bits.put(0, 2);
    bits.flag(false);
    bits.put(u32::from(MAIN), 5);
    // Compatible with Main and Main 10.
    bits.put(0x6000_0000, 32);
    // progressive_source, interlaced_source, non_packed_constraint, frame_only_constraint.
    bits.put(0b1001, 4);
    // 43 reserved bits, then general_inbld_flag.
    bits.put(0, 32);
    bits.put(0, 12);
    bits.put(u32::from(level(shape)), 8);
}

fn video_set(shape: Shape) -> Vec<u8> {
    let mut bits = Bits::default();
    // vps_video_parameter_set_id, base layer internal and available, one layer of one sub-layer,
    // temporal_id_nesting, reserved 0xffff.
    bits.put(0, 4);
    bits.put(0b11, 2);
    bits.put(0, 6);
    bits.put(0, 3);
    bits.flag(true);
    bits.put(0xffff, 16);
    write_profile(&mut bits, shape);
    write_ordering(&mut bits);
    // vps_max_layer_id, vps_num_layer_sets_minus1, timing info, extension.
    bits.put(0, 6);
    bits.number(0);
    bits.flag(false);
    bits.flag(false);
    bits.ended(&header(VIDEO_SET))
}

/// One frame in the decoder besides the one it decodes, no reordering, and no added latency.
fn write_ordering(bits: &mut Bits) {
    bits.flag(false);
    bits.number(1);
    bits.number(0);
    bits.number(0);
}

fn sequence_set(shape: Shape, tools: Capabilities) -> Vec<u8> {
    let mut bits = Bits::default();
    // sps_video_parameter_set_id, one sub-layer, temporal_id_nesting.
    bits.put(0, 4);
    bits.put(0, 3);
    bits.flag(true);
    write_profile(&mut bits, shape);
    // sps_seq_parameter_set_id, 4:2:0, then the coded size and the window that crops it.
    bits.number(0);
    bits.number(1);
    let (width, height) = shape.coded(tools.block());
    bits.number(width);
    bits.number(height);
    let (right, bottom) = shape.crop(tools.block());
    bits.flag(right > 0 || bottom > 0);
    if right > 0 || bottom > 0 {
        bits.number(0);
        bits.number(right);
        bits.number(0);
        bits.number(bottom);
    }
    // 8 bits for luma and chroma, then the length of the order count's low bits.
    bits.number(0);
    bits.number(0);
    bits.number(LOG2_ORDER_LSB as usize - 4);
    write_ordering(&mut bits);
    bits.number(tools.log2_min_block() as usize - 3);
    bits.number((tools.log2_tree() - tools.log2_min_block()) as usize);
    bits.number(tools.log2_min_transform() as usize - 2);
    bits.number((tools.log2_max_transform() - tools.log2_min_transform()) as usize);
    bits.number(tools.depth_inter() as usize);
    bits.number(tools.depth_intra() as usize);
    // scaling_list_enabled_flag, amp_enabled_flag, sample_adaptive_offset_enabled_flag,
    // pcm_enabled_flag, num_short_term_ref_pic_sets, long_term_ref_pics_present_flag,
    // sps_temporal_mvp_enabled_flag, strong_intra_smoothing_enabled_flag.
    bits.flag(false);
    bits.flag(tools.amp());
    bits.flag(tools.sao());
    bits.flag(false);
    bits.number(0);
    bits.flag(false);
    bits.flag(tools.temporal_motion());
    bits.flag(false);
    // vui_parameters_present_flag, then aspect ratio and overscan absent, and the video signal:
    // unspecified format, limited range, BT.709 primaries, transfer and matrix.
    bits.flag(true);
    bits.flag(false);
    bits.flag(false);
    bits.flag(true);
    bits.put(UNSPECIFIED_FORMAT, 3);
    bits.flag(false);
    bits.flag(true);
    bits.put(BT709, 8);
    bits.put(BT709, 8);
    bits.put(BT709, 8);
    // chroma_loc_info, neutral_chroma, field_seq, frame_field_info, default_display_window,
    // timing_info, bitstream_restriction, then sps_extension_present_flag.
    for _ in 0..8 {
        bits.flag(false);
    }
    bits.ended(&header(SEQUENCE_SET))
}

fn picture_set(tools: Capabilities) -> Vec<u8> {
    let mut bits = Bits::default();
    // pps_pic_parameter_set_id, pps_seq_parameter_set_id, dependent_slice_segments_enabled_flag,
    // output_flag_present_flag, num_extra_slice_header_bits, sign_data_hiding_enabled_flag,
    // cabac_init_present_flag, one reference in each list by default, init_qp_minus26 for
    // `INITIAL_QUANTIZER`, constrained_intra_pred_flag.
    bits.number(0);
    bits.number(0);
    bits.flag(false);
    bits.flag(false);
    bits.put(0, 3);
    bits.flag(false);
    bits.flag(false);
    bits.number(0);
    bits.number(0);
    bits.number(0);
    bits.flag(false);
    bits.flag(tools.transform_skip());
    // cu_qp_delta_enabled_flag, pps_cb_qp_offset, pps_cr_qp_offset,
    // pps_slice_chroma_qp_offsets_present_flag, weighted_pred_flag, weighted_bipred_flag,
    // transquant_bypass_enabled_flag, tiles_enabled_flag, entropy_coding_sync_enabled_flag,
    // pps_loop_filter_across_slices_enabled_flag, deblocking_filter_control_present_flag,
    // pps_scaling_list_data_present_flag, lists_modification_present_flag.
    bits.flag(false);
    bits.number(0);
    bits.number(0);
    for _ in 0..10 {
        bits.flag(false);
    }
    // log2_parallel_merge_level_minus2, slice_segment_header_extension_present_flag,
    // pps_extension_present_flag.
    bits.number(0);
    bits.flag(false);
    bits.flag(false);
    bits.ended(&header(PICTURE_SET))
}

/// The header of a frame's one slice segment, whose data the driver writes after it, and its
/// length in bits.
fn slice_header(frame: &Frame, tools: Capabilities, slice_type: u8) -> (Vec<u8>, usize) {
    let mut bits = Bits::default();
    // first_slice_segment_in_pic_flag, then no_output_of_prior_pics_flag for an IDR.
    bits.flag(true);
    if frame.key {
        bits.flag(false);
    }
    // slice_pic_parameter_set_id, slice_type.
    bits.number(0);
    bits.number(usize::from(slice_type));
    if !frame.key {
        // slice_pic_order_cnt_lsb, then a reference set of its own: the frame before.
        bits.put(frame.number % (1 << LOG2_ORDER_LSB), LOG2_ORDER_LSB);
        bits.flag(false);
        bits.number(1);
        bits.number(0);
        bits.number(0);
        bits.flag(true);
        if tools.temporal_motion() {
            bits.flag(true);
        }
    }
    if tools.sao() {
        bits.flag(true);
        bits.flag(true);
    }
    if !frame.key {
        // num_ref_idx_active_override_flag, then mvd_l1_zero_flag for B slices.
        bits.flag(false);
        if slice_type == SLICE_B {
            bits.flag(false);
        }
        // collocated_from_l0_flag, which P slices infer.
        if tools.temporal_motion() && slice_type == SLICE_B {
            bits.flag(true);
        }
        // five_minus_max_num_merge_cand.
        bits.number(0);
    }
    bits.signed(quantizer_delta(frame.quantizer));
    bits.align();
    let unit_type = if frame.key { IDR } else { TRAILING };
    bits.unit(&header(unit_type))
}
