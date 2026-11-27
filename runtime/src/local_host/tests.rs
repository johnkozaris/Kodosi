use super::*;
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
#[tokio::test]
async fn a_video_connection_tells_the_app_what_the_runtime_showed_in_order() {
    use crate::video::Shown;
    let (mut writer, input) = tokio::io::duplex(1024);
    let mut reader = FrameReader::new(input);
    let shown = [
        Shown::Stale(true),
        Shown::Message(Bytes::from_static(b"\0{}")),
        Shown::Stale(false),
        Shown::Frame(Bytes::from_static(&[1, 0, 0, 0, 7, 1, 0, 0, 0, 0, 1, 0x65])),
    ];
    for next in &shown {
        write_shown(&mut writer, next).await.unwrap();
    }
    for next in shown {
        assert_eq!(read_shown(&mut reader).await.unwrap(), next);
    }
}
#[tokio::test]
async fn a_host_names_itself_to_another_version_and_stops_for_it() {
    let (_storage, root, handle) = hosted(HostKind::Background).await;
    let root = kodosi_os::fs::canonicalize(&root).unwrap();
    let mut stream = kodosi_os::ipc::connect(&endpoint(&root).unwrap())
        .await
        .unwrap();
    let older = json!({"version":VERSION - 1,"root":root_identity(&root),"sessionId":null});
    write_json(&mut stream, &older).await.unwrap();
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
    write_json(
        &mut stream,
        &ClientRequest::Stop {
            force: false,
            update: false,
        },
    )
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
        Client::connect(&root, ClientKind::App).await,
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
    let client = connect_or_start(&root, ClientKind::App).await.unwrap();
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
async fn a_connection_that_a_stopping_host_closes_tries_again() {
    let storage = tempfile::tempdir().unwrap();
    let root = kodosi_os::fs::canonicalize(storage.path()).unwrap();
    let mut listener = kodosi_os::ipc::Listener::bind(&endpoint(&root).unwrap())
        .await
        .unwrap();
    let host = tokio::spawn(async move {
        drop(listener.accept().await.unwrap());
        welcome_client(
            &mut listener,
            vec![scoped("system.ready", None, 1)],
            Vec::new(),
        )
        .await
    });
    let client = connect_or_start(&root, ClientKind::Command).await.unwrap();
    assert_eq!(client.initial_events[0]["type"], "system.ready");
    drop(host.await.unwrap());
}

#[tokio::test]
async fn a_client_does_not_wait_for_a_saved_sign_in_that_comes_back_later() {
    let storage = tempfile::tempdir().unwrap();
    let root = kodosi_os::fs::canonicalize(storage.path()).unwrap();
    let mut listener = kodosi_os::ipc::Listener::bind(&endpoint(&root).unwrap())
        .await
        .unwrap();
    let host = tokio::spawn(async move {
        let mut offline = scoped("auth.restoring", None, 1);
        offline["reason"] = json!("offline");
        welcome_client(
            &mut listener,
            vec![
                scoped("system.ready", None, 1),
                scoped("auth.finalizing", None, 1),
            ],
            vec![offline],
        )
        .await
    });
    let client = tokio::time::timeout(
        Duration::from_secs(5),
        connect_or_start(&root, ClientKind::Command),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(client.initial_events.last().unwrap()["reason"], "offline");
    drop(host.await.unwrap());
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
async fn a_foreground_host_stops_only_from_its_own_terminal() {
    let (_storage, root, handle) = hosted(HostKind::Foreground).await;
    for force in [false, true] {
        assert!(matches!(
            stop_other_host(&root, force, false).await,
            Err(Error::HostBusy(_))
        ));
    }
    handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_a_connection_that_says_it_is_the_app_answers_the_questions_of_sharing() {
    let (_storage, root, handle) = hosted(HostKind::Background).await;
    for (kind, answer) in [
        (ClientKind::App, "This request has already ended."),
        (
            ClientKind::Command,
            "Only the Kodosi app can allow sharing.",
        ),
    ] {
        let mut client = Client::connect(&root, kind).await.unwrap();
        let scope = client.initial_events[0].clone();
        let request = Uuid::now_v7().to_string();
        client
            .command(json!({"type":"sharing.answer","requestId":request,"approvalId":Uuid::now_v7(),
                "allow":true,"accountUserId":scope["accountUserId"],"accountEpoch":scope["accountEpoch"]}))
            .await
            .unwrap();
        let heard = loop {
            let frame = client.next().await.unwrap();
            if frame["event"]["requestId"] == request.as_str() {
                break frame["event"].clone();
            }
        };
        assert_eq!(
            (heard["type"].as_str(), heard["message"].as_str()),
            (Some("sharing.error"), Some(answer))
        );
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
    let activation = json!({ "route": "kodosi://project/abc", "token": "xdg-token" });
    let mut delivered = activation.clone();
    delivered["appVersion"] = json!(crate::PRODUCT_VERSION);
    for _ in 0..2 {
        let forwarded = forward_activation(root.path(), &activation, Duration::from_secs(5))
            .await
            .unwrap();
        assert!(matches!(forwarded, Forwarded::Delivered));
        assert_eq!(received.recv().await.unwrap(), delivered);
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
    assert_eq!(
        received.recv().await.unwrap(),
        json!({ "route": null, "appVersion": crate::PRODUCT_VERSION })
    );
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
        json!({ "version": 1, "activation": "kodosi://project/abc" }),
    ] {
        let mut stream = kodosi_os::ipc::connect(&endpoint).await.unwrap();
        write_json(&mut stream, &request).await.unwrap();
        let reply: Value = read_json(&mut FrameReader::new(stream)).await.unwrap();
        assert_eq!(reply, json!({ "version": 1, "accepted": false }));
    }
    for activation in [
        json!("kodosi://project/abc"),
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
