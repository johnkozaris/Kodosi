use std::{
    collections::{BTreeMap, HashSet},
    io::Read as _,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Error, agents::Agent, terminal::TerminalSession, worktree::Worktree};

const FILE: &str = "terminals.json";
const MOST_BYTES: usize = 1024 * 1024;
const SCREENS: &str = "screens";
const SCREEN_MOST_BYTES: usize = 1024 * 1024;
const RESTORED: &[u8] = b"\x1b]8;;\x1b\\\x1b[0m\r\n\x1b[2mRestored\x1b[0m\r\n";

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
    file: PathBuf,
    screens: PathBuf,
    pending: Vec<Saved>,
    pub starting: BTreeMap<Uuid, Saved>,
    written: Vec<Saved>,
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
            file,
            screens: data_root.join(SCREENS),
            written: pending.clone(),
            pending,
            starting: BTreeMap::new(),
        }
    }

    pub(super) fn take(&mut self) -> Vec<Saved> {
        std::mem::take(&mut self.pending)
    }

    pub(super) fn save(&mut self, live: Vec<Saved>) {
        let terminals = self
            .pending
            .iter()
            .chain(self.starting.values())
            .cloned()
            .chain(live)
            .collect::<Vec<_>>();
        if terminals == self.written {
            return;
        }
        let saved = serde_json::to_vec(&terminals)
            .map_err(Error::from)
            .and_then(|bytes| Ok(kodosi_os::fs::write_private(&self.file, &bytes)?));
        match saved {
            Ok(()) => self.written = terminals,
            Err(error) => tracing::warn!(%error, "could not save the terminals that come back"),
        }
    }

    pub(super) fn screen_file(&self, id: Uuid) -> PathBuf {
        self.screens.join(id.to_string())
    }

    /// Keeps the newest lines of each screen, and removes the screens of terminals that no
    /// longer come back.
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

/// The newest whole lines of a screen that fit in `most` bytes.
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

/// A saved screen and a line that ends it, for the terminal that comes back in its place.
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

pub(super) async fn agent_line(
    terminal: TerminalSession,
    program: String,
    home: PathBuf,
    shell: PathBuf,
) -> Option<String> {
    let agent = Agent::named(&program)?;
    let (usual, variable) = agent.home()?;
    let foreground = terminal.foreground(variable).await.unwrap_or_default();
    let folder = foreground
        .variable
        .map(PathBuf::from)
        .filter(|folder| folder.is_absolute())
        .unwrap_or_else(|| home.join(usual));
    let command = agent.with_context(
        &crate::agents::account_command(agent, &home, &folder),
        &shell,
    );
    let conversation = match (agent, foreground.group) {
        (Agent::Claude, Some(group)) => crate::provider::claude_conversation(folder, group).await,
        _ => None,
    };
    resume_line(agent, &command, conversation.as_deref())
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
