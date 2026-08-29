use super::*;

#[test]
fn host_access_revocation_removes_explicit_grant_before_rotation() {
    let mut app = test_app();
    let session_id = SessionId::new();
    let mut shared = test_shared_session_state();
    shared.grant_user_at(
        "revoked-user".to_owned(),
        AccessLevel::Inject,
        time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    );
    app.state.sharing.shared_sessions.insert(session_id, shared);

    app.state
        .apply_host_access_revoked(session_id, "revoked-user");

    let shared = app
        .state
        .sharing
        .shared_sessions
        .get(session_id)
        .expect("shared session remains");
    assert!(
        !shared
            .explicit_grantee_access()
            .contains_key("revoked-user")
    );
    assert_eq!(
        app.state.pending_work.drain_host_key_rotations(),
        vec![session_id]
    );
}

fn start_relay_generation(app: &mut Runtime, id: SessionId) -> u64 {
    if !app.state.identity.auth.is_authenticated() {
        authenticate_test_app(app);
    }
    let generation = app
        .state
        .sharing
        .host_relays
        .allocate_generation()
        .expect("allocate test relay generation");
    app.state
        .sharing
        .host_relays
        .claim_generation(id, generation)
        .expect("claim test relay generation");
    generation
}

fn state_of(app: &Runtime, id: SessionId) -> Option<SessionState> {
    app.state
        .local
        .sessions
        .record(id)
        .map(|record| record.summary.state)
}

fn scope_of(app: &Runtime, id: SessionId) -> Option<ShareScope> {
    app.state
        .local
        .sessions
        .record(id)
        .map(|record| record.summary.scope)
}

fn relay_state_change(
    app: &Runtime,
    id: SessionId,
    state: SessionState,
    generation: u64,
) -> RuntimeSessionEvent {
    RuntimeSessionEvent::StateChanged {
        origin: crate::session_runtime::events::HostRelayEventOrigin {
            account_origin: current_account_origin(app),
            session_id: id,
            relay_generation: generation,
        },
        state,
    }
}

#[test]
fn unsharing_an_active_session_leaves_it_active_and_owner_only() {
    let (mut app, id) = app_with_shared_cached_session(SessionState::Published);
    let relay = start_relay_generation(&mut app, id);

    app.state.unshare_session_locally(id);

    assert_eq!(state_of(&app, id), Some(SessionState::Running));
    assert_eq!(scope_of(&app, id), Some(ShareScope::JustMe));
    assert!(!app.state.sharing.shared_sessions.contains(id));
    assert_eq!(app.state.host_ws_status, ConnectionState::Offline);
    assert_ne!(
        app.state.sharing.host_relays.generation(id),
        relay,
        "the cancelled relay must not keep state authority over the session"
    );
}

#[test]
fn unsharing_a_session_the_relay_had_already_marked_reconnecting_restores_it() {
    let (mut app, id) = app_with_shared_cached_session(SessionState::Published);
    let relay = start_relay_generation(&mut app, id);
    app.handle_session_event(relay_state_change(
        &app,
        id,
        SessionState::Reconnecting,
        relay,
    ));
    assert_eq!(state_of(&app, id), Some(SessionState::Reconnecting));

    app.state.unshare_session_locally(id);

    assert_eq!(
        state_of(&app, id),
        Some(SessionState::Running),
        "with no relay left to reconnect, the local child process is the only truth"
    );
}

#[test]
fn unsharing_a_stopped_session_leaves_it_stopped() {
    let (mut app, id) = app_with_shared_cached_session(SessionState::Stopped);

    app.state.unshare_session_locally(id);

    assert_eq!(
        state_of(&app, id),
        Some(SessionState::Stopped),
        "an unshare starts nothing, so it must not resurrect a stopped session"
    );
    assert_eq!(scope_of(&app, id), Some(ShareScope::JustMe));
}

#[test]
fn unsharing_a_failed_session_leaves_it_failed() {
    let (mut app, id) = app_with_shared_cached_session(SessionState::Failed);

    app.state.unshare_session_locally(id);

    assert_eq!(state_of(&app, id), Some(SessionState::Failed));
    assert_eq!(scope_of(&app, id), Some(ShareScope::JustMe));
}

#[test]
fn a_retired_relay_cannot_reconnect_a_session_that_was_just_unshared() {
    let (mut app, id) = app_with_shared_cached_session(SessionState::Published);
    let retired = start_relay_generation(&mut app, id);

    app.state.unshare_session_locally(id);

    app.handle_session_event(relay_state_change(
        &app,
        id,
        SessionState::Reconnecting,
        retired,
    ));

    assert_eq!(
        state_of(&app, id),
        Some(SessionState::Running),
        "a relay the unshare already cancelled must not overwrite the committed local state"
    );
    assert_eq!(app.state.host_ws_status, ConnectionState::Offline);
}

#[test]
fn a_retired_relay_cannot_stop_a_session_that_was_just_unshared() {
    let (mut app, id) = app_with_shared_cached_session(SessionState::Published);
    let retired = start_relay_generation(&mut app, id);

    app.state.unshare_session_locally(id);

    app.handle_session_event(relay_state_change(&app, id, SessionState::Stopped, retired));

    assert_eq!(
        state_of(&app, id),
        Some(SessionState::Running),
        "the backend closing an ended row says nothing about the local child process"
    );
}

#[test]
fn a_scope_change_hands_state_authority_to_the_relay_it_started() {
    let (mut app, id) = app_with_shared_cached_session(SessionState::Published);
    let retired = start_relay_generation(&mut app, id);

    app.state.sharing.host_relays.cancel(id);
    let current = start_relay_generation(&mut app, id);

    app.handle_session_event(relay_state_change(
        &app,
        id,
        SessionState::Reconnecting,
        retired,
    ));
    assert_eq!(
        state_of(&app, id),
        Some(SessionState::Published),
        "the relay the scope change replaced no longer speaks for the session"
    );

    app.handle_session_event(relay_state_change(
        &app,
        id,
        SessionState::Reconnecting,
        current,
    ));
    assert_eq!(state_of(&app, id), Some(SessionState::Reconnecting));
}

#[test]
fn re_sharing_after_an_unshare_gives_the_new_relay_state_authority() {
    let (mut app, id) = app_with_shared_cached_session(SessionState::Published);
    let retired = start_relay_generation(&mut app, id);
    app.state.unshare_session_locally(id);

    app.state
        .sharing
        .shared_sessions
        .insert(id, test_shared_session_state());
    app.state
        .apply_session_scope_locally(id, ShareScope::MyDevices);
    let current = start_relay_generation(&mut app, id);
    app.handle_session_event(relay_state_change(
        &app,
        id,
        SessionState::Published,
        current,
    ));

    assert_eq!(state_of(&app, id), Some(SessionState::Published));
    assert_eq!(scope_of(&app, id), Some(ShareScope::MyDevices));

    app.handle_session_event(relay_state_change(
        &app,
        id,
        SessionState::Reconnecting,
        retired,
    ));
    assert_eq!(
        state_of(&app, id),
        Some(SessionState::Published),
        "the relay from the previous share must not disturb the new one"
    );
}

#[test]
fn a_live_relay_still_moves_a_shared_session_to_reconnecting() {
    let (mut app, id) = app_with_shared_cached_session(SessionState::Published);
    let relay = start_relay_generation(&mut app, id);

    app.handle_session_event(relay_state_change(
        &app,
        id,
        SessionState::Reconnecting,
        relay,
    ));

    assert_eq!(state_of(&app, id), Some(SessionState::Reconnecting));
    assert_eq!(app.state.host_ws_status, ConnectionState::Reconnecting);

    app.handle_session_event(relay_state_change(&app, id, SessionState::Published, relay));

    assert_eq!(state_of(&app, id), Some(SessionState::Published));
    assert_eq!(app.state.host_ws_status, ConnectionState::Connected);
}

#[test]
fn a_state_change_from_no_relay_at_all_is_refused() {
    let (mut app, id) = app_with_shared_cached_session(SessionState::Published);

    authenticate_test_app(&mut app);
    app.handle_session_event(relay_state_change(
        &app,
        id,
        SessionState::Reconnecting,
        crate::sharing::host_relay::registry::NO_HOST_RELAY_GENERATION,
    ));

    assert_eq!(state_of(&app, id), Some(SessionState::Published));
}

#[test]
fn stale_negative_action_result_ack_retires_only_the_generation_send_key() {
    let (mut app, id) = app_with_shared_cached_session(SessionState::Published);
    let retired = start_relay_generation(&mut app, id);
    let account_user_id = app
        .state
        .identity
        .auth
        .subject_string()
        .expect("authenticated test app");
    let result = crate::runtime::action_results::OwnerActionResult {
        account_user_id: account_user_id.clone(),
        session_id: id.to_string(),
        incarnation_id: uuid::Uuid::now_v7(),
        action_id: "action-1".to_owned(),
        request_id: "request-1".to_owned(),
        request_generation: 7,
        requester_user_id: "requester-1".to_owned(),
        requester_device_id: "device-1".to_owned(),
        accepted: Some(true),
    };
    app.owner_action_results
        .admit(result.clone())
        .expect("persist action result");
    let key = (
        id,
        result.action_id.clone(),
        result.requester_user_id.clone(),
        retired,
    );
    app.insert_action_result_in_flight_for_test(key.clone());

    app.state.sharing.host_relays.cancel(id);
    assert!(
        !app.handle_session_event(RuntimeSessionEvent::HostActionResultMailboxAck {
            origin: current_host_relay_origin(&app, id, retired),
            result,
            delivered: false,
        })
    );

    assert!(!app.action_result_in_flight_for_test(&key));
    assert_eq!(
        app.owner_action_results
            .pending_for(&account_user_id, id)
            .len(),
        1,
        "a failed delivery must leave the durable result pending"
    );
}

#[test]
fn stale_negative_semantic_receipt_ack_retires_only_the_generation_send_key() {
    let (mut app, id) = app_with_shared_cached_session(SessionState::Published);
    let retired = start_relay_generation(&mut app, id);
    let request_id = uuid::Uuid::now_v7();
    let incarnation_id = uuid::Uuid::now_v7();
    let key = (id, request_id, retired);
    app.insert_semantic_receipt_in_flight_for_test(key);

    app.state.sharing.host_relays.cancel(id);
    assert!(
        !app.handle_session_event(RuntimeSessionEvent::HostSemanticReceiptMailboxAck {
            origin: current_host_relay_origin(&app, id, retired),
            request_id,
            incarnation_id,
            delivered: false,
            acknowledge_locally: true,
        })
    );

    assert!(!app.semantic_receipt_in_flight_for_test(&key));
}

#[test]
fn stopping_a_session_retires_the_relay_generation() {
    let (mut app, id) = app_with_shared_cached_session(SessionState::Running);
    let relay = start_relay_generation(&mut app, id);

    app.handle_session_event(RuntimeSessionEvent::Stopped {
        origin: local_coordinator_origin(&app, id),
        reason: StopReason::UserRequested,
    });
    app.handle_session_event(relay_state_change(
        &app,
        id,
        SessionState::Reconnecting,
        relay,
    ));

    assert_eq!(
        state_of(&app, id),
        None,
        "a torn-down relay must not resurrect a session that already ended"
    );
}
