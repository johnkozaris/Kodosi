use std::{collections::VecDeque, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{AppError, Result};
use kodosi_domain::ids::SessionId;

const FILE_VERSION: u32 = 2;
const MAX_RESULTS: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OwnerActionResult {
    pub(crate) account_user_id: String,
    pub(crate) session_id: String,
    pub(crate) incarnation_id: uuid::Uuid,
    pub(crate) action_id: String,
    pub(crate) request_id: String,
    pub(crate) request_generation: u64,
    pub(crate) requester_user_id: String,
    pub(crate) requester_device_id: String,
    pub(crate) accepted: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActionAdmission {
    New,
    Pending,
    Terminal(bool),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedOwnerActionResults {
    version: u32,
    results: Vec<OwnerActionResult>,
}

#[derive(Debug)]
pub(crate) struct OwnerActionResultStore {
    path: PathBuf,
    results: VecDeque<OwnerActionResult>,
}

impl OwnerActionResultStore {
    pub(crate) fn load_default() -> Result<Self> {
        Self::load(
            crate::support::storage::paths::data_root()?.join("pending-owner-action-results.json"),
        )
    }

    fn load(path: PathBuf) -> Result<Self> {
        let persisted = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<PersistedOwnerActionResults>(&bytes)
                .map_err(AppError::Json)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                PersistedOwnerActionResults {
                    version: FILE_VERSION,
                    results: Vec::new(),
                }
            }
            Err(error) => return Err(AppError::Io(error)),
        };
        if persisted.version != FILE_VERSION || persisted.results.len() > MAX_RESULTS {
            return Err(AppError::Unsupported {
                reason: "owner action-result ledger version or bounds are invalid".to_owned(),
            });
        }
        for result in &persisted.results {
            validate(result)?;
        }
        Ok(Self {
            path,
            results: persisted.results.into(),
        })
    }

    pub(crate) fn load_at(path: PathBuf) -> Result<Self> {
        Self::load(path)
    }

    pub(crate) fn admit(&mut self, result: OwnerActionResult) -> Result<ActionAdmission> {
        validate(&result)?;
        if let Some(existing) = self
            .results
            .iter()
            .find(|existing| same_identity(existing, &result))
        {
            if same_request(existing, &result) {
                return Ok(existing
                    .accepted
                    .map_or(ActionAdmission::Pending, ActionAdmission::Terminal));
            }
            return Err(AppError::Unsupported {
                reason: "owner action ID was reused with different correlation input".to_owned(),
            });
        }
        if self.results.len() >= MAX_RESULTS {
            return Err(AppError::ChannelFull {
                session: result.session_id,
            });
        }
        self.results.push_back(result);
        if let Err(error) = self.persist() {
            self.results.pop_back();
            return Err(error);
        }
        Ok(ActionAdmission::New)
    }

    pub(crate) fn complete_delivery_attempt(
        &mut self,
        account_user_id: &str,
        session_id: SessionId,
        action_id: &str,
        requester_user_id: &str,
        delivered: bool,
    ) -> Result<bool> {
        let session_id = session_id.to_string();
        let Some(index) = self.results.iter().position(|result| {
            result.account_user_id == account_user_id
                && result.session_id == session_id
                && result.action_id == action_id
                && result.requester_user_id == requester_user_id
        }) else {
            return Ok(false);
        };
        let previous = self.results[index].accepted;
        match previous {
            None => self.results[index].accepted = Some(delivered),
            Some(true) if !delivered => self.results[index].accepted = Some(false),
            Some(existing) => return Ok(existing == delivered),
        }
        if let Err(error) = self.persist() {
            self.results[index].accepted = previous;
            return Err(error);
        }
        Ok(true)
    }

    pub(crate) fn pending_for(
        &self,
        account_user_id: &str,
        session_id: SessionId,
    ) -> Vec<OwnerActionResult> {
        let session_id = session_id.to_string();
        self.results
            .iter()
            .filter(|result| {
                result.account_user_id == account_user_id
                    && result.session_id == session_id
                    && result.accepted.is_some()
            })
            .cloned()
            .collect()
    }

    pub(crate) fn acknowledge(&mut self, expected: &OwnerActionResult) -> Result<bool> {
        let Some(index) = self.results.iter().position(|result| result == expected) else {
            return Ok(false);
        };
        let previous = self.results.clone();
        self.results.remove(index);
        if let Err(error) = self.persist() {
            self.results = previous;
            return Err(error);
        }
        Ok(true)
    }

    pub(crate) fn clear_account(&mut self, account_user_id: &str) -> Result<()> {
        let previous = self.results.clone();
        self.results
            .retain(|result| result.account_user_id != account_user_id);
        if let Err(error) = self.persist() {
            self.results = previous;
            return Err(error);
        }
        Ok(())
    }

    fn persist(&self) -> Result<()> {
        let body = serde_json::to_vec_pretty(&PersistedOwnerActionResults {
            version: FILE_VERSION,
            results: self.results.iter().cloned().collect(),
        })
        .map_err(AppError::Json)?;
        crate::support::storage::atomic_file::atomic_write(
            &self.path,
            &body,
            crate::support::storage::atomic_file::FileMode::UserPrivate,
        )
    }
}

fn same_identity(left: &OwnerActionResult, right: &OwnerActionResult) -> bool {
    left.account_user_id == right.account_user_id
        && left.session_id == right.session_id
        && left.incarnation_id == right.incarnation_id
        && left.action_id == right.action_id
        && left.requester_user_id == right.requester_user_id
}

fn same_request(left: &OwnerActionResult, right: &OwnerActionResult) -> bool {
    same_identity(left, right)
        && left.request_id == right.request_id
        && left.request_generation == right.request_generation
        && left.requester_device_id == right.requester_device_id
}

fn validate(result: &OwnerActionResult) -> Result<()> {
    if result.account_user_id.trim().is_empty()
        || result.session_id.trim().is_empty()
        || result.incarnation_id.is_nil()
        || result.action_id.trim().is_empty()
        || result.request_id.trim().is_empty()
        || result.request_generation == 0
        || result.requester_user_id.trim().is_empty()
        || result.requester_device_id.trim().is_empty()
    {
        return Err(AppError::Unsupported {
            reason: "owner action-result tuple is invalid".to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result() -> OwnerActionResult {
        OwnerActionResult {
            account_user_id: "owner".to_owned(),
            session_id: SessionId::new().to_string(),
            incarnation_id: uuid::Uuid::now_v7(),
            action_id: "action".to_owned(),
            request_id: "tool".to_owned(),
            request_generation: 7,
            requester_user_id: "requester".to_owned(),
            requester_device_id: "device".to_owned(),
            accepted: None,
        }
    }

    #[test]
    fn terminal_result_replays_after_restart_until_acknowledged() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("action-results.json");
        let value = result();
        let session_id = SessionId::parse_field(&value.session_id, "session").expect("session");
        let mut store = OwnerActionResultStore::load_at(path.clone()).expect("store");
        assert_eq!(
            store.admit(value.clone()).expect("admit"),
            ActionAdmission::New
        );
        assert!(
            store
                .complete_delivery_attempt("owner", session_id, "action", "requester", true)
                .expect("complete")
        );

        let mut restarted = OwnerActionResultStore::load_at(path.clone()).expect("restart");
        let pending = restarted.pending_for("owner", session_id);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].accepted, Some(true));
        assert_eq!(
            restarted.admit(value.clone()).expect("retry"),
            ActionAdmission::Terminal(true)
        );
        assert!(restarted.acknowledge(&pending[0]).expect("ack"));
        assert!(
            OwnerActionResultStore::load_at(path)
                .expect("restart after ack")
                .pending_for("owner", session_id)
                .is_empty()
        );
    }

    #[test]
    fn action_id_conflict_is_rejected() {
        let directory = tempfile::tempdir().expect("tempdir");
        let mut store =
            OwnerActionResultStore::load_at(directory.path().join("action-results.json"))
                .expect("store");
        let value = result();
        store.admit(value.clone()).expect("admit");
        let mut conflict = value;
        conflict.request_id = "different-tool".to_owned();
        assert!(store.admit(conflict).is_err());
    }
}
