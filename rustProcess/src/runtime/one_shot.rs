use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::{
    AppError, Result, config::AppConfig, host_protocol::AuthEvent,
    identity_core::stored_auth::BackendAccessState, session_runtime::events::RuntimeSessionEvent,
};
use kodosi_backend_client::api::{BackendRoom, BackendSessionCard, BackendUserProfile};
use kodosi_domain::auth::AuthState;

use super::identity::{DeviceLinkApprovalOutcome, DeviceRevocationOutcome, SelfDeviceLinkStart};
use super::{Runtime, rooms::RoomApplication};

const BACKEND_API_CONFIG_KEY: &str = "backend.api";
const AUTH_ISSUER_CONFIG_KEY: &str = "auth.issuer";

pub(crate) struct OneShotApp {
    app: Runtime,
    runtime_authority: Option<crate::support::storage::runtime_lock::RuntimeAuthorityLock>,
    #[cfg(test)]
    runtime_authority_for_test: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct DeviceLoginPrompt {
    pub(crate) user_code: String,
    pub(crate) verification_uri: String,
}

#[derive(Debug, Clone)]
pub(crate) struct LoginSummary {
    pub(crate) user: Option<BackendUserProfile>,
    pub(crate) rooms: Vec<BackendRoom>,
    pub(crate) owned_remote_sessions: Vec<BackendSessionCard>,
}

#[derive(Debug, Clone)]
pub(crate) enum LoginStart {
    AlreadyReady(LoginSummary),
    Pending(DeviceLoginPrompt),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OneShotBackendAccess {
    Ready,
    RequiresLogin(String),
    SignedOut,
    StorageUnavailable(String),
    CollaborationQuarantined(String),
    BackendUnconfigured,
}

impl OneShotApp {
    pub(crate) fn load() -> Result<Self> {
        let config = AppConfig::load(None)?;
        let runtime_authority =
            crate::support::storage::runtime_lock::RuntimeAuthorityLock::try_acquire()?;
        let app = Runtime::new(config, CancellationToken::new())?;
        Ok(Self {
            app,
            runtime_authority,
            #[cfg(test)]
            runtime_authority_for_test: false,
        })
    }

    pub(crate) fn reset_share_transition_ledger(&mut self) -> Result<Vec<std::path::PathBuf>> {
        if !self.owns_runtime_authority() {
            return Err(AppError::Unsupported {
                reason: "another Kodosi runtime owns this data root; stop it before resetting share transition evidence"
                    .to_owned(),
            });
        }
        self.app.reset_share_transition_ledger()
    }

    pub(crate) fn reset_collaboration_cleanup_quarantine(&mut self) -> Result<usize> {
        if !self.owns_runtime_authority() {
            return Err(AppError::Unsupported {
                reason: "another Kodosi runtime owns this data root; stop it before resetting collaboration cleanup quarantine"
                    .to_owned(),
            });
        }
        self.app.reset_collaboration_cleanup_quarantine()
    }

    pub(crate) fn current_user_id_via_auth(&self) -> Option<kodosi_domain::ids::UserId> {
        self.app.state.identity.auth.subject()
    }

    pub(crate) const fn runtime(&self) -> &Runtime {
        &self.app
    }

    pub(crate) fn backend_api_configured(&self) -> bool {
        self.app.backend.is_configured()
    }

    pub(crate) fn auth_issuer_configured(&self) -> bool {
        self.app.state.identity.device_flow.is_configured()
    }

    pub(crate) fn owns_runtime_authority_for_cli(&self) -> bool {
        self.owns_runtime_authority()
    }

    fn owns_runtime_authority(&self) -> bool {
        if self.runtime_authority.is_some() {
            return true;
        }
        #[cfg(test)]
        {
            self.runtime_authority_for_test
        }
        #[cfg(not(test))]
        {
            false
        }
    }

    pub(crate) fn stored_backend_account(
        &self,
    ) -> Result<
        Option<(
            kodosi_backend_client::BackendOrigin,
            kodosi_domain::ids::UserId,
        )>,
    > {
        let Some(account) = self
            .app
            .token_store
            .peek_backend_account(super::DEFAULT_TOKEN_SUBJECT)?
        else {
            return Ok(None);
        };
        let origin: kodosi_backend_client::BackendOrigin = account.backend_origin.parse().map_err(
            |error: kodosi_backend_client::BackendClientError| AppError::InvalidBackendData {
                field: "storedTokens.backendOrigin".to_owned(),
                reason: error.to_string(),
            },
        )?;
        let user_id = kodosi_domain::ids::UserId::try_from(account.backend_user_id.as_str())
            .map_err(|error| AppError::InvalidBackendData {
                field: "storedTokens.backendUserId".to_owned(),
                reason: error.to_string(),
            })?;
        Ok(Some((origin, user_id)))
    }

    pub(crate) fn configured_backend_origin(
        &self,
    ) -> Option<&kodosi_backend_client::BackendOrigin> {
        self.app.backend.backend_origin()
    }

    pub(crate) async fn prepare_host_backed_projection(
        &mut self,
        expected_origin: &kodosi_backend_client::BackendOrigin,
        expected_user_id: kodosi_domain::ids::UserId,
    ) -> Result<()> {
        let tokens = self
            .app
            .token_store
            .peek_tokens_read_only(super::DEFAULT_TOKEN_SUBJECT)?
            .ok_or(AppError::Unauthorized)?;
        if tokens.backend_origin.as_deref() != Some(expected_origin.to_string().as_str())
            || tokens.backend_user_id.as_deref() != Some(expected_user_id.to_string().as_str())
        {
            return Err(AppError::Unsupported {
                reason: "shared credentials no longer match the running host account".to_owned(),
            });
        }
        self.app
            .backend
            .set_access_token(Some(tokens.access_token.clone()));
        self.app.state.identity.auth = AuthState::Authenticated {
            subject: Some(expected_user_id),
            expires_at: tokens.expires_at,
        };
        self.app
            .pin_store
            .bind_to_user(&expected_user_id.to_string())
            .await?;
        Ok(())
    }

    async fn resolve_pending_identity_reset(&mut self) -> Result<()> {
        if !self.owns_runtime_authority() {
            return Ok(());
        }
        match crate::runtime::identity_reset::resolve_pending(&mut self.app).await? {
            crate::runtime::identity_reset::IdentityResetResolution::Clear => Ok(()),
            crate::runtime::identity_reset::IdentityResetResolution::Pending(reason) => {
                Err(AppError::Unsupported { reason })
            }
        }
    }

    pub(crate) async fn backend_access(&mut self) -> Result<OneShotBackendAccess> {
        self.resolve_pending_identity_reset().await?;
        let Some(access_state) =
            crate::runtime::auth::ensure_backend_access_state(&mut self.app).await?
        else {
            return Ok(OneShotBackendAccess::BackendUnconfigured);
        };
        Ok(classify_backend_access(access_state))
    }

    pub(crate) async fn remote_command_access(&mut self) -> Result<OneShotBackendAccess> {
        if !self.owns_runtime_authority() {
            return Err(AppError::Unsupported {
                reason: "another Kodosi runtime owns this data root; remote work must run through that runtime"
                    .to_owned(),
            });
        }
        let cleanup_health = self.app.collaboration_cleanup_health();
        if cleanup_health.state != crate::CollaborationCleanupState::Healthy {
            return Ok(OneShotBackendAccess::CollaborationQuarantined(
                cleanup_health.message.unwrap_or_else(|| {
                    "Collaboration cleanup is degraded; remote commands are disabled.".to_owned()
                }),
            ));
        }
        let access = self.backend_access().await?;
        if access == OneShotBackendAccess::Ready && self.is_authenticated() {
            self.settle_initial_collaboration_cleanup().await?;
        }
        Ok(access)
    }

    pub(crate) async fn restore_local_command_auth(&mut self) -> Result<bool> {
        self.resolve_pending_identity_reset().await?;
        crate::runtime::auth::restore_auth(&mut self.app).await?;
        Ok(self.is_authenticated())
    }

    pub(crate) async fn fetch_current_user(&self) -> Result<BackendUserProfile> {
        self.app
            .backend
            .fetch_current_user()
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn fetch_my_sessions(&self) -> Result<Vec<BackendSessionCard>> {
        self.app
            .backend
            .fetch_my_sessions()
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn verified_current_identity(
        &self,
        user_id: &str,
    ) -> Result<crate::identity_core::device_list_pin_store::IdentityBundleView> {
        let local_keys = super::identity::DeviceKeyAccess::new(
            &self.app.state.identity.auth,
            &self.app.device_key_store,
        )
        .load_authenticated_device_keys()?;
        let bundle = self.app.backend.fetch_user_identity(user_id).await?;
        super::identity::device_keys::verify_own_identity_bundle(
            &self.app.state.identity.auth,
            &local_keys,
            &self.app.pin_store,
            &bundle,
        )
        .await
    }

    pub(crate) async fn start_login(&mut self) -> Result<LoginStart> {
        if !self.owns_runtime_authority() {
            return Err(AppError::Unsupported {
                reason: "another Kodosi runtime owns this data root; login must run through that runtime"
                    .to_owned(),
            });
        }
        self.resolve_pending_identity_reset().await?;
        crate::runtime::auth::restore_auth(&mut self.app).await?;
        if !self.app.backend.is_configured() {
            return Err(AppError::Unsupported {
                reason: format!("{BACKEND_API_CONFIG_KEY} is not configured — cannot login"),
            });
        }
        if self.is_authenticated() {
            if !crate::runtime::auth::ensure_backend_access(&mut self.app).await? {
                return Err(AppError::Unsupported {
                    reason: "stored sign-in does not currently provide verified backend access"
                        .to_owned(),
                });
            }
            self.settle_initial_collaboration_cleanup().await?;
            return Ok(LoginStart::AlreadyReady(self.login_summary().await));
        }
        if !self.app.state.identity.device_flow.is_configured() {
            return Err(AppError::Unsupported {
                reason: format!("{AUTH_ISSUER_CONFIG_KEY} is not configured — cannot login"),
            });
        }

        crate::runtime::auth::login(&mut self.app).await?;
        match &self.app.state.identity.auth {
            AuthState::WaitingForApproval {
                user_code,
                verification_uri,
            } => Ok(LoginStart::Pending(DeviceLoginPrompt {
                user_code: user_code.clone(),
                verification_uri: verification_uri.clone(),
            })),
            AuthState::Authenticated { .. } => {
                self.settle_initial_collaboration_cleanup().await?;
                Ok(LoginStart::AlreadyReady(self.login_summary().await))
            }
            _ => Err(self
                .take_auth_error()
                .unwrap_or_else(|| AppError::Unsupported {
                    reason: "login did not start".to_owned(),
                })),
        }
    }

    pub(crate) async fn wait_for_login(&mut self) -> Result<LoginSummary> {
        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    self.app.state.identity.device_flow.cancel();
                    return Err(AppError::Unsupported {
                        reason: "login cancelled".to_owned(),
                    });
                }
                () = tokio::time::sleep(Duration::from_millis(200)) => {
                    let completed =
                        crate::runtime::auth::drain_device_flow_events(&mut self.app).await;
                    if completed {
                        crate::runtime::auth::finalize_login(&mut self.app).await;
                    }
                    if self.is_authenticated() {
                        self.settle_initial_collaboration_cleanup().await?;
                        return Ok(self.login_summary().await);
                    }
                    if !matches!(
                        self.app.state.identity.auth,
                        AuthState::WaitingForApproval { .. } | AuthState::Finalizing { .. }
                    ) {
                        return Err(self.take_auth_error().unwrap_or_else(|| {
                            self.last_log_error("login did not complete")
                        }));
                    }
                }
            }
        }
    }

    pub(crate) async fn logout(&mut self) -> Result<()> {
        if !self.owns_runtime_authority() {
            return Err(AppError::Unsupported {
                reason: "another Kodosi runtime owns this data root; logout must run through that runtime"
                    .to_owned(),
            });
        }
        self.resolve_pending_identity_reset().await?;
        crate::runtime::auth::restore_auth(&mut self.app).await?;
        crate::runtime::auth::logout(&mut self.app).await.map(drop)
    }

    pub(crate) async fn revoke_device(
        &mut self,
        target_device_id: &str,
        command_name: &str,
    ) -> Result<DeviceRevocationOutcome> {
        self.ensure_remote_access(command_name).await?;
        let app = &mut self.app;
        super::identity::device_list::DeviceListCtx {
            auth: &app.state.identity.auth,
            backend: &app.backend,
            device_key_store: &app.device_key_store,
            pin_store: &app.pin_store,
            room_roster_pins_path: &app.room_roster_pins_path,
            outbox: &mut app.state.runtime_outbox,
        }
        .revoke(target_device_id)
        .await
    }

    pub(crate) async fn approve_device_link(
        &mut self,
        user_code: &str,
        command_name: &str,
    ) -> Result<DeviceLinkApprovalOutcome> {
        self.ensure_remote_access(command_name).await?;
        let app = &mut self.app;
        let account_origin = app.state.identity.current_account_event_origin();
        super::identity::device_link::DeviceLinkCtx {
            auth: &app.state.identity.auth,
            account_origin,
            backend: &app.backend,
            device_key_store: &app.device_key_store,
            pin_store: &app.pin_store,
            account_runtimes: &mut app.state.identity.account_runtimes,
            device_flow: &mut app.state.identity.device_flow,
            pending_discovery_surfaces: &mut app.state.pending_discovery_surfaces,
            session_events_tx: &app.session_events_tx,
            logs: &mut app.state.logs,
        }
        .approve(user_code)
        .await
    }

    pub(crate) async fn start_link_this_device(
        &mut self,
        label: Option<String>,
        command_name: &str,
    ) -> Result<SelfDeviceLinkStart> {
        self.ensure_self_link_access(command_name).await?;
        let app = &mut self.app;
        let account_origin = app.state.identity.current_account_event_origin();
        super::identity::device_link::DeviceLinkCtx {
            auth: &app.state.identity.auth,
            account_origin,
            backend: &app.backend,
            device_key_store: &app.device_key_store,
            pin_store: &app.pin_store,
            account_runtimes: &mut app.state.identity.account_runtimes,
            device_flow: &mut app.state.identity.device_flow,
            pending_discovery_surfaces: &mut app.state.pending_discovery_surfaces,
            session_events_tx: &app.session_events_tx,
            logs: &mut app.state.logs,
        }
        .start_self_with_label(label)
        .await
    }

    pub(crate) async fn wait_for_self_device_link_approval(&mut self) -> Result<Option<u64>> {
        self.wait_for_self_device_link_resolution().await
    }

    async fn settle_collaboration_cleanup_only(&mut self) -> Result<()> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(35);
        loop {
            if self.app.collaboration_cleanup_settled() {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(AppError::Unsupported {
                    reason:
                        "collaboration cleanup did not settle before the remote command deadline"
                            .to_owned(),
                });
            }
            self.app.process_collaboration_teardown_worker().await;
            if self.app.collaboration_teardown_task.is_some() {
                tokio::time::sleep_until(
                    (tokio::time::Instant::now() + Duration::from_millis(10)).min(deadline),
                )
                .await;
                continue;
            }
            if let Some(retry_after) = self
                .app
                .collaboration_teardown_retry_until
                .values()
                .copied()
                .min()
            {
                tokio::time::sleep_until(tokio::time::Instant::from_std(retry_after).min(deadline))
                    .await;
                continue;
            }
            if let Some(retry_after) = self.app.collaboration_teardown_retry_after {
                tokio::time::sleep_until(tokio::time::Instant::from_std(retry_after).min(deadline))
                    .await;
                continue;
            }
            tokio::task::yield_now().await;
        }
    }

    async fn settle_initial_collaboration_cleanup(&mut self) -> Result<()> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(35);
        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(AppError::Unsupported {
                    reason:
                        "collaboration cleanup did not settle before the remote command deadline"
                            .to_owned(),
                });
            }
            if self.app.remote_surfaces_ready() {
                return Ok(());
            }
            self.app.process_collaboration_teardown_worker().await;
            if self.app.collaboration_teardown_task.is_some() {
                tokio::time::sleep_until(
                    (tokio::time::Instant::now() + Duration::from_millis(10)).min(deadline),
                )
                .await;
                continue;
            }
            if let Some(retry_after) = self
                .app
                .collaboration_teardown_retry_until
                .values()
                .copied()
                .min()
            {
                let retry_after = tokio::time::Instant::from_std(retry_after).min(deadline);
                tokio::time::sleep_until(retry_after).await;
                continue;
            }
            if let Some(retry_after) = self.app.collaboration_teardown_retry_after {
                let retry_after = tokio::time::Instant::from_std(retry_after).min(deadline);
                tokio::time::sleep_until(retry_after).await;
                continue;
            }
            if let Some(retry_after) = self.app.device_enrollment_retry_after {
                let retry_after = tokio::time::Instant::from_std(retry_after).min(deadline);
                tokio::time::sleep_until(retry_after).await;
                continue;
            }
            tokio::time::timeout_at(deadline, self.app.finish_initial_collaboration_cleanup())
                .await
                .map_err(|_| AppError::Unsupported {
                    reason:
                        "collaboration cleanup did not settle before the remote command deadline"
                            .to_owned(),
                })?;
            tokio::task::yield_now().await;
        }
    }

    async fn ensure_self_link_access(&mut self, command_name: &str) -> Result<()> {
        self.ensure_authenticated_backend_access(command_name)
            .await?;
        self.settle_collaboration_cleanup_only().await
    }

    async fn ensure_remote_access(&mut self, command_name: &str) -> Result<()> {
        self.ensure_authenticated_backend_access(command_name)
            .await?;
        self.settle_initial_collaboration_cleanup().await
    }

    async fn ensure_authenticated_backend_access(&mut self, command_name: &str) -> Result<()> {
        if !self.owns_runtime_authority() {
            return Err(AppError::Unsupported {
                reason: format!(
                    "another Kodosi runtime owns this data root; `{command_name}` must run through that runtime"
                ),
            });
        }
        match self.backend_access().await? {
            OneShotBackendAccess::Ready if self.is_authenticated() => {}
            OneShotBackendAccess::BackendUnconfigured => {
                return Err(AppError::Unsupported {
                    reason: format!(
                        "{BACKEND_API_CONFIG_KEY} is not configured — `{command_name}` needs {BACKEND_API_CONFIG_KEY}"
                    ),
                });
            }
            OneShotBackendAccess::RequiresLogin(reason) => {
                return Err(AppError::Unsupported {
                    reason: format!("{reason} — run `kodosi auth login` before `{command_name}`"),
                });
            }
            OneShotBackendAccess::SignedOut | OneShotBackendAccess::Ready => {
                return Err(AppError::Unsupported {
                    reason: format!(
                        "not signed in — run `kodosi auth login` before `{command_name}`"
                    ),
                });
            }
            OneShotBackendAccess::StorageUnavailable(reason)
            | OneShotBackendAccess::CollaborationQuarantined(reason) => {
                return Err(AppError::Unsupported { reason });
            }
        }
        crate::runtime::auth::sync_current_user(&mut self.app).await?;
        Ok(())
    }

    async fn wait_for_self_device_link_resolution(&mut self) -> Result<Option<u64>> {
        let mut cancellation_requested = false;
        loop {
            let event = tokio::select! {
                _ = tokio::signal::ctrl_c(), if !cancellation_requested => {
                    let account_origin = self.app.state.identity.current_account_event_origin();
                    let _requested = super::identity::device_link::DeviceLinkCtx {
                        auth: &self.app.state.identity.auth,
                        account_origin,
                        backend: &self.app.backend,
                        device_key_store: &self.app.device_key_store,
                        pin_store: &self.app.pin_store,
                        account_runtimes: &mut self.app.state.identity.account_runtimes,
                        device_flow: &mut self.app.state.identity.device_flow,
                        pending_discovery_surfaces: &mut self.app.state.pending_discovery_surfaces,
                        session_events_tx: &self.app.session_events_tx,
                        logs: &mut self.app.state.logs,
                    }
                    .cancel_self();
                    cancellation_requested = true;
                    continue;
                }
                event = self.app.next_session_event() => event,
            };

            match event {
                Some(event @ RuntimeSessionEvent::DeviceLinkSelfResolved { outcome, .. }) => {
                    self.app.handle_session_event(event);
                    match outcome {
                        kodosi_domain::device_link::SelfDeviceLinkOutcome::Approved => {
                            self.app.finish_initial_collaboration_cleanup().await;
                            return self.fetch_current_device_generation().await;
                        }
                        kodosi_domain::device_link::SelfDeviceLinkOutcome::Cancelled => {
                            return Err(AppError::Unsupported {
                                reason: "Link request was cancelled.".to_owned(),
                            });
                        }
                        kodosi_domain::device_link::SelfDeviceLinkOutcome::Expired => {
                            return Err(AppError::Unsupported {
                                reason: "Link request expired before approval. Run `kodosi device link` again."
                                    .to_owned(),
                            });
                        }
                        kodosi_domain::device_link::SelfDeviceLinkOutcome::Failed => {
                            return Err(AppError::Unsupported {
                                reason: "Link request failed.".to_owned(),
                            });
                        }
                    }
                }
                Some(_) => {}
                None => {
                    return Err(AppError::Unsupported {
                        reason: "device link ended before a result was returned".to_owned(),
                    });
                }
            }
        }
    }

    async fn fetch_current_device_generation(&self) -> Result<Option<u64>> {
        let Some(user_id) = self.app.state.identity.auth.subject() else {
            return Ok(None);
        };
        let user_id = user_id.to_string();
        let bundle = self.app.backend.fetch_user_identity(&user_id).await?;
        Ok(Some(
            super::identity::backend_adapters::identity_bundle_view(&bundle)?
                .signed_list
                .generation,
        ))
    }

    async fn login_summary(&self) -> LoginSummary {
        let user = self.app.backend.fetch_current_user().await.ok();
        let rooms = RoomApplication::new(&self.app)
            .fetch_rooms()
            .await
            .unwrap_or_default();
        let owned_remote_sessions = self
            .app
            .backend
            .fetch_my_sessions()
            .await
            .unwrap_or_default();
        LoginSummary {
            user,
            rooms,
            owned_remote_sessions,
        }
    }

    fn is_authenticated(&self) -> bool {
        matches!(
            self.app.state.identity.auth,
            AuthState::Authenticated { .. }
        )
    }

    fn take_auth_error(&mut self) -> Option<AppError> {
        self.app
            .state
            .runtime_outbox
            .drain_auth()
            .into_iter()
            .find_map(|event| match event {
                AuthEvent::Error { message, .. } => Some(AppError::Unsupported { reason: message }),
                _ => None,
            })
    }

    fn last_log_error(&self, fallback: &str) -> AppError {
        AppError::Unsupported {
            reason: self
                .app
                .state
                .logs
                .back()
                .cloned()
                .unwrap_or_else(|| fallback.to_owned()),
        }
    }
}

fn classify_backend_access(access_state: BackendAccessState) -> OneShotBackendAccess {
    match access_state {
        BackendAccessState::Ready { .. } | BackendAccessState::TemporarilyUnavailable { .. } => {
            OneShotBackendAccess::Ready
        }
        BackendAccessState::RequiresLogin(reason) => {
            OneShotBackendAccess::RequiresLogin(reason.to_string())
        }
        BackendAccessState::SignedOut => OneShotBackendAccess::SignedOut,
        BackendAccessState::StorageUnavailable(reason) => {
            OneShotBackendAccess::StorageUnavailable(reason.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeSet,
        sync::{
            Arc, Mutex,
            atomic::{AtomicU32, AtomicUsize, Ordering},
        },
        time::Duration as StdDuration,
    };

    use axum::{
        Json, Router,
        http::StatusCode,
        routing::{delete, get, post},
    };
    use serde_json::json;
    use time::{Duration, OffsetDateTime};
    use tokio::{net::TcpListener, task::JoinSet};
    use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

    use super::{OneShotApp, OneShotBackendAccess};
    use crate::{
        RoomCommand, SessionCommand,
        config::AppConfig,
        identity_core::token_store::{StoredTokens, TokenStore},
        runtime::{DEFAULT_TOKEN_SUBJECT, Runtime},
        session_runtime::events::DiscoverySurface,
    };

    struct CompatibilityServer {
        base_url: String,
        api_version: Arc<AtomicU32>,
        compatibility_requests: Arc<AtomicUsize>,
        api_requests: Arc<AtomicUsize>,
        directed_posts: Arc<AtomicUsize>,
        room_reads: Arc<AtomicUsize>,
        cleanup_requests: Arc<Mutex<Vec<String>>>,
        task: AbortOnDropHandle<()>,
    }

    impl CompatibilityServer {
        async fn spawn(api_version: u32) -> Self {
            let api_version = Arc::new(AtomicU32::new(api_version));
            let health_api_version = Arc::clone(&api_version);
            let compatibility_requests = Arc::new(AtomicUsize::new(0));
            let compatibility_counter = Arc::clone(&compatibility_requests);
            let api_requests = Arc::new(AtomicUsize::new(0));
            let me_counter = Arc::clone(&api_requests);
            let directed_posts = Arc::new(AtomicUsize::new(0));
            let post_counter = Arc::clone(&directed_posts);
            let room_reads = Arc::new(AtomicUsize::new(0));
            let room_read_counter = Arc::clone(&room_reads);
            let cleanup_requests = Arc::new(Mutex::new(Vec::new()));
            let cleanup_log = Arc::clone(&cleanup_requests);
            let router = Router::new()
                .route(
                    "/health/ready",
                    get(move || async move {
                        compatibility_counter.fetch_add(1, Ordering::SeqCst);
                        Json(json!({
                            "status": "Ready",
                            "apiContractVersion": health_api_version.load(Ordering::SeqCst),
                            "authContractVersion": 1
                        }))
                    }),
                )
                .route(
                    "/api/me",
                    get(move || async move {
                        me_counter.fetch_add(1, Ordering::SeqCst);
                        Json(json!({
                            "id": "a9af71af-8ac6-4f35-8c4d-c0af77e52674",
                            "handle": "compatibility-test",
                            "displayName": "Compatibility Test",
                            "email": null,
                            "avatarUrl": null
                        }))
                    }),
                )
                .route("/api/rooms", get(|| async { Json(json!([])) }))
                .route("/api/rooms/", get(|| async { Json(json!([])) }))
                .route(
                    "/api/rooms/invitations/incoming",
                    get(|| async { Json(json!([])) }),
                )
                .route(
                    "/api/rooms/invitations/outgoing",
                    get(|| async { Json(json!([])) }),
                )
                .route(
                    "/api/rooms/{room_id}/chat",
                    post(move || async move {
                        post_counter.fetch_add(1, Ordering::SeqCst);
                        StatusCode::CREATED
                    }),
                )
                .route(
                    "/api/sessions/{session_id}",
                    delete(move || {
                        let cleanup_log = Arc::clone(&cleanup_log);
                        async move {
                            cleanup_log
                                .lock()
                                .expect("cleanup log")
                                .push("delete".to_owned());
                            StatusCode::NO_CONTENT
                        }
                    }),
                )
                .route(
                    "/api/rooms/{room_id}/tasks",
                    get(move || async move {
                        room_read_counter.fetch_add(1, Ordering::SeqCst);
                        ([("Kodosi-Has-More", "false"),
                          ("Kodosi-Task-Snapshot", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")], Json(json!([])))
                    }),
                );
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let task = AbortOnDropHandle::new(tokio::spawn(async move {
                axum::serve(listener, router).await.unwrap();
            }));
            Self {
                base_url: format!("http://{address}"),
                api_version,
                compatibility_requests,
                api_requests,
                directed_posts,
                room_reads,
                cleanup_requests,
                task,
            }
        }

        fn runtime(&self) -> Runtime {
            let mut config = AppConfig::load(None).unwrap();
            config.backend.api = Some(self.base_url.clone());
            config.backend.host_relay = None;
            config.backend.viewer_relay = None;
            config.backend.user_events = None;
            config.auth.issuer = Some(self.base_url.clone());
            config.auth.client_id = "compatibility-test".to_owned();
            config.auth.keyring_service =
                format!("dev.kodosi.compatibility-test.{}", uuid::Uuid::now_v7());
            let mut app = Runtime::with_dependencies(
                config.clone(),
                CancellationToken::new(),
                crate::runtime::RuntimeDependencies::isolated(&config)
                    .expect("isolated runtime dependencies"),
            )
            .unwrap();
            app.device_enrollment_satisfied = true;
            app.token_store
                .save(
                    DEFAULT_TOKEN_SUBJECT,
                    &StoredTokens::new(
                        "stored-access-token".to_owned(),
                        None,
                        OffsetDateTime::now_utc() + Duration::hours(1),
                    ),
                )
                .unwrap();
            app
        }

        fn set_api_version(&self, api_version: u32) {
            self.api_version.store(api_version, Ordering::SeqCst);
        }

        fn directed_post_count(&self) -> usize {
            self.directed_posts.load(Ordering::SeqCst)
        }

        fn compatibility_request_count(&self) -> usize {
            self.compatibility_requests.load(Ordering::SeqCst)
        }

        fn api_request_count(&self) -> usize {
            self.api_requests.load(Ordering::SeqCst)
        }

        fn room_read_count(&self) -> usize {
            self.room_reads.load(Ordering::SeqCst)
        }

        fn cleanup_request_count(&self) -> usize {
            self.cleanup_requests.lock().expect("cleanup log").len()
        }

        fn stop(&self) {
            self.task.abort();
        }
    }

    struct UnresponsiveServer {
        base_url: String,

        _task: AbortOnDropHandle<()>,
    }

    impl UnresponsiveServer {
        async fn spawn() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let task = AbortOnDropHandle::new(tokio::spawn(async move {
                let mut connections = JoinSet::new();
                loop {
                    tokio::select! {
                        accepted = listener.accept() => {
                            let Ok((stream, _)) = accepted else { return };
                            connections.spawn(async move {
                                let _stream = stream;
                                std::future::pending::<()>().await;
                            });
                        }
                        joined = connections.join_next(), if !connections.is_empty() => {
                            drop(joined);
                        }
                    }
                }
            }));
            Self {
                base_url: format!("http://{address}"),
                _task: task,
            }
        }

        fn runtime(&self) -> Runtime {
            runtime_for_backend(&self.base_url)
        }
    }

    fn runtime_for_backend(base_url: &str) -> Runtime {
        let mut config = AppConfig::load(None).unwrap();
        config.backend.api = Some(base_url.to_owned());
        config.backend.host_relay = None;
        config.backend.viewer_relay = None;
        config.backend.user_events = None;
        config.auth.issuer = Some(base_url.to_owned());
        config.auth.client_id = "compatibility-test".to_owned();
        config.auth.keyring_service =
            format!("dev.kodosi.compatibility-test.{}", uuid::Uuid::now_v7());
        let mut app = Runtime::with_dependencies(
            config.clone(),
            CancellationToken::new(),
            crate::runtime::RuntimeDependencies::isolated(&config)
                .expect("isolated runtime dependencies"),
        )
        .unwrap();
        app.device_enrollment_satisfied = true;
        app.token_store
            .save(
                DEFAULT_TOKEN_SUBJECT,
                &StoredTokens::new(
                    "stored-access-token".to_owned(),
                    None,
                    OffsetDateTime::now_utc() + Duration::hours(1),
                ),
            )
            .unwrap();
        app
    }

    #[tokio::test]
    async fn startup_keeps_local_auth_but_disables_backend_access_for_api_v1() {
        let server = CompatibilityServer::spawn(1).await;
        let mut app = server.runtime();

        app.initialize().await.unwrap();

        assert!(app.state.identity.auth.is_authenticated());
        assert!(!app.has_verified_backend_compatibility());
    }

    #[tokio::test]
    async fn startup_defers_incompatible_api_v2_until_maintenance() {
        let server = CompatibilityServer::spawn(2).await;
        let mut app = server.runtime();

        app.initialize().await.unwrap();

        assert!(app.state.identity.auth.is_authenticated());
        assert!(!app.has_verified_backend_compatibility());
    }

    #[tokio::test]
    async fn unresponsive_backend_does_not_block_startup_or_local_snapshot_work() {
        let server = UnresponsiveServer::spawn().await;
        let mut app = server.runtime();

        tokio::time::timeout(StdDuration::from_secs(10), app.initialize())
            .await
            .expect("local startup must not wait for backend compatibility")
            .unwrap();
        let effects = crate::host_protocol::sessions_command::apply_session_command(
            &mut app,
            SessionCommand::SnapshotRefresh,
        )
        .await
        .unwrap();
        tokio::time::timeout(StdDuration::from_secs(2), app.drain_session_events())
            .await
            .expect("local snapshot maintenance must stay bounded");

        assert!(effects.defer_catalog_replay);
        assert!(!app.has_verified_backend_compatibility());
    }

    #[tokio::test]
    async fn one_shot_rejects_restored_credentials_from_api_v1() {
        let server = CompatibilityServer::spawn(1).await;
        let mut app = OneShotApp {
            app: server.runtime(),
            runtime_authority: None,
            runtime_authority_for_test: false,
        };

        let error = app.backend_access().await.unwrap_err();

        assert!(error.to_string().contains("API 1"));
        assert!(app.is_authenticated());
        assert!(!app.app.has_verified_backend_compatibility());
    }

    #[tokio::test]
    async fn one_shot_accepts_restored_credentials_from_current_api() {
        let server =
            CompatibilityServer::spawn(crate::runtime::auth::EXPECTED_BACKEND_API_CONTRACT).await;
        let mut app = OneShotApp {
            app: server.runtime(),
            runtime_authority: None,
            runtime_authority_for_test: false,
        };

        app.backend_access().await.unwrap();

        assert!(app.is_authenticated());
        assert!(app.app.has_verified_backend_compatibility());
    }

    #[tokio::test]
    async fn backend_origin_mismatch_starts_signed_out_and_preserves_bound_credentials() {
        let server =
            CompatibilityServer::spawn(crate::runtime::auth::EXPECTED_BACKEND_API_CONTRACT).await;
        let runtime = server.runtime();
        let stale_origin = "http://127.0.0.1:52385/";
        runtime
            .token_store
            .save(
                DEFAULT_TOKEN_SUBJECT,
                &StoredTokens::new(
                    "stale-access-token".to_owned(),
                    None,
                    OffsetDateTime::now_utc() + Duration::hours(1),
                )
                .bind_backend_account(
                    stale_origin.to_owned(),
                    "a9af71af-8ac6-4f35-8c4d-c0af77e52674".to_owned(),
                ),
            )
            .expect("save backend-bound credentials");
        let mut app = OneShotApp {
            app: runtime,
            runtime_authority: None,
            runtime_authority_for_test: false,
        };

        let access = app
            .backend_access()
            .await
            .expect("origin mismatch is an auth disposition");
        std::assert_matches!(
            access,
            OneShotBackendAccess::RequiresLogin(reason)
                if reason.contains(stale_origin) && reason.contains(&server.base_url)
        );
        assert!(!app.is_authenticated());
        assert_eq!(server.compatibility_request_count(), 0);
        assert_eq!(server.api_request_count(), 0);
        assert_eq!(
            app.stored_backend_account()
                .expect("read retained binding")
                .expect("binding remains")
                .0
                .to_string(),
            stale_origin
        );
    }

    #[tokio::test]
    async fn one_shot_without_runtime_authority_never_dispatches_cleanup() {
        let server = CompatibilityServer::spawn(6).await;
        let runtime = server.runtime();
        let mut app = OneShotApp {
            app: runtime,
            runtime_authority: None,
            runtime_authority_for_test: false,
        };
        let session_id = kodosi_domain::ids::SessionId::new().to_string();
        let create_id = uuid::Uuid::now_v7();
        let incarnation_id = uuid::Uuid::now_v7();
        let end_id = uuid::Uuid::now_v7();
        let origin = app.app.backend.backend_origin().expect("origin").clone();
        app.app
            .collaboration_teardown
            .provision(
                &origin,
                "a9af71af-8ac6-4f35-8c4d-c0af77e52674",
                &session_id,
                create_id,
                end_id,
                1,
            )
            .expect("provision");
        app.app
            .collaboration_teardown
            .bind_incarnation(create_id, &session_id, incarnation_id)
            .expect("bind");

        let error = app
            .remote_command_access()
            .await
            .expect_err("non-authority one-shot must fail closed");

        assert!(error.to_string().contains("another Kodosi runtime owns"));
        assert_eq!(server.cleanup_request_count(), 0);
        assert_eq!(server.api_request_count(), 0);
        assert_eq!(
            app.app
                .collaboration_teardown
                .validate_and_health()
                .expect("health")
                .active_count,
            1
        );
    }

    #[tokio::test]
    async fn share_transition_reset_requires_runtime_authority_before_touching_evidence() {
        let server =
            CompatibilityServer::spawn(crate::runtime::auth::EXPECTED_BACKEND_API_CONTRACT).await;
        let mut app = OneShotApp {
            app: server.runtime(),
            runtime_authority: None,
            runtime_authority_for_test: false,
        };
        let root = tempfile::tempdir().expect("ledger root");
        let path = root.path().join("share-transitions.json");
        let evidence = b"malformed transition evidence";
        std::fs::write(&path, evidence).expect("write unavailable evidence");
        app.app.share_transitions =
            crate::runtime::share_transition_ledger::ShareTransitionLedger::unavailable_for_test(
                path.clone(),
                "malformed ledger",
            );

        let error = app
            .reset_share_transition_ledger()
            .expect_err("non-authority reset must be refused");

        assert!(error.to_string().contains("another Kodosi runtime owns"));
        assert_eq!(
            std::fs::read(path).expect("read retained evidence"),
            evidence
        );
        assert_eq!(
            std::fs::read_dir(root.path()).expect("ledger root").count(),
            1,
            "refused reset must not create a sidecar"
        );
    }

    #[tokio::test]
    async fn one_shot_remote_access_fails_closed_on_corruption_quarantine() {
        let server =
            CompatibilityServer::spawn(crate::runtime::auth::EXPECTED_BACKEND_API_CONTRACT).await;
        let runtime = server.runtime();
        let mut app = OneShotApp {
            app: runtime,
            runtime_authority: None,
            runtime_authority_for_test: true,
        };
        let root = tempfile::tempdir().expect("store root");
        let store_path = root.path().join("collaboration-teardown-obligations.json");
        let malformed = br#"{
          "version": 1,
          "records": [{"backendOrigin":"https://example.test/"}],
          "quarantinedRecords": []
        }"#;
        std::fs::write(&store_path, malformed).expect("malformed current schema");
        app.app.collaboration_teardown = crate::sharing::collaboration_teardown_obligations::CollaborationTeardownObligationStore::at(
            store_path,
            8,
        )
        .expect("quarantined store");

        let access = app
            .remote_command_access()
            .await
            .expect("typed fail-closed status");

        assert!(matches!(
            access,
            super::OneShotBackendAccess::CollaborationQuarantined(_)
        ));
        assert_eq!(server.cleanup_request_count(), 0);
        assert_eq!(
            server.api_request_count(),
            0,
            "quarantine blocks every remote read"
        );
        assert_eq!(
            std::fs::read_dir(root.path())
                .expect("store root")
                .filter_map(std::result::Result::ok)
                .filter(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("collaboration-teardown-obligations.corrupt-"))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn one_shot_remote_access_settles_cleanup_before_returning_ready() {
        let server =
            CompatibilityServer::spawn(crate::runtime::auth::EXPECTED_BACKEND_API_CONTRACT).await;
        let runtime = server.runtime();
        let mut app = OneShotApp {
            app: runtime,
            runtime_authority: None,
            runtime_authority_for_test: true,
        };
        let session_id = kodosi_domain::ids::SessionId::new().to_string();
        let create_id = uuid::Uuid::now_v7();
        let incarnation_id = uuid::Uuid::now_v7();
        let end_id = uuid::Uuid::now_v7();
        let origin = app.app.backend.backend_origin().expect("origin").clone();
        app.app
            .collaboration_teardown
            .provision(
                &origin,
                "a9af71af-8ac6-4f35-8c4d-c0af77e52674",
                &session_id,
                create_id,
                end_id,
                1,
            )
            .expect("provision");
        app.app
            .collaboration_teardown
            .bind_incarnation(create_id, &session_id, incarnation_id)
            .expect("bind");

        let access = app.remote_command_access().await.expect("remote access");

        assert_eq!(access, super::OneShotBackendAccess::Ready);
        assert!(app.app.remote_surfaces_ready());
        assert_eq!(server.cleanup_request_count(), 1);
        assert_eq!(
            app.app
                .collaboration_teardown
                .validate_and_health()
                .expect("health")
                .active_count,
            0
        );
    }

    #[tokio::test]
    async fn start_login_never_calls_api_routes_on_an_incompatible_backend() {
        let server = CompatibilityServer::spawn(1).await;
        let mut app = OneShotApp {
            app: server.runtime(),
            runtime_authority: None,
            runtime_authority_for_test: true,
        };

        let error = app.start_login().await.unwrap_err();

        assert!(error.to_string().contains("API 1"));
        assert_eq!(server.api_request_count(), 0);
    }

    #[tokio::test]
    async fn offline_headless_initialization_accepts_local_session_commands() {
        let server = CompatibilityServer::spawn(6).await;
        let mut app = server.runtime();
        server.stop();
        tokio::task::yield_now().await;

        app.initialize().await.unwrap();
        assert!(app.state.identity.auth.is_authenticated());
        assert!(!app.has_verified_backend_compatibility());
        let effects = crate::host_protocol::sessions_command::apply_session_command(
            &mut app,
            SessionCommand::SnapshotRefresh,
        )
        .await
        .unwrap();
        assert!(effects.defer_catalog_replay);
    }

    #[tokio::test]
    async fn api_v1_rejects_directed_chat_before_any_backend_post() {
        let server = CompatibilityServer::spawn(1).await;
        let mut app = server.runtime();
        app.initialize().await.unwrap();

        let error = crate::runtime::runtime_loop::dispatch_room_message_for_test(
            &mut app,
            RoomCommand::ChatPost {
                room_id: "c80a0d8f-46ba-4739-885f-4114895bf52b".to_owned(),
                body: "directed".to_owned(),
                author_session_id: None,
                recipient_session_ids: Vec::new(),
                recipient_user_ids: vec!["e220e26d-eac4-4706-9578-63ac80fe546c".to_owned()],
                request_id: Some("v1-directed-post".to_owned()),
            },
        )
        .await
        .unwrap_err();
        assert!(
            error.to_string().contains("API 1"),
            "unexpected error: {error}"
        );
        assert_eq!(server.directed_post_count(), 0);
    }

    #[tokio::test]
    async fn periodic_auth_check_recovers_when_backend_returns_with_current_api() {
        let server = CompatibilityServer::spawn(1).await;
        let mut app = server.runtime();
        app.initialize().await.unwrap();
        assert!(!app.has_verified_backend_compatibility());
        assert!(app.state.identity.auth.subject().is_none());

        server.set_api_version(crate::runtime::auth::EXPECTED_BACKEND_API_CONTRACT);
        app.run_periodic_maintenance_force().await;

        assert!(app.has_verified_backend_compatibility());
        assert!(app.state.identity.auth.subject().is_some());
        assert_eq!(
            app.state.identity.current_user_name.as_deref(),
            Some("Compatibility Test")
        );
    }

    #[tokio::test]
    async fn mailbox_waits_for_current_api_then_resumes_pending_room_work() {
        let server = CompatibilityServer::spawn(1).await;
        let mut app = server.runtime();
        app.initialize().await.unwrap();
        app.run_periodic_maintenance_force().await;
        let compatibility_checks = server.compatibility_request_count();
        app.state
            .queue_discovery_refresh([DiscoverySurface::RoomTasks]);
        app.state
            .queue_room_projection_refresh("room-a".to_owned(), [DiscoverySurface::RoomTasks]);
        app.last_auth_refresh_check = None;

        app.run_periodic_maintenance_force().await;

        assert_eq!(
            server.compatibility_request_count(),
            compatibility_checks + 1,
            "the incompatible maintenance pass must actually recheck compatibility"
        );
        assert_eq!(server.room_read_count(), 0);
        assert_eq!(
            app.state
                .pending_discovery_surfaces
                .iter()
                .copied()
                .filter(|surface| matches!(
                    surface,
                    DiscoverySurface::RoomChat | DiscoverySurface::RoomTasks
                ))
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([DiscoverySurface::RoomTasks])
        );
        assert_eq!(
            app.state.pending_room_projection_refreshes.get("room-a"),
            Some(&BTreeSet::from([DiscoverySurface::RoomTasks]))
        );

        server.set_api_version(crate::runtime::auth::EXPECTED_BACKEND_API_CONTRACT);
        app.device_enrollment_satisfied = true;
        app.last_auth_refresh_check = None;
        app.run_periodic_maintenance_force().await;

        assert!(app.has_verified_backend_compatibility());
        assert_eq!(
            server.compatibility_request_count(),
            compatibility_checks + 2,
            "the recovery pass must observe the current backend API before scheduling mailbox work"
        );
        assert_eq!(
            server.room_read_count(),
            1,
            "pending room work should resume only after current API verification"
        );
        assert!(
            !app.state
                .pending_discovery_surfaces
                .contains(&DiscoverySurface::RoomTasks),
            "the resumed mailbox surface should be consumed once"
        );
    }
}
