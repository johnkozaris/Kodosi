pub mod degradation;
pub mod diagnostics;
pub mod error;
pub mod hitl;
pub mod identity;
pub mod ids;
pub(crate) mod provider_state;
pub mod settings;
pub mod snapshot;
pub mod status;
pub mod subagents;

pub use degradation::{CopilotSessionStoreFailureKind, DegradationNotice};
pub use diagnostics::{
    ActiveCustomizationsReport, CustomizationEntry, CustomizationError, CustomizationKind,
    CustomizationStatus, CustomizationVendor, Scope,
};
pub use error::{AgentError, AgentErrorKind};
pub use hitl::{HitlPrompt, HitlPromptType};
pub use identity::{AgentIdentity, ParserCompatNotice};
pub use ids::SessionId;
#[doc(hidden)]
pub use provider_state::AgentProviderState;
pub use settings::{SettingsFilePath, SettingsScope};
pub use snapshot::{
    AgentAttention, AgentAttentionKind, AgentExceptionalKind, AgentExceptionalState,
    AgentIntelSnapshot, AgentLifecycle, AgentOutcome, AgentOutcomeKind, AgentSource,
    AgentSourceKind, ChildAgentSummary, CurrentAgentActivity, LiveAgentIdentity,
    PendingAgentInteraction, PendingInteractionKind,
};
pub use status::{AgentStatus, HealthStatus};
pub use subagents::{SubAgentInfo, SubAgentStatus, SubAgentTree};
