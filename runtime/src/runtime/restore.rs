use std::{
    collections::{BTreeMap, HashSet},
    io::Read as _,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use uuid::Uuid;

use crate::{agents::Agent, terminal::TerminalSession, worktree::Worktree};

use super::saver::Saver;

const FILE: &str = "terminals.json";
const MOST_BYTES: usize = 1024 * 1024;
const SCREENS: &str = "screens";
const SCREEN_MOST_BYTES: usize = 1024 * 1024;
const RESTORED: &[u8] = b"\x1b]8;;\x1b\\\x1b[0m\r\n\x1b[2mRestored\x1b[0m\r\n";
/// At a sign-out or a shutdown, the agent and the host stop together, so a terminal still comes
/// back with an agent that ended a moment before.
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<Worktree>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub canvas: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Sharing {
    pub owner: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub people: Vec<String>,
}

impl Sharing {
    pub(super) fn of(
        owner: Option<&String>,
        room: Option<&String>,
        people: &[String],
    ) -> Option<Self> {
        let owner = owner?.clone();
        (room.is_some() || !people.is_empty()).then(|| Self {
            owner,
            room: room.cloned(),
            people: people.to_vec(),
        })
    }
}

pub(super) struct Restore {
    screens: PathBuf,
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
        Self {
            screens: data_root.join(SCREENS),
            written: pending.clone(),
            pending,
            starting: BTreeMap::new(),
            saver: Saver::file(file, "the terminals that come back"),
        }
    }

    pub(super) fn take(&mut self) -> Vec<Saved> {
        std::mem::take(&mut self.pending)
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
    shell: PathBuf,
    folder: Option<PathBuf>,
    changed: crate::agents::AgentSettings,
) -> Option<AgentLine> {
    let agent = Agent::named(&program)?;
    let (_, variable) = agent.home()?;
    let foreground = terminal.foreground(variable).await.unwrap_or_default();
    let account = agent.account_folder(&home, account_variable(foreground.variable, variable))?;
    let command = restart_command(
        agent,
        &home,
        &account,
        &foreground.arguments,
        &changed,
        &shell,
    );
    let conversation = match foreground.group {
        Some(group) => crate::provider::shown_conversation(agent, account, group, folder).await,
        None => None,
    };
    resume_line(agent, &command, conversation.as_deref()).map(|line| AgentLine {
        line,
        named: conversation.is_some(),
    })
}

/// Kept while the agent runs, so that the terminal still comes back to its conversation when the
/// agent stops before the host asks, or the host ends without asking.
#[derive(Debug, Default)]
pub(super) struct Conversation {
    line: Option<String>,
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

    pub(super) fn found(&mut self, line: Option<String>) {
        self.asking = false;
        if line.is_some() {
            self.line = line;
            self.ended = None;
        }
    }

    pub(super) fn program_changed(&mut self, ended: bool, now: Instant) {
        self.asked = None;
        if ended && self.line.is_some() {
            self.ended.get_or_insert(now);
        } else {
            self.line = None;
            self.ended = None;
        }
    }

    fn line(&self, now: Instant) -> Option<&str> {
        self.line.as_deref().filter(|_| {
            self.ended
                .is_none_or(|ended| now.duration_since(ended) < ENDED_GRACE)
        })
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
            .or_else(|| fresh.map(|fresh| fresh.line)),
    }
}

/// Windows shows no other process's environment, and the programs of its terminals start with
/// the host's.
pub(super) fn account_variable(shown: Option<String>, name: &str) -> Option<String> {
    shown.or_else(|| cfg!(windows).then(|| std::env::var(name).ok()).flatten())
}

pub(super) fn restart_command(
    agent: Agent,
    home: &Path,
    account: &Path,
    arguments: &[String],
    changed: &crate::agents::AgentSettings,
    shell: &Path,
) -> String {
    let mut words = vec![crate::agents::account_command(agent, home, account)];
    words.extend(agent.chosen(arguments, changed));
    agent.with_context(&words.join(" "), shell)
}

pub(super) fn usual_line(program: &str, shell: &Path) -> Option<String> {
    let agent = Agent::named(program)?;
    resume_line(agent, &agent.with_context(agent.program(), shell), None)
}

pub(super) fn resume_line(
    agent: Agent,
    command: &str,
    conversation: Option<&str>,
) -> Option<String> {
    let resume = match (agent, conversation) {
        (Agent::Claude, Some(id)) => format!("--resume {id}"),
        (Agent::Copilot, Some(id)) => format!("--resume={id}"),
        (Agent::Codex, Some(id)) => format!("resume {id}"),
        (Agent::Claude | Agent::Copilot, None) => "--resume".to_owned(),
        (Agent::Codex, None) => "resume".to_owned(),
        (Agent::Cursor | Agent::Gemini | Agent::Opencode, _) => return None,
    };
    Some(format!("{command} {resume}"))
}

pub(super) fn nearest_folder(folder: Option<&str>) -> Option<String> {
    Path::new(folder?)
        .ancestors()
        .find(|folder| folder.is_dir())
        .and_then(Path::to_str)
        .map(str::to_owned)
}
