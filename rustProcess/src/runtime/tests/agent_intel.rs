use super::*;

fn live_payload(title: &str) -> serde_json::Value {
    serde_json::json!({
        "identity": {
            "agentType": "claude",
            "version": null,
            "model": null,
            "title": title,
            "cwd": "/tmp/kodosi",
            "vendorSessionId": "native-session"
        },
        "lifecycle": "working",
        "workers": {"active": 0, "blocked": 0, "failed": 0, "completed": 0},
        "source": {"kind": "command", "degraded": false}
    })
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
        payload: live_payload("Vendor title"),
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
        }] if emitted_session_id == &session_id.to_string()
            && session_incarnation_id == &incarnation.to_string()
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
        payload: live_payload("Retired"),
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
        payload: live_payload("Retired generation"),
    });
    assert!(app.state.runtime_outbox.drain_agent_intel().is_empty());

    app.handle_session_event(RuntimeSessionEvent::AgentIntelSnapshot {
        id: session_id,
        local_incarnation_id: incarnation,
        generation: current_generation,
        payload: live_payload("Current generation"),
    });
    assert_eq!(app.state.runtime_outbox.drain_agent_intel().len(), 1);
}
