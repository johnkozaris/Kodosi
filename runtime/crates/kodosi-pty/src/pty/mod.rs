use std::path::Path;

use crate::{KodosiError, Result};

#[cfg(unix)]
mod prompt_marks;
#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::{KodosiPty, PtyReader, process_group};
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{KodosiPty, PtyReader};

/// Windows has no process groups; its foreground observation names the agent's process.
#[cfg(windows)]
pub const fn process_group(_pid: u32) -> Option<u32> {
    None
}

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
    use super::program_identity;

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
}
