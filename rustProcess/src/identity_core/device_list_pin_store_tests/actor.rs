use super::*;

#[tokio::test]
async fn conditional_reset_only_removes_the_exact_identity_receipt() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device-list-pins.json");
    let mut store =
        DeviceListPinStore::load_from_with_clock(path.clone(), || 1_700_000_000_000).unwrap();
    store.bind_to_user("test-owner").unwrap();
    let handle = DeviceListPinStoreHandle::from_store(store).unwrap();
    let (alice, _, _) = make_bootstrap_bundle(TEST_USER_ID);
    let accepted_view = build_bundle_view(&alice).unwrap();
    handle
        .verify_or_pin(accepted_view.clone(), PinContext::ExplicitShare)
        .await
        .unwrap();
    let mut nonmatching = accepted_view.clone();
    nonmatching.identity_revision += 1;

    assert!(
        !handle
            .reset_if_identity_receipt("test-owner", nonmatching)
            .await
            .unwrap()
    );
    assert!(
        handle
            .identity_bundle(TEST_USER_ID)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        handle
            .reset_if_identity_receipt("test-owner", accepted_view)
            .await
            .unwrap()
    );
    assert!(
        handle
            .identity_bundle(TEST_USER_ID)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn conditional_reset_preserves_a_newer_concurrent_pin() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device-list-pins.json");
    let mut store =
        DeviceListPinStore::load_from_with_clock(path.clone(), || 1_700_000_000_000).unwrap();
    store.bind_to_user("test-owner").unwrap();
    let handle = DeviceListPinStoreHandle::from_store(store).unwrap();
    let (mut alice, signer, first_device) = make_bootstrap_bundle(TEST_USER_ID);
    let rejected_view = build_bundle_view(&alice).unwrap();
    handle
        .verify_or_pin(rejected_view.clone(), PinContext::ExplicitShare)
        .await
        .unwrap();
    add_second_device(&mut alice, &first_device, &signer);
    handle
        .verify_or_pin(
            build_bundle_view(&alice).unwrap(),
            PinContext::BackgroundFetch,
        )
        .await
        .unwrap();

    assert!(
        !handle
            .reset_if_identity_receipt("test-owner", rejected_view)
            .await
            .unwrap()
    );
    assert_eq!(
        handle
            .identity_bundle(TEST_USER_ID)
            .await
            .unwrap()
            .expect("newer pin")
            .device_list
            .generation,
        2
    );
}

#[tokio::test]
async fn migrated_legacy_pin_adopts_first_incarnation_then_rejects_the_next() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device-list-pins.json");
    let (bundle, _, _) = make_bootstrap_bundle(TEST_USER_ID);
    let legacy_pin = DeviceListPin::from_view(
        &build_bundle_view(&bundle).unwrap(),
        1_700_000_000_000,
        std::collections::BTreeSet::new(),
    );
    let mut pin = serde_json::to_value(legacy_pin).unwrap();
    pin.as_object_mut()
        .expect("pin object")
        .remove("identity_revision");
    pin.as_object_mut()
        .expect("pin object")
        .remove("identity_incarnation_id");
    std::fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "version": 2,
            "last_bound_owner_user_id": "test-owner",
            "accounts": { "test-owner": { "pins": { TEST_USER_ID: pin } } }
        }))
        .unwrap(),
    )
    .unwrap();
    let store = DeviceListPinStore::load_from_with_clock(path, || 1_700_000_000_000).unwrap();
    let handle = DeviceListPinStoreHandle::from_store(store).unwrap();
    handle.bind_to_user("test-owner").await.unwrap();
    let first_incarnation = uuid::Uuid::now_v7();

    assert_eq!(
        handle
            .apply_identity_lifecycle(
                TEST_USER_ID,
                7,
                kodosi_domain::user::IdentityLifecycleState::Enrolled {
                    incarnation_id: first_incarnation,
                },
            )
            .await
            .unwrap(),
        IdentityLifecycleApplyOutcome::Recorded
    );
    assert!(
        handle
            .identity_bundle(TEST_USER_ID)
            .await
            .unwrap()
            .is_some()
    );
    let persisted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("device-list-pins.json")).unwrap())
            .unwrap();
    let adopted = &persisted["accounts"]["test-owner"]["pins"][TEST_USER_ID];
    assert_eq!(adopted["identity_revision"], 7);
    assert_eq!(
        adopted["identity_incarnation_id"],
        first_incarnation.to_string()
    );
    assert_eq!(
        handle
            .apply_identity_lifecycle(
                TEST_USER_ID,
                8,
                kodosi_domain::user::IdentityLifecycleState::Enrolled {
                    incarnation_id: uuid::Uuid::now_v7(),
                },
            )
            .await
            .unwrap(),
        IdentityLifecycleApplyOutcome::PinCleared
    );
}

#[tokio::test]
async fn migrated_legacy_pin_accepts_first_continuous_production_bundle() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device-list-pins.json");
    let (bundle, _, _) = make_bootstrap_bundle(TEST_USER_ID);
    let legacy_pin = DeviceListPin::from_view(
        &build_bundle_view(&bundle).unwrap(),
        1_700_000_000_000,
        std::collections::BTreeSet::new(),
    );
    let mut pin = serde_json::to_value(legacy_pin).unwrap();
    pin.as_object_mut()
        .expect("pin object")
        .remove("identity_revision");
    pin.as_object_mut()
        .expect("pin object")
        .remove("identity_incarnation_id");
    std::fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "version": 2,
            "last_bound_owner_user_id": "test-owner",
            "accounts": { "test-owner": { "pins": { TEST_USER_ID: pin } } }
        }))
        .unwrap(),
    )
    .unwrap();
    let store = DeviceListPinStore::load_from_with_clock(path, || 1_700_000_000_000).unwrap();
    let handle = DeviceListPinStoreHandle::from_store(store).unwrap();
    handle.bind_to_user("test-owner").await.unwrap();
    let mut production = build_bundle_view(&bundle).unwrap();
    production.identity_revision = 7;
    production.identity_incarnation_id = uuid::Uuid::now_v7();

    assert_eq!(
        handle
            .verify_or_pin(production, PinContext::BackgroundFetch)
            .await
            .unwrap(),
        PinVerdict::AlreadyPinned
    );
    assert!(
        handle
            .identity_bundle(TEST_USER_ID)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn lifecycle_jump_clears_pin_but_never_repins_in_the_same_explicit_call() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device-list-pins.json");
    let mut store =
        DeviceListPinStore::load_from_with_clock(path.clone(), || 1_700_000_000_000).unwrap();
    store.bind_to_user("test-owner").unwrap();
    let handle = DeviceListPinStoreHandle::from_store(store).unwrap();
    let (alice, _, _) = make_bootstrap_bundle(TEST_USER_ID);
    let mut replacement = build_bundle_view(&alice).unwrap();
    handle
        .verify_or_pin(replacement.clone(), PinContext::ExplicitShare)
        .await
        .unwrap();
    replacement.identity_revision += 1;
    replacement.identity_incarnation_id = uuid::Uuid::now_v7();

    assert_eq!(
        handle
            .verify_or_pin(replacement.clone(), PinContext::ExplicitShare)
            .await
            .unwrap(),
        PinVerdict::Reject {
            reason: BreakReason::IdentityLifecycleChanged
        }
    );
    assert!(
        handle
            .identity_bundle(TEST_USER_ID)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        handle
            .verify_or_pin(replacement, PinContext::ExplicitShare)
            .await
            .unwrap(),
        PinVerdict::FirstShare
    );
}

#[tokio::test]
async fn lifecycle_revision_clears_pin_once_and_persists_watermark() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device-list-pins.json");
    let mut store =
        DeviceListPinStore::load_from_with_clock(path.clone(), || 1_700_000_000_000).unwrap();
    store.bind_to_user("test-owner").unwrap();
    let handle = DeviceListPinStoreHandle::from_store(store).unwrap();
    let (alice, _, _) = make_bootstrap_bundle(TEST_USER_ID);
    handle
        .verify_or_pin(
            build_bundle_view(&alice).unwrap(),
            PinContext::ExplicitShare,
        )
        .await
        .unwrap();

    assert_eq!(
        handle
            .apply_identity_lifecycle(
                TEST_USER_ID,
                2,
                kodosi_domain::user::IdentityLifecycleState::Withdrawn,
            )
            .await
            .unwrap(),
        IdentityLifecycleApplyOutcome::PinCleared
    );
    assert!(
        handle
            .identity_bundle(TEST_USER_ID)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        handle
            .apply_identity_lifecycle(
                TEST_USER_ID,
                2,
                kodosi_domain::user::IdentityLifecycleState::Withdrawn,
            )
            .await
            .unwrap(),
        IdentityLifecycleApplyOutcome::Stale
    );
    assert_eq!(
        handle
            .apply_identity_lifecycle(
                TEST_USER_ID,
                1,
                kodosi_domain::user::IdentityLifecycleState::Enrolled {
                    incarnation_id: uuid::Uuid::now_v7(),
                },
            )
            .await
            .unwrap(),
        IdentityLifecycleApplyOutcome::Stale
    );
}

#[tokio::test]
async fn new_enrolled_incarnation_clears_old_pin_without_background_tofu() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device-list-pins.json");
    let mut store =
        DeviceListPinStore::load_from_with_clock(path.clone(), || 1_700_000_000_000).unwrap();
    store.bind_to_user("test-owner").unwrap();
    let handle = DeviceListPinStoreHandle::from_store(store).unwrap();
    let (alice, _, _) = make_bootstrap_bundle(TEST_USER_ID);
    let old = build_bundle_view(&alice).unwrap();
    handle
        .verify_or_pin(old.clone(), PinContext::ExplicitShare)
        .await
        .unwrap();
    let new_incarnation = uuid::Uuid::now_v7();

    assert_eq!(
        handle
            .apply_identity_lifecycle(
                TEST_USER_ID,
                2,
                kodosi_domain::user::IdentityLifecycleState::Enrolled {
                    incarnation_id: new_incarnation,
                },
            )
            .await
            .unwrap(),
        IdentityLifecycleApplyOutcome::PinCleared
    );
    let mut new = old;
    new.identity_revision = 2;
    new.identity_incarnation_id = new_incarnation;
    std::assert_matches!(
        handle
            .verify_or_pin(new, PinContext::BackgroundFetch)
            .await
            .unwrap(),
        PinVerdict::Reject {
            reason: BreakReason::NoExplicitShareContext
        }
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn handle_serializes_concurrent_verify_or_pin() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device-list-pins.json");
    let mut store =
        DeviceListPinStore::load_from_with_clock(path.clone(), || 1_700_000_000_000).unwrap();
    store.bind_to_user("test-owner").unwrap();
    let handle = DeviceListPinStoreHandle::from_store(store).unwrap();

    let (alice_dto, alice_kp1, alice_dev1) = make_bootstrap_bundle(TEST_USER_ID);
    handle
        .verify_or_pin(
            build_bundle_view(&alice_dto).unwrap(),
            PinContext::ExplicitShare,
        )
        .await
        .unwrap();

    let mut extended = alice_dto.clone();
    let (_kp2, _dev2) = add_second_device(&mut extended, &alice_dev1, &alice_kp1);
    let shared_view = build_bundle_view(&extended).unwrap();

    let mut handles = Vec::with_capacity(8);
    for _ in 0..8 {
        let pin_store = handle.clone();
        let view = shared_view.clone();
        handles.push(tokio::spawn(async move {
            pin_store
                .verify_or_pin(view, PinContext::BackgroundFetch)
                .await
        }));
    }

    for handle in handles {
        handle.await.unwrap().unwrap();
    }
    handle.shutdown_blocking();

    let reloaded = DeviceListPinStore::load_from_with_clock(path, || 1_700_000_000_000).unwrap();
    let disk_pin = reloaded.get(TEST_USER_ID).expect("disk pin present");
    let summary = disk_pin.proof_summary().expect("pin summary");
    assert_eq!(summary.generation, 2);
    assert_eq!(summary.active_device_ids.len(), 2);
}
