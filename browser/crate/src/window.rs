use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use bytes::Bytes;
use serde_json::json;
use tokio::sync::{Notify, mpsc, oneshot, watch};

use crate::{
    Error,
    driver::{self, Driver},
    engine::Engine,
    flow::{Ask, Flow, HOPELESS},
    input,
    link::{EncodedFrame, EngineTab, FrameKind, MOST_ICON, ToEngine},
    message::{
        Chooser, Dialog, Download, Frame, Input, KeyAction, Menu, MenuContext, MenuItem,
        MouseAction, MouseButton, Pointer, Size, State, Tab, ToViewer, ToWindow, Viewer,
    },
};

const EVENTS: usize = 1024;
const VIEWER_QUEUE: usize = 1024;
const TICK: Duration = Duration::from_millis(100);
const SLEEP_AFTER: Duration = Duration::from_mins(10);
const RETRY_CAPTURE: Duration = Duration::from_secs(1);
/// Max time a pending resize waits for the page to ack the last press, release, or wheel event.
const ANSWER_LIMIT: Duration = Duration::from_secs(1);
/// Lower bound allows landscape phone viewports shorter than 200 pt.
const SIZES: std::ops::RangeInclusive<u32> = 100..=4096;
/// Minimum page width, matching the narrowest window that Chrome allows.
const NARROWEST: u32 = 500;
/// Max encoded frame dimensions (long, short): `VideoToolbox` caps at 4096x2304 and `OpenH264` at
/// 3840x2160.
const MOST_PICTURE: (u16, u16) = if cfg!(target_os = "macos") {
    (4096, 2304)
} else {
    (3840, 2160)
};
const MOST_DOWNLOADS: usize = 16;
const MOST_TEXT: usize = 1024 * 1024;
/// Page-controlled strings are broadcast in every state update and capped to bound message size.
const MOST_LINE: usize = 1024;
const MOST_ADDRESS: usize = 8 * 1024;
const MOST_MESSAGE: usize = 16 * 1024;
const MOST_ITEMS: usize = 1000;
/// Sentinel menu ID for host-handled context menus (distinct from page `<select>` menus).
const CONTEXT_MENU: u32 = u32::MAX;
const COPY_SELECTION: u32 = 0;
const COPY_LINK: u32 = 1;
const BLANK: &str = "about:blank";
/// Monotonic join ID so a closing pipe only removes its own seat, not a reconnected viewer's.
static SEATS: AtomicU64 = AtomicU64::new(1);

pub(crate) const FIRST_SIZE: Size = Size {
    width: 1280,
    height: 720,
    scale: 1.0,
};

pub(crate) enum Event {
    Join {
        viewer: Viewer,
        seat: u64,
        outgoing: mpsc::Sender<Bytes>,
        mailbox: Arc<Mailbox>,
    },
    Message {
        viewer: String,
        message: ToWindow,
    },
    Leave {
        viewer: String,
        seat: u64,
    },
    Driven,
    Close,
    Frame(EncodedFrame),
    /// Standalone keyframe snapshot for layer-hosting views.
    Still(Bytes),
    CaptureFailed,
    /// Core Animation context ID for the window's hosted layer.
    Layer(u32),
    NoLayer,
    /// The page finished handling the input event that was blocking a pending resize.
    Answered,
    Tabs {
        tabs: Vec<EngineTab>,
        active: Option<i64>,
    },
    Shape(String),
    Menu {
        tab: i64,
        frame: i64,
        menu: u32,
        items: Vec<MenuItem>,
    },
    Context {
        selection: String,
        link: Option<String>,
    },
    Copied(String),
    Icon {
        tab: i64,
        png: String,
    },
    Dialog {
        tab: i64,
        dialog: Dialog,
    },
    DialogClosed {
        tab: i64,
    },
    Chooser {
        session: String,
        node: i64,
        several: bool,
    },
    DownloadBegan {
        guid: String,
        name: String,
    },
    DownloadEnded {
        guid: String,
        done: bool,
    },
}

impl Event {
    /// One-shot engine signals that must be queued asynchronously rather than dropped if the
    /// window's event channel is full.
    pub(crate) const fn must_arrive(&self) -> bool {
        matches!(self, Self::CaptureFailed | Self::Layer(_) | Self::NoLayer)
    }
}

#[derive(Clone)]
pub struct Window {
    driver: Arc<Driver>,
    events: mpsc::Sender<Event>,
    state: watch::Receiver<State>,
    open: watch::Receiver<bool>,
    used: watch::Receiver<Option<Instant>>,
    emptied: Arc<AtomicBool>,
    watched: Arc<AtomicBool>,
}

impl Window {
    /// Registers a viewer; the caller must send a `watch` message first on the returned `Pipe`.
    pub async fn watch(&self, viewer: Viewer) -> Result<Pipe, Error> {
        let (outgoing, incoming) = mpsc::channel(VIEWER_QUEUE);
        let mailbox = Arc::new(Mailbox::default());
        let id = viewer.id.clone();
        let seat = SEATS.fetch_add(1, Ordering::Relaxed);
        self.events
            .send(Event::Join {
                viewer,
                seat,
                outgoing,
                mailbox: Arc::clone(&mailbox),
            })
            .await
            .map_err(|_| Error::Stopped)?;
        Ok(Pipe {
            viewer: id,
            seat,
            incoming,
            mailbox,
            events: self.events.clone(),
        })
    }

    pub fn driver(&self) -> &Driver {
        &self.driver
    }

    pub fn state(&self) -> watch::Receiver<State> {
        self.state.clone()
    }

    pub fn is_open(&self) -> bool {
        *self.open.borrow()
    }

    pub async fn closed(&self) {
        let mut open = self.open.clone();
        drop(open.wait_for(|open| !open).await);
    }

    /// Returns `true` if the window closed because its last tab was closed.
    pub fn emptied(&self) -> bool {
        self.emptied.load(Ordering::Acquire)
    }

    pub fn is_watched(&self) -> bool {
        self.watched.load(Ordering::Acquire)
    }

    pub fn used_at(&self) -> Option<Instant> {
        *self.used.borrow()
    }

    /// Marks the window as driven by an agent, keeping it awake and preventing subsequent page
    /// copies from going to a user's clipboard.
    pub async fn driven(&self) {
        drop(self.events.send(Event::Driven).await);
    }

    pub async fn close(&self) {
        drop(self.events.send(Event::Close).await);
    }
}

/// Bidirectional stream for one viewer. Outgoing messages must be delivered reliably and in
/// order: frame dropping is handled here, and downstream drops corrupt subsequent delta frames.
pub struct Pipe {
    viewer: String,
    seat: u64,
    incoming: mpsc::Receiver<Bytes>,
    mailbox: Arc<Mailbox>,
    events: mpsc::Sender<Event>,
}

/// Single-slot coalescing queue for viewer `State` updates. Replacing an unread state preserves
/// one-shot fields (`copied` and pending `icons`) so they are delivered once.
#[derive(Default)]
pub(crate) struct Mailbox {
    state: Mutex<Option<State>>,
    ready: Notify,
}

impl Mailbox {
    fn put(&self, mut state: State) {
        let mut waiting = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(earlier) = waiting.take() {
            if state.copied.is_none() {
                state.copied = earlier.copied;
            }
            for (tab, icon) in earlier.icons {
                state.icons.entry(tab).or_insert(icon);
            }
        }
        *waiting = Some(state);
        drop(waiting);
        self.ready.notify_one();
    }

    fn take(&self) -> Option<State> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }
}

impl Pipe {
    pub async fn send(&self, message: &[u8]) -> Result<(), Error> {
        let message = ToWindow::decode(message).map_err(|_| Error::Malformed)?;
        self.events
            .send(Event::Message {
                viewer: self.viewer.clone(),
                message,
            })
            .await
            .map_err(|_| Error::Stopped)
    }

    /// Returns the next outgoing message, prioritizing the latest state update over queued frames.
    pub async fn receive(&mut self) -> Option<Bytes> {
        loop {
            if let Some(state) = self.mailbox.take() {
                return Some(ToViewer::State(Box::new(state)).encode());
            }
            tokio::select! {
                biased;
                () = self.mailbox.ready.notified() => {}
                picture = self.incoming.recv() => return picture,
            }
        }
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        let viewer = std::mem::take(&mut self.viewer);
        let seat = self.seat;
        let events = self.events.clone();
        if events
            .try_send(Event::Leave {
                viewer: viewer.clone(),
                seat,
            })
            .is_err()
        {
            tokio::spawn(async move {
                drop(events.send(Event::Leave { viewer, seat }).await);
            });
        }
    }
}

pub(crate) fn open(engine: Arc<Engine>, address: &str, folder: PathBuf) -> Result<Window, Error> {
    let driver = driver::Listener::bind()?;
    let (events_in, events) = mpsc::channel(EVENTS);
    let id = engine.open_window(address, FIRST_SIZE, events_in.clone());
    let (state_in, state) = watch::channel(State {
        width: FIRST_SIZE.width,
        height: FIRST_SIZE.height,
        ..State::default()
    });
    let (open_in, open) = watch::channel(true);
    let (used_in, used) = watch::channel(None);
    let driver = Arc::new(driver.serve(Arc::clone(&engine), id, open.clone()));
    let window = Window {
        driver,
        events: events_in,
        state,
        open,
        used,
        emptied: Arc::default(),
        watched: Arc::default(),
    };
    let emptied = Arc::clone(&window.emptied);
    let watched = Arc::clone(&window.watched);
    let stopped = engine.stopped();
    let live = Live::new(
        engine,
        id,
        folder,
        state_in,
        used_in,
        window.events.downgrade(),
    );
    tokio::spawn(async move {
        emptied.store(
            live.run(events, stopped, Arc::clone(&watched)).await,
            Ordering::Release,
        );
        watched.store(false, Ordering::Release);
        open_in.send_replace(false);
    });
    Ok(window)
}

struct Seat {
    viewer: Viewer,
    number: u64,
    outgoing: mpsc::Sender<Bytes>,
    mailbox: Arc<Mailbox>,
    pointer: Option<(f64, f64)>,
    size: Option<Size>,
    /// `None` until `Watch` arrives; state is withheld until then since it depends on layer mode.
    watching: Option<Watching>,
    /// Timestamp of an unacked fallback still for a layer-hosting view; gates further stills and
    /// triggers `HOPELESS` eviction on timeout.
    still_since: Option<Instant>,
}

impl Seat {
    fn hosts_layer(&self) -> bool {
        matches!(self.watching, Some(Watching::Layer { .. }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Watching {
    Video,
    /// Local view hosting the Core Animation layer (`anchors` is saved in case of video fallback).
    Layer {
        anchors: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layer {
    Coming,
    Shows(u32),
    Missing,
}

fn queued(seats: &HashMap<String, Seat>, viewer: &str) -> usize {
    seats.get(viewer).map_or(0, |seat| {
        seat.outgoing.max_capacity() - seat.outgoing.capacity()
    })
}

fn free(seats: &mut HashMap<String, Seat>, viewer: &str, number: u64) -> Option<Seat> {
    let taken = seats.get(viewer).is_some_and(|seat| seat.number == number);
    taken.then(|| seats.remove(viewer)).flatten()
}

struct OpenMenu {
    tab: i64,
    frame: i64,
    menu: u32,
}

struct Waiting {
    session: String,
    node: i64,
}

struct Context {
    selection: String,
    link: Option<String>,
}

/// Viewer currently driving the window (last to act, or first to join). Its viewport sets the
/// page size, and menus, copies, and file pickers route to it.
struct Control {
    viewer: String,
    /// Suppresses resizes while a mouse button is held down.
    held: bool,
}

impl Control {
    fn of(viewer: &str) -> Self {
        Self {
            viewer: viewer.to_owned(),
            held: false,
        }
    }
}

struct Copied {
    viewer: String,
    text: String,
}

/// Tracks whether a user or agent acted last so agent-triggered copies do not reach a clipboard.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Actor {
    Person,
    Agent,
}

struct Live {
    engine: Arc<Engine>,
    id: i64,
    folder: PathBuf,
    flow: Flow,
    seats: HashMap<String, Seat>,
    state: State,
    published: watch::Sender<State>,
    used: watch::Sender<Option<Instant>>,
    dirty: bool,
    size: Size,
    reported: Vec<EngineTab>,
    active: Option<i64>,
    capturing: Option<i64>,
    capture_failed_at: Option<Instant>,
    /// Tab currently rendered into the window's Core Animation layer.
    layering: Option<i64>,
    layer: Layer,
    next_frame: u32,
    used_at: Instant,
    control: Option<Control>,
    actor: Actor,
    /// Pending CDP reply for the last mouse press/release/wheel; resizes wait on this.
    answer: Option<oneshot::Receiver<Result<serde_json::Value, Error>>>,
    answering: bool,
    events: mpsc::WeakSender<Event>,
    /// One-shot clipboard copy cleared after publishing so repeated identical copies still deliver.
    copied: Option<Copied>,
    /// Full favicon cache for new joiners, plus changes pending broadcast in the next publish.
    icons: BTreeMap<String, String>,
    new_icons: BTreeMap<String, String>,
    open_menu: Option<OpenMenu>,
    context: Option<Context>,
    /// Open tab dialogs ordered oldest-first; the newest is shown.
    dialogs: Vec<(i64, Dialog)>,
    chooser: Option<Waiting>,
    downloads: HashMap<String, String>,
    /// Allows one download per user or agent action so pages cannot flood the folder.
    may_download: bool,
    /// Unsolicited download GUIDs cancelled on start and tracked until cleanup in `download_ended`.
    refused: HashSet<String>,
    /// Latest queued CDP commands per method waiting for the initial page session to attach.
    waiting: Vec<(&'static str, serde_json::Value)>,
}

impl Live {
    fn new(
        engine: Arc<Engine>,
        id: i64,
        folder: PathBuf,
        published: watch::Sender<State>,
        used: watch::Sender<Option<Instant>>,
        events: mpsc::WeakSender<Event>,
    ) -> Self {
        let state = published.borrow().clone();
        Self {
            engine,
            id,
            folder,
            flow: Flow::new(),
            seats: HashMap::new(),
            state,
            published,
            used,
            dirty: false,
            size: FIRST_SIZE,
            reported: Vec::new(),
            active: None,
            capturing: None,
            capture_failed_at: None,
            layering: None,
            layer: Layer::Coming,
            next_frame: 0,
            used_at: Instant::now(),
            control: None,
            actor: Actor::Person,
            answer: None,
            answering: false,
            events,
            copied: None,
            icons: BTreeMap::new(),
            new_icons: BTreeMap::new(),
            open_menu: None,
            context: None,
            dialogs: Vec::new(),
            chooser: None,
            downloads: HashMap::new(),
            may_download: true,
            refused: HashSet::new(),
            waiting: Vec::new(),
        }
    }

    async fn run(
        mut self,
        mut events: mpsc::Receiver<Event>,
        mut stopped: watch::Receiver<bool>,
        watched: Arc<AtomicBool>,
    ) -> bool {
        let mut tick = tokio::time::interval(TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut emptied = false;
        loop {
            tokio::select! {
                event = events.recv() => {
                    let Some(event) = event else { break };
                    if matches!(event, Event::Close) {
                        break;
                    }
                    emptied = self.handle(event).await;
                    if emptied {
                        break;
                    }
                }
                _ = tick.tick() => {
                    if self.tick() {
                        break;
                    }
                }
                _ = stopped.changed() => break,
            }
            watched.store(self.watched(), Ordering::Release);
        }
        self.engine.command(ToEngine::Stop { window: self.id });
        self.engine
            .command(ToEngine::CloseWindow { window: self.id });
        self.engine.window_closed(self.id).await;
        emptied
    }

    fn tick(&mut self) -> bool {
        let now = Instant::now();
        if self
            .capture_failed_at
            .is_some_and(|at| now.duration_since(at) >= RETRY_CAPTURE)
        {
            self.capture_failed_at = None;
            self.update_capture();
        }
        let control = self.control.as_ref().map(|control| control.viewer.as_str());
        if let Some(bitrate) = self.flow.steer(control, now) {
            self.engine.command(ToEngine::Bitrate {
                window: self.id,
                bitrate,
            });
        }
        let mut hopeless = self.flow.hopeless(now);
        hopeless.extend(
            self.seats
                .iter()
                .filter(|(_, seat)| {
                    seat.still_since
                        .is_some_and(|since| now.duration_since(since) >= HOPELESS)
                })
                .map(|(viewer, _)| viewer.clone()),
        );
        if !hopeless.is_empty() {
            for viewer in hopeless {
                if let Some(left) = self.seats.remove(&viewer) {
                    self.gone(&left);
                }
            }
            self.update_capture();
            self.dirty = true;
        }
        let seats = &self.seats;
        if self.capturing.is_some()
            && let Some(ask) = self.flow.ask(now, |viewer| queued(seats, viewer))
        {
            self.ask(ask);
        }
        let marks = self.flow.held();
        if !marks.is_empty() {
            self.engine.command(ToEngine::Held {
                window: self.id,
                marks,
            });
        }
        if self.dirty {
            self.publish();
        }
        self.send_waiting();
        !self.watched() && now.duration_since(self.used_at) >= SLEEP_AFTER
    }

    fn watched(&self) -> bool {
        !self.flow.is_empty() || self.seats.values().any(Seat::hosts_layer)
    }

    fn publish(&mut self) {
        self.dirty = false;
        self.state.pointers = self
            .seats
            .values()
            .filter_map(|seat| {
                let (x, y) = seat.pointer?;
                Some(Pointer {
                    viewer: seat.viewer.id.clone(),
                    person: seat.viewer.person.clone(),
                    name: seat.viewer.name.clone(),
                    x,
                    y,
                })
            })
            .collect();
        self.published.send_replace(self.state.clone());
        let icons = std::mem::take(&mut self.new_icons);
        for seat in self.seats.values().filter(|seat| seat.watching.is_some()) {
            let mut state = self.state_for(&seat.viewer, seat.hosts_layer());
            state.icons.clone_from(&icons);
            seat.mailbox.put(state);
        }
        self.copied = None;
    }

    fn welcome(&self, viewer: &str) {
        if let Some(seat) = self.seats.get(viewer) {
            let mut state = self.state_for(&seat.viewer, seat.hosts_layer());
            state.icons.clone_from(&self.icons);
            seat.mailbox.put(state);
        }
    }

    /// Hides pointers belonging to the same user (already visible locally) and scopes `layer`,
    /// `menu`, `copied`, and `chooser` to the eligible viewer.
    fn state_for(&self, viewer: &Viewer, layer: bool) -> State {
        let pressed = self.in_control(&viewer.id);
        let mut state = self.state.clone();
        if let (Layer::Shows(context), true) = (self.layer, layer) {
            state.layer = Some(context);
        }
        state
            .pointers
            .retain(|pointer| pointer.person != viewer.person);
        state.menu.take_if(|menu| menu.viewer != viewer.id);
        state.copied = self
            .copied
            .as_ref()
            .filter(|copied| copied.viewer == viewer.id)
            .map(|copied| copied.text.clone());
        if !pressed {
            state.chooser = None;
        }
        state
    }

    /// Returns `true` when the window should close.
    async fn handle(&mut self, event: Event) -> bool {
        match event {
            Event::Join {
                viewer,
                seat,
                outgoing,
                mailbox,
            } => {
                self.seats.insert(
                    viewer.id.clone(),
                    Seat {
                        viewer,
                        number: seat,
                        outgoing,
                        mailbox,
                        pointer: None,
                        size: None,
                        watching: None,
                        still_since: None,
                    },
                );
            }
            Event::Message { viewer, message } => self.message(&viewer, message),
            Event::Leave { viewer, seat } => {
                if let Some(left) = free(&mut self.seats, &viewer, seat) {
                    self.flow.leave(&viewer);
                    self.update_capture();
                    self.gone(&left);
                    self.dirty = true;
                }
            }
            Event::Answered => {
                self.answering = false;
                self.follow();
            }
            Event::Driven => self.driven(),
            Event::Close => {}
            Event::Frame(frame) => self.frame(frame),
            Event::Still(data) => self.still(data),
            Event::CaptureFailed => {
                self.capturing = None;
                self.capture_failed_at = Some(Instant::now());
            }
            Event::Layer(context) => {
                // Ignore a queued `Layer` event that arrives after `NoLayer` disabled layer mode.
                if self.layer != Layer::Missing {
                    self.layer = Layer::Shows(context);
                    self.publish();
                }
            }
            Event::NoLayer => self.take_video(),
            Event::Tabs { tabs, active } => return self.tabs(tabs, active),
            // The user's own pointer takes this shape, so publish it now rather than at the tick.
            Event::Shape(shape) => {
                self.state.shape = shape;
                self.publish();
            }
            Event::Menu {
                tab,
                frame,
                menu,
                items,
            } => self.open_list(tab, frame, menu, items),
            Event::Context { selection, link } => self.context_menu(&selection, link),
            Event::Copied(text) => self.copy(&text),
            Event::Icon { tab, png } => self.icon(tab, png),
            Event::Dialog { tab, mut dialog } => {
                dialog.message = cut(&dialog.message, MOST_MESSAGE);
                dialog.text = cut(&dialog.text, MOST_MESSAGE);
                self.dialogs.retain(|(other, _)| *other != tab);
                self.dialogs.push((tab, dialog));
                self.show_dialog();
            }
            Event::DialogClosed { tab } => {
                self.dialogs.retain(|(other, _)| *other != tab);
                self.show_dialog();
            }
            Event::Chooser {
                session,
                node,
                several,
            } => {
                self.chooser = Some(Waiting { session, node });
                self.state.chooser = Some(Chooser { several });
                self.dirty = true;
            }
            Event::DownloadBegan { guid, name } => self.download_began(guid, name),
            Event::DownloadEnded { guid, done } => self.download_ended(&guid, done).await,
        }
        false
    }

    fn download_began(&mut self, guid: String, name: String) {
        if !std::mem::take(&mut self.may_download) {
            self.engine
                .devtools()
                .send("Browser.cancelDownload", &json!({ "guid": guid }), None);
            self.refused.insert(guid);
            return;
        }
        self.state.downloads.push(Download {
            name: name.clone(),
            path: String::new(),
            done: false,
        });
        let extra = self.state.downloads.len().saturating_sub(MOST_DOWNLOADS);
        self.state.downloads.drain(..extra);
        self.downloads.insert(guid, name);
        self.dirty = true;
    }

    fn message(&mut self, viewer: &str, message: ToWindow) {
        let now = Instant::now();
        match message {
            ToWindow::Watch {
                size,
                anchors,
                opened,
                layer,
            } => {
                let Some(seat) = self.seats.get_mut(viewer) else {
                    return;
                };
                let local = seat.viewer.local;
                let watching = if layer && local && self.layer != Layer::Missing {
                    Watching::Layer { anchors }
                } else {
                    Watching::Video
                };
                seat.watching = Some(watching);
                if watching == Watching::Video {
                    self.flow.seat(viewer, anchors, local, now);
                } else {
                    self.flow.leave(viewer);
                }
                self.used_at = now;
                if opened {
                    self.opened(viewer);
                }
                self.control.get_or_insert_with(|| Control::of(viewer));
                self.keep_size(viewer, size);
                self.update_capture();
                if watching == Watching::Video {
                    self.flow.keyframe_asked(now);
                    self.ask(Ask::Keyframe);
                } else {
                    self.engine.command(ToEngine::Still { window: self.id });
                }
                self.welcome(viewer);
            }
            ToWindow::Ack { frame } => {
                if let Some(seat) = self.seats.get_mut(viewer).filter(|seat| seat.hosts_layer()) {
                    seat.still_since = None;
                } else {
                    self.flow.ack(viewer, frame, now);
                }
            }
            ToWindow::Keyframe if self.seats.get(viewer).is_some_and(Seat::hosts_layer) => {
                self.engine.command(ToEngine::Still { window: self.id });
            }
            ToWindow::Keyframe => {
                let seats = &self.seats;
                if let Some(ask) = self.flow.reset(viewer, now, |viewer| queued(seats, viewer)) {
                    self.ask(ask);
                }
            }
            ToWindow::Size { size } => self.keep_size(viewer, size),
            ToWindow::Input { input } => self.input(viewer, input),
        }
    }

    fn ask(&self, ask: Ask) {
        let window = self.id;
        self.engine.command(match ask {
            Ask::Keyframe => ToEngine::Keyframe { window },
            Ask::Recover => ToEngine::Recover { window },
        });
    }

    fn frame(&mut self, frame: EncodedFrame) {
        let now = Instant::now();
        self.next_frame = self.next_frame.wrapping_add(1);
        let id = self.next_frame;
        let seats = &self.seats;
        let delivery = self.flow.frame(
            id,
            frame.kind,
            frame.mark,
            frame.data.len(),
            now,
            |viewer| queued(seats, viewer),
        );
        if !delivery.to.is_empty() {
            let message = ToViewer::Frame(Frame {
                id,
                kind: frame.kind,
                data: frame.data,
            })
            .encode();
            for viewer in &delivery.to {
                let sent = self
                    .seats
                    .get(viewer)
                    .is_some_and(|seat| seat.outgoing.try_send(message.clone()).is_ok());
                if !sent {
                    self.flow.lost(viewer, id, now);
                }
            }
        }
        if let Some(ask) = delivery.ask {
            self.ask(ask);
        }
    }

    /// Reuses `next_frame` without incrementing it so video viewers do not see a frame ID gap.
    fn still(&mut self, data: Bytes) {
        let now = Instant::now();
        let message = ToViewer::Frame(Frame {
            id: self.next_frame,
            kind: FrameKind::Key,
            data,
        })
        .encode();
        for seat in self
            .seats
            .values_mut()
            .filter(|seat| seat.hosts_layer() && seat.still_since.is_none())
        {
            if seat.outgoing.try_send(message.clone()).is_ok() {
                seat.still_since = Some(now);
            }
        }
    }

    /// Returns `true` if the last tab closed.
    fn tabs(&mut self, tabs: Vec<EngineTab>, active: Option<i64>) -> bool {
        let emptied = !self.state.tabs.is_empty() && tabs.is_empty();
        self.active = active;
        self.state.tabs = tabs
            .iter()
            .map(|tab| Tab {
                id: tab.id.to_string(),
                title: cut(&tab.title, MOST_LINE),
                address: cut(&tab.address, MOST_ADDRESS),
                active: Some(tab.id) == self.active,
                loading: tab.loading,
            })
            .collect();
        self.reported = tabs;
        let open: HashSet<&str> = self.state.tabs.iter().map(|tab| tab.id.as_str()).collect();
        self.icons.retain(|tab, _| open.contains(tab.as_str()));
        self.new_icons.retain(|tab, _| open.contains(tab.as_str()));
        self.dirty = true;
        self.send_waiting();
        self.update_capture();
        emptied
    }

    fn icon(&mut self, tab: i64, png: String) {
        let tab = tab.to_string();
        let png = if png.len() <= MOST_ICON {
            png
        } else {
            String::new()
        };
        if self
            .icons
            .get(&tab)
            .map_or(png.is_empty(), |icon| *icon == png)
        {
            return;
        }
        if png.is_empty() {
            self.icons.remove(&tab);
        } else {
            self.icons.insert(tab.clone(), png.clone());
        }
        self.new_icons.insert(tab, png);
        self.dirty = true;
    }

    fn update_capture(&mut self) {
        let video = self.active.filter(|_| !self.flow.is_empty());
        if video != self.capturing {
            self.capturing = video;
            match video {
                Some(tab) => self.capture(tab),
                None => self.engine.command(ToEngine::Stop { window: self.id }),
            }
        }
        let layer = self
            .active
            .filter(|_| self.seats.values().any(Seat::hosts_layer));
        if layer != self.layering {
            self.layering = layer;
            self.engine.command(ToEngine::Layer {
                window: self.id,
                tab: layer,
            });
        }
    }

    fn take_video(&mut self) {
        let now = Instant::now();
        self.layer = Layer::Missing;
        let mut moved = false;
        for (viewer, seat) in &mut self.seats {
            if let Some(Watching::Layer { anchors }) = seat.watching {
                seat.watching = Some(Watching::Video);
                seat.still_since = None;
                self.flow.seat(viewer, anchors, seat.viewer.local, now);
                moved = true;
            }
        }
        self.update_capture();
        if moved {
            self.flow.keyframe_asked(now);
            self.ask(Ask::Keyframe);
            self.publish();
        }
    }

    fn capture(&self, tab: i64) {
        self.engine.command(ToEngine::Capture {
            window: self.id,
            tab,
            bitrate: self.flow.bitrate(),
        });
    }

    /// Stores each viewer's size so passive viewers never change the page dimensions, only the
    /// render scale factor when sharper.
    fn keep_size(&mut self, viewer: &str, size: Size) {
        if let Some(seat) = self.seats.get_mut(viewer) {
            seat.size = Some(size);
        }
        self.follow();
    }

    fn driven(&mut self) {
        self.used_at = Instant::now();
        self.may_download = true;
        self.actor = Actor::Agent;
    }

    fn acted(&mut self, viewer: &str) {
        self.actor = Actor::Person;
        if !self.in_control(viewer) {
            self.control = Some(Control::of(viewer));
        }
    }

    /// Hands control to a newly opened view from the same user; another user's view only takes
    /// control when it interacts, avoiding unexpected resizes.
    fn opened(&mut self, viewer: &str) {
        let person = |view: &str| self.seats.get(view).map(|seat| &seat.viewer.person);
        let own = self.control.as_ref().is_some_and(|control| {
            !control.held
                && control.viewer != viewer
                && person(viewer).is_some()
                && person(&control.viewer) == person(viewer)
        });
        if own {
            self.control = Some(Control::of(viewer));
        }
    }

    fn in_control(&self, viewer: &str) -> bool {
        self.control
            .as_ref()
            .is_some_and(|control| control.viewer == viewer)
    }

    /// Transfers control to another view of the same user if one remains, otherwise to the
    /// earliest remaining viewer, then recalculates page size and scale.
    fn gone(&mut self, left: &Seat) {
        if self.in_control(&left.viewer.id) {
            self.control = self
                .seats
                .values()
                .min_by_key(|seat| (seat.viewer.person != left.viewer.person, seat.number))
                .map(|seat| Control::of(&seat.viewer.id));
        }
        self.follow();
    }

    /// Resizes the page to match the controlling viewer once no mouse button is held and the last
    /// press/release/wheel has been processed. Input travels over CDP while resizes use the engine
    /// link, so waiting prevents clicks from landing on a newly reflowed layout.
    fn follow(&mut self) {
        if self.answering {
            return;
        }
        let Some(size) = self
            .control
            .as_ref()
            .filter(|control| !control.held)
            .and_then(|control| self.seats.get(&control.viewer))
            .and_then(|seat| seat.size)
            .and_then(|size| fitted(size, self.seats.values().filter_map(|seat| seat.size)))
        else {
            return;
        };
        if size == self.size {
            return;
        }
        if let Some(answer) = self.answer.take() {
            self.answering = true;
            let events = self.events.clone();
            tokio::spawn(async move {
                drop(tokio::time::timeout(ANSWER_LIMIT, answer).await);
                if let Some(events) = events.upgrade() {
                    drop(events.send(Event::Answered).await);
                }
            });
            return;
        }
        self.resize(size);
    }

    /// Sends the new size to the engine and publishes immediately because viewers use the window
    /// dimensions to map pointer coordinates.
    fn resize(&mut self, size: Size) {
        self.size = size;
        self.state.width = size.width;
        self.state.height = size.height;
        self.engine.command(ToEngine::Resize {
            window: self.id,
            size,
        });
        self.publish();
    }

    fn active_session(&self) -> Option<String> {
        let tab = self.active?;
        let target = self
            .reported
            .iter()
            .find(|known| known.id == tab)?
            .target
            .as_ref()?;
        self.engine.session_of(target)
    }

    fn page(&self, method: &str, params: &serde_json::Value) {
        if let Some(session) = self.active_session() {
            self.engine.devtools().send(method, params, Some(&session));
        } else {
            tracing::debug!(method, "the window has no page for an input");
        }
    }

    /// Sends a CDP command immediately, or queues the latest call per method until the new
    /// window's initial page attaches.
    fn page_soon(&mut self, method: &'static str, params: serde_json::Value) {
        if let Some(session) = self.active_session() {
            self.engine.devtools().send(method, &params, Some(&session));
            return;
        }
        keep_latest(&mut self.waiting, method, params);
    }

    fn send_waiting(&mut self) {
        if self.waiting.is_empty() {
            return;
        }
        let Some(session) = self.active_session() else {
            return;
        };
        for (method, params) in std::mem::take(&mut self.waiting) {
            self.engine.devtools().send(method, &params, Some(&session));
        }
    }

    fn mouse(&mut self, viewer: &str, mouse: input::Mouse, mac: bool) {
        if let Some(seat) = self.seats.get_mut(viewer) {
            seat.pointer = Some((mouse.x, mouse.y));
            self.dirty = true;
        }
        let event = input::mouse(mouse, mac);
        if mouse.action == MouseAction::Move {
            self.page("Input.dispatchMouseEvent", &event);
        } else if let Some(answer) = self.active_session().and_then(|session| {
            self.engine
                .devtools()
                .ask("Input.dispatchMouseEvent", &event, Some(&session))
        }) {
            self.answer = Some(answer);
        }
        if let Some(control) = self
            .control
            .as_mut()
            .filter(|control| control.viewer == viewer)
        {
            match mouse.action {
                MouseAction::Press => control.held = true,
                MouseAction::Release => control.held = false,
                MouseAction::Move | MouseAction::Wheel => {}
            }
        }
    }

    fn input(&mut self, viewer: &str, event: Input) {
        let mac = cfg!(target_os = "macos");
        let plain_move = matches!(
            event,
            Input::Mouse {
                action: MouseAction::Move,
                button: MouseButton::None,
                ..
            }
        );
        if !plain_move {
            self.used_at = Instant::now();
            self.used.send_replace(Some(self.used_at));
            self.may_download = true;
        }
        let acts = !matches!(
            event,
            Input::Mouse {
                action: MouseAction::Move | MouseAction::Release,
                ..
            } | Input::Key {
                action: KeyAction::Up,
                ..
            }
        );
        if acts {
            self.acted(viewer);
        }
        match event {
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
                let mouse = input::Mouse {
                    action,
                    x,
                    y,
                    button,
                    clicks,
                    delta_x,
                    delta_y,
                    modifiers,
                };
                self.mouse(viewer, mouse, mac);
            }
            Input::Key {
                action,
                key,
                code,
                text,
                key_code,
                modifiers,
            } => self.page(
                "Input.dispatchKeyEvent",
                &input::key(
                    input::Key {
                        action,
                        key: &key,
                        code: &code,
                        text: text.as_deref(),
                        key_code,
                        modifiers,
                    },
                    mac,
                ),
            ),
            Input::Text { text } => self.page("Input.insertText", &json!({ "text": text })),
            Input::Open { address } => {
                self.page_soon("Page.navigate", json!({ "url": input::address(&address) }));
            }
            Input::Back => self.move_in_history(-1),
            Input::Forward => self.move_in_history(1),
            Input::Reload => self.page_soon("Page.reload", json!({})),
            Input::Tab { id } => {
                if let Ok(tab) = id.parse() {
                    self.engine.command(ToEngine::Activate { tab });
                }
            }
            Input::NewTab => self.engine.command(ToEngine::NewTab {
                window: self.id,
                address: BLANK.to_owned(),
                ask: None,
            }),
            Input::CloseTab { id } => {
                if let Ok(tab) = id.parse() {
                    self.engine.command(ToEngine::CloseTab { tab });
                }
            }
            Input::Choose { menu, choice } => self.choose(viewer, menu, choice),
            Input::Dialog { accept, text } => self.answer_dialog(accept, text),
            Input::Files { paths } => self.give_files(&paths),
        }
        self.follow();
    }

    fn move_in_history(&self, step: i64) {
        let Some(session) = self.active_session() else {
            return;
        };
        let devtools = self.engine.devtools().clone();
        tokio::spawn(async move {
            let Ok(history) = devtools
                .call("Page.getNavigationHistory", json!({}), Some(&session))
                .await
            else {
                return;
            };
            let entry = history["currentIndex"]
                .as_i64()
                .and_then(|index| usize::try_from(index + step).ok())
                .and_then(|index| history["entries"].get(index))
                .and_then(|entry| entry["id"].as_i64());
            if let Some(entry) = entry {
                devtools.send(
                    "Page.navigateToHistoryEntry",
                    &json!({ "entryId": entry }),
                    Some(&session),
                );
            }
        });
    }

    fn open_list(&mut self, tab: i64, frame: i64, menu: u32, items: Vec<MenuItem>) {
        let Some(viewer) = self.control.as_ref().map(|control| control.viewer.clone()) else {
            return;
        };
        self.open_menu = Some(OpenMenu { tab, frame, menu });
        let items = items
            .into_iter()
            .take(MOST_ITEMS)
            .map(|item| MenuItem {
                label: cut(&item.label, MOST_LINE),
                ..item
            })
            .collect();
        self.state.menu = Some(self.menu_of(viewer, menu, items, None));
        self.dirty = true;
    }

    fn context_menu(&mut self, selection: &str, link: Option<String>) {
        let Some(viewer) = self.control.as_ref().map(|control| control.viewer.clone()) else {
            return;
        };
        let context = MenuContext {
            selection: cut(selection, MOST_LINE),
            link: link.as_deref().map(|link| cut(link, MOST_ADDRESS)),
        };
        self.open_menu = None;
        self.context = Some(Context {
            selection: cut(selection, MOST_TEXT),
            link,
        });
        self.state.menu = Some(self.menu_of(viewer, CONTEXT_MENU, Vec::new(), Some(context)));
        self.dirty = true;
    }

    fn menu_of(
        &self,
        viewer: String,
        id: u32,
        items: Vec<MenuItem>,
        context: Option<MenuContext>,
    ) -> Menu {
        let (x, y) = self
            .seats
            .get(&viewer)
            .and_then(|seat| seat.pointer)
            .unwrap_or_else(|| {
                (
                    f64::from(self.state.width) / 2.0,
                    f64::from(self.state.height) / 2.0,
                )
            });
        Menu {
            id,
            viewer,
            x,
            y,
            items,
            context,
        }
    }

    fn copy(&mut self, text: &str) {
        if self.actor == Actor::Agent {
            return;
        }
        if let Some(viewer) = self.control.as_ref().map(|control| control.viewer.clone()) {
            self.copied = Some(Copied {
                viewer,
                text: cut(text, MOST_TEXT),
            });
            self.dirty = true;
        }
    }

    fn choose(&mut self, viewer: &str, menu: u32, choice: Option<u32>) {
        if self
            .state
            .menu
            .take_if(|open| open.id == menu && open.viewer == viewer)
            .is_none()
        {
            return;
        }
        self.dirty = true;
        if menu == CONTEXT_MENU {
            let copy = self.context.take().and_then(|context| match choice {
                Some(COPY_SELECTION) => Some(context.selection).filter(|words| !words.is_empty()),
                Some(COPY_LINK) => context.link,
                _ => None,
            });
            if let Some(text) = copy {
                self.copied = Some(Copied {
                    viewer: viewer.to_owned(),
                    text,
                });
            }
            return;
        }
        if let Some(open) = self.open_menu.take().filter(|open| open.menu == menu) {
            self.engine.command(ToEngine::Pick {
                tab: open.tab,
                frame: open.frame,
                menu,
                choice,
            });
        }
    }

    fn answer_dialog(&mut self, accept: bool, text: Option<String>) {
        if let Some((tab, _)) = self.dialogs.pop() {
            self.engine.command(ToEngine::Dialog { tab, accept, text });
        }
        self.show_dialog();
    }

    fn show_dialog(&mut self) {
        let shown = self.dialogs.last().map(|(_, dialog)| dialog.clone());
        if shown != self.state.dialog {
            self.state.dialog = shown;
            self.dirty = true;
        }
    }

    fn give_files(&mut self, paths: &[String]) {
        if let Some(waiting) = self.chooser.take()
            && !paths.is_empty()
        {
            self.engine.devtools().send(
                "DOM.setFileInputFiles",
                &json!({ "files": paths, "backendNodeId": waiting.node }),
                Some(&waiting.session),
            );
        }
        self.state.chooser = None;
        self.dirty = true;
    }

    async fn download_ended(&mut self, guid: &str, done: bool) {
        if self.refused.remove(guid) {
            drop(tokio::fs::remove_file(self.engine.download_path(guid)).await);
            return;
        }
        let Some(name) = self.downloads.remove(guid) else {
            return;
        };
        let source = self.engine.download_path(guid);
        let path = if done {
            match deliver(&source, &self.folder, &name).await {
                Ok(path) => Some(path),
                Err(problem) => {
                    tracing::warn!(%problem, "the download stays out of the folder");
                    None
                }
            }
        } else {
            drop(tokio::fs::remove_file(&source).await);
            None
        };
        let entry = self
            .state
            .downloads
            .iter_mut()
            .rev()
            .find(|download| download.name == name && !download.done);
        match (entry, path) {
            (Some(entry), Some(path)) => {
                entry.path = path.display().to_string();
                entry.done = true;
            }
            (Some(_), None) => self
                .state
                .downloads
                .retain(|download| download.name != name || download.done),
            (None, _) => {}
        }
        self.dirty = true;
    }
}

fn keep_latest(
    waiting: &mut Vec<(&'static str, serde_json::Value)>,
    method: &'static str,
    params: serde_json::Value,
) {
    waiting.retain(|(kind, _)| *kind != method);
    waiting.push((method, params));
}

/// Computes engine viewport dimensions and scale for the controlling `size` and all `views`.
/// Views narrower than `NARROWEST` expand proportionally at a lower scale to preserve aspect
/// ratio and pixel count, then scale up to match the sharpest viewer.
fn fitted(size: Size, views: impl IntoIterator<Item = Size>) -> Option<Size> {
    if !usable(size) {
        return None;
    }
    let width = size.width.max(NARROWEST);
    let height = (u64::from(size.height) * u64::from(width) + u64::from(size.width) / 2)
        / u64::from(size.width);
    let height = u32::try_from(height).map_or(*SIZES.end(), |height| height.min(*SIZES.end()));
    let scale = size.scale * f32::from(u16::try_from(size.width).ok()?)
        / f32::from(u16::try_from(width).ok()?);
    let scale = views
        .into_iter()
        .filter_map(|view| shown(view, width, height))
        .fold(scale.clamp(1.0, 4.0), f32::max)
        .min(4.0);
    encodable(
        Size {
            width,
            height,
            scale,
        },
        MOST_PICTURE,
    )
}

fn usable(size: Size) -> bool {
    SIZES.contains(&size.width) && SIZES.contains(&size.height) && size.scale > 0.0
}

/// Effective device pixels per page CSS pixel when aspect-fitting `width` x `height` into `view`.
fn shown(view: Size, width: u32, height: u32) -> Option<f32> {
    if !usable(view) {
        return None;
    }
    let side = |points: u32| u16::try_from(points).ok().map(f32::from);
    let fit = f32::min(
        side(view.width)? / side(width)?,
        side(view.height)? / side(height)?,
    );
    Some(fit * view.scale)
}

/// Clamps scale (and, if needed, CSS dimensions at scale 1.0) so the encoded frame fits within
/// the encoder's limits. Uses `ceil` because Chromium rounds fractional pixel dimensions up.
fn encodable(size: Size, (most_long, most_short): (u16, u16)) -> Option<Size> {
    let tall = size.height > size.width;
    let (long, short) = if tall {
        (size.height, size.width)
    } else {
        (size.width, size.height)
    };
    let (longest, shortest) = (u32::from(most_long), u32::from(most_short));
    if long > longest || short > shortest {
        let (long, short) = if long * shortest >= short * longest {
            (longest, short * longest / long)
        } else {
            (long * shortest / short, shortest)
        };
        let (width, height) = if tall { (short, long) } else { (long, short) };
        return Some(Size {
            width: width.max(NARROWEST),
            height,
            scale: 1.0,
        });
    }
    let (long, short) = (
        f32::from(u16::try_from(long).ok()?),
        f32::from(u16::try_from(short).ok()?),
    );
    let (longest, shortest) = (f32::from(most_long), f32::from(most_short));
    let fits = |scale: f32| (long * scale).ceil() <= longest && (short * scale).ceil() <= shortest;
    let mut scale = size.scale.min(longest / long).min(shortest / short);
    while !fits(scale) {
        scale = scale.next_down();
    }
    Some(Size { scale, ..size })
}

fn cut(text: &str, most: usize) -> String {
    text[..text.floor_char_boundary(most)].to_owned()
}

/// Strips directory components from the page-supplied `name` before moving the download into
/// `folder` under a unique filename.
async fn deliver(source: &Path, folder: &Path, name: &str) -> Result<PathBuf, Error> {
    let name = Path::new(name)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("download");
    let target = free_name(folder, name).await;
    tokio::fs::create_dir_all(folder).await?;
    if tokio::fs::rename(source, &target).await.is_err() {
        tokio::fs::copy(source, &target).await?;
        tokio::fs::remove_file(source).await?;
    }
    Ok(target)
}

async fn free_name(folder: &Path, name: &str) -> PathBuf {
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(name);
    let extension = path.extension().and_then(|extension| extension.to_str());
    let mut candidate = folder.join(name);
    let mut number = 1;
    while tokio::fs::try_exists(&candidate).await.unwrap_or(false) {
        number += 1;
        candidate = folder.join(extension.map_or_else(
            || format!("{stem} {number}"),
            |extension| format!("{stem} {number}.{extension}"),
        ));
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seat(number: u64) -> Seat {
        Seat {
            viewer: Viewer {
                id: "app".to_owned(),
                person: "ana".to_owned(),
                name: "Ana".to_owned(),
                local: false,
            },
            number,
            outgoing: mpsc::channel(1).0,
            mailbox: Arc::default(),
            pointer: None,
            size: None,
            watching: None,
            still_since: None,
        }
    }

    fn live() -> Live {
        Live::new(
            Engine::idle(),
            1,
            PathBuf::new(),
            watch::channel(State::default()).0,
            watch::channel(None).0,
            mpsc::channel(1).0.downgrade(),
        )
    }

    async fn join(live: &mut Live, id: &str) -> Pipe {
        join_of(live, id, id).await
    }

    async fn join_of(live: &mut Live, id: &str, person: &str) -> Pipe {
        join_from(live, id, person, false).await
    }

    async fn watching(live: &mut Live, id: &str) -> Pipe {
        watching_of(live, id, id).await
    }

    async fn watching_of(live: &mut Live, id: &str, person: &str) -> Pipe {
        let pipe = join_of(live, id, person).await;
        rejoin(live, id, view(800, 600)).await;
        pipe
    }

    async fn join_from(live: &mut Live, id: &str, person: &str, local: bool) -> Pipe {
        let (outgoing, incoming) = mpsc::channel(VIEWER_QUEUE);
        let mailbox = Arc::new(Mailbox::default());
        let viewer = Viewer {
            id: id.to_owned(),
            person: person.to_owned(),
            name: person.to_owned(),
            local,
        };
        let seat = SEATS.fetch_add(1, Ordering::Relaxed);
        live.handle(Event::Join {
            viewer,
            seat,
            outgoing,
            mailbox: Arc::clone(&mailbox),
        })
        .await;
        Pipe {
            viewer: id.to_owned(),
            seat,
            incoming,
            mailbox,
            events: mpsc::channel(1).0,
        }
    }

    fn newest(pipe: &Pipe) -> Option<State> {
        pipe.mailbox.take()
    }

    fn copied(pipe: &Pipe) -> Option<String> {
        newest(pipe).and_then(|state| state.copied)
    }

    async fn say(live: &mut Live, viewer: &str, input: serde_json::Value) {
        let message = serde_json::from_value(json!({ "type": "input", "input": input })).unwrap();
        live.handle(Event::Message {
            viewer: viewer.to_owned(),
            message,
        })
        .await;
    }

    async fn press(live: &mut Live, viewer: &str) {
        let press = json!({ "kind": "mouse", "action": "press", "x": 1, "y": 1, "button": "left", "clicks": 1 });
        say(live, viewer, press).await;
    }

    async fn release(live: &mut Live, viewer: &str) {
        let release = json!({ "kind": "mouse", "action": "release", "x": 1, "y": 1, "button": "left", "clicks": 1 });
        say(live, viewer, release).await;
    }

    fn view(width: u32, height: u32) -> Size {
        Size {
            width,
            height,
            scale: 2.0,
        }
    }

    async fn look(live: &mut Live, viewer: &str, size: Size) {
        watch(live, viewer, size, true).await;
    }

    async fn rejoin(live: &mut Live, viewer: &str, size: Size) {
        watch(live, viewer, size, false).await;
    }

    async fn watch(live: &mut Live, viewer: &str, size: Size, opened: bool) {
        watch_hosting(live, viewer, size, opened, false).await;
    }

    async fn watch_hosting(live: &mut Live, viewer: &str, size: Size, opened: bool, layer: bool) {
        live.handle(Event::Message {
            viewer: viewer.to_owned(),
            message: ToWindow::Watch {
                size,
                anchors: false,
                opened,
                layer,
            },
        })
        .await;
    }

    async fn reshape(live: &mut Live, viewer: &str, size: Size) {
        live.handle(Event::Message {
            viewer: viewer.to_owned(),
            message: ToWindow::Size { size },
        })
        .await;
    }

    #[tokio::test]
    async fn a_press_of_another_viewer_reaches_the_page_it_saw_and_the_page_takes_its_view_after_the_release()
     {
        let mut live = live();
        let _ana = join(&mut live, "ana").await;
        look(&mut live, "ana", view(900, 700)).await;
        let _bob = join(&mut live, "bob").await;
        look(&mut live, "bob", view(600, 1000)).await;
        press(&mut live, "bob").await;
        assert_eq!(live.size, view(900, 700));
        assert!(live.in_control("bob"));
        release(&mut live, "bob").await;
        assert_eq!(live.size, view(600, 1000));
    }

    #[tokio::test]
    async fn each_copy_reaches_the_viewer_who_pressed_once_also_the_same_words_again() {
        let mut live = live();
        let ana = watching(&mut live, "ana").await;
        let bob = watching(&mut live, "bob").await;
        press(&mut live, "ana").await;
        live.handle(Event::Copied("word".to_owned())).await;
        live.publish();
        assert_eq!(copied(&ana).as_deref(), Some("word"));
        assert_eq!(copied(&bob), None);

        live.handle(Event::Shape("text".to_owned())).await;
        assert_eq!(copied(&ana), None);

        live.handle(Event::Copied("word".to_owned())).await;
        live.publish();
        assert_eq!(copied(&ana).as_deref(), Some("word"));

        let again = watching(&mut live, "ana").await;
        assert_eq!(copied(&again), None);
    }

    #[tokio::test]
    async fn a_copy_goes_to_the_viewer_whose_key_asked_for_it() {
        let mut live = live();
        let ana = watching(&mut live, "ana").await;
        let bob = watching(&mut live, "bob").await;
        press(&mut live, "ana").await;
        press(&mut live, "bob").await;
        let copy = json!({ "kind": "key", "action": "down", "key": "c", "code": "KeyC", "keyCode": 67, "modifiers": 4 });
        say(&mut live, "ana", copy).await;
        live.handle(Event::Copied("word".to_owned())).await;
        live.publish();
        assert_eq!(copied(&ana).as_deref(), Some("word"));
        assert_eq!(copied(&bob), None);
    }

    #[tokio::test]
    async fn a_copy_after_an_agents_command_is_no_persons_until_a_person_acts() {
        let mut live = live();
        let ana = watching(&mut live, "ana").await;
        press(&mut live, "ana").await;
        live.handle(Event::Driven).await;
        live.handle(Event::Copied("word".to_owned())).await;
        live.publish();
        assert_eq!(copied(&ana), None);

        press(&mut live, "ana").await;
        live.handle(Event::Copied("word".to_owned())).await;
        live.publish();
        assert_eq!(copied(&ana).as_deref(), Some("word"));
    }

    #[tokio::test]
    async fn a_copy_that_waits_for_its_viewer_goes_once_with_the_newest_state() {
        let mut live = live();
        let ana = watching(&mut live, "ana").await;
        press(&mut live, "ana").await;
        live.handle(Event::Copied("word".to_owned())).await;
        live.publish();
        live.handle(Event::Shape("text".to_owned())).await;
        let state = newest(&ana).unwrap();
        assert_eq!(
            (state.copied.as_deref(), state.shape.as_str()),
            (Some("word"), "text")
        );
        live.handle(Event::Shape("pointer".to_owned())).await;
        assert_eq!(copied(&ana), None);
    }

    #[tokio::test(start_paused = true)]
    async fn a_window_ends_when_its_last_tab_closes_but_not_while_it_waits_for_its_first() {
        let (events_in, events) = mpsc::channel(8);
        let (_stop, stopped) = watch::channel(false);
        let running = tokio::spawn(live().run(events, stopped, Arc::default()));
        let tabs = |ids: &[i64]| Event::Tabs {
            tabs: ids
                .iter()
                .map(|id| EngineTab {
                    id: *id,
                    window: 1,
                    target: None,
                    title: String::new(),
                    address: String::new(),
                    active: true,
                    loading: false,
                })
                .collect(),
            active: ids.first().copied(),
        };
        events_in.send(tabs(&[])).await.unwrap();
        tokio::time::sleep(Duration::from_mins(1)).await;
        assert!(!running.is_finished());

        events_in.send(tabs(&[5])).await.unwrap();
        events_in.send(tabs(&[])).await.unwrap();
        let ended = tokio::time::timeout(Duration::from_mins(1), running).await;
        assert!(ended.unwrap().unwrap());
    }

    #[tokio::test]
    async fn the_newest_state_reaches_a_viewer_whose_queue_is_full_of_pictures() {
        let mut live = live();
        let mut ana = join(&mut live, "ana").await;
        look(&mut live, "ana", view(900, 700)).await;
        let outgoing = live.seats["ana"].outgoing.clone();
        while outgoing.try_send(Bytes::from_static(&[1])).is_ok() {}
        reshape(&mut live, "ana", view(1100, 700)).await;
        let state = ana.receive().await.unwrap();
        assert_eq!(state[0], 0, "the state must come before the pictures");
        let state: serde_json::Value = serde_json::from_slice(&state[1..]).unwrap();
        assert_eq!(
            (&state["width"], &state["height"]),
            (&json!(1100), &json!(700))
        );
    }

    #[test]
    fn a_viewer_that_joins_again_keeps_its_new_seat_when_the_earlier_pipe_goes() {
        let mut seats = HashMap::new();
        seats.insert("app".to_owned(), seat(1));
        seats.insert("app".to_owned(), seat(2));
        assert!(free(&mut seats, "app", 1).is_none());
        assert_eq!(seats.get("app").map(|seat| seat.number), Some(2));
        assert!(free(&mut seats, "app", 2).is_some());
        assert!(seats.is_empty());
    }

    #[test]
    fn a_picture_that_the_encoder_would_refuse_gets_fewer_pixels_or_a_smaller_page() {
        let openh264 = (3840, 2160);
        let retina = encodable(
            Size {
                width: 1134,
                height: 1172,
                scale: 2.0,
            },
            openh264,
        )
        .unwrap();
        assert_eq!((retina.width, retina.height), (1134, 1172));
        assert!((1134.0 * retina.scale).ceil() <= 2160.0 && retina.scale > 1.9);
        assert_eq!(
            encodable(
                Size {
                    width: 4096,
                    height: 2400,
                    scale: 1.0,
                },
                openh264,
            ),
            Some(Size {
                width: 3686,
                height: 2160,
                scale: 1.0,
            })
        );
    }

    const KEY: u8 = 1;

    fn open_tab(id: i64) -> Event {
        Event::Tabs {
            tabs: vec![EngineTab {
                id,
                window: 1,
                target: None,
                title: String::new(),
                address: String::new(),
                active: true,
                loading: false,
            }],
            active: Some(id),
        }
    }

    fn video_frame(kind: FrameKind) -> Event {
        Event::Frame(EncodedFrame {
            window: 1,
            kind,
            mark: None,
            still: false,
            data: Bytes::from_static(b"\0\0\0\x01picture"),
        })
    }

    fn key_frame() -> Event {
        video_frame(FrameKind::Key)
    }

    fn pictures(pipe: &mut Pipe) -> Vec<(u32, u8)> {
        std::iter::from_fn(|| pipe.incoming.try_recv().ok())
            .map(|message| {
                let id = u32::from_be_bytes(message[1..5].try_into().unwrap());
                (id, message[5])
            })
            .collect()
    }

    #[tokio::test]
    async fn only_a_view_on_the_browsers_computer_hosts_the_layer_and_learns_its_context() {
        let mut live = live();
        live.handle(open_tab(5)).await;
        let mut mac = join_from(&mut live, "mac", "ana", true).await;
        watch_hosting(&mut live, "mac", view(900, 700), true, true).await;
        let mut phone = join_from(&mut live, "phone", "ana", false).await;
        watch_hosting(&mut live, "phone", view(600, 1000), true, true).await;
        assert_eq!((live.capturing, live.layering), (Some(5), Some(5)));

        live.handle(Event::Layer(7)).await;
        assert_eq!(newest(&mac).and_then(|state| state.layer), Some(7));
        assert_eq!(newest(&phone).and_then(|state| state.layer), None);

        live.handle(key_frame()).await;
        assert_eq!(pictures(&mut mac), Vec::<(u32, u8)>::new());
        assert_eq!(pictures(&mut phone), [(1, KEY)]);

        let again = join_from(&mut live, "mac", "ana", true).await;
        assert_eq!(newest(&again), None);
        watch_hosting(&mut live, "mac", view(900, 700), false, true).await;
        assert_eq!(newest(&again).and_then(|state| state.layer), Some(7));
    }

    #[tokio::test]
    async fn the_engine_codes_video_only_for_a_view_that_takes_it() {
        let mut live = live();
        live.handle(open_tab(5)).await;
        let _mac = join_from(&mut live, "mac", "ana", true).await;
        watch_hosting(&mut live, "mac", view(900, 700), true, true).await;
        assert_eq!((live.capturing, live.layering), (None, Some(5)));
        let phone = join_from(&mut live, "phone", "ana", false).await;
        watch(&mut live, "phone", view(600, 1000), true).await;
        assert_eq!(live.capturing, Some(5));
        live.handle(Event::Leave {
            viewer: "phone".to_owned(),
            seat: phone.seat,
        })
        .await;
        assert_eq!((live.capturing, live.layering), (None, Some(5)));
    }

    #[tokio::test]
    async fn a_view_that_hosts_the_layer_takes_video_for_good_when_the_engine_has_no_layer() {
        let mut live = live();
        live.handle(open_tab(5)).await;
        let mut mac = join_from(&mut live, "mac", "ana", true).await;
        watch_hosting(&mut live, "mac", view(900, 700), true, true).await;
        live.handle(Event::NoLayer).await;
        live.handle(Event::Layer(7)).await;
        assert_eq!((live.capturing, live.layering), (Some(5), None));
        assert_eq!(newest(&mac).and_then(|state| state.layer), None);
        live.handle(key_frame()).await;
        assert_eq!(pictures(&mut mac), [(1, KEY)]);

        let _again = join_from(&mut live, "mac", "ana", true).await;
        watch_hosting(&mut live, "mac", view(900, 700), false, true).await;
        assert_eq!((live.capturing, live.layering), (Some(5), None));
    }

    #[tokio::test]
    async fn a_still_goes_only_to_the_views_that_host_the_layer_and_leaves_no_gap_in_the_video() {
        let mut live = live();
        live.handle(open_tab(5)).await;
        let mut mac = join_from(&mut live, "mac", "ana", true).await;
        watch_hosting(&mut live, "mac", view(900, 700), true, true).await;
        let mut phone = join_from(&mut live, "phone", "ana", false).await;
        watch(&mut live, "phone", view(600, 1000), true).await;
        live.handle(key_frame()).await;
        live.handle(Event::Still(Bytes::from_static(b"\0\0\0\x01still")))
            .await;
        live.handle(video_frame(FrameKind::Delta)).await;
        assert_eq!(pictures(&mut mac), [(1, KEY)]);
        assert_eq!(pictures(&mut phone), [(1, KEY), (2, 0)]);
    }

    #[tokio::test]
    async fn a_view_that_hosts_the_layer_gets_one_still_at_a_time_and_leaves_when_it_takes_none() {
        let mut live = live();
        live.handle(open_tab(5)).await;
        let mut mac = join_from(&mut live, "mac", "ana", true).await;
        watch_hosting(&mut live, "mac", view(900, 700), true, true).await;
        let still = || Event::Still(Bytes::from_static(b"\0\0\0\x01still"));
        for _ in 0..3 {
            live.handle(still()).await;
        }
        assert_eq!(pictures(&mut mac).len(), 1);

        live.handle(Event::Message {
            viewer: "mac".to_owned(),
            message: ToWindow::Ack { frame: 0 },
        })
        .await;
        live.handle(still()).await;
        live.handle(still()).await;
        assert_eq!(pictures(&mut mac).len(), 1);

        live.seats.get_mut("mac").unwrap().still_since = Instant::now().checked_sub(HOPELESS);
        live.tick();
        assert!(!live.seats.contains_key("mac"));
        assert!(!live.watched());
    }

    #[tokio::test]
    async fn a_window_stays_awake_while_a_view_shows_its_layer() {
        let mut live = live();
        let mac = join_from(&mut live, "mac", "ana", true).await;
        watch_hosting(&mut live, "mac", view(900, 700), true, true).await;
        live.used_at = Instant::now().checked_sub(SLEEP_AFTER).unwrap();
        assert!(!live.tick());
        live.handle(Event::Leave {
            viewer: "mac".to_owned(),
            seat: mac.seat,
        })
        .await;
        live.used_at = Instant::now().checked_sub(SLEEP_AFTER).unwrap();
        assert!(live.tick());
    }

    #[tokio::test]
    async fn a_download_cannot_leave_the_folder_and_keeps_older_files() {
        let folder = tempfile::tempdir().unwrap();
        let downloads = tempfile::tempdir().unwrap();
        let first = downloads.path().join("first");
        let second = downloads.path().join("second");
        std::fs::write(&first, "one").unwrap();
        std::fs::write(&second, "two").unwrap();
        let delivered = deliver(&first, folder.path(), "../../report.pdf")
            .await
            .unwrap();
        assert_eq!(delivered, folder.path().join("report.pdf"));
        let numbered = deliver(&second, folder.path(), "report.pdf").await.unwrap();
        assert_eq!(numbered, folder.path().join("report 2.pdf"));
        assert_eq!(std::fs::read_to_string(delivered).unwrap(), "one");
    }
}
