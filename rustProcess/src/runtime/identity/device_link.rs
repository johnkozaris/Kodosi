use std::collections::{BTreeSet, VecDeque};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};

use crate::{
    AppError, Result,
    identity_core::{device_keys::DeviceKeyStore, device_list_pin_store::DeviceListPinStoreHandle},
    runtime::state::push_log,
    session_runtime::events::{DiscoverySurface, RuntimeSessionEvent},
};
use kodosi_backend_client::{
    BackendClientError, api::DeviceLinkPollState, http_client::BackendHttpClient,
};
use kodosi_domain::{auth::AuthState, device_link::SelfDeviceLinkOutcome};

use super::{
    AccountRuntimes, DeviceFlowRuntime, DeviceKeyAccess,
    account_runtimes::{SelfDeviceLinkIdentity, SelfDeviceLinkRuntime, shutdown_account_runtimes},
    backend_adapters,
    device_keys::{install_self_pin, verify_own_identity_bundle},
};

#[derive(Debug, Clone)]
pub(crate) struct DeviceLinkApprovalOutcome {
    #[cfg(feature = "cli")]
    pub(crate) approved_user_code: String,
    #[cfg(feature = "cli")]
    pub(crate) approved_device_id: String,
    #[cfg(feature = "cli")]
    pub(crate) approved_device_label: String,
    #[cfg(feature = "cli")]
    pub(crate) new_generation: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct SelfDeviceLinkStart {
    #[cfg(feature = "cli")]
    pub(crate) device_id: String,
    #[cfg(feature = "cli")]
    pub(crate) user_code: String,
    #[cfg(feature = "cli")]
    pub(crate) expires_at: String,
}

pub(crate) struct DeviceLinkCtx<'a> {
    pub(crate) auth: &'a AuthState,
    pub(crate) account_origin: Option<crate::session_runtime::events::AccountEventOrigin>,
    pub(crate) backend: &'a BackendHttpClient,
    pub(crate) device_key_store: &'a DeviceKeyStore,
    pub(crate) pin_store: &'a DeviceListPinStoreHandle,
    pub(crate) account_runtimes: &'a mut AccountRuntimes,
    pub(crate) device_flow: &'a mut DeviceFlowRuntime,
    pub(crate) pending_discovery_surfaces: &'a mut BTreeSet<DiscoverySurface>,
    pub(crate) session_events_tx: &'a mpsc::Sender<RuntimeSessionEvent>,
    pub(crate) logs: &'a mut VecDeque<String>,
}

impl DeviceLinkCtx<'_> {
    #[tracing::instrument(skip_all, fields(user_code_len = user_code.len()), err)]
    pub(crate) async fn approve(&mut self, user_code: &str) -> Result<DeviceLinkApprovalOutcome> {
        use crate::identity_core::device_link;

        let normalized = device_link::normalize_user_code(user_code);
        if normalized.is_empty() {
            return Err(AppError::Unsupported {
                reason: "user_code is required".to_owned(),
            });
        }

        let user_id = self.auth.subject_string().ok_or(AppError::Unauthorized)?;
        let local_keys = DeviceKeyAccess::new(self.auth, self.device_key_store)
            .load_authenticated_device_keys()?;

        let pending = backend_adapters::pending_device_link(
            self.backend.fetch_device_link_pending(&normalized).await?,
        );

        let bundle = self.backend.fetch_user_identity(&user_id).await?;
        let view =
            verify_own_identity_bundle(self.auth, &local_keys, self.pin_store, &bundle).await?;
        let previous_entries = view.signed_list.entries.clone();

        if previous_entries
            .iter()
            .any(|e| e.device_id == pending.device_id)
        {
            return Err(AppError::Unsupported {
                reason: format!(
                    "Device `{}` is already enrolled — nothing to approve.",
                    pending.device_id
                ),
            });
        }

        let approve_request = device_link::build_approval_request(
            &user_id,
            &local_keys,
            view.signed_list.generation,
            &previous_entries,
            &pending,
            &normalized,
        )?;

        self.backend
            .post_device_link_approve(&backend_adapters::device_link_approval_request(
                approve_request,
            ))
            .await?;

        if let Err(err) =
            install_self_pin(self.backend, self.pin_store, &user_id, &local_keys).await
        {
            push_log(
                self.logs,
                format!("self-pin refresh failed after approve-link: {err}"),
            );
        }

        Ok(DeviceLinkApprovalOutcome {
            #[cfg(feature = "cli")]
            approved_user_code: normalized,
            #[cfg(feature = "cli")]
            approved_device_id: pending.device_id,
            #[cfg(feature = "cli")]
            approved_device_label: pending.device_label,
            #[cfg(feature = "cli")]
            new_generation: view.signed_list.generation.saturating_add(1),
        })
    }

    #[tracing::instrument(skip_all, err)]
    pub(crate) async fn start_self(&mut self) -> Result<()> {
        self.start_self_with_label(None).await.map(drop)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "single self-device-link lifecycle keeps cancellation and poll setup together"
    )]
    #[tracing::instrument(skip_all, err)]
    pub(crate) async fn start_self_with_label(
        &mut self,
        label: Option<String>,
    ) -> Result<SelfDeviceLinkStart> {
        use kodosi_backend_client::api::DeviceLinkStartRequest;

        let AuthState::Authenticated {
            subject: Some(subject),
            ..
        } = self.auth
        else {
            shutdown_account_runtimes(
                self.device_flow,
                self.account_runtimes,
                self.pending_discovery_surfaces,
            );
            return Err(AppError::Unauthorized);
        };
        let user_id = subject.to_string();
        let origin = self.account_origin.clone().ok_or(AppError::Unauthorized)?;

        if let Some(existing) = self.account_runtimes.self_device_link() {
            return replay_existing_self_device_link(existing, &origin, self.session_events_tx)
                .await;
        }

        let keys = DeviceKeyAccess::new(self.auth, self.device_key_store)
            .load_authenticated_device_keys()?;
        match self.backend.fetch_user_identity(&user_id).await {
            Ok(bundle) => {
                let view = backend_adapters::identity_bundle_view(&bundle)?;
                if view.devices.contains_key(&keys.device_id) {
                    verify_own_identity_bundle(self.auth, &keys, self.pin_store, &bundle).await?;
                    return Err(AppError::Unsupported {
                        reason: format!(
                            "This machine is already enrolled as `{}` at generation {}. Nothing to link.",
                            keys.device_id, view.signed_list.generation,
                        ),
                    });
                }
            }
            Err(BackendClientError::NotFound) => {}
            Err(error) => return Err(error.into()),
        }

        let device_label = label.unwrap_or_else(default_device_label);

        let init = self
            .backend
            .post_device_link_init(&DeviceLinkStartRequest {
                device_id: keys.device_id.clone(),
                device_label: device_label.clone(),
                kem_public_key: BASE64.encode(keys.kem_public_bytes()),
                signing_public_key: BASE64.encode(keys.signing_public_bytes()),
            })
            .await?;

        drop(
            self.session_events_tx
                .send(RuntimeSessionEvent::DeviceLinkSelfPending {
                    origin: origin.clone(),
                    user_code: init.user_code.clone(),
                    expires_at: init.expires_at.clone(),
                })
                .await,
        );

        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let session_events_tx = self.session_events_tx.clone();
        let backend = self.backend.clone();
        let pin_store = self.pin_store.clone();
        let device_code = init.device_code.clone();
        let user_code_cancel = init.user_code.clone();
        #[cfg(feature = "cli")]
        let user_code_for_task = user_code_cancel.clone();
        #[cfg(not(feature = "cli"))]
        let user_code_for_task = user_code_cancel;
        let user_id_for_task = user_id.clone();
        #[cfg(feature = "cli")]
        let linked_device_id = keys.device_id.clone();
        let pending_identity = SelfDeviceLinkIdentity {
            origin: origin.clone(),
            device_id: keys.device_id.clone(),
            device_label,
            user_code: init.user_code.clone(),
            expires_at: init.expires_at.clone(),
        };

        let join_handle = tokio::spawn(async move {
            let outcome = run_self_device_link_poll(
                &backend,
                &device_code,
                &user_code_for_task,
                task_cancellation,
            )
            .await;

            let resolved_outcome = match outcome {
                SelfDeviceLinkPollOutcome::Approved { generation, bundle } => {
                    finish_approved_self_link(
                        &backend,
                        &pin_store,
                        &user_id_for_task,
                        &keys,
                        &device_code,
                        generation,
                        &bundle,
                    )
                    .await
                }
                SelfDeviceLinkPollOutcome::Cancelled => SelfDeviceLinkOutcome::Cancelled,
                SelfDeviceLinkPollOutcome::Expired => SelfDeviceLinkOutcome::Expired,
                SelfDeviceLinkPollOutcome::Failed => SelfDeviceLinkOutcome::Failed,
            };

            drop(
                session_events_tx
                    .send(RuntimeSessionEvent::DeviceLinkSelfResolved {
                        origin,
                        user_code: user_code_for_task,
                        outcome: resolved_outcome,
                    })
                    .await,
            );
        });

        self.account_runtimes
            .attach_self_device_link(SelfDeviceLinkRuntime {
                identity: pending_identity,
                cancellation,
                join_handle,
            });
        Ok(SelfDeviceLinkStart {
            #[cfg(feature = "cli")]
            device_id: linked_device_id,
            #[cfg(feature = "cli")]
            user_code: user_code_cancel,
            #[cfg(feature = "cli")]
            expires_at: init.expires_at,
        })
    }

    pub(crate) fn cancel_self(&self) -> bool {
        self.account_runtimes
            .request_self_device_link_cancellation()
    }
}

async fn replay_existing_self_device_link(
    existing: &SelfDeviceLinkRuntime,
    origin: &crate::session_runtime::events::AccountEventOrigin,
    session_events_tx: &mpsc::Sender<RuntimeSessionEvent>,
) -> Result<SelfDeviceLinkStart> {
    if existing.cancellation.is_cancelled() {
        return Err(AppError::Unsupported {
            reason: "The previous link cancellation is still reconciling with the backend."
                .to_owned(),
        });
    }
    if existing.identity.origin != *origin {
        return Err(AppError::Unsupported {
            reason: "The pending device link belongs to a retired account epoch.".to_owned(),
        });
    }
    drop(
        session_events_tx
            .send(RuntimeSessionEvent::DeviceLinkSelfPending {
                origin: existing.identity.origin.clone(),
                user_code: existing.identity.user_code.clone(),
                expires_at: existing.identity.expires_at.clone(),
            })
            .await,
    );
    Ok(SelfDeviceLinkStart {
        #[cfg(feature = "cli")]
        device_id: existing.identity.device_id.clone(),
        #[cfg(feature = "cli")]
        user_code: existing.identity.user_code.clone(),
        #[cfg(feature = "cli")]
        expires_at: existing.identity.expires_at.clone(),
    })
}

async fn finish_approved_self_link(
    backend: &BackendHttpClient,
    pin_store: &DeviceListPinStoreHandle,
    user_id: &str,
    local_keys: &crate::identity_core::device_keys::DeviceKeys,
    device_code: &str,
    generation: i64,
    bundle: &kodosi_backend_client::api::UserIdentityBundleDto,
) -> SelfDeviceLinkOutcome {
    use kodosi_backend_client::api::DeviceLinkAcknowledgeRequest;
    let mut backoff = std::time::Duration::from_secs(1);
    loop {
        match complete_peer_enrollment(bundle, pin_store, user_id, local_keys, generation).await {
            Ok(accepted_view) => {
                let acknowledgement = backend
                    .post_device_link_ack(&DeviceLinkAcknowledgeRequest {
                        device_code: device_code.to_owned(),
                        device_id: local_keys.device_id.clone(),
                    })
                    .await;
                match acknowledgement {
                    Ok(()) => return SelfDeviceLinkOutcome::Approved,
                    Err(error) if error.is_indeterminate_write() => {
                        tracing::warn!(%error, "post-self-link acknowledgement uncertain; retrying");
                    }
                    Err(error) if device_link_receipt_was_invalidated(&error) => {
                        rollback_rejected_enrollment_pin(pin_store, user_id, accepted_view).await;
                        tracing::warn!(%error, "post-self-link acknowledgement invalidated");
                        return SelfDeviceLinkOutcome::Failed;
                    }
                    Err(error) => {
                        tracing::warn!(%error, "post-self-link acknowledgement rejected");
                        return SelfDeviceLinkOutcome::Failed;
                    }
                }
            }
            Err(error @ (AppError::Io(_) | AppError::Http(_) | AppError::Json(_))) => {
                tracing::warn!(%error, "post-self-link enrollment proof unavailable; retrying");
            }
            Err(error) => {
                tracing::warn!(%error, "post-self-link enrollment proof failed");
                return SelfDeviceLinkOutcome::Failed;
            }
        }
        tokio::time::sleep(backoff).await;
        backoff = backoff
            .saturating_mul(2)
            .min(std::time::Duration::from_secs(30));
    }
}

fn device_link_receipt_was_invalidated(error: &BackendClientError) -> bool {
    matches!(
        error,
        BackendClientError::HttpProblem {
            status: 409,
            code: Some(code),
            ..
        } if code == "DEVICE_LINK_RECEIPT_INVALIDATED"
    )
}

async fn rollback_rejected_enrollment_pin(
    pin_store: &DeviceListPinStoreHandle,
    owner_user_id: &str,
    accepted_view: crate::identity_core::device_list_pin_store::IdentityBundleView,
) {
    let generation = accepted_view.signed_list.generation;
    if let Err(error) = pin_store
        .reset_if_identity_receipt(owner_user_id, accepted_view)
        .await
    {
        tracing::warn!(%error, generation, "failed to roll back rejected enrollment pin");
    }
}

async fn complete_peer_enrollment(
    bundle: &kodosi_backend_client::api::UserIdentityBundleDto,
    pin_store: &DeviceListPinStoreHandle,
    user_id: &str,
    local_keys: &crate::identity_core::device_keys::DeviceKeys,
    expected_generation: i64,
) -> Result<crate::identity_core::device_list_pin_store::IdentityBundleView> {
    use crate::identity_core::device_list_pin_store::{PinContext, PinVerdict};

    let expected_generation =
        u64::try_from(expected_generation).map_err(|_| AppError::InvalidBackendData {
            field: "deviceLink.deviceListGeneration".to_owned(),
            reason: "approved link returned a negative generation".to_owned(),
        })?;

    let view = backend_adapters::identity_bundle_view(bundle)?;
    if view.signed_list.generation != expected_generation {
        return Err(AppError::InvalidBackendData {
            field: "identity.deviceList.generation".to_owned(),
            reason: format!(
                "peer-enrollment bundle generation {} does not match approved generation {expected_generation}",
                view.signed_list.generation
            ),
        });
    }
    if view.user_id != user_id {
        return Err(AppError::InvalidBackendData {
            field: "identity.userId".to_owned(),
            reason: "peer-enrollment bundle names another user".to_owned(),
        });
    }
    let local =
        view.devices
            .get(&local_keys.device_id)
            .ok_or_else(|| AppError::InvalidBackendData {
                field: "identity.devices".to_owned(),
                reason: "peer-enrollment bundle omits the linked local device".to_owned(),
            })?;
    if local.sig_public_key != local_keys.signing_public_bytes()
        || local.certificate.kem_public_key != local_keys.kem_public_bytes()
    {
        return Err(AppError::InvalidBackendData {
            field: "identity.devices".to_owned(),
            reason: "peer-enrollment bundle changed the linked local device keys".to_owned(),
        });
    }
    if local.certificate.is_self_signed()
        || local.certificate.signer_device_id != view.signed_list.signer_device_id
    {
        return Err(AppError::InvalidBackendData {
            field: "identity.devices".to_owned(),
            reason:
                "peer-enrollment certificate and committed list must share one enrolled approver"
                    .to_owned(),
        });
    }
    match pin_store
        .verify_or_pin_for_owner(user_id, view.clone(), PinContext::PeerEnrollment)
        .await?
    {
        PinVerdict::FirstShare | PinVerdict::AcceptUpdate | PinVerdict::AlreadyPinned => Ok(view),
        PinVerdict::Reject { reason } => Err(AppError::InvalidBackendData {
            field: "identity.deviceList".to_owned(),
            reason: format!("peer-enrollment list failed pin verification: {reason:?}"),
        }),
    }
}

fn default_device_label() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Unknown device".to_owned())
}

#[derive(Debug)]
enum SelfDeviceLinkPollOutcome {
    Approved {
        generation: i64,
        bundle: Box<kodosi_backend_client::api::UserIdentityBundleDto>,
    },
    Cancelled,
    Expired,
    Failed,
}

#[tracing::instrument(skip_all)]
async fn run_self_device_link_poll(
    backend: &BackendHttpClient,
    device_code: &str,
    user_code: &str,
    cancellation: CancellationToken,
) -> SelfDeviceLinkPollOutcome {
    use kodosi_backend_client::api::DeviceLinkPollRequest;
    use std::time::Duration;

    let poll_interval = Duration::from_secs(5);
    let mut transient_backoff = poll_interval;
    let mut cancellation_requested = false;
    let request = DeviceLinkPollRequest {
        device_code: device_code.to_owned(),
    };
    loop {
        if !cancellation_requested && cancellation.is_cancelled() {
            cancellation_requested = true;
            if let Err(error) = backend.delete_device_link_request(user_code).await {
                tracing::debug!(%error, "self-link cancellation request requires poll reconciliation");
            }
        }

        let poll = if cancellation_requested {
            backend.post_device_link_poll(&request).await
        } else {
            tokio::select! {
                biased;
                () = cancellation.cancelled() => {
                    continue;
                }
                result = backend.post_device_link_poll(&request) => result,
            }
        };
        let response = match poll {
            Ok(response) => response,
            Err(error) if is_transient_device_link_poll_error(&error) => {
                tracing::warn!(
                    %error,
                    retry_seconds = transient_backoff.as_secs(),
                    "self-device-link poll failed; retrying"
                );
                if cancellation_requested {
                    tokio::time::sleep(transient_backoff).await;
                } else {
                    tokio::select! {
                        () = tokio::time::sleep(transient_backoff) => {}
                        () = cancellation.cancelled() => continue,
                    }
                }
                transient_backoff = transient_backoff
                    .saturating_mul(2)
                    .min(Duration::from_secs(30));
                continue;
            }
            Err(error) => {
                tracing::warn!(%error, "self-device-link poll cannot be reconciled");
                return SelfDeviceLinkPollOutcome::Failed;
            }
        };
        transient_backoff = poll_interval;
        match response.state {
            DeviceLinkPollState::Pending => {}
            DeviceLinkPollState::Approved => {
                let Some(generation) = response.device_list_generation else {
                    tracing::warn!("approved self-device-link omitted committed generation");
                    return SelfDeviceLinkPollOutcome::Failed;
                };
                let Some(bundle) = response.identity_bundle else {
                    tracing::warn!("approved self-device-link omitted committed identity bundle");
                    return SelfDeviceLinkPollOutcome::Failed;
                };
                return SelfDeviceLinkPollOutcome::Approved {
                    generation,
                    bundle: Box::new(bundle),
                };
            }
            DeviceLinkPollState::Cancelled => return SelfDeviceLinkPollOutcome::Cancelled,
            DeviceLinkPollState::Expired => return SelfDeviceLinkPollOutcome::Expired,
            DeviceLinkPollState::Unknown => {
                tracing::warn!("unknown self-device-link poll state");
                return SelfDeviceLinkPollOutcome::Failed;
            }
        }
        if cancellation_requested {
            tokio::time::sleep(poll_interval).await;
        } else {
            tokio::select! {
                () = tokio::time::sleep(poll_interval) => {}
                () = cancellation.cancelled() => {}
            }
        }
    }
}

fn is_transient_device_link_poll_error(error: &BackendClientError) -> bool {
    match error {
        BackendClientError::Io(_)
        | BackendClientError::Http(_)
        | BackendClientError::Timeout { .. } => true,
        BackendClientError::HttpProblem { status, .. } => {
            matches!(*status, 408 | 425 | 429 | 500..=599)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        identity_core::{
            device_cert::build_self_cert,
            device_keys::{DeviceKeys, generate_device_keys_for_test},
            device_link::{PendingDeviceLink, build_approval_request},
            device_list_pin_store::{DeviceListPinStore, DeviceListPinStoreHandle},
            signed_device_list::build_bootstrap_list,
        },
        session_runtime::events::{AccountEpoch, AccountEventOrigin},
    };
    use kodosi_backend_client::{
        api::{UserDeviceCertificateDto, UserDeviceListDto, UserIdentityBundleDto},
        config::BackendClientConfig,
    };
    use std::sync::{Arc, Mutex};
    use time::OffsetDateTime;

    const USER_ID: &str = "11111111-1111-1111-1111-111111111111";

    #[derive(Clone)]
    enum ScriptedPollResponse {
        Json(serde_json::Value),
        NotFound,
    }

    struct DeviceLinkServer {
        base_url: String,
        requests: Arc<Mutex<Vec<String>>>,
        task: tokio::task::AbortHandle,
    }

    impl DeviceLinkServer {
        async fn spawn(
            delete_status: axum::http::StatusCode,
            scripted_poll: ScriptedPollResponse,
        ) -> Self {
            use axum::{
                Json, Router,
                extract::{Path, State},
                response::{IntoResponse, Response},
                routing::{delete, post},
            };
            use tokio::net::TcpListener;

            #[derive(Clone)]
            struct ServerState {
                requests: Arc<Mutex<Vec<String>>>,
                delete_status: axum::http::StatusCode,
                poll: ScriptedPollResponse,
            }

            async fn cancel(
                State(state): State<ServerState>,
                Path(user_code): Path<String>,
            ) -> Response {
                state
                    .requests
                    .lock()
                    .expect("request log")
                    .push(format!("DELETE /api/devices/link/requests/{user_code}"));
                state.delete_status.into_response()
            }

            async fn poll(
                State(state): State<ServerState>,
                Json(request): Json<serde_json::Value>,
            ) -> Response {
                let device_code = request
                    .get("deviceCode")
                    .and_then(serde_json::Value::as_str)
                    .expect("device code");
                state
                    .requests
                    .lock()
                    .expect("request log")
                    .push(format!("POST /api/devices/link/poll {device_code}"));
                match state.poll {
                    ScriptedPollResponse::Json(value) => Json(value).into_response(),
                    ScriptedPollResponse::NotFound => {
                        axum::http::StatusCode::NOT_FOUND.into_response()
                    }
                }
            }

            async fn acknowledge(
                State(state): State<ServerState>,
                Json(request): Json<serde_json::Value>,
            ) -> Response {
                let device_code = request
                    .get("deviceCode")
                    .and_then(serde_json::Value::as_str)
                    .expect("device code");
                state
                    .requests
                    .lock()
                    .expect("request log")
                    .push(format!("POST /api/devices/link/ack {device_code}"));
                axum::http::StatusCode::NO_CONTENT.into_response()
            }

            let requests = Arc::new(Mutex::new(Vec::new()));
            let state = ServerState {
                requests: Arc::clone(&requests),
                delete_status,
                poll: scripted_poll,
            };
            let router = Router::new()
                .route("/api/devices/link/requests/{user_code}", delete(cancel))
                .route("/api/devices/link/poll", post(poll))
                .route("/api/devices/link/ack", post(acknowledge))
                .with_state(state);
            let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
            let address = listener.local_addr().expect("address");
            let task = tokio::spawn(async move {
                axum::serve(listener, router)
                    .await
                    .expect("device-link server");
            });
            Self {
                base_url: format!("http://{address}"),
                requests,
                task: task.abort_handle(),
            }
        }

        fn client(&self) -> BackendHttpClient {
            BackendHttpClient::new(&BackendClientConfig::new(
                Some(self.base_url.clone()),
                None,
                None,
                None,
            ))
            .expect("backend client")
        }

        fn requests(&self) -> Vec<String> {
            self.requests.lock().expect("request log").clone()
        }
    }

    impl Drop for DeviceLinkServer {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    #[tokio::test]
    async fn repeated_self_link_start_replays_exact_pending_identity() {
        let origin = AccountEventOrigin {
            account_user_id: USER_ID.to_owned(),
            epoch: AccountEpoch::for_test(7),
        };
        let cancellation = CancellationToken::new();
        let runtime = SelfDeviceLinkRuntime {
            identity: SelfDeviceLinkIdentity {
                origin: origin.clone(),
                device_id: "device-new".to_owned(),
                device_label: "John’s Mac".to_owned(),
                user_code: "ABCD-EFGH".to_owned(),
                expires_at: "2026-08-18T09:00:00Z".to_owned(),
            },
            cancellation,
            join_handle: tokio::spawn(std::future::pending::<()>()),
        };
        let abort = runtime.join_handle.abort_handle();
        let (events, mut received) = mpsc::channel(1);

        let start = replay_existing_self_device_link(&runtime, &origin, &events)
            .await
            .expect("same account epoch should receive the retained link");

        #[cfg(feature = "cli")]
        {
            assert_eq!(start.device_id, "device-new");
            assert_eq!(start.user_code, "ABCD-EFGH");
            assert_eq!(start.expires_at, "2026-08-18T09:00:00Z");
        }
        #[cfg(not(feature = "cli"))]
        drop(start);
        std::assert_matches!(
            received.recv().await,
            Some(RuntimeSessionEvent::DeviceLinkSelfPending {
                origin: event_origin,
                user_code,
                expires_at,
            }) if event_origin == origin
                && user_code == "ABCD-EFGH"
                && expires_at == "2026-08-18T09:00:00Z"
        );
        abort.abort();
    }

    #[tokio::test]
    async fn self_link_replay_rejects_a_retired_account_epoch() {
        let current = AccountEventOrigin {
            account_user_id: USER_ID.to_owned(),
            epoch: AccountEpoch::for_test(8),
        };
        let runtime = SelfDeviceLinkRuntime {
            identity: SelfDeviceLinkIdentity {
                origin: AccountEventOrigin {
                    account_user_id: USER_ID.to_owned(),
                    epoch: AccountEpoch::for_test(7),
                },
                device_id: "device-new".to_owned(),
                device_label: "John’s Mac".to_owned(),
                user_code: "ABCD-EFGH".to_owned(),
                expires_at: "2026-08-18T09:00:00Z".to_owned(),
            },
            cancellation: CancellationToken::new(),
            join_handle: tokio::spawn(std::future::pending::<()>()),
        };
        let abort = runtime.join_handle.abort_handle();
        let (events, mut received) = mpsc::channel(1);

        let error = replay_existing_self_device_link(&runtime, &current, &events)
            .await
            .expect_err("retired epoch must not replay pending identity");

        assert!(error.to_string().contains("retired account epoch"));
        assert!(received.try_recv().is_err());
        abort.abort();
    }

    #[tokio::test]
    async fn cancellation_deletes_once_then_observes_backend_cancelled() {
        let server = DeviceLinkServer::spawn(
            axum::http::StatusCode::NO_CONTENT,
            ScriptedPollResponse::Json(serde_json::json!({ "state": "cancelled" })),
        )
        .await;
        let cancellation = CancellationToken::new();
        cancellation.cancel();

        let outcome =
            run_self_device_link_poll(&server.client(), "device-secret", "BCDF-2345", cancellation)
                .await;

        assert!(matches!(outcome, SelfDeviceLinkPollOutcome::Cancelled));
        assert_eq!(
            server.requests(),
            [
                "DELETE /api/devices/link/requests/BCDF-2345",
                "POST /api/devices/link/poll device-secret",
            ]
        );
    }

    #[tokio::test]
    async fn cancellation_reconciliation_can_resolve_expired() {
        let server = DeviceLinkServer::spawn(
            axum::http::StatusCode::CONFLICT,
            ScriptedPollResponse::Json(serde_json::json!({ "state": "expired" })),
        )
        .await;
        let cancellation = CancellationToken::new();
        cancellation.cancel();

        let outcome =
            run_self_device_link_poll(&server.client(), "device-secret", "BCDF-2345", cancellation)
                .await;

        assert!(matches!(outcome, SelfDeviceLinkPollOutcome::Expired));
        assert_eq!(server.requests().len(), 2);
    }

    #[tokio::test]
    async fn cancellation_reconciliation_fails_on_not_found() {
        let server = DeviceLinkServer::spawn(
            axum::http::StatusCode::NO_CONTENT,
            ScriptedPollResponse::NotFound,
        )
        .await;
        let cancellation = CancellationToken::new();
        cancellation.cancel();

        let outcome =
            run_self_device_link_poll(&server.client(), "device-secret", "BCDF-2345", cancellation)
                .await;

        assert!(matches!(outcome, SelfDeviceLinkPollOutcome::Failed));
        assert_eq!(server.requests().len(), 2);
    }

    #[tokio::test]
    async fn cancellation_reconciliation_fails_on_unknown_state() {
        let server = DeviceLinkServer::spawn(
            axum::http::StatusCode::NO_CONTENT,
            ScriptedPollResponse::Json(serde_json::json!({ "state": "future" })),
        )
        .await;
        let cancellation = CancellationToken::new();
        cancellation.cancel();

        let outcome =
            run_self_device_link_poll(&server.client(), "device-secret", "BCDF-2345", cancellation)
                .await;

        assert!(matches!(outcome, SelfDeviceLinkPollOutcome::Failed));
    }

    #[tokio::test]
    async fn approval_can_win_cancellation_reconciliation() {
        let approver = generate_device_keys_for_test(USER_ID).expect("approver keys");
        let local = generate_device_keys_for_test(USER_ID).expect("local keys");
        let bundle = approver_signed_bundle(&approver, &local, &local);
        let server = DeviceLinkServer::spawn(
            axum::http::StatusCode::CONFLICT,
            ScriptedPollResponse::Json(serde_json::json!({
                "state": "approved",
                "deviceListGeneration": 2,
                "identityBundle": bundle,
            })),
        )
        .await;
        let cancellation = CancellationToken::new();
        cancellation.cancel();

        let outcome =
            run_self_device_link_poll(&server.client(), "device-secret", "BCDF-2345", cancellation)
                .await;
        let SelfDeviceLinkPollOutcome::Approved { generation, bundle } = outcome else {
            panic!("approval must win cancellation reconciliation");
        };
        let (_dir, pin_store) = empty_bound_pin_store(USER_ID).await;
        let resolved = finish_approved_self_link(
            &server.client(),
            &pin_store,
            USER_ID,
            &local,
            "device-secret",
            generation,
            &bundle,
        )
        .await;

        assert_eq!(resolved, SelfDeviceLinkOutcome::Approved);
        assert_eq!(
            server.requests(),
            [
                "DELETE /api/devices/link/requests/BCDF-2345",
                "POST /api/devices/link/poll device-secret",
                "POST /api/devices/link/ack device-secret",
            ]
        );
    }

    #[tokio::test]
    async fn peer_enrollment_pins_approver_signed_bundle_with_exact_local_keys() {
        let approver = generate_device_keys_for_test(USER_ID).expect("approver keys");
        let local = generate_device_keys_for_test(USER_ID).expect("local keys");
        let bundle = approver_signed_bundle(&approver, &local, &local);
        let (_dir, pin_store) = empty_bound_pin_store(USER_ID).await;

        complete_peer_enrollment(&bundle, &pin_store, USER_ID, &local, 2)
            .await
            .expect("peer enrollment");

        let pinned = pin_store
            .identity_bundle(USER_ID)
            .await
            .expect("pin lookup")
            .expect("pinned identity");
        assert_eq!(pinned.device_list.generation, 2);
        assert_eq!(pinned.device_list.signer_device_id, approver.device_id);
        let local_pin = pinned
            .devices
            .iter()
            .find(|device| device.device_id == local.device_id)
            .expect("local device pin");
        assert_eq!(
            BASE64
                .decode(&local_pin.signing_public_key)
                .expect("signing key"),
            local.signing_public_bytes()
        );
        assert_eq!(
            BASE64.decode(&local_pin.kem_public_key).expect("KEM key"),
            local.kem_public_bytes()
        );
    }

    #[tokio::test]
    async fn peer_enrollment_rejects_negative_committed_generation_without_pinning() {
        let approver = generate_device_keys_for_test(USER_ID).expect("approver keys");
        let local = generate_device_keys_for_test(USER_ID).expect("local keys");
        let bundle = approver_signed_bundle(&approver, &local, &local);
        let (_dir, pin_store) = empty_bound_pin_store(USER_ID).await;

        let error = complete_peer_enrollment(&bundle, &pin_store, USER_ID, &local, -1)
            .await
            .expect_err("negative generation must fail");

        std::assert_matches!(
            error,
            AppError::InvalidBackendData { ref field, .. }
                if field == "deviceLink.deviceListGeneration"
        );
        assert_no_pin(&pin_store).await;
    }

    #[tokio::test]
    async fn peer_enrollment_rejects_wrong_generation_without_pinning() {
        let approver = generate_device_keys_for_test(USER_ID).expect("approver keys");
        let local = generate_device_keys_for_test(USER_ID).expect("local keys");
        let bundle = approver_signed_bundle(&approver, &local, &local);
        let (_dir, pin_store) = empty_bound_pin_store(USER_ID).await;

        let error = complete_peer_enrollment(&bundle, &pin_store, USER_ID, &local, 3)
            .await
            .expect_err("wrong generation must fail");

        std::assert_matches!(
            error,
            AppError::InvalidBackendData { ref field, .. }
                if field == "identity.deviceList.generation"
        );
        assert_no_pin(&pin_store).await;
    }

    #[tokio::test]
    async fn peer_enrollment_rejects_substituted_local_keys_without_pinning() {
        let approver = generate_device_keys_for_test(USER_ID).expect("approver keys");
        let local = generate_device_keys_for_test(USER_ID).expect("local keys");
        let substituted = generate_device_keys_for_test(USER_ID).expect("substituted keys");
        let bundle = approver_signed_bundle(&approver, &local, &substituted);
        let (_dir, pin_store) = empty_bound_pin_store(USER_ID).await;

        let error = complete_peer_enrollment(&bundle, &pin_store, USER_ID, &local, 2)
            .await
            .expect_err("substituted keys must fail");

        std::assert_matches!(
            error,
            AppError::InvalidBackendData { ref field, .. } if field == "identity.devices"
        );
        assert_no_pin(&pin_store).await;
    }

    #[tokio::test]
    async fn peer_enrollment_rejects_self_signed_bootstrap_shape_without_pinning() {
        let local = generate_device_keys_for_test(USER_ID).expect("local keys");
        let bundle = bootstrap_bundle(&local);
        let (_dir, pin_store) = empty_bound_pin_store(USER_ID).await;

        let error = complete_peer_enrollment(&bundle, &pin_store, USER_ID, &local, 1)
            .await
            .expect_err("peer enrollment cannot accept self-signed bootstrap");

        std::assert_matches!(
            error,
            AppError::InvalidBackendData { ref field, .. } if field == "identity.devices"
        );
        assert_no_pin(&pin_store).await;
    }

    fn approver_signed_bundle(
        approver: &DeviceKeys,
        linked_identity: &DeviceKeys,
        submitted_keys: &DeviceKeys,
    ) -> UserIdentityBundleDto {
        let now_ms = now_ms();
        let approver_signer = approver.signing_key().expect("approver signer");
        let approver_cert = build_self_cert(
            USER_ID,
            &approver.device_id,
            "Approver",
            approver.kem_public_bytes(),
            &approver_signer,
            now_ms,
            None,
        )
        .expect("approver cert");
        let bootstrap =
            build_bootstrap_list(USER_ID, &approver.device_id, &approver_signer, now_ms, None)
                .expect("bootstrap list");
        let approval = build_approval_request(
            USER_ID,
            approver,
            1,
            &bootstrap.list.entries,
            &PendingDeviceLink {
                device_id: linked_identity.device_id.clone(),
                device_label: "Linked device".to_owned(),
                kem_public_key: BASE64.encode(submitted_keys.kem_public_bytes()),
                signing_public_key: BASE64.encode(submitted_keys.signing_public_bytes()),
            },
            "TEST-CODE",
        )
        .expect("approval bundle");
        let linked_cert_body = BASE64
            .decode(&approval.device_certificate)
            .expect("linked cert");
        let linked_cert_signature = BASE64
            .decode(&approval.device_certificate_signature)
            .expect("linked signature");

        UserIdentityBundleDto {
            user_id: USER_ID.to_owned(),
            identity_revision: 1,
            identity_incarnation_id: uuid::Uuid::from_u128(
                0x0190_0000_0000_7000_8000_0000_0000_0001,
            ),
            device_list: UserDeviceListDto {
                body: approval.signed_device_list,
                signature: approval.signed_device_list_signature,
            },
            devices: vec![
                device_dto(&approver_cert.body_bytes, &approver_cert.signature),
                device_dto(&linked_cert_body, &linked_cert_signature),
            ],
            historical_devices: vec![],
        }
    }

    fn bootstrap_bundle(local: &DeviceKeys) -> UserIdentityBundleDto {
        let now_ms = now_ms();
        let signer = local.signing_key().expect("local signer");
        let cert = build_self_cert(
            USER_ID,
            &local.device_id,
            "Local",
            local.kem_public_bytes(),
            &signer,
            now_ms,
            None,
        )
        .expect("self cert");
        let list = build_bootstrap_list(USER_ID, &local.device_id, &signer, now_ms, None)
            .expect("bootstrap list");
        UserIdentityBundleDto {
            user_id: USER_ID.to_owned(),
            identity_revision: 1,
            identity_incarnation_id: uuid::Uuid::from_u128(
                0x0190_0000_0000_7000_8000_0000_0000_0001,
            ),
            device_list: UserDeviceListDto {
                body: BASE64.encode(&list.body_bytes),
                signature: BASE64.encode(&list.signature),
            },
            devices: vec![device_dto(&cert.body_bytes, &cert.signature)],
            historical_devices: vec![],
        }
    }

    fn device_dto(cert_body: &[u8], cert_signature: &[u8]) -> UserDeviceCertificateDto {
        UserDeviceCertificateDto {
            certificate: BASE64.encode(cert_body),
            certificate_signature: BASE64.encode(cert_signature),
        }
    }

    async fn empty_bound_pin_store(user_id: &str) -> (tempfile::TempDir, DeviceListPinStoreHandle) {
        let dir = tempfile::tempdir().expect("pin directory");
        let store =
            DeviceListPinStore::load_from(dir.path().join("pins.json")).expect("empty pin store");
        let handle = DeviceListPinStoreHandle::from_store(store).expect("pin actor");
        handle.bind_to_user(user_id).await.expect("bind pin actor");
        (dir, handle)
    }

    async fn assert_no_pin(pin_store: &DeviceListPinStoreHandle) {
        assert!(
            pin_store
                .identity_bundle(USER_ID)
                .await
                .expect("pin lookup")
                .is_none()
        );
    }

    fn now_ms() -> u64 {
        u64::try_from(OffsetDateTime::now_utc().unix_timestamp()).expect("timestamp") * 1_000
    }
}
