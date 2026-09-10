use super::*;
use crate::identity_core::{
    device_cert::{build_cert_for, build_self_cert},
    device_keys::generate_device_keys_for_test,
    device_list_pin_store::{DeviceListPinStore, DeviceListPinStoreHandle},
    signed_device_list::{DeviceListEntry, build_replacement_list},
};
use kodosi_backend_client::{
    api::{UserDeviceCertificateDto, UserDeviceListDto, UserIdentityBundleDto},
    artifact_endorsement::ArtifactEndorsement,
    config::BackendClientConfig,
    http_client::BackendHttpClient,
};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn current_device_preserves_exact_roster_before_revocation_and_rejects_backdating() {
    const USER: &str = "11111111-1111-4111-8111-111111111111";
    let old = generate_device_keys_for_test(USER).unwrap();
    let survivor = generate_device_keys_for_test(USER).unwrap();
    let now = current_epoch_ms().unwrap();
    let old_signing = old.signing_key().unwrap();
    let survivor_signing = survivor.signing_key().unwrap();
    let old_cert = build_self_cert(
        USER,
        &old.device_id,
        "Original",
        old.kem_public_bytes(),
        &old_signing,
        now - 2000,
        None,
    )
    .unwrap();
    let survivor_cert = build_cert_for(
        USER,
        &survivor.device_id,
        "Survivor",
        survivor.kem_public_bytes(),
        survivor.signing_public_bytes(),
        &old.device_id,
        &old_signing,
        now - 1500,
        None,
    )
    .unwrap();
    let entries = vec![
        DeviceListEntry {
            device_id: old.device_id.clone(),
            signer_device_id: old.device_id.clone(),
        },
        DeviceListEntry {
            device_id: survivor.device_id.clone(),
            signer_device_id: old.device_id.clone(),
        },
    ];
    let list = build_replacement_list(
        USER,
        1,
        &entries,
        entries.clone(),
        &old.device_id,
        &old_signing,
        now - 1000,
        None,
    )
    .unwrap();
    let cert_dto = |cert: &crate::identity_core::device_cert::SignedDeviceCertificate| {
        UserDeviceCertificateDto {
            certificate: BASE64.encode(&cert.body_bytes),
            certificate_signature: BASE64.encode(&cert.signature),
        }
    };
    let old_dto = cert_dto(&old_cert);
    let survivor_dto = cert_dto(&survivor_cert);
    let incarnation = uuid::Uuid::now_v7();
    let bundle = UserIdentityBundleDto {
        user_id: USER.to_owned(),
        identity_revision: 1,
        identity_incarnation_id: incarnation,
        device_list: UserDeviceListDto {
            body: BASE64.encode(&list.body_bytes),
            signature: BASE64.encode(&list.signature),
        },
        devices: vec![old_dto.clone(), survivor_dto.clone()],
        historical_devices: vec![],
    };
    let shared_bundle = Arc::new(Mutex::new(bundle));
    let mut roster = SignedRoomRoster {
        version: ROOM_ROSTER_VERSION,
        room_id: "room".to_owned(),
        generation: 1,
        owner_user_id: USER.to_owned(),
        member_user_ids: vec![USER.to_owned()],
        signer_device_id: old.device_id.clone(),
        issued_at_ms: now - 500,
    };
    let roster_body = serde_json::to_vec(&roster).unwrap();
    let signature = BASE64.encode(
        crypto::sign_control_message(
            old.signing_pkcs8_bytes(),
            &domain_preimage(kodosi_domain::domain_tags::ROOM_ROSTER_V1, &roster_body),
        )
        .unwrap(),
    );
    let transition = serde_json::json!({"generation":1,"rosterBody":BASE64.encode(&roster_body),"rosterSignature":signature,"rosterSignerDeviceId":old.device_id});
    let room: kodosi_backend_client::api::BackendRoom=serde_json::from_value(serde_json::json!({
        "id":"room","name":"Mission","slug":"mission","ownerUserId":USER,"rosterGeneration":1,
        "rosterBody":BASE64.encode(&roster_body),"rosterSignature":signature,"rosterSignerDeviceId":old.device_id,
        "rosterActivationProof":null,"admissionProofs":[],"rosterTransitions":[transition]})).unwrap();
    let stale = ArtifactEndorsement {
        user_id: USER.to_owned(),
        identity_incarnation_id: incarnation,
        artifact_digest: "a".repeat(64),
        endorser_device_id: "previously-revoked-device".to_owned(),
        signature: BASE64.encode([0; 3309]),
    };
    let transferred_digest = "b".repeat(64);
    let transferable = ArtifactEndorsement {
        user_id: USER.to_owned(),
        identity_incarnation_id: incarnation,
        artifact_digest: transferred_digest.clone(),
        endorser_device_id: old.device_id.clone(),
        signature: BASE64.encode(
            crypto::sign_control_message(
                old.signing_pkcs8_bytes(),
                &kodosi_backend_client::artifact_endorsement::endorsement_preimage(
                    USER,
                    &incarnation,
                    &transferred_digest,
                    &old.device_id,
                )
                .unwrap(),
            )
            .unwrap(),
        ),
    };
    let endorsements = Arc::new(Mutex::new(vec![stale, transferable]));
    let app = axum::Router::new()
        .route(
            "/api/users/{user}/identity",
            axum::routing::get({
                let b = Arc::clone(&shared_bundle);
                move || {
                    let b = Arc::clone(&b);
                    async move { axum::Json(b.lock().unwrap().clone()) }
                }
            }),
        )
        .route(
            "/api/rooms/",
            axum::routing::get({
                let r = room.clone();
                move || {
                    let r = r.clone();
                    async move { ([("kodosi-has-more", "false")], axum::Json(vec![r])) }
                }
            }),
        )
        .route(
            "/api/rooms/room/roster-transitions",
            axum::routing::get(move || {
                let t = transition.clone();
                async move { ([("kodosi-has-more", "false")], axum::Json(vec![t])) }
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
            "/api/rooms/room/chat",
            axum::routing::get(|| async {
                (
                    [("kodosi-has-more", "false")],
                    axum::Json(Vec::<serde_json::Value>::new()),
                )
            }),
        )
        .route(
            "/api/rooms/room/tasks",
            axum::routing::get(|| async {
                (
                    [
                        ("kodosi-has-more", "false"),
                        (
                            "kodosi-task-snapshot",
                            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                        ),
                    ],
                    axum::Json(Vec::<serde_json::Value>::new()),
                )
            }),
        )
        .route(
            "/api/me/artifact-endorsements",
            axum::routing::get({
                let e = Arc::clone(&endorsements);
                move || {
                    let e = Arc::clone(&e);
                    async move { axum::Json(e.lock().unwrap().clone()) }
                }
            })
            .put({
                let e = Arc::clone(&endorsements);
                move |axum::Json(mut value): axum::Json<serde_json::Value>| {
                    let e = Arc::clone(&e);
                    async move {
                        value["userId"] = serde_json::json!(USER);
                        let replacement: ArtifactEndorsement =
                            serde_json::from_value(value).unwrap();
                        let mut stored = e.lock().unwrap();
                        stored.retain(|entry| entry.artifact_digest != replacement.artifact_digest);
                        stored.push(replacement);
                        axum::http::StatusCode::NO_CONTENT
                    }
                }
            }),
        )
        .route(
            "/api/users/{user}/artifact-endorsements",
            axum::routing::get({
                let e = Arc::clone(&endorsements);
                move |axum::extract::Query(q): axum::extract::Query<
                    std::collections::HashMap<String, String>,
                >| {
                    let e = Arc::clone(&e);
                    async move {
                        axum::Json(
                            e.lock()
                                .unwrap()
                                .iter()
                                .filter(|v| q.get("digest") == Some(&v.artifact_digest))
                                .cloned()
                                .collect::<Vec<_>>(),
                        )
                    }
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let pins = DeviceListPinStoreHandle::from_store(
        DeviceListPinStore::load_from(dir.path().join("pins.json")).unwrap(),
    )
    .unwrap();
    pins.bind_to_user(USER).await.unwrap();
    let client = BackendHttpClient::new(&BackendClientConfig {
        api: Some(format!("http://{address}/")),
        ..Default::default()
    })
    .unwrap();
    let mut before = RoomCryptoContext::from_parts(
        client.clone(),
        pins.clone(),
        dir.path().join("rosters.json"),
        USER.to_owned(),
        survivor,
    );
    before
        .preserve_history_before_revocation(&old.device_id)
        .await
        .unwrap();
    {
        let stored = endorsements.lock().unwrap();
        assert_eq!(stored.len(), 3);
        assert!(
            stored
                .iter()
                .any(|entry| entry.artifact_digest == transferred_digest
                    && entry.endorser_device_id == before.local_keys.device_id),
            "stale unrelated endorsements must not block preservation of the current target"
        );
        drop(stored);
    }
    let replacement = build_replacement_list(
        USER,
        2,
        &entries,
        vec![entries[1].clone()],
        &before.local_keys.device_id,
        &survivor_signing,
        now,
        None,
    )
    .unwrap();
    *shared_bundle.lock().unwrap() = UserIdentityBundleDto {
        user_id: USER.to_owned(),
        identity_revision: 1,
        identity_incarnation_id: incarnation,
        device_list: UserDeviceListDto {
            body: BASE64.encode(&replacement.body_bytes),
            signature: BASE64.encode(&replacement.signature),
        },
        devices: vec![survivor_dto],
        historical_devices: vec![old_dto],
    };
    let mut after = RoomCryptoContext::from_parts(
        client,
        pins,
        dir.path().join("rosters.json"),
        USER.to_owned(),
        before.local_keys,
    );
    after
        .verify_room(&room)
        .await
        .expect("existing Mission survives original signer revocation");
    let identity = after.verified_identity(USER).await.unwrap();
    roster.issued_at_ms = now - 400;
    let forged_body = serde_json::to_vec(&roster).unwrap();
    let forged = BASE64.encode(
        crypto::sign_control_message(
            old.signing_pkcs8_bytes(),
            &domain_preimage(kodosi_domain::domain_tags::ROOM_ROSTER_V1, &forged_body),
        )
        .unwrap(),
    );
    assert!(
        after
            .verify_historical_signature(
                &identity,
                &old.device_id,
                now - 400,
                kodosi_domain::domain_tags::ROOM_ROSTER_V1,
                &forged_body,
                &forged,
                "test"
            )
            .await
            .is_err(),
        "revoked signer cannot mint a newly backdated artifact"
    );
    server.abort();
}
