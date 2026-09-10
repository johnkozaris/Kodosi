use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomDto {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub owner_user_id: String,
    pub roster_generation: i64,
    pub roster_body: String,
    pub roster_signature: String,
    pub roster_signer_device_id: String,
    #[serde(default)]
    pub roster_activation_proof: Option<RoomInvitationProofDto>,
    #[serde(default)]
    pub admission_proofs: Vec<RoomInvitationProofDto>,
    #[serde(default)]
    pub roster_transitions: Vec<RoomRosterTransitionDto>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomRosterTransitionDto {
    pub generation: i64,
    pub roster_body: String,
    pub roster_signature: String,
    pub roster_signer_device_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomInvitationProofDto {
    pub invitation_id: String,
    pub invitee_user_id: String,
    pub proposal_body: String,
    pub proposal_signature: String,
    pub proposal_signer_device_id: String,
    pub proposal_hash: String,
    pub decision_body: String,
    pub decision_signature: String,
    pub decision_signer_device_id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomMemberDto {
    pub room_id: String,
    pub user_id: String,
    pub role: String,
    pub username: Option<String>,
    pub display_name: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomInvitationDto {
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
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    pub base_roster_generation: i64,
    pub proposed_roster_generation: i64,
    pub proposed_roster_body: String,
    pub proposed_roster_signature: String,
    pub proposed_roster_signer_device_id: String,
    pub proposal_body: String,
    pub proposal_signature: String,
    pub proposal_signer_device_id: String,
    pub proposal_hash: String,
    #[serde(with = "time::serde::rfc3339")]
    pub proposal_issued_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
    pub decision_body: Option<String>,
    pub decision_signature: Option<String>,
    pub decision_signer_device_id: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreateRoomRequest<'a> {
    pub room_id: &'a str,
    pub name: &'a str,
    pub slug: &'a str,
    pub roster_generation: i64,
    pub roster_body: &'a str,
    pub roster_signature: &'a str,
    pub roster_signer_device_id: &'a str,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomInvitationProposalRequest<'a> {
    pub invitation_id: &'a str,
    pub invitee_user_id: &'a str,
    pub proposal_body: &'a str,
    pub proposal_signature: &'a str,
    pub proposal_signer_device_id: &'a str,
    pub proposed_roster_generation: i64,
    pub proposed_roster_body: &'a str,
    pub proposed_roster_signature: &'a str,
    pub proposed_roster_signer_device_id: &'a str,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomInvitationDecisionRequest<'a> {
    pub request_id: &'a uuid::Uuid,
    pub decision_body: &'a str,
    pub decision_signature: &'a str,
    pub decision_signer_device_id: &'a str,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CancelRoomInvitationRequest<'a> {
    pub request_id: &'a uuid::Uuid,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReplaceRoomRosterRequest<'a> {
    pub request_id: &'a uuid::Uuid,
    #[serde(rename = "rosterGeneration")]
    pub generation: i64,
    #[serde(rename = "rosterBody")]
    pub body: &'a str,
    #[serde(rename = "rosterSignature")]
    pub signature: &'a str,
    #[serde(rename = "rosterSignerDeviceId")]
    pub signer_device_id: &'a str,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomChatMessageDto {
    pub id: String,
    pub room_id: String,
    pub author_user_id: String,
    pub author_session_id: Option<String>,
    pub author_kind: String,
    pub body: String,
    #[serde(default)]
    pub recipient_session_ids: Vec<String>,
    #[serde(default)]
    pub recipient_user_ids: Vec<String>,
    pub seq: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub posted_at: OffsetDateTime,
}
#[derive(Debug, Clone)]
pub struct RoomChatPageDto {
    pub items: Vec<RoomChatMessageDto>,
    pub has_more: bool,
    pub next_since: Option<i64>,
    pub next_before: Option<i64>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PostRoomChatRequest<'a> {
    pub message_id: &'a str,
    pub body: &'a str,
    pub author_session_id: Option<&'a str>,
    pub author_kind: &'a str,
    pub recipient_session_ids: &'a [String],
    pub recipient_user_ids: &'a [String],
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomTaskDto {
    pub id: String,
    pub room_id: String,
    pub created_by_user_id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub revision: i64,
    pub assigned_session_id: Option<String>,
    pub assigned_session_incarnation_id: Option<String>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub due_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub completed_at: Option<OffsetDateTime>,
    pub result: Option<String>,
    pub result_author_user_id: Option<String>,
    #[serde(skip)]
    pub content_unavailable: bool,
}

#[derive(Debug, Clone)]
pub struct RoomTaskPageDto {
    pub snapshot: String,
    pub items: Vec<RoomTaskDto>,
    pub has_more: bool,
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreateRoomTaskRequest<'a> {
    pub task_id: &'a str,
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub assigned_session_id: Option<&'a str>,
    pub assigned_session_incarnation_id: Option<&'a str>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub due_at: Option<OffsetDateTime>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateRoomTaskStatusRequest<'a> {
    pub request_id: &'a uuid::Uuid,
    pub expected_task_revision: i64,
    pub status: &'a str,
    pub actor_session_id: Option<&'a str>,
    pub actor_session_incarnation_id: Option<&'a str>,
    pub result: Option<&'a str>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssignRoomTaskRequest<'a> {
    pub request_id: &'a uuid::Uuid,
    pub expected_task_revision: i64,
    pub session_id: Option<&'a str>,
    pub session_incarnation_id: Option<&'a str>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RoomMutationOperationDto {
    RemoveMember,
    AcceptInvitation,
    DeclineInvitation,
    CancelInvitation,
    #[serde(rename = "tasks.assign")]
    TasksAssign,
    #[serde(rename = "tasks.transition")]
    TasksTransition,
}
impl RoomMutationOperationDto {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RemoveMember => "removeMember",
            Self::AcceptInvitation => "acceptInvitation",
            Self::DeclineInvitation => "declineInvitation",
            Self::CancelInvitation => "cancelInvitation",
            Self::TasksAssign => "tasks.assign",
            Self::TasksTransition => "tasks.transition",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RoomMutationReceiptDto {
    pub request_id: uuid::Uuid,
    pub operation: String,
    pub room_id: uuid::Uuid,
    pub entity_id: uuid::Uuid,
    pub result: String,
    pub revision: Option<i64>,
    pub assignee_session_id: Option<uuid::Uuid>,
    pub assignee_session_incarnation_id: Option<uuid::Uuid>,
    pub target_fingerprint: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomMutationResponseDto {
    pub request_id: uuid::Uuid,
    pub operation: String,
    pub room_id: uuid::Uuid,
    pub entity_id: uuid::Uuid,
    pub result: String,
    pub revision: Option<i64>,
    pub assignee_session_id: Option<uuid::Uuid>,
    pub assignee_session_incarnation_id: Option<uuid::Uuid>,
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{RoomInvitationDto, RoomRosterTransitionDto};

    fn invitation_json() -> Value {
        json!({
            "id": "invitation-1",
            "roomId": "room-1",
            "roomName": "Room",
            "roomSlug": "room",
            "inviteeUserId": "invitee",
            "inviteeHandle": "invitee-handle",
            "inviteeDisplayName": null,
            "invitedByUserId": "owner",
            "invitedByHandle": "owner-handle",
            "invitedByDisplayName": "Owner",
            "status": "Pending",
            "createdAt": "2026-08-19T12:00:00Z",
            "baseRosterGeneration": 1,
            "proposedRosterGeneration": 2,
            "proposedRosterBody": "roster-body",
            "proposedRosterSignature": "roster-signature",
            "proposedRosterSignerDeviceId": "owner-device",
            "proposalBody": "proposal-body",
            "proposalSignature": "proposal-signature",
            "proposalSignerDeviceId": "owner-device",
            "proposalHash": "proposal-hash",
            "proposalIssuedAt": "2026-08-19T12:00:00Z",
            "expiresAt": "2026-08-20T12:00:00Z",
            "decisionBody": null,
            "decisionSignature": null,
            "decisionSignerDeviceId": null
        })
    }

    fn roster_transition_json() -> Value {
        json!({
            "generation": 2,
            "rosterBody": "roster-body",
            "rosterSignature": "roster-signature",
            "rosterSignerDeviceId": "owner-device"
        })
    }

    #[test]
    fn room_invitation_decodes_without_retired_timestamp_fields() {
        let invitation: RoomInvitationDto = serde_json::from_value(invitation_json())
            .expect("invitation without retired timestamp fields should decode");

        assert_eq!(invitation.id, "invitation-1");
    }

    #[test]
    fn room_invitation_ignores_retired_timestamp_extra_fields() {
        let mut value = invitation_json();
        let object = value
            .as_object_mut()
            .expect("invitation fixture should be an object");
        object.insert("respondedAt".to_owned(), json!("2026-08-19T12:30:00Z"));
        object.insert("decisionIssuedAt".to_owned(), json!("2026-08-19T12:29:00Z"));

        let invitation: RoomInvitationDto = serde_json::from_value(value)
            .expect("invitation with retired timestamp fields should decode");

        assert_eq!(invitation.id, "invitation-1");
    }

    #[test]
    fn room_roster_transition_decodes_without_admission_invitation_id() {
        let transition: RoomRosterTransitionDto = serde_json::from_value(roster_transition_json())
            .expect("roster transition without admissionInvitationId should decode");

        assert_eq!(transition.generation, 2);
    }

    #[test]
    fn room_roster_transition_ignores_admission_invitation_id_extra_field() {
        let mut value = roster_transition_json();
        value
            .as_object_mut()
            .expect("roster transition fixture should be an object")
            .insert("admissionInvitationId".to_owned(), json!("invitation-1"));

        let transition: RoomRosterTransitionDto = serde_json::from_value(value)
            .expect("roster transition with admissionInvitationId should decode");

        assert_eq!(transition.generation, 2);
    }
}
