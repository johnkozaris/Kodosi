use time::OffsetDateTime;

use crate::{
    host_protocol::{
        LocalSessionListEntry, PermissionFlags, RemoteSessionListEntry, RoomListEntry,
        RuntimeSessionStatus, SessionListEntry, SessionListEntryMeta,
    },
    session_runtime::project::ProjectDiscovery,
};
use kodosi_domain::{
    lifecycle::{ConnectionState, RemoteSessionAccessIssue, RemoteSessionAccessState},
    session::{SessionRole, SessionState},
};

use super::{AppState, Runtime};
use crate::CollaborationCleanupHealth;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeHealthSnapshot {
    pub(crate) collaboration_cleanup: CollaborationCleanupHealth,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RefreshFingerprint {
    pub(crate) account_user_id: Option<String>,
    pub(crate) account_epoch: u64,
    pub(crate) sessions: Vec<SessionListEntry>,
    pub(crate) rooms: Option<Vec<RoomListEntry>>,
    pub(crate) runtime_health: RuntimeHealthSnapshot,
}

pub(crate) fn build_refresh_fingerprint(
    app: &Runtime,
    collaboration_cleanup: CollaborationCleanupHealth,
) -> RefreshFingerprint {
    let state = &app.state;
    let (account_user_id, account_epoch) = state.identity.event_context();
    let rooms = state
        .room_catalog_loaded
        .then(|| state.available_rooms.clone());
    RefreshFingerprint {
        account_user_id,
        account_epoch,
        sessions: build_session_catalog(app),
        rooms,
        runtime_health: RuntimeHealthSnapshot {
            collaboration_cleanup,
        },
    }
}

pub(crate) fn build_session_catalog(app: &Runtime) -> Vec<SessionListEntry> {
    let state = &app.state;
    let mut sessions = Vec::new();

    for session_id in state.local.sessions.ids() {
        if let Some(record) = state.local.sessions.record(*session_id) {
            let room = state
                .sharing
                .shared_sessions
                .room_for_scope(*session_id, record.summary.scope);
            sessions.push(build_local_session_list_entry(
                state,
                &record.summary,
                record.terminal_title.as_deref(),
                record.create_request_id.clone(),
                record.local_incarnation_id,
                record.recovery,
                room.as_ref().map(|room| room.id.clone()),
                room.map(|room| room.name),
            ));
        }
    }

    for remote_id in state.discovery.remote_session_ids() {
        if let Some(session) = state.discovery.session(remote_id) {
            if session.viewer_hidden {
                continue;
            }

            if state.local.sessions.record(remote_id).is_some()
                || state
                    .sharing
                    .shared_sessions
                    .local_id_for_backend(&remote_id.to_string())
                    .is_some()
            {
                continue;
            }
            let summary = &session.summary;
            let is_owned = summary.role == SessionRole::Owner;
            let room_name = session
                .room_id
                .as_ref()
                .and_then(|_| summary.room_name.clone());
            sessions.push(build_remote_session_list_entry(
                RemoteSessionListEntryInput {
                    summary,
                    incarnation_id: session.incarnation_id,
                    is_owned,
                    room_id: session.room_id.clone(),
                    room_name,
                    connection_state: session.connection_state,
                    connection_reason: session.connection_reason.clone(),
                    access_state: session.access_state,
                    access_reason: session.access_reason.clone(),
                    access_issue: session.access_issue,
                },
            ));
        }
    }

    sessions.sort_by(|left, right| left.id().cmp(right.id()));
    sessions
}

fn build_local_session_list_entry(
    state: &AppState,
    summary: &kodosi_domain::session::SessionSummary,
    terminal_title: Option<&str>,
    create_request_id: Option<String>,
    local_incarnation_id: uuid::Uuid,
    recovery: kodosi_domain::session::LocalSessionRecoveryState,
    room_id: Option<String>,
    room_name: Option<String>,
) -> SessionListEntry {
    let project = session_project(summary, "local");
    let meta = runtime_meta(
        summary.working_dir.as_deref(),
        state.project_discovery(summary.working_dir.as_deref()),
        summary.running_command.as_deref(),
        summary.detected_agent.as_deref(),
        terminal_title,
    );
    let shared_session = state.sharing.shared_sessions.get(summary.id);
    SessionListEntry::Local {
        entry: LocalSessionListEntry {
            id: summary.id.to_string(),
            incarnation_id: local_incarnation_id.to_string(),
            create_request_id,
            name: summary.title.clone(),
            project,
            mode: summary.mode,
            status: catalog_status(summary.state),
            recovery,
            scope: summary.scope,
            access: summary.access,
            room_id,
            room_name,
            active_count: summary.active_count,
            entitled_count: summary.entitled_count,
            last_activity: format_last_activity(summary.last_update).into(),
            semantic_actions: local_semantic_actions(summary.state),
            backend_session_id: shared_session.map(|shared| shared.backend_session_id().to_owned()),
            backend_incarnation_id: shared_session
                .map(|shared| shared.backend_incarnation_id().to_string()),
            meta: meta.map(Box::new),
        },
    }
}

struct RemoteSessionListEntryInput<'a> {
    summary: &'a kodosi_domain::session::SessionSummary,
    incarnation_id: Option<uuid::Uuid>,
    is_owned: bool,
    room_id: Option<String>,
    room_name: Option<String>,
    connection_state: Option<ConnectionState>,
    connection_reason: Option<String>,
    access_state: Option<RemoteSessionAccessState>,
    access_reason: Option<String>,
    access_issue: Option<RemoteSessionAccessIssue>,
}

fn build_remote_session_list_entry(input: RemoteSessionListEntryInput<'_>) -> SessionListEntry {
    let RemoteSessionListEntryInput {
        summary,
        incarnation_id,
        is_owned,
        room_id,
        room_name,
        connection_state,
        connection_reason,
        access_state,
        access_reason,
        access_issue,
    } = input;
    let project = session_project(summary, "remote");
    let owner = if is_owned {
        None
    } else {
        Some(summary.owner_name.clone())
    };
    let permissions = if is_owned {
        PermissionFlags::OWNER
    } else {
        permissions_from_access_level(summary.access)
    };
    SessionListEntry::Remote {
        entry: RemoteSessionListEntry {
            id: summary.id.to_string(),
            incarnation_id: incarnation_id.map(|value| value.to_string()),
            name: summary.title.clone(),
            project,
            mode: summary.mode,
            status: catalog_status(summary.state),
            scope: summary.scope,
            access: summary.access,
            owner,
            owner_user_id: summary.owner_id.map(|value| value.to_string()),
            permissions,
            room_id,
            room_name,
            connection_state,
            connection_reason,
            access_state,
            access_reason,
            access_issue,
            active_count: summary.active_count,
            entitled_count: summary.entitled_count,
            last_activity: format_last_activity(summary.last_update).into(),

            semantic_actions: remote_semantic_actions(
                is_owned,
                summary.state,
                connection_state,
                access_state,
            ),
        },
    }
}

fn permissions_from_access_level(
    access: kodosi_domain::permissions::AccessLevel,
) -> PermissionFlags {
    PermissionFlags::from_access(access)
}

fn session_project(
    summary: &kodosi_domain::session::SessionSummary,
    fallback: &'static str,
) -> String {
    summary
        .working_dir
        .clone()
        .or_else(|| summary.room_name.clone())
        .unwrap_or_else(|| fallback.to_owned())
}

fn runtime_meta(
    working_dir: Option<&str>,
    discovery: Option<&ProjectDiscovery>,
    running_command: Option<&str>,
    detected_agent: Option<&str>,
    terminal_title: Option<&str>,
) -> Option<SessionListEntryMeta> {
    let dir = working_dir?;
    Some(SessionListEntryMeta {
        agent: detected_agent.unwrap_or("terminal").to_owned(),
        working_dir: dir.to_owned(),
        token_percent: 0,
        git_repo: None,
        git_url: discovery.and_then(|value| value.git_url.clone()),
        git_branch: discovery.and_then(|value| value.git_branch.clone()),
        git_remotes: discovery
            .map(|value| value.git_remotes.clone())
            .unwrap_or_default(),
        project_type: discovery.and_then(|value| value.project_type.clone()),
        package_manager: discovery.and_then(|value| value.package_manager.clone()),
        manifest_files: discovery
            .map(|value| value.manifest_files.clone())
            .unwrap_or_default(),
        running_command: running_command.map(str::to_owned),
        detected_agent: detected_agent.map(str::to_owned),
        terminal_title: terminal_title.map(str::to_owned),
    })
}

fn local_semantic_actions(state: SessionState) -> crate::host_protocol::SessionSemanticActions {
    let live = matches!(
        state,
        SessionState::Starting
            | SessionState::Running
            | SessionState::Published
            | SessionState::Reconnecting
    );
    crate::host_protocol::SessionSemanticActions {
        queue: false,
        steer: false,
        stop_and_send: live,
    }
}

fn remote_semantic_actions(
    is_owned: bool,
    state: SessionState,
    connection_state: Option<ConnectionState>,
    access_state: Option<RemoteSessionAccessState>,
) -> crate::host_protocol::SessionSemanticActions {
    let ready = is_owned
        && matches!(
            state,
            SessionState::Running | SessionState::Published | SessionState::Reconnecting
        )
        && connection_state == Some(ConnectionState::Connected)
        && access_state == Some(RemoteSessionAccessState::Ready);
    crate::host_protocol::SessionSemanticActions {
        queue: ready,
        steer: ready,
        stop_and_send: ready,
    }
}

fn format_last_activity(timestamp: OffsetDateTime) -> String {
    let now = OffsetDateTime::now_utc();
    let elapsed = now - timestamp;
    let seconds = elapsed.whole_seconds().max(0);

    if seconds < 60 {
        "just now".to_owned()
    } else if seconds < 3600 {
        format!("{}m ago", seconds / 60)
    } else if seconds < 86_400 {
        format!("{}h ago", seconds / 3600)
    } else {
        format!("{}d ago", seconds / 86_400)
    }
}

fn catalog_status(state: SessionState) -> RuntimeSessionStatus {
    match state {
        SessionState::Starting => RuntimeSessionStatus::Waiting,
        SessionState::Running | SessionState::Published => RuntimeSessionStatus::Active,
        SessionState::Reconnecting => RuntimeSessionStatus::Reconnecting,
        SessionState::Stopping => RuntimeSessionStatus::Stopping,
        SessionState::Stopped => RuntimeSessionStatus::Stopped,
        SessionState::Failed => RuntimeSessionStatus::Blocked,
    }
}

#[cfg(test)]
mod tests {
    use tokio_util::sync::CancellationToken;

    use super::{
        build_refresh_fingerprint, catalog_status, local_semantic_actions,
        permissions_from_access_level, remote_semantic_actions,
    };
    use crate::{
        config::AppConfig,
        host_protocol::{PermissionFlags, RoomListEntry, RuntimeSessionStatus},
        runtime::Runtime,
    };
    use kodosi_domain::{
        ids::SessionId,
        lifecycle::{ConnectionState, RemoteSessionAccessState},
        permissions::AccessLevel,
        session::{SessionState, SessionSummary},
        terminal::TerminalSize,
    };

    #[test]
    fn local_queue_and_steer_stay_off_without_a_connected_vendor_adapter() {
        let running = local_semantic_actions(SessionState::Running);
        assert!(!running.queue);
        assert!(!running.steer);
        assert!(running.stop_and_send);
        assert_eq!(
            local_semantic_actions(SessionState::Stopped),
            crate::host_protocol::SessionSemanticActions::NONE
        );
    }

    #[test]
    fn remote_semantic_actions_require_owned_connected_ready_session() {
        let enabled = remote_semantic_actions(
            true,
            SessionState::Running,
            Some(ConnectionState::Connected),
            Some(RemoteSessionAccessState::Ready),
        );
        assert!(enabled.queue && enabled.steer && enabled.stop_and_send);
        assert_eq!(
            remote_semantic_actions(
                false,
                SessionState::Running,
                Some(ConnectionState::Connected),
                Some(RemoteSessionAccessState::Ready),
            ),
            crate::host_protocol::SessionSemanticActions::NONE
        );
        assert_eq!(
            remote_semantic_actions(
                true,
                SessionState::Running,
                Some(ConnectionState::Reconnecting),
                Some(RemoteSessionAccessState::Ready),
            ),
            crate::host_protocol::SessionSemanticActions::NONE
        );
    }

    #[test]
    fn access_level_view_maps_to_view_only() {
        let perms = permissions_from_access_level(AccessLevel::View);
        assert!(perms.contains(PermissionFlags::VIEW));
        assert!(!perms.contains(PermissionFlags::SEND_INPUT));
        assert!(!perms.contains(PermissionFlags::FOCUS_BLUR));
    }

    #[test]
    fn access_level_suggest_cannot_emit_terminal_focus_input() {
        let perms = permissions_from_access_level(AccessLevel::Suggest);
        assert!(perms.contains(PermissionFlags::VIEW));
        assert!(!perms.contains(PermissionFlags::FOCUS_BLUR));
        assert!(!perms.contains(PermissionFlags::SEND_INPUT));
        assert!(!perms.contains(PermissionFlags::RESIZE));
    }

    #[test]
    fn access_level_inject_maps_to_full_participation_minus_owner_ops() {
        let perms = permissions_from_access_level(AccessLevel::Inject);
        assert!(perms.contains(PermissionFlags::VIEW));
        assert!(perms.contains(PermissionFlags::SEND_INPUT));
        assert!(perms.contains(PermissionFlags::RESIZE));
        assert!(perms.contains(PermissionFlags::FOCUS_BLUR));
        assert!(!perms.contains(PermissionFlags::STOP));
        assert!(!perms.contains(PermissionFlags::RENAME));
        assert!(!perms.contains(PermissionFlags::DELETE));
        assert!(!perms.contains(PermissionFlags::SET_MODE));
    }

    #[test]
    fn snapshot_signal_omits_room_catalog_until_loaded() {
        let mut config = AppConfig::default();
        config.auth.keyring_service = "kodosi.test".to_owned();
        let mut app = Runtime::with_dependencies(
            config.clone(),
            CancellationToken::new(),
            crate::runtime::RuntimeDependencies::isolated(&config)
                .expect("isolated runtime dependencies"),
        )
        .unwrap_or_else(|error| panic!("test app should construct: {error}"));

        app.state.set_available_rooms(vec![RoomListEntry {
            id: "room-1".to_owned(),
            name: "Room".to_owned(),
            slug: "room".to_owned(),
        }]);
        app.state.room_catalog_loaded = false;

        assert_eq!(
            build_refresh_fingerprint(
                &app,
                crate::CollaborationCleanupHealth {
                    state: crate::CollaborationCleanupState::Healthy,
                    pending_count: 0,
                    quarantined_count: 0,
                    message: None,
                }
            )
            .rooms,
            None
        );

        app.state.room_catalog_loaded = true;

        assert_eq!(
            build_refresh_fingerprint(
                &app,
                crate::CollaborationCleanupHealth {
                    state: crate::CollaborationCleanupState::Healthy,
                    pending_count: 0,
                    quarantined_count: 0,
                    message: None,
                }
            )
            .rooms,
            Some(vec![RoomListEntry {
                id: "room-1".to_owned(),
                name: "Room".to_owned(),
                slug: "room".to_owned(),
            }])
        );
    }

    #[test]
    fn catalog_equality_ignores_activity_only_changes() {
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
        let mut summary = SessionSummary::new_owned(
            session_id,
            "Session".to_owned(),
            "/tmp".to_owned(),
            TerminalSize::new(80, 24).expect("valid terminal size"),
            None,
        );
        summary.state = SessionState::Running;
        app.state.local.sessions.insert(summary);

        let before = super::build_session_catalog(&app);
        app.state
            .local
            .sessions
            .record_mut(session_id)
            .expect("session record")
            .summary
            .last_update -= time::Duration::minutes(2);
        let activity_only = super::build_session_catalog(&app);
        assert_ne!(
            serde_json::to_value(&before).expect("catalog serializes"),
            serde_json::to_value(&activity_only).expect("catalog serializes")
        );
        assert_eq!(before, activity_only);

        app.state
            .local
            .sessions
            .record_mut(session_id)
            .expect("session record")
            .summary
            .state = SessionState::Stopping;
        let closing = super::build_session_catalog(&app);
        assert_ne!(before, closing);
    }

    #[test]
    fn published_sessions_still_project_as_active() {
        assert_eq!(
            catalog_status(SessionState::Published),
            RuntimeSessionStatus::Active
        );
    }
}
