use serde::{Deserialize, Serialize};

use super::HostCommandValidationError;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum FriendsCommand {
    #[serde(rename = "friends.refresh")]
    Refresh,
    #[serde(rename = "friends.request.send")]
    RequestSend {
        username: String,
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "friends.request.accept")]
    RequestAccept { username: String },
    #[serde(rename = "friends.request.reject")]
    RequestReject { username: String },
    #[serde(rename = "friends.request.cancel")]
    RequestCancel { username: String },
    #[serde(rename = "friends.remove")]
    Remove { username: String },
}

impl FriendsCommand {
    pub fn operation(&self) -> &'static str {
        match self {
            Self::Refresh => "refresh",
            Self::RequestSend { .. } => "request.send",
            Self::RequestAccept { .. } => "request.accept",
            Self::RequestReject { .. } => "request.reject",
            Self::RequestCancel { .. } => "request.cancel",
            Self::Remove { .. } => "remove",
        }
    }

    pub(crate) fn validate(&self) -> Result<(), HostCommandValidationError> {
        match self {
            Self::Refresh => Ok(()),
            Self::RequestSend {
                username,
                request_id,
            } => {
                validate_username(username)?;
                validate_uuid_v7(request_id, "requestId")
            }
            Self::RequestAccept { username }
            | Self::RequestReject { username }
            | Self::RequestCancel { username }
            | Self::Remove { username } => validate_username(username),
        }
    }
}

fn validate_username(value: &str) -> Result<(), HostCommandValidationError> {
    if value.trim().is_empty() {
        return Err(HostCommandValidationError::EmptyField("username"));
    }
    Ok(())
}

fn validate_uuid_v7(value: &str, field: &'static str) -> Result<(), HostCommandValidationError> {
    let id =
        uuid::Uuid::parse_str(value).map_err(|_| HostCommandValidationError::InvalidField {
            field,
            reason: "must be a UUIDv7",
        })?;
    if id.is_nil() || id.get_version_num() != 7 {
        return Err(HostCommandValidationError::InvalidField {
            field,
            reason: "must be a UUIDv7",
        });
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FriendsEvent {
    #[serde(rename = "friends.snapshot")]
    Snapshot {
        friends: Vec<FriendEntry>,
        incoming: Vec<FriendRequestEntry>,
        outgoing: Vec<FriendRequestEntry>,
        #[serde(rename = "requestId", skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },
    #[serde(rename = "friends.error")]
    Error {
        operation: String,
        message: String,
        #[serde(rename = "requestId", skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FriendEntry {
    pub user_id: String,
    pub handle: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FriendRequestEntry {
    pub user_id: String,
    pub handle: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use crate::host_protocol::HostCommandValidationError;

    use super::{FriendEntry, FriendRequestEntry, FriendsCommand, FriendsEvent};

    #[test]
    fn serializes_friends_snapshot_shape() {
        let payload = serde_json::to_value(FriendsEvent::Snapshot {
            friends: vec![FriendEntry {
                user_id: "user-1".to_owned(),
                handle: "alice".to_owned(),
                display_name: "Alice".to_owned(),
                avatar_url: None,
            }],
            incoming: vec![FriendRequestEntry {
                user_id: "user-2".to_owned(),
                handle: "bob".to_owned(),
                display_name: "Bob".to_owned(),
                avatar_url: Some("https://example.com/bob.png".to_owned()),
                created_at: "2026-04-24T10:00:00Z".to_owned(),
            }],
            outgoing: Vec::new(),
            request_id: Some("01900000-0000-7000-8000-000000000001".to_owned()),
        })
        .unwrap_or_else(|error| panic!("friends.snapshot should serialize: {error}"));

        assert_eq!(
            payload.get("type").and_then(serde_json::Value::as_str),
            Some("friends.snapshot")
        );
        assert!(
            payload
                .get("friends")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|friends| friends[0].get("avatarUrl").is_none())
        );
        assert_eq!(
            payload
                .get("incoming")
                .and_then(serde_json::Value::as_array)
                .and_then(|incoming| incoming[0].get("createdAt"))
                .and_then(serde_json::Value::as_str),
            Some("2026-04-24T10:00:00Z")
        );
        assert_eq!(
            payload.get("requestId").and_then(serde_json::Value::as_str),
            Some("01900000-0000-7000-8000-000000000001")
        );
    }

    #[test]
    fn serializes_friends_error_shape() {
        let payload = serde_json::to_value(FriendsEvent::Error {
            operation: "request.send".to_owned(),
            message: "user not found".to_owned(),
            request_id: Some("01900000-0000-7000-8000-000000000001".to_owned()),
        })
        .unwrap_or_else(|error| panic!("friends.error should serialize: {error}"));

        assert_eq!(
            payload.get("type").and_then(serde_json::Value::as_str),
            Some("friends.error")
        );
        assert_eq!(
            payload.get("operation").and_then(serde_json::Value::as_str),
            Some("request.send")
        );
    }

    #[test]
    fn deserializes_friends_command_shape() {
        let payload = r#"{"type":"friends.request.accept","username":"alice"}"#;
        let command: FriendsCommand = serde_json::from_str(payload)
            .unwrap_or_else(|error| panic!("friends command should deserialize: {error}"));

        std::assert_matches!(
            command,
            FriendsCommand::RequestAccept { ref username } if username == "alice"
        );
        assert_eq!(command.operation(), "request.accept");
        assert!(command.validate().is_ok());
    }

    #[test]
    fn rejects_empty_friend_command_handles() {
        assert_eq!(
            FriendsCommand::RequestSend {
                username: " ".to_owned(),
                request_id: "01900000-0000-7000-8000-000000000001".to_owned(),
            }
            .validate(),
            Err(HostCommandValidationError::EmptyField("username"))
        );
    }

    #[test]
    fn rejects_non_v7_friend_request_identity() {
        let error = FriendsCommand::RequestSend {
            username: "alice".to_owned(),
            request_id: "550e8400-e29b-41d4-a716-446655440000".to_owned(),
        }
        .validate();
        assert_eq!(
            error,
            Err(HostCommandValidationError::InvalidField {
                field: "requestId",
                reason: "must be a UUIDv7",
            })
        );
    }
}
