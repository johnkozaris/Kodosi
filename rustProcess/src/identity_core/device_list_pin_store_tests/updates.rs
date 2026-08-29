use super::*;

#[test]
fn chain_accepts_update_signed_by_pinned_device() {
    let (_dir, mut store) = make_store();
    let (mut dto, kp, device_id) = make_bootstrap_bundle(TEST_USER_ID);
    let view_1 = build_bundle_view(&dto).unwrap();
    store
        .verify_or_pin(&view_1, PinContext::ExplicitShare)
        .unwrap();

    add_second_device(&mut dto, &device_id, &kp);
    let view_2 = build_bundle_view(&dto).unwrap();

    let verdict = store
        .verify_or_pin(&view_2, PinContext::BackgroundFetch)
        .unwrap();
    assert_eq!(verdict, PinVerdict::AcceptUpdate);
    let pin = store.get(TEST_USER_ID).unwrap();
    let summary = pin.proof_summary().expect("pin summary");
    assert_eq!(summary.generation, 2);
    assert_eq!(summary.active_device_ids.len(), 2);
}

#[test]
fn historical_ancestry_round_trips_through_persisted_pin() {
    let (_dir, mut store) = make_store();
    let (mut dto, device_a_keypair, device_a_id) = make_bootstrap_bundle(TEST_USER_ID);
    let (device_b_keypair, device_b_id) =
        add_second_device(&mut dto, &device_a_id, &device_a_keypair);
    let view_2 = build_bundle_view(&dto).unwrap();
    store
        .verify_or_pin(&view_2, PinContext::ExplicitShare)
        .unwrap();

    revoke_dev1_keep_dev2(&mut dto, &device_a_id, &device_b_id, &device_b_keypair);
    let view_3 = build_bundle_view(&dto).unwrap();
    assert_eq!(view_3.historical_devices.len(), 1);
    assert_eq!(
        store
            .verify_or_pin(&view_3, PinContext::BackgroundFetch)
            .unwrap(),
        PinVerdict::AcceptUpdate
    );

    let reconstructed = store
        .identity_bundle(TEST_USER_ID)
        .unwrap()
        .expect("persisted identity proof");
    assert_eq!(reconstructed.historical_devices.len(), 1);
    let rebuilt = crate::identity_core::identity_bundle_view::build_bundle_view(&reconstructed)
        .expect("persisted proof remains self-contained");
    assert_eq!(rebuilt.signed_list.generation, 3);
    assert_eq!(rebuilt.historical_devices.len(), 1);
    assert!(rebuilt.devices.contains_key(&device_b_id));
}

#[test]
fn update_persist_failure_keeps_previous_pin() {
    let (_dir, mut store) = make_store();
    let (mut dto, kp, device_id) = make_bootstrap_bundle(TEST_USER_ID);
    let view_1 = build_bundle_view(&dto).unwrap();
    store
        .verify_or_pin(&view_1, PinContext::ExplicitShare)
        .unwrap();
    let blocked_dir = tempfile::tempdir().unwrap();
    let blocked_parent = blocked_dir.path().join("not-a-directory");
    std::fs::write(&blocked_parent, "not a directory").unwrap();
    store.path = blocked_parent.join("device-list-pins.json");

    add_second_device(&mut dto, &device_id, &kp);
    let view_2 = build_bundle_view(&dto).unwrap();
    let result = store.verify_or_pin(&view_2, PinContext::BackgroundFetch);

    assert!(result.is_err());
    let pin = store.get(TEST_USER_ID).unwrap();
    let summary = pin.proof_summary().expect("pin summary");
    assert_eq!(summary.generation, 1);
    assert_eq!(summary.active_device_ids.len(), 1);
}

#[test]
fn chain_rejects_update_signed_by_unknown_device() {
    let (_dir, mut store) = make_store();
    let (dto_1, _kp_1, _id_1) = make_bootstrap_bundle(TEST_USER_ID);
    let view_1 = build_bundle_view(&dto_1).unwrap();
    store
        .verify_or_pin(&view_1, PinContext::ExplicitShare)
        .unwrap();

    let kp_fresh = fresh_signing_keypair();
    let kem_pub_fresh = fake_kem_pub();
    let sig_pub_fresh = kp_fresh.public_key().as_ref().to_vec();
    let id_fresh = "alice-dev-FRESH".to_owned();
    let issued = 1_700_000_200_000;
    let cert_fresh = build_self_cert(
        TEST_USER_ID,
        &id_fresh,
        "Fresh",
        &kem_pub_fresh,
        &kp_fresh,
        issued,
        None,
    )
    .unwrap();
    let fresh_list = SignedDeviceList {
        user_id: TEST_USER_ID.to_owned(),
        generation: 2,
        entries: vec![DeviceListEntry {
            device_id: id_fresh.clone(),
            signer_device_id: id_fresh.clone(),
        }],
        signer_device_id: id_fresh.clone(),
        issued_at_ms: issued,
        expires_at_ms: None,
    };
    let (list_body, list_sig) = sign_raw_list(fresh_list, &kp_fresh);

    let dto_2 = UserIdentityBundleDto {
        user_id: TEST_USER_ID.to_owned(),
        device_list: UserDeviceListDto {
            generation: 2,
            signer_device_id: id_fresh.clone(),
            issued_at_ms: issued,
            expires_at_ms: None,
            body: BASE64.encode(&list_body),
            signature: BASE64.encode(&list_sig),
        },
        devices: vec![UserDeviceCertificateDto {
            device_id: id_fresh.clone(),
            kem_public_key: BASE64.encode(&kem_pub_fresh),
            signing_public_key: BASE64.encode(&sig_pub_fresh),
            certificate: BASE64.encode(&cert_fresh.body_bytes),
            certificate_signature: BASE64.encode(&cert_fresh.signature),
        }],
        historical_devices: vec![],
    };

    let view_2 = build_bundle_view(&dto_2).unwrap();
    let verdict = store
        .verify_or_pin(&view_2, PinContext::BackgroundFetch)
        .unwrap();
    assert_eq!(
        verdict,
        PinVerdict::Reject {
            reason: BreakReason::NoChainToPin
        }
    );
    assert_eq!(
        store
            .get(TEST_USER_ID)
            .unwrap()
            .proof_summary()
            .expect("pin summary")
            .generation,
        1
    );
}

#[test]
fn stale_generation_is_rejected() {
    let (_dir, mut store) = make_store();
    let (mut dto, kp, device_id) = make_bootstrap_bundle(TEST_USER_ID);
    add_second_device(&mut dto, &device_id, &kp);
    let view_gen2 = build_bundle_view(&dto).unwrap();
    store
        .verify_or_pin(&view_gen2, PinContext::ExplicitShare)
        .unwrap();

    let issued = 1_700_000_050_000;
    let bootstrap = build_bootstrap_list(TEST_USER_ID, &device_id, &kp, issued, None).unwrap();
    let cert = build_self_cert(
        TEST_USER_ID,
        &device_id,
        "Test MacBook",
        &fake_kem_pub(),
        &kp,
        issued,
        None,
    )
    .unwrap();
    let dto_stale = UserIdentityBundleDto {
        user_id: TEST_USER_ID.to_owned(),
        device_list: UserDeviceListDto {
            generation: 1,
            signer_device_id: device_id.clone(),
            issued_at_ms: issued,
            expires_at_ms: None,
            body: BASE64.encode(&bootstrap.body_bytes),
            signature: BASE64.encode(&bootstrap.signature),
        },
        devices: vec![UserDeviceCertificateDto {
            device_id: device_id.clone(),
            kem_public_key: BASE64.encode(fake_kem_pub()),
            signing_public_key: BASE64.encode(kp.public_key().as_ref()),
            certificate: BASE64.encode(&cert.body_bytes),
            certificate_signature: BASE64.encode(&cert.signature),
        }],
        historical_devices: vec![],
    };
    let view_stale = build_bundle_view(&dto_stale).unwrap();

    let verdict = store
        .verify_or_pin(&view_stale, PinContext::BackgroundFetch)
        .unwrap();
    assert_eq!(
        verdict,
        PinVerdict::Reject {
            reason: BreakReason::StaleGeneration
        }
    );
}

#[test]
fn already_pinned_refetch_is_noop() {
    let (_dir, mut store) = make_store();
    let (dto, _kp, _id) = make_bootstrap_bundle(TEST_USER_ID);
    let view = build_bundle_view(&dto).unwrap();
    store
        .verify_or_pin(&view, PinContext::ExplicitShare)
        .unwrap();

    let verdict = store
        .verify_or_pin(&view, PinContext::BackgroundFetch)
        .unwrap();
    assert_eq!(verdict, PinVerdict::AlreadyPinned);
}
