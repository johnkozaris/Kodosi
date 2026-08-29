use aws_lc_rs::digest;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AppError, Result,
    host_protocol::{SessionAccessMutationKind, SessionAccessMutationOutcome},
};
use kodosi_domain::{ids::SessionId, permissions::AccessLevel};

const FINGERPRINT_DOMAIN: &[u8] = b"kodosi:session-access-mutation-target:v1:";
pub(crate) const MAX_TERMINAL_MESSAGE_BYTES: usize = 16 * 1_024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum SessionAccessMutationTarget {
    Grant {
        #[serde(rename = "actorUserId")]
        actor_user_id: Uuid,
        #[serde(rename = "accessLevel")]
        access_level: AccessLevel,
        #[serde(rename = "expiresAtUnixMs")]
        expires_at_unix_ms: i64,
    },
    Revoke {
        #[serde(rename = "actorUserId")]
        actor_user_id: Uuid,
    },
    Leave,
}

impl SessionAccessMutationTarget {
    pub(crate) const fn kind(&self) -> SessionAccessMutationKind {
        match self {
            Self::Grant { .. } => SessionAccessMutationKind::Grant,
            Self::Revoke { .. } => SessionAccessMutationKind::Revoke,
            Self::Leave => SessionAccessMutationKind::Leave,
        }
    }

    pub(crate) fn actor_user_id(&self) -> Option<String> {
        match self {
            Self::Grant { actor_user_id, .. } | Self::Revoke { actor_user_id } => {
                Some(actor_user_id.to_string())
            }
            Self::Leave => None,
        }
    }

    pub(crate) const fn access_level(&self) -> Option<AccessLevel> {
        match self {
            Self::Grant { access_level, .. } => Some(*access_level),
            Self::Revoke { .. } | Self::Leave => None,
        }
    }

    pub(crate) fn expires_at(&self) -> Option<String> {
        let Self::Grant {
            expires_at_unix_ms, ..
        } = self
        else {
            return None;
        };
        time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(*expires_at_unix_ms) * 1_000_000)
            .ok()
            .and_then(|value| {
                value
                    .format(&time::format_description::well_known::Rfc3339)
                    .ok()
            })
    }

    pub(crate) const fn is_leave(&self) -> bool {
        matches!(self, Self::Leave)
    }

    pub(crate) fn fingerprint(
        &self,
        backend_session_id: &str,
        backend_incarnation_id: Uuid,
    ) -> Result<String> {
        let session_id =
            Uuid::parse_str(backend_session_id).map_err(|_| AppError::InvalidBackendData {
                field: "sessionAccessMutation.backendSessionId".to_owned(),
                reason: "must be a UUID".to_owned(),
            })?;
        let mut bytes = FINGERPRINT_DOMAIN.to_vec();
        push_field(&mut bytes, session_id.as_bytes())?;
        push_field(&mut bytes, backend_incarnation_id.as_bytes())?;
        match self {
            Self::Grant {
                actor_user_id,
                access_level,
                expires_at_unix_ms,
            } => {
                push_field(&mut bytes, b"grant")?;
                push_optional(&mut bytes, Some(actor_user_id.as_bytes()))?;
                push_optional(&mut bytes, Some(access_level_wire(*access_level)))?;
                push_optional(&mut bytes, Some(&expires_at_unix_ms.to_be_bytes()))?;
            }
            Self::Revoke { actor_user_id } => {
                push_field(&mut bytes, b"revoke")?;
                push_optional(&mut bytes, Some(actor_user_id.as_bytes()))?;
                push_optional(&mut bytes, None)?;
                push_optional(&mut bytes, None)?;
            }
            Self::Leave => {
                push_field(&mut bytes, b"leave")?;
                push_optional(&mut bytes, None)?;
                push_optional(&mut bytes, None)?;
                push_optional(&mut bytes, None)?;
            }
        }
        Ok(hex(digest::digest(&digest::SHA256, &bytes).as_ref()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SessionAccessMutationTerminalStatus {
    Applied,
    Rejected,
}

impl SessionAccessMutationTerminalStatus {
    pub(crate) const fn outcome(self) -> SessionAccessMutationOutcome {
        match self {
            Self::Applied => SessionAccessMutationOutcome::Applied,
            Self::Rejected => SessionAccessMutationOutcome::Rejected,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SessionAccessMutationTerminal {
    pub(crate) status: SessionAccessMutationTerminalStatus,
    pub(crate) message: Option<String>,
}

impl SessionAccessMutationTerminal {
    pub(crate) fn new(
        status: SessionAccessMutationTerminalStatus,
        message: Option<String>,
    ) -> Self {
        Self {
            status,
            message: message.map(bounded_terminal_message),
        }
    }
}

pub(crate) fn bounded_terminal_message(mut message: String) -> String {
    if message.len() <= MAX_TERMINAL_MESSAGE_BYTES {
        return message;
    }
    let mut boundary = MAX_TERMINAL_MESSAGE_BYTES;
    while !message.is_char_boundary(boundary) {
        boundary -= 1;
    }
    message.truncate(boundary);
    message
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PreparedSessionAccessMutationState {
    Prepared,
    Attempting,
    OutcomeUnknown,
    Retiring,
    ReceiptConfirmed,
    EffectPending,
    RelayPending { key_generation: u32 },
    Terminal(SessionAccessMutationTerminal),
}

impl PreparedSessionAccessMutationState {
    pub(crate) const fn holds_local_authority(&self) -> bool {
        !matches!(self, Self::Retiring | Self::Terminal(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PreparedSessionAccessMutation {
    pub(crate) mutation_id: Uuid,
    pub(crate) account_user_id: String,
    pub(crate) originating_account_epoch: u64,
    pub(crate) runtime_session_id: SessionId,
    pub(crate) expected_runtime_incarnation_id: Uuid,
    pub(crate) backend_session_id: String,
    pub(crate) backend_incarnation_id: Uuid,
    pub(crate) target: SessionAccessMutationTarget,
    pub(crate) fingerprint: String,
    pub(crate) state: PreparedSessionAccessMutationState,
}

impl PreparedSessionAccessMutation {
    pub(crate) fn new(
        mutation_id: Uuid,
        account_user_id: String,
        originating_account_epoch: u64,
        runtime_session_id: SessionId,
        expected_runtime_incarnation_id: Uuid,
        backend_session_id: String,
        backend_incarnation_id: Uuid,
        target: SessionAccessMutationTarget,
    ) -> Result<Self> {
        let fingerprint = target.fingerprint(&backend_session_id, backend_incarnation_id)?;
        let value = Self {
            mutation_id,
            account_user_id,
            originating_account_epoch,
            runtime_session_id,
            expected_runtime_incarnation_id,
            backend_session_id,
            backend_incarnation_id,
            target,
            fingerprint,
            state: PreparedSessionAccessMutationState::Prepared,
        };
        value.validate()?;
        Ok(value)
    }

    pub(crate) fn validate(&self) -> Result<()> {
        let canonical_backend = Uuid::parse_str(&self.backend_session_id)
            .is_ok_and(|value| value.to_string() == self.backend_session_id.to_lowercase());
        if self.mutation_id.get_version_num() != 7
            || self.mutation_id.to_string() != self.mutation_id.hyphenated().to_string()
            || self.account_user_id.trim().is_empty()
            || self.account_user_id.len() > 1_024
            || self.originating_account_epoch == 0
            || self.expected_runtime_incarnation_id.is_nil()
            || self.backend_incarnation_id.is_nil()
            || !canonical_backend
        {
            return Err(AppError::InvalidBackendData {
                field: "pendingSessionAccessMutations.identity".to_owned(),
                reason: "mutation identity or incarnation fence is invalid".to_owned(),
            });
        }
        if self.target.is_leave()
            && (self.backend_session_id != self.runtime_session_id.to_string()
                || self.backend_incarnation_id != self.expected_runtime_incarnation_id)
        {
            return Err(AppError::InvalidBackendData {
                field: "pendingSessionAccessMutations.leaveIdentity".to_owned(),
                reason: "leave must bind one remote session and incarnation identity".to_owned(),
            });
        }
        let expected = self
            .target
            .fingerprint(&self.backend_session_id, self.backend_incarnation_id)?;
        if self.fingerprint != expected || !is_sha256_hex(&self.fingerprint) {
            return Err(AppError::InvalidBackendData {
                field: "pendingSessionAccessMutations.fingerprint".to_owned(),
                reason: "stored fingerprint does not match the exact target".to_owned(),
            });
        }
        if let PreparedSessionAccessMutationState::RelayPending { key_generation } = self.state
            && key_generation == 0
        {
            return Err(AppError::InvalidBackendData {
                field: "pendingSessionAccessMutations.relayPending".to_owned(),
                reason: "relay-pending key generation must be positive".to_owned(),
            });
        }
        if let PreparedSessionAccessMutationState::Terminal(terminal) = &self.state
            && terminal
                .message
                .as_ref()
                .is_some_and(|message| message.len() > MAX_TERMINAL_MESSAGE_BYTES)
        {
            return Err(AppError::InvalidBackendData {
                field: "pendingSessionAccessMutations.terminal".to_owned(),
                reason: "terminal message must be bounded".to_owned(),
            });
        }
        Ok(())
    }
}

fn access_level_wire(level: AccessLevel) -> &'static [u8] {
    match level {
        AccessLevel::View => b"view",
        AccessLevel::Suggest => b"suggest",
        AccessLevel::Inject => b"inject",
        AccessLevel::Approve => b"approve",
    }
}

fn push_field(bytes: &mut Vec<u8>, value: &[u8]) -> Result<()> {
    let length = i32::try_from(value.len()).map_err(|_| AppError::Unsupported {
        reason: "session access mutation fingerprint field is too large".to_owned(),
    })?;
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(value);
    Ok(())
}

fn push_optional(bytes: &mut Vec<u8>, value: Option<&[u8]>) -> Result<()> {
    bytes.push(u8::from(value.is_some()));
    if let Some(value) = value {
        push_field(bytes, value)?;
    }
    Ok(())
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(
        String::with_capacity(bytes.len() * 2),
        |mut output, byte| {
            let _ = write!(output, "{byte:02x}");
            output
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(account: &str) -> PreparedSessionAccessMutation {
        PreparedSessionAccessMutation::new(
            Uuid::parse_str("01900000-0000-7000-8000-000000000001").unwrap(),
            account.to_owned(),
            3,
            SessionId::parse_field("01900000-0000-7000-8000-000000000002", "sessionId").unwrap(),
            Uuid::parse_str("01900000-0000-7000-8000-000000000003").unwrap(),
            "01900000-0000-7000-8000-000000000004".to_owned(),
            Uuid::parse_str("01900000-0000-7000-8000-000000000005").unwrap(),
            SessionAccessMutationTarget::Revoke {
                actor_user_id: Uuid::parse_str("01900000-0000-7000-8000-000000000006").unwrap(),
            },
        )
        .unwrap()
    }

    #[test]
    fn exact_target_fingerprint_is_stable_and_tamper_evident() {
        let first = record("account");
        let second = record("account");
        assert_eq!(first.fingerprint, second.fingerprint);
        let mut rebound = first.clone();
        rebound.backend_incarnation_id =
            Uuid::parse_str("01900000-0000-7000-8000-000000000007").unwrap();
        assert!(rebound.validate().is_err());
    }

    #[test]
    fn terminal_messages_are_bounded_without_splitting_utf8() {
        let message = format!("{}é", "x".repeat(MAX_TERMINAL_MESSAGE_BYTES - 1));
        let terminal = SessionAccessMutationTerminal::new(
            SessionAccessMutationTerminalStatus::Rejected,
            Some(message),
        );
        let bounded = terminal.message.unwrap();
        assert_eq!(bounded.len(), MAX_TERMINAL_MESSAGE_BYTES - 1);
        assert!(bounded.is_char_boundary(bounded.len()));
        assert!(bounded.bytes().all(|byte| byte == b'x'));
    }

    #[test]
    fn terminal_messages_preserve_maximum_multibyte_utf8() {
        let message = "🦀".repeat(MAX_TERMINAL_MESSAGE_BYTES / "🦀".len());
        let terminal = SessionAccessMutationTerminal::new(
            SessionAccessMutationTerminalStatus::Rejected,
            Some(message.clone()),
        );
        assert_eq!(terminal.message.as_deref(), Some(message.as_str()));
        assert_eq!(message.len(), MAX_TERMINAL_MESSAGE_BYTES);
    }

    #[test]
    fn provenance_requires_uuid_v7_and_positive_epoch() {
        let mut invalid = record("account");
        invalid.mutation_id = Uuid::from_u128(1);
        assert!(invalid.validate().is_err());
        let mut invalid = record("account");
        invalid.originating_account_epoch = 0;
        assert!(invalid.validate().is_err());
    }
}
