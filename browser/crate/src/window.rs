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
    flow::{self, Ask, Flow},
    input,
    link::{EncodedFrame, EngineTab, MOST_ICON, ToEngine},
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
/// How long a new size of the page waits for the page to take the last press, release or scroll.
const ANSWER_LIMIT: Duration = Duration::from_secs(1);
const SIZES: std::ops::RangeInclusive<u32> = 200..=4096;
/// Chrome keeps its window at least this wide.
const NARROWEST: u32 = 500;
/// The longer and the shorter side of the largest picture that the encoder takes: `VideoToolbox`
/// refuses one larger than 4096 by 2304, and `OpenH264` one larger than 3840 by 2160.
const MOST_PICTURE: (u16, u16) = if cfg!(target_os = "macos") {
    (4096, 2304)
} else {
    (3840, 2160)
};
const MOST_DOWNLOADS: usize = 16;
const MOST_TEXT: usize = 1024 * 1024;
/// What a page says goes to every viewer with each change of the window, so it has bounds.
const MOST_LINE: usize = 1024;
const MOST_ADDRESS: usize = 8 * 1024;
const MOST_MESSAGE: usize = 16 * 1024;
const MOST_ITEMS: usize = 1000;
/// The menu that a right-click opens, which the page does not know about.
const CONTEXT_MENU: u32 = u32::MAX;
const COPY_SELECTION: u32 = 0;
const COPY_LINK: u32 = 1;
const BLANK: &str = "about:blank";
/// Numbers each join, so that a pipe that goes frees only the seat that it took.
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
    CaptureFailed,
    /// The page took the last press, release or scroll that a new size waited for.
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

#[derive(Clone)]
pub struct Window {
    driver: Arc<Driver>,
    events: mpsc::Sender<Event>,
    state: watch::Receiver<State>,
    open: watch::Receiver<bool>,
    used: watch::Receiver<Option<Instant>>,
    emptied: Arc<AtomicBool>,
}

impl Window {
    /// The viewer's first message is `watch`.
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

    /// Whether the window ended because its last tab closed, as a browser's window does.
    pub fn emptied(&self) -> bool {
        self.emptied.load(Ordering::Acquire)
    }

    pub fn used_at(&self) -> Option<Instant> {
        *self.used.borrow()
    }

    /// An agent's command starts. It counts as use, so an agent's work keeps the window awake,
    /// and a copy that the page makes after it is no person's.
    pub async fn driven(&self) {
        drop(self.events.send(Event::Driven).await);
    }

    pub async fn close(&self) {
        drop(self.events.send(Event::Close).await);
    }
}

/// What a window sends to one viewer must reach it all: the window chooses the pictures that a
/// slow viewer misses, and a picture that goes missing on the way breaks the ones after it.
pub struct Pipe {
    viewer: String,
    seat: u64,
    incoming: mpsc::Receiver<Bytes>,
    mailbox: Arc<Mailbox>,
    events: mpsc::Sender<Event>,
}

/// The newest state for one viewer, which its pipe takes as soon as it has room. A newer state
/// takes the place of one that waits but keeps its copy, so that a copy reaches its viewer once.
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

    /// The newest state comes before the pictures that wait.
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
    };
    let emptied = Arc::clone(&window.emptied);
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
        emptied.store(live.run(events, stopped).await, Ordering::Release);
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
    /// The size of the viewer's view, which the page takes while this viewer is in control.
    size: Option<Size>,
}

/// The frames that wait in a viewer's pipe.
fn queued(seats: &HashMap<String, Seat>, viewer: &str) -> usize {
    seats.get(viewer).map_or(0, |seat| {
        seat.outgoing.max_capacity() - seat.outgoing.capacity()
    })
}

/// Frees a viewer's seat only for the join that took it. A viewer that joined again on a new
/// pipe keeps its new seat when the earlier pipe goes.
fn free(seats: &mut HashMap<String, Seat>, viewer: &str, number: u64) -> bool {
    let taken = seats.get(viewer).is_some_and(|seat| seat.number == number);
    if taken {
        seats.remove(viewer);
    }
    taken
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

/// The viewer whose press, key or scroll came last, or the first viewer while nobody acted: the
/// page has the size of its view, and a list, a copy or a file picker that the page opens is its
/// own.
struct Control {
    viewer: String,
    /// The viewer holds a button, so the page keeps its size until the release.
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

/// Whose act on the page came last. A copy goes to the clipboard of the person who made it, and
/// a copy after an agent's command is no person's.
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
    next_frame: u32,
    used_at: Instant,
    control: Option<Control>,
    actor: Actor,
    /// The page's answer to the last press, release or scroll, which a new size waits for.
    answer: Option<oneshot::Receiver<Result<serde_json::Value, Error>>>,
    answering: bool,
    events: mpsc::WeakSender<Event>,
    /// A copy goes once, with the next state, so that each copy reaches the pasteboard, also
    /// the same words again, and an old copy never comes back.
    copied: Option<Copied>,
    /// The icon of each tab, and those that changed since the last state, which go to every
    /// viewer once.
    icons: BTreeMap<String, String>,
    new_icons: BTreeMap<String, String>,
    open_menu: Option<OpenMenu>,
    context: Option<Context>,
    /// The dialogs of the window's tabs, one for each tab, oldest first. The window shows the
    /// newest.
    dialogs: Vec<(i64, Dialog)>,
    chooser: Option<Waiting>,
    downloads: HashMap<String, String>,
    /// Whether a page may start a download. Each action of a person or an agent allows one, as
    /// in a browser, so a page cannot fill the folder on its own.
    may_download: bool,
    /// Downloads that a page started with no action before them, until they end.
    refused: HashSet<String>,
    /// Steps that came before the window knew its page, such as the address of a new window:
    /// the latest of each kind.
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

    /// Returns whether the window ended because its last tab closed.
    async fn run(
        mut self,
        mut events: mpsc::Receiver<Event>,
        mut stopped: watch::Receiver<bool>,
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
        let hopeless = self.flow.hopeless(now);
        if !hopeless.is_empty() {
            for viewer in hopeless {
                self.seats.remove(&viewer);
                self.gone(&viewer);
            }
            self.update_capture();
            self.dirty = true;
        }
        let seats = &self.seats;
        if self.capturing.is_some()
            && let Some(ask) = self.flow.due(now, |viewer| queued(seats, viewer))
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
        self.flow.is_empty() && now.duration_since(self.used_at) >= SLEEP_AFTER
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
        for seat in self.seats.values() {
            let mut state = self.state_for(&seat.viewer.id);
            state.icons.clone_from(&icons);
            seat.mailbox.put(state);
        }
        self.copied = None;
    }

    /// A list, a copy and a file picker belong to the viewer in control, and a viewer's own
    /// pointer is on its own screen.
    fn state_for(&self, viewer: &str) -> State {
        let pressed = self.in_control(viewer);
        let mut state = self.state.clone();
        state.pointers.retain(|pointer| pointer.viewer != viewer);
        state.menu.take_if(|menu| menu.viewer != viewer);
        state.copied = self
            .copied
            .as_ref()
            .filter(|copied| copied.viewer == viewer)
            .map(|copied| copied.text.clone());
        if !pressed {
            state.chooser = None;
        }
        state
    }

    /// Returns whether the window ends.
    async fn handle(&mut self, event: Event) -> bool {
        match event {
            Event::Join {
                viewer,
                seat,
                outgoing,
                mailbox,
            } => {
                let mut state = self.state_for(&viewer.id);
                state.icons.clone_from(&self.icons);
                mailbox.put(state);
                self.seats.insert(
                    viewer.id.clone(),
                    Seat {
                        viewer,
                        number: seat,
                        outgoing,
                        mailbox,
                        pointer: None,
                        size: None,
                    },
                );
            }
            Event::Message { viewer, message } => self.message(&viewer, message),
            Event::Leave { viewer, seat } => {
                if free(&mut self.seats, &viewer, seat) {
                    self.flow.leave(&viewer);
                    self.update_capture();
                    self.gone(&viewer);
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
            Event::CaptureFailed => {
                self.capturing = None;
                self.capture_failed_at = Some(Instant::now());
            }
            Event::Tabs { tabs, active } => return self.tabs(tabs, active),
            // The person's own pointer takes this shape, so it cannot wait for the tick.
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
            ToWindow::Watch { size, anchors } => {
                self.flow.seat(viewer, anchors, now);
                self.used_at = now;
                self.control.get_or_insert_with(|| Control::of(viewer));
                self.keep_size(viewer, size);
                self.update_capture();
                self.flow.keyframe_asked(now);
                self.ask(Ask::Keyframe);
            }
            ToWindow::Ack { frame } => self.flow.ack(viewer, frame, now),
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
                timestamp: frame.timestamp,
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

    /// Returns whether the last tab closed.
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
        let wanted = if self.flow.is_empty() {
            None
        } else {
            self.active
        };
        if wanted == self.capturing {
            return;
        }
        self.capturing = wanted;
        match wanted {
            Some(tab) => self.capture(tab),
            None => self.engine.command(ToEngine::Stop { window: self.id }),
        }
    }

    fn capture(&self, tab: i64) {
        self.engine.command(ToEngine::Capture {
            window: self.id,
            tab,
            bitrate: flow::FIRST_BITRATE,
        });
    }

    /// A viewer that only looks keeps its size here, so that it never changes the shape of the
    /// page under the viewer who uses it, only the pixels that the page gets.
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

    fn in_control(&self, viewer: &str) -> bool {
        self.control
            .as_ref()
            .is_some_and(|control| control.viewer == viewer)
    }

    /// The control goes to the viewer that came first of those that stay, and the page loses the
    /// pixels that only the viewer that went showed.
    fn gone(&mut self, viewer: &str) {
        if self.in_control(viewer) {
            self.control = self
                .seats
                .values()
                .min_by_key(|seat| seat.number)
                .map(|seat| Control::of(&seat.viewer.id));
        }
        self.follow();
    }

    /// The page takes the size of the view in control, but never under a held button, and only
    /// after the page took the last press, release or scroll. Those reach the page by its debug
    /// port and the size by the engine's link, so without the wait a press could meet the new
    /// layout with the place that the viewer found on the old one.
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

    /// The engine draws the pages of this window at the size of the viewer in control and with
    /// the pixels of the sharpest view, so that each picture has the pixels that its viewers show.
    /// The new size goes to the viewers at once, because they place each point by it.
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

    /// A new window learns its page a moment after it opens, so a step that the page must get,
    /// such as an address, waits for it.
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

    /// A menu opens where its viewer last pointed, which is where a press opened it.
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

/// A view that is narrower than Chrome's window gets a wider page of its own shape, drawn with
/// fewer pixels for each point, so that the picture fills the view and still has its pixels. Each
/// of the `views` fits the page into itself, and the page gets the pixels of the sharpest.
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

/// The pixels for each point of a page that a view shows when it fits the page into itself.
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

/// A page whose picture the encoder would refuse gets fewer pixels for each point, and one that is
/// too large for it even at one pixel for each point gets smaller in its own shape. Chrome rounds
/// the pixels of each side up.
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

/// A page names the file, so only its last part counts, and an equal name gets a number.
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
            },
            number,
            outgoing: mpsc::channel(1).0,
            mailbox: Arc::default(),
            pointer: None,
            size: None,
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
        let (outgoing, incoming) = mpsc::channel(VIEWER_QUEUE);
        let mailbox = Arc::new(Mailbox::default());
        let viewer = Viewer {
            id: id.to_owned(),
            person: id.to_owned(),
            name: id.to_owned(),
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
        live.handle(Event::Message {
            viewer: viewer.to_owned(),
            message: ToWindow::Watch {
                size,
                anchors: false,
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
    async fn a_viewer_that_only_joins_leaves_the_page_in_the_shape_of_the_one_who_uses_it() {
        let mut live = live();
        let _ana = join(&mut live, "ana").await;
        look(&mut live, "ana", view(900, 700)).await;
        let _bob = join(&mut live, "bob").await;
        look(&mut live, "bob", view(600, 1000)).await;
        reshape(&mut live, "bob", view(620, 1000)).await;
        assert_eq!(live.size, view(900, 700));
    }

    #[tokio::test]
    async fn a_sharper_view_that_only_looks_gets_its_pixels_until_it_goes() {
        let mut live = live();
        let _ana = join(&mut live, "ana").await;
        let small = Size {
            width: 600,
            height: 571,
            scale: 1.0,
        };
        look(&mut live, "ana", small).await;
        let bob = join(&mut live, "bob").await;
        look(&mut live, "bob", view(1134, 1172)).await;
        assert_eq!((live.size.width, live.size.height), (600, 571));
        assert!((live.size.scale - 3.78).abs() < 0.01);
        live.handle(Event::Leave {
            viewer: "bob".to_owned(),
            seat: bob.seat,
        })
        .await;
        assert_eq!(live.size, small);
    }

    #[tokio::test]
    async fn a_new_size_of_the_viewer_in_control_applies_at_once_and_reaches_every_viewer() {
        let mut live = live();
        let ana = join(&mut live, "ana").await;
        look(&mut live, "ana", view(900, 700)).await;
        reshape(&mut live, "ana", view(1100, 700)).await;
        assert_eq!(live.size, view(1100, 700));
        let shown = newest(&ana).map(|state| (state.width, state.height));
        assert_eq!(shown, Some((1100, 700)));
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
        let ana = join(&mut live, "ana").await;
        let bob = join(&mut live, "bob").await;
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

        let again = join(&mut live, "ana").await;
        assert_eq!(copied(&again), None);
    }

    #[tokio::test]
    async fn a_copy_goes_to_the_viewer_whose_key_asked_for_it() {
        let mut live = live();
        let ana = join(&mut live, "ana").await;
        let bob = join(&mut live, "bob").await;
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
        let ana = join(&mut live, "ana").await;
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
        let ana = join(&mut live, "ana").await;
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

    #[tokio::test]
    async fn a_right_click_menu_opens_where_its_viewer_pressed_and_copies_its_link_for_that_viewer()
    {
        let mut live = live();
        let ana = join(&mut live, "ana").await;
        let bob = join(&mut live, "bob").await;
        let right = json!({ "kind": "mouse", "action": "press", "x": 40, "y": 30, "button": "right", "clicks": 1 });
        say(&mut live, "ana", right).await;
        live.handle(Event::Context {
            selection: String::new(),
            link: Some("https://kodosi.com/".to_owned()),
        })
        .await;
        live.publish();
        let menu = newest(&ana).and_then(|state| state.menu).unwrap();
        assert_eq!((menu.x, menu.y), (40.0, 30.0));
        assert_eq!(newest(&bob).and_then(|state| state.menu), None);

        say(&mut live, "ana", json!({ "kind": "choose", "menu": 7 })).await;
        live.publish();
        assert_eq!(
            newest(&ana).and_then(|state| state.menu),
            Some(menu.clone())
        );

        let link = json!({ "kind": "choose", "menu": menu.id, "choice": COPY_LINK });
        say(&mut live, "ana", link).await;
        live.publish();
        assert_eq!(copied(&ana).as_deref(), Some("https://kodosi.com/"));
    }

    #[tokio::test]
    async fn each_viewer_gets_a_tab_icon_once_also_through_a_newer_state_and_after_joining_late() {
        let mut live = live();
        let ana = join(&mut live, "ana").await;
        live.handle(Event::Icon {
            tab: 5,
            png: "aWNvbg==".to_owned(),
        })
        .await;
        live.publish();
        live.handle(Event::Shape("text".to_owned())).await;
        let icon = |state: Option<State>| state.and_then(|state| state.icons.get("5").cloned());
        assert_eq!(icon(newest(&ana)).as_deref(), Some("aWNvbg=="));

        live.handle(Event::Shape("pointer".to_owned())).await;
        assert_eq!(icon(newest(&ana)), None);

        let bob = join(&mut live, "bob").await;
        assert_eq!(icon(newest(&bob)).as_deref(), Some("aWNvbg=="));
    }

    #[tokio::test(start_paused = true)]
    async fn a_window_ends_when_its_last_tab_closes_but_not_while_it_waits_for_its_first() {
        let (events_in, events) = mpsc::channel(8);
        let (_stop, stopped) = watch::channel(false);
        let running = tokio::spawn(live().run(events, stopped));
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
    async fn each_viewer_sees_where_the_others_point_but_not_its_own_pointer() {
        let mut live = live();
        let ana = join(&mut live, "ana").await;
        let bob = join(&mut live, "bob").await;
        say(
            &mut live,
            "ana",
            json!({ "kind": "mouse", "action": "move", "x": 40, "y": 30 }),
        )
        .await;
        live.tick();
        let pointers = |pipe: &Pipe| {
            newest(pipe).map(|state| {
                state
                    .pointers
                    .into_iter()
                    .map(|pointer| (pointer.viewer, pointer.x, pointer.y))
                    .collect::<Vec<_>>()
            })
        };
        assert_eq!(pointers(&bob), Some(vec![("ana".to_owned(), 40.0, 30.0)]));
        assert_eq!(pointers(&ana), Some(Vec::new()));
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
        assert!(!free(&mut seats, "app", 1));
        assert_eq!(seats.get("app").map(|seat| seat.number), Some(2));
        assert!(free(&mut seats, "app", 2));
        assert!(seats.is_empty());
    }

    #[test]
    fn a_new_window_opens_the_latest_address_that_came_before_its_page() {
        let mut waiting = Vec::new();
        keep_latest(
            &mut waiting,
            "Page.navigate",
            json!({ "url": "https://a.test" }),
        );
        keep_latest(
            &mut waiting,
            "Emulation.setDeviceMetricsOverride",
            json!({}),
        );
        keep_latest(
            &mut waiting,
            "Page.navigate",
            json!({ "url": "https://b.test" }),
        );
        assert_eq!(
            waiting,
            vec![
                ("Emulation.setDeviceMetricsOverride", json!({})),
                ("Page.navigate", json!({ "url": "https://b.test" })),
            ]
        );
    }

    #[test]
    fn a_narrow_view_gets_a_wider_page_of_its_own_shape_with_its_own_pixels() {
        assert_eq!(
            fitted(
                Size {
                    width: 440,
                    height: 717,
                    scale: 3.0,
                },
                [],
            ),
            Some(Size {
                width: NARROWEST,
                height: 815,
                scale: 2.64,
            })
        );
        assert_eq!(
            fitted(
                Size {
                    width: 1280,
                    height: 800,
                    scale: 2.0,
                },
                [],
            ),
            Some(Size {
                width: 1280,
                height: 800,
                scale: 2.0,
            })
        );
    }

    #[test]
    fn a_broken_size_is_ignored() {
        assert_eq!(
            fitted(
                Size {
                    width: 0,
                    height: 400,
                    scale: 1.0
                },
                [],
            ),
            None
        );
        assert_eq!(
            fitted(
                Size {
                    width: 800,
                    height: 9000,
                    scale: 1.0
                },
                [],
            ),
            None
        );
        assert_eq!(
            fitted(
                Size {
                    width: 800,
                    height: 400,
                    scale: 0.0
                },
                [],
            ),
            None
        );
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
