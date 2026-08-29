use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::mcp::McpHealth;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeGlobalStatus {
    pub cwd: Option<String>,
    pub claude_code_version: Option<String>,
    pub installed_plugins: Vec<PluginSummary>,
    pub loaded_skills: Vec<SkillSummary>,
    pub loaded_agents: Vec<AgentDefSummary>,
    pub mcp_servers: Vec<McpServerSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notices: Option<Vec<crate::domain::DegradationNotice>>,
}

impl ClaudeGlobalStatus {
    #[must_use]
    pub fn read(cwd: Option<&str>) -> Self {
        let home = crate::runtime::paths::home_dir().join(".claude");
        let cwd_path = cwd.map(Path::new);
        Self {
            cwd: cwd.map(str::to_owned),
            claude_code_version: None,
            installed_plugins: super::filesystem::read_installed_plugins(&home),
            loaded_skills: super::filesystem::scan_skills_with_cwd(&home, cwd_path),
            loaded_agents: super::filesystem::scan_custom_agents(&home, cwd_path),
            mcp_servers: super::filesystem::read_mcp_servers(&home, cwd_path),
            notices: Self::non_empty_notices(crate::ops::diagnostics::catalog_notices(
                &crate::runtime::paths::home_dir(),
                cwd_path,
                crate::domain::CustomizationVendor::Claude,
            )),
        }
    }

    #[must_use]
    pub fn with_version(mut self, version: Option<String>) -> Self {
        self.claude_code_version = version;
        self
    }

    fn non_empty_notices(
        notices: Vec<crate::domain::DegradationNotice>,
    ) -> Option<Vec<crate::domain::DegradationNotice>> {
        (!notices.is_empty()).then_some(notices)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PluginSummary {
    pub id: String,
    pub marketplace: String,
    pub scope: String,
    pub version: Option<String>,
    pub installed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SkillSummary {
    pub name: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    pub description: Option<String>,
    pub user_invocable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentDefSummary {
    pub name: String,
    pub source: String,
    pub description: Option<String>,
    #[serde(default)]
    pub disallowed_tools: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct McpServerSummary {
    pub name: String,
    pub scope: String,
    pub transport: Option<String>,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<McpHealth>,
}
