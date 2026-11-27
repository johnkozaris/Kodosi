//! Viewers' hands at the X server, on one connection on a thread of its own: its test devices play
//! their input as the computer's own pointer and keyboard, its clipboard takes their text and
//! gives them what the person at the computer copies, and `RandR` lists its displays.

use std::{
    collections::HashMap,
    os::fd::{AsFd as _, OwnedFd},
    sync::{Arc, mpsc as orders},
    thread::JoinHandle,
};

use rustix::event::{EventfdFlags, PollFd, PollFlags};
use screen::link::MOST_CLIPBOARD;
use stream::viewer::{Input, KeyAction, MouseAction, MouseButton};
use tokio::sync::{mpsc, oneshot};
use x11rb::{
    CURRENT_TIME, NONE,
    connection::Connection as _,
    protocol::{
        Event as Happened,
        randr::{self, ConnectionExt as _},
        xfixes::{self, ConnectionExt as _},
        xproto::{
            self, Atom, AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, Mapping,
            PropMode, SelectionNotifyEvent, SelectionRequestEvent, Window, WindowClass,
        },
        xtest::ConnectionExt as _,
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};
use xkeysym::Keysym;

use super::{Monitor, keys, problem, session::Event};
use crate::linux::{
    hand::Hand,
    input::{single, typed},
    keys::is_modifier,
};

const ALT: u8 = 1;
const CONTROL: u8 = 2;
const META: u8 = 4;
const SHIFT: u8 = 8;
/// The viewer's modifier bits, with the keysym that the X server's keymap holds for each.
const MODIFIERS: [(u8, Keysym); 4] = [
    (SHIFT, Keysym::Shift_L),
    (CONTROL, Keysym::Control_L),
    (ALT, Keysym::Alt_L),
    (META, Keysym::Super_L),
];
/// Browsers scroll about this many pixels for one notch of a wheel, which X11 turns as a click of
/// a button.
const WHEEL_STEP: f64 = 100.0;
const WHEEL_UP: u8 = 4;
const WHEEL_DOWN: u8 = 5;
const WHEEL_LEFT: u8 = 6;
const WHEEL_RIGHT: u8 = 7;
/// Where the core keyboard mapping puts the levels of the first group: plain, with Shift, with
/// `AltGr`, which xkb calls the third level, and with both.
const LEVELS: [Level; 4] = [
    Level {
        index: 0,
        shift: false,
        third: false,
    },
    Level {
        index: 1,
        shift: true,
        third: false,
    },
    Level {
        index: 4,
        shift: false,
        third: true,
    },
    Level {
        index: 5,
        shift: true,
        third: true,
    },
];

enum Order {
    /// Input whose pointer is in the points of a display whose top left is at the origin.
    Play(Input, (f64, f64)),
    /// Lets go of what viewers hold down.
    LetGo,
    /// Puts a viewer's text on the clipboard, and answers once the helper holds it there.
    Put(String, oneshot::Sender<()>),
    /// Whether the session wants what the person at the computer copies.
    Copies(bool),
    Monitors(oneshot::Sender<Vec<Monitor>>),
}

pub(super) struct Desk {
    orders: orders::Sender<Order>,
    /// Wakes the thread, which waits for the X server otherwise.
    wake: Arc<OwnedFd>,
    thread: Option<JoinHandle<()>>,
}

impl Desk {
    pub(super) async fn open(
        events: mpsc::UnboundedSender<Event>,
        hand: Arc<Hand>,
    ) -> Result<Self, String> {
        let wake = rustix::event::eventfd(0, EventfdFlags::CLOEXEC | EventfdFlags::NONBLOCK)
            .map_err(|error| error.to_string())?;
        let wake = Arc::new(wake);
        let (orders, taken) = orders::channel();
        let (ready, opened) = oneshot::channel();
        let woken = Arc::clone(&wake);
        let thread = std::thread::Builder::new()
            .name("kodosi-screen-desk".to_owned())
            .spawn(move || match Desked::open(events, hand) {
                Ok(mut desked) => {
                    drop(ready.send(Ok(())));
                    desked.run(&taken, &woken);
                    desked.end();
                }
                Err(problem) => drop(ready.send(Err(problem))),
            })
            .map_err(|error| error.to_string())?;
        opened
            .await
            .unwrap_or_else(|_| Err("the X server did not answer".to_owned()))?;
        Ok(Self {
            orders,
            wake,
            thread: Some(thread),
        })
    }

    pub(super) fn play(&self, input: Input, origin: (f64, f64)) {
        self.order(Order::Play(input, origin));
    }

    pub(super) fn let_go(&self) {
        self.order(Order::LetGo);
    }

    pub(super) fn copies(&self, wanted: bool) {
        self.order(Order::Copies(wanted));
    }

    /// Returns once the text is on the clipboard, so that the paste that follows takes it.
    pub(super) async fn put(&self, text: String) {
        let (reply, put) = oneshot::channel();
        self.order(Order::Put(text, reply));
        drop(put.await);
    }

    /// The displays, the main one first.
    pub(super) async fn monitors(&self) -> Vec<Monitor> {
        let (reply, listed) = oneshot::channel();
        self.order(Order::Monitors(reply));
        listed.await.unwrap_or_default()
    }

    fn order(&self, order: Order) {
        if self.orders.send(order).is_ok() {
            wake(&self.wake);
        }
    }
}

impl Drop for Desk {
    fn drop(&mut self) {
        // The thread ends once it finds no more orders can come.
        drop(std::mem::replace(&mut self.orders, orders::channel().0));
        wake(&self.wake);
        if let Some(thread) = self.thread.take() {
            drop(thread.join());
        }
    }
}

fn wake(fd: &OwnedFd) {
    let _ = rustix::io::write(fd, &1_u64.to_ne_bytes());
}

#[derive(Debug, Clone, Copy)]
struct Level {
    index: usize,
    shift: bool,
    third: bool,
}

struct Atoms {
    clipboard: Atom,
    utf8: Atom,
    targets: Atom,
    text: Atom,
    incr: Atom,
    /// Where the clipboard's text comes to the helper's window.
    landing: Atom,
}

impl Atoms {
    fn intern(x: &RustConnection) -> Result<Self, String> {
        let atom = |name: &[u8]| -> Result<Atom, String> {
            Ok(x.intern_atom(false, name)
                .map_err(problem)?
                .reply()
                .map_err(problem)?
                .atom)
        };
        Ok(Self {
            clipboard: atom(b"CLIPBOARD")?,
            utf8: atom(b"UTF8_STRING")?,
            targets: atom(b"TARGETS")?,
            text: atom(b"TEXT")?,
            incr: atom(b"INCR")?,
            landing: atom(b"KODOSI_SCREEN_CLIPBOARD")?,
        })
    }
}

/// The X server's keymap as the core protocol tells it: the keysyms of each key, by level.
struct Keyboard {
    first: u8,
    per: usize,
    keysyms: Vec<u32>,
    /// Keys without any keysym, which type a character that the keymap lacks, in turn.
    spares: Vec<u8>,
    next_spare: usize,
    /// Spares that hold a character for now, which go back to holding none at the end.
    lent: Vec<u8>,
}

impl Keyboard {
    fn read(x: &RustConnection) -> Result<Self, String> {
        let setup = x.setup();
        let (first, last) = (setup.min_keycode, setup.max_keycode);
        let mapping = x
            .get_keyboard_mapping(first, last.saturating_sub(first).saturating_add(1))
            .map_err(problem)?
            .reply()
            .map_err(problem)?;
        let per = usize::from(mapping.keysyms_per_keycode).max(1);
        let spares = mapping
            .keysyms
            .chunks(per)
            .zip(first..=last)
            .filter(|(keysyms, _)| keysyms.iter().all(|keysym| *keysym == 0))
            .map(|(_, key)| key)
            .collect();
        Ok(Self {
            first,
            per,
            keysyms: mapping.keysyms,
            spares,
            next_spare: 0,
            lent: Vec::new(),
        })
    }

    /// Reads the keymap again after it changed, keeping the spares that hold a character.
    fn reread(&mut self, x: &RustConnection) {
        if let Ok(mut read) = Self::read(x) {
            read.spares.retain(|spare| !self.lent.contains(spare));
            read.spares.extend(self.lent.iter().copied());
            read.lent = std::mem::take(&mut self.lent);
            read.next_spare = self.next_spare;
            *self = read;
        }
    }

    /// The key and level that type `keysym` in the first group.
    fn find(&self, keysym: Keysym) -> Option<(u8, Level)> {
        let keysym = keysym.raw();
        if keysym == 0 {
            return None;
        }
        LEVELS.into_iter().find_map(|level| {
            self.keysyms
                .chunks(self.per)
                .position(|keysyms| keysyms.get(level.index) == Some(&keysym))
                .and_then(|place| self.first.checked_add(u8::try_from(place).ok()?))
                .map(|key| (key, level))
        })
    }

    /// The key that holds `keysym` plainly, or at any level.
    fn key(&self, keysym: Keysym) -> Option<u8> {
        self.find(keysym).map(|(key, _)| key).or_else(|| {
            let place = self
                .keysyms
                .chunks(self.per)
                .position(|keysyms| keysyms.contains(&keysym.raw()))?;
            self.first.checked_add(u8::try_from(place).ok()?)
        })
    }

    /// The key that switches to the third level, if the keymap has one.
    fn third(&self) -> Option<u8> {
        self.key(Keysym::ISO_Level3_Shift)
            .or_else(|| self.key(Keysym::Mode_switch))
    }

    /// Puts `keysym` on the next spare key at every level, which then types it.
    fn lend(&mut self, x: &RustConnection, keysym: Keysym) -> Option<u8> {
        if self.spares.is_empty() {
            return None;
        }
        let spare = self.spares[self.next_spare % self.spares.len()];
        self.next_spare = self.next_spare.wrapping_add(1);
        let keysyms = vec![keysym.raw(); self.per];
        drop(x.change_keyboard_mapping(1, spare, u8::try_from(self.per).ok()?, &keysyms));
        let at = usize::from(spare.checked_sub(self.first)?) * self.per;
        self.keysyms
            .get_mut(at..at + self.per)?
            .copy_from_slice(&keysyms);
        if !self.lent.contains(&spare) {
            self.lent.push(spare);
        }
        Some(spare)
    }

    /// Empties the spares that held a character.
    fn give_back(&mut self, x: &RustConnection) {
        let Ok(per) = u8::try_from(self.per) else {
            return;
        };
        let empty = vec![0; self.per];
        for spare in self.lent.drain(..) {
            drop(x.change_keyboard_mapping(1, spare, per, &empty));
        }
    }
}

/// The thread's side: the connection and what viewers hold.
struct Desked {
    x: RustConnection,
    root: Window,
    /// The window that holds a viewer's text on the clipboard and takes the computer's.
    window: Window,
    atoms: Atoms,
    keyboard: Keyboard,
    events: mpsc::UnboundedSender<Event>,
    hand: Arc<Hand>,
    /// The X key that each viewer key holds down, by DOM code, which lets go of the same key.
    held: HashMap<String, u8>,
    /// The buttons that viewers hold down.
    pressed: Vec<u8>,
    at: Option<(i16, i16)>,
    /// Wheel pixels that did not add up to a notch yet, across and down.
    wheel: (f64, f64),
    /// A viewer's text, while the clipboard holds it.
    put: Option<String>,
    copies: bool,
    /// The computer's clipboard text that comes in parts, while it does.
    incoming: Option<Vec<u8>>,
}

impl Desked {
    fn open(events: mpsc::UnboundedSender<Event>, hand: Arc<Hand>) -> Result<Self, String> {
        let (x, screen) = RustConnection::connect(None).map_err(problem)?;
        let root = x
            .setup()
            .roots
            .get(screen)
            .ok_or("the X server has no such screen")?
            .root;
        let window = x.generate_id().map_err(problem)?;
        x.create_window(
            0,
            window,
            root,
            -1,
            -1,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            0,
            &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )
        .map_err(problem)?;
        let atoms = Atoms::intern(&x)?;
        if x.xfixes_query_version(5, 0)
            .ok()
            .and_then(|asked| asked.reply().ok())
            .is_some()
        {
            drop(x.xfixes_select_selection_input(
                window,
                atoms.clipboard,
                xfixes::SelectionEventMask::SET_SELECTION_OWNER,
            ));
        }
        if x.randr_query_version(1, 5)
            .ok()
            .and_then(|asked| asked.reply().ok())
            .is_some()
        {
            drop(x.randr_select_input(root, randr::NotifyMask::SCREEN_CHANGE));
        }
        drop(x.xtest_get_version(2, 2));
        let keyboard = Keyboard::read(&x)?;
        x.flush().map_err(problem)?;
        Ok(Self {
            x,
            root,
            window,
            atoms,
            keyboard,
            events,
            hand,
            held: HashMap::new(),
            pressed: Vec::new(),
            at: None,
            wheel: (0.0, 0.0),
            put: None,
            copies: false,
            incoming: None,
        })
    }

    /// Serves orders and the X server's events until no more orders can come or the X server
    /// goes away.
    fn run(&mut self, taken: &orders::Receiver<Order>, wake: &OwnedFd) {
        loop {
            let mut busy = false;
            loop {
                match self.x.poll_for_event() {
                    Ok(Some(happened)) => {
                        busy = true;
                        self.happened(&happened);
                    }
                    Ok(None) => break,
                    Err(_) => return,
                }
            }
            loop {
                match taken.try_recv() {
                    Ok(order) => {
                        busy = true;
                        self.order(order);
                    }
                    Err(orders::TryRecvError::Empty) => break,
                    Err(orders::TryRecvError::Disconnected) => return,
                }
            }
            if self.x.flush().is_err() {
                return;
            }
            // An order may have read events while it waited for a reply.
            if busy {
                continue;
            }
            let mut fds = [
                PollFd::new(self.x.stream(), PollFlags::IN),
                PollFd::new(wake, PollFlags::IN),
            ];
            if rustix::event::poll(&mut fds, None).is_err() {
                return;
            }
            let mut woken = [0; 8];
            let _ = rustix::io::read(wake.as_fd(), &mut woken);
        }
    }

    /// Leaves the computer as viewers found it: nothing held down, and no spare key that types.
    fn end(&mut self) {
        self.let_go();
        self.keyboard.give_back(&self.x);
        drop(self.x.flush());
    }

    fn order(&mut self, order: Order) {
        match order {
            Order::Play(input, origin) => self.play(&input, origin),
            Order::LetGo => self.let_go(),
            Order::Put(text, reply) => {
                self.take_clipboard(text);
                let _ = reply.send(());
            }
            Order::Copies(wanted) => {
                self.copies = wanted;
                if !wanted {
                    self.incoming = None;
                }
            }
            Order::Monitors(reply) => drop(reply.send(self.monitors())),
        }
    }

    fn happened(&mut self, happened: &Happened) {
        match happened {
            Happened::MappingNotify(changed)
                if changed.request == Mapping::KEYBOARD || changed.request == Mapping::MODIFIER =>
            {
                self.keyboard.reread(&self.x);
            }
            Happened::RandrScreenChangeNotify(_) => drop(self.events.send(Event::Changed)),
            Happened::XfixesSelectionNotify(changed)
                if changed.selection == self.atoms.clipboard
                    && changed.owner != self.window
                    && changed.owner != NONE
                    && self.copies =>
            {
                drop(self.x.convert_selection(
                    self.window,
                    self.atoms.clipboard,
                    self.atoms.utf8,
                    self.atoms.landing,
                    changed.timestamp,
                ));
            }
            Happened::SelectionNotify(converted)
                if converted.requestor == self.window && converted.property != NONE =>
            {
                self.copied_arrives();
            }
            Happened::PropertyNotify(changed)
                if changed.window == self.window
                    && changed.atom == self.atoms.landing
                    && changed.state == xproto::Property::NEW_VALUE
                    && self.incoming.is_some() =>
            {
                self.copied_part_arrives();
            }
            Happened::SelectionRequest(request) => self.give_clipboard(request),
            Happened::SelectionClear(cleared) if cleared.selection == self.atoms.clipboard => {
                self.put = None;
            }
            _ => {}
        }
    }

    fn monitors(&self) -> Vec<Monitor> {
        let listed = self
            .x
            .randr_get_monitors(self.root, true)
            .ok()
            .and_then(|asked| asked.reply().ok())
            .map_or_else(Vec::new, |reply| reply.monitors);
        let mut monitors: Vec<(bool, Monitor)> = listed
            .into_iter()
            .filter(|monitor| monitor.width > 0 && monitor.height > 0)
            .map(|monitor| {
                (
                    monitor.primary,
                    Monitor {
                        x: monitor.x,
                        y: monitor.y,
                        width: monitor.width,
                        height: monitor.height,
                    },
                )
            })
            .collect();
        monitors.sort_by_key(|(primary, _)| !primary);
        if monitors.is_empty()
            && let Some(geometry) = self
                .x
                .get_geometry(self.root)
                .ok()
                .and_then(|asked| asked.reply().ok())
        {
            monitors.push((
                true,
                Monitor {
                    x: 0,
                    y: 0,
                    width: geometry.width,
                    height: geometry.height,
                },
            ));
        }
        monitors.into_iter().map(|(_, monitor)| monitor).collect()
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
                        self.holding(*modifiers, |desk| desk.button(*button, true));
                    }
                    MouseAction::Release => {
                        self.holding(*modifiers, |desk| desk.button(*button, false));
                    }
                    MouseAction::Wheel => {
                        self.holding(*modifiers, |desk| desk.scroll(*delta_x, *delta_y));
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
            Input::Text { text } => self.type_text(text),
        }
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "the X server's screen counts its pixels in an i16"
    )]
    fn point(&mut self, x: f64, y: f64) {
        let at = (
            x.round().clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16,
            y.round().clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16,
        );
        if self.at == Some(at) {
            return;
        }
        drop(self.x.xtest_fake_input(
            xproto::MOTION_NOTIFY_EVENT,
            0,
            CURRENT_TIME,
            self.root,
            at.0,
            at.1,
            0,
        ));
        self.at = Some(at);
        self.hand.put((f64::from(at.0), f64::from(at.1)));
    }

    fn button(&mut self, button: MouseButton, down: bool) {
        let number = match button {
            MouseButton::Left => 1,
            MouseButton::Middle => 2,
            MouseButton::Right => 3,
            MouseButton::None => return,
        };
        self.click(number, down);
        self.pressed.retain(|pressed| *pressed != number);
        if down {
            self.pressed.push(number);
        }
    }

    fn click(&self, button: u8, down: bool) {
        let kind = if down {
            xproto::BUTTON_PRESS_EVENT
        } else {
            xproto::BUTTON_RELEASE_EVENT
        };
        drop(
            self.x
                .xtest_fake_input(kind, button, CURRENT_TIME, self.root, 0, 0, 0),
        );
    }

    /// DOM wheel deltas grow as the page moves up, which the wheel's down button does in X11.
    fn scroll(&mut self, delta_x: f64, delta_y: f64) {
        self.wheel.0 += delta_x;
        self.wheel.1 += delta_y;
        let notches = [
            (notches(&mut self.wheel.1), WHEEL_UP, WHEEL_DOWN),
            (notches(&mut self.wheel.0), WHEEL_LEFT, WHEEL_RIGHT),
        ];
        for (notches, back, forth) in notches {
            let button = if notches > 0 { forth } else { back };
            for _ in 0..notches.unsigned_abs() {
                self.click(button, true);
                self.click(button, false);
            }
        }
    }

    /// A shortcut goes by the character that its key types on the viewer's keyboard, so that it
    /// means the same with the computer's own layout, and by the key's US character when the
    /// layout has no such character. Other keys type the text that they typed on the viewer's
    /// keyboard.
    fn key(&mut self, down: bool, character: &str, code: &str, text: Option<&str>, modifiers: u8) {
        if !down {
            if let Some(key) = self.held.remove(code) {
                self.press(key, false);
            }
            return;
        }
        let shortcut = modifiers & (CONTROL | META) != 0 && !is_modifier(code);
        if let Some(text) = text.filter(|text| !shortcut && typed(text)) {
            self.type_text(text);
            return;
        }
        let by_letter = shortcut
            .then(|| single(character))
            .flatten()
            .and_then(|letter| {
                self.keyboard
                    .find(Keysym::from_char(letter.to_ascii_lowercase()))
            })
            .map(|(key, _)| key);
        let Some(key) = by_letter.or_else(|| self.keyboard.key(keys::keysym(code)?)) else {
            return;
        };
        if let Some(repeated) = self.held.insert(code.to_owned(), key) {
            self.press(repeated, false);
        }
        if is_modifier(code) {
            self.press(key, true);
        } else {
            self.holding(modifiers, |desk| desk.press(key, true));
        }
    }

    /// Plays `act` with `modifiers` down: presses those that the viewer does not hold down, as
    /// when the key down of the viewer's own modifier never came, and lets go of them after.
    fn holding(&mut self, modifiers: u8, act: impl FnOnce(&mut Self)) {
        let added: Vec<u8> = MODIFIERS
            .into_iter()
            .filter(|(bit, _)| modifiers & bit != 0)
            .filter_map(|(_, keysym)| self.keyboard.key(keysym))
            .filter(|key| !self.holds(*key))
            .collect();
        for key in &added {
            self.press(*key, true);
        }
        act(self);
        for key in added.iter().rev() {
            self.press(*key, false);
        }
    }

    fn holds(&self, key: u8) -> bool {
        self.held.values().any(|held| *held == key)
    }

    fn press(&self, key: u8, down: bool) {
        let kind = if down {
            xproto::KEY_PRESS_EVENT
        } else {
            xproto::KEY_RELEASE_EVENT
        };
        drop(
            self.x
                .xtest_fake_input(kind, key, CURRENT_TIME, self.root, 0, 0, 0),
        );
    }

    /// Types text with the keys of the computer's layout, and a character that the layout lacks
    /// with a spare key that the helper gives it.
    fn type_text(&mut self, text: &str) {
        let mut characters = text.chars().peekable();
        while let Some(character) = characters.next() {
            let keysym = match character {
                // A line that ends in both, as on Windows, takes one Return.
                '\r' if characters.peek() == Some(&'\n') => continue,
                '\n' | '\r' => Keysym::Return,
                '\t' => Keysym::Tab,
                character => Keysym::from_char(character),
            };
            let stroke = self.keyboard.find(keysym).or_else(|| {
                let spare = self.keyboard.lend(&self.x, keysym)?;
                Some((spare, LEVELS[0]))
            });
            if let Some((key, level)) = stroke {
                self.stroke(key, level);
            }
        }
    }

    /// Presses a key at a level, with the modifiers that it needs and with no other ones that the
    /// viewer holds down, then leaves the modifiers as they were.
    fn stroke(&self, key: u8, level: Level) {
        let third = self.keyboard.third();
        let held = |key: &u8| self.holds(*key);
        let shifts: Vec<u8> = [Keysym::Shift_L, Keysym::Shift_R]
            .into_iter()
            .filter_map(|keysym| self.keyboard.key(keysym))
            .collect();
        let mut lifted = Vec::new();
        let mut added = Vec::new();
        if !level.shift {
            lifted.extend(shifts.iter().copied().filter(held));
        } else if !shifts.iter().any(held)
            && let Some(shift) = shifts.first()
        {
            added.push(*shift);
        }
        if let Some(third) = third {
            match (held(&third), level.third) {
                (true, false) => lifted.push(third),
                (false, true) => added.push(third),
                _ => {}
            }
        }
        // Alt would make the character a shortcut.
        lifted.extend(
            [Keysym::Alt_L, Keysym::Alt_R]
                .into_iter()
                .filter_map(|keysym| self.keyboard.key(keysym))
                .filter(|alt| Some(*alt) != third && held(alt)),
        );
        for key in &lifted {
            self.press(*key, false);
        }
        for key in &added {
            self.press(*key, true);
        }
        self.press(key, true);
        self.press(key, false);
        for key in added.iter().rev() {
            self.press(*key, false);
        }
        for key in lifted.iter().rev() {
            self.press(*key, true);
        }
    }

    fn let_go(&mut self) {
        for (_, key) in std::mem::take(&mut self.held) {
            self.press(key, false);
        }
        for button in std::mem::take(&mut self.pressed) {
            self.click(button, false);
        }
        self.wheel = (0.0, 0.0);
    }

    /// Holds a viewer's text on the clipboard, until someone else copies something.
    fn take_clipboard(&mut self, text: String) {
        drop(
            self.x
                .set_selection_owner(self.window, self.atoms.clipboard, CURRENT_TIME),
        );
        let owner = self
            .x
            .get_selection_owner(self.atoms.clipboard)
            .ok()
            .and_then(|asked| asked.reply().ok())
            .map(|reply| reply.owner);
        self.put = (owner == Some(self.window)).then_some(text);
    }

    /// Gives a program that pastes the viewer's text, in the form that it asks for.
    fn give_clipboard(&self, request: &SelectionRequestEvent) {
        let property = if request.property == NONE {
            request.target
        } else {
            request.property
        };
        let given = match &self.put {
            Some(text) if request.selection == self.atoms.clipboard => {
                if request.target == self.atoms.targets {
                    let targets = [
                        self.atoms.targets,
                        self.atoms.utf8,
                        self.atoms.text,
                        AtomEnum::STRING.into(),
                    ];
                    self.x
                        .change_property32(
                            PropMode::REPLACE,
                            request.requestor,
                            property,
                            AtomEnum::ATOM,
                            &targets,
                        )
                        .is_ok()
                } else if [self.atoms.utf8, self.atoms.text, AtomEnum::STRING.into()]
                    .contains(&request.target)
                {
                    self.x
                        .change_property8(
                            PropMode::REPLACE,
                            request.requestor,
                            property,
                            self.atoms.utf8,
                            text.as_bytes(),
                        )
                        .is_ok()
                } else {
                    false
                }
            }
            _ => false,
        };
        let answer = SelectionNotifyEvent {
            response_type: xproto::SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: request.time,
            requestor: request.requestor,
            selection: request.selection,
            target: request.target,
            property: if given { property } else { NONE },
        };
        drop(
            self.x
                .send_event(false, request.requestor, EventMask::NO_EVENT, answer),
        );
    }

    /// The computer's clipboard text came, whole or as the start of parts.
    fn copied_arrives(&mut self) {
        let Some(landed) = self.landed() else {
            return;
        };
        if landed.type_ == self.atoms.incr {
            self.incoming = Some(Vec::new());
        } else {
            self.copied(landed.value);
        }
    }

    fn copied_part_arrives(&mut self) {
        let Some(landed) = self.landed() else {
            self.incoming = None;
            return;
        };
        let Some(mut incoming) = self.incoming.take() else {
            return;
        };
        if landed.value.is_empty() {
            self.copied(incoming);
        } else if incoming.len() + landed.value.len() <= MOST_CLIPBOARD {
            incoming.extend_from_slice(&landed.value);
            self.incoming = Some(incoming);
        }
    }

    /// Takes what landed on the helper's window, which tells the owner of the clipboard to send
    /// the next part.
    fn landed(&self) -> Option<xproto::GetPropertyReply> {
        let longest = u32::try_from(MOST_CLIPBOARD.div_ceil(4)).ok()?;
        self.x
            .get_property(
                true,
                self.window,
                self.atoms.landing,
                AtomEnum::ANY,
                0,
                longest,
            )
            .ok()?
            .reply()
            .ok()
            .filter(|landed| landed.bytes_after == 0)
    }

    fn copied(&self, bytes: Vec<u8>) {
        if bytes.is_empty() || bytes.len() > MOST_CLIPBOARD {
            return;
        }
        if let Ok(text) = String::from_utf8(bytes) {
            drop(self.events.send(Event::Copied(text)));
        }
    }
}

/// Takes the whole notches of the wheel's pixels, and leaves the rest for later.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the notches are truncated to whole ones in the range of an i32"
)]
fn notches(pixels: &mut f64) -> i32 {
    let taken = (*pixels / WHEEL_STEP)
        .trunc()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX));
    *pixels = taken.mul_add(-WHEEL_STEP, *pixels);
    taken as i32
}
