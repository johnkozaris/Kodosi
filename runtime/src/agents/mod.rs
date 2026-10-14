#[cfg(feature = "host")]
mod accounts;
#[cfg(feature = "host")]
mod learned;
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

    /// The option that gives the agent its first prompt in an interactive
    /// session; none when the prompt is the last word alone.
    #[cfg(feature = "host")]
    const fn prompt_flag(self) -> Option<&'static str> {
        match self {
            Self::Copilot | Self::Gemini => Some("-i"),
            Self::Opencode => Some("--prompt"),
            Self::Claude | Self::Codex | Self::Cursor => None,
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
    /// The option before a first prompt; none when the prompt comes last alone.
    pub prompt_flag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentAccount {
    pub name: String,
    /// What a shell runs to start the agent with this account.
    pub command: String,
    /// The e-mail address or user name that the account signed in with.
    pub identity: Option<String>,
    /// The folder that keeps the sign-in, with `~` for the home folder; none
    /// when the agent keeps no such folder or Kodosi cannot tell which one the
    /// command uses. Commands with the same folder share one sign-in.
    pub folder: Option<String>,
    /// The options that the command adds, such as `--yolo`.
    pub options: Option<String>,
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

/// The agents of this computer, as a new terminal's shell finds them, with
/// what the runtime keeps in `store` about the commands that started them.
#[cfg(feature = "host")]
pub async fn catalog(home: &Path, shell: &Path, store: &Path) -> Result<AgentCatalog, String> {
    let facts = shell::facts(shell, true).await;
    let home = home.to_owned();
    let store = store.to_owned();
    tokio::task::spawn_blocking(move || accounts::catalog(&home, &facts, &learned::load(&store)))
        .await
        .map_err(|error| format!("Agent discovery failed: {error}"))
}

/// The variable that moves the sign-in folder of the agent that `program`
/// starts, if that agent keeps one.
#[cfg(feature = "host")]
pub(crate) fn sign_in_variable(program: &str) -> Option<&'static str> {
    Agent::ALL
        .into_iter()
        .find(|agent| agent.program() == program)
        .and_then(Agent::home)
        .map(|(_, variable)| variable)
}

/// Remembers the sign-in folder that the person's own `command` gave the agent
/// that `program` starts: `folder`, the value of its variable, or the usual
/// folder when the agent started without it.
#[cfg(feature = "host")]
pub(crate) fn remember(
    store: &Path,
    home: &Path,
    program: &str,
    command: &str,
    folder: Option<String>,
) {
    let Some(agent) = Agent::ALL
        .into_iter()
        .find(|agent| agent.program() == program)
    else {
        return;
    };
    let Some((usual, _)) = agent.home() else {
        return;
    };
    let folder = folder.map_or_else(|| home.join(usual), std::path::PathBuf::from);
    // A word that only runs the next one, such as `env` before a sign-in folder, names no command.
    if command == program
        || [
            "env",
            "command",
            "exec",
            "builtin",
            "noglob",
            "nocorrect",
            "nohup",
            "time",
        ]
        .contains(&command)
        || !folder.is_absolute()
        || !command
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return;
    }
    if let Err(error) = learned::remember(store, agent, command, folder) {
        tracing::warn!(%error, "the sign-in folder of a command was not kept");
    }
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

/// `text` as one word for `shell`, which a person can type at its prompt.
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
            // A raw string ends at a quote and as many hashes as it opened with.
            let longest = text.split(|c| c != '#').map(str::len).max().unwrap_or(0);
            let hashes = "#".repeat(longest + 1);
            format!("r{hashes}'{text}'{hashes}")
        }
        _ => format!("'{}'", text.replace('\'', r"'\''")),
    }
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
