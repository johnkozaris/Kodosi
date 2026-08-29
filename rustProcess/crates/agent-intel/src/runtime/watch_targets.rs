use std::path::{Path, PathBuf};

use kodosi_session::AgentKind;

use super::watcher_impl::WatchTarget;

#[must_use]
pub fn global(_agent: AgentKind) -> Vec<WatchTarget> {
    Vec::new()
}

#[must_use]
pub fn workspace(agent: AgentKind, cwd: &Path) -> Vec<WatchTarget> {
    match agent {
        AgentKind::Claude => {
            let home = crate::runtime::paths::claude_home();
            let cwd_string = cwd.to_string_lossy();
            let project_slug = crate::claude::ClaudeCodeProvider::encode_project_path(&cwd_string);
            vec![WatchTarget {
                path: home.join("projects").join(project_slug),
                recursive: true,
            }]
        }
        AgentKind::Copilot => Vec::new(),
    }
}

#[must_use]
pub fn normalize_workspace_root(cwd: &str) -> PathBuf {
    let supplied = PathBuf::from(cwd);
    let absolute = if supplied.is_absolute() {
        supplied
    } else {
        std::env::current_dir().map_or_else(|_| supplied.clone(), |base| base.join(&supplied))
    };
    std::fs::canonicalize(&absolute).unwrap_or(absolute)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_workspace_target_is_only_the_transcript_directory() {
        let cwd = Path::new("/Users/john/Repos/Kodosi");
        let targets = workspace(AgentKind::Claude, cwd);
        assert!(targets.iter().any(|target| {
            target
                .path
                .to_string_lossy()
                .contains("projects/-Users-john-Repos-Kodosi")
        }));
        assert_eq!(targets.len(), 1);
        assert!(targets.iter().all(|target| target.recursive));
    }

    #[test]
    fn copilot_needs_no_artifact_watcher() {
        let cwd = Path::new("/repo");
        assert!(global(AgentKind::Copilot).is_empty());
        assert!(workspace(AgentKind::Copilot, cwd).is_empty());
    }

    #[test]
    fn normalization_makes_relative_workspace_absolute() {
        assert!(normalize_workspace_root(".").is_absolute());
    }
}
