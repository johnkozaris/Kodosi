use super::*;
use std::sync::Arc;
use tokio::time::{Instant, timeout};

struct TestRuntime {
    handle: RuntimeHandle,
    _storage: tempfile::TempDir,
}

impl TestRuntime {
    async fn new() -> Self {
        let storage = tempfile::tempdir().unwrap();
        let mut config = Config::isolated(&storage.path().canonicalize().unwrap()).unwrap();
        config.initial_shell = Some("/bin/sh".to_owned());
        let handle = start(config).await.unwrap();
        Self {
            handle,
            _storage: storage,
        }
    }

    async fn shutdown(&self) {
        self.handle.shutdown().await;
    }

    async fn send(&self, command: Command) {
        let events = self.handle.snapshot().await.unwrap();
        let event = events.first().unwrap();
        self.handle
            .try_send(CommandEnvelope {
                account_user_id: event.account_user_id.clone(),
                account_epoch: event.account_epoch,
                command,
            })
            .unwrap();
    }

    async fn create(&self) -> (Uuid, Uuid) {
        let (_, mut events) = self.handle.observe().await.unwrap();
        let request = Uuid::now_v7().to_string();
        self.send(Command::CreateSession {
            request_id: request.clone(),
            name: "Test terminal".to_owned(),
            working_dir: None,
            resume: None,
        })
        .await;
        timeout(Duration::from_secs(10), async {
            loop {
                let event = serde_json::to_value(events.recv().await.unwrap()).unwrap();
                if event["type"] == "sessions.snapshot" {
                    for entry in event["sessions"].as_array().unwrap() {
                        if entry["createRequestId"] == request {
                            return (
                                Uuid::parse_str(entry["id"].as_str().unwrap()).unwrap(),
                                Uuid::parse_str(entry["incarnationId"].as_str().unwrap()).unwrap(),
                            );
                        }
                    }
                }
                assert_ne!(event["type"], "session.error", "{event}");
            }
        })
        .await
        .unwrap()
    }
}

impl Drop for TestRuntime {
    fn drop(&mut self) {
        let handle = self.handle.clone();
        tokio::spawn(async move {
            handle.shutdown().await;
        });
    }
}

async fn receive_text(subscription: &mut Subscription, wanted: &[u8]) -> Vec<u8> {
    timeout(Duration::from_secs(10), async {
        let mut received = Vec::new();
        while !received.windows(wanted.len()).any(|part| part == wanted) {
            let frame = subscription
                .data
                .recv()
                .await
                .expect("terminal output stream");
            received.extend_from_slice(&frame.bytes);
        }
        received
    })
    .await
    .expect("terminal output timeout")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_terminal_keeps_running_after_view_closes_then_stop_removes_it() {
    let runtime = TestRuntime::new().await;
    let (session, incarnation) = runtime.create().await;
    let mut first = runtime.handle.subscribe_terminal(session).await.unwrap();
    runtime
        .handle
        .input(
            session,
            incarnation,
            first.connection_id,
            Bytes::from_static(b"printf 'first-%s\\n' marker\n"),
        )
        .unwrap();
    receive_text(&mut first, b"first-marker").await;
    runtime
        .handle
        .unsubscribe_terminal(session, first.connection_id)
        .await;
    drop(first);
    let mut second = runtime.handle.subscribe_terminal(session).await.unwrap();
    runtime
        .handle
        .input(
            session,
            incarnation,
            second.connection_id,
            Bytes::from_static(b"printf 'second-%s\\n' marker\n"),
        )
        .unwrap();
    receive_text(&mut second, b"second-marker").await;
    let (_, mut events) = runtime.handle.observe().await.unwrap();
    runtime
        .send(Command::StopSession {
            request_id: Uuid::now_v7().to_string(),
            session_id: session.to_string(),
            expected_runtime_incarnation_id: incarnation.to_string(),
        })
        .await;
    timeout(Duration::from_secs(12), async {
        loop {
            let event = serde_json::to_value(events.recv().await.unwrap()).unwrap();
            if event["type"] == "sessions.snapshot"
                && event["sessions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|entry| entry["id"] != session.to_string())
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert!(matches!(
        runtime.handle.subscribe_terminal(session).await,
        Err(Error::NotFound)
    ));
    runtime.shutdown().await;
    drop(runtime);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slow_login_does_not_block_local_terminal_or_runtime_observation() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let received = Arc::new(tokio::sync::Notify::new());
    let signal = Arc::clone(&received);
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        signal.notify_one();
        tokio::time::sleep(Duration::from_secs(30)).await;
        drop(socket);
    });
    let storage = tempfile::tempdir().unwrap();
    let mut config = Config::isolated(&storage.path().canonicalize().unwrap()).unwrap();
    config.initial_shell = Some("/bin/sh".to_owned());
    config.oidc_issuer = format!("http://{address}");
    let runtime = TestRuntime {
        handle: start(config).await.unwrap(),
        _storage: storage,
    };
    let (session, incarnation) = runtime.create().await;
    let mut terminal = runtime.handle.subscribe_terminal(session).await.unwrap();
    runtime.send(Command::Login {}).await;
    timeout(Duration::from_secs(3), received.notified())
        .await
        .unwrap();
    let (_, _) = timeout(Duration::from_millis(500), runtime.handle.observe())
        .await
        .expect("login blocked registry")
        .unwrap();
    runtime
        .handle
        .input(
            session,
            incarnation,
            terminal.connection_id,
            Bytes::from_static(b"printf 'during-%s\\n' login\n"),
        )
        .unwrap();
    receive_text(&mut terminal, b"during-login").await;
    timeout(Duration::from_secs(12), runtime.shutdown())
        .await
        .expect("shutdown waited for login HTTP timeout");
    drop(runtime);
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stale_epoch_cannot_create_or_shutdown_current_runtime() {
    let runtime = TestRuntime::new().await;
    let (snapshot, mut events) = runtime.handle.observe().await.unwrap();
    let scope = &snapshot[0];
    for command in [
        Command::CreateSession {
            request_id: Uuid::now_v7().to_string(),
            name: "stale".to_owned(),
            working_dir: None,
            resume: None,
        },
        Command::Shutdown {},
    ] {
        runtime
            .handle
            .try_send(CommandEnvelope {
                account_user_id: scope.account_user_id.clone(),
                account_epoch: scope.account_epoch + 1,
                command,
            })
            .unwrap();
    }
    timeout(Duration::from_secs(2), async {
        let mut errors = 0;
        while errors < 2 {
            let event = events.recv().await.unwrap();
            if matches!(event.kind(), "session.error" | "system.error") {
                errors += 1;
            }
        }
    })
    .await
    .unwrap();
    let snapshot = runtime.handle.snapshot().await.unwrap();
    let sessions = snapshot
        .iter()
        .find(|event| event.kind() == "sessions.snapshot")
        .unwrap();
    assert!(
        serde_json::to_value(sessions).unwrap()["sessions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    runtime.shutdown().await;
    drop(runtime);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_shutdown_waits_for_the_same_completion() {
    let runtime = TestRuntime::new().await;
    let _ = runtime.create().await;
    let first = runtime.handle.clone();
    let second = runtime.handle.clone();
    timeout(Duration::from_secs(12), async move {
        tokio::join!(first.shutdown(), second.shutdown());
    })
    .await
    .unwrap();
    timeout(Duration::from_millis(100), runtime.handle.stopped())
        .await
        .unwrap();
    assert!(matches!(
        runtime.handle.snapshot().await,
        Err(Error::Stopped)
    ));
    runtime.shutdown().await;
    drop(runtime);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_retry_is_exact_and_does_not_launch_a_second_process() {
    let runtime = TestRuntime::new().await;
    let command = Command::CreateSession {
        request_id: Uuid::now_v7().to_string(),
        name: "same".to_owned(),
        working_dir: None,
        resume: None,
    };
    runtime.send(command.clone()).await;
    runtime.send(command.clone()).await;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let events = runtime.handle.snapshot().await.unwrap();
        let sessions = serde_json::to_value(
            events
                .iter()
                .find(|event| event.kind() == "sessions.snapshot")
                .unwrap(),
        )
        .unwrap();
        if sessions["sessions"].as_array().unwrap().len() == 1 {
            break;
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let (_, mut events) = runtime.handle.observe().await.unwrap();
    runtime.send(command).await;
    timeout(Duration::from_secs(2), async {
        loop {
            if events.recv().await.unwrap().kind() == "session.result" {
                break;
            }
        }
    })
    .await
    .unwrap();
    let snapshot = runtime.handle.snapshot().await.unwrap();
    let sessions = snapshot
        .iter()
        .find(|event| event.kind() == "sessions.snapshot")
        .unwrap();
    assert_eq!(
        serde_json::to_value(sessions).unwrap()["sessions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    runtime.shutdown().await;
    drop(runtime);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn generic_terminal_command_cannot_bypass_subscription_admission() {
    let runtime = TestRuntime::new().await;
    let (session, incarnation) = runtime.create().await;
    let snapshot = runtime.handle.snapshot().await.unwrap();
    let error = runtime
        .handle
        .try_send(CommandEnvelope {
            account_user_id: snapshot[0].account_user_id.clone(),
            account_epoch: snapshot[0].account_epoch,
            command: Command::Focus {
                request_id: Uuid::now_v7().to_string(),
                session_id: session.to_string(),
                expected_runtime_incarnation_id: incarnation.to_string(),
                client_id: Uuid::now_v7().to_string(),
                subscription_generation: 1,
            },
        })
        .unwrap_err();
    assert!(matches!(error, Error::Invalid(_)));
    runtime.shutdown().await;
    drop(runtime);
}

fn registry_fixture() -> (Runtime, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let config = Config::isolated(&root.path().canonicalize().unwrap()).unwrap();
    let network = Network::new(NetworkConfig {
        api_url: config.backend_url.clone(),
        issuer: config.oidc_issuer.clone(),
        client_id: config.oidc_client_id.clone(),
        scopes: config.oidc_scopes.clone(),
        audience: None,
        data_root: config.data_root.clone(),
        secret_service: config.secret_service.clone(),
        isolated: true,
    })
    .unwrap();
    let scope = Scope {
        user: None,
        epoch: 0,
        network_generation: network.generation(),
    };
    let (events, _) = broadcast::channel(256);
    let (changes, _) = mpsc::channel(128);
    (
        Runtime {
            config,
            network,
            events,
            scope,
            local: HashMap::new(),
            creating: HashMap::new(),
            remotes: HashMap::new(),
            connections: HashMap::new(),
            opening: HashMap::new(),
            reconnect: HashMap::new(),
            publishing: BTreeSet::new(),
            retiring: BTreeSet::new(),
            unpublishing: BTreeSet::new(),
            administering: BTreeSet::new(),
            jobs: JoinSet::new(),
            changes,
            dark: true,
            initializing: false,
            auth_pending: false,
            latest_auth: None,
            cached_events: BTreeMap::new(),
        },
        root,
    )
}

fn remote_session(id: Uuid, incarnation_id: Uuid) -> RemoteSession {
    RemoteSession {
        id,
        incarnation_id,
        name: "Remote".to_owned(),
        owner_user_id: Uuid::now_v7().to_string(),
        owner_name: "Friend".to_owned(),
        host_device_id: "other-device".to_owned(),
        host_name: "Other computer".to_owned(),
        room_id: None,
        room_name: None,
        shared_with: vec![],
        online: true,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn login_failure_after_retiring_account_is_visible_and_retryable() {
    let runtime = TestRuntime::new().await;
    for _ in 0..2 {
        let (_, mut events) = runtime.handle.observe().await.unwrap();
        runtime.send(Command::Login {}).await;
        let error = timeout(Duration::from_secs(5), async {
            loop {
                let event = events.recv().await.unwrap();
                if event.kind() == "auth.error" {
                    break event;
                }
            }
        })
        .await
        .expect("login error dropped after generation changed");
        let value = serde_json::to_value(&error).unwrap();
        assert_eq!(value["operation"], "auth.login.start");
        let snapshot = runtime.handle.snapshot().await.unwrap();
        assert_eq!(error.account_epoch, snapshot[0].account_epoch);
    }
    runtime.shutdown().await;
    drop(runtime);
}

#[tokio::test]
async fn input_admission_is_byte_bounded_before_the_registry_polls() {
    let (commands, mut requests) = mpsc::channel(COMMAND_CAPACITY);
    let (events, _) = broadcast::channel(16);
    let (_stopped, stopped) = watch::channel(false);
    let handle = RuntimeHandle {
        commands,
        events,
        stopped,
        input_budget: Arc::new(Semaphore::new(MAX_QUEUED_INPUT_BYTES)),
    };
    let (session, incarnation, connection) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
    let bytes = Bytes::from(vec![b'x'; 1024 * 1024]);
    for _ in 0..4 {
        handle
            .input(session, incarnation, connection, bytes.clone())
            .unwrap();
    }
    assert!(matches!(
        handle.input(session, incarnation, connection, bytes.clone()),
        Err(Error::Busy)
    ));
    drop(requests.recv().await);
    handle
        .input(session, incarnation, connection, bytes)
        .unwrap();
}

#[tokio::test]
async fn stale_discovery_does_not_cancel_a_direct_link_open() {
    let (mut runtime, _root) = registry_fixture();
    let id = Uuid::now_v7();
    let cancellation = CancellationToken::new();
    runtime.opening.insert(
        id,
        Opening {
            attempt: Uuid::now_v7(),
            cancellation: cancellation.clone(),
            commands: vec![],
        },
    );
    runtime.replace_remotes(vec![]);
    assert!(runtime.opening.contains_key(&id));
    assert!(!cancellation.is_cancelled());
}

#[tokio::test]
async fn closed_remote_is_not_reported_connected_and_old_close_cannot_replace_new_connection() {
    let (mut runtime, _root) = registry_fixture();
    let id = Uuid::now_v7();
    let incarnation = Uuid::now_v7();
    let remote = remote_session(id, incarnation);
    let (old, _requests, _updates) = crate::network::test_remote_connection(remote.clone());
    runtime
        .connections
        .insert(id, RemoteTerminal::spawn(old, runtime.changes.clone()));
    let old_instance = runtime.connections[&id].instance_id;
    runtime.connections[&id].disconnect();
    runtime.replace_remotes(vec![remote.clone()]);
    assert_eq!(
        runtime.remotes[&id].connection_state,
        ConnectionState::Offline
    );
    assert!(!runtime.connections.contains_key(&id));
    let (new, _requests, _updates) = crate::network::test_remote_connection(remote);
    runtime
        .connections
        .insert(id, RemoteTerminal::spawn(new, runtime.changes.clone()));
    let new_instance = runtime.connections[&id].instance_id;
    runtime.change(SessionChange::RemoteClosed {
        id,
        incarnation,
        instance_id: old_instance,
        reason: "old connection".to_owned(),
    });
    assert_eq!(runtime.connections[&id].instance_id, new_instance);
    runtime.connections[&id].disconnect();
}

#[tokio::test]
async fn observation_retains_pending_sign_in_and_discards_stale_auth_events() {
    let (mut runtime, _root) = registry_fixture();
    let pending = json!({"type":"auth.device_code", "userCode":"ABCD-EFGH", "verificationUri":"https://example.invalid/device"});
    runtime.network_event(NetworkEvent {
        generation: runtime.network.generation(),
        user_id: None,
        event: pending.clone(),
    });
    assert_eq!(runtime.auth_event(), pending);
    runtime.network_event(NetworkEvent {
        generation: runtime.network.generation() - 1,
        user_id: None,
        event: json!({"type":"auth.required", "reason":"signedOut"}),
    });
    assert_eq!(runtime.auth_event(), pending);
    runtime.network_event(NetworkEvent { generation: runtime.network.generation(), user_id: None, event: json!({"type":"auth.error", "operation":"auth.login.start", "message":"Code expired"}) });
    assert_eq!(runtime.auth_event()["type"], "auth.required");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_completion_publishes_catalog_before_its_result_and_retry_checks_directory() {
    let runtime = TestRuntime::new().await;
    let (_, mut events) = runtime.handle.observe().await.unwrap();
    let request_id = Uuid::now_v7().to_string();
    let command = Command::CreateSession {
        request_id: request_id.clone(),
        name: "ordered".to_owned(),
        working_dir: None,
        resume: None,
    };
    runtime.send(command).await;
    timeout(Duration::from_secs(5), async {
        let mut observed = false;
        loop {
            let value = serde_json::to_value(events.recv().await.unwrap()).unwrap();
            if value["type"] == "sessions.snapshot" {
                observed |= value["sessions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|entry| entry["createRequestId"] == request_id);
            }
            if value["type"] == "session.result" && value["requestId"] == request_id {
                assert!(observed);
                break;
            }
        }
    })
    .await
    .unwrap();
    runtime
        .send(Command::CreateSession {
            request_id: request_id.clone(),
            name: "ordered".to_owned(),
            working_dir: Some("/".to_owned()),
            resume: None,
        })
        .await;
    timeout(Duration::from_secs(2), async {
        loop {
            let event = serde_json::to_value(events.recv().await.unwrap()).unwrap();
            if event["type"] == "session.error" && event["requestId"] == request_id {
                break;
            }
        }
    })
    .await
    .unwrap();
    runtime.shutdown().await;
    drop(runtime);
}

#[tokio::test]
async fn first_remote_open_retains_retry_demand_until_explicit_disconnect() {
    let (mut runtime, _root) = registry_fixture();
    let id = Uuid::now_v7();
    runtime.replace_remotes(vec![remote_session(id, Uuid::now_v7())]);
    runtime
        .open_remote(
            id,
            Command::OpenRemote {
                request_id: Uuid::now_v7().to_string(),
                session_id: id.to_string(),
            },
        )
        .unwrap();
    assert!(runtime.reconnect.contains_key(&id));
    let attempt = runtime.opening[&id].attempt;
    runtime.complete(Job {
        scope: runtime.scope.clone(),
        completion: Completion::Connected {
            id,
            attempt,
            result: Err(Error::Stale),
        },
    });
    assert!(runtime.reconnect.contains_key(&id));
    runtime
        .apply(&Command::DisconnectRemote {
            session_id: id.to_string(),
        })
        .unwrap();
    assert!(!runtime.reconnect.contains_key(&id));
}

#[tokio::test]
async fn remote_recovery_preserves_only_open_current_incarnations() {
    let (mut runtime, _root) = registry_fixture();
    let id = Uuid::now_v7();
    let incarnation = Uuid::now_v7();
    let remote = remote_session(id, incarnation);
    runtime.replace_remotes(vec![remote]);
    runtime.reconnect.insert(
        id,
        Reconnect {
            incarnation,
            failures: 0,
            next: Instant::now(),
        },
    );
    runtime.reconnect_views();
    assert!(runtime.opening[&id].commands.is_empty());
    runtime
        .apply(&Command::DisconnectRemote {
            session_id: id.to_string(),
        })
        .unwrap();
    assert!(!runtime.reconnect.contains_key(&id));
    assert!(!runtime.opening.contains_key(&id));
    runtime.reconnect.insert(
        id,
        Reconnect {
            incarnation,
            failures: 0,
            next: Instant::now(),
        },
    );
    runtime.replace_remotes(vec![remote_session(id, Uuid::now_v7())]);
    assert!(!runtime.reconnect.contains_key(&id));
}

#[tokio::test]
async fn catalog_replacement_cancels_inflight_automatic_recovery() {
    let (mut runtime, _root) = registry_fixture();
    let id = Uuid::now_v7();
    let incarnation = Uuid::now_v7();
    runtime.replace_remotes(vec![remote_session(id, incarnation)]);
    runtime.reconnect.insert(
        id,
        Reconnect {
            incarnation,
            failures: 0,
            next: Instant::now(),
        },
    );
    runtime.reconnect_views();
    let cancellation = runtime.opening[&id].cancellation.clone();
    runtime.replace_remotes(vec![remote_session(id, Uuid::now_v7())]);
    assert!(cancellation.is_cancelled());
    assert!(!runtime.opening.contains_key(&id));
    assert!(!runtime.reconnect.contains_key(&id));
}
