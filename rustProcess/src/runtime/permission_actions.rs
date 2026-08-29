use std::{
    collections::{HashSet, VecDeque},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};

use crate::{AppError, Result};
use kodosi_domain::ids::SessionId;

const FILE_VERSION: u32 = 3;
const LEGACY_FILE_VERSION: u32 = 2;
const MAX_PENDING: usize = 128;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemotePermissionAction {
    pub(crate) account_user_id: String,
    pub(crate) session_id: String,
    pub(crate) incarnation_id: uuid::Uuid,
    pub(crate) action_id: String,
    pub(crate) request_id: String,
    pub(crate) request_generation: u64,
    pub(crate) decision: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedRemotePermissionActions {
    version: u32,
    pending: Vec<RemotePermissionAction>,
    #[serde(default)]
    owner_confirmed_action_ids: Vec<String>,
}

#[derive(Debug)]
pub(crate) struct RemotePermissionActionStore {
    path: PathBuf,
    pending: VecDeque<RemotePermissionAction>,
    owner_confirmed_action_ids: HashSet<String>,
}

impl RemotePermissionActionStore {
    pub(crate) fn load_default() -> Result<Self> {
        Self::load(
            crate::support::storage::paths::data_root()?
                .join("pending-remote-permission-actions.json"),
        )
    }

    pub(crate) fn load_at(path: PathBuf) -> Result<Self> {
        Self::load(path)
    }

    fn load(path: PathBuf) -> Result<Self> {
        let persisted = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<PersistedRemotePermissionActions>(&bytes)
                .map_err(AppError::Json)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                PersistedRemotePermissionActions {
                    version: FILE_VERSION,
                    pending: Vec::new(),
                    owner_confirmed_action_ids: Vec::new(),
                }
            }
            Err(error) => return Err(AppError::Io(error)),
        };
        if !matches!(persisted.version, LEGACY_FILE_VERSION | FILE_VERSION)
            || persisted.pending.len() > MAX_PENDING
            || persisted.owner_confirmed_action_ids.len() > persisted.pending.len()
        {
            return Err(AppError::Unsupported {
                reason: "remote permission-action outbox version or bounds are invalid".to_owned(),
            });
        }
        for action in &persisted.pending {
            validate(action)?;
        }
        let unique_action_ids = persisted
            .pending
            .iter()
            .map(|action| &action.action_id)
            .collect::<HashSet<_>>();
        if unique_action_ids.len() != persisted.pending.len() {
            return Err(AppError::Unsupported {
                reason: "remote permission-action IDs are not unique".to_owned(),
            });
        }
        let owner_confirmed_action_ids = persisted
            .owner_confirmed_action_ids
            .into_iter()
            .collect::<HashSet<_>>();
        if owner_confirmed_action_ids.len() > persisted.pending.len()
            || owner_confirmed_action_ids.iter().any(|action_id| {
                !persisted
                    .pending
                    .iter()
                    .any(|action| action.action_id == *action_id)
            })
        {
            return Err(AppError::Unsupported {
                reason: "remote permission-action confirmation set is invalid".to_owned(),
            });
        }
        Ok(Self {
            path,
            pending: persisted.pending.into(),
            owner_confirmed_action_ids,
        })
    }

    pub(crate) fn admit_or_existing(
        &mut self,
        action: RemotePermissionAction,
    ) -> Result<RemotePermissionAction> {
        validate(&action)?;
        if let Some(existing) = self.pending.iter().find(|existing| {
            existing.account_user_id == action.account_user_id
                && existing.session_id == action.session_id
                && existing.incarnation_id == action.incarnation_id
                && existing.request_id == action.request_id
                && existing.request_generation == action.request_generation
        }) {
            if existing.decision == action.decision {
                return Ok(existing.clone());
            }
            return Err(AppError::Unsupported {
                reason: "remote permission request was retried with a different decision"
                    .to_owned(),
            });
        }
        if let Some(existing) = self
            .pending
            .iter()
            .find(|existing| existing.action_id == action.action_id)
        {
            if existing == &action {
                return Ok(existing.clone());
            }
            return Err(AppError::Unsupported {
                reason: "remote permission action ID was reused with different input".to_owned(),
            });
        }
        if self.pending.len() >= MAX_PENDING {
            return Err(AppError::ChannelFull {
                session: action.session_id,
            });
        }
        self.pending.push_back(action.clone());
        if let Err(error) = self.persist() {
            self.pending.pop_back();
            return Err(error);
        }
        Ok(action)
    }

    pub(crate) fn pending_for_incarnation(
        &self,
        account_user_id: &str,
        session_id: SessionId,
        incarnation_id: uuid::Uuid,
    ) -> Vec<RemotePermissionAction> {
        let session_id = session_id.to_string();
        self.pending
            .iter()
            .filter(|action| {
                action.account_user_id == account_user_id
                    && action.session_id == session_id
                    && action.incarnation_id == incarnation_id
            })
            .cloned()
            .collect()
    }

    pub(crate) fn replayable_for_incarnation(
        &self,
        account_user_id: &str,
        session_id: SessionId,
        incarnation_id: uuid::Uuid,
    ) -> Vec<RemotePermissionAction> {
        self.pending_for_incarnation(account_user_id, session_id, incarnation_id)
            .into_iter()
            .filter(|action| !self.owner_confirmed_action_ids.contains(&action.action_id))
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn contains_request(
        &self,
        account_user_id: &str,
        session_id: SessionId,
        incarnation_id: uuid::Uuid,
        request_id: &str,
        request_generation: u64,
    ) -> bool {
        let session_id = session_id.to_string();
        self.pending.iter().any(|action| {
            action.account_user_id == account_user_id
                && action.session_id == session_id
                && action.incarnation_id == incarnation_id
                && action.request_id == request_id
                && action.request_generation == request_generation
        })
    }

    pub(crate) fn contains_action(
        &self,
        account_user_id: &str,
        session_id: SessionId,
        action_id: &str,
    ) -> bool {
        let session_id = session_id.to_string();
        self.pending.iter().any(|action| {
            action.account_user_id == account_user_id
                && action.session_id == session_id
                && action.action_id == action_id
        })
    }

    pub(crate) fn matches_result(
        &self,
        account_user_id: &str,
        session_id: SessionId,
        action_id: &str,
        request_id: &str,
        request_generation: u64,
    ) -> bool {
        let session_id = session_id.to_string();
        self.pending.iter().any(|action| {
            action.account_user_id == account_user_id
                && action.session_id == session_id
                && action.action_id == action_id
                && action.request_id == request_id
                && action.request_generation == request_generation
        })
    }

    pub(crate) fn is_owner_confirmed_exact(
        &self,
        account_user_id: &str,
        session_id: SessionId,
        action_id: &str,
        request_id: &str,
        request_generation: u64,
    ) -> bool {
        self.matches_result(
            account_user_id,
            session_id,
            action_id,
            request_id,
            request_generation,
        ) && self.owner_confirmed_action_ids.contains(action_id)
    }

    pub(crate) fn mark_owner_confirmed_exact(
        &mut self,
        account_user_id: &str,
        session_id: SessionId,
        action_id: &str,
        request_id: &str,
        request_generation: u64,
    ) -> Result<bool> {
        if !self.matches_result(
            account_user_id,
            session_id,
            action_id,
            request_id,
            request_generation,
        ) {
            return Ok(false);
        }
        if self.owner_confirmed_action_ids.contains(action_id) {
            return Ok(true);
        }
        self.owner_confirmed_action_ids.insert(action_id.to_owned());
        if let Err(error) = self.persist() {
            self.owner_confirmed_action_ids.remove(action_id);
            return Err(error);
        }
        Ok(true)
    }

    pub(crate) fn complete_request_exact(
        &mut self,
        account_user_id: &str,
        session_id: SessionId,
        incarnation_id: uuid::Uuid,
        request_id: &str,
        request_generation: u64,
    ) -> Result<Option<RemotePermissionAction>> {
        let session_id = session_id.to_string();
        let Some(index) = self.pending.iter().position(|action| {
            action.account_user_id == account_user_id
                && action.session_id == session_id
                && action.incarnation_id == incarnation_id
                && action.request_id == request_id
                && action.request_generation == request_generation
        }) else {
            return Ok(None);
        };
        self.remove_at(index)
    }

    pub(crate) fn complete_exact(
        &mut self,
        account_user_id: &str,
        session_id: SessionId,
        action_id: &str,
        request_id: &str,
        request_generation: u64,
    ) -> Result<Option<RemotePermissionAction>> {
        let session_id = session_id.to_string();
        let Some(index) = self.pending.iter().position(|action| {
            action.account_user_id == account_user_id
                && action.session_id == session_id
                && action.action_id == action_id
                && action.request_id == request_id
                && action.request_generation == request_generation
        }) else {
            return Ok(None);
        };
        self.remove_at(index)
    }

    fn remove_at(&mut self, index: usize) -> Result<Option<RemotePermissionAction>> {
        let removed = self.pending.remove(index);
        let removed_confirmation = removed
            .as_ref()
            .is_some_and(|action| self.owner_confirmed_action_ids.remove(&action.action_id));
        if let Err(error) = self.persist() {
            if let Some(action) = removed {
                if removed_confirmation {
                    self.owner_confirmed_action_ids
                        .insert(action.action_id.clone());
                }
                self.pending.insert(index, action);
            }
            return Err(error);
        }
        Ok(removed)
    }

    pub(crate) fn clear_session_incarnation(
        &mut self,
        account_user_id: &str,
        session_id: SessionId,
        incarnation_id: uuid::Uuid,
    ) -> Result<()> {
        let previous = self.pending.clone();
        let previous_confirmed = self.owner_confirmed_action_ids.clone();
        let session_id = session_id.to_string();
        self.pending.retain(|action| {
            action.account_user_id != account_user_id
                || action.session_id != session_id
                || action.incarnation_id != incarnation_id
        });
        self.retain_confirmations_for_pending();
        if let Err(error) = self.persist() {
            self.pending = previous;
            self.owner_confirmed_action_ids = previous_confirmed;
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn clear_account(&mut self, account_user_id: &str) -> Result<()> {
        let previous = self.pending.clone();
        let previous_confirmed = self.owner_confirmed_action_ids.clone();
        self.pending
            .retain(|action| action.account_user_id != account_user_id);
        self.retain_confirmations_for_pending();
        if let Err(error) = self.persist() {
            self.pending = previous;
            self.owner_confirmed_action_ids = previous_confirmed;
            return Err(error);
        }
        Ok(())
    }

    fn retain_confirmations_for_pending(&mut self) {
        self.owner_confirmed_action_ids.retain(|action_id| {
            self.pending
                .iter()
                .any(|action| action.action_id == *action_id)
        });
    }

    fn persist(&self) -> Result<()> {
        let mut owner_confirmed_action_ids = self
            .owner_confirmed_action_ids
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        owner_confirmed_action_ids.sort();
        let body = serde_json::to_vec_pretty(&PersistedRemotePermissionActions {
            version: FILE_VERSION,
            pending: self.pending.iter().cloned().collect(),
            owner_confirmed_action_ids,
        })
        .map_err(AppError::Json)?;
        crate::support::storage::atomic_file::atomic_write(
            &self.path,
            &body,
            crate::support::storage::atomic_file::FileMode::UserPrivate,
        )
    }
}

fn validate(action: &RemotePermissionAction) -> Result<()> {
    if action.account_user_id.trim().is_empty()
        || action.session_id.trim().is_empty()
        || action.incarnation_id.is_nil()
        || uuid::Uuid::parse_str(&action.action_id)
            .map_or(true, |id| id.get_version() != Some(uuid::Version::SortRand))
        || action.request_id.trim().is_empty()
        || action.request_generation == 0
        || !matches!(action.decision.as_str(), "allow" | "deny")
    {
        return Err(AppError::Unsupported {
            reason: "remote permission-action tuple is invalid".to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_request_retry_reuses_action_and_rejects_changed_decision() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("actions.json");
        let session_id = SessionId::new();
        let action = RemotePermissionAction {
            account_user_id: "account".to_owned(),
            session_id: session_id.to_string(),
            incarnation_id: uuid::Uuid::now_v7(),
            action_id: uuid::Uuid::now_v7().to_string(),
            request_id: "tool-use".to_owned(),
            request_generation: 7,
            decision: "allow".to_owned(),
        };
        let mut store = RemotePermissionActionStore::load_at(path).expect("store");
        assert_eq!(
            store
                .admit_or_existing(action.clone())
                .expect("first admission"),
            action
        );
        let retry = RemotePermissionAction {
            action_id: uuid::Uuid::now_v7().to_string(),
            ..action.clone()
        };
        assert_eq!(
            store
                .admit_or_existing(retry.clone())
                .expect("identical retry"),
            action
        );
        assert_eq!(
            store
                .replayable_for_incarnation("account", session_id, action.incarnation_id)
                .len(),
            1
        );

        let changed = RemotePermissionAction {
            decision: "deny".to_owned(),
            ..retry
        };
        assert!(store.admit_or_existing(changed).is_err());
        assert_eq!(
            store.replayable_for_incarnation("account", session_id, action.incarnation_id),
            [action]
        );
    }

    #[test]
    fn owner_confirmation_survives_restart_and_stops_replay_until_terminal_snapshot() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("actions.json");
        let session_id = SessionId::new();
        let incarnation_id = uuid::Uuid::now_v7();
        let action = RemotePermissionAction {
            account_user_id: "account".to_owned(),
            session_id: session_id.to_string(),
            incarnation_id,
            action_id: uuid::Uuid::now_v7().to_string(),
            request_id: "tool-use".to_owned(),
            request_generation: 7,
            decision: "allow".to_owned(),
        };
        let mut store = RemotePermissionActionStore::load_at(path.clone()).expect("store");
        store.admit_or_existing(action.clone()).expect("admit");
        assert!(
            store
                .mark_owner_confirmed_exact(
                    "account",
                    session_id,
                    &action.action_id,
                    &action.request_id,
                    action.request_generation,
                )
                .expect("confirm")
        );
        assert!(
            store
                .replayable_for_incarnation("account", session_id, incarnation_id)
                .is_empty()
        );

        let mut restarted = RemotePermissionActionStore::load_at(path).expect("restart");
        assert!(
            restarted
                .replayable_for_incarnation("account", session_id, incarnation_id)
                .is_empty()
        );
        assert!(restarted.contains_request(
            "account",
            session_id,
            incarnation_id,
            &action.request_id,
            action.request_generation,
        ));
        assert_eq!(
            restarted
                .complete_request_exact(
                    "account",
                    session_id,
                    incarnation_id,
                    &action.request_id,
                    action.request_generation,
                )
                .expect("terminal snapshot"),
            Some(action.clone())
        );
        assert!(!restarted.contains_request(
            "account",
            session_id,
            incarnation_id,
            &action.request_id,
            action.request_generation,
        ));
    }

    #[test]
    fn legacy_v2_outbox_loads_as_replayable() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("actions.json");
        let session_id = SessionId::new();
        let incarnation_id = uuid::Uuid::now_v7();
        let action = RemotePermissionAction {
            account_user_id: "account".to_owned(),
            session_id: session_id.to_string(),
            incarnation_id,
            action_id: uuid::Uuid::now_v7().to_string(),
            request_id: "tool-use".to_owned(),
            request_generation: 7,
            decision: "allow".to_owned(),
        };
        std::fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({
                "version": LEGACY_FILE_VERSION,
                "pending": [action.clone()],
            }))
            .expect("legacy JSON"),
        )
        .expect("write legacy outbox");

        let store = RemotePermissionActionStore::load_at(path).expect("legacy load");
        assert_eq!(
            store.replayable_for_incarnation("account", session_id, incarnation_id),
            [action]
        );
    }

    #[test]
    fn pending_actions_are_filtered_by_active_incarnation() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("actions.json");
        let session_id = SessionId::new();
        let first_incarnation = uuid::Uuid::now_v7();
        let second_incarnation = uuid::Uuid::now_v7();
        let action = |incarnation_id| RemotePermissionAction {
            account_user_id: "account".to_owned(),
            session_id: session_id.to_string(),
            incarnation_id,
            action_id: uuid::Uuid::now_v7().to_string(),
            request_id: "tool-use".to_owned(),
            request_generation: 7,
            decision: "allow".to_owned(),
        };
        let first = action(first_incarnation);
        let second = action(second_incarnation);
        let mut store = RemotePermissionActionStore::load_at(path).expect("store");
        store
            .admit_or_existing(first.clone())
            .expect("first action");
        store
            .admit_or_existing(second.clone())
            .expect("second action");

        assert_eq!(
            store.replayable_for_incarnation("account", session_id, first_incarnation),
            vec![first]
        );
        assert_eq!(
            store.replayable_for_incarnation("account", session_id, second_incarnation),
            vec![second]
        );
    }

    #[test]
    fn pending_action_survives_restart_until_terminal_result() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("actions.json");
        let session_id = SessionId::new();
        let action = RemotePermissionAction {
            account_user_id: "account".to_owned(),
            session_id: session_id.to_string(),
            incarnation_id: uuid::Uuid::now_v7(),
            action_id: uuid::Uuid::now_v7().to_string(),
            request_id: "tool-use".to_owned(),
            request_generation: 7,
            decision: "allow".to_owned(),
        };
        let mut store = RemotePermissionActionStore::load_at(path.clone()).expect("store");
        store.admit_or_existing(action.clone()).expect("admit");
        let mut restarted = RemotePermissionActionStore::load_at(path).expect("restart");
        assert_eq!(
            restarted.replayable_for_incarnation("account", session_id, action.incarnation_id),
            std::slice::from_ref(&action)
        );
        assert_eq!(
            restarted
                .complete_exact(
                    "account",
                    session_id,
                    &action.action_id,
                    &action.request_id,
                    action.request_generation,
                )
                .expect("complete"),
            Some(action.clone())
        );
        assert!(!restarted.contains_action("account", session_id, &action.action_id));
    }
}
