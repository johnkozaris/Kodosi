use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use serde_json::Value;
use shared_browser::{Input, Place, State, ToWindow, Viewer, Window};
use tokio::{
    sync::{mpsc, watch},
    time::Instant,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    BrowserAct, BrowserAction, BrowserDoing, BrowserDoingKind, BrowserPoint, BrowserState,
    BrowserTab, CanvasAnswer, CanvasBrowser, PIPE_QUEUE, PipeEnds, engine::BrowserHost,
    files::Stamp,
};

const COMMAND_LIMIT: Duration = Duration::from_mins(2);
const QUICK_LIMIT: Duration = Duration::from_secs(30);
/// While people look, the agent's pointer reaches the element before the agent acts on it.
const POINTER_LEAD: Duration = Duration::from_millis(280);
const MOST_OUTPUT: usize = 1024 * 1024;
const FIRST_SIZE: (u32, u32) = (1280, 720);
/// agent-browser leaves the browser when the agent is idle, because a page can notice a client
/// on its debug port.
const DAEMON_IDLE_MS: &str = "60000";
const FIRST_PAGE: &str = "about:blank";
const NO_AGENT_BROWSER: &str = "This Kodosi has no agent-browser for the shared browser.";
/// The profile that keeps a person's sign-ins. A shared terminal has a profile of its own.
pub(super) const KEPT_PROFILE: &str = "person";
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
    sockets: PathBuf,
    config: PathBuf,
}

impl Places {
    pub(super) fn of(data_root: &Path) -> Self {
        Self {
            binary: agent_browser(),
            sockets: sockets(data_root),
            config: data_root.join("browser").join("agent-browser.json"),
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

/// The folder of the profile that belongs to one terminal.
pub(super) fn own_profile(data_root: &Path, id: Uuid) -> PathBuf {
    data_root
        .join("browser")
        .join("profiles")
        .join(id.to_string())
}

/// Why this computer cannot run the agent's side of the shared browser, if it cannot.
pub(super) fn missing() -> Option<&'static str> {
    (!agent_browser().is_file()).then_some(NO_AGENT_BROWSER)
}

#[derive(Default)]
struct Activity {
    command_at: Option<Instant>,
    introduced: bool,
    address: Option<String>,
    shown_downloads: HashSet<String>,
}

struct Run {
    code: i32,
    output: String,
}

/// What a browser needs from its terminal.
pub(super) struct Terminal {
    pub(super) id: Uuid,
    pub(super) folder: watch::Receiver<PathBuf>,
    pub(super) shared: watch::Receiver<bool>,
    /// Puts a finished download on the canvas.
    pub(super) show: Box<dyn Fn(PathBuf) + Send + Sync>,
}

pub(super) struct Browser {
    terminal: Terminal,
    places: Places,
    host: BrowserHost,
    stamp: Stamp,
    state: watch::Sender<CanvasBrowser>,
    commands: tokio::sync::Mutex<()>,
    window: tokio::sync::Mutex<Option<Window>>,
    activity: Mutex<Activity>,
    closed: CancellationToken,
}

impl Browser {
    pub(super) fn start(
        terminal: Terminal,
        places: Places,
        host: BrowserHost,
        stamp: Stamp,
    ) -> Arc<Self> {
        let (width, height) = FIRST_SIZE;
        Arc::new(Self {
            terminal,
            places,
            host,
            state: watch::Sender::new(CanvasBrowser {
                state: BrowserState::Asleep,
                changed: stamp.next(),
                tabs: Vec::new(),
                width,
                height,
                getting: None,
                doing: None,
                last: None,
                problem: None,
            }),
            stamp,
            commands: tokio::sync::Mutex::new(()),
            window: tokio::sync::Mutex::new(None),
            activity: Mutex::new(Activity::default()),
            closed: CancellationToken::new(),
        })
    }

    pub(super) fn state(&self) -> watch::Receiver<CanvasBrowser> {
        self.state.subscribe()
    }

    fn change(&self, change: impl FnOnce(&mut CanvasBrowser)) {
        self.state.send_modify(change);
    }

    /// The page that the browser opens when it wakes.
    pub(super) fn reopen_at(&self, address: Option<String>) {
        self.activity(|activity| activity.address = address);
    }

    fn activity<T>(&self, use_it: impl FnOnce(&mut Activity) -> T) -> T {
        use_it(
            &mut self
                .activity
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    fn profile(&self) -> String {
        if *self.terminal.shared.borrow() {
            self.terminal.id.to_string()
        } else {
            KEPT_PROFILE.to_owned()
        }
    }

    /// The window of this terminal, opened if it is not open. Kodosi Browser comes the first
    /// time here, and the canvas shows how far that is.
    async fn window(self: &Arc<Self>) -> Result<Window, String> {
        let mut open = self.window.lock().await;
        if let Some(window) = open.as_ref().filter(|window| window.is_open()) {
            return Ok(window.clone());
        }
        self.change(|state| {
            state.state = BrowserState::Starting;
            state.problem = None;
        });
        let browsers = self
            .host
            .browsers(|part| self.change(|state| state.getting = Some(part)))
            .await
            .map_err(|problem| self.problem(problem))?;
        self.change(|state| state.getting = None);
        let place = Place {
            profile: self.profile(),
            window: self.terminal.id.to_string(),
            address: self
                .activity(|activity| activity.address.clone())
                .unwrap_or_else(|| FIRST_PAGE.to_owned()),
            folder: self.terminal.folder.borrow().clone(),
        };
        let window = browsers
            .open(place)
            .await
            .map_err(|problem| self.problem(problem.to_string()))?;
        tokio::spawn(follow(
            Arc::downgrade(self),
            window.clone(),
            self.closed.clone(),
        ));
        *open = Some(window.clone());
        drop(open);
        Ok(window)
    }

    /// Shows what the window says on the canvas.
    fn show(&self, window: &State) {
        let tabs: Vec<BrowserTab> = window
            .tabs
            .iter()
            .take(super::MOST_TABS)
            .map(|tab| BrowserTab {
                id: cut(&tab.id, 64),
                title: cut(&tab.title, super::MOST_TITLE),
                url: cut(&tab.address, super::MOST_URL),
                active: tab.active,
            })
            .collect();
        if let Some(address) = tabs
            .iter()
            .find(|tab| tab.active)
            .map(|tab| tab.url.clone())
            .filter(|address| address.starts_with("http"))
        {
            self.activity(|activity| activity.address = Some(address));
        }
        for download in window.downloads.iter().filter(|download| download.done) {
            let new =
                self.activity(|activity| activity.shown_downloads.insert(download.path.clone()));
            if new {
                (self.terminal.show)(PathBuf::from(&download.path));
            }
        }
        self.change(|state| {
            state.state = BrowserState::Awake;
            state.tabs = tabs;
            state.width = window.width;
            state.height = window.height;
            state.problem = None;
        });
    }

    /// The middle of the element that a command names, when it is in view.
    async fn point(&self, folder: &Path, port: u16, target: &str) -> Option<BrowserPoint> {
        let run = self
            .agent_browser(
                folder,
                &[
                    "get".into(),
                    "box".into(),
                    target.to_owned(),
                    "--json".into(),
                ],
                Some(port),
                QUICK_LIMIT,
            )
            .await
            .ok()
            .filter(|run| run.code == 0)?;
        let (width, height) = {
            let state = self.state.borrow();
            (state.width, state.height)
        };
        middle(&run.output, width, height)
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
                .agent_browser(&folder, &arguments, None, QUICK_LIMIT)
                .await?;
            return Ok(CanvasAnswer::Text {
                text: run.output,
                code: run.code,
            });
        }
        let _one = self.commands.lock().await;
        let window = self.window().await?;
        window.touch().await;
        let port = window.debug_port();
        let acting =
            !(LOOKS.contains(&command.as_str()) || (command == "tab" && arguments.len() == 1));
        let (used, introduced) = self.activity(|activity| {
            let used = window
                .used_at()
                .map(Instant::from_std)
                .filter(|used| activity.command_at.is_none_or(|command| *used > command))
                .map(|used| used.elapsed().as_secs());
            activity.command_at = Some(Instant::now());
            (used, std::mem::replace(&mut activity.introduced, true))
        });
        let agent = agent.unwrap_or_else(|| "The agent".to_owned());
        if let Some((kind, target)) = doing(&arguments) {
            let point = match target {
                Some(target) => self.point(&folder, port, &target).await,
                None => None,
            };
            let doer = agent.clone();
            self.change(|state| {
                state.doing = Some(BrowserDoing {
                    agent: doer,
                    kind,
                    point,
                });
            });
            if point.is_some() {
                tokio::time::sleep(POINTER_LEAD).await;
            }
        }
        let run = self
            .agent_browser(&folder, &arguments, Some(port), COMMAND_LIMIT)
            .await;
        let action = acting.then(|| BrowserAction {
            agent,
            words: words(&arguments),
        });
        self.finish(action);
        let run = run.map_err(|problem| self.problem(problem))?;
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

    fn finish(&self, action: Option<BrowserAction>) {
        let changed = action.as_ref().map(|_| self.stamp.next());
        self.change(|state| {
            state.doing = None;
            if let Some(action) = action {
                state.last = Some(action);
            }
            if let Some(changed) = changed {
                state.changed = changed;
            }
        });
    }

    /// A person's step that needs no view of the browser.
    pub(super) async fn act(self: &Arc<Self>, action: BrowserAct) -> Result<CanvasAnswer, String> {
        let window = self.window().await?;
        if let BrowserAct::Open { url } = action {
            let open = ToWindow::Input {
                input: Input::Open { address: url },
            };
            window
                .tell(open)
                .await
                .map_err(|problem| problem.to_string())?;
        }
        Ok(CanvasAnswer::Done)
    }

    /// Gives a person the live picture of this browser. The two ends carry the browser's own
    /// messages, so one who only looks needs nothing more.
    pub(super) async fn watch(self: &Arc<Self>, viewer: Viewer) -> Result<PipeEnds, String> {
        let window = self.window().await?;
        let mut pipe = window
            .watch(viewer)
            .await
            .map_err(|problem| problem.to_string())?;
        let (sender, mut from_view) = mpsc::channel::<Vec<u8>>(PIPE_QUEUE);
        let (to_view, receiver) = mpsc::channel(PIPE_QUEUE);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    message = from_view.recv() => {
                        let Some(message) = message else { break };
                        if pipe.send(&message).await.is_err() {
                            break;
                        }
                    }
                    message = pipe.receive() => {
                        let Some(message) = message else { break };
                        // A view that falls behind asks for a new key frame.
                        if to_view.try_send(message.to_vec()).is_err() && to_view.is_closed() {
                            break;
                        }
                    }
                }
            }
        });
        Ok(PipeEnds { sender, receiver })
    }

    fn problem(&self, problem: String) -> String {
        self.change(|state| {
            state.state = BrowserState::Asleep;
            state.getting = None;
            state.problem = Some(problem.chars().take(300).collect());
        });
        problem
    }

    /// Closes the window. The profile stays, so the browser wakes with the same sign-ins.
    pub(super) async fn sleep(&self) {
        let window = self.window.lock().await.take();
        if let Some(window) = window {
            window.close().await;
            window.closed().await;
        }
        self.change(|state| {
            state.state = BrowserState::Asleep;
            state.doing = None;
        });
    }

    /// Closes the window for good, with the profile of a shared terminal and what agent-browser
    /// keeps of the session.
    pub(super) async fn remove(&self) {
        self.closed.cancel();
        self.sleep().await;
        if *self.terminal.shared.borrow()
            && let Err(problem) = self
                .host
                .remove_profile(&self.terminal.id.to_string())
                .await
        {
            tracing::warn!(%problem, "the profile of a closed terminal stays");
        }
        let sockets = self.places.sockets.clone();
        let session = format!("{}.", self.terminal.id.simple());
        drop(
            tokio::task::spawn_blocking(move || {
                for file in std::fs::read_dir(sockets).into_iter().flatten().flatten() {
                    if file.file_name().to_string_lossy().starts_with(&session) {
                        drop(std::fs::remove_file(file.path()));
                    }
                }
            })
            .await,
        );
    }

    /// Runs agent-browser with this terminal's session and settings, and only them. With a port
    /// it works on this terminal's browser.
    async fn agent_browser(
        &self,
        folder: &Path,
        arguments: &[String],
        port: Option<u16>,
        limit: Duration,
    ) -> Result<Run, String> {
        if !self.places.binary.is_file() {
            return Err(NO_AGENT_BROWSER.to_owned());
        }
        prepare(&self.places).await?;
        let mut command = agent_browser_command(&self.places.binary, &self.places.sockets);
        command
            .arg("--session")
            .arg(self.terminal.id.simple().to_string())
            .arg("--config")
            .arg(&self.places.config);
        if let Some(port) = port {
            command.arg("--cdp").arg(port.to_string());
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

/// Keeps the canvas in step with the window until it closes.
async fn follow(browser: Weak<Browser>, window: Window, closed: CancellationToken) {
    let mut state = window.state();
    loop {
        {
            let Some(browser) = browser.upgrade() else {
                return;
            };
            browser.show(&state.borrow_and_update());
        }
        tokio::select! {
            () = closed.cancelled() => return,
            () = window.closed() => break,
            changed = state.changed() => {
                if changed.is_err() {
                    break;
                }
            }
        }
    }
    if let Some(browser) = browser.upgrade() {
        browser.change(|state| {
            state.state = BrowserState::Asleep;
            state.doing = None;
        });
    }
}

fn cut(text: &str, most: usize) -> String {
    let mut end = text.len().min(most);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
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
        .stdin(Stdio::null())
        .kill_on_drop(true);
    kodosi_os::process::without_terminal(command.as_std_mut());
    command
}

async fn prepare(places: &Places) -> Result<(), String> {
    let config = places.config.clone();
    let sockets = places.sockets.clone();
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        kodosi_os::fs::create_private_dir(&sockets)?;
        if let Some(folder) = config.parent() {
            kodosi_os::fs::create_private_dir(folder)?;
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

/// What a command does, for the people who look, and the element that it acts on.
pub(super) fn doing(arguments: &[String]) -> Option<(BrowserDoingKind, Option<String>)> {
    let words = arguments
        .iter()
        .filter(|word| !word.starts_with('-'))
        .map(String::as_str)
        .collect::<Vec<_>>();
    let (&command, rest) = words.split_first()?;
    let target = rest.first().map(|word| (*word).to_owned());
    Some(match command {
        "open" | "goto" | "navigate" | "back" | "forward" | "reload" => {
            (BrowserDoingKind::Open, None)
        }
        "tab" if rest.is_empty() => (BrowserDoingKind::Read, None),
        "tab" => (BrowserDoingKind::Open, None),
        "click" | "dblclick" | "check" | "uncheck" | "hover" | "focus" | "select" | "drag"
        | "upload" | "tap" => (BrowserDoingKind::Click, target),
        "fill" | "type" => (BrowserDoingKind::Type, target),
        "press" | "key" | "keyboard" | "keydown" | "keyup" => (BrowserDoingKind::Type, None),
        "scrollintoview" => (BrowserDoingKind::Scroll, target),
        "scroll" | "swipe" => (BrowserDoingKind::Scroll, None),
        "find"
            if rest.iter().any(|word| {
                matches!(
                    *word,
                    "click" | "dblclick" | "check" | "uncheck" | "hover" | "tap"
                )
            }) =>
        {
            (BrowserDoingKind::Click, None)
        }
        "find" if rest.iter().any(|word| matches!(*word, "fill" | "type")) => {
            (BrowserDoingKind::Type, None)
        }
        "snapshot" | "get" | "is" | "screenshot" | "find" | "console" | "errors" | "pdf" => {
            (BrowserDoingKind::Read, None)
        }
        "wait" => (BrowserDoingKind::Wait, None),
        "session" | "close" => return None,
        _ => (BrowserDoingKind::Other, None),
    })
}

/// The middle of the box that `get box` prints, when it is inside the browser's view.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the point is checked to be inside the browser's view"
)]
pub(super) fn middle(output: &str, width: u32, height: u32) -> Option<BrowserPoint> {
    let found = serde_json::from_str::<Value>(output.trim()).ok()?;
    let side = |name: &str| {
        found["data"][name]
            .as_f64()
            .filter(|value| value.is_finite())
    };
    let x = side("x")? + side("width")? / 2.0;
    let y = side("y")? + side("height")? / 2.0;
    (x >= 0.0 && y >= 0.0 && x < f64::from(width) && y < f64::from(height)).then_some(
        BrowserPoint {
            x: x as u32,
            y: y as u32,
        },
    )
}

fn push_line(text: &mut String, line: &str) {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(line);
}

/// Removes the sockets of agent-browsers that are left, for example after a crash. Each one
/// leaves by itself when it is idle.
pub(super) async fn close_left(data_root: &Path) {
    let sockets = sockets(data_root);
    drop(tokio::task::spawn_blocking(move || std::fs::remove_dir_all(sockets)).await);
}

/// The profile folders of terminals that do not come back. A person's kept profile stays.
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
                .is_some_and(|id| !returning.contains(&id))
        })
        .map(|entry| entry.path())
        .collect()
}
