use std::{
    collections::BTreeSet,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};

use kodosi_backend_client::BackendOrigin;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AppError, Result,
    support::{
        platform::fs as support_fs,
        storage::atomic_file::{
            AtomicWriteFailure, FileMode, atomic_write_commit_aware, retry_sync_parent,
        },
    },
};

const FILE_SCHEMA_VERSION: u32 = 1;
#[cfg_attr(test, allow(dead_code))]
const DEFAULT_CAPACITY: usize = 1_024;
const DEFAULT_PARTITION_CAPACITY: usize = 256;
const DEFAULT_QUARANTINE_CAPACITY: usize = 1_024;
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_IDENTIFIER_BYTES: usize = 1_024;
const STORE_FILE_NAME: &str = "collaboration-teardown-obligations.json";
const LOCK_FILE_NAME: &str = "collaboration-teardown-obligations.lock";
const STORE_STATE_HEALTHY: u8 = 0;
const STORE_STATE_CORRUPTION_QUARANTINED: u8 = 1;
const STORE_STATE_UNAVAILABLE: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DurableStoreState {
    Healthy,
    CorruptionQuarantined,
    Unavailable,
}

impl DurableStoreState {
    const fn encode(self) -> u8 {
        match self {
            Self::Healthy => STORE_STATE_HEALTHY,
            Self::CorruptionQuarantined => STORE_STATE_CORRUPTION_QUARANTINED,
            Self::Unavailable => STORE_STATE_UNAVAILABLE,
        }
    }

    fn decode(value: u8) -> Self {
        match value {
            STORE_STATE_HEALTHY => Self::Healthy,
            STORE_STATE_CORRUPTION_QUARANTINED => Self::CorruptionQuarantined,
            _ => Self::Unavailable,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum QuarantineReason {
    InvalidRemoteIdentity,
    ReplacedIncarnation,
    AccountMismatch,
    MutationConflict,
    NonRetryableClientError,
    RemoteTargetMismatch,
    StoreInvariantViolation,
    OperatorIntervention,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TeardownObligation {
    pub(crate) backend_origin: BackendOrigin,
    pub(crate) account_subject: String,
    pub(crate) backend_session_id: String,
    pub(crate) create_idempotency_id: Uuid,
    pub(crate) backend_incarnation_id: Option<Uuid>,
    pub(crate) end_mutation_id: Uuid,
    pub(crate) created_at_ms: i64,
}

impl TeardownObligation {
    fn validate(&self) -> Result<()> {
        validate_identifier("backendOrigin", self.backend_origin.as_str())?;
        validate_identifier("accountSubject", &self.account_subject)?;
        validate_identifier("backendSessionId", &self.backend_session_id)?;
        validate_uuid_v7("createIdempotencyId", self.create_idempotency_id)?;
        if let Some(incarnation_id) = self.backend_incarnation_id {
            validate_uuid_v7("backendIncarnationId", incarnation_id)?;
        }
        validate_uuid_v7("endMutationId", self.end_mutation_id)?;
        if self.created_at_ms < 0 {
            return invalid_data("createdAtMs", "must be non-negative");
        }
        Ok(())
    }

    fn logical_identity(&self) -> LogicalIdentity<'_> {
        LogicalIdentity {
            backend_origin: &self.backend_origin,
            account_subject: &self.account_subject,
            backend_session_id: &self.backend_session_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct QuarantinedTeardownObligation {
    pub(crate) record: TeardownObligation,
    pub(crate) reason: QuarantineReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProvisionOutcome {
    Inserted(TeardownObligation),
    Existing(TeardownObligation),
}

impl ProvisionOutcome {
    #[cfg(test)]
    pub(crate) fn record(&self) -> &TeardownObligation {
        match self {
            Self::Inserted(record) | Self::Existing(record) => record,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BindOutcome {
    Bound,
    AlreadyBound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AcknowledgeOutcome {
    Acknowledged,
    NotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StoreHealth {
    pub(crate) active_count: usize,
    pub(crate) quarantined_count: usize,
    pub(crate) durable_state: DurableStoreState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CleanupHealth {
    pub(crate) pending_count: usize,
    pub(crate) quarantined_count: usize,
    pub(crate) durable_state: DurableStoreState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QuarantineOutcome {
    Quarantined,
    CapacityFull,
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct LogicalIdentity<'a> {
    backend_origin: &'a BackendOrigin,
    account_subject: &'a str,
    backend_session_id: &'a str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ObligationFile {
    version: u32,
    records: Vec<TeardownObligation>,
    quarantined_records: Vec<QuarantinedTeardownObligation>,
}

impl Default for ObligationFile {
    fn default() -> Self {
        Self {
            version: FILE_SCHEMA_VERSION,
            records: Vec::new(),
            quarantined_records: Vec::new(),
        }
    }
}

impl ObligationFile {
    fn validate_and_sort(&mut self) -> Result<()> {
        if self.version != FILE_SCHEMA_VERSION {
            return invalid_data(
                "version",
                format!(
                    "unsupported collaboration teardown obligation file version {}",
                    self.version
                ),
            );
        }

        let mut identities = BTreeSet::new();
        let mut create_ids = BTreeSet::new();
        let mut end_ids = BTreeSet::new();
        for record in &self.records {
            record.validate()?;
            if !identities.insert(record.logical_identity()) {
                return invalid_data("records", "duplicate logical obligation identity");
            }
            if !create_ids.insert(record.create_idempotency_id) {
                return invalid_data("records", "duplicate create idempotency identifier");
            }
            if !end_ids.insert(record.end_mutation_id) {
                return invalid_data("records", "duplicate end mutation identifier");
            }
        }
        for quarantined in &self.quarantined_records {
            quarantined.record.validate()?;
            if !identities.insert(quarantined.record.logical_identity()) {
                return invalid_data(
                    "quarantinedRecords",
                    "duplicate logical obligation identity",
                );
            }
            if !create_ids.insert(quarantined.record.create_idempotency_id) {
                return invalid_data(
                    "quarantinedRecords",
                    "duplicate create idempotency identifier",
                );
            }
            if !end_ids.insert(quarantined.record.end_mutation_id) {
                return invalid_data("quarantinedRecords", "duplicate end mutation identifier");
            }
        }
        sort_records(&mut self.records);
        self.quarantined_records.sort_by(|left, right| {
            record_sort_key(&left.record).cmp(&record_sort_key(&right.record))
        });
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CollaborationTeardownObligationStore {
    path: PathBuf,
    capacity: usize,
    partition_capacity: usize,
    quarantine_capacity: usize,
    durable_state: Arc<AtomicU8>,
}

impl CollaborationTeardownObligationStore {
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn load_default() -> Result<Self> {
        Self::at(
            crate::support::storage::paths::collaboration_teardown_obligations_path()?,
            DEFAULT_CAPACITY,
        )
    }

    pub(crate) fn at(path: PathBuf, capacity: usize) -> Result<Self> {
        Self::with_capacities(
            path,
            capacity,
            capacity.min(DEFAULT_PARTITION_CAPACITY),
            DEFAULT_QUARANTINE_CAPACITY,
        )
    }

    #[cfg(test)]
    pub(crate) fn with_test_capacities(
        path: PathBuf,
        capacity: usize,
        partition_capacity: usize,
        quarantine_capacity: usize,
    ) -> Result<Self> {
        Self::with_capacities(path, capacity, partition_capacity, quarantine_capacity)
    }

    fn with_capacities(
        path: PathBuf,
        capacity: usize,
        partition_capacity: usize,
        quarantine_capacity: usize,
    ) -> Result<Self> {
        if capacity == 0 || partition_capacity == 0 || quarantine_capacity == 0 {
            return Err(AppError::Unsupported {
                reason: "collaboration teardown obligation capacities must be positive".to_owned(),
            });
        }
        if partition_capacity > capacity {
            return Err(AppError::Unsupported {
                reason:
                    "collaboration teardown partition capacity cannot exceed global active capacity"
                        .to_owned(),
            });
        }
        let file_name = path.file_name().and_then(std::ffi::OsStr::to_str);
        if file_name != Some(STORE_FILE_NAME) {
            return Err(AppError::Unsupported {
                reason: format!(
                    "collaboration teardown obligation path must end in {STORE_FILE_NAME}"
                ),
            });
        }
        if path.parent().is_none() {
            return Err(AppError::Unsupported {
                reason: "collaboration teardown obligation path has no parent".to_owned(),
            });
        }
        let store = Self {
            path,
            capacity,
            partition_capacity,
            quarantine_capacity,
            durable_state: Arc::new(AtomicU8::new(STORE_STATE_HEALTHY)),
        };
        store.refresh_durable_state()?;
        Ok(store)
    }

    pub(crate) fn validate_and_health(&self) -> Result<StoreHealth> {
        let result = self.with_lock(|_| {
            let mut file = self.load_under_lock()?;
            file.validate_and_sort()?;
            let durable_state = self.detect_durable_state()?;
            Ok(StoreHealth {
                active_count: file.records.len(),
                quarantined_count: file.quarantined_records.len(),
                durable_state,
            })
        });
        self.observe_result(&result);
        result
    }

    pub(crate) fn durable_state(&self) -> DurableStoreState {
        DurableStoreState::decode(self.durable_state.load(Ordering::Acquire))
    }

    fn set_durable_state(&self, state: DurableStoreState) {
        self.durable_state.store(state.encode(), Ordering::Release);
    }

    fn observe_result<T>(&self, result: &Result<T>) {
        if result.is_err() {
            self.set_durable_state(DurableStoreState::Unavailable);
        }
    }

    fn refresh_durable_state(&self) -> Result<()> {
        let result = self.with_lock(|_| {
            let state = self.detect_durable_state()?;
            self.set_durable_state(state);
            Ok(())
        });
        self.observe_result(&result);
        result
    }

    fn detect_durable_state(&self) -> Result<DurableStoreState> {
        Ok(if self.has_corruption_quarantine()? {
            DurableStoreState::CorruptionQuarantined
        } else {
            DurableStoreState::Healthy
        })
    }

    pub(crate) fn cleanup_health(
        &self,
        is_live: impl Fn(&TeardownObligation) -> bool,
    ) -> Result<CleanupHealth> {
        let result = self.with_lock(|_| {
            let mut file = self.load_under_lock()?;
            file.validate_and_sort()?;
            let pending_count = file
                .records
                .iter()
                .filter(|record| !is_live(record))
                .count();
            let durable_state = self.detect_durable_state()?;
            Ok(CleanupHealth {
                pending_count,
                quarantined_count: file.quarantined_records.len(),
                durable_state,
            })
        });
        self.observe_result(&result);
        result
    }

    fn has_corruption_quarantine(&self) -> Result<bool> {
        let parent = self.path.parent().ok_or_else(|| AppError::Unsupported {
            reason: "collaboration teardown obligation path has no parent".to_owned(),
        })?;
        Ok(!corruption_quarantine_paths(parent)?.is_empty())
    }

    pub(crate) fn provision(
        &self,
        backend_origin: &BackendOrigin,
        account_subject: &str,
        backend_session_id: &str,
        create_idempotency_id: Uuid,
        end_mutation_id: Uuid,
        created_at_ms: i64,
    ) -> Result<ProvisionOutcome> {
        let candidate = TeardownObligation {
            backend_origin: backend_origin.clone(),
            account_subject: account_subject.to_owned(),
            backend_session_id: backend_session_id.to_owned(),
            create_idempotency_id,
            backend_incarnation_id: None,
            end_mutation_id,
            created_at_ms,
        };
        candidate.validate()?;

        self.update(|file| {
            if let Some(existing) = file.records.iter().find(|existing| {
                existing.logical_identity() == candidate.logical_identity()
                    || existing.create_idempotency_id == candidate.create_idempotency_id
                    || existing.end_mutation_id == candidate.end_mutation_id
            }) {
                if existing.backend_origin == candidate.backend_origin
                    && existing.account_subject == candidate.account_subject
                    && existing.backend_session_id == candidate.backend_session_id
                    && existing.create_idempotency_id == candidate.create_idempotency_id
                    && existing.end_mutation_id == candidate.end_mutation_id
                    && existing.created_at_ms == candidate.created_at_ms
                {
                    return Ok((ProvisionOutcome::Existing(existing.clone()), false));
                }
                return invalid_data(
                    "provision",
                    "logical identity or idempotency identifier conflicts with an existing record",
                );
            }
            if file.quarantined_records.iter().any(|quarantined| {
                quarantined.record.logical_identity() == candidate.logical_identity()
                    || quarantined.record.create_idempotency_id == candidate.create_idempotency_id
                    || quarantined.record.end_mutation_id == candidate.end_mutation_id
            }) {
                return invalid_data(
                    "provision",
                    "logical identity or idempotency identifier belongs to a quarantined record",
                );
            }
            if file.records.len() >= self.capacity {
                return Err(AppError::Unsupported {
                    reason: format!(
                        "global collaboration teardown active capacity {} is exhausted",
                        self.capacity
                    ),
                });
            }
            let partition_count = file
                .records
                .iter()
                .filter(|record| {
                    record.backend_origin == candidate.backend_origin
                        && record.account_subject == candidate.account_subject
                })
                .count();
            if partition_count >= self.partition_capacity {
                return Err(AppError::Unsupported {
                    reason: format!(
                        "collaboration teardown partition capacity {} is exhausted",
                        self.partition_capacity
                    ),
                });
            }
            file.records.push(candidate.clone());
            Ok((ProvisionOutcome::Inserted(candidate), true))
        })
    }

    pub(crate) fn bind_incarnation(
        &self,
        create_idempotency_id: Uuid,
        expected_backend_session_id: &str,
        incarnation_id: Uuid,
    ) -> Result<BindOutcome> {
        validate_uuid_v7("createIdempotencyId", create_idempotency_id)?;
        validate_identifier("backendSessionId", expected_backend_session_id)?;
        validate_uuid_v7("backendIncarnationId", incarnation_id)?;

        self.update(|file| {
            let Some(record) = file
                .records
                .iter_mut()
                .find(|record| record.create_idempotency_id == create_idempotency_id)
            else {
                return invalid_data("bind", "create idempotency identifier was not found");
            };
            if record.backend_session_id != expected_backend_session_id {
                return invalid_data(
                    "bind",
                    "backend session identifier does not match the record",
                );
            }
            match record.backend_incarnation_id {
                None => {
                    record.backend_incarnation_id = Some(incarnation_id);
                    Ok((BindOutcome::Bound, true))
                }
                Some(existing) if existing == incarnation_id => {
                    Ok((BindOutcome::AlreadyBound, false))
                }
                Some(_) => invalid_data("bind", "record is bound to a different incarnation"),
            }
        })
    }

    pub(crate) fn list_for_account(
        &self,
        backend_origin: &BackendOrigin,
        account_subject: &str,
    ) -> Result<Vec<TeardownObligation>> {
        let backend_origin = backend_origin.clone();
        validate_identifier("accountSubject", account_subject)?;
        self.with_lock(|_| {
            let mut file = self.load_under_lock()?;
            file.validate_and_sort()?;
            Ok(file
                .records
                .into_iter()
                .filter(|record| {
                    record.backend_origin == backend_origin
                        && record.account_subject == account_subject
                })
                .collect())
        })
    }

    pub(crate) fn contains_create_id(&self, create_idempotency_id: Uuid) -> Result<bool> {
        validate_uuid_v7("createIdempotencyId", create_idempotency_id)?;
        self.with_lock(|_| {
            let mut file = self.load_under_lock()?;
            file.validate_and_sort()?;
            Ok(file
                .records
                .iter()
                .any(|record| record.create_idempotency_id == create_idempotency_id)
                || file
                    .quarantined_records
                    .iter()
                    .any(|record| record.record.create_idempotency_id == create_idempotency_id))
        })
    }

    pub(crate) fn acknowledge(
        &self,
        create_idempotency_id: Uuid,
        backend_incarnation_id: Option<Uuid>,
        end_mutation_id: Uuid,
    ) -> Result<AcknowledgeOutcome> {
        validate_uuid_v7("createIdempotencyId", create_idempotency_id)?;
        if let Some(incarnation_id) = backend_incarnation_id {
            validate_uuid_v7("backendIncarnationId", incarnation_id)?;
        }
        validate_uuid_v7("endMutationId", end_mutation_id)?;

        self.update(|file| {
            let Some(index) = file
                .records
                .iter()
                .position(|record| record.create_idempotency_id == create_idempotency_id)
            else {
                return Ok((AcknowledgeOutcome::NotFound, false));
            };
            let record = &file.records[index];
            if record.backend_incarnation_id != backend_incarnation_id
                || record.end_mutation_id != end_mutation_id
            {
                return invalid_data(
                    "acknowledge",
                    "incarnation or end mutation identifier does not match the record",
                );
            }
            file.records.remove(index);
            Ok((AcknowledgeOutcome::Acknowledged, true))
        })
    }

    pub(crate) fn quarantine(
        &self,
        create_idempotency_id: Uuid,
        backend_incarnation_id: Option<Uuid>,
        end_mutation_id: Uuid,
        reason: QuarantineReason,
    ) -> Result<QuarantineOutcome> {
        validate_uuid_v7("createIdempotencyId", create_idempotency_id)?;
        if let Some(incarnation_id) = backend_incarnation_id {
            validate_uuid_v7("backendIncarnationId", incarnation_id)?;
        }
        validate_uuid_v7("endMutationId", end_mutation_id)?;

        self.update(|file| {
            let Some(index) = file
                .records
                .iter()
                .position(|record| record.create_idempotency_id == create_idempotency_id)
            else {
                return Ok((QuarantineOutcome::NotFound, false));
            };
            let record = &file.records[index];
            if record.backend_incarnation_id != backend_incarnation_id
                || record.end_mutation_id != end_mutation_id
            {
                return invalid_data(
                    "quarantine",
                    "incarnation or end mutation identifier does not match the record",
                );
            }
            if file.quarantined_records.len() >= self.quarantine_capacity {
                return Ok((QuarantineOutcome::CapacityFull, false));
            }
            let record = file.records.remove(index);
            file.quarantined_records
                .push(QuarantinedTeardownObligation { record, reason });
            Ok((QuarantineOutcome::Quarantined, true))
        })
    }

    fn update<T>(
        &self,
        mutate: impl FnOnce(&mut ObligationFile) -> Result<(T, bool)>,
    ) -> Result<T> {
        let result = self.with_lock(|_| {
            let mut file = self.load_under_lock()?;
            if self.has_corruption_quarantine()? {
                self.set_durable_state(DurableStoreState::CorruptionQuarantined);
                return Err(AppError::Unsupported {
                    reason: "collaboration cleanup store is read-only until corruption quarantine is resolved"
                        .to_owned(),
                });
            }
            file.validate_and_sort()?;
            let (output, changed) = mutate(&mut file)?;
            if changed {
                self.persist_under_lock(&mut file)?;
            }
            self.set_durable_state(DurableStoreState::Healthy);
            Ok(output)
        });
        if result.is_err() && self.durable_state() != DurableStoreState::CorruptionQuarantined {
            self.set_durable_state(DurableStoreState::Unavailable);
        }
        result
    }

    fn persist_under_lock(&self, file: &mut ObligationFile) -> Result<()> {
        file.validate_and_sort()?;
        self.persist_payload_under_lock(file, |payload| {
            atomic_write_commit_aware(&self.path, payload, FileMode::UserPrivate)
        })
    }

    fn persist_payload_under_lock(
        &self,
        file: &ObligationFile,
        persist: impl FnOnce(&[u8]) -> std::result::Result<(), AtomicWriteFailure>,
    ) -> Result<()> {
        self.persist_payload_with_recovery_under_lock(file, persist, || {
            retry_sync_parent(&self.path)
        })
    }

    fn persist_payload_with_recovery_under_lock(
        &self,
        file: &ObligationFile,
        persist: impl FnOnce(&[u8]) -> std::result::Result<(), AtomicWriteFailure>,
        retry_parent_sync: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        let payload = serde_json::to_vec_pretty(file).map_err(AppError::Json)?;
        if u64::try_from(payload.len()).unwrap_or(u64::MAX) > MAX_FILE_BYTES {
            return invalid_data(
                "file",
                format!("store exceeds the {MAX_FILE_BYTES}-byte limit"),
            );
        }
        match persist(&payload) {
            Ok(()) => ensure_regular_file(&self.path, "store"),
            Err(AtomicWriteFailure::NotReplaced(error)) => {
                self.set_durable_state(DurableStoreState::Unavailable);
                Err(error)
            }
            Err(AtomicWriteFailure::ReplacedDurabilityUncertain(error)) => {
                if retry_parent_sync().is_ok() {
                    return ensure_regular_file(&self.path, "store");
                }
                self.reconcile_ambiguous_persist_under_lock(file, &error)
            }
        }
    }

    fn reconcile_ambiguous_persist_under_lock(
        &self,
        candidate: &ObligationFile,
        error: &AppError,
    ) -> Result<()> {
        self.set_durable_state(DurableStoreState::Unavailable);
        let visible = self
            .load_under_lock()
            .map_err(|reload_error| AppError::Unsupported {
                reason: format!(
                    "collaboration cleanup store durability is uncertain after atomic replacement: \
                 {error}; visible destination could not be reloaded: {reload_error}"
                ),
            })?;
        let observation = if same_obligation_file(&visible, candidate) {
            "candidate is visible"
        } else {
            "unexpected prior or third state is visible"
        };
        Err(AppError::Unsupported {
            reason: format!(
                "collaboration cleanup store durability is uncertain after atomic replacement: \
                 {error}; {observation}"
            ),
        })
    }

    fn with_lock<T>(&self, operation: impl FnOnce(&fs::File) -> Result<T>) -> Result<T> {
        let parent = self.path.parent().ok_or_else(|| AppError::Unsupported {
            reason: "collaboration teardown obligation path has no parent".to_owned(),
        })?;
        ensure_private_directory(parent)?;
        ensure_store_path_is_dedicated(parent, &self.path)?;
        let lock_path = parent.join(LOCK_FILE_NAME);
        let lock = open_lock_file(&lock_path)?;
        lock.lock().map_err(AppError::Io)?;
        operation(&lock)
    }

    fn load_under_lock(&self) -> Result<ObligationFile> {
        let Some(bytes) = read_regular_file(&self.path)? else {
            return Ok(ObligationFile::default());
        };

        let value: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(error) => {
                self.quarantine_malformed_file()?;
                tracing::warn!(
                    path = %self.path.display(),
                    %error,
                    "quarantined malformed collaboration teardown obligation store"
                );
                return Ok(ObligationFile::default());
            }
        };
        let Some(version) = value.get("version").and_then(serde_json::Value::as_u64) else {
            self.quarantine_malformed_file()?;
            tracing::warn!(
                path = %self.path.display(),
                "quarantined collaboration teardown obligation store without a valid schema version"
            );
            return Ok(ObligationFile::default());
        };
        if version > u64::from(FILE_SCHEMA_VERSION) {
            return invalid_data(
                "version",
                format!(
                    "future collaboration teardown obligation file version {version}; refusing to overwrite"
                ),
            );
        }
        if version != u64::from(FILE_SCHEMA_VERSION) {
            self.quarantine_malformed_file()?;
            tracing::warn!(
                path = %self.path.display(),
                version,
                "quarantined unsupported old collaboration teardown obligation store"
            );
            return Ok(ObligationFile::default());
        }

        match serde_json::from_value::<ObligationFile>(value) {
            Ok(mut file) => match file.validate_and_sort() {
                Ok(()) => Ok(file),
                Err(error) => {
                    self.quarantine_malformed_file()?;
                    tracing::warn!(
                        path = %self.path.display(),
                        %error,
                        "quarantined invalid collaboration teardown obligation store"
                    );
                    Ok(ObligationFile::default())
                }
            },
            Err(error) => {
                self.quarantine_malformed_file()?;
                tracing::warn!(
                    path = %self.path.display(),
                    %error,
                    "quarantined malformed collaboration teardown obligation store"
                );
                Ok(ObligationFile::default())
            }
        }
    }

    fn quarantine_malformed_file(&self) -> Result<PathBuf> {
        ensure_regular_file(&self.path, "store")?;
        let parent = self.path.parent().ok_or_else(|| AppError::Unsupported {
            reason: "collaboration teardown obligation path has no parent".to_owned(),
        })?;
        let quarantine_path = unique_corrupt_path(parent)?;
        fs::rename(&self.path, &quarantine_path).map_err(AppError::Io)?;
        sync_directory(parent)?;
        self.set_durable_state(DurableStoreState::CorruptionQuarantined);
        Ok(quarantine_path)
    }

    #[cfg(any(test, feature = "cli"))]
    pub(crate) fn reset_corruption_quarantine(&self) -> Result<usize> {
        let result = self.with_lock(|_| {
            let parent = self.path.parent().ok_or_else(|| AppError::Unsupported {
                reason: "collaboration teardown obligation path has no parent".to_owned(),
            })?;
            let sidecars = corruption_quarantine_paths(parent)?;
            if sidecars.is_empty() {
                return Err(AppError::Unsupported {
                    reason: "collaboration cleanup store has no corruption quarantine to reset"
                        .to_owned(),
                });
            }
            if self.path.exists() {
                let mut file = self.load_under_lock()?;
                file.validate_and_sort()?;
                if !file.records.is_empty() || !file.quarantined_records.is_empty() {
                    return Err(AppError::Unsupported {
                        reason: "collaboration cleanup corruption reset refuses to replace known obligations"
                            .to_owned(),
                    });
                }
            }
            self.persist_under_lock(&mut ObligationFile::default())?;
            for sidecar in &sidecars {
                let resolved = unique_resolved_path(parent)?;
                fs::rename(sidecar, resolved).map_err(AppError::Io)?;
            }
            sync_directory(parent)?;
            self.set_durable_state(DurableStoreState::Healthy);
            Ok(sidecars.len())
        });
        self.observe_result(&result);
        result
    }
}

fn same_obligation_file(left: &ObligationFile, right: &ObligationFile) -> bool {
    left.version == right.version
        && left.records == right.records
        && left.quarantined_records == right.quarantined_records
}

fn validate_identifier(field: &str, value: &str) -> Result<()> {
    if value.is_empty() || value.len() > MAX_IDENTIFIER_BYTES || value.chars().any(char::is_control)
    {
        return invalid_data(
            field,
            format!(
                "must be non-empty, contain no control characters, and be at most {MAX_IDENTIFIER_BYTES} bytes"
            ),
        );
    }
    Ok(())
}

fn validate_uuid_v7(field: &str, value: Uuid) -> Result<()> {
    if value.get_version_num() != 7 {
        return invalid_data(field, "must be a UUIDv7");
    }
    Ok(())
}

fn invalid_data<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T> {
    Err(AppError::InvalidBackendData {
        field: format!("collaborationTeardownObligations.{}", field.into()),
        reason: reason.into(),
    })
}

fn record_sort_key(record: &TeardownObligation) -> (&BackendOrigin, &str, &str, i64, Uuid, Uuid) {
    (
        &record.backend_origin,
        &record.account_subject,
        &record.backend_session_id,
        record.created_at_ms,
        record.create_idempotency_id,
        record.end_mutation_id,
    )
}

fn sort_records(records: &mut [TeardownObligation]) {
    records.sort_by(|left, right| record_sort_key(left).cmp(&record_sort_key(right)));
}

fn ensure_store_path_is_dedicated(parent: &Path, path: &Path) -> Result<()> {
    if path.parent() != Some(parent)
        || path.file_name().and_then(std::ffi::OsStr::to_str) != Some(STORE_FILE_NAME)
    {
        return Err(AppError::Unsupported {
            reason: "collaboration teardown obligation store path escaped its dedicated file"
                .to_owned(),
        });
    }
    ensure_existing_regular_file(path, "store")
}

fn ensure_private_directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(AppError::Unsupported {
                reason: format!(
                    "collaboration teardown obligation directory `{}` is not a regular directory",
                    path.display()
                ),
            });
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            support_fs::ensure_dir(path)?;
        }
        Err(error) => return Err(AppError::Io(error)),
    }
    support_fs::set_dir_permissions(path)
}

fn open_lock_file(path: &Path) -> Result<fs::File> {
    ensure_existing_regular_file(path, "lock")?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    ensure_open_file_matches_path(path, &file, "lock")?;
    support_fs::set_file_permissions(path)?;
    Ok(file)
}

fn read_regular_file(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(AppError::Unsupported {
                reason: format!(
                    "collaboration teardown obligation store `{}` is not a regular file",
                    path.display()
                ),
            });
        }
        Ok(metadata) if metadata.len() > MAX_FILE_BYTES => {
            return invalid_data(
                "file",
                format!("store exceeds the {MAX_FILE_BYTES}-byte limit"),
            );
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(AppError::Io(error)),
    }

    let file = fs::File::open(path)?;
    ensure_open_file_matches_path(path, &file, "store")?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_FILE_BYTES {
        return invalid_data(
            "file",
            format!("store exceeds the {MAX_FILE_BYTES}-byte limit"),
        );
    }
    Ok(Some(bytes))
}

fn ensure_regular_file(path: &Path, label: &str) -> Result<()> {
    ensure_existing_regular_file(path, label)
}

fn ensure_existing_regular_file(path: &Path, label: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(AppError::Unsupported {
                reason: format!(
                    "collaboration teardown obligation {label} `{}` is not a regular file",
                    path.display()
                ),
            })
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppError::Io(error)),
    }
}

fn ensure_open_file_matches_path(path: &Path, file: &fs::File, label: &str) -> Result<()> {
    let opened = file.metadata()?;
    if !opened.is_file() {
        return Err(AppError::Unsupported {
            reason: format!("collaboration teardown obligation {label} is not a regular file"),
        });
    }
    let path_metadata = fs::symlink_metadata(path)?;
    if path_metadata.file_type().is_symlink() || !path_metadata.is_file() {
        return Err(AppError::Unsupported {
            reason: format!(
                "collaboration teardown obligation {label} `{}` changed type while opening",
                path.display()
            ),
        });
    }
    if !same_file(&opened, &path_metadata) {
        return Err(AppError::Unsupported {
            reason: format!(
                "collaboration teardown obligation {label} `{}` changed while opening",
                path.display()
            ),
        });
    }
    Ok(())
}

#[cfg(unix)]
fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

fn corruption_quarantine_paths(parent: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("collaboration-teardown-obligations.corrupt-")
            && name.ends_with(".json")
        {
            ensure_existing_regular_file(&entry.path(), "corruption quarantine")?;
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

fn unique_corrupt_path(parent: &Path) -> Result<PathBuf> {
    for _ in 0..32 {
        let candidate = parent.join(format!(
            "collaboration-teardown-obligations.corrupt-{}.json",
            Uuid::now_v7()
        ));
        match fs::symlink_metadata(&candidate) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(candidate),
            Ok(_) => {}
            Err(error) => return Err(AppError::Io(error)),
        }
    }
    Err(AppError::Unsupported {
        reason: "could not allocate a unique corruption quarantine path".to_owned(),
    })
}

#[cfg(any(test, feature = "cli"))]
fn unique_resolved_path(parent: &Path) -> Result<PathBuf> {
    for _ in 0..32 {
        let candidate = parent.join(format!(
            "collaboration-teardown-obligations.resolved-{}.json",
            Uuid::now_v7()
        ));
        match fs::symlink_metadata(&candidate) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(candidate),
            Ok(_) => {}
            Err(error) => return Err(AppError::Io(error)),
        }
    }
    Err(AppError::Unsupported {
        reason: "could not allocate a unique resolved quarantine path".to_owned(),
    })
}

fn sync_directory(path: &Path) -> Result<()> {
    crate::support::storage::atomic_file::sync_directory(path).map_err(AppError::Io)
}

#[cfg(test)]
mod tests;
