pub(crate) mod claude_agents;
pub(crate) mod copilot_extension;
pub(crate) mod global_refresh;
pub(crate) mod lifecycle;
pub(crate) mod mcp_health_task;
pub(crate) mod mode;
pub(crate) mod permission_decision_registry;
mod registry;
pub(crate) mod risk;
pub(crate) mod state;
mod task;
pub(crate) mod telemetry;

pub(crate) use registry::{AgentIntelRegistry, TerminalIntelEnqueueOutcome};
pub(crate) use state::AgentIntelState;
