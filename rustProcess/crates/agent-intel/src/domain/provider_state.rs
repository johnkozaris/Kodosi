use crate::domain::{
    AgentError, AgentIdentity, AgentStatus, HealthStatus, HitlPrompt, SubAgentTree,
};

#[derive(Debug, Clone, Default)]
#[cfg_attr(test, derive(serde::Serialize))]
#[cfg_attr(test, serde(rename_all = "camelCase"))]
pub struct AgentProviderState {
    pub identity: AgentIdentity,
    pub status: AgentStatus,
    pub health: HealthStatus,
    pub error: Option<AgentError>,
    pub hitl_prompt: Option<HitlPrompt>,
    pub suggested_title: Option<String>,
    pub activity: Option<String>,
    pub sub_agents: Option<SubAgentTree>,
    pub cwd: Option<String>,
}
