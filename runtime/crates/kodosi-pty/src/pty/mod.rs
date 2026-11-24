use std::path::{Path, PathBuf};

use crate::{KodosiError, Result};

#[cfg(unix)]
mod prompt_marks;
#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::{KodosiPty, PtyReader, open_files, process_group, process_session, process_start};
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{KodosiPty, PtyReader, process_start};

/// What a running process of this user started with.
pub struct ProcessStart {
    pub program: PathBuf,
    pub arguments: Vec<String>,
    pub variables: Vec<(String, String)>,
    pub directory: PathBuf,
}

/// Windows has no process groups; its foreground observation names the agent's process.
#[cfg(windows)]
pub const fn process_group(_pid: u32) -> Option<u32> {
    None
}

/// Kodosi does not read the open files of a process on Windows.
#[cfg(windows)]
pub const fn open_files(_group: u32) -> Option<Vec<PathBuf>> {
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
    // `kodosi acp -- <agent>` runs the agent's server for the terminal's chat.
    if executable == "kodosi" && arguments.get(1).is_some_and(|action| action == "acp") {
        let agent = arguments.iter().position(|argument| argument == "--")?;
        return program_identity(&arguments[agent + 1..]);
    }
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
        "opencode" => Some("opencode"),
        "agy" => Some("antigravity"),
        "cursor-agent" | "agent" if basename == "cursor-agent" || candidate.contains("cursor") => {
            Some("cursor")
        }
        _ if candidate.contains("@anthropic-ai/claude-code/") => Some("claude"),
        _ if candidate.contains("@github/copilot/") => Some("copilot"),
        _ if candidate.contains("@openai/codex/") => Some("codex"),
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
            (vec!["/home/ana/.local/bin/agy"], Some("antigravity")),
            (
                vec!["node", "/opt/lib/node_modules/opencode-ai/bin/opencode"],
                Some("opencode"),
            ),
            (
                vec!["kodosi", "acp", "--mode=", "--", "agy", "acp"],
                Some("antigravity"),
            ),
            (vec!["kodosi", "acp", "--", "/bin/zsh"], None),
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
