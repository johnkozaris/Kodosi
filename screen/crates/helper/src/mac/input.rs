//! Viewers' mouse and keys, posted as this computer's own input.

use std::{collections::HashMap, ffi::c_ulong};

use objc2_core_foundation::{CFRetained, CGPoint, CGRect};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
    CGEventType, CGMouseButton, CGScrollEventUnit,
};
use stream::viewer::{Input, KeyAction, MouseAction, MouseButton};

use super::{hand, keys, layout};

/// `CGEventKeyboardSetUnicodeString` takes this many UTF-16 units per event at most.
const TEXT_CHUNK: usize = 20;
const ALT: u8 = 1;
const CONTROL: u8 = 2;
const META: u8 = 4;
const SHIFT: u8 = 8;

pub(super) struct Hands {
    source: Option<CFRetained<CGEventSource>>,
    /// The button held down, which turns moves into drags.
    held: MouseButton,
    /// The Mac's key for each viewer key held down, which lets go of the same key.
    keys: HashMap<String, u16>,
}

impl Hands {
    pub(super) fn new() -> Self {
        Self {
            source: CGEventSource::new(CGEventSourceStateID::HIDSystemState),
            held: MouseButton::None,
            keys: HashMap::new(),
        }
    }

    /// Plays input whose pointer is in the points of a source at `bounds`.
    pub(super) fn play(&mut self, input: &Input, bounds: CGRect) {
        match input {
            Input::Mouse {
                action,
                x,
                y,
                button,
                clicks,
                delta_x,
                delta_y,
                modifiers,
            } => {
                let at = CGPoint::new(bounds.origin.x + x, bounds.origin.y + y);
                let flags = flags(*modifiers);
                match action {
                    MouseAction::Move => {
                        self.mouse(Some(moved(self.held)), at, self.held, 0, flags);
                    }
                    MouseAction::Press => {
                        self.held = *button;
                        self.mouse(pressed(*button), at, *button, *clicks, flags);
                    }
                    MouseAction::Release => {
                        self.held = MouseButton::None;
                        self.mouse(released(*button), at, *button, *clicks, flags);
                    }
                    MouseAction::Wheel => self.scroll(at, *delta_x, *delta_y, flags),
                }
            }
            Input::Key {
                action,
                key,
                code,
                text,
                modifiers,
                ..
            } => self.key(
                *action == KeyAction::Down,
                key,
                code,
                text.as_deref(),
                *modifiers,
            ),
            Input::Text { text } => self.text(text),
        }
    }

    fn mouse(
        &self,
        kind: Option<CGEventType>,
        at: CGPoint,
        button: MouseButton,
        clicks: u8,
        flags: CGEventFlags,
    ) {
        let Some(event) = kind.and_then(|kind| {
            CGEvent::new_mouse_event(self.source.as_deref(), kind, at, mouse_button(button))
        }) else {
            return;
        };
        if clicks > 0 {
            CGEvent::set_integer_value_field(
                Some(&event),
                CGEventField::MouseEventClickState,
                i64::from(clicks),
            );
        }
        CGEvent::set_flags(Some(&event), flags);
        post(&event);
    }

    /// DOM wheel deltas grow as the page moves up, Core Graphics ones as it moves down.
    fn scroll(&self, at: CGPoint, delta_x: f64, delta_y: f64, flags: CGEventFlags) {
        let Some(event) = CGEvent::new_scroll_wheel_event2(
            self.source.as_deref(),
            CGScrollEventUnit::Pixel,
            2,
            pixels(-delta_y),
            pixels(-delta_x),
            0,
        ) else {
            return;
        };
        CGEvent::set_integer_value_field(
            Some(&event),
            CGEventField::ScrollWheelEventIsContinuous,
            1,
        );
        CGEvent::set_location(Some(&event), at);
        CGEvent::set_flags(Some(&event), flags);
        post(&event);
    }

    /// A shortcut goes by the character that its key types on the viewer's keyboard, so that it
    /// means the same with the Mac's own layout, and by the key's place when the Mac's layout
    /// has no such character. Other keys carry the text that they typed on the viewer's keyboard.
    fn key(&mut self, down: bool, character: &str, code: &str, text: Option<&str>, modifiers: u8) {
        let shortcut = modifiers & (CONTROL | META) != 0 && !keys::is_modifier(code);
        let key = if down {
            let key = shortcut
                .then(|| single(character).and_then(layout::key_for))
                .flatten()
                .or_else(|| keys::virtual_key(code));
            if let Some(key) = key {
                self.keys.insert(code.to_owned(), key);
            }
            key
        } else {
            self.keys.remove(code).or_else(|| keys::virtual_key(code))
        };
        let typed = text.filter(|text| !text.is_empty() && !shortcut);
        if key.is_none() && typed.is_none() {
            return;
        }
        let Some(event) =
            CGEvent::new_keyboard_event(self.source.as_deref(), key.unwrap_or_default(), down)
        else {
            return;
        };
        if keys::is_modifier(code) {
            CGEvent::set_type(Some(&event), CGEventType::FlagsChanged);
        } else if let Some(typed) = typed {
            let units: Vec<u16> = typed.encode_utf16().take(TEXT_CHUNK).collect();
            set_text(&event, &units);
        }
        CGEvent::set_flags(Some(&event), flags(modifiers));
        post(&event);
    }

    /// Lets go of the keys and the button that viewers hold, which would stay down once the
    /// capture stops and their own release no longer comes through.
    pub(super) fn let_go(&mut self) {
        let button = std::mem::replace(&mut self.held, MouseButton::None);
        if button != MouseButton::None
            && let Some(event) = CGEvent::new(None)
        {
            let at = CGEvent::location(Some(&event));
            self.mouse(released(button), at, button, 0, CGEventFlags::empty());
        }
        for (code, key) in std::mem::take(&mut self.keys) {
            let Some(event) = CGEvent::new_keyboard_event(self.source.as_deref(), key, false)
            else {
                continue;
            };
            if keys::is_modifier(&code) {
                CGEvent::set_type(Some(&event), CGEventType::FlagsChanged);
            }
            CGEvent::set_flags(Some(&event), CGEventFlags::empty());
            post(&event);
        }
    }

    fn text(&self, text: &str) {
        let units: Vec<u16> = text.encode_utf16().collect();
        for chunk in units.chunks(TEXT_CHUNK) {
            for down in [true, false] {
                let Some(event) = CGEvent::new_keyboard_event(self.source.as_deref(), 0, down)
                else {
                    return;
                };
                set_text(&event, chunk);
                post(&event);
            }
        }
    }
}

fn single(text: &str) -> Option<char> {
    let mut characters = text.chars();
    characters.next().filter(|_| characters.next().is_none())
}

fn set_text(event: &CGEvent, units: &[u16]) {
    let length = c_ulong::try_from(units.len()).unwrap_or_default();
    // SAFETY: the pointer and the length describe `units`.
    unsafe { CGEvent::keyboard_set_unicode_string(Some(event), length, units.as_ptr()) };
}

fn post(event: &CGEvent) {
    CGEvent::set_integer_value_field(Some(event), CGEventField::EventSourceUserData, hand::MARK);
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(event));
}

fn flags(modifiers: u8) -> CGEventFlags {
    [
        (ALT, CGEventFlags::MaskAlternate),
        (CONTROL, CGEventFlags::MaskControl),
        (META, CGEventFlags::MaskCommand),
        (SHIFT, CGEventFlags::MaskShift),
    ]
    .into_iter()
    .filter(|(bit, _)| modifiers & bit != 0)
    .fold(CGEventFlags::empty(), |flags, (_, flag)| flags | flag)
}

const fn pressed(button: MouseButton) -> Option<CGEventType> {
    match button {
        MouseButton::Left => Some(CGEventType::LeftMouseDown),
        MouseButton::Right => Some(CGEventType::RightMouseDown),
        MouseButton::Middle => Some(CGEventType::OtherMouseDown),
        MouseButton::None => None,
    }
}

const fn released(button: MouseButton) -> Option<CGEventType> {
    match button {
        MouseButton::Left => Some(CGEventType::LeftMouseUp),
        MouseButton::Right => Some(CGEventType::RightMouseUp),
        MouseButton::Middle => Some(CGEventType::OtherMouseUp),
        MouseButton::None => None,
    }
}

const fn moved(held: MouseButton) -> CGEventType {
    match held {
        MouseButton::Left => CGEventType::LeftMouseDragged,
        MouseButton::Right => CGEventType::RightMouseDragged,
        MouseButton::Middle => CGEventType::OtherMouseDragged,
        MouseButton::None => CGEventType::MouseMoved,
    }
}

const fn mouse_button(button: MouseButton) -> CGMouseButton {
    match button {
        MouseButton::Right => CGMouseButton::Right,
        MouseButton::Middle => CGMouseButton::Center,
        MouseButton::Left | MouseButton::None => CGMouseButton::Left,
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "the delta is rounded and clamped to the range of an i32"
)]
fn pixels(delta: f64) -> i32 {
    delta
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}
