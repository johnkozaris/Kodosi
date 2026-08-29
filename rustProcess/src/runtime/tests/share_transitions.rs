use super::{
    authenticate_test_app, claude_owned_summary, insert_owned_session_for_test,
    local_coordinator_origin, test_app, test_session_handle,
};
use crate::runtime::{
    share_transition_worker::ShareTransitionWorkerMode,
    share_transitions::{
        PreparedShareTransition, ShareAudience, ShareTransitionCleanupIdentity,
        ShareTransitionState, ShareTransitionTerminalStatus,
    },
};
use kodosi_domain::{ids::SessionId, permissions::ShareScope};

fn prepared_share_distribution(
    backend_session_id: String,
    backend_incarnation_id: uuid::Uuid,
    previous_generation: u32,
    sender_signing_pkcs8: zeroize::Zeroizing<Vec<u8>>,
) -> crate::runtime::access_effect_worker::PreparedShareKeyDistribution {
    crate::runtime::access_effect_worker::PreparedShareKeyDistribution {
        backend_session_id,
        backend_incarnation_id,
        previous_generation,
        session_key: [7; 32],
        sender_device_id: "device-test".to_owned(),
        owner_user_id: "owner-test".to_owned(),
        sender_signing_pkcs8,
        authorized_devices: Vec::new(),
        authorized_recipients: std::collections::HashMap::new(),
        verified_identities: std::collections::HashMap::new(),
    }
}

fn owned_app() -> (crate::runtime::Runtime, SessionId, uuid::Uuid, String) {
    let mut app = test_app();
    app.backend = kodosi_backend_client::http_client::BackendHttpClient::new(
        &kodosi_backend_client::config::BackendClientConfig {
            api: Some("http://127.0.0.1:9/".to_owned()),
            ..kodosi_backend_client::config::BackendClientConfig::default()
        },
    )
    .expect("backend");
    let origin = authenticate_test_app(&mut app);
    app.set_remote_surfaces_ready_for_test();
    let id = SessionId::new();
    insert_owned_session_for_test(
        &mut app,
        claude_owned_summary(id, "/tmp/kodosi"),
        test_session_handle(),
    );
    let incarnation = app
        .state
        .local
        .sessions
        .record(id)
        .expect("owned session")
        .local_incarnation_id;
    (app, id, incarnation, origin.account_user_id)
}

fn cleanup(app: &crate::runtime::Runtime) -> ShareTransitionCleanupIdentity {
    ShareTransitionCleanupIdentity {
        backend_origin: app
            .backend
            .backend_origin()
            .map_or_else(|| "http://127.0.0.1:9/".to_owned(), ToString::to_string),
        create_idempotency_id: uuid::Uuid::now_v7(),
        end_mutation_id: uuid::Uuid::now_v7(),
        created_at_ms: 1,
    }
}

async fn hanging_request_server(
    expected_prefix: &'static str,
) -> (String, tokio::sync::oneshot::Receiver<()>) {
    use tokio::io::AsyncReadExt as _;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let address = listener.local_addr().expect("address");
    let (accepted_tx, accepted_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept request");
        let mut request = [0_u8; 4096];
        let read = socket.read(&mut request).await.expect("read request");
        assert!(String::from_utf8_lossy(&request[..read]).starts_with(expected_prefix));
        accepted_tx.send(()).ok();
        std::future::pending::<()>().await;
    });
    (format!("http://{address}/"), accepted_rx)
}

async fn hanging_patch_server() -> (String, tokio::sync::oneshot::Receiver<()>) {
    hanging_request_server("PATCH ").await
}

async fn hanging_websocket_server(
    expected_path: String,
) -> (String, tokio::sync::oneshot::Receiver<()>) {
    hanging_websocket_server_for_paths(vec![expected_path]).await
}

async fn hanging_websocket_server_for_paths(
    expected_paths: Vec<String>,
) -> (String, tokio::sync::oneshot::Receiver<()>) {
    use std::collections::HashSet;
    use tokio::io::AsyncReadExt as _;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let address = listener.local_addr().expect("address");
    let (accepted_tx, accepted_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let mut expected = expected_paths.into_iter().collect::<HashSet<_>>();
        while !expected.is_empty() {
            let (mut socket, _) = listener.accept().await.expect("accept websocket");
            let mut request = [0_u8; 4096];
            let read = socket
                .read(&mut request)
                .await
                .expect("read websocket request");
            let path = String::from_utf8_lossy(&request[..read])
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .expect("websocket request path")
                .to_owned();
            assert!(expected.remove(&path), "unexpected websocket path {path}");
            tokio::spawn(async move {
                let _socket = socket;
                std::future::pending::<()>().await;
            });
        }
        accepted_tx.send(()).ok();
    });
    (format!("ws://{address}/"), accepted_rx)
}

async fn accepted_websocket_server(
    session_id: SessionId,
    incarnation_id: uuid::Uuid,
) -> (
    String,
    tokio::sync::oneshot::Receiver<()>,
    tokio::sync::oneshot::Receiver<()>,
) {
    use futures_util::{SinkExt as _, StreamExt as _};
    use tokio_tungstenite::tungstenite::Message;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let address = listener.local_addr().expect("address");
    let (primed_tx, primed_rx) = tokio::sync::oneshot::channel();
    let (closed_tx, closed_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let (socket, _) = listener.accept().await.expect("accept websocket");
        let mut stream = tokio_tungstenite::accept_async(socket)
            .await
            .expect("websocket handshake");
        let device_id = "relay-owner";
        let connection_id = "relay-test-connection";
        let challenge = [7_u8; 32];
        stream
            .send(Message::Text(
                serde_json::json!({
                    "type": "device.proofChallenge",
                    "connectionId": connection_id,
                    "purpose": "host",
                    "sessionId": session_id.to_string(),
                    "challenge": base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        challenge,
                    ),
                })
                .to_string()
                .into(),
            ))
            .await
            .expect("device proof challenge");
        let proof = stream
            .next()
            .await
            .expect("device proof")
            .expect("device proof frame");
        let Message::Text(proof) = proof else {
            panic!("device proof must be text")
        };
        let proof: serde_json::Value = serde_json::from_str(&proof).expect("device proof json");
        assert_eq!(proof["type"], "device.proof");
        assert!(
            proof["deviceId"]
                .as_str()
                .is_some_and(|value| value.starts_with(device_id))
        );
        assert!(!proof["signature"].as_str().unwrap_or_default().is_empty());
        assert_eq!(proof["sessionId"], session_id.to_string());
        assert_eq!(proof["expectedIncarnationId"], incarnation_id.to_string());

        let hello = stream
            .next()
            .await
            .expect("host hello")
            .expect("host hello frame");
        let Message::Text(hello) = hello else {
            panic!("host hello must be text")
        };
        let hello: serde_json::Value = serde_json::from_str(&hello).expect("host hello json");
        assert_eq!(hello["type"], "host.hello");
        assert_eq!(hello["sessionId"], session_id.to_string());
        assert_eq!(hello["expectedIncarnationId"], incarnation_id.to_string());
        stream
            .send(Message::Text(
                serde_json::json!({
                    "type": "host.accepted",
                    "sessionId": session_id.to_string(),
                    "relayEpoch": "relay-test-epoch",
                    "incarnationId": incarnation_id.to_string(),
                    "incarnationGeneration": 1,
                    "relayProtocolVersion": 10,
                })
                .to_string()
                .into(),
            ))
            .await
            .expect("host accepted");
        let rotation = stream
            .next()
            .await
            .expect("key rotation")
            .expect("key rotation frame");
        assert!(matches!(rotation, Message::Text(_)));
        primed_tx.send(()).ok();
        while let Some(message) = stream.next().await {
            match message {
                Ok(Message::Close(_)) | Err(_) => break,
                Ok(_) => {}
            }
        }
        closed_tx.send(()).ok();
    });
    (format!("ws://{address}/"), primed_rx, closed_rx)
}

fn configure_relay_test_auth(app: &mut crate::runtime::Runtime) {
    use crate::identity_core::token_store::TokenStore as _;

    app.state.identity.device_flow = crate::runtime::identity::DeviceFlowRuntime::new(
        crate::identity_core::device_flow::DeviceFlowClient::new(&crate::config::AuthConfig {
            issuer: Some("https://auth.invalid/".to_owned()),
            ..crate::config::AuthConfig::default()
        })
        .expect("configured auth client"),
    );
    app.token_store
        .save(
            crate::runtime::DEFAULT_TOKEN_SUBJECT,
            &crate::identity_core::token_store::StoredTokens::new(
                "relay-test-token".to_owned(),
                None,
                time::OffsetDateTime::now_utc() + time::Duration::hours(1),
            ),
        )
        .expect("store relay token");
}

fn relay_pending_transition(
    app: &mut crate::runtime::Runtime,
    id: SessionId,
    incarnation: uuid::Uuid,
    account: &str,
    host_device_id: &str,
) -> PreparedShareTransition {
    let backend_incarnation_id = install_shared_authority(app, id, account);
    assert!(
        app.state
            .sharing
            .shared_sessions
            .set_host_device_for_backend(&id.to_string(), host_device_id.to_owned())
    );
    let relay_generation = app
        .state
        .sharing
        .host_relays
        .allocate_generation()
        .expect("relay generation");
    let cleanup_record = app
        .collaboration_teardown
        .list_for_account(app.backend.backend_origin().expect("origin"), account)
        .expect("cleanup obligations")
        .into_iter()
        .find(|record| record.backend_session_id == id.to_string())
        .expect("cleanup obligation");
    let cleanup = ShareTransitionCleanupIdentity {
        backend_origin: cleanup_record.backend_origin.to_string(),
        create_idempotency_id: cleanup_record.create_idempotency_id,
        end_mutation_id: cleanup_record.end_mutation_id,
        created_at_ms: cleanup_record.created_at_ms,
    };
    let mut prepared = PreparedShareTransition::new(
        uuid::Uuid::now_v7(),
        1,
        account.to_owned(),
        app.state.identity.account_epoch().value(),
        id,
        incarnation,
        ShareAudience {
            scope: ShareScope::MyDevices,
            room_id: None,
        },
        ShareAudience {
            scope: ShareScope::MyDevices,
            room_id: None,
        },
        id.to_string(),
        Some(backend_incarnation_id),
        Some(1),
        Some(cleanup),
    )
    .expect("transition");
    prepared
        .bind_claimed_key_generation(1)
        .expect("key binding");
    prepared
        .bind_relay_generation(relay_generation)
        .expect("relay binding");
    prepared.state = ShareTransitionState::RelayPending {
        key_generation: 1,
        relay_generation,
    };
    app.share_transitions
        .put(prepared.clone())
        .expect("persist relay phase");
    app.share_transition_in_flight
        .insert(id, prepared.transition_id);
    app.install_share_transition_deadline_for_test(prepared.transition_id);
    prepared
}

fn maintenance_relay_owner(
    account: &str,
    id: SessionId,
    incarnation: uuid::Uuid,
    backend_incarnation_id: uuid::Uuid,
) -> crate::runtime::relay_prepare_worker::RelayPrepareOwner {
    crate::runtime::relay_prepare_worker::RelayPrepareOwner::Maintenance {
        account_user_id: account.to_owned(),
        runtime_session_id: id,
        runtime_incarnation_id: incarnation,
        backend_session_id: id.to_string(),
        backend_incarnation_id,
    }
}

fn attempting_access_mutation(
    app: &mut crate::runtime::Runtime,
    id: SessionId,
    incarnation: uuid::Uuid,
    account: &str,
    backend_incarnation_id: uuid::Uuid,
) -> crate::runtime::access_mutations::PreparedSessionAccessMutation {
    let mut access = crate::runtime::access_mutations::PreparedSessionAccessMutation::new(
        uuid::Uuid::now_v7(),
        account.to_owned(),
        app.state.identity.account_epoch().value(),
        id,
        incarnation,
        id.to_string(),
        backend_incarnation_id,
        crate::runtime::access_mutations::SessionAccessMutationTarget::Revoke {
            actor_user_id: uuid::Uuid::now_v7(),
        },
    )
    .expect("access mutation");
    access.state = crate::runtime::access_mutations::PreparedSessionAccessMutationState::Attempting;
    app.access_mutations
        .put(access.clone())
        .expect("persist access mutation");
    access
}

fn install_shared_authority(
    app: &mut crate::runtime::Runtime,
    id: SessionId,
    account: &str,
) -> uuid::Uuid {
    let backend_incarnation_id = uuid::Uuid::now_v7();
    app.state.sharing.shared_sessions.insert(
        id,
        crate::sharing::shared_session_registry::SharedSessionState::new(
            id.to_string(),
            backend_incarnation_id,
            "owner-secret".to_owned(),
            ShareScope::MyDevices,
            None,
            Some([7; 32]),
            Some(1),
        ),
    );
    app.state
        .apply_session_scope_locally(id, ShareScope::MyDevices);
    let create_id = uuid::Uuid::now_v7();
    let end_id = uuid::Uuid::now_v7();
    app.collaboration_teardown
        .provision(
            app.backend.backend_origin().expect("origin"),
            account,
            &id.to_string(),
            create_id,
            end_id,
            1,
        )
        .expect("cleanup obligation");
    app.collaboration_teardown
        .bind_incarnation(create_id, &id.to_string(), backend_incarnation_id)
        .expect("cleanup binding");
    backend_incarnation_id
}

async fn assert_snapshot_responsive(app: &mut crate::runtime::Runtime) {
    tokio::time::timeout(
        std::time::Duration::from_millis(100),
        crate::host_protocol::sessions_command::apply_session_command(
            app,
            crate::host_protocol::SessionCommand::SnapshotRefresh,
        ),
    )
    .await
    .expect("local snapshot must remain responsive")
    .expect("snapshot command");
}

#[tokio::test]
async fn expired_transition_deadline_aborts_work_and_enters_cleanup() {
    let (mut app, id, incarnation, account) = owned_app();
    let prepared = app
        .begin_share_transition(
            uuid::Uuid::now_v7(),
            id,
            incarnation,
            ShareScope::MyDevices,
            None,
        )
        .expect("transition admission");
    app.set_share_transition_deadline_for_test(
        prepared.transition_id,
        std::time::Instant::now() - std::time::Duration::from_millis(1),
    );

    app.expire_share_transition_deadlines();

    assert!(
        !app.share_transition_tasks
            .contains_key(&prepared.transition_id)
    );
    assert!(!app.share_transition_in_flight.contains_key(&id));
    assert!(app.state.sharing.shared_sessions.get(id).is_none());
    assert!(matches!(
        app.share_transitions
            .get(&account, prepared.transition_id)
            .expect("ledger")
            .expect("transition")
            .state,
        ShareTransitionState::CleanupPending(ref terminal)
            if terminal.status == ShareTransitionTerminalStatus::Rejected
                && terminal.message.as_deref().is_some_and(|message| message.contains("deadline"))
    ));
}

#[tokio::test]
async fn cleanup_settlement_write_failure_retries_without_restart() {
    let (mut app, id, incarnation, account) = owned_app();
    let prepared = app
        .begin_share_transition(
            uuid::Uuid::now_v7(),
            id,
            incarnation,
            ShareScope::MyDevices,
            None,
        )
        .expect("transition admission");
    let original_path = app
        .share_transitions
        .replace_path_for_test(std::path::PathBuf::from(
            "/missing-parent/share-transitions.json",
        ));

    app.cancel_share_transition_for_session(id, "cancel with failed persistence");
    assert!(app.share_transition_in_flight.contains_key(&id));
    assert!(
        app.share_transition_settlement_retry
            .contains_key(&prepared.transition_id)
    );
    app.share_transitions.replace_path_for_test(original_path);
    app.retry_share_transition_settlements();

    assert!(!app.share_transition_in_flight.contains_key(&id));
    assert!(app.share_transition_settlement_retry.is_empty());
    assert!(matches!(
        app.share_transitions
            .get(&account, prepared.transition_id)
            .expect("ledger")
            .expect("transition")
            .state,
        ShareTransitionState::CleanupPending(ref terminal)
            if terminal.status == ShareTransitionTerminalStatus::Cancelled
    ));
}

#[tokio::test]
async fn applied_terminal_write_failure_retries_before_emitting_success() {
    let (mut app, id, incarnation, account) = owned_app();
    let mut prepared = PreparedShareTransition::new(
        uuid::Uuid::now_v7(),
        1,
        account.clone(),
        app.state.identity.account_epoch().value(),
        id,
        incarnation,
        ShareAudience {
            scope: ShareScope::JustMe,
            room_id: None,
        },
        ShareAudience {
            scope: ShareScope::MyDevices,
            room_id: None,
        },
        id.to_string(),
        None,
        None,
        Some(cleanup(&app)),
    )
    .expect("transition");
    prepared.state = ShareTransitionState::CommitReady;
    app.share_transitions
        .put(prepared.clone())
        .expect("persist commit-ready transition");
    app.share_transition_in_flight
        .insert(id, prepared.transition_id);
    app.install_share_transition_deadline_for_test(prepared.transition_id);
    let original_path = app
        .share_transitions
        .replace_path_for_test(std::path::PathBuf::from(
            "/missing-parent/share-transitions.json",
        ));

    app.finish_applied_share_transition_for_test(&prepared)
        .expect_err("first terminal write should fail");

    assert!(app.share_transition_in_flight.contains_key(&id));
    assert!(
        app.share_transition_settlement_retry
            .contains_key(&prepared.transition_id)
    );
    assert!(
        app.state
            .runtime_outbox
            .drain_sessions()
            .into_iter()
            .all(|event| !matches!(event, crate::SessionEvent::ScopeChanged { .. })),
        "success cannot be visible before its terminal receipt is durable"
    );

    app.share_transitions.replace_path_for_test(original_path);
    app.retry_share_transition_settlements();

    assert!(!app.share_transition_in_flight.contains_key(&id));
    assert!(app.share_transition_settlement_retry.is_empty());
    assert!(matches!(
        app.share_transitions
            .get(&account, prepared.transition_id)
            .expect("ledger")
            .expect("transition")
            .state,
        ShareTransitionState::Terminal(ref terminal)
            if terminal.status == ShareTransitionTerminalStatus::Applied
    ));
    let events = app.state.runtime_outbox.drain_sessions();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, crate::SessionEvent::ScopeChanged { .. }))
            .count(),
        1
    );
}

#[tokio::test]
async fn duplicate_cleanup_pending_request_replays_stored_error() {
    let (mut app, id, incarnation, account) = owned_app();
    let request_id = uuid::Uuid::now_v7();
    let prepared = app
        .begin_share_transition(request_id, id, incarnation, ShareScope::MyDevices, None)
        .expect("transition admission");
    app.cancel_share_transition_for_session(id, "stored cancellation");
    assert!(matches!(
        app.share_transitions
            .get(&account, request_id)
            .expect("ledger")
            .expect("transition")
            .state,
        ShareTransitionState::CleanupPending(_)
    ));

    app.begin_share_transition(request_id, id, incarnation, ShareScope::MyDevices, None)
        .expect("duplicate should replay");
    let events = app.state.runtime_outbox.drain_sessions();
    assert!(events.iter().any(|event| matches!(
        event,
        crate::SessionEvent::Error {
            request_id: Some(replayed),
            message,
            ..
        } if replayed == &prepared.transition_id.to_string() && message.contains("stored cancellation")
    )));
}

#[tokio::test]
async fn unshare_receipt_is_durable_before_access_work_is_preempted() {
    let (mut app, id, incarnation, account) = owned_app();
    let backend_incarnation_id = install_shared_authority(&mut app, id, &account);
    let mut access = crate::runtime::access_mutations::PreparedSessionAccessMutation::new(
        uuid::Uuid::now_v7(),
        account,
        app.state.identity.account_epoch().value(),
        id,
        incarnation,
        id.to_string(),
        backend_incarnation_id,
        crate::runtime::access_mutations::SessionAccessMutationTarget::Revoke {
            actor_user_id: uuid::Uuid::now_v7(),
        },
    )
    .expect("access mutation");
    access.state = crate::runtime::access_mutations::PreparedSessionAccessMutationState::Attempting;
    let access_id = access.mutation_id;
    app.access_mutations.put(access).expect("persist access");
    let original_path = app
        .share_transitions
        .replace_path_for_test(std::path::PathBuf::from(
            "/missing-parent/share-transitions.json",
        ));

    app.begin_share_transition(
        uuid::Uuid::now_v7(),
        id,
        incarnation,
        ShareScope::JustMe,
        None,
    )
    .expect_err("unshare admission should fail before preemption");
    app.share_transitions.replace_path_for_test(original_path);

    assert!(matches!(
        app.access_mutations
            .get(
                app.state
                    .identity
                    .auth
                    .subject_string()
                    .as_deref()
                    .expect("account"),
                access_id,
            )
            .expect("ledger")
            .expect("access")
            .state,
        crate::runtime::access_mutations::PreparedSessionAccessMutationState::Attempting
    ));
    assert!(app.state.sharing.shared_sessions.get(id).is_some());
}

#[tokio::test]
async fn unshare_persists_owner_only_scope_immediately() {
    let (mut app, id, incarnation, account) = owned_app();
    install_shared_authority(&mut app, id, &account);
    let catalog_dir = tempfile::tempdir().expect("catalog dir");
    app.local_catalog =
        crate::runtime::local_control::LocalSessionCatalog::at(catalog_dir.path().to_path_buf())
            .expect("catalog");
    app.local_catalog
        .persist_record(app.state.local.sessions.record(id).expect("session"))
        .expect("initial descriptor");

    app.begin_share_transition(
        uuid::Uuid::now_v7(),
        id,
        incarnation,
        ShareScope::JustMe,
        None,
    )
    .expect("unshare");
    let discovered = app.local_catalog.discover(None, Vec::new());
    let restored = discovered
        .into_iter()
        .find(|session| session.summary.id == id)
        .expect("restored descriptor");
    assert_eq!(restored.summary.scope, ShareScope::JustMe);
}

#[tokio::test]
async fn hanging_maintenance_relay_upgrade_does_not_block_actor_work() {
    let (mut app, id, incarnation, account) = owned_app();
    configure_relay_test_auth(&mut app);
    let backend_incarnation_id = install_shared_authority(&mut app, id, &account);
    let signing = crate::identity_core::device_keys::generate_device_keys_for_test("relay-owner")
        .expect("signing key");
    assert!(
        app.state
            .sharing
            .shared_sessions
            .set_host_device_for_backend(&id.to_string(), signing.device_id.clone())
    );
    let (host_relay, accepted) = hanging_websocket_server(format!("/hosts/{id}")).await;
    app.host_ws = kodosi_backend_client::host_ws::HostWsClient::new(Some(&host_relay))
        .expect("host websocket client");

    app.start_relay_prepare_worker(
        maintenance_relay_owner(&account, id, incarnation, backend_incarnation_id),
        Some(1),
        zeroize::Zeroizing::new(signing.signing_pkcs8_bytes().to_vec()),
    )
    .expect("maintenance relay preparation");
    tokio::time::timeout(std::time::Duration::from_secs(1), accepted)
        .await
        .expect("websocket upgrade should dispatch")
        .expect("server should observe websocket upgrade");

    assert_snapshot_responsive(&mut app).await;
    assert_eq!(
        app.relay_prepare_owners.get(&id),
        Some(&crate::runtime::relay_prepare_worker::RelayPrepareOwnerKind::Maintenance)
    );
    app.cancel_relay_prepare_for_session(id);
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while app.terminal_hub.connection_count(id) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("maintenance provisional bridge should unregister");
}

#[tokio::test]
async fn periodic_maintenance_reaps_finished_relay_without_notify_delivery() {
    let (mut app, id, incarnation, account) = owned_app();
    configure_relay_test_auth(&mut app);
    let backend_incarnation_id = install_shared_authority(&mut app, id, &account);
    let signing = crate::identity_core::device_keys::generate_device_keys_for_test("relay-owner")
        .expect("signing key");
    assert!(
        app.state
            .sharing
            .shared_sessions
            .set_host_device_for_backend(&id.to_string(), signing.device_id.clone())
    );
    let (host_relay, primed, _closed) = accepted_websocket_server(id, backend_incarnation_id).await;
    app.host_ws = kodosi_backend_client::host_ws::HostWsClient::new(Some(&host_relay))
        .expect("host websocket client");
    app.start_relay_prepare_worker(
        maintenance_relay_owner(&account, id, incarnation, backend_incarnation_id),
        Some(1),
        zeroize::Zeroizing::new(signing.signing_pkcs8_bytes().to_vec()),
    )
    .expect("maintenance relay preparation");
    tokio::time::timeout(std::time::Duration::from_secs(1), primed)
        .await
        .expect("relay should prime")
        .expect("server should observe key rotation");
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while !app
            .relay_prepare_tasks
            .get(&id)
            .is_some_and(tokio::task::JoinHandle::is_finished)
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("relay preparation should finish");
    app.backend_compatibility_verified = false;
    app.last_auth_refresh_check = Some(std::time::Instant::now());

    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        app.run_periodic_maintenance_force(),
    )
    .await
    .expect("periodic sweep should remain bounded");

    assert!(app.state.sharing.host_relays.active(id));
    assert!(!app.relay_prepare_tasks.contains_key(&id));
    assert!(!app.relay_prepare_owners.contains_key(&id));
}

#[tokio::test]
async fn hosted_key_invalidation_preempts_maintenance_relay_preparation() {
    let (mut app, id, incarnation, account) = owned_app();
    configure_relay_test_auth(&mut app);
    let backend_incarnation_id = install_shared_authority(&mut app, id, &account);
    let signing = crate::identity_core::device_keys::generate_device_keys_for_test("relay-owner")
        .expect("signing key");
    assert!(
        app.state
            .sharing
            .shared_sessions
            .set_host_device_for_backend(&id.to_string(), signing.device_id.clone())
    );
    let (host_relay, accepted) = hanging_websocket_server(format!("/hosts/{id}")).await;
    app.host_ws = kodosi_backend_client::host_ws::HostWsClient::new(Some(&host_relay))
        .expect("host websocket client");
    app.start_relay_prepare_worker(
        maintenance_relay_owner(&account, id, incarnation, backend_incarnation_id),
        Some(1),
        zeroize::Zeroizing::new(signing.signing_pkcs8_bytes().to_vec()),
    )
    .expect("maintenance relay preparation");
    tokio::time::timeout(std::time::Duration::from_secs(1), accepted)
        .await
        .expect("websocket upgrade should dispatch")
        .expect("server should observe websocket upgrade");
    assert_eq!(app.terminal_hub.connection_count(id), 1);

    app.invalidate_hosted_session_keys();

    assert!(!app.relay_prepare_tasks.contains_key(&id));
    assert!(!app.relay_prepare_owners.contains_key(&id));
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while app.terminal_hub.connection_count(id) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("invalidated provisional bridge should unregister");
    let shared = app
        .state
        .sharing
        .shared_sessions
        .get(id)
        .expect("shared session remains for redistribution");
    assert!(shared.session_key().is_none());
    assert!(shared.session_key_generation().is_none());
}

#[tokio::test]
async fn remote_share_admission_preempts_maintenance_relay_preparation() {
    let (api, patch_accepted) = hanging_patch_server().await;
    let (mut app, id, incarnation, account) = owned_app();
    configure_relay_test_auth(&mut app);
    app.backend = kodosi_backend_client::http_client::BackendHttpClient::new(
        &kodosi_backend_client::config::BackendClientConfig {
            api: Some(api),
            ..kodosi_backend_client::config::BackendClientConfig::default()
        },
    )
    .expect("backend");
    let backend_incarnation_id = install_shared_authority(&mut app, id, &account);
    let signing = crate::identity_core::device_keys::generate_device_keys_for_test("relay-owner")
        .expect("signing key");
    assert!(
        app.state
            .sharing
            .shared_sessions
            .set_host_device_for_backend(&id.to_string(), signing.device_id.clone())
    );
    let (host_relay, relay_accepted) = hanging_websocket_server(format!("/hosts/{id}")).await;
    app.host_ws = kodosi_backend_client::host_ws::HostWsClient::new(Some(&host_relay))
        .expect("host websocket client");
    app.start_relay_prepare_worker(
        maintenance_relay_owner(&account, id, incarnation, backend_incarnation_id),
        Some(1),
        zeroize::Zeroizing::new(signing.signing_pkcs8_bytes().to_vec()),
    )
    .expect("maintenance relay preparation");
    tokio::time::timeout(std::time::Duration::from_secs(1), relay_accepted)
        .await
        .expect("relay websocket upgrade should dispatch")
        .expect("server should observe relay websocket upgrade");
    app.state.available_rooms.push(crate::RoomListEntry {
        id: "room-a".to_owned(),
        name: "Room A".to_owned(),
        slug: "room-a".to_owned(),
    });

    app.begin_share_transition(
        uuid::Uuid::now_v7(),
        id,
        incarnation,
        ShareScope::Room,
        Some("room-a"),
    )
    .expect("share transition should admit");
    tokio::time::timeout(std::time::Duration::from_secs(1), patch_accepted)
        .await
        .expect("scope PATCH should dispatch")
        .expect("server should observe scope PATCH");

    assert!(!app.relay_prepare_tasks.contains_key(&id));
    assert!(!app.relay_prepare_owners.contains_key(&id));
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while app.terminal_hub.connection_count(id) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("preempted maintenance bridge should unregister");
    app.cancel_share_transition_for_session(id, "test cleanup");
}

#[tokio::test]
async fn hanging_relay_upgrade_does_not_block_actor_and_cancellation_drops_bridge() {
    let (mut app, id, incarnation, account) = owned_app();
    configure_relay_test_auth(&mut app);
    let prepared = relay_pending_transition(&mut app, id, incarnation, &account, "relay-owner");
    let signing = crate::identity_core::device_keys::generate_device_keys_for_test("relay-owner")
        .expect("signing key");
    let (host_relay, accepted) = hanging_websocket_server(format!("/hosts/{id}")).await;
    app.host_ws = kodosi_backend_client::host_ws::HostWsClient::new(Some(&host_relay))
        .expect("host websocket client");

    app.start_relay_prepare_worker(
        crate::runtime::relay_prepare_worker::RelayPrepareOwner::Share {
            prepared: prepared.clone(),
        },
        Some(1),
        zeroize::Zeroizing::new(signing.signing_pkcs8_bytes().to_vec()),
    )
    .expect("relay preparation");
    tokio::time::timeout(std::time::Duration::from_secs(1), accepted)
        .await
        .expect("websocket upgrade should dispatch")
        .expect("server should observe websocket upgrade");
    assert_eq!(app.terminal_hub.connection_count(id), 1);
    assert_snapshot_responsive(&mut app).await;

    app.cancel_share_transition_for_session(id, "test relay cancellation");
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while app.terminal_hub.connection_count(id) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("provisional bridge should unregister");
    assert!(!app.relay_prepare_tasks.contains_key(&id));
    assert!(!app.relay_prepare_owners.contains_key(&id));
    assert!(matches!(
        app.share_transitions
            .get(&account, prepared.transition_id)
            .expect("ledger")
            .expect("transition")
            .state,
        ShareTransitionState::CleanupPending(ref terminal)
            if terminal.status == ShareTransitionTerminalStatus::Cancelled
    ));
}

#[tokio::test]
async fn stale_successful_relay_completion_drops_every_provisional_resource() {
    let (mut app, id, incarnation, account) = owned_app();
    configure_relay_test_auth(&mut app);
    let prepared = relay_pending_transition(&mut app, id, incarnation, &account, "relay-owner");
    let backend_incarnation_id = prepared
        .backend_incarnation_id
        .expect("backend incarnation");
    let signing = crate::identity_core::device_keys::generate_device_keys_for_test("relay-owner")
        .expect("signing key");
    let (host_relay, primed, closed) = accepted_websocket_server(id, backend_incarnation_id).await;
    app.host_ws = kodosi_backend_client::host_ws::HostWsClient::new(Some(&host_relay))
        .expect("host websocket client");
    app.start_relay_prepare_worker(
        crate::runtime::relay_prepare_worker::RelayPrepareOwner::Share {
            prepared: prepared.clone(),
        },
        Some(1),
        zeroize::Zeroizing::new(signing.signing_pkcs8_bytes().to_vec()),
    )
    .expect("relay preparation");
    tokio::time::timeout(std::time::Duration::from_secs(1), primed)
        .await
        .expect("relay should prime")
        .expect("server should observe key rotation");
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while !app
            .relay_prepare_tasks
            .get(&id)
            .is_some_and(tokio::task::JoinHandle::is_finished)
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("relay preparation should complete");
    app.state
        .local
        .sessions
        .record_mut(id)
        .expect("session")
        .local_incarnation_id = uuid::Uuid::now_v7();

    app.pump_relay_prepare_worker().await;

    assert!(!app.state.sharing.host_relays.active(id));
    assert_eq!(app.terminal_hub.connection_count(id), 0);
    assert!(!app.relay_prepare_tasks.contains_key(&id));
    assert!(!app.relay_prepare_owners.contains_key(&id));
    tokio::time::timeout(std::time::Duration::from_secs(1), closed)
        .await
        .expect("stale prepared socket should close")
        .expect("server should observe socket close");
    assert!(matches!(
        app.share_transitions
            .get(&account, prepared.transition_id)
            .expect("ledger")
            .expect("transition")
            .state,
        ShareTransitionState::RelayPending { .. }
    ));
}

#[tokio::test]
async fn relay_preparation_is_concurrent_and_cancelled_per_session() {
    let (mut app, first_id, first_incarnation, account) = owned_app();
    configure_relay_test_auth(&mut app);
    let second_id = SessionId::new();
    insert_owned_session_for_test(
        &mut app,
        claude_owned_summary(second_id, "/tmp/kodosi-second"),
        test_session_handle(),
    );
    let second_incarnation = app
        .state
        .local
        .sessions
        .record(second_id)
        .expect("second session")
        .local_incarnation_id;
    let first = relay_pending_transition(
        &mut app,
        first_id,
        first_incarnation,
        &account,
        "relay-owner-first",
    );
    let second = relay_pending_transition(
        &mut app,
        second_id,
        second_incarnation,
        &account,
        "relay-owner-second",
    );
    let first_signing =
        crate::identity_core::device_keys::generate_device_keys_for_test("relay-owner-first")
            .expect("first signing key");
    let second_signing =
        crate::identity_core::device_keys::generate_device_keys_for_test("relay-owner-second")
            .expect("second signing key");
    let (host_relay, both_accepted) = hanging_websocket_server_for_paths(vec![
        format!("/hosts/{first_id}"),
        format!("/hosts/{second_id}"),
    ])
    .await;
    app.host_ws = kodosi_backend_client::host_ws::HostWsClient::new(Some(&host_relay))
        .expect("host websocket client");

    app.start_relay_prepare_worker(
        crate::runtime::relay_prepare_worker::RelayPrepareOwner::Share {
            prepared: first.clone(),
        },
        Some(1),
        zeroize::Zeroizing::new(first_signing.signing_pkcs8_bytes().to_vec()),
    )
    .expect("first relay preparation");
    app.start_relay_prepare_worker(
        crate::runtime::relay_prepare_worker::RelayPrepareOwner::Share {
            prepared: second.clone(),
        },
        Some(1),
        zeroize::Zeroizing::new(second_signing.signing_pkcs8_bytes().to_vec()),
    )
    .expect("second relay preparation");
    tokio::time::timeout(std::time::Duration::from_secs(1), both_accepted)
        .await
        .expect("both websocket upgrades should dispatch")
        .expect("server should observe both websocket upgrades");
    assert_eq!(app.relay_prepare_tasks.len(), 2);
    assert_eq!(app.terminal_hub.connection_count(first_id), 1);
    assert_eq!(app.terminal_hub.connection_count(second_id), 1);
    assert_snapshot_responsive(&mut app).await;

    app.cancel_share_transition_for_session(first_id, "cancel first relay");
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while app.terminal_hub.connection_count(first_id) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("first provisional bridge should unregister");
    assert!(!app.relay_prepare_tasks.contains_key(&first_id));
    assert!(app.relay_prepare_tasks.contains_key(&second_id));
    assert_eq!(app.terminal_hub.connection_count(second_id), 1);

    app.cancel_share_transition_for_session(second_id, "cancel second relay");
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while app.terminal_hub.connection_count(second_id) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("second provisional bridge should unregister");
    assert!(app.relay_prepare_tasks.is_empty());
    assert!(app.relay_prepare_owners.is_empty());
}

#[tokio::test]
async fn panicking_share_worker_becomes_typed_cleanup_completion() {
    let (mut app, id, incarnation, account) = owned_app();
    let cleanup = cleanup(&app);
    app.collaboration_teardown
        .provision(
            app.backend.backend_origin().expect("origin"),
            &account,
            &id.to_string(),
            cleanup.create_idempotency_id,
            cleanup.end_mutation_id,
            cleanup.created_at_ms,
        )
        .expect("cleanup obligation");
    let mut prepared = PreparedShareTransition::new(
        uuid::Uuid::now_v7(),
        1,
        account.clone(),
        app.state.identity.account_epoch().value(),
        id,
        incarnation,
        ShareAudience {
            scope: ShareScope::JustMe,
            room_id: None,
        },
        ShareAudience {
            scope: ShareScope::MyDevices,
            room_id: None,
        },
        id.to_string(),
        None,
        None,
        Some(cleanup),
    )
    .expect("transition");
    prepared.state = ShareTransitionState::ApplyingTarget;
    app.share_transitions
        .put(prepared.clone())
        .expect("persist transition");
    app.share_transition_in_flight
        .insert(id, prepared.transition_id);
    app.install_share_transition_deadline_for_test(prepared.transition_id);
    app.spawn_panicking_share_worker_for_test(
        prepared.clone(),
        ShareTransitionWorkerMode::ApplyTarget,
    )
    .expect("spawn panicking worker");

    let completion = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        app.share_transition_completion_rx.recv(),
    )
    .await
    .expect("panic completion deadline")
    .expect("typed panic completion");
    assert!(
        completion
            .outcome
            .as_ref()
            .expect_err("panic must become an error")
            .to_string()
            .contains("worker panicked")
    );
    app.apply_received_share_transition_completion(completion);

    assert!(
        !app.share_transition_tasks
            .contains_key(&prepared.transition_id)
    );
    assert!(!app.share_transition_in_flight.contains_key(&id));
    assert!(app.state.sharing.shared_sessions.get(id).is_none());
    assert!(matches!(
        app.share_transitions
            .get(&account, prepared.transition_id)
            .expect("ledger")
            .expect("transition")
            .state,
        ShareTransitionState::CleanupPending(ref terminal)
            if terminal.status == ShareTransitionTerminalStatus::Rejected
    ));
}

#[tokio::test]
async fn share_worker_capacity_failure_enters_durable_cleanup() {
    let (mut app, first_id, first_incarnation, account) = owned_app();
    let mut sessions = vec![(first_id, first_incarnation)];
    for index in 1..=32 {
        let id = SessionId::new();
        insert_owned_session_for_test(
            &mut app,
            claude_owned_summary(id, &format!("/tmp/kodosi-capacity-{index}")),
            test_session_handle(),
        );
        let incarnation = app
            .state
            .local
            .sessions
            .record(id)
            .expect("capacity session")
            .local_incarnation_id;
        sessions.push((id, incarnation));
    }

    for (id, incarnation) in sessions.iter().copied().take(32) {
        app.begin_share_transition(
            uuid::Uuid::now_v7(),
            id,
            incarnation,
            ShareScope::MyDevices,
            None,
        )
        .expect("worker slot");
    }
    assert_eq!(app.share_transition_tasks.len(), 32);

    let (overflow_id, overflow_incarnation) = sessions[32];
    let overflow_transition_id = uuid::Uuid::now_v7();
    let error = app
        .begin_share_transition(
            overflow_transition_id,
            overflow_id,
            overflow_incarnation,
            ShareScope::MyDevices,
            None,
        )
        .expect_err("thirty-third worker must be rejected");
    assert!(matches!(error, crate::AppError::ChannelFull { .. }));
    assert!(matches!(
        app.share_transitions
            .get(&account, overflow_transition_id)
            .expect("ledger")
            .expect("overflow transition")
            .state,
        ShareTransitionState::CleanupPending(ref terminal)
            if terminal.status == ShareTransitionTerminalStatus::Rejected
    ));
    assert!(!app.share_transition_in_flight.contains_key(&overflow_id));
    assert!(app.state.sharing.shared_sessions.get(overflow_id).is_none());
}

#[tokio::test]
async fn unavailable_share_ledger_keeps_local_commands_live_and_fences_collaboration() {
    let (mut app, id, incarnation, account) = owned_app();
    let backend_incarnation_id = install_shared_authority(&mut app, id, &account);
    let create_id = app
        .collaboration_teardown
        .list_for_account(app.backend.backend_origin().expect("origin"), &account)
        .expect("obligations")
        .into_iter()
        .next()
        .expect("obligation")
        .create_idempotency_id;
    app.share_transitions =
        crate::runtime::share_transition_ledger::ShareTransitionLedger::unavailable_for_test(
            std::path::PathBuf::from("unavailable-share-ledger.json"),
            "unsupported ledger version 1",
        );

    assert_snapshot_responsive(&mut app).await;
    assert!(!app.remote_surfaces_ready());
    assert!(app.share_transition_active_for_session(&account, id));
    assert!(app.share_transition_holds_cleanup(create_id));
    let scope_error = app
        .begin_share_transition(
            uuid::Uuid::now_v7(),
            id,
            incarnation,
            ShareScope::JustMe,
            None,
        )
        .expect_err("scope change must remain fenced");
    assert!(scope_error.to_string().contains("retained evidence"));
    let access_error = crate::runtime::sharing::revoke_access(
        &mut app,
        id,
        incarnation,
        uuid::Uuid::now_v7(),
        &uuid::Uuid::now_v7().to_string(),
    )
    .expect_err("access change must remain fenced");
    assert!(
        access_error
            .to_string()
            .contains("remote operations are not ready")
    );
    app.process_collaboration_teardown_worker().await;
    assert!(app.collaboration_teardown_task.is_none());
    assert_eq!(
        app.state
            .sharing
            .shared_sessions
            .get(id)
            .expect("shared authority")
            .backend_incarnation_id(),
        &backend_incarnation_id
    );
}

#[tokio::test]
async fn unavailable_share_ledger_fences_resident_remote_open() {
    let (mut app, _, _, _) = owned_app();
    let remote_id = SessionId::new();
    app.state.discovery.replace_remote_sessions(
        vec![crate::discovery::RemoteSessionRecord {
            summary: kodosi_domain::session::SessionSummary::new_remote(
                remote_id,
                "Remote".to_owned(),
                "Owner".to_owned(),
                None,
                ShareScope::Friends,
                kodosi_domain::permissions::AccessLevel::Inject,
                kodosi_domain::terminal::TerminalSize::default(),
            ),
            incarnation_id: Some(uuid::Uuid::now_v7()),
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
    app.share_transitions =
        crate::runtime::share_transition_ledger::ShareTransitionLedger::unavailable_for_test(
            std::path::PathBuf::from("unavailable-share-ledger.json"),
            "unsupported ledger version 1",
        );

    let error = crate::runtime::remote_sessions::open(&mut app, remote_id)
        .await
        .expect_err("resident discovery must not bypass unavailable collaboration evidence");

    assert!(
        error
            .to_string()
            .contains("remote operations are not ready")
    );
    assert!(!app.state.remote.session_relays.contains(remote_id));
}

#[tokio::test]
async fn hanging_key_generation_claim_does_not_block_actor_work() {
    let (api, accepted) = hanging_request_server("POST ").await;
    let backend = kodosi_backend_client::http_client::BackendHttpClient::new(
        &kodosi_backend_client::config::BackendClientConfig {
            api: Some(api),
            ..kodosi_backend_client::config::BackendClientConfig::default()
        },
    )
    .expect("backend");
    let signing = crate::identity_core::device_keys::generate_device_keys_for_test("owner-test")
        .expect("signing key");
    let prepared = prepared_share_distribution(
        uuid::Uuid::now_v7().to_string(),
        uuid::Uuid::now_v7(),
        1,
        zeroize::Zeroizing::new(signing.signing_pkcs8_bytes().to_vec()),
    );
    let worker_backend = backend.clone();
    let worker = tokio::spawn(async move {
        crate::runtime::access_effect_worker::claim_share_key_generation(&worker_backend, prepared)
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(1), accepted)
        .await
        .expect("claim should dispatch")
        .expect("server should observe claim");
    let (mut app, _, _, _) = owned_app();
    assert_snapshot_responsive(&mut app).await;
    assert!(!worker.is_finished());
    worker.abort();
}

#[tokio::test]
async fn hanging_blob_publication_does_not_block_actor_work() {
    let (api, accepted) = hanging_request_server("POST ").await;
    let backend = kodosi_backend_client::http_client::BackendHttpClient::new(
        &kodosi_backend_client::config::BackendClientConfig {
            api: Some(api),
            ..kodosi_backend_client::config::BackendClientConfig::default()
        },
    )
    .expect("backend");
    let signing = crate::identity_core::device_keys::generate_device_keys_for_test("owner-test")
        .expect("signing key");
    let prepared = prepared_share_distribution(
        uuid::Uuid::now_v7().to_string(),
        uuid::Uuid::now_v7(),
        1,
        zeroize::Zeroizing::new(signing.signing_pkcs8_bytes().to_vec()),
    );
    let claimed = crate::runtime::access_effect_worker::ClaimedShareKeyDistribution {
        prepared,
        key_generation: 2,
    };
    let worker_backend = backend.clone();
    let worker = tokio::spawn(async move {
        crate::runtime::access_effect_worker::publish_share_key_distribution(
            &worker_backend,
            claimed,
        )
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(1), accepted)
        .await
        .expect("blob upload should dispatch")
        .expect("server should observe blob upload");
    let (mut app, _, _, _) = owned_app();
    assert_snapshot_responsive(&mut app).await;
    assert!(!worker.is_finished());
    worker.abort();
}

#[tokio::test]
async fn hanging_target_patch_does_not_block_actor_work() {
    let (api, accepted) = hanging_patch_server().await;
    let (mut app, id, incarnation, account) = owned_app();
    app.backend = kodosi_backend_client::http_client::BackendHttpClient::new(
        &kodosi_backend_client::config::BackendClientConfig {
            api: Some(api),
            ..kodosi_backend_client::config::BackendClientConfig::default()
        },
    )
    .expect("backend");
    install_shared_authority(&mut app, id, &account);

    let prepared = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        std::future::ready(app.begin_share_transition(
            uuid::Uuid::now_v7(),
            id,
            incarnation,
            ShareScope::Room,
            Some("room-a"),
        )),
    )
    .await
    .expect("actor admission must not await PATCH");
    assert!(
        prepared.is_err(),
        "unknown room should fail before dispatch"
    );

    app.state.available_rooms.push(crate::RoomListEntry {
        id: "room-a".to_owned(),
        name: "Room A".to_owned(),
        slug: "room-a".to_owned(),
    });
    let prepared = app
        .begin_share_transition(
            uuid::Uuid::now_v7(),
            id,
            incarnation,
            ShareScope::Room,
            Some("room-a"),
        )
        .expect("transition admission");
    tokio::time::timeout(std::time::Duration::from_secs(1), accepted)
        .await
        .expect("PATCH should dispatch")
        .expect("server should observe PATCH");
    let effects = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        crate::host_protocol::sessions_command::apply_session_command(
            &mut app,
            crate::host_protocol::SessionCommand::SnapshotRefresh,
        ),
    )
    .await
    .expect("unrelated local command must remain responsive")
    .expect("snapshot command");
    assert!(effects.defer_catalog_replay);
    app.cancel_share_transition_for_session(id, "test cancellation");
    assert!(matches!(
        app.share_transitions
            .get(&account, prepared.transition_id)
            .expect("ledger")
            .expect("transition")
            .state,
        ShareTransitionState::CleanupPending(_)
    ));
}

#[tokio::test]
async fn local_noop_is_terminal_and_replays_correlated_success() {
    let (mut app, id, incarnation, account) = owned_app();
    let request_id = uuid::Uuid::now_v7();

    let prepared = app
        .begin_share_transition(request_id, id, incarnation, ShareScope::JustMe, None)
        .expect("local no-op should settle");

    assert!(matches!(
        app.share_transitions
            .get(&account, request_id)
            .expect("ledger")
            .expect("transition")
            .state,
        ShareTransitionState::Terminal(ref terminal)
            if terminal.status == ShareTransitionTerminalStatus::Applied
    ));
    assert!(prepared.cleanup.is_none());
    app.begin_share_transition(request_id, id, incarnation, ShareScope::JustMe, None)
        .expect("duplicate no-op should replay");
    assert!(app.state.snapshot_refresh_pending);
}

#[tokio::test]
async fn offline_unshare_retires_local_authority_immediately() {
    let (mut app, id, incarnation, account) = owned_app();
    let backend_session_id = id.to_string();
    let backend_incarnation_id = uuid::Uuid::now_v7();
    app.state.sharing.shared_sessions.insert(
        id,
        crate::sharing::shared_session_registry::SharedSessionState::new(
            backend_session_id.clone(),
            backend_incarnation_id,
            "owner-secret".to_owned(),
            ShareScope::MyDevices,
            None,
            Some([7; 32]),
            Some(1),
        ),
    );
    let create_id = uuid::Uuid::now_v7();
    let end_id = uuid::Uuid::now_v7();
    let backend_origin = app
        .backend
        .backend_origin()
        .expect("backend origin")
        .clone();
    app.collaboration_teardown
        .provision(
            &backend_origin,
            &account,
            &backend_session_id,
            create_id,
            end_id,
            1,
        )
        .expect("cleanup obligation");
    app.collaboration_teardown
        .bind_incarnation(create_id, &backend_session_id, backend_incarnation_id)
        .expect("cleanup binding");
    app.state
        .apply_session_scope_locally(id, ShareScope::MyDevices);
    app.backend_compatibility_verified = false;

    app.begin_share_transition(
        uuid::Uuid::now_v7(),
        id,
        incarnation,
        ShareScope::JustMe,
        None,
    )
    .expect("offline unshare should remain local");

    assert!(app.state.sharing.shared_sessions.get(id).is_none());
    assert_eq!(
        app.state
            .local
            .sessions
            .record(id)
            .expect("session")
            .summary
            .scope,
        ShareScope::JustMe
    );
}

#[tokio::test]
async fn restart_moves_every_nonterminal_remote_phase_to_cleanup_only() {
    let (mut app, id, incarnation, account) = owned_app();
    let cleanup = cleanup(&app);
    app.collaboration_teardown
        .provision(
            app.backend.backend_origin().expect("origin"),
            &account,
            &id.to_string(),
            cleanup.create_idempotency_id,
            cleanup.end_mutation_id,
            cleanup.created_at_ms,
        )
        .expect("cleanup obligation");
    let mut prepared = PreparedShareTransition::new(
        uuid::Uuid::now_v7(),
        1,
        account.clone(),
        app.state.identity.account_epoch().value(),
        id,
        incarnation,
        ShareAudience {
            scope: ShareScope::JustMe,
            room_id: None,
        },
        ShareAudience {
            scope: ShareScope::MyDevices,
            room_id: None,
        },
        id.to_string(),
        None,
        None,
        Some(cleanup),
    )
    .expect("transition");
    prepared.state = ShareTransitionState::GenerationClaiming;
    app.share_transitions
        .put(prepared.clone())
        .expect("persist");

    app.recover_share_transitions();

    assert!(matches!(
        app.share_transitions
            .get(&account, prepared.transition_id)
            .expect("ledger")
            .expect("transition")
            .state,
        ShareTransitionState::CleanupPending(_)
    ));
    assert!(app.state.sharing.shared_sessions.get(id).is_none());
}

#[tokio::test]
async fn exact_completion_fence_rejects_each_authority_change() {
    let (mut app, id, incarnation, account) = owned_app();
    let cleanup = cleanup(&app);
    let mut prepared = PreparedShareTransition::new(
        uuid::Uuid::now_v7(),
        1,
        account.clone(),
        app.state.identity.account_epoch().value(),
        id,
        incarnation,
        ShareAudience {
            scope: ShareScope::JustMe,
            room_id: None,
        },
        ShareAudience {
            scope: ShareScope::MyDevices,
            room_id: None,
        },
        id.to_string(),
        None,
        None,
        Some(cleanup),
    )
    .expect("transition");
    prepared.state = ShareTransitionState::ApplyingTarget;
    app.share_transitions
        .put(prepared.clone())
        .expect("persist");
    app.share_transition_in_flight
        .insert(id, prepared.transition_id);
    let epoch = app.state.identity.account_epoch().value();
    assert!(app.share_completion_matches_for_test(
        &prepared,
        epoch,
        ShareTransitionWorkerMode::ApplyTarget,
    ));

    assert!(!app.share_completion_matches_for_test(
        &prepared,
        epoch + 1,
        ShareTransitionWorkerMode::ApplyTarget,
    ));
    app.share_transition_in_flight.remove(&id);
    assert!(!app.share_completion_matches_for_test(
        &prepared,
        epoch,
        ShareTransitionWorkerMode::ApplyTarget,
    ));
    app.share_transition_in_flight
        .insert(id, prepared.transition_id);
    app.state
        .local
        .sessions
        .record_mut(id)
        .expect("session")
        .local_incarnation_id = uuid::Uuid::now_v7();
    assert!(!app.share_completion_matches_for_test(
        &prepared,
        epoch,
        ShareTransitionWorkerMode::ApplyTarget,
    ));
}

#[tokio::test]
async fn spontaneous_stop_cancels_active_share_transition() {
    let (mut app, id, incarnation, account) = owned_app();
    let cleanup = cleanup(&app);
    let mut prepared = PreparedShareTransition::new(
        uuid::Uuid::now_v7(),
        1,
        account.clone(),
        app.state.identity.account_epoch().value(),
        id,
        incarnation,
        ShareAudience {
            scope: ShareScope::JustMe,
            room_id: None,
        },
        ShareAudience {
            scope: ShareScope::MyDevices,
            room_id: None,
        },
        id.to_string(),
        None,
        None,
        Some(cleanup),
    )
    .expect("transition");
    prepared.state = ShareTransitionState::ApplyingTarget;
    app.share_transitions
        .put(prepared.clone())
        .expect("persist transition");
    app.share_transition_in_flight
        .insert(id, prepared.transition_id);

    app.handle_session_event(
        crate::session_runtime::events::RuntimeSessionEvent::Stopped {
            origin: local_coordinator_origin(&app, id),
            reason: kodosi_domain::lifecycle::StopReason::UserRequested,
        },
    );

    assert!(matches!(
        app.share_transitions
            .get(&account, prepared.transition_id)
            .expect("ledger")
            .expect("transition")
            .state,
        ShareTransitionState::CleanupPending(_)
    ));
    assert!(
        !app.share_transition_tasks
            .contains_key(&prepared.transition_id)
    );
}

#[tokio::test]
async fn stopping_session_rejects_new_access_mutation_before_ledger_admission() {
    let (mut app, id, incarnation, account) = owned_app();
    install_shared_authority(&mut app, id, &account);
    app.state
        .local
        .sessions
        .update_state(id, kodosi_domain::session::SessionState::Stopping);
    let mutation_id = uuid::Uuid::now_v7();

    let error = crate::runtime::sharing::revoke_access(
        &mut app,
        id,
        incarnation,
        mutation_id,
        &uuid::Uuid::now_v7().to_string(),
    )
    .expect_err("stopping session cannot admit access changes");

    assert!(matches!(error, crate::AppError::NoActiveSession));
    assert!(
        app.access_mutations
            .get(&account, mutation_id)
            .expect("ledger")
            .is_none()
    );
}

#[tokio::test]
async fn applied_access_completion_while_stopping_enters_retiring_without_local_effects() {
    let (mut app, id, incarnation, account) = owned_app();
    let backend_incarnation_id = install_shared_authority(&mut app, id, &account);
    let access =
        attempting_access_mutation(&mut app, id, incarnation, &account, backend_incarnation_id);
    app.state
        .local
        .sessions
        .update_state(id, kodosi_domain::session::SessionState::Stopping);
    app.retire_access_mutations_for_session(id, "session stopping");

    app.apply_access_mutation_completion_for_test(
        crate::runtime::access_mutation_worker::SessionAccessMutationWorkerCompletion {
            prepared: access.clone(),
            account_epoch: app.state.identity.account_epoch().value(),
            mode: crate::runtime::access_mutation_worker::SessionAccessMutationWorkerMode::Dispatch,
            outcome:
                crate::runtime::access_mutation_worker::SessionAccessMutationWorkerOutcome::Applied,
            access_snapshot: None,
        },
    )
    .await;

    assert!(matches!(
        app.access_mutations
            .get(&account, access.mutation_id)
            .expect("ledger")
            .expect("access mutation")
            .state,
        crate::runtime::access_mutations::PreparedSessionAccessMutationState::Terminal(
            crate::runtime::access_mutations::SessionAccessMutationTerminal {
                status:
                    crate::runtime::access_mutations::SessionAccessMutationTerminalStatus::Applied,
                ..
            }
        )
    ));
    assert!(app.access_effect_task.is_none());
    assert!(!app.relay_prepare_tasks.contains_key(&id));
}

#[tokio::test]
async fn stopped_event_retires_uncertain_access_without_blocking_replacement_authority() {
    let (mut app, id, incarnation, account) = owned_app();
    let backend_incarnation_id = install_shared_authority(&mut app, id, &account);
    let access =
        attempting_access_mutation(&mut app, id, incarnation, &account, backend_incarnation_id);

    app.handle_session_event(
        crate::session_runtime::events::RuntimeSessionEvent::Stopped {
            origin: local_coordinator_origin(&app, id),
            reason: kodosi_domain::lifecycle::StopReason::UserRequested,
        },
    );

    assert!(matches!(
        app.access_mutations
            .get(&account, access.mutation_id)
            .expect("ledger")
            .expect("access mutation")
            .state,
        crate::runtime::access_mutations::PreparedSessionAccessMutationState::Retiring
    ));
    assert!(
        !app.collaboration_change_active_for_test(&account, id)
            .expect("collaboration predicate"),
        "retiring receipt reconciliation must not hold replacement local authority"
    );
}

#[tokio::test]
async fn unshare_preempts_active_access_mutation_into_receipt_only_retirement() {
    let (mut app, id, incarnation, account) = owned_app();
    let backend_session_id = id.to_string();
    let backend_incarnation_id = uuid::Uuid::now_v7();
    app.state.sharing.shared_sessions.insert(
        id,
        crate::sharing::shared_session_registry::SharedSessionState::new(
            backend_session_id.clone(),
            backend_incarnation_id,
            "owner-secret".to_owned(),
            ShareScope::MyDevices,
            None,
            Some([7; 32]),
            Some(1),
        ),
    );
    app.state
        .apply_session_scope_locally(id, ShareScope::MyDevices);
    let create_id = uuid::Uuid::now_v7();
    let end_id = uuid::Uuid::now_v7();
    app.collaboration_teardown
        .provision(
            app.backend.backend_origin().expect("origin"),
            &account,
            &backend_session_id,
            create_id,
            end_id,
            1,
        )
        .expect("cleanup obligation");
    app.collaboration_teardown
        .bind_incarnation(create_id, &backend_session_id, backend_incarnation_id)
        .expect("cleanup binding");
    let mut access = crate::runtime::access_mutations::PreparedSessionAccessMutation::new(
        uuid::Uuid::now_v7(),
        account.clone(),
        app.state.identity.account_epoch().value(),
        id,
        incarnation,
        backend_session_id,
        backend_incarnation_id,
        crate::runtime::access_mutations::SessionAccessMutationTarget::Revoke {
            actor_user_id: uuid::Uuid::now_v7(),
        },
    )
    .expect("access mutation");
    access.state = crate::runtime::access_mutations::PreparedSessionAccessMutationState::Attempting;
    let access_id = access.mutation_id;
    app.access_mutations.put(access).expect("persist access");

    app.begin_share_transition(
        uuid::Uuid::now_v7(),
        id,
        incarnation,
        ShareScope::JustMe,
        None,
    )
    .expect("unshare should preempt access work");

    assert!(matches!(
        app.access_mutations
            .get(&account, access_id)
            .expect("ledger")
            .expect("access mutation")
            .state,
        crate::runtime::access_mutations::PreparedSessionAccessMutationState::Retiring
    ));
    assert!(app.state.sharing.shared_sessions.get(id).is_none());
}

#[tokio::test]
async fn unshare_preempts_initial_share_while_local_scope_is_still_just_me() {
    let (api, accepted) = hanging_request_server("POST ").await;
    let (mut app, id, incarnation, account) = owned_app();
    app.backend = kodosi_backend_client::http_client::BackendHttpClient::new(
        &kodosi_backend_client::config::BackendClientConfig {
            api: Some(api),
            ..kodosi_backend_client::config::BackendClientConfig::default()
        },
    )
    .expect("backend");
    let original = app
        .begin_share_transition(
            uuid::Uuid::now_v7(),
            id,
            incarnation,
            ShareScope::MyDevices,
            None,
        )
        .expect("initial share admission");
    tokio::time::timeout(std::time::Duration::from_secs(1), accepted)
        .await
        .expect("CREATE should dispatch")
        .expect("server should observe CREATE");
    assert_eq!(
        app.state
            .local
            .sessions
            .record(id)
            .expect("session")
            .summary
            .scope,
        ShareScope::JustMe
    );

    let unshare_id = uuid::Uuid::now_v7();
    app.begin_share_transition(unshare_id, id, incarnation, ShareScope::JustMe, None)
        .expect("unshare should preempt initial share");

    assert!(
        !app.share_transition_tasks
            .contains_key(&original.transition_id)
    );
    assert!(!app.share_transition_in_flight.contains_key(&id));
    assert!(matches!(
        app.share_transitions
            .get(&account, original.transition_id)
            .expect("ledger")
            .expect("original transition")
            .state,
        ShareTransitionState::CleanupPending(ref terminal)
            if terminal.status == ShareTransitionTerminalStatus::Cancelled
    ));
    assert!(matches!(
        app.share_transitions
            .get(&account, unshare_id)
            .expect("ledger")
            .expect("unshare transition")
            .state,
        ShareTransitionState::Terminal(ref terminal)
            if terminal.status == ShareTransitionTerminalStatus::Applied
    ));
    assert!(app.state.sharing.shared_sessions.get(id).is_none());
    assert!(!app.state.sharing.host_relays.active(id));
    let cleanup = original.cleanup.as_ref().expect("cleanup identity");
    let obligations = app
        .collaboration_teardown
        .list_for_account(app.backend.backend_origin().expect("origin"), &account)
        .expect("cleanup obligations");
    assert_eq!(obligations.len(), 1);
    assert_eq!(
        obligations[0].create_idempotency_id,
        cleanup.create_idempotency_id
    );
    assert_eq!(obligations[0].end_mutation_id, cleanup.end_mutation_id);

    app.apply_received_share_transition_completion(
        crate::runtime::share_transition_worker::ShareTransitionCompletion {
            prepared: original,
            account_epoch: app.state.identity.account_epoch().value(),
            mode: ShareTransitionWorkerMode::ApplyTarget,
            outcome: Err(crate::AppError::NoActiveSession),
        },
    );
    assert!(app.state.sharing.shared_sessions.get(id).is_none());
    assert!(matches!(
        app.share_transitions
            .get(&account, unshare_id)
            .expect("ledger")
            .expect("unshare transition")
            .state,
        ShareTransitionState::Terminal(ref terminal)
            if terminal.status == ShareTransitionTerminalStatus::Applied
    ));
}

#[tokio::test]
async fn unshare_handoff_from_pending_scope_patch_reuses_exact_cleanup() {
    let (api, accepted) = hanging_patch_server().await;
    let (mut app, id, incarnation, account) = owned_app();
    app.backend = kodosi_backend_client::http_client::BackendHttpClient::new(
        &kodosi_backend_client::config::BackendClientConfig {
            api: Some(api),
            ..kodosi_backend_client::config::BackendClientConfig::default()
        },
    )
    .expect("backend");
    let backend_incarnation_id = install_shared_authority(&mut app, id, &account);
    app.state.available_rooms.push(crate::RoomListEntry {
        id: "room-a".to_owned(),
        name: "Room A".to_owned(),
        slug: "room-a".to_owned(),
    });
    let original = app
        .begin_share_transition(
            uuid::Uuid::now_v7(),
            id,
            incarnation,
            ShareScope::Room,
            Some("room-a"),
        )
        .expect("scope patch admission");
    tokio::time::timeout(std::time::Duration::from_secs(1), accepted)
        .await
        .expect("PATCH should dispatch")
        .expect("server should observe PATCH");
    let cleanup = original.cleanup.clone().expect("cleanup identity");

    let unshare_id = uuid::Uuid::now_v7();
    app.begin_share_transition(unshare_id, id, incarnation, ShareScope::JustMe, None)
        .expect("unshare should preempt scope patch");

    assert!(
        !app.share_transition_tasks
            .contains_key(&original.transition_id)
    );
    assert!(!app.share_transition_in_flight.contains_key(&id));
    assert!(matches!(
        app.share_transitions
            .get(&account, original.transition_id)
            .expect("ledger")
            .expect("original transition")
            .state,
        ShareTransitionState::CleanupPending(ref terminal)
            if terminal.status == ShareTransitionTerminalStatus::Cancelled
    ));
    assert!(matches!(
        app.share_transitions
            .get(&account, unshare_id)
            .expect("ledger")
            .expect("unshare transition")
            .state,
        ShareTransitionState::Terminal(ref terminal)
            if terminal.status == ShareTransitionTerminalStatus::Applied
    ));
    assert!(app.state.sharing.shared_sessions.get(id).is_none());
    assert_eq!(
        app.state
            .local
            .sessions
            .record(id)
            .expect("session")
            .summary
            .scope,
        ShareScope::JustMe
    );
    let obligations = app
        .collaboration_teardown
        .list_for_account(app.backend.backend_origin().expect("origin"), &account)
        .expect("cleanup obligations");
    assert_eq!(obligations.len(), 1);
    assert_eq!(
        obligations[0].backend_incarnation_id,
        Some(backend_incarnation_id)
    );
    assert_eq!(
        obligations[0].create_idempotency_id,
        cleanup.create_idempotency_id
    );
    assert_eq!(obligations[0].end_mutation_id, cleanup.end_mutation_id);
}

#[tokio::test]
async fn access_and_share_operations_exclude_each_other() {
    let (mut app, id, incarnation, account) = owned_app();
    let mut access = crate::runtime::access_mutations::PreparedSessionAccessMutation::new(
        uuid::Uuid::now_v7(),
        account,
        app.state.identity.account_epoch().value(),
        id,
        incarnation,
        id.to_string(),
        uuid::Uuid::now_v7(),
        crate::runtime::access_mutations::SessionAccessMutationTarget::Revoke {
            actor_user_id: uuid::Uuid::now_v7(),
        },
    )
    .expect("access mutation");
    access.state = crate::runtime::access_mutations::PreparedSessionAccessMutationState::Attempting;
    app.access_mutations.put(access).expect("persist access");

    let error = app
        .begin_share_transition(
            uuid::Uuid::now_v7(),
            id,
            incarnation,
            ShareScope::MyDevices,
            None,
        )
        .expect_err("active access mutation must exclude share transition");
    assert!(error.to_string().contains("collaboration change"));
}
