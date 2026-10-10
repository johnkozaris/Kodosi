use std::path::{Path, PathBuf};

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Worktree {
    pub path: PathBuf,
    repository: PathBuf,
    branch: String,
    base: String,
}

pub(crate) fn is_repository(directory: &Path) -> bool {
    directory
        .ancestors()
        .any(|folder| folder.join(".git").exists())
}

pub(crate) fn branch_name(name: &str) -> Result<()> {
    let plain = !name.is_empty()
        && name.len() <= 100
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/'))
        && !name.starts_with(['-', '/', '.'])
        && !name.ends_with(['/', '.'])
        && !name.contains("..")
        && !name.contains("//");
    if plain {
        Ok(())
    } else {
        Err(Error::Invalid(
            "Use letters, numbers, dots, dashes and slashes in a branch name.".to_owned(),
        ))
    }
}

pub(crate) async fn create(
    directory: &Path,
    branch: &str,
    place: Option<&Path>,
) -> Result<Worktree> {
    branch_name(branch)?;
    let folder = git(directory, &["rev-parse", "--show-toplevel"])
        .await
        .map(PathBuf::from)
        .map_err(|_| Error::Invalid("This folder is not in a Git repository.".to_owned()))?;
    let base = git(&folder, &["rev-parse", "--verify", "HEAD"])
        .await
        .map_err(|_| Error::Invalid("Make a first commit before you start a branch.".to_owned()))?;
    let repository = main_folder(&folder).await.unwrap_or_else(|| folder.clone());
    let name = repository
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::Invalid("The repository folder has no name.".to_owned()))?;
    let name = format!("{name}-{}", branch.replace('/', "-"));
    let path = place
        .or_else(|| repository.parent())
        .ok_or_else(|| Error::Invalid("The repository has no folder beside it.".to_owned()))?
        .join(&name);
    if tokio::fs::try_exists(&path).await? {
        return Err(Error::Invalid(format!(
            "The folder {name} is there already. Use another branch name."
        )));
    }
    let reference = format!("refs/heads/{branch}");
    if git(&folder, &["rev-parse", "--verify", "--quiet", &reference])
        .await
        .is_ok()
    {
        return Err(Error::Invalid(format!(
            "The branch {branch} is there already. Use another name."
        )));
    }
    let target = path
        .to_str()
        .ok_or_else(|| Error::Invalid("The branch folder is not UTF-8.".to_owned()))?;
    git(&folder, &["worktree", "add", "-b", branch, target]).await?;
    Ok(Worktree {
        path: tokio::fs::canonicalize(&path).await?,
        repository,
        branch: branch.to_owned(),
        base,
    })
}

async fn main_folder(folder: &Path) -> Option<PathBuf> {
    let common = git(
        folder,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .await
    .ok()
    .map(PathBuf::from)?;
    (common.file_name()? == ".git").then(|| common.parent().map(Path::to_path_buf))?
}

pub(crate) async fn remove_unchanged(worktree: Worktree) {
    let unchanged = git(&worktree.path, &["status", "--porcelain", "--ignored"])
        .await
        .is_ok_and(|changes| changes.is_empty())
        && git(
            &worktree.repository,
            &[
                "rev-list",
                "--count",
                &format!("{}..{}", worktree.base, worktree.branch),
            ],
        )
        .await
        .is_ok_and(|commits| commits == "0");
    if !unchanged {
        return;
    }
    let Some(path) = worktree.path.to_str() else {
        return;
    };
    if let Err(error) = git(&worktree.repository, &["worktree", "remove", path]).await {
        tracing::debug!(%error, "unchanged worktree was kept");
        return;
    }
    if let Err(error) = git(&worktree.repository, &["branch", "-d", &worktree.branch]).await {
        tracing::debug!(%error, "unchanged branch was kept");
    }
}

async fn git(directory: &Path, arguments: &[&str]) -> Result<String> {
    let output = tokio::process::Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|_| Error::Invalid("Git is not available on this computer.".to_owned()))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        let message = String::from_utf8_lossy(&output.stderr);
        Err(Error::Invalid(
            message
                .lines()
                .last()
                .map_or("Git refused the command.", |line| {
                    line.trim_start_matches("fatal: ")
                })
                .to_owned(),
        ))
    }
}

#[cfg(test)]
#[path = "worktree_tests.rs"]
mod tests;
