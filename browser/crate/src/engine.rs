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
use tokio::{
    net::TcpListener,
    process::{Child, Command},
    sync::{mpsc, oneshot, watch},
};

use crate::{
    Error,
    devtools::{self, Devtools},
    link::{EncodedFrame, EngineTab, FromEngine, ToEngine},
    message::Size,
    socket::{Report, Socket},
    window::Event,
};

const START_LIMIT: Duration = Duration::from_secs(30);
const START_CHECK: Duration = Duration::from_millis(50);
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
    /// The host's own pages, by their sessions.
    pages: HashMap<String, mpsc::UnboundedSender<devtools::Event>>,
    /// Host pages that the engine opens, until it names their targets.
    opening: HashMap<i64, oneshot::Sender<String>>,
    /// Tabs that a window's driver asked for, until the engine names their targets.
    asked: HashMap<i64, oneshot::Sender<String>>,
    /// Targets that the host attaches to now.
    attaching: HashSet<String>,
    /// The engine's last report of its tabs, which the windows get again once the host attached.
    last_tabs: Vec<EngineTab>,
    /// Windows that lost a frame. The frames after it need it, so a window gets none until its
    /// next key frame.
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

    /// Places each tab in its window, and tells each window its tabs and the engine's active
    /// one.
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

/// Forgets a driver's ask for a tab when its wait ends, also when the driver went first.
struct TabAsk<'a> {
    engine: &'a Engine,
    ask: i64,
}

impl Drop for TabAsk<'_> {
    fn drop(&mut self) {
        self.engine.routes().asked.remove(&self.ask);
    }
}

/// Closes a host page whose opening was dropped before it ended, so that no page stays open
/// that nobody holds.
struct PageOpening<'a> {
    engine: &'a Engine,
    page: i64,
    ended: bool,
}

impl Drop for PageOpening<'_> {
    fn drop(&mut self) {
        if !self.ended {
            self.engine.routes().opening.remove(&self.page);
            self.engine.command(ToEngine::ClosePage { page: self.page });
        }
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
    /// The browser's own debug connection.
    address: String,
    commands: mpsc::Sender<ToEngine>,
    routes: Arc<Mutex<Routes>>,
    /// Changes each time the tabs have their windows again.
    placed: watch::Sender<()>,
    stopped: watch::Sender<bool>,
    /// Ends the engine when the host lets go of it.
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
    ) -> Result<Arc<Self>, Error> {
        let folder = profiles.join(profile);
        tokio::fs::create_dir_all(&folder).await?;
        let socket = Socket::open().await?;
        let link = folder.join(LINK_FILE);
        socket.write_address(&link).await?;
        let port = free_port().await?;
        let mut process = launch(program, &arguments(&folder, &link, port))?;
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

    /// The window of a tab's target, once the engine placed the tab.
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

    /// Opens a tab in a window, which becomes its active tab, and gives the tab's target.
    pub(crate) async fn open_tab(&self, window: i64, address: &str) -> Option<String> {
        let ask = self.next_ask.fetch_add(1, Ordering::Relaxed);
        let (named, target) = oneshot::channel();
        self.routes().asked.insert(ask, named);
        let _asking = TabAsk { engine: self, ask };
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

    /// Opens a page apart from the people's tabs, with no stored data.
    pub(crate) async fn open_page(&self, width: u32, height: u32) -> Result<OpenedPage, Error> {
        let page = self.next_page.fetch_add(1, Ordering::Relaxed);
        let (named, target) = oneshot::channel();
        self.routes().opening.insert(page, named);
        let mut opening = PageOpening {
            engine: self,
            page,
            ended: false,
        };
        self.command(ToEngine::OpenPage {
            page,
            width,
            height,
        });
        let target = tokio::time::timeout(ATTACH_LIMIT, target).await;
        let Ok(Ok(target)) = target else {
            opening.ended = true;
            self.routes().opening.remove(&page);
            self.close_page(page, None).await;
            return Err(Error::Engine("the page did not open".to_owned()));
        };
        let Some(session) = self.attach(&target).await else {
            opening.ended = true;
            self.close_page(page, Some(&target)).await;
            return Err(Error::Engine("the page did not attach".to_owned()));
        };
        opening.ended = true;
        let (sender, events) = mpsc::unbounded_channel();
        self.routes().pages.insert(session.clone(), sender);
        Ok(OpenedPage {
            page,
            target,
            session,
            events,
        })
    }

    /// The host attaches to each new target itself, and the windows get their tabs again with
    /// its session. CEF's browser-wide auto-attach misses the pages that the engine opens and
    /// holds back the popups that a page opens.
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

    /// Closes a host page now; a dropped page has no time to wait for the browser to stop.
    pub(crate) fn forget_page(&self, page: i64, target: Option<&str>) {
        if let Some(session) = target.and_then(|target| self.session_of(target)) {
            self.routes().pages.remove(&session);
        }
        self.command(ToEngine::ClosePage { page });
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
            routes.behind.remove(&window);
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
        let sender = {
            let mut routes = self.routes();
            if frame.key {
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

async fn wait_for_address(port: u16, process: &mut Child) -> Result<String, Error> {
    tokio::time::timeout(START_LIMIT, async {
        loop {
            if let Some(address) = devtools::address_on(port).await {
                return Ok(address);
            }
            if let Ok(Some(status)) = process.try_wait() {
                return Err(Error::Engine(ended(status)));
            }
            tokio::time::sleep(START_CHECK).await;
        }
    })
    .await
    .map_err(|_| Error::Engine("the debug port did not answer".to_owned()))?
}

fn ended(status: std::process::ExitStatus) -> String {
    if status.code() == Some(crate::link::AS_ROOT) {
        "it does not run as root, because Chromium's sandbox needs a normal user. Run Kodosi as a normal user.".to_owned()
    } else {
        format!("the program ended: {status}")
    }
}

/// Reaps the engine when it ends, and ends it when the host lets go of it first.
fn keep(mut process: Child, released: oneshot::Receiver<()>) {
    tokio::spawn(async move {
        tokio::select! {
            _ = process.wait() => {}
            _ = released => {}
        }
    });
}

/// Starts the program of an app bundle, or the program itself. The engine is a child of the
/// host and ends with its link.
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
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| Error::Engine(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(id: i64, window: i64, active: bool) -> EngineTab {
        EngineTab {
            id,
            window,
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
    fn a_window_gets_its_tabs_and_the_engines_active_one() {
        let (mut routes, _receiver) = routes_with(&[1]);
        let tabs = [tab(10, 1, false), tab(20, 1, true)];
        let (tabs, active) = placed(routes.place(&tabs));
        assert_eq!(tabs, [10, 20]);
        assert_eq!(active, Some(20));
        assert_eq!(routes.targets.get("target-20"), Some(&1));
    }

    #[test]
    fn a_tab_of_nobody_stays_out() {
        let (mut routes, _receiver) = routes_with(&[1]);
        let tabs = [tab(10, 1, true), tab(30, 3, true)];
        let (tabs, active) = placed(routes.place(&tabs));
        assert_eq!(tabs, [10]);
        assert_eq!(active, Some(10));
        assert!(!routes.tabs.contains_key(&30));
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
