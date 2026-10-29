use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use tokio::{sync::watch, time::Instant};
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    BrowserAct, BrowserAction, BrowserFrame, BrowserInput, BrowserState, BrowserTab, CanvasAnswer,
    CanvasBrowser, KeyEvent, files::Stamp,
};

const COMMAND_LIMIT: Duration = Duration::from_mins(2);
const QUICK_LIMIT: Duration = Duration::from_secs(30);
const SLEEP_AFTER: Duration = Duration::from_mins(10);
const SLEEP_CHECK: Duration = Duration::from_secs(30);
const PICTURE_WAIT: Duration = Duration::from_secs(8);
const WATCH_KEPT: Duration = Duration::from_secs(20);
const MOST_OUTPUT: usize = 1024 * 1024;
const FIRST_SIZE: (u32, u32) = (1280, 720);
const SIZES: std::ops::RangeInclusive<u32> = 200..=4096;
const DAEMON_IDLE_MS: &str = "1200000";
const NO_CHROMIUM: &str = "There is no Chrome, Chromium, Edge or Brave on this computer. Install one of them for the shared browser.";
const NO_AGENT_BROWSER: &str = "This Kodosi has no agent-browser for the shared browser.";
const ASLEEP: &str = "The browser sleeps. Open it again.";
/// Options and commands that would give the browser of this terminal another session, profile,
/// program or lifetime.
const NOT_HERE: &[&str] = &[
    "--cdp",
    "--session",
    "--session-name",
    "--profile",
    "--executable-path",
    "--config",
    "--auto-connect",
    "--namespace",
    "--provider",
    "-p",
    "--headed",
    "--debug",
    "--idle-timeout",
    "--default-timeout",
    "--action-policy",
    "--confirm-actions",
    "--confirm-interactive",
    "--no-auto-dialog",
    "connect",
    "install",
    "upgrade",
    "stream",
    "dashboard",
    "mcp",
    "plugin",
    "doctor",
];
const LOOKS: &[&str] = &[
    "snapshot",
    "get",
    "screenshot",
    "is",
    "wait",
    "console",
    "errors",
    "pdf",
    "find",
    "session",
];

pub(super) struct Places {
    binary: PathBuf,
    pub profile: PathBuf,
    sockets: PathBuf,
    config: PathBuf,
}

impl Places {
    pub(super) fn of(data_root: &Path, id: Uuid) -> Self {
        let root = data_root.join("browser");
        Self {
            binary: agent_browser(),
            profile: root.join("profiles").join(id.to_string()),
            sockets: sockets(data_root),
            config: root.join("agent-browser.json"),
        }
    }
}

/// The agent-browser that ships next to the kodosi program.
pub(super) fn agent_browser() -> PathBuf {
    let name = if cfg!(windows) {
        "agent-browser.exe"
    } else {
        "agent-browser"
    };
    std::env::current_exe()
        .ok()
        .and_then(|program| program.parent().map(|folder| folder.join(name)))
        .unwrap_or_else(|| PathBuf::from(name))
}

/// The folder of the browsers' sockets. A Unix socket path has at most 103 bytes, so it is the
/// short private folder of the host's own socket.
pub(super) fn sockets(data_root: &Path) -> PathBuf {
    #[cfg(unix)]
    if let Ok(folder) = kodosi_os::ipc::private_dir(&crate::local_host::namespace(data_root)) {
        return folder.join("browser");
    }
    data_root.join("browser").join("sockets")
}

/// The Chromium-family browser of this computer that the shared browser runs.
/// Why this computer cannot run the shared browser, if it cannot.
pub(super) fn missing() -> Option<&'static str> {
    if !agent_browser().is_file() {
        Some(NO_AGENT_BROWSER)
    } else if chromium().is_none() {
        Some(NO_CHROMIUM)
    } else {
        None
    }
}

pub(super) fn chromium() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let apps = [
            "Google Chrome.app/Contents/MacOS/Google Chrome",
            "Chromium.app/Contents/MacOS/Chromium",
            "Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "Brave Browser.app/Contents/MacOS/Brave Browser",
        ];
        let folders = [
            Some(PathBuf::from("/Applications")),
            kodosi_os::fs::home_dir()
                .ok()
                .map(|home| home.join("Applications")),
        ];
        apps.iter()
            .flat_map(|app| folders.iter().flatten().map(move |folder| folder.join(app)))
            .find(|path| path.is_file())
    }
    #[cfg(target_os = "linux")]
    {
        [
            "google-chrome",
            "google-chrome-stable",
            "chromium",
            "chromium-browser",
            "microsoft-edge",
            "microsoft-edge-stable",
            "brave-browser",
        ]
        .iter()
        .find_map(|name| kodosi_os::process::find_executable(name))
    }
    #[cfg(windows)]
    {
        let under = |variable: &str, rest: &str| {
            std::env::var_os(variable).map(|folder| PathBuf::from(folder).join(rest))
        };
        [
            under("ProgramFiles", r"Google\Chrome\Application\chrome.exe"),
            under("ProgramFiles(x86)", r"Google\Chrome\Application\chrome.exe"),
            under("LOCALAPPDATA", r"Google\Chrome\Application\chrome.exe"),
            under(
                "ProgramFiles(x86)",
                r"Microsoft\Edge\Application\msedge.exe",
            ),
            under("ProgramFiles", r"Microsoft\Edge\Application\msedge.exe"),
            under(
                "ProgramFiles",
                r"BraveSoftware\Brave-Browser\Application\brave.exe",
            ),
        ]
        .into_iter()
        .flatten()
        .find(|path| path.is_file())
    }
}

#[derive(Clone)]
pub(super) struct Picture {
    sequence: u64,
    pub width: u32,
    pub height: u32,
    data: Arc<str>,
}

#[derive(Default)]
struct Activity {
    command_at: Option<Instant>,
    used_at: Option<Instant>,
    watched_at: Option<Instant>,
    agent_size: bool,
    introduced: bool,
    address: Option<String>,
}

struct Stream {
    input: tokio::sync::mpsc::Sender<Value>,
    cancel: CancellationToken,
}

struct Run {
    code: i32,
    output: String,
}

/// How a command reaches agent-browser. Each command that can start the browser carries the
/// same launch options, because agent-browser starts Chrome again for other options.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reach {
    Launch,
    Daemon,
}

pub(super) struct Browser {
    id: Uuid,
    places: Places,
    stamp: Stamp,
    state: watch::Sender<CanvasBrowser>,
    commands: tokio::sync::Mutex<()>,
    pictures: watch::Sender<Option<Picture>>,
    activity: Mutex<Activity>,
    stream: tokio::sync::Mutex<Option<Stream>>,
    closed: CancellationToken,
}

impl Browser {
    pub(super) fn start(id: Uuid, places: Places, stamp: Stamp) -> Arc<Self> {
        let (width, height) = FIRST_SIZE;
        let browser = Arc::new(Self {
            id,
            places,
            state: watch::Sender::new(CanvasBrowser {
                state: BrowserState::Asleep,
                changed: stamp.next(),
                tabs: Vec::new(),
                width,
                height,
                last: None,
                problem: None,
            }),
            stamp,
            commands: tokio::sync::Mutex::new(()),
            pictures: watch::Sender::new(None),
            activity: Mutex::new(Activity::default()),
            stream: tokio::sync::Mutex::new(None),
            closed: CancellationToken::new(),
        });
        tokio::spawn(sleeper(Arc::downgrade(&browser), browser.closed.clone()));
        browser
    }

    pub(super) fn state(&self) -> watch::Receiver<CanvasBrowser> {
        self.state.subscribe()
    }

    fn change(&self, change: impl FnOnce(&mut CanvasBrowser)) {
        self.state.send_modify(change);
    }

    fn activity<T>(&self, use_it: impl FnOnce(&mut Activity) -> T) -> T {
        use_it(
            &mut self
                .activity
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    fn awake(&self) -> bool {
        self.state.borrow().state == BrowserState::Awake
    }

    /// Runs an agent's agent-browser command on this terminal's browser.
    pub(super) async fn run(
        self: &Arc<Self>,
        folder: PathBuf,
        arguments: Vec<String>,
        terminal: &str,
        agent: Option<String>,
    ) -> Result<CanvasAnswer, String> {
        if let Some(refused) = refused(&arguments) {
            return Err(format!(
                "Kodosi runs the browser of this terminal, so {refused} is not available here. Use the other commands."
            ));
        }
        let command = arguments
            .iter()
            .find(|word| !word.starts_with('-'))
            .cloned()
            .unwrap_or_default();
        if matches!(command.as_str(), "" | "help" | "skills")
            || arguments
                .iter()
                .any(|word| matches!(word.as_str(), "--help" | "-h" | "--version" | "-V"))
        {
            let run = self
                .agent_browser(&folder, &arguments, Reach::Daemon, QUICK_LIMIT)
                .await?;
            return Ok(CanvasAnswer::Text {
                text: run.output,
                code: run.code,
            });
        }
        let _one = self.commands.lock().await;
        if self.state.borrow().state == BrowserState::Asleep {
            self.change(|state| {
                state.state = BrowserState::Starting;
                state.problem = None;
            });
        }
        let acting =
            !(LOOKS.contains(&command.as_str()) || (command == "tab" && arguments.len() == 1));
        let (used, introduced) = self.activity(|activity| {
            let used = activity
                .used_at
                .filter(|used| activity.command_at.is_none_or(|command| *used > command))
                .map(|used| used.elapsed().as_secs());
            activity.command_at = Some(Instant::now());
            if command == "set" && arguments.iter().any(|word| word == "viewport") {
                activity.agent_size = true;
            }
            (used, std::mem::replace(&mut activity.introduced, true))
        });
        let run = match self
            .agent_browser(&folder, &arguments, Reach::Launch, COMMAND_LIMIT)
            .await
        {
            Ok(run) => run,
            Err(problem) => {
                self.refresh(None).await;
                return Err(self.problem(problem));
            }
        };
        let action = acting.then(|| BrowserAction {
            agent: agent.unwrap_or_else(|| "The agent".to_owned()),
            words: words(&arguments),
        });
        self.refresh(action).await;
        let mut text = run.output;
        if !introduced {
            push_line(
                &mut text,
                &format!(
                    "The browser is on the canvas of {terminal}. The people in this terminal see it there and can use it too."
                ),
            );
        }
        if let Some(seconds) = used {
            push_line(
                &mut text,
                &format!(
                    "A person used the browser {seconds} s ago. Take a new snapshot before you use refs."
                ),
            );
        }
        Ok(CanvasAnswer::Text {
            text,
            code: run.code,
        })
    }

    /// The next picture after `after`, or the same number when none came in a few seconds.
    pub(super) async fn picture(self: &Arc<Self>, after: u64) -> Result<CanvasAnswer, String> {
        self.activity(|activity| activity.watched_at = Some(Instant::now()));
        let none = CanvasAnswer::Picture {
            sequence: after,
            frame: None,
        };
        if !self.awake() {
            return Ok(none);
        }
        self.stream().await?;
        let mut pictures = self.pictures.subscribe();
        let next = async {
            loop {
                let current = pictures.borrow_and_update().clone();
                if let Some(picture) = current.filter(|picture| picture.sequence > after) {
                    return Some(picture);
                }
                if pictures.changed().await.is_err() {
                    return None;
                }
            }
        };
        Ok(match tokio::time::timeout(PICTURE_WAIT, next).await {
            Ok(Some(picture)) => CanvasAnswer::Picture {
                sequence: picture.sequence,
                frame: Some(BrowserFrame {
                    width: picture.width,
                    height: picture.height,
                    data: picture.data.to_string(),
                }),
            },
            _ => none,
        })
    }

    /// A person's pointer, keys and text, with the size of the view that they use.
    pub(super) async fn input(
        self: &Arc<Self>,
        events: Vec<BrowserInput>,
        width: u32,
        height: u32,
    ) -> Result<CanvasAnswer, String> {
        if !self.awake() {
            return Err(ASLEEP.to_owned());
        }
        self.activity(|activity| activity.used_at = Some(Instant::now()));
        self.fit(width, height).await;
        let stream = self.stream().await?;
        for event in events {
            match stream_input(event) {
                Ok(event) => {
                    if stream.send(event).await.is_err() {
                        return Err("The browser stopped. Open it again.".to_owned());
                    }
                }
                Err(text) => {
                    let home = self.places.sockets.clone();
                    self.agent_browser(
                        &home,
                        &["keyboard".to_owned(), "inserttext".to_owned(), text],
                        Reach::Launch,
                        QUICK_LIMIT,
                    )
                    .await?;
                }
            }
        }
        Ok(CanvasAnswer::Done)
    }

    /// A person's step in the browser bar.
    pub(super) async fn act(self: &Arc<Self>, action: BrowserAct) -> Result<CanvasAnswer, String> {
        self.activity(|activity| activity.used_at = Some(Instant::now()));
        let arguments: Vec<String> = match action {
            BrowserAct::Wake { width, height } => return self.wake(width, height).await,
            BrowserAct::Open { url } => vec!["open".into(), url],
            BrowserAct::Back => vec!["back".into()],
            BrowserAct::Forward => vec!["forward".into()],
            BrowserAct::Reload => vec!["reload".into()],
            BrowserAct::Tab { id } => vec!["tab".into(), id],
            BrowserAct::NewTab => vec!["tab".into(), "new".into()],
            BrowserAct::CloseTab { id } => vec!["tab".into(), "close".into(), id],
        };
        if !self.awake() {
            return Err(ASLEEP.to_owned());
        }
        let _one = self.commands.lock().await;
        let home = self.places.sockets.clone();
        let run = self
            .agent_browser(&home, &arguments, Reach::Launch, QUICK_LIMIT)
            .await?;
        self.refresh(None).await;
        if run.code == 0 {
            Ok(CanvasAnswer::Done)
        } else {
            Err(first_line(&run.output))
        }
    }

    async fn wake(self: &Arc<Self>, width: u32, height: u32) -> Result<CanvasAnswer, String> {
        self.activity(|activity| {
            activity.watched_at = Some(Instant::now());
            activity.agent_size = false;
        });
        let _one = self.commands.lock().await;
        if self.state.borrow().state != BrowserState::Asleep {
            return Ok(CanvasAnswer::Done);
        }
        self.change(|state| {
            state.state = BrowserState::Starting;
            state.problem = None;
        });
        let (width, height) = sized(width, height).unwrap_or(FIRST_SIZE);
        let address = self
            .activity(|activity| activity.address.clone())
            .unwrap_or_else(|| "about:blank".to_owned());
        let home = self.places.sockets.clone();
        let mut run = self
            .agent_browser(
                &home,
                &[
                    "set".into(),
                    "viewport".into(),
                    width.to_string(),
                    height.to_string(),
                ],
                Reach::Launch,
                QUICK_LIMIT,
            )
            .await;
        if run.as_ref().is_ok_and(|run| run.code == 0) {
            self.change(|state| (state.width, state.height) = (width, height));
            run = self
                .agent_browser(&home, &["open".into(), address], Reach::Launch, QUICK_LIMIT)
                .await;
        }
        self.refresh(None).await;
        match run {
            Ok(run) if run.code == 0 => Ok(CanvasAnswer::Done),
            Ok(run) => Err(self.problem(first_line(&run.output))),
            Err(problem) => Err(self.problem(problem)),
        }
    }

    fn problem(&self, problem: String) -> String {
        self.change(|state| state.problem = Some(problem.chars().take(300).collect()));
        problem
    }

    /// Gives the browser the size of a person's view, unless the agent set a size.
    async fn fit(&self, width: u32, height: u32) {
        let Some((width, height)) = sized(width, height) else {
            return;
        };
        let current = {
            let state = self.state.borrow();
            (state.width, state.height)
        };
        if self.activity(|activity| activity.agent_size)
            || (current.0.abs_diff(width) <= 2 && current.1.abs_diff(height) <= 2)
        {
            return;
        }
        let home = self.places.sockets.clone();
        if let Ok(run) = self
            .agent_browser(
                &home,
                &[
                    "set".into(),
                    "viewport".into(),
                    width.to_string(),
                    height.to_string(),
                ],
                Reach::Launch,
                QUICK_LIMIT,
            )
            .await
            && run.code == 0
        {
            self.change(|state| (state.width, state.height) = (width, height));
        }
    }

    /// Reads whether Chrome runs and, when it does, its tabs. The check starts nothing.
    async fn refresh(&self, action: Option<BrowserAction>) {
        let home = self.places.sockets.clone();
        let tabs = if self.launched().await {
            self.agent_browser(
                &home,
                &["tab".into(), "--json".into()],
                Reach::Launch,
                QUICK_LIMIT,
            )
            .await
            .ok()
            .filter(|run| run.code == 0)
            .and_then(|run| serde_json::from_str::<Value>(&run.output).ok())
            .map(|answer| read_tabs(&answer))
        } else {
            None
        };
        if let Some(address) = tabs
            .as_ref()
            .and_then(|tabs| tabs.iter().find(|tab| tab.active))
            .map(|tab| tab.url.clone())
            .filter(|address| address.starts_with("http"))
        {
            self.activity(|activity| activity.address = Some(address));
        }
        let changed = action.as_ref().map(|_| self.stamp.next());
        self.change(|state| {
            match tabs {
                Some(tabs) => {
                    state.state = BrowserState::Awake;
                    state.tabs = tabs;
                    state.problem = None;
                }
                None => state.state = BrowserState::Asleep,
            }
            if let Some(action) = action {
                state.last = Some(action);
            }
            if let Some(changed) = changed {
                state.changed = changed;
            }
        });
        if !self.awake() {
            self.stop_stream().await;
        }
    }

    async fn launched(&self) -> bool {
        let home = self.places.sockets.clone();
        self.agent_browser(
            &home,
            &["session".into(), "info".into(), "--json".into()],
            Reach::Daemon,
            QUICK_LIMIT,
        )
        .await
        .ok()
        .and_then(|run| serde_json::from_str::<Value>(&run.output).ok())
        .is_some_and(|info| {
            let data = &info["data"];
            data["active"].as_bool() == Some(true)
                && data["runtime"]["browserLaunched"].as_bool() == Some(true)
        })
    }

    /// Closes Chrome and keeps the profile, so the browser wakes with the same sign-ins.
    pub(super) async fn sleep(&self) {
        let home = self.places.sockets.clone();
        if self.launched().await {
            drop(
                self.agent_browser(&home, &["close".into()], Reach::Daemon, QUICK_LIMIT)
                    .await,
            );
        }
        self.stop_stream().await;
        self.change(|state| state.state = BrowserState::Asleep);
    }

    /// Closes Chrome and deletes the profile with its sign-ins.
    pub(super) async fn remove(&self) {
        self.closed.cancel();
        self.sleep().await;
        let profile = self.places.profile.clone();
        drop(tokio::task::spawn_blocking(move || std::fs::remove_dir_all(profile)).await);
    }

    async fn stream(&self) -> Result<tokio::sync::mpsc::Sender<Value>, String> {
        let mut stream = self.stream.lock().await;
        if let Some(open) = stream.as_ref().filter(|open| !open.cancel.is_cancelled()) {
            return Ok(open.input.clone());
        }
        let home = self.places.sockets.clone();
        let status = self
            .agent_browser(
                &home,
                &["stream".into(), "status".into(), "--json".into()],
                Reach::Daemon,
                QUICK_LIMIT,
            )
            .await?;
        let port = serde_json::from_str::<Value>(&status.output)
            .ok()
            .and_then(|answer| answer["data"]["port"].as_u64())
            .and_then(|port| u16::try_from(port).ok())
            .ok_or_else(|| "The browser does not send pictures now. Open it again.".to_owned())?;
        let (socket, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}"))
            .await
            .map_err(|_| "The browser does not send pictures now. Open it again.".to_owned())?;
        let (input, inputs) = tokio::sync::mpsc::channel(256);
        let cancel = self.closed.child_token();
        tokio::spawn(relay(socket, inputs, self.pictures.clone(), cancel.clone()));
        *stream = Some(Stream {
            input: input.clone(),
            cancel,
        });
        drop(stream);
        Ok(input)
    }

    async fn stop_stream(&self) {
        let stream = self.stream.lock().await.take();
        if let Some(stream) = stream {
            stream.cancel.cancel();
        }
    }

    /// Runs agent-browser with this terminal's session, profile and settings, and only them.
    async fn agent_browser(
        &self,
        folder: &Path,
        arguments: &[String],
        reach: Reach,
        limit: Duration,
    ) -> Result<Run, String> {
        if !self.places.binary.is_file() {
            return Err(NO_AGENT_BROWSER.to_owned());
        }
        let chromium = match reach {
            Reach::Launch => Some(chromium().ok_or_else(|| NO_CHROMIUM.to_owned())?),
            Reach::Daemon => None,
        };
        prepare(&self.places).await?;
        let mut command = agent_browser_command(&self.places.binary, &self.places.sockets);
        command
            .arg("--session")
            .arg(self.id.simple().to_string())
            .arg("--config")
            .arg(&self.places.config);
        if let Some(chromium) = chromium {
            command
                .arg("--profile")
                .arg(&self.places.profile)
                .arg("--executable-path")
                .arg(chromium);
        }
        command
            .args(arguments)
            .current_dir(if folder.is_dir() {
                folder
            } else {
                &self.places.sockets
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = command
            .spawn()
            .map_err(|error| format!("agent-browser did not start: {error}"))?;
        let output = tokio::time::timeout(limit, child.wait_with_output())
            .await
            .map_err(|_| format!("agent-browser did not answer in {} s.", limit.as_secs()))?
            .map_err(|error| format!("agent-browser failed: {error}"))?;
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        let errors = String::from_utf8_lossy(&output.stderr);
        if !errors.trim().is_empty() {
            push_line(&mut text, errors.trim_end());
        }
        if text.len() > MOST_OUTPUT {
            let mut end = MOST_OUTPUT;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
            text.push_str("\n…");
        }
        Ok(Run {
            code: output.status.code().unwrap_or(1),
            output: text.trim_end().to_owned(),
        })
    }
}

/// The word of a command that Kodosi does not run: an option or command of [`NOT_HERE`], or
/// `--all` with `close`, which would close the browsers of the other terminals.
pub(super) fn refused(arguments: &[String]) -> Option<&str> {
    let closing = arguments
        .iter()
        .find(|word| !word.starts_with('-'))
        .is_some_and(|command| matches!(command.as_str(), "close" | "quit" | "exit"));
    arguments
        .iter()
        .find(|word| {
            NOT_HERE.iter().any(|refused| {
                *word == refused
                    || (refused.starts_with("--") && word.starts_with(&format!("{refused}=")))
            }) || (closing && matches!(word.as_str(), "--all" | "-a"))
        })
        .map(String::as_str)
}

/// agent-browser with only Kodosi's settings: none of the person's `AGENT_BROWSER_` variables.
fn agent_browser_command(binary: &Path, sockets: &Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(binary);
    for (variable, _) in std::env::vars_os() {
        if variable.to_string_lossy().starts_with("AGENT_BROWSER_") {
            command.env_remove(variable);
        }
    }
    command
        .env("AGENT_BROWSER_SOCKET_DIR", sockets)
        .env("AGENT_BROWSER_IDLE_TIMEOUT_MS", DAEMON_IDLE_MS)
        .env("AGENT_BROWSER_STREAM_QUALITY", "60")
        .env("AGENT_BROWSER_STREAM_MAX_WIDTH", "1600")
        .env("AGENT_BROWSER_STREAM_MAX_HEIGHT", "1600")
        .stdin(Stdio::null())
        .kill_on_drop(true);
    kodosi_os::process::without_terminal(command.as_std_mut());
    command
}

async fn prepare(places: &Places) -> Result<(), String> {
    let config = places.config.clone();
    let folders = [places.sockets.clone(), places.profile.clone()];
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        for folder in folders {
            kodosi_os::fs::create_private_dir(&folder)?;
        }
        if !config.is_file() {
            kodosi_os::fs::write_private(&config, b"{}\n")?;
        }
        Ok(())
    })
    .await
    .map_err(|_| "The browser folder could not be made.".to_owned())?
    .map_err(|error| format!("The browser folder could not be made: {error}"))
}

/// Puts the browser to sleep after a while with no command, no input and nobody looking, and
/// stops its pictures when nobody looks.
async fn sleeper(browser: Weak<Browser>, closed: CancellationToken) {
    loop {
        tokio::select! {
            () = closed.cancelled() => return,
            () = tokio::time::sleep(SLEEP_CHECK) => {}
        }
        let Some(browser) = browser.upgrade() else {
            return;
        };
        let (idle, unwatched) = browser.activity(|activity| {
            let last = [activity.command_at, activity.used_at, activity.watched_at]
                .into_iter()
                .flatten()
                .max();
            (
                last.is_none_or(|last| last.elapsed() > SLEEP_AFTER),
                [activity.used_at, activity.watched_at]
                    .into_iter()
                    .flatten()
                    .max()
                    .is_none_or(|last| last.elapsed() > WATCH_KEPT),
            )
        });
        if browser.awake() && idle {
            browser.sleep().await;
        } else if unwatched {
            browser.stop_stream().await;
        }
    }
}

async fn relay(
    socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    mut inputs: tokio::sync::mpsc::Receiver<Value>,
    pictures: watch::Sender<Option<Picture>>,
    cancel: CancellationToken,
) {
    let (mut sink, mut source) = socket.split();
    if sink
        .send(Message::text(
            json!({"type":"config","maxFps":10}).to_string(),
        ))
        .await
        .is_err()
    {
        cancel.cancel();
        return;
    }
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            input = inputs.recv() => match input {
                Some(input) => if sink.send(Message::text(input.to_string())).await.is_err() { break },
                None => break,
            },
            message = source.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    if let Some(picture) = picture_of(text.as_str()) {
                        pictures.send_modify(|current| {
                            let sequence = current.as_ref().map_or(0, |current| current.sequence) + 1;
                            *current = Some(Picture { sequence, ..picture });
                        });
                    }
                }
                Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
        }
    }
    cancel.cancel();
}

pub(super) fn picture_of(text: &str) -> Option<Picture> {
    let frame: Value = serde_json::from_str(text).ok()?;
    if frame["type"].as_str() != Some("frame") {
        return None;
    }
    let data = frame["data"].as_str()?;
    if data.len() > super::MOST_PICTURE {
        return None;
    }
    let size = |field: &str| pixels(&frame["metadata"][field]);
    Some(Picture {
        sequence: 0,
        width: size("deviceWidth")?,
        height: size("deviceHeight")?,
        data: Arc::from(data),
    })
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is checked to be from 1 to 8192"
)]
fn pixels(value: &Value) -> Option<u32> {
    value
        .as_f64()
        .filter(|value| (1.0..=8192.0).contains(value))
        .map(|value| value.round() as u32)
}

/// One input event in the form of agent-browser's stream, or the text of a paste, which the
/// browser inserts in one step. On a computer that is not a Mac, a Mac's Command is Control.
pub(super) fn stream_input(event: BrowserInput) -> Result<Value, String> {
    let mac = cfg!(target_os = "macos");
    let modifiers = |modifiers: u8| {
        if mac || modifiers & 4 == 0 {
            modifiers
        } else {
            (modifiers & !4) | 2
        }
    };
    Ok(match event {
        BrowserInput::Mouse {
            event_type,
            x,
            y,
            button,
            click_count,
            delta_x,
            delta_y,
            modifiers: held,
        } => json!({
            "type": "input_mouse",
            "eventType": event_type,
            "x": x,
            "y": y,
            "button": button,
            "clickCount": click_count,
            "deltaX": delta_x,
            "deltaY": delta_y,
            "modifiers": modifiers(held),
        }),
        BrowserInput::Key {
            event_type,
            key,
            code,
            text,
            key_code,
            modifiers: held,
        } => {
            let (key, code) = if !mac && key == "Meta" {
                let side = if code == "MetaRight" {
                    "ControlRight"
                } else {
                    "ControlLeft"
                };
                ("Control".to_owned(), side.to_owned())
            } else {
                (key, code)
            };
            let text = text.or_else(|| {
                (key == "Enter" && event_type == KeyEvent::KeyDown).then(|| "\r".to_owned())
            });
            let mut value = json!({
                "type": "input_keyboard",
                "eventType": event_type,
                "key": key,
                "code": code,
                "windowsVirtualKeyCode": key_code,
                "modifiers": modifiers(held),
            });
            if let Some(text) = text {
                value["text"] = json!(text);
            }
            value
        }
        BrowserInput::Touch {
            event_type,
            points,
            modifiers: held,
        } => json!({
            "type": "input_touch",
            "eventType": event_type,
            "touchPoints": points,
            "modifiers": modifiers(held),
        }),
        BrowserInput::Text { text } => return Err(text),
    })
}

pub(super) fn read_tabs(answer: &Value) -> Vec<BrowserTab> {
    let cut = |text: &str, most: usize| {
        let mut end = text.len().min(most);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text[..end].to_owned()
    };
    answer["data"]["tabs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|tab| tab["type"].as_str().is_none_or(|kind| kind == "page"))
        .take(super::MOST_TABS)
        .filter_map(|tab| {
            Some(BrowserTab {
                id: cut(tab["tabId"].as_str()?, 64),
                title: cut(tab["title"].as_str().unwrap_or_default(), super::MOST_TITLE),
                url: cut(tab["url"].as_str().unwrap_or_default(), super::MOST_URL),
                active: tab["active"].as_bool().unwrap_or(false),
            })
        })
        .collect()
}

/// The command and its target, without what the agent types, which can be secret.
pub(super) fn words(arguments: &[String]) -> String {
    let words = arguments
        .iter()
        .filter(|word| !word.starts_with('-'))
        .take(2)
        .cloned()
        .collect::<Vec<_>>();
    let typed = matches!(
        words.first().map(String::as_str),
        Some("fill" | "type" | "keyboard" | "eval" | "press")
    );
    let shown = if typed { &words[..1] } else { &words[..] };
    shown.join(" ").chars().take(80).collect()
}

fn sized(width: u32, height: u32) -> Option<(u32, u32)> {
    (SIZES.contains(&width) && SIZES.contains(&height)).then_some((width, height))
}

fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("The browser did not do that.")
        .chars()
        .take(300)
        .collect()
}

fn push_line(text: &mut String, line: &str) {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(line);
}

/// Closes the browsers that are left, for example after a crash, and removes their sockets.
pub(super) async fn close_left(data_root: &Path) {
    let sockets = sockets(data_root);
    let binary = agent_browser();
    let left = std::fs::read_dir(&sockets).is_ok_and(|mut entries| entries.next().is_some());
    if binary.is_file() && left {
        let mut command = agent_browser_command(&binary, &sockets);
        command
            .args(["close", "--all"])
            .current_dir(data_root)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Ok(child) = command.spawn() {
            drop(tokio::time::timeout(QUICK_LIMIT, child.wait_with_output()).await);
        }
    }
    drop(tokio::task::spawn_blocking(move || std::fs::remove_dir_all(sockets)).await);
}

/// The profile folders of terminals that do not come back.
pub(super) fn left_profiles(data_root: &Path, returning: &HashSet<Uuid>) -> Vec<PathBuf> {
    let profiles = data_root.join("browser").join("profiles");
    std::fs::read_dir(profiles)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .and_then(|name| Uuid::parse_str(name).ok())
                .is_none_or(|id| !returning.contains(&id))
        })
        .map(|entry| entry.path())
        .collect()
}
