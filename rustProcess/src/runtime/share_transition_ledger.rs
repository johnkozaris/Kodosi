#[cfg(any(test, feature = "cli"))]
use std::io::Seek as _;
use std::{
    collections::BTreeMap,
    fs,
    io::Read as _,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::share_transitions::PreparedShareTransition;
#[cfg(any(test, feature = "cli"))]
use crate::support::storage::atomic_file::atomic_write_new;
use crate::{
    AppError, Result,
    support::storage::atomic_file::{
        AtomicWriteFailure, FileMode, atomic_write_json_commit_aware, retry_sync_parent,
    },
};

const FILE_VERSION: u32 = 2;
const MAX_ENTRIES: usize = 256;
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShareTransitionLedgerHeader {
    version: u32,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ShareTransitionLedgerFile {
    version: u32,
    accounts: BTreeMap<String, BTreeMap<Uuid, PreparedShareTransition>>,
}

#[derive(Debug)]
pub(crate) struct ShareTransitionLedger {
    path: PathBuf,
    file: ShareTransitionLedgerFile,
    unavailable_reason: Option<String>,
}

impl ShareTransitionLedger {
    pub(crate) fn load_default() -> Result<Self> {
        let path = crate::support::storage::paths::data_root()?.join("share-transitions.json");
        match Self::load(path.clone()) {
            Ok(ledger) => Ok(ledger),
            Err(AppError::Io(error)) => Err(AppError::Io(error)),
            Err(error) => Ok(Self::unavailable(path, error.to_string())),
        }
    }

    #[cfg(test)]
    pub(crate) fn unavailable_for_test(path: PathBuf, reason: &str) -> Self {
        Self::unavailable(path, reason.to_owned())
    }

    fn unavailable(path: PathBuf, reason: String) -> Self {
        Self {
            path,
            file: ShareTransitionLedgerFile {
                version: FILE_VERSION,
                accounts: BTreeMap::new(),
            },
            unavailable_reason: Some(reason),
        }
    }

    fn load(path: PathBuf) -> Result<Self> {
        let parent = path.parent().ok_or_else(|| AppError::Unsupported {
            reason: "share transition ledger path has no parent".to_owned(),
        })?;
        if unresolved_evidence_exists(parent)? {
            return Err(invalid(
                "shareTransitions",
                "unresolved preserved evidence remains beside the primary ledger",
            ));
        }
        let file = match read_regular_file(&path)? {
            Some(bytes) => {
                if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_FILE_BYTES {
                    return Err(invalid("shareTransitions", "ledger exceeds byte limit"));
                }
                let header: ShareTransitionLedgerHeader =
                    serde_json::from_slice(&bytes).map_err(|error| {
                        invalid("shareTransitions", &format!("malformed ledger: {error}"))
                    })?;
                if header.version != FILE_VERSION {
                    return Err(invalid(
                        "shareTransitions.version",
                        &format!("unsupported ledger version {}", header.version),
                    ));
                }
                let file: ShareTransitionLedgerFile =
                    serde_json::from_slice(&bytes).map_err(|error| {
                        invalid("shareTransitions", &format!("malformed ledger: {error}"))
                    })?;
                let count = file.accounts.values().map(BTreeMap::len).sum::<usize>();
                if count > MAX_ENTRIES {
                    return Err(invalid(
                        "shareTransitions.entries",
                        "ledger exceeds entry limit",
                    ));
                }
                for (account, entries) in &file.accounts {
                    if account.trim().is_empty() || account.len() > 1_024 {
                        return Err(invalid(
                            "shareTransitions.account",
                            "account key is invalid",
                        ));
                    }
                    for (transition_id, entry) in entries {
                        if entry.account_user_id != *account
                            || entry.transition_id != *transition_id
                        {
                            return Err(invalid(
                                "shareTransitions.identity",
                                "map keys do not match transition identity",
                            ));
                        }
                        entry.validate()?;
                    }
                }
                file
            }
            None => ShareTransitionLedgerFile {
                version: FILE_VERSION,
                accounts: BTreeMap::new(),
            },
        };
        Ok(Self {
            path,
            file,
            unavailable_reason: None,
        })
    }

    pub(crate) fn load_at(path: PathBuf) -> Result<Self> {
        Self::load(path)
    }

    #[cfg(test)]
    pub(crate) fn replace_path_for_test(&mut self, path: PathBuf) -> PathBuf {
        std::mem::replace(&mut self.path, path)
    }

    #[cfg(any(test, feature = "cli"))]
    pub(crate) fn reset_unavailable_preserving_evidence(&mut self) -> Result<Vec<PathBuf>> {
        self.ensure_unavailable()?;
        let parent = self.path.parent().ok_or_else(|| AppError::Unsupported {
            reason: "share transition ledger path has no parent".to_owned(),
        })?;
        let fresh = ShareTransitionLedgerFile {
            version: FILE_VERSION,
            accounts: BTreeMap::new(),
        };
        let fresh_bytes = serde_json::to_vec_pretty(&fresh).map_err(AppError::Json)?;
        let mut evidence = unresolved_evidence_paths(parent)?;
        let mut fresh_primary_visible = false;
        if let Some(mut source) = open_regular_file(&self.path)? {
            fresh_primary_visible = !evidence.is_empty() && file_is_exact_fresh_stage(&mut source)?;
            if !fresh_primary_visible {
                move_path_to_evidence(parent, "share-transitions-unresolved-", &self.path, &source)
                    .map_err(|error| {
                        evidence_preserved_error(&evidence, "could not preserve primary", &error)
                    })?;
                let mut moved = unresolved_evidence_paths(parent)?;
                evidence.append(&mut moved);
                evidence.sort();
                evidence.dedup();
                crate::support::storage::atomic_file::sync_directory(parent).map_err(|error| {
                    evidence_preserved_error(
                        &evidence,
                        "could not sync evidence move",
                        &AppError::Io(error),
                    )
                })?;
            }
        }
        if evidence.is_empty() {
            return Err(AppError::Unsupported {
                reason: "share transition ledger reset has no unavailable evidence to preserve"
                    .to_owned(),
            });
        }
        if fresh_primary_visible {
            retry_sync_parent(&self.path)?;
        } else if !atomic_write_new(&self.path, &fresh_bytes, FileMode::UserPrivate)? {
            return Err(AppError::Unsupported {
                reason: format!(
                    "share transition ledger path was recreated during repair; preserved evidence remains at {}",
                    display_paths(&evidence)
                ),
            });
        }
        let resolved = resolve_evidence_paths(parent, &evidence)?;
        self.file = fresh;
        self.unavailable_reason = None;
        Ok(resolved)
    }

    #[cfg(any(test, feature = "cli"))]
    fn ensure_unavailable(&self) -> Result<()> {
        if self.unavailable_reason.is_none() {
            return Err(AppError::Unsupported {
                reason: "share transition ledger is healthy and must not be reset".to_owned(),
            });
        }
        Ok(())
    }

    pub(crate) fn ensure_available(&self) -> Result<()> {
        if let Some(reason) = &self.unavailable_reason {
            return Err(AppError::Unsupported {
                reason: format!(
                    "share transition ledger is unavailable; retained evidence at {} requires repair: {reason}",
                    self.path.display()
                ),
            });
        }
        Ok(())
    }

    pub(crate) fn entries(&self, account: &str) -> Result<Vec<PreparedShareTransition>> {
        self.ensure_available()?;
        Ok(self
            .file
            .accounts
            .get(account)
            .map(|entries| entries.values().cloned().collect())
            .unwrap_or_default())
    }

    pub(crate) fn get(
        &self,
        account: &str,
        transition_id: Uuid,
    ) -> Result<Option<&PreparedShareTransition>> {
        self.ensure_available()?;
        Ok(self
            .file
            .accounts
            .get(account)
            .and_then(|entries| entries.get(&transition_id)))
    }

    pub(crate) fn put(&mut self, entry: PreparedShareTransition) -> Result<()> {
        self.ensure_available()?;
        entry.validate()?;
        let count = self
            .file
            .accounts
            .values()
            .map(BTreeMap::len)
            .sum::<usize>();
        let existing = self
            .get(&entry.account_user_id, entry.transition_id)?
            .cloned();
        if let Some(existing) = existing.as_ref() {
            validate_replacement(existing, &entry)?;
        }
        if existing.is_none() && count >= MAX_ENTRIES {
            let oldest_terminal = self
                .file
                .accounts
                .iter()
                .flat_map(|(account, entries)| {
                    entries.values().filter_map(move |entry| {
                        matches!(
                            entry.state,
                            super::share_transitions::ShareTransitionState::Terminal(_)
                        )
                        .then_some((
                            entry.transition_epoch,
                            entry.transition_id,
                            account.clone(),
                        ))
                    })
                })
                .min();
            let Some((_, terminal_id, terminal_account)) = oldest_terminal else {
                return Err(AppError::Unsupported {
                    reason: "share transition ledger is full of active evidence".to_owned(),
                });
            };
            let mut candidate = self.file.clone();
            if let Some(entries) = candidate.accounts.get_mut(&terminal_account) {
                entries.remove(&terminal_id);
            }
            if candidate
                .accounts
                .get(&terminal_account)
                .is_some_and(BTreeMap::is_empty)
            {
                candidate.accounts.remove(&terminal_account);
            }
            candidate
                .accounts
                .entry(entry.account_user_id.clone())
                .or_default()
                .insert(entry.transition_id, entry);
            return self.persist_candidate(candidate);
        }
        let mut candidate = self.file.clone();
        candidate
            .accounts
            .entry(entry.account_user_id.clone())
            .or_default()
            .insert(entry.transition_id, entry);
        self.persist_candidate(candidate)
    }

    fn persist_candidate(&mut self, candidate: ShareTransitionLedgerFile) -> Result<()> {
        match atomic_write_json_commit_aware(&self.path, &candidate, true, FileMode::UserPrivate) {
            Ok(()) => {
                self.file = candidate;
                Ok(())
            }
            Err(AtomicWriteFailure::NotReplaced(error)) => Err(error),
            Err(AtomicWriteFailure::ReplacedDurabilityUncertain(error)) => {
                if retry_sync_parent(&self.path).is_ok() {
                    self.file = candidate;
                    return Ok(());
                }
                let reason = format!(
                    "share transition ledger durability is uncertain after replacement: {error}"
                );
                self.unavailable_reason = Some(reason.clone());
                Err(AppError::Unsupported { reason })
            }
        }
    }
}

fn validate_replacement(
    existing: &PreparedShareTransition,
    replacement: &PreparedShareTransition,
) -> Result<()> {
    use super::share_transitions::ShareTransitionState;

    validate_immutable_binding(existing, replacement)?;
    if !state_transition_allowed(&existing.state, &replacement.state) {
        return Err(AppError::Unsupported {
            reason: "share transition state cannot regress or skip a durable exposure boundary"
                .to_owned(),
        });
    }
    if matches!(existing.state, ShareTransitionState::Terminal(_))
        && existing.state != replacement.state
    {
        return Err(AppError::Unsupported {
            reason: "terminal share transition outcome is immutable".to_owned(),
        });
    }
    Ok(())
}

fn validate_immutable_binding(
    existing: &PreparedShareTransition,
    replacement: &PreparedShareTransition,
) -> Result<()> {
    if existing.fingerprint != replacement.fingerprint
        || existing.transition_epoch != replacement.transition_epoch
        || existing.runtime_session_id != replacement.runtime_session_id
        || existing.expected_runtime_incarnation_id != replacement.expected_runtime_incarnation_id
        || existing.backend_session_id != replacement.backend_session_id
        || existing.cleanup != replacement.cleanup
    {
        return Err(AppError::Unsupported {
            reason: "share transition ID is already bound to another exact intent".to_owned(),
        });
    }
    for (field, old, new) in [
        (
            "backend incarnation",
            existing
                .backend_incarnation_id
                .map(|value| value.to_string()),
            replacement
                .backend_incarnation_id
                .map(|value| value.to_string()),
        ),
        (
            "source key generation",
            existing
                .source_key_generation
                .map(|value| value.to_string()),
            replacement
                .source_key_generation
                .map(|value| value.to_string()),
        ),
        (
            "claimed key generation",
            existing
                .claimed_key_generation
                .map(|value| value.to_string()),
            replacement
                .claimed_key_generation
                .map(|value| value.to_string()),
        ),
        (
            "relay generation",
            existing.relay_generation.map(|value| value.to_string()),
            replacement.relay_generation.map(|value| value.to_string()),
        ),
    ] {
        if old.is_some() && old != new {
            return Err(AppError::Unsupported {
                reason: format!("share transition {field} binding cannot change"),
            });
        }
    }
    Ok(())
}

fn state_transition_allowed(
    current: &super::share_transitions::ShareTransitionState,
    next: &super::share_transitions::ShareTransitionState,
) -> bool {
    use super::share_transitions::ShareTransitionState;

    match current {
        ShareTransitionState::Prepared => matches!(
            next,
            ShareTransitionState::Prepared
                | ShareTransitionState::ApplyingTarget
                | ShareTransitionState::CommitReady
                | ShareTransitionState::CleanupPending(_)
        ),
        ShareTransitionState::ApplyingTarget => matches!(
            next,
            ShareTransitionState::TargetObserved | ShareTransitionState::CleanupPending(_)
        ),
        ShareTransitionState::TargetObserved => matches!(
            next,
            ShareTransitionState::KeyPreparing | ShareTransitionState::CleanupPending(_)
        ),
        ShareTransitionState::KeyPreparing => matches!(
            next,
            ShareTransitionState::GenerationClaiming | ShareTransitionState::CleanupPending(_)
        ),
        ShareTransitionState::GenerationClaiming => matches!(
            next,
            ShareTransitionState::BlobsPublishing { .. } | ShareTransitionState::CleanupPending(_)
        ),
        ShareTransitionState::BlobsPublishing { .. } => matches!(
            next,
            ShareTransitionState::RelayPending { .. } | ShareTransitionState::CleanupPending(_)
        ),
        ShareTransitionState::RelayPending { .. } => matches!(
            next,
            ShareTransitionState::CommitReady | ShareTransitionState::CleanupPending(_)
        ),
        ShareTransitionState::CommitReady => matches!(
            next,
            ShareTransitionState::Terminal(_) | ShareTransitionState::CleanupPending(_)
        ),
        ShareTransitionState::CleanupPending(_) => matches!(
            next,
            ShareTransitionState::CleanupPending(_) | ShareTransitionState::Terminal(_)
        ),
        ShareTransitionState::Terminal(_) => matches!(next, ShareTransitionState::Terminal(_)),
    }
}

fn read_regular_file(path: &Path) -> Result<Option<Vec<u8>>> {
    let Some(mut file) = open_regular_file(path)? else {
        return Ok(None);
    };
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(AppError::Io)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_FILE_BYTES {
        return Err(invalid("shareTransitions", "ledger exceeds byte limit"));
    }
    Ok(Some(bytes))
}

fn open_regular_file(path: &Path) -> Result<Option<fs::File>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(AppError::Unsupported {
                reason: format!(
                    "share transition ledger `{}` is not a regular file",
                    path.display()
                ),
            });
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(AppError::Io(error)),
    }
    let file = super::share_transition_ledger_file::open_read_only(path)?;
    ensure_open_file_matches_path(path, &file)?;
    Ok(Some(file))
}

fn ensure_open_file_matches_path(path: &Path, file: &fs::File) -> Result<()> {
    let opened = file.metadata().map_err(AppError::Io)?;
    let current = fs::symlink_metadata(path).map_err(AppError::Io)?;
    if !opened.is_file() || current.file_type().is_symlink() || !current.is_file() {
        return Err(AppError::Unsupported {
            reason: format!(
                "share transition ledger `{}` changed type while opening",
                path.display()
            ),
        });
    }
    if !same_file(&opened, &current) {
        return Err(AppError::Unsupported {
            reason: format!(
                "share transition ledger `{}` changed while opening",
                path.display()
            ),
        });
    }
    Ok(())
}

#[cfg(unix)]
fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(any(test, feature = "cli"))]
fn file_is_exact_fresh_stage(file: &mut fs::File) -> Result<bool> {
    let mut bytes = Vec::new();
    std::io::Read::by_ref(file)
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(AppError::Io)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_FILE_BYTES {
        return Ok(false);
    }
    Ok(serde_json::from_slice::<ShareTransitionLedgerFile>(&bytes)
        .is_ok_and(|value| value.version == FILE_VERSION && value.accounts.is_empty()))
}

#[cfg(any(test, feature = "cli"))]
fn resolve_evidence_paths(parent: &Path, unresolved: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let previously_resolved = resolved_evidence_paths(parent)?;
    let mut participating = Vec::with_capacity(unresolved.len());
    for source in unresolved {
        let source_file = open_existing_regular_file(source)?;
        let existing = previously_resolved
            .iter()
            .find(|candidate| evidence_path_matches_file(candidate, &source_file))
            .cloned();
        let moved =
            move_path_to_evidence(parent, "share-transitions-resolved-", source, &source_file)?;
        participating.push(existing.unwrap_or(moved));
    }
    crate::support::storage::atomic_file::sync_directory(parent).map_err(AppError::Io)?;
    participating.sort();
    participating.dedup();
    Ok(participating)
}

#[cfg(any(test, feature = "cli"))]
fn evidence_path_matches_file(path: &Path, file: &fs::File) -> bool {
    let Ok(path_metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    let Ok(file_metadata) = file.metadata() else {
        return false;
    };
    !path_metadata.file_type().is_symlink()
        && path_metadata.is_file()
        && same_file(&path_metadata, &file_metadata)
}

fn open_existing_regular_file(path: &Path) -> Result<fs::File> {
    open_regular_file(path)?.ok_or_else(|| {
        AppError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("share transition evidence `{}` disappeared", path.display()),
        ))
    })
}

#[cfg(any(test, feature = "cli"))]
fn copy_open_file_to_evidence(
    parent: &Path,
    prefix: &str,
    source_file: &fs::File,
) -> Result<PathBuf> {
    let mut source = source_file.try_clone().map_err(AppError::Io)?;
    source.rewind().map_err(AppError::Io)?;
    for _ in 0..32 {
        let candidate = parent.join(format!("{prefix}{}.json", Uuid::now_v7()));
        let mut evidence_file =
            match super::share_transition_ledger_file::create_evidence(&candidate) {
                Ok(file) => file,
                Err(AppError::Io(error)) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    continue;
                }
                Err(error) => return Err(error),
            };
        super::share_transition_ledger_file::set_private_permissions(&evidence_file)?;
        std::io::copy(&mut source, &mut evidence_file).map_err(AppError::Io)?;
        evidence_file.sync_all().map_err(AppError::Io)?;
        return Ok(candidate);
    }
    Err(AppError::Unsupported {
        reason: "could not allocate a unique share transition evidence path".to_owned(),
    })
}

#[cfg(any(test, feature = "cli"))]
fn move_path_to_evidence(
    parent: &Path,
    prefix: &str,
    source_path: &Path,
    source_file: &fs::File,
) -> Result<PathBuf> {
    let source_copy = copy_open_file_to_evidence(parent, prefix, source_file)?;
    for _ in 0..32 {
        let candidate = parent.join(format!("{prefix}{}.json", Uuid::now_v7()));
        match super::share_transition_ledger_file::rename_no_replace(source_path, &candidate) {
            Ok(()) => {
                let evidence_file = super::share_transition_ledger_file::open_evidence(&candidate)?;
                ensure_open_file_matches_path(&candidate, &evidence_file)?;
                super::share_transition_ledger_file::set_private_permissions(&evidence_file)?;
                evidence_file.sync_all().map_err(AppError::Io)?;
                return if same_file(
                    &source_file.metadata().map_err(AppError::Io)?,
                    &evidence_file.metadata().map_err(AppError::Io)?,
                ) {
                    fs::remove_file(&source_copy).map_err(AppError::Io)?;
                    Ok(candidate)
                } else {
                    Ok(source_copy)
                };
            }
            Err(AppError::Io(error)) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(AppError::Unsupported {
        reason: format!(
            "could not move share transition path; copied opened evidence to {}",
            source_copy.display()
        ),
    })
}

#[cfg(any(test, feature = "cli"))]
fn resolved_evidence_paths(parent: &Path) -> Result<Vec<PathBuf>> {
    evidence_paths_with_prefix(parent, "share-transitions-resolved-")
}

fn unresolved_evidence_paths(parent: &Path) -> Result<Vec<PathBuf>> {
    evidence_paths_with_prefix(parent, "share-transitions-unresolved-")
}

fn evidence_paths_with_prefix(parent: &Path, prefix: &str) -> Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(AppError::Io(error)),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(AppError::Io)?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(prefix) && name.ends_with(".json") {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(AppError::Io)?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(AppError::Unsupported {
                    reason: format!(
                        "share transition evidence `{}` is not a regular file",
                        path.display()
                    ),
                });
            }
            let file = open_existing_regular_file(&path)?;
            drop(file);
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn unresolved_evidence_exists(parent: &Path) -> Result<bool> {
    Ok(!unresolved_evidence_paths(parent)?.is_empty())
}

#[cfg(any(test, feature = "cli"))]
fn evidence_preserved_error(paths: &[PathBuf], context: &str, error: &AppError) -> AppError {
    AppError::Unsupported {
        reason: format!(
            "{context}; preserved share transition evidence at {}: {error}",
            display_paths(paths)
        ),
    }
}

#[cfg(any(test, feature = "cli"))]
fn display_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn invalid(field: &str, reason: &str) -> AppError {
    AppError::InvalidBackendData {
        field: field.to_owned(),
        reason: reason.to_owned(),
    }
}

#[cfg(test)]
mod tests;
