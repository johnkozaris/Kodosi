//! Text copied on the computer goes to the viewer in control, and a viewer's text comes onto the
//! computer's clipboard before it pastes there.

use std::{thread, time::Duration};

use screen::link::MOST_CLIPBOARD;
use windows::{
    Win32::{
        Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND},
        System::{
            DataExchange::{
                CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber,
                IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW,
                SetClipboardData,
            },
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock},
            Ole::CF_UNICODETEXT,
        },
    },
    core::{PCWSTR, w},
};

use super::app::Window;

/// How often the helper tries to open the clipboard, which another program may hold for a moment.
const TRIES: u32 = 10;
const RETRY: Duration = Duration::from_millis(10);
/// Password managers mark what they copy so that clipboard tools leave it alone.
const PRIVATE: [PCWSTR; 2] = [
    w!("ExcludeClipboardContentFromMonitorProcessing"),
    w!("Clipboard Viewer Ignore"),
];
/// Text that may not go into Windows' clipboard history or to the person's other devices, which
/// says 0 in one of these, stays on the computer too.
const ALLOWED: [PCWSTR; 2] = [
    w!("CanIncludeInClipboardHistory"),
    w!("CanUploadToCloudClipboard"),
];

/// What the helper last saw on the clipboard or put there itself.
pub(super) struct Clipboard {
    seen: u32,
    /// The text that last went between the computer and a viewer, which a viewer's device or
    /// a clipboard shared between devices brings back.
    last: Option<String>,
}

impl Clipboard {
    pub(super) fn new() -> Self {
        Self {
            seen: sequence(),
            last: None,
        }
    }

    /// True for text that did not just go between the computer and a viewer, which goes on.
    pub(super) fn is_new(&mut self, text: &str) -> bool {
        if self.last.as_deref() == Some(text) {
            return false;
        }
        self.last = Some(text.to_owned());
        true
    }

    /// Only what is copied from now on goes to viewers.
    pub(super) fn settle(&mut self) {
        self.seen = sequence();
    }

    /// True once something other than the helper's own text was copied since it last looked.
    pub(super) fn changed(&mut self) -> bool {
        let now = sequence();
        now != std::mem::replace(&mut self.seen, now)
    }

    pub(super) fn put(&mut self, window: Window, text: &str) {
        self.last = Some(text.to_owned());
        window.put_clipboard(text);
        self.seen = sequence();
    }
}

fn sequence() -> u32 {
    // SAFETY: a getter.
    unsafe { GetClipboardSequenceNumber() }
}

/// Puts text on the clipboard for `owner`, the helper's window, whose thread runs this.
pub(super) fn write(owner: HWND, text: &str) {
    if !open(Some(owner)) {
        return;
    }
    let units: Vec<u16> = text.encode_utf16().chain([0]).collect();
    // SAFETY: the clipboard is open, the memory holds the text, and the clipboard owns the memory
    // once it took it.
    unsafe {
        if EmptyClipboard().is_ok()
            && let Ok(memory) = GlobalAlloc(GMEM_MOVEABLE, size_of_val(units.as_slice()))
        {
            let locked = GlobalLock(memory).cast::<u16>();
            let set = !locked.is_null() && {
                locked.copy_from_nonoverlapping(units.as_ptr(), units.len());
                drop(GlobalUnlock(memory));
                SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(HANDLE(memory.0))).is_ok()
            };
            if !set {
                drop(GlobalFree(Some(memory)));
            }
        }
        drop(CloseClipboard());
    }
}

/// The copied text, unless it is private or too long. Reading may wait for the program that
/// copied it to give it, so this runs away from the link's thread.
pub(super) fn copied() -> Option<String> {
    if !open(None) {
        return None;
    }
    let text = if private() { None } else { text() };
    // SAFETY: the clipboard was opened above.
    drop(unsafe { CloseClipboard() });
    text
}

/// The clipboard's text while it is open.
fn text() -> Option<String> {
    // SAFETY: the clipboard is open, and its data stays while it is.
    unsafe {
        let memory = HGLOBAL(GetClipboardData(u32::from(CF_UNICODETEXT.0)).ok()?.0);
        let units = GlobalSize(memory) / 2;
        if units == 0 || units > MOST_CLIPBOARD + 1 {
            return None;
        }
        let locked = GlobalLock(memory).cast::<u16>();
        if locked.is_null() {
            return None;
        }
        let all = std::slice::from_raw_parts(locked, units);
        let end = all.iter().position(|unit| *unit == 0).unwrap_or(units);
        let text = String::from_utf16(&all[..end]).ok();
        drop(GlobalUnlock(memory));
        text.filter(|text| !text.is_empty() && text.len() <= MOST_CLIPBOARD)
    }
}

/// True for text that its program marked to stay on the computer, while the clipboard is open.
fn private() -> bool {
    PRIVATE.into_iter().any(|name| held(name).is_some())
        || ALLOWED
            .into_iter()
            .any(|name| held(name).is_some_and(says_no))
}

/// The clipboard format of this name, when the clipboard holds it.
fn held(name: PCWSTR) -> Option<u32> {
    // SAFETY: registering a format by its name returns the same number each time, and the
    // clipboard is open.
    unsafe {
        let format = RegisterClipboardFormatW(name);
        (format != 0 && IsClipboardFormatAvailable(format).is_ok()).then_some(format)
    }
}

/// True when the format's data is the 32-bit number 0.
fn says_no(format: u32) -> bool {
    // SAFETY: the clipboard is open, so its data stays while it is read.
    unsafe {
        let Ok(data) = GetClipboardData(format) else {
            return false;
        };
        let memory = HGLOBAL(data.0);
        if GlobalSize(memory) < size_of::<u32>() {
            return false;
        }
        let locked = GlobalLock(memory).cast::<u32>();
        if locked.is_null() {
            return false;
        }
        let no = locked.read_unaligned() == 0;
        drop(GlobalUnlock(memory));
        no
    }
}

fn open(owner: Option<HWND>) -> bool {
    for _ in 0..TRIES {
        // SAFETY: the clipboard is closed again by the caller.
        if unsafe { OpenClipboard(owner) }.is_ok() {
            return true;
        }
        thread::sleep(RETRY);
    }
    false
}
