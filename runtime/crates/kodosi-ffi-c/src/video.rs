//! H.264 for an app that has no decoder of its own: Qt WebEngine usually has none.

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
    let Ok(decoded) = decoder.decoder.decode(unit) else {
        return -1;
    };
    let Some(decoded) = decoded else {
        return 0;
    };
    let (width, height) = decoded.dimensions();
    decoder.rgba.resize(width * height * 4, 0);
    decoded.write_rgba8(&mut decoder.rgba);
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
