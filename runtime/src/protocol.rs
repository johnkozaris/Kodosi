use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    Error, Result,
    agents::{Agent, AgentChoiceChange, AgentStart},
    dosi::DosiContext,
    identity::friends::parse_invite,
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

pub const VERSION: u32 = 71;
include!(concat!(env!("OUT_DIR"), "/protocol_contract.rs"));
pub const MAX_COMMAND_BYTES: usize = 2 * 1024 * 1024;
pub(crate) const OTHER_VERSION: &str = "Update Kodosi on both computers.";

#[derive(Debug, Clone, Serialize, Deserialize, specta_macros::Type)]
#[serde(rename_all = "camelCase")]
pub struct CommandEnvelope {
    #[serde(deserialize_with = "account_user")]
    pub account_user_id: Option<String>,
    pub account_epoch: u64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "computer"
    )]
    pub computer: Option<String>,
    #[serde(flatten, deserialize_with = "validated_command")]
    pub command: Command,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta_macros::Type)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Command {
    #[serde(rename = "auth.login.start")]
    Login {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        browser: Option<SignInBrowser>,
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "auth.login.finish")]
    FinishLogin {
        url: String,
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "auth.login.cancel")]
    CancelLogin {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "auth.logout")]
    Logout {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "auth.deleteAccount")]
    DeleteAccount {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "auth.deleteAccount.cancel")]
    CancelDeleteAccount {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "auth.refresh")]
    RefreshAuth {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "devices.refresh")]
    RefreshDevices {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "devices.revoke")]
    RevokeDevice {
        #[serde(rename = "deviceId")]
        device_id: String,
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "devices.link.approve")]
    ApproveDevice {
        code: String,
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "devices.link.startSelf")]
    LinkDevice {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "devices.link.cancelSelf")]
    CancelDeviceLink {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "devices.reset")]
    ResetDevices {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "devices.recovery.create")]
    CreateRecoveryKey {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "devices.recovery.use")]
    UseRecoveryKey {
        key: String,
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "friends.refresh")]
    RefreshFriends {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "friends.request.send")]
    RequestFriend {
        username: String,
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "friends.request.accept")]
    AcceptFriend {
        username: String,
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "friends.request.reject")]
    RejectFriend {
        username: String,
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "friends.request.cancel")]
    CancelFriend {
        username: String,
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "friends.remove")]
    RemoveFriend {
        username: String,
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "friends.identity.trust")]
    TrustFriend {
        username: String,
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "friends.verify")]
    VerifyFriend {
        username: String,
        invite: String,
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "friends.invite")]
    FriendInvite {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "session.list")]
    ListSessions {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "session.create")]
    CreateSession {
        #[serde(rename = "requestId")]
        request_id: String,
        name: String,
        #[serde(rename = "workingDir", default)]
        working_dir: Option<String>,
        #[serde(rename = "missionId", default)]
        mission_id: Option<String>,
        #[serde(default)]
        resume: Option<ConversationIdentity>,
        #[serde(default)]
        branch: Option<String>,
        #[serde(default)]
        worktrees: Option<String>,
        #[serde(default)]
        command: Option<String>,
        #[serde(default)]
        agent: Option<AgentStart>,
        #[serde(default)]
        prompt: Option<String>,
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
    #[serde(rename = "session.run")]
    RunInSession {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
        #[serde(default)]
        command: Option<String>,
        #[serde(default)]
        agent: Option<AgentStart>,
    },
    #[serde(rename = "session.input")]
    Input {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
        text: String,
    },
    #[serde(rename = "session.read")]
    Read {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
        #[serde(default)]
        history: bool,
    },
    #[serde(rename = "session.show")]
    Show {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    #[serde(rename = "session.visible")]
    Visible {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionIds")]
        session_ids: Vec<String>,
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
        #[serde(rename = "requestId")]
        request_id: String,
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
    #[serde(rename = "session.hide")]
    HideSession {
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
        #[serde(rename = "missionId")]
        mission_id: Option<String>,
    },
    #[serde(rename = "room.command")]
    Room {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "roomId")]
        room_id: String,
        action: crate::rooms::Action,
    },
    #[serde(rename = "mission.show")]
    ShowMission {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "missionId")]
        mission_id: String,
    },
    #[serde(rename = "mission.list")]
    ListMissions {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "mission.create")]
    CreateMission {
        #[serde(rename = "requestId")]
        request_id: String,
        name: String,
    },
    #[serde(rename = "mission.rename")]
    RenameMission {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "missionId")]
        mission_id: String,
        name: String,
    },
    #[serde(rename = "mission.delete")]
    DeleteMission {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "missionId")]
        mission_id: String,
    },
    #[serde(rename = "mission.open")]
    OpenMission {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "missionId")]
        mission_id: String,
    },
    #[serde(rename = "mission.seen")]
    SeeMission {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "missionId")]
        mission_id: String,
        sequence: u64,
    },
    #[serde(rename = "mission.invite")]
    InviteMission {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "missionId")]
        mission_id: String,
        #[serde(rename = "userId")]
        user_id: String,
    },
    #[serde(rename = "mission.invitation.accept")]
    AcceptMission {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "invitationId")]
        invitation_id: String,
    },
    #[serde(rename = "mission.invitation.reject")]
    RejectMission {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "invitationId")]
        invitation_id: String,
    },
    #[serde(rename = "mission.removeMember")]
    RemoveMissionMember {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "missionId")]
        mission_id: String,
        #[serde(rename = "userId")]
        user_id: String,
    },
    #[serde(rename = "mission.leave")]
    LeaveMission {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "missionId")]
        mission_id: String,
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
    #[serde(rename = "provider.agents")]
    InspectAgents {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "provider.agentModels")]
    InspectAgentModels {
        #[serde(rename = "requestId")]
        request_id: String,
        agent: Agent,
    },
    #[serde(rename = "provider.usage")]
    InspectUsage {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "provider.addAccount")]
    AddAgentAccount {
        #[serde(rename = "requestId")]
        request_id: String,
        agent: Agent,
        name: String,
    },
    #[serde(rename = "provider.choose")]
    ChooseAgent {
        #[serde(rename = "requestId")]
        request_id: String,
        change: AgentChoiceChange,
    },
    #[serde(rename = "folders.list")]
    ListFolders {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(default)]
        path: Option<String>,
    },
    #[serde(rename = "dosi.send")]
    DosiSend {
        #[serde(rename = "requestId")]
        request_id: String,
        text: String,
        #[serde(default)]
        context: Option<DosiContext>,
    },
    #[serde(rename = "dosi.stop")]
    DosiStop {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "dosi.unqueue")]
    DosiUnqueue {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "queuedId")]
        queued_id: String,
    },
    #[serde(rename = "dosi.answer")]
    DosiAnswer {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "itemId")]
        item_id: String,
        choices: Vec<String>,
    },
    #[serde(rename = "dosi.ask")]
    DosiAsk {
        #[serde(rename = "requestId")]
        request_id: String,
        token: String,
        question: String,
        options: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        keys: Vec<String>,
        #[serde(default)]
        multiple: bool,
        #[serde(default)]
        terminal: Option<String>,
    },
    #[serde(rename = "dosi.new")]
    DosiNew {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "dosi.setup")]
    DosiConfigure {
        #[serde(rename = "requestId")]
        request_id: String,
        change: AgentChoiceChange,
    },
    #[serde(rename = "dosi.instructions.read")]
    DosiReadInstructions {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "dosi.instructions.write")]
    DosiWriteInstructions {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(default)]
        text: Option<String>,
    },
    #[serde(rename = "dosi.history")]
    DosiHistory {
        #[serde(rename = "requestId")]
        request_id: String,
        before: String,
    },
    #[serde(rename = "session.resize")]
    Resize {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(flatten)]
        identity: ResizeIdentity,
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
        #[serde(rename = "requestId")]
        request_id: String,
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
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is a separate fact about one terminal"
)]
pub struct SessionEntry {
    #[serde(default)]
    pub attention: bool,
    #[serde(default)]
    pub connected_users: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    pub prompt: bool,
    pub repository: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program_status: Option<crate::terminal::ProgramStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<crate::terminal::TerminalContext>,
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
    #[serde(default)]
    pub hidden: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mission_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mission_name: Option<String>,
    pub shared_with: Vec<String>,
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
    Offline,
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
    #[serde(skip)]
    pub recipient: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta_macros::Type)]
#[serde(tag = "type", rename_all_fields = "camelCase", deny_unknown_fields)]
pub enum EventBody {
    #[serde(rename = "system.ready")]
    SystemReady {
        protocol_version: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        host: Option<crate::HostKind>,
    },
    #[serde(rename = "system.error")]
    SystemError { message: String },
    #[serde(rename = "system.updateRequired")]
    SystemUpdateRequired {},
    #[serde(rename = "auth.ready")]
    AuthReady {
        user_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enrolled: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        credential_store: Option<CredentialStore>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        credential_folder: Option<String>,
    },
    #[serde(rename = "auth.required")]
    AuthRequired { reason: AuthRequiredReason },
    #[serde(rename = "auth.restoring")]
    AuthRestoring {
        reason: AuthRestoringReason,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        credential_store: Option<CredentialStore>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        credential_folder: Option<String>,
    },
    #[serde(rename = "auth.device_code")]
    AuthDeviceCode {
        request_id: String,
        user_code: String,
        verification_uri: String,
    },
    #[serde(rename = "auth.browser")]
    AuthBrowser { request_id: String, url: String },
    #[serde(rename = "auth.finalizing")]
    AuthFinalizing {},
    #[serde(rename = "auth.deletion_pending")]
    AuthDeletionPending {
        request_id: String,
        confirmation_uri: String,
    },
    #[serde(rename = "auth.result")]
    AuthResult {
        request_id: String,
        operation: String,
    },
    #[serde(rename = "auth.error")]
    AuthError {
        request_id: String,
        operation: String,
        message: String,
    },
    #[serde(rename = "friends.snapshot")]
    FriendsSnapshot {
        friends: Vec<FriendEntry>,
        incoming: Vec<FriendRequestEntry>,
        outgoing: Vec<FriendRequestEntry>,
    },
    #[serde(rename = "friends.result")]
    FriendsResult {
        request_id: String,
        operation: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    #[serde(rename = "friends.error")]
    FriendsError {
        request_id: String,
        operation: String,
        message: String,
    },
    #[serde(rename = "devices.list")]
    DevicesList {
        self_device_id: String,
        local_device_enrolled: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        notice: Option<String>,
        devices: Vec<DeviceEntry>,
    },
    #[serde(rename = "devices.hosts")]
    DevicesHosts { device_ids: Vec<String> },
    #[serde(rename = "devices.link.snapshot")]
    DeviceLinksSnapshot { requests: Vec<DeviceLinkRequest> },
    #[serde(rename = "devices.link.resolved")]
    DeviceLinkResolved {
        device_label: String,
        outcome: DeviceLinkOutcome,
    },
    #[serde(rename = "devices.link.selfResolved")]
    DeviceLinkSelfResolved { outcome: DeviceLinkOutcome },
    #[serde(rename = "devices.result")]
    DevicesResult {
        request_id: String,
        operation: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
    },
    #[serde(rename = "devices.error")]
    DevicesError {
        request_id: String,
        operation: String,
        message: String,
    },
    #[serde(rename = "sessions.snapshot")]
    SessionsSnapshot { sessions: Vec<SessionEntry> },
    #[serde(rename = "session.result")]
    SessionResult {
        request_id: String,
        operation: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    #[serde(rename = "session.show")]
    SessionShow { session_id: String },
    #[serde(rename = "session.error")]
    SessionError {
        request_id: String,
        operation: String,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
    },
    #[serde(rename = "room.snapshot")]
    RoomSnapshot { room: crate::rooms::Snapshot },
    #[serde(rename = "room.result")]
    RoomResult {
        request_id: String,
        operation: String,
        room_id: String,
        item_id: Option<String>,
        #[serde(default)]
        action: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        repository_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        issues: Option<Vec<crate::rooms::Issue>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        room: Option<crate::rooms::Snapshot>,
    },
    #[serde(rename = "room.error")]
    RoomError {
        request_id: String,
        operation: String,
        message: String,
    },
    #[serde(rename = "missions.snapshot")]
    MissionsSnapshot {
        missions: Vec<MissionEntry>,
        invitations: Vec<MissionInvitationEntry>,
        #[serde(default)]
        missions_truncated: bool,
        #[serde(default)]
        invitations_truncated: bool,
    },
    #[serde(rename = "mission.result")]
    MissionResult {
        request_id: String,
        operation: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mission_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mission: Option<MissionEntry>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        members: Option<Vec<MissionMemberEntry>>,
    },
    #[serde(rename = "mission.error")]
    MissionError {
        request_id: String,
        operation: String,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mission_id: Option<String>,
    },
    #[serde(rename = "provider.result")]
    ProviderResult {
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
    #[serde(rename = "folders.result")]
    FoldersResult {
        request_id: String,
        operation: String,
        folders: FolderList,
    },
    #[serde(rename = "folders.error")]
    FoldersError {
        request_id: String,
        operation: String,
        message: String,
    },
    #[serde(rename = "mission.show")]
    MissionShow { mission_id: String },
    #[serde(rename = "dosi.snapshot")]
    DosiSnapshot { dosi: crate::dosi::DosiSnapshot },
    #[serde(rename = "dosi.item")]
    DosiItem { item: crate::dosi::DosiItem },
    #[serde(rename = "dosi.result")]
    DosiResult {
        request_id: String,
        operation: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instructions: Option<crate::dosi::DosiInstructions>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        page: Option<crate::dosi::DosiPage>,
    },
    #[serde(rename = "dosi.error")]
    DosiError {
        request_id: String,
        operation: String,
        message: String,
    },
    #[serde(rename = "term.bell")]
    TerminalBell { session_id: String },
    #[serde(rename = "term.attention")]
    TerminalAttention { session_id: String },
    #[serde(rename = "term.title")]
    TerminalTitle { session_id: String, title: String },
    #[serde(rename = "term.notification")]
    TerminalNotification {
        session_id: String,
        runtime_incarnation_id: String,
        title: String,
        body: String,
    },
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
    #[serde(rename = "provider.agents")]
    Agents(crate::agents::AgentCatalog),
    #[serde(rename = "provider.agentModels")]
    AgentModels(crate::agents::AgentModelsReply),
    #[serde(rename = "provider.usage")]
    Usage(crate::agents::AgentUsageReply),
    #[serde(rename = "provider.addAccount")]
    AddAccount(crate::agents::AgentAccount),
    #[serde(rename = "provider.choose")]
    Choice(crate::agents::AgentChoice),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta_macros::Type)]
#[serde(rename_all = "camelCase")]
pub enum SignInBrowser {
    System,
    App,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta_macros::Type)]
#[serde(rename_all = "camelCase")]
pub enum AuthRequiredReason {
    SignedOut,
    Expired,
    Cancelled,
    AccountDeleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta_macros::Type)]
#[serde(rename_all = "camelCase")]
pub enum AuthRestoringReason {
    Offline,
    Unavailable,
    UpdateRequired,
    /// The keychain or the files that keep the sign-in cannot be read now.
    Unreadable,
}

/// Where this computer keeps the sign-in and the device keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta_macros::Type)]
#[serde(rename_all = "camelCase")]
pub enum CredentialStore {
    /// The Keychain on Apple systems, the Secret Service on Linux.
    Keychain,
    /// Files that Windows data protection seals for the user.
    ProtectedFile,
    /// Files that only the user can read, on a Linux computer without a Secret Service.
    PrivateFile,
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
    pub verified: bool,
    pub identity_state: FriendIdentityState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta_macros::Type)]
#[serde(rename_all = "lowercase")]
pub enum FriendIdentityState {
    Fixed,
    Changed,
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
    pub cert_issued_at_ms: i64,
    pub recovery_key: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta_macros::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FolderList {
    pub path: String,
    pub parent: Option<String>,
    pub home: String,
    pub folders: Vec<String>,
    pub recent: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta_macros::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceLinkRequest {
    pub request_id: String,
    pub device_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta_macros::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MissionEntry {
    pub id: String,
    pub name: String,
    pub owner_user_id: String,
    #[serde(default)]
    pub unread: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta_macros::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MissionInvitationEntry {
    pub id: String,
    pub mission_id: String,
    pub mission_name: String,
    pub inviter_name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta_macros::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MissionMemberEntry {
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

pub fn control_answer(command: &Command, result: Result<()>) -> Option<Value> {
    let reason = result.err().map(|error| error.to_string());
    let applied = reason.is_none();
    let (kind, value) = match command {
        Command::Resize {
            session_id,
            identity,
        } => (
            if applied {
                "term.resizeApplied"
            } else {
                "term.resizeRejected"
            },
            serde_json::to_value(ResizeResult {
                session_id: session_id.clone(),
                runtime_incarnation_id: identity.expected_runtime_incarnation_id.clone(),
                identity: identity.clone(),
                reason,
            }),
        ),
        Command::Focus {
            request_id,
            session_id,
            expected_runtime_incarnation_id,
            client_id,
            subscription_generation,
        } => (
            if applied {
                "term.focusApplied"
            } else {
                "term.focusRejected"
            },
            serde_json::to_value(FocusResult {
                session_id: session_id.clone(),
                request_id: request_id.clone(),
                runtime_incarnation_id: expected_runtime_incarnation_id.clone(),
                subscription_id: client_id.clone(),
                subscription_generation: *subscription_generation,
                reason,
            }),
        ),
        _ => return None,
    };
    let mut value = value.ok()?;
    value["type"] = json!(kind);
    Some(value)
}

impl EventBody {
    pub fn new(value: Value) -> Result<Self> {
        serde_json::from_value(value).map_err(Error::from)
    }

    pub fn request_id(&self) -> Option<&str> {
        match self {
            Self::AuthDeviceCode { request_id, .. }
            | Self::AuthBrowser { request_id, .. }
            | Self::AuthDeletionPending { request_id, .. }
            | Self::AuthResult { request_id, .. }
            | Self::AuthError { request_id, .. }
            | Self::FriendsResult { request_id, .. }
            | Self::FriendsError { request_id, .. }
            | Self::DevicesResult { request_id, .. }
            | Self::DevicesError { request_id, .. }
            | Self::SessionResult { request_id, .. }
            | Self::SessionError { request_id, .. }
            | Self::RoomResult { request_id, .. }
            | Self::RoomError { request_id, .. }
            | Self::MissionResult { request_id, .. }
            | Self::MissionError { request_id, .. }
            | Self::ProviderResult { request_id, .. }
            | Self::ProviderError { request_id, .. }
            | Self::FoldersResult { request_id, .. }
            | Self::FoldersError { request_id, .. }
            | Self::DosiResult { request_id, .. }
            | Self::DosiError { request_id, .. } => Some(request_id),
            _ => None,
        }
    }

    pub fn is_answer(&self) -> bool {
        self.request_id().is_some() && is_final(self.kind())
    }

    pub const fn kind(&self) -> &'static str {
        match self {
            Self::SystemReady { .. } => "system.ready",
            Self::SystemError { .. } => "system.error",
            Self::SystemUpdateRequired {} => "system.updateRequired",
            Self::AuthReady { .. } => "auth.ready",
            Self::AuthRequired { .. } => "auth.required",
            Self::AuthRestoring { .. } => "auth.restoring",
            Self::AuthDeviceCode { .. } => "auth.device_code",
            Self::AuthBrowser { .. } => "auth.browser",
            Self::AuthFinalizing {} => "auth.finalizing",
            Self::AuthDeletionPending { .. } => "auth.deletion_pending",
            Self::AuthResult { .. } => "auth.result",
            Self::AuthError { .. } => "auth.error",
            Self::FriendsSnapshot { .. } => "friends.snapshot",
            Self::FriendsResult { .. } => "friends.result",
            Self::FriendsError { .. } => "friends.error",
            Self::DevicesList { .. } => "devices.list",
            Self::DevicesHosts { .. } => "devices.hosts",
            Self::DeviceLinksSnapshot { .. } => "devices.link.snapshot",
            Self::DeviceLinkResolved { .. } => "devices.link.resolved",
            Self::DeviceLinkSelfResolved { .. } => "devices.link.selfResolved",
            Self::DevicesResult { .. } => "devices.result",
            Self::DevicesError { .. } => "devices.error",
            Self::SessionsSnapshot { .. } => "sessions.snapshot",
            Self::SessionShow { .. } => "session.show",
            Self::SessionResult { .. } => "session.result",
            Self::SessionError { .. } => "session.error",
            Self::RoomSnapshot { .. } => "room.snapshot",
            Self::RoomResult { .. } => "room.result",
            Self::RoomError { .. } => "room.error",
            Self::MissionsSnapshot { .. } => "missions.snapshot",
            Self::MissionResult { .. } => "mission.result",
            Self::MissionError { .. } => "mission.error",
            Self::ProviderResult { .. } => "provider.result",
            Self::ProviderError { .. } => "provider.error",
            Self::FoldersResult { .. } => "folders.result",
            Self::FoldersError { .. } => "folders.error",
            Self::MissionShow { .. } => "mission.show",
            Self::DosiSnapshot { .. } => "dosi.snapshot",
            Self::DosiItem { .. } => "dosi.item",
            Self::DosiResult { .. } => "dosi.result",
            Self::DosiError { .. } => "dosi.error",
            Self::TerminalBell { .. } => "term.bell",
            Self::TerminalAttention { .. } => "term.attention",
            Self::TerminalTitle { .. } => "term.title",
            Self::TerminalNotification { .. } => "term.notification",
        }
    }
}

impl Event {
    pub fn new(user: Option<String>, epoch: u64, value: Value) -> Result<Self> {
        Ok(Self {
            account_user_id: user,
            account_epoch: epoch,
            event: EventBody::new(value)?,
            recipient: None,
        })
    }

    pub fn reaches(&self, consumer: Uuid) -> bool {
        self.recipient.is_none_or(|recipient| recipient == consumer)
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

fn computer<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<String>, D::Error> {
    let device = Option::<String>::deserialize(deserializer)?;
    if let Some(device) = &device {
        text(device, "computer", 1024).map_err(serde::de::Error::custom)?;
    }
    Ok(device)
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
            Self::Login { .. } => "auth.login.start",
            Self::FinishLogin { .. } => "auth.login.finish",
            Self::CancelLogin { .. } => "auth.login.cancel",
            Self::Logout { .. } => "auth.logout",
            Self::DeleteAccount { .. } => "auth.deleteAccount",
            Self::CancelDeleteAccount { .. } => "auth.deleteAccount.cancel",
            Self::RefreshAuth { .. } => "auth.refresh",
            Self::RefreshDevices { .. } => "devices.refresh",
            Self::RevokeDevice { .. } => "devices.revoke",
            Self::ApproveDevice { .. } => "devices.link.approve",
            Self::LinkDevice { .. } => "devices.link.startSelf",
            Self::CancelDeviceLink { .. } => "devices.link.cancelSelf",
            Self::ResetDevices { .. } => "devices.reset",
            Self::CreateRecoveryKey { .. } => "devices.recovery.create",
            Self::UseRecoveryKey { .. } => "devices.recovery.use",
            Self::RefreshFriends { .. } => "friends.refresh",
            Self::RequestFriend { .. } => "friends.request.send",
            Self::AcceptFriend { .. } => "friends.request.accept",
            Self::RejectFriend { .. } => "friends.request.reject",
            Self::CancelFriend { .. } => "friends.request.cancel",
            Self::RemoveFriend { .. } => "friends.remove",
            Self::TrustFriend { .. } => "friends.identity.trust",
            Self::VerifyFriend { .. } => "friends.verify",
            Self::FriendInvite { .. } => "friends.invite",
            Self::ListSessions { .. } => "session.list",
            Self::CreateSession { .. } => "session.create",
            Self::RenameSession { .. } => "session.rename",
            Self::CloseSession { .. } => "session.close",
            Self::InterruptSession { .. } => "session.interrupt",
            Self::RunInSession { .. } => "session.run",
            Self::Input { .. } => "session.input",
            Self::Visible { .. } => "session.visible",
            Self::Read { .. } => "session.read",
            Self::Show { .. } => "session.show",
            Self::OpenRemote { .. } => "session.openRemote",
            Self::DisconnectRemote { .. } => "session.disconnect",
            Self::ShareSession { .. } => "session.share",
            Self::LeaveSession { .. } => "session.leave",
            Self::HideSession { .. } => "session.hide",
            Self::AttachSession { .. } => "session.attachMission",
            Self::Room { .. } => "room.command",
            Self::ShowMission { .. } => "mission.show",
            Self::ListMissions { .. } => "mission.list",
            Self::CreateMission { .. } => "mission.create",
            Self::RenameMission { .. } => "mission.rename",
            Self::DeleteMission { .. } => "mission.delete",
            Self::SeeMission { .. } => "mission.seen",
            Self::OpenMission { .. } => "mission.open",
            Self::InviteMission { .. } => "mission.invite",
            Self::AcceptMission { .. } => "mission.invitation.accept",
            Self::RejectMission { .. } => "mission.invitation.reject",
            Self::RemoveMissionMember { .. } => "mission.removeMember",
            Self::LeaveMission { .. } => "mission.leave",
            Self::DiscoverConversations { .. } => "provider.discoverConversations",
            Self::ReadConversation { .. } => "provider.readConversation",
            Self::InspectProvider { .. } => "provider.inspect",
            Self::InspectAgents { .. } => "provider.agents",
            Self::InspectAgentModels { .. } => "provider.agentModels",
            Self::InspectUsage { .. } => "provider.usage",
            Self::AddAgentAccount { .. } => "provider.addAccount",
            Self::ChooseAgent { .. } => "provider.choose",
            Self::ListFolders { .. } => "folders.list",
            Self::DosiSend { .. } => "dosi.send",
            Self::DosiStop { .. } => "dosi.stop",
            Self::DosiUnqueue { .. } => "dosi.unqueue",
            Self::DosiAnswer { .. } => "dosi.answer",
            Self::DosiAsk { .. } => "dosi.ask",
            Self::DosiNew { .. } => "dosi.new",
            Self::DosiConfigure { .. } => "dosi.setup",
            Self::DosiReadInstructions { .. } => "dosi.instructions.read",
            Self::DosiWriteInstructions { .. } => "dosi.instructions.write",
            Self::DosiHistory { .. } => "dosi.history",
            Self::Resize { .. } => "session.resize",
            Self::Focus { .. } => "session.focus",
            Self::Blur { .. } => "session.blur",
        }
    }
    pub const fn is_dosi(&self) -> bool {
        matches!(
            self,
            Self::DosiSend { .. }
                | Self::DosiStop { .. }
                | Self::DosiUnqueue { .. }
                | Self::DosiAnswer { .. }
                | Self::DosiAsk { .. }
                | Self::DosiNew { .. }
                | Self::DosiConfigure { .. }
                | Self::DosiReadInstructions { .. }
                | Self::DosiWriteInstructions { .. }
                | Self::DosiHistory { .. }
        )
    }

    pub const fn runs_on_other_computers(&self) -> bool {
        matches!(
            self,
            Self::InspectAgents { .. }
                | Self::InspectAgentModels { .. }
                | Self::InspectUsage { .. }
                | Self::AddAgentAccount { .. }
                | Self::ChooseAgent { .. }
                | Self::ListFolders { .. }
                | Self::CreateSession { resume: None, .. }
        )
    }

    pub fn request_id(&self) -> &str {
        match self {
            Self::Resize { identity, .. } => &identity.request_id,
            Self::Login { request_id, .. }
            | Self::FinishLogin { request_id, .. }
            | Self::CancelLogin { request_id }
            | Self::Logout { request_id }
            | Self::DeleteAccount { request_id }
            | Self::CancelDeleteAccount { request_id }
            | Self::RefreshAuth { request_id }
            | Self::RefreshDevices { request_id }
            | Self::RevokeDevice { request_id, .. }
            | Self::ApproveDevice { request_id, .. }
            | Self::LinkDevice { request_id }
            | Self::CancelDeviceLink { request_id }
            | Self::ResetDevices { request_id }
            | Self::CreateRecoveryKey { request_id }
            | Self::UseRecoveryKey { request_id, .. }
            | Self::RefreshFriends { request_id }
            | Self::RequestFriend { request_id, .. }
            | Self::AcceptFriend { request_id, .. }
            | Self::RejectFriend { request_id, .. }
            | Self::CancelFriend { request_id, .. }
            | Self::RemoveFriend { request_id, .. }
            | Self::TrustFriend { request_id, .. }
            | Self::VerifyFriend { request_id, .. }
            | Self::FriendInvite { request_id }
            | Self::ListSessions { request_id }
            | Self::CreateSession { request_id, .. }
            | Self::RenameSession { request_id, .. }
            | Self::CloseSession { request_id, .. }
            | Self::InterruptSession { request_id, .. }
            | Self::RunInSession { request_id, .. }
            | Self::Input { request_id, .. }
            | Self::Visible { request_id, .. }
            | Self::Read { request_id, .. }
            | Self::Show { request_id, .. }
            | Self::OpenRemote { request_id, .. }
            | Self::DisconnectRemote { request_id, .. }
            | Self::ShareSession { request_id, .. }
            | Self::LeaveSession { request_id, .. }
            | Self::HideSession { request_id, .. }
            | Self::AttachSession { request_id, .. }
            | Self::Room { request_id, .. }
            | Self::ListMissions { request_id }
            | Self::ShowMission { request_id, .. }
            | Self::CreateMission { request_id, .. }
            | Self::RenameMission { request_id, .. }
            | Self::DeleteMission { request_id, .. }
            | Self::SeeMission { request_id, .. }
            | Self::OpenMission { request_id, .. }
            | Self::InviteMission { request_id, .. }
            | Self::AcceptMission { request_id, .. }
            | Self::RejectMission { request_id, .. }
            | Self::RemoveMissionMember { request_id, .. }
            | Self::LeaveMission { request_id, .. }
            | Self::DiscoverConversations { request_id, .. }
            | Self::ReadConversation { request_id, .. }
            | Self::InspectProvider { request_id, .. }
            | Self::InspectAgents { request_id }
            | Self::InspectAgentModels { request_id, .. }
            | Self::InspectUsage { request_id }
            | Self::AddAgentAccount { request_id, .. }
            | Self::ChooseAgent { request_id, .. }
            | Self::ListFolders { request_id, .. }
            | Self::DosiSend { request_id, .. }
            | Self::DosiStop { request_id }
            | Self::DosiUnqueue { request_id, .. }
            | Self::DosiAnswer { request_id, .. }
            | Self::DosiAsk { request_id, .. }
            | Self::DosiNew { request_id }
            | Self::DosiConfigure { request_id, .. }
            | Self::DosiReadInstructions { request_id }
            | Self::DosiWriteInstructions { request_id, .. }
            | Self::DosiHistory { request_id, .. }
            | Self::Focus { request_id, .. }
            | Self::Blur { request_id, .. } => request_id,
        }
    }
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::RenameSession { session_id, .. }
            | Self::CloseSession { session_id, .. }
            | Self::InterruptSession { session_id, .. }
            | Self::RunInSession { session_id, .. }
            | Self::Input { session_id, .. }
            | Self::Read { session_id, .. }
            | Self::Show { session_id, .. }
            | Self::OpenRemote { session_id, .. }
            | Self::DisconnectRemote { session_id, .. }
            | Self::ShareSession { session_id, .. }
            | Self::LeaveSession { session_id, .. }
            | Self::HideSession { session_id, .. }
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
            | Self::RunInSession {
                expected_runtime_incarnation_id,
                ..
            }
            | Self::Input {
                expected_runtime_incarnation_id,
                ..
            }
            | Self::Read {
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
            | Self::HideSession {
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
                mission_id: Some(mission_id),
                ..
            }
            | Self::RenameMission { mission_id, .. }
            | Self::DeleteMission { mission_id, .. }
            | Self::OpenMission { mission_id, .. }
            | Self::ShowMission { mission_id, .. }
            | Self::SeeMission { mission_id, .. }
            | Self::LeaveMission { mission_id, .. } => {
                parse_id(mission_id)?;
            }
            Self::InviteMission {
                mission_id,
                user_id,
                ..
            }
            | Self::RemoveMissionMember {
                mission_id,
                user_id,
                ..
            } => {
                parse_id(mission_id)?;
                parse_id(user_id)?;
            }
            Self::AcceptMission { invitation_id, .. }
            | Self::RejectMission { invitation_id, .. } => {
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
            }
            | Self::ListFolders {
                path: Some(directory),
                ..
            } => directory_input(directory)?,
            _ => {}
        }
        Ok(())
    }

    fn validate_dosi(&self) -> Result<()> {
        use crate::dosi::{
            MAX_ANSWER_BYTES, MAX_INSTRUCTIONS_BYTES, MAX_MESSAGE_BYTES, MAX_OPTION_BYTES,
            MAX_OPTIONS, MAX_QUESTION_BYTES,
        };
        match self {
            Self::DosiSend { text, context, .. } => {
                writing(text, "message", MAX_MESSAGE_BYTES)?;
                if let Some(context) = context {
                    for id in [&context.terminal, &context.room].into_iter().flatten() {
                        parse_id(id)?;
                    }
                }
            }
            Self::DosiUnqueue { queued_id: id, .. } | Self::DosiHistory { before: id, .. } => {
                parse_id(id)?;
            }
            Self::DosiAnswer {
                item_id, choices, ..
            } => {
                parse_id(item_id)?;
                if choices.is_empty() || choices.len() > MAX_OPTIONS + 1 {
                    return Err(Error::Invalid(
                        "Choose one to eight answers, and add at most one of your own.".to_owned(),
                    ));
                }
                for choice in choices {
                    text(choice, "answer", MAX_ANSWER_BYTES)?;
                    if choice.trim().is_empty() {
                        return Err(Error::Invalid("An answer must have words.".to_owned()));
                    }
                }
            }
            Self::DosiAsk {
                token,
                question,
                options,
                keys,
                terminal,
                ..
            } => {
                parse_id(token)?;
                text(question, "question", MAX_QUESTION_BYTES)?;
                if options.is_empty() || options.len() > MAX_OPTIONS {
                    return Err(Error::Invalid("Give one to eight choices.".to_owned()));
                }
                let mut seen = std::collections::BTreeSet::new();
                for option in options {
                    text(option, "choice", MAX_OPTION_BYTES)?;
                    if !seen.insert(option) {
                        return Err(Error::Invalid("Each choice must be different.".to_owned()));
                    }
                }
                if !keys.is_empty() && (terminal.is_none() || keys.len() != options.len()) {
                    return Err(Error::Invalid(
                        "Give the keys of every choice, and the terminal they go to.".to_owned(),
                    ));
                }
                for typed in keys {
                    if typed.len() > MAX_OPTION_BYTES || typed.chars().any(char::is_control) {
                        return Err(Error::Invalid(format!(
                            "keys must contain at most {MAX_OPTION_BYTES} bytes without control characters"
                        )));
                    }
                }
                if let Some(terminal) = terminal {
                    text(terminal, "terminal", 128)?;
                }
            }
            Self::DosiWriteInstructions {
                text: Some(instructions),
                ..
            } if instructions.trim().is_empty() => {}
            Self::DosiWriteInstructions {
                text: Some(instructions),
                ..
            } => writing(instructions, "instructions", MAX_INSTRUCTIONS_BYTES)?,
            _ => {}
        }
        Ok(())
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one typed command validation dispatch"
    )]
    pub fn validate(&self) -> Result<()> {
        if let Self::Room {
            room_id, action, ..
        } = self
        {
            parse_id(room_id)?;
            action.validate()?;
        }
        parse_id(self.request_id())?;
        if let Some(id) = self.session_id() {
            parse_id(id)?;
        }
        match self {
            Self::CreateSession { name, .. }
            | Self::RenameSession { name, .. }
            | Self::CreateMission { name, .. }
            | Self::RenameMission { name, .. } => {
                text(name, "name", 128)?;
                text(name.trim(), "name", 128)?;
            }
            Self::RunInSession { command, agent, .. } => {
                if let Some(command) = command {
                    text(command, "command", 1024)?;
                    text(command.trim(), "command", 1024)?;
                }
                if let Some(agent) = agent {
                    agent_start(agent)?;
                }
            }
            Self::Visible { session_ids, .. } => {
                if session_ids.len() > 256 {
                    return Err(Error::Invalid(
                        "Name at most 256 visible terminals.".to_owned(),
                    ));
                }
                for id in session_ids {
                    parse_id(id)?;
                }
            }
            Self::Input { text, .. }
                if text.is_empty() || text.len() > crate::terminal::TERMINAL_INPUT_MAX_BYTES =>
            {
                return Err(Error::Invalid(
                    "Input must contain 1 byte to 1 MiB.".to_owned(),
                ));
            }
            Self::AddAgentAccount { name, .. } => text(name, "name", 64)?,
            Self::ChooseAgent {
                change: AgentChoiceChange::Pick(pick),
                ..
            }
            | Self::DosiConfigure {
                change: AgentChoiceChange::Pick(pick),
                ..
            } => {
                for value in [&pick.account, &pick.model, &pick.effort]
                    .into_iter()
                    .flatten()
                {
                    text(value, "choice", 128)?;
                }
            }
            _ => {}
        }
        self.validate_resource_ids()?;
        self.validate_provider()?;
        self.validate_dosi()?;
        match self {
            Self::RevokeDevice { device_id, .. } => text(device_id, "deviceId", 1024)?,
            Self::FinishLogin { url, .. } => text(url, "url", 8192)?,
            Self::ApproveDevice { code, .. } => text(code, "code", 64)?,
            Self::RequestFriend { username, .. } if parse_invite(username).is_some() => {}
            Self::RequestFriend { username, .. }
            | Self::AcceptFriend { username, .. }
            | Self::RejectFriend { username, .. }
            | Self::CancelFriend { username, .. }
            | Self::RemoveFriend { username, .. }
            | Self::TrustFriend { username, .. }
            | Self::VerifyFriend { username, .. } => {
                let username = username.trim();
                if !(3..=64).contains(&username.len())
                    || !username
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
                {
                    return Err(Error::Invalid("username must contain 3 to 64 ASCII letters, digits, underscores, or hyphens".to_owned()));
                }
                if let Self::VerifyFriend { invite, .. } = self
                    && parse_invite(invite).is_none()
                {
                    return Err(Error::Invalid(
                        "invite must be the invite text of a friend".to_owned(),
                    ));
                }
            }
            Self::CreateSession {
                working_dir,
                mission_id,
                resume,
                branch,
                worktrees,
                command,
                agent,
                prompt,
                ..
            } => {
                if let Some(id) = mission_id {
                    parse_id(id)?;
                }
                if let Some(command) = command {
                    text(command, "command", 1024)?;
                    text(command.trim(), "command", 1024)?;
                }
                if let Some(agent) = agent {
                    agent_start(agent)?;
                }
                if let Some(prompt) = prompt {
                    if command.is_none() && agent.is_none() {
                        return Err(Error::Invalid(
                            "A prompt needs an agent or a command.".to_owned(),
                        ));
                    }
                    text(prompt, "prompt", 4096)?;
                    text(prompt.trim(), "prompt", 4096)?;
                }
                if let Some(directory) = working_dir {
                    directory_input(directory)?;
                }
                if let Some(directory) = worktrees {
                    directory_input(directory)?;
                }
                if let Some(branch) = branch {
                    crate::worktree::branch_name(branch)?;
                    if working_dir.is_none() {
                        return Err(Error::Invalid(
                            "A new branch needs the folder of its repository.".to_owned(),
                        ));
                    }
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
        self.geometry().map(|_| ())
    }

    pub(crate) fn geometry(
        &self,
    ) -> Result<(
        crate::terminal::TerminalSize,
        crate::terminal::TerminalPixelGeometry,
    )> {
        let size = crate::terminal::TerminalSize::new(self.rows, self.cols)
            .map_err(|error| Error::Invalid(error.to_string()))?;
        let geometry = crate::terminal::TerminalPixelGeometry::new(
            self.width_pixels,
            self.height_pixels,
            self.cell_width_pixels,
            self.cell_height_pixels,
        )
        .and_then(|geometry| geometry.validate_for_size(size))
        .map_err(|error| Error::Invalid(error.to_string()))?;
        Ok((size, geometry))
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

fn writing(value: &str, label: &str, maximum: usize) -> Result<()> {
    if value.trim().is_empty()
        || value.len() > maximum
        || value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(Error::Invalid(format!(
            "{label} must contain 1 to {maximum} bytes of text"
        )));
    }
    Ok(())
}

fn agent_start(start: &AgentStart) -> Result<()> {
    if let Some(account) = &start.account {
        text(account, "account", 128)?;
    }
    for value in [&start.model, &start.effort].into_iter().flatten() {
        if !value.is_empty() {
            text(value, "choice", 128)?;
        }
    }
    Ok(())
}

fn directory_input(value: &str) -> Result<()> {
    text(value, "working directory", 4096)?;
    let bytes = value.as_bytes();
    let absolute = value.starts_with('/')
        || value.starts_with(r"\\")
        || (bytes.len() > 2
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/'));
    if !absolute || value.split(['/', '\\']).any(|part| part == "..") {
        return Err(Error::Invalid(
            "working directory must be absolute without parent traversal".to_owned(),
        ));
    }
    Ok(())
}

pub fn answer_time(operation: &str) -> std::time::Duration {
    match operation {
        "auth.login.start" => crate::network::SIGN_IN_LIFE + std::time::Duration::from_mins(1),
        "auth.deleteAccount" => crate::network::DELETION_LIFE + std::time::Duration::from_mins(1),
        "room.command" => std::time::Duration::from_mins(1),
        _ => std::time::Duration::from_secs(30),
    }
}

pub fn is_final(kind: &str) -> bool {
    matches!(kind.rsplit_once('.'), Some((_, "result" | "error")))
}

pub fn answer(event: &Value, request: &str) -> Option<std::result::Result<Value, String>> {
    if event.get("requestId").and_then(Value::as_str) != Some(request) {
        return None;
    }
    match event
        .get("type")
        .and_then(Value::as_str)?
        .rsplit_once('.')?
        .1
    {
        "result" => Some(Ok(event.clone())),
        "error" => Some(Err(event
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("That did not work.")
            .to_owned())),
        _ => None,
    }
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
    #[cfg(unix)]
    const DIRECTORY: &str = "/tmp/project";
    #[cfg(windows)]
    const DIRECTORY: &str = r"C:\project";

    fn envelope(mut value: Value) -> Value {
        value["accountUserId"] = Value::Null;
        value["accountEpoch"] = json!(0);
        value
    }

    fn commands() -> Vec<Value> {
        let mut values = [
            "auth.login.start",
            "auth.login.cancel",
            "auth.logout",
            "auth.deleteAccount",
            "auth.deleteAccount.cancel",
            "auth.refresh",
            "devices.refresh",
            "devices.reset",
            "devices.recovery.create",
            "devices.link.startSelf",
            "devices.link.cancelSelf",
            "friends.refresh",
            "friends.invite",
            "session.list",
            "mission.list",
        ]
        .into_iter()
        .map(|kind| json!({"type":kind,"requestId":ID}))
        .collect::<Vec<_>>();
        values.extend([
            json!({"type":"auth.login.start","requestId":ID,"browser":"system"}),
            json!({"type":"auth.login.start","requestId":ID,"browser":"app"}),
            json!({"type":"auth.login.finish","requestId":ID,"url":"com.kodosi:/auth?code=c&state=s"}),
            json!({"type":"devices.revoke","requestId":ID,"deviceId":"native-device-name"}),
            json!({"type":"devices.link.approve","requestId":ID,"code":"ABCD-EFGH-JKMN"}),
            json!({"type":"devices.recovery.use","requestId":ID,"key":"ABCD-EFGH-JKMN-PQRS-TVWX-YZ01-2345-6789"}),
            json!({"type":"friends.request.send","username":"example","requestId":ID}),
            json!({"type":"session.create","requestId":ID,"name":"Terminal"}),
            json!({"type":"session.create","requestId":ID,"name":"Resume","workingDir":DIRECTORY,"resume":{"provider":"claude","nativeConversationId":ID}}),
            json!({"type":"session.rename","requestId":ID,"sessionId":ID,"expectedRuntimeIncarnationId":ID,"name":"Renamed"}),
            json!({"type":"session.openRemote","requestId":ID,"sessionId":ID}),
            json!({"type":"session.disconnect","requestId":ID,"sessionId":ID}),
            json!({"type":"session.share","requestId":ID,"sessionId":ID,"expectedRuntimeIncarnationId":ID,"userIds":[ID]}),
            json!({"type":"session.attachMission","requestId":ID,"sessionId":ID,"expectedRuntimeIncarnationId":ID,"missionId":null}),
            json!({"type":"session.hide","requestId":ID,"sessionId":ID,"expectedRuntimeIncarnationId":ID}),
            json!({"type":"mission.create","requestId":ID,"name":"Release"}),
            json!({"type":"mission.rename","requestId":ID,"missionId":ID,"name":"Renamed"}),
            json!({"type":"provider.discoverConversations","requestId":ID,"provider":"claude","workingDirectory":DIRECTORY}),
            json!({"type":"provider.readConversation","requestId":ID,"provider":"copilot","workingDirectory":DIRECTORY,"nativeConversationId":ID,"beforeByte":123,"limit":2,"maxBytes":4096}),
            json!({"type":"provider.inspect","requestId":ID,"provider":"claude"}),
            json!({"type":"folders.list","requestId":ID,"path":DIRECTORY}),
            resize(),
            json!({"type":"mission.show","requestId":ID,"missionId":ID}),
            json!({"type":"dosi.send","requestId":ID,"text":"What needs me?\nAnd why?","context":{"terminal":ID,"room":null}}),
            json!({"type":"dosi.stop","requestId":ID}),
            json!({"type":"dosi.unqueue","requestId":ID,"queuedId":ID}),
            json!({"type":"dosi.answer","requestId":ID,"itemId":ID,"choices":["work"]}),
            json!({"type":"dosi.ask","requestId":ID,"token":ID,"question":"Overwrite the notes?","options":["Yes","No"],"keys":["y","n"],"multiple":false,"terminal":"Release notes"}),
            json!({"type":"dosi.new","requestId":ID}),
            json!({"type":"dosi.setup","requestId":ID,"change":{"type":"pick","agent":"claude","account":"default","model":"opus","effort":"high"}}),
            json!({"type":"dosi.setup","requestId":ID,"change":{"type":"tab","agent":"codex"}}),
            json!({"type":"dosi.instructions.read","requestId":ID}),
            json!({"type":"dosi.instructions.write","requestId":ID,"text":"# Dosi\n\nBe brief."}),
            json!({"type":"dosi.history","requestId":ID,"before":ID}),
            json!({"type":"session.focus","requestId":ID,"sessionId":ID,"expectedRuntimeIncarnationId":ID,"clientId":ID,"subscriptionGeneration":1}),
            json!({"type":"session.blur","requestId":ID,"sessionId":ID,"expectedRuntimeIncarnationId":ID,"clientId":ID,"subscriptionGeneration":1}),
        ]);
        values.extend(
            [
                "friends.request.accept",
                "friends.request.reject",
                "friends.request.cancel",
                "friends.remove",
                "friends.identity.trust",
            ]
            .into_iter()
            .map(|kind| json!({"type":kind,"requestId":ID,"username":"example"})),
        );
        let invite = format!("kodosi:example:{}", "ab".repeat(32));
        values.extend([
            json!({"type":"friends.request.send","username":invite,"requestId":ID}),
            json!({"type":"friends.verify","requestId":ID,"username":"example","invite":invite}),
        ]);
        values.extend(["session.close","session.interrupt","session.leave"].into_iter().map(|kind| json!({"type":kind,"requestId":ID,"sessionId":ID,"expectedRuntimeIncarnationId":ID})));
        values.extend(
            ["mission.delete", "mission.open", "mission.leave"]
                .into_iter()
                .map(|kind| json!({"type":kind,"requestId":ID,"missionId":ID})),
        );
        values.extend(
            ["mission.invite", "mission.removeMember"]
                .into_iter()
                .map(|kind| json!({"type":kind,"requestId":ID,"missionId":ID,"userId":ID})),
        );
        values.extend(
            ["mission.invitation.accept", "mission.invitation.reject"]
                .into_iter()
                .map(|kind| json!({"type":kind,"requestId":ID,"invitationId":ID})),
        );
        values
    }

    fn resize() -> Value {
        json!({"type":"session.resize","sessionId":ID,"requestId":ID,"expectedRuntimeIncarnationId":ID,"subscriptionId":ID,"subscriptionGeneration":1,"surfaceGeneration":1,"cols":80,"rows":24,"widthPixels":800,"heightPixels":480,"cellWidthPixels":10,"cellHeightPixels":20})
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
            invalid["unknownField"] = json!(true);
            assert!(serde_json::from_value::<CommandEnvelope>(invalid).is_err());
        }
        assert_eq!(kinds.len(), 63);
    }

    #[test]
    fn a_command_names_another_computer_and_any_platform_folder() {
        let create = |computer: Value, folder: &str| {
            let mut value = envelope(
                json!({"type":"session.create","requestId":ID,"name":"Terminal","workingDir":folder}),
            );
            value["computer"] = computer;
            serde_json::from_value::<CommandEnvelope>(value)
        };
        let command = create(json!("device-2"), r"C:\Users\ana\Repos").unwrap();
        assert_eq!(command.computer.as_deref(), Some("device-2"));
        assert!(command.command.runs_on_other_computers());
        assert_eq!(
            serde_json::to_value(&command).unwrap()["computer"],
            "device-2"
        );
        assert!(create(Value::Null, "/home/ana/Repos").is_ok());
        for folder in ["Repos", r"C:Repos", "/home/ana/../bob", r"C:\Users\..\bob"] {
            assert!(create(json!("device-2"), folder).is_err(), "{folder}");
        }
        assert!(create(json!(""), "/home/ana").is_err());
        assert!(create(json!("bad\ndevice"), "/home/ana").is_err());
    }

    #[test]
    fn dosi_questions_and_messages_stay_within_their_bounds() {
        let ask = |options: Value| {
            serde_json::from_value::<CommandEnvelope>(envelope(
                json!({"type":"dosi.ask","requestId":ID,"token":ID,"question":"Which?","options":options}),
            ))
        };
        assert!(ask(json!(["a", "b"])).is_ok());
        assert!(ask(json!([])).is_err());
        assert!(ask(json!(["a", "a"])).is_err());
        assert!(ask(json!((0..9).map(|n| n.to_string()).collect::<Vec<_>>())).is_err());
        assert!(ask(json!(["new\nline"])).is_err());
        let keyed = |keys: Value, terminal: Value| {
            serde_json::from_value::<CommandEnvelope>(envelope(
                json!({"type":"dosi.ask","requestId":ID,"token":ID,"question":"Overwrite?","options":["Yes","No"],"keys":keys,"terminal":terminal}),
            ))
        };
        assert!(keyed(json!(["y", ""]), json!("Release notes")).is_ok());
        assert!(keyed(json!(["y"]), json!("Release notes")).is_err());
        assert!(keyed(json!(["y", "n"]), json!(null)).is_err());
        assert!(keyed(json!(["y", "\u{1b}"]), json!("Release notes")).is_err());
        let answer = |choices: Value| {
            serde_json::from_value::<CommandEnvelope>(envelope(
                json!({"type":"dosi.answer","requestId":ID,"itemId":ID,"choices":choices}),
            ))
        };
        assert!(answer(json!(["Yes", "Use the other file first"])).is_ok());
        assert!(answer(json!(["  "])).is_err());
        assert!(answer(json!([])).is_err());
        for text in [
            "",
            "  \n ",
            "bell\u{7}",
            &"x".repeat(crate::dosi::MAX_MESSAGE_BYTES + 1),
        ] {
            assert!(
                serde_json::from_value::<CommandEnvelope>(envelope(
                    json!({"type":"dosi.send","requestId":ID,"text":text})
                ))
                .is_err()
            );
        }
    }

    #[test]
    fn every_command_carries_the_request_identity_that_its_error_names() {
        for mut sample in commands() {
            sample["requestId"] = json!(ID);
            let command: CommandEnvelope =
                serde_json::from_value(envelope(sample.clone())).unwrap();
            assert_eq!(command.command.request_id(), ID, "{sample}");
            sample["requestId"] = json!("bad");
            assert!(serde_json::from_value::<CommandEnvelope>(envelope(sample)).is_err());
        }
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
                    json!({"type":"mission.create","requestId":ID,"name":name})
                ))
                .is_err()
            );
        }
        for (limit, max_bytes) in [(0, 4096), (501, 4096), (10, 1024 * 1024 + 1)] {
            assert!(serde_json::from_value::<CommandEnvelope>(envelope(json!({"type":"provider.readConversation","requestId":ID,"provider":"claude","nativeConversationId":ID,"workingDirectory":DIRECTORY,"limit":limit,"maxBytes":max_bytes}))).is_err());
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
    fn native_resume_requires_directory_and_closed_identity() {
        let base = json!({"type":"session.create","requestId":ID,"name":"Resume","resume":{"provider":"claude","nativeConversationId":ID}});
        assert!(serde_json::from_value::<CommandEnvelope>(envelope(base.clone())).is_err());
        let mut value = base;
        value["workingDir"] = json!(DIRECTORY);
        value["resume"]["permissionBypass"] = json!(true);
        assert!(serde_json::from_value::<CommandEnvelope>(envelope(value)).is_err());
    }

    #[test]
    fn closed_events_preserve_flattened_provider_payloads() {
        let reply = json!({"type":"provider.result","requestId":ID,"operation":"provider.inspect","result":{"provider":"claude","executable":null,"files":[],"message":null}});
        let event = Event::new(None, 0, reply.clone()).unwrap();
        assert!(event.event.is_answer());
        let encoded = serde_json::to_value(event).unwrap();
        assert_eq!(encoded["result"], reply["result"]);
        assert!(Event::new(None,0,json!({"type":"provider.result","requestId":ID,"operation":"provider.inspect","result":{"anything":true}})).is_err());
        assert!(Event::new(None, 0, json!({"type":"unknown.event"})).is_err());
        assert!(Event::new(None, 0, json!({"type":"auth.finalizing","extra":true})).is_err());
    }
}
