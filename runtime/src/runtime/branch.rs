use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::time::Instant;
use uuid::Uuid;

use super::{Completion, Runtime};
use crate::worktree::{Worktree, WorktreeChanges};
use crate::{Error, Result};

/// A branch folder is counted again at most this often while nothing else asks for it.
const COUNT_EVERY: Duration = Duration::from_secs(10);
const FOLDER: &str = "changes";

/// The diff of a branch folder, named after the folder so that it stays on the canvas when the
/// terminal comes back after a restart.
pub(super) fn diff_file(data_root: &Path, worktree: &Worktree) -> Option<PathBuf> {
    let name = worktree.path.file_name()?.to_str()?;
    Some(data_root.join(FOLDER).join(format!("{name}.diff")))
}

/// Leaves an unchanged diff as it is, since every viewer of the canvas reloads a file that changed.
fn write_diff(file: &Path, diff: &str) -> Result<()> {
    if std::fs::read(file).is_ok_and(|written| written == diff.as_bytes()) {
        return Ok(());
    }
    if let Some(folder) = file.parent() {
        kodosi_os::fs::create_private_dir(folder)?;
    }
    kodosi_os::fs::write_private(file, diff.as_bytes())?;
    Ok(())
}

impl Runtime {
    /// Counts the branch folders that were not counted for a while.
    pub(super) fn count_branches(&mut self) {
        let now = Instant::now();
        let due = self
            .local
            .iter()
            .filter(|(_, local)| {
                local.worktree.is_some()
                    && !local.counting
                    && local
                        .counted
                        .is_none_or(|at| now.duration_since(at) >= COUNT_EVERY)
            })
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for id in due {
            self.count_branch(id);
        }
    }

    /// Counts a terminal's branch folder now, such as when its agent ends a turn.
    pub(super) fn count_branch(&mut self, id: Uuid) {
        let Some(local) = self.local.get_mut(&id) else {
            return;
        };
        let Some(worktree) = local.worktree.clone().filter(|_| !local.counting) else {
            return;
        };
        local.counting = true;
        let shown = diff_file(&self.config.data_root, &worktree)
            .filter(|file| local.terminal.canvas.paths().contains(file));
        self.spawn(async move {
            let changes = worktree.changes().await;
            if let (Some(file), Ok(_)) = (shown, &changes) {
                let written = match worktree.diff().await {
                    Ok(diff) => write_diff(&file, &diff),
                    Err(error) => Err(error),
                };
                if let Err(error) = written {
                    tracing::debug!(%error, "the diff on the canvas stays as it was");
                }
            }
            Completion::Counted { id, changes }
        });
    }

    pub(super) fn counted(&mut self, id: Uuid, changes: Result<Option<WorktreeChanges>>) {
        let Some(local) = self.local.get_mut(&id) else {
            return;
        };
        local.counting = false;
        local.counted = Some(Instant::now());
        let changes = match changes {
            Ok(changes) => changes,
            Err(error) => {
                tracing::debug!(%error, "a branch folder was not counted");
                return;
            }
        };
        if local.entry.changes == changes {
            return;
        }
        local.entry.changes = changes;
        if local.terminal.set_changes(changes).is_err() {
            tracing::debug!("a terminal closed before it heard its changes");
        }
        self.publish_catalog();
    }

    /// Shows a terminal's diff on its canvas, for this computer or for a view on another one.
    pub(super) fn changes_shown(
        &self,
        id: Uuid,
    ) -> Result<impl Future<Output = Result<()>> + Send + use<>> {
        let (worktree, canvas, terminal) = self
            .local
            .get(&id)
            .map(|local| {
                (
                    local.worktree.clone(),
                    local.terminal.canvas.clone(),
                    local.entry.name.clone(),
                )
            })
            .ok_or(Error::NotFound)?;
        let worktree = worktree
            .ok_or_else(|| Error::Refused(text!("This terminal has no branch of its own.")))?;
        let file = diff_file(&self.config.data_root, &worktree)
            .ok_or_else(|| Error::Invalid("The branch folder has no name.".to_owned()))?;
        Ok(async move {
            let diff = worktree.diff().await?;
            write_diff(&file, &diff)?;
            canvas
                .show(crate::canvas::Show {
                    paths: vec![file],
                    address: None,
                    browser_as_it_is: false,
                    screen: None,
                    folder: worktree.path.clone(),
                    terminal,
                    agent: None,
                })
                .await
                .map(drop)
                .map_err(Error::Text)
        })
    }

    /// A view on another computer asked for the diff, so only the log hears when it fails.
    pub(super) fn show_changes_here(&mut self, id: Uuid) {
        match self.changes_shown(id) {
            Ok(shown) => self.spawn(async move {
                Completion::ChangesShown {
                    result: shown.await,
                }
            }),
            Err(error) => tracing::debug!(%error, "the diff of a terminal was not shown"),
        }
    }

    /// Removes the diff of a terminal that closed for good.
    pub(super) fn forget_diff(&self, worktree: &Worktree) {
        if let Some(file) = diff_file(&self.config.data_root, worktree) {
            drop(std::fs::remove_file(file));
        }
    }
}
