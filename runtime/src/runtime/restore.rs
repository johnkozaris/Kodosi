use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Error, agents::Agent, terminal::TerminalSession, worktree::Worktree};

const FILE: &str = "terminals.json";
const MOST_BYTES: usize = 1024 * 1024;

/// A terminal that an app starts again after its host stopped.
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

/// The room or the people of a terminal, for the account that chose them.
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

/// The terminals of the last host, kept until an app asks for them, and the live terminals.
pub(super) struct Restore {
    file: PathBuf,
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
            written: pending.clone(),
            pending,
            starting: BTreeMap::new(),
        }
    }

    /// The saved terminals to start, when an app asks for them.
    pub(super) fn take(&mut self) -> Vec<Saved> {
        std::mem::take(&mut self.pending)
    }

    /// Saves the terminals that come back, when they changed since the last save.
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
}

/// The command that continues the agent conversation that a terminal shows.
pub(super) async fn agent_line(
    terminal: TerminalSession,
    program: String,
    home: PathBuf,
    shell: PathBuf,
) -> Option<String> {
    let agent = Agent::ALL
        .into_iter()
        .find(|agent| agent.name() == program)?;
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

/// The line for a terminal whose agent was not asked, as when its host ended without a stop.
pub(super) fn usual_line(program: &str, shell: &Path) -> Option<String> {
    let agent = Agent::ALL
        .into_iter()
        .find(|agent| agent.name() == program)?;
    resume_line(agent, &agent.with_context(agent.program(), shell), None)
}

/// Without a known conversation, the agent lists its conversations in that folder to continue.
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

/// The saved folder, or the nearest folder above it that is still there.
pub(super) fn nearest_folder(folder: Option<&str>) -> Option<String> {
    Path::new(folder?)
        .ancestors()
        .find(|folder| folder.is_dir())
        .and_then(Path::to_str)
        .map(str::to_owned)
}
