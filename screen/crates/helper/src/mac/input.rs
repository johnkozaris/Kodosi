//! Viewers' mouse and keys, posted as this computer's own input.

use std::ffi::c_ulong;

use objc2_core_foundation::{CFRetained, CGPoint, CGRect};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
    CGEventType, CGMouseButton, CGScrollEventUnit,
};
use stream::viewer::{Input, KeyAction, MouseAction, MouseButton};

use super::keys;

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
}

impl Hands {
    pub(super) fn new() -> Self {
        Self {
            source: CGEventSource::new(CGEventSourceStateID::HIDSystemState),
            held: MouseButton::None,
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
                code,
                text,
                modifiers,
                ..
            } => self.key(
                *action == KeyAction::Down,
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

    /// Shortcuts go by key, so that the Mac's own keyboard layout reads them; other keys carry
    /// the text that they typed on the viewer's keyboard.
    fn key(&self, down: bool, code: &str, text: Option<&str>, modifiers: u8) {
        let key = keys::virtual_key(code);
        let typed = text.filter(|text| !text.is_empty() && modifiers & (CONTROL | META) == 0);
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

fn set_text(event: &CGEvent, units: &[u16]) {
    let length = c_ulong::try_from(units.len()).unwrap_or_default();
    // SAFETY: the pointer and the length describe `units`.
    unsafe { CGEvent::keyboard_set_unicode_string(Some(event), length, units.as_ptr()) };
}

fn post(event: &CGEvent) {
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
