//! Viewers' mouse and keys, sent as this computer's own input. Windows keeps such input from the
//! windows of programs that run as administrator, so viewers cannot use those.

use std::collections::HashMap;

use stream::viewer::{Input, KeyAction, MouseAction, MouseButton};
use windows::Win32::UI::{
    Input::KeyboardAndMouse::{
        GetKeyState, GetKeyboardLayout, HKL, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE,
        KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE,
        KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, MAPVK_VK_TO_VSC_EX, MAPVK_VSC_TO_VK_EX,
        MOUSE_EVENT_FLAGS, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN,
        MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
        MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL,
        MOUSEINPUT, MapVirtualKeyExW, SendInput, ToUnicodeEx, VIRTUAL_KEY, VK_CAPITAL, VK_CONTROL,
        VK_MENU, VK_SHIFT, VkKeyScanExW,
    },
    WindowsAndMessaging::{
        GetForegroundWindow, GetSystemMetrics, GetWindowThreadProcessId, SM_CXVIRTUALSCREEN,
        SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    },
};

use super::{keys, sources::Screen};

/// A notch of Windows' wheel is 120, which browsers there scroll by 100 pixels.
const WHEEL_PER_PIXEL: f64 = 1.2;
const ALT: u8 = 1;
const CONTROL: u8 = 2;
const META: u8 = 4;
const SHIFT: u8 = 8;
/// Asks `ToUnicodeEx` to leave the keyboard's state, such as a waiting dead key, as it is.
const KEEP_STATE: u32 = 4;
/// The far corner of the desktop in the coordinates of absolute mouse input.
const FAR: f64 = 65535.0;
const ENTER: u16 = 0x001c;
const TAB: u16 = 0x000f;
const LEFT_SHIFT: u16 = 0x002a;
const ALT_GR: u16 = 0xe038;
/// The modifiers of viewer input, with the keys of the viewer's that hold each one and the key
/// that the helper presses for it.
const MODIFIERS: [(u8, [&str; 2], u16); 3] = [
    (SHIFT, ["ShiftLeft", "ShiftRight"], LEFT_SHIFT),
    (CONTROL, ["ControlLeft", "ControlRight"], 0x001d),
    (META, ["MetaLeft", "MetaRight"], 0xe05b),
];
const LEFT_ALT: u16 = 0x0038;
/// `VkKeyScanExW`'s states of Shift, and of Control with Alt, which is Alt Gr.
const SHIFT_STATE: u8 = 1;
const ALT_GR_STATE: u8 = 6;

/// What went down on the computer for a viewer's key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pressed {
    /// A key of the computer's keyboard by its place.
    Scan(u16),
    /// A key that the computer's keyboard has by its meaning only.
    Virtual(u16),
}

/// A character as a key of the computer's layout, with the Shift and Alt Gr that it needs.
#[derive(Debug, Clone, Copy)]
struct Stroke {
    scan: u16,
    shift: bool,
    alt_gr: bool,
}

/// A viewer's Alt key while they hold it. It goes down on the computer only once a key or a
/// button needs it: a Mac's Option, which viewers send as Alt, chooses the character that the
/// next key types, and Windows opens a window's menu when Alt comes up after nothing else.
struct Alt {
    code: String,
    /// Down on the computer.
    down: bool,
    /// Another key or a button went down while the viewer held Alt, so letting go is no tap.
    used: bool,
}

pub(super) struct Hands {
    /// What went down for each viewer key that is down, which lets go of the same key.
    keys: HashMap<String, Pressed>,
    alt: Option<Alt>,
    /// The mouse button that a viewer holds down.
    button: Option<MouseButton>,
    /// Turns of the wheel smaller than Windows takes, until they add up.
    wheel: (f64, f64),
}

impl Hands {
    pub(super) fn new() -> Self {
        Self {
            keys: HashMap::new(),
            alt: None,
            button: None,
            wheel: (0.0, 0.0),
        }
    }

    /// Lets go of every key and button that viewers hold down, which Windows would otherwise
    /// keep down for the person at the computer once the screen is no longer shared.
    pub(super) fn let_go(&mut self) {
        let mut inputs: Vec<INPUT> = self
            .button
            .take()
            .and_then(released)
            .map(|flags| INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dwFlags: flags,
                        ..MOUSEINPUT::default()
                    },
                },
            })
            .into_iter()
            .collect();
        inputs.extend(self.keys.drain().map(|(_, pressed)| key(pressed, false)));
        if let Some(alt) = self.alt.take().filter(|alt| alt.down)
            && let Some(scan) = keys::scan_code(&alt.code)
        {
            inputs.push(key(Pressed::Scan(scan), false));
        }
        send(&inputs);
    }

    /// Plays input whose pointer is in the points of `screen`.
    pub(super) fn play(&mut self, input: &Input, screen: &Screen) {
        match input {
            Input::Mouse {
                action,
                x,
                y,
                button,
                delta_x,
                delta_y,
                modifiers,
                ..
            } => {
                let at = screen.pixel(*x, *y);
                let inputs = match action {
                    MouseAction::Move => {
                        send(&[mouse(at, MOUSE_EVENT_FLAGS(0), 0)]);
                        return;
                    }
                    MouseAction::Press => {
                        self.hold_alt();
                        let flags = pressed(*button);
                        if flags.is_some() {
                            self.button = Some(*button);
                        }
                        flags.map(|flags| vec![mouse(at, flags, 0)])
                    }
                    MouseAction::Release => {
                        if self.button == Some(*button) {
                            self.button = None;
                        }
                        released(*button).map(|flags| vec![mouse(at, flags, 0)])
                    }
                    MouseAction::Wheel => {
                        self.hold_alt();
                        Some(self.scroll(at, *delta_x, *delta_y))
                    }
                };
                if let Some(inputs) = inputs {
                    send(&around(&self.missing(*modifiers), inputs));
                }
            }
            Input::Key {
                action,
                key,
                code,
                text,
                key_code,
                modifiers,
            } => self.key(
                *action == KeyAction::Down,
                key,
                code,
                text.as_deref(),
                *key_code,
                *modifiers,
            ),
            Input::Text { text } => {
                self.lift_alt();
                self.type_text(text, layout());
            }
        }
    }

    /// DOM wheel deltas grow as the page moves up, and Windows' wheel turns as it moves down.
    fn scroll(&mut self, at: (f64, f64), delta_x: f64, delta_y: f64) -> Vec<INPUT> {
        self.wheel.0 = delta_x.mul_add(WHEEL_PER_PIXEL, self.wheel.0);
        self.wheel.1 = delta_y.mul_add(-WHEEL_PER_PIXEL, self.wheel.1);
        let (across, down) = (whole(&mut self.wheel.0), whole(&mut self.wheel.1));
        let mut inputs = vec![mouse(at, MOUSE_EVENT_FLAGS(0), 0)];
        if down != 0 {
            inputs.push(mouse(at, MOUSEEVENTF_WHEEL, down));
        }
        if across != 0 {
            inputs.push(mouse(at, MOUSEEVENTF_HWHEEL, across));
        }
        inputs
    }

    /// A shortcut, which is a key with Control or Meta, or with Alt and no text, goes by the
    /// character that its key types on the viewer's keyboard, so that it means the same with the
    /// computer's own layout, and by the key's place when that layout has no such character. Other
    /// keys go by their place when it types the same text on the computer, and as their text
    /// otherwise.
    fn key(
        &mut self,
        down: bool,
        character: &str,
        code: &str,
        text: Option<&str>,
        key_code: u32,
        modifiers: u8,
    ) {
        if matches!(code, "AltLeft" | "AltRight") {
            self.alt_key(down, code);
            return;
        }
        if !down {
            if let Some(pressed) = self.keys.remove(code) {
                send(&[key(pressed, false)]);
            }
            return;
        }
        if let Some(alt) = &mut self.alt {
            alt.used = true;
        }
        if keys::is_modifier(code) {
            if let Some(pressed) = placed(code, key_code) {
                self.press(code, pressed, &[]);
            }
            return;
        }
        let text = text.filter(|text| printable(text));
        if text.is_none() && matches!(character, "" | "Dead") {
            // The viewer's keyboard composes a character, which the key after this one types.
            return;
        }
        let layout = layout();
        let shortcut =
            modifiers & (CONTROL | META) != 0 || (modifiers & ALT != 0 && text.is_none());
        if !shortcut
            && let Some(text) = text
            && !code.starts_with("Numpad")
        {
            self.lift_alt();
            let placed = keys::scan_code(code).map(|scan| Stroke {
                scan,
                shift: !self.shifts().is_empty(),
                alt_gr: false,
            });
            match placed.filter(|stroke| types(*stroke, text, layout)) {
                Some(stroke) => self.press(code, Pressed::Scan(stroke.scan), &[]),
                None => self.type_text(text, layout),
            }
            return;
        }
        let pressed = if shortcut {
            single(character)
                .and_then(|letter| stroke(letter, layout))
                .map(|stroke| Pressed::Scan(stroke.scan))
                .or_else(|| placed(code, key_code))
        } else {
            placed(code, key_code)
        };
        if let Some(pressed) = pressed {
            self.hold_alt();
            let added = self.missing(modifiers);
            self.press(code, pressed, &added);
        }
    }

    /// Types text with the keys of the computer's layout, so that it arrives whole however slowly
    /// a program reads its keys, and as characters where the layout has no key for them.
    fn type_text(&self, text: &str, layout: HKL) {
        let shifts = self.shifts();
        let mut inputs = Vec::new();
        let mut characters = text.chars().peekable();
        while let Some(character) = characters.next() {
            let named = match character {
                '\r' => {
                    characters.next_if_eq(&'\n');
                    Some(ENTER)
                }
                '\n' => Some(ENTER),
                '\t' => Some(TAB),
                _ => None,
            };
            if let Some(scan) = named {
                inputs.extend([
                    key(Pressed::Scan(scan), true),
                    key(Pressed::Scan(scan), false),
                ]);
            } else if let Some(stroke) = stroke(character, layout) {
                strike(&mut inputs, stroke, &shifts);
            } else {
                let mut units = [0; 2];
                for unit in character.encode_utf16(&mut units) {
                    inputs.extend([packet(*unit, true), packet(*unit, false)]);
                }
            }
        }
        send(&inputs);
    }

    /// Presses a key with `added` modifiers around it, and lets go of what the same viewer key
    /// pressed before when that was another key.
    fn press(&mut self, code: &str, pressed: Pressed, added: &[Pressed]) {
        let mut inputs = Vec::new();
        if let Some(earlier) = self.keys.insert(code.to_owned(), pressed)
            && earlier != pressed
        {
            inputs.push(key(earlier, false));
        }
        inputs.extend(around(added, vec![key(pressed, true)]));
        send(&inputs);
    }

    /// The Shift keys that the viewer holds down on the computer.
    fn shifts(&self) -> Vec<Pressed> {
        ["ShiftLeft", "ShiftRight"]
            .into_iter()
            .filter_map(|code| self.keys.get(code).copied())
            .collect()
    }

    /// The modifiers that an input's bitmask holds and the computer does not hold for the viewer,
    /// as when the viewer's own modifier key went down elsewhere or its message was lost.
    fn missing(&self, modifiers: u8) -> Vec<Pressed> {
        let mut missing: Vec<Pressed> = MODIFIERS
            .into_iter()
            .filter(|(bit, codes, _)| {
                modifiers & bit != 0 && !codes.iter().any(|code| self.keys.contains_key(*code))
            })
            .map(|(_, _, scan)| Pressed::Scan(scan))
            .collect();
        if modifiers & ALT != 0 && self.alt.is_none() {
            missing.push(Pressed::Scan(LEFT_ALT));
        }
        missing
    }

    /// Alt that comes up before anything needed it was a tap, which Windows takes as asking for
    /// the menu of the window in front.
    fn alt_key(&mut self, down: bool, code: &str) {
        if down {
            if self.alt.is_none() {
                self.alt = Some(Alt {
                    code: code.to_owned(),
                    down: false,
                    used: false,
                });
            }
            return;
        }
        let Some(alt) = self.alt.take_if(|alt| alt.code == code) else {
            return;
        };
        let Some(place) = keys::scan_code(code).map(Pressed::Scan) else {
            return;
        };
        if alt.down {
            send(&[key(place, false)]);
        } else if !alt.used {
            send(&[key(place, true), key(place, false)]);
        }
    }

    /// Puts the viewer's Alt down on the computer for a key or a button that it changes.
    fn hold_alt(&mut self) {
        let Some(alt) = &mut self.alt else {
            return;
        };
        alt.used = true;
        if !alt.down
            && let Some(scan) = keys::scan_code(&alt.code)
        {
            alt.down = true;
            send(&[key(Pressed::Scan(scan), true)]);
        }
    }

    /// Lets go of Alt on the computer before text, which Alt would turn into a shortcut.
    fn lift_alt(&mut self) {
        let Some(alt) = &mut self.alt else {
            return;
        };
        alt.used = true;
        if alt.down
            && let Some(scan) = keys::scan_code(&alt.code)
        {
            alt.down = false;
            send(&[key(Pressed::Scan(scan), false)]);
        }
    }
}

/// The key at the viewer key's place, or the key that the viewer named.
fn placed(code: &str, key_code: u32) -> Option<Pressed> {
    keys::scan_code(code).map(Pressed::Scan).or_else(|| {
        u16::try_from(key_code)
            .ok()
            .filter(|key| *key != 0)
            .map(Pressed::Virtual)
    })
}

/// The keyboard layout of the window in front, which types what the viewer's keys send.
fn layout() -> HKL {
    // SAFETY: getters; without a window in front, the thread is 0 and the layout the helper's.
    unsafe { GetKeyboardLayout(GetWindowThreadProcessId(GetForegroundWindow(), None)) }
}

/// The key that types `character` on the computer's layout with its Caps Lock, when it has one.
fn stroke(character: char, layout: HKL) -> Option<Stroke> {
    let mut units = [0; 2];
    let [unit] = character.encode_utf16(&mut units) else {
        return None;
    };
    // SAFETY: a lookup in the layout.
    let found = unsafe { VkKeyScanExW(*unit, layout) };
    if found == -1 {
        return None;
    }
    let [virtual_key, state] = found.to_le_bytes();
    // SAFETY: as above.
    let scan =
        unsafe { MapVirtualKeyExW(u32::from(virtual_key), MAPVK_VK_TO_VSC_EX, Some(layout)) };
    let scan = u16::try_from(scan).ok().filter(|scan| *scan != 0)?;
    let alt_gr = state & ALT_GR_STATE == ALT_GR_STATE;
    let shift = state & SHIFT_STATE != 0;
    let text = character.to_string();
    // Caps Lock turns Shift around for letters.
    [shift, !shift]
        .into_iter()
        .map(|shift| Stroke {
            scan,
            shift,
            alt_gr,
        })
        .find(|stroke| types(*stroke, &text, layout))
}

/// A stroke's key with the Shift and Alt Gr that its character needs, and without the Shift that
/// the viewer holds when the character does not need it.
fn strike(inputs: &mut Vec<INPUT>, stroke: Stroke, shifts: &[Pressed]) {
    let lifted = if stroke.shift { &[][..] } else { shifts };
    let added: Vec<Pressed> = [
        (stroke.shift && shifts.is_empty()).then_some(Pressed::Scan(LEFT_SHIFT)),
        stroke.alt_gr.then_some(Pressed::Scan(ALT_GR)),
    ]
    .into_iter()
    .flatten()
    .collect();
    inputs.extend(lifted.iter().map(|shift| key(*shift, false)));
    inputs.extend(added.iter().map(|pressed| key(*pressed, true)));
    inputs.extend([
        key(Pressed::Scan(stroke.scan), true),
        key(Pressed::Scan(stroke.scan), false),
    ]);
    inputs.extend(added.iter().rev().map(|pressed| key(*pressed, false)));
    inputs.extend(lifted.iter().map(|shift| key(*shift, true)));
}

/// True when the stroke types `text` on the computer's layout with its Caps Lock.
fn types(stroke: Stroke, text: &str, layout: HKL) -> bool {
    // SAFETY: a lookup in the layout.
    let virtual_key =
        unsafe { MapVirtualKeyExW(u32::from(stroke.scan), MAPVK_VSC_TO_VK_EX, Some(layout)) };
    if virtual_key == 0 {
        return false;
    }
    let mut state = [0_u8; 256];
    if stroke.shift {
        state[usize::from(VK_SHIFT.0)] = 0x80;
    }
    if stroke.alt_gr {
        state[usize::from(VK_CONTROL.0)] = 0x80;
        state[usize::from(VK_MENU.0)] = 0x80;
    }
    // SAFETY: a getter.
    if unsafe { GetKeyState(i32::from(VK_CAPITAL.0)) } & 1 != 0 {
        state[usize::from(VK_CAPITAL.0)] = 1;
    }
    let mut units = [0_u16; 8];
    // SAFETY: the buffers are as long as their slices say, and the call keeps the keyboard's state.
    let written = unsafe {
        ToUnicodeEx(
            virtual_key,
            u32::from(stroke.scan & 0xff),
            &state,
            &mut units,
            KEEP_STATE,
            Some(layout),
        )
    };
    usize::try_from(written)
        .ok()
        .and_then(|written| units.get(..written))
        .is_some_and(|units| String::from_utf16(units).is_ok_and(|typed| typed == text))
}

fn single(text: &str) -> Option<char> {
    let mut characters = text.chars();
    characters.next().filter(|_| characters.next().is_none())
}

fn printable(text: &str) -> bool {
    !text.is_empty() && !text.chars().any(char::is_control)
}

/// Input with modifiers that go down before it and come up after it.
fn around(added: &[Pressed], inputs: Vec<INPUT>) -> Vec<INPUT> {
    added
        .iter()
        .map(|modifier| key(*modifier, true))
        .chain(inputs)
        .chain(added.iter().rev().map(|modifier| key(*modifier, false)))
        .collect()
}

/// A character that the layout has no key for, which Windows hands to the program in front as
/// it reads its next key.
fn packet(unit: u16, down: bool) -> INPUT {
    keyboard(KEYBDINPUT {
        wScan: unit,
        dwFlags: if down {
            KEYEVENTF_UNICODE
        } else {
            KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
        },
        ..KEYBDINPUT::default()
    })
}

fn key(pressed: Pressed, down: bool) -> INPUT {
    let up = if down {
        KEYBD_EVENT_FLAGS(0)
    } else {
        KEYEVENTF_KEYUP
    };
    keyboard(match pressed {
        Pressed::Scan(scan) => KEYBDINPUT {
            wScan: scan & 0xff,
            dwFlags: KEYEVENTF_SCANCODE
                | up
                | if scan & 0xff00 == 0xe000 {
                    KEYEVENTF_EXTENDEDKEY
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
            ..KEYBDINPUT::default()
        },
        Pressed::Virtual(virtual_key) => KEYBDINPUT {
            wVk: VIRTUAL_KEY(virtual_key),
            // SAFETY: a lookup in the helper's layout.
            wScan: u16::try_from(unsafe {
                MapVirtualKeyExW(u32::from(virtual_key), MAPVK_VK_TO_VSC, None)
            })
            .unwrap_or_default(),
            dwFlags: up,
            ..KEYBDINPUT::default()
        },
    })
}

fn keyboard(ki: KEYBDINPUT) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki },
    }
}

/// The pointer at a pixel of the desktop, with `flags` and the wheel's `turn`.
fn mouse(at: (f64, f64), flags: MOUSE_EVENT_FLAGS, turn: i32) -> INPUT {
    // SAFETY: getters.
    let (left, top, width, height) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: normalized(at.0, left, width),
                dy: normalized(at.1, top, height),
                mouseData: u32::from_ne_bytes(turn.to_ne_bytes()),
                dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK | flags,
                ..MOUSEINPUT::default()
            },
        },
    }
}

/// A desktop pixel in absolute mouse input, which spans the desktop from 0 to `FAR`.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the value is rounded and clamped to the range of absolute input"
)]
fn normalized(at: f64, start: i32, extent: i32) -> i32 {
    ((at - f64::from(start)) * FAR / f64::from((extent - 1).max(1)))
        .round()
        .clamp(0.0, FAR) as i32
}

/// Takes the whole part of the wheel's turn, and leaves the rest for later.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the turn is truncated to whole units in the range of an i32"
)]
fn whole(turn: &mut f64) -> i32 {
    let taken = turn.trunc().clamp(f64::from(i32::MIN), f64::from(i32::MAX));
    *turn -= taken;
    taken as i32
}

fn send(inputs: &[INPUT]) {
    if inputs.is_empty() {
        return;
    }
    // SAFETY: the inputs are complete and the size is theirs.
    unsafe {
        SendInput(
            inputs,
            i32::try_from(size_of::<INPUT>()).unwrap_or_default(),
        )
    };
}

fn pressed(button: MouseButton) -> Option<MOUSE_EVENT_FLAGS> {
    match button {
        MouseButton::Left => Some(MOUSEEVENTF_LEFTDOWN),
        MouseButton::Right => Some(MOUSEEVENTF_RIGHTDOWN),
        MouseButton::Middle => Some(MOUSEEVENTF_MIDDLEDOWN),
        MouseButton::None => None,
    }
}

fn released(button: MouseButton) -> Option<MOUSE_EVENT_FLAGS> {
    match button {
        MouseButton::Left => Some(MOUSEEVENTF_LEFTUP),
        MouseButton::Right => Some(MOUSEEVENTF_RIGHTUP),
        MouseButton::Middle => Some(MOUSEEVENTF_MIDDLEUP),
        MouseButton::None => None,
    }
}
