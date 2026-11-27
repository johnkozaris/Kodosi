//! The X keysyms of the keys that viewers press, by DOM `KeyboardEvent.code`: what each key is
//! that does not type, and the character that a key types on a US keyboard, for a key that comes
//! without its character. The X server's keymap then says which of its keys has the keysym, as X
//! servers number their keys differently.

use xkeysym::Keysym;

pub(super) fn keysym(code: &str) -> Option<Keysym> {
    named(code).or_else(|| typing(code).map(Keysym::from_char))
}

fn named(code: &str) -> Option<Keysym> {
    Some(match code {
        "Escape" => Keysym::Escape,
        "Backspace" => Keysym::BackSpace,
        "Tab" => Keysym::Tab,
        "Enter" => Keysym::Return,
        "Space" => Keysym::space,
        "CapsLock" => Keysym::Caps_Lock,
        "ShiftLeft" => Keysym::Shift_L,
        "ShiftRight" => Keysym::Shift_R,
        "ControlLeft" => Keysym::Control_L,
        "ControlRight" => Keysym::Control_R,
        "AltLeft" => Keysym::Alt_L,
        "AltRight" => Keysym::Alt_R,
        "MetaLeft" => Keysym::Super_L,
        "MetaRight" => Keysym::Super_R,
        "ContextMenu" => Keysym::Menu,
        "F1" => Keysym::F1,
        "F2" => Keysym::F2,
        "F3" => Keysym::F3,
        "F4" => Keysym::F4,
        "F5" => Keysym::F5,
        "F6" => Keysym::F6,
        "F7" => Keysym::F7,
        "F8" => Keysym::F8,
        "F9" => Keysym::F9,
        "F10" => Keysym::F10,
        "F11" => Keysym::F11,
        "F12" => Keysym::F12,
        "F13" => Keysym::F13,
        "F14" => Keysym::F14,
        "F15" => Keysym::F15,
        "F16" => Keysym::F16,
        "F17" => Keysym::F17,
        "F18" => Keysym::F18,
        "F19" => Keysym::F19,
        "PrintScreen" => Keysym::Print,
        "ScrollLock" => Keysym::Scroll_Lock,
        "Pause" => Keysym::Pause,
        "Insert" => Keysym::Insert,
        "Delete" => Keysym::Delete,
        "Home" => Keysym::Home,
        "End" => Keysym::End,
        "PageUp" => Keysym::Prior,
        "PageDown" => Keysym::Next,
        "ArrowUp" => Keysym::Up,
        "ArrowDown" => Keysym::Down,
        "ArrowLeft" => Keysym::Left,
        "ArrowRight" => Keysym::Right,
        "NumLock" => Keysym::Num_Lock,
        "Numpad0" => Keysym::KP_0,
        "Numpad1" => Keysym::KP_1,
        "Numpad2" => Keysym::KP_2,
        "Numpad3" => Keysym::KP_3,
        "Numpad4" => Keysym::KP_4,
        "Numpad5" => Keysym::KP_5,
        "Numpad6" => Keysym::KP_6,
        "Numpad7" => Keysym::KP_7,
        "Numpad8" => Keysym::KP_8,
        "Numpad9" => Keysym::KP_9,
        "NumpadDecimal" => Keysym::KP_Decimal,
        "NumpadAdd" => Keysym::KP_Add,
        "NumpadSubtract" => Keysym::KP_Subtract,
        "NumpadMultiply" => Keysym::KP_Multiply,
        "NumpadDivide" => Keysym::KP_Divide,
        "NumpadEnter" => Keysym::KP_Enter,
        "NumpadEqual" => Keysym::KP_Equal,
        "AudioVolumeMute" => Keysym::XF86_AudioMute,
        "AudioVolumeDown" => Keysym::XF86_AudioLowerVolume,
        "AudioVolumeUp" => Keysym::XF86_AudioRaiseVolume,
        "MediaPlayPause" => Keysym::XF86_AudioPlay,
        "MediaStop" => Keysym::XF86_AudioStop,
        "MediaTrackNext" => Keysym::XF86_AudioNext,
        "MediaTrackPrevious" => Keysym::XF86_AudioPrev,
        _ => return None,
    })
}

/// The character that the key types on a US keyboard without Shift.
fn typing(code: &str) -> Option<char> {
    let single = |rest: &str| {
        let mut characters = rest.chars();
        characters.next().filter(|_| characters.next().is_none())
    };
    if let Some(letter) = code.strip_prefix("Key").and_then(single) {
        return letter
            .is_ascii_uppercase()
            .then(|| letter.to_ascii_lowercase());
    }
    if let Some(digit) = code.strip_prefix("Digit").and_then(single) {
        return digit.is_ascii_digit().then_some(digit);
    }
    Some(match code {
        "Minus" => '-',
        "Equal" => '=',
        "BracketLeft" => '[',
        "BracketRight" => ']',
        "Backslash" => '\\',
        "Semicolon" => ';',
        "Quote" => '\'',
        "Backquote" => '`',
        "Comma" => ',',
        "Period" => '.',
        "Slash" => '/',
        "IntlBackslash" => '<',
        _ => return None,
    })
}
