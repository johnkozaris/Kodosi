use super::permission_decision_registry::PermissionDecisionRegistry;
use super::registry::AgentIntelRegistry;

#[derive(Debug, Default)]
pub(crate) struct AgentIntelState {
    pub(crate) registry: AgentIntelRegistry,
    pub(crate) permission_decisions: PermissionDecisionRegistry,
    pub(crate) permission_timeout: std::time::Duration,
}

impl AgentIntelState {
    pub(crate) fn with_permission_timeout(timeout: std::time::Duration) -> Self {
        Self {
            permission_timeout: timeout,
            ..Self::default()
        }
    }
}
