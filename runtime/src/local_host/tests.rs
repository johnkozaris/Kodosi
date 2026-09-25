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
    assert!(reader.body.is_empty());
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
fn endpoint_rejects_foreign_file_and_symlink_shapes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("host.sock");
    std::fs::write(&path, b"not a socket").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(existing_owned_file(&path, true).is_err());
    let link = directory.path().join("link");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    assert!(existing_owned_file(&link, false).is_err());
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
    assert!(serde_json::from_str::<Hello>(r#"{"version":18,"root":"a","sessionId":null}"#).is_ok());
    assert!(serde_json::from_str::<Hello>(r#"{"version":18}"#).is_err());
    assert!(serde_json::from_str::<Hello>(r#"{"version":18,"root":"a","legacy":true}"#).is_err());
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
    let events = [
        json!({"type":"sessions.snapshot","sessions":[{"kind":"local"},{"kind":"remote"},{"kind":"local"}]}),
        json!({"type":"auth.required"}),
    ];
    assert_eq!(local_sessions_in(&events), 2);
    assert_eq!(local_sessions_in(&[]), 0);
}
async fn hosted(kind: HostKind) -> (tempfile::TempDir, PathBuf, RuntimeHandle) {
    let storage = tempfile::tempdir().unwrap();
    let mut config = crate::Config::isolated(&storage.path().canonicalize().unwrap()).unwrap();
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
