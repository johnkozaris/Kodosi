use super::*;

const PEER_USER_ID: &str = "01900000-0000-7000-8000-000000000041";

#[tokio::test]
async fn identity_lifecycle_revision_orders_reset_reenrollment_and_delayed_reset() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pin_path = dir.path().join("device-list-pins.json");
    let pin_store = DeviceListPinStoreHandle::from_store(
        DeviceListPinStore::load_from(pin_path).expect("test pin store should load"),
    )
    .expect("test pin-store actor should start");
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies")
            .with_pin_store(pin_store),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let origin = authenticate_test_app(&mut app);
    app.pin_store
        .bind_to_user(&origin.account_user_id)
        .await
        .expect("test pin store should bind to the authenticated account");

    let old_view = fake_identity_view(PEER_USER_ID, "alice-dev-1");
    assert_eq!(
        app.pin_store
            .verify_or_pin(old_view.clone(), PinContext::ExplicitShare)
            .await
            .expect("initial pin should install"),
        PinVerdict::FirstShare
    );

    app.handle_session_event(RuntimeSessionEvent::UserIdentityLifecycleChanged {
        origin: origin.clone(),
        user_id: PEER_USER_ID.to_owned(),
        identity_revision: 2,
        state: kodosi_domain::user::IdentityLifecycleState::Withdrawn,
    });
    app.flush_pending_session_events().await;
    assert!(
        app.pin_store
            .identity_bundle(PEER_USER_ID)
            .await
            .expect("pin lookup after reset")
            .is_none()
    );

    let new_incarnation = uuid::Uuid::now_v7();
    app.handle_session_event(RuntimeSessionEvent::UserIdentityLifecycleChanged {
        origin: origin.clone(),
        user_id: PEER_USER_ID.to_owned(),
        identity_revision: 3,
        state: kodosi_domain::user::IdentityLifecycleState::Enrolled {
            incarnation_id: new_incarnation,
        },
    });
    app.flush_pending_session_events().await;

    let mut new_view = old_view;
    new_view.identity_revision = 3;
    new_view.identity_incarnation_id = new_incarnation;
    assert_eq!(
        app.pin_store
            .verify_or_pin(new_view, PinContext::ExplicitShare)
            .await
            .expect("explicit verification should establish the new incarnation"),
        PinVerdict::FirstShare
    );

    app.handle_session_event(RuntimeSessionEvent::UserIdentityLifecycleChanged {
        origin,
        user_id: PEER_USER_ID.to_owned(),
        identity_revision: 2,
        state: kodosi_domain::user::IdentityLifecycleState::Withdrawn,
    });
    app.flush_pending_session_events().await;

    assert!(
        app.pin_store
            .identity_bundle(PEER_USER_ID)
            .await
            .expect("new incarnation survives delayed reset")
            .is_some()
    );
}
