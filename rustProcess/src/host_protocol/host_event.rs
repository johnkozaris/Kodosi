use ::agent_intel::domain::{
    AgentAttentionKind, AgentExceptionalKind, AgentIntelSnapshot, AgentLifecycle, AgentOutcomeKind,
    AgentSourceKind, PendingInteractionKind,
};
use ::agent_intel::ops::dto::{ClaudeGlobalStatus, CopilotGlobalStatus};
use serde::{Deserialize, Serialize};

use crate::agent_intel::risk::ApprovalRisk;

#[cfg(feature = "cli")]
use super::{AuthEvent, TerminalEvent};
use super::{DeviceEvent, FriendsEvent, RelayActionStatus, RoomEvent, SessionEvent, TrustEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RemotePermissionDecisionPhase {
    Pending,
    Sending,
    Resolved,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum PendingPermissionDecisionPhase {
    Actionable,
    Sending,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ActivePendingPermission {
    pub session_id: String,
    pub session_incarnation_id: String,
    pub request_generation: u64,
    pub tool_use_id: String,
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub created_at_ms: u64,
    pub deadline_at_ms: u64,
    pub risk: ApprovalRisk,
    pub decision_phase: PendingPermissionDecisionPhase,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PendingPermissionsSnapshot {
    pub generation: u64,
    pub requests: Vec<ActivePendingPermission>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LiveAgentIntelEntry {
    pub session_id: String,
    pub session_incarnation_id: String,
    pub snapshot: AgentIntelSnapshotV34,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentIntelSnapshotV34 {
    pub identity: LiveAgentIdentityV34,
    pub lifecycle: AgentLifecycle,
    pub attention: Option<AgentAttentionV34>,
    pub pending_interaction: Option<PendingAgentInteractionV34>,
    pub current_activity: Option<CurrentAgentActivityV34>,
    pub workers: ChildAgentSummaryV34,
    pub outcome: Option<AgentOutcomeV34>,
    pub exceptional_state: Option<AgentExceptionalStateV34>,
    pub source: AgentSourceV34,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LiveAgentIdentityV34 {
    pub agent_type: String,
    pub version: Option<String>,
    pub model: Option<String>,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub vendor_session_id: Option<String>,
    pub process_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentAttentionV34 {
    pub kind: AgentAttentionKind,
    pub summary: String,
    pub actionable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the complete v34 interaction projection exposes independent action capabilities"
)]
pub struct PendingAgentInteractionV34 {
    pub kind: PendingInteractionKind,
    pub summary: String,
    pub tool_name: Option<String>,
    pub can_approve: bool,
    pub can_deny: bool,
    pub can_answer: bool,
    pub can_focus: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CurrentAgentActivityV34 {
    pub summary: String,
    pub last_progress_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChildAgentSummaryV34 {
    pub active: u32,
    pub blocked: u32,
    pub failed: u32,
    pub completed: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentOutcomeV34 {
    pub kind: AgentOutcomeKind,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentExceptionalStateV34 {
    pub kind: AgentExceptionalKind,
    pub summary: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentSourceV34 {
    pub kind: AgentSourceKind,
    pub degraded: bool,
    pub detail: Option<String>,
}

impl From<AgentIntelSnapshot> for AgentIntelSnapshotV34 {
    fn from(snapshot: AgentIntelSnapshot) -> Self {
        Self {
            identity: LiveAgentIdentityV34 {
                agent_type: snapshot.identity.agent_type,
                version: snapshot.identity.version,
                model: snapshot.identity.model,
                title: snapshot.identity.title,
                cwd: snapshot.identity.cwd,
                vendor_session_id: snapshot.identity.vendor_session_id,
                process_id: snapshot.identity.process_id,
            },
            lifecycle: snapshot.lifecycle,
            attention: snapshot.attention.map(|attention| AgentAttentionV34 {
                kind: attention.kind,
                summary: attention.summary,
                actionable: attention.actionable,
            }),
            pending_interaction: snapshot.pending_interaction.map(|interaction| {
                PendingAgentInteractionV34 {
                    kind: interaction.kind,
                    summary: interaction.summary,
                    tool_name: interaction.tool_name,
                    can_approve: interaction.can_approve,
                    can_deny: interaction.can_deny,
                    can_answer: interaction.can_answer,
                    can_focus: interaction.can_focus,
                }
            }),
            current_activity: snapshot
                .current_activity
                .map(|activity| CurrentAgentActivityV34 {
                    summary: activity.summary,
                    last_progress_at: activity.last_progress_at,
                }),
            workers: ChildAgentSummaryV34 {
                active: snapshot.workers.active,
                blocked: snapshot.workers.blocked,
                failed: snapshot.workers.failed,
                completed: snapshot.workers.completed,
            },
            outcome: snapshot.outcome.map(|outcome| AgentOutcomeV34 {
                kind: outcome.kind,
                summary: outcome.summary,
            }),
            exceptional_state: snapshot.exceptional_state.map(|exceptional| {
                AgentExceptionalStateV34 {
                    kind: exceptional.kind,
                    summary: exceptional.summary,
                    retryable: exceptional.retryable,
                }
            }),
            source: AgentSourceV34 {
                kind: snapshot.source.kind,
                degraded: snapshot.source.degraded,
                detail: snapshot.source.detail,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CollaborationCleanupState {
    Healthy,
    Quarantined,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CollaborationCleanupHealth {
    pub state: CollaborationCleanupState,
    pub pending_count: usize,
    pub quarantined_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SystemEvent {
    #[serde(rename = "heartbeat")]
    Heartbeat,
    #[serde(rename = "error")]
    Error {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        context: Option<String>,
    },
    #[serde(rename = "runtime.health")]
    RuntimeHealth {
        #[serde(rename = "collaborationCleanup")]
        collaboration_cleanup: CollaborationCleanupHealth,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AgentIntelFailureKind {
    Deterministic,
    DeliveryAmbiguous,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentIntelEvent {
    #[serde(rename = "agent.intel.snapshot")]
    Snapshot {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "sessionIncarnationId")]
        session_incarnation_id: String,
        payload: serde_json::Value,
    },
    #[serde(rename = "agent.intel.cleared")]
    Cleared {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "sessionIncarnationId")]
        session_incarnation_id: String,
    },
    #[serde(rename = "agent.intel.liveSet")]
    LiveSet {
        #[serde(rename = "requestId")]
        request_id: Option<String>,
        #[serde(rename = "authorityIncarnationId")]
        authority_incarnation_id: String,
        revision: u64,
        entries: Vec<LiveAgentIntelEntry>,
    },
    #[serde(rename = "agent.intel.reply")]
    Reply {
        #[serde(rename = "requestId")]
        request_id: String,
        payload: serde_json::Value,
    },
    #[serde(rename = "agent.intel.error")]
    Error {
        #[serde(rename = "requestId")]
        request_id: String,
        message: String,
        #[serde(rename = "failureKind")]
        failure_kind: AgentIntelFailureKind,
        #[serde(rename = "mutationId")]
        mutation_id: Option<String>,
        #[serde(rename = "reconciliationRequired")]
        reconciliation_required: bool,
    },
    #[serde(rename = "agent.intel.pendingPermissionsSnapshot")]
    PendingPermissionsSnapshot {
        generation: u64,
        requests: Vec<ActivePendingPermission>,
    },

    #[serde(rename = "agent.intel.remotePermissionDecisionState")]
    RemotePermissionDecisionState {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "sessionIncarnationId")]
        session_incarnation_id: String,
        #[serde(rename = "toolUseId")]
        tool_use_id: String,
        #[serde(rename = "requestGeneration")]
        request_generation: u64,
        phase: RemotePermissionDecisionPhase,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<RelayActionStatus>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },

    #[serde(rename = "agent.intel.steerState")]
    SteerState {
        entry: super::SteerQueueEntry,
        transition: super::SteerTransition,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentGlobalEvent {
    #[serde(rename = "agent.global.claude.status")]
    ClaudeStatus {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        generation: Option<u64>,
        status: ClaudeGlobalStatus,
    },
    #[serde(rename = "agent.global.copilot.status")]
    CopilotStatus {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        generation: Option<u64>,
        status: CopilotGlobalStatus,
    },
    #[serde(rename = "agent.global.mcp.health")]
    McpHealth {
        vendor: String,
        scope: String,
        #[serde(rename = "serverName")]
        server_name: String,
        health: ::agent_intel::mcp::McpHealth,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum EventAuthority {
    AccountContext,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountContextEvent<T> {
    pub authority: EventAuthority,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_user_id: Option<String>,
    pub account_epoch: u64,
    #[serde(flatten)]
    pub event: T,
}

impl<T> AccountContextEvent<T> {
    pub fn new(account_user_id: Option<String>, account_epoch: u64, event: T) -> Self {
        Self {
            authority: EventAuthority::AccountContext,
            account_user_id,
            account_epoch,
            event,
        }
    }
}

pub type AccountSessionEvent = AccountContextEvent<SessionEvent>;
pub type AccountAgentIntelEvent = AccountContextEvent<AgentIntelEvent>;
pub type AccountFriendsEvent = AccountContextEvent<FriendsEvent>;
pub type AccountDeviceEvent = AccountContextEvent<DeviceEvent>;
pub type AccountTrustEvent = AccountContextEvent<TrustEvent>;
pub type AccountRoomEvent = AccountContextEvent<RoomEvent>;

#[cfg(feature = "cli")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum HostEvent {
    Terminal(TerminalEvent),
    System(SystemEvent),
    Auth(AuthEvent),
    Friends(FriendsEvent),
    Devices(DeviceEvent),
    Trust(TrustEvent),
    Room(RoomEvent),
    Session(SessionEvent),
    AgentIntel(AgentIntelEvent),
    AgentGlobal(AgentGlobalEvent),
}

#[cfg(feature = "cli")]
impl From<TerminalEvent> for HostEvent {
    fn from(event: TerminalEvent) -> Self {
        Self::Terminal(event)
    }
}

#[cfg(feature = "cli")]
impl From<SystemEvent> for HostEvent {
    fn from(event: SystemEvent) -> Self {
        Self::System(event)
    }
}

#[cfg(feature = "cli")]
impl From<AuthEvent> for HostEvent {
    fn from(event: AuthEvent) -> Self {
        Self::Auth(event)
    }
}

#[cfg(feature = "cli")]
impl From<FriendsEvent> for HostEvent {
    fn from(event: FriendsEvent) -> Self {
        Self::Friends(event)
    }
}

#[cfg(feature = "cli")]
impl From<DeviceEvent> for HostEvent {
    fn from(event: DeviceEvent) -> Self {
        Self::Devices(event)
    }
}

#[cfg(feature = "cli")]
impl From<TrustEvent> for HostEvent {
    fn from(event: TrustEvent) -> Self {
        Self::Trust(event)
    }
}

#[cfg(feature = "cli")]
impl From<RoomEvent> for HostEvent {
    fn from(event: RoomEvent) -> Self {
        Self::Room(event)
    }
}

#[cfg(feature = "cli")]
impl From<SessionEvent> for HostEvent {
    fn from(event: SessionEvent) -> Self {
        Self::Session(event)
    }
}

#[cfg(feature = "cli")]
impl From<AgentIntelEvent> for HostEvent {
    fn from(event: AgentIntelEvent) -> Self {
        Self::AgentIntel(event)
    }
}

#[cfg(feature = "cli")]
impl From<AgentGlobalEvent> for HostEvent {
    fn from(event: AgentGlobalEvent) -> Self {
        Self::AgentGlobal(event)
    }
}
