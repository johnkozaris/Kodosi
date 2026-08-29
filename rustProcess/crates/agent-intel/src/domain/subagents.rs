#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum SubAgentStatus {
    Running,
    Completed,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SubAgentInfo {
    pub status: SubAgentStatus,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct SubAgentTree {
    pub agents: Vec<SubAgentInfo>,
}
