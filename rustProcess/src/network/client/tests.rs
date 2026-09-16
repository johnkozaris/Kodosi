use super::*;

fn fixture() -> (tempfile::TempDir, Network) {
    let root = tempfile::tempdir().unwrap();
    let network = Network::new(NetworkConfig {
        api_url: reqwest::Url::parse("http://127.0.0.1:1/").unwrap(),
        issuer: "http://127.0.0.1:1".into(),
        client_id: "test".into(),
        scopes: vec![],
        data_root: root.path().to_owned(),
        secret_service: "test".into(),
        isolated: true,
    })
    .unwrap();
    (root, network)
}

#[test]
fn legacy_identity_pins_are_not_silently_replaced() {
    let (root, network) = fixture();
    let legacy = root.path().join("device-list-pins.json");
    std::fs::write(&legacy, b"existing trust").unwrap();
    assert!(matches!(
        Network::new(network.inner.config.clone()),
        Err(Error::Trust(_))
    ));
    assert_eq!(std::fs::read(legacy).unwrap(), b"existing trust");
}

#[tokio::test]
async fn queued_command_never_becomes_a_new_account_request() {
    let (_root, network) = fixture();
    let guard = network.inner.operations.lock().await;
    let operation = network.execute("friends.refresh", Value::Null);
    tokio::pin!(operation);
    assert!(futures_util::poll!(&mut operation).is_pending());
    network.inner.generation.fetch_add(1, Ordering::AcqRel);
    drop(guard);
    assert!(matches!(operation.await, Err(Error::Stale)));
}

#[tokio::test]
async fn delayed_events_keep_their_original_account() {
    let (_root, network) = fixture();
    let mut events = network.events();
    let old = network.generation();
    network.inner.generation.fetch_add(1, Ordering::AcqRel);
    network.emit_for(
        old,
        Some("old-account".into()),
        json!({"type":"auth.notice","message":"old"}),
    );
    let event = events.recv().await.unwrap();
    assert_eq!(event.generation, old);
    assert_eq!(event.user_id.as_deref(), Some("old-account"));
}

#[tokio::test]
async fn quit_retains_saved_signin_but_logout_removes_it() {
    let (_root, network) = fixture();
    network
        .inner
        .state
        .lock()
        .await
        .secrets
        .store("tokens", "saved-token")
        .unwrap();
    network.shutdown().await;
    assert_eq!(
        network
            .inner
            .state
            .lock()
            .await
            .secrets
            .load("tokens")
            .unwrap()
            .unwrap()
            .as_str(),
        "saved-token"
    );
    let (_root, network) = fixture();
    network
        .inner
        .state
        .lock()
        .await
        .secrets
        .store("tokens", "saved-token")
        .unwrap();
    network.execute("auth.logout", Value::Null).await.unwrap();
    assert!(
        network
            .inner
            .state
            .lock()
            .await
            .secrets
            .load("tokens")
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn logout_cancels_credentials_without_deleting_device_identity() {
    let (_root, network) = fixture();
    let user = Uuid::now_v7().to_string();
    let credentials = {
        let state = network.inner.state.lock().await;
        Credentials {
            user_id: user.clone(),
            token: Zeroizing::new("secret".into()),
            keys: Arc::new(DeviceKeys::load_or_create(&state.secrets, &user).unwrap()),
            enrolled: true,
            generation: network.generation(),
            cancel: state.account_cancel.clone(),
        }
    };
    let device = credentials.keys.device_id.clone();
    *network.inner.credentials.write().unwrap() = Some(credentials.clone());
    network.execute("auth.logout", Value::Null).await.unwrap();
    assert!(credentials.cancel.is_cancelled());
    assert!(network.check_credentials(&credentials).is_err());
    let restored =
        DeviceKeys::load_or_create(&network.inner.state.lock().await.secrets, &user).unwrap();
    assert_eq!(restored.device_id, device);
}

#[tokio::test]
async fn explicit_pending_device_removal_survives_restart_without_clearing_pins() {
    let (_root, network) = fixture();
    let user = Uuid::now_v7().to_string();
    let device = Uuid::now_v7().to_string();
    network.block_device(&user, &device).await.unwrap();
    let restored = Network::new(network.inner.config.clone()).unwrap();
    assert!(
        restored
            .inner
            .state
            .lock()
            .await
            .blocked_devices
            .contains(&(user, device))
    );
}

#[tokio::test]
async fn failed_unpublish_retains_the_cleanup_obligation() {
    let (_root, network) = fixture();
    let id = Uuid::now_v7();
    let incarnation = Uuid::now_v7();
    let (requests, _) = mpsc::channel(4);
    let (_output_tx, output) = broadcast::channel(4);
    let publication = Arc::new(relay::Publication::new(
        LocalPublication {
            session_id: id,
            incarnation_id: incarnation,
            name: "test".into(),
            room_id: None,
            shared_with: BTreeSet::new(),
        },
        requests,
        output,
        SessionDto {
            id,
            incarnation_id: incarnation,
            name: "test".into(),
            owner_user_id: Uuid::now_v7().to_string(),
            owner_name: "test".into(),
            host_device_id: Uuid::now_v7().to_string(),
            host_name: "test".into(),
            room_id: None,
            room_name: None,
            shared_with: vec![],
            authorization_revision: 1,
            key_generation: 1,
            ready: true,
            host_online: true,
        },
    ));
    network
        .inner
        .publications
        .lock()
        .await
        .insert(id, Arc::clone(&publication));
    assert!(matches!(
        relay::bootstrap(&publication, Uuid::now_v7()).await,
        Err(Error::Closed)
    ));
    publication.drained.cancel();
    for _ in 0..2 {
        assert!(
            tokio::time::timeout(Duration::from_secs(1), network.unpublish(id))
                .await
                .unwrap()
                .is_err()
        );
    }
    assert!(publication.cancel.is_cancelled());
    assert!(network.inner.publications.lock().await.contains_key(&id));
}

#[tokio::test]
async fn expired_publication_discards_pending_grants_before_reconciliation() {
    let (_root, network) = fixture();
    let id = Uuid::now_v7();
    let incarnation = Uuid::now_v7();
    let friend = Uuid::now_v7().to_string();
    let (requests, _) = mpsc::channel(4);
    let (_output, output) = broadcast::channel(4);
    let publication = Arc::new(relay::Publication::new(
        LocalPublication {
            session_id: id,
            incarnation_id: incarnation,
            name: "live shell".into(),
            room_id: Some(Uuid::now_v7()),
            shared_with: BTreeSet::from([friend.clone()]),
        },
        requests,
        output,
        SessionDto {
            id,
            incarnation_id: incarnation,
            name: "live shell".into(),
            owner_user_id: Uuid::now_v7().to_string(),
            owner_name: "owner".into(),
            host_device_id: "host".into(),
            host_name: "host".into(),
            room_id: None,
            room_name: None,
            shared_with: vec![friend.clone()],
            authorization_revision: 4,
            key_generation: 2,
            ready: false,
            host_online: false,
        },
    ));
    *publication.pending_shares.lock().await = Some(BTreeSet::from([friend]));
    network
        .inner
        .publications
        .lock()
        .await
        .insert(id, Arc::clone(&publication));
    publication.expire_sharing().await;
    network.reconcile_shares().await.unwrap();
    let info = publication.info.read().await;
    assert!(info.shared_with.is_empty());
    assert!(info.room_id.is_none());
    assert_eq!(info.incarnation_id, incarnation);
    drop(info);
    assert!(publication.pending_shares.lock().await.is_none());
    assert!(!publication.cancel.is_cancelled());
}

#[test]
fn host_labels_are_bounded_trimmed_and_have_a_fallback() {
    assert_eq!(
        normalize_host_label("  office  ").as_deref(),
        Some("office")
    );
    assert!(normalize_host_label("  ").is_none());
    assert!(normalize_host_label("bad\nname").is_none());
    assert!(normalize_host_label(&"é".repeat(100)).unwrap().len() <= 128);
}
