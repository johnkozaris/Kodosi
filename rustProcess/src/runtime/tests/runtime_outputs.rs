use super::*;

#[test]
fn clipboard_updates_write_to_runtime_clipboard() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    config.permissions.allow_terminal_clipboard_write = true;
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies")
            .with_clipboard(SystemClipboardBridge::recording()),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let session_id = SessionId::new();
    let origin = seed_local_coordinator_origin(&mut app, session_id);

    app.handle_session_event(RuntimeSessionEvent::ClipboardUpdate {
        origin,
        text: "copied text".to_owned(),
    });

    assert_eq!(
        app.clipboard.recorded_texts(),
        vec!["copied text".to_owned()]
    );
    assert!(app.state.runtime_outbox.drain_terminal_control().is_empty());
}

#[test]
fn clipboard_write_reply_is_sent_after_the_runtime_write_succeeds() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    config.permissions.allow_terminal_clipboard_write = true;
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies")
            .with_clipboard(SystemClipboardBridge::recording()),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let session_id = SessionId::new();
    let origin = seed_local_coordinator_origin(&mut app, session_id);
    let (reply, result) = std::sync::mpsc::sync_channel(1);

    app.handle_session_event(RuntimeSessionEvent::ClipboardWriteRequest {
        origin,
        text: "copied synchronously".to_owned(),
        reply,
    });

    assert_eq!(
        result.recv().expect("clipboard result"),
        kodosi_session::ClipboardWriteOutcome::Success
    );
    assert_eq!(
        app.clipboard.recorded_texts(),
        vec!["copied synchronously".to_owned()]
    );
}

#[test]
fn clipboard_update_failures_are_logged() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    config.permissions.allow_terminal_clipboard_write = true;
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies")
            .with_clipboard(SystemClipboardBridge::failing("clipboard busy")),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let session_id = SessionId::new();
    let origin = seed_local_coordinator_origin(&mut app, session_id);
    let (reply, result) = std::sync::mpsc::sync_channel(1);

    app.handle_session_event(RuntimeSessionEvent::ClipboardWriteRequest {
        origin,
        text: "copied text".to_owned(),
        reply,
    });

    assert_eq!(
        result.recv().expect("clipboard result"),
        kodosi_session::ClipboardWriteOutcome::IoError
    );
    assert!(
        app.state
            .logs
            .iter()
            .any(|line| line.contains("failed to update system clipboard: clipboard busy"))
    );
}

#[test]
fn clipboard_update_is_denied_without_explicit_policy() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies")
            .with_clipboard(SystemClipboardBridge::recording()),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let session_id = SessionId::new();
    let origin = seed_local_coordinator_origin(&mut app, session_id);
    let (reply, result) = std::sync::mpsc::sync_channel(1);

    app.handle_session_event(RuntimeSessionEvent::ClipboardWriteRequest {
        origin,
        text: "denied".to_owned(),
        reply,
    });

    assert_eq!(
        result.recv().expect("clipboard result"),
        kodosi_session::ClipboardWriteOutcome::Denied
    );
    assert!(app.clipboard.recorded_texts().is_empty());
    assert!(app.state.logs.iter().any(|line| line.contains(
        "denied terminal-originated clipboard write by local policy"
    )));
}

#[test]
fn terminal_bell_and_title_events_keep_order_and_update_metadata() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let session_id = SessionId::new();
    let summary = kodosi_domain::session::SessionSummary::new_owned(
        session_id,
        "session".to_owned(),
        "runtime".to_owned(),
        kodosi_domain::terminal::TerminalSize::default(),
        None,
    );
    let mut summary = summary;
    summary.working_dir = Some("/repo".to_owned());
    app.state.local.sessions.insert(summary);
    let origin = local_coordinator_origin(&app, session_id);

    app.handle_session_event(RuntimeSessionEvent::TerminalTitleChanged {
        origin,
        title: Some("first".to_owned()),
    });
    app.handle_session_event(RuntimeSessionEvent::TerminalBell { origin });
    app.handle_session_event(RuntimeSessionEvent::TerminalTitleChanged {
        origin,
        title: Some("second".to_owned()),
    });
    app.handle_session_event(RuntimeSessionEvent::TerminalBell { origin });

    assert_eq!(
        app.state
            .local
            .sessions
            .record(session_id)
            .and_then(|record| record.terminal_title.as_deref()),
        Some("second")
    );
    let catalog = crate::runtime::session_catalog::build_session_catalog(&app);
    let Some(crate::host_protocol::SessionListEntry::Local { entry }) = catalog.first() else {
        panic!("local session should be catalogued");
    };
    assert_eq!(
        entry
            .meta
            .as_deref()
            .and_then(|meta| meta.terminal_title.as_deref()),
        Some("second")
    );
    assert_eq!(
        app.state.runtime_outbox.drain_terminal_control(),
        vec![
            TerminalEvent::Title {
                session_id: session_id.to_string(),
                title: Some("first".to_owned()),
            },
            TerminalEvent::Bell {
                session_id: session_id.to_string(),
            },
            TerminalEvent::Title {
                session_id: session_id.to_string(),
                title: Some("second".to_owned()),
            },
            TerminalEvent::Bell {
                session_id: session_id.to_string(),
            },
        ]
    );
}

#[test]
fn terminal_notifications_emit_control_messages() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    let session_id = SessionId::new();
    let origin = seed_local_coordinator_origin(&mut app, session_id);

    app.handle_session_event(RuntimeSessionEvent::TerminalNotification {
        origin,
        title: Some("Build complete".to_owned()),
        body: Some("All tests passed".to_owned()),
    });

    let runtime_events = app.state.runtime_outbox.drain_terminal_control();
    assert_eq!(runtime_events.len(), 1);
    std::assert_matches!(
        runtime_events.first(),
        Some(TerminalEvent::Notification {
            session_id: queued_session_id,
            title,
            body,
        }) if queued_session_id == &session_id.to_string()
            && title.as_deref() == Some("Build complete")
            && body.as_deref() == Some("All tests passed")
    );
}
