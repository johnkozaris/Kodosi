use serde::{Deserialize, Serialize};

use super::HostCommandValidationError;

pub const ROOM_LENGTH_UNIT: &str = "utf16CodeUnits";
pub const ROOM_NAME_MAX_LEN: usize = 128;
pub const ROOM_SLUG_MIN_LEN: usize = 3;
pub const ROOM_SLUG_MAX_LEN: usize = 64;
pub const ROOM_SLUG_PATTERN: &str = "^[a-z0-9-]{3,64}$";
pub const CHAT_BODY_MAX_LEN: usize = 4000;
pub const CHAT_RECIPIENT_MAX_COUNT: usize = 32;
pub const TASK_TITLE_MAX_LEN: usize = 200;
pub const TASK_DESCRIPTION_MAX_LEN: usize = 4000;
pub const TASK_RESULT_MAX_LEN: usize = 4000;
pub const TASK_STATUSES: [&str; 5] = ["Open", "InProgress", "Review", "Done", "Archived"];

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum RoomCommand {
    #[serde(rename = "room.refresh")]
    Refresh,
    #[serde(rename = "room.create")]
    Create {
        name: String,
        slug: String,
        #[serde(rename = "requestId", default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },
    #[serde(rename = "room.removeMember")]
    RemoveMember {
        room_id: String,
        user_id: String,
        expected_roster_generation: i64,
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "room.invite")]
    Invite {
        room_id: String,
        invitee_user_id: String,
        #[serde(rename = "requestId", default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },
    #[serde(rename = "room.acceptInvitation")]
    AcceptInvitation {
        invitation_id: String,
        room_id: String,
        expected_roster_generation: i64,
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "room.declineInvitation")]
    DeclineInvitation {
        invitation_id: String,
        room_id: String,
        expected_roster_generation: i64,
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "room.cancelInvitation")]
    CancelInvitation {
        invitation_id: String,
        room_id: String,
        expected_roster_generation: i64,
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "room.refreshMembers")]
    RefreshMembers {
        room_id: String,
        hydration_id: String,
    },
    #[serde(rename = "room.refreshInvitations")]
    RefreshInvitations,
    #[serde(rename = "room.mutations.recover")]
    RecoverMutations,
    #[serde(rename = "room.mutations.reconcile")]
    ReconcileMutation {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "room.chat.list")]
    ChatList {
        room_id: String,
        since: Option<i64>,
        limit: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tail: Option<bool>,
        hydration_id: String,
    },
    #[serde(rename = "room.chat.post")]
    ChatPost {
        room_id: String,
        body: String,
        #[serde(default)]
        author_session_id: Option<String>,
        #[serde(
            rename = "recipient_session_ids",
            default,
            skip_serializing_if = "Vec::is_empty"
        )]
        #[specta(optional)]
        recipient_session_ids: Vec<String>,
        #[serde(
            rename = "recipient_user_ids",
            default,
            skip_serializing_if = "Vec::is_empty"
        )]
        #[specta(optional)]
        recipient_user_ids: Vec<String>,
        #[serde(rename = "requestId", default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },
    #[serde(rename = "room.tasks.list")]
    TasksList {
        room_id: String,
        #[serde(default)]
        status: Option<String>,
        #[serde(default)]
        assignee: Option<String>,
        #[serde(default)]
        offset: Option<usize>,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        hydration_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        snapshot: Option<String>,
    },
    #[serde(rename = "room.tasks.create")]
    TaskCreate {
        room_id: String,
        title: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        assigned_session_id: Option<String>,
        #[serde(default)]
        assigned_session_incarnation_id: Option<String>,
        #[serde(default)]
        due_at: Option<String>,
        #[serde(rename = "requestId", default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },
    #[serde(rename = "room.tasks.transition")]
    TaskTransition {
        room_id: String,
        task_id: String,
        expected_task_revision: i64,
        to_status: String,
        #[serde(default)]
        actor_session_id: Option<String>,
        #[serde(default)]
        actor_session_incarnation_id: Option<String>,
        #[serde(default)]
        result: Option<String>,
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "room.tasks.assign")]
    TaskAssign {
        room_id: String,
        task_id: String,
        expected_task_revision: i64,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        session_incarnation_id: Option<String>,
        #[serde(rename = "requestId")]
        request_id: String,
    },
}

impl RoomCommand {
    pub(crate) fn is_receipt_authoritative(&self) -> bool {
        matches!(
            self,
            Self::RemoveMember { .. }
                | Self::AcceptInvitation { .. }
                | Self::DeclineInvitation { .. }
                | Self::CancelInvitation { .. }
                | Self::TaskTransition { .. }
                | Self::TaskAssign { .. }
        )
    }

    pub fn operation(&self) -> &'static str {
        match self {
            Self::Refresh => "refresh",
            Self::Create { .. } => "create",
            Self::RemoveMember { .. } => "removeMember",
            Self::Invite { .. } => "invite",
            Self::AcceptInvitation { .. } => "acceptInvitation",
            Self::DeclineInvitation { .. } => "declineInvitation",
            Self::CancelInvitation { .. } => "cancelInvitation",
            Self::RefreshMembers { .. } => "refreshMembers",
            Self::RefreshInvitations => "refreshInvitations",
            Self::RecoverMutations => "mutations.recover",
            Self::ReconcileMutation { .. } => "mutations.reconcile",
            Self::ChatList { .. } => "chat.list",
            Self::ChatPost { .. } => "chat.post",
            Self::TasksList { .. } => "tasks.list",
            Self::TaskCreate { .. } => "tasks.create",
            Self::TaskTransition { .. } => "tasks.transition",
            Self::TaskAssign { .. } => "tasks.assign",
        }
    }

    pub fn request_id(&self) -> Option<&str> {
        match self {
            Self::Create { request_id, .. }
            | Self::Invite { request_id, .. }
            | Self::ChatPost { request_id, .. }
            | Self::TaskCreate { request_id, .. } => request_id.as_deref(),
            Self::RemoveMember { request_id, .. }
            | Self::AcceptInvitation { request_id, .. }
            | Self::DeclineInvitation { request_id, .. }
            | Self::CancelInvitation { request_id, .. }
            | Self::TaskTransition { request_id, .. }
            | Self::TaskAssign { request_id, .. }
            | Self::ReconcileMutation { request_id } => Some(request_id.as_str()),
            Self::Refresh
            | Self::RecoverMutations
            | Self::RefreshMembers { .. }
            | Self::RefreshInvitations
            | Self::ChatList { .. }
            | Self::TasksList { .. } => None,
        }
    }

    pub fn room_id_for_error(&self) -> Option<String> {
        match self {
            Self::Refresh
            | Self::RecoverMutations
            | Self::Create { .. }
            | Self::RefreshInvitations
            | Self::ReconcileMutation { .. } => None,
            Self::AcceptInvitation { room_id, .. }
            | Self::DeclineInvitation { room_id, .. }
            | Self::CancelInvitation { room_id, .. }
            | Self::RemoveMember { room_id, .. }
            | Self::Invite { room_id, .. }
            | Self::RefreshMembers { room_id, .. }
            | Self::ChatList { room_id, .. }
            | Self::ChatPost { room_id, .. }
            | Self::TasksList { room_id, .. }
            | Self::TaskCreate { room_id, .. }
            | Self::TaskTransition { room_id, .. }
            | Self::TaskAssign { room_id, .. } => Some(room_id.clone()),
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one exhaustive validation match keeps every room command's boundary rules together"
    )]
    pub(crate) fn validate(&self) -> Result<(), HostCommandValidationError> {
        fn require_nonempty(
            value: &str,
            field: &'static str,
        ) -> Result<(), HostCommandValidationError> {
            if value.trim().is_empty() {
                return Err(HostCommandValidationError::EmptyField(field));
            }
            Ok(())
        }
        fn require_max_utf16(
            value: &str,
            max: usize,
            field: &'static str,
            reason: &'static str,
        ) -> Result<(), HostCommandValidationError> {
            if value.encode_utf16().count() > max {
                return Err(HostCommandValidationError::InvalidField { field, reason });
            }
            Ok(())
        }
        fn require_identity_pair(
            identity: Option<&str>,
            incarnation: Option<&str>,
            identity_field: &'static str,
            incarnation_field: &'static str,
        ) -> Result<(), HostCommandValidationError> {
            if identity.is_some() != incarnation.is_some() {
                return Err(HostCommandValidationError::InvalidField {
                    field: incarnation_field,
                    reason: "must be supplied exactly when the matching identity is supplied",
                });
            }
            for (value, field) in [(identity, identity_field), (incarnation, incarnation_field)] {
                if let Some(value) = value
                    && uuid::Uuid::parse_str(value).is_err()
                {
                    return Err(HostCommandValidationError::InvalidField {
                        field,
                        reason: "must be a UUID",
                    });
                }
            }
            Ok(())
        }
        fn is_canonical_uuid_v7(value: &str) -> bool {
            uuid::Uuid::parse_str(value).is_ok_and(|parsed| {
                parsed.get_version_num() == 7 && parsed.hyphenated().to_string() == value
            })
        }
        match self {
            Self::RemoveMember { request_id, .. }
            | Self::AcceptInvitation { request_id, .. }
            | Self::DeclineInvitation { request_id, .. }
            | Self::CancelInvitation { request_id, .. }
            | Self::TaskTransition { request_id, .. }
            | Self::TaskAssign { request_id, .. }
            | Self::ReconcileMutation { request_id }
                if !is_canonical_uuid_v7(request_id) =>
            {
                return Err(HostCommandValidationError::InvalidField {
                    field: "requestId",
                    reason: "must be a canonical UUIDv7",
                });
            }
            _ => {
                if self.request_id().is_some_and(|request_id| {
                    request_id.trim().is_empty() || uuid::Uuid::parse_str(request_id).is_err()
                }) {
                    return Err(HostCommandValidationError::InvalidField {
                        field: "requestId",
                        reason: "must be a UUID",
                    });
                }
            }
        }
        match self {
            Self::Refresh
            | Self::RecoverMutations
            | Self::RefreshInvitations
            | Self::ReconcileMutation { .. } => Ok(()),
            Self::Create { name, slug, .. } => {
                require_nonempty(name, "name")?;
                require_max_utf16(
                    name,
                    ROOM_NAME_MAX_LEN,
                    "name",
                    "must be 1-128 UTF-16 code units",
                )?;
                require_nonempty(slug, "slug")?;

                let slug_ok = (ROOM_SLUG_MIN_LEN..=ROOM_SLUG_MAX_LEN).contains(&slug.len())
                    && slug
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
                if !slug_ok {
                    return Err(HostCommandValidationError::InvalidField {
                        field: "slug",
                        reason: "must be 3-64 chars of lowercase letters, digits, or '-'",
                    });
                }
                Ok(())
            }
            Self::RemoveMember {
                room_id, user_id, ..
            } => {
                require_nonempty(room_id, "roomId")?;
                require_nonempty(user_id, "userId")
            }
            Self::Invite {
                room_id,
                invitee_user_id,
                ..
            } => {
                require_nonempty(room_id, "roomId")?;
                require_nonempty(invitee_user_id, "inviteeUserId")
            }
            Self::AcceptInvitation { invitation_id, .. }
            | Self::DeclineInvitation { invitation_id, .. }
            | Self::CancelInvitation { invitation_id, .. } => {
                require_nonempty(invitation_id, "invitationId")
            }
            Self::RefreshMembers {
                room_id,
                hydration_id,
            } => {
                require_nonempty(room_id, "roomId")?;
                if uuid::Uuid::parse_str(hydration_id).is_err() {
                    return Err(HostCommandValidationError::InvalidField {
                        field: "hydrationId",
                        reason: "must be a UUID",
                    });
                }
                Ok(())
            }
            Self::ChatList {
                room_id,
                since,
                tail,
                hydration_id,
                ..
            } => {
                require_nonempty(room_id, "roomId")?;
                if uuid::Uuid::parse_str(hydration_id).is_err() {
                    return Err(HostCommandValidationError::InvalidField {
                        field: "hydrationId",
                        reason: "must be a UUID",
                    });
                }
                if *tail == Some(true) && since.is_some() {
                    return Err(HostCommandValidationError::InvalidField {
                        field: "since",
                        reason: "must be omitted for a tail read",
                    });
                }
                Ok(())
            }
            Self::TasksList {
                room_id,
                offset,
                limit,
                hydration_id,
                snapshot,
                ..
            } => {
                require_nonempty(room_id, "roomId")?;
                if snapshot.as_deref().is_some_and(|value| {
                    value.len() != 64
                        || !value
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                }) {
                    return Err(HostCommandValidationError::InvalidField {
                        field: "snapshot",
                        reason: "must be a 64-character lowercase hex snapshot",
                    });
                }
                if hydration_id
                    .as_deref()
                    .is_some_and(|value| uuid::Uuid::parse_str(value).is_err())
                {
                    return Err(HostCommandValidationError::InvalidField {
                        field: "hydrationId",
                        reason: "must be a UUID",
                    });
                }
                if offset.is_some_and(|value| value > 10_000) {
                    return Err(HostCommandValidationError::InvalidField {
                        field: "offset",
                        reason: "must not exceed the bounded 10000-item snapshot",
                    });
                }
                if limit.is_some_and(|value| !(1..=500).contains(&value)) {
                    return Err(HostCommandValidationError::InvalidField {
                        field: "limit",
                        reason: "must be between 1 and 500",
                    });
                }
                Ok(())
            }
            Self::ChatPost {
                room_id,
                body,
                author_session_id,
                recipient_session_ids,
                recipient_user_ids,
                ..
            } => {
                require_nonempty(room_id, "roomId")?;
                require_nonempty(body, "body")?;
                require_max_utf16(
                    body,
                    CHAT_BODY_MAX_LEN,
                    "body",
                    "must be 1-4000 UTF-16 code units",
                )?;
                if recipient_session_ids.len() + recipient_user_ids.len() > CHAT_RECIPIENT_MAX_COUNT
                {
                    return Err(HostCommandValidationError::InvalidField {
                        field: "recipient_session_ids",
                        reason: "recipient lists may contain at most 32 total IDs",
                    });
                }
                let mut sessions = std::collections::BTreeSet::new();
                for value in recipient_session_ids {
                    let id = uuid::Uuid::parse_str(value).map_err(|_| {
                        HostCommandValidationError::InvalidField {
                            field: "recipient_session_ids",
                            reason: "every value must be a UUID",
                        }
                    })?;
                    if !sessions.insert(id) {
                        return Err(HostCommandValidationError::InvalidField {
                            field: "recipient_session_ids",
                            reason: "must not contain duplicate IDs",
                        });
                    }
                }
                let mut users = std::collections::BTreeSet::new();
                for value in recipient_user_ids {
                    let id = uuid::Uuid::parse_str(value).map_err(|_| {
                        HostCommandValidationError::InvalidField {
                            field: "recipient_user_ids",
                            reason: "every value must be a UUID",
                        }
                    })?;
                    if !users.insert(id) {
                        return Err(HostCommandValidationError::InvalidField {
                            field: "recipient_user_ids",
                            reason: "must not contain duplicate IDs",
                        });
                    }
                }
                if author_session_id
                    .as_deref()
                    .and_then(|value| uuid::Uuid::parse_str(value).ok())
                    .is_some_and(|author| sessions.contains(&author))
                {
                    return Err(HostCommandValidationError::InvalidField {
                        field: "recipient_session_ids",
                        reason: "must not contain author_session_id",
                    });
                }
                Ok(())
            }
            Self::TaskCreate {
                room_id,
                title,
                description,
                assigned_session_id,
                assigned_session_incarnation_id,
                ..
            } => {
                require_nonempty(room_id, "roomId")?;
                require_nonempty(title, "title")?;
                require_identity_pair(
                    assigned_session_id.as_deref(),
                    assigned_session_incarnation_id.as_deref(),
                    "assignedSessionId",
                    "assignedSessionIncarnationId",
                )?;
                require_max_utf16(
                    title,
                    TASK_TITLE_MAX_LEN,
                    "title",
                    "must be 1-200 UTF-16 code units",
                )?;
                if let Some(description) = description {
                    require_max_utf16(
                        description,
                        TASK_DESCRIPTION_MAX_LEN,
                        "description",
                        "must not exceed 4000 UTF-16 code units",
                    )?;
                }
                Ok(())
            }
            Self::TaskTransition {
                room_id,
                task_id,
                to_status,
                actor_session_id,
                actor_session_incarnation_id,
                result,
                ..
            } => {
                require_nonempty(room_id, "roomId")?;
                require_nonempty(task_id, "taskId")?;
                require_nonempty(to_status, "toStatus")?;
                require_identity_pair(
                    actor_session_id.as_deref(),
                    actor_session_incarnation_id.as_deref(),
                    "actorSessionId",
                    "actorSessionIncarnationId",
                )?;
                if !TASK_STATUSES.contains(&to_status.as_str()) {
                    return Err(HostCommandValidationError::InvalidField {
                        field: "toStatus",
                        reason: "must be one of Open, InProgress, Review, Done, Archived",
                    });
                }
                if let Some(result) = result {
                    require_max_utf16(
                        result,
                        TASK_RESULT_MAX_LEN,
                        "result",
                        "must not exceed 4000 UTF-16 code units",
                    )?;
                }
                Ok(())
            }
            Self::TaskAssign {
                room_id,
                task_id,
                session_id,
                session_incarnation_id,
                ..
            } => {
                require_nonempty(room_id, "roomId")?;
                require_nonempty(task_id, "taskId")?;
                require_identity_pair(
                    session_id.as_deref(),
                    session_incarnation_id.as_deref(),
                    "sessionId",
                    "sessionIncarnationId",
                )
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RoomEvent {
    #[serde(rename = "room.snapshot")]
    Snapshot { rooms: Vec<RoomEntry> },
    #[serde(rename = "room.invitations")]
    Invitations {
        incoming: Vec<RoomInvitationEntry>,
        outgoing: Vec<RoomInvitationEntry>,
    },
    #[serde(rename = "room.members")]
    Members {
        room_id: String,
        members: Vec<RoomMemberEntry>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hydration_id: Option<String>,
    },
    #[serde(rename = "room.chat.snapshot")]
    ChatSnapshot {
        room_id: String,
        messages: Vec<RoomChatEntry>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hydration_id: Option<String>,
    },
    #[serde(rename = "room.chat.posted")]
    ChatPosted {
        room_id: String,
        message: RoomChatEntry,
    },
    #[serde(rename = "room.tasks.page")]
    TasksPage {
        room_id: String,
        tasks: Vec<RoomTaskEntry>,
        has_more: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next_offset: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hydration_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_offset: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        snapshot: Option<String>,
    },
    #[serde(rename = "room.tasks.invalidated")]
    TasksInvalidated {
        room_id: String,
        hydration_id: Option<String>,
        request_offset: usize,
    },
    #[serde(rename = "room.tasks.snapshot")]
    TasksSnapshot {
        room_id: String,
        tasks: Vec<RoomTaskEntry>,
    },
    #[serde(rename = "room.tasks.upserted")]
    TaskUpserted {
        room_id: String,
        task: RoomTaskEntry,
    },
    #[serde(rename = "room.agent.delivery")]
    AgentDelivery {
        session_id: String,
        session_incarnation_id: String,
        state: RoomAgentDeliveryState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        event_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    #[serde(rename = "room.mutation.recovered")]
    MutationRecovered {
        #[serde(rename = "requestId")]
        request_id: String,
        operation: String,
        #[serde(rename = "roomId")]
        room_id: String,
        fingerprint: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<RoomActionStatus>,
        #[serde(rename = "entityId", default, skip_serializing_if = "Option::is_none")]
        entity_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    #[serde(rename = "room.action.accepted")]
    ActionAccepted {
        #[serde(rename = "requestId")]
        request_id: String,
        operation: String,
        #[serde(rename = "roomId")]
        room_id: String,
        fingerprint: String,
    },
    #[serde(rename = "room.action.result")]
    ActionResult {
        #[serde(rename = "requestId")]
        request_id: String,
        operation: String,
        #[serde(rename = "roomId", default, skip_serializing_if = "Option::is_none")]
        room_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fingerprint: Option<String>,
        status: RoomActionStatus,
        #[serde(rename = "entityId", default, skip_serializing_if = "Option::is_none")]
        entity_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    #[serde(rename = "room.error")]
    Error {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        room_id: Option<String>,
        operation: String,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RoomAgentDeliveryState {
    WaitingForAdapter,
    Ready,
    Offered,
    AcceptedByTransport,
    ActedOn,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum RoomActionStatus {
    Succeeded,
    Failed,
    Unknown,
    Conflict,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RoomEntry {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub owner_user_id: String,
    pub roster_generation: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RoomMemberEntry {
    pub room_id: String,
    pub user_id: String,
    pub role: String,
    pub username: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RoomInvitationEntry {
    pub id: String,
    pub room_id: String,
    pub room_name: String,
    pub room_slug: String,
    pub invitee_user_id: String,
    pub invitee_handle: String,
    pub invitee_display_name: Option<String>,
    pub invited_by_user_id: String,
    pub invited_by_handle: String,
    pub invited_by_display_name: Option<String>,
    pub status: String,
    pub base_roster_generation: i64,
    pub proposed_roster_generation: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RoomChatEntry {
    pub id: String,
    pub room_id: String,
    pub author_user_id: String,
    pub author_session_id: Option<String>,
    pub author_kind: String,
    pub body: String,
    #[serde(default)]
    #[specta(optional)]
    pub recipient_session_ids: Vec<String>,
    #[serde(default)]
    #[specta(optional)]
    pub recipient_user_ids: Vec<String>,
    pub seq: i64,
    pub posted_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RoomTaskEntry {
    pub id: String,
    pub room_id: String,
    pub created_by_user_id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub revision: i64,
    pub assigned_session_id: Option<String>,
    pub assigned_session_incarnation_id: Option<String>,
    pub due_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub result: Option<String>,
    pub result_author_user_id: Option<String>,
    #[serde(default)]
    pub content_unavailable: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_validates() {
        assert!(RoomCommand::Refresh.validate().is_ok());
    }

    #[test]
    fn create_rejects_blank_slug() {
        assert!(
            RoomCommand::Create {
                name: "Acme".into(),
                slug: " ".into(),
                request_id: Some("01900000-0000-7000-8000-000000000001".into()),
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn create_rejects_invalid_slug_chars() {
        assert!(
            RoomCommand::Create {
                name: "Acme".into(),
                slug: "Has Spaces".into(),
                request_id: Some("01900000-0000-7000-8000-000000000001".into()),
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn chat_post_rejects_blank_body() {
        assert!(
            RoomCommand::ChatPost {
                room_id: "r1".into(),
                body: String::new(),
                author_session_id: None,
                recipient_session_ids: Vec::new(),
                recipient_user_ids: Vec::new(),
                request_id: Some("01900000-0000-7000-8000-000000000001".into()),
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn chat_post_rejects_overlong_body() {
        assert!(
            RoomCommand::ChatPost {
                room_id: "r1".into(),
                body: "x".repeat(CHAT_BODY_MAX_LEN + 1),
                author_session_id: None,
                recipient_session_ids: Vec::new(),
                recipient_user_ids: Vec::new(),
                request_id: Some("01900000-0000-7000-8000-000000000001".into()),
            }
            .validate()
            .is_err()
        );
    }

    fn chat_post_with_recipients(
        recipient_session_ids: Vec<String>,
        recipient_user_ids: Vec<String>,
    ) -> RoomCommand {
        RoomCommand::ChatPost {
            room_id: "r1".into(),
            body: "hello".into(),
            author_session_id: Some("01900000-0000-7000-8000-000000000001".into()),
            recipient_session_ids,
            recipient_user_ids,
            request_id: Some("01900000-0000-7000-8000-000000000001".into()),
        }
    }

    #[test]
    fn chat_post_recipient_fields_are_backward_compatible_snake_case() {
        let decoded: RoomCommand = serde_json::from_value(serde_json::json!({
            "type": "room.chat.post",
            "room_id": "r1",
            "body": "hello",
            "author_session_id": null
        }))
        .expect("old desktop command should decode");
        let RoomCommand::ChatPost {
            recipient_session_ids,
            recipient_user_ids,
            ..
        } = &decoded
        else {
            panic!("expected chat post");
        };
        assert!(recipient_session_ids.is_empty());
        assert!(recipient_user_ids.is_empty());
        let value = serde_json::to_value(decoded).expect("serialize chat post");
        assert!(value.get("recipient_session_ids").is_none());
        assert!(value.get("recipient_user_ids").is_none());

        let directed = chat_post_with_recipients(
            vec!["01900000-0000-7000-8000-000000000002".into()],
            vec!["01900000-0000-7000-8000-000000000003".into()],
        );
        let value = serde_json::to_value(directed).expect("serialize directed chat post");
        assert_eq!(
            value["recipient_session_ids"],
            serde_json::json!(["01900000-0000-7000-8000-000000000002"])
        );
        assert_eq!(
            value["recipient_user_ids"],
            serde_json::json!(["01900000-0000-7000-8000-000000000003"])
        );
    }

    #[test]
    fn room_chat_entry_missing_recipients_decodes_as_broadcast() {
        let entry: RoomChatEntry = serde_json::from_value(serde_json::json!({
            "id": "m1",
            "roomId": "r1",
            "authorUserId": "01900000-0000-7000-8000-000000000010",
            "authorSessionId": null,
            "authorKind": "Human",
            "body": "hello",
            "seq": 1,
            "postedAt": "2026-07-18T00:00:00Z"
        }))
        .expect("old room chat entry should decode");
        assert!(entry.recipient_session_ids.is_empty());
        assert!(entry.recipient_user_ids.is_empty());
    }

    #[test]
    fn chat_post_rejects_invalid_duplicate_excess_and_author_recipients() {
        assert!(
            chat_post_with_recipients(vec!["not-a-uuid".into()], Vec::new())
                .validate()
                .is_err()
        );
        let duplicate = "01900000-0000-7000-8000-000000000002".to_owned();
        assert!(
            chat_post_with_recipients(vec![duplicate.clone(), duplicate], Vec::new())
                .validate()
                .is_err()
        );
        assert!(
            chat_post_with_recipients(
                vec!["01900000-0000-7000-8000-000000000001".into()],
                Vec::new(),
            )
            .validate()
            .is_err()
        );
        let recipients = (0..=CHAT_RECIPIENT_MAX_COUNT)
            .map(|index| format!("01900000-0000-7000-8000-{index:012x}"))
            .collect();
        assert!(
            chat_post_with_recipients(recipients, Vec::new())
                .validate()
                .is_err()
        );
    }

    #[test]
    fn create_rejects_overlong_name() {
        assert!(
            RoomCommand::Create {
                name: "x".repeat(ROOM_NAME_MAX_LEN + 1),
                slug: "acme".into(),
                request_id: Some("01900000-0000-7000-8000-000000000001".into()),
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn task_create_rejects_overlong_title() {
        assert!(
            RoomCommand::TaskCreate {
                room_id: "r1".into(),
                title: "x".repeat(TASK_TITLE_MAX_LEN + 1),
                description: None,
                assigned_session_id: None,
                assigned_session_incarnation_id: None,
                due_at: None,
                request_id: Some("01900000-0000-7000-8000-000000000001".into()),
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn utf16_length_limits_match_backend_semantics() {
        assert!(
            RoomCommand::Create {
                name: format!("{}😀", "x".repeat(126)),
                slug: "acme".into(),
                request_id: Some("01900000-0000-7000-8000-000000000001".into()),
            }
            .validate()
            .is_ok()
        );
        assert!(
            RoomCommand::Create {
                name: format!("{}😀", "x".repeat(127)),
                slug: "acme".into(),
                request_id: Some("01900000-0000-7000-8000-000000000001".into()),
            }
            .validate()
            .is_err()
        );
        assert!(
            RoomCommand::ChatPost {
                room_id: "r1".into(),
                body: format!("{}😀", "x".repeat(CHAT_BODY_MAX_LEN - 1)),
                author_session_id: None,
                recipient_session_ids: Vec::new(),
                recipient_user_ids: Vec::new(),
                request_id: Some("01900000-0000-7000-8000-000000000001".into()),
            }
            .validate()
            .is_err()
        );
        assert!(
            RoomCommand::TaskCreate {
                room_id: "r1".into(),
                title: format!("{}😀", "x".repeat(TASK_TITLE_MAX_LEN - 1)),
                description: None,
                assigned_session_id: None,
                assigned_session_incarnation_id: None,
                due_at: None,
                request_id: Some("01900000-0000-7000-8000-000000000001".into()),
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn task_transition_rejects_unknown_status() {
        assert!(
            RoomCommand::TaskTransition {
                room_id: "r1".into(),
                task_id: "t1".into(),
                expected_task_revision: 1,
                to_status: "InReview".into(),
                actor_session_id: None,
                actor_session_incarnation_id: None,
                result: None,
                request_id: "01900000-0000-7000-8000-000000000001".into(),
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn task_transition_accepts_known_status() {
        assert!(
            RoomCommand::TaskTransition {
                room_id: "r1".into(),
                task_id: "t1".into(),
                expected_task_revision: 1,
                to_status: "InProgress".into(),
                actor_session_id: None,
                actor_session_incarnation_id: None,
                result: None,
                request_id: "01900000-0000-7000-8000-000000000001".into(),
            }
            .validate()
            .is_ok()
        );
    }

    fn receipt_mutations(request_id: &str) -> Vec<RoomCommand> {
        vec![
            RoomCommand::RemoveMember {
                room_id: "room".into(),
                user_id: "user".into(),
                expected_roster_generation: 1,
                request_id: request_id.into(),
            },
            RoomCommand::AcceptInvitation {
                invitation_id: "invitation".into(),
                room_id: "room".into(),
                expected_roster_generation: 1,
                request_id: request_id.into(),
            },
            RoomCommand::DeclineInvitation {
                invitation_id: "invitation".into(),
                room_id: "room".into(),
                expected_roster_generation: 1,
                request_id: request_id.into(),
            },
            RoomCommand::CancelInvitation {
                invitation_id: "invitation".into(),
                room_id: "room".into(),
                expected_roster_generation: 1,
                request_id: request_id.into(),
            },
            RoomCommand::TaskTransition {
                room_id: "room".into(),
                task_id: "task".into(),
                expected_task_revision: 1,
                to_status: "Open".into(),
                actor_session_id: None,
                actor_session_incarnation_id: None,
                result: None,
                request_id: request_id.into(),
            },
            RoomCommand::TaskAssign {
                room_id: "room".into(),
                task_id: "task".into(),
                expected_task_revision: 1,
                session_id: None,
                session_incarnation_id: None,
                request_id: request_id.into(),
            },
        ]
    }

    #[test]
    fn receipt_mutations_require_canonical_uuid_v7_request_ids() {
        for command in receipt_mutations("550e8400-e29b-41d4-a716-446655440000") {
            assert!(command.validate().is_err(), "UUIDv4 requestId must fail");
        }
        for command in receipt_mutations("01900000000070008000000000000001") {
            assert!(
                command.validate().is_err(),
                "noncanonical UUIDv7 requestId must fail"
            );
        }
        for command in receipt_mutations("01900000-0000-7000-8000-000000000001") {
            command
                .validate()
                .expect("canonical UUIDv7 requestId must validate");
        }
    }

    #[test]
    fn receipt_mutations_reject_missing_request_ids_during_decode() {
        for command_type in [
            "room.removeMember",
            "room.acceptInvitation",
            "room.declineInvitation",
            "room.cancelInvitation",
            "room.tasks.transition",
            "room.tasks.assign",
        ] {
            let value = serde_json::json!({
                "type": command_type,
                "roomId": "room",
                "userId": "user",
                "invitationId": "invitation",
                "taskId": "task",
                "toStatus": "Open"
            });
            assert!(
                serde_json::from_value::<RoomCommand>(value).is_err(),
                "{command_type} must require requestId"
            );
        }
    }

    #[test]
    fn unreceipted_mutation_request_id_is_optional_and_validated() {
        let legacy: RoomCommand = serde_json::from_value(serde_json::json!({
            "type": "room.create",
            "name": "Acme",
            "slug": "acme"
        }))
        .expect("legacy uncorrelated command");
        assert_eq!(legacy.request_id(), None);

        let correlated = RoomCommand::Create {
            name: "Acme".into(),
            slug: "acme".into(),
            request_id: Some("01900000-0000-7000-8000-000000000001".into()),
        };
        let value = serde_json::to_value(&correlated).expect("correlated command");
        assert_eq!(value["requestId"], "01900000-0000-7000-8000-000000000001");

        assert!(
            RoomCommand::Create {
                name: "Acme".into(),
                slug: "acme".into(),
                request_id: Some(" ".into()),
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn invite_preserves_the_stable_request_identity() {
        let request_id = uuid::Uuid::now_v7().to_string();
        let command = RoomCommand::Invite {
            room_id: "room".into(),
            invitee_user_id: "user".into(),
            request_id: Some(request_id.clone()),
        };

        command.validate().expect("UUID request ID should validate");
        assert_eq!(command.request_id(), Some(request_id.as_str()));
    }

    #[test]
    fn room_action_result_serializes_full_correlation_shape() {
        let value = serde_json::to_value(RoomEvent::ActionResult {
            request_id: "req-1".into(),
            operation: "chat.post".into(),
            room_id: Some("r1".into()),
            fingerprint: None,
            status: RoomActionStatus::Succeeded,
            entity_id: Some("m1".into()),
            message: None,
        })
        .expect("action result");
        assert_eq!(value["type"], "room.action.result");
        assert_eq!(value["requestId"], "req-1");
        assert_eq!(value["roomId"], "r1");
        assert_eq!(value["status"], "succeeded");
        assert_eq!(value["entityId"], "m1");
        assert!(value.get("message").is_none());
    }

    #[test]
    fn snapshot_serializes_with_rooms_field() {
        let payload = serde_json::to_value(RoomEvent::Snapshot {
            rooms: vec![RoomEntry {
                id: "r1".into(),
                name: "Acme".into(),
                slug: "acme".into(),
                owner_user_id: "user-1".into(),
                roster_generation: 0,
            }],
        })
        .expect("snapshot serializes");

        assert_eq!(payload["type"], "room.snapshot");
        assert!(payload["rooms"].is_array());
    }
}
