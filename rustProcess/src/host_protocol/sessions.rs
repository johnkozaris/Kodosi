use serde::{Deserialize, Serialize};

use super::{
    HostCommandValidationError,
    session_entries::{RoomListEntry, SessionListEntry},
};
use kodosi_domain::{
    permissions::{AccessLevel, ShareScope},
    provider_conversation::ProviderConversationIdentity,
    session::SessionMode,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum RelayActionStatus {
    Accepted,
    Duplicate,
    Busy,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum SessionAccessMutationKind {
    Grant,
    Revoke,
    Leave,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum SessionAccessMutationOutcome {
    Applied,
    Rejected,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum SessionCommand {
    #[serde(rename = "session.create")]
    Create {
        #[serde(rename = "requestId")]
        request_id: String,
        name: String,
        #[serde(default, rename = "workingDir")]
        working_dir: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resume: Option<ProviderConversationIdentity>,
    },
    #[serde(rename = "session.rename")]
    Rename {
        #[serde(rename = "sessionId")]
        session_id: String,
        name: String,
    },
    #[serde(rename = "session.mode")]
    SetMode {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
        mode: SessionMode,
    },
    #[serde(rename = "session.stop")]
    Stop {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
    },
    #[serde(rename = "session.close")]
    Close {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
    },
    #[serde(rename = "session.interrupt")]
    Interrupt {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
    },
    #[serde(rename = "session.delete")]
    Delete {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
    },
    #[serde(rename = "session.reopen")]
    Reopen {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
    },
    #[serde(rename = "session.openRemote")]
    OpenRemote {
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    #[serde(rename = "session.hide")]
    Hide {
        #[serde(rename = "sessionId")]
        session_id: String,
    },

    #[serde(rename = "session.leave")]
    Leave {
        #[serde(rename = "mutationId")]
        mutation_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
    },
    #[serde(rename = "session.unhide")]
    Unhide {
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    #[serde(rename = "session.listHidden")]
    ListHidden {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "session.scope")]
    SetShareScope {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
        scope: ShareScope,
        #[serde(default, rename = "roomId")]
        room_id: Option<String>,
    },
    #[serde(rename = "session.grantAccess")]
    GrantAccess {
        #[serde(rename = "mutationId")]
        mutation_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
        #[serde(rename = "actorUserId")]
        actor_user_id: String,
        #[serde(rename = "accessLevel")]
        access_level: AccessLevel,
        #[serde(rename = "expiresAt")]
        expires_at: String,
    },
    #[serde(rename = "session.revokeAccess")]
    RevokeAccess {
        #[serde(rename = "mutationId")]
        mutation_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
        #[serde(rename = "actorUserId")]
        actor_user_id: String,
    },
    #[serde(rename = "session.listAccess")]
    ListAccess {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
    },
    #[serde(rename = "session.accessMutationsRecover")]
    RecoverAccessMutations,
    #[serde(rename = "session.accessMutationReconcile")]
    ReconcileAccessMutation {
        #[serde(rename = "mutationId")]
        mutation_id: String,
    },
    #[serde(rename = "session.accessMutationAck")]
    AcknowledgeAccessMutation {
        #[serde(rename = "mutationId")]
        mutation_id: String,
        fingerprint: String,
    },
    #[serde(rename = "session.list")]
    SnapshotRefresh,
}

impl SessionCommand {
    pub fn operation(&self) -> &'static str {
        match self {
            Self::Create { .. } => "session.create",
            Self::Rename { .. } => "session.rename",
            Self::SetMode { .. } => "session.mode",
            Self::Stop { .. } => "session.stop",
            Self::Close { .. } => "session.close",
            Self::Interrupt { .. } => "session.interrupt",
            Self::Delete { .. } => "session.delete",
            Self::Reopen { .. } => "session.reopen",
            Self::OpenRemote { .. } => "session.openRemote",
            Self::Hide { .. } => "session.hide",
            Self::Leave { .. } => "session.leave",
            Self::Unhide { .. } => "session.unhide",
            Self::ListHidden { .. } => "session.listHidden",
            Self::SetShareScope { .. } => "session.scope",
            Self::GrantAccess { .. } => "session.grantAccess",
            Self::RevokeAccess { .. } => "session.revokeAccess",
            Self::ListAccess { .. } => "session.listAccess",
            Self::RecoverAccessMutations => "session.accessMutationsRecover",
            Self::ReconcileAccessMutation { .. } => "session.accessMutationReconcile",
            Self::AcknowledgeAccessMutation { .. } => "session.accessMutationAck",
            Self::SnapshotRefresh => "session.list",
        }
    }

    pub fn request_id(&self) -> Option<&str> {
        match self {
            Self::Create { request_id, .. }
            | Self::Stop { request_id, .. }
            | Self::Close { request_id, .. }
            | Self::Interrupt { request_id, .. }
            | Self::Delete { request_id, .. }
            | Self::Reopen { request_id, .. }
            | Self::ListHidden { request_id }
            | Self::SetShareScope { request_id, .. } => Some(request_id),
            Self::Leave { mutation_id, .. }
            | Self::GrantAccess { mutation_id, .. }
            | Self::RevokeAccess { mutation_id, .. }
            | Self::ReconcileAccessMutation { mutation_id }
            | Self::AcknowledgeAccessMutation { mutation_id, .. } => Some(mutation_id),
            _ => None,
        }
    }

    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::Rename { session_id, .. }
            | Self::SetMode { session_id, .. }
            | Self::Stop { session_id, .. }
            | Self::Close { session_id, .. }
            | Self::Interrupt { session_id, .. }
            | Self::Delete { session_id, .. }
            | Self::Reopen { session_id, .. }
            | Self::OpenRemote { session_id }
            | Self::Hide { session_id }
            | Self::Leave { session_id, .. }
            | Self::Unhide { session_id }
            | Self::SetShareScope { session_id, .. }
            | Self::GrantAccess { session_id, .. }
            | Self::RevokeAccess { session_id, .. }
            | Self::ListAccess { session_id, .. } => Some(session_id),
            Self::Create { .. }
            | Self::ListHidden { .. }
            | Self::RecoverAccessMutations
            | Self::ReconcileAccessMutation { .. }
            | Self::AcknowledgeAccessMutation { .. }
            | Self::SnapshotRefresh => None,
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one exhaustive match keeps every session command boundary rule visible"
    )]
    pub(crate) fn validate(&self) -> Result<(), HostCommandValidationError> {
        match self {
            Self::Create {
                request_id,
                name,
                working_dir,
                resume,
            } => {
                validate_non_empty(request_id, "requestId")?;
                validate_non_empty(name, "name")?;
                if let Some(resume) = resume {
                    validate_uuid(
                        &resume.native_conversation_id,
                        "resume.nativeConversationId",
                    )?;
                    if working_dir.is_none() {
                        return Err(HostCommandValidationError::InvalidField {
                            field: "workingDir",
                            reason: "is required when resuming provider work",
                        });
                    }
                }
                Ok(())
            }
            Self::ListHidden { request_id } => validate_non_empty(request_id, "requestId"),
            Self::Rename { session_id, name } => {
                validate_non_empty(session_id, "sessionId")?;
                validate_non_empty(name, "name")
            }
            Self::SetMode {
                session_id,
                expected_runtime_incarnation_id,
                ..
            }
            | Self::ListAccess {
                session_id,
                expected_runtime_incarnation_id,
            } => {
                validate_non_empty(session_id, "sessionId")?;
                validate_uuid(
                    expected_runtime_incarnation_id,
                    "expectedRuntimeIncarnationId",
                )
            }
            Self::OpenRemote { session_id }
            | Self::Hide { session_id }
            | Self::Unhide { session_id } => validate_non_empty(session_id, "sessionId"),
            Self::Stop {
                request_id,
                session_id,
                expected_runtime_incarnation_id,
            }
            | Self::Close {
                request_id,
                session_id,
                expected_runtime_incarnation_id,
            }
            | Self::Interrupt {
                request_id,
                session_id,
                expected_runtime_incarnation_id,
            }
            | Self::Delete {
                request_id,
                session_id,
                expected_runtime_incarnation_id,
            }
            | Self::Reopen {
                request_id,
                session_id,
                expected_runtime_incarnation_id,
            } => {
                validate_uuid_v7(request_id, "requestId")?;
                validate_non_empty(session_id, "sessionId")?;
                validate_uuid(
                    expected_runtime_incarnation_id,
                    "expectedRuntimeIncarnationId",
                )
            }
            Self::Leave {
                mutation_id,
                session_id,
                expected_runtime_incarnation_id,
            } => {
                validate_non_empty(session_id, "sessionId")?;
                validate_uuid_v7(mutation_id, "mutationId")?;
                validate_uuid(
                    expected_runtime_incarnation_id,
                    "expectedRuntimeIncarnationId",
                )
            }
            Self::SetShareScope {
                request_id,
                session_id,
                expected_runtime_incarnation_id,
                scope,
                room_id,
            } => {
                validate_uuid_v7(request_id, "requestId")?;
                validate_non_empty(session_id, "sessionId")?;
                validate_uuid(
                    expected_runtime_incarnation_id,
                    "expectedRuntimeIncarnationId",
                )?;
                if *scope == ShareScope::Friends {
                    return Err(HostCommandValidationError::InvalidField {
                        field: "scope",
                        reason: "friends sharing is decode-only and cannot be selected",
                    });
                }
                if *scope == ShareScope::Room && room_id.as_deref().is_none_or(str::is_empty) {
                    return Err(HostCommandValidationError::MissingRoomId);
                }
                Ok(())
            }
            Self::GrantAccess {
                mutation_id,
                session_id,
                expected_runtime_incarnation_id,
                actor_user_id,
                expires_at,
                ..
            } => {
                validate_non_empty(session_id, "sessionId")?;
                validate_uuid_v7(mutation_id, "mutationId")?;
                validate_uuid(
                    expected_runtime_incarnation_id,
                    "expectedRuntimeIncarnationId",
                )?;
                validate_non_empty(actor_user_id, "actorUserId")?;
                validate_non_empty(expires_at, "expiresAt")?;
                time::OffsetDateTime::parse(
                    expires_at,
                    &time::format_description::well_known::Rfc3339,
                )
                .map_err(|_| HostCommandValidationError::InvalidField {
                    field: "expiresAt",
                    reason: "must be an RFC3339 timestamp",
                })?;
                Ok(())
            }
            Self::RevokeAccess {
                mutation_id,
                session_id,
                expected_runtime_incarnation_id,
                actor_user_id,
            } => {
                validate_non_empty(session_id, "sessionId")?;
                validate_uuid_v7(mutation_id, "mutationId")?;
                validate_uuid(
                    expected_runtime_incarnation_id,
                    "expectedRuntimeIncarnationId",
                )?;
                validate_non_empty(actor_user_id, "actorUserId")
            }
            Self::RecoverAccessMutations | Self::SnapshotRefresh => Ok(()),
            Self::ReconcileAccessMutation { mutation_id } => {
                validate_uuid_v7(mutation_id, "mutationId")
            }
            Self::AcknowledgeAccessMutation {
                mutation_id,
                fingerprint,
            } => {
                validate_uuid_v7(mutation_id, "mutationId")?;
                if fingerprint.len() != 64
                    || !fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    return Err(HostCommandValidationError::InvalidField {
                        field: "fingerprint",
                        reason: "must be a SHA-256 hex digest",
                    });
                }
                Ok(())
            }
        }
    }
}

fn validate_uuid(value: &str, field: &'static str) -> Result<(), HostCommandValidationError> {
    let id =
        uuid::Uuid::parse_str(value).map_err(|_| HostCommandValidationError::InvalidField {
            field,
            reason: "must be a UUID",
        })?;
    if id.is_nil() {
        return Err(HostCommandValidationError::InvalidField {
            field,
            reason: "must not be nil",
        });
    }
    Ok(())
}

fn validate_uuid_v7(value: &str, field: &'static str) -> Result<(), HostCommandValidationError> {
    validate_uuid(value, field)?;
    let id =
        uuid::Uuid::parse_str(value).map_err(|_| HostCommandValidationError::InvalidField {
            field,
            reason: "must be a UUIDv7",
        })?;
    if id.get_version_num() != 7 {
        return Err(HostCommandValidationError::InvalidField {
            field,
            reason: "must be a UUIDv7",
        });
    }
    Ok(())
}

fn validate_non_empty(value: &str, field: &'static str) -> Result<(), HostCommandValidationError> {
    if value.trim().is_empty() {
        return Err(HostCommandValidationError::EmptyField(field));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SessionEvent {
    #[serde(rename = "session.list")]
    List { sessions: Vec<SessionListEntry> },
    #[serde(rename = "session.upsert")]
    Upsert { session: Box<SessionListEntry> },
    #[serde(rename = "session.created")]
    Created {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "runtimeIncarnationId")]
        runtime_incarnation_id: String,
    },
    #[serde(rename = "session.removed")]
    Removed {
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    #[serde(rename = "session.opened")]
    Opened {
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    #[serde(rename = "session.interrupted")]
    Interrupted {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "runtimeIncarnationId")]
        runtime_incarnation_id: String,
    },
    #[serde(rename = "session.hiddenList")]
    HiddenList {
        #[serde(rename = "requestId")]
        request_id: String,
        entries: Vec<HiddenSessionEntry>,
    },

    #[serde(rename = "room.list")]
    RoomList { rooms: Vec<RoomListEntry> },
    #[serde(rename = "action.result")]
    ActionResult {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "actionId")]
        action_id: String,
        status: RelayActionStatus,
    },

    #[serde(rename = "session.scopeAccepted")]
    ScopeAccepted {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
        scope: ShareScope,
        #[serde(rename = "roomId")]
        #[serde(skip_serializing_if = "Option::is_none")]
        room_id: Option<String>,
        #[serde(rename = "budgetMs")]
        budget_ms: u64,
    },
    #[serde(rename = "session.scopeChanged")]
    ScopeChanged {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
        scope: ShareScope,
        #[serde(rename = "roomId")]
        #[serde(skip_serializing_if = "Option::is_none")]
        room_id: Option<String>,
    },
    #[serde(rename = "session.accessMutationAccepted")]
    AccessMutationAccepted {
        #[serde(rename = "mutationId")]
        mutation_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
        kind: SessionAccessMutationKind,
        #[serde(rename = "actorUserId", skip_serializing_if = "Option::is_none")]
        actor_user_id: Option<String>,
        #[serde(rename = "accessLevel", skip_serializing_if = "Option::is_none")]
        access_level: Option<AccessLevel>,
        #[serde(rename = "expiresAt", skip_serializing_if = "Option::is_none")]
        expires_at: Option<String>,
    },
    #[serde(rename = "session.accessMutationResult")]
    AccessMutationResult {
        #[serde(rename = "mutationId")]
        mutation_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
        kind: SessionAccessMutationKind,
        #[serde(rename = "actorUserId", skip_serializing_if = "Option::is_none")]
        actor_user_id: Option<String>,
        #[serde(rename = "accessLevel", skip_serializing_if = "Option::is_none")]
        access_level: Option<AccessLevel>,
        #[serde(rename = "expiresAt", skip_serializing_if = "Option::is_none")]
        expires_at: Option<String>,
        outcome: SessionAccessMutationOutcome,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    #[serde(rename = "session.accessMutationRecovered")]
    AccessMutationRecovered {
        #[serde(rename = "mutationId")]
        mutation_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
        #[serde(rename = "originatingAccountEpoch")]
        originating_account_epoch: u64,
        fingerprint: String,
        kind: SessionAccessMutationKind,
        #[serde(rename = "actorUserId", skip_serializing_if = "Option::is_none")]
        actor_user_id: Option<String>,
        #[serde(rename = "accessLevel", skip_serializing_if = "Option::is_none")]
        access_level: Option<AccessLevel>,
        #[serde(rename = "expiresAt", skip_serializing_if = "Option::is_none")]
        expires_at: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        outcome: Option<SessionAccessMutationOutcome>,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    #[serde(rename = "session.error")]
    Error {
        operation: String,
        #[serde(rename = "sessionId")]
        #[serde(skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
        #[serde(rename = "requestId")]
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
        message: String,
    },
    #[serde(rename = "session.accessGrants")]
    AccessGrants {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "runtimeIncarnationId")]
        runtime_incarnation_id: String,
        #[serde(rename = "accountUserId")]
        account_user_id: String,
        grants: Vec<AccessGrantEntry>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HiddenSessionEntry {
    pub id: String,
    pub name: String,
    pub project: String,
    pub owner: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AccessGrantEntry {
    pub actor_user_id: String,
    pub handle: String,
    pub display_name: String,
    pub access_level: AccessLevel,
    pub granted_at: String,
    pub expires_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::SessionCommand;
    use kodosi_domain::permissions::ShareScope;

    #[test]
    fn friends_scope_decodes_but_live_mutation_validation_rejects_it() {
        let command: SessionCommand = serde_json::from_value(serde_json::json!({
            "type": "session.scope",
            "requestId": uuid::Uuid::now_v7().to_string(),
            "sessionId": "01900000-0000-7000-8000-000000000001",
            "expectedRuntimeIncarnationId": "01900000-0000-7000-8000-000000000002",
            "scope": "friends",
        }))
        .expect("historical friends scope must remain decodable");

        std::assert_matches!(
            command,
            SessionCommand::SetShareScope {
                scope: ShareScope::Friends,
                ..
            }
        );
        assert!(
            command
                .validate()
                .expect_err("friends mutation must be rejected")
                .to_string()
                .contains("decode-only")
        );
    }
}
