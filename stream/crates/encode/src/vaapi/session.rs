//! A VA-API H.264 encoder for pictures of one size. The GPU converts each BGRA picture to BT.709
//! limited-range NV12, then encodes it as an IDR or as a P frame on the frame before.

use std::{ffi::c_int, rc::Rc};

use super::{
    headers::{self, INITIAL_QUANTIZER, LOG2_FRAME_NUMBERS, SLICE_I, SLICE_P, Shape},
    va::{
        self, ColourProperties, ConfigAttribute, Display, Id, Image, NO_ID, OUT_OF_RANGE,
        PackedHeader, PictureH264, PictureParametersH264, Pipeline, Rectangle, SequenceH264,
        SliceH264, Status,
    },
};

const FRAME_NUMBERS: u32 = 1 << LOG2_FRAME_NUMBERS;
/// Keyframes come only when asked for, but drivers want a period, and some divide by it.
const INTRA_PERIOD: u32 = 1 << 15;
/// `seq_fields`: 4:2:0, progressive, 8x8 direct inference, `frame_num`'s length, and
/// `pic_order_cnt_type` 2.
const SEQUENCE_FIELDS: u32 = 1 | (1 << 2) | (1 << 5) | ((LOG2_FRAME_NUMBERS - 4) << 6) | (2 << 10);
/// `vui_fields`: bitstream restrictions with the longest motion vectors, which may cross the
/// picture's edges.
const USABILITY_FIELDS: u32 = (1 << 2) | (15 << 3) | (15 << 8) | (1 << 15);
/// `pic_fields` beside `idr_pic_flag`: a reference picture, CABAC, 8x8 transforms, and
/// deblocking that slices control.
const PICTURE_FIELDS: u32 = (1 << 1) | (1 << 3) | (1 << 8) | (1 << 9);
/// `VAEncPackedHeaderSequence` and `VAEncPackedHeaderSlice`.
const PACKED_SETS: u32 = 1;
const PACKED_SLICE: u32 = 3;
const BT709: u8 = 1;
/// Room for the parameter sets beyond a frame as large as the raw picture.
const HEADROOM: usize = 64 * 1024;

/// The entrypoint for H.264 that a display offers, if it has all that a session needs. Low
/// power comes first: Intel graphics encode faster in it, and newer ones only in it.
pub(super) fn entrypoint(display: &Display) -> Option<c_int> {
    let offered = display.entrypoints(va::H264_HIGH);
    let entrypoint = [va::ENCODE_LOW_POWER, va::ENCODE]
        .into_iter()
        .find(|entrypoint| offered.contains(entrypoint))?;
    let [format, rate_control, packed] = display.attributes(
        va::H264_HIGH,
        entrypoint,
        [
            va::ATTRIBUTE_FORMAT,
            va::ATTRIBUTE_RATE_CONTROL,
            va::ATTRIBUTE_PACKED_HEADERS,
        ],
    )?;
    let headers = va::PACKED_SEQUENCE | va::PACKED_SLICE;
    let fits = format & va::FORMAT_YUV420 != 0
        && rate_control & va::CONSTANT_QUANTIZER != 0
        && packed & headers == headers;
    let converts = display
        .entrypoints(va::NO_PROFILE)
        .contains(&va::VIDEO_PROCESSING);
    (fits && converts).then_some(entrypoint)
}

pub(super) struct Session {
    display: Rc<Display>,
    shape: Shape,
    converter_config: Id,
    converter: Id,
    encoder_config: Id,
    encoder: Id,
    /// The BGRX surface that pictures are copied into, through the image that maps it.
    source: Id,
    image: Option<Image>,
    /// The NV12 picture that the encoder encodes, which passes encode again.
    picture: Id,
    /// What decoders make of the frames, in turns, so the frame before is the reference.
    reconstructed: [Id; 2],
    coded: Id,
    sequence: SequenceH264,
    parameter_sets: Vec<u8>,
    /// Frames since the last IDR, which order them; IDRs, which number them.
    frames: u32,
    idrs: u16,
}

impl Session {
    pub(super) fn start(
        display: &Rc<Display>,
        entrypoint: c_int,
        (width, height): (usize, usize),
    ) -> Result<Self, Status> {
        let shape = Shape { width, height };
        let mut session = Self {
            display: Rc::clone(display),
            shape,
            converter_config: NO_ID,
            converter: NO_ID,
            encoder_config: NO_ID,
            encoder: NO_ID,
            source: NO_ID,
            image: None,
            picture: NO_ID,
            reconstructed: [NO_ID; 2],
            coded: NO_ID,
            sequence: sequence(shape),
            parameter_sets: headers::parameter_sets(shape),
            frames: 0,
            idrs: 0,
        };
        session.open(entrypoint)?;
        Ok(session)
    }

    fn open(&mut self, entrypoint: c_int) -> Result<(), Status> {
        let display = Rc::clone(&self.display);
        let coded = self.shape.coded();
        self.encoder_config = display.create_config(
            va::H264_HIGH,
            entrypoint,
            &[
                ConfigAttribute {
                    kind: va::ATTRIBUTE_FORMAT,
                    value: va::FORMAT_YUV420,
                },
                ConfigAttribute {
                    kind: va::ATTRIBUTE_RATE_CONTROL,
                    value: va::CONSTANT_QUANTIZER,
                },
                ConfigAttribute {
                    kind: va::ATTRIBUTE_PACKED_HEADERS,
                    value: va::PACKED_SEQUENCE | va::PACKED_SLICE,
                },
            ],
        )?;
        self.converter_config = display.create_config(va::NO_PROFILE, va::VIDEO_PROCESSING, &[])?;
        let [picture, first, second] =
            display.create_surfaces(va::FORMAT_YUV420, va::FOURCC_NV12, coded)?;
        (self.picture, self.reconstructed) = (picture, [first, second]);
        let size = (self.shape.width, self.shape.height);
        [self.source] = display.create_surfaces(va::FORMAT_RGB32, va::FOURCC_BGRX, size)?;
        self.image = Some(display.derive_image(self.source)?);
        self.encoder =
            display.create_context(self.encoder_config, coded, &[picture, first, second])?;
        self.converter = display.create_context(self.converter_config, coded, &[picture])?;
        self.coded = display.create_coded(self.encoder, coded.0 * coded.1 * 3 / 2 + HEADROOM)?;
        Ok(())
    }

    pub(super) fn size(&self) -> (usize, usize) {
        (self.shape.width, self.shape.height)
    }

    /// Copies a BGRA picture onto the GPU and converts it into the picture that frames encode.
    pub(super) fn load(&self, bgra: &[u8]) -> Result<(), Status> {
        let Some(image) = &self.image else {
            return Err(OUT_OF_RANGE);
        };
        let size = self.size();
        self.display.write(image, size, bgra)?;
        let region = Rectangle {
            x: 0,
            y: 0,
            width: u16::try_from(size.0).map_err(|_| OUT_OF_RANGE)?,
            height: u16::try_from(size.1).map_err(|_| OUT_OF_RANGE)?,
        };
        // The same standard on both sides converts only the colour matrix, as viewers undo it:
        // Mesa would otherwise also convert sRGB's transfer curve.
        let pipeline = Pipeline {
            surface: self.source,
            surface_region: &raw const region,
            surface_standard: va::STANDARD_BT709,
            output_region: &raw const region,
            output_standard: va::STANDARD_BT709,
            input_colours: ColourProperties {
                range: va::RANGE_FULL,
                ..ColourProperties::default()
            },
            output_colours: ColourProperties {
                range: va::RANGE_LIMITED,
                primaries: BT709,
                transfer: BT709,
                matrix: BT709,
                ..ColourProperties::default()
            },
            ..Pipeline::default()
        };
        let buffer = self
            .display
            .create_parameters(self.converter, va::PIPELINE, &pipeline)?;
        let converted = self.display.render(self.converter, self.picture, &[buffer]);
        self.display.destroy_buffer(buffer);
        converted
    }

    /// Encodes the picture at `quantizer`, as an IDR when `key`, else as a P frame on the frame
    /// before; returns the frame's units.
    pub(super) fn encode(&mut self, key: bool, quantizer: u8) -> Result<Vec<u8>, Status> {
        if key {
            self.frames = 0;
            self.idrs = self.idrs.wrapping_add(1);
        }
        let mut buffers = Vec::with_capacity(7);
        let rendered = self
            .parameters(key, quantizer, &mut buffers)
            .and_then(|()| self.display.render(self.encoder, self.picture, &buffers));
        for buffer in buffers {
            self.display.destroy_buffer(buffer);
        }
        rendered?;
        self.display.sync(self.picture)?;
        let mut frame = Vec::new();
        self.display.read_coded(self.coded, &mut frame)?;
        self.frames = self.frames.wrapping_add(1);
        Ok(frame)
    }

    /// Makes the buffers that describe the next frame.
    fn parameters(&self, key: bool, quantizer: u8, buffers: &mut Vec<Id>) -> Result<(), Status> {
        let display = &self.display;
        let current = self.frame(self.frames, 0);
        let previous =
            (!key).then(|| self.frame(self.frames.wrapping_sub(1), va::SHORT_TERM_REFERENCE));
        if key {
            buffers.push(display.create_parameters(self.encoder, va::SEQUENCE, &self.sequence)?);
        }
        let mut picture = PictureParametersH264 {
            current,
            references: [PictureH264::NONE; 16],
            coded: self.coded,
            frame_number: u16::try_from(current.frame_index).map_err(|_| OUT_OF_RANGE)?,
            initial_quantizer: INITIAL_QUANTIZER,
            fields: u32::from(key) | PICTURE_FIELDS,
            ..PictureParametersH264::default()
        };
        if let Some(previous) = previous {
            picture.references[0] = previous;
        }
        buffers.push(display.create_parameters(self.encoder, va::PICTURE, &picture)?);
        if key {
            self.pack(
                PACKED_SETS,
                &self.parameter_sets,
                self.parameter_sets.len() * 8,
                buffers,
            )?;
        }
        let (header, length) =
            headers::slice_header(key, current.frame_index, self.idrs, quantizer);
        self.pack(PACKED_SLICE, &header, length, buffers)?;
        let mut slice = SliceH264 {
            blocks: u32::try_from(self.shape.blocks()).map_err(|_| OUT_OF_RANGE)?,
            block_info: NO_ID,
            slice_type: if key { SLICE_I } else { SLICE_P },
            idr_id: self.idrs,
            list0: [PictureH264::NONE; 32],
            list1: [PictureH264::NONE; 32],
            quantizer_delta: i8::try_from(i16::from(quantizer) - i16::from(INITIAL_QUANTIZER))
                .map_err(|_| OUT_OF_RANGE)?,
            ..SliceH264::default()
        };
        if let Some(previous) = previous {
            slice.list0[0] = previous;
        }
        buffers.push(display.create_parameters(self.encoder, va::SLICE, &slice)?);
        Ok(())
    }

    /// Makes the buffers that hand the driver a header to write as it is.
    fn pack(
        &self,
        kind: u32,
        header: &[u8],
        length: usize,
        buffers: &mut Vec<Id>,
    ) -> Result<(), Status> {
        let parameters = PackedHeader {
            kind,
            bit_length: u32::try_from(length).map_err(|_| OUT_OF_RANGE)?,
            has_emulation_bytes: 1,
            reserved: [0; 4],
        };
        buffers.push(self.display.create_parameters(
            self.encoder,
            va::PACKED_HEADER,
            &parameters,
        )?);
        buffers.push(
            self.display
                .create_buffer(self.encoder, va::PACKED_DATA, header)?,
        );
        Ok(())
    }

    /// The `frames`th frame since the IDR, reconstructed in its turn's surface.
    fn frame(&self, frames: u32, flags: u32) -> PictureH264 {
        let order = frames.wrapping_mul(2).cast_signed();
        PictureH264 {
            surface: self.reconstructed[usize::from(frames % 2 == 1)],
            frame_index: frames % FRAME_NUMBERS,
            flags,
            top_order: order,
            bottom_order: order,
            reserved: [0; 4],
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let display = &self.display;
        if self.coded != NO_ID {
            display.destroy_buffer(self.coded);
        }
        if let Some(image) = &self.image {
            display.destroy_image(image);
        }
        for context in [self.converter, self.encoder] {
            if context != NO_ID {
                display.destroy_context(context);
            }
        }
        let mut surfaces: Vec<Id> = [self.source, self.picture]
            .into_iter()
            .chain(self.reconstructed)
            .filter(|surface| *surface != NO_ID)
            .collect();
        display.destroy_surfaces(&mut surfaces);
        for config in [self.converter_config, self.encoder_config] {
            if config != NO_ID {
                display.destroy_config(config);
            }
        }
    }
}

fn sequence(shape: Shape) -> SequenceH264 {
    let (right, bottom) = shape.crop();
    SequenceH264 {
        level: shape.level(),
        intra_period: INTRA_PERIOD,
        idr_period: INTRA_PERIOD,
        ip_period: 1,
        reference_frames: 1,
        width_in_blocks: u16::try_from(shape.width_in_blocks()).unwrap_or(u16::MAX),
        height_in_blocks: u16::try_from(shape.height_in_blocks()).unwrap_or(u16::MAX),
        fields: SEQUENCE_FIELDS,
        cropping: u8::from(right > 0 || bottom > 0),
        crop_right: u32::try_from(right).unwrap_or_default(),
        crop_bottom: u32::try_from(bottom).unwrap_or_default(),
        vui_present: 1,
        vui_fields: USABILITY_FIELDS,
        ..SequenceH264::default()
    }
}
