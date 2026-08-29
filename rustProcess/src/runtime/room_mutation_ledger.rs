use std::{collections::BTreeMap, fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::room_mutations::PreparedRoomMutation;
use crate::{
    AppError, Result,
    support::storage::atomic_file::{
        AtomicWriteFailure, FileMode, atomic_write_json_commit_aware, retry_sync_parent,
    },
};

const FILE_VERSION: u32 = 1;
const MAX_ENTRIES: usize = 256;
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RoomMutationLedgerFile {
    version: u32,
    accounts: BTreeMap<String, BTreeMap<Uuid, PreparedRoomMutation>>,
}

#[derive(Debug)]
pub(crate) struct RoomMutationLedger {
    path: PathBuf,
    file: RoomMutationLedgerFile,
    unavailable_reason: Option<String>,
}

impl RoomMutationLedger {
    pub(crate) fn load_default() -> Result<Self> {
        let path = crate::support::storage::paths::data_root()?.join("pending-room-mutations.json");
        match Self::load(path.clone()) {
            Ok(ledger) => Ok(ledger),
            Err(AppError::Io(error)) => Err(AppError::Io(error)),
            Err(error) => Ok(Self::unavailable(path, error.to_string())),
        }
    }

    fn unavailable(path: PathBuf, reason: String) -> Self {
        Self {
            path,
            file: RoomMutationLedgerFile {
                version: FILE_VERSION,
                accounts: BTreeMap::new(),
            },
            unavailable_reason: Some(reason),
        }
    }

    fn load(path: PathBuf) -> Result<Self> {
        let file = match fs::read(&path) {
            Ok(bytes) => {
                if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_FILE_BYTES {
                    return Err(AppError::InvalidBackendData {
                        field: "pendingRoomMutations".to_owned(),
                        reason: format!("ledger exceeds {MAX_FILE_BYTES} bytes"),
                    });
                }
                let file: RoomMutationLedgerFile =
                    serde_json::from_slice(&bytes).map_err(|error| {
                        AppError::InvalidBackendData {
                            field: "pendingRoomMutations".to_owned(),
                            reason: format!("malformed durable mutation ledger: {error}"),
                        }
                    })?;
                if file.version != FILE_VERSION {
                    return Err(AppError::InvalidBackendData {
                        field: "pendingRoomMutations.version".to_owned(),
                        reason: format!("unsupported ledger version {}", file.version),
                    });
                }
                let count = file.accounts.values().map(BTreeMap::len).sum::<usize>();
                if count > MAX_ENTRIES {
                    return Err(AppError::InvalidBackendData {
                        field: "pendingRoomMutations.entries".to_owned(),
                        reason: format!("ledger exceeds {MAX_ENTRIES} entries"),
                    });
                }
                for (account, entries) in &file.accounts {
                    if account.trim().is_empty() || account.len() > 1_024 {
                        return Err(AppError::InvalidBackendData {
                            field: "pendingRoomMutations.account".to_owned(),
                            reason: "account map key must be non-empty and bounded".to_owned(),
                        });
                    }
                    for (mutation_id, entry) in entries {
                        if entry.account_user_id != *account || entry.mutation_id != *mutation_id {
                            return Err(AppError::InvalidBackendData {
                                field: "pendingRoomMutations.identity".to_owned(),
                                reason: "map keys do not match the retained mutation identity"
                                    .to_owned(),
                            });
                        }
                        entry.validate()?;
                    }
                }
                file
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => RoomMutationLedgerFile {
                version: FILE_VERSION,
                accounts: BTreeMap::new(),
            },
            Err(error) => return Err(AppError::Io(error)),
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
    pub(crate) fn unavailable_for_test(path: PathBuf, reason: impl Into<String>) -> Self {
        Self::unavailable(path, reason.into())
    }

    pub(crate) fn ensure_available(&self) -> Result<()> {
        if let Some(reason) = &self.unavailable_reason {
            return Err(AppError::Unsupported {
                reason: format!(
                    "room mutation ledger is unavailable; retained evidence at {} requires operator repair: {reason}",
                    self.path.display()
                ),
            });
        }
        Ok(())
    }

    pub(crate) fn entries(&self, account: &str) -> Result<Vec<PreparedRoomMutation>> {
        self.ensure_available()?;
        Ok(self
            .file
            .accounts
            .get(account)
            .map(|entries| entries.values().cloned().collect())
            .unwrap_or_default())
    }

    pub(crate) fn get(&self, account: &str, id: Uuid) -> Result<Option<&PreparedRoomMutation>> {
        self.ensure_available()?;
        Ok(self
            .file
            .accounts
            .get(account)
            .and_then(|entries| entries.get(&id)))
    }

    pub(crate) fn put(&mut self, entry: PreparedRoomMutation) -> Result<()> {
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
                reason: "pending room mutation ledger is full".to_owned(),
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
    ) -> Result<Option<PreparedRoomMutation>> {
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

    fn persist_candidate(&mut self, candidate: RoomMutationLedgerFile) -> Result<()> {
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
        candidate: &RoomMutationLedgerFile,
    ) -> Result<()> {
        let reason = format!(
            "room mutation ledger durability is uncertain after atomic replacement: {error}"
        );
        match Self::load(self.path.clone()) {
            Ok(reloaded) => {
                self.file = reloaded.file;
                let observation = if self.file == *candidate {
                    "candidate is visible"
                } else {
                    "unexpected prior or third state is visible"
                };
                self.unavailable_reason = Some(format!("{reason}; {observation}"));
            }
            Err(reload_error) => {
                self.file = RoomMutationLedgerFile {
                    version: FILE_VERSION,
                    accounts: BTreeMap::new(),
                };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::room_mutations::{
        DurableRoomMutationTerminal, DurableRoomMutationTerminalStatus, PreparedRoomMutation,
        PreparedRoomMutationState, RoomMutationIntent, RoomMutationTarget,
    };

    fn entry(account: &str) -> PreparedRoomMutation {
        PreparedRoomMutation::new(
            Uuid::parse_str("01900000-0000-7000-8000-000000000001").unwrap(),
            account.to_owned(),
            RoomMutationIntent::AssignTask {
                room_id: "01900000-0000-7000-8000-000000000010".into(),
                task_id: "01900000-0000-7000-8000-000000000011".into(),
                expected_task_revision: 7,
                session_id: None,
                session_incarnation_id: None,
            },
            RoomMutationTarget::AssignTask {
                room_id: "01900000-0000-7000-8000-000000000010".into(),
                task_id: "01900000-0000-7000-8000-000000000011".into(),
                expected_task_revision: 7,
                session_id: None,
                session_incarnation_id: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn ledger_survives_restart_and_is_account_scoped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let mut ledger = RoomMutationLedger::load_at(path.clone()).unwrap();
        ledger.put(entry("alice")).unwrap();
        drop(ledger);
        let ledger = RoomMutationLedger::load_at(path).unwrap();
        assert_eq!(ledger.entries("alice").unwrap().len(), 1);
        assert!(ledger.entries("bob").unwrap().is_empty());
    }

    #[test]
    fn terminal_outcome_survives_restart_until_acknowledged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let mut terminal = entry("alice");
        terminal.state = PreparedRoomMutationState::Terminal(DurableRoomMutationTerminal {
            status: DurableRoomMutationTerminalStatus::Succeeded,
            entity_id: Some("01900000-0000-7000-8000-000000000011".into()),
            message: None,
        });
        let id = terminal.mutation_id;
        let mut ledger = RoomMutationLedger::load_at(path.clone()).unwrap();
        ledger.put(terminal.clone()).unwrap();
        drop(ledger);

        let mut restarted = RoomMutationLedger::load_at(path.clone()).unwrap();
        assert_eq!(restarted.get("alice", id).unwrap(), Some(&terminal));
        assert_eq!(restarted.remove("alice", id).unwrap(), Some(terminal));
        drop(restarted);

        let acknowledged = RoomMutationLedger::load_at(path).unwrap();
        assert!(acknowledged.get("alice", id).unwrap().is_none());
    }

    #[test]
    fn terminal_conflict_survives_restart_until_acknowledged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let mut terminal = entry("alice");
        terminal.state = PreparedRoomMutationState::Terminal(DurableRoomMutationTerminal {
            status: DurableRoomMutationTerminalStatus::Conflict,
            entity_id: None,
            message: Some("concurrent modification".into()),
        });
        let id = terminal.mutation_id;
        let mut ledger = RoomMutationLedger::load_at(path.clone()).unwrap();
        ledger.put(terminal.clone()).unwrap();
        drop(ledger);

        let mut restarted = RoomMutationLedger::load_at(path.clone()).unwrap();
        let recovered = restarted.get("alice", id).unwrap().unwrap();
        let PreparedRoomMutationState::Terminal(outcome) = &recovered.state else {
            panic!("terminal conflict must remain durable");
        };
        assert_eq!(
            outcome.status.action_status(),
            crate::host_protocol::RoomActionStatus::Conflict
        );
        assert_eq!(restarted.remove("alice", id).unwrap(), Some(terminal));
        drop(restarted);
        assert!(
            RoomMutationLedger::load_at(path)
                .unwrap()
                .get("alice", id)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn valid_json_with_rebound_fingerprint_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let mut ledger = RoomMutationLedger::load_at(path.clone()).unwrap();
        ledger.put(entry("alice")).unwrap();
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let entries = value["accounts"]["alice"].as_object_mut().unwrap();
        let record = entries.values_mut().next().unwrap();
        record["fingerprint"] = serde_json::Value::String("0".repeat(64));
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(RoomMutationLedger::load_at(path).is_err());
    }

    #[test]
    fn oversized_ledger_fails_before_json_decode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        fs::write(
            &path,
            vec![b' '; usize::try_from(MAX_FILE_BYTES).unwrap() + 1],
        )
        .unwrap();
        assert!(RoomMutationLedger::load_at(path).is_err());
    }

    #[test]
    fn default_load_preserves_malformed_evidence_and_degrades_only_room_mutations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        fs::write(&path, b"not json").unwrap();
        let before = fs::read(&path).unwrap();

        let ledger = RoomMutationLedger::load(path.clone())
            .err()
            .map(|error| RoomMutationLedger::unavailable(path.clone(), error.to_string()))
            .unwrap();

        assert!(ledger.ensure_available().is_err());
        assert!(ledger.entries("alice").is_err());
        assert_eq!(fs::read(path).unwrap(), before);
    }

    #[test]
    fn ambiguous_post_replace_failure_reloads_visible_destination_and_disables_mutations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let mut ledger = RoomMutationLedger::load_at(path.clone()).unwrap();
        let expected = entry("alice");
        let id = expected.mutation_id;
        let mut candidate = ledger.file.clone();
        candidate
            .accounts
            .entry("alice".to_owned())
            .or_default()
            .insert(id, expected.clone());
        fs::write(&path, serde_json::to_vec_pretty(&candidate).unwrap()).unwrap();

        assert!(
            ledger
                .disable_after_ambiguous_persist(
                    &AppError::Io(std::io::Error::other("simulated parent sync failure")),
                    &candidate,
                )
                .is_err()
        );
        assert_eq!(ledger.file.accounts["alice"][&id], expected);
        assert!(ledger.ensure_available().is_err());
        let visible = RoomMutationLedger::load_at(path).unwrap();
        assert_eq!(visible.get("alice", id).unwrap(), Some(&expected));
    }

    #[test]
    fn malformed_ledger_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        fs::write(&path, b"not json").unwrap();
        assert!(RoomMutationLedger::load_at(path).is_err());
    }
}
