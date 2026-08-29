use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::ids::UserId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthState {
    SignedOut,
    WaitingForApproval {
        user_code: String,
        verification_uri: String,
    },

    Finalizing {
        expires_at: OffsetDateTime,
    },
    Authenticated {
        subject: Option<UserId>,
        expires_at: OffsetDateTime,
    },

    LoggingOut {
        subject: Option<UserId>,
    },
    Expiring {
        subject: Option<UserId>,
    },
    Expired {
        subject: Option<UserId>,
    },
}

impl AuthState {
    pub fn is_authenticated(&self) -> bool {
        matches!(self, Self::Authenticated { .. })
    }

    pub fn subject_string(&self) -> Option<String> {
        match self {
            Self::Authenticated {
                subject: Some(uid), ..
            }
            | Self::LoggingOut { subject: Some(uid) }
            | Self::Expiring { subject: Some(uid) }
            | Self::Expired { subject: Some(uid) } => Some(uid.to_string()),
            _ => None,
        }
    }

    pub fn subject(&self) -> Option<UserId> {
        match self {
            Self::Authenticated { subject, .. }
            | Self::LoggingOut { subject }
            | Self::Expiring { subject }
            | Self::Expired { subject } => *subject,
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_state_exposes_captured_subject() {
        let uid = UserId::try_from("11111111-1111-1111-1111-111111111111")
            .expect("UUID string is a valid UserId");
        let state = AuthState::Expired { subject: Some(uid) };
        assert_eq!(state.subject_string(), Some(uid.to_string()));
    }
}
