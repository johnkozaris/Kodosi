//! The sign-in folder that a person's own command gave an agent, as the agent
//! started with it in one of Kodosi's terminals. It tells the accounts what a
//! command does when its text does not say.

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

use super::Agent;

const FILE: &str = "agent-commands.json";
const MAX_COMMANDS: usize = 64;

pub(super) type Learned = HashMap<Agent, BTreeMap<String, PathBuf>>;

pub(super) fn load(store: &Path) -> Learned {
    std::fs::read(store.join(FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Remembers that `command` started `agent` with the sign-in folder `folder`.
pub(super) fn remember(
    store: &Path,
    agent: Agent,
    command: &str,
    folder: PathBuf,
) -> std::io::Result<()> {
    let mut learned = load(store);
    let commands = learned.entry(agent).or_default();
    if commands.get(command) == Some(&folder)
        || (commands.len() >= MAX_COMMANDS && !commands.contains_key(command))
    {
        return Ok(());
    }
    commands.insert(command.to_owned(), folder);
    kodosi_os::fs::write_private(&store.join(FILE), &serde_json::to_vec(&learned)?)
}
