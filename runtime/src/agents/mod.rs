#[cfg(feature = "host")]
mod accounts;
#[cfg(feature = "host")]
mod launch;
mod live;
#[cfg(feature = "host")]
mod models;
#[cfg(feature = "host")]
mod quiet;
#[cfg(feature = "host")]
mod shell;
#[cfg(feature = "host")]
mod usage;

#[cfg(feature = "host")]
pub use launch::Launcher;
#[cfg(feature = "host")]
pub(crate) use launch::{Launch, RunningAgent};
#[cfg(feature = "host")]
pub(crate) use live::NO_RUNNING_AGENT;
#[cfg(feature = "host")]
pub(crate) use live::asks;
pub(crate) use live::live_of;
pub use live::{AgentLive, AgentSettings};
#[cfg(feature = "host")]
pub(crate) use usage::{claude_session, codex as codex_limits};

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
    Opencode,
    Antigravity,
}

impl Agent {
    pub const ALL: [Self; 6] = [
        Self::Claude,
        Self::Codex,
        Self::Copilot,
        Self::Cursor,
        Self::Opencode,
        Self::Antigravity,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Copilot => "copilot",
            Self::Cursor => "cursor",
            Self::Opencode => "opencode",
            Self::Antigravity => "antigravity",
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
            Self::Opencode => "opencode",
            Self::Antigravity => "agy",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Copilot => "Copilot CLI",
            Self::Cursor => "Cursor",
            Self::Opencode => "OpenCode",
            Self::Antigravity => "Antigravity CLI",
        }
    }

    #[cfg(feature = "host")]
    pub(crate) const fn prompt_flag(self) -> Option<&'static str> {
        match self {
            Self::Copilot | Self::Antigravity => Some("-i"),
            Self::Opencode => Some("--prompt"),
            Self::Claude | Self::Codex | Self::Cursor => None,
        }
    }

    #[cfg(feature = "host")]
    pub(crate) const fn model_flag(self) -> &'static str {
        match self {
            Self::Codex => "-m",
            Self::Claude | Self::Copilot | Self::Cursor | Self::Opencode | Self::Antigravity => {
                "--model"
            }
        }
    }

    #[cfg(feature = "host")]
    pub(crate) const fn effort_flag(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Claude | Self::Antigravity => Some(("--effort", "")),
            Self::Codex => Some(("-c", "model_reasoning_effort=")),
            Self::Copilot => Some(("--reasoning-effort", "")),
            Self::Cursor | Self::Opencode => None,
        }
    }

    #[cfg(feature = "host")]
    pub(crate) const fn permission_words(
        self,
        permissions: Permissions,
    ) -> Option<&'static [&'static str]> {
        match (self, permissions) {
            (Self::Claude, Permissions::Ask) => Some(&["--permission-mode", "manual"]),
            (Self::Claude, Permissions::Auto) => Some(&["--permission-mode", "auto"]),
            (Self::Claude, Permissions::Unrestricted) => Some(&["--dangerously-skip-permissions"]),
            (Self::Codex, Permissions::Ask) => Some(&["-a", "on-request", "-s", "read-only"]),
            (Self::Codex, Permissions::Auto) => {
                Some(&["-a", "on-request", "-s", "workspace-write"])
            }
            (Self::Codex, Permissions::Unrestricted) => {
                Some(&["--dangerously-bypass-approvals-and-sandbox"])
            }
            (Self::Copilot | Self::Cursor | Self::Opencode, Permissions::Ask) => Some(&[]),
            (Self::Copilot, Permissions::Auto) => Some(&["--assisted-approval"]),
            (Self::Copilot, Permissions::Unrestricted) => Some(&["--yolo"]),
            (Self::Cursor, Permissions::Unrestricted) => Some(&["--force"]),
            (Self::Opencode, Permissions::Auto) => Some(&["--auto"]),
            (Self::Antigravity, Permissions::Ask) => Some(&["--settings", ANTIGRAVITY_ASK]),
            (Self::Antigravity, Permissions::Auto) => Some(&["--mode", "auto"]),
            (Self::Antigravity, Permissions::Unrestricted) => {
                Some(&["--dangerously-skip-permissions"])
            }
            (Self::Cursor, Permissions::Auto) | (Self::Opencode, Permissions::Unrestricted) => None,
        }
    }

    #[cfg(feature = "host")]
    pub(crate) const fn output_words(self, output: Output) -> Option<&'static [&'static str]> {
        match (self, output) {
            // Copilot CLI takes its quiet output only from its saved settings.
            (_, Output::Standard) | (Self::Copilot, Output::Quiet) => Some(&[]),
            (Self::Claude, Output::Quiet) => Some(&["--settings", CLAUDE_QUIET]),
            (Self::Claude, Output::Detailed) => Some(&["--settings", CLAUDE_DETAILED]),
            (Self::Codex, Output::Quiet) => Some(&[
                "-c",
                "hide_agent_reasoning=true",
                "-c",
                "show_raw_agent_reasoning=false",
                "-c",
                "model_reasoning_summary=none",
                "-c",
                "tui.auto_recap=false",
                "-c",
                "tui.show_tooltips=false",
                "-c",
                "tui.animations=false",
                "-c",
                "tui.status_line=[]",
            ]),
            (Self::Codex, Output::Detailed) => Some(&[
                "-c",
                "hide_agent_reasoning=false",
                "-c",
                "model_reasoning_summary=detailed",
            ]),
            (Self::Antigravity, Output::Quiet) => Some(&["--settings", ANTIGRAVITY_QUIET]),
            (Self::Antigravity, Output::Detailed) => Some(&["--settings", ANTIGRAVITY_DETAILED]),
            (Self::Copilot, Output::Detailed) | (Self::Cursor | Self::Opencode, _) => None,
        }
    }

    /// Claude Code ranks the `fastMode` of its launch `--settings` above a later `/fast off` and
    /// reads it again at each model change, which turns fast mode back on. `/fast` keeps its
    /// `--fast-mode` in step, although `claude --help` does not list it.
    #[cfg(feature = "host")]
    pub(crate) const fn fast_words(self, fast: bool) -> Option<&'static [&'static str]> {
        match (self, fast) {
            (Self::Claude, true) => Some(&["--fast-mode", "on"]),
            (Self::Claude, false) => Some(&["--fast-mode", "off"]),
            (Self::Codex, true) => Some(&["-c", "service_tier=fast"]),
            (Self::Codex, false) => Some(&["-c", "service_tier=default"]),
            (Self::Copilot | Self::Cursor | Self::Opencode | Self::Antigravity, _) => None,
        }
    }

    /// Antigravity CLI's terminal app reports to no chat, so its chat has a session of its own
    /// that `kodosi acp` runs in the terminal, where the chat's link reaches a socket file.
    #[cfg(feature = "host")]
    pub(crate) const fn runs_in_chat(self) -> bool {
        CHAT_SESSIONS && cfg!(unix) && matches!(self, Self::Antigravity)
    }

    #[cfg(feature = "host")]
    pub(crate) const fn has_fast(self) -> bool {
        self.fast_words(true).is_some()
    }

    #[cfg(feature = "host")]
    pub(crate) fn permissions(self) -> Vec<Permissions> {
        Permissions::ALL
            .into_iter()
            .filter(|permissions| self.permission_words(*permissions).is_some())
            .collect()
    }

    /// Antigravity CLI starts in its own default mode, which is auto. Kodosi turns its checks off
    /// only when a person chooses that.
    #[cfg(feature = "host")]
    pub(crate) fn default_permissions(self) -> Permissions {
        if self == Self::Antigravity {
            Permissions::Auto
        } else if self.permission_words(Permissions::Unrestricted).is_some() {
            Permissions::Unrestricted
        } else {
            Permissions::Ask
        }
    }

    #[cfg(feature = "host")]
    pub(crate) fn default_output(self) -> Output {
        if self.output_words(Output::Quiet).is_some() {
            Output::Quiet
        } else {
            Output::Standard
        }
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

    #[cfg(feature = "host")]
    const fn mode_flags(self) -> &'static [(&'static str, Option<&'static str>, Setting)] {
        use Setting::{Fast, Output, Permissions};
        match self {
            Self::Claude => &[
                ("--permission-mode", Some(""), Permissions),
                ("--dangerously-skip-permissions", None, Permissions),
                ("--allow-dangerously-skip-permissions", None, Permissions),
                ("--settings", Some(r#"{"viewMode""#), Output),
                ("--verbose", None, Output),
                ("--fast-mode", Some(""), Fast),
            ],
            Self::Codex => &[
                ("-a", Some(""), Permissions),
                ("--ask-for-approval", Some(""), Permissions),
                ("-s", Some(""), Permissions),
                ("--sandbox", Some(""), Permissions),
                (
                    "--dangerously-bypass-approvals-and-sandbox",
                    None,
                    Permissions,
                ),
                ("--yolo", None, Permissions),
                ("--full-auto", None, Permissions),
                ("-c", Some("hide_agent_reasoning="), Output),
                ("-c", Some("show_raw_agent_reasoning="), Output),
                ("-c", Some("model_reasoning_summary="), Output),
                ("-c", Some("tui.auto_recap="), Output),
                ("-c", Some("tui.show_tooltips="), Output),
                ("-c", Some("tui.animations="), Output),
                ("-c", Some("tui.status_line="), Output),
                ("-c", Some("service_tier="), Fast),
            ],
            Self::Copilot => &[
                ("--yolo", None, Permissions),
                ("--allow-all", None, Permissions),
                ("--allow-all-tools", None, Permissions),
                ("--allow-all-paths", None, Permissions),
                ("--allow-all-urls", None, Permissions),
                ("--assisted-approval", None, Permissions),
            ],
            Self::Cursor => &[
                ("--force", None, Permissions),
                ("-f", None, Permissions),
                ("--yolo", None, Permissions),
            ],
            Self::Opencode => &[
                ("--auto", None, Permissions),
                ("--yolo", None, Permissions),
                ("--dangerously-skip-permissions", None, Permissions),
            ],
            Self::Antigravity => &[
                ("--mode", Some(""), Permissions),
                ("--dangerously-skip-permissions", None, Permissions),
                ("--sandbox", None, Permissions),
                ("--settings", Some("{"), Output),
            ],
        }
    }

    /// The settings of a running agent as words of its start line, with the changed ones in
    /// place of what it started with. Without `permissions`, the line leaves the agent's
    /// permissions to it.
    #[cfg(feature = "host")]
    pub(crate) fn chosen(
        self,
        arguments: &[String],
        changed: &AgentSettings,
        permissions: bool,
    ) -> Vec<String> {
        let model = changed.model.clone();
        let effort = self
            .effort_flag()
            .and_then(|(_, start)| Some(format!("{start}{}", changed.effort.as_deref()?)));
        let effort_flag = self
            .effort_flag()
            .filter(|_| changed.model.is_none() || changed.effort.is_some());
        let mut words: Vec<_> = [
            Some((self.model_flag(), "", model)),
            effort_flag.map(|(flag, start)| (flag, start, effort)),
        ]
        .into_iter()
        .flatten()
        .filter_map(|(flag, start, now)| {
            let value = now.or_else(|| last_value(arguments, flag, start).map(str::to_owned))?;
            Some(format!("{flag} {}", shell_word(&value)?))
        })
        .collect();
        for (flag, start, setting) in self.mode_flags() {
            let replaced = match setting {
                Setting::Permissions => !permissions || changed.permissions.is_some(),
                Setting::Output => changed.output.is_some(),
                Setting::Fast => changed.fast.is_some(),
            };
            if replaced {
                continue;
            }
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
        let now = [
            changed
                .permissions
                .filter(|_| permissions)
                .and_then(|permissions| self.permission_words(permissions)),
            changed.output.and_then(|output| self.output_words(output)),
            changed.fast.and_then(|fast| self.fast_words(fast)),
        ];
        words.extend(
            now.into_iter()
                .flatten()
                .flatten()
                .filter_map(|word| shell_word(word)),
        );
        words
    }

    /// Codex works through the server of its terminal, which Kodosi starts with the note, so that
    /// the terminal's chat follows it. Where the host has no such server, Codex reads Kodosi's
    /// note only from its command line, and without `--no-daemon` it works in its shared
    /// background server, which has the variables of whatever started that server. Cursor loads
    /// a plugin only from its command line. `OpenCode` reads instructions only from its
    /// configuration, and `OpenCode` 2 reads that in its shared background server unless it runs
    /// with `--standalone`, so the configuration goes only to the program that Kodosi starts.
    /// Antigravity CLI loads a plugin only from its own folders or from a folder that its line adds
    /// to its workspace, and it takes the first folder there as the one where it works, so it gets
    /// no note.
    #[cfg(feature = "host")]
    pub(crate) fn with_context(self, command: &str, notes: &Notes) -> String {
        let shell = &notes.shell;
        let words = match self {
            Self::Codex if notes.codex_servers.is_some() => Some(format!(
                "{command} --remote {}",
                variable_word(shell, "", crate::chat::CODEX_VARIABLE)
            )),
            Self::Codex => Some(format!(
                "{command} --no-daemon -c {}",
                variable_word(shell, "developer_instructions=", CONTEXT_VARIABLE)
            )),
            Self::Cursor if notes.installed.cursor_plugins => cursor_plugin(&notes.folder)
                .map(|plugin| format!("{command} --plugin-dir {}", as_shell_word(shell, &plugin))),
            // Copilot loads the extension of Kodosi's plugin only with its experimental features.
            Self::Copilot => copilot_plugin(&notes.folder).map(|plugin| {
                format!(
                    "{command} --plugin-dir {} --experimental",
                    as_shell_word(shell, &plugin)
                )
            }),
            Self::Opencode if !cfg!(windows) => {
                let standalone = match notes.installed.opencode_version {
                    Some(version) if version >= 2 => " --standalone",
                    Some(_) => "",
                    None => return command.to_owned(),
                };
                opencode_config(&notes.folder).map(|config| {
                    let config = as_shell_word(shell, &format!("{OPENCODE_CONFIG}={config}"));
                    format!("env {config} {command}{standalone}")
                })
            }
            Self::Claude | Self::Cursor | Self::Opencode | Self::Antigravity => None,
        };
        words.unwrap_or_else(|| command.to_owned())
    }

    /// The variable of a running agent that Kodosi reads: its account folder, or the
    /// configuration that carries Kodosi's note.
    #[cfg(feature = "host")]
    pub(crate) const fn shown_variable(self) -> &'static str {
        match self.home() {
            Some((_, variable)) => variable,
            None if matches!(self, Self::Opencode) => OPENCODE_CONFIG,
            None => CONTEXT_VARIABLE,
        }
    }

    /// Windows shows no arguments, so there no agent counts as started without the note.
    #[cfg(feature = "host")]
    pub(crate) fn started_without_note(
        self,
        arguments: &[String],
        variable: Option<&str>,
        notes: &Notes,
    ) -> bool {
        let has_note = match self {
            Self::Codex => {
                let kodosi_server = |address: &str| {
                    notes
                        .codex_servers
                        .as_deref()
                        .is_some_and(|start| address.starts_with(start))
                };
                let in_line = arguments.iter().any(|argument| {
                    argument.starts_with("developer_instructions=") && argument.contains("Kodosi")
                });
                let on_server = arguments.iter().any(|argument| {
                    argument
                        .strip_prefix("--remote=")
                        .is_some_and(kodosi_server)
                }) || arguments
                    .windows(2)
                    .any(|pair| pair[0] == "--remote" && kodosi_server(&pair[1]));
                in_line || on_server
            }
            Self::Cursor if notes.installed.cursor_plugins => cursor_plugin(&notes.folder)
                .is_none_or(|plugin| {
                    arguments
                        .windows(2)
                        .any(|pair| pair[0] == "--plugin-dir" && pair[1] == plugin)
                }),
            Self::Opencode if notes.installed.opencode_version.is_some() => {
                opencode_config(&notes.folder)
                    .is_none_or(|config| variable == Some(config.as_str()))
            }
            Self::Claude | Self::Copilot | Self::Cursor | Self::Opencode | Self::Antigravity => {
                true
            }
        };
        !has_note && !arguments.is_empty()
    }

    #[cfg(feature = "host")]
    pub(crate) const fn home(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Claude => Some((".claude", "CLAUDE_CONFIG_DIR")),
            Self::Codex => Some((".codex", "CODEX_HOME")),
            Self::Copilot => Some((".copilot", "COPILOT_HOME")),
            Self::Cursor | Self::Opencode | Self::Antigravity => None,
        }
    }

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
const OPENCODE_CONFIG: &str = "OPENCODE_CONFIG_CONTENT";
/// Claude Code shows its focus view only in its fullscreen renderer. `viewMode` comes first,
/// because a running Claude's output is read from the start of these settings.
#[cfg(feature = "host")]
const CLAUDE_QUIET: &str = r#"{"viewMode":"focus","tui":"fullscreen","showThinkingSummaries":false,"spinnerTipsEnabled":false,"showTurnDuration":false,"prefersReducedMotion":true,"feedbackSurveyRate":0,"promptSuggestionEnabled":false}"#;
#[cfg(feature = "host")]
const CLAUDE_DETAILED: &str = r#"{"viewMode":"verbose","showThinkingSummaries":true}"#;
/// Antigravity CLI has no `--mode` for its mode that asks: with `--mode=` it starts in the
/// account's own default, which is auto where Google offers auto.
#[cfg(feature = "host")]
const ANTIGRAVITY_ASK: &str = r#"{"agentMode":""}"#;
#[cfg(feature = "host")]
const ANTIGRAVITY_QUIET: &str =
    r#"{"verbosity":"low","showTips":false,"showFeedbackSurvey":false}"#;
#[cfg(feature = "host")]
const ANTIGRAVITY_DETAILED: &str = r#"{"verbosity":"high","showThinking":true}"#;
/// Kodosi starts no agent in a chat session of its own: Google's FAQ says that agy inside
/// another application is not supported on personal plans, and no check with a signed-in
/// account showed yet that Google takes it.
#[cfg(feature = "host")]
pub(crate) const CHAT_SESSIONS: bool = false;
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
#[cfg(feature = "host")]
const CURSOR_PLUGIN: &str = "cursor";
#[cfg(feature = "host")]
const COPILOT_PLUGIN: &str = "copilot";
/// Copilot names an extension of a plugin by the plugin's name and the extension's folder.
#[cfg(feature = "host")]
const COPILOT_EXTENSION: &str = "plugin:kodosi:kodosi";
#[cfg(feature = "host")]
const NOTE: &str = "kodosi.md";

/// What the start line of an agent needs to give it Kodosi's note.
#[cfg(feature = "host")]
#[derive(Debug, Clone, Default)]
pub(crate) struct Notes {
    pub(crate) shell: std::path::PathBuf,
    pub(crate) folder: std::path::PathBuf,
    pub(crate) installed: Installed,
    /// How the address of each terminal's Codex server starts, when the host gives them.
    pub(crate) codex_servers: Option<String>,
}

/// Read when the host looks for agents. A Cursor CLI from before May 2026 ends at
/// `--plugin-dir`, and `OpenCode` 1 ends at `--standalone`.
#[cfg(feature = "host")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Installed {
    pub(crate) cursor_plugins: bool,
    pub(crate) opencode_version: Option<u32>,
}

#[cfg(feature = "host")]
pub(crate) fn notes_folder(data_root: &Path) -> std::path::PathBuf {
    data_root.join("agents")
}

/// The name of Kodosi's plugin for the agents, which names what it does in a session.
#[cfg(feature = "host")]
pub(crate) const PLUGIN_NAME: &str = "kodosi";

/// The module is Claude Code's link to the chat of its terminal.
#[cfg(feature = "host")]
const CLAUDE_HOOKS: &str = r#"{
  "modules": ["./link.js"],
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

#[cfg(feature = "host")]
pub(crate) fn write_notes(folder: &Path) -> std::io::Result<()> {
    let plugin = folder.join(PLUGIN);
    let manifest = format!(
        "{{\n  \"name\": \"{PLUGIN_NAME}\",\n  \"version\": \"{}\",\n  \"description\": \"Work with the people and agents in Kodosi, and show them what you make on the canvas of a terminal.\"\n}}\n",
        crate::PRODUCT_VERSION
    );
    let cursor = folder.join(CURSOR_PLUGIN);
    let skills = |plugin: &Path| {
        let skills = plugin.join("skills");
        [
            (skills.join("kodosi").join("SKILL.md"), SKILL.to_owned()),
            (
                skills.join("kodosi-canvas").join("SKILL.md"),
                CANVAS_SKILL.to_owned(),
            ),
            (
                skills
                    .join("kodosi-canvas")
                    .join("assets")
                    .join("page.html"),
                PAGE.to_owned(),
            ),
            (
                skills.join("kodosi-documents").join("SKILL.md"),
                DOCUMENTS_SKILL.to_owned(),
            ),
        ]
    };
    let files = [
        (
            folder
                .join(".github")
                .join("instructions")
                .join("kodosi.instructions.md"),
            format!("---\napplyTo: \"**\"\n---\n\n{}\n", CONTEXT.trim()),
        ),
        (folder.join(NOTE), format!("{}\n", CONTEXT.trim())),
        (
            plugin.join(".claude-plugin").join("plugin.json"),
            manifest.clone(),
        ),
        (
            plugin.join("hooks").join("hooks.json"),
            CLAUDE_HOOKS.to_owned(),
        ),
        (
            plugin.join("hooks").join("link.js"),
            crate::chat::CLAUDE_MODULE.to_owned(),
        ),
        (
            cursor.join(".cursor-plugin").join("plugin.json"),
            manifest.clone(),
        ),
        (folder.join(COPILOT_PLUGIN).join("plugin.json"), manifest),
        (
            folder
                .join(COPILOT_PLUGIN)
                .join("extensions")
                .join("kodosi")
                .join("extension.mjs"),
            crate::chat::COPILOT_EXTENSION.to_owned(),
        ),
        (
            cursor.join("rules").join("kodosi.mdc"),
            format!("---\nalwaysApply: true\n---\n\n{}\n", CONTEXT.trim()),
        ),
    ]
    .into_iter()
    .chain(skills(&plugin))
    .chain(skills(&cursor));
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
fn copilot_plugin(notes: &Path) -> Option<String> {
    let plugin = notes.join(COPILOT_PLUGIN);
    if !plugin.join("plugin.json").is_file() {
        return None;
    }
    plugin.to_str().map(str::to_owned)
}

/// Copilot lets an extension answer its questions only after a person allowed it, for a folder or
/// for its repository. Kodosi gives that answer in Copilot's own file for the folder where it
/// starts Copilot, so that the chat can answer from the start.
#[cfg(feature = "host")]
pub(crate) fn allow_copilot_link(home: &Path, folder: &Path) -> std::io::Result<()> {
    use std::io::Read as _;
    const APPROVALS: &str = "permissions-config.json";
    const APPROVALS_LOCK: &str = "permissions-config.json.transaction.lock";
    const MOST_BYTES: u64 = 1024 * 1024;
    let invalid = |message: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, message);
    let folder = kodosi_os::fs::canonicalize(folder)?;
    let location = folder
        .ancestors()
        .find(|folder| folder.join(".git").exists())
        .unwrap_or(&folder)
        .to_str()
        .ok_or_else(|| invalid("the folder's name is not UTF-8"))?
        .to_owned();
    std::fs::create_dir_all(home)?;
    let file = home.join(APPROVALS);
    let _lock = quiet::lock_beside(&file, APPROVALS_LOCK)?;
    let mut approvals = match std::fs::symlink_metadata(&file) {
        Ok(metadata) if metadata.is_file() => {
            let mut bytes = Vec::new();
            std::fs::File::open(&file)?
                .take(MOST_BYTES + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MOST_BYTES {
                return Err(invalid("Copilot's approvals are too long"));
            }
            serde_json::from_slice::<serde_json::Value>(&bytes)
                .map_err(|_| invalid("Copilot's approvals are not JSON"))?
        }
        Ok(_) => return Err(invalid("Copilot's approvals are not a file")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
        Err(error) => return Err(error),
    };
    let approval = serde_json::json!({
        "kind": "extension-permission-access",
        "extensionName": COPILOT_EXTENSION,
    });
    let listed = approvals
        .as_object_mut()
        .ok_or_else(|| invalid("Copilot's approvals are not an object"))?
        .entry("locations")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| invalid("Copilot's locations are not an object"))?
        .entry(location)
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| invalid("a location of Copilot is not an object"))?
        .entry("tool_approvals")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .ok_or_else(|| invalid("Copilot's tool approvals are not a list"))?;
    if listed.contains(&approval) {
        return Ok(());
    }
    listed.push(approval);
    let mut text = serde_json::to_vec_pretty(&approvals)?;
    text.push(b'\n');
    kodosi_os::fs::replace_keeping_permissions(&file, &text)
}

#[cfg(feature = "host")]
fn cursor_plugin(notes: &Path) -> Option<String> {
    let plugin = notes.join(CURSOR_PLUGIN);
    if !plugin.join(".cursor-plugin").join("plugin.json").is_file() {
        return None;
    }
    plugin.to_str().map(str::to_owned)
}

/// Antigravity CLI takes only its last `--settings`, so the settings of its permissions and of its
/// output go to it as one object.
#[cfg(feature = "host")]
fn one_settings(words: Vec<String>) -> Vec<String> {
    if words.iter().filter(|word| *word == "--settings").count() < 2 {
        return words;
    }
    let mut merged = serde_json::Map::new();
    let mut kept = Vec::with_capacity(words.len());
    let mut words = words.into_iter();
    while let Some(word) = words.next() {
        if word == "--settings"
            && let Some(value) = words.next()
        {
            match serde_json::from_str(&value) {
                Ok(serde_json::Value::Object(settings)) => merged.extend(settings),
                _ => kept.extend([word, value]),
            }
        } else {
            kept.push(word);
        }
    }
    if !merged.is_empty() {
        kept.push("--settings".to_owned());
        kept.push(serde_json::Value::Object(merged).to_string());
    }
    kept
}

#[cfg(feature = "host")]
fn opencode_config(notes: &Path) -> Option<String> {
    let note = notes.join(NOTE);
    if !note.is_file() {
        return None;
    }
    Some(serde_json::json!({ "instructions": [note.to_str()?] }).to_string())
}

#[cfg(feature = "host")]
async fn installed(facts: &shell::Facts) -> Installed {
    let run = |program: &str, arguments: &'static [&'static str]| {
        let program = facts.executable(program);
        async move { models::run(facts, program.as_deref(), arguments).await }
    };
    let (cursor, opencode) = tokio::join!(
        run(Agent::Cursor.program(), &["--help"]),
        run(Agent::Opencode.program(), &["--version"])
    );
    Installed {
        cursor_plugins: cursor.is_some_and(|help| help.contains("--plugin-dir")),
        opencode_version: opencode.as_deref().and_then(major_version),
    }
}

#[cfg(feature = "host")]
fn major_version(version: &str) -> Option<u32> {
    version
        .split_whitespace()
        .find_map(|word| word.trim_start_matches('v').split('.').next()?.parse().ok())
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
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is a separate fact about one agent"
)]
pub struct AgentInfo {
    pub agent: Agent,
    pub program: String,
    pub installed: bool,
    pub accounts: Vec<AgentAccount>,
    pub install: String,
    pub adds_accounts: bool,
    pub reports_usage: bool,
    pub permissions: Vec<Permissions>,
    pub outputs: Vec<Output>,
    pub default_permissions: Permissions,
    pub default_output: Output,
    pub fast: bool,
    /// The agent also runs in the chat of its terminal, in a session of its own.
    pub chat: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum Permissions {
    Ask,
    Auto,
    Unrestricted,
}

impl Permissions {
    pub const ALL: [Self; 3] = [Self::Ask, Self::Auto, Self::Unrestricted];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Auto => "auto",
            Self::Unrestricted => "unrestricted",
        }
    }
}

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
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentAccount {
    pub name: String,
    pub command: String,
    pub identity: Option<String>,
    pub folder: Option<String>,
}

#[cfg(feature = "host")]
impl AgentAccount {
    pub(crate) fn folder_in(&self, home: &Path) -> Option<std::path::PathBuf> {
        let folder = self.folder.as_deref()?;
        Some(folder.strip_prefix('~').map_or_else(
            || folder.into(),
            |rest| home.join(rest.trim_start_matches(['/', '\\'])),
        ))
    }
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
    #[serde(default)]
    pub fast: bool,
    #[serde(default)]
    pub chat: bool,
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
    #[serde(default)]
    pub fast: Option<bool>,
    /// Starts the agent in the chat of its terminal, in a session of its own.
    #[serde(default)]
    pub chat: Option<bool>,
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
    /// The id of the one model whose use the limit counts, or the provider's name of a model
    /// that the agent does not list; none when the limit counts every model.
    pub model: Option<String>,
    pub used: u8,
    pub level: UsageLevel,
    pub resets_at_ms: Option<u64>,
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
pub(crate) async fn catalog(
    home: &Path,
    shell: &Path,
) -> Result<(AgentCatalog, Installed), String> {
    let facts = shell::facts(shell, true).await;
    let home = home.to_owned();
    let found = facts.clone();
    let (catalog, installed) = tokio::join!(
        tokio::task::spawn_blocking(move || accounts::catalog(&home, &found)),
        installed(&facts)
    );
    catalog
        .map(|catalog| (catalog, installed))
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
    let variables = match (agent.home(), account.folder_in(home)) {
        (Some((_, variable)), Some(folder)) if account.command.starts_with("env ") => {
            vec![(variable.to_owned(), folder.to_string_lossy().into_owned())]
        }
        (Some((_, variable)), _) => facts
            .variable(variable)
            .map_or_default(|value| vec![(variable.to_owned(), value.to_owned())]),
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
    if needs_no_quotes(value) {
        Some(value.to_owned())
    } else {
        Some(format!("'{value}'"))
    }
}

#[cfg(feature = "host")]
fn needs_no_quotes(word: &str) -> bool {
    word.bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"._-/:=@+".contains(&byte))
}

#[cfg(feature = "host")]
pub(crate) fn as_shell_word(shell: &Path, word: &str) -> String {
    if !word.is_empty() && needs_no_quotes(word) {
        word.to_owned()
    } else {
        quote(shell, word)
    }
}

#[cfg(feature = "host")]
enum Setting {
    Permissions,
    Output,
    Fast,
}

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
