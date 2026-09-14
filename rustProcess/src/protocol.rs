use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    Error, Result,
    provider::{ConversationIdentity, Provider},
};

fn validate_participants(users: &[String]) -> Result<()> {
    if users.len() > 64 {
        return Err(Error::Invalid("too many shared participants".into()));
    }
    let mut ids = std::collections::BTreeSet::new();
    for user in users {
        if !ids.insert(parse_id(user)?) {
            return Err(Error::Invalid("shared participants must be unique".into()));
        }
    }
    Ok(())
}

pub const VERSION: u32 = 42;
include!(concat!(env!("OUT_DIR"), "/network_versions.rs"));
pub const MAX_COMMAND_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, specta_macros::Type)]
#[serde(rename_all = "camelCase")]
pub struct CommandEnvelope {
    #[serde(deserialize_with = "account_user")]
    pub account_user_id: Option<String>,
    pub account_epoch: u64,
    #[serde(flatten, deserialize_with = "validated_command")]
    pub command: Command,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta_macros::Type)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Command {
    #[serde(rename = "auth.login.start")]
    Login {},
    #[serde(rename = "auth.logout")]
    Logout {},
    #[serde(rename = "auth.refresh")]
    RefreshAuth {},
    #[serde(rename = "devices.refresh")]
    RefreshDevices {},
    #[serde(rename = "devices.revoke")]
    RevokeDevice {
        #[serde(rename = "deviceId")]
        device_id: String,
    },
    #[serde(rename = "devices.link.approve")]
    ApproveDevice {
        #[serde(rename = "userCode")]
        user_code: String,
    },
    #[serde(rename = "devices.link.startSelf")]
    LinkDevice {},
    #[serde(rename = "devices.link.cancelSelf")]
    CancelDeviceLink {},
    #[serde(rename = "friends.refresh")]
    RefreshFriends {},
    #[serde(rename = "friends.request.send")]
    RequestFriend {
        username: String,
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "friends.request.accept")]
    AcceptFriend { username: String },
    #[serde(rename = "friends.request.reject")]
    RejectFriend { username: String },
    #[serde(rename = "friends.request.cancel")]
    CancelFriend { username: String },
    #[serde(rename = "friends.remove")]
    RemoveFriend { username: String },
    #[serde(rename = "session.list")]
    ListSessions {},
    #[serde(rename = "session.create")]
    CreateSession {
        #[serde(rename = "requestId")]
        request_id: String,
        name: String,
        #[serde(rename = "workingDir", default)]
        working_dir: Option<String>,
        #[serde(default)]
        resume: Option<ConversationIdentity>,
    },
    #[serde(rename = "session.rename")]
    RenameSession {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
        name: String,
    },
    #[serde(rename = "session.close")]
    CloseSession {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
    },
    #[serde(rename = "session.interrupt")]
    InterruptSession {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
    },
    #[serde(rename = "session.openRemote")]
    OpenRemote {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    #[serde(rename = "session.disconnect")]
    DisconnectRemote {
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    #[serde(rename = "session.share")]
    ShareSession {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
        #[serde(rename = "userIds")]
        user_ids: Vec<String>,
        #[serde(
            rename = "expectedUserIds",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        expected_user_ids: Option<Vec<String>>,
    },
    #[serde(rename = "session.leave")]
    LeaveSession {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
    },
    #[serde(rename = "session.attachMission")]
    AttachSession {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
        #[serde(rename = "roomId")]
        room_id: Option<String>,
    },
    #[serde(rename = "room.list")]
    ListRooms {},
    #[serde(rename = "room.create")]
    CreateRoom {
        #[serde(rename = "requestId")]
        request_id: String,
        name: String,
    },
    #[serde(rename = "room.rename")]
    RenameRoom {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "roomId")]
        room_id: String,
        name: String,
    },
    #[serde(rename = "room.delete")]
    DeleteRoom {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "roomId")]
        room_id: String,
    },
    #[serde(rename = "room.open")]
    OpenRoom {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "roomId")]
        room_id: String,
    },
    #[serde(rename = "room.invite")]
    InviteRoom {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "roomId")]
        room_id: String,
        #[serde(rename = "userId")]
        user_id: String,
    },
    #[serde(rename = "room.invitation.accept")]
    AcceptRoom {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "invitationId")]
        invitation_id: String,
    },
    #[serde(rename = "room.invitation.reject")]
    RejectRoom {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "invitationId")]
        invitation_id: String,
    },
    #[serde(rename = "room.removeMember")]
    RemoveRoomMember {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "roomId")]
        room_id: String,
        #[serde(rename = "userId")]
        user_id: String,
    },
    #[serde(rename = "room.leave")]
    LeaveRoom {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "roomId")]
        room_id: String,
    },
    #[serde(rename = "provider.discoverConversations")]
    DiscoverConversations {
        #[serde(rename = "requestId")]
        request_id: String,
        provider: Provider,
        #[serde(rename = "workingDirectory")]
        working_directory: Option<String>,
        #[serde(default)]
        cursor: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(rename = "maxBytes", default)]
        max_bytes: Option<usize>,
    },
    #[serde(rename = "provider.readConversation")]
    ReadConversation {
        #[serde(rename = "requestId")]
        request_id: String,
        provider: Provider,
        #[serde(rename = "workingDirectory")]
        working_directory: String,
        #[serde(rename = "nativeConversationId")]
        native_conversation_id: String,
        #[serde(rename = "beforeByte", default)]
        before_byte: Option<u64>,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(rename = "maxBytes", default)]
        max_bytes: Option<usize>,
    },
    #[serde(rename = "provider.inspect")]
    InspectProvider {
        #[serde(rename = "requestId")]
        request_id: String,
        provider: Provider,
        #[serde(rename = "workingDirectory", default)]
        working_directory: Option<String>,
    },
    #[serde(rename = "system.setTheme")]
    Theme { dark: bool },
    #[serde(rename = "shutdown")]
    Shutdown {},
    #[serde(rename = "session.resize")]
    Resize {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(flatten)]
        identity: ResizeIdentity,
        claim: bool,
    },
    #[serde(rename = "session.focus")]
    Focus {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
        #[serde(rename = "clientId")]
        client_id: String,
        #[serde(rename = "subscriptionGeneration")]
        subscription_generation: u64,
    },
    #[serde(rename = "session.blur")]
    Blur {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
        #[serde(rename = "clientId")]
        client_id: String,
        #[serde(rename = "subscriptionGeneration")]
        subscription_generation: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta_macros::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResizeIdentity {
    pub request_id: String,
    pub expected_runtime_incarnation_id: String,
    pub subscription_id: String,
    pub subscription_generation: u64,
    pub surface_generation: u64,
    pub cols: u16,
    pub rows: u16,
    pub width_pixels: u32,
    pub height_pixels: u32,
    pub cell_width_pixels: u32,
    pub cell_height_pixels: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta_macros::Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionEntry {
    #[serde(default)]
    pub connected_users: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub id: String,
    pub incarnation_id: String,
    pub kind: SessionKind,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_name: Option<String>,
    pub is_owner: bool,
    pub status: SessionStatus,
    pub connection_state: ConnectionState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_name: Option<String>,
    pub shared_with: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_request_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta_macros::Type)]
#[serde(rename_all = "lowercase")]
pub enum SessionKind {
    Local,
    Remote,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta_macros::Type)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Running,
    Reconnecting,
    Closing,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta_macros::Type)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionState {
    Local,
    Connecting,
    Connected,
    Offline,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta_macros::Type)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub account_user_id: Option<String>,
    pub account_epoch: u64,
    #[serde(flatten)]
    pub event: EventBody,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta_macros::Type)]
#[serde(tag = "type", rename_all_fields = "camelCase", deny_unknown_fields)]
pub enum EventBody {
    #[serde(rename = "system.ready")]
    SystemReady { protocol_version: u32 },
    #[serde(rename = "system.error")]
    SystemError { message: String },
    #[serde(rename = "auth.ready")]
    AuthReady {
        user_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        device_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enrolled: Option<bool>,
    },
    #[serde(rename = "auth.required")]
    AuthRequired { reason: AuthRequiredReason },
    #[serde(rename = "auth.device_code")]
    AuthDeviceCode {
        user_code: String,
        verification_uri: String,
    },
    #[serde(rename = "auth.finalizing")]
    AuthFinalizing {},
    #[serde(rename = "auth.notice")]
    AuthNotice {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    #[serde(rename = "auth.error")]
    AuthError {
        operation: String,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },
    #[serde(rename = "friends.snapshot")]
    FriendsSnapshot {
        friends: Vec<FriendEntry>,
        incoming: Vec<FriendRequestEntry>,
        outgoing: Vec<FriendRequestEntry>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },
    #[serde(rename = "friends.error")]
    FriendsError {
        operation: String,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },
    #[serde(rename = "devices.list")]
    DevicesList {
        self_device_id: String,
        local_device_enrolled: bool,
        devices: Vec<DeviceEntry>,
    },
    #[serde(rename = "devices.link.snapshot")]
    DeviceLinksSnapshot { requests: Vec<DeviceLinkRequest> },
    #[serde(rename = "devices.link.resolved")]
    DeviceLinkResolved {
        user_code: String,
        outcome: DeviceLinkOutcome,
    },
    #[serde(rename = "devices.link.selfPending")]
    DeviceLinkSelfPending {
        user_code: String,
        expires_at: String,
    },
    #[serde(rename = "devices.link.selfResolved")]
    DeviceLinkSelfResolved { outcome: DeviceLinkOutcome },
    #[serde(rename = "devices.error")]
    DevicesError {
        operation: String,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        user_code: Option<String>,
    },
    #[serde(rename = "sessions.snapshot")]
    SessionsSnapshot { sessions: Vec<SessionEntry> },
    #[serde(rename = "session.result")]
    SessionResult {
        request_id: String,
        operation: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
    },
    #[serde(rename = "session.error")]
    SessionError {
        operation: String,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
    },
    #[serde(rename = "rooms.snapshot")]
    RoomsSnapshot {
        rooms: Vec<RoomEntry>,
        invitations: Vec<RoomInvitationEntry>,
        #[serde(default)]
        rooms_truncated: bool,
        #[serde(default)]
        invitations_truncated: bool,
    },
    #[serde(rename = "room.snapshot")]
    RoomSnapshot {
        request_id: String,
        room: RoomEntry,
        members: Vec<RoomMemberEntry>,
    },
    #[serde(rename = "room.result")]
    RoomResult {
        request_id: String,
        operation: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        room_id: Option<String>,
    },
    #[serde(rename = "room.error")]
    RoomError {
        operation: String,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        room_id: Option<String>,
    },
    #[serde(rename = "provider.reply")]
    ProviderReply {
        request_id: String,
        #[serde(flatten)]
        reply: ProviderReply,
    },
    #[serde(rename = "provider.error")]
    ProviderError {
        request_id: String,
        operation: String,
        message: String,
    },
    #[serde(rename = "term.bell")]
    TerminalBell { session_id: String },
    #[serde(rename = "term.title")]
    TerminalTitle { session_id: String, title: String },
    #[serde(rename = "term.notification")]
    TerminalNotification {
        session_id: String,
        runtime_incarnation_id: String,
        title: String,
        body: String,
    },
    #[serde(rename = "term.focusApplied")]
    FocusApplied(FocusResult),
    #[serde(rename = "term.focusRejected")]
    FocusRejected(FocusResult),
    #[serde(rename = "term.resizeApplied")]
    ResizeApplied(ResizeResult),
    #[serde(rename = "term.resizeRejected")]
    ResizeRejected(ResizeResult),
}

#[derive(Debug, Clone, Serialize, Deserialize, specta_macros::Type)]
#[serde(tag = "operation", content = "result", deny_unknown_fields)]
pub enum ProviderReply {
    #[serde(rename = "provider.discoverConversations")]
    Conversations(crate::provider::ConversationListPage),
    #[serde(rename = "provider.readConversation")]
    Conversation(crate::provider::ConversationPage),
    #[serde(rename = "provider.inspect")]
    Inspect(crate::provider::ProviderInfo),
}

#[derive(Debug, Clone, Serialize, Deserialize, specta_macros::Type)]
#[serde(rename_all = "camelCase")]
pub enum AuthRequiredReason {
    SignedOut,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta_macros::Type)]
#[serde(rename_all = "lowercase")]
pub enum DeviceLinkOutcome {
    Approved,
    Cancelled,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta_macros::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FriendEntry {
    pub user_id: String,
    pub handle: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta_macros::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FriendRequestEntry {
    pub user_id: String,
    pub handle: String,
    pub display_name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta_macros::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceEntry {
    pub device_id: String,
    pub label: String,
    pub cert_signer_device_id: String,
    pub cert_issued_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta_macros::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceLinkRequest {
    pub user_code: String,
    pub device_label: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta_macros::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoomEntry {
    pub id: String,
    pub name: String,
    pub owner_user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta_macros::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoomInvitationEntry {
    pub id: String,
    pub room_id: String,
    pub room_name: String,
    pub inviter_name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta_macros::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoomMemberEntry {
    pub user_id: String,
    pub handle: String,
    pub display_name: String,
    pub is_owner: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta_macros::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FocusResult {
    pub session_id: String,
    pub request_id: String,
    pub runtime_incarnation_id: String,
    pub subscription_id: String,
    pub subscription_generation: u64,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta_macros::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResizeResult {
    pub session_id: String,
    pub runtime_incarnation_id: String,
    #[serde(flatten)]
    pub identity: ResizeIdentity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl EventBody {
    pub fn new(value: Value) -> Result<Self> {
        serde_json::from_value(value).map_err(Error::from)
    }

    pub const fn kind(&self) -> &'static str {
        match self {
            Self::SystemReady { .. } => "system.ready",
            Self::SystemError { .. } => "system.error",
            Self::AuthReady { .. } => "auth.ready",
            Self::AuthRequired { .. } => "auth.required",
            Self::AuthDeviceCode { .. } => "auth.device_code",
            Self::AuthFinalizing {} => "auth.finalizing",
            Self::AuthNotice { .. } => "auth.notice",
            Self::AuthError { .. } => "auth.error",
            Self::FriendsSnapshot { .. } => "friends.snapshot",
            Self::FriendsError { .. } => "friends.error",
            Self::DevicesList { .. } => "devices.list",
            Self::DeviceLinksSnapshot { .. } => "devices.link.snapshot",
            Self::DeviceLinkResolved { .. } => "devices.link.resolved",
            Self::DeviceLinkSelfPending { .. } => "devices.link.selfPending",
            Self::DeviceLinkSelfResolved { .. } => "devices.link.selfResolved",
            Self::DevicesError { .. } => "devices.error",
            Self::SessionsSnapshot { .. } => "sessions.snapshot",
            Self::SessionResult { .. } => "session.result",
            Self::SessionError { .. } => "session.error",
            Self::RoomsSnapshot { .. } => "rooms.snapshot",
            Self::RoomSnapshot { .. } => "room.snapshot",
            Self::RoomResult { .. } => "room.result",
            Self::RoomError { .. } => "room.error",
            Self::ProviderReply { .. } => "provider.reply",
            Self::ProviderError { .. } => "provider.error",
            Self::TerminalBell { .. } => "term.bell",
            Self::TerminalTitle { .. } => "term.title",
            Self::TerminalNotification { .. } => "term.notification",
            Self::FocusApplied(_) => "term.focusApplied",
            Self::FocusRejected(_) => "term.focusRejected",
            Self::ResizeApplied(_) => "term.resizeApplied",
            Self::ResizeRejected(_) => "term.resizeRejected",
        }
    }
}

impl Event {
    pub fn new(user: Option<String>, epoch: u64, value: Value) -> Result<Self> {
        Ok(Self {
            account_user_id: user,
            account_epoch: epoch,
            event: EventBody::new(value)?,
        })
    }
    pub const fn kind(&self) -> &'static str {
        self.event.kind()
    }
}

fn account_user<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<String>, D::Error> {
    let user = Option::<String>::deserialize(deserializer)?;
    if let Some(id) = &user {
        parse_id(id).map_err(serde::de::Error::custom)?;
    }
    Ok(user)
}

fn validated_command<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Command, D::Error> {
    let command = Command::deserialize(deserializer)?;
    command.validate().map_err(serde::de::Error::custom)?;
    Ok(command)
}

impl Command {
    pub fn operation(&self) -> &'static str {
        match self {
            Self::Login {} => "auth.login.start",
            Self::Logout {} => "auth.logout",
            Self::RefreshAuth {} => "auth.refresh",
            Self::RefreshDevices {} => "devices.refresh",
            Self::RevokeDevice { .. } => "devices.revoke",
            Self::ApproveDevice { .. } => "devices.link.approve",
            Self::LinkDevice {} => "devices.link.startSelf",
            Self::CancelDeviceLink {} => "devices.link.cancelSelf",
            Self::RefreshFriends {} => "friends.refresh",
            Self::RequestFriend { .. } => "friends.request.send",
            Self::AcceptFriend { .. } => "friends.request.accept",
            Self::RejectFriend { .. } => "friends.request.reject",
            Self::CancelFriend { .. } => "friends.request.cancel",
            Self::RemoveFriend { .. } => "friends.remove",
            Self::ListSessions {} => "session.list",
            Self::CreateSession { .. } => "session.create",
            Self::RenameSession { .. } => "session.rename",
            Self::CloseSession { .. } => "session.close",
            Self::InterruptSession { .. } => "session.interrupt",
            Self::OpenRemote { .. } => "session.openRemote",
            Self::DisconnectRemote { .. } => "session.disconnect",
            Self::ShareSession { .. } => "session.share",
            Self::LeaveSession { .. } => "session.leave",
            Self::AttachSession { .. } => "session.attachMission",
            Self::ListRooms {} => "room.list",
            Self::CreateRoom { .. } => "room.create",
            Self::RenameRoom { .. } => "room.rename",
            Self::DeleteRoom { .. } => "room.delete",
            Self::OpenRoom { .. } => "room.open",
            Self::InviteRoom { .. } => "room.invite",
            Self::AcceptRoom { .. } => "room.invitation.accept",
            Self::RejectRoom { .. } => "room.invitation.reject",
            Self::RemoveRoomMember { .. } => "room.removeMember",
            Self::LeaveRoom { .. } => "room.leave",
            Self::DiscoverConversations { .. } => "provider.discoverConversations",
            Self::ReadConversation { .. } => "provider.readConversation",
            Self::InspectProvider { .. } => "provider.inspect",
            Self::Theme { .. } => "system.setTheme",
            Self::Shutdown {} => "shutdown",
            Self::Resize { .. } => "session.resize",
            Self::Focus { .. } => "session.focus",
            Self::Blur { .. } => "session.blur",
        }
    }
    pub fn request_id(&self) -> Option<&str> {
        match self {
            Self::RequestFriend { request_id, .. }
            | Self::CreateSession { request_id, .. }
            | Self::RenameSession { request_id, .. }
            | Self::CloseSession { request_id, .. }
            | Self::InterruptSession { request_id, .. }
            | Self::OpenRemote { request_id, .. }
            | Self::ShareSession { request_id, .. }
            | Self::LeaveSession { request_id, .. }
            | Self::AttachSession { request_id, .. }
            | Self::CreateRoom { request_id, .. }
            | Self::RenameRoom { request_id, .. }
            | Self::DeleteRoom { request_id, .. }
            | Self::OpenRoom { request_id, .. }
            | Self::InviteRoom { request_id, .. }
            | Self::AcceptRoom { request_id, .. }
            | Self::RejectRoom { request_id, .. }
            | Self::RemoveRoomMember { request_id, .. }
            | Self::LeaveRoom { request_id, .. }
            | Self::DiscoverConversations { request_id, .. }
            | Self::ReadConversation { request_id, .. }
            | Self::InspectProvider { request_id, .. }
            | Self::Focus { request_id, .. } => Some(request_id),
            Self::Resize { identity, .. } => Some(&identity.request_id),
            _ => None,
        }
    }
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::RenameSession { session_id, .. }
            | Self::CloseSession { session_id, .. }
            | Self::InterruptSession { session_id, .. }
            | Self::OpenRemote { session_id, .. }
            | Self::DisconnectRemote { session_id, .. }
            | Self::ShareSession { session_id, .. }
            | Self::LeaveSession { session_id, .. }
            | Self::AttachSession { session_id, .. }
            | Self::Resize { session_id, .. }
            | Self::Focus { session_id, .. }
            | Self::Blur { session_id, .. } => Some(session_id),
            _ => None,
        }
    }
    fn validate_resource_ids(&self) -> Result<()> {
        match self {
            Self::RenameSession {
                expected_runtime_incarnation_id,
                ..
            }
            | Self::CloseSession {
                expected_runtime_incarnation_id,
                ..
            }
            | Self::InterruptSession {
                expected_runtime_incarnation_id,
                ..
            }
            | Self::ShareSession {
                expected_runtime_incarnation_id,
                ..
            }
            | Self::LeaveSession {
                expected_runtime_incarnation_id,
                ..
            }
            | Self::AttachSession {
                expected_runtime_incarnation_id,
                ..
            }
            | Self::Focus {
                expected_runtime_incarnation_id,
                ..
            }
            | Self::Blur {
                expected_runtime_incarnation_id,
                ..
            } => {
                parse_id(expected_runtime_incarnation_id)?;
            }
            _ => {}
        }
        match self {
            Self::AttachSession {
                room_id: Some(room_id),
                ..
            }
            | Self::RenameRoom { room_id, .. }
            | Self::DeleteRoom { room_id, .. }
            | Self::OpenRoom { room_id, .. }
            | Self::LeaveRoom { room_id, .. } => {
                parse_id(room_id)?;
            }
            Self::InviteRoom {
                room_id, user_id, ..
            }
            | Self::RemoveRoomMember {
                room_id, user_id, ..
            } => {
                parse_id(room_id)?;
                parse_id(user_id)?;
            }
            Self::AcceptRoom { invitation_id, .. } | Self::RejectRoom { invitation_id, .. } => {
                parse_id(invitation_id)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn validate_provider(&self) -> Result<()> {
        match self {
            Self::DiscoverConversations {
                working_directory,
                cursor,
                limit,
                max_bytes,
                ..
            } => {
                if let Some(directory) = working_directory {
                    directory_input(directory)?;
                }
                if let Some(cursor) = cursor {
                    text(cursor, "cursor", 128)?;
                }
                bound(*limit, 100, "limit")?;
                bound(*max_bytes, 512 * 1024, "maxBytes")?;
            }
            Self::ReadConversation {
                working_directory,
                native_conversation_id,
                limit,
                max_bytes,
                ..
            } => {
                directory_input(working_directory)?;
                parse_id(native_conversation_id)?;
                bound(*limit, 500, "limit")?;
                bound(*max_bytes, 1024 * 1024, "maxBytes")?;
            }
            Self::InspectProvider {
                working_directory: Some(directory),
                ..
            } => directory_input(directory)?,
            _ => {}
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        if let Some(id) = self.request_id() {
            parse_id(id)?;
        }
        if let Some(id) = self.session_id() {
            parse_id(id)?;
        }
        match self {
            Self::CreateSession { name, .. }
            | Self::RenameSession { name, .. }
            | Self::CreateRoom { name, .. }
            | Self::RenameRoom { name, .. } => {
                text(name, "name", 128)?;
                text(name.trim(), "name", 128)?;
            }
            _ => {}
        }
        self.validate_resource_ids()?;
        self.validate_provider()?;
        match self {
            Self::RevokeDevice { device_id } => text(device_id, "deviceId", 1024)?,
            Self::ApproveDevice { user_code } => text(user_code, "userCode", 64)?,
            Self::RequestFriend { username, .. }
            | Self::AcceptFriend { username }
            | Self::RejectFriend { username }
            | Self::CancelFriend { username }
            | Self::RemoveFriend { username } => {
                let username = username.trim();
                if !(3..=64).contains(&username.len())
                    || !username
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
                {
                    return Err(Error::Invalid("username must contain 3 to 64 ASCII letters, digits, underscores, or hyphens".to_owned()));
                }
            }
            Self::CreateSession {
                working_dir,
                resume,
                ..
            } => {
                if let Some(directory) = working_dir {
                    directory_input(directory)?;
                }
                if let Some(resume) = resume {
                    parse_id(&resume.native_conversation_id)?;
                    if working_dir.is_none() {
                        return Err(Error::Invalid("resume requires workingDir".to_owned()));
                    }
                }
            }
            Self::ShareSession {
                user_ids,
                expected_user_ids,
                ..
            } => {
                validate_participants(user_ids)?;
                if let Some(expected) = expected_user_ids {
                    validate_participants(expected)?;
                }
            }
            Self::Resize { identity, .. } => identity.validate()?,
            Self::Focus {
                client_id,
                subscription_generation,
                ..
            }
            | Self::Blur {
                client_id,
                subscription_generation,
                ..
            } => {
                parse_id(client_id)?;
                if *subscription_generation == 0 {
                    return Err(Error::Invalid(
                        "subscriptionGeneration must be nonzero".to_owned(),
                    ));
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl ResizeIdentity {
    fn validate(&self) -> Result<()> {
        parse_id(&self.request_id)?;
        parse_id(&self.expected_runtime_incarnation_id)?;
        parse_id(&self.subscription_id)?;
        if self.subscription_generation == 0 || self.surface_generation == 0 {
            return Err(Error::Invalid(
                "terminal generations must be nonzero".to_owned(),
            ));
        }
        if !(1..=crate::terminal::MAX_TERMINAL_ROWS).contains(&self.rows)
            || !(1..=crate::terminal::MAX_TERMINAL_COLS).contains(&self.cols)
            || self.cell_width_pixels == 0
            || self.cell_height_pixels == 0
            || u32::from(self.cols).checked_mul(self.cell_width_pixels) != Some(self.width_pixels)
            || u32::from(self.rows).checked_mul(self.cell_height_pixels) != Some(self.height_pixels)
        {
            return Err(Error::Invalid("invalid terminal geometry".to_owned()));
        }
        Ok(())
    }
}

fn bound(value: Option<usize>, maximum: usize, label: &str) -> Result<()> {
    if value.is_some_and(|value| value == 0 || value > maximum) {
        return Err(Error::Invalid(format!(
            "{label} must be between 1 and {maximum}"
        )));
    }
    Ok(())
}

fn text(value: &str, label: &str, maximum: usize) -> Result<()> {
    if value.is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(Error::Invalid(format!(
            "{label} must contain 1 to {maximum} bytes without control characters"
        )));
    }
    Ok(())
}

fn directory_input(value: &str) -> Result<()> {
    text(value, "working directory", 4096)?;
    let path = std::path::Path::new(value);
    if !path.is_absolute()
        || path
            .components()
            .any(|component| component == std::path::Component::ParentDir)
    {
        return Err(Error::Invalid(
            "working directory must be absolute without parent traversal".to_owned(),
        ));
    }
    Ok(())
}

pub fn parse_id(value: &str) -> Result<Uuid> {
    let id = Uuid::parse_str(value).map_err(|_| Error::Invalid("expected a UUID".to_owned()))?;
    if id.is_nil() || id.hyphenated().to_string() != value {
        return Err(Error::Invalid(
            "expected a canonical lowercase nonzero UUID".to_owned(),
        ));
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ID: &str = "11111111-1111-4111-8111-111111111111";

    fn envelope(mut value: Value) -> Value {
        value["accountUserId"] = Value::Null;
        value["accountEpoch"] = json!(0);
        value
    }

    fn commands() -> Vec<Value> {
        let mut values = [
            "auth.login.start",
            "auth.logout",
            "auth.refresh",
            "devices.refresh",
            "devices.link.startSelf",
            "devices.link.cancelSelf",
            "friends.refresh",
            "session.list",
            "room.list",
            "shutdown",
        ]
        .into_iter()
        .map(|kind| json!({"type":kind}))
        .collect::<Vec<_>>();
        values.extend([
            json!({"type":"devices.revoke","deviceId":"native-device-name"}),
            json!({"type":"devices.link.approve","userCode":"ABCD-EFGH"}),
            json!({"type":"friends.request.send","username":"example","requestId":ID}),
            json!({"type":"session.create","requestId":ID,"name":"Terminal"}),
            json!({"type":"session.create","requestId":ID,"name":"Resume","workingDir":"/tmp/project","resume":{"provider":"claude","nativeConversationId":ID}}),
            json!({"type":"session.rename","requestId":ID,"sessionId":ID,"expectedRuntimeIncarnationId":ID,"name":"Renamed"}),
            json!({"type":"session.openRemote","requestId":ID,"sessionId":ID}),
            json!({"type":"session.disconnect","sessionId":ID}),
            json!({"type":"session.share","requestId":ID,"sessionId":ID,"expectedRuntimeIncarnationId":ID,"userIds":[ID]}),
            json!({"type":"session.attachMission","requestId":ID,"sessionId":ID,"expectedRuntimeIncarnationId":ID,"roomId":null}),
            json!({"type":"room.create","requestId":ID,"name":"Release"}),
            json!({"type":"room.rename","requestId":ID,"roomId":ID,"name":"Renamed"}),
            json!({"type":"provider.discoverConversations","requestId":ID,"provider":"claude","workingDirectory":"/tmp/project"}),
            json!({"type":"provider.readConversation","requestId":ID,"provider":"copilot","workingDirectory":"/tmp/project","nativeConversationId":ID,"beforeByte":123,"limit":2,"maxBytes":4096}),
            json!({"type":"provider.inspect","requestId":ID,"provider":"claude"}),
            json!({"type":"system.setTheme","dark":true}),
            resize(),
            json!({"type":"session.focus","requestId":ID,"sessionId":ID,"expectedRuntimeIncarnationId":ID,"clientId":ID,"subscriptionGeneration":1}),
            json!({"type":"session.blur","sessionId":ID,"expectedRuntimeIncarnationId":ID,"clientId":ID,"subscriptionGeneration":1}),
        ]);
        values.extend(
            [
                "friends.request.accept",
                "friends.request.reject",
                "friends.request.cancel",
                "friends.remove",
            ]
            .into_iter()
            .map(|kind| json!({"type":kind,"username":"example"})),
        );
        values.extend(["session.close","session.interrupt","session.leave"].into_iter().map(|kind| json!({"type":kind,"requestId":ID,"sessionId":ID,"expectedRuntimeIncarnationId":ID})));
        values.extend(
            ["room.delete", "room.open", "room.leave"]
                .into_iter()
                .map(|kind| json!({"type":kind,"requestId":ID,"roomId":ID})),
        );
        values.extend(
            ["room.invite", "room.removeMember"]
                .into_iter()
                .map(|kind| json!({"type":kind,"requestId":ID,"roomId":ID,"userId":ID})),
        );
        values.extend(
            ["room.invitation.accept", "room.invitation.reject"]
                .into_iter()
                .map(|kind| json!({"type":kind,"requestId":ID,"invitationId":ID})),
        );
        values
    }

    fn resize() -> Value {
        json!({"type":"session.resize","sessionId":ID,"requestId":ID,"expectedRuntimeIncarnationId":ID,"subscriptionId":ID,"subscriptionGeneration":1,"surfaceGeneration":1,"cols":80,"rows":24,"widthPixels":800,"heightPixels":480,"cellWidthPixels":10,"cellHeightPixels":20,"claim":true})
    }

    #[test]
    fn every_command_round_trips_and_rejects_unknown_fields() {
        let mut kinds = std::collections::BTreeSet::new();
        for sample in commands() {
            let command: CommandEnvelope =
                serde_json::from_value(envelope(sample.clone())).unwrap();
            kinds.insert(command.command.operation());
            assert_eq!(
                command.command.operation(),
                sample["type"].as_str().unwrap()
            );
            let encoded = serde_json::to_value(&command).unwrap();
            let decoded: CommandEnvelope = serde_json::from_value(encoded).unwrap();
            assert_eq!(decoded.command, command.command);
            let mut invalid = envelope(sample);
            invalid["retiredFeature"] = json!(true);
            assert!(serde_json::from_value::<CommandEnvelope>(invalid).is_err());
        }
        assert_eq!(kinds.len(), 42);
    }

    #[test]
    fn required_envelope_and_terminal_identities_are_not_defaulted() {
        for field in ["accountUserId", "accountEpoch"] {
            let mut value = envelope(json!({"type":"session.list"}));
            value.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<CommandEnvelope>(value).is_err());
        }
        for field in [
            "requestId",
            "expectedRuntimeIncarnationId",
            "subscriptionId",
            "subscriptionGeneration",
            "surfaceGeneration",
        ] {
            let mut value = resize();
            value.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<CommandEnvelope>(envelope(value)).is_err(),
                "{field}"
            );
        }
        for kind in ["session.focus", "session.blur"] {
            let value = json!({"type":kind,"requestId":ID,"sessionId":ID,"expectedRuntimeIncarnationId":ID,"clientId":ID});
            assert!(serde_json::from_value::<CommandEnvelope>(envelope(value)).is_err());
        }
    }

    #[test]
    fn malformed_identifiers_and_bounds_are_rejected() {
        for value in [
            "",
            "00000000-0000-0000-0000-000000000000",
            "11111111111141118111111111111111",
            "AAAA1111-1111-4111-8111-111111111111",
            "../elsewhere",
        ] {
            assert!(parse_id(value).is_err());
        }
        for field in [
            "requestId",
            "sessionId",
            "expectedRuntimeIncarnationId",
            "subscriptionId",
        ] {
            let mut value = resize();
            value[field] = json!("bad");
            assert!(serde_json::from_value::<CommandEnvelope>(envelope(value)).is_err());
        }
        for (field, value) in [
            ("rows", json!(0)),
            ("cols", json!(1025)),
            ("widthPixels", json!(801)),
            ("cellHeightPixels", json!(0)),
            ("subscriptionGeneration", json!(0)),
            ("surfaceGeneration", json!(0)),
        ] {
            let mut resize = resize();
            resize[field] = value;
            assert!(
                serde_json::from_value::<CommandEnvelope>(envelope(resize)).is_err(),
                "{field}"
            );
        }
        for name in [
            String::new(),
            " ".to_owned(),
            "你".repeat(43),
            "bad\nname".to_owned(),
        ] {
            assert!(
                serde_json::from_value::<CommandEnvelope>(envelope(
                    json!({"type":"room.create","requestId":ID,"name":name})
                ))
                .is_err()
            );
        }
        assert!(
            serde_json::from_value::<CommandEnvelope>(envelope(
                json!({"type":"room.create","requestId":ID,"name":"Valid","slug":"retired"})
            ))
            .is_err()
        );
        for (limit, max_bytes) in [(0, 4096), (501, 4096), (10, 1024 * 1024 + 1)] {
            assert!(serde_json::from_value::<CommandEnvelope>(envelope(json!({"type":"provider.readConversation","requestId":ID,"provider":"claude","nativeConversationId":ID,"workingDirectory":"/tmp","limit":limit,"maxBytes":max_bytes}))).is_err());
        }
    }

    #[test]
    fn close_is_the_only_terminal_termination_command() {
        let mut value = json!({"type":"session.close","requestId":ID,"sessionId":ID,"expectedRuntimeIncarnationId":ID});
        assert!(matches!(
            serde_json::from_value::<Command>(value.clone()),
            Ok(Command::CloseSession { .. })
        ));
        value["type"] = json!("session.stop");
        assert!(serde_json::from_value::<Command>(value).is_err());
    }

    #[test]
    fn retired_commands_are_absent() {
        for kind in [
            "agent.intel.semanticSend",
            "agent.intel.queryPendingPermissions",
            "claude.global.refresh",
            "session.scope",
            "session.mode",
            "session.reopen",
            "session.inputBytes",
            "room.chatPost",
            "room.taskCreate",
            "trust.reset",
        ] {
            assert!(
                serde_json::from_value::<Command>(json!({"type":kind})).is_err(),
                "{kind}"
            );
        }
    }

    #[test]
    fn native_resume_requires_directory_and_closed_identity() {
        let base = json!({"type":"session.create","requestId":ID,"name":"Resume","resume":{"provider":"claude","nativeConversationId":ID}});
        assert!(serde_json::from_value::<CommandEnvelope>(envelope(base.clone())).is_err());
        let mut value = base;
        value["workingDir"] = json!("/tmp");
        value["resume"]["permissionBypass"] = json!(true);
        assert!(serde_json::from_value::<CommandEnvelope>(envelope(value)).is_err());
    }

    #[test]
    fn closed_events_preserve_flattened_terminal_and_provider_payloads() {
        let mut resized = resize();
        resized.as_object_mut().unwrap().remove("claim");
        resized["type"] = json!("term.resizeApplied");
        resized["runtimeIncarnationId"] = json!(ID);
        let result = Event::new(None, 0, resized).unwrap();
        assert_eq!(result.kind(), "term.resizeApplied");
        let value = serde_json::to_value(result).unwrap();
        assert_eq!(value["cols"], 80);
        assert!(value.get("identity").is_none());
        let reply = json!({"type":"provider.reply","requestId":ID,"operation":"provider.inspect","result":{"provider":"claude","executable":null,"files":[],"message":null}});
        let event = Event::new(None, 0, reply.clone()).unwrap();
        let encoded = serde_json::to_value(event).unwrap();
        assert_eq!(encoded["result"], reply["result"]);
        assert!(Event::new(None,0,json!({"type":"provider.reply","requestId":ID,"operation":"provider.inspect","result":{"anything":true}})).is_err());
        assert!(Event::new(None, 0, json!({"type":"agent.intel.snapshot"})).is_err());
        assert!(Event::new(None, 0, json!({"type":"auth.finalizing","extra":true})).is_err());
    }
}
