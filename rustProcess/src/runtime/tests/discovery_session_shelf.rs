use super::*;

#[test]
fn discovery_invalidations_are_deduplicated() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));

    let origin = authenticate_test_app(&mut app);

    app.handle_session_event(RuntimeSessionEvent::DiscoveryInvalidated {
        origin: origin.clone(),
        surfaces: vec![DiscoverySurface::Friends, DiscoverySurface::RoomCatalog],
        room_id: None,
    });
    app.handle_session_event(RuntimeSessionEvent::DiscoveryInvalidated {
        origin,
        surfaces: vec![
            DiscoverySurface::RoomCatalog,
            DiscoverySurface::RoomFeed,
            DiscoverySurface::OwnSessions,
        ],
        room_id: None,
    });

    assert_eq!(
        app.state.take_pending_discovery_refresh(),
        BTreeSet::from([
            DiscoverySurface::Friends,
            DiscoverySurface::OwnSessions,
            DiscoverySurface::RoomCatalog,
            DiscoverySurface::RoomFeed,
        ])
    );
}

#[test]
fn prior_account_invalidation_is_rejected_and_room_scopes_coalesce() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let origin = authenticate_test_app(&mut app);

    app.handle_session_event(RuntimeSessionEvent::DiscoveryInvalidated {
        origin: AccountEventOrigin {
            account_user_id: "22222222-2222-2222-2222-222222222222".to_owned(),
            epoch: origin.epoch,
        },
        surfaces: vec![DiscoverySurface::RoomChat],
        room_id: Some("room-a".to_owned()),
    });
    assert!(app.state.take_pending_discovery_refresh().is_empty());
    assert!(
        app.state
            .take_pending_room_projection_refreshes()
            .is_empty()
    );

    for surface in [DiscoverySurface::RoomChat, DiscoverySurface::RoomTasks] {
        app.handle_session_event(RuntimeSessionEvent::DiscoveryInvalidated {
            origin: origin.clone(),
            surfaces: vec![surface],
            room_id: Some("room-a".to_owned()),
        });
    }
    assert_eq!(
        app.state
            .take_pending_room_projection_refreshes()
            .remove("room-a"),
        Some(BTreeSet::from([
            DiscoverySurface::RoomChat,
            DiscoverySurface::RoomTasks,
        ]))
    );
}

#[test]
fn published_state_change_does_not_activate_owned_session() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let active_id = SessionId::new();
    let published_id = SessionId::new();

    app.state
        .local
        .sessions
        .insert(claude_owned_summary(active_id, "/tmp/active"));
    app.state
        .local
        .sessions
        .insert(claude_owned_summary(published_id, "/tmp/published"));
    app.state.shelf.activate(ShelfItem::Owned(active_id));
    authenticate_test_app(&mut app);
    let relay_generation = app
        .state
        .sharing
        .host_relays
        .allocate_generation()
        .expect("allocate test relay generation");
    app.state
        .sharing
        .host_relays
        .claim_generation(published_id, relay_generation)
        .expect("claim test relay generation");

    let origin = current_host_relay_origin(&app, published_id, relay_generation);
    app.handle_session_event(RuntimeSessionEvent::StateChanged {
        origin: origin.clone(),
        state: SessionState::Published,
    });

    assert_eq!(
        app.state.shelf.active_session(),
        Some(ShelfItem::Owned(active_id))
    );

    app.handle_session_event(RuntimeSessionEvent::StateChanged {
        origin,
        state: SessionState::Running,
    });

    assert_eq!(
        app.state.shelf.active_session(),
        Some(ShelfItem::Owned(published_id))
    );
}
