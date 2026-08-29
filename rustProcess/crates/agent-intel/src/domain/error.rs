use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentError {
    pub kind: AgentErrorKind,
    pub message: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AgentErrorKind {
    RateLimit,
    AuthFailure,
    QuotaExceeded,
    ContextOverflow,
    Other,
}
