use serde::Serialize;

use crate::{
    AppError, Result,
    host_protocol::{
        FriendEntry, FriendRequestEntry, HiddenSessionEntry, LocalSessionListEntry, MyDeviceEntry,
        PermissionFlags, RemoteSessionListEntry, RoomEntry, RoomListEntry, RuntimeSessionStatus,
        SessionListEntry, SessionListEntryMeta, TrustPinEntry,
    },
};
use kodosi_domain::{
    lifecycle::{ConnectionState, RemoteSessionAccessIssue, RemoteSessionAccessState},
    permissions::{AccessLevel, ShareScope},
    session::SessionMode,
};

pub(super) const SESSION_LIST_ENTRY_META_FIELDS: &[&str] = &[
    "agent",
    "workingDir",
    "tokenPercent",
    "gitRepo",
    "gitUrl",
    "gitBranch",
    "gitRemotes",
    "projectType",
    "packageManager",
    "manifestFiles",
    "runningCommand",
    "detectedAgent",
    "terminalTitle",
];

pub(super) const LOCAL_SESSION_ENTRY_FIELDS: &[&str] = &[
    "kind",
    "id",
    "incarnationId",
    "createRequestId",
    "name",
    "project",
    "mode",
    "status",
    "recovery",
    "scope",
    "access",
    "roomId",
    "roomName",
    "activeCount",
    "entitledCount",
    "lastActivity",
    "semanticActions",
    "backendSessionId",
    "backendIncarnationId",
    "meta",
];

pub(super) const REMOTE_SESSION_ENTRY_FIELDS: &[&str] = &[
    "kind",
    "id",
    "incarnationId",
    "name",
    "project",
    "mode",
    "status",
    "scope",
    "access",
    "owner",
    "ownerUserId",
    "permissions",
    "roomId",
    "roomName",
    "connectionState",
    "connectionReason",
    "accessState",
    "accessReason",
    "accessIssue",
    "activeCount",
    "entitledCount",
    "lastActivity",
    "semanticActions",
];

pub(super) const ROOM_LIST_ENTRY_FIELDS: &[&str] = &["id", "name", "slug"];
pub(super) const HIDDEN_SESSION_ENTRY_FIELDS: &[&str] = &["id", "name", "project", "owner"];
pub(super) const ROOM_ENTRY_FIELDS: &[&str] =
    &["id", "name", "slug", "ownerUserId", "rosterGeneration"];
pub(super) const TRUST_PIN_ENTRY_FIELDS: &[&str] = &[
    "userId",
    "generation",
    "signerDeviceId",
    "deviceCount",
    "pinnedAtMs",
];
pub(super) const FRIEND_ENTRY_FIELDS: &[&str] = &["userId", "handle", "displayName", "avatarUrl"];
pub(super) const FRIEND_REQUEST_ENTRY_FIELDS: &[&str] =
    &["userId", "handle", "displayName", "avatarUrl", "createdAt"];
pub(super) const MY_DEVICE_ENTRY_FIELDS: &[&str] =
    &["deviceId", "label", "certSignerDeviceId", "certIssuedAtMs"];

pub(super) fn validate_authority_shapes() -> Result<()> {
    validate_field_set(
        "SessionListEntryMeta",
        extract_field_names(&sample_runtime_session_meta())?,
        SESSION_LIST_ENTRY_META_FIELDS,
    )?;
    validate_field_set(
        "LocalSessionListEntry",
        extract_field_names(&sample_runtime_local_session_info())?,
        LOCAL_SESSION_ENTRY_FIELDS,
    )?;
    validate_field_set(
        "RemoteSessionListEntry",
        extract_field_names(&sample_runtime_remote_session_info())?,
        REMOTE_SESSION_ENTRY_FIELDS,
    )?;
    validate_field_set(
        "RoomListEntry",
        extract_field_names(&sample_room_list_entry())?,
        ROOM_LIST_ENTRY_FIELDS,
    )?;
    validate_field_set(
        "HiddenSessionEntry",
        extract_field_names(&sample_hidden_session_entry())?,
        HIDDEN_SESSION_ENTRY_FIELDS,
    )?;
    validate_field_set(
        "FriendEntry",
        extract_field_names(&sample_friend_entry())?,
        FRIEND_ENTRY_FIELDS,
    )?;
    validate_field_set(
        "FriendRequestEntry",
        extract_field_names(&sample_friend_request_entry())?,
        FRIEND_REQUEST_ENTRY_FIELDS,
    )?;
    validate_field_set(
        "MyDeviceEntry",
        extract_field_names(&sample_my_device_entry())?,
        MY_DEVICE_ENTRY_FIELDS,
    )?;
    validate_field_set(
        "RoomEntry",
        extract_field_names(&sample_room_entry())?,
        ROOM_ENTRY_FIELDS,
    )?;
    validate_field_set(
        "TrustPinEntry",
        extract_field_names(&sample_trust_pin_entry())?,
        TRUST_PIN_ENTRY_FIELDS,
    )?;
    Ok(())
}

fn extract_field_names<T: Serialize>(value: &T) -> Result<Vec<String>> {
    Ok(serde_json::to_value(value)?
        .as_object()
        .ok_or_else(|| AppError::InvalidBackendData {
            field: "shape sample".to_owned(),
            reason: "shape sample did not serialize as an object".to_owned(),
        })?
        .keys()
        .cloned()
        .collect())
}

fn validate_field_set(
    shape: &'static str,
    mut actual: Vec<String>,
    expected: &[&str],
) -> Result<()> {
    let mut expected = to_strings(expected);
    actual.sort();
    expected.sort();
    if actual == expected {
        return Ok(());
    }

    Err(AppError::InvalidBackendData {
        field: shape.to_owned(),
        reason: format!("serialized field set changed: actual={actual:?}, expected={expected:?}"),
    })
}

pub(super) fn to_strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

pub(super) fn sample_runtime_session_meta() -> SessionListEntryMeta {
    SessionListEntryMeta {
        agent: "terminal".to_owned(),
        working_dir: "/Users/john/Repos/kodosi".to_owned(),
        token_percent: 42,
        git_repo: Some("kodosi".to_owned()),
        git_url: Some("https://github.com/example/kodosi".to_owned()),
        git_branch: Some("main".to_owned()),
        git_remotes: vec!["origin".to_owned()],
        project_type: Some("rust".to_owned()),
        package_manager: Some("cargo".to_owned()),
        manifest_files: vec!["Cargo.toml".to_owned()],
        running_command: Some("cargo nextest run".to_owned()),
        detected_agent: Some("Claude Code".to_owned()),
        terminal_title: Some("cargo nextest run".to_owned()),
    }
}

pub(super) fn sample_runtime_remote_session_info() -> SessionListEntry {
    SessionListEntry::Remote {
        entry: RemoteSessionListEntry {
            id: "session-1".to_owned(),
            incarnation_id: Some("01900000-0000-7000-8000-000000000001".to_owned()),
            name: "session-1".to_owned(),
            project: "/Users/john/Repos/kodosi".to_owned(),
            mode: SessionMode::Autopilot,
            status: RuntimeSessionStatus::Reconnecting,
            scope: ShareScope::Room,
            access: AccessLevel::Suggest,
            owner: Some("alice".to_owned()),
            owner_user_id: Some("user-alice".to_owned()),
            permissions: PermissionFlags(0x09),
            room_id: Some("room-1".to_owned()),
            room_name: Some("Acme".to_owned()),
            connection_state: Some(ConnectionState::Offline),
            connection_reason: Some("Reconnecting".to_owned()),
            access_state: Some(RemoteSessionAccessState::AwaitingKey),
            access_reason: Some("Waiting for an encrypted session key from the owner.".to_owned()),
            access_issue: Some(RemoteSessionAccessIssue::PeerIdentityChanged),
            active_count: 3,
            entitled_count: 12,
            last_activity: "just now".to_owned().into(),
            semantic_actions: super::super::SessionSemanticActions::NONE,
        },
    }
}

pub(in crate::host_protocol) fn sample_runtime_local_session_info() -> SessionListEntry {
    SessionListEntry::Local {
        entry: LocalSessionListEntry {
            id: "session-2".to_owned(),
            incarnation_id: "01900000-0000-7000-8000-000000000002".to_owned(),
            create_request_id: Some("req-2".to_owned()),
            name: "Pink Iguana".to_owned(),
            project: "/Users/john/Repos/kodosi".to_owned(),
            mode: SessionMode::Normal,
            status: RuntimeSessionStatus::Waiting,
            recovery: kodosi_domain::session::LocalSessionRecoveryState::Live,
            scope: ShareScope::JustMe,
            access: AccessLevel::Inject,
            room_id: Some("room-2".to_owned()),
            room_name: Some("Core team".to_owned()),
            active_count: 0,
            entitled_count: 1,
            last_activity: "just now".to_owned().into(),
            semantic_actions: super::super::SessionSemanticActions {
                queue: true,
                steer: true,
                stop_and_send: true,
            },
            backend_session_id: Some("b-sess-2".to_owned()),
            backend_incarnation_id: Some("01900000-0000-7000-8000-000000000003".to_owned()),
            meta: Some(Box::new(sample_runtime_session_meta())),
        },
    }
}

pub(super) fn sample_room_list_entry() -> RoomListEntry {
    RoomListEntry {
        id: "room-1".to_owned(),
        name: "Acme".to_owned(),
        slug: "acme".to_owned(),
    }
}

pub(super) fn sample_hidden_session_entry() -> HiddenSessionEntry {
    HiddenSessionEntry {
        id: "session-hidden".to_owned(),
        name: "Hidden session".to_owned(),
        project: "/repo/project".to_owned(),
        owner: "Alice".to_owned(),
    }
}

pub(super) fn sample_friend_entry() -> FriendEntry {
    FriendEntry {
        user_id: "user-1".to_owned(),
        handle: "alice".to_owned(),
        display_name: "Alice".to_owned(),
        avatar_url: Some("https://example.com/alice.png".to_owned()),
    }
}

pub(super) fn sample_friend_request_entry() -> FriendRequestEntry {
    FriendRequestEntry {
        user_id: "user-2".to_owned(),
        handle: "bob".to_owned(),
        display_name: "Bob".to_owned(),
        avatar_url: Some("https://example.com/bob.png".to_owned()),
        created_at: "2026-04-24T10:00:00Z".to_owned(),
    }
}

pub(super) fn sample_my_device_entry() -> MyDeviceEntry {
    MyDeviceEntry {
        device_id: "device-self".to_owned(),
        label: "MacBook Pro".to_owned(),
        cert_signer_device_id: "device-self".to_owned(),
        cert_issued_at_ms: 1_700_000_000_000,
    }
}

pub(super) fn sample_room_entry() -> RoomEntry {
    RoomEntry {
        id: "room-1".to_owned(),
        name: "Acme".to_owned(),
        slug: "acme".to_owned(),
        owner_user_id: "user-1".to_owned(),
        roster_generation: 7,
    }
}

pub(super) fn sample_trust_pin_entry() -> TrustPinEntry {
    TrustPinEntry {
        user_id: "user-1".to_owned(),
        generation: 3,
        signer_device_id: "device-1".to_owned(),
        device_count: 2,
        pinned_at_ms: 1_700_000_000_000,
    }
}
