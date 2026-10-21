use super::*;
use std::sync::Arc;
use tokio::time::{Instant, timeout};

#[cfg(unix)]
const SHELL: &str = "/bin/sh";
#[cfg(windows)]
const SHELL: &str = "cmd.exe";
#[cfg(all(unix, feature = "host"))]
const ROOT: &str = "/";
#[cfg(all(windows, feature = "host"))]
const ROOT: &str = r"C:\";

struct TestRuntime {
    handle: RuntimeHandle,
    _storage: tempfile::TempDir,
}

impl TestRuntime {
    async fn new() -> Self {
        let storage = tempfile::tempdir().unwrap();
        let mut config =
            Config::isolated(&kodosi_os::fs::canonicalize(storage.path()).unwrap()).unwrap();
        config.initial_shell = Some(SHELL.to_owned());
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

    #[cfg(feature = "host")]
    async fn create(&self) -> (Uuid, Uuid) {
        let (_, mut events) = self.handle.observe().await.unwrap();
        let request = Uuid::now_v7().to_string();
        self.send(Command::CreateSession {
            request_id: request.clone(),
            name: "Test terminal".to_owned(),
            working_dir: None,
            mission_id: None,
            resume: None,
            branch: None,
            worktrees: None,
            command: None,
            agent: None,
            prompt: None,
        })
        .await;
        let entry = created(&mut events, &request).await;
        (
            Uuid::parse_str(entry["id"].as_str().unwrap()).unwrap(),
            Uuid::parse_str(entry["incarnationId"].as_str().unwrap()).unwrap(),
        )
    }
}

#[cfg(feature = "host")]
async fn created(events: &mut broadcast::Receiver<Event>, request: &str) -> Value {
    timeout(Duration::from_secs(10), async {
        let mut sessions = Value::Null;
        loop {
            let event = serde_json::to_value(events.recv().await.unwrap()).unwrap();
            assert_ne!(event["type"], "session.error", "{event}");
            if event["type"] == "sessions.snapshot" {
                sessions = event["sessions"].clone();
            }
            if event["type"] == "session.result" && event["requestId"] == request {
                return sessions
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|entry| entry["id"] == event["sessionId"])
                    .cloned()
                    .unwrap();
            }
        }
    })
    .await
    .unwrap()
}

impl Drop for TestRuntime {
    fn drop(&mut self) {
        let handle = self.handle.clone();
        tokio::spawn(async move {
            handle.shutdown().await;
        });
    }
}

#[cfg(all(unix, feature = "host"))]
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

#[cfg(feature = "host")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_terminal_on_a_new_branch_starts_in_its_own_folder_and_takes_it_away_unchanged() {
    async fn start(runtime: &TestRuntime, folder: &Path, branch: &str) -> Value {
        let (_, mut events) = runtime.handle.observe().await.unwrap();
        let request = Uuid::now_v7().to_string();
        runtime
            .send(Command::CreateSession {
                request_id: request.clone(),
                name: branch.to_owned(),
                working_dir: Some(folder.to_string_lossy().into_owned()),
                mission_id: None,
                resume: None,
                branch: Some(branch.to_owned()),
                worktrees: None,
                command: None,
                agent: None,
                prompt: None,
            })
            .await;
        created(&mut events, &request).await
    }
    let root = tempfile::tempdir().unwrap();
    let folder = kodosi_os::fs::canonicalize(root.path())
        .unwrap()
        .join("Project");
    std::fs::create_dir(&folder).unwrap();
    for arguments in [
        vec!["init", "--quiet"],
        vec![
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            "First",
        ],
    ] {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(&folder)
            .args(arguments)
            .status()
            .unwrap();
        assert!(status.success());
    }
    let runtime = TestRuntime::new().await;
    let entry = start(&runtime, &folder, "trial").await;
    let branch_folder = folder.with_file_name("Project-trial");
    assert_eq!(
        entry["workingDir"],
        branch_folder.to_string_lossy().as_ref()
    );
    assert_eq!(entry["repository"], true);
    assert!(branch_folder.join(".git").exists());

    runtime
        .send(Command::CloseSession {
            request_id: Uuid::now_v7().to_string(),
            session_id: entry["id"].as_str().unwrap().to_owned(),
            expected_runtime_incarnation_id: entry["incarnationId"].as_str().unwrap().to_owned(),
        })
        .await;
    timeout(Duration::from_secs(12), async {
        while branch_folder.exists() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();

    start(&runtime, &folder, "open").await;
    let open_folder = folder.with_file_name("Project-open");
    assert!(open_folder.exists());
    runtime.shutdown().await;
    assert!(!open_folder.exists());
    drop(runtime);
}

#[cfg(all(unix, feature = "host"))]
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
        .send(Command::CloseSession {
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

#[cfg(all(unix, feature = "host"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_notifications_do_not_require_a_mounted_view() {
    let runtime = TestRuntime::new().await;
    let (id, incarnation) = runtime.create().await;
    let (_, mut events) = runtime.handle.observe().await.unwrap();
    let subscription = runtime.handle.subscribe_terminal(id).await.unwrap();
    runtime
        .handle
        .write_input(
            id,
            incarnation,
            subscription.connection_id,
            Bytes::from_static(b"sleep 0.2; printf '\\033]9;BACKGROUND_NOTICE\\007'\n"),
        )
        .await
        .unwrap();
    runtime
        .handle
        .unsubscribe_terminal(id, subscription.connection_id)
        .await;
    drop(subscription);
    timeout(Duration::from_secs(3), async {
        loop {
            if let crate::protocol::EventBody::TerminalNotification {
                session_id,
                runtime_incarnation_id,
                body,
                ..
            } = events.recv().await.unwrap().event
            {
                assert_eq!(session_id, id.to_string());
                assert_eq!(runtime_incarnation_id, incarnation.to_string());
                assert_eq!(body, "BACKGROUND_NOTICE");
                break;
            }
        }
    })
    .await
    .unwrap();
    runtime.shutdown().await;
    drop(runtime);
}

#[cfg(all(unix, feature = "host"))]
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
    let mut config =
        Config::isolated(&kodosi_os::fs::canonicalize(storage.path()).unwrap()).unwrap();
    config.initial_shell = Some("/bin/sh".to_owned());
    config.oidc_issuer = format!("http://{address}");
    let runtime = TestRuntime {
        handle: start(config).await.unwrap(),
        _storage: storage,
    };
    let (session, incarnation) = runtime.create().await;
    let mut terminal = runtime.handle.subscribe_terminal(session).await.unwrap();
    runtime
        .send(Command::Login {
            browser: None,
            request_id: Uuid::now_v7().to_string(),
        })
        .await;
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
async fn a_stale_epoch_creates_nothing_and_hears_why() {
    let runtime = TestRuntime::new().await;
    let (snapshot, mut events) = runtime.handle.observe().await.unwrap();
    let scope = &snapshot[0];
    let stale = Uuid::now_v7().to_string();
    runtime
        .handle
        .try_send(CommandEnvelope {
            account_user_id: scope.account_user_id.clone(),
            account_epoch: scope.account_epoch + 1,
            command: Command::CreateSession {
                request_id: stale.clone(),
                name: "stale".to_owned(),
                working_dir: None,
                mission_id: None,
                resume: None,
                branch: None,
                worktrees: None,
                command: None,
                agent: None,
                prompt: None,
            },
        })
        .unwrap();
    runtime
        .handle
        .try_send(CommandEnvelope {
            account_user_id: scope.account_user_id.clone(),
            account_epoch: scope.account_epoch,
            command: Command::ListSessions {
                request_id: Uuid::now_v7().to_string(),
            },
        })
        .unwrap();
    timeout(Duration::from_secs(2), async {
        let mut refused = false;
        loop {
            let event = events.recv().await.unwrap();
            if event.event.kind() == "session.error" {
                assert_eq!(event.event.request_id(), Some(stale.as_str()));
                assert_eq!(event.recipient, Some(runtime.handle.consumer()));
                refused = true;
            }
            if event.event.kind() == "sessions.snapshot" {
                assert!(refused);
                break;
            }
        }
    })
    .await
    .unwrap();
    let snapshot = runtime.handle.snapshot().await.unwrap();
    let sessions = snapshot
        .iter()
        .find(|event| event.event.kind() == "sessions.snapshot")
        .unwrap();
    assert_eq!(
        serde_json::to_value(sessions).unwrap()["sessions"],
        serde_json::json!([])
    );
    runtime.shutdown().await;
    drop(runtime);
}

#[cfg(not(feature = "host"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_viewer_says_that_terminals_and_conversations_stay_on_a_computer() {
    let runtime = TestRuntime::new().await;
    let (_, mut events) = runtime.handle.observe().await.unwrap();
    let create = Uuid::now_v7().to_string();
    let discover = Uuid::now_v7().to_string();
    runtime
        .send(Command::CreateSession {
            request_id: create.clone(),
            name: "Viewer".to_owned(),
            working_dir: None,
            mission_id: None,
            resume: None,
            branch: None,
            worktrees: None,
            command: None,
            agent: None,
            prompt: None,
        })
        .await;
    runtime
        .send(Command::DiscoverConversations {
            request_id: discover.clone(),
            provider: crate::provider::Provider::Claude,
            working_directory: None,
            cursor: None,
            limit: None,
            max_bytes: None,
        })
        .await;
    let mut refused = BTreeMap::new();
    timeout(Duration::from_secs(5), async {
        while refused.len() < 2 {
            let event = serde_json::to_value(events.recv().await.unwrap()).unwrap();
            if matches!(
                event["type"].as_str(),
                Some("session.error" | "provider.error")
            ) {
                refused.insert(event["requestId"].as_str().unwrap().to_owned(), event);
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(refused[&create]["operation"], "session.create");
    assert_eq!(
        refused[&create]["message"],
        "This device cannot start terminals."
    );
    assert_eq!(
        refused[&discover]["message"],
        "Open agent conversations on the computer that runs the agent."
    );
    runtime.shutdown().await;
    drop(runtime);
}

#[cfg(feature = "host")]
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

#[cfg(feature = "host")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_retry_is_exact_and_does_not_launch_a_second_process() {
    let runtime = TestRuntime::new().await;
    let command = Command::CreateSession {
        request_id: Uuid::now_v7().to_string(),
        name: "same".to_owned(),
        working_dir: None,
        mission_id: None,
        resume: None,
        branch: None,
        worktrees: None,
        command: None,
        agent: None,
        prompt: None,
    };
    runtime.send(command.clone()).await;
    runtime.send(command.clone()).await;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let events = runtime.handle.snapshot().await.unwrap();
        let sessions = serde_json::to_value(
            events
                .iter()
                .find(|event| event.event.kind() == "sessions.snapshot")
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
            if events.recv().await.unwrap().event.kind() == "session.result" {
                break;
            }
        }
    })
    .await
    .unwrap();
    let snapshot = runtime.handle.snapshot().await.unwrap();
    let sessions = snapshot
        .iter()
        .find(|event| event.event.kind() == "sessions.snapshot")
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

#[cfg(feature = "host")]
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
    let config = Config::isolated(&kodosi_os::fs::canonicalize(root.path()).unwrap()).unwrap();
    let network = BackendClient::new(BackendConfig {
        api_url: config.backend_url.clone(),
        issuer: config.oidc_issuer.clone(),
        client_id: config.oidc_client_id.clone(),
        scopes: config.oidc_scopes.clone(),
        data_root: config.data_root.clone(),
        secret_service: config.secret_service.clone(),
        isolated: config.isolated,
    })
    .unwrap();
    let scope = Scope {
        user: None,
        epoch: 0,
        network_generation: network.generation(),
    };
    let (events, _) = broadcast::channel(256);
    let (changes, _) = mpsc::channel(128);
    #[cfg(feature = "host")]
    let launcher = Arc::new(crate::agents::Launcher::load(
        &config.data_root,
        &config.home,
        &commands::shell(&config),
    ));
    (
        Runtime {
            #[cfg(feature = "host")]
            dosi: crate::dosi::Engine::load(
                &config.data_root,
                &config.home,
                Arc::new(crate::dosi::Installed {
                    launcher: Arc::clone(&launcher),
                    shell: commands::shell(&config),
                    home: config.home.clone(),
                }),
            )
            .0,
            #[cfg(feature = "host")]
            launcher,
            #[cfg(feature = "host")]
            config,
            network,
            events,
            scope,
            #[cfg(feature = "host")]
            local: HashMap::new(),
            #[cfg(feature = "host")]
            creating: HashMap::new(),
            listed: HashMap::new(),
            remotes: HashMap::new(),
            connections: HashMap::new(),
            opening: HashMap::new(),
            views: HashMap::new(),
            reconnect: HashMap::new(),
            #[cfg(feature = "host")]
            publishing: BTreeSet::new(),
            #[cfg(feature = "host")]
            retiring: BTreeSet::new(),
            unpublishing: BTreeSet::new(),
            administering: BTreeSet::new(),
            jobs: JoinSet::new(),
            changes,
            initializing: false,
            auth_pending: false,
            latest_auth: None,
            cached_events: BTreeMap::new(),
            answers: std::sync::Mutex::new(HashMap::new()),
            host: Some(crate::HostKind::Background),
            visible: HashMap::new(),
            attention: BTreeSet::new(),
            activity: HashMap::new(),
            room_latest: HashMap::new(),
            room_seen: None,
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
        mission_id: None,
        mission_name: None,
        shared_with: vec![],
        online: true,
        status: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn login_failure_after_retiring_account_is_visible_and_retryable() {
    let runtime = TestRuntime::new().await;
    for _ in 0..2 {
        let (_, mut events) = runtime.handle.observe().await.unwrap();
        runtime
            .send(Command::Login {
                browser: None,
                request_id: Uuid::now_v7().to_string(),
            })
            .await;
        let error = timeout(Duration::from_secs(5), async {
            loop {
                let event = events.recv().await.unwrap();
                if event.event.kind() == "auth.error" {
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
async fn a_refresh_of_the_runtime_fails_quietly_and_a_request_hears_its_own_error() {
    let (mut runtime, _root) = registry_fixture();
    let mut events = runtime.events.subscribe();
    let (scope, generation) = (runtime.scope.clone(), runtime.network.generation());
    let unreachable = || Err(Error::Other("The server is not reachable.".to_owned()));
    let refresh = Command::RefreshFriends {
        request_id: Uuid::now_v7().to_string(),
    };
    runtime.network_completed(&scope, &refresh, false, generation, unreachable());
    assert!(events.try_recv().is_err());
    let request = Uuid::now_v7().to_string();
    let asked = Command::RefreshFriends {
        request_id: request.clone(),
    };
    let client = Uuid::now_v7();
    runtime
        .expect_answer(&asked, client, CancellationToken::new())
        .unwrap();
    runtime.network_completed(&scope, &asked, true, generation, unreachable());
    let error = events.try_recv().unwrap();
    assert_eq!(error.recipient, Some(client));
    let error = serde_json::to_value(error).unwrap();
    assert_eq!(error["type"], "friends.error");
    assert_eq!(error["requestId"], request.as_str());
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
        clients: watch::channel(0).1,
        input_budget: Arc::new(Semaphore::new(MAX_QUEUED_INPUT_BYTES)),
        consumer: Arc::new(Consumer::default()),
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
async fn observation_retains_a_finishing_sign_in_until_its_answer() {
    let (mut runtime, _root) = registry_fixture();
    let pending = json!({"type":"auth.finalizing"});
    runtime.network_event(BackendEvent {
        generation: runtime.network.generation(),
        user_id: None,
        event: pending.clone(),
    });
    assert_eq!(runtime.auth_event(), pending);
    runtime.network_event(BackendEvent {
        generation: runtime.network.generation() - 1,
        user_id: None,
        event: json!({"type":"auth.required", "reason":"signedOut"}),
    });
    assert_eq!(runtime.auth_event(), pending);
    runtime.network_event(BackendEvent { generation: runtime.network.generation(), user_id: None, event: json!({"type":"auth.error", "requestId":Uuid::now_v7().to_string(), "operation":"auth.login.start", "message":"Code expired"}) });
    assert_eq!(runtime.auth_event()["type"], "auth.required");
}

#[cfg(feature = "host")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_completion_publishes_catalog_before_its_result_and_retry_checks_directory() {
    let runtime = TestRuntime::new().await;
    let (_, mut events) = runtime.handle.observe().await.unwrap();
    let request_id = Uuid::now_v7().to_string();
    let room_id = Uuid::now_v7().to_string();
    let command = Command::CreateSession {
        request_id: request_id.clone(),
        name: "ordered".to_owned(),
        working_dir: None,
        mission_id: Some(room_id.clone()),
        resume: None,
        branch: None,
        worktrees: None,
        command: None,
        agent: None,
        prompt: None,
    };
    runtime.send(command).await;
    assert_eq!(
        created(&mut events, &request_id).await["missionId"],
        room_id
    );
    runtime
        .send(Command::CreateSession {
            request_id: request_id.clone(),
            name: "ordered".to_owned(),
            working_dir: Some(ROOT.to_owned()),
            mission_id: Some(room_id.clone()),
            resume: None,
            branch: None,
            worktrees: None,
            command: None,
            agent: None,
            prompt: None,
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
    let consumer = Uuid::now_v7();
    let lifetime = CancellationToken::new();
    runtime
        .apply_for_consumer(
            &Command::OpenRemote {
                request_id: Uuid::now_v7().to_string(),
                session_id: id.to_string(),
            },
            consumer,
            lifetime,
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
        .apply_for_consumer(
            &Command::DisconnectRemote {
                session_id: id.to_string(),
                request_id: Uuid::now_v7().to_string(),
            },
            consumer,
            CancellationToken::new(),
        )
        .unwrap();
    assert!(!runtime.reconnect.contains_key(&id));
}

#[tokio::test]
async fn a_failed_reconnect_waits_longer_each_time_and_only_a_final_answer_ends_it() {
    let (mut runtime, _root) = registry_fixture();
    let id = Uuid::now_v7();
    runtime.replace_remotes(vec![remote_session(id, Uuid::now_v7())]);
    runtime
        .apply_for_consumer(
            &Command::OpenRemote {
                request_id: Uuid::now_v7().to_string(),
                session_id: id.to_string(),
            },
            Uuid::now_v7(),
            CancellationToken::new(),
        )
        .unwrap();
    let mut waits = Vec::new();
    for result in [
        Error::Other("Your sign-in has expired. Sign in again.".to_owned()),
        Error::Other("The server did not respond.".to_owned()),
    ] {
        let attempt = runtime.opening[&id].attempt;
        let before = Instant::now();
        runtime.complete(Job {
            scope: runtime.scope.clone(),
            completion: Completion::Connected {
                id,
                attempt,
                result: Err(result),
            },
        });
        waits.push(runtime.reconnect[&id].next - before);
        runtime.reconnect.get_mut(&id).unwrap().next = Instant::now();
        runtime.reconnect_views();
    }
    assert_eq!(runtime.reconnect[&id].failures, 2);
    assert!(waits[1] > waits[0]);

    let attempt = runtime.opening[&id].attempt;
    runtime.complete(Job {
        scope: runtime.scope.clone(),
        completion: Completion::Connected {
            id,
            attempt,
            result: Err(Error::Invalid("This terminal is not available.".to_owned())),
        },
    });
    assert!(!runtime.reconnect.contains_key(&id));
}

fn first_snapshot() -> crate::network::RemoteUpdate {
    let mut terminal =
        ghostty_vt::Terminal::new(20, 4, ghostty_vt::TerminalPolicy::default()).unwrap();
    let snapshot = terminal
        .snapshot(crate::terminal::snapshot_limits(u64::MAX))
        .unwrap();
    crate::network::RemoteUpdate::Checkpoint {
        checkpoint: crate::terminal::Checkpoint::new(snapshot, false).unwrap(),
        next_sequence: 0,
    }
}

type RemoteState = (ConnectionState, SessionStatus, Option<String>);

fn remote_state(runtime: &Runtime, id: Uuid) -> RemoteState {
    let entry = &runtime.remotes[&id];
    (entry.connection_state, entry.status, entry.message.clone())
}

fn connected(runtime: &mut Runtime, id: Uuid, result: Result<RemoteConnection>) {
    let attempt = runtime.opening[&id].attempt;
    runtime.complete(Job {
        scope: runtime.scope.clone(),
        completion: Completion::Connected {
            id,
            attempt,
            result,
        },
    });
}

async fn open_view(
    runtime: &mut Runtime,
    remote: &RemoteSession,
) -> (
    Subscription,
    mpsc::Receiver<crate::network::TestRemoteRequest>,
    mpsc::Sender<crate::network::RemoteUpdate>,
) {
    let id = remote.id;
    runtime.replace_remotes(vec![remote.clone()]);
    runtime
        .apply_for_consumer(
            &Command::OpenRemote {
                request_id: Uuid::now_v7().to_string(),
                session_id: id.to_string(),
            },
            Uuid::now_v7(),
            CancellationToken::new(),
        )
        .unwrap();
    let (connection, mut requests, updates) =
        crate::network::test_remote_connection(remote.clone());
    connected(runtime, id, Ok(connection));
    assert_eq!(
        remote_state(runtime, id),
        (ConnectionState::Connected, SessionStatus::Running, None)
    );
    let view = runtime.connections[&id].subscribe();
    requests.recv().await.unwrap();
    updates.send(first_snapshot()).await.unwrap();
    (view.await.unwrap(), requests, updates)
}

async fn lose_link(
    runtime: &mut Runtime,
    remote: &RemoteSession,
    updates: &mpsc::Sender<crate::network::RemoteUpdate>,
    reason: &str,
) {
    let id = remote.id;
    updates
        .send(crate::network::RemoteUpdate::Closed {
            reason: "link lost".to_owned(),
        })
        .await
        .unwrap();
    for _ in 0..400 {
        if runtime.connections[&id].is_interrupted() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let instance = runtime.connections[&id].instance_id;
    runtime.remote_lost(id, remote.incarnation_id, instance, reason.to_owned(), true);
}

fn reconnect(
    runtime: &mut Runtime,
    remote: &RemoteSession,
) -> mpsc::Sender<crate::network::RemoteUpdate> {
    runtime.reconnect.get_mut(&remote.id).unwrap().next = Instant::now();
    runtime.reconnect_views();
    let (again, _requests, updates) = crate::network::test_remote_connection(remote.clone());
    connected(runtime, remote.id, Ok(again));
    updates
}

#[tokio::test]
async fn an_open_view_stays_connected_and_reports_reconnecting_while_its_link_is_lost() {
    let (mut runtime, _root) = registry_fixture();
    let remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    let id = remote.id;
    let (_view, _requests, updates) = open_view(&mut runtime, &remote).await;

    lose_link(
        &mut runtime,
        &remote,
        &updates,
        "The connection has closed.",
    )
    .await;
    let reconnecting = (
        ConnectionState::Connected,
        SessionStatus::Reconnecting,
        None,
    );
    assert_eq!(remote_state(&runtime, id), reconnecting);

    runtime.reconnect.get_mut(&id).unwrap().next = Instant::now();
    runtime.reconnect_views();
    assert_eq!(remote_state(&runtime, id), reconnecting);
    connected(
        &mut runtime,
        id,
        Err(Error::Other("The server did not respond.".to_owned())),
    );
    assert_eq!(remote_state(&runtime, id), reconnecting);
    assert!(runtime.reconnect[&id].next > Instant::now());
    runtime.replace_remotes(vec![remote.clone()]);
    assert!(runtime.reconnect[&id].next <= Instant::now());

    let _updates = reconnect(&mut runtime, &remote);
    assert_eq!(
        remote_state(&runtime, id),
        (ConnectionState::Connected, SessionStatus::Running, None)
    );
}

#[tokio::test]
async fn a_view_says_that_input_was_not_sent_until_the_next_input() {
    let (mut runtime, _root) = registry_fixture();
    let remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    let id = remote.id;
    let (_view, _requests, updates) = open_view(&mut runtime, &remote).await;
    let notice = Some(INPUT_NOT_SENT.to_owned());

    let reason = format!("The connection has closed. {INPUT_NOT_SENT}");
    lose_link(&mut runtime, &remote, &updates, &reason).await;
    assert_eq!(
        remote_state(&runtime, id),
        (
            ConnectionState::Connected,
            SessionStatus::Reconnecting,
            notice.clone()
        )
    );
    let _updates = reconnect(&mut runtime, &remote);
    assert_eq!(
        remote_state(&runtime, id),
        (ConnectionState::Connected, SessionStatus::Running, notice)
    );
    runtime.clear_input_notice(id);
    assert_eq!(
        remote_state(&runtime, id),
        (ConnectionState::Connected, SessionStatus::Running, None)
    );
}

fn host_notice(runtime: &mut Runtime, remote: &RemoteSession, online: bool) {
    let generation = runtime.network.generation();
    runtime.network_event(BackendEvent {
        generation,
        user_id: None,
        event: json!({"type":"network.terminal","sessionId":remote.id,
            "incarnationId":remote.incarnation_id,"online":online,"status":null}),
    });
}

#[tokio::test]
async fn a_view_whose_computer_goes_offline_says_so_and_connects_again_when_it_is_back() {
    let (mut runtime, _root) = registry_fixture();
    let remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    let id = remote.id;
    let (_view, _requests, updates) = open_view(&mut runtime, &remote).await;

    lose_link(&mut runtime, &remote, &updates, "The host is offline.").await;
    runtime.remotes.get_mut(&id).unwrap().program_status = Some(crate::terminal::ProgramStatus {
        state: crate::terminal::ProgramState::Working,
        kind: None,
        progress: None,
        title: None,
        message: None,
    });
    runtime.publish_catalog();
    host_notice(&mut runtime, &remote, false);
    assert_eq!(
        remote_state(&runtime, id),
        (ConnectionState::Connected, SessionStatus::Offline, None)
    );
    assert_eq!(runtime.remotes[&id].program_status, None);
    assert!(!runtime.attention.contains(&id.to_string()));
    let (reply, typed) = oneshot::channel();
    runtime.input(
        id,
        remote.incarnation_id,
        Uuid::now_v7(),
        Bytes::from_static(b"ls\r"),
        Arc::new(Semaphore::new(3))
            .try_acquire_many_owned(3)
            .unwrap(),
        Some(reply),
    );
    assert_eq!(
        typed.await.unwrap().unwrap_err().to_string(),
        "Other computer is offline."
    );

    runtime.reconnect.get_mut(&id).unwrap().next = Instant::now();
    runtime.reconnect_views();
    assert!(!runtime.opening.contains_key(&id));

    runtime.reconnect.get_mut(&id).unwrap().next = Instant::now() + Duration::from_mins(1);
    host_notice(&mut runtime, &remote, true);
    assert_eq!(remote_state(&runtime, id).1, SessionStatus::Reconnecting);
    runtime.reconnect_views();
    assert!(runtime.opening.contains_key(&id));
    let (again, _requests, _updates) = crate::network::test_remote_connection(remote.clone());
    connected(&mut runtime, id, Ok(again));
    assert_eq!(
        remote_state(&runtime, id),
        (ConnectionState::Connected, SessionStatus::Running, None)
    );
}

#[tokio::test]
async fn opening_a_terminal_whose_computer_is_offline_waits_for_the_computer() {
    let (mut runtime, _root) = registry_fixture();
    let mut remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    remote.online = false;
    let id = remote.id;
    runtime.replace_remotes(vec![remote.clone()]);
    assert_eq!(
        remote_state(&runtime, id),
        (ConnectionState::Offline, SessionStatus::Offline, None)
    );
    runtime
        .apply_for_consumer(
            &Command::OpenRemote {
                request_id: Uuid::now_v7().to_string(),
                session_id: id.to_string(),
            },
            Uuid::now_v7(),
            CancellationToken::new(),
        )
        .unwrap();
    assert!(!runtime.opening.contains_key(&id));
    assert!(runtime.reconnect.contains_key(&id));
    let close = Command::CloseSession {
        request_id: Uuid::now_v7().to_string(),
        session_id: id.to_string(),
        expected_runtime_incarnation_id: remote.incarnation_id.to_string(),
    };
    assert_eq!(
        runtime.apply(&close).unwrap_err().to_string(),
        "Other computer is offline."
    );

    remote.online = true;
    runtime.replace_remotes(vec![remote]);
    assert_eq!(
        remote_state(&runtime, id),
        (ConnectionState::Offline, SessionStatus::Running, None)
    );
    runtime.reconnect_views();
    assert!(runtime.opening.contains_key(&id));
}

#[tokio::test]
async fn remote_demand_is_released_only_by_its_consumer() {
    let (mut runtime, _root) = registry_fixture();
    let id = Uuid::now_v7();
    runtime.replace_remotes(vec![remote_session(id, Uuid::now_v7())]);
    let desktop = Uuid::now_v7();
    let cli = Uuid::now_v7();
    let cli_lifetime = CancellationToken::new();
    for (consumer, lifetime) in [
        (desktop, CancellationToken::new()),
        (cli, cli_lifetime.clone()),
    ] {
        runtime
            .apply_for_consumer(
                &Command::OpenRemote {
                    request_id: Uuid::now_v7().to_string(),
                    session_id: id.to_string(),
                },
                consumer,
                lifetime,
            )
            .unwrap();
    }
    runtime
        .apply_for_consumer(
            &Command::DisconnectRemote {
                session_id: id.to_string(),
                request_id: Uuid::now_v7().to_string(),
            },
            desktop,
            CancellationToken::new(),
        )
        .unwrap();
    assert!(runtime.reconnect.contains_key(&id));
    assert_eq!(runtime.views[&id].len(), 1);
    assert!(runtime.opening.contains_key(&id));
    cli_lifetime.cancel();
    runtime.prune_views();
    assert!(!runtime.views.contains_key(&id));
    assert!(!runtime.reconnect.contains_key(&id));
    assert!(!runtime.opening.contains_key(&id));
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
            connected: None,
        },
    );
    runtime.reconnect_views();
    assert_eq!(runtime.opening[&id].commands, [] as [Command; 0]);
    runtime.views.insert(id, HashMap::new());
    runtime.prune_views();
    assert!(!runtime.reconnect.contains_key(&id));
    assert!(!runtime.opening.contains_key(&id));
    runtime.reconnect.insert(
        id,
        Reconnect {
            incarnation,
            failures: 0,
            next: Instant::now(),
            connected: None,
        },
    );
    runtime.replace_remotes(vec![remote_session(id, Uuid::now_v7())]);
    assert!(!runtime.reconnect.contains_key(&id));
}

#[tokio::test]
async fn sessions_hosted_by_an_earlier_run_are_ended_instead_of_listed() {
    let (mut runtime, _root) = registry_fixture();
    runtime.network.assume_identity(crate::network::Identity {
        user_id: "owner".to_owned(),
        device_id: "this-device".to_owned(),
        display_name: "You".to_owned(),
        enrolled: true,
    });
    let mut orphan = remote_session(Uuid::now_v7(), Uuid::now_v7());
    orphan.host_device_id = "this-device".to_owned();
    orphan.owner_user_id = "owner".to_owned();
    let other = remote_session(Uuid::now_v7(), Uuid::now_v7());
    runtime.replace_remotes(vec![orphan.clone(), other.clone()]);
    assert!(!runtime.remotes.contains_key(&orphan.id));
    assert!(runtime.remotes.contains_key(&other.id));
    assert!(runtime.unpublishing.contains(&orphan.id));
    assert!(!runtime.unpublishing.contains(&other.id));
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
            connected: None,
        },
    );
    runtime.reconnect_views();
    let cancellation = runtime.opening[&id].cancellation.clone();
    runtime.replace_remotes(vec![remote_session(id, Uuid::now_v7())]);
    assert!(cancellation.is_cancelled());
    assert!(!runtime.opening.contains_key(&id));
    assert!(!runtime.reconnect.contains_key(&id));
}

#[tokio::test]
async fn a_terminal_that_needs_its_person_waits_for_a_look_unless_someone_watches() {
    use crate::terminal::{ProgramState, ProgramStatus};
    let (mut runtime, _root) = registry_fixture();
    let mut events = runtime.events.subscribe();
    let id = Uuid::now_v7();
    runtime.replace_remotes(vec![remote_session(id, Uuid::now_v7())]);
    let show = |runtime: &mut Runtime, state| {
        let entry = runtime.remotes.get_mut(&id).unwrap();
        entry.is_owner = true;
        entry.program_status = Some(ProgramStatus {
            state,
            kind: None,
            progress: None,
            title: None,
            message: None,
        });
        runtime.publish_catalog();
        runtime.entries()[0].attention
    };
    assert!(!show(&mut runtime, ProgramState::Working));
    assert!(show(&mut runtime, ProgramState::Done));
    let watching = CancellationToken::new();
    runtime.see(
        Uuid::now_v7(),
        watching.clone(),
        BTreeSet::from([id.to_string()]),
    );
    assert!(!runtime.entries()[0].attention);
    assert!(!show(&mut runtime, ProgramState::Working));
    assert!(!show(&mut runtime, ProgramState::Done));
    watching.cancel();
    assert!(!show(&mut runtime, ProgramState::Working));
    assert!(show(&mut runtime, ProgramState::Blocked));
    let mut notices = 0;
    while let Ok(event) = events.try_recv() {
        if event.event.kind() == "term.attention" {
            notices += 1;
        }
    }
    assert_eq!(notices, 2);
}

#[tokio::test]
async fn a_room_stays_unread_until_its_person_sees_what_others_wrote() {
    let (mut runtime, _root) = registry_fixture();
    runtime.scope.user = Some("me".to_owned());
    let room = Uuid::now_v7().to_string();
    runtime.cached_events.insert(
        "missions.snapshot".to_owned(),
        json!({"type":"missions.snapshot","missions":[{"id":room,"name":"Room","ownerUserId":"me"}],"invitations":[]}),
    );
    let message = |sequence: u64, author: &str, agent: Option<&str>| {
        json!({"id":Uuid::now_v7().to_string(),"sequence":sequence,"authorId":author,"authorName":author,
            "agent":agent,"terminalId":null,"text":"hello","createdAt":"2026-10-17T00:00:00Z"})
    };
    let unread = |runtime: &Runtime| {
        runtime.cached_events["missions.snapshot"]["missions"][0]["unread"] == true
    };
    runtime.room_changed(&json!({"roomId":room,"messages":[message(1, "friend", None)]}));
    assert!(!unread(&runtime));
    runtime.room_changed(&json!({"roomId":room,"messages":[message(2, "me", None)]}));
    assert!(!unread(&runtime));
    runtime.room_changed(&json!({"roomId":room,"messages":[message(3, "me", Some("claude"))]}));
    assert!(unread(&runtime));
    runtime.see_room(&room, 3).unwrap();
    assert!(!unread(&runtime));
    runtime.room_seen = None;
    assert_eq!(runtime.seen_rooms().unwrap()[&room], 3);
}

#[tokio::test]
async fn a_terminal_shows_the_status_of_its_host_while_nobody_here_has_it_open() {
    use crate::terminal::{BlockedKind, ProgramState, TerminalMetadata};
    let (mut runtime, _root) = registry_fixture();
    let (id, incarnation) = (Uuid::now_v7(), Uuid::now_v7());
    let remote = RemoteSession {
        status: Some(TerminalStatus {
            program: Some("claude".to_owned()),
            state: Some(ProgramState::Blocked),
            kind: Some(BlockedKind::Permission),
        }),
        ..remote_session(id, incarnation)
    };
    let shown = |runtime: &Runtime| {
        let entry = &runtime.remotes[&id];
        (
            entry.program.clone(),
            entry.program_status.as_ref().map(|status| status.state),
            entry
                .program_status
                .as_ref()
                .and_then(|status| status.message.clone()),
        )
    };
    runtime.replace_remotes(vec![remote.clone()]);
    assert_eq!(
        shown(&runtime),
        (Some("claude".to_owned()), Some(ProgramState::Blocked), None)
    );

    let (connection, _requests, _updates) = crate::network::test_remote_connection(remote);
    runtime.connections.insert(
        id,
        RemoteTerminal::spawn(connection, runtime.changes.clone()),
    );
    let instance_id = runtime.connections[&id].instance_id;
    runtime.change(SessionChange::Metadata {
        id,
        incarnation,
        instance_id,
        metadata: TerminalMetadata {
            connected_users: vec![],
            directory: None,
            title: None,
            program: Some("claude".to_owned()),
            status: Some(crate::terminal::ProgramStatus {
                state: ProgramState::Working,
                kind: None,
                progress: Some(40),
                title: None,
                message: Some("Building".to_owned()),
            }),
            context: None,
        },
    });
    let generation = runtime.network.generation();
    let notice = |online: bool, state: &str| BackendEvent {
        generation,
        user_id: None,
        event: json!({"type":"network.terminal","sessionId":id,"incarnationId":incarnation,
            "online":online,"status":{"program":"claude","state":state}}),
    };
    runtime.network_event(notice(true, "done"));
    let open = (
        Some("claude".to_owned()),
        Some(ProgramState::Working),
        Some("Building".to_owned()),
    );
    assert_eq!(shown(&runtime), open);

    runtime.change(SessionChange::RemoteClosed {
        id,
        incarnation,
        instance_id,
        reason: "Closed here.".to_owned(),
    });
    assert_eq!(
        shown(&runtime),
        (Some("claude".to_owned()), Some(ProgramState::Done), None)
    );
    runtime.network_event(notice(false, "done"));
    assert_eq!(shown(&runtime), (None, None, None));
    assert_eq!(runtime.remotes[&id].status, SessionStatus::Offline);
}
