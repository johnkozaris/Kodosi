use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use bytes::Bytes;
use serde_json::json;
use tokio::sync::{mpsc, watch};

use crate::{
    Error, driver,
    engine::Engine,
    flow::{self, Flow},
    input,
    link::{EncodedFrame, EngineTab, ToEngine},
    message::{
        Chooser, Dialog, Download, Frame, Input, Menu, MenuItem, MouseAction, MouseButton, Pointer,
        Size, State, Tab, ToViewer, ToWindow, Viewer,
    },
};

const EVENTS: usize = 1024;
const VIEWER_QUEUE: usize = 1024;
const TICK: Duration = Duration::from_millis(100);
const SLEEP_AFTER: Duration = Duration::from_mins(10);
const RETRY_CAPTURE: Duration = Duration::from_secs(1);
const SIZES: std::ops::RangeInclusive<u32> = 200..=4096;
/// Chrome keeps its window at least this wide.
const NARROWEST: u32 = 500;
const MOST_DOWNLOADS: usize = 16;
const MOST_TEXT: usize = 1024 * 1024;
/// The menu that a right-click opens, which the page does not know about.
const CONTEXT_MENU: u32 = u32::MAX;
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
    },
    Message {
        viewer: String,
        message: ToWindow,
    },
    Leave {
        viewer: String,
        seat: u64,
    },
    Touch,
    Close,
    Frame(EncodedFrame),
    CaptureFailed,
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
    Dialog {
        session: String,
        dialog: Dialog,
    },
    DialogClosed,
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

/// One window of the shared browser, for a host.
#[derive(Clone)]
pub struct Window {
    driver: Arc<str>,
    events: mpsc::Sender<Event>,
    state: watch::Receiver<State>,
    open: watch::Receiver<bool>,
    used: watch::Receiver<Option<Instant>>,
}

impl Window {
    /// Gives a viewer its end of the pipe. Its first message is `watch`.
    pub async fn watch(&self, viewer: Viewer) -> Result<Pipe, Error> {
        let (outgoing, incoming) = mpsc::channel(VIEWER_QUEUE);
        let id = viewer.id.clone();
        let seat = SEATS.fetch_add(1, Ordering::Relaxed);
        self.events
            .send(Event::Join {
                viewer,
                seat,
                outgoing,
            })
            .await
            .map_err(|_| Error::Stopped)?;
        Ok(Pipe {
            viewer: id,
            seat,
            incoming,
            events: self.events.clone(),
        })
    }

    /// The debug connection for the agent's driver, which shows the tabs of this window only.
    pub fn driver(&self) -> &str {
        &self.driver
    }

    pub fn state(&self) -> watch::Receiver<State> {
        self.state.clone()
    }

    pub fn is_open(&self) -> bool {
        *self.open.borrow()
    }

    /// Returns when the window closed.
    pub async fn closed(&self) {
        let mut open = self.open.clone();
        drop(open.wait_for(|open| !open).await);
    }

    /// When a person last clicked, typed or moved between pages.
    pub fn used_at(&self) -> Option<Instant> {
        *self.used.borrow()
    }

    /// A step of the host itself, in no viewer's name.
    pub async fn tell(&self, message: ToWindow) -> Result<(), Error> {
        self.events
            .send(Event::Message {
                viewer: String::new(),
                message,
            })
            .await
            .map_err(|_| Error::Stopped)
    }

    /// Counts as use, so an agent's work keeps the window awake.
    pub async fn touch(&self) {
        drop(self.events.send(Event::Touch).await);
    }

    pub async fn close(&self) {
        drop(self.events.send(Event::Close).await);
    }
}

/// A viewer's end. The host carries these bytes in any way it likes.
pub struct Pipe {
    viewer: String,
    seat: u64,
    incoming: mpsc::Receiver<Bytes>,
    events: mpsc::Sender<Event>,
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

    pub async fn receive(&mut self) -> Option<Bytes> {
        self.incoming.recv().await
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
    let driver = Arc::from(driver.serve(Arc::clone(&engine), id, open.clone()));
    let window = Window {
        driver,
        events: events_in,
        state,
        open,
        used,
    };
    let stopped = engine.stopped();
    let live = Live::new(engine, id, folder, state_in, used_in);
    tokio::spawn(async move {
        live.run(events, stopped).await;
        open_in.send_replace(false);
    });
    Ok(window)
}

struct Seat {
    viewer: Viewer,
    number: u64,
    outgoing: mpsc::Sender<Bytes>,
    pointer: Option<(f64, f64)>,
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

/// What the items of the right-click menu copy, in their order.
struct Context {
    copies: Vec<String>,
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
    last_press: Option<String>,
    open_menu: Option<OpenMenu>,
    context: Option<Context>,
    dialog_session: Option<String>,
    chooser: Option<Waiting>,
    downloads: HashMap<String, String>,
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
            last_press: None,
            open_menu: None,
            context: None,
            dialog_session: None,
            chooser: None,
            downloads: HashMap::new(),
            waiting: Vec::new(),
        }
    }

    async fn run(mut self, mut events: mpsc::Receiver<Event>, mut stopped: watch::Receiver<bool>) {
        let mut tick = tokio::time::interval(TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                event = events.recv() => {
                    let Some(event) = event else { break };
                    if matches!(event, Event::Close) {
                        break;
                    }
                    self.handle(event).await;
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
        if let Some(bitrate) = self.flow.steer(now) {
            self.engine.command(ToEngine::Bitrate {
                window: self.id,
                bitrate,
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
        for seat in self.seats.values() {
            let state = self.state_for(&seat.viewer.id);
            drop(
                seat.outgoing
                    .try_send(ToViewer::State(Box::new(state)).encode()),
            );
        }
    }

    /// What one viewer sees. A list, a copy and a file picker belong to the viewer whose press
    /// opened them, and a viewer's own pointer is on its own screen.
    fn state_for(&self, viewer: &str) -> State {
        let pressed = self.last_press.as_deref() == Some(viewer);
        let mut state = self.state.clone();
        state.pointers.retain(|pointer| pointer.viewer != viewer);
        state.menu.take_if(|menu| menu.viewer != viewer);
        if !pressed {
            state.copied = None;
            state.chooser = None;
        }
        state
    }

    async fn handle(&mut self, event: Event) {
        match event {
            Event::Join {
                viewer,
                seat,
                outgoing,
            } => {
                let state = self.state_for(&viewer.id);
                drop(outgoing.try_send(ToViewer::State(Box::new(state)).encode()));
                self.seats.insert(
                    viewer.id.clone(),
                    Seat {
                        viewer,
                        number: seat,
                        outgoing,
                        pointer: None,
                    },
                );
            }
            Event::Message { viewer, message } => self.message(&viewer, message),
            Event::Leave { viewer, seat } => {
                if free(&mut self.seats, &viewer, seat) {
                    self.flow.leave(&viewer);
                    self.update_capture();
                    self.dirty = true;
                }
            }
            Event::Touch => self.used_at = Instant::now(),
            Event::Close => {}
            Event::Frame(frame) => self.frame(frame),
            Event::CaptureFailed => {
                self.capturing = None;
                self.capture_failed_at = Some(Instant::now());
            }
            Event::Tabs { tabs, active } => self.tabs(tabs, active),
            Event::Shape(shape) => {
                self.state.shape = shape;
                self.dirty = true;
            }
            Event::Menu {
                tab,
                frame,
                menu,
                items,
            } => self.open_list(tab, frame, menu, items),
            Event::Context { selection, link } => self.context_menu(&selection, link),
            Event::Copied(text) => {
                self.state.copied = Some(cut(&text, MOST_TEXT));
                self.dirty = true;
            }
            Event::Dialog { session, dialog } => {
                self.dialog_session = Some(session);
                self.state.dialog = Some(dialog);
                self.dirty = true;
            }
            Event::DialogClosed => {
                self.dialog_session = None;
                self.state.dialog = None;
                self.dirty = true;
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
            Event::DownloadBegan { guid, name } => {
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
            Event::DownloadEnded { guid, done } => self.download_ended(&guid, done).await,
        }
    }

    fn message(&mut self, viewer: &str, message: ToWindow) {
        let now = Instant::now();
        match message {
            ToWindow::Watch { size } => {
                self.flow.seat(viewer, now);
                self.used_at = now;
                self.resize(size);
                self.update_capture();
                self.ask_keyframe();
            }
            ToWindow::Ack { frame } => self.flow.ack(viewer, frame, now),
            ToWindow::Keyframe => {
                if self.flow.keyframe_allowed(now) {
                    self.ask_keyframe();
                }
            }
            ToWindow::Size { size } => self.resize(size),
            ToWindow::Input { input } => self.input(viewer, input),
        }
    }

    fn ask_keyframe(&self) {
        self.engine.command(ToEngine::Keyframe { window: self.id });
    }

    fn frame(&mut self, frame: EncodedFrame) {
        let now = Instant::now();
        self.next_frame = self.next_frame.wrapping_add(1);
        let id = self.next_frame;
        let delivery = self.flow.frame(id, frame.key, frame.data.len(), now);
        if !delivery.to.is_empty() {
            let message = ToViewer::Frame(Frame {
                id,
                key: frame.key,
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
                    self.flow.lost(viewer, id);
                }
            }
        }
        if delivery.ask_keyframe {
            self.ask_keyframe();
        }
        for viewer in delivery.dropped {
            self.seats.remove(&viewer);
            self.dirty = true;
        }
    }

    fn tabs(&mut self, tabs: Vec<EngineTab>, active: Option<i64>) {
        self.active = active;
        self.state.tabs = tabs
            .iter()
            .map(|tab| Tab {
                id: tab.id.to_string(),
                title: tab.title.clone(),
                address: tab.address.clone(),
                active: Some(tab.id) == self.active,
                loading: tab.loading,
            })
            .collect();
        self.reported = tabs;
        self.dirty = true;
        self.send_waiting();
        self.update_capture();
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

    /// The engine draws the pages of this window at the size and the scale of the viewer, so
    /// that the picture has the pixels that the viewer shows.
    fn resize(&mut self, size: Size) {
        let Some(size) = fitted(size) else {
            return;
        };
        if size == self.size {
            return;
        }
        self.size = size;
        self.state.width = size.width;
        self.state.height = size.height;
        self.dirty = true;
        self.engine.command(ToEngine::Resize {
            window: self.id,
            size,
        });
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

    /// Input for the page that the person sees. With no page there is nothing to point at.
    fn page(&self, method: &str, params: &serde_json::Value) {
        if let Some(session) = self.active_session() {
            self.engine.devtools().send(method, params, Some(&session));
        } else {
            tracing::debug!(method, "the window has no page for an input");
        }
    }

    /// A step that the page must get, such as an address. A new window learns its page a
    /// moment after it opens, so the step waits for it.
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

    /// A press names the viewer whose menu or file picker may follow.
    fn mouse(&mut self, viewer: &str, mouse: input::Mouse, mac: bool) {
        if let Some(seat) = self.seats.get_mut(viewer) {
            seat.pointer = Some((mouse.x, mouse.y));
            self.dirty = true;
        }
        if mouse.action == MouseAction::Press {
            self.last_press = Some(viewer.to_owned());
        }
        self.page("Input.dispatchMouseEvent", &input::mouse(mouse, mac));
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
            Input::Open { address } => self.page_soon("Page.navigate", json!({ "url": address })),
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
            Input::Choose { menu, choice } => self.choose(menu, choice),
            Input::Dialog { accept, text } => self.answer_dialog(accept, text),
            Input::Files { paths } => self.give_files(&paths),
        }
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
        let Some(viewer) = self.last_press.clone() else {
            return;
        };
        self.open_menu = Some(OpenMenu { tab, frame, menu });
        self.state.menu = Some(Menu {
            id: menu,
            viewer,
            items,
        });
        self.dirty = true;
    }

    fn context_menu(&mut self, selection: &str, link: Option<String>) {
        let Some(viewer) = self.last_press.clone() else {
            return;
        };
        let item = |label: &str, enabled: bool| MenuItem {
            label: label.to_owned(),
            enabled,
            chosen: false,
        };
        let mut items = vec![item("Copy", !selection.is_empty())];
        let mut copies = vec![cut(selection, MOST_TEXT)];
        if let Some(link) = link {
            items.push(item("Copy Link", true));
            copies.push(link);
        }
        self.open_menu = None;
        self.context = Some(Context { copies });
        self.state.menu = Some(Menu {
            id: CONTEXT_MENU,
            viewer,
            items,
        });
        self.dirty = true;
    }

    fn choose(&mut self, menu: u32, choice: Option<u32>) {
        self.state.menu = None;
        self.dirty = true;
        if menu == CONTEXT_MENU {
            let context = self.context.take();
            if let (Some(mut context), Some(choice)) = (context, choice)
                && let Ok(choice) = usize::try_from(choice)
                && choice < context.copies.len()
            {
                self.state.copied = Some(context.copies.swap_remove(choice));
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
        if let Some(session) = self.dialog_session.take() {
            let mut answer = json!({ "accept": accept });
            if let Some(text) = text {
                answer["promptText"] = json!(text);
            }
            self.engine
                .devtools()
                .send("Page.handleJavaScriptDialog", &answer, Some(&session));
        }
        self.state.dialog = None;
        self.dirty = true;
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

fn fitted(size: Size) -> Option<Size> {
    (SIZES.contains(&size.width) && SIZES.contains(&size.height) && size.scale > 0.0).then_some(
        Size {
            width: size.width.max(NARROWEST),
            height: size.height,
            scale: size.scale.clamp(1.0, 4.0),
        },
    )
}

fn cut(text: &str, most: usize) -> String {
    let mut end = text.len().min(most);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

/// Moves a finished download into the folder. A page names the file, so only its last part
/// counts, and an equal name gets a number.
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
            pointer: None,
        }
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
    fn a_narrow_view_keeps_the_browser_at_its_narrowest() {
        let size = fitted(Size {
            width: 300,
            height: 400,
            scale: 2.0,
        });
        assert_eq!(size.map(|size| size.width), Some(NARROWEST));
    }

    #[test]
    fn a_broken_size_is_ignored() {
        assert_eq!(
            fitted(Size {
                width: 0,
                height: 400,
                scale: 1.0
            }),
            None
        );
        assert_eq!(
            fitted(Size {
                width: 800,
                height: 9000,
                scale: 1.0
            }),
            None
        );
        assert_eq!(
            fitted(Size {
                width: 800,
                height: 400,
                scale: 0.0
            }),
            None
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
