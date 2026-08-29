use std::fmt;

use serde::{Deserialize, Serialize};
#[cfg(feature = "cli")]
use serde::{Deserializer, de};

#[cfg(feature = "cli")]
use super::{
    AuthCommand, DeviceCommand, FriendsCommand, RoomCommand, SessionCommand, TerminalCommand,
    TrustCommand,
};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum SystemCommand {
    #[serde(rename = "shutdown")]
    Shutdown,
    #[serde(rename = "claude.global.refresh")]
    RefreshClaudeGlobal {
        cwd: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        generation: Option<u64>,
    },

    #[serde(rename = "system.setTheme")]
    SetHostTheme { dark: bool },
    #[serde(rename = "agent.intel.queryPendingPermissions")]
    QueryPendingPermissions {
        #[serde(rename = "requestId")]
        request_id: String,
    },

    #[serde(rename = "agent.intel.allowPendingPermissionRequest")]
    AllowPendingPermissionRequest {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "sessionIncarnationId")]
        session_incarnation_id: String,
        #[serde(rename = "toolUseId")]
        tool_use_id: String,
        #[serde(rename = "requestGeneration")]
        request_generation: u64,
    },

    #[serde(rename = "agent.intel.denyPendingPermissionRequest")]
    DenyPendingPermissionRequest {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "sessionIncarnationId")]
        session_incarnation_id: String,
        #[serde(rename = "toolUseId")]
        tool_use_id: String,
        #[serde(rename = "requestGeneration")]
        request_generation: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },

    #[serde(rename = "agent.intel.semanticSend")]
    SemanticSend {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "incarnationId")]
        incarnation_id: String,
        mode: crate::host_protocol::SemanticSendMode,
        text: String,
    },
    #[serde(rename = "agent.intel.cancelSteer")]
    CancelSteer {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "steerId")]
        steer_id: String,
    },
    #[serde(rename = "agent.intel.querySteer")]
    QuerySteer {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(
            rename = "semanticRequestId",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        semantic_request_id: Option<String>,
    },
}

impl SystemCommand {
    pub(crate) fn validate(&self) -> Result<(), HostCommandValidationError> {
        match self {
            Self::SemanticSend {
                request_id,
                session_id,
                incarnation_id,
                text,
                ..
            } => {
                validate_uuid_v7(request_id, "requestId")?;
                validate_present(session_id, "sessionId")?;
                validate_uuid_v7(incarnation_id, "incarnationId")?;
                validate_present(text, "text")
            }
            Self::QuerySteer {
                semantic_request_id: Some(semantic_request_id),
                ..
            } => validate_uuid_v7(semantic_request_id, "semanticRequestId"),
            Self::Shutdown
            | Self::RefreshClaudeGlobal { .. }
            | Self::SetHostTheme { .. }
            | Self::QueryPendingPermissions { .. }
            | Self::AllowPendingPermissionRequest { .. }
            | Self::DenyPendingPermissionRequest { .. }
            | Self::CancelSteer { .. }
            | Self::QuerySteer {
                semantic_request_id: None,
                ..
            } => Ok(()),
        }
    }

    pub(crate) fn semantic_reply_request_id(&self) -> Option<&str> {
        match self {
            Self::SemanticSend { request_id, .. }
            | Self::CancelSteer { request_id, .. }
            | Self::QuerySteer { request_id, .. } => Some(request_id),
            _ => None,
        }
    }
}

fn validate_present(value: &str, field: &'static str) -> Result<(), HostCommandValidationError> {
    if value.trim().is_empty() {
        return Err(HostCommandValidationError::EmptyField(field));
    }
    Ok(())
}

fn validate_uuid_v7(value: &str, field: &'static str) -> Result<(), HostCommandValidationError> {
    let id =
        uuid::Uuid::parse_str(value).map_err(|_| HostCommandValidationError::InvalidField {
            field,
            reason: "must be a canonical UUIDv7",
        })?;
    if id.get_version_num() != 7 || id.hyphenated().to_string() != value {
        return Err(HostCommandValidationError::InvalidField {
            field,
            reason: "must be a canonical UUIDv7",
        });
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum AgentIntelCommand {
    #[serde(rename = "agent.intel.readSettings")]
    ReadSettings {
        #[serde(rename = "requestId")]
        request_id: String,
        agent: String,
        cwd: Option<String>,
    },
    #[serde(rename = "agent.intel.listClaudeProjects")]
    ListClaudeProjects {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "agent.intel.listProjectSessions")]
    ListProjectSessions {
        #[serde(rename = "requestId")]
        request_id: String,
        slug: String,
    },
    #[serde(rename = "agent.intel.listProjectMemories")]
    ListProjectMemories {
        #[serde(rename = "requestId")]
        request_id: String,
        slug: String,
    },
    #[serde(rename = "agent.intel.readProjectMemory")]
    ReadProjectMemory {
        #[serde(rename = "requestId")]
        request_id: String,
        slug: String,
        filename: String,
    },
    #[serde(rename = "agent.intel.listClaudeMemory")]
    ListClaudeMemory {
        #[serde(rename = "requestId")]
        request_id: String,
        cwd: String,
    },
    #[serde(rename = "agent.intel.readClaudeMemory")]
    ReadClaudeMemory {
        #[serde(rename = "requestId")]
        request_id: String,
        cwd: String,
        filename: String,
    },
    #[serde(rename = "agent.intel.readSessionConversation")]
    ReadSessionConversation {
        #[serde(rename = "requestId")]
        request_id: String,
        agent: String,
        cwd: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(
            rename = "beforeByte",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        before_byte: Option<u64>,
        #[serde(
            rename = "maxRecords",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        max_records: Option<usize>,
        #[serde(rename = "maxBytes", default, skip_serializing_if = "Option::is_none")]
        max_bytes: Option<usize>,
    },
    #[serde(rename = "agent.intel.listCopilotRepositories")]
    ListCopilotRepositories {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "agent.intel.listCopilotRepoSessions")]
    ListCopilotRepoSessions {
        #[serde(rename = "requestId")]
        request_id: String,
        repository: String,
    },
    #[serde(rename = "agent.intel.discoverProviderConversations")]
    DiscoverProviderConversations {
        #[serde(rename = "requestId")]
        request_id: String,
        provider: String,
        #[serde(rename = "workingDirectory")]
        working_directory: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<usize>,
        #[serde(rename = "maxBytes", default, skip_serializing_if = "Option::is_none")]
        max_bytes: Option<usize>,
    },
    #[serde(rename = "agent.intel.copyProjectMemory")]
    CopyProjectMemory {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "sourceSlug")]
        source_slug: String,
        filename: String,
        #[serde(rename = "targetSlug")]
        target_slug: String,
    },

    #[serde(rename = "agent.intel.readClaudeAutoModeRules")]
    ReadClaudeAutoModeRules {
        #[serde(rename = "requestId")]
        request_id: String,
    },

    #[serde(rename = "agent.intel.writeClaudeAutoModeRules")]
    WriteClaudeAutoModeRules {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(default)]
        environment: Vec<String>,
        #[serde(default)]
        allow: Vec<String>,
        #[serde(default, rename = "softDeny")]
        soft_deny: Vec<String>,
        #[serde(default, rename = "hardDeny")]
        hard_deny: Vec<String>,
    },
    #[serde(rename = "agent.intel.listCustomAgents")]
    ListCustomAgents {
        #[serde(rename = "requestId")]
        request_id: String,

        directory: String,
    },
    #[serde(rename = "agent.intel.listActiveCustomizations")]
    ListActiveCustomizations {
        #[serde(rename = "requestId")]
        request_id: String,
        cwd: String,
        agent: String,
    },
    #[serde(rename = "agent.intel.discoverExternalMcpServers")]
    DiscoverExternalMcpServers {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "agent.intel.discoverExternalSessions")]
    DiscoverExternalSessions {
        #[serde(rename = "requestId")]
        request_id: String,

        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent: Option<String>,
    },

    #[serde(rename = "agent.intel.resolveActiveSession")]
    ResolveActiveSession {
        #[serde(rename = "requestId")]
        request_id: String,
        cwd: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
    },

    #[serde(rename = "agent.intel.listSubagentTranscripts")]
    ListSubagentTranscripts {
        #[serde(rename = "requestId")]
        request_id: String,
        cwd: String,
        #[serde(rename = "sessionId")]
        session_id: String,
    },

    #[serde(rename = "agent.intel.readSubagentTranscript")]
    ReadSubagentTranscript {
        #[serde(rename = "requestId")]
        request_id: String,
        agent: String,
        path: String,
        #[serde(
            rename = "beforeByte",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        before_byte: Option<u64>,
        #[serde(
            rename = "maxRecords",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        max_records: Option<usize>,
        #[serde(rename = "maxBytes", default, skip_serializing_if = "Option::is_none")]
        max_bytes: Option<usize>,
    },
}

impl AgentIntelCommand {
    pub(crate) fn request_id(&self) -> &str {
        match self {
            Self::ReadSettings { request_id, .. }
            | Self::ListClaudeProjects { request_id }
            | Self::ListProjectSessions { request_id, .. }
            | Self::ListProjectMemories { request_id, .. }
            | Self::ReadProjectMemory { request_id, .. }
            | Self::ListClaudeMemory { request_id, .. }
            | Self::ReadClaudeMemory { request_id, .. }
            | Self::ReadSessionConversation { request_id, .. }
            | Self::ListCopilotRepositories { request_id }
            | Self::ListCopilotRepoSessions { request_id, .. }
            | Self::DiscoverProviderConversations { request_id, .. }
            | Self::CopyProjectMemory { request_id, .. }
            | Self::WriteClaudeAutoModeRules { request_id, .. }
            | Self::ResolveActiveSession { request_id, .. }
            | Self::ListSubagentTranscripts { request_id, .. }
            | Self::ReadClaudeAutoModeRules { request_id, .. }
            | Self::ListCustomAgents { request_id, .. }
            | Self::ListActiveCustomizations { request_id, .. }
            | Self::DiscoverExternalMcpServers { request_id, .. }
            | Self::DiscoverExternalSessions { request_id, .. }
            | Self::ReadSubagentTranscript { request_id, .. } => request_id,
        }
    }
}

#[cfg(feature = "cli")]
#[derive(Debug, Clone)]
pub(crate) enum HostCommand {
    Terminal(TerminalCommand),
    System(SystemCommand),
    Auth(AuthCommand),
    Friends(FriendsCommand),
    Devices(DeviceCommand),
    Trust(TrustCommand),
    Room(RoomCommand),
    Session(SessionCommand),
    AgentIntel(AgentIntelCommand),
}

#[cfg(feature = "cli")]
impl HostCommand {
    pub(crate) fn message_type(&self) -> &'static str {
        match self {
            Self::Terminal(command) => terminal_command_type(command),
            Self::System(command) => system_command_type(command),
            Self::Auth(command) => auth_command_type(command),
            Self::Friends(command) => command.operation(),
            Self::Devices(command) => command.operation(),
            Self::Trust(command) => command.operation(),
            Self::Room(command) => command.operation(),
            Self::Session(command) => command.operation(),
            Self::AgentIntel(command) => agent_intel_command_type(command),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HostCommandValidationError {
    EmptyField(&'static str),
    InvalidField {
        field: &'static str,
        reason: &'static str,
    },
    MissingRoomId,
}

impl fmt::Display for HostCommandValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(f, "{field} cannot be empty"),
            Self::InvalidField { field, reason } => write!(f, "{field} is invalid: {reason}"),
            Self::MissingRoomId => {
                write!(f, "roomId is required for room sharing")
            }
        }
    }
}

#[cfg(feature = "cli")]
#[derive(Debug, Clone, PartialEq, Eq)]
enum HostCommandDecodeError {
    UnsupportedType(Option<String>),
    InvalidPayload {
        command_type: String,
        message: String,
    },
}

#[cfg(feature = "cli")]
impl fmt::Display for HostCommandDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedType(Some(command_type)) => {
                write!(f, "unsupported host command type `{command_type}`")
            }
            Self::UnsupportedType(None) => write!(f, "unsupported host command type"),
            Self::InvalidPayload {
                command_type,
                message,
            } => {
                write!(f, "invalid payload for `{command_type}`: {message}")
            }
        }
    }
}

#[cfg(feature = "cli")]
impl<'de> Deserialize<'de> for HostCommand {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;

        let Some(t) = command_type(&value) else {
            return Err(de::Error::custom(HostCommandDecodeError::UnsupportedType(
                None,
            )));
        };
        let Some(family) = HostCommandFamily::from_type(&t) else {
            return Err(de::Error::custom(HostCommandDecodeError::UnsupportedType(
                Some(t),
            )));
        };
        family.decode(value).map_err(|message| {
            de::Error::custom(HostCommandDecodeError::InvalidPayload {
                command_type: t,
                message,
            })
        })
    }
}

#[cfg(feature = "cli")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HostCommandFamily {
    Terminal,
    System,
    Auth,
    Friends,
    Devices,
    Trust,
    Room,
    Session,
    AgentIntel,
}

#[cfg(feature = "cli")]
const SESSION_COMMAND_TYPES: &[&str] = &[
    "session.create",
    "session.rename",
    "session.mode",
    "session.stop",
    "session.close",
    "session.interrupt",
    "session.delete",
    "session.reopen",
    "session.openRemote",
    "session.hide",
    "session.leave",
    "session.unhide",
    "session.listHidden",
    "session.scope",
    "session.grantAccess",
    "session.revokeAccess",
    "session.listAccess",
    "session.list",
    "session.accessMutationsRecover",
    "session.accessMutationReconcile",
    "session.accessMutationAck",
];

#[cfg(feature = "cli")]
const AGENT_INTEL_COMMAND_TYPES: &[&str] = &[
    "agent.intel.readSettings",
    "agent.intel.listClaudeProjects",
    "agent.intel.listProjectSessions",
    "agent.intel.listProjectMemories",
    "agent.intel.readProjectMemory",
    "agent.intel.listClaudeMemory",
    "agent.intel.readClaudeMemory",
    "agent.intel.readSessionConversation",
    "agent.intel.listCopilotRepositories",
    "agent.intel.listCopilotRepoSessions",
    "agent.intel.discoverProviderConversations",
    "agent.intel.copyProjectMemory",
    "agent.intel.readClaudeAutoModeRules",
    "agent.intel.writeClaudeAutoModeRules",
    "agent.intel.listCustomAgents",
    "agent.intel.listActiveCustomizations",
    "agent.intel.discoverExternalMcpServers",
    "agent.intel.discoverExternalSessions",
    "agent.intel.resolveActiveSession",
    "agent.intel.listSubagentTranscripts",
    "agent.intel.readSubagentTranscript",
];

#[cfg(feature = "cli")]
impl HostCommandFamily {
    fn from_type(t: &str) -> Option<Self> {
        match t {
            "session.inputBytes" | "session.resize" | "session.focus" | "session.blur" => {
                Some(Self::Terminal)
            }
            "shutdown"
            | "claude.global.refresh"
            | "system.setTheme"
            | "agent.intel.queryPendingPermissions"
            | "agent.intel.allowPendingPermissionRequest"
            | "agent.intel.denyPendingPermissionRequest"
            | "agent.intel.semanticSend"
            | "agent.intel.cancelSteer"
            | "agent.intel.querySteer" => Some(Self::System),
            _ if AGENT_INTEL_COMMAND_TYPES.contains(&t) => Some(Self::AgentIntel),
            _ if t.starts_with("auth.") => Some(Self::Auth),
            _ if t.starts_with("friends.") => Some(Self::Friends),
            _ if t.starts_with("devices.") => Some(Self::Devices),
            _ if t.starts_with("trust.") => Some(Self::Trust),
            _ if t.starts_with("room.") => Some(Self::Room),
            _ if SESSION_COMMAND_TYPES.contains(&t) => Some(Self::Session),
            _ => None,
        }
    }

    fn decode(self, value: serde_json::Value) -> Result<HostCommand, String> {
        fn map<T, F>(value: serde_json::Value, wrap: F) -> Result<HostCommand, String>
        where
            T: for<'de> Deserialize<'de>,
            F: FnOnce(T) -> HostCommand,
        {
            serde_json::from_value::<T>(value)
                .map(wrap)
                .map_err(|error| error.to_string())
        }
        match self {
            Self::Terminal => map(value, HostCommand::Terminal),
            Self::System => map(value, HostCommand::System),
            Self::Auth => map(value, HostCommand::Auth),
            Self::Friends => map(value, HostCommand::Friends),
            Self::Devices => map(value, HostCommand::Devices),
            Self::Trust => map(value, HostCommand::Trust),
            Self::Room => map(value, HostCommand::Room),
            Self::Session => map(value, HostCommand::Session),
            Self::AgentIntel => map(value, HostCommand::AgentIntel),
        }
    }
}

#[cfg(feature = "cli")]
fn command_type(value: &serde_json::Value) -> Option<String> {
    value
        .get("type")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
}

#[cfg(feature = "cli")]
fn terminal_command_type(command: &TerminalCommand) -> &'static str {
    match command {
        TerminalCommand::InputBytes { .. } => "session.inputBytes",
        TerminalCommand::Resize { .. } | TerminalCommand::HeadlessResize { .. } => "session.resize",
        TerminalCommand::Focus { .. } => "session.focus",
        TerminalCommand::Blur { .. } => "session.blur",
    }
}

#[cfg(feature = "cli")]
fn system_command_type(command: &SystemCommand) -> &'static str {
    match command {
        SystemCommand::Shutdown => "shutdown",
        SystemCommand::RefreshClaudeGlobal { .. } => "claude.global.refresh",
        SystemCommand::SetHostTheme { .. } => "system.setTheme",
        SystemCommand::QueryPendingPermissions { .. } => "agent.intel.queryPendingPermissions",
        SystemCommand::AllowPendingPermissionRequest { .. } => {
            "agent.intel.allowPendingPermissionRequest"
        }
        SystemCommand::DenyPendingPermissionRequest { .. } => {
            "agent.intel.denyPendingPermissionRequest"
        }
        SystemCommand::SemanticSend { .. } => "agent.intel.semanticSend",
        SystemCommand::CancelSteer { .. } => "agent.intel.cancelSteer",
        SystemCommand::QuerySteer { .. } => "agent.intel.querySteer",
    }
}

#[cfg(feature = "cli")]
fn auth_command_type(command: &AuthCommand) -> &'static str {
    match command {
        AuthCommand::LoginStart => "auth.login.start",
        AuthCommand::Logout => "auth.logout",
        AuthCommand::IdentityReset => "auth.identity.reset",
        AuthCommand::Refresh => "auth.refresh",
    }
}

#[cfg(feature = "cli")]
fn agent_intel_command_type(command: &AgentIntelCommand) -> &'static str {
    match command {
        AgentIntelCommand::ReadSettings { .. } => "agent.intel.readSettings",
        AgentIntelCommand::ListClaudeProjects { .. } => "agent.intel.listClaudeProjects",
        AgentIntelCommand::ListProjectSessions { .. } => "agent.intel.listProjectSessions",
        AgentIntelCommand::ListProjectMemories { .. } => "agent.intel.listProjectMemories",
        AgentIntelCommand::ReadProjectMemory { .. } => "agent.intel.readProjectMemory",
        AgentIntelCommand::ListClaudeMemory { .. } => "agent.intel.listClaudeMemory",
        AgentIntelCommand::ReadClaudeMemory { .. } => "agent.intel.readClaudeMemory",
        AgentIntelCommand::ReadSessionConversation { .. } => "agent.intel.readSessionConversation",
        AgentIntelCommand::ListCopilotRepositories { .. } => "agent.intel.listCopilotRepositories",
        AgentIntelCommand::ListCopilotRepoSessions { .. } => "agent.intel.listCopilotRepoSessions",
        AgentIntelCommand::DiscoverProviderConversations { .. } => {
            "agent.intel.discoverProviderConversations"
        }
        AgentIntelCommand::CopyProjectMemory { .. } => "agent.intel.copyProjectMemory",
        AgentIntelCommand::WriteClaudeAutoModeRules { .. } => {
            "agent.intel.writeClaudeAutoModeRules"
        }
        AgentIntelCommand::ResolveActiveSession { .. } => "agent.intel.resolveActiveSession",
        AgentIntelCommand::ListSubagentTranscripts { .. } => "agent.intel.listSubagentTranscripts",
        AgentIntelCommand::ReadSubagentTranscript { .. } => "agent.intel.readSubagentTranscript",
        AgentIntelCommand::ReadClaudeAutoModeRules { .. } => "agent.intel.readClaudeAutoModeRules",
        AgentIntelCommand::ListCustomAgents { .. } => "agent.intel.listCustomAgents",
        AgentIntelCommand::ListActiveCustomizations { .. } => {
            "agent.intel.listActiveCustomizations"
        }
        AgentIntelCommand::DiscoverExternalMcpServers { .. } => {
            "agent.intel.discoverExternalMcpServers"
        }
        AgentIntelCommand::DiscoverExternalSessions { .. } => {
            "agent.intel.discoverExternalSessions"
        }
    }
}

#[cfg(all(test, feature = "cli"))]
mod tests {
    use super::{
        AgentIntelCommand, AuthCommand, DeviceCommand, FriendsCommand, HostCommand,
        HostCommandValidationError, RoomCommand, SessionCommand, SystemCommand, TerminalCommand,
        TrustCommand,
    };
    use kodosi_domain::session::SessionMode;
    use std::collections::BTreeSet;

    fn round_trip(original: HostCommand) -> HostCommand {
        let json = match &original {
            HostCommand::Terminal(c) => serde_json::to_value(c),
            HostCommand::System(c) => serde_json::to_value(c),
            HostCommand::Auth(c) => serde_json::to_value(c),
            HostCommand::Friends(c) => serde_json::to_value(c),
            HostCommand::Devices(c) => serde_json::to_value(c),
            HostCommand::Trust(c) => serde_json::to_value(c),
            HostCommand::Room(c) => serde_json::to_value(c),
            HostCommand::Session(c) => serde_json::to_value(c),
            HostCommand::AgentIntel(c) => serde_json::to_value(c),
        }
        .expect("serialize sample");
        let type_tag = json
            .get("type")
            .and_then(|v| v.as_str())
            .expect("sample has a type tag")
            .to_owned();
        serde_json::from_value::<HostCommand>(json).unwrap_or_else(|error| {
            panic!("round-trip failed for `{type_tag}`: {error}");
        })
    }

    fn assert_same_family(label: &str, decoded: &HostCommand, original: &HostCommand) {
        let same = matches!(
            (decoded, original),
            (HostCommand::Terminal(_), HostCommand::Terminal(_))
                | (HostCommand::System(_), HostCommand::System(_))
                | (HostCommand::Auth(_), HostCommand::Auth(_))
                | (HostCommand::Friends(_), HostCommand::Friends(_))
                | (HostCommand::Devices(_), HostCommand::Devices(_))
                | (HostCommand::Trust(_), HostCommand::Trust(_))
                | (HostCommand::Room(_), HostCommand::Room(_))
                | (HostCommand::Session(_), HostCommand::Session(_))
                | (HostCommand::AgentIntel(_), HostCommand::AgentIntel(_))
        );
        assert!(
            same,
            "`{label}` misrouted: original={original:?} decoded={decoded:?}",
        );
    }

    #[test]
    fn semantic_send_requires_present_fields_and_canonical_uuid_v7_ids() {
        let command = |request_id: &str, session_id: &str, incarnation_id: &str, text: &str| {
            SystemCommand::SemanticSend {
                request_id: request_id.to_owned(),
                session_id: session_id.to_owned(),
                incarnation_id: incarnation_id.to_owned(),
                mode: crate::host_protocol::SemanticSendMode::Steer,
                text: text.to_owned(),
            }
        };
        let v7 = "01900000-0000-7000-8000-000000000001";

        assert!(command(v7, "session-1", v7, "ship it").validate().is_ok());
        for request_id in [
            "not-a-uuid",
            "550e8400-e29b-41d4-a716-446655440000",
            "01900000000070008000000000000001",
        ] {
            assert_eq!(
                command(request_id, "session-1", v7, "ship it").validate(),
                Err(HostCommandValidationError::InvalidField {
                    field: "requestId",
                    reason: "must be a canonical UUIDv7",
                })
            );
        }
        assert_eq!(
            command(v7, " ", v7, "ship it").validate(),
            Err(HostCommandValidationError::EmptyField("sessionId"))
        );
        assert_eq!(
            command(v7, "session-1", v7, "  ").validate(),
            Err(HostCommandValidationError::EmptyField("text"))
        );
    }

    #[test]
    fn query_steer_validates_semantic_request_id_only_when_present() {
        let command = |semantic_request_id: Option<&str>| SystemCommand::QuerySteer {
            request_id: "query-1".to_owned(),
            session_id: "session-1".to_owned(),
            semantic_request_id: semantic_request_id.map(str::to_owned),
        };

        assert!(command(None).validate().is_ok());
        assert!(
            command(Some("01900000-0000-7000-8000-000000000001"))
                .validate()
                .is_ok()
        );
        assert_eq!(
            command(Some("550e8400-e29b-41d4-a716-446655440000")).validate(),
            Err(HostCommandValidationError::InvalidField {
                field: "semanticRequestId",
                reason: "must be a canonical UUIDv7",
            })
        );
    }

    #[test]
    fn every_variant_routes_to_its_own_family() {
        let samples: Vec<(&str, HostCommand)> = vec![
            (
                "session.inputBytes",
                HostCommand::Terminal(TerminalCommand::InputBytes {
                    session_id: "session-1".to_owned(),
                    bytes: vec![0x1b, b'[', b'A'],
                    expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000001"
                        .to_owned(),
                    subscription_id: Some("terminal-1".to_owned()),
                    subscription_generation: Some(1),
                }),
            ),
            (
                "session.resize",
                HostCommand::Terminal(TerminalCommand::Resize {
                    session_id: "session-1".to_owned(),
                    identity: crate::host_protocol::TerminalResizeIdentity {
                        request_id: "01900000-0000-7000-8000-000000000004".to_owned(),
                        expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000001"
                            .to_owned(),
                        subscription_id: "terminal-1".to_owned(),
                        subscription_generation: 7,
                        surface_generation: 3,
                        cols: 120,
                        rows: 40,
                        width_pixels: 1_200,
                        height_pixels: 800,
                        cell_width_pixels: 10,
                        cell_height_pixels: 20,
                    },
                    claim: false,
                }),
            ),
            (
                "session.focus",
                HostCommand::Terminal(TerminalCommand::Focus {
                    session_id: "session-1".to_owned(),
                    client_id: "client-1".to_owned(),
                    request_id: "01900000-0000-7000-8000-000000000002".to_owned(),
                    expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000001"
                        .to_owned(),
                }),
            ),
            (
                "session.blur",
                HostCommand::Terminal(TerminalCommand::Blur {
                    session_id: "session-1".to_owned(),
                    client_id: "client-1".to_owned(),
                    expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000001"
                        .to_owned(),
                }),
            ),
            ("shutdown", HostCommand::System(SystemCommand::Shutdown)),
            (
                "claude.global.refresh",
                HostCommand::System(SystemCommand::RefreshClaudeGlobal {
                    cwd: Some("/tmp/kodosi".to_owned()),
                    generation: Some(7),
                }),
            ),
            (
                "system.setTheme",
                HostCommand::System(SystemCommand::SetHostTheme { dark: true }),
            ),
            (
                "agent.intel.queryPendingPermissions",
                HostCommand::System(SystemCommand::QueryPendingPermissions {
                    request_id: "r-p".to_owned(),
                }),
            ),
            (
                "agent.intel.allowPendingPermissionRequest",
                HostCommand::System(SystemCommand::AllowPendingPermissionRequest {
                    session_id: "00000000-0000-0000-0000-000000000000".to_owned(),
                    session_incarnation_id: "01900000-0000-7000-8000-000000000002".to_owned(),
                    tool_use_id: "toolu_test".to_owned(),
                    request_generation: 7,
                }),
            ),
            (
                "agent.intel.denyPendingPermissionRequest",
                HostCommand::System(SystemCommand::DenyPendingPermissionRequest {
                    session_id: "00000000-0000-0000-0000-000000000000".to_owned(),
                    session_incarnation_id: "01900000-0000-7000-8000-000000000002".to_owned(),
                    tool_use_id: "toolu_test".to_owned(),
                    request_generation: 7,
                    reason: Some("blocked by policy".to_owned()),
                }),
            ),
            (
                "auth.login.start",
                HostCommand::Auth(AuthCommand::LoginStart),
            ),
            ("auth.logout", HostCommand::Auth(AuthCommand::Logout)),
            (
                "auth.identity.reset",
                HostCommand::Auth(AuthCommand::IdentityReset),
            ),
            ("auth.refresh", HostCommand::Auth(AuthCommand::Refresh)),
            (
                "friends.refresh",
                HostCommand::Friends(FriendsCommand::Refresh),
            ),
            (
                "friends.request.send",
                HostCommand::Friends(FriendsCommand::RequestSend {
                    username: "alice".to_owned(),
                    request_id: "01900000-0000-7000-8000-000000000001".to_owned(),
                }),
            ),
            (
                "devices.refresh",
                HostCommand::Devices(DeviceCommand::Refresh),
            ),
            (
                "devices.link.startSelf",
                HostCommand::Devices(DeviceCommand::LinkStartSelf),
            ),
            (
                "trust.refresh",
                HostCommand::Trust(TrustCommand::Refresh {
                    request_id: "request-1".to_owned(),
                }),
            ),
            ("room.refresh", HostCommand::Room(RoomCommand::Refresh)),
            (
                "session.list",
                HostCommand::Session(SessionCommand::SnapshotRefresh),
            ),
            (
                "session.create",
                HostCommand::Session(SessionCommand::Create {
                    request_id: "r-1".to_owned(),
                    name: "demo".to_owned(),
                    working_dir: None,
                    resume: None,
                }),
            ),
            (
                "session.listHidden",
                HostCommand::Session(SessionCommand::ListHidden {
                    request_id: "r-hidden".to_owned(),
                }),
            ),
            (
                "session.mode",
                HostCommand::Session(SessionCommand::SetMode {
                    session_id: "session-1".to_owned(),
                    expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000001"
                        .to_owned(),
                    mode: SessionMode::Plan,
                }),
            ),
            (
                "agent.intel.readSettings",
                HostCommand::AgentIntel(AgentIntelCommand::ReadSettings {
                    request_id: "r-1".to_owned(),
                    agent: "claude".to_owned(),
                    cwd: None,
                }),
            ),
            (
                "agent.intel.resolveActiveSession",
                HostCommand::AgentIntel(AgentIntelCommand::ResolveActiveSession {
                    request_id: "r-5".to_owned(),
                    cwd: "/tmp/project".to_owned(),
                    session_id: "01900000-0000-7000-8000-000000000001".to_owned(),
                    expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000002"
                        .to_owned(),
                }),
            ),
        ];

        for (label, original) in &samples {
            let decoded = round_trip(original.clone());
            assert_same_family(label, &decoded, original);
        }
    }

    #[test]
    fn generated_command_type_sets_route_to_their_declared_families() {
        let authority: serde_json::Value = serde_json::from_str(
            &crate::host_protocol::authority::render_desktop_runtime_authority_json()
                .expect("render authority"),
        )
        .expect("parse authority");
        for (field, expected) in [
            ("systemCommandTypes", super::HostCommandFamily::System),
            (
                "agentIntelCommandTypes",
                super::HostCommandFamily::AgentIntel,
            ),
            ("sessionCommandTypes", super::HostCommandFamily::Session),
        ] {
            for command_type in authority[field].as_array().expect("command type array") {
                let command_type = command_type.as_str().expect("command type string");
                assert_eq!(
                    super::HostCommandFamily::from_type(command_type),
                    Some(expected),
                    "`{command_type}` is not routed through its declared {field} family"
                );
            }
        }
    }

    #[test]
    fn unknown_type_is_unsupported_not_invalid_payload() {
        let json = serde_json::json!({ "type": "totally.bogus.command" });
        let error = serde_json::from_value::<HostCommand>(json)
            .expect_err("bogus type must not deserialize");
        let msg = error.to_string();
        assert!(
            msg.contains("unsupported host command type"),
            "expected unsupported-type error, got: {msg}",
        );
    }

    #[test]
    fn missing_type_field_is_unsupported() {
        let json = serde_json::json!({ "sessionId": "session-1" });
        let error = serde_json::from_value::<HostCommand>(json)
            .expect_err("missing type must not deserialize");
        let msg = error.to_string();
        assert!(
            msg.contains("unsupported host command type"),
            "expected unsupported-type error, got: {msg}",
        );
    }

    #[test]
    fn recognised_type_with_bad_payload_surfaces_serde_error() {
        let json = serde_json::json!({ "type": "session.create" });
        let error = serde_json::from_value::<HostCommand>(json)
            .expect_err("incomplete payload must not deserialize");
        let msg = error.to_string();
        assert!(
            msg.contains("invalid payload for `session.create`"),
            "expected payload error tagged with command type, got: {msg}",
        );
        assert!(
            msg.contains("missing field"),
            "expected serde's missing-field detail, got: {msg}",
        );
    }

    fn authority_types(field: &str) -> Vec<String> {
        let json = crate::host_protocol::authority::render_desktop_runtime_authority_json()
            .expect("the authority manifest must render");
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("the authority manifest must be JSON");
        value
            .get(field)
            .and_then(serde_json::Value::as_array)
            .unwrap_or_else(|| panic!("the manifest must carry `{field}`"))
            .iter()
            .map(|entry| {
                entry
                    .as_str()
                    .unwrap_or_else(|| panic!("`{field}` entries must be strings"))
                    .to_owned()
            })
            .collect()
    }

    #[test]
    fn session_command_tags_cover_every_variant() {
        let declared: BTreeSet<String> = super::SESSION_COMMAND_TYPES
            .iter()
            .map(|tag| (*tag).to_owned())
            .collect();

        assert_eq!(
            declared,
            authority_types("sessionCommandTypes")
                .into_iter()
                .collect::<BTreeSet<_>>(),
            "a `SessionCommand` variant the router does not name would be reported \
             as an unsupported type instead of being dispatched",
        );
    }

    #[test]
    fn agent_intel_command_tags_cover_every_variant() {
        let declared: BTreeSet<String> = super::AGENT_INTEL_COMMAND_TYPES
            .iter()
            .map(|tag| (*tag).to_owned())
            .collect();

        assert_eq!(
            declared,
            authority_types("agentIntelCommandTypes")
                .into_iter()
                .collect::<BTreeSet<_>>(),
            "an `AgentIntelCommand` variant the router does not name would be \
             reported as an unsupported type instead of being dispatched",
        );
    }

    #[test]
    fn unknown_variants_in_a_served_namespace_are_unsupported_not_malformed() {
        for tag in [
            "agent.intel.readSomethingFromTheFuture",
            "agent.intel.listQuantumPlans",
            "session.teleport",
            "session.createDeluxe",
            "participant.applaud",
            "participant.suggest",
            "participant.inject",
        ] {
            let error = serde_json::from_value::<HostCommand>(serde_json::json!({ "type": tag }))
                .expect_err("an unknown variant must not deserialize")
                .to_string();
            assert!(
                error.contains("unsupported host command type"),
                "`{tag}` must classify as an unsupported type, got: {error}",
            );
            assert!(
                error.contains(tag),
                "the verdict must name the tag the caller sent, got: {error}",
            );
            assert!(
                !error.contains("invalid payload"),
                "`{tag}` is not a payload mistake, got: {error}",
            );
        }
    }

    #[test]
    fn known_variants_in_a_served_namespace_still_report_payload_faults() {
        for (tag, missing) in [
            ("agent.intel.readSettings", "requestId"),
            ("session.rename", "sessionId"),
        ] {
            let error = serde_json::from_value::<HostCommand>(serde_json::json!({ "type": tag }))
                .expect_err("an empty payload must not deserialize")
                .to_string();
            assert!(
                error.contains(&format!("invalid payload for `{tag}`")),
                "`{tag}` must classify as a payload fault, got: {error}",
            );
            assert!(
                error.contains(missing),
                "the verdict must name the missing field `{missing}`, got: {error}",
            );
        }
    }
}
