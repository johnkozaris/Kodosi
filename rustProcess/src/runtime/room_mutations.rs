use aws_lc_rs::digest;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AppError, Result};
use kodosi_backend_client::api::BackendRoomMutationReceipt;

const BACKEND_DOMAIN: &[u8] = b"kodosi:room-mutation-target:v1:";
const INTENT_DOMAIN: &[u8] = b"kodosi.room-mutation-intent.v1\0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum RoomMutationIntent {
    AcceptInvitation {
        room_id: String,
        invitation_id: String,
        expected_roster_generation: i64,
    },
    DeclineInvitation {
        room_id: String,
        invitation_id: String,
        expected_roster_generation: i64,
    },
    CancelInvitation {
        room_id: String,
        invitation_id: String,
        expected_roster_generation: i64,
    },
    RemoveMember {
        room_id: String,
        user_id: String,
        expected_roster_generation: i64,
    },
    AssignTask {
        room_id: String,
        task_id: String,
        expected_task_revision: i64,
        session_id: Option<String>,
        session_incarnation_id: Option<String>,
    },
    TransitionTask {
        room_id: String,
        task_id: String,
        expected_task_revision: i64,
        to_status: String,
        actor_session_id: Option<String>,
        actor_session_incarnation_id: Option<String>,
        result_intent_sha256: Option<String>,
    },
}
impl RoomMutationIntent {
    fn invitation_target(&self) -> Option<(&str, &str, i64)> {
        match self {
            Self::AcceptInvitation {
                room_id,
                invitation_id,
                expected_roster_generation,
            }
            | Self::DeclineInvitation {
                room_id,
                invitation_id,
                expected_roster_generation,
            }
            | Self::CancelInvitation {
                room_id,
                invitation_id,
                expected_roster_generation,
            } => Some((room_id, invitation_id, *expected_roster_generation)),
            _ => None,
        }
    }

    pub(crate) fn matches_target(&self, target: &RoomMutationTarget) -> bool {
        if let Some((room_id, invitation_id, expected_generation)) = self.invitation_target() {
            return target.invitation_target().is_some_and(
                |(target_room, target_invitation, base_generation)| {
                    room_id == target_room
                        && invitation_id == target_invitation
                        && expected_generation == base_generation
                },
            );
        }
        match (self, target) {
            (
                Self::RemoveMember {
                    room_id,
                    user_id,
                    expected_roster_generation,
                },
                RoomMutationTarget::RemoveMember {
                    room_id: target_room,
                    user_id: target_user,
                    base_roster_generation,
                    ..
                },
            ) => {
                room_id == target_room
                    && user_id == target_user
                    && expected_roster_generation == base_roster_generation
            }
            (
                Self::AssignTask {
                    room_id,
                    task_id,
                    expected_task_revision,
                    session_id,
                    session_incarnation_id,
                },
                RoomMutationTarget::AssignTask {
                    room_id: target_room,
                    task_id: target_task,
                    expected_task_revision: target_revision,
                    session_id: target_session,
                    session_incarnation_id: target_incarnation,
                },
            ) => {
                room_id == target_room
                    && task_id == target_task
                    && expected_task_revision == target_revision
                    && session_id == target_session
                    && session_incarnation_id == target_incarnation
            }
            (
                Self::TransitionTask {
                    room_id,
                    task_id,
                    expected_task_revision,
                    to_status,
                    actor_session_id,
                    actor_session_incarnation_id,
                    result_intent_sha256,
                },
                RoomMutationTarget::TransitionTask {
                    room_id: target_room,
                    task_id: target_task,
                    expected_task_revision: target_revision,
                    to_status: target_status,
                    actor_session_id: target_actor,
                    actor_session_incarnation_id: target_incarnation,
                    encrypted_result,
                },
            ) => {
                room_id == target_room
                    && task_id == target_task
                    && expected_task_revision == target_revision
                    && to_status == target_status
                    && actor_session_id == target_actor
                    && actor_session_incarnation_id == target_incarnation
                    && result_intent_sha256.is_some() == encrypted_result.is_some()
                    && result_intent_sha256.as_deref().is_none_or(is_sha256_hex)
            }
            _ => false,
        }
    }

    pub(crate) fn from_target(target: &RoomMutationTarget, result: Option<&str>) -> Self {
        match target {
            RoomMutationTarget::AcceptInvitation {
                room_id,
                invitation_id,
                base_roster_generation,
                ..
            } => Self::AcceptInvitation {
                room_id: room_id.clone(),
                invitation_id: invitation_id.clone(),
                expected_roster_generation: *base_roster_generation,
            },
            RoomMutationTarget::DeclineInvitation {
                room_id,
                invitation_id,
                base_roster_generation,
                ..
            } => Self::DeclineInvitation {
                room_id: room_id.clone(),
                invitation_id: invitation_id.clone(),
                expected_roster_generation: *base_roster_generation,
            },
            RoomMutationTarget::CancelInvitation {
                room_id,
                invitation_id,
                base_roster_generation,
            } => Self::CancelInvitation {
                room_id: room_id.clone(),
                invitation_id: invitation_id.clone(),
                expected_roster_generation: *base_roster_generation,
            },
            RoomMutationTarget::RemoveMember {
                room_id,
                user_id,
                base_roster_generation,
                ..
            } => Self::RemoveMember {
                room_id: room_id.clone(),
                user_id: user_id.clone(),
                expected_roster_generation: *base_roster_generation,
            },
            RoomMutationTarget::AssignTask {
                room_id,
                task_id,
                expected_task_revision,
                session_id,
                session_incarnation_id,
            } => Self::AssignTask {
                room_id: room_id.clone(),
                task_id: task_id.clone(),
                expected_task_revision: *expected_task_revision,
                session_id: session_id.clone(),
                session_incarnation_id: session_incarnation_id.clone(),
            },
            RoomMutationTarget::TransitionTask {
                room_id,
                task_id,
                expected_task_revision,
                to_status,
                actor_session_id,
                actor_session_incarnation_id,
                ..
            } => Self::TransitionTask {
                room_id: room_id.clone(),
                task_id: task_id.clone(),
                expected_task_revision: *expected_task_revision,
                to_status: to_status.clone(),
                actor_session_id: actor_session_id.clone(),
                actor_session_incarnation_id: actor_session_incarnation_id.clone(),
                result_intent_sha256: result.map(intent_digest),
            },
        }
    }
    pub(crate) fn from_command(c: &crate::host_protocol::RoomCommand) -> Option<Self> {
        use crate::host_protocol::RoomCommand;
        match c {
            RoomCommand::RemoveMember {
                room_id,
                user_id,
                expected_roster_generation,
                ..
            } => Some(Self::RemoveMember {
                room_id: room_id.clone(),
                user_id: user_id.clone(),
                expected_roster_generation: *expected_roster_generation,
            }),
            RoomCommand::AcceptInvitation {
                room_id,
                invitation_id,
                expected_roster_generation,
                ..
            } => Some(Self::AcceptInvitation {
                room_id: room_id.clone(),
                invitation_id: invitation_id.clone(),
                expected_roster_generation: *expected_roster_generation,
            }),
            RoomCommand::DeclineInvitation {
                room_id,
                invitation_id,
                expected_roster_generation,
                ..
            } => Some(Self::DeclineInvitation {
                room_id: room_id.clone(),
                invitation_id: invitation_id.clone(),
                expected_roster_generation: *expected_roster_generation,
            }),
            RoomCommand::CancelInvitation {
                room_id,
                invitation_id,
                expected_roster_generation,
                ..
            } => Some(Self::CancelInvitation {
                room_id: room_id.clone(),
                invitation_id: invitation_id.clone(),
                expected_roster_generation: *expected_roster_generation,
            }),
            RoomCommand::TaskAssign {
                room_id,
                task_id,
                expected_task_revision,
                session_id,
                session_incarnation_id,
                ..
            } => Some(Self::AssignTask {
                room_id: room_id.clone(),
                task_id: task_id.clone(),
                expected_task_revision: *expected_task_revision,
                session_id: session_id.clone(),
                session_incarnation_id: session_incarnation_id.clone(),
            }),
            RoomCommand::TaskTransition {
                room_id,
                task_id,
                expected_task_revision,
                to_status,
                actor_session_id,
                actor_session_incarnation_id,
                result,
                ..
            } => Some(Self::TransitionTask {
                room_id: room_id.clone(),
                task_id: task_id.clone(),
                expected_task_revision: *expected_task_revision,
                to_status: to_status.clone(),
                actor_session_id: actor_session_id.clone(),
                actor_session_incarnation_id: actor_session_incarnation_id.clone(),
                result_intent_sha256: result.as_deref().map(intent_digest),
            }),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum RoomMutationTarget {
    AcceptInvitation {
        room_id: String,
        invitation_id: String,
        base_roster_generation: i64,
        proposed_roster_generation: i64,
        decision_body: String,
        decision_signature: String,
        decision_signer_device_id: String,
    },
    DeclineInvitation {
        room_id: String,
        invitation_id: String,
        base_roster_generation: i64,
        decision_body: String,
        decision_signature: String,
        decision_signer_device_id: String,
    },
    CancelInvitation {
        room_id: String,
        invitation_id: String,
        base_roster_generation: i64,
    },
    RemoveMember {
        room_id: String,
        user_id: String,
        base_roster_generation: i64,
        desired_roster_generation: i64,
        roster_body: String,
        roster_signature: String,
        roster_signer_device_id: String,
    },
    AssignTask {
        room_id: String,
        task_id: String,
        expected_task_revision: i64,
        session_id: Option<String>,
        session_incarnation_id: Option<String>,
    },
    TransitionTask {
        room_id: String,
        task_id: String,
        expected_task_revision: i64,
        to_status: String,
        actor_session_id: Option<String>,
        actor_session_incarnation_id: Option<String>,
        encrypted_result: Option<String>,
    },
}
impl RoomMutationTarget {
    fn invitation_target(&self) -> Option<(&str, &str, i64)> {
        match self {
            Self::AcceptInvitation {
                room_id,
                invitation_id,
                base_roster_generation,
                ..
            }
            | Self::DeclineInvitation {
                room_id,
                invitation_id,
                base_roster_generation,
                ..
            }
            | Self::CancelInvitation {
                room_id,
                invitation_id,
                base_roster_generation,
            } => Some((room_id, invitation_id, *base_roster_generation)),
            _ => None,
        }
    }

    pub(crate) fn operation(&self) -> kodosi_backend_client::api::BackendRoomMutationOperation {
        use kodosi_backend_client::api::BackendRoomMutationOperation as O;
        match self {
            Self::AcceptInvitation { .. } => O::AcceptInvitation,
            Self::DeclineInvitation { .. } => O::DeclineInvitation,
            Self::CancelInvitation { .. } => O::CancelInvitation,
            Self::RemoveMember { .. } => O::RemoveMember,
            Self::AssignTask { .. } => O::TasksAssign,
            Self::TransitionTask { .. } => O::TasksTransition,
        }
    }
    pub(crate) fn kind(&self) -> &'static str {
        self.operation().as_str()
    }
    pub(crate) fn room_id(&self) -> &str {
        match self {
            Self::AcceptInvitation { room_id, .. }
            | Self::DeclineInvitation { room_id, .. }
            | Self::CancelInvitation { room_id, .. }
            | Self::RemoveMember { room_id, .. }
            | Self::AssignTask { room_id, .. }
            | Self::TransitionTask { room_id, .. } => room_id,
        }
    }
    pub(crate) fn fingerprint(&self) -> Result<String> {
        let mut w = FingerprintWriter::new();
        match self {
            Self::AcceptInvitation {
                room_id,
                invitation_id,
                base_roster_generation,
                proposed_roster_generation,
                decision_body,
                decision_signature,
                decision_signer_device_id,
            } => {
                w.string("acceptInvitation")?;
                w.guid(room_id)?;
                w.guid(invitation_id)?;
                w.i64(*base_roster_generation)?;
                w.i64(*proposed_roster_generation)?;
                w.hash_b64(decision_body)?;
                w.hash_b64(decision_signature)?;
                w.string(decision_signer_device_id)?;
            }
            Self::DeclineInvitation {
                room_id,
                invitation_id,
                base_roster_generation,
                decision_body,
                decision_signature,
                decision_signer_device_id,
            } => {
                w.string("declineInvitation")?;
                w.guid(room_id)?;
                w.guid(invitation_id)?;
                w.i64(*base_roster_generation)?;
                w.hash_b64(decision_body)?;
                w.hash_b64(decision_signature)?;
                w.string(decision_signer_device_id)?;
            }
            Self::CancelInvitation {
                room_id,
                invitation_id,
                base_roster_generation,
            } => {
                w.string("cancelInvitation")?;
                w.guid(room_id)?;
                w.guid(invitation_id)?;
                w.i64(*base_roster_generation)?;
            }
            Self::RemoveMember {
                room_id,
                user_id,
                base_roster_generation,
                desired_roster_generation,
                roster_body,
                roster_signature,
                roster_signer_device_id,
            } => {
                w.string("removeMember")?;
                w.guid(room_id)?;
                w.guid(user_id)?;
                w.i64(*base_roster_generation)?;
                w.i64(*desired_roster_generation)?;
                w.hash_b64(roster_body)?;
                w.hash_b64(roster_signature)?;
                w.string(roster_signer_device_id)?;
            }
            Self::AssignTask {
                room_id,
                task_id,
                expected_task_revision,
                session_id,
                session_incarnation_id,
            } => {
                w.string("assignTask")?;
                w.guid(room_id)?;
                w.guid(task_id)?;
                w.i64(*expected_task_revision)?;
                w.nullable_guid(session_id.as_deref())?;
                w.nullable_guid(session_incarnation_id.as_deref())?;
            }
            Self::TransitionTask {
                room_id,
                task_id,
                expected_task_revision,
                to_status,
                actor_session_id,
                actor_session_incarnation_id,
                encrypted_result,
            } => {
                w.string("transitionTask")?;
                w.guid(room_id)?;
                w.guid(task_id)?;
                w.i64(*expected_task_revision)?;
                w.string(to_status)?;
                w.nullable_guid(actor_session_id.as_deref())?;
                w.nullable_guid(actor_session_incarnation_id.as_deref())?;
                w.nullable_hash(encrypted_result.as_deref())?;
            }
        }
        Ok(hex(digest::digest(&digest::SHA256, &w.bytes).as_ref()))
    }
    pub(crate) fn match_receipt(
        &self,
        mutation_id: Uuid,
        fingerprint: &str,
        receipt: &BackendRoomMutationReceipt,
    ) -> Option<RoomMutationReceiptOutcome> {
        let expected = self.expected_receipt().ok()?;
        if receipt.request_id != mutation_id
            || receipt.operation != self.kind()
            || receipt.room_id != expected.room_id
            || receipt.entity_id != expected.entity_id
            || receipt.revision != expected.revision
            || !expected.assignee.matches(
                receipt.assignee_session_id,
                receipt.assignee_session_incarnation_id,
            )
            || receipt.target_fingerprint != fingerprint
        {
            return None;
        }
        if receipt.result == expected.result {
            return Some(RoomMutationReceiptOutcome::Succeeded);
        }
        match (self, receipt.result.as_str()) {
            (Self::AcceptInvitation { .. } | Self::DeclineInvitation { .. }, "Expired") => {
                Some(RoomMutationReceiptOutcome::Failed(
                    "Invitation proposal has expired; request a new invitation.",
                ))
            }
            (Self::AcceptInvitation { .. }, "Superseded") => {
                Some(RoomMutationReceiptOutcome::Failed(
                    "Room roster changed after this invitation was proposed; re-invite the member.",
                ))
            }
            _ => None,
        }
    }

    fn expected_receipt(&self) -> std::result::Result<ExpectedRoomMutationReceipt<'_>, ()> {
        let parse = |value: &str| Uuid::parse_str(value).map_err(|_| ());
        match self {
            Self::AcceptInvitation {
                room_id,
                invitation_id,
                proposed_roster_generation,
                ..
            } => Ok(ExpectedRoomMutationReceipt {
                room_id: parse(room_id)?,
                entity_id: parse(invitation_id)?,
                result: "Accepted",
                revision: Some(*proposed_roster_generation),
                assignee: ExpectedAssignee::Exact(None, None),
            }),
            Self::DeclineInvitation {
                room_id,
                invitation_id,
                base_roster_generation,
                ..
            } => Ok(ExpectedRoomMutationReceipt {
                room_id: parse(room_id)?,
                entity_id: parse(invitation_id)?,
                result: "Declined",
                revision: Some(*base_roster_generation),
                assignee: ExpectedAssignee::Exact(None, None),
            }),
            Self::CancelInvitation {
                room_id,
                invitation_id,
                base_roster_generation,
            } => Ok(ExpectedRoomMutationReceipt {
                room_id: parse(room_id)?,
                entity_id: parse(invitation_id)?,
                result: "Cancelled",
                revision: Some(*base_roster_generation),
                assignee: ExpectedAssignee::Exact(None, None),
            }),
            Self::RemoveMember {
                room_id,
                user_id,
                desired_roster_generation,
                ..
            } => Ok(ExpectedRoomMutationReceipt {
                room_id: parse(room_id)?,
                entity_id: parse(user_id)?,
                result: "Removed",
                revision: Some(*desired_roster_generation),
                assignee: ExpectedAssignee::Exact(None, None),
            }),
            Self::AssignTask {
                room_id,
                task_id,
                expected_task_revision,
                session_id,
                session_incarnation_id,
            } => Ok(ExpectedRoomMutationReceipt {
                room_id: parse(room_id)?,
                entity_id: parse(task_id)?,
                result: "Assigned",
                revision: Some(expected_task_revision.checked_add(1).ok_or(())?),
                assignee: ExpectedAssignee::Exact(
                    session_id.as_deref().map(parse).transpose()?,
                    session_incarnation_id.as_deref().map(parse).transpose()?,
                ),
            }),
            Self::TransitionTask {
                room_id,
                task_id,
                expected_task_revision,
                to_status,
                actor_session_id,
                actor_session_incarnation_id,
                ..
            } => Ok(ExpectedRoomMutationReceipt {
                room_id: parse(room_id)?,
                entity_id: parse(task_id)?,
                result: to_status,
                revision: Some(expected_task_revision.checked_add(1).ok_or(())?),
                assignee: if let Some(session_id) = actor_session_id.as_deref() {
                    ExpectedAssignee::Exact(
                        Some(parse(session_id)?),
                        actor_session_incarnation_id
                            .as_deref()
                            .map(parse)
                            .transpose()?,
                    )
                } else {
                    ExpectedAssignee::Any
                },
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoomMutationReceiptOutcome {
    Succeeded,
    Failed(&'static str),
}

struct ExpectedRoomMutationReceipt<'a> {
    room_id: Uuid,
    entity_id: Uuid,
    result: &'a str,
    revision: Option<i64>,
    assignee: ExpectedAssignee,
}

#[derive(Debug, Clone, Copy)]
enum ExpectedAssignee {
    Exact(Option<Uuid>, Option<Uuid>),
    Any,
}

impl ExpectedAssignee {
    fn matches(self, session_id: Option<Uuid>, incarnation_id: Option<Uuid>) -> bool {
        match self {
            Self::Exact(expected_session, expected_incarnation) => {
                session_id == expected_session && incarnation_id == expected_incarnation
            }
            Self::Any => session_id.is_some() == incarnation_id.is_some(),
        }
    }
}

struct FingerprintWriter {
    bytes: Vec<u8>,
}
impl FingerprintWriter {
    fn new() -> Self {
        Self {
            bytes: BACKEND_DOMAIN.to_vec(),
        }
    }
    fn raw(&mut self, bytes: &[u8]) -> Result<()> {
        let length = i32::try_from(bytes.len()).map_err(|_| AppError::Unsupported {
            reason: "room mutation fingerprint field exceeds i32 length encoding".to_owned(),
        })?;
        self.bytes.extend_from_slice(&length.to_be_bytes());
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    fn string(&mut self, value: &str) -> Result<()> {
        self.raw(value.as_bytes())
    }
    fn guid(&mut self, s: &str) -> Result<()> {
        let id = Uuid::parse_str(s).map_err(|_| AppError::InvalidBackendData {
            field: "roomMutation.guid".into(),
            reason: "must be UUID".into(),
        })?;
        self.raw(id.as_bytes())
    }
    fn nullable_guid(&mut self, s: Option<&str>) -> Result<()> {
        self.bytes.push(u8::from(s.is_some()));
        if let Some(s) = s {
            self.guid(s)?;
        }
        Ok(())
    }
    fn i64(&mut self, value: i64) -> Result<()> {
        self.raw(&value.to_be_bytes())
    }
    fn hash(&mut self, bytes: &[u8]) -> Result<()> {
        self.raw(digest::digest(&digest::SHA256, bytes).as_ref())
    }
    fn hash_b64(&mut self, s: &str) -> Result<()> {
        use base64::Engine as _;
        let b = base64::engine::general_purpose::STANDARD
            .decode(s)
            .map_err(|e| AppError::InvalidBackendData {
                field: "roomMutation.base64".into(),
                reason: e.to_string(),
            })?;
        self.hash(&b)
    }
    fn nullable_hash(&mut self, value: Option<&str>) -> Result<()> {
        self.bytes.push(u8::from(value.is_some()));
        if let Some(value) = value {
            self.hash(value.as_bytes())?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PreparedRoomMutationState {
    Prepared,
    Attempting,
    OutcomeUnknown,
    Terminal(DurableRoomMutationTerminal),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DurableRoomMutationTerminal {
    pub(crate) status: DurableRoomMutationTerminalStatus,
    pub(crate) entity_id: Option<String>,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DurableRoomMutationTerminalStatus {
    Succeeded,
    Conflict,
    Failed,
}

impl DurableRoomMutationTerminalStatus {
    pub(crate) const fn action_status(self) -> crate::host_protocol::RoomActionStatus {
        match self {
            Self::Succeeded => crate::host_protocol::RoomActionStatus::Succeeded,
            Self::Conflict => crate::host_protocol::RoomActionStatus::Conflict,
            Self::Failed => crate::host_protocol::RoomActionStatus::Failed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PreparedRoomMutation {
    pub(crate) mutation_id: Uuid,
    pub(crate) account_user_id: String,
    pub(crate) intent: RoomMutationIntent,
    pub(crate) fingerprint: String,
    pub(crate) target: RoomMutationTarget,
    pub(crate) state: PreparedRoomMutationState,
}
impl PreparedRoomMutation {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.mutation_id.get_version_num() != 7
            || self.mutation_id.to_string().len() != 36
            || self.account_user_id.trim().is_empty()
            || self.account_user_id.len() > 1_024
        {
            return Err(AppError::InvalidBackendData {
                field: "pendingRoomMutations.identity".into(),
                reason: "mutationId must be UUIDv7 and accountUserId must be bounded".into(),
            });
        }
        if !self.intent.matches_target(&self.target) {
            return Err(AppError::InvalidBackendData {
                field: "pendingRoomMutations.intent".into(),
                reason: "intent does not match the retained exact target".into(),
            });
        }
        let fingerprint = self.target.fingerprint()?;
        if self.fingerprint != fingerprint || !is_sha256_hex(&self.fingerprint) {
            return Err(AppError::InvalidBackendData {
                field: "pendingRoomMutations.fingerprint".into(),
                reason: "stored fingerprint does not match the retained exact target".into(),
            });
        }
        if let PreparedRoomMutationState::Terminal(terminal) = &self.state
            && (terminal
                .entity_id
                .as_ref()
                .is_some_and(|value| value.trim().is_empty() || value.len() > 1_024)
                || terminal
                    .message
                    .as_ref()
                    .is_some_and(|value| value.len() > 16 * 1_024))
        {
            return Err(AppError::InvalidBackendData {
                field: "pendingRoomMutations.terminal".into(),
                reason: "terminal entity and message must be bounded".into(),
            });
        }
        Ok(())
    }

    pub(crate) fn new(
        id: Uuid,
        account: String,
        intent: RoomMutationIntent,
        target: RoomMutationTarget,
    ) -> Result<Self> {
        if id.get_version_num() != 7 {
            return Err(AppError::Unsupported {
                reason: "room mutationId must be UUIDv7".into(),
            });
        }
        let fingerprint = target.fingerprint()?;
        Ok(Self {
            mutation_id: id,
            account_user_id: account,
            intent,
            fingerprint,
            target,
            state: PreparedRoomMutationState::Prepared,
        })
    }
}
fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

fn intent_digest(s: &str) -> String {
    let mut b = INTENT_DOMAIN.to_vec();
    b.extend_from_slice(s.as_bytes());
    hex(digest::digest(&digest::SHA256, &b).as_ref())
}
fn hex(b: &[u8]) -> String {
    b.iter()
        .fold(String::with_capacity(b.len() * 2), |mut o, v| {
            use std::fmt::Write as _;
            let _ = write!(o, "{v:02x}");
            o
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changed_plaintext_intent_conflicts() {
        let c = |r: &str| crate::host_protocol::RoomCommand::TaskTransition {
            room_id: "01900000-0000-7000-8000-000000000010".into(),
            task_id: "01900000-0000-7000-8000-000000000011".into(),
            expected_task_revision: 7,
            to_status: "Done".into(),
            actor_session_id: None,
            actor_session_incarnation_id: None,
            result: Some(r.into()),
            request_id: "01900000-0000-7000-8000-000000000001".into(),
        };
        assert_ne!(
            RoomMutationIntent::from_command(&c("a")),
            RoomMutationIntent::from_command(&c("b"))
        );
    }
    #[test]
    fn receipt_matching_requires_every_correlated_field() {
        let mutation_id = Uuid::parse_str("01900000-0000-7000-8000-000000000001").unwrap();
        let target = RoomMutationTarget::AssignTask {
            room_id: "01900000-0000-7000-8000-000000000010".into(),
            task_id: "01900000-0000-7000-8000-000000000011".into(),
            expected_task_revision: 7,
            session_id: Some("01900000-0000-7000-8000-000000000012".into()),
            session_incarnation_id: Some("01900000-0000-7000-8000-000000000013".into()),
        };
        let fingerprint = target.fingerprint().unwrap();
        let mut receipt = BackendRoomMutationReceipt {
            request_id: mutation_id,
            operation: "tasks.assign".into(),
            room_id: Uuid::parse_str("01900000-0000-7000-8000-000000000010").unwrap(),
            entity_id: Uuid::parse_str("01900000-0000-7000-8000-000000000011").unwrap(),
            result: "Assigned".into(),
            revision: Some(8),
            assignee_session_id: Some(
                Uuid::parse_str("01900000-0000-7000-8000-000000000012").unwrap(),
            ),
            assignee_session_incarnation_id: Some(
                Uuid::parse_str("01900000-0000-7000-8000-000000000013").unwrap(),
            ),
            target_fingerprint: fingerprint.clone(),
            created_at: time::OffsetDateTime::UNIX_EPOCH,
        };
        assert_eq!(
            target.match_receipt(mutation_id, &fingerprint, &receipt),
            Some(RoomMutationReceiptOutcome::Succeeded)
        );
        receipt.entity_id = Uuid::parse_str("01900000-0000-7000-8000-000000000014").unwrap();
        assert_eq!(
            target.match_receipt(mutation_id, &fingerprint, &receipt),
            None
        );
    }

    #[test]
    fn invitation_lifecycle_receipt_is_a_correlated_failure() {
        let mutation_id = Uuid::parse_str("01900000-0000-7000-8000-000000000001").unwrap();
        let target = RoomMutationTarget::AcceptInvitation {
            room_id: "01900000-0000-7000-8000-000000000010".into(),
            invitation_id: "01900000-0000-7000-8000-000000000011".into(),
            base_roster_generation: 7,
            proposed_roster_generation: 8,
            decision_body: "YQ==".into(),
            decision_signature: "Yg==".into(),
            decision_signer_device_id: "device".into(),
        };
        let fingerprint = target.fingerprint().unwrap();
        let receipt = BackendRoomMutationReceipt {
            request_id: mutation_id,
            operation: "acceptInvitation".into(),
            room_id: Uuid::parse_str("01900000-0000-7000-8000-000000000010").unwrap(),
            entity_id: Uuid::parse_str("01900000-0000-7000-8000-000000000011").unwrap(),
            result: "Superseded".into(),
            revision: Some(8),
            assignee_session_id: None,
            assignee_session_incarnation_id: None,
            target_fingerprint: fingerprint.clone(),
            created_at: time::OffsetDateTime::UNIX_EPOCH,
        };

        assert_eq!(
            target.match_receipt(mutation_id, &fingerprint, &receipt),
            Some(RoomMutationReceiptOutcome::Failed(
                "Room roster changed after this invitation was proposed; re-invite the member."
            ))
        );
    }

    #[test]
    fn receipt_matching_rejects_revision_overflow() {
        let target = RoomMutationTarget::AssignTask {
            room_id: "01900000-0000-7000-8000-000000000010".into(),
            task_id: "01900000-0000-7000-8000-000000000011".into(),
            expected_task_revision: i64::MAX,
            session_id: None,
            session_incarnation_id: None,
        };
        assert!(target.expected_receipt().is_err());
    }

    #[test]
    fn canonical_operations() {
        let t = RoomMutationTarget::AssignTask {
            room_id: "01900000-0000-7000-8000-000000000010".into(),
            task_id: "01900000-0000-7000-8000-000000000011".into(),
            expected_task_revision: 1,
            session_id: None,
            session_incarnation_id: None,
        };
        assert_eq!(t.kind(), "tasks.assign");
    }
}
