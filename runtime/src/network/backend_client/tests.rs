use super::*;

fn fixture() -> (tempfile::TempDir, BackendClient) {
    let root = tempfile::tempdir().unwrap();
    let network = BackendClient::new(BackendConfig {
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
        json!({"type":"auth.finalizing"}),
    );
    let event = events.recv().await.unwrap();
    assert_eq!(event.generation, old);
    assert_eq!(event.user_id.as_deref(), Some("old-account"));
}

#[tokio::test]
async fn a_sign_in_without_a_network_keeps_the_saved_sign_in() {
    let (_root, network) = fixture();
    let saved = r#"{"access_token":"access","refresh_token":"refresh","expires_at":0}"#;
    network
        .inner
        .state
        .lock()
        .await
        .secrets
        .store("tokens", saved)
        .unwrap();
    let error = network
        .execute("auth.login.start", Value::Null)
        .await
        .unwrap_err();
    assert!(error.unanswered(), "{error}");
    assert!(network.inner.restore_pending.load(Ordering::Acquire));
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
        saved
    );
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
            notice: None,
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
    let restored = BackendClient::new(network.inner.config.clone()).unwrap();
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
    let publication = Arc::new(terminal_connections::Publication::new(
        LocalPublication {
            session_id: id,
            incarnation_id: incarnation,
            name: "test".into(),
            mission_id: None,
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
            mission_id: None,
            mission_name: None,
            shared_with: vec![],
            authorization_revision: 1,
            host_online: true,
        },
    ));
    network
        .inner
        .publications
        .lock()
        .await
        .insert(id, Arc::clone(&publication));
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
async fn missing_publication_discards_pending_grants_before_reconciliation() {
    let (_root, network) = fixture();
    let id = Uuid::now_v7();
    let incarnation = Uuid::now_v7();
    let friend = Uuid::now_v7().to_string();
    let (requests, _) = mpsc::channel(4);
    let (_output, output) = broadcast::channel(4);
    let publication = Arc::new(terminal_connections::Publication::new(
        LocalPublication {
            session_id: id,
            incarnation_id: incarnation,
            name: "live shell".into(),
            mission_id: Some(Uuid::now_v7()),
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
            mission_id: None,
            mission_name: None,
            shared_with: vec![friend.clone()],
            authorization_revision: 4,
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
    publication.clear_sharing().await;
    network.reconcile_shares().await.unwrap();
    let info = publication.info.read().await;
    assert!(info.shared_with.is_empty());
    assert!(info.mission_id.is_none());
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

#[tokio::test]
async fn cancelling_a_login_reports_a_cancelled_sign_out() {
    let (_root, network) = fixture();
    let reply = network
        .execute("auth.login.cancel", Value::Null)
        .await
        .unwrap();
    assert_eq!(
        reply.events,
        vec![json!({"type":"auth.required","reason":"cancelled"})]
    );
    assert!(network.identity().is_none());
}

#[tokio::test]
async fn retiring_a_terminal_leaves_operations_free_while_its_host_drains() {
    let (_root, network) = fixture();
    let id = Uuid::now_v7();
    let incarnation = Uuid::now_v7();
    let (requests, _host) = mpsc::channel(1);
    let (_frames, output) = broadcast::channel(1);
    let dto: SessionDto = serde_json::from_value(json!({
        "id":id,"incarnationId":incarnation,"name":"Terminal","ownerUserId":"owner","ownerName":"Owner",
        "hostDeviceId":"device","hostName":"Host","missionId":null,"missionName":null,"sharedWith":[],
        "authorizationRevision":1,"hostOnline":true
    }))
    .unwrap();
    let info = LocalPublication {
        session_id: id,
        incarnation_id: incarnation,
        name: "Terminal".into(),
        mission_id: None,
        shared_with: BTreeSet::new(),
    };
    network.inner.publications.lock().await.insert(
        id,
        Arc::new(terminal_connections::Publication::new(
            info, requests, output, dto,
        )),
    );
    let retirement = network.unpublish(id);
    tokio::pin!(retirement);
    assert!(futures_util::poll!(&mut retirement).is_pending());
    assert!(network.inner.operations.try_lock().is_ok());
}

#[test]
fn a_typed_code_selects_only_the_request_with_the_keys_of_the_device_that_showed_it() {
    use crate::identity::link_code::{LinkIdentity, LinkKey};

    const USER: &str = "11111111-1111-1111-1111-111111111111";
    const CODE: &str = "TAFX-E5HG-TN8E";
    let proof = LinkKey::derive(CODE, &[4; 16])
        .request_proof(&LinkIdentity {
            user_id: USER,
            device_id: "laptop",
            label: "Laptop",
            signing_public_key: &[1; 1952],
        })
        .unwrap();
    let request = |device: &str, signing: u8| {
        serde_json::from_value::<enrollment::LinkRequest>(json!({
            "requestId": Uuid::now_v7(),
            "deviceId": device,
            "deviceLabel": "Laptop",
            "signingPublicKey": BASE64.encode([signing; 1952]),
            "nonce": BASE64.encode([4; 16]),
            "proof": BASE64.encode(proof),
            "expiresAt": "2026-10-04T12:00:00Z",
        }))
        .unwrap()
    };
    let requests = vec![request("other", 1), request("laptop", 1)];
    let found = enrollment::request_with_code(USER, requests, CODE)
        .unwrap()
        .unwrap();
    assert_eq!(found.device_id, "laptop");
    for (requests, code) in [
        (vec![request("laptop", 2)], CODE),
        (vec![request("laptop", 1)], "TAFX-E5HG-TN8F"),
    ] {
        assert!(
            enrollment::request_with_code(USER, requests, code)
                .unwrap()
                .is_none()
        );
    }
}
