use std::path::Path;

use serde::{Deserialize, Serialize};

use super::extensions::{AgentDefSummary, McpServerSummary, PluginSummary, SkillSummary};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CopilotGlobalStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copilot_cli_version: Option<String>,
    pub installed_plugins: Vec<PluginSummary>,
    pub loaded_skills: Vec<SkillSummary>,
    pub loaded_agents: Vec<AgentDefSummary>,
    pub mcp_servers: Vec<McpServerSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notices: Option<Vec<crate::domain::DegradationNotice>>,
}

impl CopilotGlobalStatus {
    #[must_use]
    pub fn read(cwd: Option<&str>) -> Self {
        let home = super::filesystem::copilot_home();
        let cwd_path = cwd.map(Path::new);
        Self {
            cwd: cwd.map(str::to_owned),
            copilot_cli_version: None,
            installed_plugins: super::filesystem::read_installed_plugins(&home),
            loaded_skills: super::filesystem::scan_skills_with_cwd(&home, cwd_path),
            loaded_agents: super::filesystem::scan_custom_agents(&home, cwd_path),
            mcp_servers: super::filesystem::read_mcp_servers(&home, cwd_path),
            notices: non_empty_notices(crate::ops::diagnostics::catalog_notices(
                &crate::runtime::paths::home_dir(),
                cwd_path,
                crate::domain::CustomizationVendor::Copilot,
            )),
        }
    }

    #[must_use]
    pub fn with_version(mut self, version: Option<String>) -> Self {
        self.copilot_cli_version = version;
        self
    }
}

fn non_empty_notices(
    notices: Vec<crate::domain::DegradationNotice>,
) -> Option<Vec<crate::domain::DegradationNotice>> {
    (!notices.is_empty()).then_some(notices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_status_has_no_install_metadata() {
        let status = CopilotGlobalStatus {
            cwd: None,
            copilot_cli_version: None,
            installed_plugins: Vec::new(),
            loaded_skills: Vec::new(),
            loaded_agents: Vec::new(),
            mcp_servers: Vec::new(),
            notices: None,
        };
        let json = serde_json::to_value(&status).unwrap();
        assert!(json.get("installedPlugins").is_some());
        assert!(json.get("loadedSkills").is_some());
        assert!(json.get("loadedAgents").is_some());
        assert!(json.get("mcpServers").is_some());
    }

    #[test]
    fn with_version_threads_version_through() {
        let status = CopilotGlobalStatus {
            cwd: None,
            copilot_cli_version: None,
            installed_plugins: Vec::new(),
            loaded_skills: Vec::new(),
            loaded_agents: Vec::new(),
            mcp_servers: Vec::new(),
            notices: None,
        }
        .with_version(Some("1.2.3".into()));
        assert_eq!(status.copilot_cli_version.as_deref(), Some("1.2.3"));
    }
}
