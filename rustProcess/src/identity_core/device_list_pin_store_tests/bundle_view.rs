use super::*;

#[test]
fn build_bundle_view_accepts_bootstrap_bundle() {
    let (dto, _kp, _id) = make_bootstrap_bundle(TEST_USER_ID);
    let view = build_bundle_view(&dto).expect("valid bundle");
    assert_eq!(view.user_id, TEST_USER_ID);
    assert_eq!(view.signed_list.generation, 1);
    assert_eq!(view.devices.len(), 1);
}

#[test]
fn historical_certificate_cannot_authorize_artifact_signatures() {
    let (mut dto, first_keypair, first_device_id) = make_bootstrap_bundle(TEST_USER_ID);
    let (second_keypair, second_device_id) =
        add_second_device(&mut dto, &first_device_id, &first_keypair);
    revoke_dev1_keep_dev2(
        &mut dto,
        &first_device_id,
        &second_device_id,
        &second_keypair,
    );

    let view = build_bundle_view(&dto).expect("valid bundle with certificate ancestry");
    assert!(
        view.signing_device_at(&first_device_id, 1_700_000_200_000)
            .is_none(),
        "backend-supplied historical metadata is not signed-list authority"
    );
    assert!(
        view.signing_device_at(&second_device_id, 1_700_000_300_000)
            .is_some()
    );
}

#[test]
fn build_bundle_view_rejects_tampered_cert_pubkey() {
    let (mut dto, _kp, _id) = make_bootstrap_bundle(TEST_USER_ID);
    let attacker = fresh_signing_keypair();
    dto.devices[0].signing_public_key = BASE64.encode(attacker.public_key().as_ref());

    let err = build_bundle_view(&dto).expect_err("should reject");
    std::assert_matches!(err, AppError::Unsupported { ref reason }
        if reason.contains("sig_public_key"));
}

#[test]
fn build_bundle_view_rejects_list_signer_not_in_devices() {
    let (mut dto, _kp, _id) = make_bootstrap_bundle(TEST_USER_ID);
    dto.device_list.signer_device_id = "bogus-device".to_owned();

    let err = build_bundle_view(&dto).expect_err("should reject");
    std::assert_matches!(err, AppError::Unsupported { ref reason }
        if reason.contains("signer"));
}

#[test]
fn build_bundle_view_rejects_cert_lifted_from_another_users_bundle() {
    let (mut dto, _kp, _id) = make_bootstrap_bundle(TEST_USER_ID);
    dto.user_id = "mallory".to_owned();

    let err = build_bundle_view(&dto).expect_err("should reject");
    std::assert_matches!(err, AppError::Unsupported { ref reason }
        if reason.contains("cert user_id"));
}

#[test]
fn build_bundle_view_rejects_duplicate_device_ids() {
    let (mut dto, _kp, _id) = make_bootstrap_bundle(TEST_USER_ID);
    let dup = dto.devices[0].clone();
    dto.devices.push(dup);

    let err = build_bundle_view(&dto).expect_err("should reject");
    std::assert_matches!(err, AppError::Unsupported { ref reason }
        if reason.contains("duplicate"));
}
