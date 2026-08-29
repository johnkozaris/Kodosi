use super::*;
use crate::terminal_transport::{TerminalCapability, TerminalSurface};

fn seed_active_share_transition(app: &mut Runtime, session_id: SessionId) -> (String, uuid::Uuid) {
    let account = authenticate_test_app(app).account_user_id;
    let incarnation = app
        .state
        .local
        .sessions
        .record(session_id)
        .expect("session")
        .local_incarnation_id;
    let audience = crate::runtime::share_transitions::ShareAudience {
        scope: ShareScope::JustMe,
        room_id: None,
    };
    let prepared = crate::runtime::share_transitions::PreparedShareTransition::new(
        uuid::Uuid::now_v7(),
        1,
        account.clone(),
        app.state.identity.account_epoch().value(),
        session_id,
        incarnation,
        audience.clone(),
        audience,
        session_id.to_string(),
        None,
        None,
        None,
    )
    .expect("transition");
    app.share_transitions
        .put(prepared.clone())
        .expect("persist transition");
    app.share_transition_in_flight
        .insert(session_id, prepared.transition_id);
    app.install_share_transition_deadline_for_test(prepared.transition_id);
    (account, prepared.transition_id)
}

fn assert_share_transition_prepared(
    app: &Runtime,
    account: &str,
    session_id: SessionId,
    transition_id: uuid::Uuid,
) {
    assert_eq!(
        app.share_transition_in_flight.get(&session_id),
        Some(&transition_id)
    );
    assert!(matches!(
        app.share_transitions
            .get(account, transition_id)
            .expect("ledger")
            .expect("transition")
            .state,
        crate::runtime::share_transitions::ShareTransitionState::Prepared
    ));
}

#[test]
fn authoritative_stopped_makes_durable_backend_end_obligation_eligible() {
    let mut app = test_app();
    let session_id = SessionId::new();
    app.state
        .local
        .sessions
        .insert(claude_owned_summary(session_id, "/tmp/kodosi"));
    app.state.identity.auth = AuthState::Authenticated {
        subject: Some(
            UserId::try_from("01900000-0000-7000-8000-000000000001").expect("test user ID"),
        ),
        expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    app.backend = kodosi_backend_client::http_client::BackendHttpClient::new(
        &kodosi_backend_client::config::BackendClientConfig {
            api: Some("http://127.0.0.1:9/".to_owned()),
            ..kodosi_backend_client::config::BackendClientConfig::default()
        },
    )
    .expect("backend client");
    let backend_origin = app
        .backend
        .backend_origin()
        .expect("configured backend origin")
        .clone();
    let backend_session_id = session_id.to_string();
    let backend_incarnation_id = uuid::Uuid::now_v7();
    let create_id = uuid::Uuid::now_v7();
    let end_id = uuid::Uuid::now_v7();
    app.collaboration_teardown
        .provision(
            &backend_origin,
            "01900000-0000-7000-8000-000000000001",
            &backend_session_id,
            create_id,
            end_id,
            1,
        )
        .expect("durable teardown obligation");
    app.collaboration_teardown
        .bind_incarnation(create_id, &backend_session_id, backend_incarnation_id)
        .expect("bind backend incarnation");
    app.state.sharing.shared_sessions.insert(
        session_id,
        SharedSessionState::new(
            backend_session_id,
            backend_incarnation_id,
            "owner-secret".to_owned(),
            ShareScope::MyDevices,
            None,
            None,
            None,
        ),
    );
    assert_eq!(app.collaboration_cleanup_health().pending_count, 0);

    app.handle_session_event(RuntimeSessionEvent::Stopped {
        origin: local_coordinator_origin(&app, session_id),
        reason: StopReason::UserRequested,
    });

    assert_eq!(app.collaboration_cleanup_health().pending_count, 1);
    assert!(!app.state.sharing.shared_sessions.contains(session_id));
}

#[test]
fn failed_event_clears_shared_session_state_and_host_status() {
    let (mut app, id) = app_with_shared_cached_session(SessionState::Running);

    app.handle_session_event(RuntimeSessionEvent::Failed {
        origin: local_coordinator_origin(&app, id),
        message: "pty exited".to_owned(),
    });

    assert!(!app.state.sharing.shared_sessions.contains(id));
    assert_eq!(app.state.host_ws_status, ConnectionState::Offline);
    assert_eq!(
        app.state
            .local
            .sessions
            .record(id)
            .map(|record| record.summary.state),
        Some(SessionState::Failed)
    );
}

#[test]
fn failed_event_with_pending_delete_clears_shared_session_state() {
    let (mut app, id) = app_with_shared_cached_session(SessionState::Stopping);
    app.state.pending_work.queue_delete_after_stop(id);

    app.state.apply_session_event(RuntimeSessionEvent::Failed {
        origin: local_coordinator_origin(&app, id),
        message: "stop watchdog failed".to_owned(),
    });

    assert!(!app.state.sharing.shared_sessions.contains(id));
    assert_eq!(app.state.host_ws_status, ConnectionState::Offline);
    assert_eq!(
        app.state
            .local
            .sessions
            .record(id)
            .map(|record| record.summary.state),
        Some(SessionState::Failed)
    );
}
#[test]
fn delete_session_drops_pending_delete_on_immediate_path() {
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
    app.state
        .local
        .sessions
        .insert(claude_owned_summary(session_id, "/tmp/kodosi"));
    app.state.pending_work.queue_delete_after_stop(session_id);

    crate::runtime::local_sessions::lifecycle(&mut app)
        .delete(session_id)
        .unwrap_or_else(|error| panic!("delete_session should succeed: {error}"));

    assert!(
        !app.state.pending_work.promote_delete_after_stop(session_id),
        "delete_session immediate branch must drop queued delete work"
    );
    assert!(
        app.state.local.sessions.record(session_id).is_none(),
        "delete_session should remove the record"
    );
}

#[tokio::test]
async fn duplicate_resumed_create_replays_after_working_directory_disappears() {
    let mut app = test_app();
    let working_directory_root = tempfile::tempdir().expect("working directory");
    let working_directory = working_directory_root
        .path()
        .canonicalize()
        .expect("canonical working directory")
        .to_string_lossy()
        .into_owned();
    let session_id = SessionId::new();
    let request_id = "request-resume".to_owned();
    let resume_source = kodosi_domain::provider_conversation::ProviderConversationIdentity {
        provider: kodosi_domain::provider_conversation::ProviderConversationProvider::Claude,
        native_conversation_id: "conversation".to_owned(),
    };
    let mut summary = claude_owned_summary(session_id, &working_directory);
    summary.title = "Resumed".to_owned();
    app.state.local.sessions.insert(summary);
    let record = app
        .state
        .local
        .sessions
        .record_mut(session_id)
        .expect("session record");
    record.create_request_id = Some(request_id.clone());
    record.resume_source = Some(resume_source.clone());
    drop(working_directory_root);

    let replayed = crate::runtime::local_sessions::lifecycle(&mut app)
        .create(
            Some("Resumed".to_owned()),
            Some(working_directory),
            Some(request_id),
            Some(resume_source),
        )
        .await
        .expect("duplicate create replays from committed identity");

    assert!(replayed);
    assert_eq!(app.state.local.sessions.ids(), &[session_id]);
}

#[tokio::test]
async fn failed_session_event_drains_pending_delete_and_tears_down_side_tasks() {
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
    insert_owned_session_for_test(
        &mut app,
        claude_owned_summary(session_id, "/tmp/kodosi"),
        test_session_handle(),
    );
    app.state
        .local
        .sessions
        .update_state(session_id, SessionState::Stopping);
    app.state.pending_work.queue_delete_after_stop(session_id);
    let host_relay_cancellation = CancellationToken::new();
    let (pending_tx, _pending_rx) = tokio::sync::watch::channel(None);
    app.state.sharing.host_relays.attach(
        session_id,
        host_relay_cancellation.clone(),
        pending_tx,
        tokio::sync::mpsc::channel(1).0,
        tokio::sync::mpsc::channel(1).0,
        tokio::sync::mpsc::channel(1).0,
        tokio::spawn(std::future::pending::<()>()),
    );
    app.state
        .agent_intel
        .registry
        .set_agent_session_id(session_id, Some("claude-session-id"));
    assert!(app.state.sharing.host_relays.active(session_id));

    app.handle_session_event(RuntimeSessionEvent::Failed {
        origin: local_coordinator_origin(&app, session_id),
        message: "coordinator exited".to_owned(),
    });

    assert!(
        !app.state.pending_work.promote_delete_after_stop(session_id),
        "Failed arm must drain queued delete work so the queued delete finalizes \
         (mirrors the Stopped arm)"
    );
    assert!(host_relay_cancellation.is_cancelled());
    assert!(!app.state.sharing.host_relays.active(session_id));
    assert_eq!(
        app.state.agent_intel.registry.agent_session_id(session_id),
        None
    );
    assert!(
        app.state.local.sessions.record(session_id).is_none(),
        "Failed arm must clear the live runtime before draining the queued delete"
    );
}

#[tokio::test]
async fn failed_session_event_without_queued_delete_keeps_coordinator_for_stopped_cleanup() {
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
    insert_owned_session_for_test(
        &mut app,
        claude_owned_summary(session_id, "/tmp/kodosi"),
        test_session_handle(),
    );

    app.handle_session_event(RuntimeSessionEvent::Failed {
        origin: local_coordinator_origin(&app, session_id),
        message: "coordinator failed before final stop".to_owned(),
    });

    assert!(
        app.state.local.owned_session_runtimes.contains(session_id),
        "Failed without queued delete must let the coordinator emit Stopped"
    );
    assert_eq!(
        app.state
            .local
            .sessions
            .record(session_id)
            .map(|record| record.summary.state),
        Some(SessionState::Failed)
    );

    app.handle_session_event(RuntimeSessionEvent::Stopped {
        origin: local_coordinator_origin(&app, session_id),
        reason: StopReason::Failed,
    });

    assert!(!app.state.local.owned_session_runtimes.contains(session_id));
}

#[tokio::test]
async fn failed_reopen_restores_terminal_tombstone_and_preserves_sharing_state() {
    let mut app = test_app();
    let catalog_parent = tempfile::tempdir().expect("catalog parent");
    let catalog_root = catalog_parent.path().join("catalog");
    app.local_catalog =
        crate::runtime::local_control::LocalSessionCatalog::at(catalog_root.clone())
            .expect("test catalog");
    let session_id = SessionId::new();
    let mut summary = claude_owned_summary(session_id, "/tmp/kodosi");
    summary.state = SessionState::Stopped;
    app.state.local.sessions.insert(summary);
    let prior_reason = crate::terminal_transport::TerminalCloseReason::IoError(
        "prior coordinator failure".to_owned(),
    );
    app.terminal_hub.end_session(session_id, &prior_reason);
    app.state.sharing.shared_sessions.insert(
        session_id,
        SharedSessionState::new(
            session_id.to_string(),
            uuid::Uuid::now_v7(),
            "owner-secret".to_owned(),
            ShareScope::MyDevices,
            None,
            None,
            None,
        ),
    );
    std::fs::remove_dir(&catalog_root).expect("remove catalog directory");
    std::fs::write(&catalog_root, b"not a directory").expect("block catalog persistence");

    let result = crate::runtime::local_sessions::lifecycle(&mut app)
        .reopen(session_id)
        .await;

    assert!(result.is_err());
    assert_eq!(
        app.terminal_hub.close_reason(session_id),
        Some(prior_reason)
    );
    assert!(
        app.terminal_hub
            .publish(session_id, bytes::Bytes::from_static(b"late"))
            .is_none()
    );
    assert!(
        app.terminal_hub
            .register(
                session_id,
                TerminalSurface::Desktop,
                TerminalCapability::ReadOnly,
            )
            .is_none()
    );
    assert!(!app.state.local.owned_session_runtimes.contains(session_id));
    assert!(app.state.sharing.shared_sessions.contains(session_id));
}

#[test]
fn all_stale_coordinator_events_are_rejected_after_replacement() {
    let mut app = test_app();
    let session_id = SessionId::new();
    app.state
        .local
        .sessions
        .insert(claude_owned_summary(session_id, "/current"));
    let stale = local_coordinator_origin(&app, session_id);
    let current_incarnation = uuid::Uuid::now_v7();
    set_local_incarnation_for_test(&mut app, session_id, current_incarnation);
    let original_size = app
        .state
        .local
        .sessions
        .record(session_id)
        .expect("session")
        .summary
        .size;
    let candidate_size = TerminalSize::new(33, 111).expect("size");

    let events = [
        RuntimeSessionEvent::TerminalOutputObserved {
            origin: stale,
            data: bytes::Bytes::from_static(b"stale-output"),
        },
        RuntimeSessionEvent::CaptureMetadata {
            origin: stale,
            size: candidate_size,
            working_dir: Some("/stale-capture".to_owned()),
            refresh_working_dir: true,
        },
        RuntimeSessionEvent::RuntimeMetadata {
            origin: stale,
            working_dir: Some("/stale-runtime".to_owned()),
            running_command: Some("stale-command".to_owned()),
            detected_agent: Some("stale-agent".to_owned()),
        },
        RuntimeSessionEvent::WorkingDirChanged {
            origin: stale,
            working_dir: "/stale-cwd".to_owned(),
        },
        RuntimeSessionEvent::ClipboardUpdate {
            origin: stale,
            text: "stale-clipboard".to_owned(),
        },
        RuntimeSessionEvent::TerminalBell { origin: stale },
        RuntimeSessionEvent::TerminalTitleChanged {
            origin: stale,
            title: Some("stale-title".to_owned()),
        },
        RuntimeSessionEvent::TerminalNotification {
            origin: stale,
            title: Some("stale-notification".to_owned()),
            body: Some("stale-body".to_owned()),
        },
        RuntimeSessionEvent::Stopped {
            origin: stale,
            reason: StopReason::UserRequested,
        },
        RuntimeSessionEvent::Failed {
            origin: stale,
            message: "stale-failure".to_owned(),
        },
    ];

    for event in events {
        assert!(!app.handle_session_event(event));
    }

    let record = app
        .state
        .local
        .sessions
        .record(session_id)
        .expect("session");
    assert_eq!(record.local_incarnation_id, current_incarnation);
    assert_eq!(record.summary.state, SessionState::Running);
    assert_eq!(record.summary.working_dir.as_deref(), Some("/current"));
    assert_eq!(record.summary.size, original_size);
    assert!(record.summary.running_command.is_none());
    assert!(record.terminal_title.is_none());
    assert!(app.state.runtime_outbox.drain_terminal_control().is_empty());
}

#[tokio::test]
async fn stale_terminal_events_preserve_current_incarnation_owned_state() {
    for stale_event in ["stopped", "failed"] {
        let mut app = test_app();
        let session_id = SessionId::new();
        insert_owned_session_for_test(
            &mut app,
            claude_owned_summary(session_id, "/current"),
            test_session_handle(),
        );
        let stale = local_coordinator_origin(&app, session_id);
        assert!(app.terminal_hub.end_local_incarnation(
            stale,
            &crate::terminal_transport::TerminalCloseReason::SessionEnded,
        ));
        let current_incarnation = uuid::Uuid::now_v7();
        set_local_incarnation_for_test(&mut app, session_id, current_incarnation);
        assert!(
            app.terminal_hub
                .open_local_incarnation(session_id, current_incarnation, 0,)
        );
        let mut subscriber = app
            .terminal_hub
            .register(
                session_id,
                TerminalSurface::Desktop,
                TerminalCapability::ReadOnly,
            )
            .expect("current hub authority");
        app.state
            .sharing
            .shared_sessions
            .insert(session_id, test_shared_session_state());
        let host_relay_cancellation = CancellationToken::new();
        let host_generation = app
            .state
            .sharing
            .host_relays
            .allocate_generation()
            .expect("allocate host relay generation");
        app.state
            .sharing
            .host_relays
            .claim_generation(session_id, host_generation)
            .expect("claim host relay generation");
        app.state.sharing.host_relays.attach(
            session_id,
            host_relay_cancellation.clone(),
            tokio::sync::watch::channel(None).0,
            tokio::sync::mpsc::channel(1).0,
            tokio::sync::mpsc::channel(1).0,
            tokio::sync::mpsc::channel(1).0,
            tokio::spawn(std::future::pending::<()>()),
        );
        app.state
            .agent_intel
            .registry
            .set_agent_session_id(session_id, Some("current-agent-session"));
        let _ = app
            .client_focus
            .note_focus(session_id, "current-client".to_owned());
        app.state.pending_work.queue_delete_after_stop(session_id);

        let event = if stale_event == "stopped" {
            RuntimeSessionEvent::Stopped {
                origin: stale,
                reason: StopReason::UserRequested,
            }
        } else {
            RuntimeSessionEvent::Failed {
                origin: stale,
                message: "stale failure".to_owned(),
            }
        };
        assert!(!app.handle_session_event(event));

        let record = app
            .state
            .local
            .sessions
            .record(session_id)
            .expect("current record");
        assert_eq!(record.local_incarnation_id, current_incarnation);
        assert_eq!(record.summary.state, SessionState::Running);
        assert!(app.state.local.owned_session_runtimes.contains(session_id));
        assert_eq!(app.terminal_hub.next_sequence(session_id), Some(0));
        assert!(app.terminal_hub.close_reason(session_id).is_none());
        assert!(subscriber.control_rx.try_recv().is_err());
        assert!(app.state.sharing.shared_sessions.contains(session_id));
        assert!(app.state.sharing.host_relays.active(session_id));
        assert_eq!(
            app.state.sharing.host_relays.generation(session_id),
            host_generation
        );
        assert!(!host_relay_cancellation.is_cancelled());
        assert_eq!(
            app.state.agent_intel.registry.agent_session_id(session_id),
            Some("current-agent-session")
        );
        assert_eq!(
            app.client_focus.clients(session_id),
            vec!["current-client".to_owned()]
        );
        assert!(app.state.pending_work.promote_delete_after_stop(session_id));
    }
}

#[test]
fn project_discovery_control_remains_cwd_scoped_and_admitted() {
    let mut app = test_app();
    let working_dir = "/project-control".to_owned();
    let discovery = crate::session_runtime::project::ProjectDiscovery {
        project_type: Some("rust".to_owned()),
        package_manager: Some("cargo".to_owned()),
        manifest_files: vec!["Cargo.toml".to_owned()],
        git_url: None,
        git_branch: None,
        git_remotes: Vec::new(),
    };

    app.handle_session_event(RuntimeSessionEvent::ProjectDiscoveryReady {
        working_dir: working_dir.clone(),
        discovery,
    });

    let cached = app
        .state
        .project_discovery(Some(&working_dir))
        .expect("cwd-scoped discovery admitted");
    assert_eq!(cached.project_type.as_deref(), Some("rust"));
    assert_eq!(cached.package_manager.as_deref(), Some("cargo"));
}

#[tokio::test]
async fn maintenance_reduces_queued_stopped_before_classifying_finished_coordinator() {
    let mut app = test_app();
    let session_id = SessionId::new();
    let (screen_tx, _screen_rx) = tokio::sync::mpsc::channel(1);
    let (pty_tx, _pty_rx) = tokio::sync::mpsc::channel(1);
    let origin = crate::session_runtime::events::LocalCoordinatorOrigin {
        session_id,
        local_incarnation_id: uuid::Uuid::now_v7(),
    };
    let stopped_tx = app.session_events_tx.clone();
    let join_handle = tokio::spawn(async move {
        stopped_tx
            .send(RuntimeSessionEvent::Stopped {
                origin,
                reason: StopReason::UserRequested,
            })
            .await
            .expect("runtime event lane remains open");
    });
    let completion = join_handle.abort_handle();
    let handle = crate::session_runtime::handles::OwnedSessionHandle {
        senders: crate::session_runtime::handles::SessionSenders::new(
            Some(screen_tx),
            Some(pty_tx),
        ),
        cancellation: CancellationToken::new(),
        join_handle,
    };
    insert_owned_session_for_test(
        &mut app,
        claude_owned_summary(session_id, "/tmp/kodosi"),
        handle,
    );
    set_local_incarnation_for_test(&mut app, session_id, origin.local_incarnation_id);
    wait_for_finished(&completion).await;

    app.run_periodic_maintenance_force().await;

    assert!(app.state.local.sessions.record(session_id).is_none());
    assert!(
        !app.state
            .logs
            .iter()
            .any(|message| message.contains("coordinator task exited unexpectedly"))
    );
}

#[tokio::test]
async fn rejected_stop_preserves_active_share_transition() {
    let mut app = test_app();
    let session_id = SessionId::new();
    let (pty_tx, pty_rx) = tokio::sync::mpsc::channel(1);
    drop(pty_rx);
    let (screen_tx, _screen_rx) = tokio::sync::mpsc::channel(1);
    insert_owned_session_for_test(
        &mut app,
        claude_owned_summary(session_id, "/tmp/kodosi"),
        crate::session_runtime::handles::OwnedSessionHandle {
            senders: crate::session_runtime::handles::SessionSenders::new(
                Some(screen_tx),
                Some(pty_tx),
            ),
            cancellation: CancellationToken::new(),
            join_handle: tokio::spawn(std::future::pending::<()>()),
        },
    );
    let (account, transition_id) = seed_active_share_transition(&mut app, session_id);

    app.stop_session(session_id)
        .await
        .expect_err("closed PTY lane must reject stop");

    assert_share_transition_prepared(&app, &account, session_id, transition_id);
}

#[tokio::test]
async fn active_reopen_noop_preserves_active_share_transition() {
    let mut app = test_app();
    let session_id = SessionId::new();
    insert_owned_session_for_test(
        &mut app,
        claude_owned_summary(session_id, "/tmp/kodosi"),
        test_session_handle(),
    );
    let incarnation = app
        .state
        .local
        .sessions
        .record(session_id)
        .expect("session")
        .local_incarnation_id;
    let (account, transition_id) = seed_active_share_transition(&mut app, session_id);

    crate::host_protocol::sessions_command::apply_session_command(
        &mut app,
        crate::SessionCommand::Reopen {
            request_id: uuid::Uuid::now_v7().to_string(),
            session_id: session_id.to_string(),
            expected_runtime_incarnation_id: incarnation.to_string(),
        },
    )
    .await
    .expect("active reopen is an admitted no-op");

    assert_share_transition_prepared(&app, &account, session_id, transition_id);
}

#[tokio::test]
async fn rejected_delete_preserves_active_share_transition() {
    let mut app = test_app();
    let session_id = SessionId::new();
    insert_owned_session_for_test(
        &mut app,
        claude_owned_summary(session_id, "/tmp/kodosi"),
        test_session_handle(),
    );
    let incarnation = app
        .state
        .local
        .sessions
        .record(session_id)
        .expect("session")
        .local_incarnation_id;
    let (account, transition_id) = seed_active_share_transition(&mut app, session_id);

    crate::host_protocol::sessions_command::apply_session_command(
        &mut app,
        crate::SessionCommand::Delete {
            request_id: uuid::Uuid::now_v7().to_string(),
            session_id: session_id.to_string(),
            expected_runtime_incarnation_id: incarnation.to_string(),
        },
    )
    .await
    .expect_err("active session delete must reject");

    assert_share_transition_prepared(&app, &account, session_id, transition_id);
}

#[tokio::test]
async fn failed_stop_admission_keeps_session_running_and_retryable() {
    let mut app = test_app();
    let session_id = SessionId::new();
    let (pty_tx, pty_rx) = tokio::sync::mpsc::channel(1);
    drop(pty_rx);
    let (screen_tx, _screen_rx) = tokio::sync::mpsc::channel(1);
    let handle = crate::session_runtime::handles::OwnedSessionHandle {
        senders: crate::session_runtime::handles::SessionSenders::new(
            Some(screen_tx),
            Some(pty_tx),
        ),
        cancellation: CancellationToken::new(),
        join_handle: tokio::spawn(std::future::pending::<()>()),
    };
    insert_owned_session_for_test(
        &mut app,
        claude_owned_summary(session_id, "/tmp/kodosi"),
        handle,
    );

    let error = app
        .stop_session(session_id)
        .await
        .expect_err("a closed PTY lane must reject stop admission");

    std::assert_matches!(error, AppError::ChannelClosed { .. });
    let record = app
        .state
        .local
        .sessions
        .record(session_id)
        .expect("session record remains live");
    assert_eq!(record.summary.state, SessionState::Running);
    assert!(record.stopping_since.is_none());
}

#[tokio::test]
async fn stop_session_does_not_mutate_cached_terminal_sessions_without_runtime() {
    let mut config = AppConfig::default();
    config.auth.keyring_service = "kodosi.test".to_owned();
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));

    for terminal_state in [SessionState::Stopped, SessionState::Failed] {
        let session_id = SessionId::new();
        let mut summary = claude_owned_summary(session_id, "/tmp/kodosi");
        summary.state = terminal_state;
        app.state.local.sessions.insert(summary);

        app.stop_session(session_id)
            .await
            .unwrap_or_else(|error| panic!("stop_session should be ignored: {error}"));

        let record = app
            .state
            .local
            .sessions
            .record(session_id)
            .unwrap_or_else(|| panic!("cached session should remain"));
        assert_eq!(record.summary.state, terminal_state);
        assert!(
            record.stopping_since.is_none(),
            "cached terminal sessions must not enter the Stopping watchdog"
        );
    }
}
