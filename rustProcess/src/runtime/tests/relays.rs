use super::*;

#[tokio::test]
async fn dropping_runtime_retires_owned_relay_tasks_without_cancelling_external_root() {
    let external_root = CancellationToken::new();
    let host_cancellation = external_root.child_token();
    let session_cancellation = external_root.child_token();
    let host_task = tokio::spawn(std::future::pending::<()>());
    let host_abort = host_task.abort_handle();
    let session_task = tokio::spawn(std::future::pending::<()>());
    let session_abort = session_task.abort_handle();
    let session_id = SessionId::new();
    let mut app = test_app();

    let host_generation = app
        .state
        .sharing
        .host_relays
        .allocate_generation()
        .expect("host generation");
    app.state
        .sharing
        .host_relays
        .claim_generation(session_id, host_generation)
        .expect("claim host generation");
    app.state.attach_host_relay(
        session_id,
        host_cancellation.clone(),
        tokio::sync::watch::channel(None).0,
        mpsc::channel(1).0,
        mpsc::channel(1).0,
        mpsc::channel(1).0,
        host_task,
    );
    app.state.remote.session_relays.attach_for_test(
        session_id,
        session_cancellation.clone(),
        mpsc::channel(1).0,
        ShareScope::MyDevices,
        kodosi_backend_client::session_relay::RemoteRelayMode::SharedParticipant,
        ConnectionState::Connected,
        session_task,
    );

    drop(app);

    assert!(!external_root.is_cancelled());
    assert!(host_cancellation.is_cancelled());
    assert!(session_cancellation.is_cancelled());
    tokio::task::yield_now().await;
    assert!(host_abort.is_finished());
    assert!(session_abort.is_finished());
}

fn claim_host_generation(app: &mut Runtime, session_id: SessionId) -> u64 {
    let generation = app
        .state
        .sharing
        .host_relays
        .allocate_generation()
        .expect("allocate host generation");
    app.state
        .sharing
        .host_relays
        .claim_generation(session_id, generation)
        .expect("claim host generation");
    generation
}

fn host_origin(
    app: &mut Runtime,
    session_id: SessionId,
) -> crate::session_runtime::events::HostRelayEventOrigin {
    if !app.state.identity.auth.is_authenticated() {
        authenticate_test_app(app);
    }
    let generation = claim_host_generation(app, session_id);
    current_host_relay_origin(app, session_id, generation)
}

#[test]
fn stale_host_generation_cannot_queue_current_key_work() {
    let mut app = test_app();
    authenticate_test_app(&mut app);
    let session_id = SessionId::new();
    let stale_generation = claim_host_generation(&mut app, session_id);
    let current_generation = claim_host_generation(&mut app, session_id);

    app.handle_session_event(RuntimeSessionEvent::HostKeyDistributionRequested {
        origin: current_host_relay_origin(&app, session_id, stale_generation),
        fence_id: "stale-fence".to_owned(),
    });
    assert!(
        app.state
            .pending_work
            .drain_key_redistributions()
            .is_empty()
    );

    app.handle_session_event(RuntimeSessionEvent::HostKeyDistributionRequested {
        origin: current_host_relay_origin(&app, session_id, current_generation),
        fence_id: "current-fence".to_owned(),
    });
    let work = app.state.pending_work.drain_key_redistributions();
    assert_eq!(work.len(), 1);
    assert_eq!(work[0].session_id, session_id);
    assert_eq!(work[0].fence_ids, vec!["current-fence".to_owned()]);
}

#[tokio::test]
async fn current_relay_connected_transition_reasserts_surviving_focus() {
    let mut app = test_app();
    authenticate_test_app(&mut app);
    let session_id = SessionId::new();
    seed_visible_remote_record(&mut app, session_id);
    app.state
        .discovery
        .session_mut(session_id)
        .expect("remote session")
        .summary
        .access = AccessLevel::Inject;
    let _ = app
        .client_focus
        .note_focus(session_id, "client-a".to_owned());
    let (command_tx, mut command_rx) = mpsc::channel(2);
    let relay_generation = app.state.remote.session_relays.attach_for_test(
        session_id,
        CancellationToken::new(),
        command_tx,
        ShareScope::Friends,
        kodosi_backend_client::session_relay::RemoteRelayMode::SharedParticipant,
        ConnectionState::Connecting,
        tokio::spawn(std::future::pending()),
    );

    app.handle_session_event(RuntimeSessionEvent::RemoteSessionConnectionChanged {
        id: session_id,
        status: ConnectionState::Connected,
        reason: None,
        relay_generation,
    });

    std::assert_matches!(
        tokio::time::timeout(std::time::Duration::from_secs(1), command_rx.recv()).await,
        Ok(Some(
            kodosi_backend_client::session_relay::SessionRelayCommand::OwnerFocusChanged {
                focused: true,
            }
        ))
    );
    assert_eq!(app.client_focus.clients(session_id), ["client-a"]);
}

#[tokio::test]
async fn stale_host_generation_cannot_resolve_current_permission() {
    let mut app = test_app();
    authenticate_test_app(&mut app);
    let session_id = SessionId::new();
    let stale_generation = claim_host_generation(&mut app, session_id);
    let current_generation = claim_host_generation(&mut app, session_id);
    let incarnation_id = uuid::Uuid::now_v7();
    let key = crate::agent_intel::permission_decision_registry::PendingKey {
        session_id,
        session_incarnation_id: incarnation_id,
        tool_use_id: "tool-current".to_owned(),
    };
    let receiver = app
        .state
        .agent_intel
        .permission_decisions
        .park(key.clone())
        .expect("park request");
    assert!(app.state.agent_intel.permission_decisions.stage_metadata(
        &key,
        crate::agent_intel::permission_decision_registry::PendingPermissionMetadata {
            tool_name: "Bash".to_owned(),
            tool_input: serde_json::Value::Null,
            deadline_at_ms: 1,
            risk: crate::ApprovalRisk::Unknown,
        },
    ));
    assert_eq!(
        app.state
            .agent_intel
            .permission_decisions
            .activate_local(&key, 1),
        Some(1)
    );

    let (stale_reply, stale_result) =
        kodosi_backend_client::relay::SemanticAdmissionReply::channel();
    app.handle_session_event(RuntimeSessionEvent::RemotePermissionDecision {
        origin: current_host_relay_origin(&app, session_id, stale_generation),
        incarnation_id,
        action_id: "action-stale".to_owned(),
        request_id: key.tool_use_id.clone(),
        request_generation: 1,
        decision: "allow".to_owned(),
        decider_user_id: "remote-user".to_owned(),
        decider_device_id: Some("remote-device".to_owned()),
        reply: stale_reply,
    });
    assert!(!stale_result.await.expect("stale reply"));
    let snapshot = app.state.agent_intel.permission_decisions.snapshot();
    assert_eq!(snapshot.requests.len(), 1);
    assert_eq!(snapshot.requests[0].request_generation, 1);

    let (current_reply, current_result) =
        kodosi_backend_client::relay::SemanticAdmissionReply::channel();
    app.handle_session_event(RuntimeSessionEvent::RemotePermissionDecision {
        origin: current_host_relay_origin(&app, session_id, current_generation),
        incarnation_id,
        action_id: "action-current".to_owned(),
        request_id: key.tool_use_id,
        request_generation: 1,
        decision: "allow".to_owned(),
        decider_user_id: "remote-user".to_owned(),
        decider_device_id: Some("remote-device".to_owned()),
        reply: current_reply,
    });
    assert!(current_result.await.expect("current reply"));
    let resolved = tokio::time::timeout(std::time::Duration::from_secs(1), receiver)
        .await
        .expect("current generation should resolve")
        .expect("decision sender");
    assert_eq!(
        resolved.decision,
        crate::agent_intel::permission_decision_registry::PermissionDecision::Allow
    );
    let snapshot = app.state.agent_intel.permission_decisions.snapshot();
    assert_eq!(snapshot.requests.len(), 1);
    assert_eq!(
        snapshot.requests[0].decision_phase,
        crate::PendingPermissionDecisionPhase::Sending
    );
}

#[tokio::test]
async fn host_semantic_send_fences_backend_incarnation_and_targets_local_incarnation() {
    let mut app = test_app();
    let account = authenticate_test_app(&mut app).account_user_id;
    let session_id = SessionId::new();
    app.state
        .local
        .sessions
        .insert(claude_owned_summary(session_id, "/tmp/kodosi"));
    let local_incarnation = app
        .state
        .local
        .sessions
        .record(session_id)
        .expect("local session")
        .local_incarnation_id;
    let backend_incarnation = uuid::Uuid::now_v7();
    assert_ne!(backend_incarnation, local_incarnation);
    app.state.sharing.shared_sessions.insert(
        session_id,
        SharedSessionState::new(
            "backend-session".to_owned(),
            backend_incarnation,
            "owner-secret".to_owned(),
            ShareScope::MyDevices,
            None,
            Some([9; 32]),
            Some(1),
        ),
    );
    let origin = host_origin(&mut app, session_id);
    let text = "send to the current local process".to_owned();
    let request_id = uuid::Uuid::now_v7();
    let (reply, result) = kodosi_backend_client::relay::SemanticAdmissionReply::channel();

    app.handle_session_event(RuntimeSessionEvent::HostSemanticSend {
        origin: origin.clone(),
        request_id,
        incarnation_id: backend_incarnation,
        mode: kodosi_backend_client::session_relay::wire::RelaySemanticMode::Steer,
        payload_sha256: crate::runtime::steering::payload_sha256(&text),
        text,
        requester_user_id: account.clone(),
        requester_device_id: "other-device".to_owned(),
        reply,
    });

    assert!(result.await.expect("semantic admission reply"));
    let queued = app.query_steers(session_id, Some(&request_id.to_string()));
    assert_eq!(queued.len(), 1);
    assert_eq!(
        queued[0].session_incarnation_id,
        local_incarnation.to_string()
    );

    let (cancel_reply, cancel_result) =
        kodosi_backend_client::relay::SemanticAdmissionReply::channel();
    app.handle_session_event(RuntimeSessionEvent::HostSemanticCancel {
        origin: origin.clone(),
        request_id,
        incarnation_id: backend_incarnation,
        mode: kodosi_backend_client::session_relay::wire::RelaySemanticMode::Steer,
        payload_sha256: crate::runtime::steering::payload_sha256(&queued[0].text),
        requester_user_id: account.clone(),
        requester_device_id: "other-device".to_owned(),
        reply: cancel_reply,
    });
    assert!(cancel_result.await.expect("semantic cancellation reply"));
    let cancelled = app.query_steers(session_id, Some(&request_id.to_string()));
    assert_eq!(cancelled.len(), 1);
    assert_eq!(
        cancelled[0].delivery_state,
        crate::host_protocol::SteerDeliveryState::Cancelled
    );

    let stale_request_id = uuid::Uuid::now_v7();
    let stale_text = "must not cross an incarnation".to_owned();
    let (stale_reply, stale_result) =
        kodosi_backend_client::relay::SemanticAdmissionReply::channel();
    app.handle_session_event(RuntimeSessionEvent::HostSemanticSend {
        origin,
        request_id: stale_request_id,
        incarnation_id: uuid::Uuid::now_v7(),
        mode: kodosi_backend_client::session_relay::wire::RelaySemanticMode::Steer,
        payload_sha256: crate::runtime::steering::payload_sha256(&stale_text),
        text: stale_text,
        requester_user_id: account,
        requester_device_id: "other-device".to_owned(),
        reply: stale_reply,
    });

    assert!(!stale_result.await.expect("stale semantic reply"));
    assert!(
        app.query_steers(session_id, Some(&stale_request_id.to_string()))
            .is_empty()
    );
}

#[test]
fn stale_host_account_cannot_expire_current_auth() {
    let mut app = test_app();
    let stale_account = authenticate_test_app(&mut app);
    let current_user =
        UserId::try_from("22222222-2222-2222-2222-222222222222").expect("current user");
    authenticate_test_app_as(&mut app, current_user);
    let session_id = SessionId::new();
    let generation = claim_host_generation(&mut app, session_id);

    app.handle_session_event(RuntimeSessionEvent::HostRelayBackendAccessInvalid {
        origin: crate::session_runtime::events::HostRelayEventOrigin {
            account_origin: stale_account,
            session_id,
            relay_generation: generation,
        },
        reason: "stale relay credentials".to_owned(),
    });

    assert_eq!(app.state.identity.auth.subject(), Some(current_user));
    assert!(app.state.identity.auth.is_authenticated());
}

#[tokio::test]
async fn owner_key_distribution_requests_are_deduplicated() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let session_id = SessionId::new();

    let origin = host_origin(&mut app, session_id);
    app.handle_session_event(RuntimeSessionEvent::HostKeyDistributionRequested {
        origin: origin.clone(),
        fence_id: "fence-1".to_owned(),
    });
    app.handle_session_event(RuntimeSessionEvent::HostKeyDistributionRequested {
        origin,
        fence_id: "fence-1".to_owned(),
    });

    let work = app.state.pending_work.drain_key_redistributions();
    assert_eq!(work.len(), 1);
    assert_eq!(work[0].session_id, session_id);
    assert_eq!(work[0].fence_ids, vec!["fence-1".to_owned()]);
}

#[test]
fn own_device_list_change_stops_old_keys_and_queues_rotation() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let origin = authenticate_test_app(&mut app);
    let session_id = SessionId::new();
    app.state
        .sharing
        .shared_sessions
        .insert(session_id, test_shared_session_state());
    assert!(
        app.state
            .sharing
            .shared_sessions
            .get(session_id)
            .and_then(SharedSessionState::session_key)
            .is_some()
    );

    app.handle_session_event(RuntimeSessionEvent::UserDeviceListChanged {
        origin,
        user_id: test_user_id().to_string(),
        generation: 2,
    });

    assert!(
        app.state
            .sharing
            .shared_sessions
            .get(session_id)
            .and_then(SharedSessionState::session_key)
            .is_none(),
        "old generation must be unusable before asynchronous rotation"
    );
    assert_eq!(
        app.state.pending_work.drain_host_key_rotations(),
        vec![session_id]
    );
}

#[test]
fn stale_device_list_change_cannot_queue_pin_or_host_key_work() {
    let mut app = test_app();
    let stale_origin = authenticate_test_app(&mut app);
    authenticate_test_app_as(
        &mut app,
        UserId::try_from("22222222-2222-2222-2222-222222222222").expect("second test user id"),
    );
    let session_id = SessionId::new();
    app.state
        .sharing
        .shared_sessions
        .insert(session_id, test_shared_session_state());

    app.handle_session_event(RuntimeSessionEvent::UserDeviceListChanged {
        origin: stale_origin,
        user_id: test_user_id().to_string(),
        generation: 2,
    });

    assert!(
        app.state
            .sharing
            .shared_sessions
            .get(session_id)
            .and_then(SharedSessionState::session_key)
            .is_some(),
        "stale event must not invalidate hosted session keys"
    );
    assert!(!app.state.pending_work.take_pin_reset_all());
    assert!(app.state.pending_work.pop_identity_lifecycle().is_none());
    assert!(app.state.pending_work.drain_pin_resets().is_empty());
    assert!(app.state.pending_work.drain_pin_refreshes().is_empty());
    assert!(app.state.pending_work.drain_host_key_rotations().is_empty());
}

#[test]
fn control_replay_exhaustion_queues_a_key_rotation() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let session_id = SessionId::new();

    let origin = host_origin(&mut app, session_id);
    app.handle_session_event(RuntimeSessionEvent::HostKeyRotationRequired {
        origin,
        reason: "control replay window exhausted".to_owned(),
        fail_closed: true,
    });

    assert_eq!(
        app.state.pending_work.drain_host_key_rotations(),
        vec![session_id],
        "a fail-closed control window must rotate the key epoch"
    );
}

#[test]
fn control_replay_watermark_queues_rotation_before_controls_are_refused() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let session_id = SessionId::new();

    let origin = host_origin(&mut app, session_id);
    app.handle_session_event(RuntimeSessionEvent::HostKeyRotationRequired {
        origin,
        reason: "control replay window approaching capacity".to_owned(),
        fail_closed: false,
    });

    assert_eq!(
        app.state.pending_work.drain_host_key_rotations(),
        vec![session_id]
    );
}

#[test]
fn repeated_rotation_demands_collapse_to_one_pending_rotation() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let session_id = SessionId::new();

    let origin = host_origin(&mut app, session_id);
    for _ in 0..5 {
        app.handle_session_event(RuntimeSessionEvent::HostKeyRotationRequired {
            origin: origin.clone(),
            reason: "control replay window exhausted".to_owned(),
            fail_closed: true,
        });
    }

    assert_eq!(
        app.state.pending_work.drain_host_key_rotations(),
        vec![session_id],
        "rotation demands must not pile up while one rotation is pending"
    );
}

#[tokio::test]
async fn current_relay_connected_transition_replays_durable_remote_semantics() {
    let mut app = test_app();
    let account = authenticate_test_app(&mut app).account_user_id;
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let text = "retry after reconnect".to_owned();
    let request = crate::runtime::remote_semantics::RemoteSemanticRequest {
        account_user_id: account.clone(),
        requester_device_id: "requester-device".to_owned(),
        session_id: session_id.to_string(),
        incarnation_id,
        request_id: uuid::Uuid::now_v7(),
        mode: kodosi_backend_client::session_relay::wire::RelaySemanticMode::Steer,
        payload_sha256: kodosi_backend_client::crypto::sha256_hex(text.as_bytes()),
        text: text.clone(),
        signer: Some(
            crate::runtime::remote_semantics::RemoteSemanticSignerSnapshot {
                account_user_id: account.clone(),
                session_id: session_id.to_string(),
                incarnation_id,
                owner_user_id: account,
                owner_device_id: "owner-device".to_owned(),
                owner_signing_public_key: vec![1],
                device_list_generation: 1,
                identity_fingerprint: [7; 32],
            },
        ),
    };
    let admitted = app
        .remote_semantics
        .admit(request)
        .expect("durable semantic request should admit");
    app.state.discovery.replace_remote_sessions(
        vec![RemoteSessionRecord {
            summary: SessionSummary::new_remote(
                session_id,
                "Remote".to_owned(),
                "Owner".to_owned(),
                Some(test_user_id()),
                ShareScope::Friends,
                AccessLevel::Suggest,
                TerminalSize::default(),
            ),
            incarnation_id: Some(incarnation_id),
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
    let (command_tx, mut command_rx) = mpsc::channel(2);
    let relay_generation = app.state.remote.session_relays.attach_for_test(
        session_id,
        CancellationToken::new(),
        command_tx,
        ShareScope::Friends,
        kodosi_backend_client::session_relay::RemoteRelayMode::SharedParticipant,
        ConnectionState::Connecting,
        tokio::spawn(std::future::pending()),
    );
    assert!(command_rx.try_recv().is_err());

    app.handle_session_event(RuntimeSessionEvent::RemoteSessionConnectionChanged {
        id: session_id,
        status: ConnectionState::Connected,
        reason: None,
        relay_generation,
    });

    let replayed = tokio::time::timeout(std::time::Duration::from_secs(1), command_rx.recv())
        .await
        .expect("connected relay should replay durable semantics promptly");
    std::assert_matches!(
        replayed,
        Some(kodosi_backend_client::session_relay::SessionRelayCommand::SemanticSend {
            request_id,
            incarnation_id: replayed_incarnation,
            text: replayed_text,
            ..
        }) if request_id == admitted.request_id
            && replayed_incarnation == incarnation_id
            && replayed_text == text
    );
}

#[test]
fn remote_connection_connected_does_not_clear_access_block() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let session_id = SessionId::new();
    let summary = SessionSummary::new_remote(
        session_id,
        "Remote".to_owned(),
        "Owner".to_owned(),
        Some(test_user_id()),
        ShareScope::Friends,
        AccessLevel::Suggest,
        TerminalSize::new(120, 40)
            .unwrap_or_else(|error| panic!("terminal size should be valid: {error}")),
    );

    app.state.discovery.replace_remote_sessions(
        vec![RemoteSessionRecord {
            summary,
            incarnation_id: None,
            room_id: None,
            connection_state: Some(ConnectionState::Reconnecting),
            connection_reason: None,
            access_state: Some(RemoteSessionAccessState::AccessDenied),
            access_reason: Some("Trust again".to_owned()),
            access_issue: Some(RemoteSessionAccessIssue::PeerIdentityChanged),
            viewer_blocked: true,
            viewer_hidden: false,
        }],
        false,
    );

    let relay_generation = app
        .state
        .remote
        .session_relays
        .reserve_generation(session_id)
        .expect("test relay generation");
    app.handle_session_event(RuntimeSessionEvent::RemoteSessionConnectionChanged {
        id: session_id,
        status: ConnectionState::Connected,
        reason: None,
        relay_generation,
    });

    let record = app
        .state
        .discovery
        .session(session_id)
        .expect("remote record should still exist");
    assert!(record.viewer_blocked);
    assert_eq!(
        record.access_issue,
        Some(RemoteSessionAccessIssue::PeerIdentityChanged)
    );
}

fn seed_visible_remote_record(app: &mut Runtime, id: SessionId) {
    let summary = SessionSummary::new_remote(
        id,
        "Remote".to_owned(),
        "Owner".to_owned(),
        Some(test_user_id()),
        ShareScope::Friends,
        AccessLevel::Suggest,
        TerminalSize::new(120, 40)
            .unwrap_or_else(|error| panic!("terminal size should be valid: {error}")),
    );
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
    let ids = app.state.discovery.non_hidden_remote_session_ids();
    app.state.shelf.sync_remote_sessions(&ids);
}

#[tokio::test]
async fn hide_marks_record_and_prunes_shelf_visibility() {
    let hidden_dir = tempfile::tempdir().expect("hidden-session tempdir");
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    app.hidden_session_store = crate::discovery::hidden_sessions::HiddenSessionStore::at(
        hidden_dir.path().join("hidden-sessions.json"),
    );
    authenticate_test_app(&mut app);
    let session_id = SessionId::new();
    seed_visible_remote_record(&mut app, session_id);
    app.state.shelf.activate(ShelfItem::Remote(session_id));

    let changed = crate::runtime::remote_sessions::hide(&mut app, session_id)
        .await
        .expect("hide persists");
    assert!(changed, "first hide should report a state change");

    let record = app
        .state
        .discovery
        .session(session_id)
        .expect("record still in catalog after hide");
    assert!(record.viewer_hidden);
    assert_eq!(
        app.hidden_session_store
            .load(&test_user_id().to_string())
            .expect("persisted hidden sessions"),
        std::collections::BTreeSet::from([session_id])
    );

    assert!(
        !app.state
            .shelf
            .visible_sessions()
            .contains(&ShelfItem::Remote(session_id)),
        "hidden remote must leave the shelf visible_sessions"
    );

    assert!(
        !crate::runtime::remote_sessions::hide(&mut app, session_id)
            .await
            .expect("repeat hide"),
        "hiding an already-hidden session is a no-op"
    );

    assert!(
        crate::runtime::remote_sessions::unhide(&mut app, session_id,).expect("unhide persists")
    );
    let record = app
        .state
        .discovery
        .session(session_id)
        .expect("record still in catalog after unhide");
    assert!(!record.viewer_hidden);
    assert!(
        app.hidden_session_store
            .load(&test_user_id().to_string())
            .expect("persisted unhide")
            .is_empty()
    );
}

#[tokio::test]
async fn inject_participant_input_and_focus_use_relay_but_resize_is_owner_controlled() {
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
    let incarnation_id = uuid::Uuid::now_v7();
    let summary = SessionSummary::new_remote(
        session_id,
        "Remote".to_owned(),
        "Owner".to_owned(),
        Some(test_user_id()),
        ShareScope::Friends,
        AccessLevel::Inject,
        TerminalSize::default(),
    );
    app.state.discovery.replace_remote_sessions(
        vec![RemoteSessionRecord {
            summary,
            incarnation_id: Some(incarnation_id),
            room_id: None,
            connection_state: Some(ConnectionState::Connected),
            connection_reason: None,
            access_state: Some(RemoteSessionAccessState::Ready),
            access_reason: None,
            access_issue: None,
            viewer_blocked: false,
            viewer_hidden: false,
        }],
        false,
    );
    let (command_tx, mut command_rx) = mpsc::channel(4);
    app.state.remote.session_relays.attach_for_test(
        session_id,
        CancellationToken::new(),
        command_tx,
        ShareScope::Friends,
        kodosi_backend_client::session_relay::RemoteRelayMode::SharedParticipant,
        ConnectionState::Connected,
        tokio::spawn(std::future::pending::<()>()),
    );

    app.send_input_to_session(
        session_id,
        incarnation_id,
        crate::session_runtime::commands::SessionInput::new(b"whoami".to_vec()),
    )
    .await
    .expect("participant input should enqueue");
    let resize_outcome = app
        .resize_session(
            session_id,
            TerminalSize::new(24, 80).expect("valid size"),
            None,
        )
        .await
        .expect("remote resize should return an explicit rejection");
    assert_eq!(
        resize_outcome,
        crate::local_sessions::ops::LocalResizeOutcome::RejectedRemoteOwnerOnly
    );
    app.focus_session(session_id, "tile".to_owned())
        .await
        .expect("participant focus should enqueue");

    let input_command = tokio::time::timeout(std::time::Duration::from_secs(1), command_rx.recv())
        .await
        .expect("participant input should enqueue promptly");
    std::assert_matches!(
        input_command,
        Some(kodosi_backend_client::session_relay::SessionRelayCommand::OwnerInject { payload })
            if payload == b"whoami"
    );
    let focus_command = tokio::time::timeout(std::time::Duration::from_secs(1), command_rx.recv())
        .await
        .expect("participant focus should enqueue promptly");
    std::assert_matches!(
        focus_command,
        Some(
            kodosi_backend_client::session_relay::SessionRelayCommand::OwnerFocusChanged {
                focused: true,
            }
        )
    );

    app.state.apply_remote_access_revoked(session_id);
    let error = app
        .send_input_to_session(
            session_id,
            incarnation_id,
            crate::session_runtime::commands::SessionInput::new(b"\r".to_vec()),
        )
        .await
        .expect_err("revoked participant input must fail closed");
    assert!(error.to_string().contains("requires current Inject access"));
    assert!(command_rx.try_recv().is_err());
}

#[tokio::test]
async fn owned_remote_use_my_size_enqueues_owner_resize_claim() {
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
    let mut summary = SessionSummary::new_remote(
        session_id,
        "Remote".to_owned(),
        "You".to_owned(),
        Some(test_user_id()),
        ShareScope::Friends,
        AccessLevel::Approve,
        TerminalSize::default(),
    );
    summary.role = kodosi_domain::session::SessionRole::Owner;
    app.state.discovery.replace_remote_sessions(
        vec![RemoteSessionRecord {
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
        }],
        false,
    );
    let (command_tx, mut command_rx) = mpsc::channel(1);
    app.state.remote.session_relays.attach_for_test(
        session_id,
        CancellationToken::new(),
        command_tx,
        ShareScope::Friends,
        kodosi_backend_client::session_relay::RemoteRelayMode::OwnerParticipant,
        ConnectionState::Connected,
        tokio::spawn(std::future::pending::<()>()),
    );

    let action_id = "resize-action".to_owned();
    app.claim_size_and_resize(
        session_id,
        action_id.clone(),
        TerminalSize::new(30, 100).expect("valid size"),
        None,
    )
    .await
    .expect("owner size claim should enqueue");

    std::assert_matches!(
        command_rx.recv().await,
        Some(
            kodosi_backend_client::session_relay::SessionRelayCommand::OwnerResize {
                action_id: received_action_id,
                rows: 30,
                cols: 100,
                pixel_geometry: None,
                claim: true,
            }
        ) if received_action_id == action_id
    );
}

fn insert_pending_remote_resize(
    app: &mut Runtime,
    session_id: SessionId,
    action_id: &str,
    relay_generation: u64,
) -> crate::host_protocol::TerminalResizeIdentity {
    let identity = super::resize_identity_for_test(
        action_id,
        uuid::Uuid::now_v7().to_string(),
        "terminal-subscription",
        9,
        3,
        30,
        100,
    );
    app.pending_remote_resizes.insert(
        (session_id, action_id.to_owned()),
        crate::runtime::PendingRemoteResize {
            session_id,
            relay_generation,
            identity: identity.clone(),
        },
    );
    identity
}

#[test]
fn exact_remote_resize_result_emits_one_correlated_terminal_receipt() {
    let mut app = test_app();
    let session_id = SessionId::new();
    let identity = insert_pending_remote_resize(&mut app, session_id, "resize-accepted", 7);

    assert!(
        !app.handle_session_event(RuntimeSessionEvent::RemoteActionResult {
            id: session_id,
            action_id: "resize-accepted".to_owned(),
            request_id: Some("resize-accepted".to_owned()),
            request_generation: None,
            status: kodosi_domain::lifecycle::RemoteActionStatus::Accepted,
            relay_generation: 7,
        })
    );

    std::assert_matches!(
        app.state.runtime_outbox.drain_terminal_control().as_slice(),
        [crate::TerminalEvent::ResizeApplied {
            session_id: result_session_id,
            identity: applied,
        }] if result_session_id == &session_id.to_string() && applied == &identity
    );
    assert!(app.state.runtime_outbox.drain_sessions().is_empty());
    assert!(app.pending_remote_resizes.is_empty());
}

#[test]
fn rejected_remote_resize_emits_rejection_without_permission_projection() {
    let mut app = test_app();
    let session_id = SessionId::new();
    let identity = insert_pending_remote_resize(&mut app, session_id, "resize-rejected", 4);

    app.handle_session_event(RuntimeSessionEvent::RemoteActionResult {
        id: session_id,
        action_id: "resize-rejected".to_owned(),
        request_id: Some("resize-rejected".to_owned()),
        request_generation: None,
        status: kodosi_domain::lifecycle::RemoteActionStatus::Rejected,
        relay_generation: 4,
    });

    std::assert_matches!(
        app.state.runtime_outbox.drain_terminal_control().as_slice(),
        [crate::TerminalEvent::ResizeRejected {
            identity: rejected,
            reason,
            ..
        }] if rejected == &identity && reason.contains("could not apply")
    );
    assert!(app.state.runtime_outbox.drain_agent_intel().is_empty());
    assert!(app.state.runtime_outbox.drain_sessions().is_empty());
}

#[test]
fn stale_remote_resize_result_cannot_consume_current_generation_request() {
    let mut app = test_app();
    let session_id = SessionId::new();
    let identity = insert_pending_remote_resize(&mut app, session_id, "resize-current", 12);

    app.handle_session_event(RuntimeSessionEvent::RemoteActionResult {
        id: session_id,
        action_id: "resize-current".to_owned(),
        request_id: Some("resize-current".to_owned()),
        request_generation: None,
        status: kodosi_domain::lifecycle::RemoteActionStatus::Accepted,
        relay_generation: 11,
    });
    assert!(app.state.runtime_outbox.drain_terminal_control().is_empty());
    assert_eq!(app.pending_remote_resizes.len(), 1);

    app.handle_session_event(RuntimeSessionEvent::RemoteActionResult {
        id: session_id,
        action_id: "resize-current".to_owned(),
        request_id: Some("resize-current".to_owned()),
        request_generation: None,
        status: kodosi_domain::lifecycle::RemoteActionStatus::Duplicate,
        relay_generation: 12,
    });
    std::assert_matches!(
        app.state.runtime_outbox.drain_terminal_control().as_slice(),
        [crate::TerminalEvent::ResizeApplied { identity: applied, .. }]
            if applied == &identity
    );
    assert!(app.pending_remote_resizes.is_empty());
}

#[test]
fn remote_resize_teardown_rejects_only_the_matching_session_and_generation() {
    let mut app = test_app();
    let first = SessionId::new();
    let second = SessionId::new();
    let rejected_identity = insert_pending_remote_resize(&mut app, first, "first-old", 2);
    insert_pending_remote_resize(&mut app, first, "first-current", 3);
    insert_pending_remote_resize(&mut app, second, "second", 3);

    app.reject_pending_remote_resizes(first, Some(2), "relay closed");

    std::assert_matches!(
        app.state.runtime_outbox.drain_terminal_control().as_slice(),
        [crate::TerminalEvent::ResizeRejected {
            session_id,
            identity,
            reason,
        }] if session_id == &first.to_string()
            && identity == &rejected_identity
            && reason == "relay closed"
    );
    assert!(
        app.pending_remote_resizes
            .contains_key(&(first, "first-current".to_owned()))
    );
    assert!(
        app.pending_remote_resizes
            .contains_key(&(second, "second".to_owned()))
    );
}

#[tokio::test]
async fn non_inject_participant_terminal_controls_fail_closed() {
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
    seed_visible_remote_record(&mut app, session_id);

    let outcome = app
        .resize_session(session_id, TerminalSize::default(), None)
        .await
        .expect("every remote session reports the owner-controlled resize outcome");
    assert_eq!(
        outcome,
        crate::local_sessions::ops::LocalResizeOutcome::RejectedRemoteOwnerOnly
    );
}

fn test_remote_checkpoint(rows: u16, cols: u16) -> kodosi_domain::terminal::TerminalCheckpointV2 {
    let mut terminal = ghostty_vt::Terminal::new(cols, rows, ghostty_vt::TerminalPolicy::default())
        .expect("test terminal");
    let semantic = terminal
        .semantic_checkpoint(ghostty_vt::CheckpointLimits::default())
        .expect("semantic checkpoint");
    let state = terminal.state().expect("terminal state");
    kodosi_domain::terminal::TerminalCheckpointV2::new(
        TerminalSize::new(rows, cols).expect("valid terminal size"),
        kodosi_domain::terminal::TerminalScreen::Primary,
        semantic.into_bytes(),
        state.cursor_x,
        state.cursor_y,
        !state.cursor_visible,
    )
    .expect("valid terminal checkpoint")
}

fn test_remote_presentation(
    rows: u16,
    cols: u16,
    line: &str,
) -> kodosi_domain::terminal::TerminalPresentationV2 {
    kodosi_domain::terminal::TerminalPresentationV2::new(
        TerminalSize::new(rows, cols).expect("valid terminal size"),
        kodosi_domain::terminal::TerminalScreen::Primary,
        vec![line.to_owned(); usize::from(rows)],
        0,
        0,
        false,
    )
    .expect("valid terminal presentation")
}

#[tokio::test]
async fn old_exit_after_replacement_preserves_relay_terminal_cache_and_hub() {
    let mut app = test_app();
    let session_id = SessionId::new();
    let (first_tx, _first_rx) = mpsc::channel(1);
    let first_generation = app.state.remote.session_relays.attach_for_test(
        session_id,
        CancellationToken::new(),
        first_tx,
        ShareScope::Friends,
        kodosi_backend_client::session_relay::RemoteRelayMode::SharedParticipant,
        ConnectionState::Connected,
        tokio::spawn(std::future::pending::<()>()),
    );
    let (replacement_tx, _replacement_rx) = mpsc::channel(1);
    let replacement_generation = app.state.remote.session_relays.attach_for_test(
        session_id,
        CancellationToken::new(),
        replacement_tx,
        ShareScope::Friends,
        kodosi_backend_client::session_relay::RemoteRelayMode::SharedParticipant,
        ConnectionState::Connecting,
        tokio::spawn(std::future::pending::<()>()),
    );
    app.handle_session_event(RuntimeSessionEvent::RemoteCheckpoint {
        id: session_id,
        next_sequence: 4,
        checkpoint: test_remote_checkpoint(24, 80),
        application: kodosi_backend_client::session_relay::events::ApplicationAck::default(),
        relay_generation: replacement_generation,
    });
    app.handle_session_event(RuntimeSessionEvent::RemotePlainPresentation {
        id: session_id,
        presentation: test_remote_presentation(24, 80, "replacement"),
        relay_generation: replacement_generation,
    });

    app.handle_session_event(RuntimeSessionEvent::RemoteSessionRelayExited {
        id: session_id,
        relay_generation: first_generation,
    });

    assert!(app.state.remote.session_relays.contains(session_id));
    assert_eq!(
        app.state.remote.session_relays.generation(session_id),
        replacement_generation
    );
    assert_eq!(
        app.state.remote.session_relays.status(session_id),
        Some(ConnectionState::Connecting)
    );
    let cached = app
        .remote_terminal
        .cached(session_id)
        .expect("replacement terminal remains cached");
    assert_eq!(cached.next_sequence, 4);
    assert_eq!(
        cached
            .presentation
            .expect("replacement presentation remains")
            .plain_lines,
        vec!["replacement".to_owned(); 24]
    );
    assert_eq!(app.terminal_hub.next_sequence(session_id), Some(4));
    assert_eq!(app.terminal_hub.close_reason(session_id), None);
}

#[test]
fn stale_relay_terminal_raw_presentation_status_and_state_are_ignored() {
    let mut app = test_app();
    let session_id = SessionId::new();
    seed_visible_remote_record(&mut app, session_id);
    let stale_generation = app
        .state
        .remote
        .session_relays
        .reserve_generation(session_id)
        .expect("stale generation");
    let current_generation = app
        .state
        .remote
        .session_relays
        .reserve_generation(session_id)
        .expect("current generation");
    app.handle_session_event(RuntimeSessionEvent::RemoteCheckpoint {
        id: session_id,
        next_sequence: 5,
        checkpoint: test_remote_checkpoint(24, 80),
        application: kodosi_backend_client::session_relay::events::ApplicationAck::default(),
        relay_generation: current_generation,
    });
    app.handle_session_event(RuntimeSessionEvent::RemoteRawBatch {
        id: session_id,
        first_sequence: 5,
        next_sequence: 6,
        chunks: vec![b"current".to_vec()],
        relay_generation: current_generation,
    });
    app.handle_session_event(RuntimeSessionEvent::RemotePlainPresentation {
        id: session_id,
        presentation: test_remote_presentation(24, 80, "current"),
        relay_generation: current_generation,
    });
    app.handle_session_event(RuntimeSessionEvent::RemoteSessionConnectionChanged {
        id: session_id,
        status: ConnectionState::Connected,
        reason: None,
        relay_generation: current_generation,
    });
    app.handle_session_event(RuntimeSessionEvent::RemoteStateChanged {
        id: session_id,
        state: SessionState::Running,
        relay_generation: current_generation,
    });

    app.handle_session_event(RuntimeSessionEvent::RemoteCheckpoint {
        id: session_id,
        next_sequence: 90,
        checkpoint: test_remote_checkpoint(12, 34),
        application: kodosi_backend_client::session_relay::events::ApplicationAck::default(),
        relay_generation: stale_generation,
    });
    app.handle_session_event(RuntimeSessionEvent::RemoteRawBatch {
        id: session_id,
        first_sequence: 6,
        next_sequence: 7,
        chunks: vec![b"stale".to_vec()],
        relay_generation: stale_generation,
    });
    app.handle_session_event(RuntimeSessionEvent::RemotePlainPresentation {
        id: session_id,
        presentation: test_remote_presentation(12, 34, "stale"),
        relay_generation: stale_generation,
    });
    app.handle_session_event(RuntimeSessionEvent::RemoteSessionConnectionChanged {
        id: session_id,
        status: ConnectionState::Offline,
        reason: Some("old relay exited".to_owned()),
        relay_generation: stale_generation,
    });
    app.handle_session_event(RuntimeSessionEvent::RemoteStateChanged {
        id: session_id,
        state: SessionState::Stopped,
        relay_generation: stale_generation,
    });

    let record = app
        .state
        .discovery
        .session(session_id)
        .expect("remote record remains");
    assert_eq!(record.connection_state, Some(ConnectionState::Connected));
    assert_eq!(record.summary.state, SessionState::Running);
    let cached = app
        .remote_terminal
        .cached(session_id)
        .expect("current terminal remains");
    assert_eq!(cached.next_sequence, 5);
    assert_eq!(
        cached
            .checkpoint
            .expect("current checkpoint remains")
            .size(),
        TerminalSize::new(24, 80).expect("valid terminal size")
    );
    assert_eq!(
        cached
            .presentation
            .expect("current presentation remains")
            .plain_lines,
        vec!["current".to_owned(); 24]
    );
    assert_eq!(app.terminal_hub.next_sequence(session_id), Some(6));
    assert_eq!(app.terminal_hub.close_reason(session_id), None);
}

#[test]
fn hidden_remote_is_dropped_from_session_catalog_snapshot() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let session_id = SessionId::new();
    seed_visible_remote_record(&mut app, session_id);

    let baseline = crate::runtime::session_catalog::build_session_catalog(&app);
    assert!(
        baseline
            .iter()
            .any(|entry| entry.id() == session_id.to_string()),
        "non-hidden session must surface in snapshot"
    );

    app.state
        .discovery
        .set_hidden_session_ids(std::collections::BTreeSet::from([session_id]));
    let after = crate::runtime::session_catalog::build_session_catalog(&app);
    assert!(
        after
            .iter()
            .all(|entry| entry.id() != session_id.to_string()),
        "hidden session must be filtered from snapshot"
    );
}

#[tokio::test]
async fn local_resize_drains_coordinator_events_while_waiting_for_completion() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let session_id = SessionId::new();
    let (screen_tx, mut screen_rx) = mpsc::channel::<SessionScreenInstruction>(1);
    let (pty_tx, _pty_rx) = mpsc::channel::<SessionPtyInstruction>(1);
    insert_owned_session_for_test(
        &mut app,
        claude_owned_summary(session_id, "/tmp/kodosi"),
        OwnedSessionHandle {
            senders: SessionSenders::new(Some(screen_tx), Some(pty_tx)),
            cancellation: CancellationToken::new(),
            join_handle: tokio::spawn(std::future::pending::<()>()),
        },
    );
    let events = app.session_events_tx.clone();
    for _ in 0..crate::runtime::SESSION_EVENT_CAPACITY {
        events
            .try_send(RuntimeSessionEvent::TerminalBell {
                origin: local_coordinator_origin(&app, session_id),
            })
            .expect("fill session event lane exactly");
    }
    assert_eq!(events.capacity(), 0);

    let requested = TerminalSize::new(30, 100).expect("valid terminal size");
    let resize = app.resize_session(session_id, requested, None);
    let coordinator = async {
        let Some(SessionScreenInstruction::Resize {
            size,
            completion: Some(completion),
            ..
        }) = screen_rx.recv().await
        else {
            panic!("expected correlated resize")
        };
        assert_eq!(size, requested);
        while events.capacity() == 0 {
            tokio::task::yield_now().await;
        }
        completion
            .send(Ok(()))
            .expect("runtime still awaits exact resize result");
    };

    let (outcome, ()) = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        tokio::join!(resize, coordinator)
    })
    .await
    .expect("resize must not self-deadlock on its own full event lane");
    assert_eq!(
        outcome.expect("resize applies"),
        crate::local_sessions::ops::LocalResizeOutcome::Applied
    );
}

#[tokio::test]
async fn local_terminal_controls_win_over_same_id_discovery_twin() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let session_id = SessionId::new();
    let (screen_tx, mut screen_rx) = mpsc::channel::<SessionScreenInstruction>(8);
    let (pty_tx, mut pty_rx) = mpsc::channel::<SessionPtyInstruction>(4);
    let cancellation = CancellationToken::new();
    insert_owned_session_for_test(
        &mut app,
        claude_owned_summary(session_id, "/tmp/kodosi"),
        OwnedSessionHandle {
            senders: SessionSenders::new(Some(screen_tx), Some(pty_tx)),
            cancellation,
            join_handle: tokio::spawn(std::future::pending::<()>()),
        },
    );
    seed_visible_remote_record(&mut app, session_id);
    let incarnation_id = app
        .state
        .local
        .sessions
        .record(session_id)
        .expect("local session should remain authoritative")
        .local_incarnation_id;

    app.send_input_to_session(
        session_id,
        incarnation_id,
        crate::session_runtime::commands::SessionInput::new(b"local".to_vec()),
    )
    .await
    .expect("same-id local input must not route to participant discovery");

    let Some(SessionScreenInstruction::Input(input)) = screen_rx.recv().await else {
        panic!("expected local screen input");
    };
    assert_eq!(input.as_bytes(), b"local");

    let requested_size = TerminalSize::new(30, 100).expect("valid terminal size");
    let resize = app.resize_session(session_id, requested_size, None);
    let acknowledge_resize = async {
        match screen_rx.recv().await {
            Some(SessionScreenInstruction::Resize {
                size,
                pixel_geometry: _,
                completion,
            }) => {
                assert_eq!(size, requested_size);
                completion
                    .expect("correlated local resize")
                    .send(Ok(()))
                    .expect("resize caller remains live");
            }
            other => panic!("expected local resize, got {other:?}"),
        }
    };
    let (resize_result, ()) = tokio::join!(resize, acknowledge_resize);
    resize_result.expect("same-id resize must stay local");

    app.focus_session(session_id, "tile".to_owned())
        .await
        .expect("same-id focus must stay local");
    std::assert_matches!(
        screen_rx.recv().await,
        Some(SessionScreenInstruction::Focus { client_id }) if client_id == "tile"
    );
    app.blur_session(session_id, "tile".to_owned())
        .await
        .expect("same-id blur must stay local");
    std::assert_matches!(
        screen_rx.recv().await,
        Some(SessionScreenInstruction::Blur { client_id }) if client_id == "tile"
    );

    let claim = app.claim_size_and_resize(
        session_id,
        "local-resize-action".to_owned(),
        TerminalSize::new(40, 120).expect("valid terminal size"),
        None,
    );
    let apply = async {
        let Some(SessionScreenInstruction::Resize {
            size,
            completion: Some(completion),
            ..
        }) = screen_rx.recv().await
        else {
            panic!("claiming resize must await the screen actor");
        };
        assert_eq!(
            size,
            TerminalSize::new(40, 120).expect("valid terminal size")
        );
        completion.send(Ok(())).expect("resize completion receiver");
    };
    let (result, ()) = tokio::join!(claim, apply);
    result.expect("same-id size claim must stay local");

    app.interrupt_session(session_id)
        .await
        .expect("same-id interrupt must stay local");
    std::assert_matches!(pty_rx.recv().await, Some(SessionPtyInstruction::Interrupt));
    app.stop_session(session_id)
        .await
        .expect("same-id stop must stay local");
    std::assert_matches!(pty_rx.recv().await, Some(SessionPtyInstruction::Kill));
}

#[tokio::test]
async fn failed_participant_focus_enqueue_does_not_poison_retry() {
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
    let summary = SessionSummary::new_remote(
        session_id,
        "Remote".to_owned(),
        "Owner".to_owned(),
        Some(test_user_id()),
        ShareScope::Friends,
        AccessLevel::Inject,
        TerminalSize::default(),
    );
    app.state.discovery.replace_remote_sessions(
        vec![RemoteSessionRecord {
            summary,
            incarnation_id: None,
            room_id: None,
            connection_state: Some(ConnectionState::Connected),
            connection_reason: None,
            access_state: Some(RemoteSessionAccessState::Ready),
            access_reason: None,
            access_issue: None,
            viewer_blocked: false,
            viewer_hidden: false,
        }],
        false,
    );

    app.focus_session(session_id, "tile".to_owned())
        .await
        .expect_err("focus without an active relay must fail");

    let (command_tx, mut command_rx) = mpsc::channel(1);
    app.state.remote.session_relays.attach_for_test(
        session_id,
        CancellationToken::new(),
        command_tx,
        ShareScope::Friends,
        kodosi_backend_client::session_relay::RemoteRelayMode::SharedParticipant,
        ConnectionState::Connected,
        tokio::spawn(std::future::pending::<()>()),
    );
    assert!(app.state.remote.session_relays.contains(session_id));
    assert_eq!(
        app.state.remote.session_relays.status(session_id),
        Some(ConnectionState::Connected)
    );
    app.focus_session(session_id, "tile".to_owned())
        .await
        .unwrap_or_else(|error| {
            panic!(
                "retry must enqueue after the relay becomes active: {error}; logs={:?}",
                app.state.logs
            )
        });
    let focus_command = tokio::time::timeout(std::time::Duration::from_secs(1), command_rx.recv())
        .await
        .expect("focus retry should enqueue promptly");
    std::assert_matches!(
        focus_command,
        Some(
            kodosi_backend_client::session_relay::SessionRelayCommand::OwnerFocusChanged {
                focused: true,
            }
        )
    );
}

#[tokio::test]
async fn local_session_wins_over_same_id_discovery_twin() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let session_id = SessionId::new();
    insert_owned_session_for_test(
        &mut app,
        claude_owned_summary(session_id, "/tmp/kodosi"),
        test_session_handle(),
    );
    seed_visible_remote_record(&mut app, session_id);

    let catalog = crate::runtime::session_catalog::build_session_catalog(&app);
    let matching = catalog
        .iter()
        .filter(|entry| entry.id() == session_id.to_string())
        .collect::<Vec<_>>();

    assert_eq!(matching.len(), 1);
    std::assert_matches!(
        matching[0],
        crate::host_protocol::SessionListEntry::Local { .. }
    );
}

#[test]
fn discovery_refresh_preserves_viewer_hidden_flag() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let session_id = SessionId::new();
    seed_visible_remote_record(&mut app, session_id);
    app.state
        .discovery
        .set_hidden_session_ids(std::collections::BTreeSet::from([session_id]));

    let summary = SessionSummary::new_remote(
        session_id,
        "Remote".to_owned(),
        "Owner".to_owned(),
        Some(test_user_id()),
        ShareScope::Friends,
        AccessLevel::Suggest,
        TerminalSize::new(120, 40)
            .unwrap_or_else(|error| panic!("terminal size should be valid: {error}")),
    );
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

    let record = app
        .state
        .discovery
        .session(session_id)
        .expect("record present after refresh");
    assert!(
        record.viewer_hidden,
        "discovery refresh must carry the viewer-side hide forward"
    );
}

#[tokio::test]
async fn permission_action_result_exposes_tool_use_id_to_embedder() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let session_id = SessionId::new();
    let account = authenticate_test_app(&mut app).account_user_id;
    seed_visible_remote_record(&mut app, session_id);
    let record = app
        .state
        .discovery
        .session_mut(session_id)
        .expect("remote record");
    record.summary.access = AccessLevel::Approve;
    let incarnation_id = uuid::Uuid::now_v7();
    record.incarnation_id = Some(incarnation_id);
    let key = crate::agent_intel::permission_decision_registry::PendingKey {
        session_id,
        session_incarnation_id: incarnation_id,
        tool_use_id: "tool-use-42".to_owned(),
    };
    assert!(
        app.state
            .agent_intel
            .permission_decisions
            .activate_remote(
                key,
                7,
                1,
                crate::agent_intel::permission_decision_registry::PendingPermissionMetadata {
                    tool_name: "Bash".to_owned(),
                    tool_input: serde_json::Value::Null,
                    deadline_at_ms: 100,
                    risk: crate::ApprovalRisk::Unknown,
                },
            )
            .changed()
    );
    let (command_tx, mut command_rx) =
        mpsc::channel::<kodosi_backend_client::session_relay::SessionRelayCommand>(1);
    let relay_generation = app.state.remote.session_relays.attach_for_test(
        session_id,
        CancellationToken::new(),
        command_tx,
        ShareScope::Friends,
        kodosi_backend_client::session_relay::RemoteRelayMode::SharedParticipant,
        ConnectionState::Connected,
        tokio::spawn(std::future::pending::<()>()),
    );

    assert_eq!(
        crate::runtime::remote_sessions::permission_decision(
            &mut app,
            session_id,
            "tool-use-42",
            7,
            "allow",
        ),
        crate::runtime::SessionRelayCommandDispatchOutcome::Enqueued
    );
    let first_command = tokio::time::timeout(std::time::Duration::from_secs(1), command_rx.recv())
        .await
        .expect("first permission decision should enqueue promptly");
    let action_id = match first_command {
        Some(kodosi_backend_client::session_relay::SessionRelayCommand::PermissionDecision {
            action_id,
            ..
        }) => action_id,
        other => panic!("expected permission decision, got {other:?}"),
    };
    assert_eq!(
        crate::runtime::remote_sessions::permission_decision(
            &mut app,
            session_id,
            "tool-use-42",
            7,
            "allow",
        ),
        crate::runtime::SessionRelayCommandDispatchOutcome::Enqueued
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), command_rx.recv())
            .await
            .is_err(),
        "an identical retry must not duplicate an in-flight relay command"
    );
    assert_eq!(
        app.remote_permission_actions
            .pending_for_incarnation(&account, session_id, incarnation_id)
            .len(),
        1
    );

    app.handle_session_event(RuntimeSessionEvent::RemoteActionResult {
        id: session_id,
        action_id: action_id.clone(),
        request_id: Some("tool-use-42".to_owned()),
        request_generation: Some(7),
        status: kodosi_domain::lifecycle::RemoteActionStatus::Busy,
        relay_generation,
    });
    assert_eq!(
        app.remote_permission_actions
            .pending_for_incarnation(&account, session_id, incarnation_id)
            .len(),
        1,
        "busy is retryable and must retain the durable permission action"
    );
    let agent_events = app.state.runtime_outbox.drain_agent_intel();
    assert!(agent_events.iter().any(|event| matches!(
        event,
        crate::AgentIntelEvent::RemotePermissionDecisionState {
            tool_use_id,
            phase: crate::RemotePermissionDecisionPhase::Sending,
            status: Some(crate::host_protocol::RelayActionStatus::Busy),
            message: Some(message),
            ..
        } if tool_use_id == "tool-use-42" && message.contains("queued for retry")
    )));
    assert!(!agent_events.iter().any(|event| matches!(
        event,
        crate::AgentIntelEvent::PendingPermissionsSnapshot { requests, .. }
            if requests.iter().any(|request|
                request.tool_use_id == "tool-use-42"
                    && request.request_generation == 7
                    && request.decision_phase
                        == crate::PendingPermissionDecisionPhase::Actionable)
    )));
    assert_eq!(
        app.state
            .agent_intel
            .permission_decisions
            .snapshot()
            .requests[0]
            .decision_phase,
        crate::PendingPermissionDecisionPhase::Sending
    );
    assert!(
        app.state
            .runtime_outbox
            .drain_sessions()
            .into_iter()
            .any(|event| matches!(
                event,
                crate::host_protocol::SessionEvent::ActionResult {
                    action_id,
                    status: crate::host_protocol::RelayActionStatus::Busy,
                    ..
                } if action_id == "tool-use-42"
            ))
    );
    crate::runtime::remote_sessions::replay_remote_permission_actions(&mut app, session_id);
    std::assert_matches!(
        tokio::time::timeout(std::time::Duration::from_secs(1), command_rx.recv()).await,
        Ok(Some(kodosi_backend_client::session_relay::SessionRelayCommand::PermissionDecision {
            action_id: replayed_action_id,
            request_id,
            request_generation: 7,
            ..
        })) if replayed_action_id == action_id && request_id == "tool-use-42"
    );
    crate::runtime::remote_sessions::replay_remote_permission_actions(&mut app, session_id);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), command_rx.recv())
            .await
            .is_err(),
        "maintenance must not duplicate the replay while it is in flight"
    );

    app.handle_session_event(RuntimeSessionEvent::RemoteActionResult {
        id: session_id,
        action_id,
        request_id: Some("tool-use-42".to_owned()),
        request_generation: Some(7),
        status: kodosi_domain::lifecycle::RemoteActionStatus::Rejected,
        relay_generation,
    });
    assert!(
        app.remote_permission_actions
            .pending_for_incarnation(&account, session_id, incarnation_id)
            .is_empty(),
        "terminal rejection retires the durable permission action"
    );

    let rejected_events = app.state.runtime_outbox.drain_agent_intel();
    assert!(rejected_events.iter().any(|event| matches!(
        event,
        crate::AgentIntelEvent::RemotePermissionDecisionState {
            session_id: result_session_id,
            tool_use_id,
            phase: crate::RemotePermissionDecisionPhase::Failed,
            status: Some(crate::host_protocol::RelayActionStatus::Rejected),
            ..
        } if result_session_id == &session_id.to_string() && tool_use_id == "tool-use-42"
    )));
    assert!(rejected_events.iter().any(|event| matches!(
        event,
        crate::AgentIntelEvent::PendingPermissionsSnapshot { requests, .. }
            if requests.iter().any(|request|
                request.tool_use_id == "tool-use-42"
                    && request.request_generation == 7
                    && request.decision_phase
                        == crate::PendingPermissionDecisionPhase::Actionable)
    )));
    assert_eq!(
        app.state
            .agent_intel
            .permission_decisions
            .snapshot()
            .requests[0]
            .decision_phase,
        crate::PendingPermissionDecisionPhase::Actionable
    );

    assert!(
        app.state
            .runtime_outbox
            .drain_sessions()
            .into_iter()
            .any(|event| matches!(
                event,
                crate::host_protocol::SessionEvent::ActionResult {
                    session_id: result_session_id,
                    action_id,
                    status: crate::host_protocol::RelayActionStatus::Rejected,
                } if result_session_id == session_id.to_string() && action_id == "tool-use-42"
            ))
    );

    assert_eq!(
        crate::runtime::remote_sessions::permission_decision(
            &mut app,
            session_id,
            "tool-use-42",
            7,
            "allow",
        ),
        crate::runtime::SessionRelayCommandDispatchOutcome::Enqueued
    );
    let second_command = tokio::time::timeout(std::time::Duration::from_secs(1), command_rx.recv())
        .await
        .expect("second permission decision should enqueue promptly");
    let accepted_action_id = match second_command {
        Some(kodosi_backend_client::session_relay::SessionRelayCommand::PermissionDecision {
            action_id,
            ..
        }) => action_id,
        other => panic!("expected second permission decision, got {other:?}"),
    };
    app.handle_session_event(RuntimeSessionEvent::RemoteActionResult {
        id: session_id,
        action_id: accepted_action_id.clone(),
        request_id: Some("tool-use-42".to_owned()),
        request_generation: Some(7),
        status: kodosi_domain::lifecycle::RemoteActionStatus::Accepted,
        relay_generation,
    });
    assert_eq!(
        app.remote_permission_actions
            .pending_for_incarnation(&account, session_id, incarnation_id)
            .len(),
        1,
        "owner confirmation remains durable until the terminal pending-state snapshot"
    );
    crate::runtime::remote_sessions::replay_remote_permission_actions(&mut app, session_id);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), command_rx.recv())
            .await
            .is_err(),
        "owner-confirmed permission actions must not replay"
    );
    let agent_events = app.state.runtime_outbox.drain_agent_intel();
    assert!(agent_events.iter().any(|event| matches!(
        event,
        crate::AgentIntelEvent::RemotePermissionDecisionState {
            tool_use_id,
            phase: crate::RemotePermissionDecisionPhase::Sending,
            status: Some(crate::host_protocol::RelayActionStatus::Accepted),
            ..
        } if tool_use_id == "tool-use-42"
    )));
    assert!(agent_events.iter().any(|event| matches!(
        event,
        crate::AgentIntelEvent::PendingPermissionsSnapshot { requests, .. }
            if requests.iter().any(|request|
                request.tool_use_id == "tool-use-42"
                    && request.request_generation == 7
                    && request.decision_phase
                        == crate::PendingPermissionDecisionPhase::Sending)
    )));
    assert!(
        app.state
            .runtime_outbox
            .drain_sessions()
            .into_iter()
            .any(|event| matches!(
                event,
                crate::host_protocol::SessionEvent::ActionResult {
                    action_id,
                    status: crate::host_protocol::RelayActionStatus::Accepted,
                    ..
                } if action_id == "tool-use-42"
            ))
    );

    assert!(
        !app.handle_session_event(RuntimeSessionEvent::RemoteActionResult {
            id: session_id,
            action_id: accepted_action_id,
            request_id: Some("tool-use-42".to_owned()),
            request_generation: Some(7),
            status: kodosi_domain::lifecycle::RemoteActionStatus::Busy,
            relay_generation,
        })
    );
    assert!(app.state.runtime_outbox.drain_agent_intel().is_empty());
    assert!(app.state.runtime_outbox.drain_sessions().is_empty());
    assert_eq!(
        app.state
            .agent_intel
            .permission_decisions
            .snapshot()
            .requests[0]
            .decision_phase,
        crate::PendingPermissionDecisionPhase::Sending
    );
}

#[tokio::test]
async fn remote_pending_snapshot_replaces_registry_emits_projection_and_clears_atomically() {
    let mut app = test_app();
    let session_id = SessionId::new();
    seed_visible_remote_record(&mut app, session_id);
    let incarnation_id = uuid::Uuid::now_v7();
    app.state
        .discovery
        .session_mut(session_id)
        .expect("remote session")
        .incarnation_id = Some(incarnation_id);
    let relay_generation = app.state.remote.session_relays.attach_for_test(
        session_id,
        CancellationToken::new(),
        mpsc::channel(1).0,
        ShareScope::Friends,
        kodosi_backend_client::session_relay::RemoteRelayMode::SharedParticipant,
        ConnectionState::Connected,
        tokio::spawn(std::future::pending::<()>()),
    );
    let request = crate::ActivePendingPermission {
        session_id: session_id.to_string(),
        session_incarnation_id: incarnation_id.to_string(),
        request_generation: 1,
        tool_use_id: "tool-1".to_owned(),
        tool_name: "Bash".to_owned(),
        tool_input: serde_json::json!({"command": "ls"}),
        created_at_ms: 1,
        deadline_at_ms: 2,
        risk: crate::ApprovalRisk::Safe,
        decision_phase: crate::PendingPermissionDecisionPhase::Actionable,
    };
    let snapshot = |generation, requests| crate::PendingPermissionsSnapshot {
        generation,
        requests,
    };
    let apply = |app: &mut Runtime, snapshot: crate::PendingPermissionsSnapshot| {
        app.handle_session_event(RuntimeSessionEvent::RemotePendingPermissionsSnapshot {
            id: session_id,
            incarnation_id,
            generation: snapshot.generation,
            snapshot: serde_json::to_value(snapshot).expect("snapshot JSON"),
            relay_generation,
        });
    };

    apply(&mut app, snapshot(1, vec![request.clone()]));
    assert_eq!(
        app.state
            .agent_intel
            .permission_decisions
            .snapshot()
            .requests
            .len(),
        1
    );
    std::assert_matches!(
        app.state.runtime_outbox.drain_agent_intel().last(),
        Some(crate::AgentIntelEvent::PendingPermissionsSnapshot { requests, .. })
            if requests.len() == 1
    );

    let mut mismatched_request = request;
    mismatched_request.session_id = SessionId::new().to_string();
    apply(&mut app, snapshot(2, vec![mismatched_request]));
    assert_eq!(
        app.state
            .agent_intel
            .permission_decisions
            .snapshot()
            .requests
            .len(),
        1,
        "one mismatched entry must reject the entire snapshot"
    );
    assert!(app.state.runtime_outbox.drain_agent_intel().is_empty());

    apply(&mut app, snapshot(3, Vec::new()));
    assert!(
        app.state
            .agent_intel
            .permission_decisions
            .snapshot()
            .requests
            .is_empty()
    );
    apply(&mut app, snapshot(2, Vec::new()));
    assert!(
        app.state
            .agent_intel
            .permission_decisions
            .snapshot()
            .requests
            .is_empty()
    );

    app.handle_session_event(RuntimeSessionEvent::RemotePendingPermissionsSnapshot {
        id: session_id,
        incarnation_id: uuid::Uuid::now_v7(),
        generation: 4,
        snapshot: serde_json::to_value(snapshot(4, Vec::new())).expect("snapshot JSON"),
        relay_generation,
    });
    assert!(
        app.state
            .agent_intel
            .permission_decisions
            .snapshot()
            .requests
            .is_empty()
    );
}

fn pending_remote_request(
    session_id: SessionId,
    incarnation_id: uuid::Uuid,
    tool_use_id: &str,
) -> crate::ActivePendingPermission {
    crate::ActivePendingPermission {
        session_id: session_id.to_string(),
        session_incarnation_id: incarnation_id.to_string(),
        request_generation: 1,
        tool_use_id: tool_use_id.to_owned(),
        tool_name: "Bash".to_owned(),
        tool_input: serde_json::json!({"command": "ls"}),
        created_at_ms: 1,
        deadline_at_ms: 2,
        risk: crate::ApprovalRisk::Safe,
        decision_phase: crate::PendingPermissionDecisionPhase::Actionable,
    }
}

fn install_remote_pending(
    app: &mut Runtime,
    session_id: SessionId,
    incarnation_id: uuid::Uuid,
    tool_use_id: &str,
) -> u64 {
    seed_visible_remote_record(app, session_id);
    app.state
        .discovery
        .session_mut(session_id)
        .expect("remote session")
        .incarnation_id = Some(incarnation_id);
    let relay_generation = app.state.remote.session_relays.attach_for_test(
        session_id,
        CancellationToken::new(),
        mpsc::channel(1).0,
        ShareScope::Friends,
        kodosi_backend_client::session_relay::RemoteRelayMode::SharedParticipant,
        ConnectionState::Connected,
        tokio::spawn(std::future::pending::<()>()),
    );
    app.handle_session_event(RuntimeSessionEvent::RemotePendingPermissionsSnapshot {
        id: session_id,
        incarnation_id,
        generation: 1,
        snapshot: serde_json::to_value(crate::PendingPermissionsSnapshot {
            generation: 1,
            requests: vec![pending_remote_request(
                session_id,
                incarnation_id,
                tool_use_id,
            )],
        })
        .expect("snapshot JSON"),
        relay_generation,
    });
    relay_generation
}

fn admit_remote_permission_action(
    app: &mut Runtime,
    account_user_id: &str,
    session_id: SessionId,
    incarnation_id: uuid::Uuid,
    action_id: &str,
) {
    app.remote_permission_actions
        .admit_or_existing(crate::runtime::permission_actions::RemotePermissionAction {
            account_user_id: account_user_id.to_owned(),
            session_id: session_id.to_string(),
            incarnation_id,
            action_id: uuid::Uuid::now_v7().to_string(),
            request_id: action_id.to_owned(),
            request_generation: 1,
            decision: "allow".to_owned(),
        })
        .expect("remote permission action");
}

fn assert_remote_pending_cleanup(event: impl FnOnce(SessionId, u64) -> RuntimeSessionEvent) {
    let mut app = test_app();
    let account = authenticate_test_app(&mut app).account_user_id;
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let relay_generation = install_remote_pending(&mut app, session_id, incarnation_id, "current");
    admit_remote_permission_action(
        &mut app,
        &account,
        session_id,
        incarnation_id,
        "current-action",
    );

    let previous_incarnation = uuid::Uuid::now_v7();
    assert!(
        app.state
            .agent_intel
            .permission_decisions
            .replace_remote_snapshot(
                session_id,
                previous_incarnation,
                crate::PendingPermissionsSnapshot {
                    generation: 1,
                    requests: vec![pending_remote_request(
                        session_id,
                        previous_incarnation,
                        "previous",
                    )],
                },
            )
            .changed()
    );
    admit_remote_permission_action(
        &mut app,
        &account,
        session_id,
        previous_incarnation,
        "previous-action",
    );

    let other_session = SessionId::new();
    let other_incarnation = uuid::Uuid::now_v7();
    assert!(
        app.state
            .agent_intel
            .permission_decisions
            .replace_remote_snapshot(
                other_session,
                other_incarnation,
                crate::PendingPermissionsSnapshot {
                    generation: 1,
                    requests: vec![pending_remote_request(
                        other_session,
                        other_incarnation,
                        "other",
                    )],
                },
            )
            .changed()
    );
    admit_remote_permission_action(
        &mut app,
        &account,
        other_session,
        other_incarnation,
        "other-action",
    );
    let current_key = crate::agent_intel::permission_decision_registry::PendingKey {
        session_id,
        session_incarnation_id: incarnation_id,
        tool_use_id: "current".to_owned(),
    };
    assert!(
        app.state
            .agent_intel
            .permission_decisions
            .mark_sending(&current_key, 1)
            .changed()
    );
    app.state.runtime_outbox.drain_agent_intel();

    app.handle_session_event(event(session_id, relay_generation));
    app.handle_session_event(RuntimeSessionEvent::RemotePendingPermissionsSnapshot {
        id: session_id,
        incarnation_id,
        generation: 2,
        snapshot: serde_json::to_value(crate::PendingPermissionsSnapshot {
            generation: 2,
            requests: vec![pending_remote_request(session_id, incarnation_id, "late")],
        })
        .expect("late snapshot JSON"),
        relay_generation,
    });

    let remaining = app
        .state
        .agent_intel
        .permission_decisions
        .snapshot()
        .requests;
    assert_eq!(remaining.len(), 2);
    assert!(remaining.iter().all(|request| {
        request.session_incarnation_id != incarnation_id.to_string()
            && (request.session_incarnation_id == previous_incarnation.to_string()
                || request.session_id == other_session.to_string())
    }));
    assert!(
        app.remote_permission_actions
            .pending_for_incarnation(&account, session_id, incarnation_id)
            .is_empty()
    );
    assert!(
        !app.state
            .agent_intel
            .permission_decisions
            .mark_actionable(&current_key, 1)
            .changed(),
        "a failed handler must not restore actionability after relay exit"
    );
    assert_eq!(
        app.remote_permission_actions
            .pending_for_incarnation(&account, session_id, previous_incarnation)
            .len(),
        1
    );
    assert_eq!(
        app.remote_permission_actions
            .pending_for_incarnation(&account, other_session, other_incarnation)
            .len(),
        1
    );
    std::assert_matches!(
        app.state.runtime_outbox.drain_agent_intel().last(),
        Some(crate::AgentIntelEvent::PendingPermissionsSnapshot { requests, .. })
            if requests.len() == 2
    );
}

#[tokio::test]
async fn remote_access_revocation_clears_only_current_incarnation_pending_state() {
    assert_remote_pending_cleanup(|id, relay_generation| {
        RuntimeSessionEvent::RemoteAccessRevoked {
            id,
            relay_generation,
        }
    });
}

#[tokio::test]
async fn remote_stopped_clears_only_current_incarnation_pending_state() {
    assert_remote_pending_cleanup(
        |id, relay_generation| RuntimeSessionEvent::RemoteStateChanged {
            id,
            state: SessionState::Stopped,
            relay_generation,
        },
    );
}

#[tokio::test]
async fn remote_failed_clears_only_current_incarnation_pending_state() {
    assert_remote_pending_cleanup(
        |id, relay_generation| RuntimeSessionEvent::RemoteStateChanged {
            id,
            state: SessionState::Failed,
            relay_generation,
        },
    );
}

#[tokio::test]
async fn remote_relay_exit_clears_only_current_incarnation_pending_state() {
    assert_remote_pending_cleanup(|id, relay_generation| {
        RuntimeSessionEvent::RemoteSessionRelayExited {
            id,
            relay_generation,
        }
    });
}

#[tokio::test]
async fn retryable_relay_reconnect_restores_cached_equal_generation_snapshot() {
    let mut app = test_app();
    authenticate_test_app(&mut app);
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let old_relay_generation =
        install_remote_pending(&mut app, session_id, incarnation_id, "current");

    let other_incarnation = uuid::Uuid::now_v7();
    assert!(
        app.state
            .agent_intel
            .permission_decisions
            .replace_remote_snapshot(
                session_id,
                other_incarnation,
                crate::PendingPermissionsSnapshot {
                    generation: 1,
                    requests: vec![pending_remote_request(
                        session_id,
                        other_incarnation,
                        "other-incarnation",
                    )],
                },
            )
            .changed()
    );
    let other_session = SessionId::new();
    let other_session_incarnation = uuid::Uuid::now_v7();
    assert!(
        app.state
            .agent_intel
            .permission_decisions
            .replace_remote_snapshot(
                other_session,
                other_session_incarnation,
                crate::PendingPermissionsSnapshot {
                    generation: 1,
                    requests: vec![pending_remote_request(
                        other_session,
                        other_session_incarnation,
                        "other-session",
                    )],
                },
            )
            .changed()
    );
    app.state.runtime_outbox.drain_agent_intel();

    app.handle_session_event(RuntimeSessionEvent::RemoteSessionRelayExited {
        id: session_id,
        relay_generation: old_relay_generation,
    });
    let after_exit = app
        .state
        .agent_intel
        .permission_decisions
        .snapshot()
        .requests;
    assert_eq!(after_exit.len(), 2);
    assert!(
        after_exit
            .iter()
            .all(|request| { request.session_incarnation_id != incarnation_id.to_string() })
    );
    std::assert_matches!(
        app.state.runtime_outbox.drain_agent_intel().last(),
        Some(crate::AgentIntelEvent::PendingPermissionsSnapshot { requests, .. })
            if requests.len() == 2
    );

    let replacement_relay_generation = app.state.remote.session_relays.attach_for_test(
        session_id,
        CancellationToken::new(),
        mpsc::channel(1).0,
        ShareScope::Friends,
        kodosi_backend_client::session_relay::RemoteRelayMode::SharedParticipant,
        ConnectionState::Connecting,
        tokio::spawn(std::future::pending::<()>()),
    );
    assert!(app.state.set_session_relay_status(
        session_id,
        replacement_relay_generation,
        ConnectionState::Connected,
    ));
    let cached = crate::PendingPermissionsSnapshot {
        generation: 1,
        requests: vec![pending_remote_request(
            session_id,
            incarnation_id,
            "current",
        )],
    };
    app.handle_session_event(RuntimeSessionEvent::RemotePendingPermissionsSnapshot {
        id: session_id,
        incarnation_id,
        generation: cached.generation,
        snapshot: serde_json::to_value(cached).expect("cached snapshot JSON"),
        relay_generation: replacement_relay_generation,
    });
    let restored = app
        .state
        .agent_intel
        .permission_decisions
        .snapshot()
        .requests;
    assert_eq!(restored.len(), 3);
    assert!(restored.iter().any(|request| {
        request.session_incarnation_id == incarnation_id.to_string()
            && request.tool_use_id == "current"
    }));

    app.handle_session_event(RuntimeSessionEvent::RemotePendingPermissionsSnapshot {
        id: session_id,
        incarnation_id,
        generation: 2,
        snapshot: serde_json::to_value(crate::PendingPermissionsSnapshot {
            generation: 2,
            requests: vec![pending_remote_request(
                session_id,
                incarnation_id,
                "late-old-relay",
            )],
        })
        .expect("late snapshot JSON"),
        relay_generation: old_relay_generation,
    });
    let after_late = app
        .state
        .agent_intel
        .permission_decisions
        .snapshot()
        .requests;
    assert_eq!(after_late, restored);
}

#[tokio::test]
async fn remote_offline_exit_clears_only_current_incarnation_pending_state() {
    assert_remote_pending_cleanup(|id, relay_generation| {
        RuntimeSessionEvent::RemoteSessionConnectionChanged {
            id,
            status: ConnectionState::Offline,
            reason: Some("protocol drift".to_owned()),
            relay_generation,
        }
    });
}

#[tokio::test]
async fn local_permission_activation_publishes_and_backend_restart_republishes() {
    let mut app = test_app();
    authenticate_test_app(&mut app);
    let session_id = SessionId::new();
    app.state
        .local
        .sessions
        .insert(claude_owned_summary(session_id, "/tmp/kodosi"));
    let incarnation_id = app
        .state
        .local
        .sessions
        .record(session_id)
        .expect("local session")
        .local_incarnation_id;
    let key = crate::agent_intel::permission_decision_registry::PendingKey {
        session_id,
        session_incarnation_id: incarnation_id,
        tool_use_id: "tool-1".to_owned(),
    };
    let _receiver = app
        .state
        .agent_intel
        .permission_decisions
        .park(key.clone())
        .expect("park");
    assert!(app.state.agent_intel.permission_decisions.stage_metadata(
        &key,
        crate::agent_intel::permission_decision_registry::PendingPermissionMetadata {
            tool_name: "Bash".to_owned(),
            tool_input: serde_json::json!({"command": "ls"}),
            deadline_at_ms: 2,
            risk: crate::ApprovalRisk::Safe,
        },
    ));
    let generation = app
        .state
        .sharing
        .host_relays
        .allocate_generation()
        .expect("relay generation");
    app.state
        .sharing
        .host_relays
        .claim_generation(session_id, generation)
        .expect("claim relay");
    let (pending_tx, mut pending_rx) = tokio::sync::watch::channel(None);
    app.state.sharing.host_relays.attach(
        session_id,
        CancellationToken::new(),
        pending_tx,
        mpsc::channel(1).0,
        mpsc::channel(1).0,
        mpsc::channel(1).0,
        tokio::spawn(std::future::pending()),
    );

    app.handle_session_event(RuntimeSessionEvent::PendingPermissionRequest {
        id: session_id,
        local_incarnation_id: incarnation_id,
        tool_use_id: "tool-1".to_owned(),
    });

    pending_rx.changed().await.expect("snapshot published");
    let published = pending_rx.borrow_and_update().clone().expect("snapshot");
    assert_eq!(published.incarnation_id, incarnation_id);
    let snapshot: crate::PendingPermissionsSnapshot =
        serde_json::from_slice(&published.plaintext).expect("snapshot plaintext");
    assert_eq!(snapshot.generation, published.generation);
    assert_eq!(snapshot.requests.len(), 1);
    assert_eq!(snapshot.requests[0].request_generation, 1);

    let origin = current_host_relay_origin(&app, session_id, generation);
    app.handle_session_event(RuntimeSessionEvent::HostRelayBackendRestarted { origin });
    pending_rx.changed().await.expect("snapshot republished");
    let republished = pending_rx.borrow_and_update().clone().expect("snapshot");
    assert!(republished.generation > published.generation);
    let snapshot: crate::PendingPermissionsSnapshot =
        serde_json::from_slice(&republished.plaintext).expect("snapshot plaintext");
    assert_eq!(snapshot.requests.len(), 1);
}

#[tokio::test]
async fn finished_agent_intel_restart_clears_pending_and_preserves_generations() {
    let mut app = test_app();
    let session_id = SessionId::new();
    insert_owned_session_for_test(
        &mut app,
        claude_owned_summary(session_id, "/workspace"),
        test_session_handle(),
    );
    let incarnation_id = app
        .state
        .local
        .sessions
        .record(session_id)
        .expect("local session")
        .local_incarnation_id;
    let first_key = crate::agent_intel::permission_decision_registry::PendingKey {
        session_id,
        session_incarnation_id: incarnation_id,
        tool_use_id: "tool-before-crash".to_owned(),
    };
    let _receiver = app
        .state
        .agent_intel
        .permission_decisions
        .park(first_key.clone())
        .expect("park pending permission");
    assert!(app.state.agent_intel.permission_decisions.stage_metadata(
        &first_key,
        crate::agent_intel::permission_decision_registry::PendingPermissionMetadata {
            tool_name: "Bash".to_owned(),
            tool_input: serde_json::json!({"command": "ls"}),
            deadline_at_ms: 2,
            risk: crate::ApprovalRisk::Safe,
        },
    ));
    assert_eq!(
        app.state
            .agent_intel
            .permission_decisions
            .activate_local(&first_key, 1),
        Some(1)
    );

    let relay_generation = app
        .state
        .sharing
        .host_relays
        .allocate_generation()
        .expect("relay generation");
    app.state
        .sharing
        .host_relays
        .claim_generation(session_id, relay_generation)
        .expect("claim relay");
    let (pending_tx, mut pending_rx) = tokio::sync::watch::channel(None);
    app.state.sharing.host_relays.attach(
        session_id,
        CancellationToken::new(),
        pending_tx,
        mpsc::channel(1).0,
        mpsc::channel(1).0,
        mpsc::channel(1).0,
        tokio::spawn(std::future::pending()),
    );
    app.publish_pending_permissions(session_id, incarnation_id);
    pending_rx.changed().await.expect("initial snapshot");
    let initial = pending_rx.borrow_and_update().clone().expect("snapshot");

    app.state
        .agent_intel
        .registry
        .attach_finished_for_test(session_id);
    tokio::task::yield_now().await;

    app.run_periodic_maintenance_force().await;

    tokio::time::timeout(std::time::Duration::from_secs(1), pending_rx.changed())
        .await
        .expect("finished task must publish a clear snapshot")
        .expect("pending snapshot channel");
    let cleared = pending_rx
        .borrow_and_update()
        .clone()
        .expect("clear snapshot");
    assert!(cleared.generation > initial.generation);
    let snapshot: crate::PendingPermissionsSnapshot =
        serde_json::from_slice(&cleared.plaintext).expect("snapshot plaintext");
    assert!(snapshot.requests.is_empty());
    assert!(
        app.state
            .agent_intel
            .permission_decisions
            .snapshot()
            .requests
            .is_empty()
    );
    assert!(app.state.agent_intel.registry.active(session_id));

    let mut second_key = first_key;
    second_key.tool_use_id = "tool-after-restart".to_owned();
    let _receiver = app
        .state
        .agent_intel
        .permission_decisions
        .park(second_key.clone())
        .expect("park replacement permission");
    assert!(app.state.agent_intel.permission_decisions.stage_metadata(
        &second_key,
        crate::agent_intel::permission_decision_registry::PendingPermissionMetadata {
            tool_name: "Bash".to_owned(),
            tool_input: serde_json::json!({"command": "pwd"}),
            deadline_at_ms: 3,
            risk: crate::ApprovalRisk::Safe,
        },
    ));
    assert_eq!(
        app.state
            .agent_intel
            .permission_decisions
            .activate_local(&second_key, 2),
        Some(2)
    );
}

#[test]
fn backend_request_id_recovers_permission_result_after_local_correlation_loss() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let session_id = SessionId::new();
    let account = authenticate_test_app(&mut app).account_user_id;
    let action_id = uuid::Uuid::now_v7().to_string();
    let action_incarnation_id = uuid::Uuid::now_v7();
    app.remote_permission_actions
        .admit_or_existing(crate::runtime::permission_actions::RemotePermissionAction {
            account_user_id: account,
            session_id: session_id.to_string(),
            incarnation_id: action_incarnation_id,
            action_id: action_id.clone(),
            request_id: "tool-use-after-restart".to_owned(),
            request_generation: 7,
            decision: "allow".to_owned(),
        })
        .expect("persisted permission action");
    let relay_generation = app
        .state
        .remote
        .session_relays
        .reserve_generation(session_id)
        .expect("test relay generation");

    for request_generation in [None, Some(6)] {
        assert!(
            !app.handle_session_event(RuntimeSessionEvent::RemoteActionResult {
                id: session_id,
                action_id: action_id.clone(),
                request_id: Some("tool-use-after-restart".to_owned()),
                request_generation,
                status: kodosi_domain::lifecycle::RemoteActionStatus::Busy,
                relay_generation,
            })
        );
        assert_eq!(
            app.remote_permission_actions
                .pending_for_incarnation(
                    &app.state.identity.auth.subject_string().expect("account"),
                    session_id,
                    action_incarnation_id,
                )
                .len(),
            1
        );
        assert!(app.state.runtime_outbox.drain_agent_intel().is_empty());
        assert!(app.state.runtime_outbox.drain_sessions().is_empty());
    }

    app.handle_session_event(RuntimeSessionEvent::RemoteActionResult {
        id: session_id,
        action_id,
        request_id: Some("tool-use-after-restart".to_owned()),
        request_generation: Some(7),
        status: kodosi_domain::lifecycle::RemoteActionStatus::Busy,
        relay_generation,
    });

    assert!(
        app.state
            .runtime_outbox
            .drain_agent_intel()
            .into_iter()
            .any(|event| matches!(
                event,
                crate::AgentIntelEvent::RemotePermissionDecisionState {
                    session_id: result_session_id,
                    tool_use_id,
                    phase: crate::RemotePermissionDecisionPhase::Sending,
                    status: Some(crate::host_protocol::RelayActionStatus::Busy),
                    message: Some(message),
                    ..
                } if result_session_id == session_id.to_string()
                    && tool_use_id == "tool-use-after-restart"
                    && message.contains("queued for retry")
            ))
    );
    assert!(
        app.state
            .runtime_outbox
            .drain_sessions()
            .into_iter()
            .any(|event| matches!(
                event,
                crate::host_protocol::SessionEvent::ActionResult {
                    session_id: result_session_id,
                    action_id,
                    status: crate::host_protocol::RelayActionStatus::Busy,
                } if result_session_id == session_id.to_string()
                    && action_id == "tool-use-after-restart"
            ))
    );
}
