use serde::{Deserialize, Serialize};

use crate::{
    BackendClientError, Result, labels,
    session_relay_authority::{relay_protocol_uses_exact_match, relay_protocol_version},
};
use kodosi_domain::{
    lifecycle::RemoteActionStatus,
    permissions::{AccessLevel, SessionCapabilities},
    session::SessionState,
};

#[derive(Debug, Deserialize)]
pub struct SessionRelayEnvelope {
    #[serde(rename = "type")]
    pub message_type: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantAcceptedMessage {
    pub session_id: String,
    pub access: String,
    pub capabilities: SessionCapabilities,
    pub incarnation_id: uuid::Uuid,
    pub incarnation_generation: u64,
    pub relay_protocol_version: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum BackendSessionStatus {
    Pending,
    Live,
    Reconnecting,
    Ended,
}

impl ParticipantAcceptedMessage {
    pub fn validate_incarnation(&self, expected_incarnation_id: &uuid::Uuid) -> Result<()> {
        if !relay_protocol_uses_exact_match()
            || self.incarnation_id != *expected_incarnation_id
            || self.incarnation_generation == 0
            || self.relay_protocol_version != relay_protocol_version()
        {
            return Err(BackendClientError::InvalidBackendData {
                field: "participant.accepted.incarnationId".to_owned(),
                reason: "backend accepted a different session incarnation or protocol version"
                    .to_owned(),
            });
        }
        Ok(())
    }

    pub fn access_level(&self) -> Result<AccessLevel> {
        labels::parse_access_level(&self.access).ok_or_else(|| {
            BackendClientError::InvalidBackendData {
                field: "participant.accepted.access".to_owned(),
                reason: format!("unknown access level {}", self.access),
            }
        })
    }

    pub fn validated_access(&self) -> Result<AccessLevel> {
        let access = self.access_level()?;
        let expected = SessionCapabilities::from_access(access, false);
        if self.capabilities != expected {
            return Err(BackendClientError::InvalidBackendData {
                field: "participant.accepted.capabilities".to_owned(),
                reason: "capabilities do not match the granted access role".to_owned(),
            });
        }
        Ok(access)
    }
}

impl BackendSessionStatus {
    pub const fn into_session_state(self) -> SessionState {
        match self {
            Self::Pending => SessionState::Starting,
            Self::Live => SessionState::Running,
            Self::Reconnecting => SessionState::Reconnecting,
            Self::Ended => SessionState::Stopped,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ActionResultMessage, ParticipantAcceptedMessage};
    use crate::session_relay_authority::relay_protocol_version;
    use kodosi_domain::permissions::SessionCapabilities;

    #[test]
    fn action_result_preserves_optional_permission_generation() {
        let permission: ActionResultMessage = serde_json::from_str(
            r#"{"sessionId":"session-1","actionId":"action-1","requestId":"tool-1","requestGeneration":7,"status":"busy"}"#,
        )
        .expect("permission action result");
        assert_eq!(permission.request_id.as_deref(), Some("tool-1"));
        assert_eq!(permission.request_generation, Some(7));

        let generic: ActionResultMessage = serde_json::from_str(
            r#"{"sessionId":"session-1","actionId":"resize-1","status":"accepted"}"#,
        )
        .expect("generic action result");
        assert!(generic.request_id.is_none());
        assert!(generic.request_generation.is_none());
    }

    #[test]
    fn participant_accepted_rejects_a_different_incarnation() {
        let accepted = ParticipantAcceptedMessage {
            session_id: "session-1".to_owned(),
            access: "View".to_owned(),
            capabilities: SessionCapabilities(SessionCapabilities::VIEW),
            incarnation_id: uuid::Uuid::from_u128(2),
            incarnation_generation: 2,
            relay_protocol_version: relay_protocol_version(),
        };

        assert!(
            accepted
                .validate_incarnation(&uuid::Uuid::from_u128(3))
                .is_err()
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RelaySemanticMode {
    Queue,
    Steer,
    StopAndSend,
}

impl RelaySemanticMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queue => "queue",
            Self::Steer => "steer",
            Self::StopAndSend => "stopAndSend",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RelaySemanticOutcome {
    Injected,
    Cancelled,
    DeliveryUnknown,
}

impl RelaySemanticOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Injected => "injected",
            Self::Cancelled => "cancelled",
            Self::DeliveryUnknown => "deliveryUnknown",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantSemanticReceiptMessage {
    pub session_id: String,
    pub incarnation_id: uuid::Uuid,
    pub request_id: uuid::Uuid,
    pub mode: RelaySemanticMode,
    pub payload_sha256: String,
    pub outcome: RelaySemanticOutcome,
    pub requester_user_id: String,
    pub requester_device_id: String,
    pub owner_user_id: String,
    pub owner_device_id: String,
    pub signature: String,
}

impl ParticipantSemanticReceiptMessage {
    pub fn validate_target(
        &self,
        expected_session_id: &str,
        expected_incarnation_id: &uuid::Uuid,
        expected_requester_user_id: &str,
        expected_requester_device_id: &str,
    ) -> Result<()> {
        if self.session_id != expected_session_id
            || self.incarnation_id != *expected_incarnation_id
            || self.request_id.get_version() != Some(uuid::Version::SortRand)
            || self.requester_user_id != expected_requester_user_id
            || self.requester_device_id != expected_requester_device_id
            || self.payload_sha256.len() != 64
            || !self
                .payload_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || self.owner_user_id.trim().is_empty()
            || self.owner_device_id.trim().is_empty()
            || self.signature.trim().is_empty()
        {
            return Err(BackendClientError::InvalidBackendData {
                field: "participant.semanticReceipt".to_owned(),
                reason: "receipt target, UUIDv7, fingerprint, owner, or signature is invalid"
                    .to_owned(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatusMessage {
    pub session_id: String,
    pub status: BackendSessionStatus,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResultMessage {
    pub session_id: String,
    pub action_id: String,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub request_generation: Option<u64>,
    status: ActionResultStatus,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ActionResultStatus {
    Accepted,
    Duplicate,
    Busy,
    Rejected,
}

impl ActionResultMessage {
    pub const fn remote_status(&self) -> RemoteActionStatus {
        match self.status {
            ActionResultStatus::Accepted => RemoteActionStatus::Accepted,
            ActionResultStatus::Duplicate => RemoteActionStatus::Duplicate,
            ActionResultStatus::Busy => RemoteActionStatus::Busy,
            ActionResultStatus::Rejected => RemoteActionStatus::Rejected,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEndedMessage {
    pub session_id: String,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionAccessRevokedMessage {
    pub session_id: String,
}
