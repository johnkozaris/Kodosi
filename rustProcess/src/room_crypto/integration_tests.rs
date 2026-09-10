use super::*;
use crate::identity_core::{
    device_cert::{build_cert_for, build_self_cert},
    device_keys::generate_device_keys_for_test,
    device_list_pin_store::{DeviceListPinStore, DeviceListPinStoreHandle},
    signed_device_list::{DeviceListEntry, build_bootstrap_list, build_replacement_list},
};
use kodosi_backend_client::{
    api::{UserDeviceCertificateDto, UserDeviceListDto, UserIdentityBundleDto},
    config::BackendClientConfig,
    http_client::BackendHttpClient,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[tokio::test]
async fn late_join_content_is_typed_and_room_snapshot_is_reused() {
    let user = "11111111-1111-4111-8111-111111111111";
    let keys = generate_device_keys_for_test(user).unwrap();
    let other_device = generate_device_keys_for_test(user).unwrap();
    let now = current_epoch_ms().unwrap();
    let signing = keys.signing_key().unwrap();
    let cert = build_self_cert(
        user,
        &keys.device_id,
        "Owner",
        keys.kem_public_bytes(),
        &signing,
        now - 1000,
        None,
    )
    .unwrap();
    let list = build_bootstrap_list(user, &keys.device_id, &signing, now - 1000, None).unwrap();
    let mut identity = UserIdentityBundleDto {
        user_id: user.to_owned(),
        identity_revision: 1,
        identity_incarnation_id: uuid::Uuid::now_v7(),
        device_list: UserDeviceListDto {
            body: BASE64.encode(&list.body_bytes),
            signature: BASE64.encode(&list.signature),
        },
        devices: vec![UserDeviceCertificateDto {
            certificate: BASE64.encode(&cert.body_bytes),
            certificate_signature: BASE64.encode(&cert.signature),
        }],
        historical_devices: vec![],
    };
    let roster = SignedRoomRoster {
        version: ROOM_ROSTER_VERSION,
        room_id: "room".to_owned(),
        generation: 1,
        owner_user_id: user.to_owned(),
        member_user_ids: vec![user.to_owned()],
        signer_device_id: keys.device_id.clone(),
        issued_at_ms: now - 500,
    };
    let body = serde_json::to_vec(&roster).unwrap();
    let signature = BASE64.encode(
        crypto::sign_control_message(
            keys.signing_pkcs8_bytes(),
            &domain_preimage(kodosi_domain::domain_tags::ROOM_ROSTER_V1, &body),
        )
        .unwrap(),
    );
    let transition = serde_json::json!({"generation":1,"rosterBody":BASE64.encode(&body),"rosterSignature":signature,"rosterSignerDeviceId":keys.device_id});
    let room = serde_json::json!({"id":"room","name":"Mission","slug":"mission","ownerUserId":user,
        "rosterGeneration":1,"rosterBody":BASE64.encode(&body),"rosterSignature":signature,
        "rosterSignerDeviceId":keys.device_id,"rosterActivationProof":null,"admissionProofs":[],"rosterTransitions":[transition]});
    let shared_identity = Arc::new(std::sync::Mutex::new(identity.clone()));
    let tasks = Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new()));
    let requests = Arc::new(AtomicUsize::new(0));
    let room_reads = Arc::clone(&requests);
    let app = axum::Router::new()
        .route(
            "/api/rooms/",
            axum::routing::get(move || {
                let room = room.clone();
                let reads = Arc::clone(&room_reads);
                async move {
                    reads.fetch_add(1, Ordering::SeqCst);
                    ([("kodosi-has-more", "false")], axum::Json(vec![room]))
                }
            }),
        )
        .route(
            "/api/rooms/room/roster-transitions",
            axum::routing::get(move || {
                let transition = transition.clone();
                async move { ([("kodosi-has-more", "false")], axum::Json(vec![transition])) }
            }),
        )
        .route(
            "/api/rooms/room/admission-proofs",
            axum::routing::get(|| async {
                (
                    [("kodosi-has-more", "false")],
                    axum::Json(Vec::<serde_json::Value>::new()),
                )
            }),
        )
        .route(
            "/api/rooms/room/members",
            axum::routing::get(|| async { axum::Json(Vec::<serde_json::Value>::new()) }),
        )
        .route(
            "/api/users/11111111-1111-4111-8111-111111111111/identity",
            axum::routing::get({
                let identity = Arc::clone(&shared_identity);
                move || { let identity = Arc::clone(&identity);
                    async move { axum::Json(identity.lock().unwrap().clone()) }
                }
            }),
        )
        .route("/api/rooms/room/tasks", axum::routing::get({
            let tasks = Arc::clone(&tasks);
            move || { let tasks = Arc::clone(&tasks);
                async move { ([("kodosi-has-more", "false"),
                    ("kodosi-task-snapshot", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")],
                    axum::Json(tasks.lock().unwrap().clone())) }
            }
        }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let directory = tempfile::tempdir().unwrap();
    let pins = DeviceListPinStoreHandle::from_store(
        DeviceListPinStore::load_from(directory.path().join("pins.json")).unwrap(),
    )
    .unwrap();
    pins.bind_to_user(user).await.unwrap();
    let client = BackendHttpClient::new(&BackendClientConfig {
        api: Some(format!("http://{address}/")),
        ..Default::default()
    })
    .unwrap();
    let mut sender = RoomCryptoContext::from_parts(
        client.clone(),
        pins.clone(),
        directory.path().join("rosters.json"),
        user.to_owned(),
        keys,
    );
    let encrypted = sender
        .encrypt_json("room", "message", "chat", &"Readable message")
        .await
        .unwrap();
    let old_task = sender
        .encrypt_json(
            "room",
            "old-task",
            "task",
            &RoomTaskPrivate {
                title: "Old task".to_owned(),
                description: None,
            },
        )
        .await
        .unwrap();
    let keys = sender.local_keys;
    let mut reader = RoomCryptoContext::from_parts(
        client.clone(),
        pins.clone(),
        directory.path().join("rosters.json"),
        user.to_owned(),
        keys,
    );
    requests.store(0, Ordering::SeqCst);
    for _ in 0..3 {
        let plain: String = reader
            .decrypt_json("room", "message", "chat", user, &encrypted)
            .await
            .unwrap();
        assert_eq!(plain, "Readable message");
    }
    assert_eq!(
        requests.load(Ordering::SeqCst),
        1,
        "one verified room snapshot per hydration context"
    );
    let mut newcomer = RoomCryptoContext::from_parts(
        client.clone(),
        pins.clone(),
        directory.path().join("rosters.json"),
        user.to_owned(),
        other_device,
    );
    let unavailable = newcomer
        .decrypt_json::<String>("room", "message", "chat", user, &encrypted)
        .await
        .unwrap_err();
    assert!(matches!(unavailable, AppError::RoomContentNotRecipient));

    let new_cert = build_cert_for(
        user,
        &newcomer.local_keys.device_id,
        "New device",
        newcomer.local_keys.kem_public_bytes(),
        newcomer.local_keys.signing_public_bytes(),
        &reader.local_keys.device_id,
        &signing,
        now,
        None,
    )
    .unwrap();
    let original = DeviceListEntry {
        device_id: reader.local_keys.device_id.clone(),
        signer_device_id: reader.local_keys.device_id.clone(),
    };
    let next_list = build_replacement_list(
        user,
        1,
        std::slice::from_ref(&original),
        vec![
            original.clone(),
            DeviceListEntry {
                device_id: newcomer.local_keys.device_id.clone(),
                signer_device_id: original.device_id.clone(),
            },
        ],
        &original.device_id,
        &signing,
        now,
        None,
    )
    .unwrap();
    identity.device_list = UserDeviceListDto {
        body: BASE64.encode(&next_list.body_bytes),
        signature: BASE64.encode(&next_list.signature),
    };
    identity.devices.push(UserDeviceCertificateDto {
        certificate: BASE64.encode(&new_cert.body_bytes),
        certificate_signature: BASE64.encode(&new_cert.signature),
    });
    *shared_identity.lock().unwrap() = identity;
    let mut enrolled_sender = RoomCryptoContext::from_parts(
        client,
        pins.clone(),
        directory.path().join("rosters.json"),
        user.to_owned(),
        reader.local_keys,
    );
    let new_task = enrolled_sender
        .encrypt_json(
            "room",
            "new-task",
            "task",
            &RoomTaskPrivate {
                title: "Readable new task".to_owned(),
                description: Some("Keeps the queue usable".to_owned()),
            },
        )
        .await
        .unwrap();
    let record = |id: &str, title: &str| {
        serde_json::json!({
            "id":id, "roomId":"room", "createdByUserId":user, "title":title, "description":null,
            "status":"Open", "revision":1, "assignedSessionId":null, "assignedSessionIncarnationId":null,
            "dueAt":null, "createdAt":"2026-09-01T00:00:00Z", "updatedAt":"2026-09-01T00:00:00Z",
            "completedAt":null, "result":null, "resultAuthorUserId":null
        })
    };
    *tasks.lock().unwrap() = vec![record("old-task", &old_task), record("new-task", &new_task)];
    let mut config = crate::config::AppConfig::default();
    config.backend.api = Some(format!("http://{address}/"));
    config.auth.keyring_service = format!("kodosi.task-history.test.{}", uuid::Uuid::now_v7());
    let dependencies = crate::runtime::RuntimeDependencies::isolated(&config)
        .unwrap()
        .with_pin_store(pins);
    let mut runtime = Runtime::with_dependencies(
        config,
        tokio_util::sync::CancellationToken::new(),
        dependencies,
    )
    .unwrap();
    runtime.state.identity.auth = kodosi_domain::auth::AuthState::Authenticated {
        subject: Some(user.try_into().unwrap()),
        expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    runtime
        .device_key_store
        .save_for_test(user, &newcomer.local_keys)
        .unwrap();
    let page = crate::runtime::rooms::RoomApplication::new(&runtime)
        .fetch_room_tasks_page("room", None, None, 0, 100, None)
        .await
        .unwrap();
    assert_eq!(page.items.len(), 2);
    assert!(page.items[0].content_unavailable);
    assert_eq!(page.items[0].id, "old-task");
    assert_eq!(page.items[1].title, "Readable new task");
    assert!(!page.items[1].content_unavailable);
    tasks.lock().unwrap()[0]["title"] = serde_json::json!("tampered");
    assert!(
        crate::runtime::rooms::RoomApplication::new(&runtime)
            .fetch_room_tasks_page("room", None, None, 0, 100, None)
            .await
            .is_err(),
        "malformed ciphertext must not be treated as unavailable history"
    );
    server.abort();
}
