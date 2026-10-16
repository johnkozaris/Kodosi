use std::path::Path;

use crate::{KodosiError, Result};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::{KodosiPty, PtyReader};
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{KodosiPty, PtyReader};

pub const CONSOLE_HOST: bool = cfg!(windows);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownStage {
    Interrupt,
    Hangup,
    Terminate,
    Force,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitOutcome {
    Reaped,
    TimedOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transfer {
    Read(usize),
    Wrote(usize),
}

const FOREIGN_CONTEXT_PREFIXES: [&str; 7] = [
    "CLAUDE_CODE_",
    "CLAUDE_EFFORT",
    "CLAUDE_PID",
    "COPILOT_",
    "CODEX_",
    "CURSOR_",
    "VSCODE_",
];
const FOREIGN_CONTEXT_KEYS: [&str; 10] = [
    "AGENT",
    "AI_AGENT",
    "CLAUDECODE",
    "COLUMNS",
    "LINES",
    "TERMINFO",
    "TERM_PROGRAM_VERSION",
    "TERM_SESSION_ID",
    "WT_PROFILE_ID",
    "WT_SESSION",
];

const AGENT_FOLDERS: [&str; 2] = ["CODEX_HOME", "COPILOT_HOME"];

fn inherits_foreign_context(key: &str) -> bool {
    if AGENT_FOLDERS
        .iter()
        .any(|folder| folder.eq_ignore_ascii_case(key))
    {
        return false;
    }
    FOREIGN_CONTEXT_KEYS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(key))
        || FOREIGN_CONTEXT_PREFIXES.iter().any(|prefix| {
            key.get(..prefix.len())
                .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
        })
}

fn program_identity(arguments: &[String]) -> Option<&'static str> {
    let executable = Path::new(arguments.first()?).file_name()?.to_str()?;
    let candidate = if matches!(executable, "node" | "bun" | "deno") {
        arguments
            .iter()
            .skip(1)
            .find(|argument| !argument.starts_with('-'))?
            .as_str()
    } else {
        arguments.first()?.as_str()
    };
    let basename = Path::new(candidate).file_name()?.to_str()?;
    match basename {
        "claude" => Some("claude"),
        "copilot" => Some("copilot"),
        "codex" | "codex.js" => Some("codex"),
        "gemini" => Some("gemini"),
        "opencode" => Some("opencode"),
        "cursor-agent" | "agent" if basename == "cursor-agent" || candidate.contains("cursor") => {
            Some("cursor")
        }
        _ if candidate.contains("@anthropic-ai/claude-code/") => Some("claude"),
        _ if candidate.contains("@github/copilot/") => Some("copilot"),
        _ if candidate.contains("@openai/codex/") => Some("codex"),
        _ if candidate.contains("@google/gemini-cli/") => Some("gemini"),
        _ => None,
    }
}

fn validate_working_dir(working_dir: Option<&str>) -> Result<Option<&Path>> {
    let Some(directory) = working_dir.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let directory = Path::new(directory);
    if directory.is_dir() {
        Ok(Some(directory))
    } else {
        Err(KodosiError::Spawn(format!(
            "working directory does not exist or is not a directory: {}",
            directory.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::{inherits_foreign_context, program_identity};

    #[test]
    fn provider_identity_uses_executable_not_prompt_arguments() {
        for (arguments, expected) in [
            (vec!["/opt/bin/claude"], Some("claude")),
            (vec!["claude"], Some("claude")),
            (
                vec!["node", "/lib/node_modules/@github/copilot/index.js"],
                Some("copilot"),
            ),
            (
                vec!["node", "/lib/node_modules/@openai/codex/bin/codex.js"],
                Some("codex"),
            ),
            (vec!["/opt/bin/cursor-agent"], Some("cursor")),
            (
                vec![
                    "node",
                    "/opt/lib/node_modules/@google/gemini-cli/dist/index.js",
                ],
                Some("gemini"),
            ),
            (
                vec!["node", "/opt/lib/node_modules/opencode-ai/bin/opencode"],
                Some("opencode"),
            ),
            (vec!["/bin/zsh", "-c", "claude"], None),
            (vec!["/usr/bin/printf", "claude"], None),
            (vec!["node"], None),
        ] {
            assert_eq!(
                program_identity(&arguments.into_iter().map(str::to_owned).collect::<Vec<_>>()),
                expected
            );
        }
    }

    #[test]
    fn agent_and_foreign_terminal_context_never_reaches_a_session() {
        for key in [
            "AI_AGENT",
            "CLAUDECODE",
            "CLAUDE_CODE_MESSAGING_TOKEN",
            "CLAUDE_PID",
            "COPILOT_CLI",
            "CODEX_SANDBOX",
            "TERMINFO",
            "TERM_PROGRAM_VERSION",
            "VSCODE_INJECTION",
            "WT_SESSION",
            "wt_session",
        ] {
            assert!(inherits_foreign_context(key), "{key}");
        }
        for key in [
            "CLAUDE_CONFIG_DIR",
            "CODEX_HOME",
            "COPILOT_HOME",
            "HOME",
            "PATH",
            "SSH_AUTH_SOCK",
            "TERM_PROGRAM",
            "LANG",
        ] {
            assert!(!inherits_foreign_context(key), "{key}");
        }
    }
}
