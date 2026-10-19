use super::*;
#[tokio::test]
async fn command_reader_rejects_large_prefix_before_allocating() {
    let (mut writer, stream) = tokio::io::duplex(32);
    let mut reader = FrameReader::new(stream);
    reader.maximum = crate::protocol::MAX_COMMAND_BYTES;
    writer
        .write_u32(u32::try_from(reader.maximum + 1).unwrap())
        .await
        .unwrap();
    assert!(read_frame(&mut reader).await.is_err());
    assert_eq!(reader.body, [] as [u8; 0]);
}

#[tokio::test]
async fn frames_roundtrip_and_oversized_prefix_is_rejected() {
    let (mut a, b) = tokio::io::duplex(512);
    let mut b = FrameReader::new(b);
    write_frame(&mut a, b"hello").await.unwrap();
    assert_eq!(read_frame(&mut b).await.unwrap(), b"hello");
    a.write_u32(u32::try_from(MAX_FRAME + 1).unwrap())
        .await
        .unwrap();
    assert!(read_frame(&mut b).await.is_err());
}
#[tokio::test]
async fn interrupted_frame_read_keeps_consumed_prefix_and_body() {
    let (mut writer, input) = tokio::io::duplex(64);
    let mut reader = FrameReader::new(input);
    writer.write_all(&[0, 0]).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(5), read_frame(&mut reader))
            .await
            .is_err()
    );
    writer.write_all(&[0, 5, b'h', b'e']).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(5), read_frame(&mut reader))
            .await
            .is_err()
    );
    writer.write_all(b"llo").await.unwrap();
    assert_eq!(read_frame(&mut reader).await.unwrap(), b"hello");
}
#[test]
fn host_lock_prevents_competing_local_owners() {
    let directory = tempfile::tempdir().unwrap();
    let first = lock_root(directory.path()).unwrap();
    assert!(lock_root(directory.path()).is_err());
    drop(first);
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while lock_root(directory.path()).is_err() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn root_identity_and_socket_namespace_are_stable_and_separate() {
    assert_eq!(
        root_identity(Path::new("/a")),
        root_identity(Path::new("/a"))
    );
    assert_ne!(
        root_identity(Path::new("/a")),
        root_identity(Path::new("/b"))
    );
}
#[test]
fn a_hello_of_another_version_still_reads() {
    assert!(serde_json::from_str::<Hello>(r#"{"version":20,"root":"a","sessionId":null}"#).is_ok());
    assert!(serde_json::from_str::<Hello>(r#"{"version":30,"root":"a","added":true}"#).is_ok());
    assert!(serde_json::from_str::<Hello>(r#"{"version":19}"#).is_err());
}

#[tokio::test]
async fn a_host_names_itself_to_another_version_and_stops_for_it() {
    let (_storage, root, handle) = hosted(HostKind::Background).await;
    let root = kodosi_os::fs::canonicalize(&root).unwrap();
    let mut stream = kodosi_os::ipc::connect(&endpoint(&root).unwrap())
        .await
        .unwrap();
    let hello = Hello {
        version: VERSION + 1,
        root: root_identity(&root),
        session_id: None,
    };
    write_json(&mut stream, &hello).await.unwrap();
    let welcome: Welcome = read_json(&mut FrameReader::new(&mut stream)).await.unwrap();
    assert!(welcome.error.is_some());
    assert_eq!(welcome.terminals, 0);
    assert_eq!(
        welcome.host,
        Some(HostDescription {
            pid: std::process::id(),
            kind: HostKind::Background
        })
    );
    write_json(&mut stream, &ClientRequest::Stop { force: false })
        .await
        .unwrap();
    let reply: Value = read_json(&mut FrameReader::new(&mut stream)).await.unwrap();
    assert_eq!(reply["kind"], "stopping");
    tokio::time::timeout(Duration::from_secs(10), handle.stopped())
        .await
        .unwrap();
}

#[tokio::test]
async fn a_client_names_an_older_host() {
    let storage = tempfile::tempdir().unwrap();
    let root = kodosi_os::fs::canonicalize(storage.path()).unwrap();
    let mut listener = kodosi_os::ipc::Listener::bind(&endpoint(&root).unwrap())
        .await
        .unwrap();
    let host = tokio::spawn(async move {
        let mut stream = listener.accept().await.unwrap();
        let hello: Hello = read_json(&mut FrameReader::new(&mut stream)).await.unwrap();
        assert_eq!(hello.version, VERSION);
        let welcome = Welcome {
            version: VERSION - 1,
            error: Some("local runtime protocol mismatch".into()),
            host: Some(HostDescription {
                pid: 42,
                kind: HostKind::Foreground,
            }),
            terminals: 2,
            ..Welcome::default()
        };
        write_json(&mut stream, &welcome).await.unwrap();
    });
    assert!(matches!(
        Client::connect(&root).await,
        Err(Error::HostOutdated {
            kind: HostKind::Foreground,
            pid: 42,
            terminals: 2
        })
    ));
    host.await.unwrap();
}

async fn welcome_client(
    listener: &mut kodosi_os::ipc::Listener,
    welcomed: Vec<Value>,
    then: Vec<Value>,
) -> kodosi_os::ipc::Stream {
    let mut stream = listener.accept().await.unwrap();
    let hello: Hello = read_json(&mut FrameReader::new(&mut stream)).await.unwrap();
    assert_eq!(hello.version, VERSION);
    let welcome = Welcome {
        version: VERSION,
        events: welcomed,
        host: Some(HostDescription {
            pid: 42,
            kind: HostKind::Background,
        }),
        ..Welcome::default()
    };
    write_json(&mut stream, &welcome).await.unwrap();
    for event in then {
        write_json(&mut stream, &json!({"kind":"event","event":event}))
            .await
            .unwrap();
    }
    stream
}

fn scoped(kind: &str, user: Option<&str>, epoch: u64) -> Value {
    json!({"type":kind,"accountUserId":user,"accountEpoch":epoch})
}

#[tokio::test]
async fn a_client_starts_from_the_account_that_the_host_read() {
    let storage = tempfile::tempdir().unwrap();
    let root = kodosi_os::fs::canonicalize(storage.path()).unwrap();
    let mut listener = kodosi_os::ipc::Listener::bind(&endpoint(&root).unwrap())
        .await
        .unwrap();
    let host = tokio::spawn(async move {
        let before = welcome_client(
            &mut listener,
            vec![
                scoped("system.ready", None, 1),
                scoped("auth.finalizing", None, 1),
            ],
            vec![
                scoped("auth.finalizing", Some("me"), 2),
                scoped("auth.ready", Some("me"), 2),
            ],
        )
        .await;
        let after = welcome_client(
            &mut listener,
            vec![
                scoped("system.ready", Some("me"), 2),
                scoped("auth.finalizing", Some("me"), 2),
            ],
            vec![scoped("auth.ready", Some("me"), 2)],
        )
        .await;
        (before, after)
    });
    let client = connect_or_start(&root).await.unwrap();
    let heard: Vec<_> = client
        .initial_events
        .iter()
        .map(|event| {
            (
                event["type"].as_str().unwrap(),
                event["accountEpoch"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        heard,
        [
            ("system.ready", 2),
            ("auth.finalizing", 2),
            ("auth.ready", 2)
        ]
    );
    drop(host.await.unwrap());
}

#[tokio::test]
async fn a_client_hears_why_the_host_could_not_read_the_saved_sign_in() {
    let storage = tempfile::tempdir().unwrap();
    let root = kodosi_os::fs::canonicalize(storage.path()).unwrap();
    let mut listener = kodosi_os::ipc::Listener::bind(&endpoint(&root).unwrap())
        .await
        .unwrap();
    let host = tokio::spawn(async move {
        let mut expired = scoped("auth.required", None, 1);
        expired["reason"] = json!("expired");
        welcome_client(
            &mut listener,
            vec![
                scoped("system.ready", None, 1),
                scoped("auth.finalizing", None, 1),
            ],
            vec![expired],
        )
        .await
    });
    let client = connect_or_start(&root).await.unwrap();
    assert_eq!(client.initial_events.last().unwrap()["reason"], "expired");
    drop(host.await.unwrap());
}

#[cfg(unix)]
#[tokio::test]
async fn a_terminal_connection_answers_its_controls_and_refreshes() {
    let (_storage, root, handle) = hosted(HostKind::Background).await;
    let mut client = Client::connect(&root).await.unwrap();
    let epoch = client.initial_events[0]["accountEpoch"].clone();
    let create = Uuid::now_v7().to_string();
    client
        .command(
            json!({"type":"session.create","requestId":create,"name":"Terminal",
            "accountUserId":null,"accountEpoch":epoch}),
        )
        .await
        .unwrap();
    let session: Uuid = answered(&mut client, &create).await["sessionId"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let mut terminal = TerminalClient::connect(&root, session).await.unwrap();
    assert!(matches!(
        read_terminal(&mut terminal.reader).await.unwrap(),
        TerminalFrame::Checkpoint { .. }
    ));
    let focus = Uuid::now_v7().to_string();
    write_control(
        &mut terminal.writer,
        &json!({"type":"session.focus","requestId":focus,"sessionId":session,
            "expectedRuntimeIncarnationId":terminal.incarnation_id,"clientId":Uuid::now_v7(),
            "subscriptionGeneration":1,"accountUserId":null,"accountEpoch":epoch}),
    )
    .await
    .unwrap();
    write_refresh(&mut terminal.writer).await.unwrap();
    let (mut focused, mut refreshed) = (false, false);
    while !(focused && refreshed) {
        let frame =
            tokio::time::timeout(Duration::from_secs(10), read_terminal(&mut terminal.reader))
                .await
                .unwrap()
                .unwrap();
        match frame {
            TerminalFrame::Answer(answer) => {
                assert_eq!(answer["type"], "term.focusApplied", "{answer}");
                assert_eq!(answer["requestId"], focus.as_str());
                focused = true;
            }
            TerminalFrame::Checkpoint { .. } => refreshed = true,
            _ => {}
        }
    }
    handle.shutdown().await;
}
#[test]
fn stop_permission_protects_a_foreground_host_and_open_terminals() {
    assert!(stop_permission(HostKind::Foreground, false, 0).is_err());
    assert!(stop_permission(HostKind::Foreground, true, 0).is_err());
    assert!(stop_permission(HostKind::Background, false, 1).is_err());
    assert!(stop_permission(HostKind::Background, false, 0).is_ok());
    assert!(stop_permission(HostKind::Background, true, 1).is_ok());
}
#[test]
fn local_session_counts_ignore_remote_entries() {
    let session = |kind: &str| {
        json!({"id":"11111111-1111-4111-8111-111111111111","incarnationId":"11111111-1111-4111-8111-111111111111",
            "kind":kind,"name":"Terminal","prompt":false,"repository":false,"isOwner":true,"status":"running",
            "connectionState":"local","sharedWith":[]})
    };
    let events = [
        Event::new(None, 0, json!({"type":"sessions.snapshot","sessions":[session("local"),session("remote"),session("local")]})).unwrap(),
        Event::new(None, 0, json!({"type":"auth.required","reason":"signedOut"})).unwrap(),
    ];
    assert_eq!(local_sessions(&events), 2);
    assert_eq!(local_sessions(&[]), 0);
}
#[tokio::test]
async fn an_answer_reaches_only_the_client_that_asked() {
    let (_storage, root, handle) = hosted(HostKind::Background).await;
    let mut asking = Client::connect(&root).await.unwrap();
    let mut other = Client::connect(&root).await.unwrap();
    let epoch = asking.initial_events[0]["accountEpoch"].clone();
    let ask = |request: &str| json!({"type":"session.list","requestId":request,"accountUserId":null,"accountEpoch":epoch});
    let mine = Uuid::now_v7().to_string();
    asking.command(ask(&mine)).await.unwrap();
    assert_eq!(answered(&mut asking, &mine).await["type"], "session.result");
    let theirs = Uuid::now_v7().to_string();
    other.command(ask(&theirs)).await.unwrap();
    assert_eq!(
        answered(&mut other, &theirs).await["type"],
        "session.result"
    );
    handle.shutdown().await;
}

async fn answered(client: &mut Client, request: &str) -> Value {
    loop {
        let frame = client.next().await.unwrap();
        if let Some(answer) = crate::protocol::answer(&frame["event"], request) {
            return answer.unwrap();
        }
        assert!(frame["event"].get("requestId").is_none(), "{frame}");
    }
}

async fn hosted(kind: HostKind) -> (tempfile::TempDir, PathBuf, RuntimeHandle) {
    let storage = tempfile::tempdir().unwrap();
    let mut config =
        crate::Config::isolated(&kodosi_os::fs::canonicalize(storage.path()).unwrap()).unwrap();
    config.host = kind;
    let root = config.data_root.clone();
    let handle = crate::start(config).await.unwrap();
    (storage, root, handle)
}
#[tokio::test]
async fn idle_background_hosts_identify_themselves_and_yield() {
    let (_storage, root, handle) = hosted(HostKind::Background).await;
    let status = host_status(&root).await.unwrap();
    assert_eq!(
        status.host,
        HostDescription {
            pid: std::process::id(),
            kind: HostKind::Background
        }
    );
    assert_eq!(status.local_sessions, 0);
    stop_other_host(&root, false).await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), handle.stopped())
        .await
        .unwrap();
}
#[tokio::test]
async fn a_foreground_host_stops_only_from_its_own_terminal() {
    let (_storage, root, handle) = hosted(HostKind::Foreground).await;
    for force in [false, true] {
        assert!(matches!(
            stop_other_host(&root, force).await,
            Err(Error::HostBusy(_))
        ));
    }
    handle.shutdown().await;
}

async fn serve_into(
    claim: AppClaim,
    accept: bool,
) -> (LocalServer, tokio::sync::mpsc::UnboundedReceiver<Value>) {
    let (sender, received) = tokio::sync::mpsc::unbounded_channel();
    let server = serve_activations(claim, move |bytes| {
        sender
            .send(serde_json::from_slice(&bytes).unwrap())
            .unwrap();
        accept
    })
    .await
    .unwrap();
    (server, received)
}

#[tokio::test]
async fn one_app_claims_a_folder_and_later_launches_hand_it_their_request() {
    let root = tempfile::tempdir().unwrap();
    let claim = claim_app(root.path()).unwrap().unwrap();
    assert!(claim_app(root.path()).unwrap().is_none());
    let (server, mut received) = serve_into(claim, true).await;
    let activation = json!({ "route": "kodosi://room/abc", "token": "xdg-token" });
    for _ in 0..2 {
        let forwarded = forward_activation(root.path(), &activation, Duration::from_secs(5))
            .await
            .unwrap();
        assert!(matches!(forwarded, Forwarded::Delivered));
        assert_eq!(received.recv().await.unwrap(), activation);
    }
    server.shutdown().await;
    assert!(claim_released(root.path()).await);
}

#[tokio::test]
async fn a_waiting_launch_claims_the_folder_when_the_owner_exits() {
    let root = tempfile::tempdir().unwrap();
    let starting = claim_app(root.path()).unwrap().unwrap();
    let exit = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        drop(starting);
    });
    let forwarded = forward_activation(root.path(), &json!({}), Duration::from_secs(5))
        .await
        .unwrap();
    assert!(matches!(forwarded, Forwarded::Claimed(_)));
    exit.await.unwrap();
    assert!(claim_app(root.path()).unwrap().is_none());
    drop(forwarded);
    assert!(claim_released(root.path()).await);
}

// A process that another test forks holds our open files until it execs.
async fn claim_released(root: &Path) -> bool {
    for _ in 0..100 {
        if claim_app(root).unwrap().is_some() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    false
}

#[tokio::test]
async fn a_closing_owner_leaves_requests_unanswered_until_its_claim_passes_on() {
    let root = tempfile::tempdir().unwrap();
    let (server, mut received) = serve_into(claim_app(root.path()).unwrap().unwrap(), false).await;
    let launch = {
        let root = root.path().to_owned();
        tokio::spawn(async move {
            forward_activation(&root, &json!({ "route": null }), Duration::from_secs(5)).await
        })
    };
    assert_eq!(received.recv().await.unwrap(), json!({ "route": null }));
    server.shutdown().await;
    assert!(matches!(
        launch.await.unwrap().unwrap(),
        Forwarded::Claimed(_)
    ));
}

#[tokio::test]
async fn activations_from_another_version_or_of_another_shape_are_refused() {
    let root = tempfile::tempdir().unwrap();
    let (server, mut received) = serve_into(claim_app(root.path()).unwrap().unwrap(), true).await;
    let endpoint =
        role_endpoint(&kodosi_os::fs::canonicalize(root.path()).unwrap(), "app").unwrap();
    for request in [
        json!({ "version": 2, "activation": {} }),
        json!({ "version": 1, "activation": "kodosi://room/abc" }),
    ] {
        let mut stream = kodosi_os::ipc::connect(&endpoint).await.unwrap();
        write_json(&mut stream, &request).await.unwrap();
        let reply: Value = read_json(&mut FrameReader::new(stream)).await.unwrap();
        assert_eq!(reply, json!({ "version": 1, "accepted": false }));
    }
    for activation in [
        json!("kodosi://room/abc"),
        json!({ "pad": "x".repeat(MAX_ACTIVATION_BYTES) }),
    ] {
        assert!(matches!(
            forward_activation(root.path(), &activation, Duration::from_secs(1)).await,
            Err(Error::Invalid(_))
        ));
    }
    server.shutdown().await;
    assert!(received.try_recv().is_err());
}
#[test]
fn a_started_host_gets_the_session_environment_and_this_commands_settings() {
    let pairs = |list: &[(&str, &str)]| {
        list.iter()
            .map(|(name, value)| {
                (
                    std::ffi::OsString::from(name),
                    std::ffi::OsString::from(value),
                )
            })
            .collect::<Vec<_>>()
    };
    let session = pairs(&[
        ("PATH", "/usr/bin"),
        ("SESSION_ONLY", "1"),
        ("KODOSI_DATA_ROOT", "/session"),
    ]);
    let own = pairs(&[
        ("PATH", "/agent/bin"),
        ("GH_TOKEN", "fake"),
        ("KODOSI_DATA_ROOT", "/isolated"),
        ("KODOSI__BACKEND__API", "http://127.0.0.1:5180"),
    ]);
    assert_eq!(
        client::host_environment(session, own),
        pairs(&[
            ("PATH", "/usr/bin"),
            ("SESSION_ONLY", "1"),
            ("KODOSI_DATA_ROOT", "/isolated"),
            ("KODOSI__BACKEND__API", "http://127.0.0.1:5180"),
        ])
    );
}
