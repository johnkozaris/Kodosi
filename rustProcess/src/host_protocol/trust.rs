use serde::{Deserialize, Serialize};

use super::HostCommandValidationError;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum TrustCommand {
    #[serde(rename = "trust.refresh")]
    Refresh {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "trust.reset")]
    Reset {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "userId")]
        user_id: String,
    },
}

impl TrustCommand {
    pub fn operation(&self) -> &'static str {
        match self {
            Self::Refresh { .. } => "refresh",
            Self::Reset { .. } => "reset",
        }
    }

    pub(crate) fn validate(&self) -> Result<(), HostCommandValidationError> {
        match self {
            Self::Refresh { request_id } => {
                if request_id.trim().is_empty() {
                    return Err(HostCommandValidationError::EmptyField("requestId"));
                }
                Ok(())
            }
            Self::Reset {
                request_id,
                user_id,
            } => {
                if request_id.trim().is_empty() {
                    return Err(HostCommandValidationError::EmptyField("requestId"));
                }
                if user_id.trim().is_empty() {
                    return Err(HostCommandValidationError::EmptyField("userId"));
                }
                Ok(())
            }
        }
    }

    pub(crate) fn request_id(&self) -> &str {
        match self {
            Self::Refresh { request_id } | Self::Reset { request_id, .. } => request_id,
        }
    }

    pub(crate) fn user_id(&self) -> Option<&str> {
        match self {
            Self::Refresh { .. } => None,
            Self::Reset { user_id, .. } => Some(user_id),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TrustEvent {
    #[serde(rename = "trust.snapshot")]
    Snapshot {
        #[serde(rename = "requestId")]
        request_id: String,
        pins: Vec<TrustPinEntry>,
    },
    #[serde(rename = "trust.reset")]
    Reset {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "userId")]
        user_id: String,
        cleared: bool,
    },
    #[serde(rename = "trust.error")]
    Error {
        #[serde(rename = "requestId", default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
        #[serde(rename = "userId", default, skip_serializing_if = "Option::is_none")]
        user_id: Option<String>,
        operation: String,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TrustPinEntry {
    pub user_id: String,
    pub generation: u64,
    pub signer_device_id: String,
    pub device_count: u32,
    pub pinned_at_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_validates() {
        assert!(
            TrustCommand::Refresh {
                request_id: "request-1".to_owned(),
            }
            .validate()
            .is_ok()
        );
        assert!(
            TrustCommand::Refresh {
                request_id: " ".to_owned(),
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn reset_rejects_empty_user_id() {
        assert!(
            TrustCommand::Reset {
                request_id: "request-1".to_owned(),
                user_id: " ".to_owned()
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn reset_event_requires_correlation_and_result() {
        assert!(
            serde_json::from_value::<TrustEvent>(serde_json::json!({
                "type": "trust.reset",
                "userId": "user-1",
            }))
            .is_err()
        );
    }

    #[test]
    fn snapshot_serializes_with_pinsfield() {
        let payload = serde_json::to_value(TrustEvent::Snapshot {
            request_id: "request-1".to_owned(),
            pins: vec![TrustPinEntry {
                user_id: "user-1".to_owned(),
                generation: 3,
                signer_device_id: "dev-A".to_owned(),
                device_count: 2,
                pinned_at_ms: 1_700_000_000_000,
            }],
        })
        .unwrap_or_else(|error| panic!("trust.snapshot should serialize: {error}"));

        assert_eq!(
            payload.get("type").and_then(serde_json::Value::as_str),
            Some("trust.snapshot")
        );
        assert_eq!(
            payload.get("requestId").and_then(serde_json::Value::as_str),
            Some("request-1")
        );
        assert!(
            payload
                .get("pins")
                .and_then(serde_json::Value::as_array)
                .is_some()
        );
    }
}
