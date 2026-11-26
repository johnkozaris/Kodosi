//! The computer's keyboard layout, from the keymap that the desktop's libei server sends: the
//! key, and whether Shift or `AltGr` goes with it, that types each character, so that a viewer's
//! text comes out the same whatever the two layouts are.

use std::{
    collections::HashMap,
    ffi::c_char,
    fs::File,
    os::{fd::OwnedFd, unix::fs::FileExt as _},
};

use xkbcommon_dl::{
    xkb_context_flags, xkb_keymap_compile_flags, xkb_keymap_format, xkbcommon_option,
};
use xkeysym::Keysym;

/// xkb numbers keys 8 higher than the kernel does.
const KERNEL_OFFSET: u32 = 8;
/// The largest keymap that the helper reads.
const MOST_KEYMAP: u32 = 1024 * 1024;

/// A key press that types a character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Stroke {
    /// The kernel's key code.
    pub(super) key: u32,
    pub(super) shift: bool,
    /// `AltGr`, which xkb calls the third level.
    pub(super) third: bool,
}

pub(super) struct Keymap {
    text: Vec<u8>,
    group: u32,
    strokes: HashMap<u32, Stroke>,
}

impl Keymap {
    pub(super) fn read(keymap: &OwnedFd, size: u32) -> Option<Self> {
        let file = File::from(keymap.try_clone().ok()?);
        let mut text = vec![0; usize::try_from(size.min(MOST_KEYMAP)).ok()?];
        file.read_exact_at(&mut text, 0).ok()?;
        while text.last() == Some(&0) {
            text.pop();
        }
        let strokes = strokes(&text, 0)?;
        Some(Self {
            text,
            group: 0,
            strokes,
        })
    }

    /// Follows the computer's current layout among the keymap's layouts.
    pub(super) fn follow(&mut self, group: u32) {
        if group != self.group
            && let Some(strokes) = strokes(&self.text, group)
        {
            self.group = group;
            self.strokes = strokes;
        }
    }

    pub(super) fn stroke(&self, character: char) -> Option<Stroke> {
        let keysym = match character {
            '\n' | '\r' => Keysym::Return,
            '\t' => Keysym::Tab,
            _ => Keysym::from_char(character),
        };
        self.strokes.get(&keysym.raw()).copied()
    }

    /// The key that holds `AltGr` down, which differs between layouts.
    pub(super) fn third_level(&self) -> Option<u32> {
        self.strokes
            .get(&Keysym::ISO_Level3_Shift.raw())
            .map(|stroke| stroke.key)
    }
}

/// The first stroke that types each keysym, with the fewest modifiers.
fn strokes(text: &[u8], group: u32) -> Option<HashMap<u32, Stroke>> {
    let xkb = xkbcommon_option()?;
    // SAFETY: libxkbcommon's calls take the objects that it made, each released once, and the
    // keymap's text with its length.
    unsafe {
        let context = (xkb.xkb_context_new)(xkb_context_flags::XKB_CONTEXT_NO_FLAGS);
        if context.is_null() {
            return None;
        }
        let keymap = (xkb.xkb_keymap_new_from_buffer)(
            context,
            text.as_ptr().cast::<c_char>(),
            text.len(),
            xkb_keymap_format::XKB_KEYMAP_FORMAT_TEXT_V1,
            xkb_keymap_compile_flags::XKB_KEYMAP_COMPILE_NO_FLAGS,
        );
        (xkb.xkb_context_unref)(context);
        if keymap.is_null() {
            return None;
        }
        let state = (xkb.xkb_state_new)(keymap);
        if state.is_null() {
            (xkb.xkb_keymap_unref)(keymap);
            return None;
        }
        let bit = |name: &std::ffi::CStr| {
            let index = (xkb.xkb_keymap_mod_get_index)(keymap, name.as_ptr());
            1_u32.checked_shl(index).unwrap_or_default()
        };
        let shift = bit(c"Shift");
        let third = match bit(c"LevelThree") {
            0 => bit(c"Mod5"),
            third => third,
        };
        let (first, last) = (
            (xkb.xkb_keymap_min_keycode)(keymap).max(KERNEL_OFFSET),
            (xkb.xkb_keymap_max_keycode)(keymap),
        );
        let mut strokes = HashMap::new();
        for (with_shift, with_third) in [(false, false), (true, false), (false, true), (true, true)]
        {
            if with_third && third == 0 {
                continue;
            }
            let mask = if with_shift { shift } else { 0 } | if with_third { third } else { 0 };
            (xkb.xkb_state_update_mask)(state, mask, 0, 0, 0, 0, group);
            for code in first..=last {
                let keysym = (xkb.xkb_state_key_get_one_sym)(state, code);
                if keysym != 0 {
                    strokes.entry(keysym).or_insert(Stroke {
                        key: code - KERNEL_OFFSET,
                        shift: with_shift,
                        third: with_third,
                    });
                }
            }
        }
        (xkb.xkb_state_unref)(state);
        (xkb.xkb_keymap_unref)(keymap);
        Some(strokes)
    }
}
