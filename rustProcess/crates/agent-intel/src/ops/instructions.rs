use std::path::{Path, PathBuf};

use crate::claude::extensions::{InstructionScope, InstructionSource};

use super::path_safety;

#[derive(Debug, Clone, Copy)]
pub enum InstructionAgentType {
    Claude,
    Copilot,
}

impl InstructionAgentType {
    fn workspace_filenames(self) -> &'static [&'static str] {
        match self {
            Self::Claude => &["CLAUDE.md", "AGENTS.md"],
            Self::Copilot => &[".github/copilot-instructions.md", "AGENTS.md"],
        }
    }

    fn user_paths(self, home: &Path) -> Vec<PathBuf> {
        match self {
            Self::Claude => vec![home.join(".claude").join("CLAUDE.md")],
            Self::Copilot => vec![
                home.join(".copilot").join("copilot-instructions.md"),
                home.join(".copilot").join("AGENTS.md"),
            ],
        }
    }
}

pub async fn scan_instructions(
    home: &Path,
    cwd: &str,
    agent_type: InstructionAgentType,
) -> Result<Vec<InstructionSource>, String> {
    let canonical_cwd = path_safety::canonicalize_user_dir("cwd", cwd)
        .map_err(|err| format!("invalid cwd: {err}"))?;
    let home = home.to_path_buf();

    tokio::task::spawn_blocking(move || scan_instructions_at(&home, &canonical_cwd, agent_type))
        .await
        .map_err(|e| format!("task join error: {e}"))
}

pub(crate) fn scan_instructions_at(
    home: &Path,
    cwd: &Path,
    agent_type: InstructionAgentType,
) -> Vec<InstructionSource> {
    let mut out = Vec::new();
    let mut order: u8 = 0;

    for user_path in agent_type.user_paths(home) {
        if user_path.is_file() {
            out.push(InstructionSource {
                path: user_path.to_string_lossy().into_owned(),
                scope: InstructionScope::User,
                order,
                additional_fields: std::collections::BTreeMap::new(),
            });
            order = order.saturating_add(1);
        }
    }

    let project_root = find_project_root(cwd);
    let mut current = cwd.to_path_buf();
    loop {
        let scope = if Some(&current) == project_root.as_ref() {
            InstructionScope::Project
        } else {
            InstructionScope::Local
        };

        for filename in agent_type.workspace_filenames() {
            let candidate = current.join(filename);
            if candidate.is_file() {
                out.push(InstructionSource {
                    path: candidate.to_string_lossy().into_owned(),
                    scope,
                    order,
                    additional_fields: std::collections::BTreeMap::new(),
                });
                order = order.saturating_add(1);
            }
        }

        if project_root.as_ref() == Some(&current) {
            break;
        }
        match current.parent() {
            Some(parent) if parent != current => current = parent.to_path_buf(),
            _ => break,
        }
    }

    out
}

fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut current = start;
    loop {
        if current.join(".git").exists() {
            return Some(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) if parent != current => current = parent,
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[tokio::test]
    async fn claude_scan_walks_up_to_git_root() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();

        let repo = temp.path().join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        write(&repo.join("CLAUDE.md"), "repo root instructions");
        let subdir = repo.join("src").join("nested");
        fs::create_dir_all(&subdir).unwrap();
        write(&subdir.join("CLAUDE.md"), "subdir instructions");
        write(
            &temp.path().join("CLAUDE.md"),
            "above-repo (should be ignored)",
        );

        write(&home.join(".claude").join("CLAUDE.md"), "user-level");

        let result = scan_instructions(
            &home,
            subdir.to_str().unwrap(),
            InstructionAgentType::Claude,
        )
        .await
        .unwrap();

        let paths: Vec<&str> = result.iter().map(|s| s.path.as_str()).collect();
        assert!(paths.iter().any(|p| p.ends_with(".claude/CLAUDE.md")));
        assert!(paths.iter().any(|p| p.ends_with("nested/CLAUDE.md")));
        assert!(paths.iter().any(|p| p.ends_with("repo/CLAUDE.md")));
        assert!(
            !paths
                .iter()
                .any(|p| p == &temp.path().join("CLAUDE.md").to_string_lossy()),
            "must not walk above the git root"
        );

        for src in &result {
            if src.path.contains(".claude/CLAUDE.md") {
                assert_eq!(src.scope, InstructionScope::User);
            } else if src.path.ends_with("nested/CLAUDE.md") {
                assert_eq!(src.scope, InstructionScope::Local);
            } else if src.path.ends_with("repo/CLAUDE.md") {
                assert_eq!(src.scope, InstructionScope::Project);
            }
        }
    }

    #[tokio::test]
    async fn copilot_scan_finds_dot_github_path() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        write(
            &repo.join(".github").join("copilot-instructions.md"),
            "copilot",
        );
        write(&repo.join("AGENTS.md"), "agents");

        let result =
            scan_instructions(&home, repo.to_str().unwrap(), InstructionAgentType::Copilot)
                .await
                .unwrap();

        let paths: Vec<&str> = result.iter().map(|s| s.path.as_str()).collect();
        assert!(
            paths
                .iter()
                .any(|p| p.ends_with(".github/copilot-instructions.md"))
        );
        assert!(paths.iter().any(|p| p.ends_with("AGENTS.md")));
    }

    #[tokio::test]
    async fn empty_dir_returns_empty() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let cwd = temp.path().join("empty");
        fs::create_dir_all(&cwd).unwrap();

        let result = scan_instructions(&home, cwd.to_str().unwrap(), InstructionAgentType::Claude)
            .await
            .unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn no_git_root_walks_to_filesystem_root() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let nested = temp.path().join("a").join("b");
        fs::create_dir_all(&nested).unwrap();
        write(&nested.join("CLAUDE.md"), "nested");

        let result = scan_instructions(
            &home,
            nested.to_str().unwrap(),
            InstructionAgentType::Claude,
        )
        .await
        .unwrap();
        let nested_entry = result.iter().find(|s| s.path.ends_with("b/CLAUDE.md"));
        assert!(nested_entry.is_some());
        assert_eq!(nested_entry.unwrap().scope, InstructionScope::Local);
    }
}
