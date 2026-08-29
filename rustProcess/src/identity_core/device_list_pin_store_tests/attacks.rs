use super::*;

#[test]
fn expired_identity_bundle_is_rejected_before_pinning() {
    let (_dir, mut store) = make_store();
    let (dto, _, _) = make_bootstrap_bundle(TEST_USER_ID);
    let mut view = build_bundle_view(&dto).unwrap();
    view.signed_list.expires_at_ms = Some(1_699_999_999_999);

    assert_eq!(
        store
            .verify_or_pin(&view, PinContext::ExplicitShare)
            .unwrap(),
        PinVerdict::Reject {
            reason: BreakReason::IdentityExpired
        }
    );
    assert!(store.get(TEST_USER_ID).is_none());
}

#[test]
fn pin_hijack_blocked_via_same_id_different_key() {
    let (_dir, mut store) = make_store();
    let (dto_1, _kp_real, device_id) = make_bootstrap_bundle(TEST_USER_ID);
    let view_1 = build_bundle_view(&dto_1).unwrap();
    store
        .verify_or_pin(&view_1, PinContext::ExplicitShare)
        .unwrap();

    let kp_attacker = fresh_signing_keypair();
    let sig_pub_attacker = kp_attacker.public_key().as_ref().to_vec();
    let kem_pub = fake_kem_pub();
    let issued = 1_700_000_300_000;
    let attacker_cert = build_self_cert(
        TEST_USER_ID,
        &device_id,
        "Alice MacBook",
        &kem_pub,
        &kp_attacker,
        issued,
        None,
    )
    .unwrap();
    let attacker_list = SignedDeviceList {
        user_id: TEST_USER_ID.to_owned(),
        generation: 2,
        entries: vec![DeviceListEntry {
            device_id: device_id.clone(),
            signer_device_id: device_id.clone(),
        }],
        signer_device_id: device_id.clone(),
        issued_at_ms: issued,
        expires_at_ms: None,
    };
    let (list_body, list_sig) = sign_raw_list(attacker_list, &kp_attacker);

    let dto_attack = UserIdentityBundleDto {
        user_id: TEST_USER_ID.to_owned(),
        device_list: UserDeviceListDto {
            generation: 2,
            signer_device_id: device_id.clone(),
            issued_at_ms: issued,
            expires_at_ms: None,
            body: BASE64.encode(&list_body),
            signature: BASE64.encode(&list_sig),
        },
        devices: vec![UserDeviceCertificateDto {
            device_id: device_id.clone(),
            kem_public_key: BASE64.encode(&kem_pub),
            signing_public_key: BASE64.encode(&sig_pub_attacker),
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
            reason: BreakReason::NoChainToPin
        },
        "pin hijack via same-id-different-key MUST be rejected"
    );
    let pin = store.get(TEST_USER_ID).unwrap();
    assert_eq!(pin.proof_summary().expect("pin summary").generation, 1);
    let pinned_certificate = BASE64
        .decode(&pin.device_proofs[&device_id].certificate)
        .unwrap();
    let pinned_certificate =
        crate::identity_core::device_cert::DeviceCertificate::parse_body(&pinned_certificate)
            .unwrap();
    assert_ne!(pinned_certificate.sig_public_key, sig_pub_attacker);
}

#[test]
fn retained_device_key_substitution_on_authentic_update_is_rejected() {
    let (_dir, mut store) = make_store();
    let (mut dto, device_a_keypair, device_a_id) = make_bootstrap_bundle(TEST_USER_ID);
    let (device_b_keypair, device_b_id) =
        add_second_device(&mut dto, &device_a_id, &device_a_keypair);
    let view_2 = build_bundle_view(&dto).unwrap();
    store
        .verify_or_pin(&view_2, PinContext::ExplicitShare)
        .unwrap();

    let attacker_keypair = fresh_signing_keypair();
    let attacker_signing_public_key = attacker_keypair.public_key().as_ref().to_vec();
    let attacker_kem_public_key = fake_kem_pub();
    let issued_at_ms = 1_700_000_200_000;
    let attacker_certificate = build_self_cert(
        TEST_USER_ID,
        &device_a_id,
        "Substituted MacBook",
        &attacker_kem_public_key,
        &attacker_keypair,
        issued_at_ms,
        None,
    )
    .unwrap();

    let entries = vec![
        DeviceListEntry {
            device_id: device_a_id.clone(),
            signer_device_id: device_a_id.clone(),
        },
        DeviceListEntry {
            device_id: device_b_id.clone(),
            signer_device_id: device_a_id.clone(),
        },
    ];
    let list = SignedDeviceList {
        user_id: TEST_USER_ID.to_owned(),
        generation: 3,
        entries,
        signer_device_id: device_b_id.clone(),
        issued_at_ms,
        expires_at_ms: None,
    };
    let (list_body, list_signature) = sign_raw_list(list, &device_b_keypair);

    let device_a = dto
        .devices
        .iter_mut()
        .find(|device| device.device_id == device_a_id)
        .unwrap();
    device_a.kem_public_key = BASE64.encode(&attacker_kem_public_key);
    device_a.signing_public_key = BASE64.encode(&attacker_signing_public_key);
    device_a.certificate = BASE64.encode(&attacker_certificate.body_bytes);
    device_a.certificate_signature = BASE64.encode(&attacker_certificate.signature);

    let device_b = dto
        .devices
        .iter_mut()
        .find(|device| device.device_id == device_b_id)
        .unwrap();
    let replacement_device_b_certificate = crate::identity_core::device_cert::build_cert_for(
        TEST_USER_ID,
        &device_b_id,
        "Test iPad",
        &BASE64.decode(&device_b.kem_public_key).unwrap(),
        &BASE64.decode(&device_b.signing_public_key).unwrap(),
        &device_a_id,
        &attacker_keypair,
        1_700_000_100_000,
        None,
    )
    .unwrap();
    device_b.certificate = BASE64.encode(&replacement_device_b_certificate.body_bytes);
    device_b.certificate_signature = BASE64.encode(&replacement_device_b_certificate.signature);

    dto.device_list = UserDeviceListDto {
        generation: 3,
        signer_device_id: device_b_id,
        issued_at_ms,
        expires_at_ms: None,
        body: BASE64.encode(&list_body),
        signature: BASE64.encode(&list_signature),
    };
    let substituted_view = build_bundle_view(&dto).unwrap();

    assert_eq!(
        store
            .verify_or_pin(&substituted_view, PinContext::BackgroundFetch)
            .unwrap(),
        PinVerdict::Reject {
            reason: BreakReason::KeyMaterialChanged
        }
    );
    let retained = store.get(TEST_USER_ID).unwrap();
    assert_eq!(retained.proof_summary().expect("pin summary").generation, 2);
    let retained_certificate = BASE64
        .decode(&retained.device_proofs[&device_a_id].certificate)
        .unwrap();
    let retained_certificate =
        crate::identity_core::device_cert::DeviceCertificate::parse_body(&retained_certificate)
            .unwrap();
    assert_ne!(
        retained_certificate.sig_public_key,
        attacker_signing_public_key
    );
}

#[test]
fn same_generation_different_bytes_is_rejected() {
    let (_dir, mut store) = make_store();
    let (dto_1, kp, device_id) = make_bootstrap_bundle(TEST_USER_ID);
    let view_1 = build_bundle_view(&dto_1).unwrap();
    store
        .verify_or_pin(&view_1, PinContext::ExplicitShare)
        .unwrap();

    let alt_list = SignedDeviceList {
        user_id: TEST_USER_ID.to_owned(),
        generation: 1,
        entries: vec![DeviceListEntry {
            device_id: device_id.clone(),
            signer_device_id: device_id.clone(),
        }],
        signer_device_id: device_id.clone(),
        issued_at_ms: 1_700_000_050_000,
        expires_at_ms: None,
    };
    let (body_alt, sig_alt) = sign_raw_list(alt_list, &kp);

    let mut dto_alt = dto_1.clone();
    dto_alt.device_list.body = BASE64.encode(&body_alt);
    dto_alt.device_list.signature = BASE64.encode(&sig_alt);
    dto_alt.device_list.issued_at_ms = 1_700_000_050_000;

    let view_alt = build_bundle_view(&dto_alt).unwrap();
    let verdict = store
        .verify_or_pin(&view_alt, PinContext::BackgroundFetch)
        .unwrap();
    assert_eq!(
        verdict,
        PinVerdict::Reject {
            reason: BreakReason::StaleGeneration
        }
    );
}

#[test]
fn certified_device_omitted_from_signed_list_is_rejected() {
    let (mut dto, signer, signer_device_id) = make_bootstrap_bundle(TEST_USER_ID);
    let extra_key = fresh_signing_keypair();
    let extra_id = "alice-extra".to_owned();
    let kem = fake_kem_pub();
    let cert = crate::identity_core::device_cert::build_cert_for(
        TEST_USER_ID,
        &extra_id,
        "Stale device",
        &kem,
        extra_key.public_key().as_ref(),
        &signer_device_id,
        &signer,
        1_700_000_100_000,
        None,
    )
    .unwrap();
    dto.devices.push(UserDeviceCertificateDto {
        device_id: extra_id,
        kem_public_key: BASE64.encode(&kem),
        signing_public_key: BASE64.encode(extra_key.public_key().as_ref()),
        certificate: BASE64.encode(&cert.body_bytes),
        certificate_signature: BASE64.encode(&cert.signature),
    });

    let error = build_bundle_view(&dto).expect_err("extra device must fail closed");
    std::assert_matches!(error, AppError::Unsupported { ref reason }
            if reason.contains("exactly match"));
}

#[test]
fn cyclic_certificate_signer_graph_is_rejected() {
    let user_id = TEST_USER_ID;
    let key_a = fresh_signing_keypair();
    let key_b = fresh_signing_keypair();
    let id_a = "alice-A".to_owned();
    let id_b = "alice-B".to_owned();
    let issued_at_ms = 1_700_000_000_000;
    let kem_a = fake_kem_pub();
    let kem_b = fake_kem_pub();
    let certificate_a = crate::identity_core::device_cert::build_cert_for(
        user_id,
        &id_a,
        "A",
        &kem_a,
        key_a.public_key().as_ref(),
        &id_b,
        &key_b,
        issued_at_ms,
        None,
    )
    .unwrap();
    let certificate_b = crate::identity_core::device_cert::build_cert_for(
        user_id,
        &id_b,
        "B",
        &kem_b,
        key_b.public_key().as_ref(),
        &id_a,
        &key_a,
        issued_at_ms,
        None,
    )
    .unwrap();
    let list = SignedDeviceList {
        user_id: user_id.to_owned(),
        generation: 1,
        entries: vec![
            DeviceListEntry {
                device_id: id_a.clone(),
                signer_device_id: id_b.clone(),
            },
            DeviceListEntry {
                device_id: id_b.clone(),
                signer_device_id: id_a.clone(),
            },
        ],
        signer_device_id: id_a.clone(),
        issued_at_ms,
        expires_at_ms: None,
    };
    let (body, signature) = sign_raw_list(list, &key_a);
    let dto = UserIdentityBundleDto {
        user_id: user_id.to_owned(),
        device_list: UserDeviceListDto {
            generation: 1,
            signer_device_id: id_a.clone(),
            issued_at_ms,
            expires_at_ms: None,
            body: BASE64.encode(body),
            signature: BASE64.encode(signature),
        },
        devices: vec![
            UserDeviceCertificateDto {
                device_id: id_a,
                kem_public_key: BASE64.encode(kem_a),
                signing_public_key: BASE64.encode(key_a.public_key().as_ref()),
                certificate: BASE64.encode(certificate_a.body_bytes),
                certificate_signature: BASE64.encode(certificate_a.signature),
            },
            UserDeviceCertificateDto {
                device_id: id_b,
                kem_public_key: BASE64.encode(kem_b),
                signing_public_key: BASE64.encode(key_b.public_key().as_ref()),
                certificate: BASE64.encode(certificate_b.body_bytes),
                certificate_signature: BASE64.encode(certificate_b.signature),
            },
        ],
        historical_devices: vec![],
    };

    let error = build_bundle_view(&dto).expect_err("cycle must fail closed");
    std::assert_matches!(error, AppError::Unsupported { ref reason }
        if reason.contains("cycle"));
}

#[test]
fn split_brain_cert_and_list_signers_disagree_is_rejected() {
    let user_id = TEST_USER_ID;
    let kp_a = fresh_signing_keypair();
    let kp_b = fresh_signing_keypair();
    let kp_c = fresh_signing_keypair();
    let kem_pub = fake_kem_pub();
    let issued = 1_700_000_000_000;
    let id_a = "alice-A".to_owned();
    let id_b = "alice-B".to_owned();
    let id_c = "alice-C".to_owned();
    let sig_a = kp_a.public_key().as_ref().to_vec();
    let sig_b = kp_b.public_key().as_ref().to_vec();
    let sig_c = kp_c.public_key().as_ref().to_vec();

    let cert_a = build_self_cert(user_id, &id_a, "A", &kem_pub, &kp_a, issued, None).unwrap();

    let cert_c = crate::identity_core::device_cert::build_cert_for(
        user_id, &id_c, "C", &kem_pub, &sig_c, &id_b, &kp_b, issued, None,
    )
    .unwrap();
    let cert_b = crate::identity_core::device_cert::build_cert_for(
        user_id, &id_b, "B", &kem_pub, &sig_b, &id_a, &kp_a, issued, None,
    )
    .unwrap();

    let conflicting_list = SignedDeviceList {
        user_id: user_id.to_owned(),
        generation: 1,
        entries: vec![
            DeviceListEntry {
                device_id: id_a.clone(),
                signer_device_id: id_a.clone(),
            },
            DeviceListEntry {
                device_id: id_b.clone(),
                signer_device_id: id_a.clone(),
            },
            DeviceListEntry {
                device_id: id_c.clone(),
                signer_device_id: id_a.clone(),
            },
        ],
        signer_device_id: id_a.clone(),
        issued_at_ms: issued,
        expires_at_ms: None,
    };
    let (list_body, list_sig) = sign_raw_list(conflicting_list, &kp_a);

    let dto = UserIdentityBundleDto {
        user_id: user_id.to_owned(),
        device_list: UserDeviceListDto {
            generation: 1,
            signer_device_id: id_a.clone(),
            issued_at_ms: issued,
            expires_at_ms: None,
            body: BASE64.encode(&list_body),
            signature: BASE64.encode(&list_sig),
        },
        devices: vec![
            UserDeviceCertificateDto {
                device_id: id_a.clone(),
                kem_public_key: BASE64.encode(&kem_pub),
                signing_public_key: BASE64.encode(&sig_a),
                certificate: BASE64.encode(&cert_a.body_bytes),
                certificate_signature: BASE64.encode(&cert_a.signature),
            },
            UserDeviceCertificateDto {
                device_id: id_b.clone(),
                kem_public_key: BASE64.encode(&kem_pub),
                signing_public_key: BASE64.encode(&sig_b),
                certificate: BASE64.encode(&cert_b.body_bytes),
                certificate_signature: BASE64.encode(&cert_b.signature),
            },
            UserDeviceCertificateDto {
                device_id: id_c.clone(),
                kem_public_key: BASE64.encode(&kem_pub),
                signing_public_key: BASE64.encode(&sig_c),
                certificate: BASE64.encode(&cert_c.body_bytes),
                certificate_signature: BASE64.encode(&cert_c.signature),
            },
        ],
        historical_devices: vec![],
    };

    let err = build_bundle_view(&dto).expect_err("split-brain should be rejected");
    std::assert_matches!(err, AppError::Unsupported { ref reason }
        if reason.contains("disagrees"),
        "expected cert/list signer disagreement error, got {err:?}"
    );
}
