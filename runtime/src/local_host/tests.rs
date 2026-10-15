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
    assert!(lock_root(directory.path()).is_ok());
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
fn local_protocol_requires_exact_version_and_root() {
    assert!(serde_json::from_str::<Hello>(r#"{"version":20,"root":"a","sessionId":null}"#).is_ok());
    assert!(serde_json::from_str::<Hello>(r#"{"version":19}"#).is_err());
    assert!(serde_json::from_str::<Hello>(r#"{"version":20,"root":"a","legacy":true}"#).is_err());
}
#[test]
fn stop_permission_protects_the_app_and_open_terminals() {
    assert!(stop_permission(HostKind::App, true, 0).is_err());
    assert!(stop_permission(HostKind::Foreground, false, 0).is_err());
    assert!(stop_permission(HostKind::Foreground, true, 2).is_ok());
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
async fn foreground_hosts_only_stop_when_forced() {
    let (_storage, root, handle) = hosted(HostKind::Foreground).await;
    assert!(matches!(
        stop_other_host(&root, false).await,
        Err(Error::HostBusy(_))
    ));
    stop_other_host(&root, true).await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), handle.stopped())
        .await
        .unwrap();
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
