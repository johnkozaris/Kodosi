use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AgentStatus {
    Running,
    #[default]
    Idle,
    WaitingForInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum HealthStatus {
    #[default]
    Unknown,
    Healthy,
    Stalled,
    Dead,
}
