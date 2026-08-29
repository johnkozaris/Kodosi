use crate::{
    AppError, Result,
    session_runtime::events::AccountEventOrigin,
    sharing::{
        backend_adapters::{BackendSessionDetail, title_update_request},
        scope::SelectedRoom,
    },
};
use kodosi_backend_client::{
    BackendOrigin, api::BackendAccessGrant, http_client::BackendHttpClient, labels,
};
use kodosi_domain::{
    ids::SessionId,
    permissions::{AccessLevel, ShareScope},
    session::{SessionRole, SessionState},
};
use uuid::Uuid;

use super::Runtime;

pub(crate) async fn prepare_backend_session_mutation(app: &mut Runtime) -> Result<()> {
    if !app.backend.is_configured() {
        return Err(AppError::Unsupported {
            reason: "backend.api is not configured — cannot share sessions".to_owned(),
        });
    }

    crate::runtime::auth::ensure_remote_operation_ready(app).await?;

    Ok(())
}

pub(super) fn resolve_local_shared_backend_session_identity(
    app: &Runtime,
    id: SessionId,
) -> Result<(String, Uuid)> {
    app.state
        .sharing
        .shared_sessions
        .get(id)
        .map(|shared| {
            (
                shared.backend_session_id().to_owned(),
                *shared.backend_incarnation_id(),
            )
        })
        .ok_or(AppError::NoActiveSession)
}

pub(crate) fn access_grant_entry_from_dto(dto: BackendAccessGrant) -> crate::AccessGrantEntry {
    crate::AccessGrantEntry {
        actor_user_id: dto.actor_user_id,
        handle: dto.handle,
        display_name: dto.display_name,
        access_level: dto.access_level,
        granted_at: dto.granted_at,
        expires_at: dto.expires_at,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TitleMutationSessionFence {
    Local {
        local_incarnation_id: Uuid,
        backend_session_id: String,
        backend_incarnation_id: Uuid,
    },
    Remote {
        incarnation_id: Uuid,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TitleMutationFence {
    account_origin: AccountEventOrigin,
    backend_origin: BackendOrigin,
    session: TitleMutationSessionFence,
}

impl TitleMutationFence {
    fn capture(app: &Runtime, id: SessionId) -> Result<Self> {
        let account_origin = app
            .state
            .identity
            .current_account_event_origin()
            .ok_or(AppError::Unauthorized)?;
        let backend_origin =
            app.backend
                .backend_origin()
                .cloned()
                .ok_or_else(|| AppError::Unsupported {
                    reason: "backend.api is not configured — cannot share sessions".to_owned(),
                })?;
        let session = if let Some(record) = app.state.local.sessions.record(id) {
            let shared = app
                .state
                .sharing
                .shared_sessions
                .get(id)
                .ok_or(AppError::NoActiveSession)?;
            TitleMutationSessionFence::Local {
                local_incarnation_id: record.local_incarnation_id,
                backend_session_id: shared.backend_session_id().to_owned(),
                backend_incarnation_id: *shared.backend_incarnation_id(),
            }
        } else {
            let record = crate::runtime::remote_sessions::owned_remote_record(app, id)
                .ok_or(AppError::NoActiveSession)?;
            TitleMutationSessionFence::Remote {
                incarnation_id: record.incarnation_id.ok_or_else(|| AppError::Unsupported {
                    reason: "session detail is unavailable; refresh before mutating the session"
                        .to_owned(),
                })?,
            }
        };
        Ok(Self {
            account_origin,
            backend_origin,
            session,
        })
    }

    fn backend_identity(&self, id: SessionId) -> (String, Uuid) {
        match &self.session {
            TitleMutationSessionFence::Local {
                backend_session_id,
                backend_incarnation_id,
                ..
            } => (backend_session_id.clone(), *backend_incarnation_id),
            TitleMutationSessionFence::Remote { incarnation_id } => {
                (id.to_string(), *incarnation_id)
            }
        }
    }

    fn matches(&self, app: &Runtime, id: SessionId) -> bool {
        if !app
            .state
            .identity
            .accepts_account_event(&self.account_origin)
            || app.backend.backend_origin() != Some(&self.backend_origin)
        {
            return false;
        }
        match &self.session {
            TitleMutationSessionFence::Local {
                local_incarnation_id,
                backend_session_id,
                backend_incarnation_id,
            } => {
                app.state
                    .local
                    .sessions
                    .record(id)
                    .is_some_and(|record| record.local_incarnation_id == *local_incarnation_id)
                    && app
                        .state
                        .sharing
                        .shared_sessions
                        .get(id)
                        .is_some_and(|shared| {
                            shared.backend_session_id() == backend_session_id
                                && shared.backend_incarnation_id() == backend_incarnation_id
                        })
            }
            TitleMutationSessionFence::Remote { incarnation_id } => {
                crate::runtime::remote_sessions::owned_remote_record(app, id)
                    .is_some_and(|record| record.incarnation_id == Some(*incarnation_id))
            }
        }
    }

    fn is_remote(&self) -> bool {
        matches!(self.session, TitleMutationSessionFence::Remote { .. })
    }
}

pub(crate) async fn update_backend_session_title(
    app: &mut Runtime,
    id: SessionId,
    title: &str,
) -> Result<()> {
    let fence = TitleMutationFence::capture(app, id)?;
    prepare_backend_session_mutation(app).await?;
    if !fence.matches(app, id) {
        return Err(AppError::NoActiveSession);
    }
    let (backend_session_id, incarnation_id) = fence.backend_identity(id);
    let detail =
        patch_title_reconciled(&app.backend, &backend_session_id, incarnation_id, title).await?;
    let detail = BackendSessionDetail::from(detail);
    if !fence.matches(app, id) {
        return Err(AppError::NoActiveSession);
    }

    if fence.is_remote() {
        apply_remote_session_detail(app, id, &detail, None);
        app.state.record_log(format!(
            "renamed remote backend session {} -> {}",
            detail.id, detail.title
        ));
    } else {
        app.state.record_log(format!(
            "renamed backend session {} -> {}",
            detail.id, detail.title
        ));
    }
    Ok(())
}

pub(super) async fn update_backend_session_scope(
    app: &mut Runtime,
    backend_session_id: &str,
    expected_incarnation_id: Uuid,
    scope: ShareScope,
    room: Option<&SelectedRoom>,
) -> Result<BackendSessionDetail> {
    prepare_backend_session_mutation(app).await?;
    let detail = patch_scope_reconciled(
        &app.backend,
        backend_session_id,
        expected_incarnation_id,
        scope,
        room,
    )
    .await?;
    Ok(BackendSessionDetail::from(detail))
}

async fn patch_title_reconciled(
    backend: &BackendHttpClient,
    backend_session_id: &str,
    expected_incarnation_id: Uuid,
    title: &str,
) -> Result<kodosi_backend_client::api::BackendSessionDetail> {
    let request = title_update_request(title, expected_incarnation_id);
    match backend.update_session(backend_session_id, &request).await {
        Ok(detail) => {
            validate_title_detail(detail, backend_session_id, expected_incarnation_id, title)
        }
        Err(first_error) if first_error.is_indeterminate_write() => {
            match backend.update_session(backend_session_id, &request).await {
                Ok(detail) => validate_title_detail(
                    detail,
                    backend_session_id,
                    expected_incarnation_id,
                    title,
                ),
                Err(second_error) if second_error.is_indeterminate_write() => {
                    match backend.fetch_session_detail(backend_session_id).await {
                        Ok(detail) => validate_title_detail(
                            detail,
                            backend_session_id,
                            expected_incarnation_id,
                            title,
                        ),
                        Err(reconcile_error) => Err(AppError::Unsupported {
                            reason: format!(
                                "title PATCH for {backend_session_id} remained indeterminate after retry \
                                 ({second_error}); session-detail reconciliation failed: {reconcile_error}"
                            ),
                        }),
                    }
                }
                Err(error) => Err(error.into()),
            }
        }
        Err(error) => Err(error.into()),
    }
}

fn validate_detail_identity(
    detail: &kodosi_backend_client::api::BackendSessionDetail,
    backend_session_id: &str,
    expected_incarnation_id: Uuid,
) -> Result<()> {
    if detail.incarnation_id != expected_incarnation_id {
        return Err(AppError::NotFound);
    }
    if detail.id != backend_session_id {
        return Err(AppError::Unsupported {
            reason: format!(
                "session mutation response named {} instead of {backend_session_id}",
                detail.id
            ),
        });
    }
    Ok(())
}

fn validate_title_detail(
    detail: kodosi_backend_client::api::BackendSessionDetail,
    backend_session_id: &str,
    expected_incarnation_id: Uuid,
    title: &str,
) -> Result<kodosi_backend_client::api::BackendSessionDetail> {
    validate_detail_identity(&detail, backend_session_id, expected_incarnation_id)?;
    if detail.title != title {
        return Err(AppError::Unsupported {
            reason: format!(
                "title PATCH response reported {:?} instead of {:?}",
                detail.title, title
            ),
        });
    }
    Ok(detail)
}

pub(crate) async fn patch_scope_id_reconciled(
    backend: &BackendHttpClient,
    backend_session_id: &str,
    expected_incarnation_id: Uuid,
    scope: ShareScope,
    room_id: Option<&str>,
) -> Result<kodosi_backend_client::api::BackendSessionDetail> {
    let request = crate::sharing::backend_adapters::scope_update_request_for_room_id(
        scope,
        room_id,
        expected_incarnation_id,
    );
    match backend.update_session(backend_session_id, &request).await {
        Ok(detail) => validate_scope_detail_id(
            detail,
            backend_session_id,
            expected_incarnation_id,
            scope,
            room_id,
        ),
        Err(first_error) if first_error.is_indeterminate_write() => {
            match backend.update_session(backend_session_id, &request).await {
                Ok(detail) => validate_scope_detail_id(
                    detail,
                    backend_session_id,
                    expected_incarnation_id,
                    scope,
                    room_id,
                ),
                Err(second_error) if second_error.is_indeterminate_write() => {
                    match backend.fetch_session_detail(backend_session_id).await {
                        Ok(detail) => validate_scope_detail_id(
                            detail,
                            backend_session_id,
                            expected_incarnation_id,
                            scope,
                            room_id,
                        ),
                        Err(reconcile_error) => Err(AppError::Unsupported {
                            reason: format!(
                                "scope PATCH for {backend_session_id} remained indeterminate after retry \
                                 ({second_error}); session-detail reconciliation failed: {reconcile_error}"
                            ),
                        }),
                    }
                }
                Err(error) => Err(error.into()),
            }
        }
        Err(error) => Err(error.into()),
    }
}

pub(crate) async fn patch_scope_reconciled(
    backend: &BackendHttpClient,
    backend_session_id: &str,
    expected_incarnation_id: Uuid,
    scope: ShareScope,
    room: Option<&SelectedRoom>,
) -> Result<kodosi_backend_client::api::BackendSessionDetail> {
    patch_scope_id_reconciled(
        backend,
        backend_session_id,
        expected_incarnation_id,
        scope,
        room.map(|selected| selected.id.as_str()),
    )
    .await
}

fn validate_scope_detail_id(
    detail: kodosi_backend_client::api::BackendSessionDetail,
    backend_session_id: &str,
    expected_incarnation_id: Uuid,
    scope: ShareScope,
    room_id: Option<&str>,
) -> Result<kodosi_backend_client::api::BackendSessionDetail> {
    validate_detail_identity(&detail, backend_session_id, expected_incarnation_id)?;
    if detail.scope != scope || detail.room_id.as_deref() != room_id {
        return Err(AppError::Unsupported {
            reason: format!(
                "scope PATCH response reported {} instead of the requested {}",
                labels::share_scope_label(detail.scope),
                labels::share_scope_label(scope)
            ),
        });
    }
    Ok(detail)
}

pub(super) fn apply_remote_session_detail(
    app: &mut Runtime,
    id: SessionId,
    detail: &BackendSessionDetail,
    selected_room: Option<&SelectedRoom>,
) {
    let room_name = selected_room.map(|room| room.name.clone()).or_else(|| {
        detail
            .room_id
            .as_ref()
            .and_then(|room_id| {
                app.state
                    .available_rooms
                    .iter()
                    .find(|room| room.id == *room_id)
            })
            .map(|room| room.name.clone())
    });
    let Some(record) = crate::runtime::remote_sessions::owned_remote_record_mut(app, id) else {
        return;
    };

    record.summary.title.clone_from(&detail.title);
    record.incarnation_id = Some(detail.incarnation_id);
    record.summary.scope = detail.scope;
    record.summary.access = match record.summary.role {
        SessionRole::Owner => AccessLevel::Inject,
        SessionRole::Viewer => detail.effective_access.unwrap_or(detail.default_access),
    };

    record.summary.state =
        SessionState::merge_local_with_incoming(record.summary.state, detail.status);
    record.room_id.clone_from(&detail.room_id);
    record.summary.room_name = room_name;
    record.summary.last_update = time::OffsetDateTime::now_utc();
}

#[cfg(test)]
mod reconciliation_tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use axum::{
        Json, Router,
        body::Body,
        extract::State,
        http::{Response, StatusCode, header::CONTENT_TYPE},
        response::IntoResponse,
        routing::{get, patch, post},
    };
    use kodosi_backend_client::{config::BackendClientConfig, http_client::BackendHttpClient};
    use kodosi_domain::permissions::ShareScope;
    use tokio::net::TcpListener;

    use super::{
        TitleMutationFence, TitleMutationSessionFence, Uuid, patch_scope_reconciled,
        patch_title_reconciled, validate_title_detail,
    };
    use crate::runtime::sharing::{
        AccessMutationSettlement, AccessMutationTarget, dispatch_access_mutation,
    };
    use crate::{
        config::AppConfig, runtime::Runtime, session_runtime::events::AccountEpoch,
        sharing::shared_session_registry::SharedSessionState,
    };
    use kodosi_domain::{
        auth::AuthState,
        ids::{SessionId, UserId},
        permissions::AccessLevel,
        session::{SessionRole, SessionState, SessionSummary},
        terminal::TerminalSize,
    };
    use tokio_util::sync::CancellationToken;

    #[derive(Clone)]
    struct AccessServerState {
        writes: Arc<AtomicUsize>,
        receipt_found: bool,
        session_id: Uuid,
        incarnation_id: Uuid,
        mutation_id: Uuid,
        actor_user_id: Uuid,
    }

    #[tokio::test]
    async fn delayed_access_receipt_reconciliation_performs_no_write() {
        let state = AccessServerState {
            writes: Arc::new(AtomicUsize::new(0)),
            receipt_found: true,
            session_id: Uuid::now_v7(),
            incarnation_id: Uuid::now_v7(),
            mutation_id: Uuid::now_v7(),
            actor_user_id: Uuid::now_v7(),
        };
        let (client, server) = access_server(state.clone()).await;

        let receipt = client
            .get_session_access_mutation_receipt(
                &state.session_id.to_string(),
                &state.incarnation_id,
                &state.mutation_id,
            )
            .await
            .expect("delayed receipt should resolve by GET");
        crate::runtime::sharing::validate_access_mutation_receipt(
            &receipt,
            &state.session_id.to_string(),
            state.incarnation_id,
            state.mutation_id,
            &AccessMutationTarget::Grant {
                actor_user_id: state.actor_user_id,
                access_level: AccessLevel::Inject,
                expires_at_unix_ms: time::OffsetDateTime::parse(
                    "2026-08-14T00:00:00Z",
                    &time::format_description::well_known::Rfc3339,
                )
                .unwrap()
                .unix_timestamp_nanos()
                .checked_div(1_000_000)
                .and_then(|value| i64::try_from(value).ok())
                .unwrap(),
            },
        )
        .expect("exact receipt target");

        assert_eq!(state.writes.load(Ordering::SeqCst), 0);
        server.abort();
    }

    #[tokio::test]
    async fn indeterminate_access_write_settles_from_exact_receipt() {
        let state = AccessServerState {
            writes: Arc::new(AtomicUsize::new(0)),
            receipt_found: true,
            session_id: Uuid::now_v7(),
            incarnation_id: Uuid::now_v7(),
            mutation_id: Uuid::now_v7(),
            actor_user_id: Uuid::now_v7(),
        };
        let (client, server) = access_server(state.clone()).await;
        let target = AccessMutationTarget::Grant {
            actor_user_id: state.actor_user_id,
            access_level: AccessLevel::Inject,
            expires_at_unix_ms: time::OffsetDateTime::parse(
                "2026-08-14T00:00:00Z",
                &time::format_description::well_known::Rfc3339,
            )
            .unwrap()
            .unix_timestamp_nanos()
            .checked_div(1_000_000)
            .and_then(|value| i64::try_from(value).ok())
            .unwrap(),
        };

        let backend_session_id = state.session_id.to_string();
        let request = kodosi_backend_client::api::GrantBackendAccessRequest {
            expected_incarnation_id: state.incarnation_id,
            mutation_id: state.mutation_id,
            actor_user_id: state.actor_user_id.to_string(),
            access_level: AccessLevel::Inject,
            expires_at: "2026-08-14T00:00:00Z".to_owned(),
        };
        let settlement = dispatch_access_mutation(
            &client,
            &backend_session_id,
            state.incarnation_id,
            state.mutation_id,
            &target,
            || client.grant_access(&backend_session_id, &request),
        )
        .await
        .unwrap();

        assert!(matches!(settlement, AccessMutationSettlement::Applied));
        assert_eq!(state.writes.load(Ordering::SeqCst), 2);
        server.abort();
    }

    #[tokio::test]
    async fn indeterminate_access_write_without_receipt_remains_unknown() {
        let state = AccessServerState {
            writes: Arc::new(AtomicUsize::new(0)),
            receipt_found: false,
            session_id: Uuid::now_v7(),
            incarnation_id: Uuid::now_v7(),
            mutation_id: Uuid::now_v7(),
            actor_user_id: Uuid::now_v7(),
        };
        let (client, server) = access_server(state.clone()).await;
        let target = AccessMutationTarget::Revoke {
            actor_user_id: state.actor_user_id,
        };

        let backend_session_id = state.session_id.to_string();
        let actor_user_id = state.actor_user_id.to_string();
        let settlement = dispatch_access_mutation(
            &client,
            &backend_session_id,
            state.incarnation_id,
            state.mutation_id,
            &target,
            || {
                client.revoke_access(
                    &backend_session_id,
                    &state.incarnation_id,
                    &state.mutation_id,
                    &actor_user_id,
                )
            },
        )
        .await
        .unwrap();

        assert!(matches!(settlement, AccessMutationSettlement::Unknown(_)));
        assert_eq!(state.writes.load(Ordering::SeqCst), 2);
        server.abort();
    }

    async fn access_server(
        state: AccessServerState,
    ) -> (BackendHttpClient, tokio::task::JoinHandle<()>) {
        let app = Router::new()
            .route("/api/sessions/{session}/access", post(access_write))
            .route(
                "/api/sessions/{session}/access/{actor}",
                axum::routing::delete(access_write),
            )
            .route(
                "/api/sessions/{session}/access/mutations/{mutation}",
                get(access_receipt),
            )
            .with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = BackendHttpClient::new(&BackendClientConfig {
            api: Some(format!("http://{address}")),
            ..BackendClientConfig::default()
        })
        .unwrap();
        (client, server)
    }

    async fn access_write(State(state): State<AccessServerState>) -> StatusCode {
        state.writes.fetch_add(1, Ordering::SeqCst);
        StatusCode::INTERNAL_SERVER_ERROR
    }

    async fn access_receipt(
        State(state): State<AccessServerState>,
    ) -> Result<Json<serde_json::Value>, StatusCode> {
        if !state.receipt_found {
            return Err(StatusCode::NOT_FOUND);
        }
        Ok(Json(serde_json::json!({
            "mutationId": state.mutation_id,
            "sessionId": state.session_id,
            "incarnationId": state.incarnation_id,
            "kind": "grant",
            "targetUserId": state.actor_user_id,
            "accessLevel": "Inject",
            "requestedExpiresAt": "2026-08-14T00:00:00Z"
        })))
    }

    #[derive(Clone)]
    struct ScopeServerState {
        patch_calls: Arc<AtomicUsize>,
        incarnation_id: Uuid,
        malformed_patch_responses: usize,
        title: &'static str,
    }

    fn title_fence_app() -> (Runtime, SessionId) {
        let mut config = AppConfig::default();
        config.auth.keyring_service = format!("kodosi.test.{}", Uuid::now_v7());
        config.backend.api = Some("http://127.0.0.1:9".to_owned());
        let mut app = Runtime::with_dependencies(
            config.clone(),
            CancellationToken::new(),
            crate::runtime::RuntimeDependencies::isolated(&config).expect("isolated dependencies"),
        )
        .expect("runtime");
        app.state.identity.auth = AuthState::Authenticated {
            subject: Some(
                UserId::try_from("11111111-1111-1111-1111-111111111111").expect("test user"),
            ),
            expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
        };
        app.state
            .identity
            .set_account_epoch_for_test(AccountEpoch::for_test(7));
        let id = SessionId::new();
        let mut summary = SessionSummary::new_owned(
            id,
            "local".to_owned(),
            "shell".to_owned(),
            TerminalSize::default(),
            app.state.identity.auth.subject(),
        );
        summary.state = SessionState::Running;
        app.state.local.sessions.insert(summary);
        app.state.sharing.shared_sessions.insert(
            id,
            SharedSessionState::new(
                "backend-session".to_owned(),
                Uuid::from_u128(1),
                "secret".to_owned(),
                ShareScope::MyDevices,
                None,
                Some([3; 32]),
                Some(1),
            ),
        );
        (app, id)
    }

    #[test]
    fn title_fence_rejects_local_reopen_and_account_replacement() {
        let (mut app, id) = title_fence_app();
        let fence = TitleMutationFence::capture(&app, id).expect("title fence");
        assert!(fence.matches(&app, id));

        app.state
            .local
            .sessions
            .record_mut(id)
            .expect("local record")
            .local_incarnation_id = Uuid::now_v7();
        assert!(!fence.matches(&app, id), "reopen must retire the old fence");

        let (mut app, id) = title_fence_app();
        let fence = TitleMutationFence::capture(&app, id).expect("title fence");
        app.state
            .identity
            .advance_account_epoch()
            .expect("advance epoch");
        assert!(
            !fence.matches(&app, id),
            "account transition must retire the old fence"
        );
    }

    #[test]
    fn title_fence_rejects_backend_and_remote_incarnation_replacement() {
        let (mut app, id) = title_fence_app();
        let fence = TitleMutationFence::capture(&app, id).expect("local title fence");
        app.state.sharing.shared_sessions.clear(id);
        app.state.sharing.shared_sessions.insert(
            id,
            SharedSessionState::new(
                "replacement-backend".to_owned(),
                Uuid::now_v7(),
                "secret".to_owned(),
                ShareScope::MyDevices,
                None,
                Some([4; 32]),
                Some(1),
            ),
        );
        assert!(!fence.matches(&app, id));

        let (mut app, local_id) = title_fence_app();
        app.state.local.sessions.delete(local_id);
        app.state.sharing.shared_sessions.clear(local_id);
        let remote_id = SessionId::new();
        let mut summary = SessionSummary::new_remote(
            remote_id,
            "remote".to_owned(),
            "You".to_owned(),
            app.state.identity.auth.subject(),
            ShareScope::MyDevices,
            AccessLevel::Inject,
            TerminalSize::default(),
        );
        summary.role = SessionRole::Owner;
        app.state.discovery.replace_remote_sessions(
            vec![crate::discovery::RemoteSessionRecord {
                summary,
                incarnation_id: Some(Uuid::from_u128(10)),
                room_id: None,
                connection_state: None,
                connection_reason: None,
                access_state: None,
                access_reason: None,
                access_issue: None,
                viewer_blocked: false,
                viewer_hidden: false,
            }],
            false,
        );
        let fence = TitleMutationFence::capture(&app, remote_id).expect("remote title fence");
        assert_eq!(
            fence.session,
            TitleMutationSessionFence::Remote {
                incarnation_id: Uuid::from_u128(10)
            }
        );
        app.state
            .discovery
            .session_mut(remote_id)
            .expect("remote record")
            .incarnation_id = Some(Uuid::from_u128(11));
        assert!(!fence.matches(&app, remote_id));
    }

    #[tokio::test]
    async fn lost_scope_patch_response_retries_idempotently() {
        let state = ScopeServerState {
            patch_calls: Arc::new(AtomicUsize::new(0)),
            incarnation_id: Uuid::from_u128(1),
            malformed_patch_responses: 1,
            title: "Session",
        };
        let app = Router::new()
            .route("/api/sessions/{id}", patch(scope_patch).get(session_detail))
            .with_state(state.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = BackendHttpClient::new(&BackendClientConfig {
            api: Some(format!("http://{address}")),
            ..BackendClientConfig::default()
        })
        .unwrap();

        let detail = patch_scope_reconciled(
            &client,
            "01900000-0000-7000-8000-000000000001",
            state.incarnation_id,
            ShareScope::MyDevices,
            None,
        )
        .await
        .expect("the idempotent retry should reconcile the committed PATCH");

        assert_eq!(detail.scope, ShareScope::MyDevices);
        assert_eq!(state.patch_calls.load(Ordering::SeqCst), 2);
        server.abort();
    }

    #[tokio::test]
    async fn lost_title_patch_responses_reconcile_from_exact_detail() {
        let state = ScopeServerState {
            patch_calls: Arc::new(AtomicUsize::new(0)),
            incarnation_id: Uuid::from_u128(1),
            malformed_patch_responses: 2,
            title: "Renamed",
        };
        let app = Router::new()
            .route("/api/sessions/{id}", patch(scope_patch).get(session_detail))
            .with_state(state.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = BackendHttpClient::new(&BackendClientConfig {
            api: Some(format!("http://{address}")),
            ..BackendClientConfig::default()
        })
        .unwrap();

        let detail = patch_title_reconciled(
            &client,
            "01900000-0000-7000-8000-000000000001",
            state.incarnation_id,
            "Renamed",
        )
        .await
        .expect("GET should confirm committed title after two lost responses");

        assert_eq!(detail.title, "Renamed");
        assert_eq!(state.patch_calls.load(Ordering::SeqCst), 2);
        server.abort();
    }

    #[test]
    fn title_detail_rejects_wrong_identity_or_title() {
        let incarnation_id = Uuid::from_u128(1);
        let detail = |id: &str, incarnation_id: Uuid, title: &str| {
            serde_json::from_value::<kodosi_backend_client::api::BackendSessionDetail>(
                serde_json::json!({
                    "id": id,
                    "incarnationId": incarnation_id,
                    "incarnationGeneration": 1,
                    "incarnationProtocolVersion": 2,
                    "ownerUserId": "01900000-0000-7000-8000-000000000002",
                    "title": title,
                    "toolKind": "Generic",
                    "scope": "MyDevices",
                    "roomId": null,
                    "defaultAccess": "View",
                    "effectiveAccess": "Inject",
                    "status": "Pending",
                    "startedAt": "2026-08-06T01:02:03Z",
                    "endedAt": null,
                    "lastHeartbeatAt": "2026-08-06T01:02:03Z"
                }),
            )
            .expect("session detail")
        };
        let expected_id = "01900000-0000-7000-8000-000000000001";

        assert!(
            validate_title_detail(
                detail(
                    "01900000-0000-7000-8000-000000000099",
                    incarnation_id,
                    "Renamed"
                ),
                expected_id,
                incarnation_id,
                "Renamed",
            )
            .is_err()
        );
        std::assert_matches!(
            validate_title_detail(
                detail(expected_id, Uuid::from_u128(2), "Renamed"),
                expected_id,
                incarnation_id,
                "Renamed",
            ),
            Err(crate::AppError::NotFound)
        );
        assert!(
            validate_title_detail(
                detail(expected_id, incarnation_id, "Other"),
                expected_id,
                incarnation_id,
                "Renamed",
            )
            .is_err()
        );
    }

    async fn scope_patch(State(state): State<ScopeServerState>) -> Response<Body> {
        let call = state.patch_calls.fetch_add(1, Ordering::SeqCst);
        if call < state.malformed_patch_responses {
            return Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from("not-json"))
                .unwrap();
        }
        session_detail(State(state)).await.into_response()
    }

    async fn session_detail(State(state): State<ScopeServerState>) -> Json<serde_json::Value> {
        Json(serde_json::json!({
            "id": "01900000-0000-7000-8000-000000000001",
            "incarnationId": state.incarnation_id,
            "incarnationGeneration": 1,
            "incarnationProtocolVersion": 2,
            "ownerUserId": "01900000-0000-7000-8000-000000000002",
            "title": state.title,
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
