use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use tokio_util::sync::CancellationToken;

use super::{
    SharingMaintenanceBackend, SharingMaintenanceFuture, record_shared_audience,
    reserve_host_relay_ranges_or_dispose, run_relay_hub_bridge,
};
use crate::{
    AppError,
    config::AppConfig,
    discovery::state::RemoteSessionRecord,
    runtime::{Runtime, pending_work::BackendScopeRestore},
    sharing::{
        backend_adapters::BackendSessionDetail, scope::SelectedRoom,
        shared_session_registry::SharedSessionState,
    },
    terminal_transport::{
        TerminalCapability, TerminalControlFrame, TerminalSurface, hub::SessionHub,
    },
};
use kodosi_backend_client::{labels::BackendToolKind, relay};
use kodosi_domain::{
    auth::AuthState,
    ids::{SessionId, UserId},
    permissions::{AccessLevel, ShareScope},
    session::{SessionRole, SessionState, SessionSummary},
    terminal::TerminalSize,
};

fn selected_room() -> SelectedRoom {
    SelectedRoom {
        id: "01900000-0000-7000-8000-0000000000aa".to_owned(),
        name: "Acme".to_owned(),
    }
}

fn backend_incarnation_id() -> uuid::Uuid {
    uuid::Uuid::from_u128(1)
}

async fn receive_relay_event(
    rx: &mut tokio::sync::mpsc::Receiver<relay::HostRelayTerminalEvent>,
) -> relay::HostRelayTerminalEvent {
    tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("relay bridge should resume within its bounded delay")
        .expect("relay trigger channel should stay open")
}

#[tokio::test]
async fn provisional_relay_bridge_rolls_back_idle_subscriber_on_drop() {
    let mut hub = SessionHub::new();
    let id = SessionId::new();
    assert!(hub.open_local_incarnation(id, uuid::Uuid::now_v7(), 0));
    let handle = hub
        .register(id, TerminalSurface::RelayHost, TerminalCapability::ReadOnly)
        .expect("session should be live");
    let cancellation = CancellationToken::new();
    let guard = super::ProvisionalRelayBridge {
        hub: hub.clone(),
        session_id: id,
        connection_id: handle.connection_id,
        cancellation: cancellation.clone(),
        armed: true,
    };

    drop(guard);

    assert!(cancellation.is_cancelled());
    assert_eq!(hub.connection_count(id), 0);
}

#[tokio::test]
async fn disarmed_provisional_relay_bridge_transfers_subscriber_ownership() {
    let mut hub = SessionHub::new();
    let id = SessionId::new();
    assert!(hub.open_local_incarnation(id, uuid::Uuid::now_v7(), 0));
    let handle = hub
        .register(id, TerminalSurface::RelayHost, TerminalCapability::ReadOnly)
        .expect("session should be live");
    let cancellation = CancellationToken::new();
    let mut guard = super::ProvisionalRelayBridge {
        hub: hub.clone(),
        session_id: id,
        connection_id: handle.connection_id,
        cancellation: cancellation.clone(),
        armed: true,
    };
    guard.disarm();

    drop(guard);

    assert!(!cancellation.is_cancelled());
    assert_eq!(hub.connection_count(id), 1);
    hub.unregister(id, handle.connection_id);
}

#[tokio::test]
async fn relay_bridge_drains_raw_frames_before_terminal_close() {
    let mut hub = SessionHub::new();
    let id = SessionId::new();
    assert!(hub.open_local_incarnation(id, uuid::Uuid::now_v7(), 0));
    let handle = hub
        .register(id, TerminalSurface::RelayHost, TerminalCapability::ReadOnly)
        .expect("session should be live");
    let (relay_tx, mut relay_rx) = tokio::sync::mpsc::channel(4);
    let cancellation = CancellationToken::new();
    let bridge = tokio::spawn(run_relay_hub_bridge(
        hub.clone(),
        id,
        handle,
        relay_tx,
        cancellation,
    ));

    hub.publish(id, bytes::Bytes::from_static(b"first"));
    hub.publish(id, bytes::Bytes::from_static(b"second"));
    hub.end_session(
        id,
        &crate::terminal_transport::TerminalCloseReason::SessionEnded,
    );

    std::assert_matches!(
        receive_relay_event(&mut relay_rx).await,
        relay::HostRelayTerminalEvent::Raw { sequence: 0, ref bytes } if bytes.as_ref() == b"first"
    );
    std::assert_matches!(
        receive_relay_event(&mut relay_rx).await,
        relay::HostRelayTerminalEvent::Raw { sequence: 1, ref bytes } if bytes.as_ref() == b"second"
    );
    std::assert_matches!(
        receive_relay_event(&mut relay_rx).await,
        relay::HostRelayTerminalEvent::Closed { final_sequence: 2 }
    );
    bridge.await.expect("bridge task");
}

#[tokio::test]
async fn relay_bridge_close_flush_honors_downstream_backpressure() {
    let mut hub = SessionHub::new();
    let id = SessionId::new();
    assert!(hub.open_local_incarnation(id, uuid::Uuid::now_v7(), 0));
    let handle = hub
        .register(id, TerminalSurface::RelayHost, TerminalCapability::ReadOnly)
        .expect("session should be live");
    let (relay_tx, mut relay_rx) = tokio::sync::mpsc::channel(1);
    let cancellation = CancellationToken::new();
    let bridge = tokio::spawn(run_relay_hub_bridge(
        hub.clone(),
        id,
        handle,
        relay_tx,
        cancellation,
    ));

    hub.publish(id, bytes::Bytes::from_static(b"first"));
    hub.publish(id, bytes::Bytes::from_static(b"second"));
    hub.end_session(
        id,
        &crate::terminal_transport::TerminalCloseReason::SessionEnded,
    );
    tokio::task::yield_now().await;
    assert!(
        !bridge.is_finished(),
        "bridge must wait rather than drop final data"
    );

    std::assert_matches!(
        receive_relay_event(&mut relay_rx).await,
        relay::HostRelayTerminalEvent::Raw { sequence: 0, .. }
    );
    std::assert_matches!(
        receive_relay_event(&mut relay_rx).await,
        relay::HostRelayTerminalEvent::Raw { sequence: 1, .. }
    );
    std::assert_matches!(
        receive_relay_event(&mut relay_rx).await,
        relay::HostRelayTerminalEvent::Closed { final_sequence: 2 }
    );
    bridge.await.expect("bridge task");
}

#[tokio::test]
async fn relay_bridge_drains_control_bursts_without_eviction() {
    let mut hub = SessionHub::new();
    let id = SessionId::new();
    assert!(hub.open_local_incarnation(id, uuid::Uuid::now_v7(), 0));
    let handle = hub
        .register(id, TerminalSurface::RelayHost, TerminalCapability::ReadOnly)
        .expect("session should be live");
    let original_connection = handle.connection_id;
    let (relay_tx, mut relay_rx) = tokio::sync::mpsc::channel(64);
    let cancellation = CancellationToken::new();
    let bridge = tokio::spawn(run_relay_hub_bridge(
        hub.clone(),
        id,
        handle,
        relay_tx,
        cancellation.clone(),
    ));

    for at_sequence in 0..32 {
        hub.broadcast_control(
            id,
            &TerminalControlFrame::Resize {
                rows: 24,
                cols: 80,
                at_sequence,
            },
        );
        tokio::task::yield_now().await;
    }

    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while relay_rx.len() < 32 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the bridge should continuously drain more than control capacity");
    assert!(
        hub.has_connection(id, original_connection),
        "drained control traffic must not evict the relay subscriber"
    );

    hub.publish(id, bytes::Bytes::from_static(b"after-control"));
    let mut saw_data = false;
    for _ in 0..33 {
        if matches!(
            receive_relay_event(&mut relay_rx).await,
            relay::HostRelayTerminalEvent::Raw { ref bytes, .. }
                if bytes.as_ref() == b"after-control"
        ) {
            saw_data = true;
            break;
        }
    }
    assert!(
        saw_data,
        "future terminal output must still trigger a snapshot"
    );

    cancellation.cancel();
    bridge.await.expect("bridge task");
    assert_eq!(hub.connection_count(id), 0);
}

#[tokio::test]
async fn relay_bridge_resubscribes_after_data_lane_eviction() {
    let mut hub = SessionHub::new();
    let id = SessionId::new();
    assert!(hub.open_local_incarnation(id, uuid::Uuid::now_v7(), 0));
    let handle = hub
        .register(id, TerminalSurface::RelayHost, TerminalCapability::ReadOnly)
        .expect("session should be live");
    let original_connection = handle.connection_id;
    let (relay_tx, mut relay_rx) = tokio::sync::mpsc::channel(1);
    let cancellation = CancellationToken::new();
    let bridge = tokio::spawn(run_relay_hub_bridge(
        hub.clone(),
        id,
        handle,
        relay_tx,
        cancellation.clone(),
    ));

    for _ in 0..=256 {
        hub.publish(id, bytes::Bytes::from_static(b"burst"));
    }
    assert!(
        !hub.has_connection(id, original_connection),
        "a full bounded data lane should evict only the lagging subscriber"
    );

    let first_event = receive_relay_event(&mut relay_rx).await;
    assert!(
        matches!(
            first_event,
            relay::HostRelayTerminalEvent::Raw { .. }
                | relay::HostRelayTerminalEvent::ForceCheckpoint
                | relay::HostRelayTerminalEvent::ForcePresentation
        ),
        "eviction recovery may surface only raw data or explicit recovery controls"
    );
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if hub.connection_count(id) == 1 && !hub.has_connection(id, original_connection) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("bridge should deterministically install one replacement subscriber");
    assert_eq!(
        hub.connection_count(id),
        1,
        "resubscribe must not leak handles"
    );

    hub.publish(id, bytes::Bytes::from_static(b"future"));
    let future = receive_relay_event(&mut relay_rx).await;
    assert!(matches!(
        future,
        relay::HostRelayTerminalEvent::Raw { ref bytes, .. } if bytes.as_ref() == b"future"
    ));

    cancellation.cancel();
    bridge.await.expect("bridge task");
    assert_eq!(hub.connection_count(id), 0);
}

struct MockSharingBackend {
    calls: Mutex<Vec<String>>,
    restore_failures: AtomicUsize,
    stale_restore: AtomicBool,
    terminalize_during_restore: AtomicBool,
    saw_in_flight_restore_identity: AtomicBool,
}

impl MockSharingBackend {
    fn new(restore_failures: usize) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            restore_failures: AtomicUsize::new(restore_failures),
            stale_restore: AtomicBool::new(false),
            terminalize_during_restore: AtomicBool::new(false),
            saw_in_flight_restore_identity: AtomicBool::new(false),
        }
    }

    fn with_stale_restore() -> Self {
        Self {
            stale_restore: AtomicBool::new(true),
            ..Self::new(0)
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("calls lock").clone()
    }

    fn consume_failure(counter: &AtomicUsize) -> bool {
        counter
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |remaining| {
                (remaining > 0).then(|| remaining - 1)
            })
            .is_ok()
    }
}

impl SharingMaintenanceBackend for MockSharingBackend {
    fn restore_scope<'a>(
        &'a self,
        app: &'a mut Runtime,
        repair: &'a BackendScopeRestore,
    ) -> SharingMaintenanceFuture<'a> {
        Box::pin(async move {
            tokio::task::yield_now().await;
            if app
                .state
                .pending_work
                .backend_scope_restore(repair.session_id)
                == Some(repair)
            {
                self.saw_in_flight_restore_identity
                    .store(true, Ordering::SeqCst);
            }
            self.calls.lock().expect("calls lock").push(format!(
                "patch:{}",
                kodosi_backend_client::labels::share_scope_label(repair.scope)
            ));
            if self
                .terminalize_during_restore
                .swap(false, Ordering::SeqCst)
            {
                app.state
                    .local
                    .sessions
                    .update_state(repair.session_id, SessionState::Stopped);
                app.state.consume_terminal_scope_restore(repair.session_id);
                app.state.sharing.shared_sessions.clear(repair.session_id);
            }
            if self.stale_restore.swap(false, Ordering::SeqCst) {
                return Err(AppError::NotFound);
            }
            if Self::consume_failure(&self.restore_failures) {
                return Err(AppError::Unsupported {
                    reason: "mock scope restore failure".to_owned(),
                });
            }
            Ok(())
        })
    }

    fn rotate_key<'a>(
        &'a self,
        _app: &'a mut Runtime,
        _id: SessionId,
    ) -> SharingMaintenanceFuture<'a> {
        Box::pin(async move {
            self.calls
                .lock()
                .expect("calls lock")
                .push("claim".to_owned());
            Err(AppError::Unsupported {
                reason: "mock key claim failure".to_owned(),
            })
        })
    }
}

fn hosting_app(scope: ShareScope, shared: bool) -> (Runtime, SessionId) {
    let mut config = AppConfig::default();
    config.auth.keyring_service = format!("kodosi.test.{}", uuid::Uuid::now_v7());
    config.backend.api = Some("http://127.0.0.1:9".to_owned());
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    app.state.identity.auth = AuthState::Authenticated {
        subject: Some(
            UserId::try_from("11111111-1111-1111-1111-111111111111")
                .unwrap_or_else(|error| panic!("test user id should parse: {error}")),
        ),
        expires_at: time::OffsetDateTime::now_utc(),
    };

    let id = SessionId::new();
    let mut summary = SessionSummary::new_owned(
        id,
        "test".to_owned(),
        "test-runtime".to_owned(),
        TerminalSize::default(),
        app.state.identity.auth.subject(),
    );
    summary.scope = scope;
    summary.state = SessionState::Running;
    app.state.local.sessions.insert(summary);

    if shared {
        app.state.sharing.shared_sessions.insert(
            id,
            SharedSessionState::new(
                "backend-session-1".to_owned(),
                backend_incarnation_id(),
                "owner-secret".to_owned(),
                scope,
                None,
                Some([3; 32]),
                Some(1),
            ),
        );
    }
    (app, id)
}

fn scope_of(app: &Runtime, id: SessionId) -> ShareScope {
    app.state
        .local
        .sessions
        .record(id)
        .map(|record| record.summary.scope)
        .expect("session record")
}

#[test]
fn nonce_exhaustion_queues_key_rotation_instead_of_retrying_the_generation() {
    let (mut app, id) = hosting_app(ShareScope::MyDevices, true);
    app.state
        .sharing
        .shared_sessions
        .reserve_frame_nonce_block(id, u64::MAX)
        .expect("consume nonce space");

    let error = reserve_host_relay_ranges_or_dispose(&mut app, id, 1)
        .expect_err("the exhausted generation must not restart");

    assert!(error.to_string().contains("key rotation queued"));
    assert_eq!(app.state.pending_work.drain_host_key_rotations(), vec![id]);
    assert_eq!(
        app.state
            .local
            .sessions
            .record(id)
            .map(|record| record.summary.state),
        Some(SessionState::Reconnecting)
    );
    assert_eq!(
        app.state.host_ws_status,
        kodosi_domain::lifecycle::ConnectionState::Reconnecting
    );
}

#[test]
fn revision_exhaustion_terminalizes_relay_retry_for_the_incarnation() {
    let (mut app, id) = hosting_app(ShareScope::MyDevices, true);
    app.state
        .sharing
        .shared_sessions
        .reserve_frame_revision_block(
            id,
            crate::sharing::shared_session_registry::MAX_FRAME_REVISION_EXCLUSIVE - 1,
        )
        .expect("consume revision space");

    let error = reserve_host_relay_ranges_or_dispose(&mut app, id, 1)
        .expect_err("the exhausted incarnation must not restart");

    assert!(error.to_string().contains("session incarnation"));
    assert!(app.state.pending_work.drain_host_key_rotations().is_empty());
    assert_eq!(
        app.state
            .local
            .sessions
            .record(id)
            .map(|record| record.summary.state),
        Some(SessionState::Failed)
    );
    assert_eq!(
        app.state.host_ws_status,
        kodosi_domain::lifecycle::ConnectionState::Offline
    );
}

#[test]
fn owned_remote_update_keeps_owner_effective_access() {
    let (mut app, _) = hosting_app(ShareScope::JustMe, false);
    let id = SessionId::new();
    let mut summary = SessionSummary::new_remote(
        id,
        "owned remote".to_owned(),
        "You".to_owned(),
        app.state.identity.auth.subject(),
        ShareScope::MyDevices,
        AccessLevel::Inject,
        TerminalSize::default(),
    );
    summary.role = SessionRole::Owner;
    app.state.discovery.replace_remote_sessions(
        vec![RemoteSessionRecord {
            summary,
            incarnation_id: None,
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

    super::backend_session::apply_remote_session_detail(
        &mut app,
        id,
        &BackendSessionDetail {
            id: id.to_string(),
            incarnation_id: backend_incarnation_id(),
            title: "updated".to_owned(),
            scope: ShareScope::Room,
            room_id: Some("room-1".to_owned()),
            default_access: AccessLevel::View,
            effective_access: Some(AccessLevel::Suggest),
            status: SessionState::Running,
        },
        None,
    );

    let record = app
        .state
        .discovery
        .session(id)
        .expect("owned remote record");
    assert_eq!(record.summary.access, AccessLevel::Inject);
    assert_eq!(record.summary.role, SessionRole::Owner);
}

#[test]
fn create_and_update_response_adapters_preserve_backend_effective_access() {
    for effective_access in [AccessLevel::Inject, AccessLevel::Approve] {
        let response = kodosi_backend_client::api::BackendSessionDetail {
            id: SessionId::new().to_string(),
            incarnation_id: backend_incarnation_id(),
            incarnation_generation: 1,
            incarnation_protocol_version: 2,
            owner_user_id: "11111111-1111-1111-1111-111111111111".to_owned(),
            title: "session".to_owned(),
            tool_kind: BackendToolKind::Generic,
            scope: ShareScope::MyDevices,
            room_id: None,
            default_access: AccessLevel::View,
            effective_access: Some(effective_access),
            status: SessionState::Running,
            started_at: "2026-08-05T00:00:00Z".to_owned(),
            ended_at: None,
            last_heartbeat_at: "2026-08-05T00:00:00Z".to_owned(),
        };

        let detail = BackendSessionDetail::from(response);

        assert_eq!(detail.default_access, AccessLevel::View);
        assert_eq!(detail.effective_access, Some(effective_access));
    }
}

#[tokio::test]
async fn the_recorded_audience_leads_the_local_record() {
    let (mut app, id) = hosting_app(ShareScope::MyDevices, true);
    let room = selected_room();

    record_shared_audience(&mut app, id, ShareScope::Room, Some(&room))
        .expect("the audience is recorded on a shared session");

    let shared = app
        .state
        .sharing
        .shared_sessions
        .get(id)
        .expect("shared session");
    assert_eq!(shared.scope(), ShareScope::Room);
    assert_eq!(
        shared.room().map(|room| room.id.as_str()),
        Some(room.id.as_str())
    );
    assert_eq!(
        scope_of(&app, id),
        ShareScope::MyDevices,
        "the local record must only move when the transition commits"
    );
}

#[tokio::test]
async fn recording_an_audience_for_an_unshared_session_fails_closed() {
    let (mut app, id) = hosting_app(ShareScope::JustMe, false);

    let error = record_shared_audience(&mut app, id, ShareScope::Room, Some(&selected_room()))
        .expect_err("there is no share to record an audience against");

    std::assert_matches!(error, AppError::NoActiveSession);
}

fn add_owned_remote_twin(app: &mut Runtime, id: SessionId, scope: ShareScope) {
    let mut summary = kodosi_domain::session::SessionSummary::new_remote(
        id,
        "test".to_owned(),
        "You".to_owned(),
        None,
        scope,
        kodosi_domain::permissions::AccessLevel::Inject,
        TerminalSize::default(),
    );
    summary.role = kodosi_domain::session::SessionRole::Owner;
    app.state.discovery.replace_remote_sessions(
        vec![crate::discovery::RemoteSessionRecord {
            summary,
            incarnation_id: Some(backend_incarnation_id()),
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
}

#[test]
fn terminal_access_ack_requires_exact_fingerprint() {
    let (mut app, id) = hosting_app(ShareScope::MyDevices, false);
    let backend_session_id = "01900000-0000-7000-8000-000000000045";
    app.state.sharing.shared_sessions.insert(
        id,
        SharedSessionState::new(
            backend_session_id.to_owned(),
            backend_incarnation_id(),
            "owner-secret".to_owned(),
            ShareScope::MyDevices,
            None,
            Some([3; 32]),
            Some(1),
        ),
    );
    app.state.identity.auth = AuthState::Authenticated {
        subject: Some(UserId::try_from("01900000-0000-7000-8000-000000000001").unwrap()),
        expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    app.state
        .identity
        .set_account_epoch_for_test(crate::session_runtime::events::AccountEpoch::for_test(1));
    let incarnation = app
        .state
        .local
        .sessions
        .record(id)
        .unwrap()
        .local_incarnation_id;
    let (backend_session_id, backend_incarnation_id) =
        super::resolve_local_shared_backend_session_identity(&app, id).unwrap();
    let mut prepared = crate::runtime::access_mutations::PreparedSessionAccessMutation::new(
        uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000040").unwrap(),
        "01900000-0000-7000-8000-000000000001".to_owned(),
        app.state.identity.account_epoch().value(),
        id,
        incarnation,
        backend_session_id,
        backend_incarnation_id,
        crate::runtime::access_mutations::SessionAccessMutationTarget::Revoke {
            actor_user_id: uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000041").unwrap(),
        },
    )
    .unwrap();
    prepared.state = crate::runtime::access_mutations::PreparedSessionAccessMutationState::Terminal(
        crate::runtime::access_mutations::SessionAccessMutationTerminal {
            status: crate::runtime::access_mutations::SessionAccessMutationTerminalStatus::Applied,
            message: None,
        },
    );
    let mutation_id = prepared.mutation_id;
    let fingerprint = prepared.fingerprint.clone();
    app.access_mutations.put(prepared).unwrap();

    assert!(super::acknowledge_access_mutation(&mut app, mutation_id, &"0".repeat(64)).is_err());
    assert!(
        app.access_mutations
            .get("01900000-0000-7000-8000-000000000001", mutation_id)
            .unwrap()
            .is_some()
    );
    super::acknowledge_access_mutation(&mut app, mutation_id, &fingerprint).unwrap();
    assert!(
        app.access_mutations
            .get("01900000-0000-7000-8000-000000000001", mutation_id)
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn recovered_grant_retires_without_resurrecting_missing_shared_authority() {
    let (mut app, id) = hosting_app(ShareScope::MyDevices, false);
    app.state.sharing.shared_sessions.insert(
        id,
        SharedSessionState::new(
            "01900000-0000-7000-8000-000000000046".to_owned(),
            backend_incarnation_id(),
            "owner-secret".to_owned(),
            ShareScope::MyDevices,
            None,
            Some([3; 32]),
            Some(1),
        ),
    );
    app.state.identity.auth = AuthState::Authenticated {
        subject: Some(UserId::try_from("01900000-0000-7000-8000-000000000001").unwrap()),
        expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    app.state
        .identity
        .set_account_epoch_for_test(crate::session_runtime::events::AccountEpoch::for_test(1));
    let incarnation = app
        .state
        .local
        .sessions
        .record(id)
        .unwrap()
        .local_incarnation_id;
    let (backend_session_id, backend_incarnation_id) =
        super::resolve_local_shared_backend_session_identity(&app, id).unwrap();
    let prepared = crate::runtime::access_mutations::PreparedSessionAccessMutation::new(
        uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000041").unwrap(),
        "01900000-0000-7000-8000-000000000001".to_owned(),
        app.state.identity.account_epoch().value(),
        id,
        incarnation,
        backend_session_id,
        backend_incarnation_id,
        crate::runtime::access_mutations::SessionAccessMutationTarget::Grant {
            actor_user_id: uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000042").unwrap(),
            access_level: AccessLevel::View,
            expires_at_unix_ms: 1_800_000_000_000,
        },
    )
    .unwrap();
    assert!(app.state.sharing.shared_sessions.clear(id));

    super::resume_access_mutation_effects(&mut app, &prepared)
        .await
        .expect("missing key-bearing authority is already safe");
    assert!(app.state.sharing.shared_sessions.get(id).is_none());
}

#[tokio::test]
async fn recovered_access_effect_retires_without_touching_replacement_runtime_incarnation() {
    let (mut app, id) = hosting_app(ShareScope::MyDevices, false);
    app.state.sharing.shared_sessions.insert(
        id,
        SharedSessionState::new(
            "01900000-0000-7000-8000-000000000047".to_owned(),
            backend_incarnation_id(),
            "owner-secret".to_owned(),
            ShareScope::MyDevices,
            None,
            Some([3; 32]),
            Some(1),
        ),
    );
    app.state.identity.auth = AuthState::Authenticated {
        subject: Some(UserId::try_from("01900000-0000-7000-8000-000000000001").unwrap()),
        expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    app.state
        .identity
        .set_account_epoch_for_test(crate::session_runtime::events::AccountEpoch::for_test(1));
    let (backend_session_id, backend_incarnation_id) =
        super::resolve_local_shared_backend_session_identity(&app, id).unwrap();
    let prepared = crate::runtime::access_mutations::PreparedSessionAccessMutation::new(
        uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000043").unwrap(),
        "01900000-0000-7000-8000-000000000001".to_owned(),
        app.state.identity.account_epoch().value(),
        id,
        uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000099").unwrap(),
        backend_session_id,
        backend_incarnation_id,
        crate::runtime::access_mutations::SessionAccessMutationTarget::Revoke {
            actor_user_id: uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000044").unwrap(),
        },
    )
    .unwrap();

    let before = app
        .state
        .sharing
        .shared_sessions
        .get(id)
        .expect("replacement shared authority")
        .explicit_grantee_access()
        .clone();
    super::resume_access_mutation_effects(&mut app, &prepared)
        .await
        .expect("superseded predecessor effect is terminal");
    assert_eq!(
        app.state
            .sharing
            .shared_sessions
            .get(id)
            .expect("replacement shared authority")
            .explicit_grantee_access(),
        &before,
        "the predecessor revoke must not touch replacement authority"
    );
}

#[tokio::test]
async fn grant_and_revoke_reject_cleared_local_share_state_despite_owned_remote_twin() {
    for operation in ["grant", "revoke"] {
        let (mut app, id) = hosting_app(ShareScope::MyDevices, true);
        let incarnation = app
            .state
            .local
            .sessions
            .record(id)
            .expect("local session")
            .local_incarnation_id;
        add_owned_remote_twin(&mut app, id, ShareScope::MyDevices);
        assert!(app.state.sharing.shared_sessions.clear(id));

        let error = match operation {
            "grant" => super::grant_access(
                &mut app,
                id,
                incarnation,
                uuid::Uuid::now_v7(),
                "user-2",
                AccessLevel::View,
                "2026-08-14T00:00:00Z",
            )
            .expect_err("grant must require current local shared state"),
            "revoke" => {
                super::revoke_access(&mut app, id, incarnation, uuid::Uuid::now_v7(), "user-2")
                    .expect_err("revoke must require current local shared state")
            }
            _ => unreachable!(),
        };

        std::assert_matches!(error, AppError::NoActiveSession);
        assert!(
            app.state
                .discovery
                .session(id)
                .is_some_and(|record| record.incarnation_id == Some(backend_incarnation_id())),
            "the owned-remote discovery twin must not be mutated as a fallback"
        );
    }
}

#[tokio::test]
async fn maintenance_drops_nonlocal_scope_repairs_without_backend_patch() {
    let backend = MockSharingBackend::new(0);
    let (mut app, id) = hosting_app(ShareScope::MyDevices, false);
    assert!(app.state.local.sessions.delete(id));
    add_owned_remote_twin(&mut app, id, ShareScope::MyDevices);
    app.state
        .pending_work
        .queue_backend_scope_restore(BackendScopeRestore {
            session_id: id,
            backend_session_id: id.to_string(),
            incarnation_id: backend_incarnation_id(),
            scope: ShareScope::MyDevices,
            room: None,
            requires_key_rotation: false,
        });

    app.process_hosted_sharing_work_with(&backend).await;

    assert!(backend.calls().is_empty());
    assert!(app.state.pending_work.backend_scope_restore(id).is_none());
}

#[tokio::test]
async fn stale_incarnation_scope_restore_clears_local_share_and_never_requeues() {
    let backend = MockSharingBackend::with_stale_restore();
    let (mut app, id) = hosting_app(ShareScope::MyDevices, true);
    app.state
        .pending_work
        .queue_backend_scope_restore(BackendScopeRestore {
            session_id: id,
            backend_session_id: "backend-session-1".to_owned(),
            incarnation_id: backend_incarnation_id(),
            scope: ShareScope::MyDevices,
            room: None,
            requires_key_rotation: true,
        });
    app.state.pending_work.queue_host_key_rotation(id);
    app.state.pending_work.queue_key_redistribution(id);

    app.process_hosted_sharing_work_with(&backend).await;

    assert_eq!(backend.calls(), vec!["patch:MyDevices"]);
    assert!(app.state.pending_work.backend_scope_restore(id).is_none());
    assert!(app.state.sharing.shared_sessions.get(id).is_none());
    assert_eq!(scope_of(&app, id), ShareScope::JustMe);
    assert_eq!(
        app.state
            .local
            .sessions
            .record(id)
            .map(|record| record.summary.state),
        Some(SessionState::Running)
    );
    assert!(app.state.pending_work.drain_host_key_rotations().is_empty());
    assert!(
        app.state
            .pending_work
            .drain_key_redistributions()
            .is_empty()
    );

    app.process_hosted_sharing_work_with(&backend).await;
    assert_eq!(
        backend.calls(),
        vec!["patch:MyDevices"],
        "a stale incarnation is terminal and must not retry"
    );
}
