use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use serde_json::{Value, json};
#[cfg(all(unix, not(target_os = "macos")))]
use tokio::io::{AsyncBufReadExt as _, BufReader};
use tokio::{
    net::TcpListener,
    process::{Child, Command},
    sync::{mpsc, watch},
};

use crate::{
    Error, Settings,
    devtools::{self, Devtools},
    extension::{self, FromExtension, ToExtension},
    message::{Dialog, DialogKind},
    window::Event,
};

const START_LIMIT: Duration = Duration::from_secs(30);
const START_CHECK: Duration = Duration::from_millis(50);
const COMMANDS: usize = 256;
const STATE_FILE: &str = "engine-port";

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
    fn place(&mut self, tabs: &[extension::ReportedTab]) -> Vec<(mpsc::Sender<Event>, Event)> {
        let openers: HashMap<i64, Option<i64>> =
            tabs.iter().map(|tab| (tab.id, tab.opener)).collect();
        let windows_of_tabs: HashMap<i64, i64> =
            tabs.iter().map(|tab| (tab.id, tab.window)).collect();
        let owner = |tab: &extension::ReportedTab| {
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
        let mut placed: HashMap<i64, Vec<extension::ReportedTab>> = HashMap::new();
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

pub(crate) struct Engine {
    devtools: Devtools,
    pub port: u16,
    commands: mpsc::Sender<ToExtension>,
    routes: Arc<Mutex<Routes>>,
    stopped: watch::Sender<bool>,
    _process: Vec<Child>,
    downloads: PathBuf,
}

impl Engine {
    pub(crate) async fn start(settings: &Settings, profile: &str) -> Result<Arc<Self>, Error> {
        let folder = settings.profiles.join(profile);
        tokio::fs::create_dir_all(&folder).await?;
        close_left(&folder).await;
        let socket = extension::Socket::open().await?;
        let unpacked = folder.join("extension");
        extension::prepare(&unpacked, &socket).await?;
        let port = free_port().await?;
        let mut process = launch(&settings.engine, &arguments(&folder, &unpacked, port)).await?;
        tokio::fs::write(folder.join(STATE_FILE), port.to_string()).await?;
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
                tracing::warn!(%problem, "the browser's extension is gone");
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
        });
        tokio::spawn(follow(Arc::clone(&engine), events, reports));
        Ok(engine)
    }

    pub(crate) fn devtools(&self) -> &Devtools {
        &self.devtools
    }

    pub(crate) fn command(&self, command: ToExtension) {
        if self.commands.try_send(command).is_err() {
            tracing::debug!("the extension's queue is full");
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

    /// Opens a window with its first page, in the background, and returns its id.
    pub(crate) async fn open_window(
        &self,
        address: &str,
        width: u32,
        height: u32,
        events: mpsc::Sender<Event>,
    ) -> Result<i64, Error> {
        let created = self
            .devtools
            .call(
                "Target.createTarget",
                json!({
                    "url": address,
                    "newWindow": true,
                    "background": true,
                    "width": width,
                    "height": height,
                }),
                None,
            )
            .await?;
        let target = created["targetId"]
            .as_str()
            .ok_or_else(|| Error::Engine("no new window".to_owned()))?;
        let found = self
            .devtools
            .call(
                "Browser.getWindowForTarget",
                json!({ "targetId": target }),
                None,
            )
            .await?;
        let window = found["windowId"]
            .as_i64()
            .ok_or_else(|| Error::Engine("no new window".to_owned()))?;
        self.routes().windows.insert(window, events);
        self.command(ToExtension::Report);
        Ok(window)
    }

    /// Stops the browser when its last window closed.
    pub(crate) async fn window_closed(&self, window: i64) {
        let last = {
            let mut routes = self.routes();
            routes.windows.remove(&window);
            routes.windows.is_empty()
        };
        if last {
            self.stop().await;
        }
    }

    /// Stops a browser that started only for work with no window.
    pub(crate) async fn stop_if_idle(&self) {
        let idle = self.routes().windows.is_empty();
        if idle {
            self.stop().await;
        }
    }

    pub(crate) async fn stop(&self) {
        drop(self.devtools.call("Browser.close", json!({}), None).await);
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

    fn report(&self, report: FromExtension) -> Vec<(mpsc::Sender<Event>, Event)> {
        let mut routes = self.routes();
        let to_tab = |tab: i64, event: Event| {
            routes
                .sender_of_tab(tab)
                .map(|sender| vec![(sender, event)])
                .unwrap_or_default()
        };
        match report {
            FromExtension::Frame(frame) => routes
                .windows
                .get(&frame.stream)
                .cloned()
                .map(|sender| vec![(sender, Event::Frame(frame))])
                .unwrap_or_default(),
            FromExtension::Failed { stream, problem } => {
                tracing::warn!(%problem, "the capture stopped");
                routes
                    .windows
                    .get(&stream)
                    .cloned()
                    .map(|sender| vec![(sender, Event::CaptureFailed)])
                    .unwrap_or_default()
            }
            FromExtension::Shape { tab, shape } => to_tab(tab, Event::Shape(shape)),
            FromExtension::Menu {
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
            FromExtension::Context {
                tab,
                selection,
                link,
            } => to_tab(tab, Event::Context { selection, link }),
            FromExtension::Copied { tab, text } => to_tab(tab, Event::Copied(text)),
            FromExtension::Tabs { tabs, .. } => routes.place(&tabs),
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

async fn follow(
    engine: Arc<Engine>,
    mut events: mpsc::Receiver<devtools::Event>,
    mut reports: mpsc::Receiver<FromExtension>,
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
                    _ => engine.page_event(event).into_iter().collect(),
                }
            }
            report = reports.recv(), if reporting => {
                if report.is_none() {
                    reporting = false;
                }
                report.map(|report| engine.report(report)).unwrap_or_default()
            }
        };
        for (sender, event) in deliveries {
            drop(sender.send(event).await);
        }
    }
    engine.stopped.send_replace(true);
}

fn arguments(profile: &Path, extension: &Path, port: u16) -> Vec<String> {
    vec![
        format!("--user-data-dir={}", profile.display()),
        format!("--load-extension={}", extension.display()),
        format!("--allowlisted-extension-id={}", extension::ID),
        format!("--remote-debugging-port={port}"),
        "--no-startup-window".to_owned(),
        "--no-first-run".to_owned(),
        "--no-default-browser-check".to_owned(),
        "--mute-audio".to_owned(),
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

/// A browser that outlived an earlier host still holds the profile, so it closes first.
async fn close_left(profile: &Path) {
    let Some(port) = tokio::fs::read_to_string(profile.join(STATE_FILE))
        .await
        .ok()
        .and_then(|port| port.trim().parse::<u16>().ok())
    else {
        return;
    };
    let Some(address) = devtools::address_on(port).await else {
        return;
    };
    if let Ok((devtools, _events)) = Devtools::connect(&address).await {
        drop(devtools.call("Browser.close", json!({}), None).await);
        while devtools::address_on(port).await.is_some() {
            tokio::time::sleep(START_CHECK).await;
        }
    }
}

#[cfg(target_os = "macos")]
async fn launch(engine: &Path, arguments: &[String]) -> Result<Vec<Child>, Error> {
    let mut command = if engine
        .extension()
        .is_some_and(|extension| extension == "app")
    {
        let mut command = Command::new("/usr/bin/open");
        command
            .args(["-g", "-j", "-n", "-a"])
            .arg(engine)
            .arg("--args");
        command
    } else {
        Command::new(engine)
    };
    let mut child = command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| Error::Engine(error.to_string()))?;
    if engine
        .extension()
        .is_some_and(|extension| extension == "app")
    {
        child.wait().await?;
        return Ok(Vec::new());
    }
    Ok(vec![child])
}

#[cfg(windows)]
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
    use crate::extension::ReportedTab;

    fn tab(id: i64, window: i64, opener: Option<i64>, active: bool) -> ReportedTab {
        ReportedTab {
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
    fn the_start_options_keep_the_browser_out_of_sight() {
        let options = arguments(Path::new("/p"), Path::new("/e"), 9000);
        assert!(options.contains(&"--no-startup-window".to_owned()));
        assert!(options.contains(&"--remote-debugging-port=9000".to_owned()));
        assert!(options.contains(&format!("--allowlisted-extension-id={}", extension::ID)));
    }
}
