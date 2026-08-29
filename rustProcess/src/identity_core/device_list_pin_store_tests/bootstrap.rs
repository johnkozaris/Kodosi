use super::*;

#[test]
fn first_pin_installs_on_explicit_share() {
    let (_dir, mut store) = make_store();
    let (dto, _kp, _id) = make_bootstrap_bundle(TEST_USER_ID);
    let view = build_bundle_view(&dto).unwrap();

    let verdict = store
        .verify_or_pin(&view, PinContext::ExplicitShare)
        .unwrap();
    assert_eq!(verdict, PinVerdict::FirstShare);
    assert!(store.get(TEST_USER_ID).is_some());
}

#[test]
fn first_pin_persist_failure_leaves_memory_empty() {
    let dir = tempfile::tempdir().unwrap();
    let blocked_parent = dir.path().join("not-a-directory");
    std::fs::write(&blocked_parent, "not a directory").unwrap();
    let mut store = DeviceListPinStore {
        path: blocked_parent.join("device-list-pins.json"),
        pins: std::collections::BTreeMap::new(),
        owner_user_id: Some("test-owner".to_owned()),
        clock: || 1_700_000_000_000,
    };
    let (dto, _kp, _id) = make_bootstrap_bundle(TEST_USER_ID);
    let view = build_bundle_view(&dto).unwrap();

    let result = store.verify_or_pin(&view, PinContext::ExplicitShare);

    assert!(result.is_err());
    assert!(store.get(TEST_USER_ID).is_none());
}

#[test]
fn first_pin_refuses_background_fetch() {
    let (_dir, mut store) = make_store();
    let (dto, _kp, _id) = make_bootstrap_bundle(TEST_USER_ID);
    let view = build_bundle_view(&dto).unwrap();

    let verdict = store
        .verify_or_pin(&view, PinContext::BackgroundFetch)
        .unwrap();
    assert_eq!(
        verdict,
        PinVerdict::Reject {
            reason: BreakReason::NoExplicitShareContext
        }
    );
    assert!(store.get(TEST_USER_ID).is_none());
}
