use super::live_authority::LiveAgentIntelAuthority;
use super::permission_decision_registry::PermissionDecisionRegistry;
use super::registry::AgentIntelRegistry;
use agent_intel::ops::bound_auto_mode::BoundAutoModeRegistry;
use agent_intel::ops::bound_custom_agents::BoundCustomAgentRegistry;
use agent_intel::ops::bound_external::BoundExternalRegistry;
use agent_intel::ops::bound_memory::BoundClaudeMemoryRegistry;
use agent_intel::ops::bound_projects::BoundProjectSourceRegistry;
use agent_intel::ops::open_handoff::NativeOpenHandoffRegistry;
use agent_intel::ops::project_mutations::ProjectMutationLedger;

#[derive(Debug, Default)]
pub(crate) struct AgentIntelState {
    pub(crate) registry: AgentIntelRegistry,
    pub(crate) bound_custom_agents: BoundCustomAgentRegistry,
    pub(crate) bound_auto_mode: BoundAutoModeRegistry,
    pub(crate) bound_external: BoundExternalRegistry,
    pub(crate) bound_memory: BoundClaudeMemoryRegistry,
    pub(crate) bound_projects: BoundProjectSourceRegistry,
    pub(crate) open_handoffs: NativeOpenHandoffRegistry,
    pub(crate) project_mutations: ProjectMutationLedger,
    pub(crate) permission_decisions: PermissionDecisionRegistry,
    pub(crate) live_authority: LiveAgentIntelAuthority,
    pub(crate) permission_timeout: std::time::Duration,
}

impl AgentIntelState {
    pub(crate) fn with_permission_timeout(timeout: std::time::Duration) -> Self {
        Self {
            permission_timeout: timeout,
            ..Self::default()
        }
    }

    pub(crate) fn purge_expired_now(&mut self) {
        self.bound_custom_agents.purge_expired_now();
        self.bound_auto_mode.purge_expired_now();
        self.bound_external.purge_expired_now();
        self.bound_memory.purge_expired_now();
        self.bound_projects.purge_expired_now();
        self.open_handoffs.purge_expired_now();
    }

    pub(crate) fn clear_bound_authority(&mut self) {
        self.bound_custom_agents.clear();
        self.bound_auto_mode.clear();
        self.bound_external.clear();
        self.bound_memory.clear();
        self.bound_projects.clear();
        self.open_handoffs.clear();
        self.project_mutations.clear();
    }
}
