use super::*;
use crate::terminal_transport::{TerminalCapability, TerminalSurface};

fn owned_session_with_screen_lane(
    app: &mut Runtime,
    id: SessionId,
) -> mpsc::Receiver<SessionScreenInstruction> {
    let (screen_tx, screen_rx) = mpsc::channel::<SessionScreenInstruction>(16);
    let (pty_tx, pty_rx) = mpsc::channel::<SessionPtyInstruction>(16);

    std::mem::forget(pty_rx);
    insert_owned_session_for_test(
        app,
        claude_owned_summary(id, "/tmp/kodosi-focus"),
        OwnedSessionHandle {
            senders: SessionSenders::new(Some(screen_tx), Some(pty_tx)),
            cancellation: CancellationToken::new(),
            join_handle: tokio::spawn(std::future::pending::<()>()),
        },
    );
    screen_rx
}

fn drain_screen(rx: &mut mpsc::Receiver<SessionScreenInstruction>) -> Vec<String> {
    let mut seen = Vec::new();
    while let Ok(instruction) = rx.try_recv() {
        seen.push(format!("{instruction:?}"));
    }
    seen
}

#[tokio::test]
async fn a_dropped_connection_releases_the_focus_its_client_left_behind() {
    let mut app = test_app();
    let id = SessionId::new();
    let mut screen_rx = owned_session_with_screen_lane(&mut app, id);

    let handle = app
        .terminal_hub
        .register(id, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("live session should accept a subscriber");
    app.focus_session(id, "client-a".to_owned())
        .await
        .expect("focus should reach the session");
    assert!(
        drain_screen(&mut screen_rx)
            .iter()
            .any(|seen| seen.starts_with("Focus")),
        "focus must reach the emulator before the release is meaningful"
    );

    app.terminal_hub.unregister(id, handle.connection_id);
    app.release_focus_if_disconnected(id).await;

    assert!(
        drain_screen(&mut screen_rx)
            .iter()
            .any(|seen| seen.starts_with("Blur")),
        "the last connection leaving must blur the focus its client left behind"
    );
    assert!(
        app.client_focus.clients(id).is_empty(),
        "a released claim must not survive as a stale client id"
    );
}

#[tokio::test]
async fn focus_survives_while_the_same_client_holds_another_connection() {
    let mut app = test_app();
    let id = SessionId::new();
    let mut screen_rx = owned_session_with_screen_lane(&mut app, id);

    let first = app
        .terminal_hub
        .register(id, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("live session should accept a subscriber");
    let _second = app
        .terminal_hub
        .register(id, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("live session should accept a subscriber");
    app.focus_session(id, "client-a".to_owned())
        .await
        .expect("focus should reach the session");
    drain_screen(&mut screen_rx);

    app.terminal_hub.unregister(id, first.connection_id);
    app.release_focus_if_disconnected(id).await;

    assert!(
        drain_screen(&mut screen_rx).is_empty(),
        "a surviving connection must keep its client's focus"
    );
    assert_eq!(app.client_focus.clients(id), vec!["client-a".to_owned()]);
}

#[tokio::test]
async fn releasing_without_any_focus_sends_nothing() {
    let mut app = test_app();
    let id = SessionId::new();
    let mut screen_rx = owned_session_with_screen_lane(&mut app, id);

    let handle = app
        .terminal_hub
        .register(id, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("live session should accept a subscriber");
    app.terminal_hub.unregister(id, handle.connection_id);
    app.release_focus_if_disconnected(id).await;

    assert!(
        !drain_screen(&mut screen_rx)
            .iter()
            .any(|seen| seen.starts_with("Blur")),
        "an unfocused disconnect must not synthesise a blur"
    );
}

#[tokio::test]
async fn every_client_on_a_lost_connection_is_released() {
    let mut app = test_app();
    let id = SessionId::new();
    let mut screen_rx = owned_session_with_screen_lane(&mut app, id);

    let handle = app
        .terminal_hub
        .register(id, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("live session should accept a subscriber");
    for client in ["client-a", "client-b"] {
        app.focus_session(id, client.to_owned())
            .await
            .expect("focus should reach the session");
    }
    drain_screen(&mut screen_rx);

    app.terminal_hub.unregister(id, handle.connection_id);
    app.release_focus_if_disconnected(id).await;

    assert!(
        app.client_focus.clients(id).is_empty(),
        "no client id may outlive the connection that carried it"
    );
    assert!(
        drain_screen(&mut screen_rx)
            .iter()
            .any(|seen| seen.starts_with("Blur")),
        "the aggregate must fall to blurred once the last client is released"
    );
}

#[tokio::test]
async fn a_stopped_session_drops_its_focus_claims() {
    let mut app = test_app();
    let id = SessionId::new();
    let mut screen_rx = owned_session_with_screen_lane(&mut app, id);

    app.terminal_hub
        .register(id, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("live session should accept a subscriber");
    app.focus_session(id, "client-a".to_owned())
        .await
        .expect("focus should reach the session");
    drain_screen(&mut screen_rx);

    app.handle_session_event(RuntimeSessionEvent::Stopped {
        origin: local_coordinator_origin(&app, id),
        reason: StopReason::UserRequested,
    });

    assert!(
        app.client_focus.clients(id).is_empty(),
        "a stopped session must not keep focus claims for a gone emulator"
    );
}

fn remote_session_in_catalog(app: &mut Runtime, id: SessionId) {
    let summary = SessionSummary::new_remote(
        id,
        "Remote".to_owned(),
        "Owner".to_owned(),
        Some(test_user_id()),
        ShareScope::Friends,
        AccessLevel::Inject,
        TerminalSize::new(120, 40)
            .unwrap_or_else(|error| panic!("terminal size should be valid: {error}")),
    );
    app.state.discovery.replace_remote_sessions(
        vec![RemoteSessionRecord {
            summary,
            incarnation_id: None,
            room_id: None,
            connection_state: Some(ConnectionState::Connected),
            connection_reason: None,
            access_state: None,
            access_reason: None,
            access_issue: None,
            viewer_blocked: false,
            viewer_hidden: false,
        }],
        false,
    );
}

#[tokio::test]
async fn a_transient_catalog_clear_keeps_local_focus_blurrable() {
    let mut app = test_app();
    let local = SessionId::new();
    let mut screen_rx = owned_session_with_screen_lane(&mut app, local);

    app.terminal_hub
        .register(local, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("live session should accept a subscriber");
    app.focus_session(local, "client-a".to_owned())
        .await
        .expect("focus should reach the session");
    drain_screen(&mut screen_rx);

    crate::runtime::discovery::clear_remote_catalog(
        &mut app,
        crate::runtime::discovery::CatalogClearReason::TransientOffline,
    );

    assert_eq!(
        app.client_focus.clients(local),
        vec!["client-a".to_owned()],
        "a transient backend failure must not forget a local session's focus"
    );

    app.blur_session(local, "client-a".to_owned())
        .await
        .expect("blur should reach the session");
    assert!(
        drain_screen(&mut screen_rx)
            .iter()
            .any(|seen| seen.starts_with("Blur")),
        "the surviving claim must let a real blur reach the emulator"
    );
}

#[tokio::test]
async fn a_transient_catalog_clear_still_forgets_remote_focus() {
    let mut app = test_app();
    let remote = SessionId::new();
    remote_session_in_catalog(&mut app, remote);

    let _ = app.client_focus.note_focus(remote, "client-a".to_owned());
    assert!(!app.client_focus.clients(remote).is_empty());

    crate::runtime::discovery::clear_remote_catalog(
        &mut app,
        crate::runtime::discovery::CatalogClearReason::TransientOffline,
    );

    assert!(
        app.client_focus.clients(remote).is_empty(),
        "a remote session dropped from the catalog must not keep a focus claim"
    );
}

#[tokio::test]
async fn an_account_teardown_preserves_local_focus() {
    let mut app = test_app();
    let local = SessionId::new();
    let mut screen_rx = owned_session_with_screen_lane(&mut app, local);

    app.terminal_hub
        .register(local, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("live session should accept a subscriber");
    app.focus_session(local, "client-a".to_owned())
        .await
        .expect("focus should reach the session");
    drain_screen(&mut screen_rx);

    crate::runtime::discovery::clear_remote_catalog(
        &mut app,
        crate::runtime::discovery::CatalogClearReason::AccountTeardown,
    );

    assert_eq!(
        app.client_focus.clients(local),
        ["client-a"],
        "account teardown must retain focus for a surviving local connection"
    );
}
