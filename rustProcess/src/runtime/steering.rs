use std::{
    collections::{HashMap, VecDeque},
    fs::OpenOptions,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    AppError, Result,
    host_protocol::{SemanticSendMode, SteerDeliveryState, SteerQueueEntry, SteerTransition},
    session_runtime::commands::SessionInput,
};
use kodosi_domain::ids::SessionId;

const STEER_FILE_VERSION: u32 = 7;
const TOMBSTONE_JSON_FILE_VERSION: u32 = 6;
const PREVIOUS_STEER_FILE_VERSION: u32 = 5;
const INCARNATION_STEER_FILE_VERSION: u32 = 4;
const RECEIPT_ORIGIN_FILE_VERSION: u32 = 3;
const OLDER_STEER_FILE_VERSION: u32 = 2;
const LEGACY_STEER_FILE_VERSION: u32 = 1;
const LOCAL_SEMANTIC_SCOPE: &str = "local";
const MAX_PENDING_STEERS: usize = 256;
const MAX_PENDING_STEERS_PER_SESSION: usize = 32;
const MAX_COMPLETED_RELAY_SEMANTIC_SENDS: usize = 256;
const MAX_COMPLETED_DIRECT_SEMANTIC_SENDS: usize = 256;
const MAX_SEMANTIC_TURN_STATES: usize = MAX_PENDING_STEERS;
const TOMBSTONE_LOG_HEADER: &[u8] = b"KODOSI-RELAY-EXECUTION-TOMBSTONES-V1\n";
const MAX_TOMBSTONE_RECORD_BYTES: usize = 64 * 1024;
pub(crate) const MAX_STEER_TEXT_BYTES: usize = 16 * 1024;

pub(super) fn semantic_scope(account_user_id: Option<String>) -> String {
    account_user_id.unwrap_or_else(|| LOCAL_SEMANTIC_SCOPE.to_owned())
}

fn semantic_steer_id(account_user_id: &str, request_id: &str) -> String {
    let preimage = format!(
        "kodosi-semantic-steer-v1\n{}\n{account_user_id}\n{}\n{request_id}",
        account_user_id.len(),
        request_id.len()
    );
    format!(
        "semantic-{}",
        kodosi_backend_client::crypto::sha256_hex(preimage.as_bytes())
    )
}

#[derive(Debug)]
pub(crate) struct SteeringState {
    path: PathBuf,
    entries: VecDeque<SteerQueueEntry>,
    ready_boundaries: VecDeque<SemanticBoundary>,
    completed: VecDeque<SemanticSendReceipt>,
    relay_requesters: Vec<RelaySemanticRequester>,
    relay_execution_tombstones: RelayExecutionTombstoneStore,
    turn_states: Vec<SemanticTurnStateRecord>,
}

#[expect(
    clippy::struct_field_names,
    reason = "field names preserve the persisted semantic requester schema"
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct RelaySemanticRequester {
    account_user_id: String,
    request_id: String,
    requester_device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct RelaySemanticExecutionTombstone {
    account_user_id: String,
    request_id: String,
    session_id: String,
    session_incarnation_id: String,
    mode: SemanticSendMode,
    payload_sha256: String,
    requester_device_id: String,
    outcome: SteerDeliveryState,
}

impl RelaySemanticExecutionTombstone {
    fn as_entry(&self) -> SteerQueueEntry {
        SteerQueueEntry {
            steer_id: semantic_steer_id(&self.account_user_id, &self.request_id),
            account_user_id: self.account_user_id.clone(),
            request_id: self.request_id.clone(),
            session_incarnation_id: self.session_incarnation_id.clone(),
            mode: self.mode,
            session_id: self.session_id.clone(),
            text: String::new(),
            queued_at_ms: 0,
            delivery_state: self.outcome,
            at_tool_use_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct SemanticBoundary {
    account_user_id: String,
    session_id: String,
    mode: SemanticSendMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool_use_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SemanticTurnState {
    Idle,
    Running,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct SemanticTurnStateRecord {
    account_user_id: String,
    session_id: String,
    session_incarnation_id: String,
    state: SemanticTurnState,
}

struct LoadedSemanticMetadata {
    relay_execution_tombstones: Vec<RelaySemanticExecutionTombstone>,
    turn_states: Vec<SemanticTurnStateRecord>,
}

impl LoadedSemanticMetadata {
    fn empty() -> Self {
        Self {
            relay_execution_tombstones: Vec::new(),
            turn_states: Vec::new(),
        }
    }
}

fn validate_relay_execution_tombstone(tombstone: &RelaySemanticExecutionTombstone) -> Result<()> {
    if tombstone.account_user_id.trim().is_empty()
        || tombstone.request_id.trim().is_empty()
        || tombstone.session_id.trim().is_empty()
        || tombstone.session_incarnation_id.trim().is_empty()
        || tombstone.payload_sha256.trim().is_empty()
        || tombstone.requester_device_id.trim().is_empty()
    {
        return Err(AppError::Unsupported {
            reason: "semantic relay execution tombstone identity is invalid".to_owned(),
        });
    }
    Ok(())
}

fn validate_relay_execution_tombstones(
    tombstones: &[RelaySemanticExecutionTombstone],
) -> Result<()> {
    for tombstone in tombstones {
        validate_relay_execution_tombstone(tombstone)?;
    }
    let mut identities = std::collections::HashSet::new();
    if tombstones.iter().any(|tombstone| {
        !identities.insert((
            tombstone.account_user_id.as_str(),
            tombstone.request_id.as_str(),
        ))
    }) {
        return Err(AppError::Unsupported {
            reason: "semantic relay execution tombstone identity is duplicated".to_owned(),
        });
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RelayExecutionTombstoneKey {
    account_user_id: String,
    request_id: String,
}

impl RelayExecutionTombstoneKey {
    fn new(account_user_id: &str, request_id: &str) -> Self {
        Self {
            account_user_id: account_user_id.to_owned(),
            request_id: request_id.to_owned(),
        }
    }

    fn from_tombstone(tombstone: &RelaySemanticExecutionTombstone) -> Self {
        Self::new(&tombstone.account_user_id, &tombstone.request_id)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "operation")]
enum RelayExecutionTombstoneRecord {
    Add {
        tombstone: RelaySemanticExecutionTombstone,
    },
}

#[derive(Debug)]
struct RelayExecutionTombstoneStore {
    path: PathBuf,
    by_request: HashMap<RelayExecutionTombstoneKey, RelaySemanticExecutionTombstone>,
}

impl RelayExecutionTombstoneStore {
    fn load(steering_path: &Path) -> Result<Self> {
        let path = steering_path.with_extension("tombstones");
        let mut store = Self {
            path,
            by_request: HashMap::new(),
        };
        store.load_records()?;
        Ok(store)
    }

    fn load_records(&mut self) -> Result<()> {
        let mut file = match OpenOptions::new().read(true).write(true).open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(AppError::Io(error)),
        };
        let mut header = vec![0_u8; TOMBSTONE_LOG_HEADER.len()];
        file.read_exact(&mut header).map_err(AppError::Io)?;
        if header != TOMBSTONE_LOG_HEADER {
            return Err(AppError::Unsupported {
                reason: "semantic execution tombstone log header is invalid".to_owned(),
            });
        }
        let mut valid_len = u64::try_from(TOMBSTONE_LOG_HEADER.len()).unwrap_or(u64::MAX);
        loop {
            let record_start = valid_len;
            let mut length = [0_u8; 4];
            let read = file.read(&mut length[..1]).map_err(AppError::Io)?;
            if read == 0 {
                break;
            }
            if let Err(error) = file.read_exact(&mut length[1..]) {
                recover_torn_log_tail(&file, record_start, error)?;
                break;
            }
            let length = usize::try_from(u32::from_le_bytes(length)).unwrap_or(usize::MAX);
            if length == 0 || length > MAX_TOMBSTONE_RECORD_BYTES {
                return Err(AppError::Unsupported {
                    reason: "semantic execution tombstone log record is oversized".to_owned(),
                });
            }
            let mut payload = vec![0_u8; length];
            let mut checksum = [0_u8; 64];
            if let Err(error) = file.read_exact(&mut payload) {
                recover_torn_log_tail(&file, record_start, error)?;
                break;
            }
            if let Err(error) = file.read_exact(&mut checksum) {
                recover_torn_log_tail(&file, record_start, error)?;
                break;
            }
            if checksum != *kodosi_backend_client::crypto::sha256_hex(&payload).as_bytes() {
                return Err(AppError::Unsupported {
                    reason: "semantic execution tombstone log checksum is invalid".to_owned(),
                });
            }
            let record = serde_json::from_slice(&payload).map_err(AppError::Json)?;
            self.apply(record)?;
            valid_len = record_start
                .saturating_add(4)
                .saturating_add(u64::try_from(length).unwrap_or(u64::MAX))
                .saturating_add(64);
        }
        Ok(())
    }

    fn get(
        &self,
        account_user_id: &str,
        request_id: &str,
    ) -> Option<&RelaySemanticExecutionTombstone> {
        self.by_request.get(&RelayExecutionTombstoneKey::new(
            account_user_id,
            request_id,
        ))
    }

    fn insert(&mut self, tombstone: RelaySemanticExecutionTombstone) -> Result<bool> {
        validate_relay_execution_tombstone(&tombstone)?;
        let key = RelayExecutionTombstoneKey::from_tombstone(&tombstone);
        if let Some(existing) = self.by_request.get(&key) {
            if existing == &tombstone {
                return Ok(false);
            }
            return Err(AppError::Unsupported {
                reason: "semantic relay execution tombstone identity is duplicated".to_owned(),
            });
        }
        self.append_records(&[RelayExecutionTombstoneRecord::Add {
            tombstone: tombstone.clone(),
        }])?;
        self.by_request.insert(key, tombstone);
        Ok(true)
    }

    fn import_v6(&mut self, tombstones: Vec<RelaySemanticExecutionTombstone>) -> Result<()> {
        validate_relay_execution_tombstones(&tombstones)?;
        let mut records = Vec::new();
        let mut additions = Vec::new();
        for tombstone in tombstones {
            let key = RelayExecutionTombstoneKey::from_tombstone(&tombstone);
            if let Some(existing) = self.by_request.get(&key) {
                if existing != &tombstone {
                    return Err(AppError::Unsupported {
                        reason: "semantic relay execution tombstone identity is duplicated"
                            .to_owned(),
                    });
                }
                continue;
            }
            records.push(RelayExecutionTombstoneRecord::Add {
                tombstone: tombstone.clone(),
            });
            additions.push((key, tombstone));
        }
        if !records.is_empty() {
            self.append_records(&records)?;
            self.by_request.extend(additions);
        }
        Ok(())
    }

    fn retire_session(&mut self, session_id: &str) -> Result<()> {
        let retained = self
            .by_request
            .iter()
            .filter(|(_, tombstone)| tombstone.session_id != session_id)
            .map(|(key, tombstone)| (key.clone(), tombstone.clone()))
            .collect::<HashMap<_, _>>();
        self.compact_values(retained.values())?;
        self.by_request = retained;
        Ok(())
    }

    fn retire_account(&mut self, account_user_id: &str) -> Result<()> {
        let retained = self
            .by_request
            .iter()
            .filter(|(_, tombstone)| tombstone.account_user_id != account_user_id)
            .map(|(key, tombstone)| (key.clone(), tombstone.clone()))
            .collect::<HashMap<_, _>>();
        self.compact_values(retained.values())?;
        self.by_request = retained;
        Ok(())
    }

    fn apply(&mut self, record: RelayExecutionTombstoneRecord) -> Result<()> {
        match record {
            RelayExecutionTombstoneRecord::Add { tombstone } => {
                validate_relay_execution_tombstone(&tombstone)?;
                let key = RelayExecutionTombstoneKey::from_tombstone(&tombstone);
                if let Some(existing) = self.by_request.get(&key)
                    && existing != &tombstone
                {
                    return Err(AppError::Unsupported {
                        reason: "semantic relay execution tombstone identity is duplicated"
                            .to_owned(),
                    });
                }
                self.by_request.insert(key, tombstone);
            }
        }
        Ok(())
    }

    fn append_records(&self, records: &[RelayExecutionTombstoneRecord]) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        let encoded = records
            .iter()
            .map(encode_tombstone_record)
            .collect::<Result<Vec<_>>>()?;
        if !self.path.exists() {
            crate::support::storage::atomic_file::atomic_write(
                &self.path,
                TOMBSTONE_LOG_HEADER,
                crate::support::storage::atomic_file::FileMode::UserPrivate,
            )?;
        }
        let mut file = OpenOptions::new()
            .append(true)
            .open(&self.path)
            .map_err(AppError::Io)?;
        for record in &encoded {
            file.write_all(record).map_err(AppError::Io)?;
        }
        file.sync_all().map_err(AppError::Io)
    }

    fn compact_values<'a>(
        &self,
        tombstones: impl Iterator<Item = &'a RelaySemanticExecutionTombstone>,
    ) -> Result<()> {
        let mut bytes = TOMBSTONE_LOG_HEADER.to_vec();
        for tombstone in tombstones {
            write_tombstone_record(
                &mut bytes,
                &RelayExecutionTombstoneRecord::Add {
                    tombstone: tombstone.clone(),
                },
            )?;
        }
        crate::support::storage::atomic_file::atomic_write(
            &self.path,
            &bytes,
            crate::support::storage::atomic_file::FileMode::UserPrivate,
        )
    }
}

fn recover_torn_log_tail(
    file: &std::fs::File,
    valid_len: u64,
    error: std::io::Error,
) -> Result<()> {
    if error.kind() != std::io::ErrorKind::UnexpectedEof {
        return Err(AppError::Io(error));
    }
    file.set_len(valid_len).map_err(AppError::Io)?;
    file.sync_all().map_err(AppError::Io)
}

fn encode_tombstone_record(record: &RelayExecutionTombstoneRecord) -> Result<Vec<u8>> {
    let payload = serde_json::to_vec(record).map_err(AppError::Json)?;
    if payload.is_empty() || payload.len() > MAX_TOMBSTONE_RECORD_BYTES {
        return Err(AppError::Unsupported {
            reason: "semantic execution tombstone log record is oversized".to_owned(),
        });
    }
    let length = u32::try_from(payload.len()).map_err(|_| AppError::Unsupported {
        reason: "semantic execution tombstone log record is oversized".to_owned(),
    })?;
    let mut encoded = Vec::with_capacity(4 + payload.len() + 64);
    encoded.extend_from_slice(&length.to_le_bytes());
    encoded.extend_from_slice(&payload);
    encoded.extend_from_slice(kodosi_backend_client::crypto::sha256_hex(&payload).as_bytes());
    Ok(encoded)
}

fn write_tombstone_record(
    writer: &mut impl Write,
    record: &RelayExecutionTombstoneRecord,
) -> Result<()> {
    writer
        .write_all(&encode_tombstone_record(record)?)
        .map_err(AppError::Io)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedSteersHeader {
    version: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoricalSteerQueueEntry {
    steer_id: String,
    #[serde(default)]
    account_user_id: String,
    #[serde(default)]
    request_id: String,
    #[serde(default)]
    session_incarnation_id: String,
    #[serde(default)]
    mode: SemanticSendMode,
    session_id: String,
    text: String,
    queued_at_ms: u64,
    delivery_state: SteerDeliveryState,
    #[serde(default)]
    at_tool_use_id: Option<String>,
}

impl HistoricalSteerQueueEntry {
    fn into_current(self) -> Option<SteerQueueEntry> {
        if self.session_incarnation_id.is_empty() {
            return None;
        }
        Some(SteerQueueEntry {
            steer_id: self.steer_id,
            account_user_id: self.account_user_id,
            request_id: self.request_id,
            session_incarnation_id: self.session_incarnation_id,
            mode: self.mode,
            session_id: self.session_id,
            text: self.text,
            queued_at_ms: self.queued_at_ms,
            delivery_state: self.delivery_state,
            at_tool_use_id: self.at_tool_use_id,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedSteers<Entry = SteerQueueEntry> {
    version: u32,
    entries: Vec<Entry>,
    #[serde(default)]
    completed: Vec<SemanticSendReceipt>,
    #[serde(default)]
    relay_requesters: Vec<RelaySemanticRequester>,
    #[serde(default)]
    relay_execution_tombstones: Vec<RelaySemanticExecutionTombstone>,
    #[serde(default)]
    ready_boundaries: Vec<SemanticBoundary>,
    #[serde(default)]
    turn_states: Vec<SemanticTurnStateRecord>,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SemanticSendOrigin {
    DirectLocal,
    #[default]
    Relay,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SemanticSendReceipt {
    pub(crate) account_user_id: String,
    pub(crate) request_id: String,
    pub(crate) session_id: String,
    pub(crate) session_incarnation_id: String,
    pub(crate) mode: SemanticSendMode,
    pub(crate) payload_sha256: String,
    #[serde(default)]
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) queued_at_ms: u64,
    #[serde(default)]
    pub(crate) at_tool_use_id: Option<String>,
    pub(crate) outcome: SteerDeliveryState,
    #[serde(default)]
    pub(crate) origin: SemanticSendOrigin,
    #[serde(default)]
    pub(crate) relay_acknowledged: bool,
}

impl SemanticSendReceipt {
    fn as_entry(&self) -> SteerQueueEntry {
        SteerQueueEntry {
            steer_id: self.request_id.clone(),
            account_user_id: self.account_user_id.clone(),
            request_id: self.request_id.clone(),
            session_incarnation_id: self.session_incarnation_id.clone(),
            mode: self.mode,
            session_id: self.session_id.clone(),
            text: self.text.clone(),
            queued_at_ms: self.queued_at_ms,
            delivery_state: self.outcome,
            at_tool_use_id: self.at_tool_use_id.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SemanticSendAdmission {
    New(SteerQueueEntry),
    Pending(SteerQueueEntry),
    Completed(SteerQueueEntry),
    Tombstoned(SteerQueueEntry),
}

impl SteeringState {
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn load_default() -> Result<Self> {
        let path = crate::support::storage::paths::data_root()?.join("pending-steers.json");
        Self::load(path)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "loading validates and safely migrates every persisted steering schema"
    )]
    fn load(path: PathBuf) -> Result<Self> {
        #[rustfmt::skip]
        let (entries, completed, ready_boundaries, relay_requesters, metadata, loaded_version) = match std::fs::read(&path) {
                Ok(bytes) => {
                    let header: PersistedSteersHeader =
                        serde_json::from_slice(&bytes).map_err(AppError::Json)?;
                    if !matches!(
                        header.version,
                        STEER_FILE_VERSION
                            | TOMBSTONE_JSON_FILE_VERSION
                            | PREVIOUS_STEER_FILE_VERSION
                            | INCARNATION_STEER_FILE_VERSION
                            | RECEIPT_ORIGIN_FILE_VERSION
                            | OLDER_STEER_FILE_VERSION
                            | LEGACY_STEER_FILE_VERSION
                    ) {
                        return Err(AppError::Unsupported {
                            reason: format!(
                                "unsupported pending-steer file version {}",
                                header.version
                            ),
                        });
                    }
                    let persisted = if header.version == STEER_FILE_VERSION {
                        serde_json::from_slice::<PersistedSteers>(&bytes).map_err(AppError::Json)?
                    } else {
                        let historical = serde_json::from_slice::<
                            PersistedSteers<HistoricalSteerQueueEntry>,
                        >(&bytes)
                        .map_err(AppError::Json)?;
                        PersistedSteers {
                            version: historical.version,
                            entries: historical
                                .entries
                                .into_iter()
                                .filter_map(HistoricalSteerQueueEntry::into_current)
                                .collect(),
                            completed: historical.completed,
                            relay_requesters: historical.relay_requesters,
                            relay_execution_tombstones: historical.relay_execution_tombstones,
                            ready_boundaries: historical.ready_boundaries,
                            turn_states: historical.turn_states,
                        }
                    };

                    let mut entries = persisted.entries;
                    let mut completed = persisted.completed;
                    let mut ready_boundaries = persisted.ready_boundaries;
                    let mut relay_requesters = persisted.relay_requesters;
                    let relay_execution_tombstones = if persisted.version
                        == TOMBSTONE_JSON_FILE_VERSION
                    {
                        persisted.relay_execution_tombstones
                    } else {
                        if !persisted.relay_execution_tombstones.is_empty() {
                            return Err(AppError::Unsupported {
                                reason: "semantic execution tombstones are invalid in this pending-steer version"
                                    .to_owned(),
                            });
                        }
                        Vec::new()
                    };
                    let turn_states = persisted.turn_states;
                    validate_relay_execution_tombstones(&relay_execution_tombstones)?;
                    if turn_states.len() > MAX_SEMANTIC_TURN_STATES {
                        return Err(AppError::Unsupported {
                            reason: "semantic turn states exceed retention limit".to_owned(),
                        });
                    }
                    if persisted.version < RECEIPT_ORIGIN_FILE_VERSION {
                        relay_requesters.clear();
                        for receipt in &mut completed {
                            receipt.origin = SemanticSendOrigin::DirectLocal;
                        }
                    }
                    if persisted.version == RECEIPT_ORIGIN_FILE_VERSION {
                        let relay_identities = relay_requesters
                            .iter()
                            .map(|requester| {
                                (
                                    requester.account_user_id.as_str(),
                                    requester.request_id.as_str(),
                                )
                            })
                            .collect::<std::collections::HashSet<_>>();
                        for receipt in &mut completed {
                            receipt.origin = if relay_identities.contains(&(
                                receipt.account_user_id.as_str(),
                                receipt.request_id.as_str(),
                            )) {
                                SemanticSendOrigin::Relay
                            } else {
                                SemanticSendOrigin::DirectLocal
                            };
                        }
                    }
                    if persisted.version == LEGACY_STEER_FILE_VERSION {
                        entries.retain(|entry| {
                            !entry.account_user_id.is_empty()
                                && !entry.request_id.is_empty()
                                && !entry.session_incarnation_id.is_empty()
                        });
                        ready_boundaries.clear();
                        completed.retain(|receipt| {
                            !receipt.account_user_id.is_empty()
                                && !receipt.request_id.is_empty()
                                && !receipt.session_incarnation_id.is_empty()
                                && !receipt.text.is_empty()
                        });
                    }
                    if entries.len() > MAX_PENDING_STEERS {
                        return Err(AppError::Unsupported {
                            reason: "pending-steer file exceeds retention limit".to_owned(),
                        });
                    }
                    if completed
                        .iter()
                        .filter(|receipt| receipt.origin == SemanticSendOrigin::Relay)
                        .count()
                        > MAX_COMPLETED_RELAY_SEMANTIC_SENDS
                        || completed
                            .iter()
                            .filter(|receipt| receipt.origin == SemanticSendOrigin::DirectLocal)
                            .count()
                            > MAX_COMPLETED_DIRECT_SEMANTIC_SENDS
                    {
                        return Err(AppError::Unsupported {
                            reason: "completed semantic-send receipts exceed retention limit"
                                .to_owned(),
                        });
                    }
                    ready_boundaries.retain(|boundary| {
                        entries.iter().any(|entry| {
                            entry.account_user_id == boundary.account_user_id
                                && entry.session_id == boundary.session_id
                                && entry.mode == boundary.mode
                                && matches!(
                                    entry.delivery_state,
                                    SteerDeliveryState::Preparing | SteerDeliveryState::Queued
                                )
                        })
                    });
                    (
                        entries.into(),
                        completed.into(),
                        ready_boundaries.into(),
                        relay_requesters,
                        LoadedSemanticMetadata {
                            relay_execution_tombstones,
                            turn_states,
                        },
                        persisted.version,
                    )
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (
                    VecDeque::new(),
                    VecDeque::new(),
                    VecDeque::new(),
                    Vec::new(),
                    LoadedSemanticMetadata::empty(),
                    STEER_FILE_VERSION,
                ),
                Err(error) => return Err(AppError::Io(error)),
            };
        let mut relay_execution_tombstones = RelayExecutionTombstoneStore::load(&path)?;
        relay_execution_tombstones.import_v6(metadata.relay_execution_tombstones)?;
        let state = Self {
            path,
            entries,
            ready_boundaries,
            completed,
            relay_requesters,
            relay_execution_tombstones,
            turn_states: metadata.turn_states,
        };
        if loaded_version < STEER_FILE_VERSION {
            state.persist()?;
        }
        Ok(state)
    }

    pub(crate) fn load_at(path: PathBuf) -> Result<Self> {
        Self::load(path)
    }

    #[cfg(test)]
    pub(crate) fn queue(&mut self, session_id: SessionId, text: String) -> Result<SteerQueueEntry> {
        let entry = self.queue_semantic(
            String::new(),
            uuid::Uuid::now_v7(),
            session_id,
            uuid::Uuid::nil(),
            SemanticSendMode::Steer,
            text,
        )?;
        self.acknowledge_queued(&entry.steer_id)?
            .ok_or(AppError::NotFound)
    }

    pub(crate) fn note_semantic_turn_state(
        &mut self,
        account_user_id: &str,
        session_id: SessionId,
        session_incarnation_id: uuid::Uuid,
        state: SemanticTurnState,
    ) -> Result<()> {
        if account_user_id.trim().is_empty() || session_incarnation_id.is_nil() {
            return Err(AppError::Unsupported {
                reason: "semantic turn-state identity is invalid".to_owned(),
            });
        }
        let session_id = session_id.to_string();
        let session_incarnation_id = session_incarnation_id.to_string();
        let previous = self.turn_states.clone();
        let previous_boundaries = self.ready_boundaries.clone();
        self.turn_states.retain(|record| {
            record.account_user_id != account_user_id || record.session_id != session_id
        });
        if self.turn_states.len() >= MAX_SEMANTIC_TURN_STATES {
            self.turn_states = previous;
            return Err(AppError::ChannelFull {
                session: session_id,
            });
        }
        self.turn_states.push(SemanticTurnStateRecord {
            account_user_id: account_user_id.to_owned(),
            session_id: session_id.clone(),
            session_incarnation_id,
            state,
        });
        if state == SemanticTurnState::Idle {
            let has_boundary = self.ready_boundaries.iter().any(|boundary| {
                boundary.account_user_id == account_user_id
                    && boundary.session_id == session_id
                    && boundary.mode == SemanticSendMode::Queue
            });
            let has_queue = self.entries.iter().any(|entry| {
                entry.account_user_id == account_user_id
                    && entry.session_id == session_id
                    && entry.mode == SemanticSendMode::Queue
                    && matches!(
                        entry.delivery_state,
                        SteerDeliveryState::Preparing | SteerDeliveryState::Queued
                    )
            });
            if has_queue && !has_boundary {
                self.ready_boundaries.push_back(SemanticBoundary {
                    account_user_id: account_user_id.to_owned(),
                    session_id,
                    mode: SemanticSendMode::Queue,
                    tool_use_id: None,
                });
            }
        }
        if let Err(error) = self.persist() {
            self.turn_states = previous;
            self.ready_boundaries = previous_boundaries;
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn semantic_turn_state(
        &self,
        account_user_id: &str,
        session_id: SessionId,
        session_incarnation_id: uuid::Uuid,
    ) -> Option<SemanticTurnState> {
        self.turn_states
            .iter()
            .find(|record| {
                record.account_user_id == account_user_id
                    && record.session_id == session_id.to_string()
                    && record.session_incarnation_id == session_incarnation_id.to_string()
            })
            .map(|record| record.state)
    }

    pub(crate) fn relay_requester_device(
        &self,
        account_user_id: &str,
        request_id: &str,
    ) -> Option<&str> {
        self.relay_requesters
            .iter()
            .find(|requester| {
                requester.account_user_id == account_user_id && requester.request_id == request_id
            })
            .map(|requester| requester.requester_device_id.as_str())
            .or_else(|| {
                self.relay_execution_tombstone(account_user_id, request_id)
                    .map(|tombstone| tombstone.requester_device_id.as_str())
            })
    }

    fn origin_for(&self, entry: &SteerQueueEntry) -> SemanticSendOrigin {
        if self
            .relay_requester_device(&entry.account_user_id, &entry.request_id)
            .is_some()
        {
            SemanticSendOrigin::Relay
        } else {
            SemanticSendOrigin::DirectLocal
        }
    }

    pub(crate) fn pending_relay_receipts_for_account(
        &self,
        account_user_id: &str,
    ) -> Vec<SemanticSendReceipt> {
        self.completed
            .iter()
            .filter(|receipt| {
                receipt.account_user_id == account_user_id
                    && receipt.origin == SemanticSendOrigin::Relay
                    && !receipt.relay_acknowledged
            })
            .cloned()
            .collect()
    }

    pub(crate) fn pending_relay_receipts(
        &self,
        account_user_id: &str,
        session_id: SessionId,
    ) -> Vec<SemanticSendReceipt> {
        let session_id = session_id.to_string();
        self.completed
            .iter()
            .filter(|receipt| {
                receipt.account_user_id == account_user_id
                    && receipt.session_id == session_id
                    && receipt.origin == SemanticSendOrigin::Relay
                    && !receipt.relay_acknowledged
            })
            .cloned()
            .collect()
    }

    pub(crate) fn acknowledge_relay_receipt(
        &mut self,
        account_user_id: &str,
        request_id: uuid::Uuid,
        session_id: SessionId,
        incarnation_id: uuid::Uuid,
    ) -> Result<bool> {
        let request_id = request_id.to_string();
        if let Some(tombstone) = self.relay_execution_tombstone(account_user_id, &request_id) {
            if tombstone.session_id != session_id.to_string()
                || tombstone.session_incarnation_id != incarnation_id.to_string()
            {
                return Ok(false);
            }
            let Some(index) = self.completed.iter().position(|receipt| {
                receipt.account_user_id == account_user_id
                    && receipt.request_id == request_id
                    && receipt.session_id == session_id.to_string()
                    && receipt.session_incarnation_id == incarnation_id.to_string()
                    && receipt.origin == SemanticSendOrigin::Relay
            }) else {
                return Ok(true);
            };
            let previous_completed = self.completed.clone();
            let previous_requesters = self.relay_requesters.clone();
            self.completed.remove(index);
            self.relay_requesters.retain(|requester| {
                requester.account_user_id != account_user_id || requester.request_id != request_id
            });
            if let Err(error) = self.persist() {
                self.completed = previous_completed;
                self.relay_requesters = previous_requesters;
                return Err(error);
            }
            return Ok(true);
        }
        let Some(index) = self.completed.iter().position(|receipt| {
            receipt.account_user_id == account_user_id
                && receipt.request_id == request_id
                && receipt.session_id == session_id.to_string()
                && receipt.session_incarnation_id == incarnation_id.to_string()
                && receipt.origin == SemanticSendOrigin::Relay
        }) else {
            return Ok(false);
        };
        let Some(requester_device_id) = self
            .relay_requesters
            .iter()
            .find(|requester| {
                requester.account_user_id == account_user_id && requester.request_id == request_id
            })
            .map(|requester| requester.requester_device_id.clone())
        else {
            return Err(AppError::Unsupported {
                reason: "semantic relay receipt acknowledgement lacks requester identity"
                    .to_owned(),
            });
        };
        let receipt = &self.completed[index];
        let tombstone = RelaySemanticExecutionTombstone {
            account_user_id: receipt.account_user_id.clone(),
            request_id: receipt.request_id.clone(),
            session_id: receipt.session_id.clone(),
            session_incarnation_id: receipt.session_incarnation_id.clone(),
            mode: receipt.mode,
            payload_sha256: receipt.payload_sha256.clone(),
            requester_device_id,
            outcome: receipt.outcome,
        };
        let previous_completed = self.completed.clone();
        let previous_requesters = self.relay_requesters.clone();
        self.completed.remove(index);
        self.relay_requesters.retain(|requester| {
            requester.account_user_id != account_user_id || requester.request_id != request_id
        });
        self.relay_execution_tombstones.insert(tombstone)?;

        if let Err(error) = self.persist() {
            self.completed = previous_completed;
            self.relay_requesters = previous_requesters;
            return Err(error);
        }
        Ok(true)
    }

    pub(crate) fn completed_receipt(
        &self,
        account_user_id: &str,
        request_id: &str,
    ) -> Option<&SemanticSendReceipt> {
        self.completed.iter().find(|receipt| {
            receipt.account_user_id == account_user_id && receipt.request_id == request_id
        })
    }

    pub(crate) fn relay_receipt_for_exact_retry(
        &self,
        account_user_id: &str,
        request_id: &str,
        requester_device_id: &str,
    ) -> Option<SemanticSendReceipt> {
        if let Some(receipt) = self.completed_receipt(account_user_id, request_id) {
            return (receipt.origin == SemanticSendOrigin::Relay
                && self.relay_requesters.iter().any(|requester| {
                    requester.account_user_id == account_user_id
                        && requester.request_id == request_id
                        && requester.requester_device_id == requester_device_id
                }))
            .then(|| receipt.clone());
        }
        let tombstone = self.relay_execution_tombstone(account_user_id, request_id)?;
        if tombstone.requester_device_id != requester_device_id {
            return None;
        }
        Some(SemanticSendReceipt {
            account_user_id: tombstone.account_user_id.clone(),
            request_id: tombstone.request_id.clone(),
            session_id: tombstone.session_id.clone(),
            session_incarnation_id: tombstone.session_incarnation_id.clone(),
            mode: tombstone.mode,
            payload_sha256: tombstone.payload_sha256.clone(),
            text: String::new(),
            queued_at_ms: 0,
            at_tool_use_id: None,
            outcome: tombstone.outcome,
            origin: SemanticSendOrigin::Relay,
            relay_acknowledged: true,
        })
    }

    fn relay_execution_tombstone(
        &self,
        account_user_id: &str,
        request_id: &str,
    ) -> Option<&RelaySemanticExecutionTombstone> {
        self.relay_execution_tombstones
            .get(account_user_id, request_id)
    }

    pub(crate) fn pending_request(
        &self,
        account_user_id: &str,
        request_id: &str,
    ) -> Option<&SteerQueueEntry> {
        self.entries.iter().find(|entry| {
            entry.account_user_id == account_user_id && entry.request_id == request_id
        })
    }

    pub(crate) fn complete_semantic(
        &mut self,
        entry: &SteerQueueEntry,
        outcome: SteerDeliveryState,
        origin: SemanticSendOrigin,
    ) -> Result<SemanticSendReceipt> {
        if origin == SemanticSendOrigin::Relay
            && self
                .relay_requester_device(&entry.account_user_id, &entry.request_id)
                .is_none()
        {
            return Err(AppError::Unsupported {
                reason: "semantic relay completion lacks requester identity".to_owned(),
            });
        }
        let receipt = SemanticSendReceipt {
            account_user_id: entry.account_user_id.clone(),
            request_id: entry.request_id.clone(),
            session_id: entry.session_id.clone(),
            session_incarnation_id: entry.session_incarnation_id.clone(),
            mode: entry.mode,
            payload_sha256: payload_sha256(&entry.text),
            text: entry.text.clone(),
            queued_at_ms: entry.queued_at_ms,
            at_tool_use_id: entry.at_tool_use_id.clone(),
            outcome,
            origin,
            relay_acknowledged: false,
        };
        let previous_entries = self.entries.clone();
        let previous_completed = self.completed.clone();
        let previous_boundaries = self.ready_boundaries.clone();
        let previous_requesters = self.relay_requesters.clone();
        self.entries.retain(|candidate| {
            candidate.account_user_id != entry.account_user_id
                || candidate.request_id != entry.request_id
        });
        self.ready_boundaries.retain(|boundary| {
            boundary.account_user_id != entry.account_user_id
                || boundary.session_id != entry.session_id
                || boundary.mode != entry.mode
        });
        self.completed.push_back(receipt.clone());
        let limit = match origin {
            SemanticSendOrigin::DirectLocal => MAX_COMPLETED_DIRECT_SEMANTIC_SENDS,
            SemanticSendOrigin::Relay => MAX_COMPLETED_RELAY_SEMANTIC_SENDS,
        };
        while self
            .completed
            .iter()
            .filter(|candidate| candidate.origin == origin)
            .count()
            > limit
        {
            let Some(index) = self.completed.iter().position(|candidate| {
                candidate.origin == origin
                    && (origin == SemanticSendOrigin::DirectLocal || candidate.relay_acknowledged)
            }) else {
                self.entries = previous_entries;
                self.completed = previous_completed;
                self.ready_boundaries = previous_boundaries;
                self.relay_requesters = previous_requesters;
                return Err(AppError::ChannelFull {
                    session: entry.session_id.clone(),
                });
            };
            self.completed.remove(index);
        }
        self.prune_relay_requesters();
        if let Err(error) = self.persist() {
            self.entries = previous_entries;
            self.completed = previous_completed;
            self.ready_boundaries = previous_boundaries;
            self.relay_requesters = previous_requesters;
            return Err(error);
        }
        Ok(receipt)
    }

    pub(crate) fn settle_delivery_unknown(
        &mut self,
        entry: &SteerQueueEntry,
        origin: SemanticSendOrigin,
    ) -> Result<SemanticSendReceipt> {
        self.complete_semantic(entry, SteerDeliveryState::DeliveryUnknown, origin)
    }

    pub(crate) fn existing_semantic(
        &self,
        account_user_id: &str,
        request_id: uuid::Uuid,
        session_id: SessionId,
        session_incarnation_id: uuid::Uuid,
        mode: SemanticSendMode,
        text: &str,
    ) -> Result<Option<SemanticSendAdmission>> {
        let fingerprint = payload_sha256(text);
        if let Some(existing) = self.pending_request(account_user_id, &request_id.to_string()) {
            if semantic_entry_matches(
                existing,
                account_user_id,
                session_id,
                session_incarnation_id,
                mode,
                &fingerprint,
            ) {
                return Ok(Some(SemanticSendAdmission::Pending(existing.clone())));
            }
            return Err(AppError::Unsupported {
                reason: "semantic send request ID was reused with different input".to_owned(),
            });
        }
        if let Some(existing) = self.completed_receipt(account_user_id, &request_id.to_string()) {
            if existing.account_user_id == account_user_id
                && existing.session_id == session_id.to_string()
                && existing.session_incarnation_id == session_incarnation_id.to_string()
                && existing.mode == mode
                && existing.payload_sha256 == fingerprint
            {
                return Ok(Some(SemanticSendAdmission::Completed(existing.as_entry())));
            }
            return Err(AppError::Unsupported {
                reason: "semantic send request ID was reused with different input".to_owned(),
            });
        }
        if let Some(existing) =
            self.relay_execution_tombstone(account_user_id, &request_id.to_string())
        {
            if existing.session_id == session_id.to_string()
                && existing.session_incarnation_id == session_incarnation_id.to_string()
                && existing.mode == mode
                && existing.payload_sha256 == fingerprint
            {
                return Ok(Some(SemanticSendAdmission::Tombstoned(existing.as_entry())));
            }
            return Err(AppError::Unsupported {
                reason: "semantic send request ID was reused with different input".to_owned(),
            });
        }
        Ok(None)
    }

    pub(crate) fn validate_new_semantic(&self, session_id: SessionId, text: &str) -> Result<()> {
        if text.trim().is_empty() {
            return Err(AppError::Unsupported {
                reason: "steer text must not be empty".to_owned(),
            });
        }
        if text.len() > MAX_STEER_TEXT_BYTES {
            return Err(AppError::Unsupported {
                reason: format!("steer text exceeds {MAX_STEER_TEXT_BYTES} bytes"),
            });
        }
        let session_id_string = session_id.to_string();
        if self.entries.len() >= MAX_PENDING_STEERS
            || self
                .entries
                .iter()
                .filter(|entry| entry.session_id == session_id_string)
                .count()
                >= MAX_PENDING_STEERS_PER_SESSION
        {
            return Err(AppError::ChannelFull {
                session: session_id_string,
            });
        }
        Ok(())
    }

    pub(crate) fn admit_semantic(
        &mut self,
        account_user_id: String,
        request_id: uuid::Uuid,
        session_id: SessionId,
        session_incarnation_id: uuid::Uuid,
        mode: SemanticSendMode,
        text: String,
    ) -> Result<SemanticSendAdmission> {
        self.admit_semantic_with_requester(
            account_user_id,
            request_id,
            session_id,
            session_incarnation_id,
            mode,
            text,
            None,
        )
    }

    pub(crate) fn admit_relay_semantic(
        &mut self,
        account_user_id: String,
        request_id: uuid::Uuid,
        session_id: SessionId,
        session_incarnation_id: uuid::Uuid,
        mode: SemanticSendMode,
        text: String,
        requester_device_id: &str,
    ) -> Result<SemanticSendAdmission> {
        self.admit_semantic_with_requester(
            account_user_id,
            request_id,
            session_id,
            session_incarnation_id,
            mode,
            text,
            Some(requester_device_id),
        )
    }

    fn ensure_relay_receipt_capacity(&self, session_id: SessionId) -> Result<()> {
        let protected_receipts = self
            .completed
            .iter()
            .filter(|receipt| {
                receipt.origin == SemanticSendOrigin::Relay && !receipt.relay_acknowledged
            })
            .count();
        let reserved_receipts = self
            .entries
            .iter()
            .filter(|entry| {
                self.relay_requester_device(&entry.account_user_id, &entry.request_id)
                    .is_some()
            })
            .count();
        if protected_receipts.saturating_add(reserved_receipts)
            >= MAX_COMPLETED_RELAY_SEMANTIC_SENDS
        {
            return Err(AppError::ChannelFull {
                session: session_id.to_string(),
            });
        }
        Ok(())
    }

    fn admit_semantic_with_requester(
        &mut self,
        account_user_id: String,
        request_id: uuid::Uuid,
        session_id: SessionId,
        session_incarnation_id: uuid::Uuid,
        mode: SemanticSendMode,
        text: String,
        requester_device_id: Option<&str>,
    ) -> Result<SemanticSendAdmission> {
        let request_id_text = request_id.to_string();
        let existing_requester = self
            .relay_requester_device(&account_user_id, &request_id_text)
            .map(str::to_owned);
        if let Some(device_id) = requester_device_id {
            if device_id.trim().is_empty() {
                return Err(AppError::Unsupported {
                    reason: "semantic relay requester device ID is required".to_owned(),
                });
            }
            if existing_requester
                .as_deref()
                .is_some_and(|requester| requester != device_id)
            {
                return Err(AppError::Unsupported {
                    reason: "semantic relay request ID was reused by another device".to_owned(),
                });
            }
        }
        if let Some(existing) = self.existing_semantic(
            &account_user_id,
            request_id,
            session_id,
            session_incarnation_id,
            mode,
            &text,
        )? {
            let origin_matches = requester_device_id.map_or_else(
                || existing_requester.is_none(),
                |device| existing_requester.as_deref() == Some(device),
            );
            if !origin_matches {
                return Err(AppError::Unsupported {
                    reason: "semantic send request ID was reused across direct and relay origins"
                        .to_owned(),
                });
            }
            return Ok(existing);
        }
        if requester_device_id.is_some() {
            self.ensure_relay_receipt_capacity(session_id)?;
        }
        self.validate_new_semantic(session_id, &text)?;
        let entry = SteerQueueEntry {
            steer_id: semantic_steer_id(&account_user_id, &request_id_text),
            account_user_id: account_user_id.clone(),
            request_id: request_id_text.clone(),
            session_incarnation_id: session_incarnation_id.to_string(),
            mode,
            session_id: session_id.to_string(),
            text,
            queued_at_ms: current_epoch_ms(),
            delivery_state: SteerDeliveryState::Preparing,
            at_tool_use_id: None,
        };
        self.entries.push_back(entry.clone());
        let requester_added = if let Some(device_id) = requester_device_id
            && existing_requester.is_none()
        {
            self.relay_requesters.push(RelaySemanticRequester {
                account_user_id,
                request_id: request_id_text,
                requester_device_id: device_id.to_owned(),
            });
            true
        } else {
            false
        };
        let idle_queue = mode == SemanticSendMode::Queue
            && self.semantic_turn_state(&entry.account_user_id, session_id, session_incarnation_id)
                == Some(SemanticTurnState::Idle)
            && !self.ready_boundaries.iter().any(|boundary| {
                boundary.account_user_id == entry.account_user_id
                    && boundary.session_id == entry.session_id
                    && boundary.mode == SemanticSendMode::Queue
            });
        if idle_queue {
            self.ready_boundaries.push_back(SemanticBoundary {
                account_user_id: entry.account_user_id.clone(),
                session_id: entry.session_id.clone(),
                mode: SemanticSendMode::Queue,
                tool_use_id: None,
            });
        }
        if let Err(error) = self.persist() {
            self.entries.pop_back();
            if requester_added {
                self.relay_requesters.pop();
            }
            if idle_queue {
                self.ready_boundaries.pop_back();
            }
            return Err(error);
        }
        Ok(SemanticSendAdmission::New(entry))
    }

    #[cfg(test)]
    pub(crate) fn queue_semantic(
        &mut self,
        account_user_id: String,
        request_id: uuid::Uuid,
        session_id: SessionId,
        session_incarnation_id: uuid::Uuid,
        mode: SemanticSendMode,
        text: String,
    ) -> Result<SteerQueueEntry> {
        match self.admit_semantic(
            account_user_id,
            request_id,
            session_id,
            session_incarnation_id,
            mode,
            text,
        )? {
            SemanticSendAdmission::New(entry)
            | SemanticSendAdmission::Pending(entry)
            | SemanticSendAdmission::Completed(entry)
            | SemanticSendAdmission::Tombstoned(entry) => Ok(entry),
        }
    }

    pub(crate) fn cancel_semantic(
        &mut self,
        account_user_id: &str,
        session_id: SessionId,
        steer_id: &str,
        origin: SemanticSendOrigin,
    ) -> Result<Option<SteerQueueEntry>> {
        let session_id = session_id.to_string();
        let Some(index) = self.entries.iter().position(|entry| {
            entry.account_user_id == account_user_id
                && entry.session_id == session_id
                && entry.steer_id == steer_id
                && matches!(
                    entry.delivery_state,
                    SteerDeliveryState::Preparing | SteerDeliveryState::Queued
                )
        }) else {
            return Ok(None);
        };
        let entry = self.entries[index].clone();
        self.complete_semantic(&entry, SteerDeliveryState::Cancelled, origin)?;
        let mut completed = entry;
        completed.delivery_state = SteerDeliveryState::Cancelled;
        Ok(Some(completed))
    }

    #[cfg(test)]
    pub(crate) fn cancel(
        &mut self,
        session_id: SessionId,
        steer_id: &str,
    ) -> Result<Option<SteerQueueEntry>> {
        let session_id = session_id.to_string();
        let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.session_id == session_id && entry.steer_id == steer_id)
        else {
            return Ok(None);
        };
        let previous_requesters = self.relay_requesters.clone();
        let removed = self.entries.remove(index);
        self.prune_relay_requesters();
        if let Err(error) = self.persist() {
            if let Some(entry) = removed {
                self.entries.insert(index, entry);
            }
            self.relay_requesters = previous_requesters;
            return Err(error);
        }
        Ok(removed)
    }

    pub(crate) fn entries(&self, session_id: SessionId) -> Vec<SteerQueueEntry> {
        let session_id = session_id.to_string();
        self.entries
            .iter()
            .filter(|entry| entry.session_id == session_id)
            .cloned()
            .collect()
    }

    pub(crate) fn entries_for_account(
        &self,
        account_user_id: &str,
        session_id: SessionId,
    ) -> Vec<SteerQueueEntry> {
        let session_id = session_id.to_string();
        self.entries
            .iter()
            .filter(|entry| {
                entry.account_user_id == account_user_id && entry.session_id == session_id
            })
            .cloned()
            .collect()
    }

    pub(crate) fn query_semantic(
        &self,
        account_user_id: &str,
        session_id: SessionId,
        request_id: Option<&str>,
    ) -> Vec<SteerQueueEntry> {
        if let Some(request_id) = request_id {
            if let Some(entry) = self.entries.iter().find(|entry| {
                entry.account_user_id == account_user_id
                    && entry.session_id == session_id.to_string()
                    && entry.request_id == request_id
            }) {
                return vec![entry.clone()];
            }
            return self
                .completed
                .iter()
                .find(|receipt| {
                    receipt.account_user_id == account_user_id
                        && receipt.session_id == session_id.to_string()
                        && receipt.request_id == request_id
                })
                .map_or_else(Vec::new, |receipt| vec![receipt.as_entry()]);
        }
        let mut results = self.entries_for_account(account_user_id, session_id);
        let session_id = session_id.to_string();
        results.extend(
            self.completed
                .iter()
                .filter(|receipt| {
                    receipt.account_user_id == account_user_id && receipt.session_id == session_id
                })
                .map(SemanticSendReceipt::as_entry),
        );
        results
    }

    pub(crate) fn all_entries(&self) -> Vec<SteerQueueEntry> {
        self.entries.iter().cloned().collect()
    }

    pub(crate) fn note_boundary(
        &mut self,
        account_user_id: &str,
        session_id: SessionId,
        mode: SemanticSendMode,
        tool_use_id: Option<String>,
    ) -> Result<()> {
        let session_id_string = session_id.to_string();
        if self.entries.iter().any(|entry| {
            entry.account_user_id == account_user_id
                && entry.session_id == session_id_string
                && entry.mode == mode
                && matches!(
                    entry.delivery_state,
                    SteerDeliveryState::Preparing | SteerDeliveryState::Queued
                )
        }) {
            self.ready_boundaries.push_back(SemanticBoundary {
                account_user_id: account_user_id.to_owned(),
                session_id: session_id_string,
                mode,
                tool_use_id,
            });
            if let Err(error) = self.persist() {
                self.ready_boundaries.pop_back();
                return Err(error);
            }
        }
        Ok(())
    }

    pub(crate) fn note_tool_boundary(
        &mut self,
        account_user_id: &str,
        session_id: SessionId,
        tool_use_id: Option<String>,
    ) -> Result<()> {
        self.note_boundary(
            account_user_id,
            session_id,
            SemanticSendMode::Steer,
            tool_use_id,
        )
    }

    #[cfg(test)]
    pub(crate) fn note_turn_end(
        &mut self,
        account_user_id: &str,
        session_id: SessionId,
    ) -> Result<()> {
        self.note_boundary(account_user_id, session_id, SemanticSendMode::Queue, None)
    }

    pub(crate) fn acknowledge_queued(&mut self, steer_id: &str) -> Result<Option<SteerQueueEntry>> {
        let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.steer_id == steer_id)
        else {
            return Ok(None);
        };
        if self.entries[index].delivery_state != SteerDeliveryState::Preparing {
            return Ok(None);
        }
        let previous_boundaries = self.ready_boundaries.clone();
        self.entries[index].delivery_state = SteerDeliveryState::Queued;
        let entry = self.entries[index].clone();
        if entry.mode == SemanticSendMode::StopAndSend
            && !self.ready_boundaries.iter().any(|boundary| {
                boundary.account_user_id == entry.account_user_id
                    && boundary.session_id == entry.session_id
                    && boundary.mode == SemanticSendMode::StopAndSend
            })
        {
            self.ready_boundaries.push_back(SemanticBoundary {
                account_user_id: entry.account_user_id.clone(),
                session_id: entry.session_id.clone(),
                mode: SemanticSendMode::StopAndSend,
                tool_use_id: None,
            });
        }
        if let Err(error) = self.persist() {
            self.entries[index].delivery_state = SteerDeliveryState::Preparing;
            self.ready_boundaries = previous_boundaries;
            return Err(error);
        }
        Ok(Some(entry))
    }

    pub(crate) fn claim_ready(
        &mut self,
        current_account_user_id: &str,
    ) -> Result<Option<(SteerQueueEntry, Option<String>)>> {
        let boundary_count = self.ready_boundaries.len();
        for _ in 0..boundary_count {
            let Some(boundary) = self.ready_boundaries.pop_front() else {
                return Ok(None);
            };
            if boundary.account_user_id != current_account_user_id {
                self.ready_boundaries.push_back(boundary);
                continue;
            }
            let matching_index = self.entries.iter().position(|entry| {
                entry.account_user_id == boundary.account_user_id
                    && entry.session_id == boundary.session_id
                    && entry.mode == boundary.mode
            });
            let Some(index) = matching_index else {
                if let Err(error) = self.persist() {
                    self.ready_boundaries.push_front(boundary);
                    return Err(error);
                }
                continue;
            };
            if self.entries[index].delivery_state == SteerDeliveryState::Preparing {
                self.ready_boundaries.push_back(boundary);
                continue;
            }
            if self.entries[index].delivery_state != SteerDeliveryState::Queued {
                if let Err(error) = self.persist() {
                    self.ready_boundaries.push_front(boundary);
                    return Err(error);
                }
                continue;
            }
            let previous_tool_use_id = self.entries[index].at_tool_use_id.clone();
            self.entries[index].delivery_state = SteerDeliveryState::DeliveryUnknown;
            self.entries[index]
                .at_tool_use_id
                .clone_from(&boundary.tool_use_id);
            if let Err(error) = self.persist() {
                self.entries[index].delivery_state = SteerDeliveryState::Queued;
                self.entries[index].at_tool_use_id = previous_tool_use_id;
                self.ready_boundaries.push_front(boundary);
                return Err(error);
            }
            return Ok(Some((self.entries[index].clone(), boundary.tool_use_id)));
        }
        Ok(None)
    }

    #[cfg(test)]
    pub(crate) fn complete(&mut self, steer_id: &str) -> Result<Option<SteerQueueEntry>> {
        let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.steer_id == steer_id)
        else {
            return Ok(None);
        };
        let removed = self.entries.remove(index);
        if let Err(error) = self.persist() {
            if let Some(entry) = removed {
                self.entries.insert(index, entry);
            }
            return Err(error);
        }
        Ok(removed)
    }

    pub(crate) fn requeue(&mut self, steer_id: &str) -> Result<Option<SteerQueueEntry>> {
        let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.steer_id == steer_id)
        else {
            return Ok(None);
        };
        let previous_state = self.entries[index].delivery_state;
        let previous_tool_use_id = self.entries[index].at_tool_use_id.take();
        self.entries[index].delivery_state = SteerDeliveryState::Queued;
        if let Err(error) = self.persist() {
            self.entries[index].delivery_state = previous_state;
            self.entries[index].at_tool_use_id = previous_tool_use_id;
            return Err(error);
        }
        Ok(Some(self.entries[index].clone()))
    }

    pub(crate) fn clear_account(&mut self, account_user_id: &str) -> Result<()> {
        let previous_entries = self.entries.clone();
        let previous_completed = self.completed.clone();
        let previous_boundaries = self.ready_boundaries.clone();
        let previous_requesters = self.relay_requesters.clone();
        let previous_turn_states = self.turn_states.clone();
        self.entries
            .retain(|entry| entry.account_user_id != account_user_id);
        self.completed
            .retain(|receipt| receipt.account_user_id != account_user_id);
        self.ready_boundaries
            .retain(|boundary| boundary.account_user_id != account_user_id);
        self.relay_requesters
            .retain(|requester| requester.account_user_id != account_user_id);
        self.turn_states
            .retain(|record| record.account_user_id != account_user_id);
        if let Err(error) = self.persist() {
            self.entries = previous_entries;
            self.completed = previous_completed;
            self.ready_boundaries = previous_boundaries;
            self.relay_requesters = previous_requesters;
            self.turn_states = previous_turn_states;
            return Err(error);
        }
        self.relay_execution_tombstones
            .retire_account(account_user_id)?;
        Ok(())
    }

    pub(crate) fn cancel_session(&mut self, session_id: SessionId) -> Result<Vec<SteerQueueEntry>> {
        let session_id = session_id.to_string();
        let previous_entries = self.entries.clone();
        let previous_completed = self.completed.clone();
        let previous_boundaries = self.ready_boundaries.clone();
        let previous_requesters = self.relay_requesters.clone();
        let previous_turn_states = self.turn_states.clone();
        let cancelled = self
            .entries
            .iter()
            .filter(|entry| entry.session_id == session_id)
            .cloned()
            .collect::<Vec<_>>();
        let origins = cancelled
            .iter()
            .map(|entry| {
                (
                    (entry.account_user_id.clone(), entry.request_id.clone()),
                    self.origin_for(entry),
                )
            })
            .collect::<HashMap<_, _>>();
        self.entries.retain(|entry| entry.session_id != session_id);
        self.ready_boundaries
            .retain(|boundary| boundary.session_id != session_id);
        self.turn_states
            .retain(|record| record.session_id != session_id);
        for entry in &cancelled {
            if entry.request_id.is_empty() {
                continue;
            }
            self.completed.push_back(SemanticSendReceipt {
                account_user_id: entry.account_user_id.clone(),
                request_id: entry.request_id.clone(),
                session_id: entry.session_id.clone(),
                session_incarnation_id: entry.session_incarnation_id.clone(),
                mode: entry.mode,
                payload_sha256: payload_sha256(&entry.text),
                text: entry.text.clone(),
                queued_at_ms: entry.queued_at_ms,
                at_tool_use_id: entry.at_tool_use_id.clone(),
                outcome: if entry.delivery_state == SteerDeliveryState::DeliveryUnknown {
                    SteerDeliveryState::DeliveryUnknown
                } else {
                    SteerDeliveryState::Cancelled
                },
                origin: origins
                    .get(&(entry.account_user_id.clone(), entry.request_id.clone()))
                    .copied()
                    .unwrap_or(SemanticSendOrigin::DirectLocal),
                relay_acknowledged: false,
            });
        }
        for (origin, limit) in [
            (
                SemanticSendOrigin::DirectLocal,
                MAX_COMPLETED_DIRECT_SEMANTIC_SENDS,
            ),
            (
                SemanticSendOrigin::Relay,
                MAX_COMPLETED_RELAY_SEMANTIC_SENDS,
            ),
        ] {
            while self
                .completed
                .iter()
                .filter(|receipt| receipt.origin == origin)
                .count()
                > limit
            {
                let Some(index) = self.completed.iter().position(|receipt| {
                    receipt.origin == origin
                        && (origin == SemanticSendOrigin::DirectLocal || receipt.relay_acknowledged)
                }) else {
                    self.entries = previous_entries;
                    self.completed = previous_completed;
                    self.ready_boundaries = previous_boundaries;
                    self.relay_requesters = previous_requesters;
                    self.turn_states = previous_turn_states;
                    return Err(AppError::ChannelFull {
                        session: session_id,
                    });
                };
                self.completed.remove(index);
            }
        }
        self.prune_relay_requesters();
        if let Err(error) = self.persist() {
            self.entries = previous_entries;
            self.completed = previous_completed;
            self.ready_boundaries = previous_boundaries;
            self.relay_requesters = previous_requesters;
            self.turn_states = previous_turn_states;
            return Err(error);
        }
        self.relay_execution_tombstones
            .retire_session(&session_id)?;
        Ok(cancelled)
    }

    fn prune_relay_requesters(&mut self) {
        self.relay_requesters.retain(|requester| {
            self.entries.iter().any(|entry| {
                entry.account_user_id == requester.account_user_id
                    && entry.request_id == requester.request_id
            }) || self.completed.iter().any(|receipt| {
                receipt.account_user_id == requester.account_user_id
                    && receipt.request_id == requester.request_id
            })
        });
    }

    fn persist(&self) -> Result<()> {
        let body = serde_json::to_vec_pretty(&PersistedSteers {
            version: STEER_FILE_VERSION,
            entries: self.entries.iter().cloned().collect(),
            completed: self.completed.iter().cloned().collect(),
            relay_requesters: self.relay_requesters.clone(),
            relay_execution_tombstones: Vec::new(),
            ready_boundaries: self.ready_boundaries.iter().cloned().collect(),
            turn_states: self.turn_states.clone(),
        })
        .map_err(AppError::Json)?;
        crate::support::storage::atomic_file::atomic_write(
            &self.path,
            &body,
            crate::support::storage::atomic_file::FileMode::UserPrivate,
        )
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SteerCleanupOutcome {
    pub(crate) cancelled: usize,
}

pub(crate) fn payload_sha256(text: &str) -> String {
    kodosi_backend_client::crypto::sha256_hex(text.as_bytes())
}

pub(super) const fn relay_mode(
    mode: SemanticSendMode,
) -> kodosi_backend_client::session_relay::wire::RelaySemanticMode {
    match mode {
        SemanticSendMode::Queue => {
            kodosi_backend_client::session_relay::wire::RelaySemanticMode::Queue
        }
        SemanticSendMode::Steer => {
            kodosi_backend_client::session_relay::wire::RelaySemanticMode::Steer
        }
        SemanticSendMode::StopAndSend => {
            kodosi_backend_client::session_relay::wire::RelaySemanticMode::StopAndSend
        }
    }
}

pub(super) fn relay_outcome(
    outcome: SteerDeliveryState,
) -> Option<kodosi_backend_client::session_relay::wire::RelaySemanticOutcome> {
    match outcome {
        SteerDeliveryState::Injected => {
            Some(kodosi_backend_client::session_relay::wire::RelaySemanticOutcome::Injected)
        }
        SteerDeliveryState::Cancelled => {
            Some(kodosi_backend_client::session_relay::wire::RelaySemanticOutcome::Cancelled)
        }
        SteerDeliveryState::DeliveryUnknown => {
            Some(kodosi_backend_client::session_relay::wire::RelaySemanticOutcome::DeliveryUnknown)
        }
        SteerDeliveryState::Preparing | SteerDeliveryState::Queued => None,
    }
}

fn semantic_entry_matches(
    entry: &SteerQueueEntry,
    account_user_id: &str,
    session_id: SessionId,
    session_incarnation_id: uuid::Uuid,
    mode: SemanticSendMode,
    payload_sha256_value: &str,
) -> bool {
    entry.account_user_id == account_user_id
        && entry.session_id == session_id.to_string()
        && entry.session_incarnation_id == session_incarnation_id.to_string()
        && entry.mode == mode
        && payload_sha256(&entry.text) == payload_sha256_value
}

fn current_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

impl super::Runtime {
    pub(crate) fn semantic_send(
        &mut self,
        request_id: uuid::Uuid,
        session_id: SessionId,
        session_incarnation_id: uuid::Uuid,
        mode: SemanticSendMode,
        text: String,
    ) -> Result<SteerQueueEntry> {
        self.semantic_send_now(request_id, session_id, session_incarnation_id, mode, text)
    }

    pub(crate) fn semantic_send_now(
        &mut self,
        request_id: uuid::Uuid,
        session_id: SessionId,
        session_incarnation_id: uuid::Uuid,
        mode: SemanticSendMode,
        text: String,
    ) -> Result<SteerQueueEntry> {
        self.semantic_send_now_with_requester(
            request_id,
            session_id,
            session_incarnation_id,
            mode,
            text,
            None,
        )
    }

    pub(crate) fn semantic_send_now_from_relay(
        &mut self,
        request_id: uuid::Uuid,
        session_id: SessionId,
        session_incarnation_id: uuid::Uuid,
        mode: SemanticSendMode,
        text: String,
        requester_device_id: &str,
    ) -> Result<SteerQueueEntry> {
        self.semantic_send_now_with_requester(
            request_id,
            session_id,
            session_incarnation_id,
            mode,
            text,
            Some(requester_device_id),
        )
    }

    fn semantic_send_now_with_requester(
        &mut self,
        request_id: uuid::Uuid,
        session_id: SessionId,
        session_incarnation_id: uuid::Uuid,
        mode: SemanticSendMode,
        text: String,
        requester_device_id: Option<&str>,
    ) -> Result<SteerQueueEntry> {
        if request_id.get_version() != Some(uuid::Version::SortRand) {
            return Err(AppError::Unsupported {
                reason: "semantic send requestId must be UUIDv7".to_owned(),
            });
        }
        let account_user_id = semantic_scope(self.state.identity.auth.subject_string());
        if let Some(existing) = self.state.steering.existing_semantic(
            &account_user_id,
            request_id,
            session_id,
            session_incarnation_id,
            mode,
            &text,
        )? {
            let recorded_requester = self
                .state
                .steering
                .relay_requester_device(&account_user_id, &request_id.to_string());
            if recorded_requester != requester_device_id {
                return Err(AppError::Unsupported {
                    reason: "semantic send request ID was reused across requester origins"
                        .to_owned(),
                });
            }
            return match existing {
                SemanticSendAdmission::Pending(entry)
                    if entry.delivery_state == SteerDeliveryState::Preparing =>
                {
                    Ok(self
                        .state
                        .steering
                        .acknowledge_queued(&entry.steer_id)?
                        .unwrap_or(entry))
                }
                SemanticSendAdmission::Completed(entry)
                | SemanticSendAdmission::Tombstoned(entry)
                | SemanticSendAdmission::Pending(entry) => Ok(entry),
                SemanticSendAdmission::New(_) => unreachable!("existing lookup cannot create work"),
            };
        }
        let record = self
            .state
            .local
            .sessions
            .record(session_id)
            .ok_or(AppError::NoActiveSession)?;
        if record.local_incarnation_id != session_incarnation_id {
            return Err(AppError::Unsupported {
                reason: "semantic send targeted a stale session incarnation".to_owned(),
            });
        }
        if matches!(
            record.summary.state,
            kodosi_domain::session::SessionState::Stopping
                | kodosi_domain::session::SessionState::Stopped
                | kodosi_domain::session::SessionState::Failed
        ) {
            return Err(AppError::Unsupported {
                reason: "cannot send to a stopped session".to_owned(),
            });
        }

        self.state
            .steering
            .validate_new_semantic(session_id, &text)?;
        let entry = match requester_device_id {
            Some(device_id) => self.state.steering.admit_relay_semantic(
                account_user_id,
                request_id,
                session_id,
                session_incarnation_id,
                mode,
                text,
                device_id,
            )?,
            None => self.state.steering.admit_semantic(
                account_user_id,
                request_id,
                session_id,
                session_incarnation_id,
                mode,
                text,
            )?,
        };
        let entry = match entry {
            SemanticSendAdmission::Completed(entry)
            | SemanticSendAdmission::Tombstoned(entry)
            | SemanticSendAdmission::Pending(entry) => {
                return Ok(entry);
            }
            SemanticSendAdmission::New(entry) => entry,
        };
        let queued = self
            .state
            .steering
            .acknowledge_queued(&entry.steer_id)?
            .ok_or_else(|| AppError::Unsupported {
                reason: "durable semantic send could not become delivery-eligible".to_owned(),
            })?;
        self.publish_steer_transition(queued.clone(), SteerTransition::Queued, None);
        Ok(queued)
    }

    pub(crate) fn cancel_steer_with_origin(
        &mut self,
        session_id: SessionId,
        steer_id: &str,
        origin: SemanticSendOrigin,
    ) -> Result<SteerQueueEntry> {
        let account_user_id = semantic_scope(self.state.identity.auth.subject_string());
        let entry = self
            .state
            .steering
            .entries_for_account(&account_user_id, session_id)
            .into_iter()
            .find(|entry| {
                entry.steer_id == steer_id
                    && matches!(
                        entry.delivery_state,
                        SteerDeliveryState::Preparing | SteerDeliveryState::Queued
                    )
            })
            .ok_or(AppError::NotFound)?;
        let record = self
            .state
            .local
            .sessions
            .record(session_id)
            .ok_or(AppError::NoActiveSession)?;
        if record.local_incarnation_id.to_string() != entry.session_incarnation_id {
            return Err(AppError::Unsupported {
                reason: "semantic cancellation targeted a stale session incarnation".to_owned(),
            });
        }
        let entry = self
            .state
            .steering
            .cancel_semantic(&account_user_id, session_id, steer_id, origin)?
            .ok_or(AppError::NotFound)?;
        self.publish_steer_transition(entry.clone(), SteerTransition::Cancelled, None);
        Ok(entry)
    }

    pub(crate) fn cancel_steer(
        &mut self,
        session_id: SessionId,
        steer_id: &str,
    ) -> Result<SteerQueueEntry> {
        self.cancel_steer_with_origin(session_id, steer_id, SemanticSendOrigin::DirectLocal)
    }

    pub(crate) fn query_steers(
        &self,
        session_id: SessionId,
        request_id: Option<&str>,
    ) -> Vec<SteerQueueEntry> {
        let account_user_id = semantic_scope(self.state.identity.auth.subject_string());
        self.state
            .steering
            .query_semantic(&account_user_id, session_id, request_id)
    }

    pub(crate) fn cancel_session_steers(
        &mut self,
        session_id: SessionId,
    ) -> Result<SteerCleanupOutcome> {
        let pending = self.state.steering.entries(session_id);
        if pending.is_empty() {
            return Ok(SteerCleanupOutcome::default());
        }
        let cancelled = self.state.steering.cancel_session(session_id)?;
        let mut outcome = SteerCleanupOutcome::default();
        for entry in cancelled {
            outcome.cancelled += 1;
            if entry.delivery_state == SteerDeliveryState::DeliveryUnknown {
                self.publish_steer_transition(
                    entry,
                    SteerTransition::DeliveryUnknown,
                    Some("session ended while delivery remained unprovable".to_owned()),
                );
            } else {
                self.publish_steer_transition(entry, SteerTransition::Cancelled, None);
            }
        }
        Ok(outcome)
    }

    pub(crate) async fn inject_ready_steer(&mut self) -> Result<bool> {
        let account_user_id = semantic_scope(self.state.identity.auth.subject_string());
        let Some((entry, _tool_use_id)) = self.state.steering.claim_ready(&account_user_id)? else {
            return Ok(false);
        };
        self.publish_steer_transition(entry.clone(), SteerTransition::Sending, None);
        let delivery_result = if entry.mode == SemanticSendMode::StopAndSend {
            match self
                .state
                .local
                .owned_session_runtimes
                .runtime_handle(SessionId::parse_field(&entry.session_id, "sessionId")?)
            {
                Some(runtime) => {
                    runtime
                        .interrupt_then_write(format!("{}\r", entry.text).into_bytes())
                        .await
                }
                None => Err(AppError::NoActiveSession),
            }
        } else {
            let input = SessionInput::new(format!("{}\r", entry.text).into_bytes());
            self.send_confirmed_local_input(
                SessionId::parse_field(&entry.session_id, "sessionId")?,
                input,
            )
            .await
        };
        if let Err(error) = delivery_result {
            if entry.mode == SemanticSendMode::StopAndSend
                || matches!(error, AppError::DeliveryUnknown { .. })
            {
                let message = error.to_string();
                let origin = self.state.steering.origin_for(&entry);
                match self
                    .state
                    .steering
                    .settle_delivery_unknown(&entry, origin)
                {
                    Ok(_) => self.publish_steer_transition(
                        entry,
                        SteerTransition::DeliveryUnknown,
                        Some(message),
                    ),
                    Err(settle_error) => self.state.record_log(format!(
                        "irreversible Stop & Send {} could not persist terminal deliveryUnknown after {message}: {settle_error}",
                        entry.steer_id
                    )),
                }
            } else if let Some(requeued) = self.state.steering.requeue(&entry.steer_id)? {
                self.publish_steer_transition(
                    requeued,
                    SteerTransition::Failed,
                    Some(error.to_string()),
                );
            }
            return Err(error);
        }
        let origin = self.state.steering.origin_for(&entry);
        self.state
            .steering
            .complete_semantic(&entry, SteerDeliveryState::Injected, origin)?;
        self.publish_steer_transition(entry, SteerTransition::Injected, None);
        Ok(true)
    }

    pub(crate) fn resolve_pending_steers_for_ended_session(&mut self, session_id: SessionId) {
        match self.cancel_session_steers(session_id) {
            Ok(_) => {}
            Err(error) => {
                self.state.record_log(format!(
                    "{} pending steer cleanup failed: {error}",
                    session_id.short()
                ));
            }
        }
    }

    pub(crate) fn drain_owner_action_results(&mut self) {
        let Some(origin) = self.state.identity.current_account_event_origin() else {
            return;
        };
        let session_ids = self.state.local.sessions.ids().to_vec();
        for session_id in session_ids {
            if !self.state.sharing.host_relays.active(session_id) {
                continue;
            }
            let generation = self.state.sharing.host_relays.generation(session_id);
            for result in self
                .owner_action_results
                .pending_for(&origin.account_user_id, session_id)
            {
                let key = (
                    session_id,
                    result.action_id.clone(),
                    result.requester_user_id.clone(),
                    generation,
                );
                if !self.action_results_in_flight.insert(key.clone()) {
                    continue;
                }
                let Some(accepted) = result.accepted else {
                    self.action_results_in_flight.remove(&key);
                    continue;
                };
                let (delivery, delivered) = tokio::sync::oneshot::channel();
                let wire = kodosi_backend_client::relay::HostRelayActionResultDelivery {
                    result: kodosi_backend_client::relay::HostRelayActionResult {
                        incarnation_id: result.incarnation_id,
                        action_id: result.action_id.clone(),
                        request_id: result.request_id.clone(),
                        request_generation: result.request_generation,
                        requester_user_id: result.requester_user_id.clone(),
                        requester_device_id: result.requester_device_id.clone(),
                        accepted,
                    },
                    delivery,
                };
                if self
                    .state
                    .sharing
                    .host_relays
                    .send_action_result(session_id, wire)
                    .is_err()
                {
                    self.action_results_in_flight.remove(&key);
                    continue;
                }
                let events = self.session_events_tx.clone();
                let event_origin = crate::session_runtime::events::HostRelayEventOrigin {
                    account_origin: origin.clone(),
                    session_id,
                    relay_generation: generation,
                };
                tokio::spawn(async move {
                    let delivered = delivered.await.unwrap_or(false);
                    drop(
                        events
                            .send(crate::session_runtime::events::RuntimeSessionEvent::HostActionResultMailboxAck {
                                origin: event_origin,
                                result,
                                delivered,
                            })
                            .await,
                    );
                });
            }
        }
    }

    pub(crate) fn resend_semantic_relay_receipt(
        &mut self,
        account_user_id: &str,
        request_id: uuid::Uuid,
        session_id: SessionId,
        incarnation_id: uuid::Uuid,
        mode: SemanticSendMode,
        payload_sha256: &str,
        requester_device_id: &str,
    ) -> bool {
        let Some(receipt) = self.state.steering.relay_receipt_for_exact_retry(
            account_user_id,
            &request_id.to_string(),
            requester_device_id,
        ) else {
            return false;
        };
        if receipt.session_id != session_id.to_string()
            || receipt.session_incarnation_id != incarnation_id.to_string()
            || receipt.mode != mode
            || receipt.payload_sha256 != payload_sha256
        {
            return false;
        }
        self.send_semantic_relay_receipt(&receipt, requester_device_id)
    }

    fn send_semantic_relay_receipt(
        &mut self,
        receipt: &SemanticSendReceipt,
        requester_device_id: &str,
    ) -> bool {
        let Ok(session_id) = SessionId::parse_field(&receipt.session_id, "sessionId") else {
            return false;
        };
        if !self.state.sharing.host_relays.active(session_id) {
            return false;
        }
        let Ok(request_id) = uuid::Uuid::parse_str(&receipt.request_id) else {
            return false;
        };
        let Ok(incarnation_id) = uuid::Uuid::parse_str(&receipt.session_incarnation_id) else {
            return false;
        };
        let Some(outcome) = relay_outcome(receipt.outcome) else {
            return false;
        };
        let Some(owner_device_id) = self
            .state
            .sharing
            .shared_sessions
            .get(session_id)
            .and_then(|shared| shared.host_device_id())
            .map(str::to_owned)
        else {
            return false;
        };
        let generation = self.state.sharing.host_relays.generation(session_id);
        let key = (session_id, request_id, generation);
        if !self.semantic_receipts_in_flight.insert(key) {
            return true;
        }
        let (delivery, delivered) = tokio::sync::oneshot::channel();
        let wire = kodosi_backend_client::relay::HostRelaySemanticReceiptDelivery {
            receipt: kodosi_backend_client::relay::HostRelaySemanticReceipt {
                request_id,
                incarnation_id,
                mode: relay_mode(receipt.mode),
                payload_sha256: receipt.payload_sha256.clone(),
                outcome,
                requester_user_id: receipt.account_user_id.clone(),
                requester_device_id: requester_device_id.to_owned(),
                owner_user_id: receipt.account_user_id.clone(),
                owner_device_id,
                signature: None,
            },
            delivery,
        };
        if self
            .state
            .sharing
            .host_relays
            .send_semantic_receipt(session_id, wire)
            .is_err()
        {
            self.semantic_receipts_in_flight.remove(&key);
            return false;
        }
        let Some(account_origin) = self.state.identity.current_account_event_origin() else {
            self.semantic_receipts_in_flight.remove(&key);
            return false;
        };
        let acknowledge_locally = !receipt.relay_acknowledged;
        let events = self.session_events_tx.clone();
        let event_origin = crate::session_runtime::events::HostRelayEventOrigin {
            account_origin,
            session_id,
            relay_generation: generation,
        };
        tokio::spawn(async move {
            let delivered = delivered.await.unwrap_or(false);
            drop(
                events
                    .send(crate::session_runtime::events::RuntimeSessionEvent::HostSemanticReceiptMailboxAck {
                        origin: event_origin,
                        request_id,
                        incarnation_id,
                        delivered,
                        acknowledge_locally,
                    })
                    .await,
            );
        });
        true
    }

    pub(crate) fn drain_semantic_relay_receipts(&mut self) {
        let Some(account_user_id) = self.state.identity.auth.subject_string() else {
            return;
        };
        let session_ids = self.state.local.sessions.ids().to_vec();
        for session_id in session_ids {
            let receipts = self
                .state
                .steering
                .pending_relay_receipts(&account_user_id, session_id);
            for receipt in receipts {
                let Some(requester_device_id) = self
                    .state
                    .steering
                    .relay_requester_device(&receipt.account_user_id, &receipt.request_id)
                    .map(str::to_owned)
                else {
                    continue;
                };
                let _ = self.send_semantic_relay_receipt(&receipt, &requester_device_id);
            }
        }
    }

    fn publish_steer_transition(
        &mut self,
        entry: SteerQueueEntry,
        transition: SteerTransition,
        message: Option<String>,
    ) {
        self.state
            .runtime_outbox
            .queue_agent_intel(crate::AgentIntelEvent::SteerState {
                entry,
                transition,
                message,
            });
    }
}

#[cfg(test)]
mod tests;
