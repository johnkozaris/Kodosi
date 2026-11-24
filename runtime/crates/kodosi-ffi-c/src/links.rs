use std::panic::AssertUnwindSafe;

use crate::instance::payload;
use crate::{KODOSI_FFI_DESER_FAILED, KODOSI_FFI_PANIC};

/// Decides what a click on a link does, by the rule of every app. `computer` names the other
/// computer of the terminal that shows the link, and is empty for a terminal of this device.
/// Writes the decision as JSON to `out` and returns its length in bytes, which is larger than
/// `capacity` when `out` is too small, or a negative error code.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_link_decision(
    link: *const u8,
    link_length: usize,
    computer: *const u8,
    computer_length: usize,
    out: *mut u8,
    capacity: usize,
) -> i64 {
    std::panic::catch_unwind(AssertUnwindSafe(|| {
        let link = unsafe { payload(link, link_length) }.map(std::str::from_utf8);
        let computer = unsafe { payload(computer, computer_length) }.map(std::str::from_utf8);
        let (Ok(Ok(link)), Ok(Ok(computer))) = (link, computer) else {
            return -i64::from(KODOSI_FFI_DESER_FAILED);
        };
        let Ok(decision) = serde_json::to_vec(&kodosi_runtime::links::decide(link, Some(computer)))
        else {
            return -i64::from(KODOSI_FFI_DESER_FAILED);
        };
        if decision.len() <= capacity && !out.is_null() {
            unsafe { std::ptr::copy_nonoverlapping(decision.as_ptr(), out, decision.len()) };
        }
        i64::try_from(decision.len()).unwrap_or(i64::MAX)
    }))
    .unwrap_or_else(|_| i64::from(KODOSI_FFI_PANIC))
}
