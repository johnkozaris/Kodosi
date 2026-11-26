use std::{
    collections::{HashMap, HashSet},
    mem::DropGuard,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};

use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt as _, BufReader},
    net::TcpListener,
    process::{Child, Command},
    sync::{mpsc, oneshot, watch},
};

use crate::{
    Error, Privacy,
    devtools::{self, Devtools},
    link::{EncodedFrame, EngineTab, FrameKind, FromEngine, ToEngine},
    message::Size,
    socket::{Report, Socket},
    window::Event,
};

const START_LIMIT: Duration = Duration::from_secs(30);
const ATTACH_LIMIT: Duration = Duration::from_secs(5);
const COMMANDS: usize = 256;
const QUIT_LIMIT: Duration = Duration::from_secs(5);
const LINK_FILE: &str = "link.json";
const BUNDLE_PROGRAM: &str = "Contents/MacOS/Kodosi Browser";

#[derive(Default)]
pub(crate) struct Routes {
    windows: HashMap<i64, mpsc::Sender<Event>>,
    tabs: HashMap<i64, i64>,
    targets: HashMap<String, i64>,
    sessions: HashMap<String, String>,
    session_of_target: HashMap<String, String>,
    downloads: HashMap<String, i64>,
    pages: HashMap<String, mpsc::UnboundedSender<devtools::Event>>,
    /// Pending host page opens waiting for their CDP target ID.
    opening: HashMap<i64, oneshot::Sender<String>>,
    /// Pending driver-initiated tab opens waiting for their CDP target ID.
    asked: HashMap<i64, oneshot::Sender<String>>,
    attaching: HashSet<String>,
    /// Most recent tab snapshot from the engine, re-dispatched once new targets attach.
    last_tabs: Vec<EngineTab>,
    /// Windows that dropped a frame and must skip delta frames until the next keyframe.
    behind: HashSet<i64>,
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

    fn place(&mut self, tabs: &[EngineTab]) -> Vec<(mpsc::Sender<Event>, Event)> {
        let mut placed: HashMap<i64, Vec<EngineTab>> = HashMap::new();
        self.tabs.clear();
        self.targets.clear();
        for tab in tabs
            .iter()
            .filter(|tab| self.windows.contains_key(&tab.window))
        {
            self.tabs.insert(tab.id, tab.window);
            if let Some(target) = &tab.target {
                self.targets.insert(target.clone(), tab.window);
            }
            placed.entry(tab.window).or_default().push(tab.clone());
        }
        self.windows
            .iter()
            .map(|(window, sender)| {
                let tabs = placed.remove(window).unwrap_or_default();
                let active = tabs.iter().find(|tab| tab.active).map(|tab| tab.id);
                (sender.clone(), Event::Tabs { tabs, active })
            })
            .collect()
    }
}

pub(crate) struct OpenedPage {
    pub page: i64,
    pub target: String,
    pub session: String,
    pub events: mpsc::UnboundedReceiver<devtools::Event>,
}

pub(crate) struct Engine {
    devtools: Devtools,
    address: String,
    commands: mpsc::Sender<ToEngine>,
    routes: Arc<Mutex<Routes>>,
    /// Notified whenever tab-to-window mappings are updated.
    placed: watch::Sender<()>,
    stopped: watch::Sender<bool>,
    /// Dropping this sender releases the engine process task, killing the child.
    _kept: oneshot::Sender<()>,
    downloads: PathBuf,
    next_window: AtomicI64,
    next_page: AtomicI64,
    next_ask: AtomicI64,
}

impl Engine {
    pub(crate) async fn start(
        program: &Path,
        profiles: &Path,
        profile: &str,
        system_keychain: bool,
        privacy: Privacy,
    ) -> Result<Arc<Self>, Error> {
        let folder = profiles.join(profile);
        tokio::fs::create_dir_all(&folder).await?;
        let socket = Socket::open().await?;
        let link = folder.join(LINK_FILE);
        socket.write_address(&link).await?;
        let port = free_port().await?;
        let mut process = launch(
            program,
            &arguments(&folder, &link, port, system_keychain, privacy),
        )?;
        let address = wait_for_address(port, &mut process).await?;
        let (kept, released) = oneshot::channel();
        keep(process, released);
        let (devtools, events) = Devtools::connect(&address).await?;
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
            address,
            commands,
            routes: Arc::default(),
            placed: watch::Sender::new(()),
            stopped: watch::Sender::new(false),
            _kept: kept,
            downloads,
            next_window: AtomicI64::new(1),
            next_page: AtomicI64::new(1),
            next_ask: AtomicI64::new(1),
        });
        tokio::spawn(follow(Arc::clone(&engine), events, reports));
        Ok(engine)
    }

    #[cfg(test)]
    pub(crate) fn idle() -> Arc<Self> {
        Arc::new(Self {
            devtools: Devtools::idle(),
            address: String::new(),
            commands: mpsc::channel(COMMANDS).0,
            routes: Arc::default(),
            placed: watch::Sender::new(()),
            stopped: watch::Sender::new(false),
            _kept: oneshot::channel().0,
            downloads: PathBuf::new(),
            next_window: AtomicI64::new(1),
            next_page: AtomicI64::new(1),
            next_ask: AtomicI64::new(1),
        })
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

    pub(crate) fn address(&self) -> &str {
        &self.address
    }

    pub(crate) fn placed(&self) -> watch::Receiver<()> {
        self.placed.subscribe()
    }

    pub(crate) fn window_of(&self, target: &str) -> Option<i64> {
        self.routes().targets.get(target).copied()
    }

    pub(crate) fn tab_of(&self, target: &str) -> Option<i64> {
        self.routes()
            .last_tabs
            .iter()
            .find(|tab| tab.target.as_deref() == Some(target))
            .map(|tab| tab.id)
    }

    pub(crate) fn active_target(&self, window: i64) -> Option<String> {
        self.routes()
            .last_tabs
            .iter()
            .find(|tab| tab.window == window && tab.active)
            .and_then(|tab| tab.target.clone())
    }

    pub(crate) async fn open_tab(&self, window: i64, address: &str) -> Option<String> {
        let ask = self.next_ask.fetch_add(1, Ordering::Relaxed);
        let (named, target) = oneshot::channel();
        self.routes().asked.insert(ask, named);
        // Remove the pending entry if the driver disconnects and cancels this future.
        let _asking = DropGuard::new(ask, |ask| {
            self.routes().asked.remove(&ask);
        });
        self.command(ToEngine::NewTab {
            window,
            address: address.to_owned(),
            ask: Some(ask),
        });
        tokio::time::timeout(ATTACH_LIMIT, target).await.ok()?.ok()
    }

    fn routes(&self) -> std::sync::MutexGuard<'_, Routes> {
        self.routes.lock().unwrap_or_else(PoisonError::into_inner)
    }

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

    pub(crate) async fn open_page(&self, width: u32, height: u32) -> Result<OpenedPage, Error> {
        let page = self.next_page.fetch_add(1, Ordering::Relaxed);
        let (named, target) = oneshot::channel();
        self.routes().opening.insert(page, named);
        // Close the page in the engine if this future is cancelled before returning.
        let opening = DropGuard::new(page, |page| {
            self.routes().opening.remove(&page);
            self.command(ToEngine::ClosePage { page });
        });
        self.command(ToEngine::OpenPage {
            page,
            width,
            height,
        });
        let target = tokio::time::timeout(ATTACH_LIMIT, target).await;
        let Ok(Ok(target)) = target else {
            drop(opening);
            self.stop_if_idle().await;
            return Err(Error::Engine("the page did not open".to_owned()));
        };
        let Some(session) = self.attach(&target).await else {
            DropGuard::dismiss(opening);
            self.close_page(page, Some(&target)).await;
            return Err(Error::Engine("the page did not attach".to_owned()));
        };
        DropGuard::dismiss(opening);
        let (sender, events) = mpsc::unbounded_channel();
        self.routes().pages.insert(session.clone(), sender);
        Ok(OpenedPage {
            page,
            target,
            session,
            events,
        })
    }

    /// Attaches to newly reported targets manually because CEF's browser-wide auto-attach
    /// misses engine-opened pages and pauses popups.
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
        self.placed.send_replace(());
        let session = session?;
        self.attached(&json!({ "sessionId": session, "targetInfo": { "targetId": target } }));
        for (sender, event) in deliveries {
            drop(sender.try_send(event));
        }
        Some(session)
    }

    pub(crate) async fn close_page(&self, page: i64, target: Option<&str>) {
        self.forget_page(page, target);
        self.stop_if_idle().await;
    }

    /// Synchronous teardown for `HostPage::drop`, which cannot await `stop_if_idle`.
    pub(crate) fn forget_page(&self, page: i64, target: Option<&str>) {
        if let Some(session) = target.and_then(|target| self.session_of(target)) {
            self.routes().pages.remove(&session);
        }
        self.command(ToEngine::ClosePage { page });
    }

    fn page_of(&self, event: &devtools::Event) -> Option<mpsc::UnboundedSender<devtools::Event>> {
        self.routes().pages.get(event.session.as_deref()?).cloned()
    }

    pub(crate) async fn window_closed(&self, window: i64) {
        let last = {
            let mut routes = self.routes();
            routes.windows.remove(&window);
            routes.behind.remove(&window);
            routes.windows.is_empty() && routes.pages.is_empty()
        };
        if last {
            self.stop().await;
        }
    }

    pub(crate) async fn stop_if_idle(&self) {
        let idle = {
            let routes = self.routes();
            routes.windows.is_empty() && routes.pages.is_empty()
        };
        if idle {
            self.stop().await;
        }
    }

    /// Requests a clean quit and waits for exit so the profile directory is unlocked.
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

    /// Drops frames for slow windows until the next keyframe so one window cannot stall
    /// others or block engine reports.
    fn deliver_frame(&self, frame: EncodedFrame) {
        let window = frame.window;
        let sender = {
            let mut routes = self.routes();
            if frame.kind == FrameKind::Key {
                routes.behind.remove(&window);
            } else if routes.behind.contains(&window) {
                return;
            }
            routes.windows.get(&window).cloned()
        };
        let Some(sender) = sender else {
            return;
        };
        if sender.try_send(Event::Frame(frame)).is_err() && self.routes().behind.insert(window) {
            tracing::debug!(window, "a window is behind, so it waits for a key frame");
            self.command(ToEngine::Keyframe { window });
        }
    }

    fn to_window(&self, window: i64, event: Event) -> Vec<(mpsc::Sender<Event>, Event)> {
        let sender = self.routes().windows.get(&window).cloned();
        sender.map_or_default(|sender| vec![(sender, event)])
    }

    fn report(&self, report: Report) -> Vec<(mpsc::Sender<Event>, Event)> {
        let said = match report {
            Report::Frame(frame) if frame.still => {
                return self.to_window(frame.window, Event::Still(frame.data));
            }
            Report::Frame(frame) => {
                self.deliver_frame(frame);
                return Vec::new();
            }
            Report::Said(said) => said,
        };
        let to_tab = |tab: i64, event: Event| {
            self.routes()
                .sender_of_tab(tab)
                .map_or_default(|sender| vec![(sender, event)])
        };
        match said {
            FromEngine::Failed { window, problem } => {
                tracing::warn!(%problem, "the capture stopped");
                self.to_window(window, Event::CaptureFailed)
            }
            FromEngine::Layer { window, context } => self.to_window(window, Event::Layer(context)),
            FromEngine::NoLayer { window, problem } => {
                tracing::warn!(%problem, "the browser's views take video");
                self.to_window(window, Event::NoLayer)
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
            FromEngine::Icon { tab, png } => to_tab(tab, Event::Icon { tab, png }),
            FromEngine::Dialog { tab, dialog } => to_tab(tab, Event::Dialog { tab, dialog }),
            FromEngine::DialogClosed { tab } => to_tab(tab, Event::DialogClosed { tab }),
            FromEngine::Tabs { tabs, .. } => {
                let deliveries = {
                    let mut routes = self.routes();
                    routes.last_tabs.clone_from(&tabs);
                    routes.place(&tabs)
                };
                self.placed.send_replace(());
                deliveries
            }
            FromEngine::PageOpened { page, target } => {
                let named = self.routes().opening.remove(&page);
                if let Some(named) = named {
                    drop(named.send(target));
                }
                Vec::new()
            }
            FromEngine::TabOpened { ask, target } => {
                let named = self.routes().asked.remove(&ask);
                if let Some(named) = named {
                    drop(named.send(target));
                }
                Vec::new()
            }
        }
    }
}

/// Dispatches engine and `DevTools` events without blocking on windows so incoming streams drain.
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
                report.map_or_default(|report| engine.report(report))
            }
        };
        for (sender, event) in deliveries {
            deliver(sender, event);
        }
    }
    engine.stopped.send_replace(true);
}

fn deliver(sender: mpsc::Sender<Event>, event: Event) {
    match sender.try_send(event) {
        Err(mpsc::error::TrySendError::Full(event)) if event.must_arrive() => {
            tokio::spawn(async move { drop(sender.send(event).await) });
        }
        Err(mpsc::error::TrySendError::Full(_)) => {
            tracing::debug!("a window is behind, so it misses an event of its page");
        }
        _ => {}
    }
}

fn arguments(
    profile: &Path,
    link: &Path,
    port: u16,
    system_keychain: bool,
    privacy: Privacy,
) -> Vec<String> {
    let mut arguments = vec![
        format!("--kodosi-profile={}", profile.display()),
        format!("--kodosi-link={}", link.display()),
        format!("--kodosi-debug-port={port}"),
        format!("--disable-features={}", features_off(privacy).join(",")),
    ];
    if !privacy.cross_site_cookies {
        arguments.push("--enable-features=ForceThirdPartyCookieBlockingEnabled".to_owned());
    }
    if !system_keychain {
        arguments.extend([
            "--use-mock-keychain".to_owned(),
            "--password-store=basic".to_owned(),
        ]);
    }
    arguments
}

/// Chromium features that call Google by themselves. `AimEnabled` asks Google, with its cookies,
/// whether Chromium's own address bar may offer AI Mode, and `PreconnectToSearch` connects to the
/// search engine early. Kodosi Browser has no such address bar, and its windowless pages never
/// count as being in front, which the early connection waits for.
fn features_off(privacy: Privacy) -> Vec<&'static str> {
    let mut features = vec!["AimEnabled", "PreconnectToSearch"];
    if !privacy.form_predictions {
        features.push("AutofillServerCommunication");
    }
    features
}

/// Passing a host-chosen debug port keeps `navigator.webdriver` false.
async fn free_port() -> Result<u16, Error> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    Ok(listener.local_addr()?.port())
}

/// Reads the `DevTools` URL from stderr and continues draining stderr so the engine never blocks
/// on a full pipe.
async fn wait_for_address(port: u16, process: &mut Child) -> Result<String, Error> {
    let output = process
        .stderr
        .take()
        .ok_or_else(|| Error::Engine("the program has no error output".to_owned()))?;
    let mut output = BufReader::new(output);
    let address = tokio::time::timeout(START_LIMIT, async {
        let mut line = Vec::new();
        while output.read_until(b'\n', &mut line).await? > 0 {
            if let Some(address) = debug_address(&String::from_utf8_lossy(&line), port) {
                return Ok(address);
            }
            line.clear();
        }
        Err(Error::Engine(ended(process.wait().await?)))
    })
    .await
    .map_err(|_| Error::Engine("the debug port did not answer".to_owned()))??;
    tokio::spawn(async move { tokio::io::copy(&mut output, &mut tokio::io::sink()).await });
    Ok(address)
}

/// Parses the `DevTools` URL from the engine's own stderr rather than probing the port, preventing
/// hijacking if another process binds the port first. Accepts `::1` for IPv6 fallback.
fn debug_address(line: &str, port: u16) -> Option<String> {
    let address = line.trim().strip_prefix("DevTools listening on ")?;
    let browser = ["127.0.0.1", "[::1]"]
        .into_iter()
        .find_map(|host| address.strip_prefix(&format!("ws://{host}:{port}/devtools/browser/")))?;
    (!browser.is_empty()
        && browser
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'))
    .then(|| address.to_owned())
}

fn ended(status: std::process::ExitStatus) -> String {
    if status.code() == Some(crate::link::AS_ROOT) {
        "it does not run as root, because Chromium's sandbox needs a normal user. Run Kodosi as a normal user.".to_owned()
    } else {
        format!("the program ended: {status}")
    }
}

fn keep(mut process: Child, released: oneshot::Receiver<()>) {
    tokio::spawn(async move {
        tokio::select! {
            _ = process.wait() => {}
            _ = released => {}
        }
    });
}

fn launch(engine: &Path, arguments: &[String]) -> Result<Child, Error> {
    let program = if cfg!(target_os = "macos")
        && engine
            .extension()
            .is_some_and(|extension| extension == "app")
    {
        engine.join(BUNDLE_PROGRAM)
    } else {
        engine.to_path_buf()
    };
    Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| Error::Engine(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_report_that_the_engine_says_once_reaches_a_window_that_is_behind() {
        let (sender, mut receiver) = mpsc::channel(1);
        deliver(sender.clone(), Event::Answered);
        deliver(sender.clone(), Event::Shape("pointer".to_owned()));
        deliver(sender, Event::Layer(7));
        let first = tokio::time::timeout(Duration::from_secs(5), receiver.recv()).await;
        assert!(matches!(first, Ok(Some(Event::Answered))));
        let second = tokio::time::timeout(Duration::from_secs(5), receiver.recv()).await;
        assert!(matches!(second, Ok(Some(Event::Layer(7)))));
    }

    #[test]
    fn only_the_engine_line_for_its_own_port_gives_the_debug_address() {
        let address = "ws://127.0.0.1:34387/devtools/browser/e81b9d87-9ad6-4c2b-8509-7f09dc467d48";
        let line = format!("DevTools listening on {address}\n");
        assert_eq!(debug_address(&line, 34387).as_deref(), Some(address));
        assert_eq!(debug_address(&line, 34388), None);
        let fallback = "ws://[::1]:34387/devtools/browser/79590703-47df-4aaf-8c11-287032c0d5e2";
        assert_eq!(
            debug_address(&format!("DevTools listening on {fallback}"), 34387).as_deref(),
            Some(fallback)
        );
        for other in [
            "DevTools listening on ws://10.0.0.1:34387/devtools/browser/e81b9d87",
            "DevTools listening on ws://127.0.0.1:34387/devtools/page/e81b9d87",
            "DevTools listening on ws://127.0.0.1:34387/devtools/browser/e81b9d87?x=1",
            "DevTools listening on ws://127.0.0.1:34387/devtools/browser/",
            "[WARNING] ws://127.0.0.1:34387/devtools/browser/e81b9d87",
        ] {
            assert_eq!(debug_address(other, 34387), None, "{other}");
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_line_that_is_not_text_does_not_hide_the_debug_address() {
        let address = "ws://127.0.0.1:34387/devtools/browser/e81b9d87-9ad6-4c2b-8509-7f09dc467d48";
        let mut process = Command::new("sh")
            .arg("-c")
            .arg(format!(
                "printf 'warning \\377\\n' >&2; echo 'DevTools listening on {address}' >&2; sleep 5"
            ))
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        assert_eq!(
            wait_for_address(34387, &mut process).await.unwrap(),
            address
        );
    }

    #[test]
    fn the_engine_gets_its_profile_link_and_debug_port_and_no_token() {
        let options = arguments(
            Path::new("/p"),
            Path::new("/p/link.json"),
            9000,
            true,
            Privacy::default(),
        );
        assert_eq!(
            options,
            [
                "--kodosi-profile=/p",
                "--kodosi-link=/p/link.json",
                "--kodosi-debug-port=9000",
                "--disable-features=AimEnabled,PreconnectToSearch,AutofillServerCommunication",
                "--enable-features=ForceThirdPartyCookieBlockingEnabled"
            ]
        );
    }

    #[test]
    fn each_choice_that_a_person_turns_on_comes_back_alone() {
        let options = |privacy| {
            arguments(
                Path::new("/p"),
                Path::new("/p/link.json"),
                9000,
                true,
                privacy,
            )[3..]
                .to_vec()
        };
        let predictions = Privacy {
            form_predictions: true,
            ..Privacy::default()
        };
        let cookies = Privacy {
            cross_site_cookies: true,
            ..Privacy::default()
        };
        assert_eq!(
            options(predictions),
            [
                "--disable-features=AimEnabled,PreconnectToSearch",
                "--enable-features=ForceThirdPartyCookieBlockingEnabled"
            ]
        );
        assert_eq!(
            options(cookies),
            ["--disable-features=AimEnabled,PreconnectToSearch,AutofillServerCommunication"]
        );
    }

    #[test]
    fn a_profile_that_is_thrown_away_keeps_its_key_out_of_the_system_keychain() {
        let options = arguments(
            Path::new("/p"),
            Path::new("/p/link.json"),
            9000,
            false,
            Privacy::default(),
        );
        assert!(options.ends_with(&[
            "--use-mock-keychain".to_owned(),
            "--password-store=basic".to_owned()
        ]));
    }
}
