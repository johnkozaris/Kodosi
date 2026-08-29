use std::result::Result;

use serde::{Deserialize, Serialize, Serializer};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::labels::{self, BackendToolKind};
use kodosi_domain::{
    permissions::{AccessLevel, DefaultAudienceAccess, ShareScope},
    session::SessionState,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCardDto {
    pub id: String,
    pub title: String,
    #[serde(
        deserialize_with = "labels::deserialize_share_scope",
        serialize_with = "labels::serialize_share_scope"
    )]
    pub scope: ShareScope,
    #[serde(
        deserialize_with = "labels::deserialize_access_level",
        serialize_with = "labels::serialize_access_level"
    )]
    pub access: AccessLevel,
    #[serde(
        deserialize_with = "labels::deserialize_session_state",
        serialize_with = "labels::serialize_session_state"
    )]
    pub status: SessionState,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PaginatedSessionFeedDto {
    pub items: Vec<SessionCardDto>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetailDto {
    pub id: String,
    pub incarnation_id: Uuid,
    pub incarnation_generation: u64,
    pub incarnation_protocol_version: u32,
    pub owner_user_id: String,
    pub title: String,
    #[serde(
        deserialize_with = "labels::deserialize_tool_kind",
        serialize_with = "labels::serialize_tool_kind"
    )]
    pub tool_kind: BackendToolKind,
    #[serde(
        deserialize_with = "labels::deserialize_share_scope",
        serialize_with = "labels::serialize_share_scope"
    )]
    pub scope: ShareScope,
    pub room_id: Option<String>,
    #[serde(
        deserialize_with = "labels::deserialize_access_level",
        serialize_with = "labels::serialize_access_level"
    )]
    pub default_access: AccessLevel,
    #[serde(
        default,
        deserialize_with = "labels::deserialize_optional_access_level",
        serialize_with = "labels::serialize_optional_access_level"
    )]
    pub effective_access: Option<AccessLevel>,
    #[serde(
        deserialize_with = "labels::deserialize_session_state",
        serialize_with = "labels::serialize_session_state"
    )]
    pub status: SessionState,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub last_heartbeat_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionCreationReceiptDto {
    pub session_id: String,
    pub create_idempotency_key: Uuid,
    pub incarnation_id: Uuid,
    pub generation: u64,
    pub protocol_version: u32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    pub id: String,
    pub idempotency_key: Uuid,
    pub title: String,
    #[serde(serialize_with = "labels::serialize_share_scope")]
    pub scope: ShareScope,
    #[serde(serialize_with = "labels::serialize_tool_kind")]
    pub tool_kind: BackendToolKind,

    #[serde(serialize_with = "labels::serialize_default_audience_access")]
    pub default_access: DefaultAudienceAccess,
    #[serde(serialize_with = "serialize_zeroizing_string")]
    pub owner_secret: Zeroizing<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_id: Option<String>,
}

fn serialize_zeroizing_string<S>(
    value: &Zeroizing<String>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(value.as_str())
}

impl std::fmt::Debug for CreateSessionRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CreateSessionRequest")
            .field("id", &self.id)
            .field("idempotency_key", &self.idempotency_key)
            .field("title", &self.title)
            .field("scope", &self.scope)
            .field("tool_kind", &self.tool_kind)
            .field("default_access", &self.default_access)
            .field("owner_secret", &"<redacted>")
            .field("room_id", &self.room_id)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionRequest {
    pub expected_incarnation_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(serialize_with = "labels::serialize_optional_share_scope")]
    pub scope: Option<ShareScope>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "labels::serialize_optional_default_audience_access"
    )]
    pub default_access: Option<DefaultAudienceAccess>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantAccessRequest {
    pub expected_incarnation_id: Uuid,
    pub mutation_id: Uuid,
    pub actor_user_id: String,
    #[serde(serialize_with = "labels::serialize_access_level")]
    pub access_level: AccessLevel,
    pub expires_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionAccessMutationKindDto {
    Grant,
    Revoke,
    Leave,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionAccessMutationReceiptDto {
    pub mutation_id: Uuid,
    pub session_id: Uuid,
    pub incarnation_id: Uuid,
    pub kind: SessionAccessMutationKindDto,
    pub target_user_id: Option<Uuid>,
    #[serde(
        default,
        deserialize_with = "labels::deserialize_optional_access_level"
    )]
    pub access_level: Option<AccessLevel>,
    pub requested_expires_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessGrantDto {
    pub actor_user_id: String,
    pub handle: String,
    pub display_name: String,
    #[serde(deserialize_with = "labels::deserialize_access_level")]
    pub access_level: AccessLevel,
    pub granted_at: String,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccessGrantsDto {
    pub incarnation_id: Uuid,
    pub grants: Vec<AccessGrantDto>,
}

#[cfg(test)]
mod tests {
    use super::{
        CreateSessionRequest, GrantAccessRequest, PaginatedSessionFeedDto, SessionDetailDto,
    };
    use crate::labels::BackendToolKind;
    use kodosi_domain::permissions::{AccessLevel, DefaultAudienceAccess, ShareScope};

    #[test]
    fn create_request_serializes_but_never_debugs_owner_secret() {
        let request = CreateSessionRequest {
            id: "session".to_owned(),
            idempotency_key: uuid::Uuid::now_v7(),
            title: "title".to_owned(),
            scope: ShareScope::MyDevices,
            tool_kind: BackendToolKind::Generic,
            default_access: DefaultAudienceAccess::View,
            owner_secret: zeroize::Zeroizing::new("owner-secret".to_owned()),
            room_id: None,
        };

        let json = serde_json::to_string(&request).expect("serialize request");
        assert!(json.contains("owner-secret"));
        let debug = format!("{request:?}");
        assert!(!debug.contains("owner-secret"));
        assert!(debug.contains("<redacted>"));
    }

    #[test]
    fn paginated_room_feed_envelope_matches_backend_shape() {
        let page: PaginatedSessionFeedDto = serde_json::from_str(
            r#"{
                "items": [{
                    "id": "01900000-0000-7000-8000-000000000001",
                    "title": "Agent A",
                    "scope": "Room",
                    "access": "Suggest",
                    "status": "Live"
                }],
                "nextCursor": "cursor-2",
                "hasMore": true
            }"#,
        )
        .expect("backend room-feed envelope should decode");

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.next_cursor.as_deref(), Some("cursor-2"));
        assert!(page.has_more);
    }

    #[test]
    fn session_detail_decodes_persistent_incarnation_uuid() {
        let detail: SessionDetailDto = serde_json::from_str(
            r#"{
                "id":"01900000-0000-7000-8000-000000000001",
                "incarnationId":"01900000-0000-7000-8000-000000000002",
                "incarnationGeneration":1,
                "incarnationProtocolVersion":2,
                "ownerUserId":"01900000-0000-7000-8000-000000000003",
                "title":"Session",
                "toolKind":"Generic",
                "scope":"MyDevices",
                "roomId":null,
                "defaultAccess":"View",
                "effectiveAccess":"Inject",
                "status":"Pending",
                "startedAt":"2026-08-06T01:02:03Z",
                "endedAt":null,
                "lastHeartbeatAt":"2026-08-06T01:02:03Z"
            }"#,
        )
        .expect("session detail should decode");

        assert_eq!(
            detail.incarnation_id,
            uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000002")
                .expect("incarnation UUID")
        );
    }

    #[test]
    fn grant_access_serializes_expected_incarnation_fence() {
        let request = GrantAccessRequest {
            expected_incarnation_id: uuid::Uuid::from_u128(1),
            mutation_id: uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000004")
                .expect("mutation UUID"),
            actor_user_id: "user-2".to_owned(),
            access_level: AccessLevel::View,
            expires_at: "2026-08-14T00:00:00.000Z".to_owned(),
        };

        let json = serde_json::to_value(request).expect("grant request should serialize");
        assert_eq!(
            json["expectedIncarnationId"],
            "00000000-0000-0000-0000-000000000001"
        );
        assert_eq!(json["mutationId"], "01900000-0000-7000-8000-000000000004");
        assert_eq!(json["expiresAt"], "2026-08-14T00:00:00.000Z");
    }
}
