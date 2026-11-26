//! The keys of the computer's own keyboard layout by the characters they type, so that a shortcut
//! means the same on it whatever the viewer's layout is: ⌘A selects all.

use std::{
    collections::HashMap,
    ffi::c_void,
    ptr::NonNull,
    sync::{Mutex, PoisonError},
};

use objc2_core_foundation::{CFData, CFRetained, CFString, CFType};

/// `kUCKeyActionDisplay`: the character a key shows, without pressing it.
const DISPLAY: u16 = 3;
/// `kUCKeyTranslateNoDeadKeysMask`.
const NO_DEAD_KEYS: u32 = 1;
/// The virtual keys of a Mac keyboard.
const KEYS: u16 = 128;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    static kTISPropertyUnicodeKeyLayoutData: &'static CFString;
    fn TISCopyCurrentASCIICapableKeyboardLayoutInputSource() -> Option<NonNull<c_void>>;
    fn TISGetInputSourceProperty(source: NonNull<c_void>, key: &CFString) -> Option<&CFData>;
    fn LMGetKbdType() -> u8;
    fn UCKeyTranslate(
        layout: *const c_void,
        key: u16,
        action: u16,
        modifiers: u32,
        keyboard: u32,
        options: u32,
        dead_keys: *mut u32,
        most: usize,
        length: *mut usize,
        text: *mut u16,
    ) -> i32;
}

static LETTERS: Mutex<Option<HashMap<char, u16>>> = Mutex::new(None);

/// Reads the layout that shortcuts use, which is the current one unless it does not type Latin
/// letters. macOS reads input sources on the main thread only.
pub(super) fn learn() {
    let letters = read();
    *LETTERS.lock().unwrap_or_else(PoisonError::into_inner) = letters;
}

/// The key that types `letter` on the computer's layout.
pub(super) fn key_for(letter: char) -> Option<u16> {
    LETTERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()?
        .get(&letter.to_ascii_lowercase())
        .copied()
}

fn read() -> Option<HashMap<char, u16>> {
    // SAFETY: the call returns a retained input source, or nothing.
    let source = unsafe { TISCopyCurrentASCIICapableKeyboardLayoutInputSource() }?;
    // SAFETY: the input source is a Core Foundation object that the caller releases.
    let source: CFRetained<CFType> = unsafe { CFRetained::from_raw(source.cast()) };
    // SAFETY: the source is valid, and the property of a keyboard layout is its data, which
    // lives as long as the source.
    let data = unsafe {
        TISGetInputSourceProperty(
            CFRetained::as_ptr(&source).cast(),
            kTISPropertyUnicodeKeyLayoutData,
        )
    }?;
    let layout = data.byte_ptr().cast::<c_void>();
    // SAFETY: a getter without arguments.
    let keyboard = u32::from(unsafe { LMGetKbdType() });
    let mut letters = HashMap::new();
    for key in 0..KEYS {
        let mut dead_keys = 0;
        let mut length = 0;
        let mut text = [0_u16; 4];
        // SAFETY: the layout data lives as long as `source`, and the buffers are as long as
        // the call is told.
        let status = unsafe {
            UCKeyTranslate(
                layout,
                key,
                DISPLAY,
                0,
                keyboard,
                NO_DEAD_KEYS,
                &raw mut dead_keys,
                text.len(),
                &raw mut length,
                text.as_mut_ptr(),
            )
        };
        let typed = (status == 0 && length == 1)
            .then(|| char::from_u32(u32::from(text[0])))
            .flatten()
            .filter(char::is_ascii_graphic);
        if let Some(letter) = typed {
            letters.entry(letter.to_ascii_lowercase()).or_insert(key);
        }
    }
    drop(source);
    Some(letters)
}
