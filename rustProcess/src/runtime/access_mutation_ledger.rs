use std::{
    collections::BTreeMap,
    fs,
    io::Read as _,
    path::{Path, PathBuf},
};

use serde::{
    Deserialize, Deserializer, Serialize,
    de::{Error as _, MapAccess, Visitor},
};
use uuid::Uuid;

use super::access_mutations::{
    MAX_TERMINAL_MESSAGE_BYTES, PreparedSessionAccessMutation, PreparedSessionAccessMutationState,
    SessionAccessMutationTerminal, SessionAccessMutationTerminalStatus,
};
use crate::{
    AppError, Result,
    support::storage::atomic_file::{
        AtomicWriteFailure, FileMode, atomic_write_json_commit_aware, retry_sync_parent,
    },
};

const FILE_VERSION: u32 = 1;
const MAX_ENTRIES: usize = 256;
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionAccessMutationLedgerFile {
    version: u32,
    #[serde(deserialize_with = "deserialize_accounts")]
    accounts: BTreeMap<String, BTreeMap<Uuid, PreparedSessionAccessMutation>>,
}

#[derive(Debug)]
pub(crate) struct SessionAccessMutationLedger {
    path: PathBuf,
    file: SessionAccessMutationLedgerFile,
    unavailable_reason: Option<String>,
}

impl SessionAccessMutationLedger {
    pub(crate) fn load_default() -> Result<Self> {
        let path = crate::support::storage::paths::data_root()?
            .join("pending-session-access-mutations.json");
        Self::load(path)
    }

    #[allow(clippy::unnecessary_wraps)]
    fn load(path: PathBuf) -> Result<Self> {
        match Self::read_file(&path) {
            Ok(Some(file)) => Ok(Self {
                path,
                file,
                unavailable_reason: None,
            }),
            Ok(None) | Err(AppError::InvalidBackendData { .. }) => Ok(Self {
                path,
                file: empty_file(),
                unavailable_reason: None,
            }),
            Err(error) => Ok(Self::unavailable(path, error.to_string())),
        }
    }

    fn unavailable(path: PathBuf, reason: String) -> Self {
        Self {
            path,
            file: empty_file(),
            unavailable_reason: Some(reason),
        }
    }

    fn read_file(path: &Path) -> Result<Option<SessionAccessMutationLedgerFile>> {
        let parent = path.parent().ok_or_else(|| AppError::Unsupported {
            reason: "session access mutation ledger path has no parent directory".to_owned(),
        })?;
        let parent_before = fs::metadata(parent).map_err(AppError::Io)?;
        let path_metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let parent_after = fs::metadata(parent).map_err(AppError::Io)?;
                if same_directory_observation(&parent_before, &parent_after) {
                    return Ok(None);
                }
                return Err(unstable_ledger(
                    "ledger disappeared while inspecting its parent",
                ));
            }
            Err(error) => return Err(AppError::Io(error)),
        };
        if path_metadata.file_type().is_symlink() || !path_metadata.is_file() {
            return Err(unstable_ledger("ledger path is not a regular file"));
        }
        let mut source = super::share_transition_ledger_file::open_read_only(path)?;
        let opened_before = source.metadata().map_err(AppError::Io)?;
        if !opened_before.is_file() || !same_file(&path_metadata, &opened_before) {
            return Err(unstable_ledger("ledger changed while opening"));
        }
        let current_metadata = fs::symlink_metadata(path).map_err(AppError::Io)?;
        if !stable_file_observation(&opened_before, &current_metadata) {
            return Err(unstable_ledger("ledger changed before reading"));
        }
        let mut bytes = Vec::new();
        source
            .by_ref()
            .take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(AppError::Io)?;
        let opened_after = source.metadata().map_err(AppError::Io)?;
        if !stable_file_observation(&opened_before, &opened_after) {
            return Err(unstable_ledger("ledger changed while reading"));
        }
        let current_metadata = fs::symlink_metadata(path).map_err(AppError::Io)?;
        if !stable_file_observation(&opened_after, &current_metadata) {
            return Err(unstable_ledger("ledger changed after reading"));
        }
        if opened_after.len() > MAX_FILE_BYTES {
            return Err(invalid_ledger(format!(
                "ledger exceeds {MAX_FILE_BYTES} bytes"
            )));
        }
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) != opened_after.len() {
            return Err(unstable_ledger("ledger changed while reading"));
        }
        let file: SessionAccessMutationLedgerFile =
            serde_json::from_slice(&bytes).map_err(|error| {
                invalid_ledger(format!("malformed durable mutation ledger: {error}"))
            })?;
        validate_file(&file)?;
        if !serialized_size_available(&file)? || !progression_envelope_available(&file)? {
            return Err(invalid_ledger(format!(
                "canonical ledger or progression envelope exceeds {MAX_FILE_BYTES} bytes"
            )));
        }
        Ok(Some(file))
    }

    pub(crate) fn load_at(path: PathBuf) -> Result<Self> {
        Self::load(path)
    }

    pub(crate) fn ensure_available(&self) -> Result<()> {
        if let Some(reason) = &self.unavailable_reason {
            return Err(AppError::Unsupported {
                reason: format!(
                    "session access mutation ledger is unavailable at {}: {reason}",
                    self.path.display()
                ),
            });
        }
        Ok(())
    }

    pub(crate) fn entries(&self, account: &str) -> Result<Vec<PreparedSessionAccessMutation>> {
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
        id: Uuid,
    ) -> Result<Option<&PreparedSessionAccessMutation>> {
        self.ensure_available()?;
        Ok(self
            .file
            .accounts
            .get(account)
            .and_then(|entries| entries.get(&id)))
    }

    pub(crate) fn put(&mut self, entry: PreparedSessionAccessMutation) -> Result<()> {
        self.ensure_available()?;
        entry.validate()?;
        let count = self
            .file
            .accounts
            .values()
            .map(BTreeMap::len)
            .sum::<usize>();
        let exists = self
            .get(&entry.account_user_id, entry.mutation_id)?
            .is_some();
        if !exists && count >= MAX_ENTRIES {
            return Err(AppError::Unsupported {
                reason: "pending session access mutation ledger is full".to_owned(),
            });
        }
        let mut candidate = self.file.clone();
        candidate
            .accounts
            .entry(entry.account_user_id.clone())
            .or_default()
            .insert(entry.mutation_id, entry);
        self.persist_candidate(candidate)
    }

    pub(crate) fn remove(
        &mut self,
        account: &str,
        id: Uuid,
    ) -> Result<Option<PreparedSessionAccessMutation>> {
        self.ensure_available()?;
        let mut candidate = self.file.clone();
        let removed = candidate
            .accounts
            .get_mut(account)
            .and_then(|entries| entries.remove(&id));
        if candidate
            .accounts
            .get(account)
            .is_some_and(BTreeMap::is_empty)
        {
            candidate.accounts.remove(account);
        }
        self.persist_candidate(candidate)?;
        Ok(removed)
    }

    fn persist_candidate(&mut self, candidate: SessionAccessMutationLedgerFile) -> Result<()> {
        ensure_serialized_size(&candidate)?;
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
                self.disable_after_ambiguous_persist(&error, &candidate)
            }
        }
    }

    fn disable_after_ambiguous_persist(
        &mut self,
        error: &AppError,
        candidate: &SessionAccessMutationLedgerFile,
    ) -> Result<()> {
        let reason = format!(
            "session access mutation ledger durability is uncertain after atomic replacement: {error}"
        );
        match Self::read_file(&self.path) {
            Ok(Some(file)) => {
                self.file = file;
                let observation = if self.file == *candidate {
                    "candidate is visible"
                } else {
                    "unexpected prior or third state is visible"
                };
                self.unavailable_reason = Some(format!("{reason}; {observation}"));
            }
            Ok(None) => {
                self.file = empty_file();
                self.unavailable_reason = Some(format!("{reason}; destination is missing"));
            }
            Err(reload_error) => {
                self.file = empty_file();
                self.unavailable_reason = Some(format!(
                    "{reason}; visible destination could not be reloaded: {reload_error}"
                ));
            }
        }
        Err(AppError::Unsupported {
            reason: self.unavailable_reason.clone().unwrap_or(reason),
        })
    }
}

fn stable_file_observation(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;

    same_file(left, right)
        && left.is_file()
        && right.is_file()
        && left.len() == right.len()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
}

fn same_directory_observation(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    same_file(left, right)
        && left.is_dir()
        && right.is_dir()
        && left.modified().ok() == right.modified().ok()
        && left.len() == right.len()
}

fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    left.dev() == right.dev() && left.ino() == right.ino()
}

fn deserialize_accounts<'de, D>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, BTreeMap<Uuid, PreparedSessionAccessMutation>>, D::Error>
where
    D: Deserializer<'de>,
{
    struct AccountsVisitor;

    impl<'de> Visitor<'de> for AccountsVisitor {
        type Value = BTreeMap<String, BTreeMap<Uuid, PreparedSessionAccessMutation>>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an account mutation map without duplicate keys")
        }

        fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let mut accounts = BTreeMap::new();
            while let Some(account) = map.next_key::<String>()? {
                if accounts.contains_key(&account) {
                    return Err(A::Error::custom(format!(
                        "duplicate account key `{account}`"
                    )));
                }
                let entries = map.next_value_seed(MutationMapSeed)?;
                accounts.insert(account, entries);
            }
            Ok(accounts)
        }
    }

    deserializer.deserialize_map(AccountsVisitor)
}

struct MutationMapSeed;

impl<'de> serde::de::DeserializeSeed<'de> for MutationMapSeed {
    type Value = BTreeMap<Uuid, PreparedSessionAccessMutation>;

    fn deserialize<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MutationMapVisitor;

        impl<'de> Visitor<'de> for MutationMapVisitor {
            type Value = BTreeMap<Uuid, PreparedSessionAccessMutation>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a mutation map without duplicate keys")
            }

            fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut entries = BTreeMap::new();
                while let Some(mutation_id) = map.next_key::<Uuid>()? {
                    if entries.contains_key(&mutation_id) {
                        return Err(A::Error::custom(format!(
                            "duplicate mutation key `{mutation_id}`"
                        )));
                    }
                    entries.insert(mutation_id, map.next_value()?);
                }
                Ok(entries)
            }
        }

        deserializer.deserialize_map(MutationMapVisitor)
    }
}

fn empty_file() -> SessionAccessMutationLedgerFile {
    SessionAccessMutationLedgerFile {
        version: FILE_VERSION,
        accounts: BTreeMap::new(),
    }
}

fn validate_file(file: &SessionAccessMutationLedgerFile) -> Result<()> {
    if file.version != FILE_VERSION {
        return Err(invalid_ledger(format!(
            "unsupported ledger version {}",
            file.version
        )));
    }
    let count = file.accounts.values().map(BTreeMap::len).sum::<usize>();
    if count > MAX_ENTRIES {
        return Err(invalid_ledger(format!(
            "ledger exceeds {MAX_ENTRIES} entries"
        )));
    }
    for (account, entries) in &file.accounts {
        if account.trim().is_empty() || account.len() > 1_024 {
            return Err(invalid_ledger(
                "account map key must be non-empty and bounded",
            ));
        }
        for (mutation_id, entry) in entries {
            if entry.account_user_id != *account || entry.mutation_id != *mutation_id {
                return Err(invalid_ledger(
                    "map keys do not match the retained mutation identity",
                ));
            }
            entry
                .validate()
                .map_err(|error| invalid_ledger(error.to_string()))?;
        }
    }
    Ok(())
}

fn canonical_bytes(file: &SessionAccessMutationLedgerFile) -> Result<Vec<u8>> {
    serde_json::to_vec_pretty(file).map_err(AppError::Json)
}

fn serialized_size_available(file: &SessionAccessMutationLedgerFile) -> Result<bool> {
    Ok(u64::try_from(canonical_bytes(file)?.len()).unwrap_or(u64::MAX) <= MAX_FILE_BYTES)
}

fn progression_envelope(file: &SessionAccessMutationLedgerFile) -> SessionAccessMutationLedgerFile {
    let mut projected = file.clone();
    for entry in projected
        .accounts
        .values_mut()
        .flat_map(BTreeMap::values_mut)
    {
        if !matches!(entry.state, PreparedSessionAccessMutationState::Terminal(_)) {
            entry.state = largest_future_state();
        }
    }
    projected
}

fn progression_envelope_available(file: &SessionAccessMutationLedgerFile) -> Result<bool> {
    serialized_size_available(&progression_envelope(file))
}

fn largest_future_state() -> PreparedSessionAccessMutationState {
    PreparedSessionAccessMutationState::Terminal(SessionAccessMutationTerminal::new(
        SessionAccessMutationTerminalStatus::Rejected,
        Some("\0".repeat(MAX_TERMINAL_MESSAGE_BYTES)),
    ))
}

fn ensure_serialized_size(file: &SessionAccessMutationLedgerFile) -> Result<()> {
    if !serialized_size_available(file)? || !progression_envelope_available(file)? {
        return Err(AppError::Unsupported {
            reason: format!(
                "pending session access mutation ledger exceeds {MAX_FILE_BYTES} bytes"
            ),
        });
    }
    Ok(())
}

fn unstable_ledger(reason: impl Into<String>) -> AppError {
    AppError::Unsupported {
        reason: reason.into(),
    }
}

fn invalid_ledger(reason: impl Into<String>) -> AppError {
    AppError::InvalidBackendData {
        field: "pendingSessionAccessMutations".to_owned(),
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::access_mutations::{
        PreparedSessionAccessMutation, PreparedSessionAccessMutationState,
        SessionAccessMutationTarget, SessionAccessMutationTerminal,
        SessionAccessMutationTerminalStatus,
    };
    use kodosi_domain::ids::SessionId;

    fn entry(account: &str) -> PreparedSessionAccessMutation {
        let session_id =
            SessionId::parse_field("01900000-0000-7000-8000-000000000002", "sessionId").unwrap();
        let incarnation_id = Uuid::parse_str("01900000-0000-7000-8000-000000000003").unwrap();
        PreparedSessionAccessMutation::new(
            Uuid::parse_str("01900000-0000-7000-8000-000000000001").unwrap(),
            account.to_owned(),
            3,
            session_id,
            incarnation_id,
            session_id.to_string(),
            incarnation_id,
            SessionAccessMutationTarget::Leave,
        )
        .unwrap()
    }

    fn entry_with_id(account: &str, sequence: u64) -> PreparedSessionAccessMutation {
        let mut mutation = entry(account);
        mutation.mutation_id =
            Uuid::parse_str(&format!("01900000-0000-7000-8000-{sequence:012x}")).unwrap();
        mutation
    }

    #[test]
    fn ledger_survives_restart_and_is_account_scoped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let mut ledger = SessionAccessMutationLedger::load_at(path.clone()).unwrap();
        ledger.put(entry("alice")).unwrap();
        drop(ledger);
        let ledger = SessionAccessMutationLedger::load_at(path).unwrap();
        assert_eq!(ledger.entries("alice").unwrap().len(), 1);
        assert!(ledger.entries("bob").unwrap().is_empty());
    }

    fn future_states() -> Vec<PreparedSessionAccessMutationState> {
        vec![
            PreparedSessionAccessMutationState::Prepared,
            PreparedSessionAccessMutationState::Attempting,
            PreparedSessionAccessMutationState::OutcomeUnknown,
            PreparedSessionAccessMutationState::Retiring,
            PreparedSessionAccessMutationState::ReceiptConfirmed,
            PreparedSessionAccessMutationState::EffectPending,
            PreparedSessionAccessMutationState::RelayPending {
                key_generation: u32::MAX,
            },
            PreparedSessionAccessMutationState::Terminal(SessionAccessMutationTerminal::new(
                SessionAccessMutationTerminalStatus::Applied,
                Some("\0".repeat(MAX_TERMINAL_MESSAGE_BYTES)),
            )),
            largest_future_state(),
        ]
    }

    #[test]
    fn progression_projection_is_at_least_every_reachable_state() {
        let mut projected = empty_file();
        let account = "alice";
        let mut mutation = entry(account);
        mutation.state = largest_future_state();
        projected
            .accounts
            .entry(account.to_owned())
            .or_default()
            .insert(mutation.mutation_id, mutation);
        let projected_size = canonical_bytes(&projected).unwrap().len();

        for state in future_states() {
            let mut candidate = projected.clone();
            candidate
                .accounts
                .get_mut(account)
                .unwrap()
                .values_mut()
                .next()
                .unwrap()
                .state = state;
            assert!(canonical_bytes(&candidate).unwrap().len() <= projected_size);
        }
    }

    #[test]
    fn relay_pending_generation_survives_restart_and_rejects_zero() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let mut pending = entry("alice");
        pending.state = PreparedSessionAccessMutationState::RelayPending { key_generation: 7 };
        let mutation_id = pending.mutation_id;
        let mut ledger = SessionAccessMutationLedger::load_at(path.clone()).unwrap();
        ledger.put(pending).unwrap();
        drop(ledger);

        let restarted = SessionAccessMutationLedger::load_at(path).unwrap();
        assert!(matches!(
            restarted.get("alice", mutation_id).unwrap().unwrap().state,
            PreparedSessionAccessMutationState::RelayPending { key_generation: 7 }
        ));

        let mut invalid = entry("bob");
        invalid.state = PreparedSessionAccessMutationState::RelayPending { key_generation: 0 };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn admitted_mutation_can_persist_every_future_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let mut ledger = SessionAccessMutationLedger::load_at(path.clone()).unwrap();
        let mut pending = entry("alice");
        pending.state = PreparedSessionAccessMutationState::Attempting;
        ledger.put(pending.clone()).unwrap();

        for state in future_states() {
            pending.state = state;
            ledger.put(pending.clone()).unwrap();
        }

        drop(ledger);
        let restarted = SessionAccessMutationLedger::load_at(path).unwrap();
        assert!(matches!(
            restarted
                .get("alice", pending.mutation_id)
                .unwrap()
                .unwrap()
                .state,
            PreparedSessionAccessMutationState::Terminal(_)
        ));
    }

    #[test]
    fn terminal_outcome_survives_until_exact_acknowledgment() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let mut terminal = entry("alice");
        terminal.state =
            PreparedSessionAccessMutationState::Terminal(SessionAccessMutationTerminal {
                status: SessionAccessMutationTerminalStatus::Applied,
                message: None,
            });
        let id = terminal.mutation_id;
        let fingerprint = terminal.fingerprint.clone();
        let mut ledger = SessionAccessMutationLedger::load_at(path.clone()).unwrap();
        ledger.put(terminal.clone()).unwrap();
        drop(ledger);

        let mut restarted = SessionAccessMutationLedger::load_at(path.clone()).unwrap();
        assert_ne!(
            restarted.get("alice", id).unwrap().unwrap().fingerprint,
            "wrong"
        );
        assert_eq!(
            restarted.get("alice", id).unwrap().unwrap().fingerprint,
            fingerprint
        );
        assert_eq!(restarted.remove("alice", id).unwrap(), Some(terminal));
        drop(restarted);
        assert!(
            SessionAccessMutationLedger::load_at(path)
                .unwrap()
                .get("alice", id)
                .unwrap()
                .is_none()
        );
    }

    fn envelope_boundary_file(
        terminal_message_bytes: usize,
        pending_state: PreparedSessionAccessMutationState,
    ) -> SessionAccessMutationLedgerFile {
        let account = "alice";
        let mut file = empty_file();
        let entries = file.accounts.entry(account.to_owned()).or_default();
        for sequence in 1..MAX_ENTRIES {
            let mut mutation = entry_with_id(account, u64::try_from(sequence).unwrap());
            mutation.state =
                PreparedSessionAccessMutationState::Terminal(SessionAccessMutationTerminal::new(
                    SessionAccessMutationTerminalStatus::Rejected,
                    Some("x".repeat(terminal_message_bytes)),
                ));
            entries.insert(mutation.mutation_id, mutation);
        }
        let mut pending = entry_with_id(account, u64::try_from(MAX_ENTRIES).unwrap());
        pending.state = pending_state;
        entries.insert(pending.mutation_id, pending);
        file
    }

    fn largest_fitting_terminal_message_bytes(
        pending_state: PreparedSessionAccessMutationState,
        require_progression_envelope: bool,
    ) -> usize {
        let mut low = 0;
        let mut high = MAX_TERMINAL_MESSAGE_BYTES + 1;
        while low + 1 < high {
            let midpoint = low + (high - low) / 2;
            let file = envelope_boundary_file(midpoint, pending_state.clone());
            let fits = if require_progression_envelope {
                progression_envelope_available(&file).unwrap()
            } else {
                serialized_size_available(&file).unwrap()
            };
            if fits {
                low = midpoint;
            } else {
                high = midpoint;
            }
        }
        low
    }

    fn near_capacity_terminal_entries(message_bytes: usize) -> SessionAccessMutationLedgerFile {
        let mut file = envelope_boundary_file(
            message_bytes,
            PreparedSessionAccessMutationState::Attempting,
        );
        file.accounts
            .get_mut("alice")
            .unwrap()
            .remove(&entry_with_id("alice", u64::try_from(MAX_ENTRIES).unwrap()).mutation_id);
        file
    }

    #[test]
    fn admission_rejects_candidate_without_progression_capacity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let message_bytes = largest_fitting_terminal_message_bytes(
            PreparedSessionAccessMutationState::Attempting,
            false,
        );
        let file = near_capacity_terminal_entries(message_bytes);
        let mut ledger = SessionAccessMutationLedger {
            path: path.clone(),
            file,
            unavailable_reason: None,
        };
        ensure_serialized_size(&ledger.file).unwrap();
        ledger.persist_candidate(ledger.file.clone()).unwrap();
        let original = fs::read(&path).unwrap();
        let mut pending = entry_with_id("alice", u64::try_from(MAX_ENTRIES).unwrap());
        pending.state = PreparedSessionAccessMutationState::Attempting;
        let mut current_candidate = ledger.file.clone();
        current_candidate
            .accounts
            .get_mut("alice")
            .unwrap()
            .insert(pending.mutation_id, pending.clone());
        assert!(serialized_size_available(&current_candidate).unwrap());
        assert!(!progression_envelope_available(&current_candidate).unwrap());

        assert!(ledger.put(pending).is_err());
        assert_eq!(fs::read(path).unwrap(), original);
    }

    #[test]
    fn just_under_envelope_admission_completes_all_transitions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let message_bytes = largest_fitting_terminal_message_bytes(
            PreparedSessionAccessMutationState::Attempting,
            true,
        );
        let mut pending = entry_with_id("alice", u64::try_from(MAX_ENTRIES).unwrap());
        pending.state = PreparedSessionAccessMutationState::Attempting;
        let mut ledger = SessionAccessMutationLedger {
            path,
            file: near_capacity_terminal_entries(message_bytes),
            unavailable_reason: None,
        };

        ledger.put(pending.clone()).unwrap();
        for state in future_states() {
            pending.state = state;
            ledger.put(pending.clone()).unwrap();
        }
    }

    #[test]
    fn load_resets_current_schema_ledger_without_progression_capacity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let file = envelope_boundary_file(
            largest_fitting_terminal_message_bytes(
                PreparedSessionAccessMutationState::Attempting,
                false,
            ),
            PreparedSessionAccessMutationState::Attempting,
        );
        let bytes = canonical_bytes(&file).unwrap();
        assert!(bytes.len() <= usize::try_from(MAX_FILE_BYTES).unwrap());
        assert!(!progression_envelope_available(&file).unwrap());
        fs::write(&path, &bytes).unwrap();

        let ledger = SessionAccessMutationLedger::load_at(path.clone()).unwrap();
        ledger.ensure_available().unwrap();
        assert!(ledger.entries("alice").unwrap().is_empty());
        assert_eq!(fs::read(path).unwrap(), bytes);
    }

    #[test]
    fn writer_refuses_oversized_candidate_without_replacing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let mut ledger = SessionAccessMutationLedger::load_at(path.clone()).unwrap();
        ledger.put(entry("alice")).unwrap();
        let original = fs::read(&path).unwrap();
        let mut oversized = entry("bob");
        oversized.state =
            PreparedSessionAccessMutationState::Terminal(SessionAccessMutationTerminal {
                status: SessionAccessMutationTerminalStatus::Rejected,
                message: Some("x".repeat(usize::try_from(MAX_FILE_BYTES).unwrap())),
            });
        assert!(ledger.put(oversized).is_err());
        assert_eq!(fs::read(path).unwrap(), original);
    }

    #[test]
    fn canonical_oversized_compact_input_is_logically_reset_without_rewriting_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let account = "alice";
        let mut file = empty_file();
        let entries = file.accounts.entry(account.to_owned()).or_default();
        for sequence in 1..=MAX_ENTRIES {
            let mut mutation = entry_with_id(account, u64::try_from(sequence).unwrap());
            mutation.state =
                PreparedSessionAccessMutationState::Terminal(SessionAccessMutationTerminal {
                    status: SessionAccessMutationTerminalStatus::Rejected,
                    message: Some("x".repeat(16 * 1_024)),
                });
            entries.insert(mutation.mutation_id, mutation);
        }
        while serde_json::to_vec(&file).unwrap().len() > usize::try_from(MAX_FILE_BYTES).unwrap() {
            for mutation in file.accounts.get_mut(account).unwrap().values_mut() {
                let PreparedSessionAccessMutationState::Terminal(terminal) = &mut mutation.state
                else {
                    unreachable!();
                };
                let message = terminal.message.as_mut().unwrap();
                message.truncate(message.len() - 64);
            }
        }
        let serialized = serde_json::to_vec(&file).unwrap();
        assert!(serialized.len() <= usize::try_from(MAX_FILE_BYTES).unwrap());
        assert!(
            serde_json::to_vec_pretty(&file).unwrap().len()
                > usize::try_from(MAX_FILE_BYTES).unwrap()
        );
        fs::write(&path, &serialized).unwrap();

        let ledger = SessionAccessMutationLedger::load_at(path.clone()).unwrap();
        ledger.ensure_available().unwrap();
        assert!(ledger.entries(account).unwrap().is_empty());
        assert_eq!(fs::read(path).unwrap(), serialized);
    }

    #[test]
    fn missing_parent_is_unavailable_instead_of_treated_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing").join("ledger.json");
        let ledger = SessionAccessMutationLedger::load_at(path).unwrap();
        assert!(ledger.ensure_available().is_err());
    }

    #[test]
    fn unreadable_regular_file_is_preserved_and_fences_access() {
        use std::os::unix::fs::PermissionsExt as _;

        if rustix::process::geteuid().is_root() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let bytes = serde_json::to_vec(&serde_json::json!({
            "version": FILE_VERSION,
            "accounts": { "alice": {} }
        }))
        .unwrap();
        fs::write(&path, &bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();

        let mut ledger = SessionAccessMutationLedger::load_at(path.clone()).unwrap();
        assert!(ledger.ensure_available().is_err());
        assert!(ledger.entries("alice").is_err());
        assert!(ledger.put(entry("alice")).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(fs::read(path).unwrap(), bytes);
    }

    #[test]
    fn symlink_is_preserved_and_fences_access() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let referent = dir.path().join("referent.json");
        let path = dir.path().join("ledger.json");
        fs::write(&referent, serde_json::to_vec(&empty_file()).unwrap()).unwrap();
        let original = fs::read(&referent).unwrap();
        symlink(&referent, &path).unwrap();
        let ledger = SessionAccessMutationLedger::load_at(path.clone()).unwrap();
        assert!(ledger.ensure_available().is_err());
        assert!(
            fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(referent).unwrap(), original);
    }

    #[test]
    fn dangling_symlink_is_preserved_and_fences_access() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let target = dir.path().join("missing.json");
        symlink(&target, &path).unwrap();
        let ledger = SessionAccessMutationLedger::load_at(path.clone()).unwrap();
        assert!(ledger.ensure_available().is_err());
        assert!(fs::symlink_metadata(path).unwrap().file_type().is_symlink());
        assert!(!target.exists());
    }

    #[test]
    fn fifo_is_preserved_without_opening_and_fences_access() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let status = std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        let ledger = SessionAccessMutationLedger::load_at(path.clone()).unwrap();
        assert!(ledger.ensure_available().is_err());
        assert!(!fs::metadata(path).unwrap().is_file());
    }

    #[test]
    fn character_device_is_rejected_without_blocking() {
        let path = PathBuf::from("/dev/null");
        let metadata = fs::symlink_metadata(&path).unwrap();
        assert!(!metadata.is_file());
        assert!(matches!(
            SessionAccessMutationLedger::read_file(&path),
            Err(AppError::Unsupported { .. })
        ));
    }

    #[test]
    fn directory_preserves_path_and_fences_access() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        fs::create_dir(&path).unwrap();
        let ledger = SessionAccessMutationLedger::load_at(path.clone()).unwrap();
        assert!(ledger.ensure_available().is_err());
        assert!(fs::metadata(path).unwrap().is_dir());
    }

    #[test]
    fn malformed_json_is_logically_reset_without_rewriting_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let bytes = b"not-json";
        fs::write(&path, bytes).unwrap();
        let ledger = SessionAccessMutationLedger::load_at(path.clone()).unwrap();
        ledger.ensure_available().unwrap();
        assert!(ledger.entries("alice").unwrap().is_empty());
        assert_eq!(fs::read(path).unwrap(), bytes);
    }

    #[test]
    fn malformed_file_remains_logically_reset_when_parent_is_read_only() {
        use std::os::unix::fs::PermissionsExt as _;

        if rustix::process::geteuid().is_root() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let bytes = b"not-json";
        fs::write(&path, bytes).unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();

        let ledger = SessionAccessMutationLedger::load_at(path.clone()).unwrap();
        ledger.ensure_available().unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(fs::read(path).unwrap(), bytes);
    }

    #[test]
    fn duplicate_account_key_resets_entire_ledger() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let bytes = br#"{"version":1,"accounts":{"alice":{},"alice":{}}}"#;
        fs::write(&path, bytes).unwrap();
        let ledger = SessionAccessMutationLedger::load_at(path.clone()).unwrap();
        ledger.ensure_available().unwrap();
        assert!(ledger.entries("alice").unwrap().is_empty());
        assert_eq!(fs::read(path).unwrap(), bytes);
    }

    #[test]
    fn duplicate_mutation_key_resets_entire_ledger() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let entry_json = serde_json::to_string(&entry("alice")).unwrap();
        let mutation = "01900000-0000-7000-8000-000000000001";
        let document = serde_json::json!({
            "version": FILE_VERSION,
            "accounts": { "alice": { mutation: entry("alice") } }
        });
        let serialized = serde_json::to_string(&document).unwrap();
        let needle = format!("\"{mutation}\":");
        let duplicate = serialized.replacen(&needle, &format!("{needle}{entry_json},{needle}"), 1);
        fs::write(&path, &duplicate).unwrap();
        let ledger = SessionAccessMutationLedger::load_at(path.clone()).unwrap();
        ledger.ensure_available().unwrap();
        assert!(ledger.entries("alice").unwrap().is_empty());
        assert_eq!(fs::read_to_string(path).unwrap(), duplicate);
    }

    #[test]
    fn invalid_leave_session_identity_is_logically_reset() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let mut invalid = entry("alice");
        invalid.backend_session_id = "01900000-0000-7000-8000-000000000004".to_owned();
        invalid.fingerprint = invalid
            .target
            .fingerprint(&invalid.backend_session_id, invalid.backend_incarnation_id)
            .unwrap();
        write_single_entry(&path, invalid);
        let bytes = fs::read(&path).unwrap();
        let ledger = SessionAccessMutationLedger::load_at(path.clone()).unwrap();
        ledger.ensure_available().unwrap();
        assert!(ledger.entries("alice").unwrap().is_empty());
        assert_eq!(fs::read(path).unwrap(), bytes);
    }

    #[test]
    fn invalid_leave_incarnation_identity_is_logically_reset() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let mut invalid = entry("alice");
        invalid.backend_incarnation_id =
            Uuid::parse_str("01900000-0000-7000-8000-000000000004").unwrap();
        invalid.fingerprint = invalid
            .target
            .fingerprint(&invalid.backend_session_id, invalid.backend_incarnation_id)
            .unwrap();
        write_single_entry(&path, invalid);
        let bytes = fs::read(&path).unwrap();
        let ledger = SessionAccessMutationLedger::load_at(path.clone()).unwrap();
        ledger.ensure_available().unwrap();
        assert!(ledger.entries("alice").unwrap().is_empty());
        assert_eq!(fs::read(path).unwrap(), bytes);
    }

    #[test]
    fn unknown_version_is_logically_reset_without_rewriting_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let bytes = br#"{"version":2,"accounts":{}}"#;
        fs::write(&path, bytes).unwrap();
        let ledger = SessionAccessMutationLedger::load_at(path.clone()).unwrap();
        ledger.ensure_available().unwrap();
        assert!(ledger.entries("alice").unwrap().is_empty());
        assert_eq!(fs::read(path).unwrap(), bytes);
    }

    fn write_single_entry(path: &Path, entry: PreparedSessionAccessMutation) {
        let mutation_id = entry.mutation_id;
        let account = entry.account_user_id.clone();
        fs::write(
            path,
            serde_json::to_vec(&serde_json::json!({
                "version": FILE_VERSION,
                "accounts": { account: { mutation_id.to_string(): entry } }
            }))
            .unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn rebound_fingerprint_is_logically_reset_without_rewriting_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let mut ledger = SessionAccessMutationLedger::load_at(path.clone()).unwrap();
        ledger.put(entry("alice")).unwrap();
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let record = value["accounts"]["alice"]
            .as_object_mut()
            .unwrap()
            .values_mut()
            .next()
            .unwrap();
        record["fingerprint"] = serde_json::Value::String("0".repeat(64));
        let bytes = serde_json::to_vec(&value).unwrap();
        fs::write(&path, &bytes).unwrap();
        let ledger = SessionAccessMutationLedger::load_at(path.clone()).unwrap();
        ledger.ensure_available().unwrap();
        assert!(ledger.entries("alice").unwrap().is_empty());
        assert_eq!(fs::read(path).unwrap(), bytes);
    }

    fn assert_oversized_logical_reset(size: usize) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let bytes = vec![b' '; size];
        fs::write(&path, &bytes).unwrap();
        let ledger = SessionAccessMutationLedger::load_at(path.clone()).unwrap();
        ledger.ensure_available().unwrap();
        assert!(ledger.entries("alice").unwrap().is_empty());
        assert_eq!(fs::read(path).unwrap(), bytes);
    }

    #[test]
    fn oversized_ledger_is_logically_reset_without_rewriting_file() {
        let maximum = usize::try_from(MAX_FILE_BYTES).unwrap();
        assert_oversized_logical_reset(maximum + 1);
        assert_oversized_logical_reset(maximum + 2);
        assert_oversized_logical_reset(maximum + 1_024 * 1_024);
    }
}
