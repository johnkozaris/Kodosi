use serde::Serialize;

#[derive(Serialize, specta::Type)]
pub struct AgentSettingsBundle {
    pub managed: Option<serde_json::Value>,
    pub user: Option<serde_json::Value>,
    pub project: Option<serde_json::Value>,
    pub local: Option<serde_json::Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRef {
    pub id: String,
    pub size_bytes: u64,
    pub modified_at: Option<String>,
    pub started_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRef {
    pub filename: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub kind: Option<String>,
    pub size_bytes: u64,
    pub modified_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeProjectRef {
    pub slug: String,
    pub label: String,
    pub memory_count: u32,
    pub session_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConversationRef {
    pub provider: String,
    pub native_conversation_id: String,
    pub working_directory: String,
    pub title: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConversationPage {
    pub items: Vec<ProviderConversationRef>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub response_bytes: usize,
}

pub use crate::copilot::session_store_db::{RepoSessionRow, RepositoryRow};

pub use crate::claude::global::{
    AgentDefSummary, ClaudeGlobalStatus, McpServerSummary, PluginSummary, SkillSummary,
};

pub use crate::copilot::global::CopilotGlobalStatus;
