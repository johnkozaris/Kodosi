use std::panic::AssertUnwindSafe;

use crate::{
    KODOSI_FFI_PANIC, KODOSI_FFI_PAYLOAD_TOO_LARGE, KODOSI_FFI_PLATFORM_FAILED,
    KODOSI_SCREEN_VOUCH_BYTES,
};

/// Kodosi's app vouches that the person at this Mac just clicked its switch to share the screen,
/// so that Kodosi Screen asks them; only the code that runs on that click calls this. Writes what
/// `screen.allow` carries as `vouch` to `out`, which holds `KODOSI_SCREEN_VOUCH_BYTES`, and
/// returns its length, or a negative error code where no Mac's app vouches.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_screen_vouch(out: *mut u8, capacity: usize) -> i64 {
    if out.is_null() || capacity < KODOSI_SCREEN_VOUCH_BYTES {
        return -i64::from(KODOSI_FFI_PAYLOAD_TOO_LARGE);
    }
    std::panic::catch_unwind(AssertUnwindSafe(|| {
        #[cfg(all(target_os = "macos", feature = "host"))]
        {
            let Ok(vouch) = kodosi_runtime::canvas::screen_vouch() else {
                return -i64::from(KODOSI_FFI_PLATFORM_FAILED);
            };
            if vouch.len() > capacity {
                return -i64::from(KODOSI_FFI_PAYLOAD_TOO_LARGE);
            }
            unsafe { std::ptr::copy_nonoverlapping(vouch.as_ptr(), out, vouch.len()) };
            i64::try_from(vouch.len()).unwrap_or(i64::MAX)
        }
        #[cfg(not(all(target_os = "macos", feature = "host")))]
        -i64::from(KODOSI_FFI_PLATFORM_FAILED)
    }))
    .unwrap_or_else(|_| i64::from(KODOSI_FFI_PANIC))
}
