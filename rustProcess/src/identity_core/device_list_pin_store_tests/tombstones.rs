use super::*;

#[test]
fn revocation_tombstones_dropped_devices() {
    let (_dir, mut store) = make_store();
    let (mut dto, kp1, dev1) = make_bootstrap_bundle(TEST_USER_ID);
    let view_1 = build_bundle_view(&dto).unwrap();
    store
        .verify_or_pin(&view_1, PinContext::ExplicitShare)
        .unwrap();

    let (kp2, dev2) = add_second_device(&mut dto, &dev1, &kp1);
    let view_2 = build_bundle_view(&dto).unwrap();
    store
        .verify_or_pin(&view_2, PinContext::BackgroundFetch)
        .unwrap();
    assert_eq!(store.get(TEST_USER_ID).unwrap().revoked_device_ids.len(), 0);

    revoke_dev1_keep_dev2(&mut dto, &dev1, &dev2, &kp2);
    let view_revoke = build_bundle_view(&dto).unwrap();
    let verdict = store
        .verify_or_pin(&view_revoke, PinContext::BackgroundFetch)
        .unwrap();
    assert_eq!(verdict, PinVerdict::AcceptUpdate);

    let pin = store.get(TEST_USER_ID).unwrap();
    assert!(
        pin.revoked_device_ids.contains(&dev1),
        "dev-1 should be tombstoned after revocation"
    );
    assert_eq!(pin.proof_summary().expect("pin summary").generation, 3);
}

#[test]
fn update_signed_by_tombstoned_device_is_rejected() {
    let (_dir, mut store) = make_store();
    let (mut dto, kp1, dev1) = make_bootstrap_bundle(TEST_USER_ID);
    store
        .verify_or_pin(&build_bundle_view(&dto).unwrap(), PinContext::ExplicitShare)
        .unwrap();
    let (kp2, dev2) = add_second_device(&mut dto, &dev1, &kp1);
    store
        .verify_or_pin(
            &build_bundle_view(&dto).unwrap(),
            PinContext::BackgroundFetch,
        )
        .unwrap();
    revoke_dev1_keep_dev2(&mut dto, &dev1, &dev2, &kp2);
    store
        .verify_or_pin(
            &build_bundle_view(&dto).unwrap(),
            PinContext::BackgroundFetch,
        )
        .unwrap();

    let issued = 1_700_000_400_000;
    let attacker_cert = build_self_cert(
        TEST_USER_ID,
        &dev1,
        "Pwn",
        &fake_kem_pub(),
        &kp1,
        issued,
        None,
    )
    .unwrap();
    let attacker_list = SignedDeviceList {
        user_id: TEST_USER_ID.to_owned(),
        generation: 4,
        entries: vec![DeviceListEntry {
            device_id: dev1.clone(),
            signer_device_id: dev1.clone(),
        }],
        signer_device_id: dev1.clone(),
        issued_at_ms: issued,
        expires_at_ms: None,
    };
    let (body, sig) = sign_raw_list(attacker_list, &kp1);
    let dto_attack = UserIdentityBundleDto {
        user_id: TEST_USER_ID.to_owned(),
        device_list: UserDeviceListDto {
            generation: 4,
            signer_device_id: dev1.clone(),
            issued_at_ms: issued,
            expires_at_ms: None,
            body: BASE64.encode(&body),
            signature: BASE64.encode(&sig),
        },
        devices: vec![UserDeviceCertificateDto {
            device_id: dev1.clone(),
            kem_public_key: BASE64.encode(fake_kem_pub()),
            signing_public_key: BASE64.encode(kp1.public_key().as_ref()),
            certificate: BASE64.encode(&attacker_cert.body_bytes),
            certificate_signature: BASE64.encode(&attacker_cert.signature),
        }],
        historical_devices: vec![],
    };
    let view_attack = build_bundle_view(&dto_attack).unwrap();

    let verdict = store
        .verify_or_pin(&view_attack, PinContext::BackgroundFetch)
        .unwrap();
    assert_eq!(
        verdict,
        PinVerdict::Reject {
            reason: BreakReason::SignerRevoked
        }
    );
    assert_eq!(
        store
            .get(TEST_USER_ID)
            .unwrap()
            .proof_summary()
            .expect("pin summary")
            .generation,
        3
    );
}

#[test]
fn phantom_readd_of_revoked_device_is_rejected() {
    let (_dir, mut store) = make_store();
    let (mut dto, kp1, dev1) = make_bootstrap_bundle(TEST_USER_ID);
    store
        .verify_or_pin(&build_bundle_view(&dto).unwrap(), PinContext::ExplicitShare)
        .unwrap();
    let (kp2, dev2) = add_second_device(&mut dto, &dev1, &kp1);
    store
        .verify_or_pin(
            &build_bundle_view(&dto).unwrap(),
            PinContext::BackgroundFetch,
        )
        .unwrap();
    revoke_dev1_keep_dev2(&mut dto, &dev1, &dev2, &kp2);
    store
        .verify_or_pin(
            &build_bundle_view(&dto).unwrap(),
            PinContext::BackgroundFetch,
        )
        .unwrap();

    let issued = 1_700_000_500_000;
    let readd_list = SignedDeviceList {
        user_id: TEST_USER_ID.to_owned(),
        generation: 4,
        entries: vec![
            DeviceListEntry {
                device_id: dev2.clone(),
                signer_device_id: dev1.clone(),
            },
            DeviceListEntry {
                device_id: dev1.clone(),
                signer_device_id: dev1.clone(),
            },
        ],
        signer_device_id: dev2.clone(),
        issued_at_ms: issued,
        expires_at_ms: None,
    };
    let (body, sig) = sign_raw_list(readd_list, &kp2);
    let new_cert_for_dev1 = build_self_cert(
        TEST_USER_ID,
        &dev1,
        "Resurrected",
        &fake_kem_pub(),
        &kp1,
        issued,
        None,
    )
    .unwrap();
    let dto_readd = UserIdentityBundleDto {
        user_id: TEST_USER_ID.to_owned(),
        device_list: UserDeviceListDto {
            generation: 4,
            signer_device_id: dev2.clone(),
            issued_at_ms: issued,
            expires_at_ms: None,
            body: BASE64.encode(&body),
            signature: BASE64.encode(&sig),
        },
        devices: vec![
            dto.devices[0].clone(),
            UserDeviceCertificateDto {
                device_id: dev1.clone(),
                kem_public_key: BASE64.encode(fake_kem_pub()),
                signing_public_key: BASE64.encode(kp1.public_key().as_ref()),
                certificate: BASE64.encode(&new_cert_for_dev1.body_bytes),
                certificate_signature: BASE64.encode(&new_cert_for_dev1.signature),
            },
        ],
        historical_devices: vec![],
    };

    let view_readd = build_bundle_view(&dto_readd).unwrap();
    let verdict = store
        .verify_or_pin(&view_readd, PinContext::BackgroundFetch)
        .unwrap();
    assert_eq!(
        verdict,
        PinVerdict::Reject {
            reason: BreakReason::RevokedDeviceReappeared
        }
    );
}

#[test]
fn tombstones_persist_across_reload() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device-list-pins.json");
    let (dev1, dev2) = {
        let mut store =
            DeviceListPinStore::load_from_with_clock(&path, || 1_700_000_000_000).unwrap();
        store.bind_to_user("test-owner").unwrap();
        let (mut dto, kp1, dev1) = make_bootstrap_bundle(TEST_USER_ID);
        store
            .verify_or_pin(&build_bundle_view(&dto).unwrap(), PinContext::ExplicitShare)
            .unwrap();
        let (kp2, dev2) = add_second_device(&mut dto, &dev1, &kp1);
        store
            .verify_or_pin(
                &build_bundle_view(&dto).unwrap(),
                PinContext::BackgroundFetch,
            )
            .unwrap();
        revoke_dev1_keep_dev2(&mut dto, &dev1, &dev2, &kp2);
        store
            .verify_or_pin(
                &build_bundle_view(&dto).unwrap(),
                PinContext::BackgroundFetch,
            )
            .unwrap();
        (dev1, dev2)
    };

    let reloaded = DeviceListPinStore::load_from_with_clock(&path, || 1_700_000_000_000).unwrap();
    let pin = reloaded.get(TEST_USER_ID).unwrap();
    assert!(pin.revoked_device_ids.contains(&dev1));
    assert!(!pin.revoked_device_ids.contains(&dev2));
}

#[test]
fn trust_reset_clears_tombstones_too() {
    let (_dir, mut store) = make_store();
    let (mut dto, kp1, dev1) = make_bootstrap_bundle(TEST_USER_ID);
    store
        .verify_or_pin(&build_bundle_view(&dto).unwrap(), PinContext::ExplicitShare)
        .unwrap();
    let (kp2, dev2) = add_second_device(&mut dto, &dev1, &kp1);
    store
        .verify_or_pin(
            &build_bundle_view(&dto).unwrap(),
            PinContext::BackgroundFetch,
        )
        .unwrap();
    revoke_dev1_keep_dev2(&mut dto, &dev1, &dev2, &kp2);
    store
        .verify_or_pin(
            &build_bundle_view(&dto).unwrap(),
            PinContext::BackgroundFetch,
        )
        .unwrap();
    assert!(
        store
            .get(TEST_USER_ID)
            .unwrap()
            .revoked_device_ids
            .contains(&dev1)
    );

    assert!(store.reset(TEST_USER_ID).unwrap());
    assert!(store.get(TEST_USER_ID).is_none());
}
