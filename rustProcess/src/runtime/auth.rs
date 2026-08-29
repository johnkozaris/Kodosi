use kodosi_domain::{auth::AuthState, lifecycle::ConnectionState};
use time::OffsetDateTime;

use kodosi_backend_client::{BackendClientError, api::BackendCompatibility, user_events};
use tokio::sync::mpsc;

use super::discovery::CatalogClearReason;
use super::identity::auth_lifecycle::bind_pin_store_for_subject;
use crate::{
    AppError, Result,
    host_protocol::AuthEvent,
    identity_core::{
        access_token_resolver::AccessTokenResolver,
        device_flow::DeviceFlowResult,
        stored_auth::{
            BackendAccessState, RefreshStoredAuthResult, StoredAuthAccessor, StoredAuthIssue,
        },
        token_store::{PlatformTokenStore, StoredTokens, TokenStore as _},
    },
};
use kodosi_backend_client::auth::BackendAuthProvider;

use super::{BackendReconciliationState, DEFAULT_TOKEN_SUBJECT, Runtime, SESSION_EVENT_CAPACITY};

include!(concat!(env!("OUT_DIR"), "/backend_contract_versions.rs"));

#[derive(Debug, Default)]
pub(crate) struct LogoutReport {
    pub(crate) warnings: Vec<String>,
}

#[tracing::instrument(skip_all, err)]
pub(crate) async fn login(app: &mut Runtime) -> Result<()> {
    if matches!(
        crate::runtime::identity_reset::resolve_pending(app).await?,
        crate::runtime::identity_reset::IdentityResetResolution::Pending(_)
    ) {
        return Err(AppError::Unsupported {
            reason: "identity reset remains pending; retry identity reset before signing in"
                .to_owned(),
        });
    }
    if !app.state.identity.device_flow.is_configured() {
        let message = "auth.issuer not configured — cannot login".to_owned();
        app.state.record_log(message.clone());
        app.state.identity.auth = AuthState::SignedOut;
        app.state.runtime_outbox.queue_auth(AuthEvent::Error {
            operation: "login.start".to_owned(),
            message,
        });
        return Ok(());
    }
    if matches!(
        app.state.identity.auth,
        AuthState::WaitingForApproval { .. }
    ) {
        app.state.record_log("login already in progress".to_owned());
        return Ok(());
    }
    if matches!(app.state.identity.auth, AuthState::Authenticated { .. }) {
        app.state.record_log("already signed in".to_owned());
        return Ok(());
    }

    ensure_backend_compatibility(app).await?;
    if matches!(app.state.identity.auth, AuthState::Finalizing { .. }) {
        app.state.pending_post_login_work = true;
        app.state
            .record_log("retrying Kodosi backend sign-in finalization".to_owned());
        return Ok(());
    }

    app.state
        .record_log("starting device login flow…".to_owned());

    let response = app.state.identity.device_flow.start_authorization().await?;
    let launch_uri = response.launch_uri().to_owned();

    app.state.identity.auth = AuthState::WaitingForApproval {
        user_code: response.user_code.clone(),
        verification_uri: launch_uri.clone(),
    };
    app.state
        .identity
        .device_flow
        .start_poll(&response, &app.shutdown);
    match crate::support::ui::browser::open_url(&launch_uri) {
        Ok(()) => app
            .state
            .record_log("requested browser for device login".to_owned()),
        Err(error) => app
            .state
            .record_log(format!("could not open browser automatically: {error}")),
    }

    app.state.record_log(format!(
        "open {} and enter code: {}",
        launch_uri, response.user_code
    ));

    Ok(())
}

async fn verify_backend_compatibility(app: &Runtime) -> Result<()> {
    if !app.backend.is_configured() {
        return Ok(());
    }
    let compatibility = app.backend.fetch_compatibility().await.map_err(|error| {
        let reason = match error {
            BackendClientError::NotFound
            | BackendClientError::Json(_)
            | BackendClientError::Protocol { .. } => {
                "The configured Kodosi backend is out of date. Rebuild or update it before signing in."
                    .to_owned()
            }
            other => format!("The Kodosi backend is not ready for sign-in: {other}"),
        };
        AppError::Unsupported { reason }
    })?;
    validate_backend_compatibility(compatibility)
}

pub(crate) fn retire_local_collaboration_authority(app: &mut Runtime) {
    let transitioning = app
        .share_transition_in_flight
        .keys()
        .copied()
        .collect::<Vec<_>>();
    for id in transitioning {
        app.cancel_share_transition_for_session(id, "account changed during share transition");
    }
    for (_, task) in app.relay_prepare_tasks.drain() {
        task.abort();
    }
    app.relay_prepare_owners.clear();
    if let Some(task) = app.access_effect_task.take() {
        task.abort();
    }
    app.access_effect_in_flight = None;
    if let Some(task) = app.access_mutation_task.take() {
        task.abort();
    }
    app.access_mutation_in_flight = None;
    if let Some(task) = app.collaboration_teardown_task.take() {
        task.abort();
    }
    app.collaboration_teardown_retry_after = None;
    app.collaboration_teardown_retry_until.clear();
    app.collaboration_teardown_deferred_until.clear();
    app.device_enrollment_retry_after = None;
    app.device_enrollment_satisfied = false;
    app.state.sharing.host_relays.cancel_all_immediate();
    app.semantic_receipts_in_flight.clear();
    app.action_results_in_flight.clear();
    let ids = app.state.sharing.shared_sessions.ids().collect::<Vec<_>>();
    for id in ids {
        app.state.unshare_session_locally(id);
    }
    app.last_maintenance_ran = None;
}

fn teardown_account_bound_runtime(app: &mut Runtime, retiring_account_user_id: Option<&str>) {
    let self_device_link_retired = app
        .state
        .identity
        .account_runtimes
        .self_device_link()
        .is_some();
    retire_local_collaboration_authority(app);
    super::identity::account_runtimes::shutdown_account_runtimes(
        &mut app.state.identity.device_flow,
        &mut app.state.identity.account_runtimes,
        &mut app.state.pending_discovery_surfaces,
    );
    app.state.pending_work.clear_pin_work();
    app.room_mailbox_retry.reset();
    super::discovery::clear_remote_catalog_for_account(
        app,
        CatalogClearReason::AccountTeardown,
        retiring_account_user_id,
    );
    if self_device_link_retired {
        app.state.apply_device_link_self_resolved(
            kodosi_domain::device_link::SelfDeviceLinkOutcome::Failed,
        );
    }
}

fn preserve_cleanup_quarantine(
    app: &Runtime,
    fallback: BackendReconciliationState,
) -> BackendReconciliationState {
    if app.collaboration_teardown.durable_state()
        == crate::sharing::collaboration_teardown_obligations::DurableStoreState::Healthy
    {
        fallback
    } else {
        BackendReconciliationState::CleanupQuarantined
    }
}

fn disable_unverified_backend_access(app: &mut Runtime, error: &AppError) {
    app.backend_compatibility_verified = false;
    app.backend_reconciliation =
        preserve_cleanup_quarantine(app, BackendReconciliationState::IdentityPending);
    app.backend.set_access_token(None);
    stop_user_events_stream(app);

    super::discovery::clear_remote_catalog(app, CatalogClearReason::TransientOffline);
    retire_local_collaboration_authority(app);
    app.state.cancel_all_session_relays_immediate();
    app.state.backend_status = ConnectionState::Reconnecting;
    app.state.record_log(format!(
        "remote backend access is disabled until API v{EXPECTED_BACKEND_API_CONTRACT} compatibility is verified: {error}"
    ));
}

async fn ensure_backend_compatibility(app: &mut Runtime) -> Result<()> {
    if app.backend_compatibility_verified {
        return Ok(());
    }
    if !app.backend.is_configured() {
        return Err(AppError::Unsupported {
            reason: "backend is not configured".to_owned(),
        });
    }

    match verify_backend_compatibility(app).await {
        Ok(()) => {
            app.backend_compatibility_verified = true;
            app.backend_reconciliation =
                preserve_cleanup_quarantine(app, BackendReconciliationState::IdentityPending);
            Ok(())
        }
        Err(error) => {
            disable_unverified_backend_access(app, &error);
            Err(error)
        }
    }
}

fn validate_backend_compatibility(compatibility: BackendCompatibility) -> Result<()> {
    if compatibility.api_contract_version != EXPECTED_BACKEND_API_CONTRACT
        || compatibility.auth_contract_version != EXPECTED_BACKEND_AUTH_CONTRACT
    {
        return Err(AppError::Unsupported {
            reason: format!(
                "Kodosi backend contract mismatch (API {}, auth {}; expected API {}, auth {}). Update the backend or desktop runtime.",
                compatibility.api_contract_version,
                compatibility.auth_contract_version,
                EXPECTED_BACKEND_API_CONTRACT,
                EXPECTED_BACKEND_AUTH_CONTRACT
            ),
        });
    }
    Ok(())
}

#[tracing::instrument(skip_all, err)]
pub(crate) async fn logout(app: &mut Runtime) -> Result<LogoutReport> {
    app.state.identity.device_flow.cancel();

    let mut report = LogoutReport::default();

    let stored_tokens = match app.token_store.load(DEFAULT_TOKEN_SUBJECT) {
        Ok(tokens) => tokens,
        Err(error) => {
            let warning = format!("logout could not read stored tokens before cleanup: {error}");
            app.state.record_log(warning.clone());
            report.warnings.push(warning);
            None
        }
    };
    if matches!(app.state.identity.auth, AuthState::SignedOut) && stored_tokens.is_none() {
        teardown_account_bound_runtime(app, None);
        app.state.record_log("already signed out".to_owned());
        return Ok(report);
    }

    let retiring_account_user_id = app.state.identity.auth.subject_string();
    let prior_subject = app.state.identity.auth.subject();
    advance_account_epoch(app, "logout started")?;
    app.state.identity.auth = AuthState::LoggingOut {
        subject: prior_subject,
    };

    teardown_account_bound_runtime(app, retiring_account_user_id.as_deref());

    let clear_result = app.token_store.clear(DEFAULT_TOKEN_SUBJECT);
    finish_logout_after_token_clear(app, prior_subject, clear_result)?;
    Ok(report)
}

fn advance_account_epoch(app: &mut Runtime, reason: &'static str) -> Result<()> {
    let epoch = app.state.identity.advance_account_epoch()?;
    tracing::debug!(?epoch, reason, "advanced authenticated account epoch");
    Ok(())
}

pub(crate) fn finish_logout_after_token_clear(
    app: &mut Runtime,
    prior_subject: Option<kodosi_domain::ids::UserId>,
    clear_result: Result<()>,
) -> Result<()> {
    app.backend.set_access_token(None);
    app.backend_compatibility_verified = false;
    app.backend_reconciliation = BackendReconciliationState::Idle;
    app.state.backend_status = ConnectionState::Offline;
    if let Err(error) = clear_result {
        app.state.identity.auth = AuthState::Expired {
            subject: prior_subject,
        };
        app.state
            .record_log(format!("logout could not clear stored tokens: {error}"));
        return Err(error);
    }
    app.state.identity.auth = AuthState::SignedOut;
    app.state.identity.current_user_name = None;
    app.state.record_log("signed out".to_owned());
    tracing::info!("signed out");
    Ok(())
}

pub(crate) fn mark_expired_from_backend(
    app: &mut Runtime,
    reason: impl Into<String>,
) -> Result<()> {
    let reason = reason.into();
    let retiring_account_user_id = app.state.identity.auth.subject_string();
    let prior_subject = app.state.identity.auth.subject();
    advance_account_epoch(app, "backend auth expired")?;
    app.state.identity.auth = AuthState::Expiring {
        subject: prior_subject,
    };
    teardown_account_bound_runtime(app, retiring_account_user_id.as_deref());
    app.backend.set_access_token(None);
    app.backend_compatibility_verified = false;
    app.backend_reconciliation = BackendReconciliationState::Idle;
    app.state.backend_status = ConnectionState::Offline;
    app.state.identity.auth = AuthState::Expired {
        subject: prior_subject,
    };
    tracing::warn!(%reason, "auth expired");
    app.state.record_log(reason);
    Ok(())
}

pub(crate) fn fence_account_for_pending_reset(app: &mut Runtime, account_user_id: &str) {
    if !matches!(app.state.identity.auth, AuthState::SignedOut)
        && let Err(error) = advance_account_epoch(app, "identity reset pending")
    {
        app.state.record_log(format!(
            "identity reset pending fence could not advance the account epoch: {error}"
        ));
    }
    teardown_account_bound_runtime(app, Some(account_user_id));
    app.backend.set_access_token(None);
    app.backend_compatibility_verified = false;
    app.backend_reconciliation = BackendReconciliationState::IdentityPending;
    app.state.backend_status = ConnectionState::Offline;
    app.state.identity.auth = AuthState::SignedOut;
    app.state.identity.current_user_name = None;
}

pub(crate) fn set_signed_out(app: &mut Runtime) -> Result<()> {
    if matches!(app.state.identity.auth, AuthState::SignedOut) {
        app.backend.set_access_token(None);
        app.backend_compatibility_verified = false;
        app.backend_reconciliation = BackendReconciliationState::Idle;
        app.state.backend_status = ConnectionState::Offline;
        app.state.identity.current_user_name = None;
        return Ok(());
    }
    let retiring_account_user_id = app.state.identity.auth.subject_string();
    advance_account_epoch(app, "signed-out transition")?;
    teardown_account_bound_runtime(app, retiring_account_user_id.as_deref());
    app.backend.set_access_token(None);
    app.backend_compatibility_verified = false;
    app.backend_reconciliation = BackendReconciliationState::Idle;
    app.state.backend_status = ConnectionState::Offline;
    app.state.identity.auth = AuthState::SignedOut;
    app.state.identity.current_user_name = None;
    Ok(())
}

pub(crate) fn note_reconnecting(app: &mut Runtime, reason: impl Into<String>) {
    let reason = reason.into();
    app.state.backend_status = ConnectionState::Reconnecting;
    tracing::info!(%reason, "backend reconnect pending");
    app.state.record_log(reason);
}

#[allow(
    clippy::unused_async,
    reason = "async shape matches the runtime maintenance tick + one_shot login loop; both await this alongside other async helpers"
)]
pub(crate) async fn drain_device_flow_events(app: &mut Runtime) -> bool {
    let mut completed_login = false;
    for result in app.state.identity.device_flow.drain_results() {
        if !matches!(
            app.state.identity.auth,
            AuthState::WaitingForApproval { .. }
        ) {
            app.state
                .record_log("ignored stale device login result".to_owned());
            continue;
        }

        match result {
            DeviceFlowResult::Success(tokens) => {
                let expires_at =
                    crate::identity_core::device_flow::token_expires_at(tokens.expires_in);
                let stored =
                    StoredTokens::new(tokens.access_token, tokens.refresh_token, expires_at);
                if let Err(error) = app.token_store.save(DEFAULT_TOKEN_SUBJECT, &stored) {
                    app.state
                        .record_log(format!("failed to store login tokens: {error}"));
                    finish_failed_login(
                        app,
                        "Kodosi couldn’t save this sign-in securely. Check Keychain access and try again.",
                    );
                    continue;
                }
                complete_login(app, stored).await;
                completed_login = true;
            }
            DeviceFlowResult::Denied => finish_failed_login(app, "login denied by user"),
            DeviceFlowResult::Expired => {
                finish_failed_login(app, "device code expired — try again");
            }
            DeviceFlowResult::Failed(msg) => {
                finish_failed_login(app, format!("login failed: {msg}"));
            }
        }
    }
    completed_login
}

#[allow(
    clippy::unused_async,
    reason = "kept async for symmetry with finalize_login + the device-flow dispatch site which awaits both"
)]
async fn complete_login(app: &mut Runtime, stored: StoredTokens) {
    let expires_at = stored.expires_at;
    app.backend.set_access_token(Some(stored.access_token));
    app.state.identity.auth = AuthState::Finalizing { expires_at };
    app.state.pending_post_login_work = true;
}

pub(crate) async fn finalize_login(app: &mut Runtime) {
    let finalize_started = std::time::Instant::now();
    let profile_started = std::time::Instant::now();
    let login_message = if app.backend.is_configured() {
        match sync_current_user_profile(app).await {
            Ok(Some(name)) => format!("login successful — signed in as {name}"),
            Ok(None) => "login successful".to_owned(),
            Err(AppError::Unauthorized) => {
                let message = "Sign-in succeeded in the browser, but the Kodosi backend rejected the token. Check the configured issuer, client, and audience.";
                discard_stored_auth(app, message);
                app.state.runtime_outbox.queue_auth(AuthEvent::Error {
                    operation: "login.finalize".to_owned(),
                    message: message.to_owned(),
                });
                return;
            }
            Err(error @ AppError::InvalidBackendData { .. }) => {
                let message = format!(
                    "Sign-in succeeded, but the backend returned an invalid profile: {error}"
                );
                discard_stored_auth(app, &message);
                app.state.runtime_outbox.queue_auth(AuthEvent::Error {
                    operation: "login.finalize".to_owned(),
                    message,
                });
                return;
            }
            Err(error) => {
                tracing::warn!(%error, "login succeeded but backend current user sync failed");
                app.state.backend_status = ConnectionState::Reconnecting;
                let message = format!(
                    "Browser sign-in succeeded, but Kodosi couldn’t finish connecting to the backend: {error}"
                );
                app.state.record_log(message.clone());
                app.state.runtime_outbox.queue_auth(AuthEvent::Error {
                    operation: "login.finalize".to_owned(),
                    message,
                });
                return;
            }
        }
    } else {
        if let AuthState::Finalizing { expires_at } = app.state.identity.auth {
            app.state.identity.auth = AuthState::Authenticated {
                subject: None,
                expires_at,
            };
        }
        "login successful".to_owned()
    };
    tracing::info!(
        elapsed_ms = profile_started.elapsed().as_millis(),
        "auth profile sync completed"
    );
    app.state.record_log(login_message);

    app.drain_queued_pin_resets_before_self_pin_install().await;

    app.backend_reconciliation = if app.collaboration_teardown.durable_state()
        == crate::sharing::collaboration_teardown_obligations::DurableStoreState::Healthy
    {
        BackendReconciliationState::CleanupPending
    } else {
        BackendReconciliationState::CleanupQuarantined
    };
    app.last_maintenance_ran = None;
    tracing::info!(
        elapsed_ms = finalize_started.elapsed().as_millis(),
        "auth finalization completed"
    );
}

fn reconcile_stored_backend_binding(app: &mut Runtime) -> Result<Option<StoredAuthIssue>> {
    let Some(configured_origin) = app.backend.backend_origin() else {
        return Ok(None);
    };
    let tokens = match app.token_store.peek_tokens_read_only(DEFAULT_TOKEN_SUBJECT) {
        Ok(Some(tokens)) => tokens,
        Ok(None) => return Ok(None),
        Err(error) => {
            tracing::warn!(%error, "stored backend binding check deferred to auth storage classification");
            return Ok(None);
        }
    };
    let Some(bound_origin) = tokens.backend_origin.as_deref() else {
        return Ok(None);
    };
    let configured_origin = configured_origin.to_string();
    if bound_origin == configured_origin {
        return Ok(None);
    }
    let issue = StoredAuthIssue::BoundToOtherBackend {
        bound: bound_origin.to_owned(),
        configured: configured_origin,
    };
    set_signed_out(app)?;
    Ok(Some(issue))
}

#[tracing::instrument(skip_all, err)]
pub(crate) async fn restore_auth(app: &mut Runtime) -> Result<()> {
    if let Some(issue) = reconcile_stored_backend_binding(app)? {
        tracing::warn!(reason = %issue, "ignored stored credentials bound to another backend");
        app.state.record_log(issue.to_string());
        return Ok(());
    }
    if !app.state.identity.device_flow.is_configured() {
        app.state.identity.auth = AuthState::SignedOut;
        app.backend.set_access_token(None);
        return Ok(());
    }

    let discovery_client = app.state.identity.device_flow.client().clone();
    drop(tokio::spawn(async move {
        discovery_client.prime_discovery().await;
    }));

    let backend_access = {
        let mut accessor = stored_auth_accessor(app);
        accessor.ensure_backend_access().await?
    };
    if !apply_backend_access_state_and_log(
        app,
        backend_access,
        "could not read stored tokens — starting signed out",
    )? {
        return Ok(());
    }

    if !app.backend.is_configured() {
        app.state.record_log("restored stored sign-in".to_owned());
        return Ok(());
    }

    app.backend_reconciliation =
        preserve_cleanup_quarantine(app, BackendReconciliationState::IdentityPending);
    app.state
        .record_log("restored stored sign-in; backend reconciliation deferred".to_owned());
    Ok(())
}

async fn reconcile_restored_backend_identity(app: &mut Runtime) -> Result<()> {
    if !app.backend_compatibility_verified
        || app.backend_reconciliation != BackendReconciliationState::IdentityPending
    {
        return Ok(());
    }

    let reconciled = match sync_current_user(app).await {
        Ok(Some(name)) => {
            app.state.record_log(format!("restored sign-in for {name}"));
            true
        }
        Ok(None) => {
            app.state.record_log("restored stored sign-in".to_owned());
            true
        }
        Err(AppError::Unauthorized) => match refresh_access_token(app).await? {
            RefreshStoredAuthResult::Refreshed(_) => match sync_current_user(app).await {
                Ok(Some(name)) => {
                    app.state.record_log(format!("restored sign-in for {name}"));
                    true
                }
                Ok(None) => {
                    app.state.record_log("restored stored sign-in".to_owned());
                    true
                }
                Err(error @ AppError::InvalidBackendData { .. }) => {
                    discard_stored_auth(
                        app,
                        format!("stored sign-in returned an invalid backend profile: {error}"),
                    );
                    false
                }
                Err(error) => {
                    app.state.record_log(format!(
                        "restored stored sign-in; current user sync failed: {error}"
                    ));
                    false
                }
            },
            RefreshStoredAuthResult::RequiresLogin(reason) => {
                mark_expired_from_backend(app, reason.to_string())?;
                false
            }
            RefreshStoredAuthResult::TemporarilyUnavailable(reason) => {
                note_reconnecting(
                    app,
                    format!("restored stored sign-in; current user sync deferred: {reason}"),
                );
                false
            }
        },
        Err(error @ AppError::InvalidBackendData { .. }) => {
            discard_stored_auth(
                app,
                format!("stored sign-in returned an invalid backend profile: {error}"),
            );
            false
        }
        Err(error) => {
            tracing::warn!(%error, "restored stored sign-in but current user sync failed");
            app.state.record_log(format!(
                "restored stored sign-in; current user sync failed: {error}"
            ));
            false
        }
    };

    if reconciled {
        app.backend_reconciliation = if app.collaboration_teardown.durable_state()
            == crate::sharing::collaboration_teardown_obligations::DurableStoreState::Healthy
        {
            BackendReconciliationState::CleanupPending
        } else {
            BackendReconciliationState::CleanupQuarantined
        };
        app.last_maintenance_ran = None;
    } else if !app.state.identity.auth.is_authenticated() {
        app.backend_reconciliation = BackendReconciliationState::Idle;
    }
    Ok(())
}

pub(crate) async fn ensure_self_device_link_ready(app: &mut Runtime) -> Result<()> {
    if !ensure_backend_access(app).await? {
        return Err(AppError::Unauthorized);
    }
    match app.backend_reconciliation {
        BackendReconciliationState::Idle => Ok(()),
        BackendReconciliationState::CleanupPending if app.collaboration_cleanup_settled() => Ok(()),
        BackendReconciliationState::IdentityPending
        | BackendReconciliationState::CleanupPending
        | BackendReconciliationState::CleanupQuarantined => {
            app.last_maintenance_ran = None;
            Err(AppError::Unsupported {
                reason: "device link is waiting for collaboration cleanup reconciliation"
                    .to_owned(),
            })
        }
    }
}

pub(crate) fn ensure_collaboration_cleanup_ready(app: &mut Runtime) -> Result<()> {
    if app.collaboration_teardown.durable_state()
        != crate::sharing::collaboration_teardown_obligations::DurableStoreState::Healthy
    {
        app.last_maintenance_ran = None;
        return Err(AppError::Unsupported {
            reason: "remote operations are disabled while collaboration cleanup has a corruption quarantine or is unavailable"
                .to_owned(),
        });
    }
    if matches!(
        app.backend_reconciliation,
        BackendReconciliationState::CleanupPending | BackendReconciliationState::CleanupQuarantined
    ) {
        app.last_maintenance_ran = None;
        return Err(AppError::Unsupported {
            reason: "remote operations are waiting for collaboration cleanup reconciliation"
                .to_owned(),
        });
    }
    Ok(())
}

pub(crate) async fn ensure_remote_operation_access(app: &mut Runtime) -> Result<()> {
    if !ensure_backend_access(app).await? {
        return Err(AppError::Unauthorized);
    }
    if !app.remote_surfaces_ready() {
        app.last_maintenance_ran = None;
        return Err(AppError::Unsupported {
            reason: "remote operations are waiting for collaboration cleanup reconciliation"
                .to_owned(),
        });
    }
    Ok(())
}

pub(crate) async fn ensure_remote_operation_ready(app: &mut Runtime) -> Result<()> {
    ensure_collaboration_cleanup_ready(app)?;
    ensure_remote_operation_access(app).await
}

#[tracing::instrument(skip_all, err)]
pub(crate) async fn ensure_backend_access(app: &mut Runtime) -> Result<bool> {
    let Some(backend_access) = ensure_backend_access_state(app).await? else {
        return Ok(false);
    };
    Ok(matches!(
        backend_access,
        BackendAccessState::Ready { .. } | BackendAccessState::TemporarilyUnavailable { .. }
    ) && app.state.identity.auth.is_authenticated()
        && app.backend_compatibility_verified)
}

#[tracing::instrument(skip_all, err)]
pub(crate) async fn ensure_backend_access_state(
    app: &mut Runtime,
) -> Result<Option<BackendAccessState>> {
    if !app.backend.is_configured() {
        return Ok(None);
    }
    if let Some(issue) = reconcile_stored_backend_binding(app)? {
        return Ok(Some(BackendAccessState::RequiresLogin(issue)));
    }

    let backend_access = {
        let mut accessor = stored_auth_accessor(app);
        accessor.ensure_backend_access().await?
    };
    let has_backend_access = apply_backend_access_state_and_log(
        app,
        backend_access.clone(),
        "keyring unavailable — treating as signed out",
    )?;
    if has_backend_access {
        ensure_backend_compatibility(app).await?;
        reconcile_restored_backend_identity(app).await?;
    }
    Ok(Some(backend_access))
}

#[tracing::instrument(skip_all, err)]
pub(crate) async fn refresh_access_token(app: &mut Runtime) -> Result<RefreshStoredAuthResult> {
    refresh_access_token_after_rejection(app, None).await
}

pub(crate) async fn refresh_access_token_after_rejection(
    app: &mut Runtime,
    revoked_access_token: Option<&str>,
) -> Result<RefreshStoredAuthResult> {
    let refresh_result = {
        let mut accessor = stored_auth_accessor(app);
        accessor.refresh_access_token(revoked_access_token).await?
    };
    if let RefreshStoredAuthResult::Refreshed(tokens) = &refresh_result {
        app.state.identity.auth = AuthState::Authenticated {
            subject: app.state.identity.auth.subject(),
            expires_at: tokens.expires_at,
        };
        app.state.record_log("refreshed stored sign-in".to_owned());
    }
    Ok(refresh_result)
}

#[tracing::instrument(skip_all, err)]
pub(crate) async fn sync_current_user(app: &mut Runtime) -> Result<Option<String>> {
    sync_current_user_inner(app).await
}

async fn sync_current_user_profile(app: &mut Runtime) -> Result<Option<String>> {
    sync_current_user_inner(app).await
}

async fn sync_current_user_inner(app: &mut Runtime) -> Result<Option<String>> {
    if !app.backend.is_configured() {
        return Ok(None);
    }

    let Some(access_token) = app
        .token_store
        .load(DEFAULT_TOKEN_SUBJECT)?
        .map(|tokens| tokens.access_token)
    else {
        return Err(AppError::Unsupported {
            reason: "stored credential generation changed during backend profile reconciliation"
                .to_owned(),
        });
    };
    let mut profile_client = app.backend.clone();
    profile_client.set_access_token(Some(access_token.clone()));
    let profile = profile_client.fetch_current_user().await?;
    let subject = kodosi_domain::ids::UserId::try_from(profile.id.as_str()).map_err(|error| {
        AppError::InvalidBackendData {
            field: "user.id".to_owned(),
            reason: error.to_string(),
        }
    })?;
    let expires_at = match app.state.identity.auth {
        AuthState::Authenticated { expires_at, .. } | AuthState::Finalizing { expires_at } => {
            expires_at
        }
        _ => OffsetDateTime::now_utc(),
    };
    let origin = app
        .backend
        .backend_origin()
        .ok_or_else(|| AppError::Unsupported {
            reason: "backend origin is unavailable during profile reconciliation".to_owned(),
        })?;
    if !app
        .token_store
        .bind_backend_account_if_current(
            DEFAULT_TOKEN_SUBJECT,
            access_token.as_str(),
            &origin.to_string(),
            &subject.to_string(),
        )
        .await?
    {
        return Err(AppError::Unsupported {
            reason: "stored credential generation changed during backend profile reconciliation"
                .to_owned(),
        });
    }
    apply_current_user_profile(app, &profile, subject, expires_at).await
}

pub(crate) async fn apply_current_user_profile(
    app: &mut Runtime,
    profile: &kodosi_backend_client::api::BackendUserProfile,
    subject: kodosi_domain::ids::UserId,
    expires_at: OffsetDateTime,
) -> Result<Option<String>> {
    let prior_subject = app.state.identity.auth.subject();
    let subject_changed = prior_subject != Some(subject);
    if subject_changed {
        advance_account_epoch(app, "authenticated subject changed")?;
        if prior_subject.is_some() {
            let retiring_account_user_id = prior_subject.map(|subject| subject.to_string());
            app.state.identity.auth = AuthState::Expiring {
                subject: prior_subject,
            };
            teardown_account_bound_runtime(app, retiring_account_user_id.as_deref());
            app.state.identity.auth = AuthState::Finalizing { expires_at };
        }
    }
    if let Err(error) =
        bind_pin_store_for_subject(&subject.to_string(), &app.pin_store, &mut app.state.logs).await
    {
        set_signed_out(app)?;
        return Err(error);
    }

    app.state.identity.auth = AuthState::Authenticated {
        subject: Some(subject),
        expires_at,
    };
    app.state.backend_status = ConnectionState::Connected;
    match app.hidden_session_store.load(&subject.to_string()) {
        Ok(hidden) => app.state.discovery.set_hidden_session_ids(hidden),
        Err(error) => {
            app.state
                .record_log(format!("could not load hidden sessions: {error}"));
            app.state
                .discovery
                .set_hidden_session_ids(std::collections::BTreeSet::default());
        }
    }

    let display_name =
        kodosi_domain::user::display_name_or_handle(&profile.display_name, &profile.handle)
            .to_owned();
    app.state.identity.current_user_name = Some(display_name.clone());
    Ok(Some(display_name))
}

fn finish_failed_login(app: &mut Runtime, message: impl Into<String>) {
    let message = message.into();
    if let Err(error) = set_signed_out(app) {
        app.state.record_log(error.to_string());
    }
    app.state.record_log(message.clone());
    app.state.runtime_outbox.queue_auth(AuthEvent::Error {
        operation: "login.start".to_owned(),
        message,
    });
}

fn discard_stored_auth(app: &mut Runtime, reason: impl Into<String>) {
    let reason = reason.into();
    if let Err(error) = app.token_store.clear(DEFAULT_TOKEN_SUBJECT) {
        app.state
            .record_log(format!("failed to clear stored tokens: {error}"));
    }
    if let Err(error) = set_signed_out(app) {
        app.state.record_log(error.to_string());
    }
    tracing::warn!(%reason, "discarded stored sign-in");
    app.state.record_log(reason);
}

fn apply_backend_access_state_and_log(
    app: &mut Runtime,
    backend_access: BackendAccessState,
    storage_unavailable_log: &'static str,
) -> Result<bool> {
    let has_access = match backend_access {
        BackendAccessState::SignedOut => {
            set_signed_out(app)?;
            false
        }

        BackendAccessState::StorageUnavailable(reason) => {
            tracing::warn!(reason = %reason, "{storage_unavailable_log}");

            set_signed_out(app)?;
            false
        }
        BackendAccessState::RequiresLogin(reason) => {
            mark_expired_from_backend(app, reason.to_string())?;
            false
        }
        BackendAccessState::Ready { tokens, .. } => {
            app.state.identity.auth = AuthState::Authenticated {
                subject: app.state.identity.auth.subject(),
                expires_at: tokens.expires_at,
            };
            true
        }
        BackendAccessState::TemporarilyUnavailable { tokens, reason } => {
            app.state.identity.auth = AuthState::Authenticated {
                subject: app.state.identity.auth.subject(),
                expires_at: tokens.expires_at,
            };
            note_reconnecting(app, reason.to_string());
            true
        }
    };
    Ok(has_access)
}

pub(crate) fn ensure_user_events_stream(app: &mut Runtime) {
    if !app.remote_surfaces_ready() || !app.backend.is_configured() {
        stop_user_events_stream(app);
        return;
    }

    if app.state.identity.account_runtimes.user_events_healthy() {
        return;
    }

    stop_user_events_stream(app);

    let Some(origin) = app.state.identity.current_account_event_origin() else {
        tracing::warn!("not starting discovery event stream without an authenticated subject");
        return;
    };
    let subject = match kodosi_domain::ids::UserId::try_from(origin.account_user_id.as_str()) {
        Ok(subject) => subject,
        Err(error) => {
            tracing::warn!(%error, "authenticated subject was invalid for discovery event stream");
            return;
        }
    };
    let device_keys = match app.device_key_store.load_if_present(&subject.to_string()) {
        Ok(Some(keys)) if !keys.device_id.is_empty() => keys,
        Ok(_) => {
            tracing::debug!("not starting discovery event stream before device enrollment");
            return;
        }
        Err(error) => {
            tracing::warn!(%error, "could not load device identity for discovery event stream");
            return;
        }
    };

    tracing::debug!("starting discovery event stream");
    let cancellation = app.shutdown.child_token();
    let (user_event_tx, user_event_rx) = mpsc::channel(SESSION_EVENT_CAPACITY);
    drop(crate::discovery::user_events_bridge_spawn(
        origin,
        user_event_rx,
        app.session_events_tx.clone(),
        cancellation.clone(),
    ));
    let handle = user_events::spawn(
        app.user_events_ws.clone(),
        backend_auth_provider(app),
        device_keys.device_id.clone(),
        subject.to_string(),
        zeroize::Zeroizing::new(device_keys.signing_pkcs8_bytes().to_vec()),
        user_event_tx,
        cancellation,
    );
    app.state
        .identity
        .account_runtimes
        .attach_user_events(handle);
}

pub(crate) fn stop_user_events_stream(app: &mut Runtime) {
    if app.state.identity.account_runtimes.stop_user_events() {
        tracing::debug!("stopping discovery event stream");
    }
    app.state.clear_pending_discovery_refresh();
}

fn stored_auth_accessor(app: &mut Runtime) -> StoredAuthAccessor<'_, PlatformTokenStore> {
    StoredAuthAccessor::new(
        &mut app.backend,
        app.state.identity.device_flow.client(),
        &app.token_store,
        DEFAULT_TOKEN_SUBJECT,
        app.state.config.auth_refresh_skew_minutes,
    )
}

fn access_token_resolver(app: &Runtime) -> AccessTokenResolver {
    AccessTokenResolver::new(
        app.state.identity.device_flow.client().clone(),
        app.token_store.clone(),
        DEFAULT_TOKEN_SUBJECT.to_owned(),
        app.state.config.auth_refresh_skew_minutes,
    )
}

pub(crate) fn backend_auth_provider(app: &Runtime) -> BackendAuthProvider {
    super::identity::backend_access::IdentityBackendAccess::provider(access_token_resolver(app))
}

#[cfg(test)]
mod compatibility_tests {
    use kodosi_backend_client::api::BackendCompatibility;

    use super::{
        EXPECTED_BACKEND_API_CONTRACT, EXPECTED_BACKEND_AUTH_CONTRACT,
        validate_backend_compatibility,
    };

    #[test]
    fn accepts_current_backend_contract() {
        validate_backend_compatibility(BackendCompatibility {
            api_contract_version: EXPECTED_BACKEND_API_CONTRACT,
            auth_contract_version: EXPECTED_BACKEND_AUTH_CONTRACT,
        })
        .expect("current backend contract");
    }

    #[test]
    fn rejects_v2_backend_without_incarnation_fenced_key_mutations() {
        let error = validate_backend_compatibility(BackendCompatibility {
            api_contract_version: 2,
            auth_contract_version: 1,
        })
        .expect_err("v2 backend must not accept incarnation-fenced key clients");
        assert!(error.to_string().contains("contract mismatch"));
    }
}
