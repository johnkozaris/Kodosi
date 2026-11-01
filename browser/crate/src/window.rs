use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use bytes::Bytes;
use serde_json::json;
use tokio::sync::{mpsc, watch};

use crate::{
    Error,
    engine::Engine,
    extension::{EncodedFrame, ReportedTab, ToExtension},
    flow::{self, Flow},
    input,
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

pub(crate) const FIRST_SIZE: Size = Size {
    width: 1280,
    height: 720,
    scale: 1.0,
};

pub(crate) enum Event {
    Join {
        viewer: Viewer,
        outgoing: mpsc::Sender<Bytes>,
    },
    Message {
        viewer: String,
        message: ToWindow,
    },
    Leave {
        viewer: String,
    },
    Touch,
    Close,
    Frame(EncodedFrame),
    CaptureFailed,
    Tabs {
        tabs: Vec<ReportedTab>,
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
    engine: Arc<Engine>,
    events: mpsc::Sender<Event>,
    state: watch::Receiver<State>,
    open: watch::Receiver<bool>,
}

impl Window {
    /// Gives a viewer its end of the pipe. Its first message is `watch`.
    pub async fn watch(&self, viewer: Viewer) -> Result<Pipe, Error> {
        let (outgoing, incoming) = mpsc::channel(VIEWER_QUEUE);
        let id = viewer.id.clone();
        self.events
            .send(Event::Join { viewer, outgoing })
            .await
            .map_err(|_| Error::Stopped)?;
        Ok(Pipe {
            viewer: id,
            incoming,
            events: self.events.clone(),
        })
    }

    /// The port of the browser's debug connection, for the agent's driver.
    pub fn debug_port(&self) -> u16 {
        self.engine.port
    }

    pub fn state(&self) -> watch::Receiver<State> {
        self.state.clone()
    }

    pub fn is_open(&self) -> bool {
        *self.open.borrow()
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
        let events = self.events.clone();
        if events
            .try_send(Event::Leave {
                viewer: viewer.clone(),
            })
            .is_err()
        {
            tokio::spawn(async move {
                drop(events.send(Event::Leave { viewer }).await);
            });
        }
    }
}

pub(crate) async fn open(
    engine: Arc<Engine>,
    address: &str,
    folder: PathBuf,
) -> Result<Window, Error> {
    let (events_in, events) = mpsc::channel(EVENTS);
    let id = engine
        .open_window(
            address,
            FIRST_SIZE.width,
            FIRST_SIZE.height,
            events_in.clone(),
        )
        .await?;
    let (state_in, state) = watch::channel(State {
        width: FIRST_SIZE.width,
        height: FIRST_SIZE.height,
        ..State::default()
    });
    let (open_in, open) = watch::channel(true);
    let window = Window {
        engine: Arc::clone(&engine),
        events: events_in,
        state,
        open,
    };
    let stopped = engine.stopped();
    let live = Live::new(engine, id, folder, state_in);
    tokio::spawn(async move {
        live.run(events, stopped).await;
        open_in.send_replace(false);
    });
    Ok(window)
}

struct Seat {
    viewer: Viewer,
    outgoing: mpsc::Sender<Bytes>,
    pointer: Option<(f64, f64)>,
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
    dirty: bool,
    size: Size,
    reported: Vec<ReportedTab>,
    active: Option<i64>,
    chosen: Option<i64>,
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
}

impl Live {
    fn new(engine: Arc<Engine>, id: i64, folder: PathBuf, published: watch::Sender<State>) -> Self {
        let state = published.borrow().clone();
        Self {
            engine,
            id,
            folder,
            flow: Flow::new(),
            seats: HashMap::new(),
            state,
            published,
            dirty: false,
            size: FIRST_SIZE,
            reported: Vec::new(),
            active: None,
            chosen: None,
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
        self.engine.command(ToExtension::Stop { stream: self.id });
        self.engine
            .command(ToExtension::CloseWindow { window: self.id });
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
            self.engine.command(ToExtension::Bitrate {
                stream: self.id,
                bitrate,
            });
        }
        if self.dirty {
            self.publish();
        }
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
                    name: seat.viewer.name.clone(),
                    x,
                    y,
                })
            })
            .collect();
        self.published.send_replace(self.state.clone());
        let message = ToViewer::State(Box::new(self.state.clone())).encode();
        for seat in self.seats.values() {
            drop(seat.outgoing.try_send(message.clone()));
        }
    }

    async fn handle(&mut self, event: Event) {
        match event {
            Event::Join { viewer, outgoing } => {
                drop(outgoing.try_send(ToViewer::State(Box::new(self.state.clone())).encode()));
                self.seats.insert(
                    viewer.id.clone(),
                    Seat {
                        viewer,
                        outgoing,
                        pointer: None,
                    },
                );
            }
            Event::Message { viewer, message } => self.message(&viewer, message).await,
            Event::Leave { viewer } => {
                self.seats.remove(&viewer);
                self.flow.leave(&viewer);
                self.update_capture();
                self.dirty = true;
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

    async fn message(&mut self, viewer: &str, message: ToWindow) {
        let now = Instant::now();
        match message {
            ToWindow::Watch { size } => {
                self.flow.seat(viewer, now);
                self.used_at = now;
                self.resize(size).await;
                self.update_capture();
                self.ask_keyframe();
            }
            ToWindow::Ack { frame } => self.flow.ack(viewer, frame, now),
            ToWindow::Keyframe => {
                if self.flow.keyframe_allowed(now) {
                    self.ask_keyframe();
                }
            }
            ToWindow::Size { size } => self.resize(size).await,
            ToWindow::Input { input } => self.input(viewer, input),
        }
    }

    fn ask_keyframe(&self) {
        self.engine
            .command(ToExtension::Keyframe { stream: self.id });
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
                if let Some(seat) = self.seats.get(viewer) {
                    drop(seat.outgoing.try_send(message.clone()));
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

    fn tabs(&mut self, tabs: Vec<ReportedTab>, active: Option<i64>) {
        self.chosen = self
            .chosen
            .filter(|chosen| tabs.iter().any(|tab| tab.id == *chosen));
        self.active = self.chosen.or(active);
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
            None => self.engine.command(ToExtension::Stop { stream: self.id }),
        }
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "a size is at most 4096 and a scale is at most 4"
    )]
    fn capture(&self, tab: i64) {
        let pixels = |length: u32| ((length as f32 * self.size.scale) as u32).clamp(2, 7680);
        self.engine.command(ToExtension::Capture {
            stream: self.id,
            tab,
            width: pixels(self.size.width),
            height: pixels(self.size.height),
            bitrate: flow::FIRST_BITRATE,
        });
    }

    async fn resize(&mut self, size: Size) {
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
        let answer = self
            .engine
            .devtools()
            .call(
                "Browser.setContentsSize",
                json!({ "windowId": self.id, "width": size.width, "height": size.height }),
                None,
            )
            .await;
        if let Err(problem) = answer {
            tracing::debug!(%problem, "the window keeps its size");
        }
        if let Some(tab) = self.capturing {
            self.capture(tab);
        }
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
                if let Some(seat) = self.seats.get_mut(viewer) {
                    seat.pointer = Some((x, y));
                    self.dirty = true;
                }
                if action == MouseAction::Press {
                    self.last_press = Some(viewer.to_owned());
                }
                self.page(
                    "Input.dispatchMouseEvent",
                    &input::mouse(
                        input::Mouse {
                            action,
                            x,
                            y,
                            button,
                            clicks,
                            delta_x,
                            delta_y,
                            modifiers,
                        },
                        mac,
                    ),
                );
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
            Input::Touch {
                action,
                points,
                modifiers,
            } => self.page(
                "Input.dispatchTouchEvent",
                &input::touch(action, &points, modifiers, mac),
            ),
            Input::Text { text } => self.page("Input.insertText", &json!({ "text": text })),
            Input::Open { address } => self.page("Page.navigate", &json!({ "url": address })),
            Input::Back => self.move_in_history(-1),
            Input::Forward => self.move_in_history(1),
            Input::Reload => self.page("Page.reload", &json!({})),
            Input::Tab { id } => {
                if let Ok(tab) = id.parse() {
                    self.chosen = Some(tab);
                    self.engine.command(ToExtension::Activate { tab });
                }
            }
            Input::NewTab => {
                self.chosen = None;
                self.engine.command(ToExtension::NewTab { window: self.id });
            }
            Input::CloseTab { id } => {
                if let Ok(tab) = id.parse() {
                    self.engine.command(ToExtension::CloseTab { tab });
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
            self.engine.command(ToExtension::Pick {
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
