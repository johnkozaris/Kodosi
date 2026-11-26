//! The pointer that viewers draw over the picture, as the desktop duplication tells it: where it is
//! and, once for each shape, what it looks like. A pointer that the display's hardware does not
//! draw is in the picture already, and DXGI tells nothing about it.

use std::{
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use bytes::Bytes;
use screen::link::{self, FromHelper, Pointer, Shape};
use tokio::sync::mpsc;
use windows::Win32::Graphics::Dxgi::{
    DXGI_OUTDUPL_FRAME_INFO, DXGI_OUTDUPL_POINTER_SHAPE_INFO,
    DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR, DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR,
    DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME, IDXGIOutputDuplication,
};

/// What the host knows of the pointer.
#[derive(Clone, Copy, PartialEq)]
enum Known {
    Nothing,
    Where(Option<Pointer>),
}

/// The pointer's current shape and its hotspot in pixels, with its picture until the host has it.
struct Current {
    id: u64,
    x: i32,
    y: i32,
    untold: Option<Shape>,
}

pub(super) struct Pointing {
    say: mpsc::Sender<Bytes>,
    told: Known,
    /// The shapes that the host has.
    shapes: HashSet<u64>,
    current: Option<Current>,
    buffer: Vec<u8>,
}

impl Pointing {
    pub(super) fn new(say: mpsc::Sender<Bytes>) -> Self {
        Self {
            say,
            told: Known::Nothing,
            shapes: HashSet::new(),
            current: None,
            buffer: Vec::new(),
        }
    }

    /// Follows what a frame says about the pointer on a display of `scale` pixels per point.
    pub(super) fn update(
        &mut self,
        duplication: &IDXGIOutputDuplication,
        info: &DXGI_OUTDUPL_FRAME_INFO,
        scale: f64,
    ) {
        if info.PointerShapeBufferSize > 0 {
            self.current = self.read(duplication, info.PointerShapeBufferSize, scale);
        }
        if let Some(current) = &mut self.current
            && let Some(shape) = current.untold.take()
        {
            let told = self
                .say
                .try_send(link::json(&FromHelper::Shape {
                    shape: shape.clone(),
                }))
                .is_ok();
            if told {
                self.shapes.insert(shape.id);
            } else {
                current.untold = Some(shape);
            }
        }
        let position = info.PointerPosition;
        let pointer = self
            .current
            .as_ref()
            .filter(|current| position.Visible.as_bool() && current.untold.is_none())
            .map(|current| Pointer {
                x: f64::from(position.Position.x + current.x) / scale,
                y: f64::from(position.Position.y + current.y) / scale,
                shape: current.id,
            });
        if self.told != Known::Where(pointer)
            && self
                .say
                .try_send(link::json(&FromHelper::Pointer { pointer }))
                .is_ok()
        {
            self.told = Known::Where(pointer);
        }
    }

    fn read(
        &mut self,
        duplication: &IDXGIOutputDuplication,
        size: u32,
        scale: f64,
    ) -> Option<Current> {
        self.buffer
            .resize(usize::try_from(size).unwrap_or_default(), 0);
        let mut needed = 0;
        let mut info = DXGI_OUTDUPL_POINTER_SHAPE_INFO::default();
        // SAFETY: the buffer holds `size` bytes, and the other pointers are valid to write.
        unsafe {
            duplication.GetFramePointerShape(
                size,
                self.buffer.as_mut_ptr().cast(),
                &raw mut needed,
                &raw mut info,
            )
        }
        .ok()?;
        let (rgba, width, height) = rgba(&self.buffer, &info)?;
        let mut hasher = DefaultHasher::new();
        (&rgba, width, height, info.HotSpot.x, info.HotSpot.y).hash(&mut hasher);
        let id = hasher.finish();
        let untold = if self.shapes.contains(&id) {
            None
        } else {
            Some(Shape {
                id,
                png: STANDARD.encode(png(&rgba, width, height)?),
                x: f64::from(info.HotSpot.x) / scale,
                y: f64::from(info.HotSpot.y) / scale,
                width: f64::from(width) / scale,
                height: f64::from(height) / scale,
            })
        };
        Some(Current {
            id,
            x: info.HotSpot.x,
            y: info.HotSpot.y,
            untold,
        })
    }
}

/// A pixel of a pointer shape.
#[derive(Clone, Copy, PartialEq)]
enum Ink {
    Clear,
    Colour([u8; 4]),
    /// Turns the screen's colour around, which a picture cannot show.
    Inverted,
}

/// The pointer shape as RGBA rows with its size. Pixels that turn the screen's colour around
/// become black with a white outline, which shows on any background.
fn rgba(buffer: &[u8], info: &DXGI_OUTDUPL_POINTER_SHAPE_INFO) -> Option<(Vec<u8>, u32, u32)> {
    let width = usize::try_from(info.Width).ok()?;
    let pitch = usize::try_from(info.Pitch).ok()?;
    let kind = i32::try_from(info.Type).ok()?;
    let monochrome = kind == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME.0;
    let height = usize::try_from(if monochrome {
        info.Height / 2
    } else {
        info.Height
    })
    .ok()?;
    let row = |y: usize| buffer.get(y * pitch..(y + 1) * pitch);
    let mut inks = Vec::with_capacity(width * height);
    for y in 0..height {
        if monochrome {
            let (and, xor) = (row(y)?, row(y + height)?);
            let bit = |line: &[u8], x: usize| {
                line.get(x / 8)
                    .is_some_and(|byte| byte & (0x80 >> (x % 8)) != 0)
            };
            inks.extend((0..width).map(|x| match (bit(and, x), bit(xor, x)) {
                (false, false) => Ink::Colour([0, 0, 0, 255]),
                (false, true) => Ink::Colour([255, 255, 255, 255]),
                (true, false) => Ink::Clear,
                (true, true) => Ink::Inverted,
            }));
            continue;
        }
        let line = row(y)?.get(..width * 4)?;
        for [blue, green, red, alpha] in line.as_chunks::<4>().0.iter().copied() {
            inks.push(if kind == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR.0 {
                Ink::Colour([red, green, blue, alpha])
            } else if kind != DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR.0 {
                return None;
            } else if alpha == 0 {
                Ink::Colour([red, green, blue, 255])
            } else if (red, green, blue) == (0, 0, 0) {
                Ink::Clear
            } else {
                Ink::Inverted
            });
        }
    }
    let outlined = |x: usize, y: usize| {
        let near = |at: usize, end: usize| at.saturating_sub(1)..(at + 2).min(end);
        near(y, height).any(|y| near(x, width).any(|x| inks[y * width + x] == Ink::Inverted))
    };
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            rgba.extend_from_slice(&match inks[y * width + x] {
                Ink::Colour(colour) => colour,
                Ink::Inverted => [0, 0, 0, 255],
                Ink::Clear if outlined(x, y) => [255, 255, 255, 255],
                Ink::Clear => [0, 0, 0, 0],
            });
        }
    }
    Some((
        rgba,
        u32::try_from(width).ok()?,
        u32::try_from(height).ok()?,
    ))
}

fn png(rgba: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(rgba).ok()?;
    writer.finish().ok()?;
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use windows::Win32::{
        Foundation::POINT,
        Graphics::Dxgi::{
            DXGI_OUTDUPL_POINTER_SHAPE_INFO, DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR,
            DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME,
        },
    };

    use super::rgba;

    const BLACK: [u8; 4] = [0, 0, 0, 255];
    const WHITE: [u8; 4] = [255, 255, 255, 255];
    const CLEAR: [u8; 4] = [0, 0, 0, 0];

    fn info(kind: i32, width: u32, height: u32, pitch: u32) -> DXGI_OUTDUPL_POINTER_SHAPE_INFO {
        DXGI_OUTDUPL_POINTER_SHAPE_INFO {
            Type: u32::try_from(kind).unwrap(),
            Width: width,
            Height: height,
            Pitch: pitch,
            HotSpot: POINT::default(),
        }
    }

    #[test]
    fn a_monochrome_pointer_shows_what_it_inverts_in_black_with_a_white_edge() {
        // Black, white, clear, clear and inverting pixels: the AND mask's row, then the XOR's.
        let masks = [0b0011_1000, 0b0100_1000];
        let shape = info(DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME.0, 5, 2, 1);
        let (pixels, width, height) = rgba(&masks, &shape).unwrap();
        assert_eq!((width, height), (5, 1));
        assert_eq!(pixels, [BLACK, WHITE, CLEAR, WHITE, BLACK].concat());
    }

    #[test]
    fn a_masked_colour_pointer_keeps_its_colours_and_drops_what_leaves_the_screen_as_it_is() {
        // Blue, green, red and the mask in its alpha: 0 replaces the screen, 255 changes it.
        let pixels = [
            [0, 0, 255, 0],
            [0, 0, 0, 255],
            [0, 0, 0, 255],
            [255, 255, 255, 255],
        ];
        let shape = info(DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR.0, 4, 1, 16);
        let (pixels, _, _) = rgba(&pixels.concat(), &shape).unwrap();
        assert_eq!(pixels, [[255, 0, 0, 255], CLEAR, WHITE, BLACK].concat());
    }
}
