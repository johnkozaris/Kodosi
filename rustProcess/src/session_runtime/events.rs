use serde::Deserialize;

use crate::session_runtime::project::ProjectDiscovery;
use kodosi_domain::{
    device_link::{DeviceLinkOutcome, SelfDeviceLinkOutcome},
    ids::SessionId,
    lifecycle::{
        ConnectionState, RemoteActionStatus, RemoteSessionAccessIssue, RemoteSessionAccessState,
        StopReason,
    },
    session::SessionState,
    terminal::TerminalSize,
};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub(crate) struct AccountEpoch(u64);

impl AccountEpoch {
    pub(crate) const INITIAL: Self = Self(0);

    #[cfg(test)]
    pub(crate) const fn for_test(value: u64) -> Self {
        Self(value)
    }

    pub(crate) const fn value(self) -> u64 {
        self.0
    }

    pub(crate) fn next(self) -> crate::Result<Self> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(crate::AppError::AccountEpochExhausted)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LocalCoordinatorOrigin {
    pub(crate) session_id: SessionId,
    pub(crate) local_incarnation_id: uuid::Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AccountEventOrigin {
    pub(crate) account_user_id: String,
    pub(crate) epoch: AccountEpoch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HostRelayEventOrigin {
    pub(crate) account_origin: AccountEventOrigin,
    pub(crate) session_id: SessionId,
    pub(crate) relay_generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum DiscoverySurface {
    Friends,
    RoomCatalog,
    RoomFeed,
    RoomChat,
    RoomTasks,
    OwnSessions,
}

#[derive(Debug, Clone)]
pub(crate) enum RuntimeSessionEvent {
    TerminalOutputObserved {
        origin: LocalCoordinatorOrigin,
        data: bytes::Bytes,
    },
    CaptureMetadata {
        origin: LocalCoordinatorOrigin,
        size: TerminalSize,
        working_dir: Option<String>,
        refresh_working_dir: bool,
    },
    RuntimeMetadata {
        origin: LocalCoordinatorOrigin,
        working_dir: Option<String>,
        running_command: Option<String>,
        detected_agent: Option<String>,
    },
    WorkingDirChanged {
        origin: LocalCoordinatorOrigin,
        working_dir: String,
    },
    ProjectDiscoveryReady {
        working_dir: String,
        discovery: ProjectDiscovery,
    },
    ProjectDiscoveryFailed {
        working_dir: String,
        message: String,
    },
    StateChanged {
        origin: HostRelayEventOrigin,
        state: SessionState,
    },
    HostRelayInfo {
        origin: HostRelayEventOrigin,
        message: String,
    },
    HostRelayLogError {
        origin: HostRelayEventOrigin,
        message: String,
    },
    RemoteInfo {
        id: SessionId,
        message: String,
        relay_generation: u64,
    },
    RemoteLogError {
        id: SessionId,
        message: String,
        relay_generation: u64,
    },
    DiscoveryInvalidated {
        origin: AccountEventOrigin,
        surfaces: Vec<DiscoverySurface>,
        room_id: Option<String>,
    },
    UserDeviceListChanged {
        origin: AccountEventOrigin,
        user_id: String,
        generation: u64,
    },
    UserIdentityLifecycleChanged {
        origin: AccountEventOrigin,
        user_id: String,
        identity_revision: u64,
        state: kodosi_domain::user::IdentityLifecycleState,
    },
    DeviceLinkSnapshot {
        origin: AccountEventOrigin,
        requests: Vec<kodosi_backend_client::user_events::UserDeviceLinkSnapshotEntry>,
    },
    DeviceLinkRequested {
        origin: AccountEventOrigin,
        user_code: String,
        device_label: String,
        expires_at: String,
    },
    DeviceLinkResolved {
        origin: AccountEventOrigin,
        user_code: String,
        outcome: DeviceLinkOutcome,
    },
    DeviceLinkSelfPending {
        origin: AccountEventOrigin,
        user_code: String,
        expires_at: String,
    },

    DeviceLinkSelfResolved {
        origin: AccountEventOrigin,
        user_code: String,
        outcome: SelfDeviceLinkOutcome,
    },
    BackendAccessInvalid {
        origin: AccountEventOrigin,
        reason: String,
    },
    HostRelayBackendAccessInvalid {
        origin: HostRelayEventOrigin,
        reason: String,
    },
    HostDemand {
        origin: HostRelayEventOrigin,
        required: bool,
        participant_count: usize,
        reason: String,
    },
    RemoteCheckpoint {
        id: SessionId,
        next_sequence: u64,
        checkpoint: kodosi_domain::terminal::TerminalCheckpointV2,
        application: kodosi_backend_client::session_relay::events::ApplicationAck,
        relay_generation: u64,
    },
    RemoteRawBatch {
        id: SessionId,
        first_sequence: u64,
        next_sequence: u64,
        chunks: Vec<Vec<u8>>,
        relay_generation: u64,
    },
    RemotePlainPresentation {
        id: SessionId,
        presentation: kodosi_domain::terminal::TerminalPresentationV2,
        relay_generation: u64,
    },
    RemotePendingPermissionsSnapshot {
        id: SessionId,
        incarnation_id: uuid::Uuid,
        generation: u64,
        snapshot: serde_json::Value,
        relay_generation: u64,
    },
    RemoteStateChanged {
        id: SessionId,
        state: SessionState,
        relay_generation: u64,
    },
    RemoteAccessChanged {
        id: SessionId,
        access: kodosi_domain::permissions::AccessLevel,
        relay_generation: u64,
    },
    RemoteActionResult {
        id: SessionId,
        action_id: String,
        request_id: Option<String>,
        request_generation: Option<u64>,
        status: RemoteActionStatus,
        relay_generation: u64,
    },
    RemoteSessionConnectionChanged {
        id: SessionId,
        status: ConnectionState,
        reason: Option<String>,
        relay_generation: u64,
    },
    RemoteAccessStateChanged {
        id: SessionId,
        state: RemoteSessionAccessState,
        reason: Option<String>,
        issue: Option<RemoteSessionAccessIssue>,
        relay_generation: u64,
    },
    RemoteAccessRevoked {
        id: SessionId,
        relay_generation: u64,
    },
    RemoteSessionRelayExited {
        id: SessionId,
        relay_generation: u64,
    },
    HostAccessRevoked {
        origin: HostRelayEventOrigin,
        revoked_user_id: String,
    },
    HostRelayBackendRestarted {
        origin: HostRelayEventOrigin,
    },
    HostFrameReservationExhausted {
        origin: HostRelayEventOrigin,
    },
    HostKeyDistributionRequested {
        origin: HostRelayEventOrigin,
        fence_id: String,
    },

    HostKeyRotationRequired {
        origin: HostRelayEventOrigin,
        reason: String,
        fail_closed: bool,
    },
    AgentIntelSnapshot {
        id: SessionId,
        local_incarnation_id: uuid::Uuid,
        generation: uuid::Uuid,
        payload: Box<agent_intel::AgentIntelSnapshot>,
    },
    RoomAgentDeliveryState {
        id: SessionId,
        local_incarnation_id: uuid::Uuid,
        state: crate::host_protocol::RoomAgentDeliveryState,
        event_id: Option<String>,
        detail: Option<String>,
    },
    AgentBoundary {
        id: SessionId,
        local_incarnation_id: uuid::Uuid,
        kind: AgentBoundaryKind,
        tool_use_id: Option<String>,
    },
    AgentTurnStateChanged {
        id: SessionId,
        local_incarnation_id: uuid::Uuid,
        state: AgentTurnState,
    },

    PendingPermissionRequest {
        id: SessionId,
        local_incarnation_id: uuid::Uuid,
        tool_use_id: String,
    },

    PermissionResolved {
        id: SessionId,
        local_incarnation_id: uuid::Uuid,
        tool_use_id: String,
    },
    RemoteSemanticReceipt {
        id: SessionId,
        receipt: kodosi_backend_client::session_relay::wire::ParticipantSemanticReceiptMessage,
        persistence: kodosi_backend_client::session_relay::events::ApplicationAck,
        relay_generation: u64,
    },
    RemoteControlTrustEstablished {
        id: SessionId,
        incarnation_id: uuid::Uuid,
        owner_user_id: String,
        signer_device_id: String,
        signer_public_key: Vec<u8>,
        device_list_generation: u64,
        identity_fingerprint: [u8; 32],
        relay_generation: u64,
    },
    RemoteActionCompleted {
        origin: HostRelayEventOrigin,
        incarnation_id: uuid::Uuid,
        action_id: String,
        request_id: String,
        requester_user_id: String,
        requester_device_id: String,
        accepted: bool,
    },

    RemotePermissionDecision {
        origin: HostRelayEventOrigin,
        incarnation_id: uuid::Uuid,
        action_id: String,
        request_id: String,
        request_generation: u64,
        decision: String,
        decider_user_id: String,
        decider_device_id: Option<String>,
        reply: kodosi_backend_client::relay::SemanticAdmissionReply,
    },
    HostActionResultMailboxAck {
        origin: HostRelayEventOrigin,
        result: crate::runtime::action_results::OwnerActionResult,
        delivered: bool,
    },
    HostSemanticReceiptMailboxAck {
        origin: HostRelayEventOrigin,
        request_id: uuid::Uuid,
        incarnation_id: uuid::Uuid,
        delivered: bool,
        acknowledge_locally: bool,
    },
    HostSemanticSend {
        origin: HostRelayEventOrigin,
        request_id: uuid::Uuid,
        incarnation_id: uuid::Uuid,
        mode: kodosi_backend_client::session_relay::wire::RelaySemanticMode,
        payload_sha256: String,
        text: String,
        requester_user_id: String,
        requester_device_id: String,
        reply: kodosi_backend_client::relay::SemanticAdmissionReply,
    },
    HostSemanticCancel {
        origin: HostRelayEventOrigin,
        request_id: uuid::Uuid,
        incarnation_id: uuid::Uuid,
        mode: kodosi_backend_client::session_relay::wire::RelaySemanticMode,
        payload_sha256: String,
        requester_user_id: String,
        requester_device_id: String,
        reply: kodosi_backend_client::relay::SemanticAdmissionReply,
    },
    ClipboardUpdate {
        origin: LocalCoordinatorOrigin,
        text: String,
    },
    ClipboardWriteRequest {
        origin: LocalCoordinatorOrigin,
        text: String,
        reply: std::sync::mpsc::SyncSender<kodosi_session::ClipboardWriteOutcome>,
    },
    TerminalBell {
        origin: LocalCoordinatorOrigin,
    },
    TerminalTitleChanged {
        origin: LocalCoordinatorOrigin,
        title: Option<String>,
    },
    TerminalNotification {
        origin: LocalCoordinatorOrigin,
        title: Option<String>,
        body: Option<String>,
    },
    Stopped {
        origin: LocalCoordinatorOrigin,
        reason: StopReason,
    },
    Failed {
        origin: LocalCoordinatorOrigin,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentBoundaryKind {
    Tool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentTurnState {
    Running,
    Idle,
}

enum RuntimeEventOrigin<'a> {
    None,
    Account(&'a AccountEventOrigin),
    HostRelay(&'a HostRelayEventOrigin),
}

impl RuntimeSessionEvent {
    const fn origin(&self) -> RuntimeEventOrigin<'_> {
        match self {
            Self::DiscoveryInvalidated { origin, .. }
            | Self::UserDeviceListChanged { origin, .. }
            | Self::UserIdentityLifecycleChanged { origin, .. }
            | Self::DeviceLinkSnapshot { origin, .. }
            | Self::DeviceLinkRequested { origin, .. }
            | Self::DeviceLinkResolved { origin, .. }
            | Self::DeviceLinkSelfPending { origin, .. }
            | Self::DeviceLinkSelfResolved { origin, .. }
            | Self::BackendAccessInvalid { origin, .. } => RuntimeEventOrigin::Account(origin),
            Self::StateChanged { origin, .. }
            | Self::HostRelayInfo { origin, .. }
            | Self::HostRelayLogError { origin, .. }
            | Self::HostRelayBackendAccessInvalid { origin, .. }
            | Self::HostDemand { origin, .. }
            | Self::HostAccessRevoked { origin, .. }
            | Self::HostRelayBackendRestarted { origin }
            | Self::HostFrameReservationExhausted { origin }
            | Self::HostKeyDistributionRequested { origin, .. }
            | Self::HostKeyRotationRequired { origin, .. }
            | Self::RemoteActionCompleted { origin, .. }
            | Self::HostActionResultMailboxAck { origin, .. }
            | Self::HostSemanticReceiptMailboxAck { origin, .. }
            | Self::HostSemanticSend { origin, .. }
            | Self::HostSemanticCancel { origin, .. }
            | Self::RemotePermissionDecision { origin, .. } => {
                RuntimeEventOrigin::HostRelay(origin)
            }
            Self::TerminalOutputObserved { .. }
            | Self::CaptureMetadata { .. }
            | Self::RuntimeMetadata { .. }
            | Self::WorkingDirChanged { .. }
            | Self::ProjectDiscoveryReady { .. }
            | Self::ProjectDiscoveryFailed { .. }
            | Self::RemoteInfo { .. }
            | Self::RemoteLogError { .. }
            | Self::RemoteCheckpoint { .. }
            | Self::RemoteRawBatch { .. }
            | Self::RemotePlainPresentation { .. }
            | Self::RemotePendingPermissionsSnapshot { .. }
            | Self::RemoteSemanticReceipt { .. }
            | Self::RemoteStateChanged { .. }
            | Self::RemoteAccessChanged { .. }
            | Self::RemoteActionResult { .. }
            | Self::RemoteSessionConnectionChanged { .. }
            | Self::RemoteAccessStateChanged { .. }
            | Self::RemoteAccessRevoked { .. }
            | Self::RemoteSessionRelayExited { .. }
            | Self::AgentIntelSnapshot { .. }
            | Self::RoomAgentDeliveryState { .. }
            | Self::AgentBoundary { .. }
            | Self::AgentTurnStateChanged { .. }
            | Self::PendingPermissionRequest { .. }
            | Self::PermissionResolved { .. }
            | Self::RemoteControlTrustEstablished { .. }
            | Self::ClipboardUpdate { .. }
            | Self::ClipboardWriteRequest { .. }
            | Self::TerminalBell { .. }
            | Self::TerminalTitleChanged { .. }
            | Self::TerminalNotification { .. }
            | Self::Stopped { .. }
            | Self::Failed { .. } => RuntimeEventOrigin::None,
        }
    }
    pub(crate) const fn local_descriptor_candidate(&self) -> Option<SessionId> {
        if !matches!(self, Self::TerminalOutputObserved { .. })
            && let Some(origin) = self.local_coordinator_origin()
        {
            return Some(origin.session_id);
        }
        match self {
            Self::AgentIntelSnapshot { id, .. }
            | Self::RoomAgentDeliveryState { id, .. }
            | Self::AgentBoundary { id, .. }
            | Self::AgentTurnStateChanged { id, .. }
            | Self::PendingPermissionRequest { id, .. }
            | Self::PermissionResolved { id, .. } => Some(*id),
            Self::StateChanged { origin, .. }
            | Self::HostRelayInfo { origin, .. }
            | Self::HostRelayLogError { origin, .. }
            | Self::HostRelayBackendAccessInvalid { origin, .. }
            | Self::HostDemand { origin, .. }
            | Self::HostAccessRevoked { origin, .. }
            | Self::HostRelayBackendRestarted { origin }
            | Self::HostFrameReservationExhausted { origin }
            | Self::HostKeyDistributionRequested { origin, .. }
            | Self::HostKeyRotationRequired { origin, .. }
            | Self::RemoteActionCompleted { origin, .. }
            | Self::HostActionResultMailboxAck { origin, .. }
            | Self::HostSemanticReceiptMailboxAck { origin, .. }
            | Self::HostSemanticSend { origin, .. }
            | Self::HostSemanticCancel { origin, .. }
            | Self::RemotePermissionDecision { origin, .. } => Some(origin.session_id),
            Self::TerminalOutputObserved { .. }
            | Self::CaptureMetadata { .. }
            | Self::RuntimeMetadata { .. }
            | Self::WorkingDirChanged { .. }
            | Self::ClipboardUpdate { .. }
            | Self::ClipboardWriteRequest { .. }
            | Self::TerminalBell { .. }
            | Self::TerminalTitleChanged { .. }
            | Self::TerminalNotification { .. }
            | Self::Stopped { .. }
            | Self::Failed { .. }
            | Self::ProjectDiscoveryReady { .. }
            | Self::ProjectDiscoveryFailed { .. }
            | Self::DiscoveryInvalidated { .. }
            | Self::UserDeviceListChanged { .. }
            | Self::UserIdentityLifecycleChanged { .. }
            | Self::DeviceLinkSnapshot { .. }
            | Self::DeviceLinkRequested { .. }
            | Self::DeviceLinkResolved { .. }
            | Self::DeviceLinkSelfPending { .. }
            | Self::DeviceLinkSelfResolved { .. }
            | Self::BackendAccessInvalid { .. }
            | Self::RemoteInfo { .. }
            | Self::RemoteLogError { .. }
            | Self::RemoteCheckpoint { .. }
            | Self::RemoteRawBatch { .. }
            | Self::RemotePlainPresentation { .. }
            | Self::RemotePendingPermissionsSnapshot { .. }
            | Self::RemoteSemanticReceipt { .. }
            | Self::RemoteStateChanged { .. }
            | Self::RemoteAccessChanged { .. }
            | Self::RemoteActionResult { .. }
            | Self::RemoteSessionConnectionChanged { .. }
            | Self::RemoteAccessStateChanged { .. }
            | Self::RemoteAccessRevoked { .. }
            | Self::RemoteSessionRelayExited { .. }
            | Self::RemoteControlTrustEstablished { .. } => None,
        }
    }

    pub(crate) const fn local_coordinator_origin(&self) -> Option<LocalCoordinatorOrigin> {
        match self {
            Self::TerminalOutputObserved { origin, .. }
            | Self::CaptureMetadata { origin, .. }
            | Self::RuntimeMetadata { origin, .. }
            | Self::WorkingDirChanged { origin, .. }
            | Self::ClipboardUpdate { origin, .. }
            | Self::ClipboardWriteRequest { origin, .. }
            | Self::TerminalBell { origin }
            | Self::TerminalTitleChanged { origin, .. }
            | Self::TerminalNotification { origin, .. }
            | Self::Stopped { origin, .. }
            | Self::Failed { origin, .. } => Some(*origin),
            _ => None,
        }
    }

    pub(crate) const fn local_hook_origin(&self) -> Option<(SessionId, uuid::Uuid)> {
        match self {
            Self::PendingPermissionRequest {
                id,
                local_incarnation_id,
                ..
            }
            | Self::PermissionResolved {
                id,
                local_incarnation_id,
                ..
            } => Some((*id, *local_incarnation_id)),
            _ => None,
        }
    }

    pub(crate) fn reject_reply(&self) {
        match self {
            Self::RemotePermissionDecision { reply, .. }
            | Self::HostSemanticSend { reply, .. }
            | Self::HostSemanticCancel { reply, .. } => reply.complete(false),
            Self::ClipboardWriteRequest { reply, .. } => {
                let _ = reply.try_send(kodosi_session::ClipboardWriteOutcome::Denied);
            }
            _ => {}
        }
    }

    pub(crate) const fn account_event_origin(&self) -> Option<&AccountEventOrigin> {
        match self.origin() {
            RuntimeEventOrigin::Account(origin) => Some(origin),
            RuntimeEventOrigin::HostRelay(origin) => Some(&origin.account_origin),
            RuntimeEventOrigin::None => None,
        }
    }

    pub(crate) const fn host_relay_event_origin(&self) -> Option<&HostRelayEventOrigin> {
        match self.origin() {
            RuntimeEventOrigin::HostRelay(origin) => Some(origin),
            RuntimeEventOrigin::None | RuntimeEventOrigin::Account(_) => None,
        }
    }
}
