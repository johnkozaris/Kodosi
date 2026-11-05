use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};

use serde_json::{Value, json};
#[cfg(all(unix, not(target_os = "macos")))]
use tokio::io::{AsyncBufReadExt as _, BufReader};
use tokio::{
    net::TcpListener,
    process::{Child, Command},
    sync::{mpsc, oneshot, watch},
};

use crate::{
    Error,
    devtools::{self, Devtools},
    link::{EncodedFrame, EngineTab, FromEngine, ToEngine},
    message::{Dialog, DialogKind, Size},
    socket::{Report, Socket},
    window::Event,
};

const START_LIMIT: Duration = Duration::from_secs(30);
const START_CHECK: Duration = Duration::from_millis(50);
const ATTACH_LIMIT: Duration = Duration::from_secs(5);
const COMMANDS: usize = 256;
const QUIT_LIMIT: Duration = Duration::from_secs(5);
const LINK_FILE: &str = "link.json";
/// The program inside a Kodosi Browser app bundle.
#[cfg(target_os = "macos")]
const BUNDLE_PROGRAM: &str = "Contents/MacOS/Kodosi Browser";

/// The tabs of the browser, by the windows that hosts opened. A window that a page opened
/// belongs with the page.
#[derive(Default)]
pub(crate) struct Routes {
    windows: HashMap<i64, mpsc::Sender<Event>>,
    tabs: HashMap<i64, i64>,
    targets: HashMap<String, i64>,
    sessions: HashMap<String, String>,
    session_of_target: HashMap<String, String>,
    downloads: HashMap<String, i64>,
    /// The host's own pages, by their sessions.
    pages: HashMap<String, mpsc::UnboundedSender<devtools::Event>>,
    /// Host pages that the engine opens, until it names their targets.
    opening: HashMap<i64, oneshot::Sender<String>>,
    /// Targets that the host attaches to now.
    attaching: HashSet<String>,
    /// The engine's last report of its tabs, which the windows get again once the host attached.
    last_tabs: Vec<EngineTab>,
}

impl Routes {
    fn sender_of_tab(&self, tab: i64) -> Option<mpsc::Sender<Event>> {
        self.windows.get(self.tabs.get(&tab)?).cloned()
    }

    fn sender_of_session(&self, session: &str) -> Option<(i64, mpsc::Sender<Event>)> {
        let target = self.sessions.get(session)?;
        let window = *self.targets.get(target)?;
        Some((window, self.windows.get(&window)?.clone()))
    }

    /// Places each tab in its window, and tells each window its tabs.
    fn place(&mut self, tabs: &[EngineTab]) -> Vec<(mpsc::Sender<Event>, Event)> {
        let openers: HashMap<i64, Option<i64>> =
            tabs.iter().map(|tab| (tab.id, tab.opener)).collect();
        let windows_of_tabs: HashMap<i64, i64> =
            tabs.iter().map(|tab| (tab.id, tab.window)).collect();
        let owner = |tab: &EngineTab| {
            let mut current = tab.id;
            for _ in 0..tabs.len() {
                let window = windows_of_tabs.get(&current)?;
                if self.windows.contains_key(window) {
                    return Some(*window);
                }
                current = (*openers.get(&current)?)?;
            }
            None
        };
        let mut placed: HashMap<i64, Vec<EngineTab>> = HashMap::new();
        let mut tab_routes = HashMap::new();
        let mut target_routes = HashMap::new();
        for tab in tabs {
            if let Some(window) = owner(tab) {
                tab_routes.insert(tab.id, window);
                if let Some(target) = &tab.target {
                    target_routes.insert(target.clone(), window);
                }
                placed.entry(window).or_default().push(tab.clone());
            }
        }
        self.tabs = tab_routes;
        self.targets = target_routes;
        self.windows
            .iter()
            .map(|(window, sender)| {
                let tabs = placed.remove(window).unwrap_or_default();
                let popup = tabs
                    .iter()
                    .rfind(|tab| tab.window != *window)
                    .map(|tab| tab.id);
                let active = popup.or_else(|| tabs.iter().find(|tab| tab.active).map(|tab| tab.id));
                (sender.clone(), Event::Tabs { tabs, active })
            })
            .collect()
    }
}

/// A page that the host opened for itself, with the events of its session.
pub(crate) struct OpenedPage {
    pub page: i64,
    pub target: String,
    pub session: String,
    pub events: mpsc::UnboundedReceiver<devtools::Event>,
}

pub(crate) struct Engine {
    devtools: Devtools,
    pub port: u16,
    commands: mpsc::Sender<ToEngine>,
    routes: Arc<Mutex<Routes>>,
    stopped: watch::Sender<bool>,
    _process: Vec<Child>,
    downloads: PathBuf,
    next_window: AtomicI64,
    next_page: AtomicI64,
}

impl Engine {
    pub(crate) async fn start(
        program: &Path,
        profiles: &Path,
        profile: &str,
    ) -> Result<Arc<Self>, Error> {
        let folder = profiles.join(profile);
        tokio::fs::create_dir_all(&folder).await?;
        let socket = Socket::open().await?;
        let link = folder.join(LINK_FILE);
        socket.write_address(&link).await?;
        let port = free_port().await?;
        let mut process = launch(program, &arguments(&folder, &link, port)).await?;
        let address = wait_for_address(port, &mut process).await?;
        let (devtools, events) = Devtools::connect(&address).await?;
        devtools
            .call(
                "Target.setAutoAttach",
                json!({
                    "autoAttach": true,
                    "waitForDebuggerOnStart": false,
                    "flatten": true,
                    "filter": [{ "type": "page", "exclude": false }],
                }),
                None,
            )
            .await?;
        let downloads = folder.join("downloads");
        tokio::fs::create_dir_all(&downloads).await?;
        devtools
            .call(
                "Browser.setDownloadBehavior",
                json!({
                    "behavior": "allowAndName",
                    "downloadPath": downloads,
                    "eventsEnabled": true,
                }),
                None,
            )
            .await?;
        let (commands, waiting) = mpsc::channel(COMMANDS);
        let (reports_in, reports) = mpsc::channel(COMMANDS);
        tokio::spawn(async move {
            if let Err(problem) = socket.accept(waiting, reports_in).await {
                tracing::warn!(%problem, "the browser's link is gone");
            }
        });
        let engine = Arc::new(Self {
            devtools,
            port,
            commands,
            routes: Arc::default(),
            stopped: watch::Sender::new(false),
            _process: process,
            downloads,
            next_window: AtomicI64::new(1),
            next_page: AtomicI64::new(1),
        });
        tokio::spawn(follow(Arc::clone(&engine), events, reports));
        Ok(engine)
    }

    pub(crate) fn devtools(&self) -> &Devtools {
        &self.devtools
    }

    pub(crate) fn command(&self, command: ToEngine) {
        if self.commands.try_send(command).is_err() {
            tracing::debug!("the browser's queue is full");
        }
    }

    pub(crate) fn stopped(&self) -> watch::Receiver<bool> {
        self.stopped.subscribe()
    }

    pub(crate) fn is_stopped(&self) -> bool {
        *self.stopped.borrow()
    }

    pub(crate) fn session_of(&self, target: &str) -> Option<String> {
        self.routes().session_of_target.get(target).cloned()
    }

    fn routes(&self) -> std::sync::MutexGuard<'_, Routes> {
        self.routes.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Opens a window with its first page and returns its id.
    pub(crate) fn open_window(
        &self,
        address: &str,
        size: Size,
        events: mpsc::Sender<Event>,
    ) -> i64 {
        let window = self.next_window.fetch_add(1, Ordering::Relaxed);
        self.routes().windows.insert(window, events);
        self.command(ToEngine::OpenWindow {
            window,
            address: address.to_owned(),
            size,
        });
        window
    }

    /// Opens a page apart from the people's tabs, with no stored data.
    pub(crate) async fn open_page(&self, width: u32, height: u32) -> Result<OpenedPage, Error> {
        let page = self.next_page.fetch_add(1, Ordering::Relaxed);
        let (named, target) = oneshot::channel();
        self.routes().opening.insert(page, named);
        self.command(ToEngine::OpenPage {
            page,
            width,
            height,
        });
        let target = tokio::time::timeout(ATTACH_LIMIT, target).await;
        let Ok(Ok(target)) = target else {
            self.routes().opening.remove(&page);
            self.close_page(page, None).await;
            return Err(Error::Engine("the page did not open".to_owned()));
        };
        let Some(session) = self.attach(&target).await else {
            self.close_page(page, Some(&target)).await;
            return Err(Error::Engine("the page did not attach".to_owned()));
        };
        let (sender, events) = mpsc::unbounded_channel();
        self.routes().pages.insert(session.clone(), sender);
        Ok(OpenedPage {
            page,
            target,
            session,
            events,
        })
    }

    /// CEF tells a browser-wide auto-attach of no page that opens later, so the host attaches to
    /// each new target itself, and the windows get their tabs again with its session.
    fn attach_new(self: &Arc<Self>, tabs: &[EngineTab]) {
        let targets: Vec<String> = {
            let mut routes = self.routes();
            tabs.iter()
                .filter_map(|tab| tab.target.clone())
                .filter(|target| {
                    !routes.session_of_target.contains_key(target)
                        && routes.attaching.insert(target.clone())
                })
                .collect()
        };
        for target in targets {
            let engine = Arc::clone(self);
            tokio::spawn(async move {
                engine.attach(&target).await;
            });
        }
    }

    async fn attach(&self, target: &str) -> Option<String> {
        if let Some(session) = self.session_of(target) {
            return Some(session);
        }
        let answer = self
            .devtools
            .call(
                "Target.attachToTarget",
                json!({ "targetId": target, "flatten": true }),
                None,
            )
            .await;
        let session = answer
            .ok()
            .and_then(|answer| answer["sessionId"].as_str().map(str::to_owned));
        let deliveries = {
            let mut routes = self.routes();
            routes.attaching.remove(target);
            let tabs = routes.last_tabs.clone();
            if session.is_some() {
                routes.place(&tabs)
            } else {
                Vec::new()
            }
        };
        let session = session?;
        self.attached(&json!({ "sessionId": session, "targetInfo": { "targetId": target } }));
        for (sender, event) in deliveries {
            drop(sender.try_send(event));
        }
        Some(session)
    }

    pub(crate) async fn close_page(&self, page: i64, target: Option<&str>) {
        if let Some(session) = target.and_then(|target| self.session_of(target)) {
            self.routes().pages.remove(&session);
        }
        self.command(ToEngine::ClosePage { page });
        self.stop_if_idle().await;
    }

    /// The host page that an event belongs to.
    fn page_of(&self, event: &devtools::Event) -> Option<mpsc::UnboundedSender<devtools::Event>> {
        self.routes().pages.get(event.session.as_deref()?).cloned()
    }

    /// Stops the browser when its last window closed and no host page is open.
    pub(crate) async fn window_closed(&self, window: i64) {
        let last = {
            let mut routes = self.routes();
            routes.windows.remove(&window);
            routes.windows.is_empty() && routes.pages.is_empty()
        };
        if last {
            self.stop().await;
        }
    }

    /// Stops a browser that started only for work with no window.
    pub(crate) async fn stop_if_idle(&self) {
        let idle = {
            let routes = self.routes();
            routes.windows.is_empty() && routes.pages.is_empty()
        };
        if idle {
            self.stop().await;
        }
    }

    /// Ends the browser and waits until it is gone, so its profile is free.
    pub(crate) async fn stop(&self) {
        if self.is_stopped() {
            return;
        }
        let mut stopped = self.stopped();
        self.command(ToEngine::Quit);
        if tokio::time::timeout(QUIT_LIMIT, stopped.wait_for(|stopped| *stopped))
            .await
            .is_err()
        {
            tracing::warn!("the browser did not quit in time");
        }
        self.stopped.send_replace(true);
    }

    pub(crate) fn download_path(&self, guid: &str) -> PathBuf {
        self.downloads.join(guid)
    }

    fn attached(&self, params: &Value) {
        let (Some(session), Some(target)) = (
            params["sessionId"].as_str(),
            params["targetInfo"]["targetId"].as_str(),
        ) else {
            return;
        };
        {
            let mut routes = self.routes();
            if routes
                .session_of_target
                .get(target)
                .is_some_and(|known| known == session)
            {
                return;
            }
            routes
                .sessions
                .insert(session.to_owned(), target.to_owned());
            routes
                .session_of_target
                .insert(target.to_owned(), session.to_owned());
        }
        for (method, params) in [
            ("Page.enable", json!({})),
            (
                "Page.setInterceptFileChooserDialog",
                json!({ "enabled": true }),
            ),
            (
                "Emulation.setFocusEmulationEnabled",
                json!({ "enabled": true }),
            ),
        ] {
            self.devtools.send(method, &params, Some(session));
        }
    }

    fn detached(&self, params: &Value) {
        let Some(session) = params["sessionId"].as_str() else {
            return;
        };
        let mut routes = self.routes();
        if let Some(target) = routes.sessions.remove(session) {
            routes.session_of_target.remove(&target);
        }
    }

    fn page_event(&self, event: devtools::Event) -> Option<(mpsc::Sender<Event>, Event)> {
        let session = event.session?;
        let (window, sender) = self.routes().sender_of_session(&session)?;
        let params = &event.params;
        let forwarded = match event.method.as_str() {
            "Page.javascriptDialogOpening" => Event::Dialog {
                session,
                dialog: dialog_of(params)?,
            },
            "Page.javascriptDialogClosed" => Event::DialogClosed,
            "Page.fileChooserOpened" => Event::Chooser {
                session,
                node: params["backendNodeId"].as_i64()?,
                several: params["mode"] == "selectMultiple",
            },
            "Page.downloadWillBegin" => {
                let guid = params["guid"].as_str()?.to_owned();
                self.routes().downloads.insert(guid.clone(), window);
                Event::DownloadBegan {
                    guid,
                    name: params["suggestedFilename"].as_str()?.to_owned(),
                }
            }
            _ => return None,
        };
        Some((sender, forwarded))
    }

    fn download_progress(&self, params: &Value) -> Option<(mpsc::Sender<Event>, Event)> {
        let guid = params["guid"].as_str()?;
        let state = params["state"].as_str()?;
        if state == "inProgress" {
            return None;
        }
        let sender = {
            let mut routes = self.routes();
            let window = routes.downloads.remove(guid)?;
            routes.windows.get(&window)?.clone()
        };
        Some((
            sender,
            Event::DownloadEnded {
                guid: guid.to_owned(),
                done: state == "completed",
            },
        ))
    }

    /// A window that is behind loses a frame and starts again at a key frame, so one slow
    /// window never holds up the others or the browser's answers.
    fn deliver_frame(&self, frame: EncodedFrame) {
        let window = frame.window;
        let Some(sender) = self.routes().windows.get(&window).cloned() else {
            return;
        };
        if sender.try_send(Event::Frame(frame)).is_err() {
            tracing::debug!(window, "a window is behind, so it gets a key frame");
            self.command(ToEngine::Keyframe { window });
        }
    }

    fn report(&self, report: Report) -> Vec<(mpsc::Sender<Event>, Event)> {
        let said = match report {
            Report::Frame(frame) => {
                self.deliver_frame(frame);
                return Vec::new();
            }
            Report::Said(said) => said,
        };
        let to_tab = |tab: i64, event: Event| {
            self.routes()
                .sender_of_tab(tab)
                .map(|sender| vec![(sender, event)])
                .unwrap_or_default()
        };
        match said {
            FromEngine::Failed { window, problem } => {
                tracing::warn!(%problem, "the capture stopped");
                let sender = self.routes().windows.get(&window).cloned();
                sender
                    .map(|sender| vec![(sender, Event::CaptureFailed)])
                    .unwrap_or_default()
            }
            FromEngine::Shape { tab, shape } => to_tab(tab, Event::Shape(shape)),
            FromEngine::Menu {
                tab,
                frame,
                menu,
                items,
            } => to_tab(
                tab,
                Event::Menu {
                    tab,
                    frame,
                    menu,
                    items,
                },
            ),
            FromEngine::Context {
                tab,
                selection,
                link,
            } => to_tab(tab, Event::Context { selection, link }),
            FromEngine::Copied { tab, text } => to_tab(tab, Event::Copied(text)),
            FromEngine::Tabs { tabs, .. } => {
                let mut routes = self.routes();
                routes.last_tabs.clone_from(&tabs);
                routes.place(&tabs)
            }
            FromEngine::PageOpened { page, target } => {
                let named = self.routes().opening.remove(&page);
                if let Some(named) = named {
                    drop(named.send(target));
                }
                Vec::new()
            }
        }
    }
}

fn dialog_of(params: &Value) -> Option<Dialog> {
    let kind = match params["type"].as_str()? {
        "alert" => DialogKind::Alert,
        "confirm" => DialogKind::Confirm,
        "prompt" => DialogKind::Prompt,
        "beforeunload" => DialogKind::Beforeunload,
        _ => return None,
    };
    Some(Dialog {
        kind,
        message: params["message"].as_str()?.to_owned(),
        text: params["defaultPrompt"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
    })
}

/// Never waits for a window, so the events of the browser always drain.
async fn follow(
    engine: Arc<Engine>,
    mut events: mpsc::UnboundedReceiver<devtools::Event>,
    mut reports: mpsc::Receiver<Report>,
) {
    let mut reporting = true;
    loop {
        let deliveries = tokio::select! {
            event = events.recv() => {
                let Some(event) = event else { break };
                match event.method.as_str() {
                    "Target.attachedToTarget" => {
                        engine.attached(&event.params);
                        Vec::new()
                    }
                    "Target.detachedFromTarget" => {
                        engine.detached(&event.params);
                        Vec::new()
                    }
                    "Browser.downloadProgress" => {
                        engine.download_progress(&event.params).into_iter().collect()
                    }
                    _ => match engine.page_of(&event) {
                        Some(page) => {
                            drop(page.send(event));
                            Vec::new()
                        }
                        None => engine.page_event(event).into_iter().collect(),
                    },
                }
            }
            report = reports.recv(), if reporting => {
                if report.is_none() {
                    reporting = false;
                }
                if let Some(Report::Said(FromEngine::Tabs { tabs, .. })) = &report {
                    engine.attach_new(tabs);
                }
                report.map(|report| engine.report(report)).unwrap_or_default()
            }
        };
        for (sender, event) in deliveries {
            if let Err(mpsc::error::TrySendError::Full(_)) = sender.try_send(event) {
                tracing::debug!("a window is behind, so it misses an event of its page");
            }
        }
    }
    engine.stopped.send_replace(true);
}

fn arguments(profile: &Path, link: &Path, port: u16) -> Vec<String> {
    vec![
        format!("--kodosi-profile={}", profile.display()),
        format!("--kodosi-link={}", link.display()),
        format!("--kodosi-debug-port={port}"),
    ]
}

/// A debug port that the host picks keeps `navigator.webdriver` false.
async fn free_port() -> Result<u16, Error> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    Ok(listener.local_addr()?.port())
}

async fn wait_for_address(port: u16, process: &mut [Child]) -> Result<String, Error> {
    tokio::time::timeout(START_LIMIT, async {
        loop {
            if let Some(address) = devtools::address_on(port).await {
                return Ok(address);
            }
            if let Some(Ok(Some(status))) = process.first_mut().map(Child::try_wait) {
                return Err(Error::Engine(format!("the program ended: {status}")));
            }
            tokio::time::sleep(START_CHECK).await;
        }
    })
    .await
    .map_err(|_| Error::Engine("the debug port did not answer".to_owned()))?
}

/// Starts the program of an app bundle, or the program itself. The engine is a child of the
/// host and ends with its link.
#[cfg(target_os = "macos")]
#[expect(
    clippy::unused_async,
    reason = "the same call waits for a display on Linux"
)]
async fn launch(engine: &Path, arguments: &[String]) -> Result<Vec<Child>, Error> {
    let program = if engine
        .extension()
        .is_some_and(|extension| extension == "app")
    {
        engine.join(BUNDLE_PROGRAM)
    } else {
        engine.to_path_buf()
    };
    let child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| Error::Engine(error.to_string()))?;
    Ok(vec![child])
}

#[cfg(windows)]
#[expect(
    clippy::unused_async,
    reason = "the same call waits for a display on Linux"
)]
async fn launch(engine: &Path, arguments: &[String]) -> Result<Vec<Child>, Error> {
    let child = Command::new(engine)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| Error::Engine(error.to_string()))?;
    Ok(vec![child])
}

#[cfg(all(unix, not(target_os = "macos")))]
async fn launch(engine: &Path, arguments: &[String]) -> Result<Vec<Child>, Error> {
    let (display, number) = private_display().await?;
    let child = Command::new(engine)
        .args(arguments)
        .env("DISPLAY", format!(":{number}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| Error::Engine(error.to_string()))?;
    Ok(vec![child, display])
}

#[cfg(all(unix, not(target_os = "macos")))]
async fn private_display() -> Result<(Child, String), Error> {
    let mut display = Command::new("Xvfb")
        .args([
            "-displayfd",
            "1",
            "-screen",
            "0",
            "4096x4096x24",
            "-nolisten",
            "tcp",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| Error::Engine(format!("Xvfb did not start: {error}")))?;
    let output = display
        .stdout
        .take()
        .ok_or_else(|| Error::Engine("Xvfb gave no display".to_owned()))?;
    let mut number = String::new();
    tokio::time::timeout(START_LIMIT, BufReader::new(output).read_line(&mut number))
        .await
        .map_err(|_| Error::Engine("Xvfb gave no display".to_owned()))??;
    Ok((display, number.trim().to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(id: i64, window: i64, opener: Option<i64>, active: bool) -> EngineTab {
        EngineTab {
            id,
            window,
            opener,
            target: Some(format!("target-{id}")),
            title: String::new(),
            address: String::new(),
            active,
            loading: false,
        }
    }

    fn routes_with(windows: &[i64]) -> (Routes, mpsc::Receiver<Event>) {
        let (sender, receiver) = mpsc::channel(8);
        let mut routes = Routes::default();
        for window in windows {
            routes.windows.insert(*window, sender.clone());
        }
        (routes, receiver)
    }

    fn placed(deliveries: Vec<(mpsc::Sender<Event>, Event)>) -> (Vec<i64>, Option<i64>) {
        match deliveries.into_iter().next() {
            Some((_, Event::Tabs { tabs, active })) => {
                (tabs.iter().map(|tab| tab.id).collect(), active)
            }
            _ => (Vec::new(), None),
        }
    }

    #[test]
    fn a_popup_belongs_to_the_window_of_its_opener_and_takes_the_picture() {
        let (mut routes, _receiver) = routes_with(&[1]);
        let tabs = [tab(10, 1, None, true), tab(20, 2, Some(10), true)];
        let (tabs, active) = placed(routes.place(&tabs));
        assert_eq!(tabs, [10, 20]);
        assert_eq!(active, Some(20));
        assert_eq!(routes.targets.get("target-20"), Some(&1));
    }

    #[test]
    fn a_tab_of_nobody_stays_out() {
        let (mut routes, _receiver) = routes_with(&[1]);
        let tabs = [tab(10, 1, None, true), tab(30, 3, None, true)];
        let (tabs, active) = placed(routes.place(&tabs));
        assert_eq!(tabs, [10]);
        assert_eq!(active, Some(10));
    }

    #[test]
    fn a_circle_of_openers_ends() {
        let (mut routes, _receiver) = routes_with(&[1]);
        let tabs = [tab(10, 2, Some(20), true), tab(20, 3, Some(10), true)];
        assert_eq!(placed(routes.place(&tabs)).0, Vec::<i64>::new());
    }

    #[test]
    fn the_engine_gets_its_profile_link_and_debug_port_and_no_token() {
        let options = arguments(Path::new("/p"), Path::new("/p/link.json"), 9000);
        assert_eq!(
            options,
            [
                "--kodosi-profile=/p",
                "--kodosi-link=/p/link.json",
                "--kodosi-debug-port=9000"
            ]
        );
    }
}
