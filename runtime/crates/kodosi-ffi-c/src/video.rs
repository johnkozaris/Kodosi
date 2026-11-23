use openh264::{decoder::Decoder, formats::YUVSource as _};

pub struct KodosiVideoDecoder {
    decoder: Decoder,
    rgba: Vec<u8>,
}

/// The last picture. Its pixels stay valid until the next call with this decoder.
#[repr(C)]
pub struct KodosiPicture {
    pub width: u32,
    pub height: u32,
    pub rgba: *const u8,
}

#[unsafe(no_mangle)]
pub extern "C" fn kodosi_video_decoder_new() -> *mut KodosiVideoDecoder {
    Decoder::new().map_or(std::ptr::null_mut(), |decoder| {
        Box::into_raw(Box::new(KodosiVideoDecoder {
            decoder,
            rgba: Vec::new(),
        }))
    })
}

/// Decodes one H.264 access unit. Returns 1 with a picture, 0 without one and -1 on a broken
/// unit.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_video_decoder_decode(
    decoder: *mut KodosiVideoDecoder,
    data: *const u8,
    length: usize,
    picture: *mut KodosiPicture,
) -> i32 {
    if decoder.is_null() || data.is_null() || picture.is_null() {
        return -1;
    }
    let decoder = unsafe { &mut *decoder };
    let unit = unsafe { std::slice::from_raw_parts(data, length) };
    let Ok(output) = decoder.decoder.decode(unit) else {
        return -1;
    };
    let Some(yuv) = output else {
        return 0;
    };
    let (width, height) = yuv.dimensions();
    decoder.rgba.resize(width * height * 4, 0);
    if !write_rgba(
        [yuv.y(), yuv.u(), yuv.v()],
        yuv.strides().into(),
        (width, height),
        &mut decoder.rgba,
    ) {
        return -1;
    }
    unsafe {
        picture.write(KodosiPicture {
            width: u32::try_from(width).unwrap_or(0),
            height: u32::try_from(height).unwrap_or(0),
            rgba: decoder.rgba.as_ptr(),
        });
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_video_decoder_free(decoder: *mut KodosiVideoDecoder) {
    if !decoder.is_null() {
        drop(unsafe { Box::from_raw(decoder) });
    }
}

// The engine's encoder writes limited-range BT.601 (Kr 0.299, Kb 0.114), and the stream does not
// say so. These are the factors of that matrix, with 255/219 for luma and 255/224 for chroma,
// scaled by 2^13 so that each fits in 16 bits: then the compiler multiplies eight lanes at once.
const SHIFT: u32 = 13;
const ROUND: i32 = 1 << (SHIFT - 1);
const LUMA: i32 = 9_539;
const RED_FROM_V: i32 = 13_075;
const GREEN_FROM_U: i32 = -3_209;
const GREEN_FROM_V: i32 = -6_660;
const BLUE_FROM_U: i32 = 16_525;
const BLOCK: usize = 32;

/// Writes a YUV 4:2:0 picture as RGBA, each channel rounded to the nearest value. Returns false
/// when a plane is too short for the picture or `rgba` is not its size.
fn write_rgba(
    planes: [&[u8]; 3],
    strides: [usize; 3],
    (width, height): (usize, usize),
    rgba: &mut [u8],
) -> bool {
    let chroma = (width.div_ceil(2), height.div_ceil(2));
    let holds = |plane: &[u8], stride: usize, (columns, rows): (usize, usize)| {
        rows == 0 || (stride >= columns && plane.len() >= (rows - 1) * stride + columns)
    };
    if rgba.len() != width * height * 4
        || !holds(planes[0], strides[0], (width, height))
        || !holds(planes[1], strides[1], chroma)
        || !holds(planes[2], strides[2], chroma)
    {
        return false;
    }
    if width == 0 {
        return true;
    }
    for (row, pixels) in rgba.chunks_exact_mut(width * 4).enumerate() {
        let luma = &planes[0][row * strides[0]..][..width];
        let blue = &planes[1][row / 2 * strides[1]..][..chroma.0];
        let red = &planes[2][row / 2 * strides[2]..][..chroma.0];
        let (blocks, rest) = pixels.as_chunks_mut::<{ BLOCK * 4 }>();
        for (((pixels, luma), blue), red) in blocks
            .iter_mut()
            .zip(luma.as_chunks::<BLOCK>().0)
            .zip(blue.as_chunks::<{ BLOCK / 2 }>().0)
            .zip(red.as_chunks::<{ BLOCK / 2 }>().0)
        {
            write_block(pixels, luma, blue, red);
        }
        let done = width - rest.len() / 4;
        for (at, pixel) in (done..).zip(rest.as_chunks_mut::<4>().0) {
            *pixel = rgba_of(luma[at], blue[at / 2], red[at / 2]);
        }
    }
    true
}

/// A block of fixed size, whose chroma is first spread to each pixel, so that both loops become
/// vector code.
// The release build checks each addition for overflow, also the count of `enumerate`, and that
// check keeps a loop from vector code. The index of a fixed range needs no check.
fn write_block(
    pixels: &mut [u8; BLOCK * 4],
    luma: &[u8; BLOCK],
    blue: &[u8; BLOCK / 2],
    red: &[u8; BLOCK / 2],
) {
    let mut blues = [0; BLOCK];
    let mut reds = [0; BLOCK];
    for index in 0..BLOCK {
        blues[index] = blue[index / 2];
        reds[index] = red[index / 2];
    }
    for index in 0..BLOCK {
        let [r, g, b, a] = rgba_of(luma[index], blues[index], reds[index]);
        pixels[4 * index] = r;
        pixels[4 * index + 1] = g;
        pixels[4 * index + 2] = b;
        pixels[4 * index + 3] = a;
    }
}

fn rgba_of(luma: u8, blue: u8, red: u8) -> [u8; 4] {
    let luma = LUMA * (i32::from(luma) - 16) + ROUND;
    let blue = i32::from(blue) - 128;
    let red = i32::from(red) - 128;
    [
        channel(luma + RED_FROM_V * red),
        channel(luma + GREEN_FROM_U * blue + GREEN_FROM_V * red),
        channel(luma + BLUE_FROM_U * blue),
        u8::MAX,
    ]
}

fn channel(scaled: i32) -> u8 {
    u8::try_from((scaled >> SHIFT).clamp(0, 255)).unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{rgba_of, write_rgba};
    use openh264::{
        decoder::Decoder,
        encoder::Encoder,
        formats::{RgbSliceU8, YUVBuffer, YUVSource as _},
    };

    /// Limited-range BT.601 as its definition gives it, before rounding.
    fn exact(luma: u8, blue: u8, red: u8) -> [f64; 3] {
        let (kr, kb): (f64, f64) = (0.299, 0.114);
        let kg = 1.0 - kr - kb;
        let y = 255.0 / 219.0 * (f64::from(luma) - 16.0);
        let u = 255.0 / 224.0 * (f64::from(blue) - 128.0);
        let v = 255.0 / 224.0 * (f64::from(red) - 128.0);
        [
            (2.0 * (1.0 - kr)).mul_add(v, y),
            (-2.0 * (1.0 - kb) * kb / kg).mul_add(u, (-2.0 * (1.0 - kr) * kr / kg).mul_add(v, y)),
            (2.0 * (1.0 - kb)).mul_add(u, y),
        ]
    }

    /// Each factor is within 2^-14 of its value, so a sum of luma and two chroma terms is within
    /// (239 + 128 + 128) / 2^14 of the formula, and a value that lies this near a half may round
    /// the other way.
    const NEAR_HALF: f64 = (239.0 + 128.0 + 128.0) / 16_384.0;

    #[test]
    fn each_sample_takes_the_nearest_value_of_the_bt601_limited_range_formula() {
        for luma in (0..=u8::MAX).step_by(5) {
            for blue in (0..=u8::MAX).step_by(5) {
                for red in (0..=u8::MAX).step_by(5) {
                    let pixel = rgba_of(luma, blue, red);
                    assert_eq!(pixel[3], u8::MAX);
                    for (got, exact) in pixel.into_iter().zip(exact(luma, blue, red)) {
                        let exact = exact.clamp(0.0, 255.0);
                        let off = (f64::from(got) - exact.round()).abs();
                        let near_half = (exact.fract() - 0.5).abs() < NEAR_HALF;
                        assert!(
                            off < 0.5 || (off < 1.5 && near_half),
                            "Y {luma} U {blue} V {red}: {got} for {exact}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn an_odd_picture_takes_each_pixel_from_its_own_samples_and_skips_the_padding() {
        let (width, height) = (13, 3);
        let (luma_stride, chroma_stride) = (16, 9);
        let plane = |stride: usize, (columns, rows): (usize, usize), first: usize| -> Vec<u8> {
            (0..stride * rows)
                .map(|at| {
                    if at % stride < columns {
                        u8::try_from((first + 7 * at) % 200 + 20).unwrap()
                    } else {
                        0
                    }
                })
                .collect()
        };
        let luma = plane(luma_stride, (width, height), 0);
        let blue = plane(chroma_stride, (7, 2), 50);
        let red = plane(chroma_stride, (7, 2), 120);
        let mut rgba = vec![0; width * height * 4];
        assert!(write_rgba(
            [&luma, &blue, &red],
            [luma_stride, chroma_stride, chroma_stride],
            (width, height),
            &mut rgba
        ));
        for row in 0..height {
            for column in 0..width {
                let chroma = row / 2 * chroma_stride + column / 2;
                assert_eq!(
                    rgba[(row * width + column) * 4..][..4],
                    rgba_of(luma[row * luma_stride + column], blue[chroma], red[chroma]),
                    "row {row} column {column}"
                );
            }
        }
        let short = &luma[..2 * luma_stride + width - 1];
        assert!(!write_rgba(
            [short, &blue, &red],
            [luma_stride, chroma_stride, chroma_stride],
            (width, height),
            &mut rgba
        ));
    }

    #[test]
    fn a_decoded_frame_has_the_colours_of_the_crates_own_conversion() {
        // The crate's fast path for widths of whole blocks of 8 has factors of 6 bits and cuts off
        // its fractions; its other path cuts off the fractions of floating-point factors.
        for ((width, height), tolerance) in [((64, 48), 3), ((62, 46), 1)] {
            let rgb: Vec<u8> = (0..width * height)
                .flat_map(|at| {
                    let (x, y) = (at % width, at / width);
                    let bar = [[235, 40, 40], [40, 200, 60], [30, 60, 220], [250, 250, 250]]
                        [x * 4 / width];
                    bar.map(|value: usize| {
                        u8::try_from((value * (height - y / 2)) / height).unwrap()
                    })
                })
                .collect();
            let source = YUVBuffer::from_rgb8_source(RgbSliceU8::new(&rgb, (width, height)));
            let stream = Encoder::new().unwrap().encode(&source).unwrap().to_vec();
            let mut decoder = Decoder::new().unwrap();
            let frame = decoder.decode(&stream).unwrap().unwrap();
            assert_eq!(frame.dimensions(), (width, height));
            let mut ours = vec![0; width * height * 4];
            assert!(write_rgba(
                [frame.y(), frame.u(), frame.v()],
                frame.strides().into(),
                (width, height),
                &mut ours
            ));
            let mut crates = vec![0; width * height * 4];
            frame.write_rgba8(&mut crates);
            let worst = ours.iter().zip(&crates).map(|(a, b)| a.abs_diff(*b)).max();
            assert!(worst <= Some(tolerance), "{width}x{height}: {worst:?}");
        }
    }
}
