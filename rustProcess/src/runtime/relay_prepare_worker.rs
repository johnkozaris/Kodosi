use std::sync::Arc;

use kodosi_backend_client::{
    auth::BackendAuthProvider,
    host_ws::HostWsClient,
    relay::{
        self, HostRelayActionResultDelivery, HostRelayFenceCompletion,
        HostRelayPendingPermissionsSnapshot, HostRelaySemanticReceiptDelivery, HostRelaySpec,
        PreparedHostRelay,
    },
};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use crate::{
    Result,
    runtime::{
        access_mutation_worker::SessionAccessMutationWorkerMode,
        access_mutations::PreparedSessionAccessMutation,
        share_transitions::PreparedShareTransition, sharing::ProvisionalRelayBridge,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RelayPrepareOwnerKind {
    Access,
    Share,
    Maintenance,
}

#[derive(Debug)]
pub(crate) enum RelaySigningIdentity {
    Prepared(zeroize::Zeroizing<Vec<u8>>),
    Load {
        services: Box<super::access_effect_worker::AccessEffectServices>,
        expected_host_device_id: String,
    },
}

#[derive(Debug)]
pub(crate) enum RelayPrepareOwner {
    Access {
        prepared: PreparedSessionAccessMutation,
        mode: SessionAccessMutationWorkerMode,
        access_snapshot: Option<Result<kodosi_backend_client::api::BackendAccessGrants>>,
    },
    Share {
        prepared: PreparedShareTransition,
    },
    Maintenance {
        account_user_id: String,
        runtime_session_id: kodosi_domain::ids::SessionId,
        runtime_incarnation_id: Uuid,
        backend_session_id: String,
        backend_incarnation_id: Uuid,
    },
}

impl RelayPrepareOwner {
    pub(crate) const fn kind(&self) -> RelayPrepareOwnerKind {
        match self {
            Self::Access { .. } => RelayPrepareOwnerKind::Access,
            Self::Share { .. } => RelayPrepareOwnerKind::Share,
            Self::Maintenance { .. } => RelayPrepareOwnerKind::Maintenance,
        }
    }

    pub(crate) const fn runtime_session_id(&self) -> kodosi_domain::ids::SessionId {
        match self {
            Self::Access { prepared, .. } => prepared.runtime_session_id,
            Self::Share { prepared } => prepared.runtime_session_id,
            Self::Maintenance {
                runtime_session_id, ..
            } => *runtime_session_id,
        }
    }

    pub(crate) fn account_user_id(&self) -> &str {
        match self {
            Self::Access { prepared, .. } => &prepared.account_user_id,
            Self::Share { prepared } => &prepared.account_user_id,
            Self::Maintenance {
                account_user_id, ..
            } => account_user_id,
        }
    }

    pub(crate) const fn runtime_incarnation_id(&self) -> Uuid {
        match self {
            Self::Access { prepared, .. } => prepared.expected_runtime_incarnation_id,
            Self::Share { prepared } => prepared.expected_runtime_incarnation_id,
            Self::Maintenance {
                runtime_incarnation_id,
                ..
            } => *runtime_incarnation_id,
        }
    }

    pub(crate) fn backend_session_id(&self) -> &str {
        match self {
            Self::Access { prepared, .. } => &prepared.backend_session_id,
            Self::Share { prepared } => &prepared.backend_session_id,
            Self::Maintenance {
                backend_session_id, ..
            } => backend_session_id,
        }
    }

    pub(crate) fn backend_incarnation_id(&self) -> Option<Uuid> {
        match self {
            Self::Access { prepared, .. } => Some(prepared.backend_incarnation_id),
            Self::Share { prepared } => prepared.backend_incarnation_id,
            Self::Maintenance {
                backend_incarnation_id,
                ..
            } => Some(*backend_incarnation_id),
        }
    }

    pub(crate) fn relay_generation(&self) -> Option<u64> {
        match self {
            Self::Access { .. } | Self::Maintenance { .. } => None,
            Self::Share { prepared } => prepared.relay_generation,
        }
    }
}

pub(crate) struct RelayPrepareWork {
    pub(crate) owner: RelayPrepareOwner,
    pub(crate) account_epoch: u64,
    pub(crate) source_key_generation: Option<u32>,
    pub(crate) relay_generation: u64,
    pub(crate) deadline: Option<std::time::Instant>,
    pub(crate) signing_identity: RelaySigningIdentity,
    pub(crate) spec: HostRelaySpec,
    pub(crate) terminal_rx: mpsc::Receiver<relay::HostRelayTerminalEvent>,
    pub(crate) host_ws: HostWsClient,
    pub(crate) auth_provider: BackendAuthProvider,
    pub(crate) event_sink: Arc<dyn relay::HostRelayEventSink>,
    pub(crate) pending_permissions_rx: watch::Receiver<Option<HostRelayPendingPermissionsSnapshot>>,
    pub(crate) semantic_receipt_rx: mpsc::Receiver<HostRelaySemanticReceiptDelivery>,
    pub(crate) action_result_rx: mpsc::Receiver<HostRelayActionResultDelivery>,
    pub(crate) fence_completion_rx: mpsc::Receiver<HostRelayFenceCompletion>,
    pub(crate) cancellation: tokio_util::sync::CancellationToken,
    pub(crate) bridge: ProvisionalRelayBridge,
    pub(crate) pending_permissions_tx: watch::Sender<Option<HostRelayPendingPermissionsSnapshot>>,
    pub(crate) semantic_receipt_tx: mpsc::Sender<HostRelaySemanticReceiptDelivery>,
    pub(crate) action_result_tx: mpsc::Sender<HostRelayActionResultDelivery>,
    pub(crate) fence_completion_tx: mpsc::Sender<HostRelayFenceCompletion>,
}

impl std::fmt::Debug for RelayPrepareWork {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RelayPrepareWork")
            .field("session_id", &self.owner.runtime_session_id())
            .field("relay_generation", &self.relay_generation)
            .finish_non_exhaustive()
    }
}

pub(crate) struct RelayPrepareCommit {
    pub(crate) relay: PreparedHostRelay,
    pub(crate) bridge: ProvisionalRelayBridge,
    pub(crate) pending_permissions_tx: watch::Sender<Option<HostRelayPendingPermissionsSnapshot>>,
    pub(crate) semantic_receipt_tx: mpsc::Sender<HostRelaySemanticReceiptDelivery>,
    pub(crate) action_result_tx: mpsc::Sender<HostRelayActionResultDelivery>,
    pub(crate) fence_completion_tx: mpsc::Sender<HostRelayFenceCompletion>,
}

impl std::fmt::Debug for RelayPrepareCommit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RelayPrepareCommit")
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub(crate) struct RelayPrepareCompletion {
    pub(crate) owner: RelayPrepareOwner,
    pub(crate) account_epoch: u64,
    pub(crate) source_key_generation: Option<u32>,
    pub(crate) relay_generation: u64,
    pub(crate) outcome: Result<RelayPrepareCommit>,
}

pub(crate) async fn execute(work: RelayPrepareWork) -> RelayPrepareCompletion {
    let RelayPrepareWork {
        owner,
        account_epoch,
        source_key_generation,
        relay_generation,
        deadline,
        signing_identity,
        mut spec,
        terminal_rx,
        host_ws,
        auth_provider,
        event_sink,
        pending_permissions_rx,
        semantic_receipt_rx,
        action_result_rx,
        fence_completion_rx,
        cancellation,
        bridge,
        pending_permissions_tx,
        semantic_receipt_tx,
        action_result_tx,
        fence_completion_tx,
    } = work;
    let preparation = async {
        spec.host_signing_pkcs8 = match signing_identity {
            RelaySigningIdentity::Prepared(key) => key,
            RelaySigningIdentity::Load {
                services,
                expected_host_device_id,
            } => {
                super::access_effect_worker::load_relay_signing_identity(
                    &services,
                    &expected_host_device_id,
                )
                .await?
            }
        };
        let relay = relay::prepare(
            spec,
            terminal_rx,
            host_ws,
            auth_provider,
            event_sink,
            pending_permissions_rx,
            semantic_receipt_rx,
            action_result_rx,
            fence_completion_rx,
            cancellation,
        )
        .await
        .map_err(crate::AppError::from)?;
        Ok(RelayPrepareCommit {
            relay,
            bridge,
            pending_permissions_tx,
            semantic_receipt_tx,
            action_result_tx,
            fence_completion_tx,
        })
    };
    let outcome = if let Some(deadline) = deadline {
        tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), preparation)
            .await
            .unwrap_or_else(|_| {
                Err(crate::AppError::Unsupported {
                    reason: "share transition exceeded its execution deadline".to_owned(),
                })
            })
    } else {
        preparation.await
    };
    RelayPrepareCompletion {
        owner,
        account_epoch,
        source_key_generation,
        relay_generation,
        outcome,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RelayPrepareFence {
    pub(crate) account_user_id: String,
    pub(crate) account_epoch: u64,
    pub(crate) runtime_incarnation_id: Uuid,
    pub(crate) backend_session_id: String,
    pub(crate) backend_incarnation_id: Uuid,
    pub(crate) key_generation: Option<u32>,
    pub(crate) relay_generation: u64,
}

impl RelayPrepareCompletion {
    pub(crate) fn fence(&self) -> Option<RelayPrepareFence> {
        Some(RelayPrepareFence {
            account_user_id: self.owner.account_user_id().to_owned(),
            account_epoch: self.account_epoch,
            runtime_incarnation_id: self.owner.runtime_incarnation_id(),
            backend_session_id: self.owner.backend_session_id().to_owned(),
            backend_incarnation_id: self.owner.backend_incarnation_id()?,
            key_generation: self.source_key_generation,
            relay_generation: self.relay_generation,
        })
    }
}

pub(crate) fn exact_fence_matches(
    fence: &RelayPrepareFence,
    account_user_id: Option<&str>,
    account_epoch: u64,
    runtime_incarnation_id: Option<Uuid>,
    backend_session_id: Option<&str>,
    backend_incarnation_id: Option<Uuid>,
    key_generation: Option<u32>,
    current_relay_generation: u64,
) -> bool {
    account_user_id == Some(fence.account_user_id.as_str())
        && account_epoch == fence.account_epoch
        && runtime_incarnation_id == Some(fence.runtime_incarnation_id)
        && backend_session_id == Some(fence.backend_session_id.as_str())
        && backend_incarnation_id == Some(fence.backend_incarnation_id)
        && key_generation == fence.key_generation
        && current_relay_generation == 0
        && fence.relay_generation != 0
}

#[cfg(test)]
mod tests {
    use super::{RelayPrepareFence, exact_fence_matches};

    fn fence() -> RelayPrepareFence {
        RelayPrepareFence {
            account_user_id: "account-a".to_owned(),
            account_epoch: 7,
            runtime_incarnation_id: uuid::Uuid::now_v7(),
            backend_session_id: "backend-session".to_owned(),
            backend_incarnation_id: uuid::Uuid::now_v7(),
            key_generation: Some(4),
            relay_generation: 9,
        }
    }

    fn matches(fence: &RelayPrepareFence, current: &RelayPrepareFence) -> bool {
        exact_fence_matches(
            fence,
            Some(current.account_user_id.as_str()),
            current.account_epoch,
            Some(current.runtime_incarnation_id),
            Some(current.backend_session_id.as_str()),
            Some(current.backend_incarnation_id),
            current.key_generation,
            0,
        )
    }

    #[test]
    fn every_relay_commit_fence_must_match_exactly() {
        let expected = fence();
        assert!(matches(&expected, &expected));
        assert!(!exact_fence_matches(
            &expected,
            Some("account-b"),
            7,
            Some(expected.runtime_incarnation_id),
            Some("backend-session"),
            Some(expected.backend_incarnation_id),
            Some(4),
            0,
        ));
        assert!(!exact_fence_matches(
            &expected,
            Some("account-a"),
            8,
            Some(expected.runtime_incarnation_id),
            Some("backend-session"),
            Some(expected.backend_incarnation_id),
            Some(4),
            0,
        ));
        for changed in [
            RelayPrepareFence {
                runtime_incarnation_id: uuid::Uuid::now_v7(),
                ..expected.clone()
            },
            RelayPrepareFence {
                backend_session_id: "replacement".to_owned(),
                ..expected.clone()
            },
            RelayPrepareFence {
                backend_incarnation_id: uuid::Uuid::now_v7(),
                ..expected.clone()
            },
            RelayPrepareFence {
                key_generation: Some(5),
                ..expected.clone()
            },
            RelayPrepareFence {
                relay_generation: 0,
                ..expected.clone()
            },
        ] {
            assert!(!matches(&changed, &expected));
        }
        assert!(!exact_fence_matches(
            &expected,
            Some("account-a"),
            7,
            Some(expected.runtime_incarnation_id),
            Some("backend-session"),
            Some(expected.backend_incarnation_id),
            Some(4),
            8,
        ));
    }
}
