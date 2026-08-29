use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum AuthCommand {
    #[serde(rename = "auth.login.start")]
    LoginStart,
    #[serde(rename = "auth.logout")]
    Logout,
    #[serde(rename = "auth.identity.reset")]
    IdentityReset,
    #[serde(rename = "auth.refresh")]
    Refresh,
}

impl AuthCommand {
    pub fn operation(&self) -> &'static str {
        match self {
            Self::LoginStart => "login.start",
            Self::Logout => "logout",
            Self::IdentityReset => "identity.reset",
            Self::Refresh => "refresh",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AuthEvent {
    #[serde(rename = "auth.ready")]
    Ready {
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(rename = "userId")]
        user_id: Option<String>,
        #[serde(rename = "accountEpoch")]
        account_epoch: u64,
    },
    #[serde(rename = "auth.required")]
    Required {
        reason: AuthRequiredReason,
        #[serde(rename = "accountEpoch")]
        account_epoch: u64,
    },
    #[serde(rename = "auth.device_code")]
    DeviceCode {
        #[serde(rename = "userCode")]
        user_code: String,
        #[serde(rename = "verificationUri")]
        verification_uri: String,
    },
    #[serde(rename = "auth.finalizing")]
    Finalizing,
    #[serde(rename = "auth.notice")]
    Notice {
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    #[serde(rename = "auth.identity_health")]
    IdentityHealth {
        state: IdentityHealthState,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    #[serde(rename = "auth.error")]
    Error { operation: String, message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum IdentityHealthState {
    Healthy,
    RecoveryRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AuthRequiredReason {
    SignedOut,
    Expired,
}

#[cfg(test)]
mod tests {
    use super::{AuthCommand, AuthEvent, AuthRequiredReason};

    #[test]
    fn serializes_auth_required_reason() {
        let payload = serde_json::to_value(AuthEvent::Required {
            reason: AuthRequiredReason::Expired,
            account_epoch: 7,
        })
        .unwrap_or_else(|error| panic!("auth.required should serialize: {error}"));

        assert_eq!(
            payload.get("type").and_then(serde_json::Value::as_str),
            Some("auth.required")
        );
        assert_eq!(
            payload.get("reason").and_then(serde_json::Value::as_str),
            Some("expired")
        );
    }

    #[test]
    fn serializes_auth_notice_without_message() {
        let payload = serde_json::to_value(AuthEvent::Notice { message: None })
            .unwrap_or_else(|error| panic!("auth.notice should serialize: {error}"));

        assert_eq!(
            payload.get("type").and_then(serde_json::Value::as_str),
            Some("auth.notice")
        );
        assert!(payload.get("message").is_none());
    }

    #[test]
    fn deserializes_auth_command_shape() {
        let payload = r#"{"type":"auth.identity.reset"}"#;
        let command: AuthCommand = serde_json::from_str(payload)
            .unwrap_or_else(|error| panic!("auth command should deserialize: {error}"));

        std::assert_matches!(command, AuthCommand::IdentityReset);
        assert_eq!(command.operation(), "identity.reset");
    }
}
