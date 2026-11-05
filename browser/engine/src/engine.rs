use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap},
    os::raw::c_int,
    rc::Rc,
    sync::{Arc, Mutex},
};

use cef::*;
use shared_browser::{
    MenuItem, Size,
    link::{EngineTab, FromEngine, ToEngine},
};

use crate::{client, encode::Encoder, link::Link, renderer};

const REPORT_DELAY_MS: i64 = 50;
/// How long the browsers may take to close when the engine quits. The host waits longer.
const QUIT_LIMIT_MS: i64 = 3000;
const PAGE_SCALE: f32 = 1.0;
/// A page with no ground of its own shows white, as in a browser, not see-through.
pub(crate) const OPAQUE_WHITE: u32 = 0xFFFF_FFFF;

thread_local! {
    static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
}

/// The App of every process: the browser process starts the engine, a page process runs the
/// page script.
pub(crate) fn app() -> App {
    AppBuilder::new(Switches)
}

#[derive(Clone)]
struct Switches;

wrap_app! {
    struct AppBuilder {
        switches: Switches,
    }

    impl App {
        fn on_before_command_line_processing(
            &self,
            process_type: Option<&CefStringUtf16>,
            command_line: Option<&mut CommandLine>,
        ) {
            let (None, Some(command_line)) = (process_type, command_line) else {
                return;
            };
            command_line.append_switch(Some(&"mute-audio".into()));
            command_line.append_switch(Some(&"hide-crash-restore-bubble".into()));
            if cfg!(target_os = "linux") {
                command_line.append_switch_with_value(
                    Some(&"ozone-platform".into()),
                    Some(&"headless".into()),
                );
            }
        }

        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            Some(renderer::handler())
        }
    }
}

/// Starts the engine on the UI thread. It waits for the host's commands.
pub(crate) fn start(link: Link) {
    ENGINE.with(|engine| {
        *engine.borrow_mut() = Some(Engine::new(link));
    });
}

/// Hands a command to the UI thread.
pub(crate) fn post(command: ToEngine) {
    let mut task = CommandTaskBuilder::new(Arc::new(Mutex::new(Some(command))));
    post_task(ThreadId::UI, Some(&mut task));
}

wrap_task! {
    struct CommandTaskBuilder {
        command: Arc<Mutex<Option<ToEngine>>>,
    }

    impl Task {
        fn execute(&self) {
            let command = self.command.lock().ok().and_then(|mut command| command.take());
            if let Some(command) = command {
                run(command);
            }
        }
    }
}

wrap_task! {
    struct ReportTaskBuilder;

    impl Task {
        fn execute(&self) {
            with(Engine::report);
        }
    }
}

wrap_task! {
    struct EndTaskBuilder;

    impl Task {
        fn execute(&self) {
            quit_message_loop();
        }
    }
}

/// The engine, unless it is borrowed: a handler that CEF calls while a command runs gets `None`.
/// So no code calls CEF while it holds the engine.
pub(crate) fn with<R>(work: impl FnOnce(&mut Engine) -> R) -> Option<R> {
    ENGINE.with(|engine| {
        let mut engine = engine.try_borrow_mut().ok()?;
        engine.as_mut().map(work)
    })
}

/// What a window shares with the clients of its tabs, so that CEF can ask for it at any time.
#[derive(Clone)]
pub(crate) struct Shared {
    pub size: Rc<Cell<Size>>,
    pub capture: Rc<RefCell<Option<Capture>>>,
}

impl Shared {
    fn new(size: Size) -> Self {
        Self {
            size: Rc::new(Cell::new(size)),
            capture: Rc::default(),
        }
    }
}

pub(crate) struct Capture {
    pub tab: i64,
    pub encoder: Encoder,
}

/// Where a browser belongs. CEF gives each browser its id only when it exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    Tab {
        window: i64,
        opener: Option<i64>,
        ask: Option<i64>,
    },
    Page {
        page: i64,
    },
}

/// A window of the host. The pages that its tabs open are tabs of it too.
struct Window {
    shared: Shared,
    active: Option<i64>,
    /// The next tab that opens becomes active: the first tab, a new tab or a page's popup.
    wanted: bool,
    /// The host closed the window, which goes with its last tab.
    closing: bool,
}

struct Tab {
    browser: Browser,
    window: i64,
    opener: Option<i64>,
    /// The host's ask that opened the tab, until the tab's target is known.
    ask: Option<i64>,
    target: Option<String>,
    title: String,
    address: String,
    loading: bool,
    shape: String,
    observer: Option<Registration>,
}

struct Page {
    browser: Option<Browser>,
    observer: Option<Registration>,
    shared: Shared,
    closing: bool,
}

pub(crate) struct Engine {
    link: Link,
    windows: BTreeMap<i64, Window>,
    tabs: BTreeMap<i64, Tab>,
    pages: HashMap<i64, Page>,
    page_of_browser: HashMap<i64, i64>,
    frames: HashMap<i64, (i64, String)>,
    frame_handles: HashMap<(i64, String), i64>,
    next_frame: i64,
    next_message: c_int,
    report_waiting: bool,
    quitting: bool,
    page_context: Option<RequestContext>,
}

impl Engine {
    fn new(link: Link) -> Self {
        Self {
            link,
            windows: BTreeMap::new(),
            tabs: BTreeMap::new(),
            pages: HashMap::new(),
            page_of_browser: HashMap::new(),
            frames: HashMap::new(),
            frame_handles: HashMap::new(),
            next_frame: 1,
            next_message: 1,
            report_waiting: false,
            quitting: false,
            page_context: None,
        }
    }

    pub(crate) fn link(&self) -> &Link {
        &self.link
    }

    fn shared_of(&self, window: i64) -> Option<Shared> {
        self.windows
            .get(&window)
            .map(|window| window.shared.clone())
    }

    fn browsers_of(&self, window: i64) -> Vec<Browser> {
        self.tabs
            .values()
            .filter(|tab| tab.window == window)
            .map(|tab| tab.browser.clone())
            .collect()
    }

    fn browser_of(&self, tab: i64) -> Option<Browser> {
        self.tabs.get(&tab).map(|tab| tab.browser.clone())
    }

    fn report_soon(&mut self) {
        if self.report_waiting {
            return;
        }
        self.report_waiting = true;
        let mut task = ReportTaskBuilder::new();
        post_delayed_task(ThreadId::UI, Some(&mut task), REPORT_DELAY_MS);
    }

    fn report(&mut self) {
        self.report_waiting = false;
        let tabs = self
            .tabs
            .iter()
            .map(|(id, tab)| EngineTab {
                id: *id,
                window: tab.window,
                target: tab.target.clone(),
                title: tab.title.clone(),
                address: tab.address.clone(),
                active: self
                    .windows
                    .get(&tab.window)
                    .is_some_and(|window| window.active == Some(*id)),
                loading: tab.loading,
            })
            .collect();
        self.link.send(&FromEngine::Tabs { tabs });
    }

    fn message_id(&mut self) -> c_int {
        let id = self.next_message;
        self.next_message = self.next_message.wrapping_add(1).max(1);
        id
    }

    /// The window of a page that a tab opens: the opener's. A popup, a new window or a new tab
    /// becomes the window's active tab, a tab in the background does not.
    pub(crate) fn place_popup(
        &mut self,
        opener: i64,
        disposition: WindowOpenDisposition,
    ) -> Option<(Origin, Shared)> {
        let window = self.tabs.get(&opener)?.window;
        let known = self
            .windows
            .get_mut(&window)
            .filter(|known| !known.closing)?;
        if disposition != WindowOpenDisposition::NEW_BACKGROUND_TAB {
            known.wanted = true;
        }
        Some((
            Origin::Tab {
                window,
                opener: Some(opener),
                ask: None,
            },
            known.shared.clone(),
        ))
    }

    /// Takes in a browser that CEF made. Gives the id of the message that asks for its target,
    /// or `None` when it must close at once: its window or page closed, or the engine quits,
    /// before it existed.
    pub(crate) fn created(&mut self, browser: &Browser, origin: Origin) -> Option<c_int> {
        let id = i64::from(browser.identifier());
        let quitting = self.quitting;
        let kept = match origin {
            Origin::Tab {
                window,
                opener,
                ask,
            } => {
                let kept = match self.windows.get_mut(&window) {
                    Some(known) if !known.closing && !quitting => {
                        if known.wanted || known.active.is_none() {
                            known.active = Some(id);
                            known.wanted = false;
                        }
                        true
                    }
                    _ => false,
                };
                self.tabs.insert(
                    id,
                    Tab {
                        browser: browser.clone(),
                        window,
                        opener,
                        ask,
                        target: None,
                        title: String::new(),
                        address: String::new(),
                        loading: true,
                        shape: String::new(),
                        observer: None,
                    },
                );
                self.report_soon();
                kept
            }
            Origin::Page { page } => {
                self.page_of_browser.insert(id, page);
                match self.pages.get_mut(&page) {
                    Some(known) if !known.closing && !quitting => {
                        known.browser = Some(browser.clone());
                        true
                    }
                    _ => false,
                }
            }
        };
        kept.then(|| self.message_id())
    }

    pub(crate) fn observing(&mut self, browser: i64, registration: Option<Registration>) {
        if let Some(tab) = self.tabs.get_mut(&browser) {
            tab.observer = registration;
        } else if let Some(page) = self
            .page_of_browser
            .get(&browser)
            .and_then(|page| self.pages.get_mut(page))
        {
            page.observer = registration;
        }
    }

    pub(crate) fn target_known(&mut self, browser: i64, target: String) {
        if let Some(tab) = self.tabs.get_mut(&browser) {
            tab.target = Some(target.clone());
            tab.observer = None;
            if let Some(ask) = tab.ask.take() {
                self.link.send(&FromEngine::TabOpened { ask, target });
            }
            self.report_soon();
        } else if let Some(page) = self.page_of_browser.get(&browser).copied() {
            if let Some(known) = self.pages.get_mut(&page) {
                known.observer = None;
            }
            self.link.send(&FromEngine::PageOpened { page, target });
        }
    }

    /// Forgets a browser that closed. The window's next active tab is the closed tab's opener,
    /// or else its last tab.
    fn closed(&mut self, browser: i64) -> Closed {
        let Some(tab) = self.tabs.remove(&browser) else {
            if let Some(page) = self.page_of_browser.remove(&browser) {
                self.pages.remove(&page);
            }
            return Closed {
                window: None,
                end: self.ended(),
            };
        };
        self.frame_handles.retain(|(tab, _), _| *tab != browser);
        self.frames.retain(|_, (tab, _)| *tab != browser);
        let in_window = |id: &i64| {
            self.tabs
                .get(id)
                .is_some_and(|other| other.window == tab.window)
        };
        let next = tab.opener.filter(in_window).or_else(|| {
            self.tabs
                .iter()
                .rev()
                .find(|(_, other)| other.window == tab.window)
                .map(|(id, _)| *id)
        });
        let mut gone = false;
        let mut shown = None;
        if let Some(window) = self.windows.get_mut(&tab.window) {
            if window.active == Some(browser) {
                window.active = next;
                shown = Some(tab.window);
            }
            let capturing = window
                .shared
                .capture
                .borrow()
                .as_ref()
                .is_some_and(|capture| capture.tab == browser);
            if capturing {
                window.shared.capture.borrow_mut().take();
            }
            gone = window.closing && next.is_none();
        }
        if gone {
            self.windows.remove(&tab.window);
        }
        self.report_soon();
        Closed {
            window: shown.filter(|_| !gone),
            end: self.ended(),
        }
    }

    fn ended(&self) -> bool {
        self.quitting && self.tabs.is_empty() && self.page_of_browser.is_empty()
    }

    /// Closes a window's tabs; the window goes with its last tab.
    fn close_window(&mut self, window: i64) -> Vec<Browser> {
        let Some(known) = self.windows.get_mut(&window) else {
            return Vec::new();
        };
        known.closing = true;
        known.shared.capture.borrow_mut().take();
        let browsers = self.browsers_of(window);
        if browsers.is_empty() {
            self.windows.remove(&window);
        }
        self.report_soon();
        browsers
    }

    /// Closes a host page. A page that CEF still makes closes once it exists.
    fn close_page(&mut self, page: i64) -> Option<Browser> {
        let known = self.pages.get_mut(&page)?;
        known.closing = true;
        known.browser.clone()
    }

    pub(crate) fn title(&mut self, browser: i64, title: String) {
        if let Some(tab) = self.tabs.get_mut(&browser)
            && tab.title != title
        {
            tab.title = title;
            self.report_soon();
        }
    }

    pub(crate) fn address(&mut self, browser: i64, address: String) {
        if let Some(tab) = self.tabs.get_mut(&browser)
            && tab.address != address
        {
            tab.address = address;
            self.report_soon();
        }
    }

    pub(crate) fn loading(&mut self, browser: i64, loading: bool) {
        if let Some(tab) = self.tabs.get_mut(&browser)
            && tab.loading != loading
        {
            tab.loading = loading;
            self.report_soon();
        }
    }

    pub(crate) fn shape(&mut self, browser: i64, shape: &str) {
        if let Some(tab) = self.tabs.get_mut(&browser)
            && tab.shape != shape
        {
            shape.clone_into(&mut tab.shape);
            self.link.send(&FromEngine::Shape {
                tab: browser,
                shape: shape.to_owned(),
            });
        }
    }

    pub(crate) fn context(&self, browser: i64, selection: String, link: Option<String>) {
        if self.tabs.contains_key(&browser) {
            self.link.send(&FromEngine::Context {
                tab: browser,
                selection,
                link,
            });
        }
    }

    /// A message of the page script of a frame.
    pub(crate) fn page_said(&mut self, browser: i64, frame: String, said: &str) {
        if !self.tabs.contains_key(&browser) {
            return;
        }
        let Ok(said) = serde_json::from_str::<renderer::Said>(said) else {
            return;
        };
        let report = match said {
            renderer::Said::Menu { menu, items } => FromEngine::Menu {
                tab: browser,
                frame: self.frame_handle(browser, frame),
                menu,
                items: items
                    .into_iter()
                    .map(|item| MenuItem {
                        label: item.label,
                        enabled: item.enabled,
                        chosen: item.chosen,
                    })
                    .collect(),
            },
            renderer::Said::Copied { text } => FromEngine::Copied { tab: browser, text },
        };
        self.link.send(&report);
    }

    fn frame_handle(&mut self, browser: i64, frame: String) -> i64 {
        if let Some(handle) = self.frame_handles.get(&(browser, frame.clone())) {
            return *handle;
        }
        let handle = self.next_frame;
        self.next_frame += 1;
        self.frame_handles.insert((browser, frame.clone()), handle);
        self.frames.insert(handle, (browser, frame));
        handle
    }

    /// The tabs of a window with whether each shows: only the active one draws.
    fn shown(&self, window: i64) -> Vec<(Browser, bool)> {
        let active = self.windows.get(&window).and_then(|window| window.active);
        self.tabs
            .iter()
            .filter(|(_, tab)| tab.window == window)
            .map(|(id, tab)| (tab.browser.clone(), Some(*id) == active))
            .collect()
    }
}

/// What a closed browser leaves: a window whose active tab changed, and whether the engine
/// may end now.
struct Closed {
    window: Option<i64>,
    end: bool,
}

fn run(command: ToEngine) {
    match command {
        ToEngine::OpenWindow {
            window,
            address,
            size,
        } => open_window(window, &address, size),
        ToEngine::Resize { window, size } => resize(window, size),
        ToEngine::Capture { .. }
        | ToEngine::Stop { .. }
        | ToEngine::Keyframe { .. }
        | ToEngine::Bitrate { .. } => capture(&command),
        ToEngine::Activate { tab } => activate(tab),
        ToEngine::NewTab {
            window,
            address,
            ask,
        } => new_tab(window, &address, ask),
        ToEngine::CloseTab { tab } => close(with(|engine| engine.browser_of(tab)).flatten()),
        ToEngine::CloseWindow { window } => {
            for browser in with(|engine| engine.close_window(window)).unwrap_or_default() {
                close(Some(browser));
            }
        }
        ToEngine::Pick {
            tab,
            frame,
            menu,
            choice,
        } => pick(tab, frame, menu, choice),
        ToEngine::OpenPage {
            page,
            width,
            height,
        } => open_page(page, width, height),
        ToEngine::ResizePage {
            page,
            width,
            height,
        } => resize_page(page, width, height),
        ToEngine::ClosePage { page } => close(with(|engine| engine.close_page(page)).flatten()),
        ToEngine::Quit => quit(),
    }
}

fn open_window(window: i64, address: &str, size: Size) {
    let shared = with(|engine| {
        if engine.quitting || engine.windows.contains_key(&window) {
            return None;
        }
        let shared = Shared::new(size);
        engine.windows.insert(
            window,
            Window {
                shared: shared.clone(),
                active: None,
                wanted: true,
                closing: false,
            },
        );
        Some(shared)
    })
    .flatten();
    if let Some(shared) = shared {
        open_tab(window, address, None, shared);
    }
}

fn resize(window: i64, size: Size) {
    let browsers = with(|engine| {
        let shared = engine.shared_of(window)?;
        shared.size.set(size);
        Some(engine.browsers_of(window))
    })
    .flatten()
    .unwrap_or_default();
    for browser in browsers {
        if let Some(host) = browser.host() {
            host.notify_screen_info_changed();
            host.was_resized();
        }
    }
}

/// The commands for a window's video. A new capture or a key frame asks the page to draw again,
/// since a still page sends no pictures.
fn capture(command: &ToEngine) {
    let repaint_tab = with(|engine| match *command {
        ToEngine::Capture {
            window,
            tab,
            bitrate,
        } => {
            let shared = engine.shared_of(window)?;
            let encoder = Encoder::new(window, bitrate, engine.link().clone());
            *shared.capture.borrow_mut() = Some(Capture { tab, encoder });
            engine.browser_of(tab)
        }
        ToEngine::Stop { window } => {
            engine.shared_of(window)?.capture.borrow_mut().take();
            None
        }
        ToEngine::Keyframe { window } => {
            let shared = engine.shared_of(window)?;
            let mut capture = shared.capture.borrow_mut();
            let capture = capture.as_mut()?;
            capture.encoder.want_key();
            engine.browser_of(capture.tab)
        }
        ToEngine::Bitrate { window, bitrate } => {
            let shared = engine.shared_of(window)?;
            if let Some(capture) = shared.capture.borrow_mut().as_mut() {
                capture.encoder.set_bitrate(bitrate);
            }
            None
        }
        _ => None,
    })
    .flatten();
    if let Some(host) = repaint_tab.and_then(|browser| browser.host()) {
        host.invalidate(PaintElementType::VIEW);
    }
}

fn activate(tab: i64) {
    let shown = with(|engine| {
        let window = engine.tabs.get(&tab)?.window;
        engine.windows.get_mut(&window)?.active = Some(tab);
        engine.report_soon();
        Some(engine.shown(window))
    })
    .flatten()
    .unwrap_or_default();
    show(shown);
}

fn new_tab(window: i64, address: &str, ask: Option<i64>) {
    let shared = with(|engine| {
        let known = engine
            .windows
            .get_mut(&window)
            .filter(|known| !known.closing)?;
        known.wanted = true;
        Some(known.shared.clone())
    })
    .flatten();
    if let Some(shared) = shared {
        open_tab(window, address, ask, shared);
    }
}

fn pick(tab: i64, frame: i64, menu: u32, choice: Option<u32>) {
    let target = with(|engine| {
        let (owner, frame) = engine.frames.get(&frame)?.clone();
        (owner == tab).then_some(())?;
        Some((engine.browser_of(tab)?, frame))
    })
    .flatten();
    if let Some((browser, frame)) = target {
        renderer::pick(&browser, &frame, menu, choice);
    }
}

/// Closes every browser; the engine ends when the last one closed, or after `QUIT_LIMIT_MS`.
fn quit() {
    let browsers = with(|engine| {
        engine.quitting = true;
        engine
            .tabs
            .values()
            .map(|tab| tab.browser.clone())
            .chain(
                engine
                    .pages
                    .values()
                    .filter_map(|page| page.browser.clone()),
            )
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
    if browsers.is_empty() {
        quit_message_loop();
        return;
    }
    let mut end = EndTaskBuilder::new();
    post_delayed_task(ThreadId::UI, Some(&mut end), QUIT_LIMIT_MS);
    for browser in browsers {
        close(Some(browser));
    }
}

fn open_tab(window: i64, address: &str, ask: Option<i64>, shared: Shared) {
    let mut client = client::new(
        Origin::Tab {
            window,
            opener: None,
            ask,
        },
        shared,
    );
    browser_host_create_browser(
        Some(&window_info()),
        Some(&mut client),
        Some(&CefString::from(address)),
        Some(&browser_settings()),
        None,
        None,
    );
}

fn page_size(width: u32, height: u32) -> Size {
    Size {
        width,
        height,
        scale: PAGE_SCALE,
    }
}

/// Opens a page of the host's own, in a request context with no stored data.
fn open_page(page: i64, width: u32, height: u32) {
    let shared = Shared::new(page_size(width, height));
    let context = with(|engine| {
        if engine.quitting {
            return None;
        }
        engine.pages.insert(
            page,
            Page {
                browser: None,
                observer: None,
                shared: shared.clone(),
                closing: false,
            },
        );
        if engine.page_context.is_none() {
            engine.page_context =
                request_context_create_context(Some(&RequestContextSettings::default()), None);
        }
        engine.page_context.clone()
    })
    .flatten();
    let mut client = client::new(Origin::Page { page }, shared);
    let mut extra = dictionary_value_create();
    if let Some(extra) = extra.as_mut() {
        extra.set_bool(Some(&CefString::from(renderer::HOST_PAGE)), 1);
    }
    let started = context.is_some_and(|mut context| {
        browser_host_create_browser(
            Some(&window_info()),
            Some(&mut client),
            Some(&CefString::from("about:blank")),
            Some(&browser_settings()),
            extra.as_mut(),
            Some(&mut context),
        ) == 1
    });
    if !started {
        with(|engine| engine.pages.remove(&page));
    }
}

fn resize_page(page: i64, width: u32, height: u32) {
    let browser = with(|engine| {
        let page = engine.pages.get(&page)?;
        page.shared.size.set(page_size(width, height));
        page.browser.clone()
    })
    .flatten();
    if let Some(host) = browser.and_then(|browser| browser.host()) {
        host.was_resized();
    }
}

pub(crate) fn window_info() -> WindowInfo {
    WindowInfo {
        windowless_rendering_enabled: 1,
        shared_texture_enabled: c_int::from(cfg!(target_os = "macos")),
        ..Default::default()
    }
}

pub(crate) fn browser_settings() -> BrowserSettings {
    BrowserSettings {
        windowless_frame_rate: 60,
        background_color: OPAQUE_WHITE,
        ..Default::default()
    }
}

fn show(tabs: Vec<(Browser, bool)>) {
    for (browser, shown) in tabs {
        if let Some(host) = browser.host() {
            host.was_hidden(c_int::from(!shown));
        }
    }
}

fn close(browser: Option<Browser>) {
    if let Some(host) = browser.and_then(|browser| browser.host()) {
        host.close_browser(1);
    }
}

/// Called by a client when its browser exists: it learns its target on the debug port, or it
/// closes when nobody wants it any more.
pub(crate) fn after_created(browser: &mut Browser, origin: Origin) {
    let Some(message) = with(|engine| engine.created(browser, origin)) else {
        return;
    };
    let Some(message) = message else {
        close(Some(browser.clone()));
        return;
    };
    let id = i64::from(browser.identifier());
    if let Origin::Tab { window, .. } = origin {
        show(with(|engine| engine.shown(window)).unwrap_or_default());
    }
    let Some(host) = browser.host() else {
        return;
    };
    let mut observer = client::target_observer();
    let registration = host.add_dev_tools_message_observer(Some(&mut observer));
    with(|engine| engine.observing(id, registration));
    host.execute_dev_tools_method(message, Some(&"Target.getTargetInfo".into()), None);
}

pub(crate) fn before_close(browser: &Browser) {
    let id = i64::from(browser.identifier());
    let Some(closed) = with(|engine| engine.closed(id)) else {
        return;
    };
    if let Some(window) = closed.window {
        show(with(|engine| engine.shown(window)).unwrap_or_default());
    }
    if closed.end {
        quit_message_loop();
    }
}
