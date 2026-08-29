use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DegradationNotice {
    UnknownClaudeSubtype {
        subtype: String,
    },
    MalformedSettings {
        vendor: String,
        path: String,
        message: String,
    },
    AgentParseFailed {
        vendor: String,
        path: String,
        message: String,
    },
    UnsupportedCopilotSessionStore {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        schema_version: Option<i64>,
        supported_versions: Vec<u32>,
    },
    CopilotSessionStoreFailure {
        failure: CopilotSessionStoreFailureKind,
        message: String,
    },
    MalformedCopilotEvent {
        message: String,
    },
    UncorrelatedClaudeTask {
        event: String,
    },
    TerminalFallback {
        fields: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CopilotSessionStoreFailureKind {
    Open,
    Locked,
    Corrupt,
    Schema,
    Query,
}
