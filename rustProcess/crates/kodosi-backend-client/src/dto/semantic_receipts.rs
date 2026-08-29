use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SemanticReceiptDto {
    pub session_id: uuid::Uuid,
    pub incarnation_id: uuid::Uuid,
    pub request_id: uuid::Uuid,
    pub mode: String,
    pub payload_sha256: String,
    pub outcome: String,
    pub requester_user_id: uuid::Uuid,
    pub requester_device_id: String,
    pub owner_user_id: uuid::Uuid,
    pub owner_device_id: String,
    pub signature: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticReceiptPageDto {
    pub items: Vec<SemanticReceiptDto>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticReceiptAckRequest {
    pub session_id: uuid::Uuid,
    pub incarnation_id: uuid::Uuid,
    pub request_id: uuid::Uuid,
    pub requester_user_id: String,
    pub requester_device_id: String,
    pub signature: String,
}
