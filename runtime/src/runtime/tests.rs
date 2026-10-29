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
                computer: None,
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
async fn a_terminal_on_a_new_branch_keeps_its_folder_until_it_closes_unchanged() {
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
    assert!(open_folder.exists());
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
            computer: None,
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
            computer: None,
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
            computer: None,
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
            recent: crate::folders::Recent::load(&config.data_root),
            #[cfg(feature = "host")]
            restore: restore::Restore::load(&config.data_root),
            #[cfg(feature = "host")]
            mentions: messages::Mentions::default(),
            #[cfg(feature = "host")]
            person: Some("Ana".to_owned()),
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
            starting: HashMap::new(),
            connections: HashMap::new(),
            opening: HashMap::new(),
            views: HashMap::new(),
            reconnect: HashMap::new(),
            link_connects: 0,
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
        hidden: false,
        restored_from: None,
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
async fn an_app_can_show_a_listed_terminal_of_another_computer_that_is_not_open() {
    let (mut runtime, _root) = registry_fixture();
    let remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    runtime.replace_remotes(vec![remote.clone()]);
    let mut events = runtime.events.subscribe();
    let show = |id: Uuid| Command::Show {
        request_id: Uuid::now_v7().to_string(),
        session_id: id.to_string(),
    };
    runtime.apply(&show(remote.id)).unwrap();
    let told = std::iter::from_fn(|| events.try_recv().ok())
        .map(|event| serde_json::to_value(event).unwrap())
        .collect::<Vec<_>>();
    assert!(
        told.iter()
            .any(|event| event["type"] == "session.show"
                && event["sessionId"] == remote.id.to_string()),
        "{told:?}"
    );
    assert!(matches!(
        runtime.apply(&show(Uuid::now_v7())),
        Err(Error::NotFound)
    ));
}

#[tokio::test]
async fn a_devices_refresh_says_again_which_computers_are_online() {
    let (mut runtime, _root) = registry_fixture();
    let generation = runtime.network.generation();
    runtime.network_event(BackendEvent {
        generation,
        user_id: None,
        event: json!({"type":"devices.hosts","deviceIds":["laptop"]}),
    });
    let mut events = runtime.events.subscribe();
    let scope = runtime.scope.clone();
    let refresh = Command::RefreshDevices {
        request_id: Uuid::now_v7().to_string(),
    };
    let list = json!({"type":"devices.list","selfDeviceId":"desk","localDeviceEnrolled":true,"devices":[]});
    let reply = BackendReply {
        generation,
        user_id: None,
        events: vec![list],
        answered_later: false,
    };
    runtime.network_completed(&scope, &refresh, false, generation, Ok(reply));
    let told = std::iter::from_fn(|| events.try_recv().ok())
        .map(|event| serde_json::to_value(event).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(told.len(), 2, "{told:?}");
    assert_eq!(told[0]["type"], "devices.list");
    assert_eq!(told[1]["deviceIds"], json!(["laptop"]));
}

#[tokio::test]
async fn a_terminal_started_on_another_computer_is_answered_when_it_shows_here() {
    let (mut runtime, _root) = registry_fixture();
    let mut events = runtime.events.subscribe();
    let request = Uuid::now_v7().to_string();
    let start = Command::CreateSession {
        request_id: request.clone(),
        name: "Terminal".to_owned(),
        working_dir: None,
        mission_id: None,
        resume: None,
        branch: None,
        worktrees: None,
        command: None,
        agent: None,
        prompt: None,
    };
    runtime
        .expect_answer(&start, Uuid::now_v7(), CancellationToken::new())
        .unwrap();
    let other = json!({"type":"session.result","requestId":Uuid::now_v7().to_string(),"operation":"session.create"});
    assert!(runtime.asked(&start, other).is_err());
    let id = Uuid::now_v7();
    runtime
        .asked(&start, json!({"type":"session.result","requestId":request,"operation":"session.create","sessionId":id}))
        .unwrap();
    let mut offline = remote_session(id, Uuid::now_v7());
    offline.online = false;
    runtime.replace_remotes(vec![offline.clone()]);
    let answered = |events: &mut broadcast::Receiver<Event>| {
        std::iter::from_fn(|| events.try_recv().ok())
            .any(|event| event.event.request_id() == Some(&request))
    };
    assert!(!answered(&mut events));
    runtime.replace_remotes(vec![remote_session(id, offline.incarnation_id)]);
    assert!(answered(&mut events));
}

#[cfg(feature = "host")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn another_device_gets_answers_only_to_the_choices_of_a_new_terminal() {
    let runtime = TestRuntime::new().await;
    let caller = runtime.handle.command_client();
    let asked = Uuid::now_v7().to_string();
    let answer = caller
        .answer_call(json!({"type":"folders.list","requestId":asked}))
        .await;
    assert_eq!(answer["type"], "folders.result", "{answer}");
    assert_eq!(answer["requestId"], asked.as_str());
    assert!(answer["folders"]["folders"].is_array());
    for (request, refused) in [
        (
            json!({"type":"session.list","requestId":asked}),
            "session.error",
        ),
        (
            json!({"type":"folders.list","requestId":asked,"later":true}),
            "folders.error",
        ),
    ] {
        let answer = caller.answer_call(request).await;
        assert_eq!(
            (answer["type"].as_str(), answer["requestId"].as_str()),
            (Some(refused), Some(asked.as_str()))
        );
    }
    drop(caller);
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

#[tokio::test]
async fn an_app_that_connects_during_a_restore_hears_that_the_sign_in_is_coming_back() {
    let (mut runtime, _root) = registry_fixture();
    let restoring = json!({"type":"auth.restoring", "reason":"offline"});
    runtime.network_event(BackendEvent {
        generation: runtime.network.generation(),
        user_id: None,
        event: restoring.clone(),
    });
    assert_eq!(runtime.auth_event(), restoring);
    runtime.network_event(BackendEvent {
        generation: runtime.network.generation(),
        user_id: None,
        event: json!({"type":"auth.required", "reason":"expired"}),
    });
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
    assert!(runtime.reconnect[&id].next > Instant::now());
    runtime.network.inner.link.note_connected();
    runtime.reconnect_views();
    assert!(runtime.opening.contains_key(&id));

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
    let interrupt = Command::InterruptSession {
        request_id: Uuid::now_v7().to_string(),
        session_id: id.to_string(),
        expected_runtime_incarnation_id: remote.incarnation_id.to_string(),
    };
    assert_eq!(
        runtime.apply(&interrupt).unwrap_err().to_string(),
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
async fn a_terminal_that_another_computer_started_again_names_the_one_it_replaces() {
    let (mut runtime, _root) = registry_fixture();
    let replaced = Uuid::now_v7();
    let remote = RemoteSession {
        restored_from: Some(replaced),
        ..remote_session(Uuid::now_v7(), Uuid::now_v7())
    };
    runtime.replace_remotes(vec![remote.clone()]);
    assert_eq!(
        runtime.remotes[&remote.id].restored_from,
        Some(replaced.to_string())
    );
}

#[tokio::test]
async fn a_terminal_whose_computer_is_away_hides_or_closes_through_the_server() {
    let (mut runtime, _root) = registry_fixture();
    let mut hidden = remote_session(Uuid::now_v7(), Uuid::now_v7());
    let mut closed = remote_session(Uuid::now_v7(), Uuid::now_v7());
    let command = |remote: &RemoteSession, hide: bool| {
        let (request_id, session_id, expected_runtime_incarnation_id) = (
            Uuid::now_v7().to_string(),
            remote.id.to_string(),
            remote.incarnation_id.to_string(),
        );
        if hide {
            Command::HideSession {
                request_id,
                session_id,
                expected_runtime_incarnation_id,
            }
        } else {
            Command::CloseSession {
                request_id,
                session_id,
                expected_runtime_incarnation_id,
            }
        }
    };
    runtime.replace_remotes(vec![hidden.clone(), closed.clone()]);
    assert_eq!(
        runtime
            .apply(&command(&hidden, true))
            .unwrap_err()
            .to_string(),
        "Other computer is online."
    );

    hidden.online = false;
    hidden.hidden = true;
    closed.online = false;
    runtime.replace_remotes(vec![hidden.clone(), closed.clone()]);
    assert!(runtime.remotes[&hidden.id].hidden);
    host_notice(&mut runtime, &hidden, true);
    assert!(!runtime.remotes[&hidden.id].hidden);
    host_notice(&mut runtime, &hidden, false);
    assert!(!runtime.remotes[&hidden.id].hidden);

    runtime.apply(&command(&hidden, true)).unwrap();
    runtime.apply(&command(&closed, false)).unwrap();
    assert!(runtime.administering.contains(&hidden.id));
    assert!(runtime.administering.contains(&closed.id));
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
            failure: crate::diagnostics::Repeats::new(),
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
            failure: crate::diagnostics::Repeats::new(),
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
            failure: crate::diagnostics::Repeats::new(),
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
            canvas: None,
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

#[cfg(feature = "host")]
#[test]
fn a_restored_agent_continues_its_conversation_or_lists_them() {
    use crate::agents::Agent;
    let id = "0198f0c4-5e6f-7a8b-9c0d-1e2f3a4b5c6d";
    let line = |agent, command, conversation| restore::resume_line(agent, command, conversation);
    assert_eq!(
        line(Agent::Claude, "claude", Some(id)).unwrap(),
        format!("claude --resume {id}")
    );
    assert_eq!(
        line(Agent::Copilot, "copilot", Some(id)).unwrap(),
        format!("copilot --resume={id}")
    );
    assert_eq!(
        line(Agent::Codex, "env CODEX_HOME=/w codex", None).unwrap(),
        "env CODEX_HOME=/w codex resume"
    );
    assert_eq!(
        line(Agent::Claude, "claude", None).unwrap(),
        "claude --resume"
    );
    assert_eq!(line(Agent::Gemini, "gemini", Some(id)), None);
    let zsh = Path::new("/bin/zsh");
    assert_eq!(
        restore::usual_line("copilot", zsh).unwrap(),
        "copilot --plugin-dir \"$KODOSI_PLUGIN_DIR\" --resume"
    );
    assert_eq!(
        restore::usual_line("claude", zsh).unwrap(),
        "claude --append-system-prompt \"$KODOSI_AGENT_CONTEXT\" --resume"
    );
    assert_eq!(restore::usual_line("vim", zsh), None);
}

#[cfg(feature = "host")]
#[test]
fn saved_terminals_wait_for_an_app_and_stay_saved_until_they_start() {
    let root = tempfile::tempdir().unwrap();
    let saved = |name: &str| restore::Saved {
        id: Uuid::now_v7(),
        name: name.to_owned(),
        folder: None,
        sharing: None,
        line: None,
        worktree: None,
        canvas: Vec::new(),
    };
    let (first, second) = (saved("first"), saved("second"));
    restore::Restore::load(root.path()).save(vec![first.clone()]);
    restore::Restore::load(root.path()).save(vec![second.clone()]);
    let mut opened_by_an_app = restore::Restore::load(root.path());
    assert_eq!(opened_by_an_app.take(), vec![first, second]);
    assert_eq!(opened_by_an_app.take(), Vec::new());
}

#[cfg(feature = "host")]
#[test]
fn a_saved_screen_keeps_its_newest_lines_until_its_terminal_comes_back() {
    assert_eq!(restore::newest_lines(b"one\r\ntwo\r\nthree", 9), b"three");
    assert_eq!(restore::newest_lines(b"one\r\ntwo", 8), b"one\r\ntwo");
    assert_eq!(restore::newest_lines(b"one long line", 4), b"");

    let root = tempfile::tempdir().unwrap();
    let saved = restore::Saved {
        id: Uuid::now_v7(),
        name: "Notes".to_owned(),
        folder: None,
        sharing: None,
        line: None,
        worktree: None,
        canvas: Vec::new(),
    };
    let mut restore = restore::Restore::load(root.path());
    restore.save(vec![saved.clone()]);
    let ended = restore.screen_file(Uuid::now_v7());
    std::fs::create_dir_all(ended.parent().unwrap()).unwrap();
    std::fs::write(&ended, b"ended").unwrap();
    let lines = (0..100_000)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\r\n");
    restore.keep_screens(vec![(saved.id, lines.clone().into_bytes())]);

    assert!(!ended.exists());
    let shown = String::from_utf8(restore::earlier_screen(&restore.screen_file(saved.id))).unwrap();
    assert!(shown.len() < lines.len());
    assert!(shown.starts_with("line ") && !shown.starts_with("line 0\r\n"));
    assert!(shown.ends_with("line 99999\x1b]8;;\x1b\\\x1b[0m\r\n\x1b[2mRestored\x1b[0m\r\n"));
    restore.forget_screen(saved.id);
    assert_eq!(
        restore::earlier_screen(&restore.screen_file(saved.id)),
        Vec::<u8>::new()
    );
}

#[cfg(feature = "host")]
#[test]
fn a_restored_terminal_opens_in_the_nearest_folder_that_is_still_there() {
    let root = tempfile::tempdir().unwrap();
    let kept = root.path().join("kept");
    std::fs::create_dir(&kept).unwrap();
    let gone = kept.join("gone/deeper");
    assert_eq!(
        restore::nearest_folder(gone.to_str()).as_deref(),
        kept.to_str()
    );
    assert_eq!(restore::nearest_folder(None), None);
}

#[cfg(all(unix, feature = "host"))]
async fn send_to(handle: &RuntimeHandle, command: Command) {
    let events = handle.snapshot().await.unwrap();
    let event = events.first().unwrap();
    handle
        .try_send(CommandEnvelope {
            account_user_id: event.account_user_id.clone(),
            account_epoch: event.account_epoch,
            computer: None,
            command,
        })
        .unwrap();
}

#[cfg(all(unix, feature = "host"))]
async fn create_named(
    handle: &RuntimeHandle,
    name: &str,
    folder: &Path,
    line: Option<String>,
) -> String {
    let (_, mut events) = handle.observe().await.unwrap();
    let request = Uuid::now_v7().to_string();
    let command = Command::CreateSession {
        request_id: request.clone(),
        name: name.to_owned(),
        working_dir: Some(folder.to_string_lossy().into_owned()),
        mission_id: None,
        resume: None,
        branch: None,
        worktrees: None,
        command: line,
        agent: None,
        prompt: None,
    };
    send_to(handle, command).await;
    created(&mut events, &request).await["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[cfg(all(unix, feature = "host"))]
async fn sessions_where(
    events: &mut broadcast::Receiver<Event>,
    wanted: impl Fn(&[Value]) -> bool + Send + Sync,
) -> Vec<Value> {
    timeout(Duration::from_secs(20), async {
        loop {
            let event = serde_json::to_value(events.recv().await.unwrap()).unwrap();
            if event["type"] == "sessions.snapshot" {
                let sessions = event["sessions"].as_array().unwrap().clone();
                if wanted(&sessions) {
                    return sessions;
                }
            }
        }
    })
    .await
    .unwrap()
}

#[cfg(all(unix, feature = "host"))]
async fn screen_shows(handle: &RuntimeHandle, entry: &Value, text: &str) -> String {
    let (_, mut events) = handle.observe().await.unwrap();
    timeout(Duration::from_secs(20), async {
        loop {
            let request = Uuid::now_v7().to_string();
            let read = Command::Read {
                request_id: request.clone(),
                session_id: entry["id"].as_str().unwrap().to_owned(),
                expected_runtime_incarnation_id: entry["incarnationId"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
                history: false,
            };
            send_to(handle, read).await;
            let screen = loop {
                let event = serde_json::to_value(events.recv().await.unwrap()).unwrap();
                if event["requestId"] == request.as_str() {
                    break event["text"].as_str().unwrap_or_default().to_owned();
                }
            };
            if screen.contains(text) {
                break screen;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await
    .unwrap()
}

#[cfg(all(unix, feature = "host"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminals_come_back_after_the_host_stopped_once_an_app_asks() {
    const NAME: &str =
        "runtime::tests::terminals_come_back_after_the_host_stopped_once_an_app_asks";
    const RECORDS: &str = "KODOSI_TEST_CLAUDE_RECORDS";
    const CONVERSATION: &str = "0198f0c4-5e6f-7a8b-9c0d-1e2f3a4b5c6d";
    if let Some(records) = std::env::var_os(RECORDS) {
        let record = json!({"sessionId": CONVERSATION, "pid": std::process::id(), "cwd": "/"});
        std::fs::write(Path::new(&records).join("agent.json"), record.to_string()).unwrap();
        std::thread::sleep(Duration::from_secs(60));
        return;
    }
    let storage = tempfile::tempdir().unwrap();
    let root = kodosi_os::fs::canonicalize(storage.path()).unwrap();
    let config = || {
        let mut config = Config::isolated(&root).unwrap();
        config.initial_shell = Some(SHELL.to_owned());
        config
    };
    let (work, claude, agent) = (
        root.join("work"),
        root.join("claude"),
        root.join("bin/claude"),
    );
    let gone = work.join("gone");
    for folder in [&gone, &claude.join("sessions"), &root.join("bin")] {
        std::fs::create_dir_all(folder).unwrap();
    }
    std::os::unix::fs::symlink(std::env::current_exe().unwrap(), &agent).unwrap();

    let first = start(config()).await.unwrap();
    let (_, mut events) = first.observe().await.unwrap();
    let start_agent = format!(
        "env CLAUDE_CONFIG_DIR={} {RECORDS}={} {} --exact {NAME} --test-threads=1",
        claude.display(),
        claude.join("sessions").display(),
        agent.display()
    );
    let build = create_named(&first, "Build", &work, Some(start_agent)).await;
    let line = "echo before$((1+1))quit".to_owned();
    let notes = create_named(&first, "Notes", &gone, Some(line)).await;
    let sessions = sessions_where(&mut events, |sessions| {
        sessions
            .iter()
            .any(|entry| entry["id"] == build && entry["program"] == "claude")
    })
    .await;
    let notes_entry = sessions.iter().find(|entry| entry["id"] == notes).unwrap();
    screen_shows(&first, notes_entry, "before2quit").await;
    first.shutdown().await;
    drop(first);
    let screens = root.join("core/screens");
    let notes_screen = std::fs::read(screens.join(&notes)).unwrap();
    assert!(String::from_utf8_lossy(&notes_screen).contains("before2quit"));

    let file = root.join("core/terminals.json");
    let mut saved: Vec<Value> = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    let names = saved
        .iter()
        .map(|entry| entry["name"].clone())
        .collect::<Vec<_>>();
    assert_eq!(names, [json!("Build"), json!("Notes")]);
    let command =
        crate::agents::account_command(crate::agents::Agent::Claude, &root.join("home"), &claude);
    assert!(command.starts_with("env CLAUDE_CONFIG_DIR="), "{command}");
    assert_eq!(
        saved[0]["line"],
        format!(
            "{command} --append-system-prompt \"$KODOSI_AGENT_CONTEXT\" --resume {CONVERSATION}"
        )
    );
    assert!(saved[1].get("line").is_none());

    saved[0]["line"] = json!("echo came$((1+1))back");
    std::fs::write(&file, serde_json::to_vec(&saved).unwrap()).unwrap();
    std::fs::remove_dir(&gone).unwrap();
    let second = start(config()).await.unwrap();
    let (_, mut events) = second.observe().await.unwrap();
    let request_id = Uuid::now_v7().to_string();
    send_to(&second, Command::RestoreSessions { request_id }).await;
    let sessions = sessions_where(&mut events, |sessions| sessions.len() == 2).await;
    let restored = |from: &str| {
        sessions
            .iter()
            .find(|entry| entry["restoredFrom"] == from)
            .cloned()
            .unwrap()
    };
    let (build_again, notes_again) = (restored(&build), restored(&notes));
    assert_eq!(build_again["name"], "Build");
    assert_eq!(notes_again["name"], "Notes");
    assert_ne!(build_again["id"], build);
    for entry in [&build_again, &notes_again] {
        assert_eq!(entry["workingDir"], work.to_str().unwrap());
    }
    screen_shows(&second, &build_again, "came2back").await;
    let screen = screen_shows(&second, &notes_again, "Restored").await;
    let before = screen.find("before2quit").unwrap();
    assert!(before < screen.find("Restored").unwrap(), "{screen}");
    assert!(std::fs::read_dir(&screens).unwrap().next().is_none());
    second.shutdown().await;
    drop(second);
}

#[cfg(all(unix, feature = "host"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_restored_terminal_rejoins_its_room_for_its_account_and_leaves_a_refused_room() {
    let (mut runtime, storage) = registry_fixture();
    let id = Uuid::now_v7();
    let terminal = TerminalSession::spawn(
        id,
        Uuid::now_v7(),
        PathBuf::from(SHELL),
        Vec::new(),
        storage.path().to_owned(),
        None,
        None,
        Vec::new(),
        crate::canvas::CanvasHandle::spawn(Uuid::now_v7(), &std::env::temp_dir()),
        runtime.changes.clone(),
    )
    .await
    .unwrap();
    let replaced = Uuid::now_v7();
    let entry = serde_json::from_value(json!({
        "id": id, "incarnationId": terminal.incarnation_id, "kind": "local", "name": "Build",
        "isOwner": true, "status": "running", "connectionState": "local", "prompt": false,
        "repository": false, "sharedWith": [], "restoredFrom": replaced,
    }))
    .unwrap();
    let chosen = Uuid::now_v7().to_string();
    let incarnation = terminal.incarnation_id;
    runtime.local.insert(
        id,
        Local {
            terminal: terminal.clone(),
            entry,
            creation: Command::ListSessions {
                request_id: Uuid::now_v7().to_string(),
            },
            published: false,
            room_sequence: 0,
            worktree: None,
            wanted: restore::Sharing::of(Some(&"owner".to_owned()), Some(&chosen), &[]),
        },
    );
    assert_eq!(
        runtime.local[&id].publication(id).0.restored_from,
        Some(replaced)
    );
    let room_of = |runtime: &Runtime| runtime.local[&id].entry.mission_id.clone();
    for user in [None, Some("someone else")] {
        runtime.scope.user = user.map(str::to_owned);
        runtime.apply_wanted();
        assert_eq!(room_of(&runtime), None);
    }
    runtime.scope.user = Some("owner".to_owned());
    runtime.apply_wanted();
    assert_eq!(room_of(&runtime), Some(chosen.clone()));
    assert!(runtime.local[&id].wanted.is_none());

    runtime.published(
        id,
        incarnation,
        false,
        Err(Error::Other("offline".to_owned())),
        true,
    );
    assert_eq!(room_of(&runtime), Some(chosen));
    runtime.published(
        id,
        incarnation,
        true,
        Err(Error::Invalid("gone".to_owned())),
        true,
    );
    assert_eq!(room_of(&runtime), None);
    terminal.close().await.unwrap();
    drop(terminal);
}

#[cfg(feature = "host")]
#[test]
fn a_message_names_the_terminal_whose_whole_name_follows_the_at_sign() {
    let terminals = [(1, "Build"), (2, "Build 2"), (3, "Docs")];
    assert_eq!(
        messages::named("@Build 2 and @Docs, please", &terminals),
        [2, 3]
    );
    assert_eq!(messages::named("@Build: the test fails", &terminals), [1]);
    for text in ["@Builder", "write to ana@Build", "no names here", "@"] {
        assert_eq!(
            messages::named(text, &terminals),
            Vec::<i32>::new(),
            "{text}"
        );
    }
}

#[cfg(feature = "host")]
#[test]
fn an_agent_takes_messages_while_it_works_but_not_while_it_asks_a_person() {
    use crate::terminal::{ProgramState, ProgramStatus};
    let status = |state| ProgramStatus {
        state,
        kind: None,
        progress: None,
        title: None,
        message: None,
    };
    for state in [
        ProgramState::Idle,
        ProgramState::Working,
        ProgramState::Done,
        ProgramState::Error,
    ] {
        assert!(messages::takes_messages(
            Some("claude"),
            Some(&status(state))
        ));
    }
    let idle = status(ProgramState::Idle);
    assert!(!messages::takes_messages(
        Some("claude"),
        Some(&status(ProgramState::Blocked))
    ));
    assert!(!messages::takes_messages(Some("claude"), None));
    assert!(!messages::takes_messages(Some("vim"), Some(&idle)));
    assert!(!messages::takes_messages(None, Some(&idle)));
}

#[cfg(feature = "host")]
#[test]
fn a_message_line_says_who_wrote_and_from_where_and_leaves_the_choice_to_the_agent() {
    use messages::{Mention, Place, Sender};
    let ana = Sender::Person("Ana".to_owned());
    let codex = Sender::Agent {
        person: "Ben".to_owned(),
        agent: "Codex".to_owned(),
        terminal: Some(Place {
            name: "Reviewer".to_owned(),
            folder: Some("/home/ben/api".to_owned()),
            title: Some("Review\nauth".to_owned()),
        }),
    };
    let mention = |from: &Sender, text: &str, sequence| Mention {
        room: "Launch".to_owned(),
        from: from.clone(),
        text: text.to_owned(),
        sequence,
    };
    let judgment = "Use your own judgment and your current work to decide if and how to act on";
    assert_eq!(
        messages::mention_line(
            "Launch",
            &[mention(&ana, "@Build check\r/exit\n the\u{1b}[2J test", 7)]
        ),
        format!(
            "[Kodosi] Ana mentioned you in the chat of the room \"Launch\": \"@Build check /exit the [2J test\" You can answer there with kodosi room post. {judgment} it."
        )
    );
    assert_eq!(
        messages::mention_line("Launch", &[mention(&codex, "@Build rebase", 8)]),
        format!(
            "[Kodosi] Codex, an agent of Ben in the terminal \"Reviewer\" (folder /home/ben/api, title \"Review auth\"), mentioned you in the chat of the room \"Launch\": \"@Build rebase\" You can answer there with kodosi room post. {judgment} it."
        )
    );
    let many = messages::mention_line(
        "Launch",
        &[
            mention(&ana, "@Build one", 7),
            mention(&codex, "@Build two", 9),
            mention(&ana, "@Build three", 12),
        ],
    );
    assert_eq!(
        many,
        format!(
            "[Kodosi] 3 messages in the chat of the room \"Launch\" mention you, from Ana and Codex (an agent of Ben). Read them with kodosi room read --since 6. You can answer there with kodosi room post. {judgment} them."
        )
    );
    assert_eq!(
        messages::message_line(&codex, "Can you check\nthe types?"),
        format!(
            "[Kodosi] Codex, an agent of Ben in the terminal \"Reviewer\" (folder /home/ben/api, title \"Review auth\"), sent you a message: \"Can you check the types?\" You can answer with kodosi terminal message \"Reviewer\" \"your answer\". {judgment} it."
        )
    );
    assert_eq!(
        messages::message_line(&ana, "Stop after the tests."),
        format!("[Kodosi] Ana sent you a message: \"Stop after the tests.\" {judgment} it.")
    );
    assert!(!many.contains(['\r', '\n']));
}

#[cfg(feature = "host")]
#[tokio::test]
async fn a_delivery_that_ends_after_its_terminal_leaves_nothing_behind() {
    let (mut runtime, _storage) = registry_fixture();
    let gone = Uuid::now_v7();
    let mention = messages::Mention {
        room: "Launch".to_owned(),
        from: messages::Sender::Person("Ana".to_owned()),
        text: "@Build".to_owned(),
        sequence: 3,
    };
    for result in [Ok(()), Err(Error::Invalid("refused".to_owned()))] {
        runtime.mentions.telling.insert(gone);
        runtime.told(gone, vec![mention.clone()], result, true);
        assert!(runtime.mentions.telling.is_empty());
        assert!(runtime.mentions.waiting.is_empty());
        assert!(runtime.mentions.told.is_empty());
    }
}

#[cfg(feature = "host")]
#[test]
fn only_messages_after_the_first_look_at_a_room_name_agents() {
    let message = |sequence: u64| json!({"sequence": sequence, "text": "@Build"});
    let mut mentions = messages::Mentions::default();
    assert_eq!(
        mentions.new_messages("room", &[message(3), message(4)]),
        Vec::<Value>::new()
    );
    let fresh = mentions.new_messages("room", &[message(4), message(5), message(6)]);
    assert_eq!(fresh, [message(5), message(6)]);
    assert_eq!(
        mentions.new_messages("room", &[message(6)]),
        Vec::<Value>::new()
    );
}

#[cfg(all(unix, feature = "host"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_working_agent_gets_the_room_message_that_names_its_terminal_unless_it_asks() {
    use crate::terminal::{ProgramState, ProgramStatus};
    let (mut runtime, storage) = registry_fixture();
    let id = Uuid::now_v7();
    let terminal = TerminalSession::spawn(
        id,
        Uuid::now_v7(),
        PathBuf::from(SHELL),
        vec![
            "-c".to_owned(),
            "printf '\\033]7501;state=working:app=claude-code\\007'; exec /bin/cat".to_owned(),
        ],
        storage.path().to_owned(),
        None,
        None,
        Vec::new(),
        crate::canvas::CanvasHandle::spawn(Uuid::now_v7(), &std::env::temp_dir()),
        runtime.changes.clone(),
    )
    .await
    .unwrap();
    let room = Uuid::now_v7().to_string();
    let mut entry: SessionEntry = serde_json::from_value(json!({
        "id": id, "incarnationId": terminal.incarnation_id, "kind": "local", "name": "Build",
        "isOwner": true, "status": "running", "connectionState": "local", "prompt": false,
        "repository": false, "sharedWith": [], "missionId": room, "missionName": "Launch",
    }))
    .unwrap();
    entry.program = Some("claude".to_owned());
    runtime.local.insert(
        id,
        Local {
            terminal: terminal.clone(),
            entry,
            creation: Command::ListSessions {
                request_id: Uuid::now_v7().to_string(),
            },
            published: true,
            room_sequence: 0,
            worktree: None,
            wanted: None,
        },
    );
    let snapshot = |messages: Value| json!({"roomId": room, "messages": messages});
    runtime.note_mentions(&snapshot(
        json!([{"sequence": 1, "text": "hello", "authorName": "Ana"}]),
    ));
    runtime.note_mentions(&snapshot(json!([
        {"sequence": 2, "text": "@Build please print came-through", "authorName": "Ana"},
        {"sequence": 3, "text": "@Build from itself", "authorName": "Ana", "terminalId": id},
    ])));
    assert_eq!(runtime.mentions.waiting[&id].len(), 1);
    assert!(
        runtime.jobs.is_empty(),
        "an agent without a state gets nothing"
    );
    let set = |runtime: &mut Runtime, state| {
        runtime.local.get_mut(&id).unwrap().entry.program_status = Some(ProgramStatus {
            state,
            kind: None,
            progress: None,
            title: None,
            message: None,
        });
    };
    set(&mut runtime, ProgramState::Blocked);
    runtime.tell_mentions();
    assert!(
        runtime.jobs.is_empty(),
        "an agent that asks a person gets nothing"
    );

    set(&mut runtime, ProgramState::Working);
    timeout(Duration::from_secs(10), async {
        while runtime.mentions.waiting.contains_key(&id) || !runtime.jobs.is_empty() {
            runtime.tell_mentions();
            match runtime.jobs.join_next().await {
                Some(told) => runtime.complete(told.unwrap()),
                None => tokio::time::sleep(Duration::from_millis(50)).await,
            }
        }
    })
    .await
    .unwrap();
    timeout(Duration::from_secs(10), async {
        while !terminal.read(false).await.unwrap().contains("came-through") {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        terminal
            .read(false)
            .await
            .unwrap()
            .contains("[Kodosi] Ana mentioned you")
    );
    terminal.close().await.unwrap();
    drop(terminal);
}

#[cfg(all(unix, feature = "host"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn enter_reaches_the_program_as_a_key_of_its_own_after_the_text() {
    let (runtime, storage) = registry_fixture();
    let terminal = TerminalSession::spawn(
        Uuid::now_v7(),
        Uuid::now_v7(),
        PathBuf::from(SHELL),
        vec![
            "-c".to_owned(),
            "stty -icanon -echo min 1 time 0; echo ready; dd bs=4096 count=1 2>/dev/null | od -c; sleep 10"
                .to_owned(),
        ],
        storage.path().to_owned(),
        None,
        None,
        Vec::new(),
        crate::canvas::CanvasHandle::spawn(Uuid::now_v7(), &std::env::temp_dir()),
        runtime.changes.clone(),
    )
    .await
    .unwrap();
    let shows = async |text: &str| {
        timeout(Duration::from_secs(10), async {
            while !terminal.read(false).await.unwrap().contains(text) {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap();
    };
    shows("ready").await;
    TerminalTarget::Local(terminal.clone())
        .type_line("hello".to_owned(), false)
        .await
        .unwrap();
    shows("0000005").await;
    assert!(!terminal.read(false).await.unwrap().contains("\\r"));
    terminal.close().await.unwrap();
    drop(terminal);
}
