use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use serde_json::Value;
use shared_browser::{Driver, Place, State, Viewer, Window};
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
use crate::text::Text;

const COMMAND_LIMIT: Duration = Duration::from_mins(2);
const QUICK_LIMIT: Duration = Duration::from_secs(30);
/// Delay after showing the agent's target pointer so viewers see it move before the action runs.
const POINTER_LEAD: Duration = Duration::from_millis(280);
pub(super) const MOST_OUTPUT: usize = 1024 * 1024;
/// Read 3 extra bytes past `MOST_OUTPUT` so a 4-byte UTF-8 character straddling the boundary
/// decodes intact and is cleanly dropped by `floor_char_boundary` instead of becoming U+FFFD.
const MOST_READ: usize = MOST_OUTPUT + 3;
const FIRST_SIZE: (u32, u32) = (1280, 720);
const MOST_KEPT_ADDRESS: usize = 4096;
/// Disconnect `agent-browser` when idle because sites can detect an active CDP debug client.
const DAEMON_IDLE_MS: &str = "60000";
const FIRST_PAGE: &str = "about:blank";
/// Persisted on disk; renaming this would orphan users' existing browser profiles and sign-ins.
pub(super) const KEPT_PROFILE: &str = "person";
/// `agent-browser` flags and subcommands blocked because Kodosi manages the browser's session,
/// profile, binary, and lifecycle.
const NOT_HERE: &[&str] = &[
    "--cdp",
    "--cdp-headers",
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
/// Error prefix from `agent-browser` when arguments are missing. On Windows, PowerShell treats
/// unquoted `@e2` as splatting and `#name` as a comment, silently dropping them.
const MISSING_WORD: &str = "Missing arguments for:";
const POWERSHELL_WORDS: &str = "PowerShell drops a word that starts with @ or #. Write a reference without the @ sign, for example e2, and a selector such as \"#name\" in quotes.";
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
    pub(super) fn of(data_root: &Path, binary: PathBuf) -> Self {
        Self {
            binary,
            sockets: sockets(data_root),
            config: data_root.join("browser").join("agent-browser.json"),
        }
    }
}

/// Canonicalize `current_exe()` because `kodosi` can be a symlink into the app bundle and
/// macOS reports the symlink path rather than the real binary directory.
pub(super) fn agent_browser() -> PathBuf {
    let name = if cfg!(windows) {
        "agent-browser.exe"
    } else {
        "agent-browser"
    };
    std::env::current_exe()
        .ok()
        .map(|program| kodosi_os::fs::canonicalize(&program).unwrap_or(program))
        .and_then(|program| program.parent().map(|folder| folder.join(name)))
        .unwrap_or_else(|| PathBuf::from(name))
}

/// Unix domain socket paths are capped at 103 bytes, so place browser sockets in the host's
/// short private IPC directory rather than under `data_root`.
pub(super) fn sockets(data_root: &Path) -> PathBuf {
    #[cfg(unix)]
    if let Ok(folder) = kodosi_os::ipc::private_dir(&crate::local_host::namespace(data_root)) {
        return folder.join("browser");
    }
    data_root.join("browser").join("sockets")
}

pub(super) fn own_profile(data_root: &Path, id: Uuid) -> PathBuf {
    data_root
        .join("browser")
        .join("profiles")
        .join(id.to_string())
}

pub(super) fn missing(agent_browser: &Path) -> Option<Text> {
    (!agent_browser.is_file()).then(no_agent_browser)
}

fn no_agent_browser() -> Text {
    text!("This Kodosi has no agent-browser for the shared browser.")
}

#[derive(Default)]
struct Activity {
    command_at: Option<Instant>,
    introduced: bool,
    address: Option<String>,
    shown_downloads: HashSet<String>,
}

pub(super) struct Run {
    pub(super) code: i32,
    pub(super) output: String,
}

pub(super) struct Terminal {
    pub(super) id: Uuid,
    pub(super) folder: watch::Receiver<PathBuf>,
    pub(super) shared: watch::Receiver<bool>,
    pub(super) show: Box<dyn Fn(PathBuf) + Send + Sync>,
    pub(super) last_tab_closed: Box<dyn Fn(Weak<Browser>) + Send + Sync>,
    pub(super) restarted: Box<dyn Fn(Weak<Browser>) + Send + Sync>,
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
        let own_sign_ins = *terminal.shared.borrow();
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
                own_sign_ins,
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

    pub(super) fn reopen_at(&self, address: Option<String>) {
        self.activity(|activity| activity.address = address);
    }

    pub(super) fn page(&self) -> Option<String> {
        self.activity(|activity| activity.address.clone())
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

    async fn window(self: &Arc<Self>) -> Result<Window, Text> {
        let open = self
            .window
            .lock()
            .await
            .as_ref()
            .filter(|window| window.is_open())
            .cloned();
        if let Some(window) = open {
            return Ok(window);
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
        let mut open = self.window.lock().await;
        if let Some(window) = open.as_ref().filter(|window| window.is_open()) {
            return Ok(window.clone());
        }
        if self.closed.is_cancelled() {
            return Err(text!("The terminal of this canvas closed."));
        }
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
            .map_err(|problem| self.problem(Text::plain(problem.to_string())))?;
        tokio::spawn(follow(
            Arc::downgrade(self),
            window.clone(),
            self.closed.clone(),
        ));
        *open = Some(window.clone());
        drop(open);
        Ok(window)
    }

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
        if let Some(address) = kept_address(&window.tabs) {
            self.activity(|activity| activity.address = Some(address.to_owned()));
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

    async fn point(&self, folder: &Path, driver: &Driver, target: &str) -> Option<BrowserPoint> {
        let run = self
            .agent_browser(
                folder,
                &[
                    "get".into(),
                    "box".into(),
                    target.to_owned(),
                    "--json".into(),
                ],
                driver,
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

    pub(super) async fn run(
        self: &Arc<Self>,
        folder: PathBuf,
        arguments: Vec<String>,
        terminal: &str,
        agent: Option<String>,
    ) -> Result<CanvasAnswer, Text> {
        if let Some(refused) = refused(&arguments) {
            return Err(Text::plain(format!(
                "Kodosi runs the browser of this terminal, so {refused} is not available here. Use the other commands."
            )));
        }
        let command = asked(&arguments);
        let _one = self.commands.lock().await;
        let window = self.window().await?;
        window.driven().await;
        let driver = window.driver().clone();
        let acting = !(LOOKS.contains(&command) || (command == "tab" && arguments.len() == 1));
        let used = self.activity(|activity| {
            let used = window
                .used_at()
                .map(Instant::from_std)
                .filter(|used| activity.command_at.is_none_or(|command| *used > command))
                .map(|used| used.elapsed().as_secs());
            activity.command_at = Some(Instant::now());
            used
        });
        let agent = agent.unwrap_or_else(|| "The agent".to_owned());
        if let Some((kind, target)) = doing(&arguments) {
            let point = match target.filter(|_| window.is_watched()) {
                Some(target) => self.point(&folder, &driver, &target).await,
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
            .agent_browser(&folder, &arguments, &driver, COMMAND_LIMIT)
            .await;
        let action = acting.then(|| BrowserAction {
            agent,
            words: words(&arguments),
        });
        self.finish(action);
        let run = run.map_err(|problem| self.problem(problem))?;
        let mut text = run.output;
        if cfg!(windows) && text.starts_with(MISSING_WORD) {
            push_line(&mut text, POWERSHELL_WORDS);
        }
        let introduces = run.code == 0
            && self.activity(|activity| !std::mem::replace(&mut activity.introduced, true));
        if introduces {
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

    pub(super) async fn act(
        self: &Arc<Self>,
        BrowserAct::Wake: BrowserAct,
    ) -> Result<CanvasAnswer, Text> {
        self.window().await?;
        Ok(CanvasAnswer::Done)
    }

    pub(super) async fn watch(self: &Arc<Self>, viewer: Viewer) -> Result<PipeEnds, Text> {
        let window = self.window().await?;
        let mut pipe = window
            .watch(viewer)
            .await
            .map_err(|problem| Text::plain(problem.to_string()))?;
        let (sender, mut from_view) = mpsc::channel::<Vec<u8>>(PIPE_QUEUE);
        let (to_view, receiver) = mpsc::channel(PIPE_QUEUE);
        tokio::spawn(async move {
            // Hold one pending frame when `to_view` is full and pause `pipe.receive()` so the
            // window applies backpressure upstream: a slow view gets fewer frames instead of
            // broken ones.
            let mut waiting: Option<Vec<u8>> = None;
            loop {
                tokio::select! {
                    message = from_view.recv() => {
                        let Some(message) = message else { break };
                        if pipe.send(&message).await.is_err() {
                            break;
                        }
                    }
                    room = to_view.reserve(), if waiting.is_some() => {
                        let Ok(room) = room else { break };
                        if let Some(state) = waiting.take() {
                            room.send(state);
                        }
                    }
                    message = pipe.receive(), if waiting.is_none() => {
                        let Some(message) = message else { break };
                        match to_view.try_send(message.to_vec()) {
                            Ok(()) => {}
                            Err(mpsc::error::TrySendError::Full(message)) => waiting = Some(message),
                            Err(mpsc::error::TrySendError::Closed(_)) => break,
                        }
                    }
                }
            }
        });
        Ok(PipeEnds { sender, receiver })
    }

    fn problem(&self, problem: Text) -> Text {
        let english = problem.to_string();
        let shown = if english.chars().count() > 300 {
            Text::plain(english.chars().take(300).collect::<String>())
        } else {
            problem.clone()
        };
        self.change(|state| {
            state.state = BrowserState::Asleep;
            state.getting = None;
            state.problem = Some(shown);
        });
        problem
    }

    /// Switching between shared and private profiles requires closing the running window so the
    /// next wake reopens with the new profile.
    pub(super) async fn change_profile(&self) {
        let shared = *self.terminal.shared.borrow();
        self.change(|state| state.own_sign_ins = shared);
        self.sleep().await;
    }

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

    pub(super) async fn end(&self) {
        self.closed.cancel();
        self.sleep().await;
    }

    pub(super) async fn remove(&self) {
        self.end().await;
        if *self.terminal.shared.borrow()
            && let Err(problem) = self
                .host
                .remove_profile(&self.terminal.id.to_string())
                .await
        {
            tracing::warn!(%problem, "the profile of a closed terminal stays");
        }
        self.forget_pages().await;
    }

    /// Removes `agent-browser` session files; otherwise it treats the closed window's tab as lost
    /// and opens a duplicate tab on the next launch.
    pub(super) async fn forget_pages(&self) {
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

    async fn agent_browser(
        &self,
        folder: &Path,
        arguments: &[String],
        driver: &Driver,
        limit: Duration,
    ) -> Result<Run, Text> {
        if !self.places.binary.is_file() {
            return Err(no_agent_browser());
        }
        prepare(&self.places).await?;
        let mut command = agent_browser_command(&self.places.binary, &self.places.sockets);
        // Pass the CDP credentials in the environment, which other local users can't read
        // (unlike the command line, visible in `ps`). The address goes on the command line:
        // agent-browser takes an address from the environment only for a session that has none
        // yet, and each new window of the terminal gets a new address.
        command
            .env(
                "AGENT_BROWSER_CDP_HEADERS",
                serde_json::json!({ "Authorization": driver.authorization() }).to_string(),
            )
            .arg("--cdp")
            .arg(driver.address())
            .arg("--session")
            .arg(self.terminal.id.simple().to_string())
            .arg("--config")
            .arg(&self.places.config)
            .args(arguments)
            .current_dir(if folder.is_dir() {
                folder
            } else {
                &self.places.sockets
            });
        finished(command, limit).await
    }
}

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
    let Some(browser) = browser.upgrade() else {
        return;
    };
    if window.emptied() {
        (browser.terminal.last_tab_closed)(Arc::downgrade(&browser));
    } else if window.restarted() {
        browser.change(|state| {
            state.state = BrowserState::Starting;
            state.doing = None;
        });
        (browser.terminal.restarted)(Arc::downgrade(&browser));
    } else {
        browser.change(|state| {
            state.state = BrowserState::Asleep;
            state.doing = None;
        });
    }
}

fn cut(text: &str, most: usize) -> String {
    text[..text.floor_char_boundary(most)].to_owned()
}

/// Returns the active HTTP(S) URL to restore across restarts, dropping URLs over
/// `MOST_KEPT_ADDRESS` instead of truncating them.
pub(super) fn kept_address(tabs: &[shared_browser::Tab]) -> Option<&str> {
    tabs.iter()
        .find(|tab| tab.active)
        .map(|tab| tab.address.as_str())
        .filter(|address| address.starts_with("http") && address.len() <= MOST_KEPT_ADDRESS)
}

/// Rejects disallowed flags and subcommands, and `close --all`, which would close other
/// terminals' browsers.
pub(super) fn refused(arguments: &[String]) -> Option<&str> {
    let closing = matches!(asked(arguments), "close" | "quit" | "exit");
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

fn asked(arguments: &[String]) -> &str {
    arguments
        .iter()
        .find(|word| !word.starts_with('-'))
        .map_or("", String::as_str)
}

/// Commands that Kodosi or agent-browser answers without a browser or a terminal.
pub(crate) fn answers_alone(arguments: &[String]) -> bool {
    matches!(asked(arguments), "" | "help" | "skills")
        || arguments
            .iter()
            .any(|word| matches!(word.as_str(), "--help" | "-h" | "--version" | "-V"))
}

/// agent-browser's guides do not come with Kodosi, so its skills are Kodosi's guide.
pub(crate) async fn answer_alone(
    data_root: &Path,
    arguments: &[String],
) -> Result<(String, i32), Text> {
    let asked = asked(arguments);
    if asked == "skills" {
        return Ok((crate::agents::CANVAS_SKILL.to_owned(), 0));
    }
    let binary = agent_browser();
    if !binary.is_file() {
        return Err(no_agent_browser());
    }
    let mut command = agent_browser_command(&binary, &sockets(data_root));
    if asked == "help" {
        command
            .args(arguments.iter().filter(|word| *word != "help"))
            .arg("--help");
    } else {
        command.args(arguments);
    }
    let run = finished(command, QUICK_LIMIT).await?;
    if arguments
        .iter()
        .any(|word| matches!(word.as_str(), "--version" | "-V"))
    {
        return Ok((run.output, run.code));
    }
    Ok((
        format!("{}\n\n{}", help_in_kodosi(), run.output.trim_start()),
        run.code,
    ))
}

/// agent-browser's help names itself, its guides and the options that Kodosi keeps.
fn help_in_kodosi() -> String {
    format!(
        "In Kodosi:\n\
         - Write `kodosi browser` where this help writes `agent-browser`.\n\
         - `kodosi browser skills` gives Kodosi's guide to this browser, as `kodosi skill canvas` does.\n\
         - Kodosi runs the browser of this terminal, so these are not available here: {}, close --all.",
        NOT_HERE.join(", ")
    )
}

/// Caps retained bytes at `MOST_READ` while draining the rest to avoid deadlocking the child on
/// a full pipe.
async fn read_most(
    pipe: Option<impl tokio::io::AsyncRead + Unpin>,
) -> std::io::Result<(Vec<u8>, bool)> {
    use tokio::io::AsyncReadExt as _;

    let mut kept = Vec::new();
    let Some(mut pipe) = pipe else {
        return Ok((kept, false));
    };
    (&mut pipe)
        .take(MOST_READ as u64)
        .read_to_end(&mut kept)
        .await?;
    let rest = tokio::io::copy(&mut pipe, &mut tokio::io::sink()).await?;
    Ok((kept, rest > 0))
}

pub(super) async fn finished(
    mut command: tokio::process::Command,
    limit: Duration,
) -> Result<Run, Text> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| Text::plain(format!("agent-browser did not start: {error}")))?;
    let (stdout, stderr) = (child.stdout.take(), child.stderr.take());
    let (status, (output, output_cut), (errors, errors_cut)) = tokio::time::timeout(limit, async {
        tokio::try_join!(child.wait(), read_most(stdout), read_most(stderr))
    })
    .await
    .map_err(|_| {
        Text::plain(format!(
            "agent-browser did not answer in {} s.",
            limit.as_secs()
        ))
    })?
    .map_err(|error| Text::plain(format!("agent-browser failed: {error}")))?;
    let mut text = String::from_utf8_lossy_owned(output);
    let errors = String::from_utf8_lossy(&errors);
    if !errors.trim().is_empty() {
        push_line(&mut text, errors.trim_end());
    }
    if output_cut || errors_cut || text.len() > MOST_OUTPUT {
        text.truncate(text.floor_char_boundary(MOST_OUTPUT));
        text.push_str("\n…");
    }
    Ok(Run {
        code: status.code().unwrap_or(1),
        output: text.trim_end().to_owned(),
    })
}

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
        // Kodosi manages browser updates itself.
        .env("AGENT_BROWSER_NO_VERSION_CHECK", "1")
        .stdin(Stdio::null())
        .kill_on_drop(true);
    kodosi_os::process::without_terminal(command.as_std_mut());
    command
}

async fn prepare(places: &Places) -> Result<(), Text> {
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
    .map_err(|_| Text::plain("The browser folder could not be made."))?
    .map_err(|error| Text::plain(format!("The browser folder could not be made: {error}")))
}

/// Summarizes a browser command for display, omitting typed/evaluated text that may be secret.
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

pub(super) async fn close_left(data_root: &Path) {
    let sockets = sockets(data_root);
    drop(tokio::task::spawn_blocking(move || std::fs::remove_dir_all(sockets)).await);
}

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
