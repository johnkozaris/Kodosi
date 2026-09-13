use super::interop::{execute, ready};
use super::*;
use crate::terminal::{
    LocalSession, RemoteTerminal, SessionChange, TerminalHistoryPolicy,
    validate_terminal_checkpoint,
};

pub(super) async fn revoked_device_can_explicitly_reapprove(owner: &Network, second: &Network) {
    let removed = second.identity().unwrap().device_id;
    owner
        .block_device(&owner.identity().unwrap().user_id, &removed)
        .await
        .unwrap();
    owner.reconcile_device_removals().await.unwrap();
    assert!(
        !owner
            .inner
            .state
            .lock()
            .await
            .blocked_devices
            .contains(&(owner.identity().unwrap().user_id, removed.clone()))
    );
    second.ensure_enrolled().await.unwrap();
    assert!(!second.identity().unwrap().enrolled);
    assert_eq!(
        second.identity().unwrap().device_id,
        removed,
        "revocation must not automatically replace local keys"
    );
    let response = execute(second, "devices.link.startSelf", Value::Null).await;
    let new_id = second.identity().unwrap().device_id;
    assert_ne!(new_id, removed);
    assert!(
        second
            .inner
            .state
            .lock()
            .await
            .pins
            .lock()
            .unwrap()
            .is_revoked(&second.identity().unwrap().user_id, &removed)
    );
    let code = response
        .events
        .iter()
        .find(|event| event["type"] == "devices.link.selfPending")
        .unwrap()["userCode"]
        .clone();
    execute(owner, "devices.link.approve", json!({"userCode":code})).await;
    second.poll_link().await.unwrap();
    assert!(second.identity().unwrap().enrolled);
    assert_eq!(second.identity().unwrap().device_id, new_id);
}

#[expect(
    clippy::too_many_lines,
    reason = "related live backend expiry, rekey and account retirement invariants"
)]
pub(super) async fn expired_friend_does_not_block_owner_and_retirement_unpublishes(
    owner: &Network,
    friend: &Network,
    second: &Network,
    root: &std::path::Path,
) {
    let credentials = friend.credentials().unwrap();
    let verified = friend
        .fetch_identity(&credentials.user_id, false)
        .await
        .unwrap();
    let id = Uuid::now_v7();
    let incarnation = Uuid::now_v7();
    let (changes, _events) = mpsc::channel::<SessionChange>(128);
    let local = LocalSession::spawn(
        id,
        incarnation,
        "/bin/sh".into(),
        vec!["-c".into(), "exec /bin/cat".into()],
        root.canonicalize().unwrap(),
        true,
        changes.clone(),
    )
    .await
    .unwrap();
    owner
        .publish(
            LocalPublication {
                session_id: id,
                incarnation_id: incarnation,
                name: "expiration fixture".into(),
                room_id: None,
                shared_with: [credentials.user_id.clone()].into(),
            },
            local.host_requests.clone(),
            local.output(),
        )
        .await
        .unwrap();
    ready(owner, id).await;
    let now = identity::now_ms().max(verified.list.issued_at_ms + 1);
    let list = build_replacement_list(
        &credentials.user_id,
        verified.generation,
        &verified.list.entries,
        verified.list.entries.clone(),
        &credentials.keys.device_id,
        &credentials.keys.signing_key().unwrap(),
        now,
        Some(now + 2000),
    )
    .unwrap();

    friend.inner.http.device::<Value>(Method::POST,"api/me/identity/device-list",&credentials,Some(json!({"signedDeviceList":BASE64.encode(list.body_bytes),"signedDeviceListSignature":BASE64.encode(list.signature)}))).await.unwrap();
    tokio::time::sleep(Duration::from_millis(2100)).await;
    owner.rekey_all().await.unwrap();
    let own = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(connection) = second.connect_remote(id).await {
                break connection;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        friend.connect_remote(id).await.is_err(),
        "expired friend must not receive a content key"
    );
    let remote = RemoteTerminal::spawn(own, changes);
    let mut view = remote.subscribe().await.unwrap();
    validate_terminal_checkpoint(&view.checkpoint, TerminalHistoryPolicy::default()).unwrap();
    remote
        .input(
            view.connection_id,
            bytes::Bytes::from_static(b"healthy-owner-after-expiry\n"),
        )
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut bytes = Vec::new();
        let mut sequence = view.next_sequence;
        loop {
            let frame = view.data.recv().await.unwrap();
            assert_eq!(frame.sequence, sequence);
            sequence += 1;
            bytes.extend_from_slice(&frame.bytes);
            if bytes
                .windows(b"healthy-owner-after-expiry".len())
                .any(|part| part == b"healthy-owner-after-expiry")
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    let checkpoint = remote.checkpoint(view.connection_id).await.unwrap();
    validate_terminal_checkpoint(&checkpoint.checkpoint, TerminalHistoryPolicy::default()).unwrap();
    owner.shutdown().await;
    let catalog: Vec<SessionDto> = second
        .inner
        .http
        .device(
            Method::GET,
            "api/sessions",
            &second.credentials().unwrap(),
            None,
        )
        .await
        .unwrap();
    assert!(
        !catalog.iter().any(|session| session.id == id),
        "Quit must delete publication metadata immediately"
    );
    assert!(
        owner
            .inner
            .state
            .lock()
            .await
            .secrets
            .load("tokens")
            .unwrap()
            .is_some(),
        "Quit must retain sign-in"
    );
    assert!(
        !local.is_closed(),
        "network retirement must not stop local PTY"
    );
    local.stop().await.unwrap();
    drop(local);
    remote.disconnect();
    drop(remote);
    drop(view);

    let id = Uuid::now_v7();
    let incarnation = Uuid::now_v7();
    let (changes, _events) = mpsc::channel(128);
    let local = LocalSession::spawn(
        id,
        incarnation,
        "/bin/sh".into(),
        vec!["-c".into(), "exec /bin/cat".into()],
        root.canonicalize().unwrap(),
        true,
        changes,
    )
    .await
    .unwrap();
    second
        .publish(
            LocalPublication {
                session_id: id,
                incarnation_id: incarnation,
                name: "logout fixture".into(),
                room_id: None,
                shared_with: BTreeSet::new(),
            },
            local.host_requests.clone(),
            local.output(),
        )
        .await
        .unwrap();
    ready(second, id).await;
    let credentials = second.credentials().unwrap();
    execute(second, "auth.logout", Value::Null).await;
    let mut cleanup_credentials = credentials;
    cleanup_credentials.cancel = CancellationToken::new();
    let catalog: Vec<SessionDto> = second
        .inner
        .http
        .device(Method::GET, "api/sessions", &cleanup_credentials, None)
        .await
        .unwrap();
    assert!(
        !catalog.iter().any(|session| session.id == id),
        "Logout must delete publication metadata immediately"
    );
    assert!(
        second
            .inner
            .state
            .lock()
            .await
            .secrets
            .load("tokens")
            .unwrap()
            .is_none()
    );
    assert!(!local.is_closed(), "logout must not stop local PTY");
    local.stop().await.unwrap();
    drop(local);
}
