use super::remote_semantic_transition;
use kodosi_domain::{
    auth::AuthState,
    ids::{SessionId, UserId},
    lifecycle::StopReason,
};
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;

use crate::{
    config::AppConfig,
    host_protocol::RoomAgentDeliveryState,
    runtime::{
        Runtime,
        tests::{local_coordinator_origin, seed_local_coordinator_origin},
    },
    session_runtime::events::RuntimeSessionEvent,
    terminal_transport::{
        TerminalCapability, TerminalCloseReason, TerminalControlFrame, TerminalSurface,
    },
};

fn test_app() -> Runtime {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .expect("test app should construct")
}

fn handle_waiting_for_adapter_delivery(app: &mut Runtime, session_id: SessionId) {
    let origin = seed_local_coordinator_origin(app, session_id);
    app.handle_session_event(RuntimeSessionEvent::RoomAgentDeliveryState {
        id: session_id,
        local_incarnation_id: origin.local_incarnation_id,
        state: RoomAgentDeliveryState::WaitingForAdapter,
        event_id: None,
        detail: Some("Waiting for the session-scoped Copilot extension".to_owned()),
    });
}

#[test]
fn relay_delivery_unknown_maps_to_terminal_semantic_transition() {
    assert_eq!(
        remote_semantic_transition(
            kodosi_backend_client::session_relay::wire::RelaySemanticOutcome::DeliveryUnknown
        ),
        crate::host_protocol::SteerTransition::DeliveryUnknown
    );
}

#[test]
fn signed_out_agent_delivery_does_not_queue_an_account_room_event() {
    let mut app = test_app();
    let session_id = SessionId::new();

    handle_waiting_for_adapter_delivery(&mut app, session_id);

    assert!(
        app.state.runtime_outbox.drain_room().is_empty(),
        "a local adapter state must not create account-scoped output while signed out"
    );
}

#[test]
fn authenticated_agent_delivery_queues_the_current_account_room_event() {
    let mut app = test_app();
    app.state
        .identity
        .advance_account_epoch()
        .expect("test account epoch should advance");
    app.state.identity.auth = AuthState::Authenticated {
        subject: Some(
            UserId::try_from("11111111-1111-1111-1111-111111111111")
                .expect("test user ID should parse"),
        ),
        expires_at: OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    let session_id = SessionId::new();

    handle_waiting_for_adapter_delivery(&mut app, session_id);

    std::assert_matches!(
        app.state.runtime_outbox.drain_room().as_slice(),
        [crate::host_protocol::RoomEvent::AgentDelivery {
            session_id: queued_session_id,
            ..
        }] if queued_session_id == &session_id.to_string()
    );
}

#[test]
fn terminal_output_observed_does_not_duplicate_coordinator_publish() {
    let mut app = test_app();
    let sid = SessionId::new();
    seed_local_coordinator_origin(&mut app, sid);

    assert!(app.terminal_hub.open_local_incarnation(
        sid,
        local_coordinator_origin(&app, sid).local_incarnation_id,
        0
    ));
    let mut handle = app
        .terminal_hub
        .register(
            sid,
            TerminalSurface::HeadlessCapture,
            TerminalCapability::ReadOnly,
        )
        .expect("session should be live");

    let payload = bytes::Bytes::from_static(b"hello\r\n");
    app.terminal_hub.publish(sid, payload.clone());
    app.handle_session_event(RuntimeSessionEvent::TerminalOutputObserved {
        origin: local_coordinator_origin(&app, sid),
        data: payload,
    });

    let frame = handle
        .data_rx
        .try_recv()
        .expect("subscriber should receive frame");
    assert_eq!(frame.bytes.as_ref(), b"hello\r\n");
    assert_eq!(frame.sequence, 0);
    assert!(handle.data_rx.try_recv().is_err());
}

#[test]
fn terminal_output_observed_sequences_are_ordered() {
    let mut app = test_app();
    let sid = SessionId::new();
    seed_local_coordinator_origin(&mut app, sid);

    assert!(app.terminal_hub.open_local_incarnation(
        sid,
        local_coordinator_origin(&app, sid).local_incarnation_id,
        0
    ));
    let mut handle = app
        .terminal_hub
        .register(
            sid,
            TerminalSurface::HeadlessCapture,
            TerminalCapability::ReadOnly,
        )
        .expect("session should be live");

    for payload in [
        bytes::Bytes::from_static(b"first\r\n"),
        bytes::Bytes::from_static(b"second\r\n"),
    ] {
        app.terminal_hub.publish(sid, payload.clone());
        app.handle_session_event(RuntimeSessionEvent::TerminalOutputObserved {
            origin: local_coordinator_origin(&app, sid),
            data: payload,
        });
    }

    let f0 = handle.data_rx.try_recv().expect("frame 0");
    let f1 = handle.data_rx.try_recv().expect("frame 1");
    assert_eq!(f0.sequence, 0);
    assert_eq!(f1.sequence, 1);
    assert_eq!(f0.bytes.as_ref(), b"first\r\n");
    assert_eq!(f1.bytes.as_ref(), b"second\r\n");
}

#[test]
fn stopped_event_closes_hub_session() {
    let mut app = test_app();
    let sid = SessionId::new();
    let origin = seed_local_coordinator_origin(&mut app, sid);
    assert!(
        app.terminal_hub
            .open_local_incarnation(sid, origin.local_incarnation_id, 0)
    );
    let mut handle = app
        .terminal_hub
        .register(sid, TerminalSurface::Desktop, TerminalCapability::ReadOnly)
        .expect("session should be live");

    app.handle_session_event(RuntimeSessionEvent::Stopped {
        origin,
        reason: StopReason::ProcessExited,
    });

    let ctrl = handle
        .control_rx
        .try_recv()
        .expect("subscriber should receive Closed control frame");
    std::assert_matches!(
        ctrl,
        TerminalControlFrame::Closed {
            reason: TerminalCloseReason::SessionEnded,
            final_sequence: 0,
        },
        "expected SessionEnded close reason, got {ctrl:?}"
    );
    assert!(app.terminal_hub.next_sequence(sid).is_none());
}

#[test]
fn failed_event_closes_hub_session() {
    let mut app = test_app();
    let sid = SessionId::new();
    let origin = seed_local_coordinator_origin(&mut app, sid);
    assert!(
        app.terminal_hub
            .open_local_incarnation(sid, origin.local_incarnation_id, 0)
    );
    let mut handle = app
        .terminal_hub
        .register(
            sid,
            TerminalSurface::HeadlessCapture,
            TerminalCapability::ReadOnly,
        )
        .expect("session should be live");

    app.handle_session_event(RuntimeSessionEvent::Failed {
        origin,
        message: "PTY died".to_owned(),
    });

    let ctrl = handle
        .control_rx
        .try_recv()
        .expect("subscriber should receive Closed control frame on failure");
    std::assert_matches!(ctrl, TerminalControlFrame::Closed { reason: TerminalCloseReason::IoError(ref message), final_sequence: 0 } if message == "PTY died",
        "expected failed-session close reason, got {ctrl:?}"
    );
    assert!(app.terminal_hub.next_sequence(sid).is_none());
}

#[test]
fn terminal_output_observed_still_updates_activity_via_state_reducer() {
    use kodosi_domain::{session::SessionSummary, terminal::TerminalSize};
    use time::OffsetDateTime;

    let mut app = test_app();
    let sid = SessionId::new();

    let mut summary = SessionSummary::new_owned(
        sid,
        "test".to_owned(),
        "kodosi".to_owned(),
        TerminalSize::new(80, 24).expect("valid size"),
        None,
    );
    summary.last_update = OffsetDateTime::UNIX_EPOCH;
    app.state.local.sessions.insert(summary);

    let before = app
        .state
        .local
        .sessions
        .record(sid)
        .map(|r| r.summary.last_update)
        .expect("record should exist");

    assert!(app.terminal_hub.open_local_incarnation(
        sid,
        local_coordinator_origin(&app, sid).local_incarnation_id,
        0
    ));
    let _handle = app
        .terminal_hub
        .register(
            sid,
            TerminalSurface::HeadlessCapture,
            TerminalCapability::ReadOnly,
        )
        .expect("session should be live");

    app.handle_session_event(RuntimeSessionEvent::TerminalOutputObserved {
        origin: local_coordinator_origin(&app, sid),
        data: bytes::Bytes::from_static(b"x"),
    });

    let after = app
        .state
        .local
        .sessions
        .record(sid)
        .map(|r| r.summary.last_update)
        .expect("record should still exist");
    assert!(
        after >= before,
        "last_update should be refreshed on output: before={before}, after={after}"
    );
}

#[test]
fn permission_activation_and_resolution_use_snapshots() {
    use crate::agent_intel::risk::ApprovalRisk;
    use kodosi_domain::{session::SessionSummary, terminal::TerminalSize};

    let mut app = test_app();
    let sid = SessionId::new();
    app.state.local.sessions.insert(SessionSummary::new_owned(
        sid,
        "test".to_owned(),
        "kodosi".to_owned(),
        TerminalSize::new(80, 24).expect("valid size"),
        None,
    ));
    let local_incarnation_id = app
        .state
        .local
        .sessions
        .record(sid)
        .expect("permission test session should exist")
        .local_incarnation_id;
    let key = crate::agent_intel::permission_decision_registry::PendingKey {
        session_id: sid,
        session_incarnation_id: local_incarnation_id,
        tool_use_id: "toolu_x".to_owned(),
    };
    let _receiver = app
        .state
        .agent_intel
        .permission_decisions
        .park(key.clone())
        .expect("permission parks");
    assert!(app.state.agent_intel.permission_decisions.stage_metadata(
        &key,
        crate::agent_intel::permission_decision_registry::PendingPermissionMetadata {
            tool_name: "Bash".to_owned(),
            tool_input: serde_json::json!({"command": "rm -rf /tmp/x"}),
            deadline_at_ms: 42_000,
            risk: ApprovalRisk::Destructive,
        },
    ));

    app.handle_session_event(RuntimeSessionEvent::PendingPermissionRequest {
        id: sid,
        local_incarnation_id,
        tool_use_id: "toolu_x".to_owned(),
    });
    let activated = app.state.runtime_outbox.drain_agent_intel();
    std::assert_matches!(
        activated.as_slice(),
        [crate::AgentIntelEvent::PendingPermissionsSnapshot { requests, .. }]
            if requests.len() == 1
                && requests[0].session_id == sid.to_string()
                && requests[0].tool_use_id == "toolu_x"
                && requests[0].request_generation == 1
                && requests[0].decision_phase
                    == crate::PendingPermissionDecisionPhase::Actionable
    );

    app.handle_session_event(RuntimeSessionEvent::PermissionResolved {
        id: sid,
        local_incarnation_id,
        tool_use_id: "toolu_x".to_owned(),
    });

    let events = app.state.runtime_outbox.drain_agent_intel();
    std::assert_matches!(
        events.as_slice(),
        [crate::AgentIntelEvent::PendingPermissionsSnapshot { requests, .. }]
            if requests.is_empty()
    );
}

#[test]
fn remote_presentation_is_cached_only_for_plain_consumers() {
    use kodosi_domain::terminal::{
        Revision, TerminalPresentationFrame, TerminalPresentationV2, TerminalScreen, TerminalSize,
    };

    let mut app = test_app();
    let sid = SessionId::new();
    seed_local_coordinator_origin(&mut app, sid);

    assert!(app.terminal_hub.open_local_incarnation(
        sid,
        local_coordinator_origin(&app, sid).local_incarnation_id,
        0
    ));
    let handle = app
        .terminal_hub
        .register(sid, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("session should be live");

    let mut lines = vec![String::new(); 40];
    lines[0] = "line one".to_owned();
    lines[1] = "line two".to_owned();
    let presentation = TerminalPresentationV2::new(
        TerminalSize::new(40, 100).expect("valid size"),
        TerminalScreen::Primary,
        lines,
        0,
        0,
        false,
    )
    .expect("presentation");
    let frame = TerminalPresentationFrame::new(Revision::default(), presentation);

    let relay_generation = app
        .state
        .remote
        .session_relays
        .reserve_generation(sid)
        .expect("test relay generation");
    app.handle_session_event(RuntimeSessionEvent::RemotePlainPresentation {
        id: sid,
        presentation: frame.presentation.clone(),
        relay_generation,
    });

    assert!(
        handle.control_rx.is_empty(),
        "plain presentation must never replace a native Ghostty surface"
    );

    let cached = app
        .remote_terminal
        .cached(sid)
        .expect("presentation should be cached for plain consumers");
    let presentation = cached.presentation.expect("plain presentation");
    assert_eq!(presentation.rows(), 40);
    assert_eq!(presentation.cols(), 100);
    assert_eq!(&presentation.plain_lines[..2], ["line one", "line two"]);
    assert!(presentation.plain_lines[2..].iter().all(String::is_empty));
}

#[tokio::test]
async fn remote_session_relay_exited_clears_cache_and_closes_hub() {
    use kodosi_domain::terminal::{
        Revision, TerminalPresentationFrame, TerminalPresentationV2, TerminalScreen, TerminalSize,
    };

    let mut app = test_app();
    let sid = SessionId::new();
    seed_local_coordinator_origin(&mut app, sid);

    assert!(app.terminal_hub.open_local_incarnation(
        sid,
        local_coordinator_origin(&app, sid).local_incarnation_id,
        0
    ));
    let mut handle = app
        .terminal_hub
        .register(sid, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("session should be live");

    let presentation = TerminalPresentationV2::new(
        TerminalSize::new(24, 80).expect("valid size"),
        TerminalScreen::Primary,
        vec!["screen".to_owned(); 24],
        0,
        0,
        false,
    )
    .expect("presentation");
    let frame = TerminalPresentationFrame::new(Revision::default(), presentation);
    let relay_generation = app.state.remote.session_relays.attach_for_test(
        sid,
        CancellationToken::new(),
        tokio::sync::mpsc::channel(1).0,
        kodosi_domain::permissions::ShareScope::Friends,
        kodosi_backend_client::session_relay::RemoteRelayMode::SharedParticipant,
        kodosi_domain::lifecycle::ConnectionState::Connected,
        tokio::spawn(std::future::pending::<()>()),
    );
    app.handle_session_event(RuntimeSessionEvent::RemotePlainPresentation {
        id: sid,
        presentation: frame.presentation,
        relay_generation,
    });
    let _ = handle.control_rx.try_recv();
    assert!(app.remote_terminal.cached(sid).is_some());

    app.handle_session_event(RuntimeSessionEvent::RemoteSessionRelayExited {
        id: sid,
        relay_generation,
    });

    assert!(app.remote_terminal.cached(sid).is_none());
    let ctrl = handle
        .control_rx
        .try_recv()
        .expect("subscriber should receive Closed on relay exit");
    std::assert_matches!(
        ctrl,
        TerminalControlFrame::Closed {
            reason: TerminalCloseReason::Detached,
            final_sequence: 0,
        },
        "expected Detached close reason, got {ctrl:?}"
    );
    assert!(app.terminal_hub.next_sequence(sid).is_none());
}
