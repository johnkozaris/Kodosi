use std::collections::{BTreeMap, BTreeSet};

use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;

use super::Runtime;
use super::identity::account_runtimes::SelfDeviceLinkRuntime;
use crate::{
    AppError,
    config::AppConfig,
    discovery::{RemoteSessionRecord, ShelfItem},
    host_protocol::{AgentIntelEvent, TerminalEvent},
    identity_core::{
        device_cert::{build_self_cert, verify_certificate},
        device_flow::{DeviceFlowResult, TokenResponse},
        device_list_pin_store::{
            DeviceListPinStore, DeviceListPinStoreHandle, PinContext, PinVerdict,
        },
        identity_bundle_view::{IdentityBundleView, VerifiedDevice},
        signed_device_list::{build_bootstrap_list, verify_signed_device_list},
    },
    session_runtime::{
        events::{
            AccountEpoch, AccountEventOrigin, DiscoverySurface, LocalCoordinatorOrigin,
            RuntimeSessionEvent,
        },
        handles::{
            OwnedSessionHandle, SessionPtyInstruction, SessionScreenInstruction, SessionSenders,
        },
    },
    sharing::shared_session_registry::SharedSessionState,
    support::ui::clipboard::SystemClipboardBridge,
};
use kodosi_backend_client::user_events::UserEventsHandle;
use kodosi_domain::{
    auth::AuthState,
    ids::{SessionId, UserId},
    lifecycle::{ConnectionState, RemoteSessionAccessIssue, RemoteSessionAccessState, StopReason},
    permissions::{AccessLevel, ShareScope},
    session::{SessionState, SessionSummary},
    terminal::TerminalSize,
};
use tokio::sync::mpsc;

pub(crate) fn resize_identity_for_test(
    request_id: impl Into<String>,
    runtime_incarnation_id: impl Into<String>,
    subscription_id: impl Into<String>,
    subscription_generation: u64,
    surface_generation: u64,
    rows: u16,
    cols: u16,
) -> crate::host_protocol::TerminalResizeIdentity {
    crate::host_protocol::TerminalResizeIdentity {
        request_id: request_id.into(),
        expected_runtime_incarnation_id: runtime_incarnation_id.into(),
        subscription_id: subscription_id.into(),
        subscription_generation,
        surface_generation,
        cols,
        rows,
        width_pixels: u32::from(cols) * 10,
        height_pixels: u32::from(rows) * 20,
        cell_width_pixels: 10,
        cell_height_pixels: 20,
    }
}

fn claude_owned_summary(id: SessionId, working_dir: &str) -> SessionSummary {
    let mut summary = SessionSummary::new_owned(
        id,
        "Claude".to_owned(),
        "kodosi-test".to_owned(),
        TerminalSize::new(120, 40)
            .unwrap_or_else(|error| panic!("terminal size should be valid: {error}")),
        None,
    );
    summary.state = SessionState::Running;
    summary.working_dir = Some(working_dir.to_owned());
    summary.detected_agent = Some("Claude".to_owned());
    summary
}

fn test_user_id() -> UserId {
    UserId::try_from("11111111-1111-1111-1111-111111111111")
        .unwrap_or_else(|error| panic!("test user id should be valid: {error}"))
}

fn authenticate_test_app_as(app: &mut Runtime, subject: UserId) -> AccountEventOrigin {
    app.state
        .identity
        .advance_account_epoch()
        .expect("test account epoch should advance");
    app.state.identity.auth = AuthState::Authenticated {
        subject: Some(subject),
        expires_at: OffsetDateTime::now_utc(),
    };
    AccountEventOrigin {
        account_user_id: subject.to_string(),
        epoch: app.state.identity.account_epoch(),
    }
}

fn authenticate_test_app(app: &mut Runtime) -> AccountEventOrigin {
    authenticate_test_app_as(app, test_user_id())
}

fn current_account_origin(app: &Runtime) -> AccountEventOrigin {
    AccountEventOrigin {
        account_user_id: app
            .state
            .identity
            .auth
            .subject_string()
            .expect("test app should have an authenticated subject"),
        epoch: app.state.identity.account_epoch(),
    }
}

fn current_host_relay_origin(
    app: &Runtime,
    session_id: SessionId,
    relay_generation: u64,
) -> crate::session_runtime::events::HostRelayEventOrigin {
    crate::session_runtime::events::HostRelayEventOrigin {
        account_origin: current_account_origin(app),
        session_id,
        relay_generation,
    }
}

pub(crate) fn set_local_incarnation_for_test(
    app: &mut Runtime,
    session_id: SessionId,
    local_incarnation_id: uuid::Uuid,
) {
    app.state
        .local
        .sessions
        .record_mut(session_id)
        .expect("test session should exist")
        .local_incarnation_id = local_incarnation_id;
}

pub(crate) fn seed_local_coordinator_origin(
    app: &mut Runtime,
    session_id: SessionId,
) -> LocalCoordinatorOrigin {
    app.state.local.sessions.insert(SessionSummary::new_owned(
        session_id,
        "test".to_owned(),
        "kodosi-test".to_owned(),
        TerminalSize::default(),
        None,
    ));
    local_coordinator_origin(app, session_id)
}

pub(crate) fn local_coordinator_origin(
    app: &Runtime,
    session_id: SessionId,
) -> LocalCoordinatorOrigin {
    LocalCoordinatorOrigin {
        session_id,
        local_incarnation_id: app
            .state
            .local
            .sessions
            .record(session_id)
            .expect("test session should have coordinator origin")
            .local_incarnation_id,
    }
}

fn test_session_handle_with_runtime() -> (
    OwnedSessionHandle,
    CancellationToken,
    tokio::task::AbortHandle,
) {
    let (screen_tx, _screen_rx) = mpsc::channel::<SessionScreenInstruction>(1);
    let (pty_tx, _pty_rx) = mpsc::channel::<SessionPtyInstruction>(1);
    let cancellation = CancellationToken::new();
    let join_handle = tokio::spawn(std::future::pending::<()>());
    let abort_handle = join_handle.abort_handle();
    (
        OwnedSessionHandle {
            senders: SessionSenders::new(Some(screen_tx), Some(pty_tx)),
            cancellation: cancellation.clone(),
            join_handle,
        },
        cancellation,
        abort_handle,
    )
}

fn test_session_handle() -> OwnedSessionHandle {
    test_session_handle_with_runtime().0
}

fn insert_owned_session_for_test(
    app: &mut Runtime,
    summary: SessionSummary,
    handle: OwnedSessionHandle,
) {
    let id = summary.id;
    app.state.local.sessions.insert(summary);
    let incarnation = app
        .state
        .local
        .sessions
        .record(id)
        .expect("inserted test session")
        .local_incarnation_id;
    assert!(app.terminal_hub.open_local_incarnation(id, incarnation, 0));
    app.state
        .local
        .owned_session_runtimes
        .attach(id, incarnation, handle);
}

fn test_self_device_link_runtime_with_code(
    user_code: &str,
) -> (
    SelfDeviceLinkRuntime,
    CancellationToken,
    tokio::task::AbortHandle,
) {
    let cancellation = CancellationToken::new();
    let join_handle = tokio::spawn(std::future::pending::<()>());
    let abort_handle = join_handle.abort_handle();
    (
        SelfDeviceLinkRuntime {
            identity: crate::runtime::identity::account_runtimes::SelfDeviceLinkIdentity {
                origin: AccountEventOrigin {
                    account_user_id: test_user_id().to_string(),
                    epoch: AccountEpoch::for_test(1),
                },
                device_id: "device-local".to_owned(),
                device_label: "Test Mac".to_owned(),
                user_code: user_code.to_owned(),
                expires_at: "2099-08-18T09:00:00Z".to_owned(),
            },
            cancellation: cancellation.clone(),
            join_handle,
        },
        cancellation,
        abort_handle,
    )
}

fn test_self_device_link_runtime() -> (
    SelfDeviceLinkRuntime,
    CancellationToken,
    tokio::task::AbortHandle,
) {
    test_self_device_link_runtime_with_code("ABCD-EFGH")
}

fn test_user_events_handle() -> (
    UserEventsHandle,
    CancellationToken,
    tokio::task::AbortHandle,
) {
    let cancellation = CancellationToken::new();
    let join_handle = tokio::spawn(std::future::pending::<()>());
    let abort_handle = join_handle.abort_handle();
    (
        UserEventsHandle {
            cancellation: cancellation.clone(),
            join_handle,
        },
        cancellation,
        abort_handle,
    )
}

fn test_device_flow_poll_task() -> (CancellationToken, tokio::task::JoinHandle<()>) {
    (
        CancellationToken::new(),
        tokio::spawn(std::future::pending::<()>()),
    )
}

async fn wait_for_finished(abort_handle: &tokio::task::AbortHandle) {
    for _ in 0..10 {
        if abort_handle.is_finished() {
            return;
        }
        tokio::task::yield_now().await;
    }
    assert!(abort_handle.is_finished());
}

fn fake_identity_view(user_id: &str, device_id: &str) -> IdentityBundleView {
    use aws_lc_rs::signature::{KeyPair, ML_DSA_65_SIGNING, PqdsaKeyPair};

    let keypair = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).expect("test signing key");
    let kem_public_key = vec![5; 1184];
    let signed_certificate = build_self_cert(
        user_id,
        device_id,
        "Test Device",
        &kem_public_key,
        &keypair,
        1_700_000_000_000,
        None,
    )
    .expect("test certificate");
    let signed_list = build_bootstrap_list(user_id, device_id, &keypair, 1_700_000_000_000, None)
        .expect("test device list");
    let sig_public_key = keypair.public_key().as_ref().to_vec();
    let certificate = verify_certificate(
        &signed_certificate.body_bytes,
        &signed_certificate.signature,
        &sig_public_key,
    )
    .expect("verified test certificate");
    let parsed_list = verify_signed_device_list(
        &signed_list.body_bytes,
        &signed_list.signature,
        &sig_public_key,
    )
    .expect("verified test list");
    let mut devices = BTreeMap::new();
    devices.insert(
        device_id.to_owned(),
        VerifiedDevice {
            certificate,
            sig_public_key,
            certificate_body: signed_certificate.body_bytes,
            certificate_signature: signed_certificate.signature,
        },
    );
    IdentityBundleView {
        user_id: user_id.to_owned(),
        identity_revision: 1,
        identity_incarnation_id: uuid::Uuid::from_u128(0x0190_0000_0000_7000_8000_0000_0000_0001),
        signed_list: parsed_list,
        list_body: signed_list.body_bytes,
        list_signature: signed_list.signature,
        devices,
        historical_devices: BTreeMap::new(),
    }
}

fn test_shared_session_state() -> SharedSessionState {
    SharedSessionState::new(
        "backend-session-1".to_owned(),
        uuid::Uuid::from_u128(1),
        "owner-secret".to_owned(),
        kodosi_domain::permissions::ShareScope::MyDevices,
        None,
        Some([9; 32]),
        Some(1),
    )
}

fn app_with_shared_cached_session(state: SessionState) -> (Runtime, SessionId) {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let id = SessionId::new();
    let mut summary = claude_owned_summary(id, "/tmp/kodosi");
    summary.state = state;
    app.state.local.sessions.insert(summary);
    app.state
        .sharing
        .shared_sessions
        .insert(id, test_shared_session_state());
    app.state.sync_host_relay_status();
    let expected_status = if matches!(state, SessionState::Stopped | SessionState::Failed) {
        ConnectionState::Offline
    } else {
        ConnectionState::Connected
    };
    assert_eq!(app.state.host_ws_status, expected_status);
    (app, id)
}

fn test_app() -> Runtime {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"))
}

impl Runtime {
    fn insert_action_result_in_flight_for_test(&mut self, key: (SessionId, String, String, u64)) {
        self.action_results_in_flight.insert(key);
    }

    fn action_result_in_flight_for_test(&self, key: &(SessionId, String, String, u64)) -> bool {
        self.action_results_in_flight.contains(key)
    }

    fn insert_semantic_receipt_in_flight_for_test(&mut self, key: (SessionId, uuid::Uuid, u64)) {
        self.semantic_receipts_in_flight.insert(key);
    }

    fn semantic_receipt_in_flight_for_test(&self, key: &(SessionId, uuid::Uuid, u64)) -> bool {
        self.semantic_receipts_in_flight.contains(key)
    }

    fn set_share_transition_deadline_for_test(
        &mut self,
        transition_id: uuid::Uuid,
        deadline: std::time::Instant,
    ) {
        self.share_transition_deadlines
            .insert(transition_id, deadline);
    }

    fn install_share_transition_deadline_for_test(&mut self, transition_id: uuid::Uuid) {
        self.share_transition_deadlines.insert(
            transition_id,
            std::time::Instant::now() + std::time::Duration::from_secs(500),
        );
    }

    fn spawn_panicking_share_worker_for_test(
        &mut self,
        prepared: crate::runtime::share_transitions::PreparedShareTransition,
        mode: crate::runtime::share_transition_worker::ShareTransitionWorkerMode,
    ) -> crate::Result<()> {
        self.spawn_share_transition_worker(
            prepared,
            crate::runtime::share_transition_worker::ShareTransitionWorkerInput::Panic { mode },
        )
    }

    fn share_completion_matches_for_test(
        &self,
        prepared: &crate::runtime::share_transitions::PreparedShareTransition,
        completion_account_epoch: u64,
        mode: crate::runtime::share_transition_worker::ShareTransitionWorkerMode,
    ) -> bool {
        self.share_completion_matches(prepared, completion_account_epoch, mode)
    }

    fn finish_applied_share_transition_for_test(
        &mut self,
        prepared: &crate::runtime::share_transitions::PreparedShareTransition,
    ) -> crate::Result<()> {
        self.finish_share_transition(
            prepared,
            crate::runtime::share_transitions::ShareTransitionTerminalStatus::Applied,
            None,
            crate::runtime::share_transitions::DeferredShareVerdict::ScopeChanged,
        )
    }

    fn collaboration_change_active_for_test(
        &self,
        account_user_id: &str,
        session_id: SessionId,
    ) -> crate::Result<bool> {
        self.collaboration_change_active(account_user_id, session_id)
    }
}

mod account;
mod agent_intel;
mod discovery_session_shelf;
mod pin_reset;
mod relays;
mod runtime_outputs;
mod session_lifecycle;
mod share_state;
mod share_transitions;
mod steering_lifecycle;
mod terminal_focus_lifecycle;
