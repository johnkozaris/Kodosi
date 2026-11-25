use super::*;
use crate::protocol::{AppLifecycle, Presence};
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

    #[cfg(feature = "host")]
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
            project_id: None,
            resume: None,
            branch: None,
            worktrees: None,
            command: None,
            agent: None,
            prompt: None,
            like: None,
            size: None,
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
                project_id: None,
                resume: None,
                branch: Some(branch.to_owned()),
                worktrees: None,
                command: None,
                agent: None,
                prompt: None,
                like: None,
                size: None,
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
            language: None,
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
                project_id: None,
                resume: None,
                branch: None,
                worktrees: None,
                command: None,
                agent: None,
                prompt: None,
                like: None,
                size: None,
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
        project_id: None,
        resume: None,
        branch: None,
        worktrees: None,
        command: None,
        agent: None,
        prompt: None,
        like: None,
        size: None,
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
                returning: false,
            },
        })
        .unwrap_err();
    assert!(matches!(error, Error::Invalid(_)));
    runtime.shutdown().await;
    drop(runtime);
}

#[expect(clippy::too_many_lines, reason = "one runtime with each of its parts")]
fn registry_fixture() -> (Runtime, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let config = Config::isolated(&kodosi_os::fs::canonicalize(root.path()).unwrap()).unwrap();
    let project_seen = project_seen::ProjectSeen::load(&config.data_root);
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
            pipes: HashMap::new(),
            direct: HashMap::new(),
            #[cfg(feature = "host")]
            services: crate::canvas::CanvasServices::new(&config.data_root, true),
            #[cfg(feature = "host")]
            links: crate::chat::Links::unserved(),
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
            #[cfg(not(feature = "host"))]
            dosi: crate::dosi::Viewer::load(&config.data_root).0,
            #[cfg(feature = "host")]
            launcher,
            #[cfg(feature = "host")]
            recent: crate::folders::Recent::load(&config.data_root),
            #[cfg(feature = "host")]
            restore: restore::Restore::load(&config.data_root),
            #[cfg(feature = "host")]
            look: look::HostLook::load(&config.data_root),
            #[cfg(feature = "host")]
            mentions: messages::Mentions::default(),
            #[cfg(feature = "host")]
            agent_changes: changes::AgentChanges::default(),
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
            typing: HashMap::new(),
            inputs: 0,
            unsent: HashMap::new(),
            folder_asks: HashMap::new(),
            link_connects: 0,
            next_reconnects: Instant::now(),
            pausing: None,
            online: true,
            consumers: HashMap::new(),
            leaving: FuturesUnordered::new(),
            approvals: BTreeMap::new(),
            lane: sharing::Lane::default(),
            #[cfg(feature = "host")]
            terminals_owner: None,
            #[cfg(feature = "host")]
            sign_in_origin: None,
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
            catalog: std::sync::Mutex::default(),
            answers: std::sync::Mutex::new(HashMap::new()),
            host: Some(crate::HostKind::Background),
            visible: HashMap::new(),
            attention: attention::Attention::default(),
            project_latest: HashMap::new(),
            project_seen,
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
        project_id: None,
        project_name: None,
        shared_with: vec![],
        online: true,
        status: None,
        hidden: false,
        restored_from: None,
    }
}

fn changes_of(runtime: &mut Runtime) -> mpsc::Receiver<SessionChange> {
    let (changes, receiver) = mpsc::channel(128);
    runtime.changes = changes;
    receiver
}

/// Initializes the network/account before starting the runtime loop so real-time disk I/O does
/// not race with paused-clock tests.
async fn loop_fixture() -> (Runtime, tempfile::TempDir, mpsc::Receiver<SessionChange>) {
    let (mut runtime, root) = registry_fixture();
    drop(runtime.network.initialize().await);
    let changes = changes_of(&mut runtime);
    (runtime, root, changes)
}

async fn start_loop(
    mut runtime: Runtime,
    changes: mpsc::Receiver<SessionChange>,
) -> (mpsc::Sender<Request>, tokio::task::JoinHandle<()>) {
    while runtime.jobs.join_next().await.is_some() {}
    let (requests, receiver) = mpsc::channel(COMMAND_CAPACITY);
    let network_events = runtime.network.events();
    let (_dosi, dosi) = mpsc::channel(1);
    let looping = tokio::spawn(Box::pin(runtime.run(
        receiver,
        changes,
        network_events,
        dosi,
    )));
    (requests, looping)
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
        project_id: None,
        resume: None,
        branch: None,
        worktrees: None,
        command: None,
        agent: None,
        prompt: None,
        like: None,
        size: None,
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
    let caller = runtime.handle.command_client(Origin::Device);
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

#[tokio::test(start_paused = true)]
async fn a_client_that_goes_lets_go_of_the_terminals_that_it_opened_at_once() {
    let (mut runtime, _root, changes) = loop_fixture().await;
    let remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    runtime.replace_remotes(vec![remote.clone()]);
    let (client, lifetime) = (Uuid::now_v7(), CancellationToken::new());
    let (_view, _updates) = open_shown(&mut runtime, &remote, client, &lifetime).await;
    let terminal = runtime.connections[&remote.id].clone();
    let shows = CommandEnvelope {
        account_user_id: runtime.scope.user.clone(),
        account_epoch: runtime.scope.epoch,
        computer: None,
        command: Command::Visible {
            request_id: Uuid::now_v7().to_string(),
            session_ids: vec![remote.id.to_string()],
            watching: true,
        },
    };
    let mut events = runtime.events.subscribe();
    let (requests, looping) = start_loop(runtime, changes).await;
    requests
        .send(Request::Command {
            envelope: shows,
            consumer: client,
            lifetime: lifetime.clone(),
            origin: Origin::App,
        })
        .await
        .unwrap();
    while events.recv().await.unwrap().recipient != Some(client) {}
    lifetime.cancel();
    timeout(Duration::from_millis(100), async {
        while !terminal.is_closed() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("the terminal is let go");
    looping.abort();
}

#[tokio::test]
async fn a_stream_of_document_parts_leaves_room_for_the_requests_of_its_client() {
    let (mut runtime, _root) = registry_fixture();
    let mut events = runtime.events.subscribe();
    let (user, epoch) = (runtime.scope.user.clone(), runtime.scope.epoch);
    let envelope = |command| CommandEnvelope {
        account_user_id: user.clone(),
        account_epoch: epoch,
        computer: None,
        command,
    };
    let (consumer, lifetime) = (Uuid::now_v7(), CancellationToken::new());
    let pipe = Uuid::now_v7().to_string();
    for _ in 0..=MAX_ANSWERS {
        let part = Command::DocumentSend {
            request_id: Uuid::now_v7().to_string(),
            pipe: pipe.clone(),
            more: false,
            data: String::new(),
        };
        runtime.command(&envelope(part), consumer, lifetime.clone(), Origin::App);
    }
    let visible = Command::Visible {
        request_id: Uuid::now_v7().to_string(),
        session_ids: Vec::new(),
        watching: true,
    };
    runtime.command(&envelope(visible), consumer, lifetime, Origin::App);
    let answers: Vec<_> = std::iter::from_fn(|| events.try_recv().ok())
        .map(|event| (event.event.kind(), event.recipient))
        .collect();
    assert_eq!(answers, [("session.result", Some(consumer))]);
    assert!(runtime.answers().is_empty());
}

#[tokio::test]
async fn an_app_that_fell_behind_gets_the_whole_state_again_also_after_an_account_change() {
    let (mut runtime, _root) = registry_fixture();
    let mut events = runtime.events.subscribe();
    let (consumer, lifetime) = (Uuid::now_v7(), CancellationToken::new());
    let resync = CommandEnvelope {
        account_user_id: Some("an account that went away".to_owned()),
        account_epoch: runtime.scope.epoch + 1,
        computer: None,
        command: Command::Resync {
            request_id: Uuid::now_v7().to_string(),
        },
    };
    runtime.command(&resync, consumer, lifetime, Origin::App);
    let heard: Vec<_> = std::iter::from_fn(|| events.try_recv().ok())
        .map(|event| (event.event.kind(), event.recipient, event.account_epoch))
        .collect();
    let snapshot: Vec<_> = runtime
        .snapshot_events()
        .unwrap()
        .iter()
        .map(|event| (event.event.kind(), Some(consumer), runtime.scope.epoch))
        .chain([("app.result", Some(consumer), runtime.scope.epoch)])
        .collect();
    assert_eq!(heard, snapshot);
    assert!(heard.iter().any(|(kind, ..)| *kind == "sessions.snapshot"));
}

struct OpenDocument {
    pipe: Uuid,
    consumer: Uuid,
    lifetime: CancellationToken,
    events: mpsc::Receiver<Event>,
}

fn as_client(runtime: &Runtime, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        account_user_id: runtime.scope.user.clone(),
        account_epoch: runtime.scope.epoch,
        computer: None,
        command,
    }
}

fn open_document(runtime: &mut Runtime, session: Uuid) -> OpenDocument {
    let (consumer, lifetime) = (Uuid::now_v7(), CancellationToken::new());
    let (own, events) = mpsc::channel(64);
    runtime.direct.insert(consumer, own);
    let pipe = Uuid::now_v7();
    let open = Command::DocumentOpen {
        request_id: Uuid::now_v7().to_string(),
        session_id: session.to_string(),
        pipe: pipe.to_string(),
        key: "browser".to_owned(),
    };
    runtime.command(
        &as_client(runtime, open),
        consumer,
        lifetime.clone(),
        Origin::App,
    );
    OpenDocument {
        pipe,
        consumer,
        lifetime,
        events,
    }
}

fn send_part(runtime: &mut Runtime, document: &OpenDocument, more: bool, part: &[u8]) {
    let send = Command::DocumentSend {
        request_id: Uuid::now_v7().to_string(),
        pipe: document.pipe.to_string(),
        more,
        data: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, part),
    };
    runtime.command(
        &as_client(runtime, send),
        document.consumer,
        document.lifetime.clone(),
        Origin::App,
    );
}

async fn attempt_finished(runtime: &mut Runtime) {
    while !opens_pipe(runtime).await {}
}

async fn opens_pipe(runtime: &mut Runtime) -> bool {
    let job = runtime.jobs.join_next().await.unwrap().unwrap();
    let document = matches!(job.completion, Completion::Document { .. });
    runtime.complete(job);
    document
}

async fn pipe_opened(
    requests: &mut mpsc::Receiver<crate::network::TestRemoteRequest>,
) -> mpsc::Sender<Vec<u8>> {
    loop {
        if let crate::network::TestRemoteRequest::PipeOpen { frames, .. } =
            requests.recv().await.unwrap()
        {
            return frames;
        }
    }
}

async fn pipe_part(requests: &mut mpsc::Receiver<crate::network::TestRemoteRequest>) -> Bytes {
    loop {
        if let crate::network::TestRemoteRequest::PipeData { part, .. } =
            requests.recv().await.unwrap()
        {
            return part;
        }
    }
}

async fn pipe_ended(runtime: &mut Runtime, changes: &mut mpsc::Receiver<SessionChange>) {
    loop {
        let change = changes.recv().await.unwrap();
        if matches!(change, SessionChange::PipeEnded { .. }) {
            runtime.change(change);
            return;
        }
    }
}

async fn heard(document: &mut OpenDocument) -> Event {
    document.events.recv().await.unwrap()
}

#[tokio::test(start_paused = true)]
async fn a_document_opens_again_as_soon_as_its_terminal_runs_again() {
    let (mut runtime, _root) = registry_fixture();
    let mut changes = changes_of(&mut runtime);
    let remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    let (_view, mut requests, updates) = open_view(&mut runtime, &remote).await;
    let mut document = open_document(&mut runtime, remote.id);
    attempt_finished(&mut runtime).await;
    let frames = pipe_opened(&mut requests).await;
    assert_eq!(heard(&mut document).await.event.kind(), "document.opened");
    frames.send(b"page".to_vec()).await.unwrap();
    assert_eq!(heard(&mut document).await.event.kind(), "document.frame");

    lose_link(
        &mut runtime,
        &remote,
        &updates,
        "The connection has closed.",
    )
    .await;
    drop(frames);
    pipe_ended(&mut runtime, &mut changes).await;
    runtime.keep_pipes();
    let lost_at = Instant::now();
    assert_eq!(heard(&mut document).await.event.kind(), "document.lost");
    assert!(lost_at.elapsed() >= Duration::from_millis(1500));
    assert!(runtime.jobs.is_empty(), "the pipe waits for its terminal");

    runtime.reconnect.get_mut(&remote.id).unwrap().next = Instant::now();
    runtime.reconnect_views();
    let (again, mut requests, _updates) = crate::network::test_remote_connection(remote.clone());
    connected(&mut runtime, remote.id, Ok(again));
    while runtime.connections[&remote.id].is_interrupted() {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    let back = Instant::now();
    runtime.keep_pipes();
    attempt_finished(&mut runtime).await;
    pipe_opened(&mut requests).await;
    assert_eq!(heard(&mut document).await.event.kind(), "document.opened");
    assert!(back.elapsed() < Duration::from_millis(100));
}

#[tokio::test(start_paused = true)]
async fn a_short_break_of_a_document_shows_no_sign_and_drops_the_rest_of_a_cut_frame() {
    let (mut runtime, _root) = registry_fixture();
    let mut changes = changes_of(&mut runtime);
    let remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    let (_view, mut requests, _updates) = open_view(&mut runtime, &remote).await;
    let mut document = open_document(&mut runtime, remote.id);
    attempt_finished(&mut runtime).await;
    let frames = pipe_opened(&mut requests).await;
    frames.send(b"page".to_vec()).await.unwrap();
    send_part(&mut runtime, &document, true, b"head");

    drop(frames);
    pipe_ended(&mut runtime, &mut changes).await;
    runtime.keep_pipes();
    attempt_finished(&mut runtime).await;
    let frames = pipe_opened(&mut requests).await;
    send_part(&mut runtime, &document, false, b"tail");
    send_part(&mut runtime, &document, false, b"whole");
    assert_eq!(pipe_part(&mut requests).await.as_ref(), b"whole");
    frames.send(b"page again".to_vec()).await.unwrap();

    tokio::time::sleep(Duration::from_secs(3)).await;
    let heard = std::iter::from_fn(|| document.events.try_recv().ok())
        .map(|event| event.event.kind())
        .collect::<Vec<_>>();
    assert_eq!(
        heard,
        [
            "document.opened",
            "document.frame",
            "document.opened",
            "document.frame"
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn a_terminal_that_refuses_a_document_is_asked_again_later_each_time() {
    let (mut runtime, _root) = registry_fixture();
    let mut changes = changes_of(&mut runtime);
    let remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    let (_view, mut requests, _updates) = open_view(&mut runtime, &remote).await;
    let mut document = open_document(&mut runtime, remote.id);
    for wait in [1, 2, 4] {
        attempt_finished(&mut runtime).await;
        drop(pipe_opened(&mut requests).await);
        pipe_ended(&mut runtime, &mut changes).await;
        runtime.keep_pipes();
        assert!(runtime.jobs.is_empty());
        let due = runtime.pipes_due().unwrap();
        assert_eq!(due - Instant::now(), Duration::from_secs(wait));
        tokio::time::sleep_until(due).await;
        runtime.keep_pipes();
    }
    let heard = std::iter::from_fn(|| document.events.try_recv().ok())
        .map(|event| event.event.kind())
        .filter(|kind| *kind == "document.lost")
        .count();
    assert_eq!(heard, 1);
}

#[tokio::test(start_paused = true)]
async fn a_client_that_goes_closes_its_documents() {
    let (mut runtime, _root) = registry_fixture();
    let remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    let (_view, mut requests, _updates) = open_view(&mut runtime, &remote).await;
    let document = open_document(&mut runtime, remote.id);
    attempt_finished(&mut runtime).await;
    let _frames = pipe_opened(&mut requests).await;
    document.lifetime.cancel();
    runtime.keep_pipes();
    assert!(runtime.pipes.is_empty());
    assert!(matches!(
        requests.recv().await.unwrap(),
        crate::network::TestRemoteRequest::PipeClose { .. }
    ));
}

#[tokio::test(start_paused = true)]
async fn a_document_that_its_client_closed_while_it_opened_closes_when_it_opens() {
    let (mut runtime, _root) = registry_fixture();
    let remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    let (_view, mut requests, _updates) = open_view(&mut runtime, &remote).await;
    let mut document = open_document(&mut runtime, remote.id);
    runtime
        .apply(&Command::DocumentClose {
            request_id: Uuid::now_v7().to_string(),
            pipe: document.pipe.to_string(),
        })
        .unwrap();
    attempt_finished(&mut runtime).await;
    let _frames = pipe_opened(&mut requests).await;
    assert!(matches!(
        requests.recv().await.unwrap(),
        crate::network::TestRemoteRequest::PipeClose { .. }
    ));
    assert!(runtime.pipes.is_empty());
    assert!(document.events.try_recv().is_err());
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
        reason: crate::text::Text::plain("old connection"),
    });
    assert_eq!(runtime.connections[&id].instance_id, new_instance);
    runtime.connections[&id].disconnect();
}

#[cfg(feature = "host")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_completion_publishes_catalog_before_its_result_and_retry_checks_directory() {
    let runtime = TestRuntime::new().await;
    let (_, mut events) = runtime.handle.observe().await.unwrap();
    let request_id = Uuid::now_v7().to_string();
    let command = Command::CreateSession {
        request_id: request_id.clone(),
        name: "ordered".to_owned(),
        working_dir: None,
        project_id: None,
        resume: None,
        branch: None,
        worktrees: None,
        command: None,
        agent: None,
        prompt: None,
        like: None,
        size: None,
    };
    runtime.send(command).await;
    assert_eq!(created(&mut events, &request_id).await["name"], "ordered");
    runtime
        .send(Command::CreateSession {
            request_id: request_id.clone(),
            name: "ordered".to_owned(),
            working_dir: Some(ROOT.to_owned()),
            project_id: None,
            resume: None,
            branch: None,
            worktrees: None,
            command: None,
            agent: None,
            prompt: None,
            like: None,
            size: None,
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

#[tokio::test(start_paused = true)]
async fn a_view_that_cannot_start_to_connect_again_waits_like_one_that_failed() {
    let (mut runtime, _root) = registry_fixture();
    let remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    let (_view, _requests, updates) = open_view(&mut runtime, &remote).await;
    lose_link(
        &mut runtime,
        &remote,
        &updates,
        "The connection has closed.",
    )
    .await;
    for _ in runtime.jobs.len()..MAX_JOBS {
        runtime.spawn(std::future::pending());
    }
    let failures = runtime.reconnect[&remote.id].failures;
    runtime.reconnect.get_mut(&remote.id).unwrap().next = Instant::now();
    runtime.reconnect_views();
    assert!(!runtime.opening.contains_key(&remote.id));
    assert_eq!(runtime.reconnect[&remote.id].failures, failures + 1);
    assert!(runtime.reconnect[&remote.id].next > Instant::now());
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

type RemoteState = (ConnectionState, SessionStatus, Option<crate::text::Text>);

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
            reason: crate::text::Text::plain("link lost"),
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
    runtime.remote_lost(
        id,
        remote.incarnation_id,
        instance,
        crate::text::Text::plain(reason.to_owned()),
        true,
    );
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
async fn typing_that_another_computer_did_not_get_shows_on_its_terminal_and_not_as_errors() {
    let (mut runtime, _root) = registry_fixture();
    let remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    let id = remote.id;
    let (_view, _requests, _updates) = open_view(&mut runtime, &remote).await;
    let mut events = runtime.events.subscribe();
    let typed = |runtime: &mut Runtime, number, result| {
        runtime.complete(Job {
            scope: runtime.scope.clone(),
            completion: Completion::Typed {
                session: id,
                number,
                reply: None,
                result,
            },
        });
    };

    runtime.inputs = 3;
    typed(&mut runtime, 1, Err(unsent(Error::Busy)));
    typed(&mut runtime, 2, Err(unsent(Error::Busy)));
    assert_eq!(
        remote_state(&runtime, id).2,
        Some(crate::network::input_not_sent())
    );
    runtime.send_waiting_list();
    let mut published = Vec::new();
    while let Ok(event) = events.try_recv() {
        published.push(serde_json::to_value(event).unwrap()["type"].clone());
    }
    assert_eq!(published, ["sessions.snapshot"]);

    typed(&mut runtime, 3, Ok(()));
    assert_eq!(
        remote_state(&runtime, id).2,
        Some(crate::network::input_not_sent())
    );
    runtime.inputs = 4;
    typed(&mut runtime, 4, Ok(()));
    assert_eq!(remote_state(&runtime, id).2, None);
}

async fn online(events: &mut broadcast::Receiver<Event>) -> bool {
    loop {
        let event = serde_json::to_value(events.recv().await.unwrap()).unwrap();
        if event["type"] == "connection.state" {
            return event["online"] == true;
        }
    }
}

#[tokio::test(start_paused = true)]
async fn kodosi_turns_offline_when_the_server_stays_away_and_online_as_soon_as_it_is_back() {
    let (runtime, _root, changes) = loop_fixture().await;
    let link = Arc::clone(&runtime.network.inner.link);
    let mut events = runtime.events.subscribe();
    let (_requests, looping) = start_loop(runtime, changes).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    link.start_wanting();
    let down = Instant::now();
    assert!(!online(&mut events).await);
    let away = down.elapsed();
    assert!(away >= crate::network::RECONNECT_WAIT);
    assert!(away < crate::network::RECONNECT_WAIT + Duration::from_millis(100));
    link.note_connected();
    let back = Instant::now();
    assert!(online(&mut events).await);
    assert!(back.elapsed() < Duration::from_millis(100));
    looping.abort();
}

#[tokio::test]
async fn a_terminal_that_could_not_open_tries_again_as_soon_as_the_server_is_back() {
    let (mut runtime, _root) = registry_fixture();
    let remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    let id = remote.id;
    runtime.replace_remotes(vec![remote]);
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
    connected(
        &mut runtime,
        id,
        Err(Error::Text(crate::text::Text::plain("unreachable"))),
    );
    assert!(!runtime.opening.contains_key(&id));
    assert!(runtime.reconnect[&id].next > Instant::now());

    runtime.network.inner.link.note_connected();
    runtime.reconnect_views();
    assert!(runtime.opening.contains_key(&id));
}

#[tokio::test(start_paused = true)]
async fn a_phone_closes_its_connections_only_when_it_stays_in_the_background() {
    let (mut runtime, _root) = registry_fixture();
    let mut events = runtime.events.subscribe();
    let client = Uuid::now_v7();
    let tell = |runtime: &mut Runtime, state| {
        let command = Command::Lifecycle {
            request_id: Uuid::now_v7().to_string(),
            state,
        };
        runtime
            .expect_answer(&command, client, CancellationToken::new())
            .unwrap();
        runtime.apply(&command).unwrap();
    };
    let link = Arc::clone(&runtime.network.inner.link);

    tell(&mut runtime, AppLifecycle::Background);
    tokio::time::sleep(BACKGROUND_GRACE / 2).await;
    tell(&mut runtime, AppLifecycle::Foreground);
    complete_next_job(&mut runtime).await;
    assert!(!link.in_background());
    assert!(runtime.jobs.is_empty());

    let away = Instant::now();
    tell(&mut runtime, AppLifecycle::Background);
    complete_next_job(&mut runtime).await;
    assert!(away.elapsed() >= BACKGROUND_GRACE);
    assert!(link.in_background());
    let answered = std::iter::from_fn(|| events.try_recv().ok())
        .filter(|answer| answer.event.kind() == "app.result")
        .count();
    complete_next_job(&mut runtime).await;
    assert_eq!(
        std::iter::from_fn(|| events.try_recv().ok())
            .filter(|answer| answer.event.kind() == "app.result")
            .count(),
        1
    );
    assert_eq!(answered, 2);
}

#[tokio::test(start_paused = true)]
async fn a_phone_in_the_background_wakes_for_no_work_of_its_own_until_it_comes_back() {
    let (mut runtime, _root, changes) = loop_fixture().await;
    let (client, lifetime) = (Uuid::now_v7(), CancellationToken::new());
    let start = Command::CreateSession {
        request_id: Uuid::now_v7().to_string(),
        name: "Terminal".to_owned(),
        working_dir: None,
        project_id: None,
        resume: None,
        branch: None,
        worktrees: None,
        command: None,
        agent: None,
        prompt: None,
        like: None,
        size: None,
    };
    runtime
        .expect_answer(&start, client, lifetime.clone())
        .unwrap();
    runtime.starting.insert(
        Uuid::now_v7(),
        Starting {
            answer: json!({"type":"session.result","requestId":start.request_id(),"operation":"session.create"}),
            until: Instant::now() + Duration::from_secs(1),
        },
    );
    runtime.network.inner.link.set_background(true);
    let foreground = as_client(
        &runtime,
        Command::Lifecycle {
            request_id: Uuid::now_v7().to_string(),
            state: AppLifecycle::Foreground,
        },
    );
    let mut events = runtime.events.subscribe();
    let (requests, looping) = start_loop(runtime, changes).await;
    let answered = |event: &Event| event.event.request_id() == Some(start.request_id());

    tokio::time::sleep(Duration::from_mins(1)).await;
    assert!(!std::iter::from_fn(|| events.try_recv().ok()).any(|event| answered(&event)));
    requests
        .send(Request::Command {
            envelope: foreground,
            consumer: client,
            lifetime,
            origin: Origin::App,
        })
        .await
        .unwrap();
    let back = Instant::now();
    while !answered(&events.recv().await.unwrap()) {}
    assert_eq!(back.elapsed(), Duration::ZERO);
    looping.abort();
}

#[test]
fn every_command_can_fail_with_an_answer_that_its_app_can_read() {
    for sample in crate::protocol::tests::commands() {
        let envelope: CommandEnvelope =
            serde_json::from_value(crate::protocol::tests::envelope(sample)).unwrap();
        let failed = answer(&envelope.command, Err(&Error::Busy));
        assert!(
            crate::protocol::EventBody::new(failed.clone()).is_ok(),
            "{failed}"
        );
    }
}

async fn complete_next_job(runtime: &mut Runtime) {
    let job = runtime.jobs.join_next().await.unwrap().unwrap();
    runtime.complete(job);
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

    lose_link(
        &mut runtime,
        &remote,
        &updates,
        "The terminal's computer is offline.",
    )
    .await;
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
    assert!(!runtime.attention.marked(&id.to_string()));
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
    let notice = Some(crate::network::input_not_sent());
    assert_eq!(
        remote_state(&runtime, id),
        (
            ConnectionState::Connected,
            SessionStatus::Offline,
            notice.clone()
        )
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
        (ConnectionState::Connected, SessionStatus::Running, notice)
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
async fn a_view_that_goes_while_it_connects_is_answered_and_a_waiting_close_goes_on() {
    let (mut runtime, _root) = registry_fixture();
    let mut events = runtime.events.subscribe();
    let remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    let id = remote.id;
    runtime.replace_remotes(vec![remote.clone()]);
    let (user, epoch) = (runtime.scope.user.clone(), runtime.scope.epoch);
    let envelope = |command| CommandEnvelope {
        account_user_id: user.clone(),
        account_epoch: epoch,
        computer: None,
        command,
    };
    let (consumer, lifetime) = (Uuid::now_v7(), CancellationToken::new());
    let open = open_remote(id);
    let close = Command::CloseSession {
        request_id: Uuid::now_v7().to_string(),
        session_id: id.to_string(),
        expected_runtime_incarnation_id: remote.incarnation_id.to_string(),
    };
    for command in [&open, &close] {
        runtime.command(
            &envelope(command.clone()),
            consumer,
            lifetime.clone(),
            Origin::App,
        );
    }
    assert_eq!(runtime.opening[&id].commands, [open.clone(), close.clone()]);

    let leave = Command::DisconnectRemote {
        session_id: id.to_string(),
        request_id: Uuid::now_v7().to_string(),
    };
    runtime.command(&envelope(leave.clone()), consumer, lifetime, Origin::App);
    let answered: Vec<_> = std::iter::from_fn(|| events.try_recv().ok())
        .filter(|event| event.event.is_answer())
        .filter_map(|event| event.event.request_id().map(str::to_owned))
        .collect();
    assert_eq!(answered, [open.request_id(), leave.request_id()]);
    assert_eq!(runtime.opening[&id].commands, std::slice::from_ref(&close));
    assert_eq!(
        runtime.answers().keys().collect::<Vec<_>>(),
        [close.request_id()]
    );
}

#[tokio::test]
async fn remote_recovery_preserves_only_open_current_incarnations() {
    let (mut runtime, _root) = registry_fixture();
    let id = Uuid::now_v7();
    let incarnation = Uuid::now_v7();
    let remote = remote_session(id, incarnation);
    runtime.replace_remotes(vec![remote]);
    runtime.views.insert(
        id,
        HashMap::from([(Uuid::now_v7(), CancellationToken::new())]),
    );
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
    runtime.views.insert(
        id,
        HashMap::from([(Uuid::now_v7(), CancellationToken::new())]),
    );
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

fn reports(runtime: &mut Runtime, remote: &RemoteSession, online: bool, status: &Value) {
    let (generation, user_id) = (runtime.network.generation(), runtime.scope.user.clone());
    runtime.network_event(BackendEvent {
        generation,
        user_id,
        event: json!({"type":"network.terminal","sessionId":remote.id,
            "incarnationId":remote.incarnation_id,"online":online,"status":status}),
    });
}

fn calls(
    events: &mut broadcast::Receiver<Event>,
    listed: &mut Option<SessionEntry>,
) -> Vec<(Option<crate::terminal::Waiting>, crate::protocol::Cue)> {
    let mut calls = Vec::new();
    while let Ok(event) = events.try_recv() {
        match event.event {
            crate::protocol::EventBody::SessionsSnapshot { mut sessions } => {
                *listed = sessions.pop();
            }
            crate::protocol::EventBody::TerminalAttention { cue, .. } => {
                let entry = listed.as_ref().expect("the list comes before the call");
                assert!(entry.attention);
                calls.push((entry.waiting, cue));
            }
            _ => {}
        }
    }
    calls
}

#[tokio::test(start_paused = true)]
async fn a_friends_open_terminal_calls_once_a_new_state_lasts_unseen_and_not_when_it_comes_back() {
    use crate::terminal::Waiting;
    let (mut runtime, _root) = registry_fixture();
    let remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    let id = remote.id;
    runtime.replace_remotes(vec![remote.clone()]);
    runtime.remotes.get_mut(&id).unwrap().connection_state = ConnectionState::Connected;
    let mut events = runtime.events.subscribe();
    let marked = |runtime: &mut Runtime| {
        runtime.attend();
        runtime.entries()[0].attention
    };

    reports(&mut runtime, &remote, true, &json!({"state":"done"}));
    tokio::time::advance(Duration::from_millis(1900)).await;
    assert!(!marked(&mut runtime));
    tokio::time::advance(Duration::from_millis(100)).await;
    assert!(marked(&mut runtime));
    let (client, looking) = (Uuid::now_v7(), CancellationToken::new());
    show(&mut runtime, client, &looking, &[id]);
    assert!(!runtime.entries()[0].attention);
    runtime.visible.remove(&client);

    reports(&mut runtime, &remote, true, &json!({"state":"working"}));
    reports(&mut runtime, &remote, true, &json!({"state":"blocked"}));
    tokio::time::advance(Duration::from_secs(1)).await;
    reports(&mut runtime, &remote, true, &json!({"state":"working"}));
    tokio::time::advance(Duration::from_secs(3)).await;
    assert!(!marked(&mut runtime));

    reports(&mut runtime, &remote, true, &json!({"state":"error"}));
    tokio::time::advance(Duration::from_secs(3)).await;
    assert!(marked(&mut runtime));
    reports(&mut runtime, &remote, false, &Value::Null);
    reports(&mut runtime, &remote, true, &json!({"state":"error"}));
    tokio::time::advance(Duration::from_secs(3)).await;
    assert!(marked(&mut runtime));
    show(&mut runtime, client, &looking, &[id]);
    runtime.visible.remove(&client);
    reports(&mut runtime, &remote, false, &Value::Null);
    reports(&mut runtime, &remote, true, &json!({"state":"error"}));
    tokio::time::advance(Duration::from_secs(3)).await;
    assert!(!marked(&mut runtime));
    assert_eq!(
        calls(&mut events, &mut None),
        [
            (Some(Waiting::Done), crate::protocol::Cue::List),
            (Some(Waiting::Error), crate::protocol::Cue::List)
        ]
    );
}

async fn next_list(events: &mut broadcast::Receiver<Event>) -> Option<Vec<SessionEntry>> {
    timeout(Duration::from_secs(1), async {
        loop {
            if let crate::protocol::EventBody::SessionsSnapshot { sessions } =
                events.recv().await.unwrap().event
            {
                return sessions;
            }
        }
    })
    .await
    .ok()
}

#[tokio::test(start_paused = true)]
async fn a_title_that_changes_often_reaches_the_apps_once_a_moment_and_only_when_it_changed() {
    let (mut runtime, _root, changes) = loop_fixture().await;
    let remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    runtime.replace_remotes(vec![remote.clone()]);
    let (client, lifetime) = (Uuid::now_v7(), CancellationToken::new());
    let (_view, updates) = open_shown(&mut runtime, &remote, client, &lifetime).await;
    let mut events = runtime.events.subscribe();
    let (_requests, looping) = start_loop(runtime, changes).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    while events.try_recv().is_ok() {}
    let titled = |title: &str| {
        crate::network::RemoteUpdate::Metadata(crate::terminal::TerminalMetadata {
            title: Some(title.to_owned()),
            ..Default::default()
        })
    };

    updates.send(titled("one")).await.unwrap();
    let first = Instant::now();
    let listed = next_list(&mut events).await.expect("a list at once");
    assert_eq!(listed[0].title.as_deref(), Some("one"));
    assert_eq!(first.elapsed(), Duration::ZERO);
    for title in ["two", "three"] {
        tokio::time::sleep(Duration::from_millis(10)).await;
        updates.send(titled(title)).await.unwrap();
    }
    let listed = next_list(&mut events)
        .await
        .expect("a list within the moment");
    assert_eq!(listed[0].title.as_deref(), Some("three"));
    assert!(first.elapsed() <= catalog::PACE);

    tokio::time::sleep(Duration::from_secs(1)).await;
    updates.send(titled("three")).await.unwrap();
    assert!(next_list(&mut events).await.is_none());
    looping.abort();
}

#[tokio::test(start_paused = true)]
async fn a_terminal_of_the_person_on_another_computer_has_the_mark_that_its_host_reports() {
    use crate::terminal::Waiting;
    let (mut runtime, _root) = registry_fixture();
    runtime.network.assume_identity(crate::network::Identity {
        user_id: "me".to_owned(),
        device_id: "phone".to_owned(),
        display_name: "Me".to_owned(),
        enrolled: true,
    });
    runtime.scope.user = Some("me".to_owned());
    let mut remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    remote.owner_user_id = "me".to_owned();
    remote.status = Some(TerminalStatus {
        state: Some(crate::terminal::ProgramState::Done),
        attention: true,
        ..TerminalStatus::default()
    });
    let mut events = runtime.events.subscribe();
    runtime.replace_remotes(vec![remote.clone()]);
    let marked = |runtime: &Runtime| runtime.entries()[0].attention;
    assert!(marked(&runtime));
    reports(
        &mut runtime,
        &remote,
        true,
        &json!({"state":"done","attention":true}),
    );
    reports(&mut runtime, &remote, true, &json!({"state":"working"}));
    assert!(!marked(&runtime));
    reports(&mut runtime, &remote, true, &json!({"state":"blocked"}));
    tokio::time::advance(Duration::from_secs(3)).await;
    runtime.attend();
    assert!(!marked(&runtime));
    reports(
        &mut runtime,
        &remote,
        true,
        &json!({"state":"blocked","attention":true}),
    );
    assert!(marked(&runtime));
    reports(&mut runtime, &remote, false, &Value::Null);
    assert!(marked(&runtime));
    reports(
        &mut runtime,
        &remote,
        true,
        &json!({"state":"blocked","attention":true}),
    );
    reports(&mut runtime, &remote, true, &json!({"state":"blocked"}));
    assert!(!marked(&runtime));
    assert_eq!(
        calls(&mut events, &mut None),
        [(Some(Waiting::Blocked), crate::protocol::Cue::List)]
    );
}

async fn seen_reached_host(
    requests: &mut mpsc::Receiver<crate::network::TestRemoteRequest>,
) -> bool {
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut seen = false;
    while let Ok(request) = requests.try_recv() {
        if let crate::network::TestRemoteRequest::Control {
            control: TerminalControl::Seen,
            reply,
        } = request
        {
            drop(reply.send(Ok(json!({"accepted":true}))));
            seen = true;
        }
    }
    seen
}

#[tokio::test]
async fn a_person_in_kodosi_who_looks_at_a_marked_terminal_tells_its_host() {
    let (mut runtime, _root) = registry_fixture();
    runtime.network.assume_identity(crate::network::Identity {
        user_id: "me".to_owned(),
        device_id: "phone".to_owned(),
        display_name: "Me".to_owned(),
        enrolled: true,
    });
    runtime.scope.user = Some("me".to_owned());
    let mut remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    remote.owner_user_id = "me".to_owned();
    remote.status = Some(TerminalStatus {
        state: Some(crate::terminal::ProgramState::Blocked),
        attention: true,
        ..TerminalStatus::default()
    });
    let id = remote.id;
    runtime.replace_remotes(vec![remote.clone()]);
    let (client, lifetime) = (Uuid::now_v7(), CancellationToken::new());
    reports_presence(&mut runtime, (client, &lifetime), Presence::Idle);
    show(&mut runtime, client, &lifetime, &[id]);
    runtime
        .apply_for_consumer(&open_remote(id), client, lifetime.clone())
        .unwrap();
    let (connection, mut requests, updates) =
        crate::network::test_remote_connection(remote.clone());
    connected(&mut runtime, id, Ok(connection));
    let view = runtime.connections[&id].subscribe();
    requests.recv().await.unwrap();
    updates.send(first_snapshot()).await.unwrap();
    let _view = view.await.unwrap();

    runtime.attend();
    assert!(!seen_reached_host(&mut requests).await);
    reports_presence(&mut runtime, (client, &lifetime), Presence::Kodosi);
    assert!(seen_reached_host(&mut requests).await);
    runtime.attend();
    assert!(!seen_reached_host(&mut requests).await);
    assert!(runtime.entries()[0].attention);
    reports(&mut runtime, &remote, true, &json!({"state":"blocked"}));
    assert!(!runtime.entries()[0].attention);
}

#[tokio::test(start_paused = true)]
async fn a_marked_terminal_that_its_person_looked_at_while_its_link_was_lost_hears_it_once_back() {
    let (mut runtime, _root, changes) = loop_fixture().await;
    runtime.network.assume_identity(crate::network::Identity {
        user_id: "me".to_owned(),
        device_id: "phone".to_owned(),
        display_name: "Me".to_owned(),
        enrolled: true,
    });
    runtime.scope.user = Some("me".to_owned());
    let mut remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    remote.owner_user_id = "me".to_owned();
    remote.status = Some(TerminalStatus {
        state: Some(crate::terminal::ProgramState::Blocked),
        attention: true,
        ..TerminalStatus::default()
    });
    let id = remote.id;
    runtime.replace_remotes(vec![remote.clone()]);
    let (client, lifetime) = (Uuid::now_v7(), CancellationToken::new());
    reports_presence(&mut runtime, (client, &lifetime), Presence::Idle);
    let (_view, updates) = open_shown(&mut runtime, &remote, client, &lifetime).await;
    let terminal = runtime.connections[&id].clone();
    let looks = CommandEnvelope {
        account_user_id: runtime.scope.user.clone(),
        account_epoch: runtime.scope.epoch,
        computer: None,
        command: Command::Presence {
            request_id: Uuid::now_v7().to_string(),
            state: Presence::Kodosi,
        },
    };
    let mut events = runtime.events.subscribe();
    let (requests, looping) = start_loop(runtime, changes).await;
    updates
        .send(crate::network::RemoteUpdate::Closed {
            reason: crate::text::Text::plain("link lost"),
        })
        .await
        .unwrap();
    loop {
        if let crate::protocol::EventBody::SessionsSnapshot { sessions } =
            events.recv().await.unwrap().event
            && sessions.iter().any(|entry| {
                entry.id == id.to_string() && entry.status == SessionStatus::Reconnecting
            })
        {
            break;
        }
    }
    requests
        .send(Request::Command {
            envelope: looks,
            consumer: client,
            lifetime: lifetime.clone(),
            origin: Origin::App,
        })
        .await
        .unwrap();
    while events.recv().await.unwrap().recipient != Some(client) {}
    let (again, mut host, updates) = crate::network::test_remote_connection(remote);
    assert!(terminal.resume(again));
    drop(terminal);
    updates.send(first_snapshot()).await.unwrap();
    assert!(seen_reached_host(&mut host).await);
    looping.abort();
}

#[cfg(all(unix, feature = "host"))]
async fn hosted(runtime: &mut Runtime, folder: &Path) -> (Uuid, TerminalSession) {
    let id = Uuid::now_v7();
    let terminal = TerminalSession::spawn(
        id,
        Uuid::now_v7(),
        PathBuf::from(SHELL),
        vec!["-c".to_owned(), "exec cat".to_owned()],
        folder.to_owned(),
        None,
        None,
        Vec::new(),
        Default::default(),
        stand_in_canvas(),
        crate::chat::TerminalChat::alone(),
        runtime.look.watch(),
        std::env::temp_dir().join(Uuid::now_v7().to_string()),
        runtime.changes.clone(),
    )
    .await
    .unwrap();
    let entry = serde_json::from_value(json!({
        "id": id, "incarnationId": terminal.incarnation_id, "kind": "local", "name": "Build",
        "isOwner": true, "status": "running", "connectionState": "local", "prompt": false,
        "repository": false, "working": false, "sharedWith": [], "attention": false,
        "connectedUsers": [], "hidden": false, "look": "dark",
    }))
    .unwrap();
    runtime.local.insert(
        id,
        Local {
            terminal: terminal.clone(),
            entry,
            creation: Command::ListSessions {
                request_id: Uuid::now_v7().to_string(),
            },
            published: true,
            project_sequence: 0,
            worktree: None,
            wanted: None,
            conversation: restore::Conversation::default(),
            changed: crate::agents::AgentSettings::default(),
            account_folder: None,
            marked: watch::channel(None).0,
        },
    );
    runtime.publish_catalog();
    (id, terminal)
}

#[cfg(all(unix, feature = "host"))]
fn reports_state(runtime: &mut Runtime, id: Uuid, state: crate::terminal::ProgramState) {
    runtime.local.get_mut(&id).unwrap().entry.program_status =
        Some(crate::terminal::ProgramStatus {
            state,
            kind: None,
            progress: None,
            title: None,
            message: None,
        });
    runtime.publish_catalog();
}

fn reports_presence(
    runtime: &mut Runtime,
    (client, lifetime): (Uuid, &CancellationToken),
    state: Presence,
) {
    runtime
        .apply_for_consumer(
            &Command::Presence {
                request_id: Uuid::now_v7().to_string(),
                state,
            },
            client,
            lifetime.clone(),
        )
        .unwrap();
}

#[cfg(all(unix, feature = "host"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_program_notification_marks_and_shows_only_outside_kodosi() {
    use crate::{
        protocol::Cue,
        terminal::{ProgramState, Waiting},
    };
    let (mut runtime, storage) = registry_fixture();
    let (id, terminal) = hosted(&mut runtime, storage.path()).await;
    let mark = runtime.local[&id].publication(id).3;
    let mut events = runtime.events.subscribe();
    let mut noted = |runtime: &mut Runtime| {
        runtime.change(SessionChange::Notification {
            id,
            title: "Claude Code".to_owned(),
            body: "Claude is waiting for your input".to_owned(),
        });
        std::iter::from_fn(|| events.try_recv().ok())
            .filter_map(|event| match event.event {
                crate::protocol::EventBody::TerminalNotification { cue, .. } => Some(cue),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let (client, lifetime) = (Uuid::now_v7(), CancellationToken::new());
    let app = (client, &lifetime);

    assert_eq!(noted(&mut runtime), [Cue::List]);
    assert!(runtime.entries()[0].attention);
    reports_presence(&mut runtime, app, Presence::Kodosi);
    show(&mut runtime, client, &lifetime, &[id]);
    assert!(!runtime.entries()[0].attention);
    assert_eq!(noted(&mut runtime), []);
    assert!(!runtime.entries()[0].attention);
    show(&mut runtime, client, &lifetime, &[]);
    assert_eq!(noted(&mut runtime), []);
    assert!(runtime.entries()[0].attention);
    show(&mut runtime, client, &lifetime, &[id]);
    reports_presence(&mut runtime, app, Presence::Here);
    assert_eq!(noted(&mut runtime), [Cue::Banner]);

    reports_state(&mut runtime, id, ProgramState::Done);
    let later = Instant::now() + Duration::from_secs(3);
    assert!(runtime.attention.settle(later, &BTreeSet::new()));
    runtime.publish_catalog();
    assert_eq!(*mark.borrow(), Some(Waiting::Done));
    assert_eq!(noted(&mut runtime), []);
    runtime.change(SessionChange::Seen { id });
    assert_eq!(*mark.borrow(), None);
    assert!(runtime.entries()[0].attention);
    reports_presence(&mut runtime, app, Presence::Kodosi);
    assert!(!runtime.entries()[0].attention);
    assert_eq!(*mark.borrow(), None);
    terminal.close().await.unwrap();
    drop(terminal);
}

#[cfg(all(unix, feature = "host"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wait_that_its_person_saw_on_any_device_gets_no_mark_again_when_the_person_leaves() {
    use crate::terminal::{ProgramState, Waiting};
    let (mut runtime, storage) = registry_fixture();
    let (here, here_terminal) = hosted(&mut runtime, storage.path()).await;
    let (phone, phone_terminal) = hosted(&mut runtime, storage.path()).await;
    let marks = [here, phone].map(|id| runtime.local[&id].publication(id).3);
    let mut events = runtime.events.subscribe();
    let (client, lifetime) = (Uuid::now_v7(), CancellationToken::new());
    let app = (client, &lifetime);
    reports_presence(&mut runtime, app, Presence::Kodosi);
    show(&mut runtime, client, &lifetime, &[here]);
    reports_state(&mut runtime, here, ProgramState::Blocked);
    reports_state(&mut runtime, phone, ProgramState::Blocked);
    let later = Instant::now() + Duration::from_secs(3);
    let watched = runtime.watched();
    assert!(runtime.attention.settle(later, &watched));
    runtime.publish_catalog();
    assert_eq!(*marks[0].borrow(), None);
    assert_eq!(*marks[1].borrow(), Some(Waiting::Blocked));
    runtime.change(SessionChange::Seen { id: phone });

    for away in [Presence::Idle, Presence::Away] {
        reports_presence(&mut runtime, app, away);
        assert!(marks.iter().all(|mark| mark.borrow().is_none()));
        reports_presence(&mut runtime, app, Presence::Kodosi);
    }
    assert_eq!(calls(&mut events, &mut None), Vec::new());
    for terminal in [here_terminal, phone_terminal] {
        terminal.close().await.unwrap();
    }
}

#[tokio::test]
async fn a_wait_that_its_person_leaves_at_another_computer_calls_once_on_this_device() {
    use crate::terminal::Waiting;
    let (mut runtime, _root) = registry_fixture();
    runtime.network.assume_identity(crate::network::Identity {
        user_id: "me".to_owned(),
        device_id: "phone".to_owned(),
        display_name: "Me".to_owned(),
        enrolled: true,
    });
    runtime.scope.user = Some("me".to_owned());
    let mut remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    remote.owner_user_id = "me".to_owned();
    runtime.replace_remotes(vec![remote.clone()]);
    let mut events = runtime.events.subscribe();
    let mut listed = None;
    let elsewhere = |runtime: &mut Runtime, here: bool| {
        let (generation, user_id) = (runtime.network.generation(), runtime.scope.user.clone());
        runtime.network_event(BackendEvent {
            generation,
            user_id,
            event: json!({"type":"network.presence","kodosi":false,"here":here,"heard":true}),
        });
    };
    let waits = json!({"state":"blocked","attention":true});

    elsewhere(&mut runtime, true);
    reports(&mut runtime, &remote, true, &waits);
    assert_eq!(calls(&mut events, &mut listed), Vec::new());
    elsewhere(&mut runtime, false);
    reports(&mut runtime, &remote, true, &waits);
    runtime.attend();
    assert_eq!(
        calls(&mut events, &mut listed),
        [(Some(Waiting::Blocked), crate::protocol::Cue::List)]
    );

    elsewhere(&mut runtime, true);
    reports(&mut runtime, &remote, true, &json!({"state":"blocked"}));
    elsewhere(&mut runtime, false);
    assert_eq!(calls(&mut events, &mut listed), Vec::new());
    reports(&mut runtime, &remote, true, &waits);
    runtime.attend();
    assert_eq!(
        calls(&mut events, &mut listed),
        [(Some(Waiting::Blocked), crate::protocol::Cue::List)]
    );
}

fn sign_in(runtime: &mut Runtime, enrolled: bool) {
    runtime.network.assume_identity(crate::network::Identity {
        user_id: "owner".to_owned(),
        device_id: "this-device".to_owned(),
        display_name: "You".to_owned(),
        enrolled,
    });
    runtime.scope.user = Some("owner".to_owned());
}

fn my_terminal(computer: &str) -> RemoteSession {
    RemoteSession {
        owner_user_id: "owner".to_owned(),
        owner_name: "You".to_owned(),
        host_device_id: computer.to_owned(),
        host_name: computer.to_owned(),
        ..remote_session(Uuid::now_v7(), Uuid::now_v7())
    }
}

fn asking(runtime: &Runtime) -> Vec<&str> {
    let mut computers = runtime
        .folder_asks
        .iter()
        .filter(|(_, ask)| ask.asking)
        .map(|(computer, _)| computer.as_str())
        .collect::<Vec<_>>();
    computers.sort_unstable();
    computers
}

fn folders_from(runtime: &Runtime, computer: &str, asked: &[Uuid], folders: Value) -> Job {
    let mut answer = json!({"type":"session.result","requestId":Uuid::now_v7().to_string(),"operation":"session.folders"});
    answer["folders"] = folders;
    Job {
        scope: runtime.scope.clone(),
        completion: Completion::TerminalFolders {
            device: computer.to_owned(),
            asked: asked.iter().copied().collect(),
            result: Ok(answer),
        },
    }
}

#[tokio::test]
async fn a_device_asks_its_other_computers_that_are_online_for_the_folders_of_their_terminals() {
    let (mut runtime, _root) = registry_fixture();
    sign_in(&mut runtime, false);
    let (api, web) = (my_terminal("laptop"), my_terminal("laptop"));
    let away = RemoteSession {
        online: false,
        ..my_terminal("desk")
    };
    let friends = remote_session(Uuid::now_v7(), Uuid::now_v7());
    let mut listed = vec![api.clone(), web.clone(), away.clone(), friends];
    let nobody = Vec::<&str>::new();
    runtime.replace_remotes(listed.clone());
    assert_eq!(
        asking(&runtime),
        nobody,
        "a device that is not approved asks nobody"
    );

    sign_in(&mut runtime, true);
    runtime.replace_remotes(listed.clone());
    assert_eq!(asking(&runtime), ["laptop"]);
    runtime.ask_folders();
    assert_eq!(
        runtime.jobs.len(),
        1,
        "one call at a time for each computer"
    );
    {
        let unanswered = timeout(Duration::from_secs(30), runtime.jobs.join_next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(matches!(&unanswered.completion,
            Completion::TerminalFolders { device, asked, result: Err(_) }
                if device == "laptop" && *asked == BTreeSet::from([api.id, web.id])));
        runtime.complete(unanswered);
    }
    assert_eq!(
        asking(&runtime),
        nobody,
        "a computer that did not answer waits"
    );
    runtime.folder_asks.get_mut("laptop").unwrap().retry = Some(Instant::now());
    runtime.ask_folders();
    assert_eq!(asking(&runtime), ["laptop"]);
    runtime.jobs.shutdown().await;

    runtime.complete(folders_from(
        &runtime,
        "laptop",
        &[api.id, web.id],
        json!([{"sessionId":api.id,"workingDir":"/srv/api"}]),
    ));
    assert_eq!(
        runtime.remotes[&api.id].working_dir.as_deref(),
        Some("/srv/api")
    );
    assert_eq!(
        asking(&runtime),
        nobody,
        "a terminal that its computer gives no folder for is not asked for again"
    );
    runtime.replace_remotes(listed.clone());
    assert_eq!(asking(&runtime), nobody);

    listed.push(my_terminal("laptop"));
    runtime.replace_remotes(listed);
    assert_eq!(
        asking(&runtime),
        ["laptop"],
        "a new terminal is asked for at once"
    );
    let generation = runtime.network.generation();
    runtime.network_event(BackendEvent {
        generation,
        user_id: Some("owner".to_owned()),
        event: json!({"type":"network.terminal","sessionId":away.id,
            "incarnationId":away.incarnation_id,"online":true,"status":null}),
    });
    assert_eq!(asking(&runtime), ["desk", "laptop"]);
    runtime.jobs.shutdown().await;
}

#[tokio::test]
async fn folders_from_another_computer_fill_only_its_terminals_and_an_open_view_keeps_its_own() {
    let (mut runtime, _root) = registry_fixture();
    sign_in(&mut runtime, true);
    let (api, web, desk) = (
        my_terminal("laptop"),
        my_terminal("laptop"),
        my_terminal("desk"),
    );
    let friends = RemoteSession {
        host_device_id: "laptop".to_owned(),
        ..remote_session(Uuid::now_v7(), Uuid::now_v7())
    };
    runtime.replace_remotes(vec![
        api.clone(),
        web.clone(),
        desk.clone(),
        friends.clone(),
    ]);
    runtime.jobs.shutdown().await;
    let folder =
        |runtime: &Runtime, remote: &RemoteSession| runtime.remotes[&remote.id].working_dir.clone();
    for refused in [
        json!([{"sessionId":api.id,"workingDir":"srv/api"}]),
        json!([{"sessionId":api.id,"workingDir":"/srv/a\u{0}pi"}]),
        json!([{"sessionId":api.id,"workingDir":format!("/{}", "a".repeat(4096))}]),
        json!([{"sessionId":"api","workingDir":"/srv/api"}]),
        json!(
            (0..=MAX_SESSIONS)
                .map(|_| json!({"sessionId":Uuid::now_v7(),"workingDir":"/srv"}))
                .collect::<Vec<_>>()
        ),
    ] {
        runtime.complete(folders_from(&runtime, "laptop", &[api.id], refused));
        assert_eq!(folder(&runtime, &api), None);
    }

    runtime.complete(folders_from(
        &runtime,
        "laptop",
        &[api.id, web.id],
        json!([
            {"sessionId":api.id,"workingDir":"/srv/api"},
            {"sessionId":web.id,"workingDir":r"C:\srv\web"},
            {"sessionId":desk.id,"workingDir":"/srv/desk"},
            {"sessionId":friends.id,"workingDir":"/home/friend"},
        ]),
    ));
    assert_eq!(folder(&runtime, &api).as_deref(), Some("/srv/api"));
    assert_eq!(folder(&runtime, &web).as_deref(), Some(r"C:\srv\web"));
    assert_eq!(folder(&runtime, &desk), None);
    assert_eq!(folder(&runtime, &friends), None);

    let (connection, _requests, _updates) = crate::network::test_remote_connection(web.clone());
    runtime.connections.insert(
        web.id,
        RemoteTerminal::spawn(connection, runtime.changes.clone()),
    );
    runtime.change(SessionChange::Metadata {
        id: web.id,
        incarnation: web.incarnation_id,
        instance_id: runtime.connections[&web.id].instance_id,
        metadata: crate::terminal::TerminalMetadata {
            directory: Some("/srv/web/docs".to_owned()),
            ..Default::default()
        },
    });
    runtime.complete(folders_from(
        &runtime,
        "laptop",
        &[],
        json!([{"sessionId":web.id,"workingDir":"/srv/web"}]),
    ));
    assert_eq!(folder(&runtime, &web).as_deref(), Some("/srv/web/docs"));

    let list = Command::ListSessionFolders {
        request_id: Uuid::now_v7().to_string(),
    };
    runtime
        .expect_answer(&list, Uuid::now_v7(), CancellationToken::new())
        .unwrap();
    let mut events = runtime.events.subscribe();
    runtime.apply(&list).unwrap();
    let answer = std::iter::from_fn(|| events.try_recv().ok())
        .map(|event| serde_json::to_value(event).unwrap())
        .find(|event| event["requestId"] == list.request_id())
        .unwrap();
    assert_eq!(
        answer["folders"],
        json!([]),
        "a device gives no folders of the terminals of other computers"
    );
}

fn know_friends(runtime: &mut Runtime, friends: &[(&str, &str, &str)]) {
    let friends = friends
        .iter()
        .map(|(id, handle, name)| {
            json!({"userId":id,"handle":handle,"displayName":name,
            "verified":true,"identityState":"fixed","inviteNeeded":false})
        })
        .collect::<Vec<_>>();
    runtime.cached_events.insert(
        "friends.snapshot".to_owned(),
        json!({"type":"friends.snapshot","friends":friends,"incoming":[],"outgoing":[],"blocked":[]}),
    );
}

/// The friend list that this account's devices signed, with friends whose identity did not
/// change.
fn signed(friends: &[(&str, &str)]) -> sharing::Friends {
    let mut list = crate::identity::friends::FriendList::new("owner");
    for (user, handle) in friends {
        list.friends.insert(
            (*user).to_owned(),
            crate::identity::friends::Friend {
                handle: (*handle).to_owned(),
                root: None,
                verified: false,
            },
        );
    }
    sharing::Friends {
        list,
        changed: BTreeSet::new(),
    }
}

fn joined(runtime: &mut Runtime, origin: Origin) -> (Uuid, CancellationToken) {
    let (consumer, lifetime) = (Uuid::now_v7(), CancellationToken::new());
    runtime.know_consumer(consumer, origin, &lifetime);
    (consumer, lifetime)
}

fn share(terminal: &RemoteSession, users: &[&str]) -> Command {
    Command::ShareSession {
        request_id: Uuid::now_v7().to_string(),
        session_id: terminal.id.to_string(),
        expected_runtime_incarnation_id: terminal.incarnation_id.to_string(),
        user_ids: users.iter().map(|user| (*user).to_owned()).collect(),
        expected_user_ids: Some(terminal.shared_with.clone()),
    }
}

fn allow(approval: &Value, allow: bool) -> Command {
    Command::AnswerSharing {
        request_id: Uuid::now_v7().to_string(),
        approval_id: approval["approvalId"].as_str().unwrap().to_owned(),
        allow,
    }
}

fn heard_by(events: &mut broadcast::Receiver<Event>) -> Vec<(Option<Uuid>, Value)> {
    std::iter::from_fn(|| events.try_recv().ok())
        .filter(|event| event.event.kind() != "sessions.snapshot")
        .map(|event| {
            let mut value = serde_json::to_value(&event).unwrap();
            crate::text::render(&mut value, crate::text::Language::English);
            (event.recipient, value)
        })
        .collect()
}

fn question(runtime: &Runtime, facts: &sharing::Facts, command: &Command) -> Option<[String; 3]> {
    match runtime.gate(command, Origin::Command, facts).unwrap() {
        sharing::Gate::Ask(question) => {
            Some([question.title, question.message, question.confirm].map(|text| text.to_string()))
        }
        sharing::Gate::Open => None,
    }
}

fn asks(words: [&str; 3]) -> [String; 3] {
    words.map(str::to_owned)
}

fn attach(terminal: &RemoteSession, project: Uuid) -> Command {
    Command::AttachSession {
        request_id: Uuid::now_v7().to_string(),
        session_id: terminal.id.to_string(),
        expected_runtime_incarnation_id: terminal.incarnation_id.to_string(),
        project_id: Some(project.to_string()),
    }
}

fn befriend(username: &str, accept: bool) -> Command {
    let (request_id, username) = (Uuid::now_v7().to_string(), username.to_owned());
    if accept {
        Command::AcceptFriend {
            username,
            request_id,
        }
    } else {
        Command::RequestFriend {
            username,
            request_id,
        }
    }
}

fn trust(username: &str) -> Command {
    Command::TrustFriend {
        username: username.to_owned(),
        request_id: Uuid::now_v7().to_string(),
    }
}

fn verify(username: &str, invite: String) -> Command {
    Command::VerifyFriend {
        username: username.to_owned(),
        invite,
        request_id: Uuid::now_v7().to_string(),
    }
}

fn verified(people: &[&str], listed: Vec<crate::network::ProjectMember>) -> sharing::Membership {
    sharing::Membership {
        verified: Some(people.iter().map(|person| (*person).to_owned()).collect()),
        listed,
    }
}

fn member(id: &str, handle: &str) -> crate::network::ProjectMember {
    crate::network::ProjectMember {
        user_id: id.to_owned(),
        handle: handle.to_owned(),
    }
}

struct Sharing {
    runtime: Runtime,
    _root: tempfile::TempDir,
    ana: String,
    ben: String,
    terminal: RemoteSession,
    events: broadcast::Receiver<Event>,
}

/// An account whose terminal "Build" on the laptop Ana has, with Ben as another friend. The
/// device is not approved, so that it asks its computers for no folders, and its server does not
/// answer, so that this device reads nothing for its questions.
fn sharing_fixture() -> Sharing {
    let (mut runtime, root) = registry_fixture();
    sign_in(&mut runtime, false);
    let (ana, ben) = (Uuid::now_v7().to_string(), Uuid::now_v7().to_string());
    know_friends(
        &mut runtime,
        &[(&ana, "ana", "Ana Lima"), (&ben, "ben", "Ben Okafor")],
    );
    let terminal = RemoteSession {
        name: "Build".to_owned(),
        shared_with: vec![ana.clone()],
        ..my_terminal("laptop")
    };
    runtime.replace_remotes(vec![terminal.clone()]);
    let events = runtime.events.subscribe();
    Sharing {
        runtime,
        _root: root,
        ana,
        ben,
        terminal,
        events,
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "one question from its asking to the share that it lets through"
)]
#[tokio::test]
async fn a_share_that_the_kodosi_command_widens_waits_for_the_app_then_goes_to_its_computer() {
    let Sharing {
        mut runtime,
        ana,
        ben,
        terminal,
        mut events,
        ..
    } = sharing_fixture();
    let (app, app_life) = joined(&mut runtime, Origin::App);
    let (command, command_life) = joined(&mut runtime, Origin::Command);
    let widening = share(&terminal, &[&ana, &ben]);
    runtime.command(
        &as_client(&runtime, widening.clone()),
        command,
        command_life.clone(),
        Origin::Command,
    );
    assert_eq!(
        heard_by(&mut events),
        [],
        "the question waits for what it needs"
    );
    complete_next_job(&mut runtime).await;
    let asked = heard_by(&mut events);
    assert_eq!(asked.len(), 2, "{asked:?}");
    let (question, waiting) = (&asked[0], &asked[1]);
    assert_eq!(question.0, Some(app), "only the app hears the question");
    assert_eq!(
        question.1,
        json!({"type":"sharing.approval","approvalId":question.1["approvalId"],
            "operation":"session.share","title":"Share “Build” with @ben?",
            "message":"@ben gets full control: they can type anything as you on laptop. The kodosi command asked for this.",
            "confirm":"Share","from":"command","accountUserId":"owner","accountEpoch":0}),
        "a person whom this device cannot find in the signed friend list goes by the handle"
    );
    assert_eq!(waiting.0, Some(command));
    assert_eq!(
        (waiting.1["type"].as_str(), waiting.1["message"].as_str()),
        (
            Some("sharing.waiting"),
            Some("Waiting for your OK in the Kodosi app.")
        )
    );
    assert!(runtime.jobs.is_empty(), "nothing runs before the answer");

    let (later, later_life) = (Uuid::now_v7(), CancellationToken::new());
    runtime.join(later, &later_life, Origin::App, oneshot::channel().0);
    let shown = heard_by(&mut events);
    assert_eq!(
        (shown[0].0, &shown[0].1["approvalId"]),
        (Some(later), &question.1["approvalId"]),
        "an app that joins later shows the question too"
    );

    runtime.command(
        &as_client(&runtime, allow(&question.1, true)),
        command,
        command_life,
        Origin::Command,
    );
    let refused = heard_by(&mut events);
    assert_eq!(
        (refused[0].0, refused[0].1["message"].as_str()),
        (
            Some(command),
            Some("Only the Kodosi app can allow sharing.")
        )
    );
    assert!(runtime.jobs.is_empty());

    runtime.command(
        &as_client(&runtime, allow(&question.1, true)),
        app,
        app_life,
        Origin::App,
    );
    let mut answered = heard_by(&mut events)
        .into_iter()
        .map(|(to, event)| (to, event["type"].as_str().unwrap().to_owned()))
        .collect::<Vec<_>>();
    answered.sort();
    let mut expected = vec![
        (Some(app), "sharing.approvalEnded".to_owned()),
        (Some(later), "sharing.approvalEnded".to_owned()),
        (Some(app), "sharing.result".to_owned()),
    ];
    expected.sort();
    assert_eq!(answered, expected);
    assert_eq!(
        runtime
            .destination(&widening, None, Origin::Command)
            .unwrap()
            .as_deref(),
        Some("laptop")
    );
    let sent = timeout(Duration::from_secs(30), runtime.jobs.join_next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        matches!(&sent.completion, Completion::Asked { command, .. } if *command == widening),
        "the allowed share goes to the computer of the terminal as it was sent"
    );
}

#[tokio::test(start_paused = true)]
async fn a_question_that_the_app_refuses_that_waits_too_long_or_that_another_account_finds_ends() {
    let Sharing {
        mut runtime,
        ana,
        ben,
        terminal,
        mut events,
        ..
    } = sharing_fixture();
    let (app, app_life) = joined(&mut runtime, Origin::App);
    let (command, command_life) = joined(&mut runtime, Origin::Command);
    let ask = async |runtime: &mut Runtime, events: &mut broadcast::Receiver<Event>| {
        runtime.command(
            &as_client(runtime, share(&terminal, &[&ana, &ben])),
            command,
            command_life.clone(),
            Origin::Command,
        );
        complete_next_job(runtime).await;
        heard_by(events).swap_remove(0).1
    };
    let ended = |events: &mut broadcast::Receiver<Event>| {
        let heard = heard_by(events);
        let outcome = heard
            .iter()
            .find(|(to, event)| *to == Some(app) && event["type"] == "sharing.approvalEnded")
            .map(|(_, event)| event["outcome"].clone());
        let error = heard
            .iter()
            .find(|(to, event)| *to == Some(command) && event["type"] == "session.error")
            .map(|(_, event)| (event["message"].clone(), event.get("refused").cloned()));
        (outcome, error)
    };

    let question = ask(&mut runtime, &mut events).await;
    runtime.command(
        &as_client(&runtime, allow(&question, false)),
        app,
        app_life,
        Origin::App,
    );
    assert_eq!(
        ended(&mut events),
        (
            Some(json!("refused")),
            Some((
                json!("You did not allow this in Kodosi."),
                Some(json!(true))
            ))
        ),
        "an app ends a request that the owner did not allow without an error"
    );

    ask(&mut runtime, &mut events).await;
    let almost = crate::protocol::APPROVAL_WAIT
        .checked_sub(Duration::from_secs(1))
        .unwrap();
    tokio::time::advance(almost).await;
    runtime.expire_approvals();
    assert_eq!(ended(&mut events), (None, None));
    tokio::time::advance(Duration::from_secs(1)).await;
    runtime.expire_approvals();
    assert_eq!(
        ended(&mut events),
        (
            Some(json!("expired")),
            Some((
                json!("Kodosi did not get your OK in time."),
                Some(json!(true))
            ))
        )
    );

    ask(&mut runtime, &mut events).await;
    runtime.network.assume_identity(crate::network::Identity {
        user_id: "someone else".to_owned(),
        device_id: "this-device".to_owned(),
        display_name: "Other".to_owned(),
        enrolled: true,
    });
    runtime.synchronize_account().unwrap();
    assert_eq!(
        ended(&mut events),
        (
            Some(json!("expired")),
            Some((json!(Error::Stale.text().to_string()), None))
        ),
        "a question that another account ends was never the owner's to refuse"
    );
    assert!(runtime.approvals.is_empty() && runtime.jobs.is_empty());
}

#[tokio::test]
async fn with_no_app_a_widening_is_refused_and_narrowing_or_another_device_asks_nothing() {
    let Sharing {
        mut runtime,
        ana,
        ben,
        terminal,
        mut events,
        ..
    } = sharing_fixture();
    let (command, command_life) = joined(&mut runtime, Origin::Command);
    let mut sent = async |runtime: &mut Runtime, users: &[&str], origin: Origin| {
        runtime.command(
            &as_client(runtime, share(&terminal, users)),
            command,
            command_life.clone(),
            origin,
        );
        if origin == Origin::Command && users.len() > 1 {
            complete_next_job(runtime).await;
        }
        heard_by(&mut events)
            .into_iter()
            .map(|(to, event)| (to, event["type"].clone(), event["message"].clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        sent(&mut runtime, &[&ana, &ben], Origin::Command).await,
        [(
            Some(command),
            json!("session.error"),
            json!("Open the Kodosi app on this computer to allow this.")
        )]
    );
    assert!(runtime.approvals.is_empty() && runtime.jobs.is_empty());
    assert_eq!(
        sent(&mut runtime, &[&ana, &ben], Origin::Device).await,
        [(
            Some(command),
            json!("session.error"),
            json!("Only the computer that hosts this terminal can change it.")
        )],
        "a share from another device was asked there, and does not travel on"
    );
    assert!(runtime.jobs.is_empty());
    assert_eq!(
        sent(&mut runtime, &[], Origin::Command).await,
        Vec::<(Option<Uuid>, Value, Value)>::new()
    );
    assert_eq!(
        runtime.jobs.len(),
        1,
        "a narrowing share goes to the computer of the terminal at once"
    );
    runtime.jobs.shutdown().await;
}

#[tokio::test]
async fn only_the_owner_shares_a_terminal_of_another_computer_and_only_while_it_is_online() {
    let (mut runtime, _root) = registry_fixture();
    sign_in(&mut runtime, false);
    let friends = remote_session(Uuid::now_v7(), Uuid::now_v7());
    let away = RemoteSession {
        online: false,
        ..my_terminal("desk")
    };
    runtime.replace_remotes(vec![friends.clone(), away.clone()]);
    for (terminal, refusal) in [
        (&friends, "Only the owner can share this terminal."),
        (&away, "desk is offline. Share when it is back."),
    ] {
        let refused = runtime
            .destination(&share(terminal, &[]), None, Origin::App)
            .unwrap_err();
        assert_eq!(refused.to_string(), refusal);
    }
}

#[tokio::test]
async fn a_question_comes_only_after_the_checks_that_this_device_can_make() {
    let Sharing {
        mut runtime,
        mut events,
        ..
    } = sharing_fixture();
    let (_app, _app_life) = joined(&mut runtime, Origin::App);
    let (command, command_life) = joined(&mut runtime, Origin::Command);
    let theirs = remote_session(Uuid::now_v7(), Uuid::now_v7());
    runtime.replace_remotes(vec![theirs.clone()]);
    runtime.cached_events.insert(
        "devices.hosts".to_owned(),
        json!({"type":"devices.hosts","deviceIds":["desk"]}),
    );
    let start = json!({"type":"session.create","requestId":Uuid::now_v7(),"name":"New",
        "projectId":Uuid::now_v7()});
    let mut away = as_client(&runtime, serde_json::from_value::<Command>(start).unwrap());
    away.computer = Some("laptop".to_owned());
    for (envelope, refusal) in [
        (
            as_client(&runtime, attach(&theirs, Uuid::now_v7())),
            "Only the owner can change this terminal.",
        ),
        (away, "That computer is not online."),
    ] {
        runtime.command(&envelope, command, command_life.clone(), Origin::Command);
        let heard = heard_by(&mut events);
        assert_eq!(
            heard
                .iter()
                .map(|(_, event)| (event["type"].as_str(), event["message"].as_str()))
                .collect::<Vec<_>>(),
            [(Some("session.error"), Some(refusal))],
            "{heard:?}"
        );
        assert!(runtime.jobs.is_empty() && runtime.approvals.is_empty());
    }
}

#[expect(clippy::too_many_lines, reason = "one case of each rule")]
#[tokio::test]
async fn the_owner_is_asked_only_when_someone_new_would_get_in() {
    let (mut runtime, _root) = registry_fixture();
    sign_in(&mut runtime, false);
    let user = || Uuid::now_v7().to_string();
    let (ana, ben, carl, erin, eve) = (user(), user(), user(), user(), user());
    know_friends(
        &mut runtime,
        &[
            (&ana, "ana", "Ana Lima"),
            (&ben, "ben", "Ben Okafor"),
            (&erin, "erin", "Erin Wu"),
            (&eve, "eve", "Your Bank"),
        ],
    );
    let friends = || signed(&[(&ana, "ana"), (&ben, "ben"), (&erin, "erin")]);
    let project = || Uuid::now_v7();
    let (release, solo, team) = (project(), project(), project());
    let (quiet, crowded, unchecked) = (project(), project(), project());
    runtime.cached_events.insert(
        "projects.snapshot".to_owned(),
        json!({"type":"projects.snapshot","invitations":[],"projects":[
            {"id":release,"name":"Release","ownerUserId":"owner"},
            {"id":solo,"name":"Solo","ownerUserId":"owner"},
            {"id":team,"name":"Team","ownerUserId":"owner"},
            {"id":quiet,"name":"Quiet","ownerUserId":"owner"},
            {"id":crowded,"name":"Crowded","ownerUserId":"owner"},
            {"id":unchecked,"name":"Unchecked","ownerUserId":"owner"}]}),
    );
    let in_release = RemoteSession {
        name: "Deploy".to_owned(),
        project_id: Some(release),
        shared_with: vec![ana.clone()],
        ..my_terminal("laptop")
    };
    let with_ben = RemoteSession {
        name: "Docs".to_owned(),
        shared_with: vec![ben.clone()],
        ..my_terminal("laptop")
    };
    let in_solo = RemoteSession {
        name: "Notes".to_owned(),
        project_id: Some(solo),
        ..my_terminal("laptop")
    };
    let terminals = vec![in_release.clone(), with_ben.clone(), in_solo];
    runtime.replace_remotes(terminals.clone());
    let me = member("owner", "you");
    let facts_with = |release_people: sharing::Membership| sharing::Facts {
        friends: Some(friends()),
        terminals: Some(terminals.clone()),
        members: sharing::Members::from([
            (release, release_people),
            (solo, verified(&["owner"], vec![me.clone()])),
            (
                team,
                verified(&["owner", &ben], vec![me.clone(), member(&ben, "ben")]),
            ),
            (quiet, verified(&["owner", &ben], vec![me.clone()])),
            (
                crowded,
                verified(&["owner"], vec![me.clone(), member(&ben, "ben")]),
            ),
            (unchecked, sharing::Membership::default()),
        ]),
        device: None,
    };
    let listed_release = vec![me.clone(), member(&ana, "ana"), member(&carl, "carl")];
    let all = facts_with(verified(
        &["owner", &ana, &carl, &eve],
        listed_release.clone(),
    ));
    let carl_unlisted = facts_with(verified(
        &["owner", &ana, &carl],
        vec![me.clone(), member(&ana, "ana")],
    ));
    let only_friends = facts_with(verified(&["owner", &ana], listed_release.clone()));
    let unverified = facts_with(sharing::Membership::default());
    let mut resent = share(&with_ben, &[&ben]);
    if let Command::ShareSession {
        expected_user_ids, ..
    } = &mut resent
    {
        *expected_user_ids = None;
    }
    let create = |project: Uuid| {
        let start =
            json!({"type":"session.create","requestId":user(),"projectId":project,"name":"New"});
        serde_json::from_value::<Command>(start).unwrap()
    };
    let root = [0xab; 32];
    let invite = |handle: &str| crate::identity::friends::invite(handle, &root);
    let mut known_root = facts_with(verified(&["owner", &ana, &carl], listed_release));
    if let Some(friends) = &mut known_root.friends {
        friends.list.friends.get_mut(&ben).unwrap().root = Some(crate::hex::hex(&root));
    }
    let mut erin_verified = facts_with(verified(&["owner", &ana, &carl], Vec::new()));
    if let Some(friends) = &mut erin_verified.friends {
        friends.list.friends.get_mut(&erin).unwrap().verified = true;
    }
    let befriend_carl = "@carl is in Release with your terminal there. As your friend, they get full control of it. Be friends only with people you would let use your computer.";
    let befriend_anybody = "Friends you share a terminal or project with get full control of it. Be friends only with people you would let use your computer.";
    let identity = "The identity in the invite starts with abab abab abab abab.";
    let cases = [
        (&all, share(&with_ben, &[&ben]), None),
        (&all, share(&with_ben, &[]), None),
        (
            &all,
            share(&with_ben, &[&ben, &ana, &erin]),
            Some(asks([
                "Share “Docs” with Ana Lima (@ana) and Erin Wu (@erin)?",
                "Ana Lima (@ana) and Erin Wu (@erin) get full control: they can type anything as you on laptop.",
                "Share",
            ])),
        ),
        (
            &all,
            resent,
            Some(asks([
                "Share “Docs” with Ben Okafor (@ben)?",
                "Ben Okafor (@ben) gets full control: they can type anything as you on laptop.",
                "Share",
            ])),
        ),
        (&all, attach(&with_ben, solo), None),
        (&all, attach(&with_ben, team), None),
        (
            &all,
            attach(&in_release, team),
            Some(asks([
                "Share “Deploy” with Team?",
                "Ben Okafor (@ben) gets full control, and so will friends of yours who join later. It leaves Release.",
                "Share",
            ])),
        ),
        (
            &all,
            attach(&in_release, quiet),
            Some(asks([
                "Share “Deploy” with Quiet?",
                "Ben Okafor (@ben) gets full control, and so will friends of yours who join later. It leaves Release.",
                "Share",
            ])),
        ),
        (
            &all,
            attach(&in_release, unchecked),
            Some(asks([
                "Share “Deploy” with Unchecked?",
                "Members of Unchecked who are your friends get full control, and so will friends of yours who join later. It leaves Release.",
                "Share",
            ])),
        ),
        (&all, attach(&in_release, release), None),
        (&all, create(solo), None),
        (&all, create(crowded), None),
        (
            &all,
            create(team),
            Some(asks([
                "Start the new terminal in Team?",
                "Ben Okafor (@ben) gets full control of it, and so will friends of yours who join later.",
                "Start",
            ])),
        ),
        (
            &all,
            create(unchecked),
            Some(asks([
                "Start the new terminal in Unchecked?",
                "Members of Unchecked who are your friends get full control of it, and so will friends of yours who join later.",
                "Start",
            ])),
        ),
        (
            &all,
            Command::InviteProject {
                request_id: user(),
                project_id: release.to_string(),
                user_id: eve.clone(),
            },
            Some(asks([
                "Invite @eve to Release?",
                "@eve sees its chat and tasks and gets full control of your terminal in it.",
                "Invite",
            ])),
        ),
        (
            &all,
            befriend("CARL", true),
            Some(asks(["Accept @carl as a friend?", befriend_carl, "Accept"])),
        ),
        (
            &all,
            befriend("eve", false),
            Some(asks(["Add @eve as a friend?", befriend_anybody, "Add"])),
        ),
        (
            &carl_unlisted,
            befriend("carl", false),
            Some(asks(["Add @carl as a friend?", befriend_anybody, "Add"])),
        ),
        (
            &only_friends,
            befriend("dora", false),
            Some(asks(["Add @dora as a friend?", befriend_anybody, "Add"])),
        ),
        (
            &all,
            befriend(&invite("dora"), false),
            Some(asks([
                "Add @dora as a friend?",
                &format!("{befriend_anybody} {identity}"),
                "Add",
            ])),
        ),
        (
            &all,
            befriend("ana", false),
            Some(asks([
                "Add Ana Lima (@ana) as a friend?",
                befriend_anybody,
                "Add",
            ])),
        ),
        (
            &all,
            trust("ben"),
            Some(asks([
                "Trust the new identity of Ben Okafor (@ben)?",
                "Whoever holds it gets full control of your terminals shared with Ben Okafor (@ben). Trust it only if they told you that their identity changed.",
                "Trust",
            ])),
        ),
        (
            &all,
            trust("erin"),
            Some(asks([
                "Trust the new identity of Erin Wu (@erin)?",
                "Whoever holds it can act as Erin Wu (@erin), with full control of each terminal and project that you share with them. Trust it only if they told you that their identity changed.",
                "Trust",
            ])),
        ),
        (
            &unverified,
            trust("erin"),
            Some(asks([
                "Trust the new identity of Erin Wu (@erin)?",
                "Whoever holds it gets full control of your terminals shared with Erin Wu (@erin). Trust it only if they told you that their identity changed.",
                "Trust",
            ])),
        ),
        (&erin_verified, trust("erin"), None),
        (
            &all,
            verify("ben", invite("ben")),
            Some(asks([
                "Trust the new identity of Ben Okafor (@ben)?",
                &format!(
                    "Whoever holds it gets full control of your terminals shared with Ben Okafor (@ben). {identity}"
                ),
                "Trust",
            ])),
        ),
        (
            &known_root,
            verify("ben", invite("ben")),
            Some(asks([
                "Verify Ben Okafor (@ben)?",
                &format!(
                    "Whoever holds it gets full control of your terminals shared with Ben Okafor (@ben). {identity}"
                ),
                "Verify",
            ])),
        ),
        (
            &all,
            verify("erin", invite("erin")),
            Some(asks([
                "Trust the new identity of Erin Wu (@erin)?",
                &format!(
                    "Whoever holds it can act as Erin Wu (@erin), with full control of each terminal and project that you share with them. {identity}"
                ),
                "Trust",
            ])),
        ),
        (
            &all,
            verify("carl", invite("carl")),
            Some(asks([
                "Add @carl as a friend?",
                &format!("{befriend_carl} {identity}"),
                "Add",
            ])),
        ),
        (&all, verify("carl", invite("ana")), None),
    ];
    for (facts, command, expected) in cases {
        assert_eq!(question(&runtime, facts, &command), expected, "{command:?}");
    }
}

#[tokio::test]
async fn a_device_joins_the_account_only_with_the_owners_ok_in_the_app() {
    let (mut runtime, _root) = registry_fixture();
    sign_in(&mut runtime, true);
    let approve = Command::ApproveDevice {
        code: "abcd efgh jkmn".to_owned(),
        request_id: Uuid::now_v7().to_string(),
    };
    let shows = sharing::Facts {
        device: Some(Ok("Phone of Mallory".to_owned())),
        ..sharing::Facts::default()
    };
    assert_eq!(
        question(&runtime, &shows, &approve),
        Some(asks([
            "Add the device “Phone of Mallory” to your account?",
            "It gets full control of your terminals. Approve it only if it is yours and shows the code ABCD-EFGH-JKMN.",
            "Approve",
        ]))
    );
    let unknown = sharing::Facts {
        device: Some(Err(crate::text::Text::plain("No device has this code."))),
        ..sharing::Facts::default()
    };
    assert_eq!(
        runtime
            .gate(&approve, Origin::Command, &unknown)
            .err()
            .map(|error| error.to_string()),
        Some("No device has this code.".to_owned()),
        "a code that approves nothing asks nothing"
    );
    let offer = Command::OfferDevice {
        request_id: Uuid::now_v7().to_string(),
    };
    let key = Command::CreateRecoveryKey {
        request_id: Uuid::now_v7().to_string(),
    };
    let none = sharing::Facts::default();
    assert_eq!(
        question(&runtime, &none, &offer),
        Some(asks([
            "Show a code that adds a device to your account?",
            "A device that uses the code joins your account and gets full control of your terminals.",
            "Show code",
        ]))
    );
    assert_eq!(
        question(&runtime, &none, &key).map(|[title, ..]| title),
        Some("Make a new recovery key?".to_owned())
    );
    for command in [&approve, &offer, &key] {
        assert!(
            matches!(
                runtime.gate(command, Origin::App, &shows),
                Ok(sharing::Gate::Open)
            ),
            "the app's own screen is the owner's OK"
        );
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "one case of each thing that this device may not know"
)]
#[tokio::test]
async fn the_owner_is_asked_while_this_device_cannot_tell_who_would_get_in() {
    let Sharing {
        mut runtime,
        ana,
        ben,
        terminal,
        ..
    } = sharing_fixture();
    let (team, solo) = (Uuid::now_v7(), Uuid::now_v7());
    runtime.cached_events.insert(
        "projects.snapshot".to_owned(),
        json!({"type":"projects.snapshot","invitations":[],"projects":[
            {"id":team,"name":"Team","ownerUserId":"owner"},
            {"id":solo,"name":"Solo","ownerUserId":"owner"}]}),
    );
    let friends = || Some(signed(&[(&ana, "ana"), (&ben, "ben")]));
    let in_solo = RemoteSession {
        shared_with: Vec::new(),
        project_id: Some(solo),
        ..terminal.clone()
    };
    let facts = |terminals: Option<Vec<RemoteSession>>| sharing::Facts {
        friends: friends(),
        terminals,
        members: sharing::Members::from([
            (team, verified(&["owner", &ben], Vec::new())),
            (solo, verified(&["owner"], Vec::new())),
        ]),
        device: None,
    };
    let fresh = facts(Some(vec![terminal.clone()]));
    let generic = "Members of Team who are your friends get full control, and so will friends of yours who join later.";
    let message = |runtime: &Runtime, facts: &sharing::Facts, command: &Command| {
        question(runtime, facts, command).map(|[_, message, _]| message)
    };
    assert_eq!(
        message(&runtime, &fresh, &attach(&terminal, team)),
        Some(
            "Ben Okafor (@ben) gets full control, and so will friends of yours who join later."
                .to_owned()
        )
    );

    know_friends(&mut runtime, &[(&ana, "ana", "Ana Lima")]);
    assert_eq!(
        message(&runtime, &fresh, &attach(&terminal, team)),
        Some("@ben gets full control, and so will friends of yours who join later.".to_owned()),
        "a friend of the signed list whom the server leaves out still gets in"
    );
    let friends_unknown = sharing::Facts {
        friends: None,
        ..facts(Some(vec![terminal.clone()]))
    };
    assert_eq!(
        message(&runtime, &friends_unknown, &attach(&terminal, team)).as_deref(),
        Some(generic),
        "a device that could not read the signed friend list asks"
    );
    know_friends(
        &mut runtime,
        &[(&ana, "ana", "Ana Lima"), (&ben, "ben", "Ben Okafor")],
    );

    let stale = RemoteSession {
        project_id: Some(team),
        ..in_solo.clone()
    };
    runtime.replace_remotes(vec![stale.clone()]);
    assert_eq!(
        message(&runtime, &facts(Some(vec![in_solo.clone()])), &attach(&stale, team)),
        Some(
            "Ben Okafor (@ben) gets full control, and so will friends of yours who join later. It leaves Solo."
                .to_owned()
        ),
        "the server's list of now decides where the terminal is, not the list that this device keeps"
    );
    assert_eq!(
        message(&runtime, &facts(None), &attach(&stale, team)),
        Some(
            "Ben Okafor (@ben) gets full control, and so will friends of yours who join later."
                .to_owned()
        ),
        "a device that could not read where the terminal is asks"
    );

    runtime.replace_remotes(vec![in_solo.clone()]);
    let in_solo_only = facts(Some(vec![in_solo]));
    assert_eq!(
        message(&runtime, &in_solo_only, &trust("ben")),
        Some(
            "Whoever holds it can act as Ben Okafor (@ben), with full control of each terminal and project that you share with them. Trust it only if they told you that their identity changed."
                .to_owned()
        )
    );
    assert_eq!(
        message(&runtime, &facts(None), &trust("ben")),
        Some(
            "Whoever holds it gets full control of your terminals shared with Ben Okafor (@ben). Trust it only if they told you that their identity changed."
                .to_owned()
        ),
        "a device that could not read the terminals of the account counts the friend in"
    );

    runtime.scope.user = None;
    assert!(matches!(
        runtime.gate(&attach(&terminal, team), Origin::Command, &fresh),
        Err(Error::Refused(text)) if text.to_string() == "Sign in to continue."
    ));
}

#[tokio::test]
async fn requests_that_change_who_has_access_decide_one_after_another() {
    let Sharing {
        mut runtime,
        ana,
        ben,
        terminal,
        mut events,
        ..
    } = sharing_fixture();
    let (app, _app_life) = joined(&mut runtime, Origin::App);
    let (first, first_life) = joined(&mut runtime, Origin::Command);
    let (second, second_life) = joined(&mut runtime, Origin::Command);
    runtime.command(
        &as_client(&runtime, share(&terminal, &[&ana, &ben])),
        first,
        first_life.clone(),
        Origin::Command,
    );
    let accept = befriend("eve", true);
    runtime.command(
        &as_client(&runtime, accept.clone()),
        second,
        second_life,
        Origin::Command,
    );
    let waits = heard_by(&mut events);
    assert_eq!(
        waits
            .iter()
            .map(|(to, event)| (*to, event["type"].as_str(), event["message"].as_str()))
            .collect::<Vec<_>>(),
        [(
            Some(second),
            Some("sharing.waiting"),
            Some("Waiting for an earlier request to end.")
        )]
    );
    assert_eq!(runtime.jobs.len(), 1, "only the first request reads");
    complete_next_job(&mut runtime).await;
    let asked = heard_by(&mut events);
    assert_eq!(asked[0].1["operation"], "session.share");
    assert!(runtime.jobs.is_empty(), "the second request still waits");

    first_life.cancel();
    runtime.expire_approvals();
    runtime.advance_lane();
    assert_eq!(
        runtime.jobs.len(),
        1,
        "the second request reads only after the first one ended"
    );
    complete_next_job(&mut runtime).await;
    let asked = heard_by(&mut events);
    let question = asked
        .iter()
        .find(|(to, event)| *to == Some(app) && event["type"] == "sharing.approval")
        .unwrap();
    assert_eq!(question.1["operation"], "friends.request.accept");
}

#[tokio::test(start_paused = true)]
async fn a_request_whose_wait_runs_out_ends_while_the_question_before_it_stays_open() {
    let Sharing {
        mut runtime,
        ana,
        ben,
        terminal,
        mut events,
        ..
    } = sharing_fixture();
    let (_app, _app_life) = joined(&mut runtime, Origin::App);
    let (first, first_life) = joined(&mut runtime, Origin::Command);
    let (second, second_life) = joined(&mut runtime, Origin::Command);
    runtime.command(
        &as_client(&runtime, share(&terminal, &[&ana, &ben])),
        first,
        first_life,
        Origin::Command,
    );
    runtime.command(
        &as_client(&runtime, befriend("eve", true)),
        second,
        second_life,
        Origin::Command,
    );
    tokio::time::advance(Duration::from_secs(1)).await;
    complete_next_job(&mut runtime).await;
    heard_by(&mut events);
    let rest = crate::protocol::APPROVAL_WAIT
        .checked_sub(Duration::from_secs(1))
        .unwrap();
    tokio::time::advance(rest).await;
    runtime.expire_approvals();
    runtime.advance_lane();
    assert_eq!(
        heard_by(&mut events)
            .iter()
            .map(|(to, event)| (*to, event["type"].as_str(), event["message"].as_str()))
            .collect::<Vec<_>>(),
        [(
            Some(second),
            Some("friends.error"),
            Some("Other requests wait for your OK in the Kodosi app. Answer them first.")
        )]
    );
    assert!(
        runtime.approvals_due() > Some(Instant::now()),
        "no deadline that passed stays due, so the runtime does not wake for it again and again"
    );
}

#[tokio::test]
async fn a_project_choice_asks_when_its_people_cannot_be_verified_and_counts_only_signed_friends() {
    let Sharing {
        mut runtime,
        ana,
        ben,
        terminal,
        mut events,
        ..
    } = sharing_fixture();
    let (app, app_life) = joined(&mut runtime, Origin::App);
    let (command, command_life) = joined(&mut runtime, Origin::Command);
    let team = Uuid::now_v7();
    runtime.cached_events.insert(
        "projects.snapshot".to_owned(),
        json!({"type":"projects.snapshot","invitations":[],"projects":[
            {"id":team,"name":"Team","ownerUserId":"owner"}]}),
    );
    runtime.command(
        &as_client(&runtime, attach(&terminal, team)),
        command,
        command_life.clone(),
        Origin::Command,
    );
    assert_eq!(heard_by(&mut events), []);
    complete_next_job(&mut runtime).await;
    let unverified = heard_by(&mut events);
    assert_eq!(
        (unverified[0].0, &unverified[0].1["message"]),
        (
            Some(app),
            &json!(
                "Members of Team who are your friends get full control, and so will friends of yours who join later. The kodosi command asked for this."
            )
        ),
        "a project whose people this device cannot verify asks"
    );
    assert_eq!(unverified[1].1["type"], "sharing.waiting");

    runtime.command(
        &as_client(&runtime, allow(&unverified[0].1, false)),
        app,
        app_life,
        Origin::App,
    );
    let hidden = attach(&terminal, team);
    runtime.command(
        &as_client(&runtime, hidden.clone()),
        command,
        command_life.clone(),
        Origin::Command,
    );
    runtime.jobs.shutdown().await;
    heard_by(&mut events);
    let requester = sharing::Requester {
        consumer: command,
        lifetime: command_life,
        origin: Origin::Command,
    };
    let eve = Uuid::now_v7().to_string();
    know_friends(
        &mut runtime,
        &[
            (&ana, "ana", "Ana Lima"),
            (&ben, "ben", "Ben Okafor"),
            (&eve, "eve", "Eve Adams"),
        ],
    );
    let facts = |people: &[&str]| sharing::Facts {
        friends: Some(signed(&[(&ana, "ana"), (&ben, "ben")])),
        terminals: Some(vec![terminal.clone()]),
        members: sharing::Members::from([(team, verified(people, vec![member("owner", "you")]))]),
        device: None,
    };
    runtime.complete(Job {
        scope: runtime.scope.clone(),
        completion: Completion::Checked {
            held: sharing::Held::new(&hidden, None, requester),
            facts: facts(&["owner", &ben, &eve]),
        },
    });
    let asked = heard_by(&mut events);
    assert_eq!(
        (asked[0].0, &asked[0].1["operation"], &asked[0].1["message"]),
        (
            Some(app),
            &json!("session.attachProject"),
            &json!(
                "Ben Okafor (@ben) gets full control, and so will friends of yours who join later. The kodosi command asked for this."
            )
        ),
        "a signed friend whom the server leaves out of the project asks, and a friend whom only the server lists is not named"
    );
    assert_eq!(asked[1].1["type"], "sharing.waiting");
    assert_eq!(
        question(&runtime, &facts(&["owner", &eve]), &attach(&terminal, team)),
        None,
        "a friend whom only the server lists gets no terminal, so nobody is asked"
    );
}

#[tokio::test]
async fn reading_the_state_again_gives_an_app_its_open_questions_after_the_ready_event() {
    let Sharing {
        mut runtime,
        ana,
        ben,
        terminal,
        ..
    } = sharing_fixture();
    let (app, _app_life) = joined(&mut runtime, Origin::App);
    let (command, command_life) = joined(&mut runtime, Origin::Command);
    runtime.command(
        &as_client(&runtime, share(&terminal, &[&ana, &ben])),
        command,
        command_life,
        Origin::Command,
    );
    complete_next_job(&mut runtime).await;
    let mut observed = |consumer: Uuid| {
        let (reply, mut answer) = oneshot::channel();
        runtime.request(Request::Observe { consumer, reply });
        let (events, _) = answer.try_recv().unwrap().unwrap();
        events
            .into_iter()
            .map(|event| (event.recipient, event.event.kind().to_owned()))
            .collect::<Vec<_>>()
    };
    let again = observed(app);
    assert_eq!(again[0].1, "system.ready");
    assert_eq!(
        again.last(),
        Some(&(Some(app), "sharing.approval".to_owned())),
        "an app that clears its questions on the ready event gets them again after it"
    );
    assert!(
        !observed(command)
            .iter()
            .any(|(_, kind)| kind == "sharing.approval"),
        "only an app answers questions"
    );
}

#[cfg(all(unix, feature = "host"))]
async fn running_here(runtime: &mut Runtime) -> Uuid {
    runtime.config.initial_shell = Some(SHELL.to_owned());
    let start = Command::CreateSession {
        request_id: Uuid::now_v7().to_string(),
        name: "Here".to_owned(),
        working_dir: None,
        project_id: None,
        resume: None,
        branch: None,
        worktrees: None,
        command: None,
        agent: None,
        prompt: None,
        like: None,
        size: None,
    };
    runtime
        .expect_answer(&start, Uuid::now_v7(), CancellationToken::new())
        .unwrap();
    runtime.apply(&start).unwrap();
    timeout(Duration::from_secs(20), async {
        while runtime.local.is_empty() {
            complete_next_job(runtime).await;
        }
    })
    .await
    .unwrap();
    *runtime.local.keys().next().unwrap()
}

#[cfg(all(unix, feature = "host"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_share_of_a_terminal_here_asks_only_once_it_can_run() {
    let (mut runtime, _root) = registry_fixture();
    sign_in(&mut runtime, false);
    let ben = Uuid::now_v7().to_string();
    know_friends(&mut runtime, &[(&ben, "ben", "Ben Okafor")]);
    let id = running_here(&mut runtime).await;
    let incarnation = runtime.local[&id].terminal.incarnation_id;
    let (_app, _app_life) = joined(&mut runtime, Origin::App);
    let (command, command_life) = joined(&mut runtime, Origin::Command);
    let mut events = runtime.events.subscribe();
    for (expected, refusal) in [
        (
            vec![ben.clone()],
            "The sharing of this terminal changed. Open sharing again before you save.",
        ),
        (
            Vec::new(),
            "Remote access is still connecting. Try again when this device is online and approved.",
        ),
    ] {
        let widening = Command::ShareSession {
            request_id: Uuid::now_v7().to_string(),
            session_id: id.to_string(),
            expected_runtime_incarnation_id: incarnation.to_string(),
            user_ids: vec![ben.clone()],
            expected_user_ids: Some(expected),
        };
        runtime.command(
            &as_client(&runtime, widening),
            command,
            command_life.clone(),
            Origin::Command,
        );
        assert_eq!(
            heard_by(&mut events)
                .iter()
                .map(|(_, event)| (event["type"].as_str(), event["message"].as_str()))
                .collect::<Vec<_>>(),
            [(Some("session.error"), Some(refusal))]
        );
        assert!(runtime.approvals.is_empty());
    }
    let terminal = runtime.local.remove(&id).unwrap().terminal;
    terminal.close().await.unwrap();
    drop(terminal);
}

#[cfg(all(unix, feature = "host"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sign_in_of_the_kodosi_command_asks_before_another_account_takes_the_terminals_here() {
    let (mut runtime, _root) = registry_fixture();
    sign_in(&mut runtime, false);
    let id = running_here(&mut runtime).await;
    runtime.terminals_owner = Some("owner".to_owned());
    let mut events = runtime.events.subscribe();
    let signing = |user: &str| {
        let (reply, answer) = oneshot::channel();
        let sign_in = crate::network::SignIn {
            user_id: user.to_owned(),
            handle: "intruder".to_owned(),
            request_id: Uuid::now_v7().to_string(),
            reply,
        };
        (sign_in, answer)
    };
    let decided = |runtime: &mut Runtime, origin: Origin, user: &str| {
        runtime.sign_in_origin = Some(origin);
        let (sign_in, mut answer) = signing(user);
        runtime.sign_in_asked(sign_in);
        answer
            .try_recv()
            .map(|answer| answer.map_err(|error| error.text().to_string()))
    };
    assert!(matches!(
        decided(&mut runtime, Origin::App, "intruder"),
        Ok(Ok(()))
    ));
    assert!(matches!(
        decided(&mut runtime, Origin::Command, "owner"),
        Ok(Ok(()))
    ));
    assert_eq!(
        decided(&mut runtime, Origin::Command, "intruder"),
        Ok(Err(
            "Close the terminals on this computer, or sign in from the Kodosi app.".to_owned()
        )),
        "a computer without the app refuses while the terminals run"
    );

    let (app, app_life) = joined(&mut runtime, Origin::App);
    let (sign_in, mut answer) = signing("intruder");
    runtime.sign_in_asked(sign_in);
    let asked = heard_by(&mut events);
    assert_eq!(
        (
            asked[0].0,
            &asked[0].1["operation"],
            &asked[0].1["title"],
            &asked[0].1["message"]
        ),
        (
            Some(app),
            &json!("auth.login.start"),
            &json!("Sign in as intruder?"),
            &json!(
                "The terminal that runs on this computer moves to that account, which gets full control of it. The kodosi command asked for this."
            )
        )
    );
    assert!(
        answer.try_recv().is_err(),
        "the sign-in waits for the owner"
    );
    runtime.command(
        &as_client(&runtime, allow(&asked[0].1, true)),
        app,
        app_life,
        Origin::App,
    );
    assert!(matches!(answer.try_recv(), Ok(Ok(()))));
    let terminal = runtime.local.remove(&id).unwrap().terminal;
    terminal.close().await.unwrap();
    drop(terminal);
}

#[cfg(feature = "host")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn another_device_shares_a_terminal_of_this_computer_but_cannot_answer_its_questions() {
    let runtime = TestRuntime::new().await;
    let (session, incarnation) = runtime.create().await;
    let caller = runtime.handle.command_client(Origin::Device);
    let asked = Uuid::now_v7().to_string();
    let answer = caller
        .answer_call(
            json!({"type":"session.share","requestId":asked,"sessionId":session,
            "expectedRuntimeIncarnationId":incarnation,"userIds":[Uuid::now_v7()]}),
        )
        .await;
    assert_eq!(
        (answer["type"].as_str(), answer["requestId"].as_str()),
        (Some("session.error"), Some(asked.as_str()))
    );
    assert_eq!(
        answer["message"]["$text"],
        not_published().words(),
        "the computer that hosts the terminal shares it itself"
    );
    let answer = caller
        .answer_call(json!({"type":"sharing.answer","requestId":asked,
            "approvalId":Uuid::now_v7(),"allow":true}))
        .await;
    assert_eq!(answer["type"], "sharing.error");
    drop(caller);
    runtime.shutdown().await;
    drop(runtime);
}

#[cfg(feature = "host")]
#[test]
fn a_restored_server_of_the_chat_loads_its_conversation_again() {
    let id = "c0ffee00-1111-4222-8333-444455556666";
    let line = |arguments: &[&str]| {
        let arguments = arguments
            .iter()
            .map(|argument| (*argument).to_owned())
            .collect::<Vec<_>>();
        restore::resumed_server_line(&arguments, id, Path::new("/bin/zsh"))
    };
    assert_eq!(
        line(&[
            "kodosi",
            "acp",
            "--mode",
            "auto",
            "--",
            "agy",
            "acp",
            "--dangerously-skip-permissions"
        ])
        .unwrap(),
        format!("kodosi acp --resume {id} --mode auto -- agy acp --dangerously-skip-permissions")
    );
    assert_eq!(
        line(&[
            "kodosi",
            "acp",
            "--resume",
            "old",
            "--model=Gemini 3 Pro",
            "--",
            "agy",
            "acp"
        ])
        .unwrap(),
        format!("kodosi acp --resume {id} '--model=Gemini 3 Pro' -- agy acp")
    );
    assert_eq!(line(&["agy", "acp"]), None);
    assert_eq!(line(&["kodosi", "acp", "agy", "acp"]), None);
}

#[cfg(feature = "host")]
#[test]
fn a_restored_agent_continues_its_conversation_or_lists_them() {
    use crate::agents::Agent;
    let id = "0198f0c4-5e6f-7a8b-9c0d-1e2f3a4b5c6d";
    let line = |agent, command, conversation| restore::resume_line(agent, command, conversation);
    let zsh = crate::agents::Notes {
        shell: "/bin/zsh".into(),
        ..crate::agents::Notes::default()
    };
    let zsh = &zsh;
    assert_eq!(
        restore::usual_line("copilot", zsh).unwrap(),
        "copilot --resume"
    );
    assert_eq!(
        restore::usual_line("claude", zsh).unwrap(),
        "claude --resume"
    );
    assert_eq!(restore::usual_line("vim", zsh), None);
    let home = Path::new("/home/ana");
    let restart = |agent, account: &str, arguments: &[&str]| {
        let arguments = arguments
            .iter()
            .map(|argument| (*argument).to_owned())
            .collect::<Vec<_>>();
        restore::restart_command(
            agent,
            home,
            &home.join(account),
            &arguments,
            &crate::agents::AgentSettings::default(),
            zsh,
        )
    };
    let claude = restart(
        Agent::Claude,
        ".claude",
        &["claude", "--model", "opus", "--effort", "high"],
    );
    assert_eq!(
        line(Agent::Claude, &claude, Some(id)).unwrap(),
        format!("claude --model opus --effort high --resume {id}")
    );
    let codex = restart(
        Agent::Codex,
        ".codex",
        &[
            "codex",
            "-m",
            "gpt-5.6-sol",
            "-c",
            "model_reasoning_effort=high",
        ],
    );
    assert_eq!(
        line(Agent::Codex, &codex, None).unwrap(),
        "codex -m gpt-5.6-sol -c model_reasoning_effort=high --no-daemon -c \"developer_instructions=$KODOSI_AGENT_CONTEXT\" resume"
    );
}

#[cfg(feature = "host")]
#[test]
fn a_codex_that_comes_back_through_its_server_keeps_the_permissions_of_its_conversation() {
    use crate::agents::Agent;
    let id = "019f0000-0000-7000-8000-000000000001";
    let home = Path::new("/home/ana");
    let served = crate::agents::Notes {
        shell: "/bin/zsh".into(),
        codex_servers: Some("unix:///tmp/kodosi-501-a/codex-".to_owned()),
        ..crate::agents::Notes::default()
    };
    let command = restore::restart_command(
        Agent::Codex,
        home,
        &home.join(".codex"),
        &[
            "codex",
            "-m",
            "gpt-5.6-sol",
            "-a",
            "on-request",
            "-s",
            "read-only",
        ]
        .map(str::to_owned),
        &crate::agents::AgentSettings {
            permissions: Some(crate::agents::Permissions::Auto),
            ..crate::agents::AgentSettings::default()
        },
        &served,
    );
    assert_eq!(
        restore::resume_line(Agent::Codex, &command, Some(id)).unwrap(),
        format!("codex -m gpt-5.6-sol --remote \"$KODOSI_CODEX_SERVER\" resume {id}")
    );
}

#[cfg(feature = "host")]
#[test]
fn a_terminal_comes_back_to_the_conversation_its_agent_had_when_the_agent_stops_first() {
    use restore::{AgentLine, Conversation};
    let now = Instant::now();
    let later = |seconds| now + Duration::from_secs(seconds);
    let named = |line: &str| {
        Some(AgentLine {
            line: line.to_owned(),
            named: true,
        })
    };
    let listing = || {
        Some(AgentLine {
            line: "copilot --resume".to_owned(),
            named: false,
        })
    };
    let chosen = |fresh, known: &Conversation, at| restore::chosen_line(fresh, known, at);

    let mut known = Conversation::default();
    assert_eq!(
        chosen(listing(), &known, now).as_deref(),
        Some("copilot --resume")
    );
    assert!(known.ask(now));
    assert!(!known.ask(later(11)));
    known.found(named("copilot --resume=A"));
    assert!(!known.ask(later(29)));
    assert!(known.ask(later(31)));
    known.found(None);

    assert_eq!(
        chosen(named("copilot --resume=B"), &known, now).as_deref(),
        Some("copilot --resume=B")
    );
    assert_eq!(
        chosen(listing(), &known, now).as_deref(),
        Some("copilot --resume=A")
    );
    assert_eq!(
        chosen(None, &known, now).as_deref(),
        Some("copilot --resume=A")
    );

    let quiet = PathBuf::from("/home/ana/.copilot/settings.json");
    known.program_changed(true, now, Some(quiet.clone()));
    assert_eq!(
        chosen(None, &known, later(9)).as_deref(),
        Some("copilot --resume=A")
    );
    assert_eq!(
        known.quiet_settings(later(9)),
        Some(quiet),
        "a quiet Copilot CLI comes back quiet"
    );
    assert_eq!(chosen(None, &known, later(11)), None);
    assert_eq!(known.quiet_settings(later(11)), None);

    known.program_changed(false, later(12), None);
    assert_eq!(
        chosen(listing(), &known, later(12)).as_deref(),
        Some("copilot --resume")
    );
}

#[cfg(feature = "host")]
#[test]
fn an_agent_whose_conversation_is_not_known_yet_comes_back_with_its_account_and_choices() {
    use restore::{AgentLine, Conversation};
    let now = Instant::now();
    let line = |line: &str, named| {
        Some(AgentLine {
            line: line.to_owned(),
            named,
        })
    };
    let chosen = |fresh, known: &Conversation| restore::chosen_line(fresh, known, now);
    let listing = "env CODEX_HOME=/home/ana/.codex-work codex -m gpt-5.6-sol --dangerously-bypass-approvals-and-sandbox resume";

    let mut known = Conversation::default();
    assert!(known.ask(now));
    known.found(line(listing, false));
    assert_eq!(
        chosen(None, &known).as_deref(),
        Some(listing),
        "a host that ends without asking keeps the account and the choices"
    );
    assert_eq!(
        chosen(line("codex -m gpt-5.6-terra resume", false), &known).as_deref(),
        Some("codex -m gpt-5.6-terra resume"),
        "a host that asks at its end takes the choices of that moment"
    );
    let named = format!("{listing} 0198f0c4-5e6f-7a8b-9c0d-1e2f3a4b5c6d");
    known.found(line(&named, true));
    assert_eq!(chosen(None, &known), Some(named));

    let mut ended = Conversation::default();
    ended.found(line(listing, false));
    ended.program_changed(true, now, None);
    assert_eq!(
        chosen(None, &ended),
        None,
        "an agent that ended before its conversation was known stays ended"
    );
}

#[cfg(all(unix, feature = "host"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stopping_agent_gives_the_line_that_continues_its_conversation_with_its_choices() {
    let root = tempfile::tempdir().unwrap();
    let home = kodosi_os::fs::canonicalize(root.path()).unwrap();
    let id = "0198f0c4-5e6f-7a8b-9c0d-1e2f3a4b5c6d";
    let session = home.join(".copilot/session-state").join(id);
    let rollout = home.join(format!(
        ".codex/sessions/2026/11/08/rollout-2026-11-08T20-14-39-{id}.jsonl"
    ));
    std::fs::create_dir_all(&session).unwrap();
    std::fs::create_dir_all(rollout.parent().unwrap()).unwrap();
    let meta = json!({"type":"session_meta","payload":{"id":id,"cwd":home,"source":"cli"}});
    std::fs::write(&rollout, format!("{meta}\n")).unwrap();
    let (changes, _changes) = mpsc::channel(64);
    let stopping = async |program: &str, start: String, arguments: &str| {
        let agent = home.join("bin").join(program);
        std::fs::create_dir_all(agent.parent().unwrap()).unwrap();
        kodosi_os::process::write_program(
            &agent,
            &format!("#!/bin/sh\n{start}\necho agent-ready\nsleep 30\n"),
        )
        .unwrap();
        let terminal = TerminalSession::spawn(
            Uuid::now_v7(),
            Uuid::now_v7(),
            PathBuf::from(SHELL),
            vec![
                "-c".to_owned(),
                format!("exec '{}' {arguments}", agent.display()),
            ],
            home.clone(),
            None,
            None,
            Vec::new(),
            Default::default(),
            stand_in_canvas(),
            crate::chat::TerminalChat::alone(),
            watch::channel(crate::terminal::TerminalLook::Dark).1,
            home.join("pictures"),
            changes.clone(),
        )
        .await
        .unwrap();
        timeout(Duration::from_secs(10), async {
            while !terminal.read(false).await.unwrap().contains("agent-ready") {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        let line = restore::agent_line(
            terminal.clone(),
            program.to_owned(),
            home.clone(),
            crate::agents::Notes {
                shell: "/bin/zsh".into(),
                ..crate::agents::Notes::default()
            },
            Some(home.clone()),
            crate::agents::AgentSettings::default(),
        )
        .await;
        terminal.close().await.unwrap();
        drop(terminal);
        let line = line.unwrap();
        assert!(line.named);
        line.line
    };
    let copilot = stopping(
        "copilot",
        format!(": > '{}/inuse.'$$'.lock'", session.display()),
        "--yolo --model claude-opus-5.5 --reasoning-effort max",
    )
    .await;
    assert_eq!(
        copilot,
        format!("copilot --model claude-opus-5.5 --reasoning-effort max --yolo --resume={id}")
    );
    let codex = stopping(
        "codex",
        format!("exec 3<'{}'", rollout.display()),
        "-m gpt-5.6-sol -c model_reasoning_effort=high -a on-request -s workspace-write -c hide_agent_reasoning=true",
    )
    .await;
    assert_eq!(
        codex,
        format!(
            "codex -m gpt-5.6-sol -c model_reasoning_effort=high -a on-request -s workspace-write -c hide_agent_reasoning=true --no-daemon -c \"developer_instructions=$KODOSI_AGENT_CONTEXT\" resume {id}"
        )
    );
}

#[cfg(feature = "host")]
#[tokio::test]
async fn saved_terminals_wait_for_an_app_and_stay_saved_until_they_start() {
    let root = tempfile::tempdir().unwrap();
    let saved = |name: &str| restore::Saved {
        id: Uuid::now_v7(),
        name: name.to_owned(),
        folder: None,
        sharing: None,
        owner: None,
        line: None,
        worktree: None,
        canvas: Vec::new(),
        browser: None,
        quiet_settings: None,
    };
    let (first, second) = (saved("first"), saved("second"));
    for live in [first.clone(), second.clone()] {
        let mut host = restore::Restore::load(root.path());
        host.save(vec![live]);
        host.saved().await;
    }
    let mut opened_by_an_app = restore::Restore::load(root.path());
    assert_eq!(opened_by_an_app.take(), vec![first, second]);
    assert_eq!(opened_by_an_app.take(), Vec::new());
}

#[cfg(feature = "host")]
#[tokio::test]
async fn a_saved_screen_keeps_its_newest_lines_until_its_terminal_comes_back() {
    assert_eq!(restore::newest_lines(b"one\r\ntwo\r\nthree", 9), b"three");
    assert_eq!(restore::newest_lines(b"one\r\ntwo", 8), b"one\r\ntwo");
    assert_eq!(restore::newest_lines(b"one long line", 4), b"");

    let root = tempfile::tempdir().unwrap();
    let saved = restore::Saved {
        id: Uuid::now_v7(),
        name: "Notes".to_owned(),
        folder: None,
        sharing: None,
        owner: None,
        line: None,
        worktree: None,
        canvas: Vec::new(),
        browser: None,
        quiet_settings: None,
    };
    let mut restore = restore::Restore::load(root.path());
    restore.save(vec![saved.clone()]);
    restore.saved().await;
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
        project_id: None,
        resume: None,
        branch: None,
        worktrees: None,
        command: line,
        agent: None,
        prompt: None,
        like: None,
        size: None,
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
        format!("{command} --resume {CONVERSATION}")
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
async fn a_restored_terminal_rejoins_its_project_for_its_account_and_leaves_a_refused_project() {
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
        Default::default(),
        stand_in_canvas(),
        crate::chat::TerminalChat::alone(),
        runtime.look.watch(),
        std::env::temp_dir().join(Uuid::now_v7().to_string()),
        runtime.changes.clone(),
    )
    .await
    .unwrap();
    let replaced = Uuid::now_v7();
    let entry = serde_json::from_value(json!({
        "id": id, "incarnationId": terminal.incarnation_id, "kind": "local", "name": "Build",
        "isOwner": true, "status": "running", "connectionState": "local", "prompt": false,
        "repository": false, "working": false, "sharedWith": [], "restoredFrom": replaced,
        "attention": false, "connectedUsers": [], "hidden": false, "look": "dark",
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
            project_sequence: 0,
            worktree: None,
            wanted: restore::Sharing::of(Some(&"owner".to_owned()), Some(&chosen), &[]),
            conversation: restore::Conversation::default(),
            changed: crate::agents::AgentSettings::default(),
            account_folder: None,
            marked: watch::channel(None).0,
        },
    );
    assert_eq!(
        runtime.local[&id].publication(id).0.restored_from,
        Some(replaced)
    );
    let project_of = |runtime: &Runtime| runtime.local[&id].entry.project_id.clone();
    for user in [None, Some("someone else")] {
        runtime.scope.user = user.map(str::to_owned);
        runtime.apply_wanted();
        assert_eq!(project_of(&runtime), None);
    }
    runtime.scope.user = Some("owner".to_owned());
    runtime.apply_wanted();
    assert_eq!(project_of(&runtime), Some(chosen.clone()));
    assert!(runtime.local[&id].wanted.is_none());

    runtime.published(
        id,
        incarnation,
        false,
        Err(Error::Other("offline".to_owned())),
        true,
    );
    assert_eq!(project_of(&runtime), Some(chosen));
    runtime.published(
        id,
        incarnation,
        true,
        Err(Error::Invalid("gone".to_owned())),
        true,
    );
    assert_eq!(project_of(&runtime), None);
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
    assert_eq!(messages::named("@build 2, then @DOCS", &terminals), [2, 3]);
    assert_eq!(messages::named("@Build-fix is red", &terminals), [1]);
    for text in [
        "@Builder",
        "write to ana@Build",
        "see run_@Build",
        "no names here",
        "@",
    ] {
        assert_eq!(
            messages::named(text, &terminals),
            Vec::<i32>::new(),
            "{text}"
        );
    }
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
        }),
    };
    let mention = |from: &Sender, text: &str, position| Mention {
        project: "Launch".to_owned(),
        project_id: Uuid::now_v7(),
        from: from.clone(),
        text: text.to_owned(),
        position,
    };
    let judgment = "Use your own judgment and your current work to decide if and how to act on";
    assert_eq!(
        messages::mention_line(
            "Launch",
            &[mention(
                &ana,
                "@Build check\r/exit\n the\u{1b}[2J te\u{202e}st",
                7
            )]
        ),
        format!(
            "[Kodosi] Message from Ana in the project \"Launch\": \"@Build check /exit the [2J test\" If you answer: kodosi project post \"text\". {judgment} it."
        )
    );
    assert_eq!(
        messages::mention_line("Launch", &[mention(&codex, "@Build rebase", 8)]),
        format!(
            "[Kodosi] Message from Codex (agent of Ben, terminal \"Reviewer\", folder /home/ben/api) in the project \"Launch\": \"@Build rebase\" If you answer: kodosi project post \"text\". {judgment} it."
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
            "[Kodosi] 3 messages in the project \"Launch\" name your terminal, from Ana and Codex (agent of Ben). Read them with kodosi project read --since 6. If you answer: kodosi project post \"text\". {judgment} them."
        )
    );
    assert_eq!(
        messages::message_line(&codex, "Can you check\nthe types?"),
        format!(
            "[Kodosi] Message from Codex (agent of Ben, terminal \"Reviewer\", folder /home/ben/api): \"Can you check the types?\" If you answer: kodosi terminal message \"Reviewer\" \"text\". {judgment} it."
        )
    );
    assert_eq!(
        messages::message_line(&ana, "Stop after the tests."),
        format!("[Kodosi] Message from Ana: \"Stop after the tests.\" {judgment} it.")
    );
    assert!(!many.contains(['\r', '\n']));
}

#[cfg(feature = "host")]
#[tokio::test]
async fn a_deleted_message_takes_back_the_mentions_that_wait_for_its_agents() {
    let (mut runtime, _root) = registry_fixture();
    let project = Uuid::now_v7();
    let mention = |position| messages::Mention {
        project: "Launch".to_owned(),
        project_id: project,
        from: messages::Sender::Person("Ana".to_owned()),
        text: "@Build hello".to_owned(),
        position,
    };
    let terminal = Uuid::now_v7();
    runtime.mentions.keep(terminal, mention(2));
    runtime.mentions.keep(terminal, mention(5));
    runtime.mentions.wait_for_check(messages::Unchecked {
        project,
        author: "ana".to_owned(),
        terminal: Uuid::now_v7(),
        mention: mention(3),
        since: Instant::now(),
    });
    let deleted = |position: u64| {
        json!({"id":Uuid::now_v7(),"position":position,"authorId":"ana","authorName":"","agent":null,
            "terminalId":null,"text":"","deleted":true,"createdAt":"2026-11-16T00:00:00Z"})
    };
    runtime.note_mentions(
        &json!({"projectId":project,"messages":[deleted(3)],"replacements":[deleted(2)]}),
    );
    assert_eq!(runtime.mentions.waiting[&terminal], [mention(5)]);
    assert_eq!(runtime.mentions.unchecked, Vec::new());
}

#[cfg(all(unix, feature = "host"))]
fn stand_in_canvas() -> crate::canvas::CanvasHandle {
    let folder = std::env::temp_dir();
    let services = crate::canvas::CanvasServices::new(&folder, true);
    crate::canvas::CanvasHandle::spawn(Uuid::now_v7(), &folder, services)
}

#[cfg(all(unix, feature = "host"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[expect(
    clippy::too_many_lines,
    reason = "one project message from its mention to the line that the agent gets"
)]
async fn a_working_agent_gets_the_project_message_that_names_its_terminal_unless_it_asks() {
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
        Default::default(),
        stand_in_canvas(),
        crate::chat::TerminalChat::alone(),
        runtime.look.watch(),
        std::env::temp_dir().join(Uuid::now_v7().to_string()),
        runtime.changes.clone(),
    )
    .await
    .unwrap();
    let project = Uuid::now_v7().to_string();
    let mut entry: SessionEntry = serde_json::from_value(json!({
        "id": id, "incarnationId": terminal.incarnation_id, "kind": "local", "name": "Build",
        "isOwner": true, "status": "running", "connectionState": "local", "prompt": false,
        "repository": false, "working": false, "sharedWith": ["ana"], "projectId": project,
        "projectName": "Launch", "attention": false, "connectedUsers": [], "hidden": false,
        "look": "dark",
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
            project_sequence: 0,
            worktree: None,
            wanted: None,
            conversation: restore::Conversation::default(),
            changed: crate::agents::AgentSettings::default(),
            account_folder: None,
            marked: watch::channel(None).0,
        },
    );
    let snapshot = |messages: Value| json!({"projectId": project, "messages": messages});
    runtime.note_mentions(&snapshot(
        json!([{"position": 1, "text": "hello", "authorId": "ana", "authorName": "Ana"}]),
    ));
    runtime.note_mentions(&snapshot(json!([
        {"position": 2, "text": "@Build please print came-through", "authorId": "ana", "authorName": "Ana"},
        {"position": 3, "text": "@Build from itself", "authorId": "ana", "authorName": "Ana", "terminalId": id},
        {"position": 4, "text": "@Build from someone who is not a friend", "authorId": "bo", "authorName": "Bo"},
    ])));
    assert!(
        runtime.mentions.waiting.is_empty(),
        "a mention waits for the signed records of its author"
    );
    {
        let checked = timeout(Duration::from_secs(10), runtime.jobs.join_next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(
            matches!(&checked.completion, Completion::Mentioned { mentions, later } if mentions.is_empty() && later.is_empty()),
            "without the signed records, the agent gets nothing"
        );
        runtime.complete(checked);
    }
    assert!(runtime.mentions.waiting.is_empty());
    runtime.complete(Job {
        scope: runtime.scope.clone(),
        completion: Completion::Mentioned {
            mentions: vec![(
                id,
                messages::Mention {
                    project: "Launch".to_owned(),
                    project_id: Uuid::parse_str(&project).unwrap(),
                    from: messages::Sender::Person("Ana".to_owned()),
                    text: "@Build please print came-through".to_owned(),
                    position: 2,
                },
            )],
            later: Vec::new(),
        },
    });
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
            .contains("[Kodosi] Message from Ana in the project")
    );
    terminal.close().await.unwrap();
    drop(terminal);
}

#[cfg(feature = "host")]
#[tokio::test]
async fn a_mention_whose_check_gets_no_answer_waits_a_while_for_a_later_check() {
    let (mut runtime, _root) = registry_fixture();
    let unanswered = tempfile::tempdir().unwrap();
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = closed.local_addr().unwrap();
    drop(closed);
    runtime.network = BackendClient::new(BackendConfig {
        api_url: format!("http://{address}/").parse().unwrap(),
        issuer: format!("http://{address}"),
        client_id: "test".into(),
        scopes: vec![],
        data_root: unanswered.path().to_owned(),
        secret_service: "test".into(),
        isolated: true,
    })
    .unwrap();
    runtime.scope.network_generation = runtime.network.generation();
    runtime.network.assume_identity(crate::network::Identity {
        user_id: "owner".to_owned(),
        device_id: "this-device".to_owned(),
        display_name: "You".to_owned(),
        enrolled: true,
    });
    let unchecked = messages::Unchecked {
        project: Uuid::now_v7(),
        author: "ana".to_owned(),
        terminal: Uuid::now_v7(),
        mention: messages::Mention {
            project: "Launch".to_owned(),
            project_id: Uuid::now_v7(),
            from: messages::Sender::Person("Ana".to_owned()),
            text: "@Build hello".to_owned(),
            position: 2,
        },
        since: Instant::now(),
    };
    runtime.mentions.wait_for_check(unchecked.clone());
    runtime.check_mentions();
    assert_eq!(runtime.mentions.unchecked, Vec::new());
    let checked = timeout(Duration::from_secs(30), runtime.jobs.join_next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        matches!(&checked.completion, Completion::Mentioned { mentions, later }
            if mentions.is_empty() && later.as_slice() == [unchecked.clone()]),
        "a mention whose check gets no answer comes back for a later check"
    );
    runtime.mentions.wait_for_check(unchecked.clone());
    assert!(
        runtime
            .mentions
            .take_unchecked(unchecked.since + messages::CHECK_PATIENCE)
            .is_empty(),
        "a mention waits only a while"
    );
}

async fn remote_line_terminal() -> (
    RemoteTerminal,
    Subscription,
    mpsc::Receiver<crate::network::TestRemoteRequest>,
    mpsc::Sender<crate::network::RemoteUpdate>,
    mpsc::Receiver<SessionChange>,
) {
    let remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    let (connection, mut requests, updates) = crate::network::test_remote_connection(remote);
    let (changes, receiver) = mpsc::channel(16);
    let terminal = RemoteTerminal::spawn(connection, changes);
    let view = terminal.subscribe();
    assert!(matches!(
        timeout(Duration::from_secs(2), requests.recv())
            .await
            .unwrap(),
        Some(crate::network::TestRemoteRequest::Checkpoint)
    ));
    updates.send(first_snapshot()).await.unwrap();
    (terminal, view.await.unwrap(), requests, updates, receiver)
}

async fn admit_input(
    requests: &mut mpsc::Receiver<crate::network::TestRemoteRequest>,
) -> (Vec<u8>, oneshot::Sender<()>) {
    let Some(crate::network::TestRemoteRequest::Control {
        control: TerminalControl::Input { bytes },
        reply,
    }) = timeout(Duration::from_secs(2), requests.recv())
        .await
        .unwrap()
    else {
        panic!("input expected");
    };
    reply.send(Ok(Value::Null)).unwrap();
    let Some(crate::network::TestRemoteRequest::Delivered { reply }) =
        timeout(Duration::from_secs(2), requests.recv())
            .await
            .unwrap()
    else {
        panic!("a question whether the terminal has the input expected");
    };
    (bytes, reply)
}

#[tokio::test]
async fn a_line_waits_for_enter_while_a_person_types_in_the_pause() {
    let (terminal, view, mut requests, _updates, _changes) = remote_line_terminal().await;
    let line =
        tokio::spawn(TerminalTarget::Remote(terminal.clone()).type_line("hello".to_owned(), false));
    let (_, delivered) = admit_input(&mut requests).await;
    delivered.send(()).unwrap();
    terminal
        .input(view.connection_id, Bytes::from_static(b"x"))
        .unwrap();
    let Some(crate::network::TestRemoteRequest::Control {
        control: TerminalControl::Input { bytes },
        reply,
    }) = timeout(Duration::from_secs(2), requests.recv())
        .await
        .unwrap()
    else {
        panic!("the person's input expected");
    };
    assert_eq!(bytes, b"x");
    reply.send(Ok(Value::Null)).unwrap();
    line.await.unwrap().unwrap();
    assert!(requests.try_recv().is_err());
    terminal.disconnect();
}

fn show(runtime: &mut Runtime, consumer: Uuid, lifetime: &CancellationToken, ids: &[Uuid]) {
    runtime.see(
        consumer,
        lifetime.clone(),
        ids.iter().map(ToString::to_string).collect(),
        true,
    );
}

async fn open_shown(
    runtime: &mut Runtime,
    remote: &RemoteSession,
    consumer: Uuid,
    lifetime: &CancellationToken,
) -> (Subscription, mpsc::Sender<crate::network::RemoteUpdate>) {
    let id = remote.id;
    show(runtime, consumer, lifetime, &[id]);
    runtime
        .apply_for_consumer(&open_remote(id), consumer, lifetime.clone())
        .unwrap();
    let (connection, mut requests, updates) =
        crate::network::test_remote_connection(remote.clone());
    connected(runtime, id, Ok(connection));
    let view = runtime.connections[&id].subscribe();
    requests.recv().await.unwrap();
    updates.send(first_snapshot()).await.unwrap();
    (view.await.unwrap(), updates)
}

fn open_remote(id: Uuid) -> Command {
    Command::OpenRemote {
        request_id: Uuid::now_v7().to_string(),
        session_id: id.to_string(),
    }
}

#[tokio::test]
async fn a_remote_terminal_pauses_while_no_client_shows_it_and_resumes_without_reconnecting() {
    let (mut runtime, _root) = registry_fixture();
    let remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    let id = remote.id;
    runtime.replace_remotes(vec![remote.clone()]);
    let (consumer, lifetime) = (Uuid::now_v7(), CancellationToken::new());
    let (mut view, _updates) = open_shown(&mut runtime, &remote, consumer, &lifetime).await;

    show(&mut runtime, consumer, &lifetime, &[]);
    for _ in 0..400 {
        if runtime.connections[&id].is_interrupted() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let instance = runtime.connections[&id].instance_id;
    runtime.remote_paused(id, remote.incarnation_id, instance);
    assert!(runtime.connections[&id].is_paused());
    let running = (ConnectionState::Connected, SessionStatus::Running, None);
    assert_eq!(remote_state(&runtime, id), running);
    runtime.terminal_changed(&json!({
        "sessionId": id, "incarnationId": remote.incarnation_id, "online": true,
        "status": {"program": "claude", "state": "done"},
    }));
    assert_eq!(
        runtime.remotes[&id]
            .program_status
            .as_ref()
            .map(|status| status.state),
        Some(crate::terminal::ProgramState::Done)
    );
    runtime.reconnect.get_mut(&id).unwrap().next = Instant::now();
    runtime.reconnect_views();
    assert!(!runtime.opening.contains_key(&id));

    show(&mut runtime, consumer, &lifetime, &[id]);
    assert!(runtime.opening.contains_key(&id));
    assert_eq!(remote_state(&runtime, id), running);
    let (again, _requests, _updates) = crate::network::test_remote_connection(remote.clone());
    connected(&mut runtime, id, Ok(again));
    for _ in 0..400 {
        if !runtime.connections[&id].is_interrupted() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(!runtime.connections[&id].is_interrupted());
    assert_eq!(remote_state(&runtime, id), running);
    assert!(view.control.try_recv().is_err());
}

#[tokio::test]
async fn a_blocked_terminal_is_not_connected_again_when_it_shows() {
    let plain = crate::text::Text::plain;
    for refusal in [
        Error::Invalid("You cannot open this terminal.".to_owned()),
        Error::from(crate::network::Error::Trust(plain("Not trusted."))),
        Error::from(crate::network::Error::Backend {
            status: 403,
            message: plain("Refused."),
        }),
        Error::from(crate::network::Error::SignedOut),
    ] {
        let (mut runtime, _root) = registry_fixture();
        let remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
        let id = remote.id;
        runtime.replace_remotes(vec![remote]);
        let (consumer, lifetime) = (Uuid::now_v7(), CancellationToken::new());
        show(&mut runtime, consumer, &lifetime, &[id]);
        runtime
            .apply_for_consumer(&open_remote(id), consumer, lifetime.clone())
            .unwrap();
        connected(&mut runtime, id, Err(refusal));
        assert_eq!(remote_state(&runtime, id).0, ConnectionState::Blocked);
        show(&mut runtime, consumer, &lifetime, &[]);
        show(&mut runtime, consumer, &lifetime, &[id]);
        assert!(!runtime.opening.contains_key(&id));
    }
}

#[tokio::test]
async fn a_lost_terminal_says_reconnecting_and_retries_only_while_it_shows() {
    let (mut runtime, _root) = registry_fixture();
    let remote = remote_session(Uuid::now_v7(), Uuid::now_v7());
    let id = remote.id;
    runtime.replace_remotes(vec![remote.clone()]);
    let (consumer, lifetime) = (Uuid::now_v7(), CancellationToken::new());
    let (_view, updates) = open_shown(&mut runtime, &remote, consumer, &lifetime).await;
    lose_link(
        &mut runtime,
        &remote,
        &updates,
        "The connection has closed.",
    )
    .await;
    assert_eq!(remote_state(&runtime, id).1, SessionStatus::Reconnecting);
    show(&mut runtime, consumer, &lifetime, &[]);
    assert_eq!(remote_state(&runtime, id).1, SessionStatus::Running);
    runtime.reconnect.get_mut(&id).unwrap().next = Instant::now();
    runtime.reconnect_views();
    assert!(!runtime.opening.contains_key(&id));
    show(&mut runtime, consumer, &lifetime, &[id]);
    assert_eq!(remote_state(&runtime, id).1, SessionStatus::Reconnecting);
    runtime.reconnect_views();
    assert!(runtime.opening.contains_key(&id));
}

#[cfg(all(unix, feature = "host"))]
#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "one change of an agent from its request to the commands that the agent gets"
)]
async fn a_change_of_the_agent_waits_for_its_next_prompt_and_types_its_own_commands() {
    use crate::terminal::{ProgramState, ProgramStatus};
    async fn types_until(
        runtime: &mut Runtime,
        within: Duration,
        done: impl Fn(&Runtime) -> bool + Send + Sync,
    ) -> bool {
        timeout(within, async {
            while !done(runtime) {
                runtime.type_agent_changes();
                match runtime.jobs.join_next().await {
                    Some(job) => runtime.complete(job.unwrap()),
                    None => tokio::time::sleep(Duration::from_millis(50)).await,
                }
            }
        })
        .await
        .is_ok()
    }
    let (mut runtime, storage) = registry_fixture();
    let id = Uuid::now_v7();
    let typed = storage.path().join("typed");
    let terminal = TerminalSession::spawn(
        id,
        Uuid::now_v7(),
        PathBuf::from(SHELL),
        vec![
            "-c".to_owned(),
            "printf '\\033]7501;state=working:app=claude-code\\007'; while IFS= read -r line; do printf '%s\\n' \"$line\" >> \"$0\"; case \"$line\" in '/model '*) printf 'Switch model?\\n' ;; 1) printf '\\033[2J\\033[H' ;; esac; done".to_owned(),
            typed.to_str().unwrap().to_owned(),
        ],
        storage.path().to_owned(),
        None,
        None,
        Vec::new(),
        Default::default(),
        stand_in_canvas(),
        crate::chat::TerminalChat::alone(),
        runtime.look.watch(),
        std::env::temp_dir().join(Uuid::now_v7().to_string()),
        runtime.changes.clone(),
    )
    .await
    .unwrap();
    let mut entry: SessionEntry = serde_json::from_value(json!({
        "id": id, "incarnationId": terminal.incarnation_id, "kind": "local", "name": "Build",
        "isOwner": true, "status": "running", "connectionState": "local", "prompt": false,
        "repository": false, "sharedWith": [], "attention": false, "connectedUsers": [],
        "hidden": false, "working": false, "look": "dark",
    }))
    .unwrap();
    entry.program = Some("claude".to_owned());
    entry.program_settings = Some(crate::agents::AgentSettings {
        model: Some("opus".to_owned()),
        ..crate::agents::AgentSettings::default()
    });
    runtime.local.insert(
        id,
        Local {
            terminal: terminal.clone(),
            entry,
            creation: Command::ListSessions {
                request_id: Uuid::now_v7().to_string(),
            },
            published: true,
            project_sequence: 0,
            worktree: None,
            wanted: None,
            conversation: restore::Conversation::default(),
            changed: crate::agents::AgentSettings::default(),
            account_folder: None,
            marked: watch::channel(None).0,
        },
    );
    let conversation = &mut runtime.local.get_mut(&id).unwrap().conversation;
    assert!(conversation.ask(Instant::now()));
    conversation.found(Some(restore::AgentLine {
        line: "claude --model opus --resume 0198f0c4-5e6f-7a8b-9c0d-1e2f3a4b5c6d".to_owned(),
        named: true,
    }));
    let set = |runtime: &mut Runtime, state| {
        runtime.local.get_mut(&id).unwrap().entry.program_status = Some(ProgramStatus {
            state,
            kind: None,
            progress: None,
            title: None,
            message: None,
        });
    };
    let change = |model: Option<&str>, effort: Option<&str>| crate::agents::AgentSettings {
        model: model.map(str::to_owned),
        effort: effort.map(str::to_owned),
        ..crate::agents::AgentSettings::default()
    };
    let fast = |change: crate::agents::AgentSettings| crate::agents::AgentSettings {
        fast: Some(true),
        ..change
    };
    assert!(
        runtime
            .change_agent_here(
                id,
                &crate::agents::AgentSettings {
                    permissions: Some(crate::agents::Permissions::Ask),
                    ..crate::agents::AgentSettings::default()
                }
            )
            .is_err(),
        "Claude Code changes its permissions only when it starts"
    );
    set(&mut runtime, ProgramState::Working);
    runtime
        .change_agent_here(id, &change(Some("haiku"), None))
        .unwrap();
    runtime
        .change_agent_here(id, &change(None, Some("high")))
        .unwrap();
    runtime
        .change_agent_here(id, &change(Some("sonnet"), None))
        .unwrap();
    runtime
        .change_agent_here(id, &fast(change(None, None)))
        .unwrap();
    assert!(
        runtime.jobs.is_empty(),
        "an agent at work gets its commands after its turn"
    );
    assert_eq!(
        runtime.local[&id].entry.program_pending,
        Some(fast(change(Some("sonnet"), Some("high")))),
        "the apps show what waits"
    );
    set(&mut runtime, ProgramState::Done);
    assert!(
        types_until(&mut runtime, Duration::from_secs(20), |runtime| {
            runtime.local[&id].changed.model.is_some()
        })
        .await
    );
    assert!(
        runtime
            .local
            .get_mut(&id)
            .unwrap()
            .conversation
            .ask(Instant::now()),
        "the line that brings the agent back is asked for again with its new model"
    );
    assert!(
        !types_until(&mut runtime, Duration::from_secs(4), |runtime| {
            let changed = &runtime.local[&id].changed;
            changed.effort.is_some() || changed.fast.is_some()
        })
        .await,
        "Kodosi types nothing into the question that Claude Code asks on its screen"
    );
    assert_eq!(
        runtime.local[&id].entry.program_pending,
        Some(fast(change(None, Some("high"))))
    );
    terminal
        .send_input(Bytes::from_static(b"1\r"))
        .await
        .unwrap();
    assert!(
        types_until(&mut runtime, Duration::from_secs(20), |runtime| {
            runtime.local[&id].changed.fast.is_some()
        })
        .await
    );
    let lines = timeout(Duration::from_secs(5), async {
        loop {
            let lines = std::fs::read_to_string(&typed).unwrap_or_default();
            if lines.ends_with("/fast on\n") {
                return lines;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        lines, "/model sonnet\n1\n/effort high\n/fast on\n",
        "a later change of the model replaces one that waits, and the effort and the fast mode follow the answer"
    );
    assert_eq!(
        runtime.local[&id].entry.program_settings,
        Some(fast(change(Some("sonnet"), Some("high"))))
    );
    assert_eq!(runtime.local[&id].entry.program_pending, None);
    terminal.close().await.unwrap();
    drop(terminal);
}

#[cfg(all(unix, feature = "host"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[expect(
    clippy::too_many_lines,
    reason = "each way to start one more terminal like another, with the lines its shells get"
)]
async fn one_more_terminal_like_another_starts_its_agent_with_its_account_and_choices() {
    use crate::agents::{AgentChoiceChange, AgentPick, AgentSettings, Launcher, Permissions};

    async fn answer(
        runtime: &mut Runtime,
        events: &mut broadcast::Receiver<Event>,
        command: &Command,
    ) -> Value {
        runtime
            .expect_answer(command, Uuid::now_v7(), CancellationToken::new())
            .unwrap();
        runtime.apply(command).unwrap();
        timeout(Duration::from_secs(20), async {
            loop {
                while let Ok(event) = events.try_recv() {
                    let mut event = serde_json::to_value(event).unwrap();
                    if event["requestId"] == command.request_id() {
                        crate::text::render(&mut event, crate::text::Language::English);
                        return event;
                    }
                }
                match runtime.jobs.join_next().await {
                    Some(job) => runtime.complete(job.unwrap()),
                    None => tokio::time::sleep(Duration::from_millis(20)).await,
                }
            }
        })
        .await
        .unwrap()
    }
    async fn typed(folder: &Path) -> String {
        timeout(Duration::from_secs(20), async {
            loop {
                let lines = std::fs::read_to_string(folder.join("typed")).unwrap_or_default();
                if lines.ends_with('\n') {
                    return lines;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap()
    }
    let create = |request: &str, folder: &Path, like: Option<Uuid>| Command::CreateSession {
        request_id: request.to_owned(),
        name: "Build".to_owned(),
        working_dir: Some(folder.to_string_lossy().into_owned()),
        project_id: None,
        resume: None,
        branch: None,
        worktrees: None,
        command: None,
        agent: None,
        prompt: None,
        like: like.map(|id| id.to_string()),
        size: None,
    };
    let (mut runtime, storage) = registry_fixture();
    let root = kodosi_os::fs::canonicalize(storage.path()).unwrap();
    let home = runtime.config.home.clone();
    std::fs::create_dir_all(home.join(".claude-work")).unwrap();
    let executable = |path: PathBuf, text: String| {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        kodosi_os::process::write_program(&path, &text).unwrap();
        path
    };
    let bin = root.join("bin");
    executable(
        bin.join("claude"),
        "#!/bin/sh\nprintf '  --effort <level>  Effort level\\n      (low, high, max)\\n'\n"
            .to_owned(),
    );
    let shell = executable(
        root.join("shells/sh"),
        format!(
            "#!/bin/sh\nif [ \"$1\" = -l ]; then shift; PATH='{}' exec /bin/sh \"$@\"; fi\nstty -icanon\nwhile IFS= read -r line; do printf '%s\\n' \"$line\" >> typed; done\n",
            bin.display()
        ),
    );
    runtime.config.initial_shell = Some(shell.to_string_lossy().into_owned());
    runtime.launcher = Arc::new(Launcher::load(&runtime.config.data_root, &home, &shell));
    runtime.launcher.choose(&AgentChoiceChange::Pick(AgentPick {
        agent: crate::agents::Agent::Claude,
        account: Some("default".to_owned()),
        model: Some("haiku".to_owned()),
        effort: Some("high".to_owned()),
        permissions: Some(Permissions::Ask),
        output: Some(crate::agents::Output::Standard),
        fast: false,
        chat: false,
    }));
    let folder = |name: &str| {
        let folder = root.join(name);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    };
    let mut events = runtime.events.subscribe();
    let mut started = async |runtime: &mut Runtime, folder: &Path, like: Option<Uuid>| {
        let request = Uuid::now_v7().to_string();
        let answer = answer(runtime, &mut events, &create(&request, folder, like)).await;
        assert_eq!(answer["type"], "session.result", "{answer}");
        Uuid::parse_str(answer["sessionId"].as_str().unwrap()).unwrap()
    };
    let agent = started(&mut runtime, &folder("agent"), None).await;
    let plain = started(&mut runtime, &folder("plain"), None).await;
    let settings = |model: &str, effort: Option<&str>, permissions| AgentSettings {
        model: Some(model.to_owned()),
        effort: effort.map(str::to_owned),
        permissions: Some(permissions),
        ..AgentSettings::default()
    };
    let runs = |runtime: &mut Runtime, account: &str, pending: AgentSettings| {
        runtime.local.get_mut(&agent).unwrap().account_folder = Some(home.join(account));
        let entry = &mut runtime.local.get_mut(&agent).unwrap().entry;
        entry.program = Some("claude".to_owned());
        entry.program_settings = Some(settings("opus", Some("low"), Permissions::Unrestricted));
        entry.program_pending = Some(pending);
    };

    runs(
        &mut runtime,
        ".claude-work",
        AgentSettings {
            effort: Some("max".to_owned()),
            ..AgentSettings::default()
        },
    );
    let work = folder("work");
    started(&mut runtime, &work, Some(agent)).await;
    assert_eq!(
        typed(&work).await,
        format!(
            "env CLAUDE_CONFIG_DIR={}/.claude-work claude --model opus --effort max --dangerously-skip-permissions --fast-mode off\n",
            home.display()
        ),
        "the account, model and permissions of the agent and the effort that waits; the output of the last choice"
    );

    runs(
        &mut runtime,
        ".claude",
        AgentSettings {
            model: Some("sonnet".to_owned()),
            ..AgentSettings::default()
        },
    );
    let default = folder("default");
    started(&mut runtime, &default, Some(agent)).await;
    assert_eq!(
        typed(&default).await,
        "claude --model sonnet --dangerously-skip-permissions --fast-mode off\n",
        "a new model that waits runs at its own effort"
    );

    let entry = &mut runtime.local.get_mut(&agent).unwrap().entry;
    entry.program_settings = Some(settings("opus[1m]", None, Permissions::Unrestricted));
    entry.program_pending = None;
    let by_hand = folder("by-hand");
    started(&mut runtime, &by_hand, Some(agent)).await;
    assert_eq!(
        typed(&by_hand).await,
        "claude --model sonnet --dangerously-skip-permissions --fast-mode off\n",
        "a model that Kodosi cannot start stays the last choice"
    );

    let shell_only = folder("shell");
    let new = started(&mut runtime, &shell_only, Some(plain)).await;
    runtime.local[&new]
        .terminal
        .send_input(Bytes::from_static(b"probe\r"))
        .await
        .unwrap();
    assert_eq!(
        typed(&shell_only).await,
        "probe\n",
        "a terminal like a plain one types nothing before the person"
    );

    assert!(matches!(
        runtime.apply(&create(
            &Uuid::now_v7().to_string(),
            &root,
            Some(Uuid::now_v7())
        )),
        Err(Error::NotFound)
    ));
    for (field, value) in [
        ("command", json!("make")),
        ("agent", json!({"agent": "claude"})),
        (
            "resume",
            json!({"provider": "claude", "nativeConversationId": Uuid::now_v7()}),
        ),
        ("prompt", json!("Fix the build")),
    ] {
        let mut refused = json!({"type": "session.create", "requestId": Uuid::now_v7(), "name": "Build", "like": agent});
        refused[field] = value;
        let refused = serde_json::from_value(refused).unwrap();
        assert_eq!(
            answer(&mut runtime, &mut events, &refused).await["message"],
            "A terminal like another starts without a command, an agent, a conversation or a prompt.",
            "{field}"
        );
    }
    assert_eq!(runtime.local.len(), 6, "a refused terminal does not start");
    for local in runtime.local.values() {
        drop(local.terminal.close().await);
    }
}
