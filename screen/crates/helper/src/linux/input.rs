//! Viewers' mouse and keys, played through the desktop's libei server as the computer's own
//! pointer and keyboard.

use std::{
    collections::HashMap,
    io,
    os::{
        fd::{AsFd as _, OwnedFd},
        unix::net::UnixStream,
    },
    sync::Arc,
};

use futures_util::StreamExt as _;
use reis::{
    ei,
    event::{Connection, Device, DeviceCapability, EiEvent},
};
use stream::viewer::{Input, KeyAction, MouseAction, MouseButton};
use tokio::{io::Interest, sync::mpsc};

use super::{
    hand::Hand,
    keymap::{Keymap, Stroke},
    keys,
};

const LEFT_BUTTON: u32 = 0x110;
const RIGHT_BUTTON: u32 = 0x111;
const MIDDLE_BUTTON: u32 = 0x112;
const ALT: u8 = 1;
const CONTROL: u8 = 2;
const META: u8 = 4;
const SHIFT: u8 = 8;
/// Long text goes in parts of this many characters, each sent before the next is played.
const TEXT_PART: usize = 64;
/// The viewer's modifier bits, with the computer's keys for each, the one to press first.
const MODIFIERS: [(u8, [u32; 2]); 4] = [
    (SHIFT, [keys::LEFT_SHIFT, keys::RIGHT_SHIFT]),
    (CONTROL, [keys::LEFT_CONTROL, keys::RIGHT_CONTROL]),
    (ALT, [keys::LEFT_ALT, keys::RIGHT_ALT]),
    (META, [keys::LEFT_META, keys::RIGHT_META]),
];

/// The input task of one portal session.
pub(super) struct Hands {
    orders: mpsc::UnboundedSender<(Input, (f64, f64))>,
    task: tokio::task::JoinHandle<()>,
}

impl Hands {
    pub(super) fn start(eis: OwnedFd, hand: Arc<Hand>) -> Self {
        let (orders, taken) = mpsc::unbounded_channel();
        Self {
            orders,
            task: tokio::task::spawn_local(run(eis, taken, hand)),
        }
    }

    /// Plays input whose pointer is in the points of a display whose top left is at `origin`
    /// on the desktop.
    pub(super) fn play(&self, input: Input, origin: (f64, f64)) {
        let Input::Text { text } = input else {
            drop(self.orders.send((input, origin)));
            return;
        };
        let mut characters = text.chars().peekable();
        while characters.peek().is_some() {
            let text = characters.by_ref().take(TEXT_PART).collect();
            drop(self.orders.send((Input::Text { text }, origin)));
        }
    }
}

impl Drop for Hands {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[expect(
    clippy::future_not_send,
    reason = "libei's connection stays on the helper's thread"
)]
async fn run(
    eis: OwnedFd,
    mut taken: mpsc::UnboundedReceiver<(Input, (f64, f64))>,
    hand: Arc<Hand>,
) {
    let Ok(context) = ei::Context::new(UnixStream::from(eis)) else {
        return;
    };
    let Ok((connection, mut events)) = context
        .handshake_tokio("Kodosi Screen", ei::handshake::ContextType::Sender)
        .await
    else {
        return;
    };
    // reis already watches this socket for the server's events, so the helper watches a copy of
    // it to learn when the socket can take more.
    let Ok(socket) = context
        .as_fd()
        .try_clone_to_owned()
        .and_then(|socket| tokio::net::UnixStream::from_std(UnixStream::from(socket)))
    else {
        return;
    };
    let mut hands = Played::new(connection, hand);
    loop {
        tokio::select! {
            event = events.next() => match event {
                Some(Ok(event)) => hands.hear(event),
                _ => return,
            },
            order = taken.recv() => match order {
                Some((input, origin)) => hands.play(&input, origin),
                None => return,
            },
        }
        if !sent(&hands.connection, &socket).await {
            return;
        }
    }
}

/// Sends what the helper played, waiting while the desktop's libei server has yet to read what
/// came before. Returns false once the connection is lost.
async fn sent(connection: &Connection, socket: &tokio::net::UnixStream) -> bool {
    loop {
        if socket.writable().await.is_err() {
            return false;
        }
        match socket.try_io(Interest::WRITABLE, || Ok(connection.flush()?)) {
            Ok(()) => return true,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(_) => return false,
        }
    }
}

/// The devices that the libei server lends, and what the helper holds down on them.
struct Played {
    connection: Connection,
    hand: Arc<Hand>,
    sequence: u32,
    /// The devices that take input now.
    devices: Vec<Device>,
    keymap: Option<Keymap>,
    at: Option<(f32, f32)>,
    /// The kernel's key that each viewer key holds down, which lets go of the same key.
    held: HashMap<String, u32>,
}

impl Played {
    fn new(connection: Connection, hand: Arc<Hand>) -> Self {
        Self {
            connection,
            hand,
            sequence: 0,
            devices: Vec::new(),
            keymap: None,
            at: None,
            held: HashMap::new(),
        }
    }

    fn hear(&mut self, event: EiEvent) {
        match event {
            EiEvent::SeatAdded(added) => added.seat.bind_capabilities(
                DeviceCapability::Pointer
                    | DeviceCapability::PointerAbsolute
                    | DeviceCapability::Button
                    | DeviceCapability::Scroll
                    | DeviceCapability::Keyboard
                    | DeviceCapability::Text,
            ),
            EiEvent::DeviceAdded(added) => {
                if let Some(keymap) = added.device.keymap()
                    && let Some(keymap) = Keymap::read(&keymap.fd, keymap.size)
                {
                    self.keymap = Some(keymap);
                }
            }
            EiEvent::DeviceResumed(resumed) => {
                resumed
                    .device
                    .device()
                    .start_emulating(resumed.serial, self.sequence);
                self.sequence = self.sequence.wrapping_add(1);
                self.devices.push(resumed.device);
            }
            EiEvent::DevicePaused(paused) => {
                self.devices.retain(|device| *device != paused.device);
            }
            EiEvent::DeviceRemoved(removed) => {
                self.devices.retain(|device| *device != removed.device);
            }
            EiEvent::KeyboardModifiers(modifiers) => {
                if let Some(keymap) = &mut self.keymap {
                    keymap.follow(modifiers.group);
                }
            }
            _ => {}
        }
    }

    fn play(&mut self, input: &Input, origin: (f64, f64)) {
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
                self.point(origin.0 + x, origin.1 + y);
                match action {
                    MouseAction::Press => {
                        self.holding(*modifiers, |hands| hands.button(*button, true));
                    }
                    MouseAction::Release => {
                        self.holding(*modifiers, |hands| hands.button(*button, false));
                    }
                    MouseAction::Wheel => {
                        self.holding(*modifiers, |hands| hands.scroll(*delta_x, *delta_y));
                    }
                    MouseAction::Move => {}
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
            Input::Text { text } => {
                self.type_text(text);
            }
        }
    }

    fn device(&self, capability: DeviceCapability) -> Option<&Device> {
        self.devices
            .iter()
            .find(|device| device.has_capability(capability))
    }

    fn frame(&self, device: &Device) {
        device.device().frame(self.connection.serial(), now());
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "a desktop's logical pixels fit a 32-bit float"
    )]
    fn point(&mut self, x: f64, y: f64) {
        let at = (x as f32, y as f32);
        if self.at == Some(at) {
            return;
        }
        // A desktop may give each display an absolute pointer of its own.
        let pointers = || {
            self.devices
                .iter()
                .filter(|device| device.has_capability(DeviceCapability::PointerAbsolute))
        };
        let Some(device) = pointers()
            .find(|device| device.regions().iter().any(|region| holds(region, x, y)))
            .or_else(|| pointers().next())
        else {
            return;
        };
        if let Some(pointer) = device.interface::<ei::PointerAbsolute>() {
            pointer.motion_absolute(at.0, at.1);
            self.frame(device);
            self.at = Some(at);
            self.hand.put((x, y));
        }
    }

    fn button(&self, button: MouseButton, down: bool) {
        let code = match button {
            MouseButton::Left => LEFT_BUTTON,
            MouseButton::Right => RIGHT_BUTTON,
            MouseButton::Middle => MIDDLE_BUTTON,
            MouseButton::None => return,
        };
        let Some(device) = self.device(DeviceCapability::Button) else {
            return;
        };
        if let Some(buttons) = device.interface::<ei::Button>() {
            buttons.button(
                code,
                if down {
                    ei::button::ButtonState::Press
                } else {
                    ei::button::ButtonState::Released
                },
            );
            self.frame(device);
        }
    }

    /// DOM wheel deltas, like libei's, grow as the page moves up.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a scroll's pixels fit a 32-bit float"
    )]
    fn scroll(&self, delta_x: f64, delta_y: f64) {
        let Some(device) = self.device(DeviceCapability::Scroll) else {
            return;
        };
        if let Some(scroll) = device.interface::<ei::Scroll>() {
            scroll.scroll(delta_x as f32, delta_y as f32);
            self.frame(device);
        }
    }

    /// A shortcut goes by the character that its key types on the viewer's keyboard, so that it
    /// means the same with the computer's own layout, and by the key's place when the layout has
    /// no such character. Other keys type the text that they typed on the viewer's keyboard.
    fn key(&mut self, down: bool, character: &str, code: &str, text: Option<&str>, modifiers: u8) {
        let shortcut = modifiers & (CONTROL | META) != 0 && !keys::is_modifier(code);
        if !down {
            if let Some(key) = self.held.remove(code) {
                self.press(key, false);
            }
            return;
        }
        if let Some(text) = text.filter(|text| !shortcut && typed(text))
            && self.type_text(text)
        {
            return;
        }
        let by_letter = shortcut
            .then(|| single(character))
            .flatten()
            .and_then(|letter| self.keymap.as_ref()?.stroke(letter.to_ascii_lowercase()))
            .map(|stroke| stroke.key);
        let Some(key) = by_letter.or_else(|| keys::key(code)) else {
            return;
        };
        if let Some(repeated) = self.held.insert(code.to_owned(), key) {
            self.press(repeated, false);
        }
        if keys::is_modifier(code) {
            self.press(key, true);
        } else {
            self.holding(modifiers, |hands| hands.press(key, true));
        }
    }

    /// Plays `act` with `modifiers` down: presses those that the viewer does not hold down, as
    /// when the key down of the viewer's own modifier never came, and lets go of them after.
    fn holding(&self, modifiers: u8, act: impl FnOnce(&Self)) {
        let added: Vec<u32> = MODIFIERS
            .into_iter()
            .filter(|(bit, sides)| {
                modifiers & bit != 0 && !sides.iter().any(|key| self.holds(*key))
            })
            .map(|(_, [key, _])| key)
            .collect();
        for key in &added {
            self.press(*key, true);
        }
        act(self);
        for key in added.iter().rev() {
            self.press(*key, false);
        }
    }

    fn holds(&self, key: u32) -> bool {
        self.held.values().any(|held| *held == key)
    }

    fn press(&self, key: u32, down: bool) {
        let Some(device) = self.device(DeviceCapability::Keyboard) else {
            return;
        };
        if let Some(keyboard) = device.interface::<ei::Keyboard>() {
            keyboard.key(
                key,
                if down {
                    ei::keyboard::KeyState::Press
                } else {
                    ei::keyboard::KeyState::Released
                },
            );
            self.frame(device);
        }
    }

    /// Types text with the keys of the computer's layout, or as text where the desktop takes it
    /// and the layout has no key for a character. Returns false when it cannot type all of it.
    fn type_text(&self, text: &str) -> bool {
        let strokes: Vec<Option<Stroke>> = text
            .chars()
            .map(|character| self.keymap.as_ref()?.stroke(character))
            .collect();
        let writer = self.device(DeviceCapability::Text);
        if strokes.iter().any(Option::is_none) {
            let Some(writer) = writer else {
                return false;
            };
            if let Some(text_input) = writer.interface::<ei::Text>() {
                text_input.utf8(text);
                self.frame(writer);
                return true;
            }
            return false;
        }
        for stroke in strokes.into_iter().flatten() {
            self.stroke(stroke);
        }
        true
    }

    /// Presses a key with the modifiers that its character needs, and with no other ones that
    /// the viewer holds down, then leaves the modifiers as they were.
    fn stroke(&self, stroke: Stroke) {
        let third = self.keymap.as_ref().and_then(Keymap::third_level);
        let held = |key: &u32| self.holds(*key);
        let shifts = [keys::LEFT_SHIFT, keys::RIGHT_SHIFT];
        let mut lifted = Vec::new();
        let mut added = Vec::new();
        if !stroke.shift {
            lifted.extend(shifts.into_iter().filter(held));
        } else if !shifts.iter().any(held) {
            added.push(keys::LEFT_SHIFT);
        }
        if let Some(third) = third {
            match (held(&third), stroke.third) {
                (true, false) => lifted.push(third),
                (false, true) => added.push(third),
                _ => {}
            }
        }
        // Alt would make the character a shortcut.
        lifted.extend(
            [keys::LEFT_ALT, keys::RIGHT_ALT]
                .into_iter()
                .filter(|alt| Some(*alt) != third && held(alt)),
        );
        for key in &lifted {
            self.press(*key, false);
        }
        for key in &added {
            self.press(*key, true);
        }
        self.press(stroke.key, true);
        self.press(stroke.key, false);
        for key in added.iter().rev() {
            self.press(*key, false);
        }
        for key in lifted.iter().rev() {
            self.press(*key, true);
        }
    }
}

fn holds(region: &reis::event::Region, x: f64, y: f64) -> bool {
    let (left, top) = (f64::from(region.x), f64::from(region.y));
    (left..left + f64::from(region.width)).contains(&x)
        && (top..top + f64::from(region.height)).contains(&y)
}

/// Text that a key types, rather than a control character such as Enter's.
fn typed(text: &str) -> bool {
    !text.is_empty() && !text.chars().any(char::is_control)
}

fn single(text: &str) -> Option<char> {
    let mut characters = text.chars();
    characters.next().filter(|_| characters.next().is_none())
}

/// libei's time: microseconds of the monotonic clock.
fn now() -> u64 {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    u64::try_from(time.tv_sec).unwrap_or_default() * 1_000_000
        + u64::try_from(time.tv_nsec).unwrap_or_default() / 1_000
}
