use super::*;
use crate::runtime::BackendReconciliationState;

fn other_test_user_id() -> UserId {
    UserId::try_from("22222222-2222-2222-2222-222222222222")
        .unwrap_or_else(|error| panic!("second test user id should be valid: {error}"))
}

fn queued_backend_access_invalid(
    origin: AccountEventOrigin,
    source: &'static str,
) -> RuntimeSessionEvent {
    RuntimeSessionEvent::BackendAccessInvalid {
        origin,
        reason: format!("stale {source} backend access invalid"),
    }
}

fn assert_stale_relay_invalidation_is_rejected_after_account_b(source: &'static str) {
    let mut app = test_app();
    let account_a = authenticate_test_app(&mut app);
    let stale_event = queued_backend_access_invalid(account_a, source);
    let account_b = authenticate_test_app_as(&mut app, other_test_user_id());

    app.handle_session_event(stale_event);

    assert_eq!(
        app.state.identity.auth.subject(),
        Some(other_test_user_id())
    );
    assert_eq!(app.state.identity.account_epoch(), account_b.epoch);
}

fn assert_stale_relay_invalidation_is_rejected_after_same_account_relogin(source: &'static str) {
    let mut app = test_app();
    let stale_origin = authenticate_test_app(&mut app);
    let stale_event = queued_backend_access_invalid(stale_origin, source);
    crate::runtime::auth::set_signed_out(&mut app).expect("test sign-out epoch should advance");
    let current_origin = authenticate_test_app(&mut app);

    app.handle_session_event(stale_event);

    assert_eq!(app.state.identity.auth.subject(), Some(test_user_id()));
    assert_eq!(app.state.identity.account_epoch(), current_origin.epoch);
}

#[test]
fn queued_host_relay_invalidation_is_rejected_after_account_b_signs_in() {
    assert_stale_relay_invalidation_is_rejected_after_account_b("host relay");
}

#[test]
fn queued_session_relay_invalidation_is_rejected_after_account_b_signs_in() {
    assert_stale_relay_invalidation_is_rejected_after_account_b("session relay");
}

#[test]
fn queued_host_relay_invalidation_is_rejected_after_same_account_relogin() {
    assert_stale_relay_invalidation_is_rejected_after_same_account_relogin("host relay");
}

#[test]
fn queued_session_relay_invalidation_is_rejected_after_same_account_relogin() {
    assert_stale_relay_invalidation_is_rejected_after_same_account_relogin("session relay");
}

#[test]
fn signed_out_normalization_does_not_advance_account_epoch() {
    let mut app = test_app();
    crate::runtime::auth::set_signed_out(&mut app).expect("initial sign-out should succeed");
    let signed_out_epoch = app.state.identity.account_epoch();
    app.state
        .runtime_outbox
        .queue_devices(crate::host_protocol::DeviceEvent::LinkSelfResolved {
            outcome: kodosi_domain::device_link::SelfDeviceLinkOutcome::Cancelled,
        });

    crate::runtime::auth::set_signed_out(&mut app)
        .expect("signed-out normalization should succeed");

    assert_eq!(app.state.identity.account_epoch(), signed_out_epoch);
    std::assert_matches!(
        app.state.runtime_outbox.drain_devices().as_slice(),
        [crate::host_protocol::DeviceEvent::LinkSelfResolved {
            outcome: kodosi_domain::device_link::SelfDeviceLinkOutcome::Cancelled,
        }]
    );
}

#[test]
fn account_epoch_exhaustion_is_reported_without_changing_epoch_or_auth() {
    let mut app = test_app();
    let origin = authenticate_test_app(&mut app);
    app.state
        .identity
        .set_account_epoch_for_test(AccountEpoch::for_test(u64::MAX));
    let auth_before = app.state.identity.auth.clone();

    let error = crate::runtime::auth::set_signed_out(&mut app)
        .expect_err("exhausted account epoch must reject the transition");

    std::assert_matches!(error, AppError::AccountEpochExhausted);
    assert_eq!(
        app.state.identity.account_epoch(),
        AccountEpoch::for_test(u64::MAX)
    );
    assert_eq!(app.state.identity.auth, auth_before);
    assert_ne!(origin.epoch, app.state.identity.account_epoch());
}

#[test]
fn authenticated_establishment_advances_epoch_before_bridge_origin_is_captured() {
    let mut app = test_app();
    assert_eq!(app.state.identity.account_epoch(), AccountEpoch::INITIAL);

    let origin = authenticate_test_app(&mut app);

    assert_ne!(origin.epoch, AccountEpoch::INITIAL);
    assert_eq!(origin, current_account_origin(&app));
    assert!(app.state.identity.accepts_account_event(&origin));
}

#[test]
fn account_a_websocket_event_is_rejected_after_account_b_signs_in() {
    let mut app = test_app();
    let account_a = authenticate_test_app(&mut app);
    let account_b = authenticate_test_app_as(&mut app, other_test_user_id());

    app.handle_session_event(RuntimeSessionEvent::BackendAccessInvalid {
        origin: account_a.clone(),
        reason: "account A access revoked".to_owned(),
    });
    app.handle_session_event(RuntimeSessionEvent::DeviceLinkRequested {
        origin: account_a,
        user_code: "AAAA-BBBB".to_owned(),
        device_label: "Account A device".to_owned(),
        expires_at: "later".to_owned(),
    });

    assert_eq!(
        app.state.identity.auth.subject(),
        Some(other_test_user_id())
    );
    assert_eq!(app.state.identity.account_epoch(), account_b.epoch);
    assert!(app.state.runtime_outbox.drain_devices().is_empty());
}

#[test]
fn pre_logout_account_a_event_is_rejected_after_account_a_relogin() {
    let mut app = test_app();
    let stale_origin = authenticate_test_app(&mut app);
    crate::runtime::auth::set_signed_out(&mut app).expect("test sign-out epoch should advance");
    let current_origin = authenticate_test_app(&mut app);

    assert_ne!(stale_origin.epoch, current_origin.epoch);
    app.handle_session_event(RuntimeSessionEvent::DeviceLinkRequested {
        origin: stale_origin,
        user_code: "OLD1-CODE".to_owned(),
        device_label: "Stale device".to_owned(),
        expires_at: "later".to_owned(),
    });

    assert!(app.state.runtime_outbox.drain_devices().is_empty());
}

#[test]
fn current_account_epoch_link_event_reaches_ui_outbox() {
    let mut app = test_app();
    let origin = authenticate_test_app(&mut app);

    app.handle_session_event(RuntimeSessionEvent::DeviceLinkRequested {
        origin,
        user_code: "LIVE-CODE".to_owned(),
        device_label: "Current device".to_owned(),
        expires_at: "later".to_owned(),
    });

    std::assert_matches!(
        app.state.runtime_outbox.drain_devices().as_slice(),
        [crate::host_protocol::DeviceEvent::LinkRequested {
            user_code,
            device_label,
            expires_at,
        }] if user_code == "LIVE-CODE"
            && device_label == "Current device"
            && expires_at == "later"
    );
}

#[test]
fn stale_resolved_link_event_cannot_reach_ui_outbox() {
    let mut app = test_app();
    let stale_origin = authenticate_test_app(&mut app);
    authenticate_test_app_as(&mut app, other_test_user_id());

    app.handle_session_event(RuntimeSessionEvent::DeviceLinkResolved {
        origin: stale_origin,
        user_code: "OLD2-CODE".to_owned(),
        outcome: kodosi_domain::device_link::DeviceLinkOutcome::Approved,
    });

    assert!(app.state.runtime_outbox.drain_devices().is_empty());
}

#[tokio::test]
async fn approved_self_link_satisfies_enrollment_after_cleanup_settles() {
    let mut app = test_app();
    let origin = authenticate_test_app(&mut app);
    app.backend_compatibility_verified = true;
    app.backend_reconciliation = BackendReconciliationState::CleanupPending;
    app.device_enrollment_retry_after =
        Some(std::time::Instant::now() + std::time::Duration::from_secs(30));
    let (runtime, _cancellation, abort) = test_self_device_link_runtime();
    app.state
        .identity
        .account_runtimes
        .attach_self_device_link(runtime);

    app.handle_session_event(RuntimeSessionEvent::DeviceLinkSelfResolved {
        origin,
        user_code: "ABCD-EFGH".to_owned(),
        outcome: kodosi_domain::device_link::SelfDeviceLinkOutcome::Approved,
    });
    app.finish_initial_collaboration_cleanup().await;

    assert!(app.device_enrollment_satisfied);
    assert!(app.device_enrollment_retry_after.is_none());
    assert_eq!(app.backend_reconciliation, BackendReconciliationState::Idle);
    assert!(app.remote_surfaces_ready());
    abort.abort();
}

#[tokio::test]
async fn unavailable_access_mutation_ledger_fences_remote_surfaces() {
    let mut app = test_app();
    authenticate_test_app(&mut app);
    app.backend_compatibility_verified = true;
    app.backend_reconciliation = BackendReconciliationState::CleanupPending;
    app.device_enrollment_satisfied = true;
    app.finish_initial_collaboration_cleanup().await;
    assert!(app.remote_surfaces_ready());

    let dir = tempfile::tempdir().unwrap();
    let referent = dir.path().join("referent.json");
    let path = dir.path().join("pending-session-access-mutations.json");
    std::fs::write(&referent, b"preserved").unwrap();
    std::os::unix::fs::symlink(&referent, &path).unwrap();
    app.access_mutations =
        crate::runtime::access_mutation_ledger::SessionAccessMutationLedger::load_at(path).unwrap();

    assert!(!app.remote_surfaces_ready());
    assert!(app.state.local.sessions.ids().is_empty());
}

#[tokio::test]
async fn terminal_self_link_result_retires_completed_runtime() {
    let mut app = test_app();
    let origin = authenticate_test_app(&mut app);
    let (runtime, _cancellation, abort) = test_self_device_link_runtime();
    app.state
        .identity
        .account_runtimes
        .attach_self_device_link(runtime);

    app.handle_session_event(RuntimeSessionEvent::DeviceLinkSelfResolved {
        origin,
        user_code: "ABCD-EFGH".to_owned(),
        outcome: kodosi_domain::device_link::SelfDeviceLinkOutcome::Failed,
    });

    assert!(
        app.state
            .identity
            .account_runtimes
            .self_device_link()
            .is_none()
    );
    abort.abort();
}

#[tokio::test]
async fn delayed_self_link_result_cannot_retire_replacement_attempt() {
    let mut app = test_app();
    let origin = authenticate_test_app(&mut app);
    let (replacement, _replacement_cancel, replacement_abort) =
        test_self_device_link_runtime_with_code("WXYZ-2345");
    app.state
        .identity
        .account_runtimes
        .attach_self_device_link(replacement);
    app.device_enrollment_retry_after =
        Some(std::time::Instant::now() + std::time::Duration::from_secs(30));

    app.handle_session_event(RuntimeSessionEvent::DeviceLinkSelfResolved {
        origin,
        user_code: "AAAA-BBBB".to_owned(),
        outcome: kodosi_domain::device_link::SelfDeviceLinkOutcome::Approved,
    });

    assert_eq!(
        app.state
            .identity
            .account_runtimes
            .self_device_link()
            .map(|runtime| runtime.identity.user_code.as_str()),
        Some("WXYZ-2345")
    );
    assert!(!app.device_enrollment_satisfied);
    assert!(app.device_enrollment_retry_after.is_some());
    replacement_abort.abort();
}

#[test]
fn stale_self_link_approval_cannot_satisfy_current_account_enrollment() {
    let mut app = test_app();
    let stale_origin = authenticate_test_app(&mut app);
    crate::runtime::auth::set_signed_out(&mut app).expect("test sign-out should advance epoch");
    authenticate_test_app(&mut app);
    app.device_enrollment_retry_after =
        Some(std::time::Instant::now() + std::time::Duration::from_secs(30));

    app.handle_session_event(RuntimeSessionEvent::DeviceLinkSelfResolved {
        origin: stale_origin,
        user_code: "ABCD-EFGH".to_owned(),
        outcome: kodosi_domain::device_link::SelfDeviceLinkOutcome::Approved,
    });

    assert!(!app.device_enrollment_satisfied);
    assert!(app.device_enrollment_retry_after.is_some());
    assert!(app.state.runtime_outbox.drain_devices().is_empty());
}

#[tokio::test]
async fn logout_preserves_running_local_session_and_process_runtime() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    authenticate_test_app(&mut app);
    let session_id = SessionId::new();
    let (handle, cancellation, abort) = test_session_handle_with_runtime();
    insert_owned_session_for_test(
        &mut app,
        claude_owned_summary(session_id, "/tmp/kodosi"),
        handle,
    );
    app.state.sharing.shared_sessions.insert(
        session_id,
        crate::sharing::shared_session_registry::SharedSessionState::new(
            session_id.to_string(),
            uuid::Uuid::now_v7(),
            "owner-secret".to_owned(),
            ShareScope::MyDevices,
            None,
            Some([7; 32]),
            Some(1),
        ),
    );

    crate::runtime::auth::logout(&mut app)
        .await
        .unwrap_or_else(|error| panic!("logout should succeed: {error}"));

    assert!(app.state.local.sessions.record(session_id).is_some());
    assert!(app.state.local.owned_session_runtimes.contains(session_id));
    assert!(!app.state.sharing.shared_sessions.contains(session_id));
    assert_eq!(
        app.state
            .local
            .sessions
            .record(session_id)
            .map(|record| record.summary.scope),
        Some(ShareScope::JustMe)
    );
    assert!(!cancellation.is_cancelled());
    assert!(!abort.is_finished());
    abort.abort();
}

#[tokio::test]
async fn signed_out_transition_tears_down_account_relays() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    authenticate_test_app(&mut app);

    let session_id = SessionId::new();
    let host_relay_cancellation = CancellationToken::new();
    let relay_generation = app
        .state
        .sharing
        .host_relays
        .allocate_generation()
        .expect("allocate host relay generation");
    app.state
        .sharing
        .host_relays
        .claim_generation(session_id, relay_generation)
        .expect("claim host relay generation");
    let action_key = (
        session_id,
        "action-1".to_owned(),
        test_user_id().to_string(),
        relay_generation,
    );
    let receipt_key = (session_id, uuid::Uuid::now_v7(), relay_generation);
    app.insert_action_result_in_flight_for_test(action_key.clone());
    app.insert_semantic_receipt_in_flight_for_test(receipt_key);
    let (pending_tx, _pending_rx) = tokio::sync::watch::channel(None);
    app.state.sharing.host_relays.attach(
        session_id,
        host_relay_cancellation.clone(),
        pending_tx,
        tokio::sync::mpsc::channel(1).0,
        tokio::sync::mpsc::channel(1).0,
        tokio::sync::mpsc::channel(1).0,
        tokio::spawn(std::future::pending::<()>()),
    );
    let participant_session_id = SessionId::new();
    let participant_relay_cancellation = CancellationToken::new();
    let (command_tx, _command_rx) = tokio::sync::mpsc::channel(1);
    app.state.remote.session_relays.attach_for_test(
        participant_session_id,
        participant_relay_cancellation.clone(),
        command_tx,
        ShareScope::Friends,
        kodosi_backend_client::session_relay::RemoteRelayMode::SharedParticipant,
        ConnectionState::Connected,
        tokio::spawn(std::future::pending::<()>()),
    );
    assert!(app.state.sharing.host_relays.active(session_id));
    assert!(
        app.state
            .remote
            .session_relays
            .contains(participant_session_id)
    );

    crate::runtime::auth::set_signed_out(&mut app).expect("test sign-out epoch should advance");

    assert!(host_relay_cancellation.is_cancelled());
    assert!(!app.action_result_in_flight_for_test(&action_key));
    assert!(!app.semantic_receipt_in_flight_for_test(&receipt_key));
    assert!(participant_relay_cancellation.is_cancelled());
    assert!(!app.state.sharing.host_relays.active(session_id));
    assert!(
        !app.state
            .remote
            .session_relays
            .contains(participant_session_id)
    );
    std::assert_matches!(app.state.identity.auth, AuthState::SignedOut);
}

#[tokio::test]
async fn logout_shuts_down_account_runtimes_and_clears_discovery_refresh() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    authenticate_test_app(&mut app);

    let (self_link, self_link_cancel, self_link_abort) = test_self_device_link_runtime();
    let (user_events, user_events_cancel, user_events_abort) = test_user_events_handle();
    app.state
        .identity
        .account_runtimes
        .attach_self_device_link(self_link);
    app.state
        .identity
        .account_runtimes
        .attach_user_events(user_events);
    app.state
        .queue_discovery_refresh([DiscoverySurface::Friends, DiscoverySurface::OwnSessions]);

    crate::runtime::auth::logout(&mut app)
        .await
        .unwrap_or_else(|error| panic!("logout should succeed: {error}"));

    assert!(self_link_cancel.is_cancelled());
    assert!(user_events_cancel.is_cancelled());
    assert!(
        app.state
            .identity
            .account_runtimes
            .self_device_link()
            .is_none()
    );
    assert!(!app.state.identity.account_runtimes.user_events_healthy());
    wait_for_finished(&user_events_abort).await;
    assert!(app.state.take_pending_discovery_refresh().is_empty());
    std::assert_matches!(
        app.state.runtime_outbox.drain_devices().as_slice(),
        [crate::host_protocol::DeviceEvent::LinkSelfResolved {
            outcome: kodosi_domain::device_link::SelfDeviceLinkOutcome::Failed,
        }]
    );
    assert!(app.session_events_rx.try_recv().is_err());
    self_link_abort.abort();
}

#[test]
fn logout_storage_failure_never_publishes_signed_out_state() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    authenticate_test_app(&mut app);
    let subject = app.state.identity.auth.subject();
    let storage_error = AppError::Io(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "simulated durable-token deletion failure",
    ));

    crate::runtime::auth::finish_logout_after_token_clear(&mut app, subject, Err(storage_error))
        .expect_err("logout must fail while credentials remain durable");

    std::assert_matches!(
        app.state.identity.auth,
        AuthState::Expired { subject: Some(_) }
    );
    assert!(!app.state.logs.iter().any(|message| message == "signed out"));
}

#[tokio::test]
async fn shutdown_account_runtimes_cancels_device_flow_poll() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let (device_flow_cancel, device_flow_task) = test_device_flow_poll_task();
    let device_flow_abort = device_flow_task.abort_handle();
    app.state
        .identity
        .device_flow
        .attach_for_test(device_flow_cancel.clone(), device_flow_task);

    crate::runtime::identity::account_runtimes::shutdown_account_runtimes(
        &mut app.state.identity.device_flow,
        &mut app.state.identity.account_runtimes,
        &mut app.state.pending_discovery_surfaces,
    );

    assert!(device_flow_cancel.is_cancelled());
    assert!(!app.state.identity.device_flow.is_active_for_test());
    wait_for_finished(&device_flow_abort).await;
}

#[tokio::test]
async fn stale_device_flow_success_is_ignored_outside_waiting_state() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    app.state
        .identity
        .device_flow
        .enqueue_result_for_test(DeviceFlowResult::Success(TokenResponse {
            access_token: "stale-access-token".to_owned(),
            refresh_token: Some("stale-refresh-token".to_owned()),
            expires_in: 3600,
            token_type: "Bearer".to_owned(),
        }));

    let _ = crate::runtime::auth::drain_device_flow_events(&mut app).await;

    std::assert_matches!(app.state.identity.auth, AuthState::SignedOut);
    assert!(
        app.state
            .logs
            .iter()
            .any(|line| line == "ignored stale device login result")
    );
}

#[tokio::test]
async fn auth_expiry_shuts_down_account_runtimes() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    authenticate_test_app(&mut app);

    let (self_link, self_link_cancel, self_link_abort) = test_self_device_link_runtime();
    let (user_events, user_events_cancel, user_events_abort) = test_user_events_handle();
    app.state
        .identity
        .account_runtimes
        .attach_self_device_link(self_link);
    app.state
        .identity
        .account_runtimes
        .attach_user_events(user_events);

    crate::runtime::auth::mark_expired_from_backend(&mut app, "test token expired")
        .expect("test expiry epoch should advance");

    assert!(self_link_cancel.is_cancelled());
    assert!(user_events_cancel.is_cancelled());
    assert!(
        app.state
            .identity
            .account_runtimes
            .self_device_link()
            .is_none()
    );
    assert!(!app.state.identity.account_runtimes.user_events_healthy());
    std::assert_matches!(
        app.state.identity.auth,
        AuthState::Expired { subject: Some(_) }
    );
    wait_for_finished(&user_events_abort).await;
    self_link_abort.abort();
}

#[tokio::test]
async fn signed_out_self_device_link_start_cancels_stale_runtime_without_reemit() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));

    let (self_link, self_link_cancel, self_link_abort) = test_self_device_link_runtime();
    app.state
        .identity
        .account_runtimes
        .attach_self_device_link(self_link);

    let error = crate::runtime::identity::device_link::DeviceLinkCtx {
        auth: &app.state.identity.auth,
        account_origin: None,
        backend: &app.backend,
        device_key_store: &app.device_key_store,
        pin_store: &app.pin_store,
        account_runtimes: &mut app.state.identity.account_runtimes,
        device_flow: &mut app.state.identity.device_flow,
        pending_discovery_surfaces: &mut app.state.pending_discovery_surfaces,
        session_events_tx: &app.session_events_tx,
        logs: &mut app.state.logs,
    }
    .start_self()
    .await
    .expect_err("signed-out self-link start should be unauthorized");

    std::assert_matches!(error, AppError::Unauthorized);
    assert!(self_link_cancel.is_cancelled());
    assert!(
        app.state
            .identity
            .account_runtimes
            .self_device_link()
            .is_none()
    );
    assert!(app.session_events_rx.try_recv().is_err());
    self_link_abort.abort();
}

#[tokio::test]
async fn logout_drains_decrypted_remote_terminal_cache_and_closes_hub_sessions() {
    let mut app = teardown_test_app();
    let (session_id, mut handle) = cached_remote_terminal_for_test(&mut app);

    crate::runtime::auth::logout(&mut app)
        .await
        .unwrap_or_else(|error| panic!("logout should succeed: {error}"));

    assert_remote_terminal_torn_down(&mut app, session_id, &mut handle);
}

fn teardown_test_app() -> Runtime {
    teardown_test_app_with_dependencies(|dependencies| dependencies)
}

fn teardown_test_app_with_pin_store(pin_store: DeviceListPinStoreHandle) -> Runtime {
    teardown_test_app_with_dependencies(|dependencies| dependencies.with_pin_store(pin_store))
}

fn teardown_test_app_with_dependencies(
    configure: impl FnOnce(crate::runtime::RuntimeDependencies) -> crate::runtime::RuntimeDependencies,
) -> Runtime {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let dependencies = crate::runtime::RuntimeDependencies::isolated(&config)
        .expect("isolated runtime dependencies");
    let mut app =
        Runtime::with_dependencies(config, CancellationToken::new(), configure(dependencies))
            .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    authenticate_test_app(&mut app);
    app
}

fn cached_remote_terminal_for_test(
    app: &mut Runtime,
) -> (SessionId, crate::terminal_transport::hub::SubscriberHandle) {
    use crate::terminal_transport::{TerminalCapability, TerminalSurface};
    use kodosi_domain::terminal::{
        Revision, TerminalPresentationFrame, TerminalPresentationV2, TerminalScreen,
    };

    let session_id = SessionId::new();
    let presentation = TerminalPresentationV2::new(
        TerminalSize::new(24, 80).expect("valid size"),
        TerminalScreen::Primary,
        vec!["secret scrollback".to_owned(); 24],
        0,
        0,
        false,
    )
    .expect("presentation");
    let frame = TerminalPresentationFrame::new(Revision::default(), presentation);
    app.remote_terminal
        .apply_presentation(session_id, frame.presentation.clone());
    let _ = app
        .client_focus
        .note_focus(session_id, "client-a".to_owned());
    assert!(
        app.terminal_hub
            .open_local_incarnation(session_id, uuid::Uuid::now_v7(), 0)
    );
    let handle = app
        .terminal_hub
        .register(
            session_id,
            TerminalSurface::Desktop,
            TerminalCapability::ReadOnly,
        )
        .expect("session should be live");
    assert!(app.remote_terminal.cached(session_id).is_some());
    (session_id, handle)
}

fn assert_remote_terminal_torn_down(
    app: &mut Runtime,
    session_id: SessionId,
    handle: &mut crate::terminal_transport::hub::SubscriberHandle,
) {
    use crate::terminal_transport::{
        TerminalCapability, TerminalCloseReason, TerminalControlFrame, TerminalSurface,
    };

    assert!(
        app.remote_terminal.cached(session_id).is_none(),
        "decrypted remote scrollback must not survive account teardown"
    );

    let mut closed = None;
    while let Ok(frame) = handle.control_rx.try_recv() {
        if let TerminalControlFrame::Closed { reason, .. } = frame {
            closed = Some(reason);
        }
    }
    std::assert_matches!(closed, Some(TerminalCloseReason::AuthRevoked));

    assert!(
        app.client_focus.clients(session_id).is_empty(),
        "a torn-down remote session must not keep a focus claim"
    );
    assert!(
        app.terminal_hub
            .register(
                session_id,
                TerminalSurface::Desktop,
                TerminalCapability::ReadOnly,
            )
            .is_none(),
        "a torn-down session must not accept a new subscriber"
    );
}

fn account_b_profile() -> kodosi_backend_client::api::BackendUserProfile {
    kodosi_backend_client::api::BackendUserProfile {
        id: other_test_user_id().to_string(),
        handle: "account-b".to_owned(),
        display_name: "Account B".to_owned(),
        email: None,
        avatar_url: None,
    }
}

fn remote_account_record(session_id: SessionId, access: AccessLevel) -> RemoteSessionRecord {
    let summary = SessionSummary::new_remote(
        session_id,
        "Account A remote".to_owned(),
        "Owner A".to_owned(),
        Some(test_user_id()),
        ShareScope::Friends,
        access,
        TerminalSize::default(),
    );
    RemoteSessionRecord {
        summary,
        incarnation_id: Some(uuid::Uuid::now_v7()),
        room_id: None,
        connection_state: Some(ConnectionState::Connected),
        connection_reason: None,
        access_state: Some(RemoteSessionAccessState::Ready),
        access_reason: None,
        access_issue: None,
        viewer_blocked: false,
        viewer_hidden: false,
    }
}

#[tokio::test]
async fn profile_subject_switch_tears_down_a_before_publishing_b_and_fences_outbound() {
    let mut app = teardown_test_app();
    let account_a_session = SessionId::new();
    app.state.discovery.replace_remote_sessions(
        vec![remote_account_record(
            account_a_session,
            AccessLevel::Approve,
        )],
        false,
    );
    let account_a_incarnation = app
        .state
        .discovery
        .session(account_a_session)
        .and_then(|record| record.incarnation_id)
        .expect("account A remote incarnation");
    let (command_tx, mut command_rx) = tokio::sync::mpsc::channel(4);
    let participant_cancel = CancellationToken::new();
    app.state.remote.session_relays.attach_for_test(
        account_a_session,
        participant_cancel.clone(),
        command_tx,
        ShareScope::Friends,
        kodosi_backend_client::session_relay::RemoteRelayMode::SharedParticipant,
        ConnectionState::Connected,
        tokio::spawn(std::future::pending::<()>()),
    );
    app.state.runtime_outbox.queue_agent_intel(
        crate::AgentIntelEvent::RemotePermissionDecisionState {
            session_id: account_a_session.to_string(),
            session_incarnation_id: account_a_incarnation.to_string(),
            tool_use_id: "secret-tool-use".to_owned(),
            request_generation: 7,
            phase: crate::RemotePermissionDecisionPhase::Pending,
            status: None,
            message: Some("account A receipt".to_owned()),
        },
    );
    app.state
        .pending_work
        .queue_host_key_rotation(account_a_session);
    let old_epoch = app.state.identity.account_epoch();

    let name = crate::runtime::auth::apply_current_user_profile(
        &mut app,
        &account_b_profile(),
        other_test_user_id(),
        time::OffsetDateTime::now_utc(),
    )
    .await
    .expect("account B profile should reconcile");

    assert_eq!(name.as_deref(), Some("Account B"));
    assert_eq!(
        app.state.identity.auth.subject(),
        Some(other_test_user_id())
    );
    assert_ne!(app.state.identity.account_epoch(), old_epoch);
    assert!(participant_cancel.is_cancelled());
    assert!(app.state.discovery.remote_session_ids().is_empty());
    assert!(app.state.runtime_outbox.drain_agent_intel().is_empty());
    assert!(app.state.pending_work.drain_host_key_rotations().is_empty());
    assert_eq!(
        crate::runtime::remote_sessions::permission_decision(
            &mut app,
            account_a_session,
            "stale-tool-use",
            7,
            "allow",
        ),
        crate::runtime::SessionRelayCommandDispatchOutcome::Rejected
    );
    assert!(command_rx.try_recv().is_err());
}

#[tokio::test]
async fn profile_subject_is_not_published_when_pin_store_bind_fails() {
    let pin_root = tempfile::tempdir().expect("pin root");
    let pin_parent = pin_root.path().join("store");
    std::fs::create_dir(&pin_parent).expect("pin parent");
    let pin_store = DeviceListPinStoreHandle::from_store(
        DeviceListPinStore::load_from(pin_parent.join("pins.json")).expect("pin store"),
    )
    .expect("pin actor");
    pin_store
        .bind_to_user(&test_user_id().to_string())
        .await
        .expect("bind account A");
    let mut app = teardown_test_app_with_pin_store(pin_store);
    app.state.identity.current_user_name = Some("Account A".to_owned());
    let local_session = SessionId::new();
    insert_owned_session_for_test(
        &mut app,
        claude_owned_summary(local_session, "/tmp/kodosi"),
        test_session_handle(),
    );
    let (remote_session, mut remote_handle) = cached_remote_terminal_for_test(&mut app);

    std::fs::remove_dir_all(&pin_parent).expect("remove pin directory");
    std::fs::write(&pin_parent, b"not a directory").expect("block pin persistence");

    let error = crate::runtime::auth::apply_current_user_profile(
        &mut app,
        &account_b_profile(),
        other_test_user_id(),
        time::OffsetDateTime::now_utc(),
    )
    .await
    .expect_err("account B must not publish without its trust bucket");

    std::assert_matches!(error, AppError::Io(_));
    std::assert_matches!(app.state.identity.auth, AuthState::SignedOut);
    assert!(app.state.identity.current_user_name.is_none());
    assert_remote_terminal_torn_down(&mut app, remote_session, &mut remote_handle);
    assert!(!app.remote_surfaces_ready());
}

#[tokio::test]
async fn token_expiry_drains_decrypted_remote_terminal_cache() {
    let mut app = teardown_test_app();
    let (session_id, mut handle) = cached_remote_terminal_for_test(&mut app);

    crate::runtime::auth::mark_expired_from_backend(&mut app, "test token expired")
        .expect("test expiry epoch should advance");

    assert_remote_terminal_torn_down(&mut app, session_id, &mut handle);
}

#[tokio::test]
async fn set_signed_out_drains_decrypted_remote_terminal_cache() {
    let mut app = teardown_test_app();
    let (session_id, mut handle) = cached_remote_terminal_for_test(&mut app);

    crate::runtime::auth::set_signed_out(&mut app).expect("test sign-out epoch should advance");

    assert_remote_terminal_torn_down(&mut app, session_id, &mut handle);
}

fn configure_reset_backend(app: &mut Runtime) -> String {
    let origin = "https://reset.example.test/".to_owned();
    app.backend = kodosi_backend_client::http_client::BackendHttpClient::new(
        &kodosi_backend_client::config::BackendClientConfig::new(
            Some(origin.clone()),
            None,
            None,
            None,
        ),
    )
    .expect("reset backend client");
    app.backend
        .backend_origin()
        .expect("reset backend origin")
        .to_string()
}

#[tokio::test]
async fn failed_pin_cleanup_retires_authority_retains_intent_and_preserves_other_account_pin() {
    let pin_root = tempfile::tempdir().expect("pin root");
    let pin_parent = pin_root.path().join("store");
    std::fs::create_dir(&pin_parent).expect("pin parent");
    let pin_path = pin_parent.join("pins.json");
    let pin_store = DeviceListPinStoreHandle::from_store(
        DeviceListPinStore::load_from(&pin_path).expect("pin store"),
    )
    .expect("pin actor");
    let other_user = other_test_user_id().to_string();
    pin_store
        .bind_to_user(&other_user)
        .await
        .expect("bind other account");
    std::assert_matches!(
        pin_store
            .verify_or_pin(
                fake_identity_view(&other_user, "other-device"),
                PinContext::ExplicitShare,
            )
            .await
            .expect("pin other account"),
        PinVerdict::FirstShare
    );
    let mut app = teardown_test_app_with_pin_store(pin_store);
    let reset_origin = configure_reset_backend(&mut app);
    let reset_user = app
        .state
        .identity
        .auth
        .subject_string()
        .expect("authenticated subject");
    std::fs::remove_dir_all(&pin_parent).expect("remove pin directory");
    std::fs::write(&pin_parent, b"not a directory").expect("block pin persistence");
    app.identity_reset_intent
        .seed_local_cleanup_for_test(&reset_origin, &reset_user)
        .expect("seed reset intent");
    let (session_id, mut handle) = cached_remote_terminal_for_test(&mut app);

    let resolution = crate::runtime::identity_reset::resolve_pending(&mut app)
        .await
        .expect("resolve pending reset");
    std::assert_matches!(
        resolution,
        crate::runtime::identity_reset::IdentityResetResolution::Pending(_)
    );
    std::assert_matches!(app.state.identity.auth, AuthState::SignedOut);
    assert_remote_terminal_torn_down(&mut app, session_id, &mut handle);
    assert!(
        app.identity_reset_intent
            .has_pending_for_test()
            .expect("pending reset intent")
    );
    assert!(
        app.pin_store
            .list_pins()
            .await
            .expect("other account pin metadata")
            .iter()
            .any(|pin| pin.user_id == other_user),
        "failed bind must not clear the previously bound account"
    );
}

#[tokio::test]
async fn delete_required_reset_fences_account_without_wiping_device_keys() {
    let mut app = teardown_test_app();
    let reset_origin = configure_reset_backend(&mut app);
    let subject = app
        .state
        .identity
        .auth
        .subject_string()
        .expect("authenticated subject");
    let keys = crate::identity_core::device_keys::generate_device_keys_for_test(&subject)
        .expect("test device keys");
    let device_id = keys.device_id.clone();
    app.device_key_store
        .save_for_test(&subject, &keys)
        .expect("persist device keys");
    app.identity_reset_intent
        .seed_delete_required_for_test(&reset_origin, &subject)
        .expect("seed delete-required intent");
    let (session_id, mut handle) = cached_remote_terminal_for_test(&mut app);
    let relay_cancellation = CancellationToken::new();
    let (command_tx, _command_rx) = tokio::sync::mpsc::channel(1);
    app.state.remote.session_relays.attach_for_test(
        session_id,
        relay_cancellation.clone(),
        command_tx,
        ShareScope::Friends,
        kodosi_backend_client::session_relay::RemoteRelayMode::SharedParticipant,
        ConnectionState::Connected,
        tokio::spawn(std::future::pending()),
    );
    let (self_link, self_link_cancel, self_link_abort) = test_self_device_link_runtime();
    app.state
        .identity
        .account_runtimes
        .attach_self_device_link(self_link);

    let resolution = crate::runtime::identity_reset::resolve_pending(&mut app)
        .await
        .expect("resolve pending reset");

    std::assert_matches!(
        resolution,
        crate::runtime::identity_reset::IdentityResetResolution::Pending(_)
    );
    std::assert_matches!(app.state.identity.auth, AuthState::SignedOut);
    assert_remote_terminal_torn_down(&mut app, session_id, &mut handle);
    assert!(relay_cancellation.is_cancelled());
    assert!(self_link_cancel.is_cancelled());
    assert!(
        app.state
            .identity
            .account_runtimes
            .self_device_link()
            .is_none()
    );
    assert!(
        app.identity_reset_intent
            .has_pending_for_test()
            .expect("pending reset intent")
    );
    assert_eq!(
        app.device_key_store
            .load_if_present(&subject)
            .expect("load preserved keys")
            .expect("keys remain")
            .device_id,
        device_id
    );
    self_link_abort.abort();
}

#[tokio::test]
async fn pending_startup_reset_restores_local_sessions_and_blocks_login() {
    let mut app = teardown_test_app();
    let reset_origin = configure_reset_backend(&mut app);
    let subject = app
        .state
        .identity
        .auth
        .subject_string()
        .expect("authenticated subject");
    let cached_session = SessionSummary::new_owned(
        SessionId::new(),
        "Cached local".to_owned(),
        "cached-local".to_owned(),
        TerminalSize::default(),
        None,
    );
    let cache_root = app
        .state
        .session_cache_root
        .as_ref()
        .expect("test cache root");
    let session_dir = cache_root.join(&cached_session.runtime_name);
    std::fs::create_dir_all(&session_dir).expect("create legacy cache directory");
    let document = serde_json::json!({
        "sessionId": cached_session.id.to_string(),
        "title": cached_session.title,
        "runtimeName": cached_session.runtime_name,
        "workingDir": cached_session.working_dir,
        "roomName": cached_session.room_name,
        "createdAt": cached_session
            .created_at
            .format(&time::format_description::well_known::Rfc3339)
            .expect("format legacy creation time"),
        "lastUpdate": cached_session
            .last_update
            .format(&time::format_description::well_known::Rfc3339)
            .expect("format legacy update time"),
        "rows": cached_session.size.rows(),
        "cols": cached_session.size.cols()
    });
    std::fs::write(
        session_dir.join("session.json"),
        serde_json::to_vec(&document).expect("encode legacy cache fixture"),
    )
    .expect("write cached local session");
    app.identity_reset_intent
        .seed_delete_required_for_test(&reset_origin, &subject)
        .expect("seed unresolved reset intent");

    app.initialize()
        .await
        .expect("initialize local-only runtime");

    std::assert_matches!(app.state.identity.auth, AuthState::SignedOut);
    assert!(app.state.local.sessions.record(cached_session.id).is_some());
    assert!(
        app.identity_reset_intent
            .has_pending_for_test()
            .expect("pending reset intent")
    );
    let error = crate::runtime::auth::login(&mut app)
        .await
        .expect_err("pending reset must block login");
    assert!(error.to_string().contains("identity reset"));
    assert!(!app.state.identity.device_flow.is_active_for_test());
}

#[tokio::test]
async fn startup_identity_reset_intent_retires_account_before_auth_restore() {
    let mut app = teardown_test_app();
    let reset_origin = configure_reset_backend(&mut app);
    let subject = app
        .state
        .identity
        .auth
        .subject_string()
        .expect("authenticated subject");
    let (session_id, mut handle) = cached_remote_terminal_for_test(&mut app);
    let (self_link, self_link_cancel, self_link_abort) = test_self_device_link_runtime();
    app.state
        .identity
        .account_runtimes
        .attach_self_device_link(self_link);
    app.identity_reset_intent
        .seed_local_cleanup_for_test(&reset_origin, &subject)
        .expect("seed pending reset intent");

    app.initialize().await.expect("finish pending reset");

    std::assert_matches!(app.state.identity.auth, AuthState::SignedOut);
    assert_remote_terminal_torn_down(&mut app, session_id, &mut handle);
    assert!(self_link_cancel.is_cancelled());
    assert!(
        app.state
            .identity
            .account_runtimes
            .self_device_link()
            .is_none()
    );
    assert!(
        !app.identity_reset_intent
            .has_pending_for_test()
            .expect("read reset intent")
    );
    std::assert_matches!(
        app.state.runtime_outbox.drain_devices().as_slice(),
        [crate::host_protocol::DeviceEvent::LinkSelfResolved {
            outcome: kodosi_domain::device_link::SelfDeviceLinkOutcome::Failed,
        }]
    );
    self_link_abort.abort();
}

#[tokio::test]
async fn failed_identity_reset_retains_authenticated_remote_projection_for_retry() {
    let mut app = teardown_test_app();
    let (session_id, mut handle) = cached_remote_terminal_for_test(&mut app);

    let error = crate::runtime::identity_reset::reset_identity(&mut app)
        .await
        .expect_err("unconfigured backend must reject identity reset");

    assert!(app.state.identity.auth.is_authenticated());
    assert!(app.remote_terminal.cached(session_id).is_some());
    assert!(handle.control_rx.try_recv().is_err());
    std::assert_matches!(error, AppError::Unsupported { .. });
}

#[tokio::test]
async fn failed_token_clear_still_drains_decrypted_remote_terminal_cache() {
    let mut app = teardown_test_app();
    let subject = app.state.identity.auth.subject();
    let (session_id, mut handle) = cached_remote_terminal_for_test(&mut app);
    let storage_error = AppError::Io(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "simulated durable-token deletion failure",
    ));

    crate::runtime::discovery::clear_remote_catalog(
        &mut app,
        crate::runtime::discovery::CatalogClearReason::AccountTeardown,
    );
    crate::runtime::auth::finish_logout_after_token_clear(&mut app, subject, Err(storage_error))
        .expect_err("logout must fail while credentials remain durable");

    std::assert_matches!(
        app.state.identity.auth,
        AuthState::Expired { subject: Some(_) }
    );
    assert_remote_terminal_torn_down(&mut app, session_id, &mut handle);
}

#[tokio::test]
async fn logout_drops_account_content_without_erasing_device_enrollment() {
    let mut app = teardown_test_app();
    let subject = app
        .state
        .identity
        .auth
        .subject_string()
        .unwrap_or_else(|| panic!("authenticated test app should have a subject"));
    let enrollment_before = app
        .device_key_store
        .load_if_present(&subject)
        .unwrap_or_else(|error| panic!("device enrollment should be readable: {error}"))
        .map(|keys| keys.device_id.clone());
    let (session_id, mut handle) = cached_remote_terminal_for_test(&mut app);

    crate::runtime::auth::logout(&mut app)
        .await
        .unwrap_or_else(|error| panic!("logout should succeed: {error}"));

    assert_remote_terminal_torn_down(&mut app, session_id, &mut handle);
    let enrollment_after = app
        .device_key_store
        .load_if_present(&subject)
        .unwrap_or_else(|error| panic!("device enrollment should still be readable: {error}"))
        .map(|keys| keys.device_id.clone());
    assert_eq!(
        enrollment_after, enrollment_before,
        "logout must not erase device enrollment"
    );
}

#[tokio::test]
async fn a_transient_catalog_clear_also_drops_decrypted_remote_plaintext() {
    let mut app = teardown_test_app();
    let (session_id, mut handle) = cached_remote_terminal_for_test(&mut app);

    crate::runtime::discovery::clear_remote_catalog(
        &mut app,
        crate::runtime::discovery::CatalogClearReason::TransientOffline,
    );

    assert_remote_terminal_torn_down(&mut app, session_id, &mut handle);
}
