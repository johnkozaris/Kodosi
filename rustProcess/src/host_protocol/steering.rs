use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SemanticSendMode {
    Queue,
    #[default]
    Steer,
    StopAndSend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SteerDeliveryState {
    Preparing,
    Queued,
    DeliveryUnknown,
    Injected,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SteerQueueEntry {
    pub steer_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub account_user_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub request_id: String,
    pub session_incarnation_id: String,
    #[serde(default)]
    pub mode: SemanticSendMode,
    pub session_id: String,
    pub text: String,
    pub queued_at_ms: u64,
    pub delivery_state: SteerDeliveryState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_tool_use_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SteerTransition {
    Queued,
    Sending,
    Injected,
    Failed,
    Cancelled,
    DeliveryUnknown,
}
