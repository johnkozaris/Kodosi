use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use tokio_util::sync::CancellationToken;

use super::handlers::apply_recovered_room_mutation_security_effects;
use super::{
    RuntimeFlushOptions, apply_auth_message, apply_devices_message, apply_friends_message,
    apply_room_message, apply_session_message, apply_system_message, apply_terminal_message,
    apply_trust_message, flush_runtime_outputs, publish_recovered_room_mutations,
    publish_state_snapshot, register_terminal_hub_subscriber,
};
use crate::{
    AgentIntelEvent, AuthCommand, AuthEvent, AuthRequiredReason, DeviceCommand, DeviceEvent,
    FriendsCommand, FriendsEvent, RoomActionStatus, RoomCommand, RoomEvent, SessionCommand,
    SessionEvent, SystemCommand, SystemEvent, TerminalCommand, TerminalEvent, TrustCommand,
    TrustEvent,
    config::AppConfig,
    host_protocol::RoomListEntry,
    runtime::{Runtime, session_catalog},
    runtime_event_bus::runtime_event_channels,
    sharing::shared_session_registry::{SharedRoom, SharedSessionState},
    terminal_transport::{TerminalCapability, TerminalHubRequest, TerminalSurface},
};
use kodosi_domain::{
    auth::AuthState,
    ids::{SessionId, UserId},
    permissions::{AccessLevel, ShareScope},
    terminal::{
        Revision, TerminalCheckpointV2, TerminalPresentationFrame, TerminalPresentationV2,
        TerminalScreen, TerminalSize,
    },
};
use tokio::sync::oneshot;

fn remote_checkpoint(rows: u16, cols: u16) -> TerminalCheckpointV2 {
    let mut terminal = ghostty_vt::Terminal::new(cols, rows, ghostty_vt::TerminalPolicy::default())
        .expect("test terminal");
    let semantic = terminal
        .semantic_checkpoint(ghostty_vt::CheckpointLimits::default())
        .expect("semantic checkpoint");
    let state = terminal.state().expect("terminal state");
    TerminalCheckpointV2::new(
        TerminalSize::new(rows, cols).expect("size"),
        TerminalScreen::Primary,
        semantic.into_bytes(),
        state.cursor_x,
        state.cursor_y,
        !state.cursor_visible,
    )
    .expect("checkpoint")
}

fn remote_presentation(
    rows: u16,
    cols: u16,
    line: &str,
    _state: &[u8],
) -> TerminalPresentationFrame {
    let presentation = TerminalPresentationV2::new(
        TerminalSize::new(rows, cols).expect("size"),
        TerminalScreen::Primary,
        vec![line.to_owned(); usize::from(rows)],
        0,
        0,
        false,
    )
    .expect("presentation");
    TerminalPresentationFrame::new(Revision::default(), presentation)
}

async fn recv_agent_event(
    rx: &mut tokio::sync::mpsc::Receiver<crate::AccountAgentIntelEvent>,
) -> Option<AgentIntelEvent> {
    rx.recv().await.map(|envelope| envelope.event)
}

fn try_recv_agent_event(
    rx: &mut tokio::sync::mpsc::Receiver<crate::AccountAgentIntelEvent>,
) -> Result<AgentIntelEvent, tokio::sync::mpsc::error::TryRecvError> {
    rx.try_recv().map(|envelope| envelope.event)
}

fn drain_terminal_control_events(
    rx: &mut tokio::sync::mpsc::Receiver<TerminalEvent>,
) -> Vec<TerminalEvent> {
    let mut messages = Vec::new();
    while let Ok(message) = rx.try_recv() {
        messages.push(message);
    }
    messages
}

fn drain_devices_events(
    rx: &mut tokio::sync::mpsc::Receiver<crate::AccountDeviceEvent>,
) -> Vec<DeviceEvent> {
    let mut messages = Vec::new();
    while let Ok(message) = rx.try_recv() {
        messages.push(message.event);
    }
    messages
}

fn drain_trust_events(
    rx: &mut tokio::sync::mpsc::Receiver<crate::AccountTrustEvent>,
) -> Vec<TrustEvent> {
    let mut messages = Vec::new();
    while let Ok(message) = rx.try_recv() {
        messages.push(message.event);
    }
    messages
}

fn drain_friends_events(
    rx: &mut tokio::sync::mpsc::Receiver<crate::AccountFriendsEvent>,
) -> Vec<FriendsEvent> {
    let mut messages = Vec::new();
    while let Ok(message) = rx.try_recv() {
        messages.push(message.event);
    }
    messages
}

fn drain_room_events(
    rx: &mut tokio::sync::mpsc::Receiver<crate::AccountRoomEvent>,
) -> Vec<RoomEvent> {
    let mut messages = Vec::new();
    while let Ok(message) = rx.try_recv() {
        messages.push(message.event);
    }
    messages
}

fn drain_session_events(
    rx: &mut tokio::sync::mpsc::Receiver<crate::AccountSessionEvent>,
) -> Vec<SessionEvent> {
    let mut messages = Vec::new();
    while let Ok(message) = rx.try_recv() {
        messages.push(message.event);
    }
    messages
}

fn drain_auth_events(rx: &mut tokio::sync::mpsc::Receiver<AuthEvent>) -> Vec<AuthEvent> {
    let mut messages = Vec::new();
    while let Ok(message) = rx.try_recv() {
        messages.push(message);
    }
    messages
}

fn drain_system_events(rx: &mut tokio::sync::mpsc::Receiver<SystemEvent>) -> Vec<SystemEvent> {
    let mut messages = Vec::new();
    while let Ok(message) = rx.try_recv() {
        messages.push(message);
    }
    messages
}

fn drain_system_events_after_optional_healthy_snapshot(
    rx: &mut tokio::sync::mpsc::Receiver<SystemEvent>,
) -> Vec<SystemEvent> {
    let mut messages = drain_system_events(rx);
    if let Some(health_index) = messages
        .iter()
        .position(|event| matches!(event, SystemEvent::RuntimeHealth { .. }))
    {
        std::assert_matches!(
            &messages[health_index],
            SystemEvent::RuntimeHealth { collaboration_cleanup }
                if collaboration_cleanup.pending_count == 0
                    && collaboration_cleanup.quarantined_count == 0
                    && collaboration_cleanup.message.is_none()
        );
        messages.remove(health_index);
    }
    messages
}

fn test_app_with_config(config: AppConfig) -> Runtime {
    Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"))
}

fn test_app() -> Runtime {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    test_app_with_config(config)
}

#[tokio::test]
async fn established_catalog_publishes_session_deltas() {
    let mut app = test_app();
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal = None;
    let mut last_auth_event = None;
    publish_state_snapshot(&app, &tx, &mut last_signal, true, true)
        .await
        .expect("initial catalog publishes");
    drain_session_events(&mut rx.sessions_rx);
    drain_system_events(&mut rx.system_rx);

    let session_id = SessionId::new();
    let mut summary = kodosi_domain::session::SessionSummary::new_owned(
        session_id,
        "Delta".to_owned(),
        "/tmp".to_owned(),
        TerminalSize::new(80, 24).expect("valid terminal size"),
        None,
    );
    summary.state = kodosi_domain::session::SessionState::Running;
    app.state.local.sessions.insert(summary);
    flush_runtime_outputs(
        &mut app,
        &tx,
        &mut last_signal,
        &mut last_auth_event,
        RuntimeFlushOptions::standard(true),
    )
    .await
    .expect("session addition publishes");
    std::assert_matches!(
        drain_session_events(&mut rx.sessions_rx).as_slice(),
        [SessionEvent::Upsert { session }] if session.id() == session_id.to_string()
    );

    app.state.local.sessions.delete(session_id);
    flush_runtime_outputs(
        &mut app,
        &tx,
        &mut last_signal,
        &mut last_auth_event,
        RuntimeFlushOptions::standard(true),
    )
    .await
    .expect("session removal publishes");
    std::assert_matches!(
        drain_session_events(&mut rx.sessions_rx).as_slice(),
        [SessionEvent::Removed { session_id: removed }] if removed == &session_id.to_string()
    );
}

#[tokio::test]
async fn established_catalog_skips_unforced_standard_flush() {
    let mut app = test_app();
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal = None;
    let mut last_auth_event = None;

    publish_state_snapshot(&app, &tx, &mut last_signal, true, true)
        .await
        .expect("initial catalog publishes");
    drain_session_events(&mut rx.sessions_rx);
    drain_system_events(&mut rx.system_rx);

    flush_runtime_outputs(
        &mut app,
        &tx,
        &mut last_signal,
        &mut last_auth_event,
        RuntimeFlushOptions::standard(false),
    )
    .await
    .expect("unforced flush succeeds");

    assert!(drain_session_events(&mut rx.sessions_rx).is_empty());
    assert!(drain_system_events(&mut rx.system_rx).is_empty());
}

#[tokio::test]
async fn pending_discovery_catalog_check_survives_maintenance_skip() {
    let mut app = test_app();
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal = None;
    let mut last_auth_event = None;
    publish_state_snapshot(&app, &tx, &mut last_signal, true, true)
        .await
        .expect("initial catalog publishes");
    drain_session_events(&mut rx.sessions_rx);
    drain_system_events(&mut rx.system_rx);

    let session_id = SessionId::new();
    let summary = kodosi_domain::session::SessionSummary::new_remote(
        session_id,
        "Remote".to_owned(),
        "Owner".to_owned(),
        Some(UserId::try_from("11111111-1111-1111-1111-111111111111").expect("valid test user id")),
        ShareScope::Friends,
        AccessLevel::Suggest,
        TerminalSize::new(90, 30).expect("valid size"),
    );
    app.state.discovery.replace_remote_sessions(
        vec![crate::discovery::RemoteSessionRecord {
            summary,
            incarnation_id: None,
            room_id: None,
            connection_state: None,
            connection_reason: None,
            access_state: None,
            access_reason: None,
            access_issue: None,
            viewer_blocked: false,
            viewer_hidden: false,
        }],
        false,
    );
    app.state.catalog_refresh = crate::runtime::state::CatalogRefreshState::CheckPending;

    flush_runtime_outputs(
        &mut app,
        &tx,
        &mut last_signal,
        &mut last_auth_event,
        RuntimeFlushOptions::maintenance(false),
    )
    .await
    .expect("pending discovery check publishes through maintenance skip");

    std::assert_matches!(
        drain_session_events(&mut rx.sessions_rx).as_slice(),
        [SessionEvent::Upsert { session }] if session.id() == session_id.to_string()
    );
    assert_eq!(
        app.state.catalog_refresh,
        crate::runtime::state::CatalogRefreshState::Idle
    );
}

#[tokio::test]
async fn account_context_change_republishes_unchanged_catalog() {
    let mut app = test_app();
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal = None;

    publish_state_snapshot(&app, &tx, &mut last_signal, true, true)
        .await
        .expect("initial catalog publishes");
    assert_eq!(drain_session_events(&mut rx.sessions_rx).len(), 1);
    drain_system_events(&mut rx.system_rx);

    app.state.identity.auth = AuthState::Authenticated {
        subject: Some(UserId::try_from("01900000-0000-7000-8000-000000000001").expect("user ID")),
        expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    publish_state_snapshot(&app, &tx, &mut last_signal, false, true)
        .await
        .expect("account context republishes catalog");

    assert_eq!(drain_session_events(&mut rx.sessions_rx).len(), 1);
}

#[tokio::test]
async fn event_drain_checks_without_replay_and_replays_on_request() {
    let mut app = test_app();
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal = None;
    let mut last_auth_event = None;

    publish_state_snapshot(&app, &tx, &mut last_signal, true, true)
        .await
        .expect("initial catalog publishes");
    assert_eq!(drain_session_events(&mut rx.sessions_rx).len(), 1);
    std::assert_matches!(
        drain_system_events(&mut rx.system_rx).as_slice(),
        [SystemEvent::RuntimeHealth { collaboration_cleanup }]
            if collaboration_cleanup.pending_count == 0
                && collaboration_cleanup.quarantined_count == 0
                && collaboration_cleanup.message.is_none()
    );

    flush_runtime_outputs(
        &mut app,
        &tx,
        &mut last_signal,
        &mut last_auth_event,
        RuntimeFlushOptions::standard(false),
    )
    .await
    .expect("event-only drain succeeds");
    assert!(drain_session_events(&mut rx.sessions_rx).is_empty());

    app.state.catalog_refresh = crate::runtime::state::CatalogRefreshState::ReplayPending;
    flush_runtime_outputs(
        &mut app,
        &tx,
        &mut last_signal,
        &mut last_auth_event,
        RuntimeFlushOptions::standard(false),
    )
    .await
    .expect("explicit replay refreshes catalog without another force signal");
    assert_eq!(drain_session_events(&mut rx.sessions_rx).len(), 1);
}

#[test]
fn account_scoped_command_rejects_user_and_epoch_changes_after_admission() {
    let mut app = test_app();
    let user_id = "01900000-0000-7000-8000-000000000001";
    app.state.identity.auth = AuthState::Authenticated {
        subject: Some(UserId::try_from(user_id).expect("user ID")),
        expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    let (current_user, current_epoch) = app.state.identity.event_context();
    let context = crate::AccountCommandContext {
        user_id: current_user,
        epoch: current_epoch,
    };

    assert_eq!(
        super::accepts_account_command(
            &app,
            crate::AccountScopedCommand::scoped(context.clone(), "current")
        ),
        Some("current")
    );
    assert_eq!(
        super::accepts_account_command(
            &app,
            crate::AccountScopedCommand::scoped(
                crate::AccountCommandContext {
                    user_id: Some("01900000-0000-7000-8000-000000000002".to_owned()),
                    epoch: current_epoch,
                },
                "wrong-user",
            )
        ),
        None
    );
    assert_eq!(
        super::accepts_account_command(
            &app,
            crate::AccountScopedCommand::scoped(
                crate::AccountCommandContext {
                    user_id: context.user_id,
                    epoch: current_epoch + 1,
                },
                "future-epoch",
            )
        ),
        None
    );
}

#[tokio::test]
async fn collaboration_cleanup_health_emits_only_when_changed() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    config.backend.api = Some("http://127.0.0.1:9/".to_owned());
    let mut app = test_app_with_config(config);
    app.state.identity.auth = AuthState::Authenticated {
        subject: Some(UserId::try_from("01900000-0000-7000-8000-000000000001").expect("user ID")),
        expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal = None;

    publish_state_snapshot(&app, &tx, &mut last_signal, true, true)
        .await
        .expect("initial snapshot publishes");
    drain_session_events(&mut rx.sessions_rx);
    std::assert_matches!(
        drain_system_events(&mut rx.system_rx).as_slice(),
        [SystemEvent::RuntimeHealth { collaboration_cleanup }]
            if collaboration_cleanup.pending_count == 0
                && collaboration_cleanup.quarantined_count == 0
    );

    publish_state_snapshot(&app, &tx, &mut last_signal, true, true)
        .await
        .expect("forced catalog replay succeeds");
    assert!(
        drain_system_events(&mut rx.system_rx).is_empty(),
        "force must not duplicate unchanged health"
    );

    let backend_origin = "http://127.0.0.1:9/"
        .parse::<kodosi_backend_client::BackendOrigin>()
        .expect("test backend origin");
    let create_ids = [uuid::Uuid::now_v7(), uuid::Uuid::now_v7()];
    let end_ids = [uuid::Uuid::now_v7(), uuid::Uuid::now_v7()];
    let live_incarnation_id = uuid::Uuid::now_v7();
    for index in 0..2 {
        app.collaboration_teardown
            .provision(
                &backend_origin,
                "01900000-0000-7000-8000-000000000001",
                &format!("session-{index}"),
                create_ids[index],
                end_ids[index],
                1,
            )
            .expect("test obligation");
    }
    app.collaboration_teardown
        .bind_incarnation(create_ids[0], "session-0", live_incarnation_id)
        .expect("bind live test obligation");
    app.state.sharing.shared_sessions.insert(
        SessionId::new(),
        SharedSessionState::new(
            "session-0".to_owned(),
            live_incarnation_id,
            "owner-secret".to_owned(),
            ShareScope::MyDevices,
            None,
            None,
            None,
        ),
    );
    publish_state_snapshot(&app, &tx, &mut last_signal, false, true)
        .await
        .expect("changed health publishes");
    std::assert_matches!(
        drain_system_events(&mut rx.system_rx).as_slice(),
        [SystemEvent::RuntimeHealth { collaboration_cleanup }]
            if collaboration_cleanup.pending_count == 1
                && collaboration_cleanup.quarantined_count == 0
                && collaboration_cleanup.message.is_none()
    );

    app.state.identity.auth = AuthState::Authenticated {
        subject: Some(UserId::try_from("01900000-0000-7000-8000-000000000002").expect("user ID")),
        expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    publish_state_snapshot(&app, &tx, &mut last_signal, false, true)
        .await
        .expect("account mismatch publishes account-scoped cleanup");
    std::assert_matches!(
        drain_system_events(&mut rx.system_rx).as_slice(),
        [SystemEvent::RuntimeHealth { collaboration_cleanup }]
            if collaboration_cleanup.pending_count == 0
                && collaboration_cleanup.quarantined_count == 0
    );

    app.state.identity.auth = AuthState::Authenticated {
        subject: Some(UserId::try_from("01900000-0000-7000-8000-000000000001").expect("user ID")),
        expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    publish_state_snapshot(&app, &tx, &mut last_signal, false, true)
        .await
        .expect("original account republishes its pending cleanup");
    std::assert_matches!(
        drain_system_events(&mut rx.system_rx).as_slice(),
        [SystemEvent::RuntimeHealth { collaboration_cleanup }]
            if collaboration_cleanup.pending_count == 1
                && collaboration_cleanup.quarantined_count == 0
    );

    app.collaboration_teardown
        .acknowledge(create_ids[1], None, end_ids[1])
        .expect("acknowledge pending test obligation");
    publish_state_snapshot(&app, &tx, &mut last_signal, false, true)
        .await
        .expect("healthy replacement publishes");
    std::assert_matches!(
        drain_system_events(&mut rx.system_rx).as_slice(),
        [SystemEvent::RuntimeHealth { collaboration_cleanup }]
            if collaboration_cleanup.pending_count == 0
                && collaboration_cleanup.quarantined_count == 0
                && collaboration_cleanup.message.is_none()
    );
}

#[tokio::test]
async fn hub_subscribe_rejects_unknown_session() {
    let mut app = test_app();
    let session_id = SessionId::new();
    let (reply, response) = oneshot::channel();

    register_terminal_hub_subscriber(
        &mut app,
        TerminalHubRequest {
            session_id,
            surface: TerminalSurface::Cli,
            capability: TerminalCapability::Write,
            reply,
        },
    )
    .await;

    let handle = response
        .await
        .unwrap_or_else(|error| panic!("hub subscribe should respond: {error}"));
    assert!(handle.is_none());
}

#[tokio::test]
async fn missing_session_mutation_error_keeps_session_correlation() {
    let mut app = test_app();
    let (tx, mut rx) = runtime_event_channels();
    let missing = SessionId::new().to_string();
    let mut last_signal = None;
    let mut last_auth_event = None;

    apply_session_message(
        &mut app,
        SessionCommand::Rename {
            session_id: missing.clone(),
            name: "renamed".to_owned(),
        },
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .unwrap_or_else(|error| panic!("missing-session mutation should emit a lane error: {error}"));

    let events = drain_session_events(&mut rx.sessions_rx);
    assert!(
        events.iter().any(|event| matches!(
            event,
            SessionEvent::Error {
                operation,
                session_id: Some(session_id),
                request_id: None,
                ..
            } if operation == "session.rename" && session_id == &missing
        )),
        "missing-session mutation events: {events:?}"
    );
}

#[tokio::test]
async fn hub_subscribe_rejects_plain_only_remote_cache() {
    use crate::session_runtime::events::RuntimeSessionEvent;

    let mut app = test_app();
    let session_id = SessionId::new();

    let summary = kodosi_domain::session::SessionSummary::new_remote(
        session_id,
        "Remote".to_owned(),
        "Owner".to_owned(),
        Some(
            kodosi_domain::ids::UserId::try_from("11111111-1111-1111-1111-111111111111")
                .expect("valid test user id"),
        ),
        kodosi_domain::permissions::ShareScope::Friends,
        kodosi_domain::permissions::AccessLevel::Suggest,
        TerminalSize::new(90, 30).expect("valid size"),
    );
    app.state.discovery.replace_remote_sessions(
        vec![crate::discovery::RemoteSessionRecord {
            summary,
            incarnation_id: None,
            room_id: None,
            connection_state: None,
            connection_reason: None,
            access_state: None,
            access_reason: None,
            access_issue: None,
            viewer_blocked: false,
            viewer_hidden: false,
        }],
        false,
    );

    let relay_generation = app
        .state
        .remote
        .session_relays
        .reserve_generation(session_id)
        .expect("test relay generation");
    let frame = remote_presentation(30, 90, "cached", b"TS");
    app.handle_session_event(RuntimeSessionEvent::RemotePlainPresentation {
        id: session_id,
        presentation: frame.presentation,
        relay_generation,
    });

    let (reply, response) = oneshot::channel();
    register_terminal_hub_subscriber(
        &mut app,
        TerminalHubRequest {
            session_id,
            surface: TerminalSurface::Desktop,
            capability: TerminalCapability::Write,
            reply,
        },
    )
    .await;

    assert!(
        response
            .await
            .unwrap_or_else(|error| panic!("hub subscribe should respond: {error}"))
            .is_none(),
        "plain presentation is never resumable native authority"
    );

    let (dropped_reply, dropped_response) = oneshot::channel();
    drop(dropped_response);
    register_terminal_hub_subscriber(
        &mut app,
        TerminalHubRequest {
            session_id,
            surface: TerminalSurface::Desktop,
            capability: TerminalCapability::Write,
            reply: dropped_reply,
        },
    )
    .await;
    assert_eq!(
        app.terminal_hub.connection_count(session_id),
        0,
        "plain-only native rejection must not allocate an unreachable connection"
    );
}

#[tokio::test]
async fn hub_subscribe_refuses_cached_snapshot_without_a_discovery_record() {
    use crate::session_runtime::events::RuntimeSessionEvent;

    let mut app = test_app();
    let session_id = SessionId::new();

    let relay_generation = app
        .state
        .remote
        .session_relays
        .reserve_generation(session_id)
        .expect("test relay generation");
    let frame = remote_presentation(30, 90, "secret", b"TS");
    app.handle_session_event(RuntimeSessionEvent::RemotePlainPresentation {
        id: session_id,
        presentation: frame.presentation,
        relay_generation,
    });
    assert!(app.remote_terminal.cached(session_id).is_some());

    let (reply, response) = oneshot::channel();
    register_terminal_hub_subscriber(
        &mut app,
        TerminalHubRequest {
            session_id,
            surface: TerminalSurface::Desktop,
            capability: TerminalCapability::Write,
            reply,
        },
    )
    .await;

    assert!(
        response
            .await
            .unwrap_or_else(|error| panic!("hub subscribe should respond: {error}"))
            .is_none(),
        "an unbacked cache entry must not authorize a subscription"
    );
    assert!(
        app.remote_terminal.cached(session_id).is_none(),
        "the unauthorized plaintext must be dropped, not retained"
    );
}

#[tokio::test]
async fn subscribing_leaves_pending_identity_work_for_the_maintenance_tick() {
    use crate::session_runtime::events::RuntimeSessionEvent;

    let mut app = test_app();
    let session_id = SessionId::new();
    let summary = kodosi_domain::session::SessionSummary::new_remote(
        session_id,
        "Remote".to_owned(),
        "Owner".to_owned(),
        Some(
            kodosi_domain::ids::UserId::try_from("11111111-1111-1111-1111-111111111111")
                .expect("valid test user id"),
        ),
        kodosi_domain::permissions::ShareScope::Friends,
        kodosi_domain::permissions::AccessLevel::Suggest,
        TerminalSize::new(80, 24).expect("valid size"),
    );
    app.state.discovery.replace_remote_sessions(
        vec![crate::discovery::RemoteSessionRecord {
            summary,
            incarnation_id: None,
            room_id: None,
            connection_state: None,
            connection_reason: None,
            access_state: None,
            access_reason: None,
            access_issue: None,
            viewer_blocked: false,
            viewer_hidden: false,
        }],
        false,
    );
    let relay_generation = app
        .state
        .remote
        .session_relays
        .reserve_generation(session_id)
        .expect("test relay generation");
    app.handle_session_event(RuntimeSessionEvent::RemoteCheckpoint {
        id: session_id,
        next_sequence: 0,
        checkpoint: remote_checkpoint(24, 80),
        application: kodosi_backend_client::session_relay::events::ApplicationAck::default(),
        relay_generation,
    });
    app.state.pending_work.queue_pin_refresh("peer-user");
    app.state
        .queue_discovery_refresh([crate::session_runtime::events::DiscoverySurface::Friends]);

    let (reply, response) = oneshot::channel();
    register_terminal_hub_subscriber(
        &mut app,
        TerminalHubRequest {
            session_id,
            surface: TerminalSurface::Desktop,
            capability: TerminalCapability::Write,
            reply,
        },
    )
    .await;

    assert!(
        response
            .await
            .unwrap_or_else(|error| panic!("hub subscribe should respond: {error}"))
            .is_some(),
        "the attach itself must still succeed"
    );
    let queued_pin_refreshes = app.state.pending_work.drain_pin_refreshes();
    assert_eq!(queued_pin_refreshes.len(), 1);
    assert_eq!(queued_pin_refreshes[0].user_id, "peer-user");
    assert!(
        !app.state.pending_discovery_surfaces.is_empty(),
        "discovery refresh must stay queued for the tick that budgets it"
    );
}

#[tokio::test]
async fn failed_device_enrollment_schedules_retry_and_keeps_remote_readiness_fenced() {
    let mut app = test_app();
    app.state.identity.auth = AuthState::Authenticated {
        subject: Some(UserId::try_from("01900000-0000-7000-8000-000000000001").expect("user ID")),
        expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    app.backend_compatibility_verified = true;
    app.backend_reconciliation = super::super::BackendReconciliationState::CleanupPending;

    app.finish_initial_collaboration_cleanup().await;

    assert!(app.device_enrollment_retry_after.is_some());
    assert_eq!(
        app.backend_reconciliation,
        super::super::BackendReconciliationState::CleanupPending
    );
    assert!(!app.remote_surfaces_ready());
    assert!(!app.state.identity.account_runtimes.user_events_healthy());
    assert!(
        app.state
            .logs
            .iter()
            .any(|message| message.starts_with("device enrollment deferred:"))
    );
}

#[tokio::test]
async fn identity_reset_is_rejected_while_collaboration_cleanup_is_pending() {
    let mut app = test_app();
    app.state.identity.auth = AuthState::Authenticated {
        subject: Some(UserId::try_from("01900000-0000-7000-8000-000000000001").expect("user ID")),
        expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    app.backend_reconciliation = super::super::BackendReconciliationState::CleanupPending;
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal = None;
    let mut last_auth_event = None;

    apply_auth_message(
        &mut app,
        AuthCommand::IdentityReset,
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .expect("identity reset rejection should publish");

    assert!(app.state.identity.auth.is_authenticated());
    let events = drain_auth_events(&mut rx.auth_rx);
    assert!(
        events.iter().any(|event| matches!(
            event,
            AuthEvent::Error { operation, message }
                if operation == "identity.reset"
                    && message.ends_with(
                        "remote operations are waiting for collaboration cleanup reconciliation"
                    )
        )),
        "unexpected auth events: {events:?}"
    );
}

#[tokio::test]
async fn correlated_resize_rejects_pixel_geometry_that_disagrees_with_grid() {
    let mut app = test_app();
    let session_id = SessionId::new();
    let request_id = uuid::Uuid::now_v7().to_string();
    let mut identity = super::super::tests::resize_identity_for_test(
        request_id.clone(),
        uuid::Uuid::now_v7().to_string(),
        "terminal-1",
        7,
        3,
        24,
        80,
    );
    identity.width_pixels = 801;
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal = None;
    let mut last_auth_event = None;

    apply_terminal_message(
        &mut app,
        TerminalCommand::Resize {
            session_id: session_id.to_string(),
            identity: identity.clone(),
            claim: false,
        },
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .expect("invalid geometry publishes rejection");

    std::assert_matches!(
        drain_terminal_control_events(&mut rx.terminal_control_rx).as_slice(),
        [TerminalEvent::ResizeRejected { identity: rejected, reason, .. }]
            if rejected == &identity && reason.contains("invalid terminal pixel geometry")
    );
    assert!(drain_system_events_after_optional_healthy_snapshot(&mut rx.system_rx).is_empty());
}

#[tokio::test]
async fn correlated_focus_rejects_stale_runtime_incarnation() {
    let mut app = test_app();
    let session_id = SessionId::new();
    let summary = kodosi_domain::session::SessionSummary::new_owned(
        session_id,
        "Claude".to_owned(),
        "focus-test".to_owned(),
        TerminalSize::new(80, 24).expect("size"),
        None,
    );
    app.state.local.sessions.insert(summary);
    let current_incarnation = app
        .state
        .local
        .sessions
        .record(session_id)
        .expect("session")
        .local_incarnation_id;
    let stale_incarnation = loop {
        let candidate = uuid::Uuid::now_v7();
        if candidate != current_incarnation {
            break candidate;
        }
    };
    let request_id = uuid::Uuid::now_v7().to_string();
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal = None;
    let mut last_auth_event = None;

    apply_terminal_message(
        &mut app,
        TerminalCommand::Focus {
            session_id: session_id.to_string(),
            client_id: "client".to_owned(),
            request_id: request_id.clone(),
            expected_runtime_incarnation_id: stale_incarnation.to_string(),
        },
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .expect("focus rejection publishes");

    std::assert_matches!(
        drain_terminal_control_events(&mut rx.terminal_control_rx).as_slice(),
        [TerminalEvent::FocusRejected {
            session_id: rejected_session,
            request_id: rejected_request,
            runtime_incarnation_id,
            reason,
        }] if rejected_session == &session_id.to_string()
            && rejected_request == &request_id
            && runtime_incarnation_id == &stale_incarnation.to_string()
            && reason.contains("stale runtime incarnation")
    );
    assert!(drain_system_events_after_optional_healthy_snapshot(&mut rx.system_rx).is_empty());
}

#[tokio::test]
async fn correlated_resize_rejects_stale_runtime_incarnation_before_dispatch() {
    let mut app = test_app();
    let session_id = SessionId::new();
    let summary = kodosi_domain::session::SessionSummary::new_owned(
        session_id,
        "Claude".to_owned(),
        "resize-test".to_owned(),
        TerminalSize::new(80, 24).expect("size"),
        None,
    );
    app.state.local.sessions.insert(summary);
    let current_incarnation = app
        .state
        .local
        .sessions
        .record(session_id)
        .expect("session")
        .local_incarnation_id;
    let stale_incarnation = loop {
        let candidate = uuid::Uuid::now_v7();
        if candidate != current_incarnation {
            break candidate;
        }
    };
    let request_id = uuid::Uuid::now_v7().to_string();
    let identity = super::super::tests::resize_identity_for_test(
        request_id.clone(),
        stale_incarnation.to_string(),
        "terminal-1",
        7,
        3,
        30,
        100,
    );
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal = None;
    let mut last_auth_event = None;

    apply_terminal_message(
        &mut app,
        TerminalCommand::Resize {
            session_id: session_id.to_string(),
            identity: identity.clone(),
            claim: false,
        },
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .expect("stale resize rejection publishes");

    std::assert_matches!(
        drain_terminal_control_events(&mut rx.terminal_control_rx).as_slice(),
        [TerminalEvent::ResizeRejected {
            session_id: rejected_session,
            identity: rejected,
            reason,
        }] if rejected_session == &session_id.to_string()
            && rejected == &identity
            && reason.contains("stale runtime incarnation")
    );
    assert!(drain_system_events_after_optional_healthy_snapshot(&mut rx.system_rx).is_empty());
}

#[tokio::test]
async fn correlated_remote_resize_is_rejected_as_owner_controlled() {
    let mut app = test_app();
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let summary = kodosi_domain::session::SessionSummary::new_remote(
        session_id,
        "Remote".to_owned(),
        "Owner".to_owned(),
        Some(
            kodosi_domain::ids::UserId::try_from("11111111-1111-1111-1111-111111111111")
                .expect("valid test user id"),
        ),
        ShareScope::Friends,
        AccessLevel::Inject,
        TerminalSize::new(80, 24).expect("size"),
    );
    app.state.discovery.replace_remote_sessions(
        vec![crate::discovery::RemoteSessionRecord {
            summary,
            incarnation_id: Some(incarnation_id),
            room_id: None,
            connection_state: None,
            connection_reason: None,
            access_state: None,
            access_reason: None,
            access_issue: None,
            viewer_blocked: false,
            viewer_hidden: false,
        }],
        false,
    );
    let request_id = uuid::Uuid::now_v7().to_string();
    let identity = super::super::tests::resize_identity_for_test(
        request_id.clone(),
        incarnation_id.to_string(),
        "terminal-remote",
        1,
        3,
        30,
        100,
    );
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal = None;
    let mut last_auth_event = None;

    apply_terminal_message(
        &mut app,
        TerminalCommand::Resize {
            session_id: session_id.to_string(),
            identity: identity.clone(),
            claim: false,
        },
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .expect("remote resize rejection publishes");

    std::assert_matches!(
        drain_terminal_control_events(&mut rx.terminal_control_rx).as_slice(),
        [TerminalEvent::ResizeRejected {
            session_id: rejected_session,
            identity: rejected,
            reason,
        }] if rejected_session == &session_id.to_string()
            && rejected == &identity
            && reason.contains("owner-controlled")
    );
    assert!(drain_system_events_after_optional_healthy_snapshot(&mut rx.system_rx).is_empty());
}

#[tokio::test]
async fn owner_remote_resize_waits_for_exact_relay_result_before_receipt() {
    let mut app = test_app();
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let account_user_id =
        UserId::try_from("01900000-0000-7000-8000-000000000001").expect("user ID");
    app.state.identity.auth = AuthState::Authenticated {
        subject: Some(account_user_id),
        expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    let mut summary = kodosi_domain::session::SessionSummary::new_remote(
        session_id,
        "Remote".to_owned(),
        "You".to_owned(),
        Some(account_user_id),
        ShareScope::Friends,
        AccessLevel::Approve,
        TerminalSize::new(80, 24).expect("size"),
    );
    summary.role = kodosi_domain::session::SessionRole::Owner;
    app.state.discovery.replace_remote_sessions(
        vec![crate::discovery::RemoteSessionRecord {
            summary,
            incarnation_id: Some(incarnation_id),
            room_id: None,
            connection_state: Some(kodosi_domain::lifecycle::ConnectionState::Connected),
            connection_reason: None,
            access_state: Some(kodosi_domain::lifecycle::RemoteSessionAccessState::Ready),
            access_reason: None,
            access_issue: None,
            viewer_blocked: false,
            viewer_hidden: false,
        }],
        false,
    );
    let (command_tx, mut command_rx) = tokio::sync::mpsc::channel(1);
    let relay_generation = app.state.remote.session_relays.attach_for_test(
        session_id,
        CancellationToken::new(),
        command_tx,
        ShareScope::Friends,
        kodosi_backend_client::session_relay::RemoteRelayMode::OwnerParticipant,
        kodosi_domain::lifecycle::ConnectionState::Connected,
        tokio::spawn(std::future::pending::<()>()),
    );
    let request_id = uuid::Uuid::now_v7().to_string();
    let identity = super::super::tests::resize_identity_for_test(
        request_id.clone(),
        incarnation_id.to_string(),
        "terminal-owner",
        5,
        3,
        30,
        100,
    );
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal = None;
    let mut last_auth_event = None;

    apply_terminal_message(
        &mut app,
        TerminalCommand::Resize {
            session_id: session_id.to_string(),
            identity: identity.clone(),
            claim: true,
        },
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .expect("owner resize dispatches");

    std::assert_matches!(
        command_rx.recv().await,
        Some(kodosi_backend_client::session_relay::SessionRelayCommand::OwnerResize {
            action_id,
            rows: 30,
            cols: 100,
            claim: true,
            ..
        }) if action_id == request_id
    );
    assert!(drain_terminal_control_events(&mut rx.terminal_control_rx).is_empty());

    app.handle_session_event(
        crate::session_runtime::events::RuntimeSessionEvent::RemoteActionResult {
            id: session_id,
            action_id: request_id.clone(),
            request_id: Some(request_id.clone()),
            request_generation: None,
            status: kodosi_domain::lifecycle::RemoteActionStatus::Accepted,
            relay_generation,
        },
    );

    std::assert_matches!(
        app.state.runtime_outbox.drain_terminal_control().as_slice(),
        [TerminalEvent::ResizeApplied {
            session_id: result_session_id,
            identity: applied,
        }] if result_session_id == &session_id.to_string() && applied == &identity
    );
}

#[tokio::test]
async fn stale_blur_after_authoritative_removal_is_idempotent() {
    let mut app = test_app();
    let session_id = SessionId::new();
    let retired_incarnation = uuid::Uuid::now_v7();
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal = None;
    let mut last_auth_event = None;

    apply_terminal_message(
        &mut app,
        TerminalCommand::Blur {
            session_id: session_id.to_string(),
            client_id: "desktop".to_owned(),
            expected_runtime_incarnation_id: retired_incarnation.to_string(),
        },
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .expect("stale blur is already settled");

    assert!(drain_terminal_control_events(&mut rx.terminal_control_rx).is_empty());
    assert!(drain_system_events_after_optional_healthy_snapshot(&mut rx.system_rx).is_empty());
}

#[tokio::test]
async fn invalid_terminal_session_id_publishes_system_error() {
    let mut app = test_app();
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal: Option<session_catalog::RefreshFingerprint> = None;
    let mut last_auth_event: Option<AuthEvent> = None;

    apply_terminal_message(
        &mut app,
        TerminalCommand::InputBytes {
            session_id: "not-a-session-id".to_owned(),
            bytes: b"x".to_vec(),
            expected_runtime_incarnation_id: uuid::Uuid::nil().to_string(),
            subscription_id: None,
            subscription_generation: None,
        },
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .unwrap_or_else(|error| panic!("terminal validation error should be published: {error}"));

    std::assert_matches!(
        drain_system_events_after_optional_healthy_snapshot(&mut rx.system_rx).as_slice(),
        [SystemEvent::Error { message, context: None }]
            if message.contains("sessionId") && message.contains("invalid")
    );
}

#[tokio::test]
async fn system_shutdown_cancels_runtime_and_exits_after_flush() {
    let mut app = test_app();
    let (tx, _rx) = runtime_event_channels();
    let cancellation = CancellationToken::new();
    let mut last_signal: Option<session_catalog::RefreshFingerprint> = None;
    let mut last_auth_event: Option<AuthEvent> = None;

    let should_exit = apply_system_message(
        &mut app,
        SystemCommand::Shutdown,
        &tx,
        &cancellation,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .unwrap_or_else(|error| panic!("shutdown should flush without error: {error}"));

    assert!(should_exit);
    assert!(cancellation.is_cancelled());
}

#[tokio::test]
async fn runtime_owns_correlated_steer_queue_query_and_cancel() {
    use kodosi_domain::{
        session::{SessionState, SessionSummary},
        terminal::TerminalSize,
    };

    let directory = tempfile::tempdir().expect("steering tempdir");
    let mut app = test_app();
    app.state.steering = crate::runtime::steering::SteeringState::load_at(
        directory.path().join("pending-steers.json"),
    )
    .expect("test steering state");
    let session_id = SessionId::new();
    let mut summary = SessionSummary::new_owned(
        session_id,
        "Claude".to_owned(),
        "steering-test".to_owned(),
        TerminalSize::new(120, 40).expect("valid size"),
        None,
    );
    summary.state = SessionState::Running;
    app.state.local.sessions.insert(summary);
    let (tx, mut rx) = runtime_event_channels();
    let cancellation = CancellationToken::new();
    let mut last_signal = None;
    let mut last_auth_event = None;

    let incarnation_id = app
        .state
        .local
        .sessions
        .record(session_id)
        .expect("local session record")
        .local_incarnation_id;
    let semantic_request_id = uuid::Uuid::now_v7();
    apply_system_message(
        &mut app,
        SystemCommand::SemanticSend {
            request_id: semantic_request_id.to_string(),
            session_id: session_id.to_string(),
            incarnation_id: incarnation_id.to_string(),
            mode: crate::SemanticSendMode::Queue,
            text: "update the docs".to_owned(),
        },
        &tx,
        &cancellation,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .expect("semantic send command");
    let pending = app.query_steers(session_id, None);
    assert_eq!(pending.len(), 1);
    std::assert_matches!(
        recv_agent_event(&mut rx.agent_intel_rx).await,
        Some(AgentIntelEvent::Reply { request_id, .. }) if request_id == semantic_request_id.to_string()
    );
    std::assert_matches!(
        recv_agent_event(&mut rx.agent_intel_rx).await,
        Some(AgentIntelEvent::SteerState {
            transition: crate::SteerTransition::Queued,
            ..
        })
    );

    apply_system_message(
        &mut app,
        SystemCommand::QuerySteer {
            request_id: "query-1".to_owned(),
            session_id: session_id.to_string(),
            semantic_request_id: None,
        },
        &tx,
        &cancellation,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .expect("query steer command");
    std::assert_matches!(
        recv_agent_event(&mut rx.agent_intel_rx).await,
        Some(AgentIntelEvent::Reply { request_id, payload })
            if request_id == "query-1"
                && payload.as_array().is_some_and(|entries| entries.len() == 1)
    );

    apply_system_message(
        &mut app,
        SystemCommand::CancelSteer {
            request_id: "cancel-1".to_owned(),
            session_id: session_id.to_string(),
            steer_id: pending[0].steer_id.clone(),
        },
        &tx,
        &cancellation,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .expect("cancel steer command");
    let completed = app.query_steers(session_id, None);
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].request_id, semantic_request_id.to_string());
    assert_eq!(
        completed[0].delivery_state,
        crate::SteerDeliveryState::Cancelled
    );
}

fn seed_remote_pending_permission(
    app: &mut Runtime,
    session_id: SessionId,
    account_user_id: UserId,
    incarnation_id: uuid::Uuid,
    tool_use_id: &str,
) {
    app.state.identity.auth = AuthState::Authenticated {
        subject: Some(account_user_id),
        expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    let summary = kodosi_domain::session::SessionSummary::new_remote(
        session_id,
        "Remote".to_owned(),
        "Owner".to_owned(),
        Some(account_user_id),
        ShareScope::Friends,
        AccessLevel::Approve,
        TerminalSize::default(),
    );
    app.state.discovery.replace_remote_sessions(
        vec![crate::discovery::RemoteSessionRecord {
            summary,
            incarnation_id: Some(incarnation_id),
            room_id: None,
            connection_state: None,
            connection_reason: None,
            access_state: None,
            access_reason: None,
            access_issue: None,
            viewer_blocked: false,
            viewer_hidden: false,
        }],
        false,
    );
    assert!(
        app.state
            .agent_intel
            .permission_decisions
            .activate_remote(
                crate::agent_intel::permission_decision_registry::PendingKey {
                    session_id,
                    session_incarnation_id: incarnation_id,
                    tool_use_id: tool_use_id.to_owned(),
                },
                7,
                1,
                crate::agent_intel::permission_decision_registry::PendingPermissionMetadata {
                    tool_name: "Bash".to_owned(),
                    tool_input: serde_json::Value::Null,
                    deadline_at_ms: 2,
                    risk: crate::ApprovalRisk::Unknown,
                },
            )
            .changed()
    );
}

#[tokio::test]
async fn remote_permission_dispatch_emits_sending_state_with_tool_correlation() {
    let mut app = test_app();
    let session_id = SessionId::new();
    let account_user_id =
        UserId::try_from("01900000-0000-7000-8000-000000000001").expect("user ID");
    let incarnation_id = uuid::Uuid::now_v7();
    seed_remote_pending_permission(
        &mut app,
        session_id,
        account_user_id,
        incarnation_id,
        "tool-remote",
    );
    let (command_tx, mut command_rx) =
        tokio::sync::mpsc::channel::<kodosi_backend_client::session_relay::SessionRelayCommand>(1);
    app.state.remote.session_relays.attach_for_test(
        session_id,
        CancellationToken::new(),
        command_tx,
        ShareScope::Friends,
        kodosi_backend_client::session_relay::RemoteRelayMode::SharedParticipant,
        kodosi_domain::lifecycle::ConnectionState::Connected,
        tokio::spawn(std::future::pending::<()>()),
    );
    let (tx, mut rx) = runtime_event_channels();
    let cancellation = CancellationToken::new();
    let mut last_signal = None;
    let mut last_auth_event = None;

    apply_system_message(
        &mut app,
        SystemCommand::AllowPendingPermissionRequest {
            session_id: session_id.to_string(),
            session_incarnation_id: incarnation_id.to_string(),
            tool_use_id: "tool-remote".to_owned(),
            request_generation: 7,
        },
        &tx,
        &cancellation,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .expect("remote permission command should dispatch");

    std::assert_matches!(
        tokio::time::timeout(std::time::Duration::from_secs(2), command_rx.recv()).await,
        Ok(Some(kodosi_backend_client::session_relay::SessionRelayCommand::PermissionDecision {
            request_id,
            ..
        })) if request_id == "tool-remote"
    );
    std::assert_matches!(
        try_recv_agent_event(&mut rx.agent_intel_rx),
        Ok(AgentIntelEvent::RemotePermissionDecisionState {
            session_id: result_session_id,
            session_incarnation_id,
            tool_use_id,
            request_generation: 7,
            phase: crate::RemotePermissionDecisionPhase::Sending,
            status: None,
            message: None,
        }) if result_session_id == session_id.to_string()
            && session_incarnation_id == incarnation_id.to_string()
            && tool_use_id == "tool-remote"
    );

    let retry = apply_system_message(
        &mut app,
        SystemCommand::AllowPendingPermissionRequest {
            session_id: session_id.to_string(),
            session_incarnation_id: incarnation_id.to_string(),
            tool_use_id: "tool-remote".to_owned(),
            request_generation: 7,
        },
        &tx,
        &cancellation,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await;
    std::assert_matches!(
        retry,
        Err(crate::AppError::Unsupported { reason })
            if reason == "remote permission request is no longer actionable"
    );
    assert!(command_rx.try_recv().is_err());
    assert_eq!(
        app.remote_permission_actions
            .replayable_for_incarnation(&account_user_id.to_string(), session_id, incarnation_id,)
            .len(),
        1
    );
}

#[tokio::test]
async fn offline_remote_permission_is_durable_and_remains_sending() {
    let mut app = test_app();
    let session_id = SessionId::new();
    let account_user_id =
        UserId::try_from("01900000-0000-7000-8000-000000000001").expect("user ID");
    let incarnation_id = uuid::Uuid::now_v7();
    seed_remote_pending_permission(
        &mut app,
        session_id,
        account_user_id,
        incarnation_id,
        "tool-offline",
    );
    let (tx, mut rx) = runtime_event_channels();
    let cancellation = CancellationToken::new();
    let mut last_signal = None;
    let mut last_auth_event = None;

    apply_system_message(
        &mut app,
        SystemCommand::AllowPendingPermissionRequest {
            session_id: session_id.to_string(),
            session_incarnation_id: incarnation_id.to_string(),
            tool_use_id: "tool-offline".to_owned(),
            request_generation: 7,
        },
        &tx,
        &cancellation,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .expect("durable offline permission command");

    assert_eq!(
        app.remote_permission_actions
            .replayable_for_incarnation(&account_user_id.to_string(), session_id, incarnation_id,)
            .len(),
        1
    );
    let snapshot = app.state.agent_intel.permission_decisions.snapshot();
    assert_eq!(snapshot.requests.len(), 1);
    assert_eq!(
        snapshot.requests[0].decision_phase,
        crate::PendingPermissionDecisionPhase::Sending
    );
    std::assert_matches!(
        try_recv_agent_event(&mut rx.agent_intel_rx),
        Ok(AgentIntelEvent::RemotePermissionDecisionState {
            tool_use_id,
            request_generation: 7,
            phase: crate::RemotePermissionDecisionPhase::Sending,
            status: None,
            ..
        }) if tool_use_id == "tool-offline"
    );
    assert!(drain_system_events_after_optional_healthy_snapshot(&mut rx.system_rx).is_empty());

    let action_id = app.remote_permission_actions.replayable_for_incarnation(
        &account_user_id.to_string(),
        session_id,
        incarnation_id,
    )[0]
    .action_id
    .clone();
    let (command_tx, mut command_rx) =
        tokio::sync::mpsc::channel::<kodosi_backend_client::session_relay::SessionRelayCommand>(1);
    app.state.remote.session_relays.attach_for_test(
        session_id,
        CancellationToken::new(),
        command_tx,
        ShareScope::Friends,
        kodosi_backend_client::session_relay::RemoteRelayMode::SharedParticipant,
        kodosi_domain::lifecycle::ConnectionState::Connected,
        tokio::spawn(std::future::pending::<()>()),
    );
    crate::runtime::remote_sessions::replay_remote_permission_actions(&mut app, session_id);
    std::assert_matches!(
        command_rx.recv().await,
        Some(kodosi_backend_client::session_relay::SessionRelayCommand::PermissionDecision {
            action_id: replayed_action_id,
            request_id,
            request_generation: 7,
            ..
        }) if replayed_action_id == action_id && request_id == "tool-offline"
    );
}

#[tokio::test]
async fn snapshot_refresh_defers_replay_to_next_cycle() {
    let mut app = test_app();
    app.state.set_available_rooms(vec![RoomListEntry {
        id: "room-1".to_owned(),
        name: "Room".to_owned(),
        slug: "room".to_owned(),
    }]);
    app.state.room_catalog_loaded = true;

    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal: Option<session_catalog::RefreshFingerprint> = None;
    let mut last_auth_event: Option<AuthEvent> = None;

    publish_state_snapshot(&app, &tx, &mut last_signal, true, true)
        .await
        .unwrap_or_else(|error| panic!("initial publish should succeed: {error}"));
    drain_terminal_control_events(&mut rx.terminal_control_rx);
    drain_session_events(&mut rx.sessions_rx);

    apply_session_message(
        &mut app,
        SessionCommand::SnapshotRefresh,
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .unwrap_or_else(|error| panic!("snapshot refresh should succeed: {error}"));

    assert!(
        drain_session_events(&mut rx.sessions_rx).is_empty(),
        "snapshot refresh must not publish before the deferred cycle"
    );
    let immediate_messages = drain_terminal_control_events(&mut rx.terminal_control_rx);
    assert!(
        immediate_messages.is_empty(),
        "snapshot refresh should defer; got {} messages",
        immediate_messages.len()
    );
    std::assert_matches!(
        drain_auth_events(&mut rx.auth_rx).as_slice(),
        [AuthEvent::Required {
            reason: AuthRequiredReason::SignedOut,
            ..
        }]
    );
    assert!(
        app.state.snapshot_refresh_pending,
        "snapshot_refresh_pending flag should be set"
    );

    let force = std::mem::take(&mut app.state.snapshot_refresh_pending);
    if force && let Some(previous) = last_signal.as_mut() {
        previous.sessions.clear();
        previous.rooms = None;
    }
    publish_state_snapshot(&app, &tx, &mut last_signal, force, true)
        .await
        .unwrap_or_else(|error| panic!("deferred publish should succeed: {error}"));

    let messages = drain_session_events(&mut rx.sessions_rx);
    assert_eq!(messages.len(), 2, "expected session list and room replay");
    std::assert_matches!(
        messages.first(),
        Some(SessionEvent::List { sessions }) if sessions.is_empty()
    );
    std::assert_matches!(
        messages.get(1),
        Some(SessionEvent::RoomList { rooms })
            if rooms.len() == 1
                && rooms[0].id == "room-1"
                && rooms[0].slug == "room"
    );
}

#[tokio::test]
async fn invalid_semantic_fields_are_rejected_before_steering_state_changes() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let mut app = test_app();
    app.state.steering = crate::runtime::steering::SteeringState::load_at(
        directory.path().join("pending-steers.json"),
    )
    .expect("test steering state");
    let session_id = kodosi_domain::ids::SessionId::new();
    let (tx, mut rx) = runtime_event_channels();
    let cancellation = CancellationToken::new();
    let mut last_signal = None;
    let mut last_auth_event = None;

    apply_system_message(
        &mut app,
        SystemCommand::SemanticSend {
            request_id: "550e8400-e29b-41d4-a716-446655440000".to_owned(),
            session_id: session_id.to_string(),
            incarnation_id: "01900000-0000-7000-8000-000000000002".to_owned(),
            mode: crate::SemanticSendMode::Queue,
            text: "must not queue".to_owned(),
        },
        &tx,
        &cancellation,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .expect("validation rejection should be delivered as an event");

    assert!(app.query_steers(session_id, None).is_empty());
    std::assert_matches!(
        recv_agent_event(&mut rx.agent_intel_rx).await,
        Some(AgentIntelEvent::Error { request_id, message, .. })
            if request_id == "550e8400-e29b-41d4-a716-446655440000"
                && message.contains("requestId")
                && message.contains("UUIDv7")
    );
}

#[tokio::test]
async fn device_command_validation_errors_publish_to_devices_lane() {
    let mut app = test_app();
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal: Option<session_catalog::RefreshFingerprint> = None;
    let mut last_auth_event: Option<AuthEvent> = None;

    apply_devices_message(
        &mut app,
        DeviceCommand::Revoke {
            device_id: " ".to_owned(),
        },
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .unwrap_or_else(|error| panic!("device command should publish validation error: {error}"));

    let messages = drain_devices_events(&mut rx.devices_rx);
    std::assert_matches!(
        messages.as_slice(),
        [DeviceEvent::Error { operation, message, .. }]
            if operation == "revoke" && message == "deviceId cannot be empty"
    );
    std::assert_matches!(
        drain_session_events(&mut rx.sessions_rx).as_slice(),
        [SessionEvent::List { .. }]
    );
    std::assert_matches!(
        drain_auth_events(&mut rx.auth_rx).as_slice(),
        [AuthEvent::Required {
            reason: AuthRequiredReason::SignedOut,
            ..
        }]
    );
}

#[tokio::test]
async fn device_link_approve_errors_carry_the_normalized_user_code() {
    let mut app = test_app();
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal: Option<session_catalog::RefreshFingerprint> = None;
    let mut last_auth_event: Option<AuthEvent> = None;

    apply_devices_message(
        &mut app,
        DeviceCommand::LinkApprove {
            user_code: " abcd-efgh ".to_owned(),
        },
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .unwrap_or_else(|error| panic!("device link approve should publish an error event: {error}"));

    let messages = drain_devices_events(&mut rx.devices_rx);
    std::assert_matches!(
        messages.as_slice(),
        [DeviceEvent::Error {
            user_code: Some(user_code),
            operation,
            ..
        }] if operation == "link.approve" && user_code == "ABCD-EFGH"
    );
}

#[tokio::test]
async fn non_canonical_user_code_is_rejected_before_any_backend_hop() {
    let mut app = test_app();
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal: Option<session_catalog::RefreshFingerprint> = None;
    let mut last_auth_event: Option<AuthEvent> = None;

    apply_devices_message(
        &mut app,
        DeviceCommand::LinkApprove {
            user_code: "ABCD-EFGH-IJKL".to_owned(),
        },
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .unwrap_or_else(|error| panic!("device link approve should publish an error event: {error}"));

    let messages = drain_devices_events(&mut rx.devices_rx);
    std::assert_matches!(
        messages.as_slice(),
        [DeviceEvent::Error {
            user_code: Some(_),
            operation,
            message,
        }] if operation == "link.approve"
            && message.contains("userCode is invalid")
            && message.contains("BCDF-2345")
    );
}

#[tokio::test]
async fn device_refresh_while_signed_out_is_silent() {
    let mut app = test_app();
    app.state.identity.auth = AuthState::SignedOut;
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal: Option<session_catalog::RefreshFingerprint> = None;
    let mut last_auth_event: Option<AuthEvent> = None;

    apply_devices_message(
        &mut app,
        DeviceCommand::Refresh,
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .unwrap_or_else(|error| panic!("device refresh should not error when signed out: {error}"));

    assert!(drain_devices_events(&mut rx.devices_rx).is_empty());
    std::assert_matches!(
        drain_session_events(&mut rx.sessions_rx).as_slice(),
        [SessionEvent::List { .. }]
    );
    let auth_events = drain_auth_events(&mut rx.auth_rx);
    assert!(
        !auth_events.iter().any(|event| matches!(
            event,
            AuthEvent::Required {
                reason: AuthRequiredReason::Expired,
                ..
            }
        )),
        "signed-out refresh should not trigger Expired transition; got {auth_events:?}"
    );
}

#[tokio::test]
async fn friends_refresh_while_signed_out_emits_empty_snapshot() {
    let mut app = test_app();
    app.state.identity.auth = AuthState::SignedOut;
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal: Option<session_catalog::RefreshFingerprint> = None;
    let mut last_auth_event: Option<AuthEvent> = None;

    apply_friends_message(
        &mut app,
        FriendsCommand::Refresh,
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .unwrap_or_else(|error| panic!("friends refresh should not error when signed out: {error}"));

    let friends_events = drain_friends_events(&mut rx.friends_rx);
    std::assert_matches!(
            friends_events.as_slice(),
            [FriendsEvent::Snapshot {
                friends,
                incoming,
                outgoing,
                request_id: None,
            }]
                if friends.is_empty() && incoming.is_empty() && outgoing.is_empty()
        ,
        "expected single empty snapshot; got {friends_events:?}"
    );
    let auth_events = drain_auth_events(&mut rx.auth_rx);
    assert!(
        !auth_events.iter().any(|event| matches!(
            event,
            AuthEvent::Required {
                reason: AuthRequiredReason::Expired,
                ..
            }
        )),
        "signed-out refresh should not trigger Expired; got {auth_events:?}"
    );
}

#[tokio::test]
async fn room_refresh_while_signed_out_is_silent() {
    let mut app = test_app();
    app.state.identity.auth = AuthState::SignedOut;
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal: Option<session_catalog::RefreshFingerprint> = None;
    let mut last_auth_event: Option<AuthEvent> = None;

    apply_room_message(
        &mut app,
        RoomCommand::Refresh,
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .unwrap_or_else(|error| panic!("room refresh should not error when signed out: {error}"));

    assert!(
        drain_room_events(&mut rx.room_rx).is_empty(),
        "account-scoped room events require an authenticated account origin"
    );
    let auth_events = drain_auth_events(&mut rx.auth_rx);
    assert!(
        !auth_events.iter().any(|event| matches!(
            event,
            AuthEvent::Required {
                reason: AuthRequiredReason::Expired,
                ..
            }
        )),
        "signed-out refresh should not trigger Expired; got {auth_events:?}"
    );
}

#[test]
fn recovered_member_removal_invalidates_room_keys_before_terminal_success() {
    let mut app = test_app();
    let owner = "01900000-0000-7000-8000-000000000001";
    let room_id = "01900000-0000-7000-8000-000000000002";
    let removed_user_id = "01900000-0000-7000-8000-000000000003";
    let remaining_user_id = "01900000-0000-7000-8000-000000000004";
    app.state.identity.auth = AuthState::Authenticated {
        subject: Some(UserId::try_from(owner).expect("owner ID")),
        expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    let session_id = SessionId::new();
    app.state.sharing.shared_sessions.insert(
        session_id,
        SharedSessionState::new(
            "backend-session".to_owned(),
            uuid::Uuid::from_u128(1),
            "owner-secret".to_owned(),
            ShareScope::Room,
            Some(SharedRoom {
                id: room_id.to_owned(),
                name: "Room".to_owned(),
            }),
            Some([9; 32]),
            Some(1),
        ),
    );
    let roster_body = BASE64.encode(
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "roomId": room_id,
            "generation": 8,
            "ownerUserId": owner,
            "memberUserIds": [owner, remaining_user_id],
            "signerDeviceId": "owner-device",
            "issuedAtMs": 1
        }))
        .expect("roster body"),
    );
    let target = crate::runtime::room_mutations::RoomMutationTarget::RemoveMember {
        room_id: room_id.to_owned(),
        user_id: removed_user_id.to_owned(),
        base_roster_generation: 7,
        desired_roster_generation: 8,
        roster_body,
        roster_signature: BASE64.encode([1_u8; 8]),
        roster_signer_device_id: "owner-device".to_owned(),
    };
    let prepared = crate::runtime::room_mutations::PreparedRoomMutation::new(
        uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000010").expect("mutation ID"),
        owner.to_owned(),
        crate::runtime::room_mutations::RoomMutationIntent::from_target(&target, None),
        target,
    )
    .expect("prepared mutation");

    assert_eq!(
        apply_recovered_room_mutation_security_effects(&mut app, &prepared)
            .expect("first recovery"),
        Some(room_id.to_owned())
    );
    assert_eq!(
        apply_recovered_room_mutation_security_effects(&mut app, &prepared)
            .expect("idempotent recovery"),
        Some(room_id.to_owned())
    );
    assert!(
        app.state
            .sharing
            .shared_sessions
            .get(session_id)
            .expect("shared session")
            .session_key()
            .is_none()
    );
    assert_eq!(
        app.state.pending_work.drain_host_key_rotations(),
        vec![session_id]
    );
}

#[tokio::test]
async fn explicit_room_mutation_recovery_publishes_all_retained_rows_losslessly() {
    let mut app = test_app();
    let account = "01900000-0000-7000-8000-000000000001";
    app.state.identity.auth = AuthState::Authenticated {
        subject: Some(UserId::try_from(account).expect("user ID")),
        expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    let directory = tempfile::tempdir().expect("room ledger directory");
    app.room_mutations = crate::runtime::room_mutation_ledger::RoomMutationLedger::load_at(
        directory.path().join("room-mutations.json"),
    )
    .expect("room ledger");
    let mut expected = std::collections::BTreeSet::new();
    for index in 0..256 {
        let mutation_id = uuid::Uuid::now_v7();
        expected.insert(mutation_id.to_string());
        let target = crate::runtime::room_mutations::RoomMutationTarget::AssignTask {
            room_id: "01900000-0000-7000-8000-000000000010".to_owned(),
            task_id: format!("01900000-0000-7000-8000-{index:012}"),
            expected_task_revision: i64::from(index),
            session_id: None,
            session_incarnation_id: None,
        };
        app.room_mutations
            .put(
                crate::runtime::room_mutations::PreparedRoomMutation::new(
                    mutation_id,
                    account.to_owned(),
                    crate::runtime::room_mutations::RoomMutationIntent::from_target(&target, None),
                    target,
                )
                .expect("prepared mutation"),
            )
            .expect("retain mutation");
    }
    let (tx, mut rx) = runtime_event_channels();
    let consumer = tokio::spawn(async move {
        let mut received = std::collections::BTreeSet::new();
        for _ in 0..256 {
            let envelope = rx.room_rx.recv().await.expect("recovered room event");
            let RoomEvent::MutationRecovered { request_id, .. } = envelope.event else {
                panic!("expected mutation recovery");
            };
            assert!(received.insert(request_id), "duplicate recovery event");
        }
        received
    });

    publish_recovered_room_mutations(&app, &tx)
        .await
        .expect("publish all recoveries");
    let received = consumer.await.expect("recovery consumer");

    assert_eq!(received, expected);
    assert_eq!(app.room_mutations.entries(account).unwrap().len(), 256);
}

#[tokio::test]
async fn unavailable_room_ledger_rejects_mutation_without_stopping_runtime() {
    let mut app = test_app();
    app.state.identity.auth = AuthState::Authenticated {
        subject: Some(UserId::try_from("01900000-0000-7000-8000-000000000001").expect("user ID")),
        expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    app.room_mutations =
        crate::runtime::room_mutation_ledger::RoomMutationLedger::unavailable_for_test(
            std::path::PathBuf::from("/tmp/retained-room-ledger.json"),
            "invalid retained bytes",
        );
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal: Option<session_catalog::RefreshFingerprint> = None;
    let mut last_auth_event: Option<AuthEvent> = None;

    apply_room_message(
        &mut app,
        RoomCommand::TaskAssign {
            room_id: "01900000-0000-7000-8000-000000000010".to_owned(),
            task_id: "01900000-0000-7000-8000-000000000011".to_owned(),
            expected_task_revision: 1,
            session_id: None,
            session_incarnation_id: None,
            request_id: "01900000-0000-7000-8000-000000000012".to_owned(),
        },
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .expect("degraded room capability must not stop the runtime");

    let events = drain_room_events(&mut rx.room_rx);
    assert!(matches!(
        events.as_slice(),
        [
            RoomEvent::Error { message, .. },
            RoomEvent::ActionResult {
                status: RoomActionStatus::Failed,
                message: Some(result_message),
                ..
            }
        ] if message.contains("operator repair") && result_message.contains("operator repair")
    ));

    apply_room_message(
        &mut app,
        RoomCommand::ChatPost {
            room_id: "room-1".to_owned(),
            body: String::new(),
            author_session_id: None,
            recipient_session_ids: Vec::new(),
            recipient_user_ids: Vec::new(),
            request_id: Some("req-room-2".to_owned()),
        },
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .expect("runtime must continue handling room commands");
    assert!(!drain_room_events(&mut rx.room_rx).is_empty());
}

#[tokio::test]
async fn correlated_room_validation_failure_emits_action_result() {
    let mut app = test_app();
    app.state.identity.auth = AuthState::Authenticated {
        subject: Some(UserId::try_from("01900000-0000-7000-8000-000000000001").expect("user ID")),
        expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal: Option<session_catalog::RefreshFingerprint> = None;
    let mut last_auth_event: Option<AuthEvent> = None;

    apply_room_message(
        &mut app,
        RoomCommand::ChatPost {
            room_id: "room-1".to_owned(),
            body: String::new(),
            author_session_id: None,
            recipient_session_ids: Vec::new(),
            recipient_user_ids: Vec::new(),
            request_id: Some("req-room-1".to_owned()),
        },
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .expect("validation failure is reported on the room lane");

    let events = drain_room_events(&mut rx.room_rx);
    std::assert_matches!(
        events.as_slice(),
        [
            RoomEvent::Error {
                room_id: Some(room_id),
                operation,
                ..
            },
            RoomEvent::ActionResult {
                request_id,
                operation: result_operation,
                room_id: Some(result_room_id),
                fingerprint: _,
                status: RoomActionStatus::Failed,
                entity_id: None,
                message: Some(_),
            }
        ] if room_id == "room-1"
            && result_room_id == "room-1"
            && operation == "chat.post"
            && result_operation == "chat.post"
            && request_id == "req-room-1"
    );
}

#[tokio::test]
async fn canonical_trust_reset_publishes_only_trust_event() {
    let mut app = test_app();
    app.pin_store
        .bind_to_user("11111111-1111-1111-1111-111111111111")
        .await
        .expect("test pin store should bind to an account");
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal: Option<session_catalog::RefreshFingerprint> = None;
    let mut last_auth_event: Option<AuthEvent> = None;

    apply_trust_message(
        &mut app,
        TrustCommand::Reset {
            request_id: "request-1".to_owned(),
            user_id: "user-1".to_owned(),
        },
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .unwrap_or_else(|error| panic!("trust reset should publish completion: {error}"));

    std::assert_matches!(
        drain_trust_events(&mut rx.trust_rx).as_slice(),
        [TrustEvent::Reset {
            request_id,
            user_id,
            cleared: false,
        }] if request_id == "request-1" && user_id == "user-1"
    );
    assert!(drain_devices_events(&mut rx.devices_rx).is_empty());
}

#[tokio::test]
async fn trust_refresh_echoes_exact_request_correlation() {
    let mut app = test_app();
    app.pin_store
        .bind_to_user("11111111-1111-1111-1111-111111111111")
        .await
        .expect("test pin store should bind to an account");
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal: Option<session_catalog::RefreshFingerprint> = None;
    let mut last_auth_event: Option<AuthEvent> = None;

    apply_trust_message(
        &mut app,
        TrustCommand::Refresh {
            request_id: "refresh-1".to_owned(),
        },
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .unwrap_or_else(|error| panic!("trust refresh should publish: {error}"));

    std::assert_matches!(
        drain_trust_events(&mut rx.trust_rx).as_slice(),
        [TrustEvent::Snapshot {
            request_id,
            pins,
        }] if request_id == "refresh-1" && pins.is_empty()
    );
}

#[tokio::test]
async fn trust_reset_error_preserves_exact_request_and_user_correlation() {
    let mut app = test_app();
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal: Option<session_catalog::RefreshFingerprint> = None;
    let mut last_auth_event: Option<AuthEvent> = None;

    apply_trust_message(
        &mut app,
        TrustCommand::Reset {
            request_id: "request-error".to_owned(),
            user_id: " ".to_owned(),
        },
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .unwrap_or_else(|error| panic!("trust error should publish: {error}"));

    std::assert_matches!(
        drain_trust_events(&mut rx.trust_rx).as_slice(),
        [TrustEvent::Error {
            request_id: Some(request_id),
            user_id: Some(user_id),
            operation,
            ..
        }] if request_id == "request-error" && user_id == " " && operation == "reset"
    );
}

#[test]
fn maintenance_backs_off_only_after_active_window() {
    use super::{MAINTENANCE_ACTIVE_WINDOW, next_maintenance_interval};
    use std::time::Duration;

    let active = Duration::from_millis(120);
    let idle = Duration::from_secs(1);

    assert_eq!(
        next_maintenance_interval(Duration::ZERO, active, idle),
        active
    );
    assert_eq!(
        next_maintenance_interval(
            MAINTENANCE_ACTIVE_WINDOW - Duration::from_millis(1),
            active,
            idle
        ),
        active
    );
    assert_eq!(
        next_maintenance_interval(MAINTENANCE_ACTIVE_WINDOW, active, idle),
        idle
    );
    assert_eq!(
        next_maintenance_interval(
            MAINTENANCE_ACTIVE_WINDOW + Duration::from_secs(10),
            active,
            idle
        ),
        idle
    );
}

#[tokio::test]
async fn a_scope_command_is_accepted_before_the_transition_runs() {
    let mut app = test_app();
    let (tx, mut rx) = runtime_event_channels();
    let missing = SessionId::new().to_string();
    let mut last_signal = None;
    let mut last_auth_event = None;

    let expected_request_id = uuid::Uuid::now_v7().to_string();
    apply_session_message(
        &mut app,
        SessionCommand::SetShareScope {
            request_id: expected_request_id.clone(),
            session_id: missing.clone(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
            scope: ShareScope::MyDevices,
            room_id: None,
        },
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .unwrap_or_else(|error| panic!("scope command should publish a verdict: {error}"));

    let events = drain_session_events(&mut rx.sessions_rx);
    let accepted = events
        .iter()
        .position(|event| matches!(event, SessionEvent::ScopeAccepted { .. }))
        .expect("acceptance must be published");
    let failed = events
        .iter()
        .position(|event| matches!(event, SessionEvent::Error { .. }))
        .expect("the verdict must be published");
    assert!(
        accepted < failed,
        "acceptance has to reach the caller before the transition can fail"
    );

    std::assert_matches!(
        &events[accepted],
        SessionEvent::ScopeAccepted {
            request_id,
            session_id,
            expected_runtime_incarnation_id: _,
            scope,
            room_id: None,
            budget_ms,
        } if request_id == &expected_request_id
            && *session_id == missing
            && *scope == ShareScope::MyDevices


            && *budget_ms == 505_000
    );
    std::assert_matches!(
        &events[failed],
        SessionEvent::Error {
            operation,
            session_id: Some(session_id),
            request_id: Some(request_id),
            ..
        } if operation == "session.scope"
            && *session_id == missing
            && request_id == &expected_request_id
    );
}

#[tokio::test]
async fn an_unshare_publishes_the_tighter_budget_it_owns() {
    let mut app = test_app();
    let (tx, mut rx) = runtime_event_channels();
    let missing = SessionId::new().to_string();
    let mut last_signal = None;
    let mut last_auth_event = None;

    apply_session_message(
        &mut app,
        SessionCommand::SetShareScope {
            request_id: uuid::Uuid::now_v7().to_string(),
            session_id: missing,
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
            scope: ShareScope::JustMe,
            room_id: None,
        },
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .unwrap_or_else(|error| panic!("scope command should publish a verdict: {error}"));

    let events = drain_session_events(&mut rx.sessions_rx);
    std::assert_matches!(
        events
            .iter()
            .find(|event| matches!(event, SessionEvent::ScopeAccepted { .. }))
            .expect("acceptance must be published"),


        SessionEvent::ScopeAccepted { budget_ms, .. } if *budget_ms == 5_000
    );
}

#[tokio::test]
async fn a_malformed_scope_command_is_rejected_without_being_accepted() {
    let mut app = test_app();
    let (tx, mut rx) = runtime_event_channels();
    let mut last_signal = None;
    let mut last_auth_event = None;

    apply_session_message(
        &mut app,
        SessionCommand::SetShareScope {
            request_id: "req-bad".to_owned(),
            session_id: SessionId::new().to_string(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
            scope: ShareScope::Room,
            room_id: None,
        },
        &tx,
        &mut last_signal,
        &mut last_auth_event,
    )
    .await
    .unwrap_or_else(|error| panic!("invalid scope command should emit a lane error: {error}"));

    let events = drain_session_events(&mut rx.sessions_rx);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, SessionEvent::ScopeAccepted { .. })),
        "a command that never runs must not claim a budget"
    );
    std::assert_matches!(
        events
            .iter()
            .find(|event| matches!(event, SessionEvent::Error { .. }))
            .expect("the rejection must be published"),
        SessionEvent::Error {
            operation,
            request_id: Some(request_id),
            ..
        } if operation == "session.scope" && request_id == "req-bad"
    );
}

mod shutdown {
    use std::time::Duration;

    use tokio::{io::AsyncReadExt, sync::mpsc, sync::watch, time};
    use tokio_util::sync::CancellationToken;

    use crate::{
        config::AppConfig,
        runtime::{
            Runtime,
            access_mutations::{
                PreparedSessionAccessMutation, PreparedSessionAccessMutationState,
                SessionAccessMutationTarget,
            },
            runtime_loop::{HeadlessRuntimeLaneReceivers, RuntimeLaneReceivers, runtime_loop},
        },
        runtime_event_bus::runtime_event_channels,
        terminal_transport,
    };
    use kodosi_domain::ids::SessionId;

    fn test_app() -> Runtime {
        let mut config = AppConfig::default();
        config.auth.keyring_service = "kodosi.test".to_owned();

        config.runtime.tick_interval_ms = 50;
        Runtime::with_dependencies(
            config.clone(),
            CancellationToken::new(),
            crate::runtime::RuntimeDependencies::isolated(&config)
                .expect("isolated runtime dependencies"),
        )
        .unwrap_or_else(|error| panic!("test app should construct: {error}"))
    }

    const NO_HANG: Duration = Duration::from_mins(1);

    struct Lanes {
        receivers: RuntimeLaneReceivers,
        _senders: LaneSenders,
    }

    #[expect(dead_code, reason = "held only to keep the command lanes open")]
    struct LaneSenders {
        terminal: mpsc::Sender<crate::TerminalCommand>,
        system: mpsc::Sender<crate::AccountScopedCommand<crate::SystemCommand>>,
        auth: mpsc::Sender<crate::AccountScopedCommand<crate::AuthCommand>>,
        friends: mpsc::Sender<crate::AccountScopedCommand<crate::FriendsCommand>>,
        devices: mpsc::Sender<crate::AccountScopedCommand<crate::DeviceCommand>>,
        trust: mpsc::Sender<crate::AccountScopedCommand<crate::TrustCommand>>,
        room: mpsc::Sender<crate::AccountScopedCommand<crate::RoomCommand>>,
        sessions: mpsc::Sender<crate::AccountScopedCommand<crate::SessionCommand>>,
        agent_intel: mpsc::Sender<crate::AccountScopedCommand<crate::AgentIntelCommand>>,
        hub_subscribe: mpsc::Sender<terminal_transport::TerminalHubCommand>,
        remote_status: tokio::sync::watch::Receiver<crate::RemoteCommandStatus>,
        #[cfg(feature = "cli")]
        snapshot_rpc: mpsc::Sender<crate::SnapshotRpcRequest>,
        #[cfg(feature = "cli")]
        device_rpc: mpsc::Sender<crate::DeviceRpcRequest>,
    }

    fn lanes() -> Lanes {
        let (terminal, terminal_rx) = mpsc::channel(8);
        let (system, system_rx) = mpsc::channel(8);
        let (auth, auth_rx) = mpsc::channel(8);
        let (friends, friends_rx) = mpsc::channel(8);
        let (devices, devices_rx) = mpsc::channel(8);
        let (trust, trust_rx) = mpsc::channel(8);
        let (room, room_rx) = mpsc::channel(8);
        let (sessions, sessions_rx) = mpsc::channel(8);
        let (agent_intel, agent_intel_rx) = mpsc::channel(8);
        let (hub_subscribe, hub_subscribe_rx) = mpsc::channel(8);
        let (remote_status, remote_status_rx) =
            tokio::sync::watch::channel(crate::RemoteCommandStatus {
                account_user_id: None,
                account_epoch: 1,
                #[cfg(feature = "cli")]
                remote_operations_ready: false,
            });
        #[cfg(feature = "cli")]
        let (device_rpc, device_rpc_rx) = mpsc::channel(8);
        #[cfg(feature = "cli")]
        let (snapshot_rpc, snapshot_rpc_rx) = mpsc::channel(8);
        Lanes {
            receivers: RuntimeLaneReceivers {
                terminal: terminal_rx,
                system: system_rx,
                auth: auth_rx,
                friends: friends_rx,
                devices: devices_rx,
                trust: trust_rx,
                room: room_rx,
                sessions: sessions_rx,
                agent_intel: agent_intel_rx,
                hub_subscribe: hub_subscribe_rx,
                remote_status,
                headless: {
                    #[cfg(feature = "cli")]
                    {
                        HeadlessRuntimeLaneReceivers {
                            snapshot_rpc: snapshot_rpc_rx,
                            device_rpc: device_rpc_rx,
                        }
                    }
                    #[cfg(not(feature = "cli"))]
                    {
                        HeadlessRuntimeLaneReceivers
                    }
                },
            },
            _senders: LaneSenders {
                terminal,
                system,
                auth,
                friends,
                devices,
                trust,
                room,
                sessions,
                agent_intel,
                hub_subscribe,
                remote_status: remote_status_rx,
                #[cfg(feature = "cli")]
                snapshot_rpc,
                #[cfg(feature = "cli")]
                device_rpc,
            },
        }
    }

    #[tokio::test]
    async fn hanging_access_receipt_lookup_does_not_block_runtime_shutdown() {
        use kodosi_domain::{auth::AuthState, ids::UserId};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("receipt listener");
        let address = listener.local_addr().expect("receipt address");
        let (accepted_tx, accepted_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept receipt request");
            let mut request = [0_u8; 4096];
            let read = socket
                .read(&mut request)
                .await
                .expect("read receipt request");
            assert!(read > 0);
            accepted_tx.send(()).ok();
            std::future::pending::<()>().await;
        });

        let account = uuid::Uuid::now_v7().to_string();
        let mut config = AppConfig::default();
        config.auth.keyring_service = format!("kodosi.test.{}", uuid::Uuid::now_v7());
        config.backend.api = Some(format!("http://{address}"));
        config.runtime.tick_interval_ms = 10;
        let mut app = Runtime::with_dependencies(
            config.clone(),
            CancellationToken::new(),
            crate::runtime::RuntimeDependencies::isolated(&config)
                .expect("isolated runtime dependencies"),
        )
        .expect("runtime");
        app.initialize().await.expect("runtime startup initializes");
        app.state.identity.auth = AuthState::Authenticated {
            subject: Some(UserId::try_from(account.as_str()).expect("user")),
            expires_at: ::time::OffsetDateTime::now_utc() + ::time::Duration::hours(1),
        };
        app.state
            .identity
            .set_account_epoch_for_test(crate::session_runtime::events::AccountEpoch::for_test(7));
        app.set_remote_surfaces_ready_for_test();
        let session = uuid::Uuid::now_v7();
        let incarnation = uuid::Uuid::now_v7();
        let mut prepared = PreparedSessionAccessMutation::new(
            uuid::Uuid::now_v7(),
            account,
            7,
            SessionId::parse_field(&session.to_string(), "sessionId").expect("session"),
            incarnation,
            session.to_string(),
            incarnation,
            SessionAccessMutationTarget::Leave,
        )
        .expect("mutation");
        prepared.state = PreparedSessionAccessMutationState::Attempting;
        app.access_mutations
            .put(prepared)
            .expect("persist mutation");

        let lanes = lanes();
        let (events_tx, _events_rx) = runtime_event_channels();
        let cancellation = CancellationToken::new();
        let loop_handle = tokio::spawn(runtime_loop(
            app,
            lanes.receivers,
            events_tx,
            cancellation.clone(),
        ));

        time::timeout(Duration::from_secs(5), accepted_rx)
            .await
            .expect("receipt request should start")
            .expect("receipt server should observe request");
        cancellation.cancel();
        let outcome = time::timeout(Duration::from_millis(500), loop_handle)
            .await
            .expect("runtime actor must not wait for the hanging receipt lookup")
            .expect("runtime loop task should not panic");
        assert!(outcome.is_ok());
        server.abort();
    }

    #[tokio::test]
    async fn a_live_host_relay_is_torn_down_when_the_runtime_stops() {
        let mut app = test_app();
        app.initialize().await.expect("runtime startup initializes");
        let session_id = SessionId::new();
        let relay_cancel = CancellationToken::new();
        let relay_task = tokio::spawn(std::future::pending::<()>());
        let relay_abort = relay_task.abort_handle();
        let (pending_permissions, _pending_permissions_rx) = watch::channel(None);
        app.state.sharing.host_relays.attach(
            session_id,
            relay_cancel.clone(),
            pending_permissions,
            tokio::sync::mpsc::channel(1).0,
            tokio::sync::mpsc::channel(1).0,
            tokio::sync::mpsc::channel(1).0,
            relay_task,
        );
        assert!(app.state.sharing.host_relays.active(session_id));

        let lanes = lanes();
        let (events_tx, _events_rx) = runtime_event_channels();
        let cancellation = CancellationToken::new();
        let loop_handle = tokio::spawn(runtime_loop(
            app,
            lanes.receivers,
            events_tx,
            cancellation.clone(),
        ));

        cancellation.cancel();
        let outcome = time::timeout(NO_HANG, loop_handle)
            .await
            .expect("the runtime loop must stop inside its own budget")
            .expect("the runtime loop task should not panic");

        assert!(outcome.is_ok(), "orderly shutdown should not error");
        assert!(
            relay_cancel.is_cancelled(),
            "a live relay must be told the host is going away"
        );

        time::timeout(NO_HANG, async {
            while !relay_abort.is_finished() {
                time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("a relay that ignores cancellation must still be aborted");
    }
}
