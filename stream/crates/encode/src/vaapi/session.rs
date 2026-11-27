//! A VA-API encoder for pictures of one size in one codec. It encodes NV12 pictures in BT.709
//! limited range, which the GPU converts from BGRA: pixels that the session copies onto the GPU,
//! or frames that stayed on it. It codes each as an IDR or as an inter frame on the frame before;
//! the codec's syntax describes each frame to the driver.

use std::{ffi::c_int, sync::Arc};

use stream::Codec;

use super::{
    Gpu,
    convert::{self, Converter},
    h264, hevc,
    va::{
        self, ConfigAttribute, Display, Id, Image, NO_ID, OUT_OF_RANGE, PackedHeader, Status,
        Surface,
    },
};

/// Room for the parameter sets beyond a frame as large as the raw picture.
const HEADROOM: usize = 64 * 1024;

/// The size of the pictures that a stream carries.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Shape {
    pub(super) width: usize,
    pub(super) height: usize,
}

impl Shape {
    /// The size that the encoder codes: the next multiples of the `block` width and height.
    pub(super) fn coded(self, (width, height): (usize, usize)) -> (usize, usize) {
        (
            self.width.next_multiple_of(width),
            self.height.next_multiple_of(height),
        )
    }

    /// What decoders crop off the right and bottom of the coded size, in pairs of pixels.
    pub(super) fn crop(self, block: (usize, usize)) -> (usize, usize) {
        let (width, height) = self.coded(block);
        ((width - self.width) / 2, (height - self.height) / 2)
    }
}

/// What a codec's syntax needs to describe a frame.
pub(super) struct Frame {
    pub(super) key: bool,
    /// Frames since the IDR, which number and order them.
    pub(super) number: u32,
    pub(super) idr: u16,
    pub(super) quantizer: u8,
    /// Where the driver reconstructs this frame, and the frame before it.
    pub(super) current: Id,
    pub(super) previous: Id,
    pub(super) coded: Id,
}

enum Syntax {
    H264(h264::Syntax),
    Hevc(hevc::Syntax),
}

/// `VAEncPackedHeaderSequence`, for parameter sets, and `VAEncPackedHeaderSlice`.
pub(super) const PACKED_SETS: u32 = 1;
pub(super) const PACKED_SLICE: u32 = 3;

/// Makes the buffers that hand the driver a header of `length` bits to write as it is.
pub(super) fn packed(
    display: &Display,
    context: Id,
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
    buffers.push(display.create_parameters(context, va::PACKED_HEADER, &parameters)?);
    buffers.push(display.create_buffer(context, va::PACKED_DATA, header)?);
    Ok(())
}

pub(super) struct Session {
    display: Arc<Display>,
    shape: Shape,
    codec: Codec,
    syntax: Syntax,
    encoder_config: Id,
    encoder: Id,
    /// Where pictures that come as pixels go, made for the first of them.
    upload: Option<Upload>,
    /// What decoders make of the frames, in turns, so the frame before is the reference.
    reconstructed: [Id; 2],
    coded: Id,
    /// Frames since the last IDR; IDRs, which number them.
    frames: u32,
    idrs: u16,
}

/// The BGRX surface that pixels are copied into, through the image that maps it, and the NV12
/// picture that the GPU converts them into, which passes encode again.
struct Upload {
    display: Arc<Display>,
    converter: Converter,
    source: Surface,
    picture: Surface,
    image: Image,
}

impl Upload {
    fn new(
        display: &Arc<Display>,
        size: (usize, usize),
        coded: (usize, usize),
    ) -> Result<Self, Status> {
        let converter = Converter::new(display, coded)?;
        let source = Surface::new(display, va::FORMAT_RGB32, va::FOURCC_BGRX, size)?;
        let picture = Surface::nv12(display, coded)?;
        let image = display.derive_image(source.id())?;
        Ok(Self {
            display: Arc::clone(display),
            converter,
            source,
            picture,
            image,
        })
    }
}

impl Drop for Upload {
    fn drop(&mut self) {
        self.display.destroy_image(&self.image);
    }
}

impl Session {
    pub(super) fn start(
        display: &Arc<Display>,
        gpu: &Gpu,
        codec: Codec,
        (width, height): (usize, usize),
    ) -> Result<Self, Status> {
        let shape = Shape { width, height };
        let (syntax, profile, entrypoint) = match (codec, gpu.hevc) {
            (Codec::Hevc, Some(capabilities)) => (
                Syntax::Hevc(hevc::Syntax::new(shape, capabilities)?),
                va::HEVC_MAIN,
                capabilities.entrypoint,
            ),
            _ => (
                Syntax::H264(h264::Syntax::new(shape)),
                va::H264_HIGH,
                gpu.h264,
            ),
        };
        let mut session = Self {
            display: Arc::clone(display),
            shape,
            codec: match syntax {
                Syntax::H264(_) => Codec::H264,
                Syntax::Hevc(_) => Codec::Hevc,
            },
            syntax,
            encoder_config: NO_ID,
            encoder: NO_ID,
            upload: None,
            reconstructed: [NO_ID; 2],
            coded: NO_ID,
            frames: 0,
            idrs: 0,
        };
        session.open(profile, entrypoint)?;
        Ok(session)
    }

    /// The size that the encoder codes.
    fn coded(&self) -> (usize, usize) {
        let block = match &self.syntax {
            Syntax::H264(_) => (h264::BLOCK, h264::BLOCK),
            Syntax::Hevc(syntax) => syntax.alignment(),
        };
        self.shape.coded(block)
    }

    fn open(&mut self, profile: c_int, entrypoint: c_int) -> Result<(), Status> {
        let display = Arc::clone(&self.display);
        let coded = self.coded();
        self.encoder_config = display.create_config(
            profile,
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
        self.reconstructed = display.create_surfaces(va::FORMAT_YUV420, va::FOURCC_NV12, coded)?;
        self.encoder = display.create_context(self.encoder_config, coded, &self.reconstructed)?;
        self.coded = display.create_coded(self.encoder, coded.0 * coded.1 * 3 / 2 + HEADROOM)?;
        Ok(())
    }

    pub(super) fn size(&self) -> (usize, usize) {
        (self.shape.width, self.shape.height)
    }

    pub(super) fn codec(&self) -> Codec {
        self.codec
    }

    /// Copies a BGRA picture onto the GPU and converts it into the picture that `loaded` names.
    pub(super) fn load(&mut self, bgra: &[u8]) -> Result<(), Status> {
        let size = self.size();
        if self.upload.is_none() {
            self.upload = Some(Upload::new(&self.display, size, self.coded())?);
        }
        let Some(upload) = &self.upload else {
            return Err(OUT_OF_RANGE);
        };
        self.display.write(&upload.image, size, bgra)?;
        upload.converter.convert(
            upload.source.id(),
            convert::whole(size)?,
            upload.picture.id(),
            size,
        )
    }

    /// The picture of the pixels that the session loaded last.
    pub(super) fn loaded(&self) -> Result<Id, Status> {
        self.upload
            .as_ref()
            .map(|upload| upload.picture.id())
            .ok_or(OUT_OF_RANGE)
    }

    /// Encodes `picture` at `quantizer`, as an IDR when `key`, else as an inter frame on the
    /// frame before; returns the frame's units.
    pub(super) fn encode(
        &mut self,
        picture: Id,
        key: bool,
        quantizer: u8,
    ) -> Result<Vec<u8>, Status> {
        if key {
            self.frames = 0;
            self.idrs = self.idrs.wrapping_add(1);
        }
        let turn = |frames: u32| self.reconstructed[usize::from(frames % 2 == 1)];
        let frame = Frame {
            key,
            number: self.frames,
            idr: self.idrs,
            quantizer,
            current: turn(self.frames),
            previous: turn(self.frames.wrapping_sub(1)),
            coded: self.coded,
        };
        let mut buffers = Vec::with_capacity(8);
        let made = match &self.syntax {
            Syntax::H264(syntax) => {
                syntax.buffers(&self.display, self.encoder, &frame, &mut buffers)
            }
            Syntax::Hevc(syntax) => {
                syntax.buffers(&self.display, self.encoder, &frame, &mut buffers)
            }
        };
        let rendered = made.and_then(|()| self.display.render(self.encoder, picture, &buffers));
        for buffer in buffers {
            self.display.destroy_buffer(buffer);
        }
        rendered?;
        self.display.sync(picture)?;
        let mut data = Vec::new();
        self.display.read_coded(self.coded, &mut data)?;
        self.frames = self.frames.wrapping_add(1);
        Ok(data)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let display = &self.display;
        if self.coded != NO_ID {
            display.destroy_buffer(self.coded);
        }
        if self.encoder != NO_ID {
            display.destroy_context(self.encoder);
        }
        let mut surfaces: Vec<Id> = self
            .reconstructed
            .into_iter()
            .filter(|surface| *surface != NO_ID)
            .collect();
        display.destroy_surfaces(&mut surfaces);
        if self.encoder_config != NO_ID {
            display.destroy_config(self.encoder_config);
        }
    }
}
