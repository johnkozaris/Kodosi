use std::{
    collections::{HashMap, VecDeque},
    ffi::OsString,
    io::Read,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{
    auto_mode::{self, AutoModeRules},
    bound_memory::MemorySelectionScope,
};

const DEFAULT_SELECTION_CAPACITY: usize = 32;
const DEFAULT_SELECTION_TTL: Duration = Duration::from_mins(5);
const DEFAULT_RECEIPT_CAPACITY: usize = 256;
const DEFAULT_RECEIPT_BYTES: usize = 16 * 1024 * 1024;
const MAX_SETTINGS_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BoundAutoModeRead {
    pub target_token: String,
    pub revision: String,
    pub rules: AutoModeRules,
}

#[derive(Debug, Default)]
struct AutoModeReceiptLedger {
    receipts: HashMap<uuid::Uuid, AutoModeMutationReceipt>,
    order: VecDeque<uuid::Uuid>,
    retained_bytes: usize,
    receipt_bytes: HashMap<uuid::Uuid, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AutoModeMutationReceipt {
    pub mutation_id: String,
    pub outcome: AutoModeMutationOutcome,
    pub revision: String,
    pub rules: AutoModeRules,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AutoModeMutationOutcome {
    Applied,
    Indeterminate,
}

impl AutoModeMutationReceipt {
    pub fn validate(&self) -> Result<(), String> {
        match self.outcome {
            AutoModeMutationOutcome::Applied => {
                if self.detail.is_some() {
                    return Err("applied auto-mode outcome must not include detail".to_owned());
                }
            }
            AutoModeMutationOutcome::Indeterminate => {
                if self.detail.as_deref().is_none_or(str::is_empty) {
                    return Err("indeterminate auto-mode outcome requires detail".to_owned());
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct BoundAutoModeRegistry {
    selections: HashMap<uuid::Uuid, BoundTarget>,
    selection_order: VecDeque<uuid::Uuid>,
    receipts: Arc<Mutex<AutoModeReceiptLedger>>,
    selection_capacity: usize,
    receipt_capacity: usize,
    ttl: Duration,
}

impl Default for BoundAutoModeRegistry {
    fn default() -> Self {
        Self {
            selections: HashMap::new(),
            selection_order: VecDeque::new(),
            receipts: Arc::new(Mutex::new(AutoModeReceiptLedger::default())),
            selection_capacity: DEFAULT_SELECTION_CAPACITY,
            receipt_capacity: DEFAULT_RECEIPT_CAPACITY,
            ttl: DEFAULT_SELECTION_TTL,
        }
    }
}

impl BoundAutoModeRegistry {
    pub async fn read(
        &mut self,
        home: &Path,
        scope: MemorySelectionScope,
    ) -> Result<BoundAutoModeRead, String> {
        let home = home.to_owned();
        let prepared = tokio::task::spawn_blocking(move || prepare_target(&home))
            .await
            .map_err(|error| format!("bound auto-mode read task join: {error}"))??;
        let now = Instant::now();
        self.purge_expired(now);
        while self.selections.len() >= self.selection_capacity {
            let Some(oldest) = self.selection_order.pop_front() else {
                break;
            };
            self.selections.remove(&oldest);
        }
        let token = uuid::Uuid::now_v7();
        let revision = prepared.revision.clone();
        let rules = prepared.rules.clone();
        self.selection_order.push_back(token);
        self.selections.insert(
            token,
            BoundTarget {
                scope,
                expires_at: now + self.ttl,
                prepared: Arc::new(prepared),
            },
        );
        Ok(BoundAutoModeRead {
            target_token: token.to_string(),
            revision,
            rules,
        })
    }

    pub async fn write(
        &mut self,
        home: &Path,
        target_token: &str,
        expected_revision: &str,
        mutation_id: &str,
        rules: AutoModeRules,
        scope: &MemorySelectionScope,
    ) -> Result<AutoModeMutationReceipt, String> {
        let mutation_id = parse_uuid_v7(mutation_id, "auto-mode mutation id")?;
        if let Some(receipt) = self.existing_receipt(mutation_id) {
            return Ok(receipt);
        }
        rules.validate()?;
        let token = parse_uuid_v7(target_token, "auto-mode target token")?;
        let now = Instant::now();
        let target = self
            .selections
            .remove(&token)
            .ok_or_else(|| "auto-mode target is stale or already consumed".to_owned())?;
        self.selection_order.retain(|candidate| *candidate != token);
        if now >= target.expires_at {
            return Err("auto-mode target expired".to_owned());
        }
        if target.scope != *scope {
            return Err("auto-mode target belongs to another account".to_owned());
        }
        if target.prepared.revision != expected_revision {
            return Err("auto-mode rules changed; reload before saving".to_owned());
        }
        if target.prepared.root_path != home {
            return Err("auto-mode target belongs to another project root".to_owned());
        }
        let prepared = Arc::clone(&target.prepared);
        let receipts = Arc::clone(&self.receipts);
        let receipt_capacity = self.receipt_capacity;
        let written = tokio::task::spawn_blocking(move || {
            commit_rules_bound(&prepared, mutation_id, rules, &receipts, receipt_capacity)
        })
        .await
        .map_err(|error| format!("bound auto-mode write task join: {error}"))??;
        Ok(written)
    }

    pub fn reconcile(&self, mutation_id: &str) -> Result<AutoModeMutationReceipt, String> {
        let mutation_id = parse_uuid_v7(mutation_id, "auto-mode mutation id")?;
        self.existing_receipt(mutation_id)
            .ok_or_else(|| "auto-mode mutation outcome is not retained".to_owned())
    }

    pub fn clear(&mut self) {
        self.selections.clear();
        self.selection_order.clear();
        let mut receipts = self
            .receipts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        receipts.receipts.clear();
        receipts.order.clear();
        receipts.receipt_bytes.clear();
        receipts.retained_bytes = 0;
    }

    pub fn purge_expired_now(&mut self) {
        self.purge_expired(Instant::now());
    }

    fn existing_receipt(&self, id: uuid::Uuid) -> Option<AutoModeMutationReceipt> {
        self.receipts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .receipts
            .get(&id)
            .cloned()
    }

    fn purge_expired(&mut self, now: Instant) {
        self.selections
            .retain(|_, selection| selection.expires_at > now);
        self.selection_order
            .retain(|token| self.selections.contains_key(token));
    }

    #[cfg(test)]
    fn with_limits(selection_capacity: usize, receipt_capacity: usize, ttl: Duration) -> Self {
        Self {
            selections: HashMap::new(),
            selection_order: VecDeque::new(),
            receipts: Arc::new(Mutex::new(AutoModeReceiptLedger::default())),
            selection_capacity,
            receipt_capacity,
            ttl,
        }
    }
}

#[derive(Debug)]
struct BoundTarget {
    scope: MemorySelectionScope,
    expires_at: Instant,
    prepared: Arc<PreparedTarget>,
}

#[derive(Debug)]
struct PreparedTarget {
    root_path: PathBuf,
    root_parent: std::fs::File,
    root_name: OsString,
    root_directory_identity: DirectoryIdentity,
    root_directory: std::fs::File,
    claude_directory_identity: DirectoryIdentity,
    claude_directory: std::fs::File,
    settings_identity: Option<FileIdentity>,
    settings_bytes: Option<Vec<u8>>,
    settings_mode: Option<u32>,
    revision: String,
    rules: AutoModeRules,
}

impl PreparedTarget {
    fn verify_parent_current(&self) -> Result<(), String> {
        if DirectoryIdentity::from_file(&self.root_directory)? != self.root_directory_identity {
            return Err("held project root identity changed".to_owned());
        }
        let visible_root = open_directory_at(&self.root_parent, &self.root_name)
            .map_err(|error| format!("open visible project root: {error}"))?;
        if DirectoryIdentity::from_file(&visible_root)? != self.root_directory_identity {
            return Err("project root was replaced".to_owned());
        }
        if DirectoryIdentity::from_file(&self.claude_directory)? != self.claude_directory_identity {
            return Err("held Claude settings directory identity changed".to_owned());
        }
        let current = open_directory_at(&visible_root, ".claude")
            .map_err(|error| format!("open visible Claude settings directory: {error}"))?;
        if DirectoryIdentity::from_file(&current)? != self.claude_directory_identity {
            return Err("Claude settings directory was replaced".to_owned());
        }
        Ok(())
    }

    fn verify_current(&self) -> Result<(), String> {
        self.verify_parent_current()?;
        let current = read_settings_snapshot(&self.claude_directory)?;
        if current.identity != self.settings_identity
            || current.bytes != self.settings_bytes
            || revision(current.bytes.as_deref()) != self.revision
        {
            return Err("auto-mode rules changed; reload before saving".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone)]
struct SettingsSnapshot {
    identity: Option<FileIdentity>,
    bytes: Option<Vec<u8>>,
    mode: Option<u32>,
}

fn commit_rules_bound(
    prepared: &PreparedTarget,
    mutation_id: uuid::Uuid,
    rules: AutoModeRules,
    receipts: &Arc<Mutex<AutoModeReceiptLedger>>,
    receipt_capacity: usize,
) -> Result<AutoModeMutationReceipt, String> {
    commit_rules_bound_with_hook(
        prepared,
        mutation_id,
        rules,
        receipts,
        receipt_capacity,
        |_| {},
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutoModeCommitHook {
    CandidateReady,
    BeforeExchange,
    AfterExchange,
    BeforePriorVerification,
    BeforeFinalVisibility,
}

fn commit_rules_bound_with_hook(
    prepared: &PreparedTarget,
    mutation_id: uuid::Uuid,
    rules: AutoModeRules,
    receipts: &Arc<Mutex<AutoModeReceiptLedger>>,
    receipt_capacity: usize,
    mut hook: impl FnMut(AutoModeCommitHook),
) -> Result<AutoModeMutationReceipt, String> {
    use rustix::fs::{Mode, OFlags};
    use std::io::Write as _;

    let candidate = auto_mode::render_auto_mode_rules(prepared.settings_bytes.as_deref(), &rules)?;
    if u64::try_from(candidate.len()).unwrap_or(u64::MAX) > MAX_SETTINGS_BYTES {
        return Err(format!(
            "rendered Claude settings exceed {MAX_SETTINGS_BYTES} byte limit"
        ));
    }
    let candidate_revision = revision(Some(&candidate));
    let temporary_name = format!(".kodosi-settings-{mutation_id}");
    let temporary_fd = rustix::fs::openat(
        &prepared.claude_directory,
        &temporary_name,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o600),
    )
    .map_err(|error| format!("create Claude settings temporary file: {error}"))?;
    let mut temporary = std::fs::File::from(temporary_fd);
    if let Some(mode) = prepared.settings_mode
        && let Err(error) = rustix::fs::fchmod(&temporary, Mode::from_raw_mode(mode & 0o7777))
    {
        return cleanup_auto_mode_failure(
            prepared,
            &temporary_name,
            mutation_id,
            candidate_revision,
            rules,
            receipts,
            receipt_capacity,
            format!("set Claude settings temporary permissions: {error}"),
        );
    }
    if let Err(error) = temporary
        .write_all(&candidate)
        .and_then(|()| temporary.sync_all())
    {
        return cleanup_auto_mode_failure(
            prepared,
            &temporary_name,
            mutation_id,
            candidate_revision,
            rules,
            receipts,
            receipt_capacity,
            format!("write Claude settings temporary file: {error}"),
        );
    }
    let candidate_identity = FileObjectIdentity::from_file(&temporary)?;
    hook(AutoModeCommitHook::CandidateReady);
    if let Err(error) = prepared.verify_current() {
        return cleanup_auto_mode_failure(
            prepared,
            &temporary_name,
            mutation_id,
            candidate_revision,
            rules,
            receipts,
            receipt_capacity,
            error,
        );
    }
    verify_named_object(
        &prepared.claude_directory,
        &temporary_name,
        candidate_identity,
    )?;
    hook(AutoModeCommitHook::BeforeExchange);
    let pending = AutoModeMutationReceipt {
        mutation_id: mutation_id.to_string(),
        outcome: AutoModeMutationOutcome::Indeterminate,
        revision: candidate_revision.clone(),
        rules: rules.clone(),
        detail: Some("settings commit outcome is being verified".to_owned()),
    };
    insert_pending_auto_mode_receipt(receipts, mutation_id, pending);
    let commit = commit_candidate(prepared, &temporary_name);
    if let Err(error) = commit {
        remove_auto_mode_receipt(receipts, mutation_id);
        return cleanup_auto_mode_failure(
            prepared,
            &temporary_name,
            mutation_id,
            candidate_revision,
            rules,
            receipts,
            receipt_capacity,
            format!("commit Claude settings: {error}"),
        );
    }
    hook(AutoModeCommitHook::AfterExchange);
    let post_exchange = finalize_committed_candidate(
        prepared,
        &temporary_name,
        candidate_identity,
        &candidate,
        receipts,
        mutation_id,
        &mut hook,
    );
    if let Err(error) = post_exchange {
        if auto_mode_receipt_exists(receipts, mutation_id) {
            return mark_auto_mode_indeterminate(receipts, receipt_capacity, mutation_id, error);
        }
        return Err(error);
    }
    mark_auto_mode_applied(receipts, receipt_capacity, mutation_id)
}

fn commit_candidate(prepared: &PreparedTarget, temporary_name: &str) -> rustix::io::Result<()> {
    use rustix::fs::RenameFlags;

    if prepared.settings_identity.is_none() {
        rustix::fs::renameat_with(
            &prepared.claude_directory,
            temporary_name,
            &prepared.claude_directory,
            "settings.json",
            RenameFlags::NOREPLACE,
        )
    } else {
        atomic_exchange(
            &prepared.claude_directory,
            temporary_name,
            &prepared.claude_directory,
            "settings.json",
        )
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn atomic_exchange(
    old_directory: &std::fs::File,
    old_name: &str,
    new_directory: &std::fs::File,
    new_name: &str,
) -> rustix::io::Result<()> {
    rustix::fs::renameat_with(
        old_directory,
        old_name,
        new_directory,
        new_name,
        rustix::fs::RenameFlags::EXCHANGE,
    )
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn atomic_exchange(
    _old_directory: &std::fs::File,
    _old_name: &str,
    _new_directory: &std::fs::File,
    _new_name: &str,
) -> rustix::io::Result<()> {
    Err(rustix::io::Errno::NOTSUP)
}

fn finalize_committed_candidate(
    prepared: &PreparedTarget,
    temporary_name: &str,
    candidate_identity: FileObjectIdentity,
    candidate: &[u8],
    receipts: &Arc<Mutex<AutoModeReceiptLedger>>,
    mutation_id: uuid::Uuid,
    hook: &mut impl FnMut(AutoModeCommitHook),
) -> Result<(), String> {
    prepared.verify_parent_current().map_err(|error| {
        format!("settings exchanged but visible parent validation failed: {error}")
    })?;
    verify_visible_candidate(prepared, candidate_identity, candidate)?;
    if prepared.settings_identity.is_some() {
        hook(AutoModeCommitHook::BeforePriorVerification);
        let prior = read_named_settings_snapshot(&prepared.claude_directory, temporary_name)?;
        let exact_prior = prior.identity.map(FileObjectIdentity::from_identity)
            == prepared
                .settings_identity
                .map(FileObjectIdentity::from_identity)
            && prior.bytes == prepared.settings_bytes
            && revision(prior.bytes.as_deref()) == prepared.revision;
        if !exact_prior {
            return restore_after_cas_mismatch(
                prepared,
                temporary_name,
                candidate_identity,
                candidate,
                &prior,
                receipts,
                mutation_id,
            );
        }
        rustix::fs::unlinkat(
            &prepared.claude_directory,
            temporary_name,
            rustix::fs::AtFlags::empty(),
        )
        .map_err(|error| format!("settings exchanged but prior cleanup failed: {error}"))?;
    }
    prepared
        .claude_directory
        .sync_all()
        .map_err(|error| format!("settings committed but directory sync failed: {error}"))?;
    hook(AutoModeCommitHook::BeforeFinalVisibility);
    prepared.verify_parent_current().map_err(|error| {
        format!("settings committed but visible parent validation failed: {error}")
    })?;
    verify_visible_candidate(prepared, candidate_identity, candidate)?;
    Ok(())
}

fn restore_after_cas_mismatch(
    prepared: &PreparedTarget,
    temporary_name: &str,
    candidate_identity: FileObjectIdentity,
    candidate: &[u8],
    prior: &SettingsSnapshot,
    receipts: &Arc<Mutex<AutoModeReceiptLedger>>,
    mutation_id: uuid::Uuid,
) -> Result<(), String> {
    verify_visible_candidate(prepared, candidate_identity, candidate).map_err(|error| {
        format!("settings prior mismatched and candidate ownership was unprovable: {error}")
    })?;
    atomic_exchange(
        &prepared.claude_directory,
        "settings.json",
        &prepared.claude_directory,
        temporary_name,
    )
    .map_err(|error| format!("settings prior mismatched and restoration failed: {error}"))?;
    let restored = verify_restored_prior(prepared, prior);
    let cleanup = rustix::fs::unlinkat(
        &prepared.claude_directory,
        temporary_name,
        rustix::fs::AtFlags::empty(),
    );
    if let Err(error) = restored {
        return Err(format!(
            "settings prior mismatched and restored visibility was unprovable: {error}"
        ));
    }
    if let Err(error) = cleanup {
        return Err(format!(
            "settings prior mismatch was restored but candidate cleanup failed: {error}"
        ));
    }
    prepared.claude_directory.sync_all().map_err(|error| {
        format!("settings prior was restored but directory sync failed: {error}")
    })?;
    verify_restored_prior(prepared, prior).map_err(|error| {
        format!("settings prior was restored but durable visibility was unprovable: {error}")
    })?;
    remove_auto_mode_receipt(receipts, mutation_id);
    Err("auto-mode rules changed; reload before saving".to_owned())
}

fn verify_restored_prior(
    prepared: &PreparedTarget,
    prior: &SettingsSnapshot,
) -> Result<(), String> {
    prepared.verify_parent_current()?;
    let current = read_settings_snapshot(&prepared.claude_directory)?;
    if current.identity.map(FileObjectIdentity::from_identity)
        != prior.identity.map(FileObjectIdentity::from_identity)
        || current.bytes != prior.bytes
    {
        return Err("restored settings path does not name the swapped-out prior object".to_owned());
    }
    Ok(())
}

fn verify_visible_candidate(
    prepared: &PreparedTarget,
    identity: FileObjectIdentity,
    candidate: &[u8],
) -> Result<(), String> {
    prepared.verify_parent_current()?;
    let current = read_settings_snapshot(&prepared.claude_directory)?;
    let current_identity = current
        .identity
        .ok_or_else(|| "visible settings candidate disappeared".to_owned())?;
    if FileObjectIdentity::from_identity(current_identity) != identity
        || current.bytes.as_deref() != Some(candidate)
    {
        return Err("visible settings path is not owned by the committed candidate".to_owned());
    }
    Ok(())
}

fn verify_named_object(
    directory: &std::fs::File,
    name: &str,
    identity: FileObjectIdentity,
) -> Result<(), String> {
    let snapshot = read_named_settings_snapshot(directory, name)?;
    let current = snapshot
        .identity
        .ok_or_else(|| "settings candidate disappeared before commit".to_owned())?;
    if FileObjectIdentity::from_identity(current) != identity {
        return Err("settings candidate pathname was replaced before commit".to_owned());
    }
    Ok(())
}

fn cleanup_auto_mode_failure(
    prepared: &PreparedTarget,
    temporary_name: &str,
    mutation_id: uuid::Uuid,
    candidate_revision: String,
    rules: AutoModeRules,
    receipts: &Arc<Mutex<AutoModeReceiptLedger>>,
    receipt_capacity: usize,
    error: String,
) -> Result<AutoModeMutationReceipt, String> {
    match rustix::fs::unlinkat(
        &prepared.claude_directory,
        temporary_name,
        rustix::fs::AtFlags::empty(),
    ) {
        Ok(()) | Err(rustix::io::Errno::NOENT) => Err(error),
        Err(cleanup_error) => {
            let receipt = AutoModeMutationReceipt {
                mutation_id: mutation_id.to_string(),
                outcome: AutoModeMutationOutcome::Indeterminate,
                revision: candidate_revision,
                rules,
                detail: Some(format!(
                    "{error}; temporary cleanup failed: {cleanup_error}"
                )),
            };
            insert_auto_mode_receipt(receipts, receipt_capacity, mutation_id, receipt.clone());
            Ok(receipt)
        }
    }
}

fn insert_auto_mode_receipt(
    receipts: &Arc<Mutex<AutoModeReceiptLedger>>,
    capacity: usize,
    id: uuid::Uuid,
    receipt: AutoModeMutationReceipt,
) {
    let mut ledger = receipts
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if ledger.receipts.contains_key(&id) {
        return;
    }
    let receipt_bytes = retained_receipt_bytes(&receipt);
    while ledger.receipts.len() >= capacity
        || ledger.retained_bytes.saturating_add(receipt_bytes) > DEFAULT_RECEIPT_BYTES
    {
        let Some(oldest) = ledger.order.pop_front() else {
            ledger.receipts.clear();
            ledger.receipt_bytes.clear();
            ledger.retained_bytes = 0;
            break;
        };
        remove_receipt_locked(&mut ledger, oldest);
    }
    ledger.order.push_back(id);
    ledger.retained_bytes = ledger.retained_bytes.saturating_add(receipt_bytes);
    ledger.receipt_bytes.insert(id, receipt_bytes);
    ledger.receipts.insert(id, receipt);
    drop(ledger);
}

fn insert_pending_auto_mode_receipt(
    receipts: &Arc<Mutex<AutoModeReceiptLedger>>,
    id: uuid::Uuid,
    receipt: AutoModeMutationReceipt,
) {
    let mut ledger = receipts
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if ledger.receipts.contains_key(&id) {
        return;
    }
    let receipt_bytes = retained_receipt_bytes(&receipt);
    ledger.order.push_back(id);
    ledger.retained_bytes = ledger.retained_bytes.saturating_add(receipt_bytes);
    ledger.receipt_bytes.insert(id, receipt_bytes);
    ledger.receipts.insert(id, receipt);
}

fn remove_auto_mode_receipt(receipts: &Arc<Mutex<AutoModeReceiptLedger>>, id: uuid::Uuid) {
    let mut ledger = receipts
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    remove_receipt_locked(&mut ledger, id);
    ledger.order.retain(|candidate| *candidate != id);
}

fn auto_mode_receipt_exists(receipts: &Arc<Mutex<AutoModeReceiptLedger>>, id: uuid::Uuid) -> bool {
    receipts
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .receipts
        .contains_key(&id)
}

fn mark_auto_mode_indeterminate(
    receipts: &Arc<Mutex<AutoModeReceiptLedger>>,
    capacity: usize,
    id: uuid::Uuid,
    detail: String,
) -> Result<AutoModeMutationReceipt, String> {
    if detail.is_empty() {
        return Err("indeterminate auto-mode outcome requires detail".to_owned());
    }
    let mut ledger = receipts
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let receipt = ledger
        .receipts
        .get_mut(&id)
        .ok_or_else(|| "auto-mode mutation outcome is not retained".to_owned())?;
    receipt.outcome = AutoModeMutationOutcome::Indeterminate;
    receipt.detail = Some(detail);
    let receipt = receipt.clone();
    refresh_receipt_size(&mut ledger, id, &receipt);
    enforce_receipt_limits(&mut ledger, capacity, id);
    drop(ledger);
    Ok(receipt)
}

fn mark_auto_mode_applied(
    receipts: &Arc<Mutex<AutoModeReceiptLedger>>,
    capacity: usize,
    id: uuid::Uuid,
) -> Result<AutoModeMutationReceipt, String> {
    let mut ledger = receipts
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let receipt = ledger
        .receipts
        .get_mut(&id)
        .ok_or_else(|| "auto-mode mutation outcome is not retained".to_owned())?;
    receipt.outcome = AutoModeMutationOutcome::Applied;
    receipt.detail = None;
    let receipt = receipt.clone();
    refresh_receipt_size(&mut ledger, id, &receipt);
    enforce_receipt_limits(&mut ledger, capacity, id);
    drop(ledger);
    Ok(receipt)
}

fn retained_receipt_bytes(receipt: &AutoModeMutationReceipt) -> usize {
    receipt
        .mutation_id
        .len()
        .saturating_add(receipt.revision.len())
        .saturating_add(receipt.detail.as_ref().map_or(0, String::len))
        .saturating_add(
            [
                &receipt.rules.environment,
                &receipt.rules.allow,
                &receipt.rules.soft_deny,
                &receipt.rules.hard_deny,
            ]
            .into_iter()
            .flatten()
            .map(String::len)
            .sum::<usize>(),
        )
}

fn remove_receipt_locked(ledger: &mut AutoModeReceiptLedger, id: uuid::Uuid) {
    ledger.receipts.remove(&id);
    if let Some(bytes) = ledger.receipt_bytes.remove(&id) {
        ledger.retained_bytes = ledger.retained_bytes.saturating_sub(bytes);
    }
}

fn refresh_receipt_size(
    ledger: &mut AutoModeReceiptLedger,
    id: uuid::Uuid,
    receipt: &AutoModeMutationReceipt,
) {
    if let Some(old_bytes) = ledger
        .receipt_bytes
        .insert(id, retained_receipt_bytes(receipt))
    {
        ledger.retained_bytes = ledger.retained_bytes.saturating_sub(old_bytes);
    }
    ledger.retained_bytes = ledger
        .retained_bytes
        .saturating_add(retained_receipt_bytes(receipt));
}

fn enforce_receipt_limits(
    ledger: &mut AutoModeReceiptLedger,
    capacity: usize,
    protected: uuid::Uuid,
) {
    while ledger.receipts.len() > capacity || ledger.retained_bytes > DEFAULT_RECEIPT_BYTES {
        let Some(oldest) = ledger.order.front().copied() else {
            break;
        };
        if oldest == protected {
            break;
        }
        ledger.order.pop_front();
        remove_receipt_locked(ledger, oldest);
    }
}

fn prepare_target(home: &Path) -> Result<PreparedTarget, String> {
    if !home.is_absolute() {
        return Err("project root must be absolute".to_owned());
    }
    let root_name = home
        .file_name()
        .ok_or_else(|| "project root must have a parent entry".to_owned())?
        .to_owned();
    let root_parent_path = home
        .parent()
        .ok_or_else(|| "project root must have a parent directory".to_owned())?;
    let root_parent = open_absolute_directory(root_parent_path)?;
    let root_directory = open_directory_at(&root_parent, &root_name)
        .map_err(|error| format!("project root unavailable: {error}"))?;
    let root_directory_identity = DirectoryIdentity::from_file(&root_directory)?;
    let claude_directory = open_directory_at(&root_directory, ".claude")
        .map_err(|error| format!("Claude settings directory unavailable: {error}"))?;
    let claude_directory_identity = DirectoryIdentity::from_file(&claude_directory)?;
    let snapshot = read_settings_snapshot(&claude_directory)?;
    let rules = match snapshot.bytes.as_deref() {
        None => AutoModeRules::default(),
        Some(bytes) => {
            let content = std::str::from_utf8(bytes)
                .map_err(|error| format!("settings.json is not UTF-8: {error}"))?;
            auto_mode::parse_rules_content(content)?
        }
    };
    Ok(PreparedTarget {
        root_path: home.to_owned(),
        root_parent,
        root_name,
        root_directory_identity,
        root_directory,
        claude_directory_identity,
        claude_directory,
        settings_identity: snapshot.identity,
        settings_bytes: snapshot.bytes.clone(),
        settings_mode: snapshot.mode,
        revision: revision(snapshot.bytes.as_deref()),
        rules,
    })
}

fn read_settings_snapshot(directory: &std::fs::File) -> Result<SettingsSnapshot, String> {
    read_named_settings_snapshot(directory, "settings.json")
}

fn read_named_settings_snapshot(
    directory: &std::fs::File,
    name: &str,
) -> Result<SettingsSnapshot, String> {
    use rustix::fs::{Mode, OFlags};
    let fd = match rustix::fs::openat(
        directory,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(rustix::io::Errno::NOENT) => {
            return Ok(SettingsSnapshot {
                identity: None,
                bytes: None,
                mode: None,
            });
        }
        Err(error) => return Err(format!("open Claude settings: {error}")),
    };
    let mut file = std::fs::File::from(fd);
    let identity = FileIdentity::from_file(&file)?;
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::MetadataExt as _;
        Some(
            file.metadata()
                .map_err(|error| format!("inspect Claude settings permissions: {error}"))?
                .mode(),
        )
    };
    #[cfg(not(unix))]
    let mode = None;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_SETTINGS_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read Claude settings: {error}"))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_SETTINGS_BYTES {
        return Err(format!(
            "Claude settings exceed {MAX_SETTINGS_BYTES} byte limit"
        ));
    }
    if FileIdentity::from_file(&file)? != identity {
        return Err("Claude settings changed while being read".to_owned());
    }
    Ok(SettingsSnapshot {
        identity: Some(identity),
        bytes: Some(bytes),
        mode,
    })
}

fn revision(bytes: Option<&[u8]>) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    match bytes {
        None => {
            hash ^= 0xff;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        Some(bytes) => {
            for byte in bytes {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }
    format!("{hash:016x}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DirectoryIdentity {
    device: u64,
    inode: u64,
}

impl DirectoryIdentity {
    fn from_file(file: &std::fs::File) -> Result<Self, String> {
        use std::os::unix::fs::MetadataExt;
        let metadata = file
            .metadata()
            .map_err(|error| format!("inspect Claude settings directory: {error}"))?;
        if !metadata.is_dir() {
            return Err("Claude settings root must be a directory".to_owned());
        }
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileObjectIdentity {
    device: u64,
    inode: u64,
}

impl FileObjectIdentity {
    fn from_file(file: &std::fs::File) -> Result<Self, String> {
        Ok(Self::from_identity(FileIdentity::from_file(file)?))
    }

    fn from_identity(identity: FileIdentity) -> Self {
        Self {
            device: identity.device,
            inode: identity.inode,
        }
    }
}

impl FileIdentity {
    fn from_file(file: &std::fs::File) -> Result<Self, String> {
        use std::os::unix::fs::MetadataExt;
        let metadata = file
            .metadata()
            .map_err(|error| format!("inspect Claude settings: {error}"))?;
        if !metadata.is_file() {
            return Err("Claude settings must be a regular file".to_owned());
        }
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.size(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
            changed_seconds: metadata.ctime(),
            changed_nanoseconds: metadata.ctime_nsec(),
        })
    }
}

fn open_absolute_directory(path: &Path) -> Result<std::fs::File, String> {
    use rustix::fs::{Mode, OFlags};
    if !path.is_absolute() {
        return Err("Claude settings directory must be absolute".to_owned());
    }

    let mut current =
        std::fs::File::open("/").map_err(|error| format!("open filesystem root: {error}"))?;
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(name) => {
                let fd = rustix::fs::openat(
                    &current,
                    name,
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )
                .map_err(|error| format!("open Claude settings directory: {error}"))?;
                current = std::fs::File::from(fd);
            }
            _ => return Err("Claude settings directory is not canonical".to_owned()),
        }
    }
    Ok(current)
}

fn open_directory_at(
    parent: &std::fs::File,
    name: impl AsRef<std::ffi::OsStr>,
) -> Result<std::fs::File, rustix::io::Errno> {
    use rustix::fs::{Mode, OFlags};

    let fd = rustix::fs::openat(
        parent,
        name.as_ref(),
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    Ok(std::fs::File::from(fd))
}

fn parse_uuid_v7(value: &str, label: &str) -> Result<uuid::Uuid, String> {
    let id = uuid::Uuid::parse_str(value).map_err(|_| format!("invalid {label}"))?;
    if id.get_version_num() != 7 || id.hyphenated().to_string() != value {
        return Err(format!("invalid {label}"));
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> MemorySelectionScope {
        MemorySelectionScope {
            account_user_id: Some("account".to_owned()),
            account_epoch: 4,
        }
    }

    fn fixture() -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir(home.path().join(".claude")).unwrap();
        home
    }

    fn rules() -> AutoModeRules {
        AutoModeRules {
            allow: vec!["Read".to_owned()],
            ..AutoModeRules::default()
        }
    }

    #[tokio::test]
    async fn stale_write_is_rejected_and_receipt_reconciles() {
        let home = fixture();
        let mut registry = BoundAutoModeRegistry::default();
        let read = registry.read(home.path(), scope()).await.unwrap();
        std::fs::write(home.path().join(".claude/settings.json"), "{}\n").unwrap();
        let error = registry
            .write(
                home.path(),
                &read.target_token,
                &read.revision,
                &uuid::Uuid::now_v7().to_string(),
                AutoModeRules::default(),
                &scope(),
            )
            .await
            .unwrap_err();
        assert!(error.contains("changed"));

        let read = registry.read(home.path(), scope()).await.unwrap();
        let mutation_id = uuid::Uuid::now_v7().to_string();
        let receipt = registry
            .write(
                home.path(),
                &read.target_token,
                &read.revision,
                &mutation_id,
                AutoModeRules {
                    allow: vec!["Read".to_owned()],
                    ..AutoModeRules::default()
                },
                &scope(),
            )
            .await
            .unwrap();
        assert_eq!(
            receipt.outcome,
            AutoModeMutationOutcome::Applied,
            "{receipt:?}"
        );
        assert!(receipt.detail.is_none());
        assert_eq!(registry.reconcile(&mutation_id).unwrap(), receipt);
        assert!(
            std::fs::read_dir(home.path().join(".claude"))
                .unwrap()
                .all(|entry| !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".kodosi-settings-"))
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlinked_settings_file_is_rejected() {
        use std::os::unix::fs::symlink;
        let home = fixture();
        let outside = home.path().join("outside.json");
        std::fs::write(&outside, "{}").unwrap();
        symlink(&outside, home.path().join(".claude/settings.json")).unwrap();
        let mut registry = BoundAutoModeRegistry::default();
        assert!(registry.read(home.path(), scope()).await.is_err());
    }

    #[tokio::test]
    async fn replaced_claude_directory_cannot_receive_bound_write() {
        let home = fixture();
        let mut registry = BoundAutoModeRegistry::default();
        let read = registry.read(home.path(), scope()).await.unwrap();
        let retired = home.path().join(".claude-retired");
        std::fs::rename(home.path().join(".claude"), &retired).unwrap();
        std::fs::create_dir(home.path().join(".claude")).unwrap();
        let mutation_id = uuid::Uuid::now_v7().to_string();
        let error = registry
            .write(
                home.path(),
                &read.target_token,
                &read.revision,
                &mutation_id,
                AutoModeRules {
                    allow: vec!["Read".to_owned()],
                    ..AutoModeRules::default()
                },
                &scope(),
            )
            .await
            .unwrap_err();
        assert!(error.contains("replaced"), "{error}");
        assert!(!home.path().join(".claude/settings.json").exists());
        assert!(registry.reconcile(&mutation_id).is_err());
    }

    #[test]
    fn adversarial_file_replacement_at_every_commit_hook_fails_closed() {
        for hook in [
            AutoModeCommitHook::CandidateReady,
            AutoModeCommitHook::BeforeExchange,
            AutoModeCommitHook::AfterExchange,
            AutoModeCommitHook::BeforePriorVerification,
            AutoModeCommitHook::BeforeFinalVisibility,
        ] {
            let home = fixture();
            let settings = home.path().join(".claude/settings.json");
            std::fs::write(&settings, "{}\n").unwrap();
            let prepared = prepare_target(home.path()).unwrap();
            let receipts = Arc::new(Mutex::new(AutoModeReceiptLedger::default()));
            let mutation_id = uuid::Uuid::now_v7();
            let mut fired = false;
            let result = commit_rules_bound_with_hook(
                &prepared,
                mutation_id,
                rules(),
                &receipts,
                8,
                |current| {
                    if current != hook || fired {
                        return;
                    }
                    fired = true;
                    let replacement = home.path().join("replacement.json");
                    std::fs::write(&replacement, r#"{"permissions":{"allow":["Bash"]}}"#).unwrap();
                    std::fs::rename(replacement, &settings).unwrap();
                },
            );
            assert!(fired, "hook {hook:?} did not fire");
            if matches!(
                hook,
                AutoModeCommitHook::CandidateReady | AutoModeCommitHook::BeforeExchange
            ) {
                assert!(result.is_err(), "{hook:?}: {result:?}");
                assert!(!auto_mode_receipt_exists(&receipts, mutation_id));
            } else {
                let receipt = result.unwrap();
                assert_eq!(
                    receipt.outcome,
                    AutoModeMutationOutcome::Indeterminate,
                    "{hook:?}: {receipt:?}"
                );
                assert_eq!(
                    receipts
                        .lock()
                        .unwrap()
                        .receipts
                        .get(&mutation_id)
                        .map(|receipt| receipt.outcome),
                    Some(AutoModeMutationOutcome::Indeterminate)
                );
            }
        }
    }

    #[test]
    fn candidate_path_replacement_before_exchange_is_indeterminate() {
        let home = fixture();
        std::fs::write(home.path().join(".claude/settings.json"), "{}\n").unwrap();
        let prepared = prepare_target(home.path()).unwrap();
        let receipts = Arc::new(Mutex::new(AutoModeReceiptLedger::default()));
        let mutation_id = uuid::Uuid::now_v7();
        let temporary = home
            .path()
            .join(".claude")
            .join(format!(".kodosi-settings-{mutation_id}"));
        let stolen = home.path().join(".claude/stolen-candidate");
        let result =
            commit_rules_bound_with_hook(&prepared, mutation_id, rules(), &receipts, 8, |hook| {
                if hook == AutoModeCommitHook::BeforeExchange {
                    std::fs::rename(&temporary, &stolen).unwrap();
                    std::fs::write(&temporary, r#"{"attacker":true}"#).unwrap();
                }
            })
            .unwrap();
        assert_eq!(result.outcome, AutoModeMutationOutcome::Indeterminate);
        assert!(
            result
                .detail
                .as_deref()
                .is_some_and(|detail| { detail.contains("not owned by the committed candidate") })
        );
    }

    #[test]
    fn missing_file_cas_never_overwrites_concurrent_creator() {
        let home = fixture();
        let settings = home.path().join(".claude/settings.json");
        let prepared = prepare_target(home.path()).unwrap();
        let receipts = Arc::new(Mutex::new(AutoModeReceiptLedger::default()));
        let mutation_id = uuid::Uuid::now_v7();
        let result =
            commit_rules_bound_with_hook(&prepared, mutation_id, rules(), &receipts, 8, |hook| {
                if hook == AutoModeCommitHook::BeforeExchange {
                    std::fs::write(&settings, r#"{"attacker":true}"#).unwrap();
                }
            });
        assert!(result.is_err(), "{result:?}");
        assert_eq!(
            std::fs::read_to_string(settings).unwrap(),
            r#"{"attacker":true}"#
        );
        assert!(!auto_mode_receipt_exists(&receipts, mutation_id));
    }

    #[test]
    fn oversized_render_is_rejected_before_candidate_creation() {
        let home = fixture();
        let settings = home.path().join(".claude/settings.json");
        std::fs::write(&settings, "{}\n").unwrap();
        let prepared = prepare_target(home.path()).unwrap();
        let receipts = Arc::new(Mutex::new(AutoModeReceiptLedger::default()));
        let mutation_id = uuid::Uuid::now_v7();
        let oversized = AutoModeRules {
            allow: vec!["x".repeat(10 * 1024); 500],
            ..AutoModeRules::default()
        };
        oversized.validate().unwrap();
        let error =
            commit_rules_bound(&prepared, mutation_id, oversized, &receipts, 8).unwrap_err();
        assert!(error.contains("byte limit"), "{error}");
        assert_eq!(std::fs::read_to_string(settings).unwrap(), "{}\n");
        assert!(!auto_mode_receipt_exists(&receipts, mutation_id));
        assert!(
            std::fs::read_dir(home.path().join(".claude"))
                .unwrap()
                .all(|entry| !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".kodosi-settings-"))
        );
    }

    #[test]
    fn receipt_ledger_is_bounded_by_retained_bytes() {
        let receipts = Arc::new(Mutex::new(AutoModeReceiptLedger::default()));
        for _ in 0..24 {
            let id = uuid::Uuid::now_v7();
            insert_auto_mode_receipt(
                &receipts,
                DEFAULT_RECEIPT_CAPACITY,
                id,
                AutoModeMutationReceipt {
                    mutation_id: id.to_string(),
                    outcome: AutoModeMutationOutcome::Applied,
                    revision: "0123456789abcdef".to_owned(),
                    rules: AutoModeRules {
                        allow: vec!["x".repeat(10 * 1024); 100],
                        ..AutoModeRules::default()
                    },
                    detail: None,
                },
            );
        }
        let ledger = receipts.lock().unwrap();
        assert!(ledger.retained_bytes <= DEFAULT_RECEIPT_BYTES);
        assert!(ledger.receipts.len() < 24);
        drop(ledger);
    }

    #[test]
    fn deterministic_commit_failure_does_not_evict_prior_receipt() {
        let home = fixture();
        let prepared = prepare_target(home.path()).unwrap();
        let receipts = Arc::new(Mutex::new(AutoModeReceiptLedger::default()));
        let prior_id = uuid::Uuid::now_v7();
        insert_auto_mode_receipt(
            &receipts,
            1,
            prior_id,
            AutoModeMutationReceipt {
                mutation_id: prior_id.to_string(),
                outcome: AutoModeMutationOutcome::Applied,
                revision: "prior".to_owned(),
                rules: AutoModeRules::default(),
                detail: None,
            },
        );
        let mutation_id = uuid::Uuid::now_v7();
        let settings = home.path().join(".claude/settings.json");
        let result =
            commit_rules_bound_with_hook(&prepared, mutation_id, rules(), &receipts, 1, |hook| {
                if hook == AutoModeCommitHook::BeforeExchange {
                    std::fs::write(&settings, "{}\n").unwrap();
                }
            });
        assert!(result.is_err());
        let ledger = receipts.lock().unwrap();
        assert!(ledger.receipts.contains_key(&prior_id));
        assert!(!ledger.receipts.contains_key(&mutation_id));
        drop(ledger);
    }

    #[test]
    fn project_root_and_claude_rename_at_every_commit_hook_never_report_applied() {
        for rename_root in [false, true] {
            for hook in [
                AutoModeCommitHook::CandidateReady,
                AutoModeCommitHook::BeforeExchange,
                AutoModeCommitHook::AfterExchange,
                AutoModeCommitHook::BeforePriorVerification,
                AutoModeCommitHook::BeforeFinalVisibility,
            ] {
                let home = fixture();
                std::fs::write(home.path().join(".claude/settings.json"), "{}\n").unwrap();
                let prepared = prepare_target(home.path()).unwrap();
                let receipts = Arc::new(Mutex::new(AutoModeReceiptLedger::default()));
                let mutation_id = uuid::Uuid::now_v7();
                let retired = if rename_root {
                    home.path().with_extension("retired")
                } else {
                    home.path().join(".claude-retired")
                };
                let mut fired = false;
                let result = commit_rules_bound_with_hook(
                    &prepared,
                    mutation_id,
                    rules(),
                    &receipts,
                    8,
                    |current| {
                        if current != hook || fired {
                            return;
                        }
                        fired = true;
                        if rename_root {
                            std::fs::rename(home.path(), &retired).unwrap();
                            std::fs::create_dir(home.path()).unwrap();
                        } else {
                            std::fs::rename(home.path().join(".claude"), &retired).unwrap();
                        }
                        std::fs::create_dir(home.path().join(".claude")).unwrap();
                    },
                );
                assert!(fired, "hook {hook:?} did not fire");
                assert!(
                    !matches!(
                        result,
                        Ok(AutoModeMutationReceipt {
                            outcome: AutoModeMutationOutcome::Applied,
                            ..
                        })
                    ),
                    "rename_root={rename_root} hook={hook:?} result={result:?}"
                );
                drop(prepared);
                if rename_root {
                    std::fs::remove_dir_all(home.path()).unwrap();
                    std::fs::rename(&retired, home.path()).unwrap();
                } else {
                    std::fs::remove_dir_all(home.path().join(".claude")).unwrap();
                    std::fs::rename(&retired, home.path().join(".claude")).unwrap();
                }
            }
        }
    }

    #[tokio::test]
    async fn target_expiry_account_scope_and_replay_fail_closed() {
        let home = fixture();
        let mut expired = BoundAutoModeRegistry::with_limits(1, 2, Duration::ZERO);
        let read = expired.read(home.path(), scope()).await.unwrap();
        let error = expired
            .write(
                home.path(),
                &read.target_token,
                &read.revision,
                &uuid::Uuid::now_v7().to_string(),
                AutoModeRules::default(),
                &scope(),
            )
            .await
            .unwrap_err();
        assert!(error.contains("expired"), "{error}");

        let mut registry = BoundAutoModeRegistry::default();
        let read = registry.read(home.path(), scope()).await.unwrap();
        let other_scope = MemorySelectionScope {
            account_user_id: Some("other".to_owned()),
            account_epoch: 5,
        };
        let error = registry
            .write(
                home.path(),
                &read.target_token,
                &read.revision,
                &uuid::Uuid::now_v7().to_string(),
                AutoModeRules::default(),
                &other_scope,
            )
            .await
            .unwrap_err();
        assert!(error.contains("another account"), "{error}");
        let replay = registry
            .write(
                home.path(),
                &read.target_token,
                &read.revision,
                &uuid::Uuid::now_v7().to_string(),
                AutoModeRules::default(),
                &scope(),
            )
            .await
            .unwrap_err();
        assert!(replay.contains("consumed"), "{replay}");
    }

    #[tokio::test]
    async fn idle_expiry_purge_drops_target_without_followup_operation() {
        let home = fixture();
        let mut registry = BoundAutoModeRegistry::with_limits(1, 2, Duration::ZERO);
        registry.read(home.path(), scope()).await.unwrap();
        assert_eq!(registry.selections.len(), 1);
        registry.purge_expired_now();
        assert!(registry.selections.is_empty());
        assert!(registry.selection_order.is_empty());
    }

    #[test]
    fn malformed_outcome_detail_coupling_is_rejected() {
        let mutation_id = uuid::Uuid::now_v7().to_string();
        assert!(
            AutoModeMutationReceipt {
                mutation_id: mutation_id.clone(),
                outcome: AutoModeMutationOutcome::Applied,
                revision: "0123456789abcdef".to_owned(),
                rules: AutoModeRules::default(),
                detail: Some("unexpected".to_owned()),
            }
            .validate()
            .is_err()
        );
        assert!(
            AutoModeMutationReceipt {
                mutation_id,
                outcome: AutoModeMutationOutcome::Indeterminate,
                revision: "0123456789abcdef".to_owned(),
                rules: AutoModeRules::default(),
                detail: Some(String::new()),
            }
            .validate()
            .is_err()
        );
    }
}
