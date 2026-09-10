pub(crate) mod agent_intel_dispatch;
mod auth;
pub(crate) mod authority;
mod devices;
mod friends;
mod host_command;
mod host_event;
mod rooms;
mod session_catalog_events;
mod session_entries;
mod sessions;
pub(crate) mod sessions_command;
mod sessions_dispatch;
mod sessions_focus_dispatch;
mod sessions_terminal_dispatch;
mod steering;
mod terminal;
mod trust;

pub use auth::{AuthCommand, AuthEvent, AuthRequiredReason};
pub use devices::{DeviceCommand, DeviceEvent, DeviceLinkRequestEntry, MyDeviceEntry};
pub use friends::{FriendEntry, FriendRequestEntry, FriendsCommand, FriendsEvent};
pub use host_command::{AgentIntelCommand, SystemCommand};
pub use host_event::{
    AccountAgentIntelEvent, AccountContextEvent, AccountDeviceEvent, AccountFriendsEvent,
    AccountRoomEvent, AccountSessionEvent, AccountTrustEvent, ActivePendingPermission,
    AgentGlobalEvent, AgentIntelEvent, AgentIntelFailureKind, CollaborationCleanupHealth,
    CollaborationCleanupState, PendingPermissionDecisionPhase, PendingPermissionsSnapshot,
    RemotePermissionDecisionPhase, SystemEvent,
};
pub use kodosi_domain::device_link::{DeviceLinkOutcome, SelfDeviceLinkOutcome};
pub use rooms::{
    CHAT_BODY_MAX_LEN, CHAT_RECIPIENT_MAX_COUNT, ROOM_LENGTH_UNIT, ROOM_NAME_MAX_LEN,
    ROOM_SLUG_MAX_LEN, ROOM_SLUG_MIN_LEN, ROOM_SLUG_PATTERN, RoomActionStatus,
    RoomAgentDeliveryState, RoomChatEntry, RoomCommand, RoomEntry, RoomEvent, RoomInvitationEntry,
    RoomMemberEntry, RoomTaskEntry, TASK_DESCRIPTION_MAX_LEN, TASK_RESULT_MAX_LEN, TASK_STATUSES,
    TASK_TITLE_MAX_LEN,
};
pub use sessions::{AccessGrantEntry, HiddenSessionEntry, SessionCommand, SessionEvent};
pub(crate) use sessions::{SessionAccessMutationKind, SessionAccessMutationOutcome};
pub use steering::{SemanticSendMode, SteerDeliveryState, SteerQueueEntry, SteerTransition};
pub(crate) use terminal::TerminalResizeIdentity;
pub use terminal::{TerminalCommand, TerminalEvent};
pub use trust::{TrustCommand, TrustEvent, TrustPinEntry};

pub(crate) use auth::IdentityHealthState;
#[cfg(feature = "cli")]
pub(crate) use host_command::HostCommand;
pub(crate) use host_command::HostCommandValidationError;
#[cfg(feature = "cli")]
pub(crate) use host_event::HostEvent;
pub(crate) use host_event::LiveAgentIntelEntry;
pub(crate) use session_catalog_events::{room_catalog_event, session_catalog_snapshot_event};
pub(crate) use session_entries::{
    LocalSessionListEntry, PermissionFlags, RemoteSessionListEntry, RoomListEntry,
    RuntimeSessionStatus, SessionListEntry, SessionListEntryMeta, SessionSemanticActions,
};
pub(crate) use sessions::RelayActionStatus;

#[cfg(test)]
mod tests;
