use kodosi_backend_client::{
    BackendClientError, api::GrantBackendAccessRequest, http_client::BackendHttpClient,
};

use super::access_mutations::{PreparedSessionAccessMutation, SessionAccessMutationTarget};
use crate::{AppError, Result};

#[derive(Debug)]
pub(crate) struct SessionAccessMutationWorkerCompletion {
    pub(crate) prepared: PreparedSessionAccessMutation,
    pub(crate) account_epoch: u64,
    pub(crate) mode: SessionAccessMutationWorkerMode,
    pub(crate) outcome: SessionAccessMutationWorkerOutcome,
    pub(crate) access_snapshot: Option<Result<kodosi_backend_client::api::BackendAccessGrants>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionAccessMutationWorkerMode {
    Dispatch,
    Reconcile,
}

#[derive(Debug)]
pub(crate) struct SessionAccessMutationWorkerWork {
    pub(crate) prepared: PreparedSessionAccessMutation,
    pub(crate) account_epoch: u64,
    pub(crate) mode: SessionAccessMutationWorkerMode,
}

#[derive(Debug)]
pub(crate) enum SessionAccessMutationWorkerOutcome {
    Applied,
    Unknown(String),
    Rejected(AppError),
    Deferred(AppError),
}

pub(crate) async fn execute(
    backend: BackendHttpClient,
    work: SessionAccessMutationWorkerWork,
) -> SessionAccessMutationWorkerCompletion {
    let SessionAccessMutationWorkerWork {
        prepared,
        account_epoch,
        mode,
    } = work;
    let outcome = match mode {
        SessionAccessMutationWorkerMode::Dispatch => dispatch(&backend, &prepared).await,
        SessionAccessMutationWorkerMode::Reconcile => match prepared.state {
            super::access_mutations::PreparedSessionAccessMutationState::ReceiptConfirmed
            | super::access_mutations::PreparedSessionAccessMutationState::EffectPending
            | super::access_mutations::PreparedSessionAccessMutationState::RelayPending {
                ..
            } => SessionAccessMutationWorkerOutcome::Applied,
            super::access_mutations::PreparedSessionAccessMutationState::Retiring => {
                reconcile_receipt(&backend, &prepared).await
            }
            super::access_mutations::PreparedSessionAccessMutationState::Terminal(_) => {
                SessionAccessMutationWorkerOutcome::Deferred(AppError::Unsupported {
                    reason: "terminal access mutation requires no reconciliation".to_owned(),
                })
            }
            _ => reconcile_receipt(&backend, &prepared).await,
        },
    };
    let access_snapshot = if matches!(outcome, SessionAccessMutationWorkerOutcome::Applied)
        && !matches!(prepared.target, SessionAccessMutationTarget::Leave)
    {
        Some(
            backend
                .list_access(
                    &prepared.backend_session_id,
                    prepared.backend_incarnation_id,
                )
                .await
                .map_err(AppError::from),
        )
    } else {
        None
    };
    SessionAccessMutationWorkerCompletion {
        prepared,
        account_epoch,
        mode,
        outcome,
        access_snapshot,
    }
}

async fn dispatch(
    backend: &BackendHttpClient,
    prepared: &PreparedSessionAccessMutation,
) -> SessionAccessMutationWorkerOutcome {
    let result = match &prepared.target {
        SessionAccessMutationTarget::Grant {
            actor_user_id,
            access_level,
            expires_at_unix_ms,
        } => {
            let expires_at = match format_expiry(*expires_at_unix_ms) {
                Ok(value) => value,
                Err(error) => return SessionAccessMutationWorkerOutcome::Rejected(error),
            };
            let request = GrantBackendAccessRequest {
                expected_incarnation_id: prepared.backend_incarnation_id,
                mutation_id: prepared.mutation_id,
                actor_user_id: actor_user_id.to_string(),
                access_level: *access_level,
                expires_at,
            };
            super::sharing::dispatch_access_mutation(
                backend,
                &prepared.backend_session_id,
                prepared.backend_incarnation_id,
                prepared.mutation_id,
                &prepared.target,
                || backend.grant_access(&prepared.backend_session_id, &request),
            )
            .await
        }
        SessionAccessMutationTarget::Revoke { actor_user_id } => {
            let actor_user_id = actor_user_id.to_string();
            super::sharing::dispatch_access_mutation(
                backend,
                &prepared.backend_session_id,
                prepared.backend_incarnation_id,
                prepared.mutation_id,
                &prepared.target,
                || {
                    backend.revoke_access(
                        &prepared.backend_session_id,
                        &prepared.backend_incarnation_id,
                        &prepared.mutation_id,
                        &actor_user_id,
                    )
                },
            )
            .await
        }
        SessionAccessMutationTarget::Leave => {
            super::sharing::dispatch_access_mutation(
                backend,
                &prepared.backend_session_id,
                prepared.backend_incarnation_id,
                prepared.mutation_id,
                &prepared.target,
                || {
                    backend.leave_session_access(
                        &prepared.backend_session_id,
                        &prepared.backend_incarnation_id,
                        &prepared.mutation_id,
                    )
                },
            )
            .await
        }
    };
    match result {
        Ok(super::sharing::AccessMutationSettlement::Applied) => {
            SessionAccessMutationWorkerOutcome::Applied
        }
        Ok(super::sharing::AccessMutationSettlement::Unknown(message)) => {
            SessionAccessMutationWorkerOutcome::Unknown(message)
        }
        Err(error) => SessionAccessMutationWorkerOutcome::Rejected(error),
    }
}

async fn reconcile_receipt(
    backend: &BackendHttpClient,
    prepared: &PreparedSessionAccessMutation,
) -> SessionAccessMutationWorkerOutcome {
    match backend
        .get_session_access_mutation_receipt(
            &prepared.backend_session_id,
            &prepared.backend_incarnation_id,
            &prepared.mutation_id,
        )
        .await
    {
        Ok(receipt) => match super::sharing::validate_access_mutation_receipt(
            &receipt,
            &prepared.backend_session_id,
            prepared.backend_incarnation_id,
            prepared.mutation_id,
            &prepared.target,
        ) {
            Ok(()) => SessionAccessMutationWorkerOutcome::Applied,
            Err(error) => SessionAccessMutationWorkerOutcome::Rejected(error),
        },
        Err(BackendClientError::NotFound) => SessionAccessMutationWorkerOutcome::Unknown(format!(
            "session access mutation {} is still reconciling; no exact receipt is visible yet",
            prepared.mutation_id
        )),
        Err(error) => SessionAccessMutationWorkerOutcome::Deferred(error.into()),
    }
}

fn format_expiry(expires_at_unix_ms: i64) -> Result<String> {
    time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(expires_at_unix_ms) * 1_000_000)
        .map_err(|error| AppError::InvalidBackendData {
            field: "pendingSessionAccessMutations.expiresAt".to_owned(),
            reason: error.to_string(),
        })?
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|error| AppError::InvalidBackendData {
            field: "pendingSessionAccessMutations.expiresAt".to_owned(),
            reason: error.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use axum::{
        Json, Router,
        extract::State,
        http::StatusCode,
        routing::{delete, get},
    };
    use kodosi_backend_client::{config::BackendClientConfig, http_client::BackendHttpClient};
    use kodosi_domain::{ids::SessionId, permissions::AccessLevel};
    use uuid::Uuid;

    use super::{
        SessionAccessMutationWorkerMode, SessionAccessMutationWorkerOutcome,
        SessionAccessMutationWorkerWork, execute,
    };
    use crate::runtime::access_mutations::{
        PreparedSessionAccessMutation, PreparedSessionAccessMutationState,
        SessionAccessMutationTarget,
    };

    #[derive(Clone)]
    struct ServerState {
        writes: Arc<AtomicUsize>,
        receipts: Arc<AtomicUsize>,
        lists: Arc<AtomicUsize>,
        prepared: PreparedSessionAccessMutation,
    }

    #[tokio::test]
    async fn dispatch_writes_once_and_fetches_projection() {
        let prepared = prepared_revoke();
        let state = ServerState::new(prepared.clone());
        let (backend, server) = server(state.clone()).await;

        let completion = execute(
            backend,
            SessionAccessMutationWorkerWork {
                prepared,
                account_epoch: 7,
                mode: SessionAccessMutationWorkerMode::Dispatch,
            },
        )
        .await;

        assert!(matches!(
            completion.outcome,
            SessionAccessMutationWorkerOutcome::Applied
        ));
        assert!(
            completion
                .access_snapshot
                .is_some_and(|result| result.is_ok())
        );
        assert_eq!(state.writes.load(Ordering::SeqCst), 1);
        assert_eq!(state.receipts.load(Ordering::SeqCst), 0);
        assert_eq!(state.lists.load(Ordering::SeqCst), 1);
        server.abort();
    }

    #[tokio::test]
    async fn restart_reconciliation_reads_receipt_without_replaying_write() {
        let prepared = prepared_revoke();
        let state = ServerState::new(prepared.clone());
        let (backend, server) = server(state.clone()).await;

        let completion = execute(
            backend,
            SessionAccessMutationWorkerWork {
                prepared,
                account_epoch: 7,
                mode: SessionAccessMutationWorkerMode::Reconcile,
            },
        )
        .await;

        assert!(matches!(
            completion.outcome,
            SessionAccessMutationWorkerOutcome::Applied
        ));
        assert_eq!(state.writes.load(Ordering::SeqCst), 0);
        assert_eq!(state.receipts.load(Ordering::SeqCst), 1);
        assert_eq!(state.lists.load(Ordering::SeqCst), 1);
        server.abort();
    }

    #[tokio::test]
    async fn recovered_unknown_reconciliation_reads_receipt_without_replaying_write() {
        let mut prepared = prepared_revoke();
        prepared.state = PreparedSessionAccessMutationState::OutcomeUnknown;
        let state = ServerState::new(prepared.clone());
        let (backend, server) = server(state.clone()).await;

        let completion = execute(
            backend,
            SessionAccessMutationWorkerWork {
                prepared,
                account_epoch: 7,
                mode: SessionAccessMutationWorkerMode::Reconcile,
            },
        )
        .await;

        assert!(matches!(
            completion.outcome,
            SessionAccessMutationWorkerOutcome::Applied
        ));
        assert_eq!(state.writes.load(Ordering::SeqCst), 0);
        assert_eq!(state.receipts.load(Ordering::SeqCst), 1);
        assert_eq!(state.lists.load(Ordering::SeqCst), 1);
        server.abort();
    }

    impl ServerState {
        fn new(prepared: PreparedSessionAccessMutation) -> Self {
            Self {
                writes: Arc::new(AtomicUsize::new(0)),
                receipts: Arc::new(AtomicUsize::new(0)),
                lists: Arc::new(AtomicUsize::new(0)),
                prepared,
            }
        }
    }

    fn prepared_revoke() -> PreparedSessionAccessMutation {
        let session_id = Uuid::now_v7();
        let mut prepared = PreparedSessionAccessMutation::new(
            Uuid::now_v7(),
            Uuid::now_v7().to_string(),
            7,
            SessionId::parse_field(&session_id.to_string(), "sessionId").unwrap(),
            Uuid::now_v7(),
            session_id.to_string(),
            Uuid::now_v7(),
            SessionAccessMutationTarget::Revoke {
                actor_user_id: Uuid::now_v7(),
            },
        )
        .unwrap();
        prepared.state = PreparedSessionAccessMutationState::Attempting;
        prepared
    }

    async fn server(state: ServerState) -> (BackendHttpClient, tokio::task::JoinHandle<()>) {
        let router = Router::new()
            .route(
                "/api/sessions/{session}/access/{actor}",
                delete(write_access),
            )
            .route(
                "/api/sessions/{session}/access/mutations/{mutation}",
                get(read_receipt),
            )
            .route("/api/sessions/{session}/access", get(list_access))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let mut backend = BackendHttpClient::new(&BackendClientConfig {
            api: Some(format!("http://{address}")),
            ..BackendClientConfig::default()
        })
        .unwrap();
        backend.set_access_token(Some(zeroize::Zeroizing::new("test-token".to_owned())));
        (backend, server)
    }

    async fn write_access(State(state): State<ServerState>) -> StatusCode {
        state.writes.fetch_add(1, Ordering::SeqCst);
        StatusCode::NO_CONTENT
    }

    async fn read_receipt(State(state): State<ServerState>) -> Json<serde_json::Value> {
        state.receipts.fetch_add(1, Ordering::SeqCst);
        let target_user_id = match state.prepared.target {
            SessionAccessMutationTarget::Revoke { actor_user_id } => actor_user_id,
            _ => unreachable!(),
        };
        Json(serde_json::json!({
            "mutationId": state.prepared.mutation_id,
            "sessionId": state.prepared.backend_session_id,
            "incarnationId": state.prepared.backend_incarnation_id,
            "kind": "revoke",
            "targetUserId": target_user_id,
            "accessLevel": null,
            "requestedExpiresAt": null
        }))
    }

    async fn list_access(State(state): State<ServerState>) -> Json<serde_json::Value> {
        state.lists.fetch_add(1, Ordering::SeqCst);
        Json(serde_json::json!({
            "incarnationId": state.prepared.backend_incarnation_id,
            "grants": [{
                "actorUserId": Uuid::now_v7(),
                "handle": "viewer",
                "displayName": "Viewer",
                "accessLevel": AccessLevel::View,
                "grantedAt": "2026-08-15T00:00:00Z",
                "expiresAt": null
            }]
        }))
    }
}
