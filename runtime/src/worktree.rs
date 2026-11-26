#[cfg(feature = "host")]
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use crate::{Error, Result};

#[cfg(feature = "host")]
const REMOVAL_LIMIT: Duration = Duration::from_secs(5);
/// New files that Git does not track yet count as changes only up to these sizes, so a folder of
/// build output or a large data file cannot make the count or the diff slow.
#[cfg(feature = "host")]
const MOST_NEW_FILES: usize = 200;
#[cfg(feature = "host")]
const MOST_NEW_FILE_BYTES: u64 = 1024 * 1024;
/// The diff shown on the canvas stops at this size, with a line that says it stopped.
#[cfg(feature = "host")]
const MOST_DIFF_BYTES: usize = 4 * 1024 * 1024;

/// The lines that a branch folder added and removed since its branch started, with the lines of
/// new files that Git does not track yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorktreeChanges {
    pub added: u32,
    pub removed: u32,
}

#[cfg(feature = "host")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Worktree {
    pub path: PathBuf,
    repository: PathBuf,
    branch: String,
    base: String,
}

#[cfg(feature = "host")]
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
    plain.ok_or_else(|| {
        Error::Refused(text!(
            "Use letters, numbers, dots, dashes and slashes in a branch name."
        ))
    })
}

#[cfg(feature = "host")]
pub(crate) async fn create(
    directory: &Path,
    branch: &str,
    place: Option<&Path>,
) -> Result<Worktree> {
    branch_name(branch)?;
    let folder = git(directory, &["rev-parse", "--show-toplevel"])
        .await
        .map(PathBuf::from)
        .map_err(|_| Error::Refused(text!("This folder is not in a Git repository.")))?;
    let base = git(&folder, &["rev-parse", "--verify", "HEAD"])
        .await
        .map_err(|_| Error::Refused(text!("Make a first commit before you start a branch.")))?;
    let repository = main_folder(&folder).await.unwrap_or_else(|| folder.clone());
    let name = repository
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::Refused(text!("The repository folder has no name.")))?;
    let name = format!("{name}-{}", branch.replace('/', "-"));
    let path = place
        .or_else(|| repository.parent())
        .ok_or_else(|| Error::Refused(text!("The repository has no folder beside it.")))?
        .join(&name);
    if tokio::fs::try_exists(&path).await? {
        return Err(Error::Refused(text!(
            "The folder {name} is there already. Use another branch name.",
            name = &name
        )));
    }
    let reference = format!("refs/heads/{branch}");
    if git(&folder, &["rev-parse", "--verify", "--quiet", &reference])
        .await
        .is_ok()
    {
        return Err(Error::Refused(text!(
            "The branch {branch} is there already. Use another name.",
            branch = branch
        )));
    }
    let target = path
        .to_str()
        .ok_or_else(|| Error::Invalid("The branch folder is not UTF-8.".to_owned()))?;
    git(&folder, &["worktree", "add", "-b", branch, target]).await?;
    Ok(Worktree {
        path: tokio::task::spawn_blocking(move || kodosi_os::fs::canonicalize(&path))
            .await
            .map_err(std::io::Error::from)??,
        repository,
        branch: branch.to_owned(),
        base,
    })
}

#[cfg(feature = "host")]
impl Worktree {
    /// Counts the lines that changed since the branch started. `None` when nothing changed.
    pub(crate) async fn changes(&self) -> Result<Option<WorktreeChanges>> {
        let counts = git(
            &self.path,
            &[
                "diff",
                "--numstat",
                "--no-renames",
                "--no-ext-diff",
                &self.base,
            ],
        )
        .await?;
        let (mut added, removed) = numstat(&counts);
        for file in self.new_files().await? {
            if let Some(text) = new_text(&file).await {
                added = added.saturating_add(line_count(&text));
            }
        }
        Ok((added > 0 || removed > 0).then_some(WorktreeChanges { added, removed }))
    }

    /// The changes since the branch started as one unified diff, new files included.
    pub(crate) async fn diff(&self) -> Result<String> {
        let mut diff = git(
            &self.path,
            &[
                "diff",
                "--no-color",
                "--no-ext-diff",
                "--no-renames",
                &self.base,
            ],
        )
        .await?;
        for file in self.new_files().await? {
            if diff.len() >= MOST_DIFF_BYTES {
                break;
            }
            let (Some(text), Ok(name)) = (new_text(&file).await, file.strip_prefix(&self.path))
            else {
                continue;
            };
            if !diff.is_empty() {
                diff.push('\n');
            }
            diff.push_str(&new_file_diff(&name.to_string_lossy(), &text));
        }
        if diff.len() > MOST_DIFF_BYTES {
            let end = diff.floor_char_boundary(MOST_DIFF_BYTES);
            diff.truncate(end);
            diff.push_str("\n…");
        }
        Ok(diff)
    }

    async fn new_files(&self) -> Result<Vec<PathBuf>> {
        let listed = git(
            &self.path,
            &["ls-files", "--others", "--exclude-standard", "-z"],
        )
        .await?;
        Ok(listed
            .split('\0')
            .filter(|name| !name.is_empty())
            .take(MOST_NEW_FILES)
            .map(|name| self.path.join(name))
            .collect())
    }
}

/// Sums Git's `--numstat` lines. Binary files show `-` and count as no lines.
#[cfg(feature = "host")]
fn numstat(counts: &str) -> (u32, u32) {
    counts.lines().fold((0u32, 0u32), |(added, removed), line| {
        let mut columns = line.split('\t');
        let mut count = || {
            columns
                .next()
                .and_then(|column| column.parse::<u32>().ok())
                .unwrap_or(0)
        };
        let (more, fewer) = (count(), count());
        (added.saturating_add(more), removed.saturating_add(fewer))
    })
}

/// The text of a new file, or `None` for a link, a large file or one that is not text.
#[cfg(feature = "host")]
async fn new_text(file: &Path) -> Option<String> {
    let metadata = tokio::fs::symlink_metadata(file).await.ok()?;
    if !metadata.is_file() || metadata.len() > MOST_NEW_FILE_BYTES {
        return None;
    }
    let bytes = tokio::fs::read(file).await.ok()?;
    if bytes.contains(&0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

#[cfg(feature = "host")]
fn line_count(text: &str) -> u32 {
    u32::try_from(text.lines().count()).unwrap_or(u32::MAX)
}

#[cfg(feature = "host")]
fn new_file_diff(name: &str, text: &str) -> String {
    let lines = line_count(text);
    let mut diff = format!(
        "diff --git a/{name} b/{name}\nnew file\n--- /dev/null\n+++ b/{name}\n@@ -0,0 +1,{lines} @@"
    );
    for line in text.lines() {
        diff.push_str("\n+");
        diff.push_str(line);
    }
    diff
}

#[cfg(feature = "host")]
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

#[cfg(feature = "host")]
pub(crate) async fn remove_unchanged(worktree: Worktree) {
    if tokio::time::timeout(REMOVAL_LIMIT, remove(&worktree))
        .await
        .is_err()
    {
        tracing::debug!(branch = %worktree.branch, "branch folder was kept: Git was slow");
    }
}

#[cfg(feature = "host")]
async fn remove(worktree: &Worktree) {
    let reference = format!("refs/heads/{}", worktree.branch);
    let start = format!("{0}\n{0}", worktree.base);
    let unchanged = git(&worktree.path, &["status", "--porcelain", "--ignored"])
        .await
        .is_ok_and(|changes| changes.is_empty())
        && git(&worktree.path, &["rev-parse", "HEAD", &reference])
            .await
            .is_ok_and(|tips| tips == start);
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
    if let Err(error) = git(&worktree.repository, &["branch", "-D", &worktree.branch]).await {
        tracing::debug!(%error, "unchanged branch was kept");
    }
}

#[cfg(feature = "host")]
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
        .map_err(|_| Error::Refused(text!("Git is not available on this computer.")))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        let message = String::from_utf8_lossy(&output.stderr);
        Err(Error::Invalid(
            message
                .lines()
                .find_map(|line| {
                    line.strip_prefix("fatal: ")
                        .or_else(|| line.strip_prefix("error: "))
                })
                .or_else(|| message.lines().last())
                .unwrap_or("Git refused the command.")
                .to_owned(),
        ))
    }
}

#[cfg(all(test, feature = "host"))]
#[path = "worktree_tests.rs"]
mod tests;
