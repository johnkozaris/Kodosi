use kodosi_domain::{ids::SessionId, lifecycle::ConnectionState};

use crate::{
    AppError, discovery::backend::DiscoveryFetchCtx,
    identity_core::stored_auth::RefreshStoredAuthResult as RefreshAccessResult,
};

use super::{DiscoveryRefreshOutcome, Runtime};

pub(crate) async fn refresh(app: &mut Runtime) {
    if !app.remote_surfaces_ready() {
        return;
    }
    if !app.backend.is_configured() {
        app.state.backend_status = ConnectionState::Offline;
        clear_remote_catalog(app, CatalogClearReason::TransientOffline);
        return;
    }

    match super::auth::ensure_backend_access(app).await {
        Ok(true) => {}
        Ok(false) => {
            app.state.backend_status = ConnectionState::Offline;
            clear_remote_catalog(app, CatalogClearReason::TransientOffline);
            return;
        }
        Err(error) => {
            app.state.backend_status = ConnectionState::Offline;
            clear_remote_catalog(app, CatalogClearReason::TransientOffline);
            app.state
                .record_log(format!("backend auth preparation failed: {error}"));
            return;
        }
    }

    app.state.backend_status = ConnectionState::Connecting;
    match fetch_outcome(app).await {
        Ok(outcome) => apply_outcome(app, outcome),
        Err(AppError::Unauthorized) => match super::auth::refresh_access_token(app).await {
            Ok(RefreshAccessResult::Refreshed(_)) => match fetch_outcome(app).await {
                Ok(outcome) => apply_outcome(app, outcome),
                Err(AppError::Unauthorized) => {
                    if let Err(error) = super::auth::mark_expired_from_backend(
                        app,
                        "backend session expired — press Shift+L to sign in again",
                    ) {
                        app.state.record_log(error.to_string());
                    }
                }
                Err(error) => {
                    app.state.backend_status = ConnectionState::Offline;
                    app.state
                        .record_log(format!("backend refresh failed: {error}"));
                }
            },
            Ok(RefreshAccessResult::RequiresLogin(reason)) => {
                if let Err(error) = super::auth::mark_expired_from_backend(app, reason.to_string())
                {
                    app.state.record_log(error.to_string());
                }
            }
            Ok(RefreshAccessResult::TemporarilyUnavailable(reason)) => {
                super::auth::note_reconnecting(app, reason.to_string());
            }
            Err(error) => {
                app.state.backend_status = ConnectionState::Offline;
                app.state
                    .record_log(format!("backend refresh failed: {error}"));
            }
        },
        Err(error) => {
            app.state.backend_status = ConnectionState::Offline;
            app.state
                .record_log(format!("backend refresh failed: {error}"));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CatalogClearReason {
    TransientOffline,

    AccountTeardown,
}

pub(crate) fn clear_remote_catalog(app: &mut Runtime, reason: CatalogClearReason) {
    let retiring_account_user_id = (reason == CatalogClearReason::AccountTeardown)
        .then(|| app.state.identity.auth.subject_string())
        .flatten();
    clear_remote_catalog_for_account(app, reason, retiring_account_user_id.as_deref());
}

pub(crate) fn clear_remote_catalog_for_account(
    app: &mut Runtime,
    reason: CatalogClearReason,
    retiring_account_user_id: Option<&str>,
) {
    app.state.cancel_all_session_relays_immediate();
    app.state.clear_room_catalog();
    app.state.clear_pending_discovery_refresh();

    let mut catalog_remote_ids = app.state.discovery.remote_session_ids();
    let remote_permission_incarnations = catalog_remote_ids
        .iter()
        .filter_map(|id| {
            app.state
                .discovery
                .session(*id)
                .and_then(|record| record.incarnation_id)
                .map(|incarnation_id| (*id, incarnation_id))
        })
        .collect::<Vec<_>>();
    catalog_remote_ids.extend(app.remote_terminal.ids());
    catalog_remote_ids.sort_unstable();
    catalog_remote_ids.dedup();
    if reason == CatalogClearReason::AccountTeardown {
        let account_session_ids = catalog_remote_ids.iter().map(ToString::to_string).collect();
        app.semantic_receipt_cursor = None;
        if let Some(task) = app.semantic_mailbox_task.take() {
            task.abort();
        }
        app.state
            .runtime_outbox
            .clear_account_epoch(&account_session_ids, retiring_account_user_id);
        app.state.pending_work.clear_account_epoch();
        for (session_id, incarnation_id) in remote_permission_incarnations {
            app.state
                .agent_intel
                .permission_decisions
                .retire_incarnation(session_id, incarnation_id);
        }
        app.clear_pending_remote_resizes();
        app.state.discovery.drain_all();
    } else {
        for &id in &catalog_remote_ids {
            app.reject_pending_remote_resizes(
                id,
                None,
                "the remote terminal is temporarily unavailable",
            );
        }
        app.state
            .discovery
            .replace_remote_sessions(Vec::new(), false);
    }
    app.state.shelf.sync_remote_sessions(&[]);
    drain_remote_terminal_state(app, &catalog_remote_ids);
}

pub(crate) fn drain_remote_terminal_state(app: &mut Runtime, catalog_remote_ids: &[SessionId]) {
    for id in app.remote_terminal.drain_all() {
        app.terminal_hub.end_session(
            id,
            &crate::terminal_transport::TerminalCloseReason::AuthRevoked,
        );
        app.client_focus.forget(id);
    }
    for &id in catalog_remote_ids {
        app.client_focus.forget(id);
    }
}

async fn fetch_outcome(app: &mut Runtime) -> crate::Result<DiscoveryRefreshOutcome> {
    let last_terminal_size = app.state.local.last_terminal_size;
    let current_user_id = app.state.identity.auth.subject();
    DiscoveryFetchCtx {
        backend: &app.backend,
        logs: &mut app.state.logs,
    }
    .fetch_outcome(last_terminal_size, current_user_id)
    .await
}

fn retire_remote_incarnation(
    app: &mut Runtime,
    retired: crate::discovery::state::RetiredRemoteIncarnation,
) {
    let id = retired.session_id;
    app.state.cancel_session_relay_immediate(id);
    app.reject_pending_remote_resizes(
        id,
        None,
        "the remote terminal changed before applying this size",
    );
    app.remote_terminal.forget(id);
    app.client_focus.forget(id);
    app.terminal_hub.end_session(
        id,
        &crate::terminal_transport::TerminalCloseReason::Detached,
    );
    app.semantic_receipts_in_flight
        .retain(|(session_id, _, _)| *session_id != id);
    app.action_results_in_flight
        .retain(|(session_id, _, _, _)| *session_id != id);
    app.state
        .runtime_outbox
        .retire_session_incarnation(&id.to_string(), &retired.incarnation_id.to_string());
    if app
        .state
        .agent_intel
        .permission_decisions
        .retire_incarnation(id, retired.incarnation_id)
        .changed()
    {
        app.state.runtime_outbox.queue_pending_permissions_snapshot(
            app.state.agent_intel.permission_decisions.snapshot(),
        );
    }

    if let Some(account_user_id) = app.state.identity.auth.subject_string()
        && let Err(error) = app.remote_permission_actions.clear_session_incarnation(
            &account_user_id,
            id,
            retired.incarnation_id,
        )
    {
        app.state.record_log(format!(
            "{} could not retire pending permission actions for replaced incarnation: {error}",
            id.short()
        ));
    }
    app.state.record_log(format!(
        "{} retired remote incarnation {}",
        id.short(),
        retired.incarnation_id
    ));
}

fn apply_outcome(app: &mut Runtime, outcome: DiscoveryRefreshOutcome) {
    let remote_count = outcome.remote_sessions.len();
    let preserve_missing = !outcome.preservation.is_empty();
    if let Some(rooms) = outcome.available_rooms {
        app.state.set_available_rooms(rooms);
    }
    let retired_incarnations = app
        .state
        .discovery
        .replace_remote_sessions_with_policy(outcome.remote_sessions, &outcome.preservation);
    for retired in retired_incarnations {
        retire_remote_incarnation(app, retired);
    }
    let remote_ids = app.state.discovery.non_hidden_remote_session_ids();
    app.state.shelf.sync_remote_sessions(&remote_ids);
    app.state.backend_status = ConnectionState::Connected;
    if preserve_missing {
        app.state.record_log(format!(
            "loaded {remote_count} remote sessions from backend; preserved existing remote state after partial refresh failure"
        ));
    } else {
        app.state.record_log(format!(
            "loaded {remote_count} remote sessions from backend"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        agent_intel::permission_decision_registry::{PendingKey, PendingPermissionMetadata},
        config::AppConfig,
        discovery::RemoteSessionRecord,
        host_protocol::AgentIntelEvent,
        runtime::{
            RuntimeDependencies,
            permission_actions::RemotePermissionAction,
            remote_semantics::{RemoteSemanticRequest, RemoteSemanticSignerSnapshot},
        },
        terminal_transport::{TerminalCapability, TerminalSurface},
    };
    use kodosi_backend_client::session_relay::{
        RemoteRelayMode, SessionRelayCommand, wire::RelaySemanticMode,
    };
    use kodosi_domain::{
        auth::AuthState,
        ids::{SessionId, UserId},
        permissions::{AccessLevel, ShareScope},
        session::SessionSummary,
        terminal::{TerminalCheckpointV2, TerminalScreen, TerminalSize},
    };
    use time::OffsetDateTime;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    fn test_app() -> Runtime {
        let mut config = AppConfig::default();
        config.auth.keyring_service = "kodosi.test".to_owned();
        Runtime::with_dependencies(
            config.clone(),
            CancellationToken::new(),
            RuntimeDependencies::isolated(&config).expect("isolated runtime dependencies"),
        )
        .expect("test app should construct")
    }

    fn test_user_id() -> UserId {
        UserId::try_from("11111111-1111-1111-1111-111111111111")
            .expect("test user id should be valid")
    }

    fn authenticate_test_app(app: &mut Runtime) -> String {
        app.state
            .identity
            .advance_account_epoch()
            .expect("test account epoch should advance");
        let subject = test_user_id();
        app.state.identity.auth = AuthState::Authenticated {
            subject: Some(subject),
            expires_at: OffsetDateTime::now_utc(),
        };
        subject.to_string()
    }

    fn test_remote_checkpoint(rows: u16, cols: u16) -> TerminalCheckpointV2 {
        let mut terminal =
            ghostty_vt::Terminal::new(cols, rows, ghostty_vt::TerminalPolicy::default())
                .expect("test terminal");
        let semantic = terminal
            .semantic_checkpoint(ghostty_vt::CheckpointLimits::default())
            .expect("semantic checkpoint");
        let state = terminal.state().expect("terminal state");
        TerminalCheckpointV2::new(
            TerminalSize::new(rows, cols).expect("valid terminal size"),
            TerminalScreen::Primary,
            semantic.into_bytes(),
            state.cursor_x,
            state.cursor_y,
            !state.cursor_visible,
        )
        .expect("valid terminal checkpoint")
    }

    #[tokio::test]
    async fn changed_backend_incarnation_retires_predecessor_remote_state() {
        let mut app = test_app();
        let account = authenticate_test_app(&mut app);
        let session_id = SessionId::new();
        let old_incarnation = uuid::Uuid::now_v7();
        let new_incarnation = uuid::Uuid::now_v7();
        let remote_record = |incarnation_id| RemoteSessionRecord {
            summary: SessionSummary::new_remote(
                session_id,
                "Remote".to_owned(),
                "Owner".to_owned(),
                Some(test_user_id()),
                ShareScope::Friends,
                AccessLevel::Approve,
                TerminalSize::default(),
            ),
            incarnation_id: Some(incarnation_id),
            room_id: None,
            connection_state: None,
            connection_reason: None,
            access_state: None,
            access_reason: None,
            access_issue: None,
            viewer_blocked: false,
            viewer_hidden: false,
        };
        app.state
            .discovery
            .replace_remote_sessions(vec![remote_record(old_incarnation)], false);
        app.state.runtime_outbox.queue_agent_intel(
            AgentIntelEvent::RemotePermissionDecisionState {
                session_id: session_id.to_string(),
                session_incarnation_id: old_incarnation.to_string(),
                tool_use_id: "old-tool".to_owned(),
                request_generation: 7,
                phase: crate::RemotePermissionDecisionPhase::Pending,
                status: None,
                message: None,
            },
        );

        let cancellation = CancellationToken::new();
        let (command_tx, mut command_rx) = mpsc::channel(2);
        let old_generation = app.state.remote.session_relays.attach_for_test(
            session_id,
            cancellation.clone(),
            command_tx,
            ShareScope::Friends,
            RemoteRelayMode::SharedParticipant,
            ConnectionState::Connected,
            tokio::spawn(std::future::pending()),
        );
        app.remote_terminal
            .install_checkpoint(session_id, test_remote_checkpoint(24, 80), 4);
        app.terminal_hub
            .install_semantic_checkpoint(session_id, test_remote_checkpoint(24, 80), 4);
        let mut terminal = app
            .terminal_hub
            .register(
                session_id,
                TerminalSurface::Desktop,
                TerminalCapability::Write,
            )
            .expect("session should be live");
        let _ = app
            .client_focus
            .note_focus(session_id, "desktop".to_owned());
        let action = RemotePermissionAction {
            account_user_id: account.clone(),
            session_id: session_id.to_string(),
            incarnation_id: old_incarnation,
            action_id: uuid::Uuid::now_v7().to_string(),
            request_id: "tool-use".to_owned(),
            request_generation: 7,
            decision: "allow".to_owned(),
        };
        app.remote_permission_actions
            .admit_or_existing(action.clone())
            .expect("pending action");
        let old_key = PendingKey {
            session_id,
            session_incarnation_id: old_incarnation,
            tool_use_id: action.request_id.clone(),
        };
        assert!(
            app.state
                .agent_intel
                .permission_decisions
                .activate_remote(
                    old_key.clone(),
                    action.request_generation,
                    1,
                    PendingPermissionMetadata {
                        tool_name: "Bash".to_owned(),
                        tool_input: serde_json::Value::Null,
                        deadline_at_ms: 100,
                        risk: crate::ApprovalRisk::Unknown,
                    },
                )
                .changed()
        );
        let text = "stale semantic".to_owned();
        let semantic_request_id = uuid::Uuid::now_v7();
        app.remote_semantics
            .admit(RemoteSemanticRequest {
                account_user_id: account.clone(),
                requester_device_id: "requester".to_owned(),
                session_id: session_id.to_string(),
                incarnation_id: old_incarnation,
                request_id: semantic_request_id,
                mode: RelaySemanticMode::Steer,
                payload_sha256: kodosi_backend_client::crypto::sha256_hex(text.as_bytes()),
                text,
                signer: Some(RemoteSemanticSignerSnapshot {
                    account_user_id: account.clone(),
                    session_id: session_id.to_string(),
                    incarnation_id: old_incarnation,
                    owner_user_id: account.clone(),
                    owner_device_id: "owner-device".to_owned(),
                    owner_signing_public_key: vec![1],
                    device_list_generation: 1,
                    identity_fingerprint: [2; 32],
                }),
            })
            .expect("pending semantic");

        apply_outcome(
            &mut app,
            DiscoveryRefreshOutcome {
                remote_sessions: vec![remote_record(new_incarnation)],
                preservation: crate::discovery::state::DiscoveryPreservation::default(),
                available_rooms: None,
            },
        );

        assert!(cancellation.is_cancelled());
        assert!(!app.state.remote.session_relays.contains(session_id));
        assert!(
            !app.state
                .remote
                .session_relays
                .is_current_generation(session_id, old_generation)
        );
        assert!(app.remote_terminal.cached(session_id).is_none());
        assert!(app.client_focus.clients(session_id).is_empty());
        assert!(
            app.remote_permission_actions
                .pending_for_incarnation(&account, session_id, old_incarnation)
                .is_empty()
        );
        assert!(
            app.state
                .agent_intel
                .permission_decisions
                .snapshot()
                .requests
                .iter()
                .all(|request| {
                    request.session_id != session_id.to_string()
                        || request.session_incarnation_id != old_incarnation.to_string()
                }),
            "the retired incarnation must not remain in authoritative pending permissions"
        );
        let retained = app.remote_semantics.pending_for(&account, session_id);
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].request_id, semantic_request_id);
        assert_eq!(retained[0].incarnation_id, old_incarnation);
        let agent_events = app.state.runtime_outbox.drain_agent_intel();
        assert!(
            agent_events.iter().all(|event| matches!(
                event,
                AgentIntelEvent::PendingPermissionsSnapshot { requests, .. }
                    if requests.iter().all(|request| {
                        request.session_id != session_id.to_string()
                            || request.session_incarnation_id != old_incarnation.to_string()
                    })
            )),
            "only the replacement pending-permission snapshot may survive retirement"
        );
        crate::runtime::remote_sessions::replay_remote_semantics(&mut app, session_id);
        assert!(
            command_rx.try_recv().is_err(),
            "a retained old-incarnation request must never replay through the replacement relay"
        );
        assert_eq!(
            app.state
                .discovery
                .session(session_id)
                .and_then(|record| record.incarnation_id),
            Some(new_incarnation)
        );

        app.remote_permission_actions
            .admit_or_existing(action.clone())
            .expect("model retained stale action after cleanup failure");
        let (replacement_tx, mut replacement_rx) = mpsc::channel(2);
        app.state.remote.session_relays.attach_for_test(
            session_id,
            CancellationToken::new(),
            replacement_tx,
            ShareScope::Friends,
            RemoteRelayMode::SharedParticipant,
            ConnectionState::Connected,
            tokio::spawn(std::future::pending()),
        );
        crate::runtime::remote_sessions::replay_remote_permission_actions(&mut app, session_id);
        assert!(
            replacement_rx.try_recv().is_err(),
            "an old-incarnation permission action must never replay through replacement relay"
        );
        let current_action = RemotePermissionAction {
            incarnation_id: new_incarnation,
            action_id: uuid::Uuid::now_v7().to_string(),
            request_id: "current-tool-use".to_owned(),
            ..action
        };
        app.remote_permission_actions
            .admit_or_existing(current_action.clone())
            .expect("current action");
        let current_key = PendingKey {
            session_id,
            session_incarnation_id: new_incarnation,
            tool_use_id: current_action.request_id.clone(),
        };
        assert!(
            app.state
                .agent_intel
                .permission_decisions
                .activate_remote(
                    current_key,
                    current_action.request_generation,
                    1,
                    PendingPermissionMetadata {
                        tool_name: "Bash".to_owned(),
                        tool_input: serde_json::Value::Null,
                        deadline_at_ms: 100,
                        risk: crate::ApprovalRisk::Unknown,
                    },
                )
                .changed()
        );
        crate::runtime::remote_sessions::replay_remote_permission_actions(&mut app, session_id);
        std::assert_matches!(
            replacement_rx.try_recv(),
            Ok(SessionRelayCommand::PermissionDecision {
                action_id,
                request_id,
                ..
            }) if action_id == current_action.action_id && request_id == current_action.request_id
        );

        let closed = terminal
            .control_rx
            .try_recv()
            .expect("predecessor surface should close");
        std::assert_matches!(
            closed,
            crate::terminal_transport::TerminalControlFrame::Closed {
                reason: crate::terminal_transport::TerminalCloseReason::Detached,
                ..
            }
        );
    }
}
