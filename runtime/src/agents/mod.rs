#[cfg(feature = "host")]
mod accounts;
#[cfg(feature = "host")]
mod models;
#[cfg(feature = "host")]
mod shell;

#[cfg(feature = "host")]
use std::path::Path;

use serde::{Deserialize, Serialize};

/// A coding agent CLI that a new terminal can start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    Claude,
    Codex,
    Copilot,
    Cursor,
    Gemini,
    Opencode,
}

impl Agent {
    pub const ALL: [Self; 6] = [
        Self::Claude,
        Self::Codex,
        Self::Copilot,
        Self::Cursor,
        Self::Gemini,
        Self::Opencode,
    ];

    pub const fn program(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Copilot => "copilot",
            Self::Cursor => "cursor-agent",
            Self::Gemini => "gemini",
            Self::Opencode => "opencode",
        }
    }

    /// The folder in the home folder that keeps the agent's sign-in, and the
    /// variable that moves it.
    #[cfg(feature = "host")]
    const fn home(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Claude => Some((".claude", "CLAUDE_CONFIG_DIR")),
            Self::Codex => Some((".codex", "CODEX_HOME")),
            Self::Copilot => Some((".copilot", "COPILOT_HOME")),
            Self::Cursor | Self::Gemini | Self::Opencode => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentCatalog {
    pub agents: Vec<AgentInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentInfo {
    pub agent: Agent,
    /// The program that starts the agent.
    pub program: String,
    pub installed: bool,
    /// The ways to start the agent, the usual one first.
    pub accounts: Vec<AgentAccount>,
    /// The command line that installs the agent on this computer.
    pub install: String,
    /// Whether Kodosi can give another account its own sign-in folder.
    pub adds_accounts: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentAccount {
    pub name: String,
    /// What a shell runs to start the agent with this account.
    pub command: String,
    /// The e-mail address or user name that the account signed in with.
    pub identity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentModels {
    pub agent: Agent,
    pub models: Vec<AgentModel>,
    pub efforts: Vec<AgentOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentModel {
    pub id: String,
    pub label: String,
    /// The words that select the model on the agent's command line.
    pub argument: String,
    /// The efforts that the model accepts; empty when it accepts all of them.
    pub efforts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentOption {
    pub id: String,
    pub argument: String,
}

/// The agents of this computer, as a new terminal's shell finds them.
#[cfg(feature = "host")]
pub async fn catalog(home: &Path, shell: &Path) -> Result<AgentCatalog, String> {
    let facts = shell::facts(shell, true).await;
    let home = home.to_owned();
    tokio::task::spawn_blocking(move || accounts::catalog(&home, &facts))
        .await
        .map_err(|error| format!("Agent discovery failed: {error}"))
}

/// The models and efforts that the installed `agent` offers.
#[cfg(feature = "host")]
pub async fn models(shell: &Path, agent: Agent) -> AgentModels {
    let facts = shell::facts(shell, false).await;
    models::models(&facts, agent).await
}

/// Finds `program` as a new terminal's shell would.
#[cfg(feature = "host")]
pub async fn executable(shell: &Path, program: &str) -> Option<std::path::PathBuf> {
    shell::facts(shell, false).await.executable(program)
}

/// Makes an empty sign-in folder for another account of `agent`.
#[cfg(feature = "host")]
pub async fn add_account(home: &Path, agent: Agent, name: &str) -> Result<AgentAccount, String> {
    let home = home.to_owned();
    let name = name.to_owned();
    tokio::task::spawn_blocking(move || accounts::add(&home, agent, &name))
        .await
        .map_err(|error| format!("The account was not made: {error}"))?
}

/// `value` as one word in sh, bash, zsh, fish and PowerShell.
#[cfg(feature = "host")]
fn shell_word(value: &str) -> Option<String> {
    if value.is_empty() || value.contains(['\'', '\n', '\r']) {
        return None;
    }
    if value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"._-/:=@+".contains(&byte))
    {
        Some(value.to_owned())
    } else {
        Some(format!("'{value}'"))
    }
}

#[cfg(all(test, feature = "host"))]
mod tests;
