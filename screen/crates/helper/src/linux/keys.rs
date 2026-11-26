//! DOM `KeyboardEvent.code` names with the Linux kernel's key codes for the same physical keys.

pub(super) fn key(code: &str) -> Option<u32> {
    KEYS.iter()
        .find(|(name, _)| *name == code)
        .map(|(_, key)| *key)
}

/// Keys that change the modifiers rather than type.
pub(super) fn is_modifier(code: &str) -> bool {
    matches!(
        code,
        "ShiftLeft"
            | "ShiftRight"
            | "ControlLeft"
            | "ControlRight"
            | "AltLeft"
            | "AltRight"
            | "MetaLeft"
            | "MetaRight"
            | "CapsLock"
            | "Fn"
    )
}

pub(super) const LEFT_SHIFT: u32 = 42;
pub(super) const RIGHT_SHIFT: u32 = 54;
pub(super) const LEFT_CONTROL: u32 = 29;
pub(super) const RIGHT_CONTROL: u32 = 97;
pub(super) const LEFT_ALT: u32 = 56;
pub(super) const RIGHT_ALT: u32 = 100;
pub(super) const LEFT_META: u32 = 125;
pub(super) const RIGHT_META: u32 = 126;

const KEYS: &[(&str, u32)] = &[
    ("Escape", 1),
    ("Digit1", 2),
    ("Digit2", 3),
    ("Digit3", 4),
    ("Digit4", 5),
    ("Digit5", 6),
    ("Digit6", 7),
    ("Digit7", 8),
    ("Digit8", 9),
    ("Digit9", 10),
    ("Digit0", 11),
    ("Minus", 12),
    ("Equal", 13),
    ("Backspace", 14),
    ("Tab", 15),
    ("KeyQ", 16),
    ("KeyW", 17),
    ("KeyE", 18),
    ("KeyR", 19),
    ("KeyT", 20),
    ("KeyY", 21),
    ("KeyU", 22),
    ("KeyI", 23),
    ("KeyO", 24),
    ("KeyP", 25),
    ("BracketLeft", 26),
    ("BracketRight", 27),
    ("Enter", 28),
    ("ControlLeft", LEFT_CONTROL),
    ("KeyA", 30),
    ("KeyS", 31),
    ("KeyD", 32),
    ("KeyF", 33),
    ("KeyG", 34),
    ("KeyH", 35),
    ("KeyJ", 36),
    ("KeyK", 37),
    ("KeyL", 38),
    ("Semicolon", 39),
    ("Quote", 40),
    ("Backquote", 41),
    ("ShiftLeft", LEFT_SHIFT),
    ("Backslash", 43),
    ("KeyZ", 44),
    ("KeyX", 45),
    ("KeyC", 46),
    ("KeyV", 47),
    ("KeyB", 48),
    ("KeyN", 49),
    ("KeyM", 50),
    ("Comma", 51),
    ("Period", 52),
    ("Slash", 53),
    ("ShiftRight", RIGHT_SHIFT),
    ("NumpadMultiply", 55),
    ("AltLeft", LEFT_ALT),
    ("Space", 57),
    ("CapsLock", 58),
    ("F1", 59),
    ("F2", 60),
    ("F3", 61),
    ("F4", 62),
    ("F5", 63),
    ("F6", 64),
    ("F7", 65),
    ("F8", 66),
    ("F9", 67),
    ("F10", 68),
    ("NumLock", 69),
    ("ScrollLock", 70),
    ("Numpad7", 71),
    ("Numpad8", 72),
    ("Numpad9", 73),
    ("NumpadSubtract", 74),
    ("Numpad4", 75),
    ("Numpad5", 76),
    ("Numpad6", 77),
    ("NumpadAdd", 78),
    ("Numpad1", 79),
    ("Numpad2", 80),
    ("Numpad3", 81),
    ("Numpad0", 82),
    ("NumpadDecimal", 83),
    ("IntlBackslash", 86),
    ("F11", 87),
    ("F12", 88),
    ("IntlRo", 89),
    ("Convert", 92),
    ("KanaMode", 93),
    ("NonConvert", 94),
    ("NumpadEnter", 96),
    ("ControlRight", RIGHT_CONTROL),
    ("NumpadDivide", 98),
    ("PrintScreen", 99),
    ("AltRight", RIGHT_ALT),
    ("Home", 102),
    ("ArrowUp", 103),
    ("PageUp", 104),
    ("ArrowLeft", 105),
    ("ArrowRight", 106),
    ("End", 107),
    ("ArrowDown", 108),
    ("PageDown", 109),
    ("Insert", 110),
    ("Delete", 111),
    ("AudioVolumeMute", 113),
    ("AudioVolumeDown", 114),
    ("AudioVolumeUp", 115),
    ("NumpadEqual", 117),
    ("Pause", 119),
    ("NumpadComma", 121),
    ("Lang1", 122),
    ("Lang2", 123),
    ("IntlYen", 124),
    ("MetaLeft", LEFT_META),
    ("MetaRight", RIGHT_META),
    ("ContextMenu", 127),
    ("MediaTrackNext", 163),
    ("MediaPlayPause", 164),
    ("MediaTrackPrevious", 165),
    ("MediaStop", 166),
    ("F13", 183),
    ("F14", 184),
    ("F15", 185),
    ("F16", 186),
    ("F17", 187),
    ("F18", 188),
    ("F19", 189),
];
