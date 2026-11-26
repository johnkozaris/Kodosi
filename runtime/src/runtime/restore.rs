use std::{
    collections::{BTreeMap, HashSet},
    io::Read as _,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use uuid::Uuid;

use crate::{
    agents::{Agent, Notes},
    terminal::TerminalSession,
    worktree::Worktree,
};

use super::saver::Saver;

const FILE: &str = "terminals.json";
/// Written when the host stops to install an update, so the next host knows why it starts.
const UPDATING: &str = "updating.json";
/// An update that takes longer than this to come back counts as any other restart: the person
/// may have moved on, so agents wait for them instead of continuing.
const UPDATE_RETURN: Duration = Duration::from_mins(10);
const MOST_BYTES: usize = 1024 * 1024;
const SCREENS: &str = "screens";
const SCREEN_MOST_BYTES: usize = 1024 * 1024;
const RESTORED: &[u8] = b"\x1b]8;;\x1b\\\x1b[0m\r\n\x1b[2mRestored\x1b[0m\r\n";
/// Keeps a recently exited agent's resume command during shutdown/sign-out races where the
/// agent exits just before the host saves state.
const ENDED_GRACE: Duration = Duration::from_secs(10);
const ASK_AGAIN: Duration = Duration::from_secs(10);
const ASK_AGAIN_KNOWN: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Saved {
    pub id: Uuid,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sharing: Option<Sharing>,
    /// The account whose terminal it was, so that a sign-in after a restart knows whom it takes
    /// the terminal from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<Worktree>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub canvas: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quiet_settings: Option<PathBuf>,
    /// The agent worked or waited for an answer when the host saved the terminal.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub busy: bool,
}

/// What a restored agent that stopped in the middle of a task does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Unfinished {
    /// The host stopped for an update and came right back, so the agent goes on by itself.
    Continue,
    /// Anything else stopped it, so the agent waits until someone asks it to go on.
    Offer,
}

/// When the host stopped for an update, in milliseconds since the Unix epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Updating {
    at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Sharing {
    pub owner: String,
    #[serde(rename = "room", default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub people: Vec<String>,
}

impl Sharing {
    pub(super) fn of(
        owner: Option<&String>,
        project: Option<&String>,
        people: &[String],
    ) -> Option<Self> {
        let owner = owner?.clone();
        (project.is_some() || !people.is_empty()).then(|| Self {
            owner,
            project: project.cloned(),
            people: people.to_vec(),
        })
    }
}

pub(super) struct Restore {
    screens: PathBuf,
    updating: PathBuf,
    /// The last host stopped for an update and this one started soon after.
    updated: bool,
    pending: Vec<Saved>,
    pub starting: BTreeMap<Uuid, Saved>,
    written: Vec<Saved>,
    saver: Saver,
}

impl Restore {
    pub(super) fn load(data_root: &Path) -> Self {
        let file = data_root.join(FILE);
        let pending: Vec<Saved> = std::fs::read(&file)
            .ok()
            .filter(|bytes| bytes.len() <= MOST_BYTES)
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let updating = data_root.join(UPDATING);
        let updated = std::fs::read(&updating)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Updating>(&bytes).ok())
            .is_some_and(|updating| came_back(updating.at, crate::identity::now_ms()));
        drop(std::fs::remove_file(&updating));
        Self {
            screens: data_root.join(SCREENS),
            updating,
            updated,
            written: pending.clone(),
            pending,
            starting: BTreeMap::new(),
            saver: Saver::file(file, "the terminals that come back"),
        }
    }

    /// Notes that the host stops to install an update.
    pub(super) fn stop_for_update(&self) {
        let record = Updating {
            at: crate::identity::now_ms(),
        };
        let written = serde_json::to_vec(&record)
            .map_err(std::io::Error::other)
            .and_then(|bytes| kodosi_os::fs::write_private(&self.updating, &bytes));
        if let Err(error) = written {
            tracing::warn!(%error, "could not note that the host stops for an update");
        }
    }

    pub(super) fn unfinished(&self, saved: &Saved) -> Option<Unfinished> {
        (saved.busy && saved.line.is_some()).then_some(if self.updated {
            Unfinished::Continue
        } else {
            Unfinished::Offer
        })
    }

    pub(super) fn take(&mut self) -> Vec<Saved> {
        std::mem::take(&mut self.pending)
    }

    pub(super) fn owner(&self) -> Option<String> {
        self.pending
            .iter()
            .find_map(|terminal| terminal.owner.clone())
    }

    pub(super) fn returning(&self) -> HashSet<Uuid> {
        self.pending.iter().map(|terminal| terminal.id).collect()
    }

    pub(super) fn save(&mut self, live: Vec<Saved>) {
        let terminals = self
            .pending
            .iter()
            .chain(self.starting.values())
            .cloned()
            .chain(live)
            .collect::<Vec<_>>();
        if terminals == self.written && !self.saver.failed() {
            return;
        }
        match serde_json::to_vec(&terminals) {
            Ok(bytes) => {
                self.saver.save(bytes);
                self.written = terminals;
            }
            Err(error) => tracing::warn!(%error, "could not save the terminals that come back"),
        }
    }

    pub(super) async fn saved(&self) {
        self.saver.saved().await;
    }

    pub(super) fn screen_file(&self, id: Uuid) -> PathBuf {
        self.screens.join(id.to_string())
    }

    pub(super) fn keep_screens(&self, screens: Vec<(Uuid, Vec<u8>)>) {
        let saved = self
            .written
            .iter()
            .map(|terminal| terminal.id)
            .collect::<HashSet<_>>();
        for file in std::fs::read_dir(&self.screens)
            .into_iter()
            .flatten()
            .flatten()
        {
            let id = file
                .file_name()
                .to_str()
                .and_then(|name| Uuid::parse_str(name).ok());
            if !id.is_some_and(|id| saved.contains(&id)) {
                drop(std::fs::remove_file(file.path()));
            }
        }
        for (id, screen) in screens {
            let screen = newest_lines(&screen, SCREEN_MOST_BYTES);
            if screen.is_empty() {
                continue;
            }
            if let Err(error) = kodosi_os::fs::write_private(&self.screen_file(id), screen) {
                tracing::warn!(%error, "could not save the screen of a terminal");
            }
        }
    }

    pub(super) fn forget_screen(&self, id: Uuid) {
        drop(std::fs::remove_file(self.screen_file(id)));
    }
}

pub(super) fn newest_lines(screen: &[u8], most: usize) -> &[u8] {
    if screen.len() <= most {
        return screen;
    }
    let start = screen.len() - most;
    screen[start..]
        .windows(2)
        .position(|pair| pair == b"\r\n")
        .map_or(&[], |line| &screen[start + line + 2..])
}

pub(super) fn earlier_screen(file: &Path) -> Vec<u8> {
    let mut screen = Vec::new();
    let read = std::fs::File::open(file).and_then(|file| {
        file.take(SCREEN_MOST_BYTES as u64 + 1)
            .read_to_end(&mut screen)
    });
    if read.is_err() || screen.is_empty() || screen.len() > SCREEN_MOST_BYTES {
        return Vec::new();
    }
    screen.extend_from_slice(RESTORED);
    screen
}

#[derive(Debug)]
pub(super) struct AgentLine {
    pub(super) line: String,
    pub(super) named: bool,
}

pub(super) async fn agent_line(
    terminal: TerminalSession,
    program: String,
    home: PathBuf,
    notes: Notes,
    folder: Option<PathBuf>,
    changed: crate::agents::AgentSettings,
) -> Option<AgentLine> {
    let agent = Agent::named(&program)?;
    let Some((_, variable)) = agent.home() else {
        return server_line(&terminal, &notes).await;
    };
    let foreground = terminal.foreground(variable).await.unwrap_or_default();
    let account = agent.account_folder(&home, account_variable(foreground.variable, variable))?;
    let attached = |session: &str| {
        Some(AgentLine {
            line: attach_line(
                &crate::agents::account_command(agent, &home, &account),
                session,
            ),
            named: true,
        })
    };
    if agent == Agent::Claude
        && let [_, action, session] = foreground.arguments.as_slice()
        && action == "attach"
    {
        return attached(session);
    }
    let command = restart_command(
        agent,
        &home,
        &account,
        &foreground.arguments,
        &changed,
        &notes,
    );
    if agent == Agent::Copilot
        && let Some(folder) = &folder
    {
        super::commands::allow_copilot_link(account.clone(), folder.clone()).await;
    }
    let conversation = match (terminal.chat.conversation().await, foreground.group) {
        (Some(conversation), _) => Some(conversation),
        (None, Some(group)) => {
            crate::provider::shown_conversation(agent, account.clone(), group, folder.clone()).await
        }
        (None, None) => None,
    };
    if agent == Agent::Claude
        && let Some(conversation) = &conversation
        && let Some(session) =
            crate::provider::claude_background(account.clone(), folder, conversation.clone()).await
    {
        return attached(&session);
    }
    resume_line(agent, &command, conversation.as_deref()).map(|line| AgentLine {
        line,
        named: conversation.is_some(),
    })
}

/// Claude Code reattaches a background session to a terminal by the short ID it gives that
/// session.
pub(super) fn attach_line(command: &str, session: &str) -> String {
    let short = session.split('-').next().unwrap_or(session);
    format!("{command} attach {short}")
}

/// Builds a resume command for ACP agents without local account folders by reusing their
/// `kodosi acp` command line with the chat conversation ID.
async fn server_line(terminal: &TerminalSession, notes: &Notes) -> Option<AgentLine> {
    let conversation = terminal.chat.conversation().await?;
    let foreground = terminal
        .foreground(crate::agents::CONTEXT_VARIABLE)
        .await
        .ok()?;
    let line = resumed_server_line(&foreground.arguments, &conversation, &notes.shell)?;
    Some(AgentLine { line, named: true })
}

pub(super) fn resumed_server_line(
    arguments: &[String],
    conversation: &str,
    shell: &Path,
) -> Option<String> {
    let [program, action, rest @ ..] = arguments else {
        return None;
    };
    if Path::new(program).file_name()? != "kodosi" || action != "acp" {
        return None;
    }
    let (options, server) = rest.split_at(rest.iter().position(|word| word == "--")?);
    let mut words = vec![program, action];
    let mut options = options.iter();
    while let Some(option) = options.next() {
        if option == "--resume" {
            options.next();
        } else if !option.starts_with("--resume=") {
            words.push(option);
        }
    }
    let resume = ["--resume".to_owned(), conversation.to_owned()];
    words.splice(2..2, &resume);
    words.extend(server);
    Some(
        words
            .into_iter()
            .map(|word| crate::agents::as_shell_word(shell, word))
            .collect::<Vec<_>>()
            .join(" "),
    )
}

/// Caches the running agent's resume command periodically so restore still works if the agent
/// exits early or the host terminates abruptly.
#[derive(Debug, Default)]
pub(super) struct Conversation {
    line: Option<String>,
    listing: Option<String>,
    quiet_settings: Option<PathBuf>,
    ended: Option<Instant>,
    asking: bool,
    asked: Option<Instant>,
}

impl Conversation {
    pub(super) fn ask(&mut self, now: Instant) -> bool {
        let wait = if self.line.is_some() {
            ASK_AGAIN_KNOWN
        } else {
            ASK_AGAIN
        };
        if self.asking
            || self
                .asked
                .is_some_and(|asked| now.duration_since(asked) < wait)
        {
            return false;
        }
        self.asking = true;
        self.asked = Some(now);
        true
    }

    pub(super) fn found(&mut self, line: Option<AgentLine>) {
        self.asking = false;
        match line {
            Some(AgentLine { line, named: true }) => {
                self.line = Some(line);
                self.ended = None;
            }
            Some(AgentLine { line, named: false }) => self.listing = Some(line),
            None => {}
        }
    }

    pub(super) fn choices_changed(&mut self) {
        self.asked = None;
    }

    pub(super) fn program_changed(
        &mut self,
        ended: bool,
        now: Instant,
        quiet_settings: Option<PathBuf>,
    ) {
        self.asked = None;
        self.listing = None;
        if ended && self.line.is_some() {
            self.ended.get_or_insert(now);
            self.quiet_settings = quiet_settings;
        } else {
            self.line = None;
            self.quiet_settings = None;
            self.ended = None;
        }
    }

    fn line(&self, now: Instant) -> Option<&str> {
        self.line.as_deref().filter(|_| {
            self.ended
                .is_none_or(|ended| now.duration_since(ended) < ENDED_GRACE)
        })
    }

    pub(super) fn quiet_settings(&self, now: Instant) -> Option<PathBuf> {
        self.line(now).and_then(|_| self.quiet_settings.clone())
    }
}

pub(super) fn chosen_line(
    fresh: Option<AgentLine>,
    known: &Conversation,
    now: Instant,
) -> Option<String> {
    match fresh {
        Some(fresh) if fresh.named => Some(fresh.line),
        fresh => known
            .line(now)
            .map(str::to_owned)
            .or_else(|| fresh.map(|fresh| fresh.line))
            .or_else(|| known.listing.clone()),
    }
}

/// On Windows the foreground program's environment is not read, so fall back to the host's own
/// variable, which the terminal's programs inherit.
pub(super) fn account_variable(shown: Option<String>, name: &str) -> Option<String> {
    shown.or_else(|| cfg!(windows).then(|| std::env::var(name).ok()).flatten())
}

pub(super) fn restart_command(
    agent: Agent,
    home: &Path,
    account: &Path,
    arguments: &[String],
    changed: &crate::agents::AgentSettings,
    notes: &Notes,
) -> String {
    let mut words = vec![crate::agents::account_command(agent, home, account)];
    // Resuming via Codex's server preserves the original permissions and rejects
    // overriding them on the command line.
    let through_server = agent == Agent::Codex && notes.codex_servers.is_some();
    words.extend(agent.chosen(arguments, changed, !through_server));
    agent.with_context(&words.join(" "), notes)
}

pub(super) fn usual_line(program: &str, notes: &Notes) -> Option<String> {
    let agent = Agent::named(program)?;
    resume_line(agent, &agent.with_context(agent.program(), notes), None)
}

pub(super) fn resume_line(
    agent: Agent,
    command: &str,
    conversation: Option<&str>,
) -> Option<String> {
    let words = resume_words(agent, conversation)?;
    Some(format!("{command} {}", words.join(" ")))
}

/// With `conversation: None`, passes bare resume flags so the agent opens its conversation
/// picker.
pub(super) fn resume_words(agent: Agent, conversation: Option<&str>) -> Option<Vec<String>> {
    Some(match (agent, conversation) {
        (Agent::Claude, Some(id)) => vec!["--resume".to_owned(), id.to_owned()],
        (Agent::Copilot, Some(id)) => vec![format!("--resume={id}")],
        (Agent::Codex, Some(id)) => vec!["resume".to_owned(), id.to_owned()],
        (Agent::Claude | Agent::Copilot, None) => vec!["--resume".to_owned()],
        (Agent::Codex, None) => vec!["resume".to_owned()],
        (Agent::Cursor | Agent::Opencode | Agent::Antigravity, _) => return None,
    })
}

fn came_back(stopped: u64, now: u64) -> bool {
    u128::from(now.abs_diff(stopped)) <= UPDATE_RETURN.as_millis()
}

pub(super) fn nearest_folder(folder: Option<&str>) -> Option<String> {
    Path::new(folder?)
        .ancestors()
        .find(|folder| folder.is_dir())
        .and_then(Path::to_str)
        .map(str::to_owned)
}
