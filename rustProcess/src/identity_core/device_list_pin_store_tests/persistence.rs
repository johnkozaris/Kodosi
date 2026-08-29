use super::*;

#[test]
fn concurrent_process_updates_reload_and_merge_under_lock() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device-list-pins.json");
    let mut store = DeviceListPinStore::load_from_with_clock(&path, || 1_700_000_000_000).unwrap();
    store.bind_to_user("test-owner").unwrap();
    for user in [TEST_USER_ID, SECOND_TEST_USER_ID] {
        let (dto, _, _) = make_bootstrap_bundle(user);
        store
            .verify_or_pin(&build_bundle_view(&dto).unwrap(), PinContext::ExplicitShare)
            .unwrap();
    }
    drop(store);

    let executable = std::env::current_exe().unwrap();
    let spawn_worker = |user: &str| {
        std::process::Command::new(&executable)
            .args([
                "--exact",
                "identity_core::device_list_pin_store::tests::persistence::pin_store_process_worker",
                "--nocapture",
            ])
            .env("KODOSI_PIN_STORE_WORKER_PATH", &path)
            .env("KODOSI_PIN_STORE_WORKER_USER", user)
            .spawn()
            .unwrap()
    };
    let mut alice = spawn_worker(TEST_USER_ID);
    let mut bob = spawn_worker(SECOND_TEST_USER_ID);
    assert!(alice.wait().unwrap().success());
    assert!(bob.wait().unwrap().success());

    let payload = std::fs::read_to_string(&path).unwrap();
    let file: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(file["revision"].as_u64(), Some(7));
    let reloaded = DeviceListPinStore::load_from_with_clock(path, || 1_700_000_000_000).unwrap();
    assert!(reloaded.get(TEST_USER_ID).is_none());
    assert!(reloaded.get(SECOND_TEST_USER_ID).is_none());
    assert!(reloaded.identity_bundle(TEST_USER_ID).unwrap().is_none());
    assert!(
        reloaded
            .pinned_signing_pubkey(TEST_USER_ID, "any-device")
            .unwrap()
            .is_none()
    );
}

#[test]
fn pin_store_process_worker() {
    let Ok(path) = std::env::var("KODOSI_PIN_STORE_WORKER_PATH") else {
        return;
    };
    let user = std::env::var("KODOSI_PIN_STORE_WORKER_USER").unwrap();
    let mut store = DeviceListPinStore::load_from_with_clock(path, || 1_700_000_000_000).unwrap();
    store.bind_to_user("test-owner").unwrap();
    store.reset(&user).map(drop).unwrap();
}

#[test]
fn concurrent_process_v1_migration_and_bind_preserve_migrated_bucket() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device-list-pins.json");
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "revision": 3,
            "owner_user_id": "account-a",
            "pins": { "01900000-0000-7000-8000-000000000011": sample_pin("01900000-0000-7000-8000-000000000011") }
        }))
        .unwrap(),
    )
    .unwrap();

    let executable = std::env::current_exe().unwrap();
    let spawn_worker = |account: &str| {
        std::process::Command::new(&executable)
            .args([
                "--exact",
                "identity_core::device_list_pin_store::tests::persistence::pin_store_bind_process_worker",
                "--nocapture",
            ])
            .env("KODOSI_PIN_STORE_BIND_WORKER_PATH", &path)
            .env("KODOSI_PIN_STORE_BIND_WORKER_ACCOUNT", account)
            .spawn()
            .unwrap()
    };
    let mut account_a = spawn_worker("account-a");
    let mut account_b = spawn_worker("account-b");
    assert!(account_a.wait().unwrap().success());
    assert!(account_b.wait().unwrap().success());

    let mut reloaded =
        DeviceListPinStore::load_from_with_clock(path, || 1_700_000_000_000).unwrap();
    reloaded.bind_to_user("account-a").unwrap();
    assert!(
        reloaded
            .get("01900000-0000-7000-8000-000000000011")
            .is_some()
    );
    reloaded.bind_to_user("account-b").unwrap();
    assert!(
        reloaded
            .get("01900000-0000-7000-8000-000000000011")
            .is_none()
    );
}

#[test]
fn pin_store_bind_process_worker() {
    let Ok(path) = std::env::var("KODOSI_PIN_STORE_BIND_WORKER_PATH") else {
        return;
    };
    let account = std::env::var("KODOSI_PIN_STORE_BIND_WORKER_ACCOUNT").unwrap();
    let mut store = DeviceListPinStore::load_from_with_clock(path, || 1_700_000_000_000).unwrap();
    store.bind_to_user(&account).unwrap();
}

#[test]
fn reset_clears_pin() {
    let (_dir, mut store) = make_store();
    let (dto, _kp, _id) = make_bootstrap_bundle(TEST_USER_ID);
    let view = build_bundle_view(&dto).unwrap();
    store
        .verify_or_pin(&view, PinContext::ExplicitShare)
        .unwrap();
    assert!(store.get(TEST_USER_ID).is_some());

    assert!(store.reset(TEST_USER_ID).unwrap());
    assert!(store.get(TEST_USER_ID).is_none());
}

#[test]
fn reset_persist_failure_keeps_existing_pin() {
    let (_dir, mut store) = make_store();
    let (dto, _kp, _id) = make_bootstrap_bundle(TEST_USER_ID);
    store
        .verify_or_pin(&build_bundle_view(&dto).unwrap(), PinContext::ExplicitShare)
        .unwrap();
    let blocked_dir = tempfile::tempdir().unwrap();
    let blocked_parent = blocked_dir.path().join("not-a-directory");
    std::fs::write(&blocked_parent, "not a directory").unwrap();
    store.path = blocked_parent.join("device-list-pins.json");

    let result = store.reset(TEST_USER_ID);

    assert!(result.is_err());
    assert!(store.get(TEST_USER_ID).is_some());
}

#[test]
fn pins_persist_across_reload() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device-list-pins.json");
    {
        let mut store =
            DeviceListPinStore::load_from_with_clock(&path, || 1_700_000_000_000).unwrap();
        store.bind_to_user("test-owner").unwrap();
        let (dto, _kp, _id) = make_bootstrap_bundle(TEST_USER_ID);
        let view = build_bundle_view(&dto).unwrap();
        store
            .verify_or_pin(&view, PinContext::ExplicitShare)
            .unwrap();
    }
    let reloaded = DeviceListPinStore::load_from_with_clock(&path, || 1_700_000_000_000).unwrap();
    assert!(reloaded.get(TEST_USER_ID).is_some());
    assert_eq!(
        reloaded
            .get(TEST_USER_ID)
            .unwrap()
            .proof_summary()
            .expect("pin summary")
            .generation,
        1
    );
}

#[test]
fn reset_on_unknown_user_is_noop() {
    let (_dir, mut store) = make_store();
    assert!(!store.reset("not-pinned").unwrap());
}
#[test]
fn corrupt_pin_file_fails_load_with_clear_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device-list-pins.json");
    fs::write(&path, "{ not valid json").unwrap();

    let err = DeviceListPinStore::load_from_with_clock(&path, || 0)
        .expect_err("corrupt file should fail load");
    std::assert_matches!(err, AppError::Json(_));
}

#[test]
fn unknown_pin_file_version_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device-list-pins.json");
    fs::write(&path, r#"{"version":999,"pins":{}}"#).unwrap();

    let err = DeviceListPinStore::load_from_with_clock(&path, || 0)
        .expect_err("unknown version should fail load");
    std::assert_matches!(err, AppError::InvalidBackendData { ref field, .. }
        if field == "pin_file.version");
}
fn sample_pin(user_id: &str) -> DeviceListPin {
    let (bundle, _, _) = make_bootstrap_bundle(user_id);
    DeviceListPin::from_view(
        &build_bundle_view(&bundle).expect("sample bundle"),
        1_700_000_000_000,
        std::collections::BTreeSet::new(),
    )
}

#[test]
fn v2_migration_preserves_pins_and_starts_without_lifecycle_watermarks() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device-list-pins.json");
    let pin = sample_pin("01900000-0000-7000-8000-000000000011");
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "version": 2,
            "revision": 9,
            "last_bound_owner_user_id": "account-a",
            "accounts": {
                "account-a": { "pins": { "01900000-0000-7000-8000-000000000011": pin } }
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let store = DeviceListPinStore::load_from_with_clock(&path, || 1_700_000_000_000).unwrap();

    assert!(store.get("01900000-0000-7000-8000-000000000011").is_some());
    let migrated: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(migrated["version"], 4);
    assert_eq!(migrated["revision"], 10);
    assert!(
        migrated["accounts"]["account-a"]["identity_lifecycles"]
            .as_object()
            .expect("lifecycle map")
            .is_empty()
    );
}

#[test]
fn v3_migration_drops_redundant_pin_projections() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device-list-pins.json");
    let pin = sample_pin("01900000-0000-7000-8000-000000000011");
    let summary = pin.proof_summary().expect("pin summary");
    let mut legacy_pin = serde_json::to_value(&pin).unwrap();
    let pin_object = legacy_pin.as_object_mut().expect("pin object");
    pin_object.insert(
        "generation".to_owned(),
        serde_json::json!(summary.generation),
    );
    pin_object.insert(
        "signer_device_id".to_owned(),
        serde_json::json!(summary.signer_device_id),
    );
    pin_object.insert(
        "device_sig_pubkeys_b64".to_owned(),
        serde_json::json!({ "legacy-device": "ignored" }),
    );
    for proof in pin_object["device_proofs"]
        .as_object_mut()
        .expect("proof map")
        .values_mut()
    {
        let proof = proof.as_object_mut().expect("proof object");
        proof.insert("kemPublicKey".to_owned(), serde_json::json!("ignored"));
        proof.insert("signingPublicKey".to_owned(), serde_json::json!("ignored"));
    }
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "version": 3,
            "revision": 11,
            "last_bound_owner_user_id": "account-a",
            "accounts": {
                "account-a": { "pins": { "01900000-0000-7000-8000-000000000011": legacy_pin } }
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let store = DeviceListPinStore::load_from_with_clock(&path, || 1_700_000_000_000).unwrap();
    let reconstructed = store
        .get("01900000-0000-7000-8000-000000000011")
        .expect("migrated pin")
        .identity_bundle_input()
        .expect("migrated proof bundle")
        .expect("migrated proof bundle exists");
    crate::identity_core::identity_bundle_view::build_bundle_view(&reconstructed)
        .expect("migrated exact proofs verify");

    let migrated: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(migrated["version"], 4);
    assert_eq!(migrated["revision"], 12);
    let migrated_pin =
        &migrated["accounts"]["account-a"]["pins"]["01900000-0000-7000-8000-000000000011"];
    for removed in ["generation", "signer_device_id", "device_sig_pubkeys_b64"] {
        assert!(migrated_pin.get(removed).is_none(), "retained {removed}");
    }
    for proof in migrated_pin["device_proofs"]
        .as_object()
        .expect("proof map")
        .values()
    {
        assert!(proof.get("kemPublicKey").is_none());
        assert!(proof.get("signingPublicKey").is_none());
    }
}

#[test]
fn owner_tagged_v1_migrates_atomically_and_a_b_a_preserves_buckets() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device-list-pins.json");
    let alice_pin = sample_pin("01900000-0000-7000-8000-000000000011");
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "revision": 7,
            "owner_user_id": "account-a",
            "pins": { "01900000-0000-7000-8000-000000000011": alice_pin }
        }))
        .unwrap(),
    )
    .unwrap();

    let mut store = DeviceListPinStore::load_from_with_clock(&path, || 1_700_000_000_000).unwrap();
    assert!(store.get("01900000-0000-7000-8000-000000000011").is_some());
    let migrated: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).expect("migrated JSON");
    assert_eq!(migrated["version"], 4);
    assert_eq!(migrated["revision"], 8);
    assert!(
        migrated["accounts"]["account-a"]["pins"]["01900000-0000-7000-8000-000000000011"]
            .is_object()
    );

    assert!(store.bind_to_user("account-b").unwrap());
    assert!(store.get("01900000-0000-7000-8000-000000000011").is_none());
    let (bob_bundle, _, _) = make_bootstrap_bundle("01900000-0000-7000-8000-000000000013");
    store
        .verify_or_pin(
            &build_bundle_view(&bob_bundle).unwrap(),
            PinContext::ExplicitShare,
        )
        .unwrap();
    assert!(store.get("01900000-0000-7000-8000-000000000013").is_some());

    assert!(store.bind_to_user("account-a").unwrap());
    assert!(store.get("01900000-0000-7000-8000-000000000011").is_some());
    assert!(store.get("01900000-0000-7000-8000-000000000013").is_none());
    assert!(store.bind_to_user("account-b").unwrap());
    assert!(store.get("01900000-0000-7000-8000-000000000013").is_some());
}

#[test]
fn ownerless_v1_pins_are_discarded_and_never_assigned_on_bind() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device-list-pins.json");
    let legacy_pin = sample_pin("01900000-0000-7000-8000-000000000012");
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "owner_user_id": null,
            "pins": { "01900000-0000-7000-8000-000000000012": legacy_pin }
        }))
        .unwrap(),
    )
    .unwrap();

    let mut store = DeviceListPinStore::load_from_with_clock(&path, || 1_700_000_000_000).unwrap();
    assert!(store.get("01900000-0000-7000-8000-000000000012").is_none());
    let (new_bundle, _, _) = make_bootstrap_bundle("01900000-0000-7000-8000-000000000014");
    assert!(
        store
            .verify_or_pin(
                &build_bundle_view(&new_bundle).unwrap(),
                PinContext::ExplicitShare,
            )
            .is_err()
    );

    store.bind_to_user("account-b").unwrap();
    assert!(store.get("01900000-0000-7000-8000-000000000012").is_none());
    store.bind_to_user("account-a").unwrap();
    assert!(store.get("01900000-0000-7000-8000-000000000012").is_none());

    let file: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert!(file.get("quarantined_v1_pins").is_none());
    assert!(
        file["accounts"]["account-a"]["pins"]
            .as_object()
            .unwrap()
            .is_empty()
    );
    assert!(
        file["accounts"]["account-b"]["pins"]
            .as_object()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn reset_target_and_reset_all_only_modify_active_bucket() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device-list-pins.json");
    let mut store = DeviceListPinStore::load_from_with_clock(&path, || 1_700_000_000_000).unwrap();
    let peers = [
        "01900000-0000-7000-8000-000000000021",
        "01900000-0000-7000-8000-000000000022",
    ];
    for account in ["account-a", "account-b"] {
        store.bind_to_user(account).unwrap();
        for peer in peers {
            let (bundle, _, _) = make_bootstrap_bundle(peer);
            store
                .verify_or_pin(
                    &build_bundle_view(&bundle).unwrap(),
                    PinContext::ExplicitShare,
                )
                .unwrap();
        }
    }

    store.bind_to_user("account-b").unwrap();
    assert!(store.reset(peers[0]).unwrap());
    assert!(store.get(peers[0]).is_none());
    assert!(store.get(peers[1]).is_some());
    store.reset_all().unwrap();
    assert!(store.get(peers[1]).is_none());

    store.bind_to_user("account-a").unwrap();
    assert!(store.get(peers[0]).is_some());
    assert!(store.get(peers[1]).is_some());
}

#[test]
fn reset_all_wipes_every_pin_and_persists() {
    let (dir, mut store) = make_store();
    let (mut alice_dto, alice_kp1, alice_dev1) = make_bootstrap_bundle(TEST_USER_ID);
    let (mut bob_dto, bob_kp1, bob_dev1) = make_bootstrap_bundle(SECOND_TEST_USER_ID);
    store
        .verify_or_pin(
            &build_bundle_view(&alice_dto).unwrap(),
            PinContext::ExplicitShare,
        )
        .unwrap();
    store
        .verify_or_pin(
            &build_bundle_view(&bob_dto).unwrap(),
            PinContext::ExplicitShare,
        )
        .unwrap();
    let (_alice_kp2, _alice_dev2) = add_second_device(&mut alice_dto, &alice_dev1, &alice_kp1);
    let (_bob_kp2, _bob_dev2) = add_second_device(&mut bob_dto, &bob_dev1, &bob_kp1);
    assert!(store.get(TEST_USER_ID).is_some());
    assert!(store.get(SECOND_TEST_USER_ID).is_some());

    store.reset_all().unwrap();
    assert!(store.get(TEST_USER_ID).is_none());
    assert!(store.get(SECOND_TEST_USER_ID).is_none());

    let path = dir.path().join("device-list-pins.json");
    let mut reloaded =
        DeviceListPinStore::load_from_with_clock(path, || 1_700_000_000_000).unwrap();
    reloaded.bind_to_user("test-owner").unwrap();
    assert!(reloaded.get(TEST_USER_ID).is_none());
    assert!(reloaded.get("bob").is_none());

    store.reset_all().unwrap();
}
