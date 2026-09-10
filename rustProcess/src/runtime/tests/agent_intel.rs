use super::*;

fn live_payload(title: &str) -> ::agent_intel::AgentIntelSnapshot {
    ::agent_intel::AgentIntelSnapshot {
        identity: ::agent_intel::domain::LiveAgentIdentity {
            agent_type: "claude".to_owned(),
            title: Some(title.to_owned()),
            cwd: Some("/tmp/kodosi".to_owned()),
            vendor_session_id: Some("native-session".to_owned()),
            ..::agent_intel::domain::LiveAgentIdentity::default()
        },
        lifecycle: ::agent_intel::domain::AgentLifecycle::Working,
        source: ::agent_intel::domain::AgentSource {
            kind: ::agent_intel::domain::AgentSourceKind::Command,
            degraded: false,
            detail: None,
        },
        ..::agent_intel::AgentIntelSnapshot::default()
    }
}

fn runtime_with_session() -> (Runtime, SessionId) {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .expect("test app should construct");
    let session_id = SessionId::new();
    app.state
        .local
        .sessions
        .insert(claude_owned_summary(session_id, "/tmp/kodosi"));
    (app, session_id)
}

#[test]
fn current_live_snapshot_updates_title_and_publishes() {
    let (mut app, session_id) = runtime_with_session();
    let generation = app
        .state
        .agent_intel
        .registry
        .reserve_generation(session_id);
    let incarnation = app.session_incarnation_id(session_id).expect("incarnation");

    app.handle_session_event(RuntimeSessionEvent::AgentIntelSnapshot {
        generation,
        local_incarnation_id: incarnation,
        id: session_id,
        payload: Box::new(live_payload("Vendor title")),
    });

    assert_eq!(
        app.state
            .local
            .sessions
            .record(session_id)
            .map(|record| record.summary.title.as_str()),
        Some("Vendor title")
    );
    assert_eq!(
        app.state.agent_intel.registry.agent_session_id(session_id),
        Some("native-session")
    );
    std::assert_matches!(
        app.state.runtime_outbox.drain_agent_intel().as_slice(),
        [AgentIntelEvent::Snapshot {
            session_id: emitted_session_id,
            session_incarnation_id,
            ..
        }, AgentIntelEvent::LiveSet {
            revision: 1,
            entries,
            ..
        }] if emitted_session_id == &session_id.to_string()
            && session_incarnation_id == &incarnation.to_string()
            && entries.len() == 1
    );
}

#[test]
fn retired_incarnation_snapshot_is_ignored() {
    let (mut app, session_id) = runtime_with_session();
    let retired_incarnation = app.session_incarnation_id(session_id).expect("first");
    app.state
        .local
        .sessions
        .insert(claude_owned_summary(session_id, "/tmp/kodosi"));
    let generation = app
        .state
        .agent_intel
        .registry
        .reserve_generation(session_id);

    app.handle_session_event(RuntimeSessionEvent::AgentIntelSnapshot {
        id: session_id,
        generation,
        local_incarnation_id: retired_incarnation,
        payload: Box::new(live_payload("Retired")),
    });

    assert!(app.state.runtime_outbox.drain_agent_intel().is_empty());
}

#[test]
fn replaced_generation_cannot_publish() {
    let (mut app, session_id) = runtime_with_session();
    let incarnation = app.session_incarnation_id(session_id).expect("incarnation");
    let retired_generation = app
        .state
        .agent_intel
        .registry
        .reserve_generation(session_id);
    let current_generation = app
        .state
        .agent_intel
        .registry
        .reserve_generation(session_id);

    app.handle_session_event(RuntimeSessionEvent::AgentIntelSnapshot {
        id: session_id,
        local_incarnation_id: incarnation,
        generation: retired_generation,
        payload: Box::new(live_payload("Retired generation")),
    });
    assert!(app.state.runtime_outbox.drain_agent_intel().is_empty());

    app.handle_session_event(RuntimeSessionEvent::AgentIntelSnapshot {
        id: session_id,
        local_incarnation_id: incarnation,
        generation: current_generation,
        payload: Box::new(live_payload("Current generation")),
    });
    assert_eq!(app.state.runtime_outbox.drain_agent_intel().len(), 2);
}

#[test]
fn failed_session_is_removed_from_live_authority() {
    let (mut app, session_id) = runtime_with_session();
    let generation = app
        .state
        .agent_intel
        .registry
        .reserve_generation(session_id);
    let incarnation = app.session_incarnation_id(session_id).expect("incarnation");
    app.handle_session_event(RuntimeSessionEvent::AgentIntelSnapshot {
        id: session_id,
        local_incarnation_id: incarnation,
        generation,
        payload: Box::new(live_payload("Working")),
    });
    app.state.runtime_outbox.drain_agent_intel();

    app.handle_session_event(RuntimeSessionEvent::Failed {
        origin: local_coordinator_origin(&app, session_id),
        message: "coordinator exited".to_owned(),
    });

    std::assert_matches!(
        app.state.runtime_outbox.drain_agent_intel().as_slice(),
        [AgentIntelEvent::Cleared {
            session_id: cleared_session_id,
            session_incarnation_id,
        }, AgentIntelEvent::LiveSet {
            revision: 2,
            entries,
            ..
        }] if cleared_session_id == &session_id.to_string()
            && session_incarnation_id == &incarnation.to_string()
            && entries.is_empty()
    );
}
