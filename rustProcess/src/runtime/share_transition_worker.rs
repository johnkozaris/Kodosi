use kodosi_backend_client::{
    api::{BackendSessionDetail, CreateBackendSessionRequest},
    http_client::BackendHttpClient,
};
use zeroize::Zeroizing;

use crate::{
    AppError, Result,
    runtime::{
        access_effect_worker::{
            AccessEffectAudience, AccessEffectCommit, AccessEffectServices,
            ClaimedShareKeyDistribution, PreparedShareKeyDistribution, ShareKeyEffectWork,
        },
        share_transitions::{PreparedShareTransition, ShareAudience},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShareTransitionWorkerMode {
    ApplyTarget,
    PrepareKey,
    ClaimGeneration,
    PublishBlobs,
}

#[derive(Debug)]
pub(crate) enum ShareTransitionWorkerInput {
    ApplyTarget {
        create_request: Option<CreateBackendSessionRequest>,
        owner_secret: Option<Zeroizing<String>>,
    },
    PrepareKey {
        services: AccessEffectServices,
        audience: AccessEffectAudience,
        previous_generation: u32,
    },
    ClaimGeneration {
        prepared: PreparedShareKeyDistribution,
    },
    PublishBlobs {
        claimed: ClaimedShareKeyDistribution,
    },
    #[cfg(test)]
    Panic { mode: ShareTransitionWorkerMode },
}

impl ShareTransitionWorkerInput {
    pub(crate) const fn mode(&self) -> ShareTransitionWorkerMode {
        match self {
            Self::ApplyTarget { .. } => ShareTransitionWorkerMode::ApplyTarget,
            Self::PrepareKey { .. } => ShareTransitionWorkerMode::PrepareKey,
            Self::ClaimGeneration { .. } => ShareTransitionWorkerMode::ClaimGeneration,
            Self::PublishBlobs { .. } => ShareTransitionWorkerMode::PublishBlobs,
            #[cfg(test)]
            Self::Panic { mode } => *mode,
        }
    }
}

#[derive(Debug)]
pub(crate) struct ShareTransitionWork {
    pub(crate) prepared: PreparedShareTransition,
    pub(crate) account_epoch: u64,
    pub(crate) backend: BackendHttpClient,
    pub(crate) input: ShareTransitionWorkerInput,
}

#[derive(Debug)]
pub(crate) enum ShareTransitionWorkerOutcome {
    TargetObserved {
        detail: BackendSessionDetail,
        owner_secret: Option<Zeroizing<String>>,
        source_key_generation: Option<u32>,
    },
    KeyPrepared(PreparedShareKeyDistribution),
    GenerationClaimed(ClaimedShareKeyDistribution),
    BlobsPublished(AccessEffectCommit),
}

#[derive(Debug)]
pub(crate) struct ShareTransitionCompletion {
    pub(crate) prepared: PreparedShareTransition,
    pub(crate) account_epoch: u64,
    pub(crate) mode: ShareTransitionWorkerMode,
    pub(crate) outcome: Result<ShareTransitionWorkerOutcome>,
}

pub(crate) async fn execute(work: ShareTransitionWork) -> ShareTransitionCompletion {
    let ShareTransitionWork {
        prepared,
        account_epoch,
        backend,
        input,
    } = work;
    let mode = input.mode();
    let outcome = match input {
        ShareTransitionWorkerInput::ApplyTarget {
            create_request,
            owner_secret,
        } => apply_target(&backend, &prepared, create_request.as_ref(), owner_secret).await,
        ShareTransitionWorkerInput::PrepareKey {
            services,
            audience,
            previous_generation,
        } => prepare_key(&prepared, services, audience, previous_generation).await,
        ShareTransitionWorkerInput::ClaimGeneration { prepared } => {
            crate::runtime::access_effect_worker::claim_share_key_generation(&backend, prepared)
                .await
                .map(ShareTransitionWorkerOutcome::GenerationClaimed)
        }
        ShareTransitionWorkerInput::PublishBlobs { claimed } => {
            crate::runtime::access_effect_worker::publish_share_key_distribution(&backend, claimed)
                .await
                .map(ShareTransitionWorkerOutcome::BlobsPublished)
        }
        #[cfg(test)]
        ShareTransitionWorkerInput::Panic { .. } => {
            panic!("test share transition worker panic")
        }
    };
    ShareTransitionCompletion {
        prepared,
        account_epoch,
        mode,
        outcome,
    }
}

async fn apply_target(
    backend: &BackendHttpClient,
    prepared: &PreparedShareTransition,
    create_request: Option<&CreateBackendSessionRequest>,
    owner_secret: Option<Zeroizing<String>>,
) -> Result<ShareTransitionWorkerOutcome> {
    let (detail, source_key_generation) = if let Some(request) = create_request {
        let detail = create_session_reconciled(backend, request).await?;
        let generation = backend
            .fetch_current_key_generation(&detail.id, &detail.incarnation_id)
            .await?;
        (detail, Some(generation))
    } else {
        let backend_session_id = prepared.backend_session_id.as_str();
        let backend_incarnation_id = prepared
            .backend_incarnation_id
            .ok_or(AppError::NoActiveSession)?;
        let detail = crate::runtime::sharing::patch_scope_id_reconciled(
            backend,
            backend_session_id,
            backend_incarnation_id,
            prepared.target.scope,
            prepared.target.room_id.as_deref(),
        )
        .await?;
        (detail, prepared.source_key_generation)
    };
    Ok(ShareTransitionWorkerOutcome::TargetObserved {
        detail,
        owner_secret,
        source_key_generation,
    })
}

async fn prepare_key(
    prepared: &PreparedShareTransition,
    services: AccessEffectServices,
    audience: AccessEffectAudience,
    previous_generation: u32,
) -> Result<ShareTransitionWorkerOutcome> {
    let backend_session_id = prepared.backend_session_id.clone();
    let backend_incarnation_id = prepared
        .backend_incarnation_id
        .ok_or(AppError::NoActiveSession)?;
    crate::runtime::access_effect_worker::prepare_share_key_distribution(ShareKeyEffectWork {
        backend_session_id,
        backend_incarnation_id,
        previous_generation,
        audience,
        services,
    })
    .await
    .map(ShareTransitionWorkerOutcome::KeyPrepared)
}

async fn create_session_reconciled(
    backend: &BackendHttpClient,
    request: &CreateBackendSessionRequest,
) -> Result<BackendSessionDetail> {
    match backend.create_session(request).await {
        Ok(detail) => validate_create_detail(detail, request),
        Err(first_error) if first_error.is_indeterminate_write() => {
            match backend.create_session(request).await {
                Ok(detail) => validate_create_detail(detail, request),
                Err(second_error) if second_error.is_indeterminate_write() => {
                    let receipt = backend
                        .fetch_session_creation_receipt(&request.id, &request.idempotency_key)
                        .await
                        .map_err(|reconcile_error| AppError::Unsupported {
                            reason: format!(
                                "CREATE for {} remained indeterminate after retry ({second_error}); exact creation-receipt reconciliation failed: {reconcile_error}",
                                request.id
                            ),
                        })?;
                    let detail = backend.fetch_session_detail(&request.id).await.map_err(
                        |reconcile_error| AppError::Unsupported {
                            reason: format!(
                                "CREATE receipt {} for {} was found, but exact session detail failed: {reconcile_error}",
                                request.idempotency_key, request.id
                            ),
                        },
                    )?;
                    if (
                        receipt.incarnation_id,
                        receipt.generation,
                        receipt.protocol_version,
                    ) != (
                        detail.incarnation_id,
                        detail.incarnation_generation,
                        detail.incarnation_protocol_version,
                    ) {
                        return Err(AppError::Unsupported {
                            reason: format!(
                                "CREATE receipt {} for {} does not identify the current session detail incarnation",
                                request.idempotency_key, request.id
                            ),
                        });
                    }
                    validate_create_detail(detail, request)
                }
                Err(error) => Err(error.into()),
            }
        }
        Err(error) => Err(error.into()),
    }
}

fn validate_create_detail(
    detail: BackendSessionDetail,
    request: &CreateBackendSessionRequest,
) -> Result<BackendSessionDetail> {
    let matches = detail.id == request.id
        && !detail.incarnation_id.is_nil()
        && detail.incarnation_generation > 0
        && detail.incarnation_protocol_version == 2
        && detail.title == request.title
        && detail.tool_kind == request.tool_kind
        && detail.scope == request.scope
        && detail.room_id == request.room_id
        && detail.default_access == request.default_access.access_level()
        && detail.status != kodosi_domain::session::SessionState::Stopped;
    if matches {
        Ok(detail)
    } else {
        Err(AppError::Unsupported {
            reason: format!(
                "CREATE response for {} did not match idempotent request {}",
                request.id, request.idempotency_key
            ),
        })
    }
}

pub(crate) fn detail_matches_audience(
    detail: &BackendSessionDetail,
    audience: &ShareAudience,
) -> bool {
    detail.scope == audience.scope && detail.room_id == audience.room_id
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use axum::{
        Json, Router,
        body::Body,
        extract::State,
        http::{Response, StatusCode, header::CONTENT_TYPE},
        routing::{get, post},
    };
    use kodosi_backend_client::{api::CreateBackendSessionRequest, labels::BackendToolKind};
    use kodosi_domain::permissions::{DefaultAudienceAccess, ShareScope};

    use super::create_session_reconciled;

    #[derive(Clone)]
    struct LostCreateState {
        post_calls: Arc<AtomicUsize>,
        receipt_calls: Arc<AtomicUsize>,
        detail_calls: Arc<AtomicUsize>,
        incarnation_id: uuid::Uuid,
    }

    #[tokio::test]
    async fn lost_create_responses_reconcile_from_exact_receipt_and_detail() {
        let state = LostCreateState {
            post_calls: Arc::new(AtomicUsize::new(0)),
            receipt_calls: Arc::new(AtomicUsize::new(0)),
            detail_calls: Arc::new(AtomicUsize::new(0)),
            incarnation_id: uuid::Uuid::now_v7(),
        };
        let router = Router::new()
            .route("/api/sessions", post(lost_create_response))
            .route(
                "/api/sessions/{id}/creation-receipts/{key}",
                get(created_session_receipt),
            )
            .route("/api/sessions/{id}", get(created_session_detail))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener");
        let address = listener.local_addr().expect("listener address");
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.expect("test server");
        });
        let backend = kodosi_backend_client::http_client::BackendHttpClient::new(
            &kodosi_backend_client::config::BackendClientConfig {
                api: Some(format!("http://{address}")),
                ..kodosi_backend_client::config::BackendClientConfig::default()
            },
        )
        .expect("backend client");
        let request = CreateBackendSessionRequest {
            id: "01900000-0000-7000-8000-000000000001".to_owned(),
            idempotency_key: uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000008")
                .expect("UUIDv7"),
            title: "Session".to_owned(),
            scope: ShareScope::MyDevices,
            tool_kind: BackendToolKind::Generic,
            default_access: DefaultAudienceAccess::View,
            owner_secret: zeroize::Zeroizing::new("owner-secret".to_owned()),
            room_id: None,
        };

        let detail = create_session_reconciled(&backend, &request)
            .await
            .expect("session detail should reconcile two lost CREATE responses");

        assert_eq!(detail.incarnation_id, state.incarnation_id);
        assert_eq!(state.post_calls.load(Ordering::SeqCst), 2);
        assert_eq!(state.receipt_calls.load(Ordering::SeqCst), 1);
        assert_eq!(state.detail_calls.load(Ordering::SeqCst), 1);
        server.abort();
    }

    async fn lost_create_response(State(state): State<LostCreateState>) -> Response<Body> {
        state.post_calls.fetch_add(1, Ordering::SeqCst);
        Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from("not-json"))
            .expect("response")
    }

    async fn created_session_receipt(
        State(state): State<LostCreateState>,
    ) -> Json<serde_json::Value> {
        state.receipt_calls.fetch_add(1, Ordering::SeqCst);
        Json(serde_json::json!({
            "sessionId": "01900000-0000-7000-8000-000000000001",
            "createIdempotencyKey": "01900000-0000-7000-8000-000000000008",
            "incarnationId": state.incarnation_id,
            "generation": 1,
            "protocolVersion": 2
        }))
    }

    async fn created_session_detail(
        State(state): State<LostCreateState>,
    ) -> Json<serde_json::Value> {
        state.detail_calls.fetch_add(1, Ordering::SeqCst);
        Json(serde_json::json!({
            "id": "01900000-0000-7000-8000-000000000001",
            "incarnationId": state.incarnation_id,
            "incarnationGeneration": 1,
            "incarnationProtocolVersion": 2,
            "ownerUserId": "01900000-0000-7000-8000-000000000002",
            "title": "Session",
            "toolKind": "Generic",
            "scope": "MyDevices",
            "roomId": null,
            "defaultAccess": "View",
            "effectiveAccess": "Inject",
            "status": "Pending",
            "startedAt": "2026-08-06T01:02:03Z",
            "endedAt": null,
            "lastHeartbeatAt": "2026-08-06T01:02:03Z"
        }))
    }
}
