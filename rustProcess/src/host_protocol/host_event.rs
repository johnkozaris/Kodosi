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
