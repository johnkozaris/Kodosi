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
    link::{EngineTab, EngineWindow, FromEngine, ToEngine},
};

use crate::{client, encode::Encoder, link::Link, renderer};

const REPORT_DELAY_MS: i64 = 50;
/// The windows that pages open get ids that no host window has.
const FIRST_OWN_WINDOW: i64 = 1 << 40;
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

/// The engine, unless it is borrowed: a handler that CEF calls while a command runs gets `None`.
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
    Tab { window: i64, opener: Option<i64> },
    Page { page: i64 },
}

struct Window {
    popup: bool,
    shared: Shared,
    active: Option<i64>,
    /// A tab that the host asked for and that becomes active when it exists.
    wanted: bool,
}

struct Tab {
    browser: Browser,
    window: i64,
    opener: Option<i64>,
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
}

pub(crate) struct Engine {
    link: Link,
    windows: BTreeMap<i64, Window>,
    tabs: BTreeMap<i64, Tab>,
    pages: HashMap<i64, Page>,
    page_of_browser: HashMap<i64, i64>,
    frames: HashMap<i64, (i64, String)>,
    frame_handles: HashMap<(i64, String), i64>,
    next_window: i64,
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
            next_window: FIRST_OWN_WINDOW,
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
        let windows = self
            .windows
            .iter()
            .map(|(id, window)| EngineWindow {
                id: *id,
                popup: window.popup,
            })
            .collect();
        let tabs = self
            .tabs
            .iter()
            .map(|(id, tab)| EngineTab {
                id: *id,
                window: tab.window,
                opener: tab.opener,
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
        self.link.send(&FromEngine::Tabs { windows, tabs });
    }

    fn message_id(&mut self) -> c_int {
        let id = self.next_message;
        self.next_message = self.next_message.wrapping_add(1).max(1);
        id
    }

    /// The window of a page that a tab opens: a new window for a popup or a window, the
    /// opener's window for a tab.
    pub(crate) fn place_popup(
        &mut self,
        opener: i64,
        disposition: WindowOpenDisposition,
    ) -> Option<(Origin, Shared)> {
        let window = self.tabs.get(&opener)?.window;
        let own = disposition == WindowOpenDisposition::NEW_POPUP
            || disposition == WindowOpenDisposition::NEW_WINDOW;
        let window = if own {
            let id = self.next_window;
            self.next_window += 1;
            let size = self.windows.get(&window)?.shared.size.get();
            self.windows.insert(
                id,
                Window {
                    popup: disposition == WindowOpenDisposition::NEW_POPUP,
                    shared: Shared::new(size),
                    active: None,
                    wanted: true,
                },
            );
            id
        } else {
            if disposition != WindowOpenDisposition::NEW_BACKGROUND_TAB
                && let Some(known) = self.windows.get_mut(&window)
            {
                known.wanted = true;
            }
            window
        };
        Some((
            Origin::Tab {
                window,
                opener: Some(opener),
            },
            self.shared_of(window)?,
        ))
    }

    pub(crate) fn created(
        &mut self,
        browser: &Browser,
        origin: Origin,
    ) -> Option<(Browser, c_int)> {
        let id = i64::from(browser.identifier());
        let message = self.message_id();
        match origin {
            Origin::Tab { window, opener } => {
                let known = self.windows.get_mut(&window)?;
                if known.wanted || known.active.is_none() {
                    known.active = Some(id);
                    known.wanted = false;
                }
                self.tabs.insert(
                    id,
                    Tab {
                        browser: browser.clone(),
                        window,
                        opener,
                        target: None,
                        title: String::new(),
                        address: String::new(),
                        loading: true,
                        shape: String::new(),
                        observer: None,
                    },
                );
                self.report_soon();
            }
            Origin::Page { page } => {
                self.page_of_browser.insert(id, page);
                if let Some(known) = self.pages.get_mut(&page) {
                    known.browser = Some(browser.clone());
                }
            }
        }
        Some((browser.clone(), message))
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
            tab.target = Some(target);
            tab.observer = None;
            self.report_soon();
        } else if let Some(page) = self.page_of_browser.get(&browser).copied() {
            if let Some(known) = self.pages.get_mut(&page) {
                known.observer = None;
            }
            self.link.send(&FromEngine::PageOpened { page, target });
        }
    }

    /// Forgets a browser that closed, and says whether the engine may now end.
    pub(crate) fn closed(&mut self, browser: i64) -> bool {
        if let Some(tab) = self.tabs.remove(&browser) {
            self.frame_handles.retain(|(tab, _), _| *tab != browser);
            self.frames.retain(|_, (tab, _)| *tab != browser);
            let next = self
                .tabs
                .iter()
                .rev()
                .find(|(_, other)| other.window == tab.window)
                .map(|(id, _)| *id);
            if let Some(window) = self.windows.get_mut(&tab.window) {
                if window.active == Some(browser) {
                    window.active = next;
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
            }
            if tab.window >= FIRST_OWN_WINDOW && next.is_none() {
                self.windows.remove(&tab.window);
            }
            self.report_soon();
        } else if let Some(page) = self.page_of_browser.remove(&browser) {
            self.pages.remove(&page);
        }
        self.quitting && self.tabs.is_empty() && self.page_of_browser.is_empty()
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

    /// The tabs of a window with whether each should show, after the window's active tab changed.
    fn shown(&self, window: i64) -> Vec<(Browser, bool)> {
        let active = self.windows.get(&window).and_then(|window| window.active);
        self.tabs
            .iter()
            .filter(|(_, tab)| tab.window == window)
            .map(|(id, tab)| (tab.browser.clone(), Some(*id) == active))
            .collect()
    }
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
        ToEngine::NewTab { window } => new_tab(window),
        ToEngine::CloseTab { tab } => close(with(|engine| engine.browser_of(tab)).flatten()),
        ToEngine::CloseWindow { window } => {
            for browser in with(|engine| engine.browsers_of(window)).unwrap_or_default() {
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
        ToEngine::ClosePage { page } => close(
            with(|engine| {
                engine
                    .pages
                    .get(&page)
                    .and_then(|page| page.browser.clone())
            })
            .flatten(),
        ),
        ToEngine::Quit => quit(),
    }
}

fn open_window(window: i64, address: &str, size: Size) {
    let shared = with(|engine| {
        let shared = Shared::new(size);
        engine.windows.insert(
            window,
            Window {
                popup: false,
                shared: shared.clone(),
                active: None,
                wanted: true,
            },
        );
        engine.report_soon();
        shared
    });
    if let Some(shared) = shared {
        open_tab(window, address, shared);
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

fn new_tab(window: i64) {
    let shared = with(|engine| {
        engine.windows.get_mut(&window)?.wanted = true;
        engine.shared_of(window)
    })
    .flatten();
    if let Some(shared) = shared {
        open_tab(window, "about:blank", shared);
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

/// Closes every browser; the engine ends when the last one closed.
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
    });
    match browsers {
        Some(browsers) if !browsers.is_empty() => {
            for browser in browsers {
                close(Some(browser));
            }
        }
        _ => quit_message_loop(),
    }
}

fn open_tab(window: i64, address: &str, shared: Shared) {
    let info = window_info();
    let mut client = client::new(
        Origin::Tab {
            window,
            opener: None,
        },
        shared,
    );
    browser_host_create_browser(
        Some(&info),
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

fn open_page(page: i64, width: u32, height: u32) {
    let shared = Shared::new(page_size(width, height));
    let context = with(|engine| {
        engine.pages.insert(
            page,
            Page {
                browser: None,
                observer: None,
                shared: shared.clone(),
            },
        );
        if engine.page_context.is_none() {
            engine.page_context =
                request_context_create_context(Some(&RequestContextSettings::default()), None);
        }
        engine.page_context.clone()
    });
    let Some(mut context) = context.flatten() else {
        return;
    };
    let mut client = client::new(Origin::Page { page }, shared);
    let mut extra = dictionary_value_create();
    if let Some(extra) = extra.as_mut() {
        extra.set_bool(Some(&CefString::from(renderer::HOST_PAGE)), 1);
    }
    browser_host_create_browser(
        Some(&window_info()),
        Some(&mut client),
        Some(&CefString::from("about:blank")),
        Some(&browser_settings()),
        extra.as_mut(),
        Some(&mut context),
    );
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

/// Called by a client when its browser exists: it learns its target on the debug port.
pub(crate) fn after_created(browser: &mut Browser, origin: Origin) {
    let Some((browser, message)) = with(|engine| engine.created(browser, origin)).flatten() else {
        return;
    };
    let id = i64::from(browser.identifier());
    if let Origin::Tab { window, .. } = origin {
        let shown = with(|engine| engine.shown(window)).unwrap_or_default();
        show(shown);
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
    if with(|engine| engine.closed(id)).unwrap_or(false) {
        quit_message_loop();
    }
}
