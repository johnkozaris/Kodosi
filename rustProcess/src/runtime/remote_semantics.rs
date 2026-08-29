use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{AppError, Result};
use kodosi_backend_client::session_relay::wire::{RelaySemanticMode, RelaySemanticOutcome};
use kodosi_domain::ids::SessionId;

const FILE_VERSION: u32 = 2;
const PREVIOUS_FILE_VERSION: u32 = 1;
const MAX_PENDING: usize = 256;
const MAX_COMPLETED: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteSemanticSignerSnapshot {
    pub(crate) account_user_id: String,
    pub(crate) session_id: String,
    pub(crate) incarnation_id: uuid::Uuid,
    pub(crate) owner_user_id: String,
    pub(crate) owner_device_id: String,
    pub(crate) owner_signing_public_key: Vec<u8>,
    pub(crate) device_list_generation: u64,
    pub(crate) identity_fingerprint: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteSemanticRequest {
    pub(crate) account_user_id: String,
    pub(crate) requester_device_id: String,
    pub(crate) session_id: String,
    pub(crate) incarnation_id: uuid::Uuid,
    pub(crate) request_id: uuid::Uuid,
    pub(crate) mode: RelaySemanticMode,
    pub(crate) payload_sha256: String,
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) signer: Option<RemoteSemanticSignerSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteSemanticReceipt {
    pub(crate) request: RemoteSemanticRequest,
    pub(crate) outcome: RelaySemanticOutcome,
    #[serde(default)]
    pub(crate) backend_acknowledged: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedRemoteSemantics {
    version: u32,
    pending: Vec<RemoteSemanticRequest>,
    completed: Vec<RemoteSemanticReceipt>,
    #[serde(default)]
    signer_snapshots: Vec<RemoteSemanticSignerSnapshot>,
}

#[derive(Debug)]
pub(crate) struct RemoteSemanticStore {
    path: PathBuf,
    pending: VecDeque<RemoteSemanticRequest>,
    completed: VecDeque<RemoteSemanticReceipt>,
    signer_snapshots: Vec<RemoteSemanticSignerSnapshot>,
}

impl RemoteSemanticStore {
    pub(crate) fn load_default() -> Result<Self> {
        let path =
            crate::support::storage::paths::data_root()?.join("pending-remote-semantics.json");
        Self::load(&path)
    }

    fn load(path: &Path) -> Result<Self> {
        let (persisted, source_bytes) = match std::fs::read(path) {
            Ok(bytes) => {
                let persisted = serde_json::from_slice::<PersistedRemoteSemantics>(&bytes)
                    .map_err(AppError::Json)?;
                (persisted, Some(bytes))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (
                PersistedRemoteSemantics {
                    version: FILE_VERSION,
                    pending: Vec::new(),
                    completed: Vec::new(),
                    signer_snapshots: Vec::new(),
                },
                None,
            ),
            Err(error) => return Err(AppError::Io(error)),
        };
        if !matches!(persisted.version, FILE_VERSION | PREVIOUS_FILE_VERSION)
            || persisted.pending.len() > MAX_PENDING
            || persisted.completed.len() > MAX_COMPLETED
            || persisted.signer_snapshots.len() > MAX_PENDING
        {
            return Err(AppError::Unsupported {
                reason: "remote semantic ledger version or bounds are invalid".to_owned(),
            });
        }

        let rejected_count = persisted
            .pending
            .iter()
            .filter(|request| validate_request(request).is_err())
            .count()
            + persisted
                .completed
                .iter()
                .filter(|receipt| validate_request(&receipt.request).is_err())
                .count()
            + persisted
                .signer_snapshots
                .iter()
                .filter(|snapshot| validate_signer(snapshot).is_err())
                .count();
        let store = Self {
            path: path.to_path_buf(),
            pending: persisted
                .pending
                .into_iter()
                .filter(|request| validate_request(request).is_ok())
                .collect(),
            completed: persisted
                .completed
                .into_iter()
                .filter(|receipt| validate_request(&receipt.request).is_ok())
                .collect(),
            signer_snapshots: persisted
                .signer_snapshots
                .into_iter()
                .filter(|snapshot| validate_signer(snapshot).is_ok())
                .collect(),
        };
        if rejected_count > 0 {
            let quarantine_path =
                path.with_extension(format!("rejected-{}.json", uuid::Uuid::now_v7().simple()));
            crate::support::storage::atomic_file::atomic_write(
                &quarantine_path,
                source_bytes.as_deref().unwrap_or_default(),
                crate::support::storage::atomic_file::FileMode::UserPrivate,
            )?;
            store.persist()?;
            tracing::warn!(
                count = rejected_count,
                evidence = %quarantine_path.display(),
                "quarantined invalid remote semantic ledger records"
            );
        }
        Ok(store)
    }

    pub(crate) fn load_at(path: &Path) -> Result<Self> {
        Self::load(path)
    }

    pub(crate) fn record_signer_snapshot(
        &mut self,
        snapshot: RemoteSemanticSignerSnapshot,
    ) -> Result<()> {
        validate_signer(&snapshot)?;
        if let Some(existing) = self.signer_snapshots.iter().find(|existing| {
            existing.account_user_id == snapshot.account_user_id
                && existing.session_id == snapshot.session_id
                && existing.incarnation_id == snapshot.incarnation_id
        }) {
            return if existing == &snapshot {
                Ok(())
            } else {
                Err(AppError::Unsupported {
                    reason: "remote semantic signer changed within one incarnation".to_owned(),
                })
            };
        }
        if self.signer_snapshots.len() >= MAX_PENDING {
            return Err(AppError::ChannelFull {
                session: snapshot.session_id,
            });
        }
        self.signer_snapshots.push(snapshot);
        if let Err(error) = self.persist() {
            self.signer_snapshots.pop();
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn signer_snapshot(
        &self,
        account_user_id: &str,
        session_id: SessionId,
        incarnation_id: uuid::Uuid,
    ) -> Option<&RemoteSemanticSignerSnapshot> {
        self.signer_snapshots.iter().find(|snapshot| {
            snapshot.account_user_id == account_user_id
                && snapshot.session_id == session_id.to_string()
                && snapshot.incarnation_id == incarnation_id
        })
    }

    pub(crate) fn admit(
        &mut self,
        request: RemoteSemanticRequest,
    ) -> Result<RemoteSemanticRequest> {
        validate_request(&request)?;
        if let Some(existing) = self
            .pending
            .iter()
            .find(|existing| same_identity(existing, &request))
            .or_else(|| {
                self.completed
                    .iter()
                    .map(|receipt| &receipt.request)
                    .find(|existing| same_identity(existing, &request))
            })
        {
            if existing == &request {
                return Ok(existing.clone());
            }
            return Err(AppError::Unsupported {
                reason: "remote semantic request ID was reused with different input".to_owned(),
            });
        }
        let reserved_completions = self
            .pending
            .iter()
            .filter(|pending| pending.account_user_id == request.account_user_id)
            .count();
        let protected_completions = self
            .completed
            .iter()
            .filter(|receipt| {
                receipt.request.account_user_id == request.account_user_id
                    && !receipt.backend_acknowledged
            })
            .count();
        if self.pending.len() >= MAX_PENDING
            || protected_completions.saturating_add(reserved_completions) >= MAX_COMPLETED
        {
            return Err(AppError::ChannelFull {
                session: request.session_id,
            });
        }
        self.pending.push_back(request.clone());
        if let Err(error) = self.persist() {
            self.pending.pop_back();
            return Err(error);
        }
        Ok(request)
    }

    pub(crate) fn pending_for(
        &self,
        account_user_id: &str,
        session_id: SessionId,
    ) -> Vec<RemoteSemanticRequest> {
        let session_id = session_id.to_string();
        self.pending
            .iter()
            .filter(|request| {
                request.account_user_id == account_user_id && request.session_id == session_id
            })
            .cloned()
            .collect()
    }

    pub(crate) fn complete(
        &mut self,
        account_user_id: &str,
        receipt: &kodosi_backend_client::session_relay::wire::ParticipantSemanticReceiptMessage,
    ) -> Result<Option<RemoteSemanticReceipt>> {
        if let Some(existing) = self.completed.iter().find(|existing| {
            existing.request.account_user_id == account_user_id
                && receipt_matches(&existing.request, receipt)
        }) {
            verify_receipt(&existing.request, receipt)?;
            return Ok(Some(existing.clone()));
        }
        let Some(index) = self.pending.iter().position(|request| {
            request.account_user_id == account_user_id && receipt_matches(request, receipt)
        }) else {
            return Ok(None);
        };
        verify_receipt(&self.pending[index], receipt)?;
        let previous_pending = self.pending.clone();
        let previous_completed = self.completed.clone();
        let Some(request) = self.pending.remove(index) else {
            self.pending = previous_pending;
            self.completed = previous_completed;
            return Err(AppError::InvalidBackendData {
                field: "remoteSemantic.pending".to_owned(),
                reason: "matched request disappeared before completion".to_owned(),
            });
        };
        let completed = RemoteSemanticReceipt {
            request,
            outcome: receipt.outcome,
            backend_acknowledged: false,
        };
        self.completed.push_back(completed.clone());
        while self.completed.len() > MAX_COMPLETED {
            let Some(index) = self
                .completed
                .iter()
                .position(|receipt| receipt.backend_acknowledged)
            else {
                self.pending = previous_pending;
                self.completed = previous_completed;
                return Err(AppError::ChannelFull {
                    session: receipt.session_id.clone(),
                });
            };
            self.completed.remove(index);
        }
        if let Err(error) = self.persist() {
            self.pending = previous_pending;
            self.completed = previous_completed;
            return Err(error);
        }
        Ok(Some(completed))
    }

    pub(crate) fn unacknowledged_completed(
        &self,
        account_user_id: &str,
    ) -> Vec<RemoteSemanticReceipt> {
        self.completed
            .iter()
            .filter(|receipt| {
                receipt.request.account_user_id == account_user_id && !receipt.backend_acknowledged
            })
            .cloned()
            .collect()
    }

    pub(crate) fn acknowledge_backend(
        &mut self,
        account_user_id: &str,
        request_id: uuid::Uuid,
    ) -> Result<bool> {
        let Some(index) = self.completed.iter().position(|receipt| {
            receipt.request.account_user_id == account_user_id
                && receipt.request.request_id == request_id
        }) else {
            return Ok(false);
        };
        if self.completed[index].backend_acknowledged {
            return Ok(true);
        }
        self.completed[index].backend_acknowledged = true;
        if let Err(error) = self.persist() {
            self.completed[index].backend_acknowledged = false;
            return Err(error);
        }
        Ok(true)
    }

    pub(crate) fn query(
        &self,
        account_user_id: &str,
        session_id: SessionId,
        request_id: Option<&str>,
    ) -> Vec<crate::host_protocol::SteerQueueEntry> {
        let session_id_text = session_id.to_string();
        let pending = self.pending.iter().filter(|request| {
            request.account_user_id == account_user_id
                && request.session_id == session_id_text
                && request_id.is_none_or(|id| request.request_id.to_string() == id)
        });
        let completed = self.completed.iter().filter(|receipt| {
            receipt.request.account_user_id == account_user_id
                && receipt.request.session_id == session_id_text
                && request_id.is_none_or(|id| receipt.request.request_id.to_string() == id)
        });
        pending
            .map(|request| to_entry(request, None))
            .chain(completed.map(|receipt| to_entry(&receipt.request, Some(receipt.outcome))))
            .collect()
    }

    pub(crate) fn clear_account(&mut self, account_user_id: &str) -> Result<()> {
        let pending = self.pending.clone();
        let completed = self.completed.clone();
        let signer_snapshots = self.signer_snapshots.clone();
        self.pending
            .retain(|request| request.account_user_id != account_user_id);
        self.completed
            .retain(|receipt| receipt.request.account_user_id != account_user_id);
        self.signer_snapshots
            .retain(|snapshot| snapshot.account_user_id != account_user_id);
        if let Err(error) = self.persist() {
            self.pending = pending;
            self.completed = completed;
            self.signer_snapshots = signer_snapshots;
            return Err(error);
        }
        Ok(())
    }

    fn persist(&self) -> Result<()> {
        let body = serde_json::to_vec_pretty(&PersistedRemoteSemantics {
            version: FILE_VERSION,
            pending: self.pending.iter().cloned().collect(),
            completed: self.completed.iter().cloned().collect(),
            signer_snapshots: self.signer_snapshots.clone(),
        })
        .map_err(AppError::Json)?;
        crate::support::storage::atomic_file::atomic_write(
            &self.path,
            &body,
            crate::support::storage::atomic_file::FileMode::UserPrivate,
        )
    }
}

fn receipt_matches(
    request: &RemoteSemanticRequest,
    receipt: &kodosi_backend_client::session_relay::wire::ParticipantSemanticReceiptMessage,
) -> bool {
    request.session_id == receipt.session_id
        && request.incarnation_id == receipt.incarnation_id
        && request.request_id == receipt.request_id
        && request.mode == receipt.mode
        && request.payload_sha256 == receipt.payload_sha256
        && request.requester_device_id == receipt.requester_device_id
}

fn verify_receipt(
    request: &RemoteSemanticRequest,
    receipt: &kodosi_backend_client::session_relay::wire::ParticipantSemanticReceiptMessage,
) -> Result<()> {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    let signer = request
        .signer
        .as_ref()
        .ok_or_else(|| AppError::Unsupported {
            reason: "remote semantic request lacks a pinned owner signer".to_owned(),
        })?;
    #[expect(
        clippy::suspicious_operation_groupings,
        reason = "request.account_user_id is the authenticated requester bound to the wire requester_user_id"
    )]
    if receipt.requester_user_id != request.account_user_id
        || receipt.owner_user_id != signer.owner_user_id
        || receipt.owner_device_id != signer.owner_device_id
    {
        return Err(AppError::Unsupported {
            reason: "remote semantic receipt identity does not match pinned request".to_owned(),
        });
    }
    let session_id =
        uuid::Uuid::parse_str(&receipt.session_id).map_err(|_| AppError::Unsupported {
            reason: "remote semantic receipt session ID is invalid".to_owned(),
        })?;
    let requester_user_id =
        uuid::Uuid::parse_str(&receipt.requester_user_id).map_err(|_| AppError::Unsupported {
            reason: "remote semantic requester user ID is invalid".to_owned(),
        })?;
    let owner_user_id =
        uuid::Uuid::parse_str(&receipt.owner_user_id).map_err(|_| AppError::Unsupported {
            reason: "remote semantic owner user ID is invalid".to_owned(),
        })?;
    let preimage = kodosi_backend_client::crypto::semantic_receipt_preimage(
        &session_id,
        &receipt.incarnation_id,
        &receipt.request_id,
        receipt.mode.as_str(),
        &receipt.payload_sha256,
        receipt.outcome.as_str(),
        &requester_user_id,
        &receipt.requester_device_id,
        &owner_user_id,
        &receipt.owner_device_id,
    )?;
    let signature = BASE64
        .decode(&receipt.signature)
        .map_err(|_| AppError::Unsupported {
            reason: "remote semantic receipt signature is not valid base64".to_owned(),
        })?;
    kodosi_backend_client::crypto::verify_control_message(
        &signer.owner_signing_public_key,
        &preimage,
        &signature,
    )?;
    Ok(())
}

pub(super) fn to_entry_public(
    receipt: &RemoteSemanticReceipt,
) -> crate::host_protocol::SteerQueueEntry {
    to_entry(&receipt.request, Some(receipt.outcome))
}

fn to_entry(
    request: &RemoteSemanticRequest,
    outcome: Option<RelaySemanticOutcome>,
) -> crate::host_protocol::SteerQueueEntry {
    crate::host_protocol::SteerQueueEntry {
        steer_id: request.request_id.to_string(),
        account_user_id: request.account_user_id.clone(),
        request_id: request.request_id.to_string(),
        session_incarnation_id: request.incarnation_id.to_string(),
        mode: match request.mode {
            RelaySemanticMode::Queue => crate::host_protocol::SemanticSendMode::Queue,
            RelaySemanticMode::Steer => crate::host_protocol::SemanticSendMode::Steer,
            RelaySemanticMode::StopAndSend => crate::host_protocol::SemanticSendMode::StopAndSend,
        },
        session_id: request.session_id.clone(),
        text: request.text.clone(),
        queued_at_ms: 0,
        delivery_state: match outcome {
            None => crate::host_protocol::SteerDeliveryState::Preparing,
            Some(RelaySemanticOutcome::Injected) => {
                crate::host_protocol::SteerDeliveryState::Injected
            }
            Some(RelaySemanticOutcome::Cancelled) => {
                crate::host_protocol::SteerDeliveryState::Cancelled
            }
            Some(RelaySemanticOutcome::DeliveryUnknown) => {
                crate::host_protocol::SteerDeliveryState::DeliveryUnknown
            }
        },
        at_tool_use_id: None,
    }
}

fn same_identity(left: &RemoteSemanticRequest, right: &RemoteSemanticRequest) -> bool {
    left.account_user_id == right.account_user_id && left.request_id == right.request_id
}

fn validate_signer(signer: &RemoteSemanticSignerSnapshot) -> Result<()> {
    if signer.account_user_id.trim().is_empty()
        || signer.session_id.trim().is_empty()
        || signer.incarnation_id.is_nil()
        || signer.owner_user_id.trim().is_empty()
        || signer.owner_device_id.trim().is_empty()
        || signer.owner_signing_public_key.is_empty()
        || signer.device_list_generation == 0
    {
        return Err(AppError::Unsupported {
            reason: "remote semantic signer snapshot is invalid".to_owned(),
        });
    }
    Ok(())
}

fn validate_request(request: &RemoteSemanticRequest) -> Result<()> {
    let Some(signer) = request.signer.as_ref() else {
        return Err(AppError::Unsupported {
            reason: "remote semantic request lacks a pinned owner signer".to_owned(),
        });
    };
    validate_signer(signer)?;
    if request.request_id.get_version() != Some(uuid::Version::SortRand)
        || request.incarnation_id.is_nil()
        || request.account_user_id.trim().is_empty()
        || request.requester_device_id.trim().is_empty()
        || request.text.trim().is_empty()
        || request.text.len() > kodosi_domain::session::REMOTE_SEMANTIC_TEXT_MAX_BYTES
        || request.payload_sha256
            != kodosi_backend_client::crypto::sha256_hex(request.text.as_bytes())
        || signer.account_user_id != request.account_user_id
        || signer.session_id != request.session_id
        || signer.incarnation_id != request.incarnation_id
        || signer.owner_user_id != request.account_user_id
    {
        return Err(AppError::Unsupported {
            reason: "remote semantic request tuple is invalid".to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;

    fn account_id() -> String {
        "11111111-1111-1111-1111-111111111111".to_owned()
    }

    fn request() -> (RemoteSemanticRequest, aws_lc_rs::signature::PqdsaKeyPair) {
        use aws_lc_rs::signature::KeyPair as _;
        let signing_key =
            aws_lc_rs::signature::PqdsaKeyPair::generate(&aws_lc_rs::signature::ML_DSA_65_SIGNING)
                .expect("signing key");
        let session_id = SessionId::new().to_string();
        let incarnation_id = uuid::Uuid::now_v7();
        (
            RemoteSemanticRequest {
                account_user_id: account_id(),
                requester_device_id: "device".to_owned(),
                session_id: session_id.clone(),
                incarnation_id,
                request_id: uuid::Uuid::now_v7(),
                mode: RelaySemanticMode::Steer,
                payload_sha256: kodosi_backend_client::crypto::sha256_hex(b"hello"),
                text: "hello".to_owned(),
                signer: Some(RemoteSemanticSignerSnapshot {
                    account_user_id: account_id(),
                    session_id,
                    incarnation_id,
                    owner_user_id: account_id(),
                    owner_device_id: "owner-device".to_owned(),
                    owner_signing_public_key: signing_key.public_key().as_ref().to_vec(),
                    device_list_generation: 1,
                    identity_fingerprint: [7; 32],
                }),
            },
            signing_key,
        )
    }

    #[test]
    fn verified_receipt_moves_request_to_durable_terminal_history() {
        use aws_lc_rs::signature::PqdsaKeyPair;
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("remote-semantics.json");
        let (value, signing_key): (RemoteSemanticRequest, PqdsaKeyPair) = request();
        let mut store = RemoteSemanticStore::load_at(&path).expect("store");
        store.admit(value.clone()).expect("admit");
        let mut receipt =
            kodosi_backend_client::session_relay::wire::ParticipantSemanticReceiptMessage {
                session_id: value.session_id.clone(),
                incarnation_id: value.incarnation_id,
                request_id: value.request_id,
                mode: value.mode,
                payload_sha256: value.payload_sha256.clone(),
                outcome: RelaySemanticOutcome::DeliveryUnknown,
                requester_user_id: value.account_user_id.clone(),
                requester_device_id: value.requester_device_id.clone(),
                owner_user_id: value.account_user_id.clone(),
                owner_device_id: "owner-device".to_owned(),
                signature: String::new(),
            };
        let preimage = kodosi_backend_client::crypto::semantic_receipt_preimage(
            &uuid::Uuid::parse_str(&receipt.session_id).expect("session uuid"),
            &receipt.incarnation_id,
            &receipt.request_id,
            receipt.mode.as_str(),
            &receipt.payload_sha256,
            receipt.outcome.as_str(),
            &uuid::Uuid::parse_str(&receipt.requester_user_id).expect("requester uuid"),
            &receipt.requester_device_id,
            &uuid::Uuid::parse_str(&receipt.owner_user_id).expect("owner uuid"),
            &receipt.owner_device_id,
        )
        .expect("preimage");
        let mut signature_bytes = vec![0u8; 3309];
        let len = signing_key
            .sign(&preimage, &mut signature_bytes)
            .expect("sign");
        signature_bytes.truncate(len);
        receipt.signature = base64::engine::general_purpose::STANDARD.encode(signature_bytes);

        let completed = store
            .complete(&account_id(), &receipt)
            .expect("complete")
            .expect("matching request");
        assert!(!completed.backend_acknowledged);
        assert!(
            store
                .pending_for(
                    &account_id(),
                    SessionId::parse_field(&value.session_id, "session").expect("session"),
                )
                .is_empty()
        );
        assert!(
            store
                .complete(&account_id(), &receipt)
                .expect("duplicate")
                .is_some()
        );
        assert_eq!(
            store.unacknowledged_completed(&account_id()),
            std::slice::from_ref(&completed)
        );
        assert!(
            store
                .acknowledge_backend(&account_id(), value.request_id)
                .expect("ack")
        );
        let restarted = RemoteSemanticStore::load_at(&path).expect("restart");
        assert!(restarted.unacknowledged_completed(&account_id()).is_empty());
        let entries = restarted.query(
            &account_id(),
            SessionId::parse_field(&value.session_id, "session").expect("session"),
            Some(&value.request_id.to_string()),
        );
        assert_eq!(
            entries[0].delivery_state,
            crate::host_protocol::SteerDeliveryState::DeliveryUnknown
        );
    }

    #[test]
    fn unacknowledged_receipts_reserve_completion_capacity_before_admission() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("remote-semantics.json");
        let (template, _signing_key) = request();
        let completed = (0..MAX_COMPLETED)
            .map(|index| {
                let mut value = template.clone();
                value.request_id = uuid::Uuid::now_v7();
                value.text = format!("completed-{index}");
                value.payload_sha256 =
                    kodosi_backend_client::crypto::sha256_hex(value.text.as_bytes());
                RemoteSemanticReceipt {
                    request: value,
                    outcome: RelaySemanticOutcome::Injected,
                    backend_acknowledged: false,
                }
            })
            .collect();
        let payload = serde_json::to_vec_pretty(&PersistedRemoteSemantics {
            version: FILE_VERSION,
            pending: Vec::new(),
            completed,
            signer_snapshots: Vec::new(),
        })
        .expect("serialize ledger");
        std::fs::write(&path, payload).expect("write ledger");
        let mut store = RemoteSemanticStore::load_at(&path).expect("store");

        assert!(matches!(
            store.admit(template),
            Err(AppError::ChannelFull { .. })
        ));
        assert!(store.pending.is_empty());
    }

    #[test]
    fn oversized_request_is_rejected_before_persistence() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("remote-semantics.json");
        let (mut value, _signing_key) = request();
        value.text = "x".repeat(kodosi_domain::session::REMOTE_SEMANTIC_TEXT_MAX_BYTES + 1);
        value.payload_sha256 = kodosi_backend_client::crypto::sha256_hex(value.text.as_bytes());
        let mut store = RemoteSemanticStore::load_at(&path).expect("store");

        assert!(store.admit(value).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn persisted_oversized_request_is_quarantined_without_blocking_later_work() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("remote-semantics.json");
        let (valid, _signing_key) = request();
        let mut oversized = valid.clone();
        oversized.request_id = uuid::Uuid::now_v7();
        oversized.text = "x".repeat(kodosi_domain::session::REMOTE_SEMANTIC_TEXT_MAX_BYTES + 1);
        oversized.payload_sha256 =
            kodosi_backend_client::crypto::sha256_hex(oversized.text.as_bytes());
        let payload = serde_json::to_vec_pretty(&PersistedRemoteSemantics {
            version: FILE_VERSION,
            pending: vec![oversized, valid.clone()],
            completed: Vec::new(),
            signer_snapshots: Vec::new(),
        })
        .expect("serialize ledger");
        std::fs::write(&path, payload).expect("write ledger");

        let store = RemoteSemanticStore::load_at(&path).expect("quarantine poison");
        let session_id = SessionId::parse_field(&valid.session_id, "session").expect("session");
        assert_eq!(store.pending_for(&account_id(), session_id), vec![valid]);
        let evidence_count = std::fs::read_dir(directory.path())
            .expect("read directory")
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".rejected-"))
            .count();
        assert_eq!(evidence_count, 1);
        let rewritten = RemoteSemanticStore::load_at(&path).expect("rewritten ledger");
        assert_eq!(rewritten.pending_for(&account_id(), session_id).len(), 1);
    }

    #[test]
    fn exact_retry_survives_restart_and_conflict_is_rejected() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("remote-semantics.json");
        let (value, _signing_key) = request();
        let mut store = RemoteSemanticStore::load_at(&path).expect("store");
        store.admit(value.clone()).expect("admit");
        let mut restarted = RemoteSemanticStore::load_at(&path).expect("restart");
        assert_eq!(restarted.admit(value.clone()).expect("retry"), value);
        let mut conflict = value;
        conflict.text = "changed".to_owned();
        conflict.payload_sha256 = kodosi_backend_client::crypto::sha256_hex(b"changed");
        assert!(restarted.admit(conflict).is_err());
    }
}
