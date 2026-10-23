#[cfg(feature = "host")]
mod accounts;
#[cfg(feature = "host")]
mod launch;
#[cfg(feature = "host")]
mod models;
#[cfg(feature = "host")]
mod shell;
#[cfg(feature = "host")]
mod usage;

#[cfg(feature = "host")]
pub use launch::Launcher;

#[cfg(feature = "host")]
use std::path::Path;

use serde::{Deserialize, Serialize};

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

    pub const fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Copilot => "copilot",
            Self::Cursor => "cursor",
            Self::Gemini => "gemini",
            Self::Opencode => "opencode",
        }
    }

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

    pub const fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Copilot => "Copilot CLI",
            Self::Cursor => "Cursor",
            Self::Gemini => "Gemini CLI",
            Self::Opencode => "OpenCode",
        }
    }

    #[cfg(feature = "host")]
    pub(crate) const fn prompt_flag(self) -> Option<&'static str> {
        match self {
            Self::Copilot | Self::Gemini => Some("-i"),
            Self::Opencode => Some("--prompt"),
            Self::Claude | Self::Codex | Self::Cursor => None,
        }
    }

    #[cfg(feature = "host")]
    pub(crate) const fn home(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Claude => Some((".claude", "CLAUDE_CONFIG_DIR")),
            Self::Codex => Some((".codex", "CODEX_HOME")),
            Self::Copilot => Some((".copilot", "COPILOT_HOME")),
            Self::Cursor | Self::Gemini | Self::Opencode => None,
        }
    }
}

pub(crate) fn is_agent_name(name: &str) -> bool {
    Agent::ALL.iter().any(|agent| agent.name() == name)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentCatalog {
    pub agents: Vec<AgentInfo>,
    pub kodosi: Option<String>,
    pub choice: AgentChoice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentInfo {
    pub agent: Agent,
    pub program: String,
    pub installed: bool,
    pub accounts: Vec<AgentAccount>,
    pub install: String,
    pub adds_accounts: bool,
    #[serde(default)]
    pub reports_usage: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentAccount {
    pub name: String,
    pub command: String,
    pub identity: Option<String>,
    pub folder: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentModels {
    pub models: Vec<AgentModel>,
    pub efforts: Vec<AgentOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentModel {
    pub id: String,
    pub label: String,
    pub argument: String,
    pub efforts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentOption {
    pub id: String,
    pub argument: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentModelsReply {
    pub models: Vec<AgentModel>,
    pub efforts: Vec<AgentOption>,
    pub choice: AgentChoice,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentChoice {
    pub agent: Option<Agent>,
    pub picks: Vec<AgentPick>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentPick {
    pub agent: Agent,
    pub account: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentStart {
    pub agent: Agent,
    #[serde(default)]
    pub account: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentChoiceChange {
    Tab { agent: Option<Agent> },
    Pick(AgentPick),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentUsageReply {
    pub accounts: Vec<AccountUsage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountUsage {
    pub agent: Agent,
    pub account: String,
    pub limits: Vec<UsageLimit>,
    pub read_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UsageLimit {
    /// The length of a rolling window; none for a calendar month.
    pub hours: Option<u32>,
    pub scope: Option<String>,
    pub used: u8,
    pub level: UsageLevel,
    pub headline: bool,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "lowercase")]
pub enum UsageLevel {
    Normal,
    Near,
    Reached,
}

#[cfg(feature = "host")]
pub async fn catalog(home: &Path, shell: &Path) -> Result<AgentCatalog, String> {
    let facts = shell::facts(shell, true).await;
    let home = home.to_owned();
    tokio::task::spawn_blocking(move || accounts::catalog(&home, &facts))
        .await
        .map_err(|error| format!("Agent discovery failed: {error}"))
}

#[cfg(feature = "host")]
pub async fn models(shell: &Path, agent: Agent) -> AgentModels {
    let facts = shell::facts(shell, false).await;
    models::models(&facts, agent).await
}

#[cfg(feature = "host")]
pub async fn executable(shell: &Path, program: &str) -> Option<std::path::PathBuf> {
    shell::facts(shell, false).await.executable(program)
}

#[cfg(feature = "host")]
pub(crate) struct Runner {
    pub program: std::path::PathBuf,
    pub path: Option<std::ffi::OsString>,
    pub variables: Vec<(String, String)>,
}

#[cfg(feature = "host")]
pub(crate) async fn runner(
    shell: &Path,
    home: &Path,
    agent: Agent,
    account: &AgentAccount,
) -> Option<Runner> {
    let facts = shell::facts(shell, false).await;
    let program = facts.executable(agent.program())?;
    let variables = match (agent.home(), account.folder.as_deref()) {
        (Some((_, variable)), Some(folder)) if account.command.starts_with("env ") => {
            let folder = folder.strip_prefix('~').map_or_else(
                || folder.into(),
                |rest| home.join(rest.trim_start_matches(['/', '\\'])),
            );
            vec![(variable.to_owned(), folder.to_string_lossy().into_owned())]
        }
        (Some((_, variable)), _) => facts
            .variable(variable)
            .map(|value| vec![(variable.to_owned(), value.to_owned())])
            .unwrap_or_default(),
        (None, _) => Vec::new(),
    };
    Some(Runner {
        program,
        path: facts.path,
        variables,
    })
}

#[cfg(feature = "host")]
async fn add_account(
    home: &Path,
    agent: Agent,
    name: &str,
    taken: Vec<String>,
) -> Result<AgentAccount, String> {
    let home = home.to_owned();
    let name = name.to_owned();
    tokio::task::spawn_blocking(move || accounts::add(&home, agent, &name, &taken))
        .await
        .map_err(|error| format!("The account was not made: {error}"))?
}

#[cfg(feature = "host")]
pub fn quote(shell: &Path, text: &str) -> String {
    let name = shell
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match name.as_str() {
        "fish" => format!("'{}'", text.replace('\\', r"\\").replace('\'', r"\'")),
        "pwsh" | "powershell" => {
            let mut word = String::from("'");
            for character in text.chars() {
                // PowerShell also ends a quoted word at the typographic single quotes.
                if matches!(
                    character,
                    '\'' | '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}'
                ) {
                    word.push(character);
                }
                word.push(character);
            }
            word.push('\'');
            word
        }
        "cmd" => {
            let mut word = String::from("\"");
            let mut slashes = 0;
            for character in text.chars() {
                match character {
                    '\\' => slashes += 1,
                    '"' => {
                        word.push_str(&"\\".repeat(slashes * 2 + 1));
                        slashes = 0;
                    }
                    _ => {
                        word.push_str(&"\\".repeat(slashes));
                        slashes = 0;
                    }
                }
                if character != '\\' {
                    word.push(character);
                }
            }
            word.push_str(&"\\".repeat(slashes * 2));
            word.push('"');
            word
        }
        "nu" => {
            let longest = text.split(|c| c != '#').map(str::len).max().unwrap_or(0);
            let hashes = "#".repeat(longest + 1);
            format!("r{hashes}'{text}'{hashes}")
        }
        _ => format!("'{}'", text.replace('\'', r"'\''")),
    }
}

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
