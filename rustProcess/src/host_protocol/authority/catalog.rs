use serde::Serialize;
use specta::{
    Type, Types,
    datatype::{DataType, Field, Fields, NamedFields, NamedReferenceType, Reference},
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

use super::shapes::{
    sample_friend_entry, sample_friend_request_entry, sample_hidden_session_entry,
    sample_my_device_entry, sample_room_entry, sample_room_list_entry,
    sample_runtime_local_session_info, sample_runtime_remote_session_info, sample_trust_pin_entry,
};
use crate::{
    AccessGrantEntry, AppError, AuthCommand, AuthEvent, AuthRequiredReason, DeviceCommand,
    DeviceEvent, DeviceLinkRequestEntry, FriendsCommand, FriendsEvent, Result, RoomCommand,
    SessionCommand, SessionEvent, SteerDeliveryState, SteerQueueEntry, SteerTransition,
    SystemCommand, SystemEvent, TerminalCommand, TerminalEvent, TrustCommand,
    host_protocol::{
        ActivePendingPermission, AgentGlobalEvent, AgentIntelCommand, AgentIntelEvent,
        PendingPermissionDecisionPhase, RelayActionStatus, RemotePermissionDecisionPhase,
        RoomEvent, RuntimeSessionStatus, SessionAccessMutationKind, SessionAccessMutationOutcome,
        TerminalResizeIdentity, TrustEvent,
    },
};
use kodosi_domain::{
    lifecycle::{ConnectionState, RemoteSessionAccessIssue, RemoteSessionAccessState},
    permissions::{AccessLevel, ShareScope},
    session::SessionMode,
};

thread_local! {
    static MESSAGE_SHAPES: RefCell<BTreeMap<String, serde_json::Value>> =
        const { RefCell::new(BTreeMap::new()) };
}

pub(super) fn reset_message_shapes() {
    MESSAGE_SHAPES.with(|shapes| shapes.borrow_mut().clear());
}

pub(super) fn take_message_shapes() -> BTreeMap<String, serde_json::Value> {
    MESSAGE_SHAPES.with(|shapes| std::mem::take(&mut *shapes.borrow_mut()))
}

fn resize_identity(request_id: &str, cols: u16, rows: u16) -> TerminalResizeIdentity {
    TerminalResizeIdentity {
        request_id: request_id.to_owned(),
        expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000001".to_owned(),
        subscription_id: "terminal-1".to_owned(),
        subscription_generation: 7,
        surface_generation: 3,
        cols,
        rows,
        width_pixels: u32::from(cols) * 10,
        height_pixels: u32::from(rows) * 20,
        cell_width_pixels: 10,
        cell_height_pixels: 20,
    }
}

pub(super) fn terminal_command_types() -> Result<Vec<String>> {
    Ok(vec![
        extract_type(&TerminalCommand::InputBytes {
            session_id: "session-1".to_owned(),
            bytes: vec![0x1b, b'[', b'A'],
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000001".to_owned(),
            subscription_id: Some("terminal-1".to_owned()),
            subscription_generation: Some(7),
        })?,
        extract_type(&TerminalCommand::Resize {
            session_id: "session-1".to_owned(),
            identity: resize_identity("01900000-0000-7000-8000-000000000004", 120, 40),
            claim: true,
        })?,
        extract_type(&TerminalCommand::Focus {
            session_id: "session-1".to_owned(),
            client_id: "client-1".to_owned(),
            request_id: "01900000-0000-7000-8000-000000000002".to_owned(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000001".to_owned(),
        })?,
        extract_type(&TerminalCommand::Blur {
            session_id: "session-1".to_owned(),
            client_id: "client-1".to_owned(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000001".to_owned(),
        })?,
    ])
}

pub(super) fn terminal_event_types() -> Result<Vec<String>> {
    Ok(vec![
        extract_type(&TerminalEvent::Bell {
            session_id: "session-1".to_owned(),
        })?,
        extract_type(&TerminalEvent::Title {
            session_id: "session-1".to_owned(),
            title: Some("agent".to_owned()),
        })?,
        extract_type(&TerminalEvent::FocusApplied {
            session_id: "session-1".to_owned(),
            request_id: "01900000-0000-7000-8000-000000000002".to_owned(),
            runtime_incarnation_id: "01900000-0000-7000-8000-000000000001".to_owned(),
        })?,
        extract_type(&TerminalEvent::FocusRejected {
            session_id: "session-1".to_owned(),
            request_id: "01900000-0000-7000-8000-000000000003".to_owned(),
            runtime_incarnation_id: "01900000-0000-7000-8000-000000000001".to_owned(),
            reason: "stale runtime incarnation".to_owned(),
        })?,
        extract_type(&TerminalEvent::ResizeApplied {
            session_id: "session-1".to_owned(),
            identity: resize_identity("01900000-0000-7000-8000-000000000004", 120, 40),
        })?,
        extract_type(&TerminalEvent::ResizeRejected {
            session_id: "session-1".to_owned(),
            identity: resize_identity("01900000-0000-7000-8000-000000000005", 100, 30),
            reason: "another terminal surface holds size authority".to_owned(),
        })?,
        extract_type(&TerminalEvent::Notification {
            session_id: "session-1".to_owned(),
            title: Some("Build complete".to_owned()),
            body: Some("All tests passed".to_owned()),
        })?,
    ])
}

pub(super) fn system_command_types() -> Result<Vec<String>> {
    let command_types = vec![
        extract_type(&SystemCommand::Shutdown)?,
        extract_type(&SystemCommand::RefreshClaudeGlobal {
            cwd: Some("/tmp/kodosi".to_owned()),
            generation: Some(7),
        })?,
        extract_type(&SystemCommand::SetHostTheme { dark: true })?,
        extract_type(&SystemCommand::QueryPendingPermissions {
            request_id: "req-pending".to_owned(),
        })?,
        extract_type(&SystemCommand::AllowPendingPermissionRequest {
            session_id: "00000000-0000-0000-0000-000000000000".to_owned(),
            session_incarnation_id: "01900000-0000-7000-8000-000000000002".to_owned(),
            tool_use_id: "toolu_example".to_owned(),
            request_generation: 7,
        })?,
        extract_type(&SystemCommand::DenyPendingPermissionRequest {
            session_id: "00000000-0000-0000-0000-000000000000".to_owned(),
            session_incarnation_id: "01900000-0000-7000-8000-000000000002".to_owned(),
            tool_use_id: "toolu_example".to_owned(),
            request_generation: 7,
            reason: Some("not safe".to_owned()),
        })?,
        extract_type(&SystemCommand::SemanticSend {
            request_id: "01900000-0000-7000-8000-000000000001".to_owned(),
            session_id: "00000000-0000-0000-0000-000000000000".to_owned(),
            incarnation_id: "01900000-0000-7000-8000-000000000002".to_owned(),
            mode: crate::host_protocol::SemanticSendMode::Steer,
            text: "also update the tests".to_owned(),
        })?,
        extract_type(&SystemCommand::CancelSteer {
            request_id: "req-steer-cancel".to_owned(),
            session_id: "00000000-0000-0000-0000-000000000000".to_owned(),
            steer_id: "steer-1".to_owned(),
        })?,
        extract_type(&SystemCommand::QuerySteer {
            request_id: "req-steer-query".to_owned(),
            session_id: "00000000-0000-0000-0000-000000000000".to_owned(),
            semantic_request_id: Some("01900000-0000-7000-8000-000000000003".to_owned()),
        })?,
        extract_type(&SystemCommand::QuerySteer {
            request_id: "req-steer-query-all".to_owned(),
            session_id: "00000000-0000-0000-0000-000000000000".to_owned(),
            semantic_request_id: None,
        })?,
    ];
    Ok(command_types.into_iter().fold(Vec::new(), |mut types, ty| {
        if !types.contains(&ty) {
            types.push(ty);
        }
        types
    }))
}

pub(super) fn system_event_types() -> Result<Vec<String>> {
    Ok(vec![
        extract_type(&SystemEvent::Heartbeat)?,
        extract_type(&SystemEvent::Error {
            message: "boom".to_owned(),
            context: Some("context".to_owned()),
        })?,
        extract_type(&SystemEvent::RuntimeHealth {
            collaboration_cleanup: crate::CollaborationCleanupHealth {
                state: crate::CollaborationCleanupState::Quarantined,
                pending_count: 2,
                quarantined_count: 1,
                message: Some("operator attention required".to_owned()),
            },
        })?,
    ])
}

pub(super) fn session_command_types() -> Result<Vec<String>> {
    Ok(vec![
        extract_type(&SessionCommand::Create {
            request_id: "req-1".to_owned(),
            name: "session-1".to_owned(),
            working_dir: Some("/tmp/project".to_owned()),
            resume: Some(
                kodosi_domain::provider_conversation::ProviderConversationIdentity {
                    provider:
                        kodosi_domain::provider_conversation::ProviderConversationProvider::Claude,
                    native_conversation_id: "01900000-0000-7000-8000-000000000021".to_owned(),
                },
            ),
        })?,
        extract_type(&SessionCommand::Rename {
            session_id: "session-1".to_owned(),
            name: "renamed".to_owned(),
        })?,
        extract_type(&SessionCommand::SetMode {
            session_id: "session-1".to_owned(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
            mode: SessionMode::Plan,
        })?,
        extract_type(&SessionCommand::Stop {
            request_id: "01900000-0000-7000-8000-000000000020".to_owned(),
            session_id: "session-1".to_owned(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
        })?,
        extract_type(&SessionCommand::Close {
            request_id: "01900000-0000-7000-8000-000000000024".to_owned(),
            session_id: "session-1".to_owned(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
        })?,
        extract_type(&SessionCommand::Interrupt {
            request_id: "01900000-0000-7000-8000-000000000021".to_owned(),
            session_id: "session-1".to_owned(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
        })?,
        extract_type(&SessionCommand::Delete {
            request_id: "01900000-0000-7000-8000-000000000022".to_owned(),
            session_id: "session-1".to_owned(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
        })?,
        extract_type(&SessionCommand::Reopen {
            request_id: "01900000-0000-7000-8000-000000000023".to_owned(),
            session_id: "session-1".to_owned(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
        })?,
        extract_type(&SessionCommand::OpenRemote {
            session_id: "session-1".to_owned(),
        })?,
        extract_type(&SessionCommand::Hide {
            session_id: "session-1".to_owned(),
        })?,
        extract_type(&SessionCommand::Leave {
            mutation_id: "01900000-0000-7000-8000-000000000010".to_owned(),
            session_id: "session-1".to_owned(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
        })?,
        extract_type(&SessionCommand::Unhide {
            session_id: "session-1".to_owned(),
        })?,
        extract_type(&SessionCommand::ListHidden {
            request_id: "req-hidden".to_owned(),
        })?,
        extract_type(&SessionCommand::SetShareScope {
            request_id: "req-scope".to_owned(),
            session_id: "session-1".to_owned(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
            scope: ShareScope::Room,
            room_id: Some("room-1".to_owned()),
        })?,
        extract_type(&SessionCommand::GrantAccess {
            mutation_id: "01900000-0000-7000-8000-000000000012".to_owned(),
            session_id: "session-1".to_owned(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
            actor_user_id: "user-2".to_owned(),
            access_level: AccessLevel::Suggest,
            expires_at: "2026-08-14T00:00:00Z".to_owned(),
        })?,
        extract_type(&SessionCommand::RevokeAccess {
            mutation_id: "01900000-0000-7000-8000-000000000013".to_owned(),
            session_id: "session-1".to_owned(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
            actor_user_id: "user-2".to_owned(),
        })?,
        extract_type(&SessionCommand::ListAccess {
            session_id: "session-1".to_owned(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
        })?,
        extract_type(&SessionCommand::RecoverAccessMutations)?,
        extract_type(&SessionCommand::ReconcileAccessMutation {
            mutation_id: "01900000-0000-7000-8000-000000000014".to_owned(),
        })?,
        extract_type(&SessionCommand::AcknowledgeAccessMutation {
            mutation_id: "01900000-0000-7000-8000-000000000014".to_owned(),
            fingerprint: "a".repeat(64),
        })?,
        extract_type(&SessionCommand::SnapshotRefresh)?,
    ])
}

#[expect(
    clippy::too_many_lines,
    reason = "single-source protocol catalog keeps every session event exemplar exhaustive"
)]
pub(super) fn session_event_types() -> Result<Vec<String>> {
    Ok(vec![
        extract_type(&SessionEvent::List {
            sessions: vec![
                sample_runtime_remote_session_info(),
                sample_runtime_local_session_info(),
            ],
        })?,
        extract_type(&SessionEvent::Upsert {
            session: Box::new(sample_runtime_remote_session_info()),
        })?,
        extract_type(&SessionEvent::Created {
            request_id: "01900000-0000-7000-8000-000000000025".to_owned(),
            session_id: "session-1".to_owned(),
            runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
        })?,
        extract_type(&SessionEvent::Removed {
            session_id: "session-1".to_owned(),
        })?,
        extract_type(&SessionEvent::Opened {
            session_id: "session-1".to_owned(),
        })?,
        extract_type(&SessionEvent::Interrupted {
            request_id: "01900000-0000-7000-8000-000000000015".to_owned(),
            session_id: "session-1".to_owned(),
            runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
        })?,
        extract_type(&SessionEvent::HiddenList {
            request_id: "req-hidden".to_owned(),
            entries: vec![sample_hidden_session_entry()],
        })?,
        extract_type(&SessionEvent::RoomList {
            rooms: vec![sample_room_list_entry()],
        })?,
        extract_type(&SessionEvent::ActionResult {
            session_id: "session-1".to_owned(),
            action_id: "action-1".to_owned(),
            status: RelayActionStatus::Accepted,
        })?,
        extract_type(&SessionEvent::ScopeAccepted {
            request_id: "req-scope".to_owned(),
            session_id: "session-1".to_owned(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
            scope: ShareScope::Room,
            room_id: Some("room-1".to_owned()),
            budget_ms: 160_000,
        })?,
        extract_type(&SessionEvent::ScopeChanged {
            request_id: "req-scope".to_owned(),
            session_id: "session-1".to_owned(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
            scope: ShareScope::Room,
            room_id: Some("room-1".to_owned()),
        })?,
        extract_type(&SessionEvent::AccessMutationAccepted {
            mutation_id: "01900000-0000-7000-8000-000000000012".to_owned(),
            session_id: "session-1".to_owned(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
            kind: SessionAccessMutationKind::Grant,
            actor_user_id: Some("user-2".to_owned()),
            access_level: Some(AccessLevel::Suggest),
            expires_at: Some("2026-08-14T00:00:00Z".to_owned()),
        })?,
        extract_type(&SessionEvent::AccessMutationResult {
            mutation_id: "01900000-0000-7000-8000-000000000012".to_owned(),
            session_id: "session-1".to_owned(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
            kind: SessionAccessMutationKind::Grant,
            actor_user_id: Some("user-2".to_owned()),
            access_level: Some(AccessLevel::Suggest),
            expires_at: Some("2026-08-14T00:00:00Z".to_owned()),
            outcome: SessionAccessMutationOutcome::Applied,
            message: Some("access mutation completed".to_owned()),
        })?,
        extract_type(&SessionEvent::AccessMutationRecovered {
            mutation_id: "01900000-0000-7000-8000-000000000012".to_owned(),
            session_id: "session-1".to_owned(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
            originating_account_epoch: 7,
            fingerprint: "a".repeat(64),
            kind: SessionAccessMutationKind::Grant,
            actor_user_id: Some("user-2".to_owned()),
            access_level: Some(AccessLevel::Suggest),
            expires_at: Some("2026-08-14T00:00:00Z".to_owned()),
            outcome: Some(SessionAccessMutationOutcome::Applied),
            message: Some("recovered access mutation completed".to_owned()),
        })?,
        extract_type(&SessionEvent::Error {
            operation: "session.create".to_owned(),
            session_id: Some("session-1".to_owned()),
            request_id: Some("req-1".to_owned()),
            message: "boom".to_owned(),
        })?,
        extract_type(&SessionEvent::AccessGrants {
            session_id: "session-1".to_owned(),
            runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
            account_user_id: "user-1".to_owned(),
            grants: vec![AccessGrantEntry {
                actor_user_id: "user-2".to_owned(),
                handle: "alice".to_owned(),
                display_name: "Alice".to_owned(),
                access_level: AccessLevel::Suggest,
                granted_at: "2026-01-01T00:00:00Z".to_owned(),
                expires_at: Some("2026-01-02T00:00:00Z".to_owned()),
            }],
        })?,
    ])
}

#[expect(
    clippy::too_many_lines,
    reason = "single-source-of-truth catalog: every variant must be enumerated here"
)]
pub(super) fn agent_intel_command_types() -> Result<Vec<String>> {
    Ok(vec![
        extract_type(&AgentIntelCommand::ReadSettings {
            request_id: "req-1".to_owned(),
            agent: "claude".to_owned(),
            cwd: Some("/tmp/project".to_owned()),
        })?,
        extract_type(&AgentIntelCommand::ListClaudeProjects {
            request_id: "req-1".to_owned(),
        })?,
        extract_type(&AgentIntelCommand::ListProjectSessions {
            request_id: "req-1".to_owned(),
            slug: "-tmp-project".to_owned(),
        })?,
        extract_type(&AgentIntelCommand::ListProjectMemories {
            request_id: "req-1".to_owned(),
            slug: "-tmp-project".to_owned(),
        })?,
        extract_type(&AgentIntelCommand::ReadProjectMemory {
            request_id: "req-1".to_owned(),
            slug: "-tmp-project".to_owned(),
            filename: "todo.md".to_owned(),
        })?,
        extract_type(&AgentIntelCommand::ListClaudeMemory {
            request_id: "req-1".to_owned(),
            cwd: "/tmp/project".to_owned(),
        })?,
        extract_type(&AgentIntelCommand::ReadClaudeMemory {
            request_id: "req-1".to_owned(),
            cwd: "/tmp/project".to_owned(),
            filename: "todo.md".to_owned(),
        })?,
        extract_type(&AgentIntelCommand::ReadSessionConversation {
            request_id: "req-1".to_owned(),
            agent: "claude".to_owned(),
            cwd: "/tmp/project".to_owned(),
            session_id: "session-1".to_owned(),
            before_byte: Some(4096),
            max_records: Some(100),
            max_bytes: Some(262_144),
        })?,
        extract_type(&AgentIntelCommand::ListCopilotRepositories {
            request_id: "req-1".to_owned(),
        })?,
        extract_type(&AgentIntelCommand::ListCopilotRepoSessions {
            request_id: "req-1".to_owned(),
            repository: "owner/repo".to_owned(),
        })?,
        extract_type(&AgentIntelCommand::DiscoverProviderConversations {
            request_id: "req-1".to_owned(),
            provider: "claude".to_owned(),
            working_directory: "/tmp/project".to_owned(),
            cursor: Some("opaque-cursor".to_owned()),
            limit: Some(50),
            max_bytes: Some(131_072),
        })?,
        extract_type(&AgentIntelCommand::CopyProjectMemory {
            request_id: "req-1".to_owned(),
            source_slug: "-tmp-a".to_owned(),
            filename: "todo.md".to_owned(),
            target_slug: "-tmp-b".to_owned(),
        })?,
        extract_type(&AgentIntelCommand::ResolveActiveSession {
            request_id: "req-1".to_owned(),
            cwd: "/tmp/project".to_owned(),
            session_id: "01900000-0000-7000-8000-000000000001".to_owned(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000002".to_owned(),
        })?,
        extract_type(&AgentIntelCommand::ListSubagentTranscripts {
            request_id: "req-1".to_owned(),
            cwd: "/tmp/project".to_owned(),
            session_id: "sess-1".to_owned(),
        })?,
        extract_type(&AgentIntelCommand::ReadSubagentTranscript {
            request_id: "req-1".to_owned(),
            agent: "claude".to_owned(),
            path: "/tmp/project/sess-1/subagents/agent-X.jsonl".to_owned(),
            before_byte: Some(4096),
            max_records: Some(100),
            max_bytes: Some(262_144),
        })?,
        extract_type(&AgentIntelCommand::ReadClaudeAutoModeRules {
            request_id: "req-1".to_owned(),
        })?,
        extract_type(&AgentIntelCommand::WriteClaudeAutoModeRules {
            request_id: "req-1".to_owned(),
            environment: vec!["CI=1".to_owned()],
            allow: vec!["Read".to_owned()],
            soft_deny: vec!["Write".to_owned()],
            hard_deny: vec!["Bash".to_owned()],
        })?,
        extract_type(&AgentIntelCommand::ListCustomAgents {
            request_id: "req-1".to_owned(),
            directory: "/home/u/.claude/agents".to_owned(),
        })?,
        extract_type(&AgentIntelCommand::ListActiveCustomizations {
            request_id: "req-1".to_owned(),
            cwd: "/tmp/project".to_owned(),
            agent: "claude".to_owned(),
        })?,
        extract_type(&AgentIntelCommand::DiscoverExternalMcpServers {
            request_id: "req-1".to_owned(),
        })?,
        extract_type(&AgentIntelCommand::DiscoverExternalSessions {
            request_id: "req-1".to_owned(),
            agent: Some("claude".to_owned()),
        })?,
    ])
}

fn register_agent_snapshot_shape() -> Result<()> {
    let snapshot = ::agent_intel::domain::snapshot::wire_authority_sample();
    let serialized = serde_json::to_value(&snapshot)?;
    let mut types = Types::default();
    let definition = ::agent_intel::domain::AgentIntelSnapshot::definition(&mut types);
    let shape = value_shape(
        &serialized,
        Some(&definition),
        &types,
        "agent.intel.snapshot.payload",
    )?;
    MESSAGE_SHAPES.with(|shapes| {
        let mut shapes = shapes.borrow_mut();
        let event = shapes
            .get_mut("agent.intel.snapshot")
            .and_then(serde_json::Value::as_object_mut)
            .and_then(|shape| shape.get_mut("object"))
            .and_then(serde_json::Value::as_object_mut);
        if let Some(event) = event {
            event.insert("payload".to_owned(), shape);
        }
    });
    Ok(())
}

pub(super) fn agent_intel_event_types() -> Result<Vec<String>> {
    let event_types = vec![
        extract_type(&AgentIntelEvent::Snapshot {
            session_id: "session-1".to_owned(),
            session_incarnation_id: "01900000-0000-7000-8000-000000000002".to_owned(),
            payload: serde_json::Value::Null,
        })?,
        extract_type(&AgentIntelEvent::Cleared {
            session_id: "session-1".to_owned(),
            session_incarnation_id: "01900000-0000-7000-8000-000000000002".to_owned(),
        })?,
        extract_type(&AgentIntelEvent::Reply {
            request_id: "req-1".to_owned(),
            payload: serde_json::Value::Null,
        })?,
        extract_type(&AgentIntelEvent::Error {
            request_id: "req-1".to_owned(),
            message: "x".to_owned(),
        })?,
        extract_type(&AgentIntelEvent::PendingPermissionsSnapshot {
            generation: 7,
            requests: vec![ActivePendingPermission {
                session_id: "00000000-0000-0000-0000-000000000000".to_owned(),
                session_incarnation_id: "01900000-0000-7000-8000-000000000002".to_owned(),
                request_generation: 5,
                tool_use_id: "toolu_example".to_owned(),
                tool_name: "Bash".to_owned(),
                tool_input: serde_json::json!({"command": "ls"}),
                created_at_ms: 1_750_000_000_000,
                deadline_at_ms: 1_750_000_030_000,
                risk: crate::agent_intel::risk::ApprovalRisk::Unknown,
                decision_phase: PendingPermissionDecisionPhase::Actionable,
            }],
        })?,
        extract_type(&AgentIntelEvent::RemotePermissionDecisionState {
            session_id: "00000000-0000-0000-0000-000000000000".to_owned(),
            session_incarnation_id: "01900000-0000-7000-8000-000000000002".to_owned(),
            tool_use_id: "toolu_example".to_owned(),
            request_generation: 5,
            phase: RemotePermissionDecisionPhase::Sending,
            status: Some(RelayActionStatus::Accepted),
            message: Some("waiting for owner confirmation".to_owned()),
        })?,
        extract_type(&AgentIntelEvent::SteerState {
            entry: SteerQueueEntry {
                steer_id: "steer-1".to_owned(),
                account_user_id: "user-1".to_owned(),
                request_id: "01900000-0000-7000-8000-000000000003".to_owned(),
                session_incarnation_id: "01900000-0000-7000-8000-000000000002".to_owned(),
                mode: crate::host_protocol::SemanticSendMode::Steer,
                session_id: "00000000-0000-0000-0000-000000000000".to_owned(),
                text: "also update the tests".to_owned(),
                queued_at_ms: 1_700_000_000_000,
                delivery_state: SteerDeliveryState::Queued,
                at_tool_use_id: Some("toolu_example".to_owned()),
            },
            transition: SteerTransition::Queued,
            message: Some("queued for the next safe boundary".to_owned()),
        })?,
    ];
    register_agent_snapshot_shape()?;
    Ok(event_types)
}

#[expect(
    clippy::too_many_lines,
    reason = "one exhaustive sample builder keeps all AgentGlobalEvent protocol variants visible together"
)]
pub(super) fn agent_global_event_types() -> Result<Vec<String>> {
    use ::agent_intel::copilot::extensions::{
        AgentDefSummary as CopilotAgentDefSummary, McpServerSummary as CopilotMcpServerSummary,
        PluginSummary as CopilotPluginSummary, SkillSummary as CopilotSkillSummary,
    };
    use ::agent_intel::ops::dto::{
        AgentDefSummary, ClaudeGlobalStatus, CopilotGlobalStatus, McpServerSummary, PluginSummary,
        SkillSummary,
    };

    let plugins = vec![PluginSummary {
        id: "plugin-1".to_owned(),
        marketplace: "example".to_owned(),
        scope: "user".to_owned(),
        version: Some("1.0.0".to_owned()),
        installed_at: Some("2026-05-25T00:00:00Z".to_owned()),
    }];
    let skills = vec![SkillSummary {
        name: "review".to_owned(),
        source: "user".to_owned(),
        source_path: Some("/home/user/.claude/skills/review/SKILL.md".to_owned()),
        description: Some("Review code".to_owned()),
        user_invocable: true,
    }];
    let agents = vec![AgentDefSummary {
        name: "researcher".to_owned(),
        source: "user".to_owned(),
        description: Some("Research agent".to_owned()),
        disallowed_tools: vec!["Bash".to_owned()],
    }];
    let health_variants = exhaustive_mcp_health_samples();
    let degradation_notices = exhaustive_degradation_notice_samples();
    let mcp_servers = health_variants
        .iter()
        .enumerate()
        .map(|(index, health)| McpServerSummary {
            name: format!("claude-server-{index}"),
            scope: "user".to_owned(),
            transport: Some("stdio".to_owned()),
            enabled: true,
            health: Some(health.clone()),
        })
        .collect();
    let copilot_plugins = vec![CopilotPluginSummary {
        name: "plugin-1".to_owned(),
        version: Some("1.0.0".to_owned()),
        source: Some("marketplace".to_owned()),
        ..CopilotPluginSummary::default()
    }];
    let copilot_skills = vec![CopilotSkillSummary {
        name: "review".to_owned(),
        scope: "user".to_owned(),
        description: Some("Review code".to_owned()),
        ..CopilotSkillSummary::default()
    }];
    let copilot_agents = vec![CopilotAgentDefSummary {
        name: "researcher".to_owned(),
        scope: "user".to_owned(),
        description: Some("Research agent".to_owned()),
        tools: vec!["read".to_owned()],
        ..CopilotAgentDefSummary::default()
    }];
    let copilot_mcp_servers = health_variants
        .iter()
        .enumerate()
        .map(|(index, health)| CopilotMcpServerSummary {
            name: format!("copilot-server-{index}"),
            scope: "user".to_owned(),
            command: Some("mcp-filesystem".to_owned()),
            health: Some(health.clone()),
            ..CopilotMcpServerSummary::default()
        })
        .collect();
    let mut event_types = vec![
        extract_type(&AgentGlobalEvent::ClaudeStatus {
            generation: Some(7),
            status: ClaudeGlobalStatus {
                cwd: Some("/tmp/kodosi".to_owned()),
                claude_code_version: Some("2.0.0".to_owned()),
                installed_plugins: plugins,
                loaded_skills: skills,
                loaded_agents: agents,
                mcp_servers,
                notices: Some(degradation_notices.clone()),
            },
        })?,
        extract_type(&AgentGlobalEvent::CopilotStatus {
            generation: Some(7),
            status: CopilotGlobalStatus {
                cwd: Some("/tmp/kodosi".to_owned()),
                copilot_cli_version: Some("0.1.0".to_owned()),
                installed_plugins: copilot_plugins,
                loaded_skills: copilot_skills,
                loaded_agents: copilot_agents,
                mcp_servers: copilot_mcp_servers,
                notices: Some(degradation_notices),
            },
        })?,
    ];
    for health in health_variants {
        let message_type = extract_type(&AgentGlobalEvent::McpHealth {
            vendor: "claude".to_owned(),
            scope: "user".to_owned(),
            server_name: "filesystem".to_owned(),
            health,
        })?;
        if !event_types.contains(&message_type) {
            event_types.push(message_type);
        }
    }
    Ok(event_types)
}

fn exhaustive_mcp_health_samples() -> Vec<agent_intel::mcp::McpHealth> {
    vec![
        agent_intel::mcp::McpHealth::Unknown,
        agent_intel::mcp::McpHealth::Healthy,
        agent_intel::mcp::McpHealth::Unreachable {
            reason: Some("connection refused".to_owned()),
        },
        agent_intel::mcp::McpHealth::Misconfigured {
            reason: Some("missing executable".to_owned()),
        },
    ]
}

fn exhaustive_degradation_notice_samples() -> Vec<agent_intel::domain::DegradationNotice> {
    use agent_intel::domain::{CopilotSessionStoreFailureKind, DegradationNotice};

    vec![
        DegradationNotice::UnknownClaudeSubtype {
            subtype: "future_private_record".to_owned(),
        },
        DegradationNotice::MalformedSettings {
            vendor: "claude".to_owned(),
            path: "/home/u/.claude/settings.json".to_owned(),
            message: "expected value".to_owned(),
        },
        DegradationNotice::AgentParseFailed {
            vendor: "copilot".to_owned(),
            path: "/repo/.github/agents/reviewer.agent.md".to_owned(),
            message: "missing description".to_owned(),
        },
        DegradationNotice::UnsupportedCopilotSessionStore {
            schema_version: Some(99),
            supported_versions: vec![2, 3],
        },
        DegradationNotice::CopilotSessionStoreFailure {
            failure: CopilotSessionStoreFailureKind::Locked,
            message: "database is locked".to_owned(),
        },
        DegradationNotice::MalformedCopilotEvent {
            message: "invalid JSON".to_owned(),
        },
        DegradationNotice::TerminalFallback {
            fields: vec!["status".to_owned()],
        },
    ]
}

pub(super) fn auth_command_types() -> Result<Vec<String>> {
    Ok(vec![
        extract_type(&AuthCommand::LoginStart)?,
        extract_type(&AuthCommand::Logout)?,
        extract_type(&AuthCommand::IdentityReset)?,
        extract_type(&AuthCommand::Refresh)?,
    ])
}

pub(super) fn auth_event_types() -> Result<Vec<String>> {
    Ok(vec![
        extract_type(&AuthEvent::Ready {
            user_id: Some("11111111-1111-1111-1111-111111111111".to_owned()),
            account_epoch: 1,
        })?,
        extract_type(&AuthEvent::Required {
            reason: AuthRequiredReason::SignedOut,
            account_epoch: 1,
        })?,
        extract_type(&AuthEvent::DeviceCode {
            user_code: "ABCD-EFGH".to_owned(),
            verification_uri: "https://example.com/device".to_owned(),
        })?,
        extract_type(&AuthEvent::Finalizing)?,
        extract_type(&AuthEvent::Notice {
            message: Some("device registered".to_owned()),
        })?,
        extract_type(&AuthEvent::IdentityHealth {
            state: crate::host_protocol::IdentityHealthState::RecoveryRequired,
            message: Some("device identity needs recovery".to_owned()),
        })?,
        extract_type(&AuthEvent::Error {
            operation: "login.start".to_owned(),
            message: "backend unavailable".to_owned(),
        })?,
    ])
}

pub(super) fn auth_ready_keys() -> Vec<&'static str> {
    vec!["type", "userId", "accountEpoch"]
}

pub(super) fn auth_required_reasons() -> Result<Vec<String>> {
    Ok(vec![
        extract_wire_string(&AuthRequiredReason::SignedOut)?,
        extract_wire_string(&AuthRequiredReason::Expired)?,
    ])
}

pub(super) fn runtime_session_statuses() -> Result<Vec<String>> {
    Ok(vec![
        extract_wire_string(&RuntimeSessionStatus::Active)?,
        extract_wire_string(&RuntimeSessionStatus::Blocked)?,
        extract_wire_string(&RuntimeSessionStatus::Waiting)?,
        extract_wire_string(&RuntimeSessionStatus::Reconnecting)?,
        extract_wire_string(&RuntimeSessionStatus::Stopping)?,
        extract_wire_string(&RuntimeSessionStatus::Stopped)?,
    ])
}

pub(super) fn session_scopes() -> Result<Vec<String>> {
    Ok(vec![
        extract_wire_string(&ShareScope::JustMe)?,
        extract_wire_string(&ShareScope::MyDevices)?,
        extract_wire_string(&ShareScope::Friends)?,
        extract_wire_string(&ShareScope::Room)?,
    ])
}

pub(super) fn session_access_levels() -> Result<Vec<String>> {
    Ok(vec![
        extract_wire_string(&AccessLevel::View)?,
        extract_wire_string(&AccessLevel::Suggest)?,
        extract_wire_string(&AccessLevel::Inject)?,
        extract_wire_string(&AccessLevel::Approve)?,
    ])
}

pub(super) fn relay_action_statuses() -> Result<Vec<String>> {
    Ok(vec![
        extract_wire_string(&RelayActionStatus::Accepted)?,
        extract_wire_string(&RelayActionStatus::Duplicate)?,
        extract_wire_string(&RelayActionStatus::Busy)?,
        extract_wire_string(&RelayActionStatus::Rejected)?,
    ])
}

pub(super) fn semantic_send_modes() -> Result<Vec<String>> {
    Ok(vec![
        extract_wire_string(&crate::host_protocol::SemanticSendMode::Queue)?,
        extract_wire_string(&crate::host_protocol::SemanticSendMode::Steer)?,
        extract_wire_string(&crate::host_protocol::SemanticSendMode::StopAndSend)?,
    ])
}

pub(super) fn steer_transitions() -> Result<Vec<String>> {
    Ok(vec![
        extract_wire_string(&SteerTransition::Queued)?,
        extract_wire_string(&SteerTransition::Sending)?,
        extract_wire_string(&SteerTransition::Injected)?,
        extract_wire_string(&SteerTransition::Failed)?,
        extract_wire_string(&SteerTransition::Cancelled)?,
        extract_wire_string(&SteerTransition::DeliveryUnknown)?,
    ])
}

pub(super) fn steer_delivery_states() -> Result<Vec<String>> {
    Ok(vec![
        extract_wire_string(&SteerDeliveryState::Preparing)?,
        extract_wire_string(&SteerDeliveryState::Queued)?,
        extract_wire_string(&SteerDeliveryState::DeliveryUnknown)?,
        extract_wire_string(&SteerDeliveryState::Injected)?,
        extract_wire_string(&SteerDeliveryState::Cancelled)?,
    ])
}

pub(super) fn runtime_session_modes() -> Result<Vec<String>> {
    Ok(vec![
        extract_wire_string(&SessionMode::Normal)?,
        extract_wire_string(&SessionMode::Plan)?,
        extract_wire_string(&SessionMode::Autopilot)?,
    ])
}

pub(super) fn remote_session_connection_states() -> Result<Vec<String>> {
    Ok(vec![
        extract_wire_string(&ConnectionState::Connecting)?,
        extract_wire_string(&ConnectionState::Connected)?,
        extract_wire_string(&ConnectionState::Reconnecting)?,
        extract_wire_string(&ConnectionState::Offline)?,
    ])
}

pub(super) fn remote_session_access_states() -> Result<Vec<String>> {
    Ok(vec![
        extract_wire_string(&RemoteSessionAccessState::RegisteringDevice)?,
        extract_wire_string(&RemoteSessionAccessState::AwaitingKey)?,
        extract_wire_string(&RemoteSessionAccessState::Ready)?,
        extract_wire_string(&RemoteSessionAccessState::AccessDenied)?,
        extract_wire_string(&RemoteSessionAccessState::Failed)?,
    ])
}

pub(super) fn remote_session_access_issues() -> Result<Vec<String>> {
    Ok(vec![extract_wire_string(
        &RemoteSessionAccessIssue::PeerIdentityChanged,
    )?])
}

pub(super) fn friends_command_types() -> Result<Vec<String>> {
    Ok(vec![
        extract_type(&FriendsCommand::Refresh)?,
        extract_type(&FriendsCommand::RequestSend {
            username: "alice".to_owned(),
            request_id: "01900000-0000-7000-8000-000000000001".to_owned(),
        })?,
        extract_type(&FriendsCommand::RequestAccept {
            username: "alice".to_owned(),
        })?,
        extract_type(&FriendsCommand::RequestCancel {
            username: "bob".to_owned(),
        })?,
        extract_type(&FriendsCommand::RequestReject {
            username: "alice".to_owned(),
        })?,
        extract_type(&FriendsCommand::Remove {
            username: "alice".to_owned(),
        })?,
    ])
}

pub(super) fn friends_event_types() -> Result<Vec<String>> {
    Ok(vec![
        extract_type(&FriendsEvent::Snapshot {
            friends: vec![sample_friend_entry()],
            incoming: vec![sample_friend_request_entry()],
            outgoing: vec![sample_friend_request_entry()],
            request_id: Some("01900000-0000-7000-8000-000000000001".to_owned()),
        })?,
        extract_type(&FriendsEvent::Error {
            operation: "refresh".to_owned(),
            message: "backend unavailable".to_owned(),
            request_id: Some("01900000-0000-7000-8000-000000000001".to_owned()),
        })?,
    ])
}

pub(super) fn device_command_types() -> Result<Vec<String>> {
    Ok(vec![
        extract_type(&DeviceCommand::Refresh)?,
        extract_type(&DeviceCommand::Revoke {
            device_id: "device-2".to_owned(),
        })?,
        extract_type(&DeviceCommand::LinkApprove {
            user_code: "ABCD-EFGH".to_owned(),
        })?,
        extract_type(&DeviceCommand::LinkStartSelf)?,
        extract_type(&DeviceCommand::LinkCancelSelf)?,
    ])
}

pub(super) fn device_event_types() -> Result<Vec<String>> {
    Ok(vec![
        extract_type(&DeviceEvent::List {
            self_device_id: "device-self".to_owned(),
            local_device_enrolled: true,
            devices: vec![sample_my_device_entry()],
        })?,
        extract_type(&DeviceEvent::LinkSnapshot {
            requests: vec![DeviceLinkRequestEntry {
                user_code: "ABCD-EFGH".to_owned(),
                device_label: "iPhone".to_owned(),
                expires_at: "2026-04-24T10:00:00Z".to_owned(),
            }],
        })?,
        extract_type(&DeviceEvent::LinkRequested {
            user_code: "ABCD-EFGH".to_owned(),
            device_label: "iPhone".to_owned(),
            expires_at: "2026-04-24T10:00:00Z".to_owned(),
        })?,
        extract_type(&DeviceEvent::LinkResolved {
            user_code: "ABCD-EFGH".to_owned(),
            outcome: crate::DeviceLinkOutcome::Approved,
        })?,
        extract_type(&DeviceEvent::LinkSelfPending {
            user_code: "ABCD-EFGH".to_owned(),
            expires_at: "2026-04-24T10:00:00Z".to_owned(),
        })?,
        extract_type(&DeviceEvent::LinkSelfResolved {
            outcome: crate::SelfDeviceLinkOutcome::Approved,
        })?,
        extract_type(&DeviceEvent::Error {
            user_code: Some("ABCD-EFGH".to_owned()),
            operation: "refresh".to_owned(),
            message: "backend unavailable".to_owned(),
        })?,
    ])
}

pub(super) fn device_link_outcomes() -> Result<Vec<String>> {
    Ok(vec![
        extract_wire_string(&crate::DeviceLinkOutcome::Approved)?,
        extract_wire_string(&crate::DeviceLinkOutcome::Cancelled)?,
    ])
}

pub(super) fn self_device_link_outcomes() -> Result<Vec<String>> {
    Ok(vec![
        extract_wire_string(&crate::SelfDeviceLinkOutcome::Approved)?,
        extract_wire_string(&crate::SelfDeviceLinkOutcome::Cancelled)?,
        extract_wire_string(&crate::SelfDeviceLinkOutcome::Expired)?,
        extract_wire_string(&crate::SelfDeviceLinkOutcome::Failed)?,
    ])
}

pub(super) fn trust_command_types() -> Result<Vec<String>> {
    Ok(vec![
        extract_type(&TrustCommand::Refresh {
            request_id: "request-1".to_owned(),
        })?,
        extract_type(&TrustCommand::Reset {
            request_id: "request-1".to_owned(),
            user_id: "user-1".to_owned(),
        })?,
    ])
}

pub(super) fn trust_event_types() -> Result<Vec<String>> {
    Ok(vec![
        extract_type(&TrustEvent::Snapshot {
            request_id: "request-1".to_owned(),
            pins: vec![sample_trust_pin_entry()],
        })?,
        extract_type(&TrustEvent::Reset {
            request_id: "request-1".to_owned(),
            user_id: "user-1".to_owned(),
            cleared: true,
        })?,
        extract_type(&TrustEvent::Error {
            request_id: Some("request-1".to_owned()),
            user_id: Some("user-1".to_owned()),
            operation: "refresh".to_owned(),
            message: "pin store unavailable".to_owned(),
        })?,
    ])
}

pub(super) fn room_command_types() -> Result<Vec<String>> {
    Ok(vec![
        extract_type(&RoomCommand::Refresh)?,
        extract_type(&RoomCommand::Create {
            name: "Acme".into(),
            slug: "acme".into(),
            request_id: Some("01900000-0000-7000-8000-000000000101".into()),
        })?,
        extract_type(&RoomCommand::RemoveMember {
            room_id: "r1".into(),
            user_id: "u1".into(),
            expected_roster_generation: 7,
            request_id: "01900000-0000-7000-8000-000000000102".into(),
        })?,
        extract_type(&RoomCommand::Invite {
            room_id: "r1".into(),
            invitee_user_id: "u1".into(),
            request_id: Some("01900000-0000-7000-8000-000000000103".into()),
        })?,
        extract_type(&RoomCommand::AcceptInvitation {
            invitation_id: "i1".into(),
            room_id: "r1".into(),
            expected_roster_generation: 8,
            request_id: "01900000-0000-7000-8000-000000000104".into(),
        })?,
        extract_type(&RoomCommand::DeclineInvitation {
            invitation_id: "i1".into(),
            room_id: "r1".into(),
            expected_roster_generation: 8,
            request_id: "01900000-0000-7000-8000-000000000105".into(),
        })?,
        extract_type(&RoomCommand::CancelInvitation {
            invitation_id: "i1".into(),
            room_id: "r1".into(),
            expected_roster_generation: 8,
            request_id: "01900000-0000-7000-8000-000000000106".into(),
        })?,
        extract_type(&RoomCommand::RefreshMembers {
            room_id: "r1".into(),
            hydration_id: "01900000-0000-7000-8000-000000000117".into(),
        })?,
        extract_type(&RoomCommand::RefreshInvitations)?,
        extract_type(&RoomCommand::RecoverMutations)?,
        extract_type(&RoomCommand::ReconcileMutation {
            request_id: "01900000-0000-7000-8000-000000000116".into(),
        })?,
        extract_type(&RoomCommand::ChatList {
            room_id: "r1".into(),
            since: Some(1),
            limit: Some(100),
            tail: Some(false),
            hydration_id: "01900000-0000-7000-8000-000000000118".into(),
        })?,
        extract_type(&RoomCommand::ChatPost {
            room_id: "r1".into(),
            body: "hi".into(),
            author_session_id: Some("session-1".into()),
            recipient_session_ids: vec!["01900000-0000-7000-8000-000000000001".into()],
            recipient_user_ids: vec!["01900000-0000-7000-8000-000000000002".into()],
            request_id: Some("01900000-0000-7000-8000-000000000107".into()),
        })?,
        extract_type(&RoomCommand::TasksList {
            room_id: "r1".into(),
            status: Some("Open".into()),
            assignee: Some("session-1".into()),
            offset: Some(0),
            limit: Some(100),
            hydration_id: Some("01900000-0000-7000-8000-000000000117".into()),
        })?,
        extract_type(&RoomCommand::TaskCreate {
            room_id: "r1".into(),
            title: "build".into(),
            description: Some("Build the release".into()),
            assigned_session_id: Some("session-1".into()),
            assigned_session_incarnation_id: Some("01900000-0000-7000-8000-000000000115".into()),
            due_at: Some("2026-05-26T00:00:00Z".into()),
            request_id: Some("01900000-0000-7000-8000-000000000108".into()),
        })?,
        extract_type(&RoomCommand::TaskTransition {
            room_id: "r1".into(),
            task_id: "t1".into(),
            expected_task_revision: 9,
            to_status: "Open".into(),
            actor_session_id: Some("01900000-0000-7000-8000-000000000111".into()),
            actor_session_incarnation_id: Some("01900000-0000-7000-8000-000000000112".into()),
            result: Some("done".into()),
            request_id: "01900000-0000-7000-8000-000000000109".into(),
        })?,
        extract_type(&RoomCommand::TaskAssign {
            room_id: "r1".into(),
            task_id: "t1".into(),
            expected_task_revision: 9,
            session_id: Some("01900000-0000-7000-8000-000000000113".into()),
            session_incarnation_id: Some("01900000-0000-7000-8000-000000000114".into()),
            request_id: "01900000-0000-7000-8000-000000000110".into(),
        })?,
    ])
}

pub(super) fn room_event_types() -> Result<Vec<String>> {
    Ok(vec![
        extract_type(&RoomEvent::Snapshot {
            rooms: vec![sample_room_entry()],
        })?,
        extract_type(&RoomEvent::Invitations {
            incoming: vec![sample_invitation_entry()],
            outgoing: vec![sample_invitation_entry()],
        })?,
        extract_type(&RoomEvent::Members {
            room_id: "r1".into(),
            members: vec![sample_member_entry()],
            hydration_id: Some("01900000-0000-7000-8000-000000000117".into()),
        })?,
        extract_type(&RoomEvent::ChatSnapshot {
            room_id: "r1".into(),
            messages: vec![sample_chat_entry()],
            hydration_id: Some("01900000-0000-7000-8000-000000000118".into()),
        })?,
        extract_type(&RoomEvent::ChatPosted {
            room_id: "r1".into(),
            message: sample_chat_entry(),
        })?,
        extract_type(&RoomEvent::TasksPage {
            room_id: "r1".into(),
            tasks: vec![sample_task_entry()],
            has_more: true,
            next_offset: Some(1),
            hydration_id: Some("01900000-0000-7000-8000-000000000117".into()),
            request_offset: Some(0),
        })?,
        extract_type(&RoomEvent::TasksSnapshot {
            room_id: "r1".into(),
            tasks: vec![sample_task_entry()],
        })?,
        extract_type(&RoomEvent::TaskUpserted {
            room_id: "r1".into(),
            task: sample_task_entry(),
        })?,
        extract_type(&RoomEvent::AgentDelivery {
            session_id: "01900000-0000-7000-8000-000000000001".into(),
            session_incarnation_id: "01900000-0000-7000-8000-000000000002".into(),
            state: crate::host_protocol::RoomAgentDeliveryState::AcceptedByTransport,
            event_id: Some("chat:r1:7".into()),
            detail: Some("session-scoped adapter accepted the event".into()),
        })?,
        extract_type(&RoomEvent::MutationRecovered {
            request_id: "01900000-0000-7000-8000-000000000001".into(),
            operation: "tasks.assign".into(),
            room_id: "r1".into(),
            fingerprint: "abc".into(),
            status: Some(crate::host_protocol::RoomActionStatus::Succeeded),
            entity_id: Some("t1".into()),
            message: Some("recovered".into()),
        })?,
        extract_type(&RoomEvent::ActionAccepted {
            request_id: "01900000-0000-7000-8000-000000000001".into(),
            operation: "tasks.assign".into(),
            room_id: "r1".into(),
            fingerprint: "abc".into(),
        })?,
        extract_type(&RoomEvent::ActionResult {
            request_id: "req-room-action".into(),
            operation: "tasks.create".into(),
            room_id: Some("r1".into()),
            fingerprint: Some("abc".into()),
            status: crate::host_protocol::RoomActionStatus::Succeeded,
            entity_id: Some("t1".into()),
            message: Some("created".into()),
        })?,
        extract_type(&RoomEvent::Error {
            room_id: Some("r1".to_owned()),
            operation: "refresh".to_owned(),
            message: "backend unavailable".to_owned(),
        })?,
    ])
}

pub(super) fn room_action_statuses() -> Result<Vec<String>> {
    Ok(vec![
        extract_wire_string(&crate::host_protocol::RoomActionStatus::Succeeded)?,
        extract_wire_string(&crate::host_protocol::RoomActionStatus::Failed)?,
        extract_wire_string(&crate::host_protocol::RoomActionStatus::Unknown)?,
        extract_wire_string(&crate::host_protocol::RoomActionStatus::Conflict)?,
    ])
}

fn sample_chat_entry() -> crate::host_protocol::RoomChatEntry {
    crate::host_protocol::RoomChatEntry {
        id: "m1".into(),
        room_id: "r1".into(),
        author_user_id: "u1".into(),
        author_session_id: Some("session-1".into()),
        author_kind: "Human".into(),
        body: "hi".into(),
        recipient_session_ids: vec!["01900000-0000-7000-8000-000000000001".into()],
        recipient_user_ids: vec!["01900000-0000-7000-8000-000000000002".into()],
        seq: 1,
        posted_at: "2026-05-25T00:00:00Z".into(),
    }
}

fn sample_member_entry() -> crate::host_protocol::RoomMemberEntry {
    crate::host_protocol::RoomMemberEntry {
        room_id: "r1".into(),
        user_id: "u1".into(),
        role: "Member".into(),
        username: Some("alice".into()),
        display_name: Some("Alice".into()),
    }
}

fn sample_invitation_entry() -> crate::host_protocol::RoomInvitationEntry {
    crate::host_protocol::RoomInvitationEntry {
        id: "i1".into(),
        room_id: "r1".into(),
        room_name: "Acme".into(),
        room_slug: "acme".into(),
        invitee_user_id: "u2".into(),
        invitee_handle: "bob".into(),
        invitee_display_name: Some("Bob".into()),
        invited_by_user_id: "u1".into(),
        invited_by_handle: "alice".into(),
        invited_by_display_name: Some("Alice".into()),
        status: "Pending".into(),
        base_roster_generation: 8,
        proposed_roster_generation: 9,
        created_at: "2026-05-25T00:00:00Z".into(),
    }
}

pub(super) fn room_chat_entry_keys() -> Vec<&'static str> {
    vec![
        "id",
        "roomId",
        "authorUserId",
        "authorSessionId",
        "authorKind",
        "body",
        "recipientSessionIds",
        "recipientUserIds",
        "seq",
        "postedAt",
    ]
}

fn sample_task_entry() -> crate::host_protocol::RoomTaskEntry {
    crate::host_protocol::RoomTaskEntry {
        id: "t1".into(),
        room_id: "r1".into(),
        created_by_user_id: "u1".into(),
        title: "build".into(),
        description: Some("Build the release".into()),
        status: "Open".into(),
        revision: 9,
        assigned_session_id: Some("session-1".into()),
        assigned_session_incarnation_id: Some("01900000-0000-7000-8000-000000000115".into()),
        due_at: Some("2026-05-26T00:00:00Z".into()),
        created_at: "2026-05-25T00:00:00Z".into(),
        updated_at: "2026-05-25T00:00:00Z".into(),
        completed_at: Some("2026-05-25T01:00:00Z".into()),
        result: Some("done".into()),
        result_author_user_id: Some("u2".into()),
    }
}

fn extract_type<T: Serialize + Type>(value: &T) -> Result<String> {
    let serialized = serde_json::to_value(value)?;
    let message_type = serialized
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| AppError::InvalidBackendData {
            field: "type".to_owned(),
            reason: "serialized protocol sample did not include a string type".to_owned(),
        })?
        .to_owned();
    let mut types = Types::default();
    let definition = T::definition(&mut types);
    let shape = value_shape(&serialized, Some(&definition), &types, &message_type)?;
    MESSAGE_SHAPES.with(|shapes| {
        let mut shapes = shapes.borrow_mut();
        shapes
            .entry(message_type.clone())
            .and_modify(|existing| *existing = merge_shape(existing, &shape))
            .or_insert(shape);
    });
    Ok(message_type)
}

struct ObjectFields<'a> {
    groups: Vec<&'a NamedFields>,

    tag: Option<String>,
}

impl<'a> ObjectFields<'a> {
    fn find(&self, wire_name: &str) -> Option<&'a Field> {
        self.groups
            .iter()
            .find_map(|fields| find_wire_field(fields, wire_name))
    }

    fn is_tag(&self, wire_name: &str) -> bool {
        self.tag.as_deref() == Some(wire_name)
    }
}

fn value_shape(
    value: &serde_json::Value,
    ty: Option<&DataType>,
    types: &Types,
    path: &str,
) -> Result<serde_json::Value> {
    match value {
        serde_json::Value::Null => Ok(serde_json::Value::String("null".to_owned())),
        serde_json::Value::Bool(_) => Ok(serde_json::Value::String("boolean".to_owned())),
        serde_json::Value::Number(_) => Ok(serde_json::Value::String("number".to_owned())),
        serde_json::Value::String(_) => Ok(serde_json::Value::String("string".to_owned())),
        serde_json::Value::Array(items) => {
            let element_type = ty.and_then(|ty| list_element_type(ty, types));
            let mut shapes = Vec::new();
            for item in items {
                let shape = value_shape(item, element_type, types, path)?;
                if !shapes.contains(&shape) {
                    shapes.push(shape);
                }
            }
            Ok(serde_json::json!({ "array": shapes }))
        }
        serde_json::Value::Object(values) => object_shape(values, ty, types, path),
    }
}

fn object_shape(
    values: &serde_json::Map<String, serde_json::Value>,
    ty: Option<&DataType>,
    types: &Types,
    path: &str,
) -> Result<serde_json::Value> {
    let fields = match ty {
        Some(ty) => object_fields(ty, types, values, path)?,
        None => None,
    };
    let mut required_fields = BTreeSet::new();
    let mut optional_fields = BTreeSet::new();
    let mut nullable_fields = BTreeSet::new();
    let mut object = BTreeMap::new();

    for (key, value) in values {
        let field = fields.as_ref().and_then(|fields| fields.find(key));
        let field_type = field.and_then(|field| field.ty.as_ref());
        object.insert(
            key.clone(),
            value_shape(value, field_type, types, &format!("{path}.{key}"))?,
        );

        if field.is_some_and(|field| field_is_optional(field, types)) {
            optional_fields.insert(key.clone());
        } else {
            required_fields.insert(key.clone());
        }
        if field.is_some_and(|field| field_is_nullable(field, types)) {
            nullable_fields.insert(key.clone());
        }
    }

    let mut shape = serde_json::json!({
        "object": object,
        "requiredFields": required_fields,
        "optionalFields": optional_fields,
        "nullableFields": nullable_fields,
    });
    if let Some((tag, value)) = fields
        .as_ref()
        .and_then(|fields| fields.tag.as_deref())
        .filter(|tag| *tag != "type")
        .and_then(|tag| {
            values
                .get(tag)
                .and_then(serde_json::Value::as_str)
                .map(|value| (tag, value))
        })
    {
        let Some(shape) = shape.as_object_mut() else {
            return Err(AppError::InvalidBackendData {
                field: path.to_owned(),
                reason: "generated object shape was not an object".to_owned(),
            });
        };
        shape.insert(
            "discriminator".to_owned(),
            serde_json::json!({"field": tag, "value": value}),
        );
    }
    Ok(shape)
}

fn merge_shape(existing: &serde_json::Value, new: &serde_json::Value) -> serde_json::Value {
    if existing == new {
        return existing.clone();
    }
    let (Some(existing_object), Some(new_object)) = (existing.as_object(), new.as_object()) else {
        return union_shape(existing, new);
    };

    if existing_object.contains_key("array") && new_object.contains_key("array") {
        let mut items = existing_object["array"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for item in new_object["array"].as_array().into_iter().flatten() {
            if !items.contains(item) {
                items.push(item.clone());
            }
        }
        return serde_json::json!({"array": items});
    }

    let both_are_objects =
        existing_object.contains_key("object") && new_object.contains_key("object");
    if !both_are_objects {
        return union_shape(existing, new);
    }

    let enum_variants = existing_object.contains_key("discriminator")
        || new_object.contains_key("discriminator")
        || existing_object.contains_key("union")
        || new_object.contains_key("union");
    let mut merged = merge_object_shape(existing, new);
    if enum_variants {
        let variants = union_members(existing)
            .into_iter()
            .chain(union_members(new))
            .fold(Vec::new(), |mut variants, variant| {
                if !variants.contains(&variant) {
                    variants.push(variant);
                }
                variants
            });
        if let Some(object) = merged.as_object_mut() {
            object.remove("discriminator");
            object.insert("union".to_owned(), serde_json::Value::Array(variants));
        }
    }
    merged
}

fn merge_object_shape(existing: &serde_json::Value, new: &serde_json::Value) -> serde_json::Value {
    let (Some(existing_object), Some(new_object)) = (existing.as_object(), new.as_object()) else {
        return union_shape(existing, new);
    };
    let mut fields = existing_object["object"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    for (key, new_shape) in new_object["object"].as_object().into_iter().flatten() {
        if let Some(existing_shape) = fields.get_mut(key) {
            *existing_shape = merge_shape(existing_shape, new_shape);
        } else {
            fields.insert(key.clone(), new_shape.clone());
        }
    }

    let existing_required = shape_string_set(existing_object, "requiredFields");
    let new_required = shape_string_set(new_object, "requiredFields");
    let mut required = existing_required;
    required.extend(new_required);
    let mut optional = shape_string_set(existing_object, "optionalFields");
    optional.extend(shape_string_set(new_object, "optionalFields"));
    optional.retain(|field| !required.contains(field));
    let mut nullable = shape_string_set(existing_object, "nullableFields");
    nullable.extend(shape_string_set(new_object, "nullableFields"));

    let mut merged = serde_json::json!({
        "object": fields,
        "requiredFields": required,
        "optionalFields": optional,
        "nullableFields": nullable,
    });
    if existing_object.get("discriminator") == new_object.get("discriminator")
        && let Some(discriminator) = existing_object.get("discriminator")
        && let Some(merged) = merged.as_object_mut()
    {
        merged.insert("discriminator".to_owned(), discriminator.clone());
    }
    merged
}

fn shape_string_set(
    shape: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> BTreeSet<String> {
    shape
        .get(key)
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn union_members(shape: &serde_json::Value) -> Vec<serde_json::Value> {
    shape
        .get("union")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_else(|| vec![shape.clone()])
}

fn union_shape(existing: &serde_json::Value, new: &serde_json::Value) -> serde_json::Value {
    let mut variants = union_members(existing);
    for variant in union_members(new) {
        if !variants.contains(&variant) {
            variants.push(variant);
        }
    }
    serde_json::json!({"union": variants})
}

fn object_fields<'a>(
    ty: &'a DataType,
    types: &'a Types,
    values: &serde_json::Map<String, serde_json::Value>,
    path: &str,
) -> Result<Option<ObjectFields<'a>>> {
    let Some(resolved) = resolve_data_type(ty, types) else {
        return Ok(None);
    };
    let fields = match resolved {
        DataType::Struct(structure) => match &structure.fields {
            Fields::Named(fields) => Some(ObjectFields {
                groups: field_groups(fields, types, values),
                tag: None,
            }),
            Fields::Unit | Fields::Unnamed(_) => None,
        },
        DataType::Enum(enumeration) => {
            let candidates = variant_candidates(enumeration, types, values);

            if candidates
                .iter()
                .all(|candidate| candidate.groups.is_empty())
            {
                None
            } else {
                Some(select_variant(&candidates, values).ok_or_else(|| {
                    AppError::InvalidBackendData {
                        field: path.to_owned(),
                        reason: format!(
                            "no enum variant matches the serialized sample keys {:?}",
                            values.keys().collect::<Vec<_>>()
                        ),
                    }
                })?)
            }
        }
        _ => None,
    };

    let Some(fields) = fields else {
        return Ok(None);
    };
    if let Some(unknown) = values
        .keys()
        .find(|key| !fields.is_tag(key) && fields.find(key).is_none())
    {
        return Err(AppError::InvalidBackendData {
            field: path.to_owned(),
            reason: format!(
                "serialized key `{unknown}` has no resolved field metadata, so presence classification would be wrong"
            ),
        });
    }
    Ok(Some(fields))
}

struct VariantCandidate<'a> {
    name: &'a str,
    groups: Vec<&'a NamedFields>,
}

fn variant_candidates<'a>(
    enumeration: &'a specta::datatype::Enum,
    types: &'a Types,
    values: &serde_json::Map<String, serde_json::Value>,
) -> Vec<VariantCandidate<'a>> {
    enumeration
        .variants
        .iter()
        .filter(|(_, variant)| !variant.skip)
        .filter_map(|(name, variant)| {
            let groups = match &variant.fields {
                Fields::Named(fields) => field_groups(fields, types, values),
                Fields::Unit => Vec::new(),
                Fields::Unnamed(fields) => match fields.fields.as_slice() {
                    [field] => field
                        .ty
                        .as_ref()
                        .and_then(|ty| struct_fields(ty, types))
                        .map(|fields| field_groups(fields, types, values))?,
                    _ => return None,
                },
            };
            Some(VariantCandidate {
                name: name.as_ref(),
                groups,
            })
        })
        .collect()
}

fn select_variant<'a>(
    candidates: &[VariantCandidate<'a>],
    values: &serde_json::Map<String, serde_json::Value>,
) -> Option<ObjectFields<'a>> {
    let covers = |candidate: &VariantCandidate<'a>, tag: Option<&str>| {
        values
            .keys()
            .filter(|key| Some(key.as_str()) != tag)
            .all(|key| {
                candidate
                    .groups
                    .iter()
                    .any(|fields| find_wire_field(fields, key).is_some())
            })
    };

    let mut tags = values
        .iter()
        .filter(|(_, value)| value.is_string())
        .map(|(key, _)| Some(key.as_str()))
        .collect::<Vec<_>>();
    tags.sort_by_key(|tag| *tag != Some("type"));
    tags.push(None);

    for tag in tags {
        let mut covering = candidates
            .iter()
            .filter(|candidate| covers(candidate, tag))
            .collect::<Vec<_>>();
        if covering.is_empty() {
            continue;
        }
        let normalized_tag = tag
            .and_then(|tag| values.get(tag))
            .and_then(serde_json::Value::as_str)
            .map(normalized_identifier);
        covering.sort_by_key(|candidate| std::cmp::Reverse(candidate.name.len()));
        let named = normalized_tag.and_then(|normalized_tag| {
            covering
                .iter()
                .find(|candidate| normalized_tag.ends_with(&normalized_identifier(candidate.name)))
                .copied()
        });
        let chosen = named.or_else(|| {
            covering
                .iter()
                .min_by_key(|candidate| {
                    candidate
                        .groups
                        .iter()
                        .map(|fields| fields.fields.len())
                        .sum::<usize>()
                })
                .copied()
        })?;
        return Some(ObjectFields {
            groups: chosen.groups.clone(),
            tag: tag.map(ToOwned::to_owned),
        });
    }
    None
}

fn field_groups<'a>(
    fields: &'a NamedFields,
    types: &'a Types,
    values: &serde_json::Map<String, serde_json::Value>,
) -> Vec<&'a NamedFields> {
    let mut groups = Vec::new();
    collect_field_groups(fields, types, values, &mut groups);
    groups
}

fn collect_field_groups<'a>(
    fields: &'a NamedFields,
    types: &'a Types,
    values: &serde_json::Map<String, serde_json::Value>,
    groups: &mut Vec<&'a NamedFields>,
) {
    if groups.iter().any(|seen| std::ptr::eq(*seen, fields)) {
        return;
    }
    groups.push(fields);
    for (name, field) in &fields.fields {
        if !is_flattened(name, field, types, values) {
            continue;
        }
        let Some(nested) = field.ty.as_ref().and_then(|ty| struct_fields(ty, types)) else {
            continue;
        };
        if nested
            .fields
            .iter()
            .any(|(name, _)| values.contains_key(snake_to_camel(name).as_str()))
        {
            collect_field_groups(nested, types, values, groups);
        }
    }
}

fn is_flattened(
    name: &str,
    field: &Field,
    types: &Types,
    values: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    if field
        .attributes
        .get_named_as::<bool>("serde:field:flatten")
        .copied()
        .unwrap_or(false)
    {
        return true;
    }

    !values.contains_key(name)
        && !values.contains_key(snake_to_camel(name).as_str())
        && !field_is_nullable(field, types)
}

fn normalized_identifier(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

fn struct_fields<'a>(ty: &'a DataType, types: &'a Types) -> Option<&'a NamedFields> {
    let DataType::Struct(structure) = resolve_data_type(ty, types)? else {
        return None;
    };
    match &structure.fields {
        Fields::Named(fields) => Some(fields),
        Fields::Unit | Fields::Unnamed(_) => None,
    }
}

fn list_element_type<'a>(ty: &'a DataType, types: &'a Types) -> Option<&'a DataType> {
    match resolve_data_type(ty, types)? {
        DataType::List(list) => Some(&list.ty),
        _ => None,
    }
}

fn resolve_data_type<'a>(ty: &'a DataType, types: &'a Types) -> Option<&'a DataType> {
    match canonical_data_type(ty, types)? {
        DataType::Nullable(inner) => resolve_data_type(inner, types),
        ty => Some(ty),
    }
}

fn canonical_data_type<'a>(ty: &'a DataType, types: &'a Types) -> Option<&'a DataType> {
    match ty {
        DataType::Reference(Reference::Named(reference)) => match &reference.inner {
            NamedReferenceType::Inline { dt, .. } => canonical_data_type(dt, types),
            NamedReferenceType::Reference { .. } => types
                .get(reference)
                .and_then(|named| named.ty.as_ref())
                .and_then(|ty| canonical_data_type(ty, types)),
            NamedReferenceType::Recursive(_) => None,
        },
        ty => Some(ty),
    }
}

fn find_wire_field<'a>(fields: &'a NamedFields, wire_name: &str) -> Option<&'a Field> {
    fields
        .fields
        .iter()
        .find(|(name, field)| {
            field
                .attributes
                .get_named_as::<String>("serde:field:rename_serialize")
                .map(String::as_str)
                .is_some_and(|name| name == wire_name)
                || name.as_ref() == wire_name
                || snake_to_camel(name) == wire_name
        })
        .map(|(_, field)| field)
}

fn field_is_optional(field: &Field, types: &Types) -> bool {
    field.optional
        || field
            .attributes
            .get_named_as::<bool>("serde:field:default")
            .copied()
            .unwrap_or(false)
        || field
            .attributes
            .contains_key("serde:field:skip_serializing_if")
        || field_is_nullable(field, types)
}

fn field_is_nullable(field: &Field, types: &Types) -> bool {
    field
        .ty
        .as_ref()
        .and_then(|ty| canonical_data_type(ty, types))
        .is_some_and(|ty| matches!(ty, DataType::Nullable(_)))
}

fn snake_to_camel(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut uppercase = false;
    for character in value.chars() {
        if character == '_' {
            uppercase = true;
        } else if uppercase {
            result.extend(character.to_uppercase());
            uppercase = false;
        } else {
            result.push(character);
        }
    }
    result
}

pub(super) fn extract_wire_string<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_value(value)?
        .as_str()
        .ok_or_else(|| AppError::InvalidBackendData {
            field: "wire enum".to_owned(),
            reason: "wire enum sample did not serialize as a string".to_owned(),
        })?
        .to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Serialize, ser::SerializeMap};

    #[derive(Serialize, Type)]
    #[serde(rename_all = "camelCase")]
    struct Declared {
        session_id: String,
    }

    struct DriftedStruct;

    impl Serialize for DriftedStruct {
        fn serialize<S: serde::Serializer>(
            &self,
            serializer: S,
        ) -> std::result::Result<S::Ok, S::Error> {
            let mut map = serializer.serialize_map(Some(3))?;
            map.serialize_entry("type", "drift.struct")?;
            map.serialize_entry("sessionId", "session-1")?;
            map.serialize_entry("ghost", "unmodelled")?;
            map.end()
        }
    }

    impl Type for DriftedStruct {
        fn definition(types: &mut Types) -> DataType {
            Declared::definition(types)
        }
    }

    #[derive(Serialize, Type)]
    #[serde(tag = "type", rename_all = "camelCase")]
    enum DeclaredEvent {
        #[expect(
            dead_code,
            reason = "the fixture only needs this variant's declared shape"
        )]
        Ready { session_id: String },
    }

    struct DriftedEnum;

    impl Serialize for DriftedEnum {
        fn serialize<S: serde::Serializer>(
            &self,
            serializer: S,
        ) -> std::result::Result<S::Ok, S::Error> {
            let mut map = serializer.serialize_map(Some(2))?;
            map.serialize_entry("type", "drift.enum")?;
            map.serialize_entry("ghost", "unmodelled")?;
            map.end()
        }
    }

    impl Type for DriftedEnum {
        fn definition(types: &mut Types) -> DataType {
            DeclaredEvent::definition(types)
        }
    }

    #[test]
    fn unmodelled_struct_key_fails_generation() {
        let error = extract_type(&DriftedStruct).expect_err("unresolved key must fail generation");
        assert!(
            format!("{error}").contains("ghost"),
            "error must name the unresolved key: {error}"
        );
    }

    #[test]
    fn unmatched_enum_variant_fails_generation() {
        let error = extract_type(&DriftedEnum).expect_err("unmatched variant must fail generation");
        assert!(
            format!("{error}").contains("ghost"),
            "error must name the unmatched sample: {error}"
        );
    }

    #[test]
    fn resolved_samples_classify_optional_and_nullable_fields() {
        #[derive(Serialize, Type)]
        #[serde(rename_all = "camelCase")]
        struct Inner {
            required: String,
            optional: Option<String>,
        }

        #[derive(Serialize, Type)]
        #[serde(tag = "type", rename_all = "camelCase")]
        enum Outer {
            #[serde(rename = "sample.wrapped")]
            Wrapped { inner: Option<Box<Inner>> },
        }

        reset_message_shapes();
        extract_type(&Outer::Wrapped {
            inner: Some(Box::new(Inner {
                required: "value".to_owned(),
                optional: Some("value".to_owned()),
            })),
        })
        .expect("sample resolves");
        let shapes = take_message_shapes();
        let shape = &shapes["sample.wrapped"];

        assert_eq!(shape["optionalFields"], serde_json::json!(["inner"]));
        assert_eq!(shape["nullableFields"], serde_json::json!(["inner"]));
        assert_eq!(shape["requiredFields"], serde_json::json!(["type"]));

        let inner = &shape["object"]["inner"];
        assert_eq!(inner["requiredFields"], serde_json::json!(["required"]));
        assert_eq!(inner["optionalFields"], serde_json::json!(["optional"]));
        assert_eq!(inner["nullableFields"], serde_json::json!(["optional"]));
    }
}
