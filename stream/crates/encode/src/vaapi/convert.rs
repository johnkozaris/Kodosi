//! The GPU's video processing, which turns BGRX pictures into the NV12 pictures that sessions
//! encode: pixels that a session copied onto the GPU, and the compositor's own buffers, which the
//! capture's thread converts, crops and scales before it gives them back.

use std::sync::Arc;

use stream::Region;

use super::{
    Gpu, gpu, h264,
    session::Shape,
    va::{
        self, ColourProperties, Display, Id, NO_ID, OUT_OF_RANGE, Pipeline, Rectangle, Status,
        Surface,
    },
};
use crate::{
    Crop, Dmabuf,
    pixels::{self, Shared},
};

const BT709: u8 = 1;

/// Converts a part of a BGRX surface into a picture at the top left of an NV12 surface, scaled to
/// the picture's size, in BT.709 limited range.
pub(super) struct Converter {
    display: Arc<Display>,
    config: Id,
    context: Id,
}

impl Converter {
    pub(super) fn new(display: &Arc<Display>, size: (usize, usize)) -> Result<Self, Status> {
        let mut converter = Self {
            display: Arc::clone(display),
            config: display.create_config(va::NO_PROFILE, va::VIDEO_PROCESSING, &[])?,
            context: NO_ID,
        };
        converter.context = display.create_context(converter.config, size, &[])?;
        Ok(converter)
    }

    pub(super) fn convert(
        &self,
        source: Id,
        from: Rectangle,
        target: Id,
        size: (usize, usize),
    ) -> Result<(), Status> {
        let to = whole(size)?;
        // The same standard on both sides converts only the colour matrix, as viewers undo it:
        // Mesa would otherwise also convert sRGB's transfer curve.
        let pipeline = Pipeline {
            surface: source,
            surface_region: &raw const from,
            surface_standard: va::STANDARD_BT709,
            output_region: &raw const to,
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
            .create_parameters(self.context, va::PIPELINE, &pipeline)?;
        let converted = self.display.render(self.context, target, &[buffer]);
        self.display.destroy_buffer(buffer);
        converted
    }
}

impl Drop for Converter {
    fn drop(&mut self) {
        if self.context != NO_ID {
            self.display.destroy_context(self.context);
        }
        self.display.destroy_config(self.config);
    }
}

/// All of a picture of `size`.
pub(super) fn whole((width, height): (usize, usize)) -> Result<Rectangle, Status> {
    Ok(Rectangle {
        x: 0,
        y: 0,
        width: u16::try_from(width).map_err(|_| OUT_OF_RANGE)?,
        height: u16::try_from(height).map_err(|_| OUT_OF_RANGE)?,
    })
}

/// A picture that the GPU converted for the worker, on a surface of the size that encoders code.
pub(crate) struct Converted {
    pub(crate) surface: Surface,
    pub(crate) size: (usize, usize),
    pub(crate) region: Option<Region>,
}

/// Brings the compositor's frames to the worker without their pixels passing through the
/// processor. A frame stays valid only until the capture gives its buffer back, so the GPU
/// converts it at once into a picture that the worker owns.
pub(crate) struct Importer {
    display: Arc<Display>,
    /// What the surfaces' sizes are multiples of: those of every codec's coded size, as for the
    /// sessions' own pictures, so that no encoder reads past the end of a surface.
    block: (usize, usize),
    /// The converter for surfaces of its size.
    converter: Option<((usize, usize), Converter)>,
}

impl Importer {
    pub(crate) fn new(display: Arc<Display>) -> Self {
        Self {
            display,
            block: gpu().map_or((h264::BLOCK, h264::BLOCK), Gpu::block),
            converter: None,
        }
    }

    pub(crate) fn import(&self, buffer: &Dmabuf<'_>) -> Option<Surface> {
        Surface::import(&self.display, buffer).ok()
    }

    /// Converts `crop` of `frame` into a picture of `size` for the worker, as the newest. False
    /// when the GPU could not, or when the worker no longer encodes on the GPU.
    pub(crate) fn encode(
        &mut self,
        shared: &Shared,
        frame: &Surface,
        crop: Crop,
        (width, height): (usize, usize),
        region: Option<Region>,
        changed: f64,
    ) -> bool {
        let size = (width & !1, height & !1);
        if size.0 < 2 || size.1 < 2 {
            return true;
        }
        let coded = Shape {
            width: size.0,
            height: size.1,
        }
        .coded(self.block);
        let reused = {
            let mut slot = pixels::lock(shared);
            if slot.gpu_stopped {
                return false;
            }
            slot.converted
                .take()
                .map(|waiting| waiting.surface)
                .or_else(|| slot.spare_surface.take())
        };
        let target = match reused.filter(|surface| surface.size() == coded) {
            Some(surface) => surface,
            None => match Surface::nv12(&self.display, coded) {
                Ok(surface) => surface,
                Err(_) => return false,
            },
        };
        if self.convert(frame, crop, &target, size).is_err() {
            return false;
        }
        let mut slot = pixels::lock(shared);
        if slot.gpu_stopped {
            return false;
        }
        if let Some(waiting) = slot.picture.take() {
            slot.spare = waiting.bgra;
        }
        slot.converted = Some(Converted {
            surface: target,
            size,
            region,
        });
        slot.changed = (slot.changed + changed).min(1.0);
        drop(slot);
        shared.1.notify_one();
        true
    }

    /// Converts `crop` of `frame` into a picture of `size` at the top left of `target`, and waits
    /// until the GPU has read `frame`.
    fn convert(
        &mut self,
        frame: &Surface,
        crop: Crop,
        target: &Surface,
        size: (usize, usize),
    ) -> Result<(), Status> {
        let surface = target.size();
        if self
            .converter
            .as_ref()
            .is_none_or(|(made, _)| *made != surface)
        {
            self.converter = None;
            self.converter = Some((surface, Converter::new(&self.display, surface)?));
        }
        let Some((_, converter)) = &self.converter else {
            return Err(OUT_OF_RANGE);
        };
        converter.convert(
            frame.id(),
            rectangle(crop, frame.size())?,
            target.id(),
            size,
        )?;
        self.display.sync(target.id())
    }
}

/// The whole pixels of `crop`, within a frame of `size`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the values are clamped to a frame's size, which fits an i16"
)]
fn rectangle(crop: Crop, (width, height): (usize, usize)) -> Result<Rectangle, Status> {
    let span = |start: f64, length: f64, whole: usize| -> Result<(i16, u16), Status> {
        let whole = f64::from(i16::try_from(whole).map_err(|_| OUT_OF_RANGE)?);
        let first = start.round().clamp(0.0, whole - 1.0);
        let end = (start + length).round().clamp(first + 1.0, whole);
        Ok((first as i16, (end - first) as u16))
    };
    let (x, width) = span(crop.left, crop.width, width)?;
    let (y, height) = span(crop.top, crop.height, height)?;
    Ok(Rectangle {
        x,
        y,
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::rectangle;
    use crate::Crop;

    fn pixels(left: f64, top: f64, width: f64, height: f64) -> (i16, i16, u16, u16) {
        let crop = Crop {
            left,
            top,
            width,
            height,
        };
        let rectangle = rectangle(crop, (1920, 1080)).unwrap();
        (rectangle.x, rectangle.y, rectangle.width, rectangle.height)
    }

    #[test]
    fn a_zoomed_part_stays_within_the_frame_that_the_gpu_reads() {
        assert_eq!(pixels(265.6, 162.6, 786.4, 504.4), (266, 163, 786, 504));
        assert_eq!(pixels(1800.4, 1000.0, 119.9, 80.3), (1800, 1000, 120, 80));
        assert_eq!(pixels(1919.8, 1079.9, 0.1, 0.1), (1919, 1079, 1, 1));
    }
}
