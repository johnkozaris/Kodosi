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

    pub(crate) fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|agent| agent.name() == name)
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
    pub(crate) const fn model_flag(self) -> &'static str {
        match self {
            Self::Codex | Self::Gemini => "-m",
            Self::Claude | Self::Copilot | Self::Cursor | Self::Opencode => "--model",
        }
    }

    /// The flag that chooses the reasoning effort, and the start of its value.
    #[cfg(feature = "host")]
    pub(crate) const fn effort_flag(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Claude => Some(("--effort", "")),
            Self::Codex => Some(("-c", "model_reasoning_effort=")),
            Self::Copilot => Some(("--reasoning-effort", "")),
            Self::Cursor | Self::Gemini | Self::Opencode => None,
        }
    }

    /// The words that start the agent with these permissions, or `None` when it has no such
    /// mode. Asking is the usual mode of the agents that have no flag for it.
    #[cfg(feature = "host")]
    pub(crate) const fn permission_words(
        self,
        permissions: Permissions,
    ) -> Option<&'static [&'static str]> {
        match (self, permissions) {
            (Self::Claude, Permissions::Ask) => Some(&["--permission-mode", "manual"]),
            (Self::Claude, Permissions::Skip) => Some(&["--dangerously-skip-permissions"]),
            (Self::Claude, Permissions::Auto) => Some(&["--permission-mode", "auto"]),
            (Self::Codex, Permissions::Ask) => Some(&["-a", "on-request", "-s", "read-only"]),
            (Self::Codex, Permissions::Skip) => {
                Some(&["--dangerously-bypass-approvals-and-sandbox"])
            }
            (Self::Codex, Permissions::Auto) => {
                Some(&["-a", "on-request", "-s", "workspace-write"])
            }
            (Self::Copilot | Self::Cursor | Self::Opencode, Permissions::Ask) => Some(&[]),
            (Self::Copilot | Self::Gemini, Permissions::Skip) => Some(&["--yolo"]),
            (Self::Copilot, Permissions::Auto) => Some(&["--assisted-approval"]),
            (Self::Gemini, Permissions::Ask) => Some(&["--approval-mode", "default"]),
            (Self::Gemini, Permissions::Auto) => Some(&["--approval-mode", "auto_edit"]),
            (Self::Cursor, Permissions::Skip) => Some(&["--force"]),
            (Self::Opencode, Permissions::Auto) => Some(&["--auto"]),
            (Self::Cursor, Permissions::Auto) | (Self::Opencode, Permissions::Skip) => None,
        }
    }

    /// The words that start the agent with this output, or `None` when it has no such choice.
    #[cfg(feature = "host")]
    pub(crate) const fn output_words(self, output: Output) -> Option<&'static [&'static str]> {
        match (self, output) {
            (_, Output::Standard) => Some(&[]),
            (Self::Claude, Output::Quiet) => Some(&["--settings", r#"{"viewMode":"focus"}"#]),
            (Self::Claude, Output::Detailed) => Some(&["--settings", r#"{"viewMode":"verbose"}"#]),
            (Self::Codex, Output::Quiet) => Some(&[
                "-c",
                "hide_agent_reasoning=true",
                "-c",
                "model_reasoning_summary=none",
                "-c",
                "tui.auto_recap=false",
            ]),
            (Self::Codex, Output::Detailed) => Some(&["-c", "model_reasoning_summary=detailed"]),
            (Self::Copilot | Self::Cursor | Self::Gemini | Self::Opencode, _) => None,
        }
    }

    #[cfg(feature = "host")]
    pub(crate) fn permissions(self) -> Vec<Permissions> {
        Permissions::ALL
            .into_iter()
            .filter(|permissions| self.permission_words(*permissions).is_some())
            .collect()
    }

    #[cfg(feature = "host")]
    pub(crate) fn outputs(self) -> Vec<Output> {
        let outputs: Vec<_> = Output::ALL
            .into_iter()
            .filter(|output| self.output_words(*output).is_some())
            .collect();
        if outputs.len() > 1 {
            outputs
        } else {
            Vec::new()
        }
    }

    /// The flags that choose how the agent asks and what it shows, also those that a person
    /// types by hand, each with the start of the value that it keeps, or `None` for a switch.
    #[cfg(feature = "host")]
    const fn mode_flags(self) -> &'static [(&'static str, Option<&'static str>)] {
        match self {
            Self::Claude => &[
                ("--permission-mode", Some("")),
                ("--dangerously-skip-permissions", None),
                ("--allow-dangerously-skip-permissions", None),
                ("--settings", Some(r#"{"viewMode""#)),
                ("--verbose", None),
            ],
            Self::Codex => &[
                ("-a", Some("")),
                ("--ask-for-approval", Some("")),
                ("-s", Some("")),
                ("--sandbox", Some("")),
                ("--dangerously-bypass-approvals-and-sandbox", None),
                ("--yolo", None),
                ("--full-auto", None),
                ("-c", Some("hide_agent_reasoning=")),
                ("-c", Some("model_reasoning_summary=")),
                ("-c", Some("tui.auto_recap=")),
            ],
            Self::Copilot => &[
                ("--yolo", None),
                ("--allow-all", None),
                ("--allow-all-tools", None),
                ("--allow-all-paths", None),
                ("--allow-all-urls", None),
                ("--assisted-approval", None),
            ],
            Self::Gemini => &[
                ("--approval-mode", Some("")),
                ("--yolo", None),
                ("-y", None),
            ],
            Self::Cursor => &[("--force", None), ("-f", None), ("--yolo", None)],
            Self::Opencode => &[
                ("--auto", None),
                ("--yolo", None),
                ("--dangerously-skip-permissions", None),
            ],
        }
    }

    /// The model, effort, permissions and output of the agent that runs with `arguments`, in
    /// the words of the launcher.
    #[cfg(feature = "host")]
    pub(crate) fn chosen(self, arguments: &[String]) -> Vec<String> {
        let mut words: Vec<_> = [Some((self.model_flag(), "")), self.effort_flag()]
            .into_iter()
            .flatten()
            .filter_map(|(flag, start)| {
                let value = last_value(arguments, flag, start)?;
                Some(format!("{flag} {}", shell_word(value)?))
            })
            .collect();
        for (flag, start) in self.mode_flags() {
            let word = start.map_or_else(
                || {
                    arguments
                        .iter()
                        .skip(1)
                        .any(|argument| argument == flag)
                        .then(|| (*flag).to_owned())
                },
                |start| {
                    last_value(arguments, flag, start)
                        .and_then(shell_word)
                        .map(|value| format!("{flag} {value}"))
                },
            );
            words.extend(word);
        }
        words
    }

    /// The words that give a launched agent Kodosi's note. Codex reads it only from its command
    /// line; Claude Code and Copilot CLI read Kodosi's note and skills from the environment of
    /// each Kodosi terminal, also when a person starts them by hand.
    #[cfg(feature = "host")]
    pub(crate) fn context_words(self, shell: &Path) -> Option<String> {
        match self {
            Self::Codex => Some(format!(
                "-c {}",
                variable_word(shell, "developer_instructions=", CONTEXT_VARIABLE)
            )),
            Self::Claude | Self::Copilot | Self::Cursor | Self::Gemini | Self::Opencode => None,
        }
    }

    /// Whether the agent that runs with `arguments` missed Kodosi's note, because a person
    /// started it by hand. Windows shows no arguments, so there it never misses it.
    #[cfg(feature = "host")]
    pub(crate) fn outside(self, arguments: &[String]) -> bool {
        self.context_words(Path::new("")).is_some()
            && !arguments.is_empty()
            && !arguments.iter().any(|argument| {
                argument.starts_with("developer_instructions=") && argument.contains("Kodosi")
            })
    }

    #[cfg(feature = "host")]
    pub(crate) fn with_context(self, command: &str, shell: &Path) -> String {
        self.context_words(shell)
            .map_or_else(|| command.to_owned(), |words| format!("{command} {words}"))
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

    /// The account folder of a running agent: its folder variable when that is an absolute
    /// path, else the usual folder in `home`.
    #[cfg(feature = "host")]
    pub(crate) fn account_folder(
        self,
        home: &Path,
        variable: Option<String>,
    ) -> Option<std::path::PathBuf> {
        let (usual, _) = self.home()?;
        Some(
            variable
                .map(std::path::PathBuf::from)
                .filter(|folder| folder.is_absolute())
                .map_or_else(|| home.join(usual), |folder| folder.components().collect()),
        )
    }
}

pub(crate) fn is_agent_name(name: &str) -> bool {
    Agent::named(name).is_some()
}

#[cfg(feature = "host")]
pub(crate) const CONTEXT: &str = include_str!("../../skills/kodosi/CONTEXT.md");
#[cfg(feature = "host")]
pub(crate) const CONTEXT_VARIABLE: &str = "KODOSI_AGENT_CONTEXT";
#[cfg(feature = "host")]
pub(crate) const COPILOT_INSTRUCTIONS: &str = "COPILOT_CUSTOM_INSTRUCTIONS_DIRS";

#[cfg(feature = "host")]
pub(crate) const COPILOT_SKILLS: &str = "COPILOT_SKILLS_DIRS";
#[cfg(feature = "host")]
pub(crate) const CLAUDE_PLUGINS: &str = "CLAUDE_CODE_PLUGIN_DIRS";
#[cfg(feature = "host")]
pub(crate) const SKILL: &str = include_str!("../../skills/kodosi/SKILL.md");
#[cfg(feature = "host")]
pub(crate) const CANVAS_SKILL: &str = include_str!("../../skills/kodosi-canvas/SKILL.md");
#[cfg(feature = "host")]
pub(crate) const DOCUMENTS_SKILL: &str = include_str!("../../skills/kodosi-documents/SKILL.md");
#[cfg(feature = "host")]
pub(crate) const PAGE: &str = include_str!("../../skills/kodosi-canvas/assets/page.html");
#[cfg(feature = "host")]
const PLUGIN: &str = "plugin";

/// Claude Code's start of a session in a Kodosi terminal: it reads Kodosi's note there.
#[cfg(feature = "host")]
const CLAUDE_HOOKS: &str = r#"{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "kodosi skill context"
          }
        ]
      }
    ]
  }
}
"#;

/// Writes Kodosi's note for agents, and the plugin with its skills for agents that load plugins
/// or skills from a folder. Agents read both only in Kodosi terminals; nothing goes into their
/// own folders.
#[cfg(feature = "host")]
pub(crate) fn write_notes(folder: &Path) -> std::io::Result<()> {
    let plugin = folder.join(PLUGIN);
    let manifest = format!(
        "{{\n  \"name\": \"kodosi\",\n  \"version\": \"{}\",\n  \"description\": \"Work with the people and agents in Kodosi, and show them what you make on the canvas of a terminal.\"\n}}\n",
        env!("KODOSI_VERSION")
    );
    let files = [
        (
            folder
                .join(".github")
                .join("instructions")
                .join("kodosi.instructions.md"),
            format!("---\napplyTo: \"**\"\n---\n\n{}\n", CONTEXT.trim()),
        ),
        (plugin.join(".claude-plugin").join("plugin.json"), manifest),
        (
            plugin.join("hooks").join("hooks.json"),
            CLAUDE_HOOKS.to_owned(),
        ),
        (
            plugin.join("skills").join("kodosi").join("SKILL.md"),
            SKILL.to_owned(),
        ),
        (
            plugin.join("skills").join("kodosi-canvas").join("SKILL.md"),
            CANVAS_SKILL.to_owned(),
        ),
        (
            plugin
                .join("skills")
                .join("kodosi-canvas")
                .join("assets")
                .join("page.html"),
            PAGE.to_owned(),
        ),
        (
            plugin
                .join("skills")
                .join("kodosi-documents")
                .join("SKILL.md"),
            DOCUMENTS_SKILL.to_owned(),
        ),
    ];
    for (file, text) in files {
        if std::fs::read_to_string(&file).is_ok_and(|current| current == text) {
            continue;
        }
        if let Some(parent) = file.parent() {
            kodosi_os::fs::create_private_dir(parent)?;
        }
        kodosi_os::fs::write_private(&file, text.as_bytes())?;
    }
    Ok(())
}

/// The plugin folders for Claude Code in a Kodosi terminal: the person's own, then Kodosi's.
#[cfg(feature = "host")]
pub(crate) fn plugin_dirs(notes: &Path) -> Option<String> {
    let plugin = plugin_dir(notes)?;
    let separator = if cfg!(windows) { ";" } else { ":" };
    Some(
        std::env::var(CLAUDE_PLUGINS)
            .ok()
            .filter(|folders| !folders.is_empty())
            .map_or_else(
                || plugin.clone(),
                |folders| format!("{folders}{separator}{plugin}"),
            ),
    )
}

/// The skill folders for Copilot CLI in a Kodosi terminal: the person's own, then Kodosi's.
#[cfg(feature = "host")]
pub(crate) fn skill_dirs(notes: &Path) -> Option<String> {
    let skills = Path::new(&plugin_dir(notes)?).join("skills");
    let skills = skills.to_str()?.to_owned();
    Some(
        std::env::var(COPILOT_SKILLS)
            .ok()
            .filter(|folders| !folders.is_empty())
            .map_or_else(|| skills.clone(), |folders| format!("{folders},{skills}")),
    )
}

#[cfg(feature = "host")]
pub(crate) fn plugin_dir(notes: &Path) -> Option<String> {
    let plugin = notes.join(PLUGIN);
    plugin
        .join(".claude-plugin")
        .join("plugin.json")
        .is_file()
        .then(|| plugin.to_str().map(str::to_owned))
        .flatten()
}

#[cfg(feature = "host")]
pub(crate) fn variable_word(shell: &Path, prefix: &str, variable: &str) -> String {
    match shell_name(shell).as_str() {
        "pwsh" | "powershell" => format!("\"{prefix}$env:{variable}\""),
        "cmd" => format!("\"{prefix}%{variable}%\""),
        "nu" => format!("$\"{prefix}($env.{variable})\""),
        _ => format!("\"{prefix}${variable}\""),
    }
}

#[cfg(feature = "host")]
fn shell_name(shell: &Path) -> String {
    shell
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

#[cfg(feature = "host")]
pub(crate) fn account_command(agent: Agent, home: &Path, folder: &Path) -> String {
    agent
        .home()
        .filter(|(usual, _)| folder != home.join(usual))
        .and_then(|(_, variable)| accounts::home_command(variable, folder, agent.program()))
        .unwrap_or_else(|| agent.program().to_owned())
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
    pub reports_usage: bool,
    /// How the agent can ask before it acts, in the order of the launcher.
    pub permissions: Vec<Permissions>,
    /// How much of its work the agent can show; empty when it has no choice.
    pub outputs: Vec<Output>,
}

/// How an agent asks before it acts, through its own flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum Permissions {
    Ask,
    Skip,
    Auto,
}

impl Permissions {
    pub const ALL: [Self; 3] = [Self::Ask, Self::Skip, Self::Auto];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Skip => "skip",
            Self::Auto => "auto",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|permissions| permissions.name() == name)
    }
}

/// How much of its work an agent shows, through its own settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum Output {
    Quiet,
    Standard,
    Detailed,
}

impl Output {
    pub const ALL: [Self; 3] = [Self::Quiet, Self::Standard, Self::Detailed];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Quiet => "quiet",
            Self::Standard => "standard",
            Self::Detailed => "detailed",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|output| output.name() == name)
    }
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
    #[serde(default)]
    pub permissions: Option<Permissions>,
    #[serde(default)]
    pub output: Option<Output>,
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
    #[serde(default)]
    pub permissions: Option<Permissions>,
    #[serde(default)]
    pub output: Option<Output>,
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
) -> Result<AgentAccount, crate::text::Text> {
    let home = home.to_owned();
    let name = name.to_owned();
    tokio::task::spawn_blocking(move || accounts::add(&home, agent, &name, &taken))
        .await
        .map_err(|error| {
            crate::text::Text::plain(format!("Kodosi could not make the account: {error}"))
        })?
}

/// Quotes text as one word that the shell gives a program unchanged.
///
/// Line breaks and tabs go as escapes, because a typed line break ends the command in some
/// shells and a typed tab completes. cmd cannot pass them, so there they become spaces.
#[cfg(feature = "host")]
pub fn quote(shell: &Path, text: &str) -> String {
    let shell = shell_name(shell);
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    if shell == "cmd" {
        return quote_part(&shell, &text.replace(['\n', '\t'], " "));
    }
    let mut parts = Vec::new();
    let mut rest = text.as_str();
    while let Some(at) = rest.find(['\n', '\t']) {
        if at > 0 {
            parts.push(quote_part(&shell, &rest[..at]));
        }
        parts.push(escape(&shell, rest.as_bytes()[at] == b'\n').to_owned());
        rest = &rest[at + 1..];
    }
    if !rest.is_empty() || parts.is_empty() {
        parts.push(quote_part(&shell, rest));
    }
    match shell.as_str() {
        "pwsh" | "powershell" if parts.len() > 1 => format!("({})", parts.join("+")),
        "nu" if parts.len() > 1 => format!("({})", parts.join(" ++ ")),
        _ => parts.concat(),
    }
}

#[cfg(feature = "host")]
fn escape(shell: &str, line_break: bool) -> &'static str {
    match (shell, line_break) {
        ("pwsh" | "powershell", true) => "\"`n\"",
        ("pwsh" | "powershell", false) => "\"`t\"",
        ("nu", true) => r#""\n""#,
        ("nu", false) => r#""\t""#,
        ("fish", true) => r"\n",
        ("fish", false) => r"\t",
        (_, true) => r"$'\n'",
        (_, false) => r"$'\t'",
    }
}

#[cfg(feature = "host")]
fn quote_part(shell: &str, text: &str) -> String {
    match shell {
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
    if plain(value) {
        Some(value.to_owned())
    } else {
        Some(format!("'{value}'"))
    }
}

/// Whether each shell takes `word` as it is.
#[cfg(feature = "host")]
fn plain(word: &str) -> bool {
    word.bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"._-/:=@+".contains(&byte))
}

/// `word` as `shell` takes it unchanged: a plain word stays, any other gets quotes.
#[cfg(feature = "host")]
pub(crate) fn typed(shell: &Path, word: &str) -> String {
    if !word.is_empty() && plain(word) {
        word.to_owned()
    } else {
        quote(shell, word)
    }
}

/// The last value of `flag` in `arguments` that starts with `start`.
#[cfg(feature = "host")]
fn last_value<'a>(arguments: &'a [String], flag: &str, start: &str) -> Option<&'a str> {
    let mut found = None;
    let mut rest = arguments.iter().skip(1);
    while let Some(argument) = rest.next() {
        let value = if argument == flag {
            rest.next().map(String::as_str)
        } else {
            argument
                .strip_prefix(flag)
                .and_then(|value| value.strip_prefix('='))
        };
        if let Some(value) = value.filter(|value| value.starts_with(start)) {
            found = Some(value);
        }
    }
    found
}

#[cfg(all(test, feature = "host"))]
mod tests;
