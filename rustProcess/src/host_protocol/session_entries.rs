use serde::{Deserialize, Serialize};

use kodosi_domain::{
    lifecycle::{ConnectionState, RemoteSessionAccessIssue, RemoteSessionAccessState},
    permissions::{AccessLevel, ShareScope},
    session::{LocalSessionRecoveryState, SessionMode},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RuntimeSessionStatus {
    Active,
    Blocked,
    Waiting,
    Reconnecting,
    Stopping,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionListEntryMeta {
    pub(crate) agent: String,
    pub(crate) working_dir: String,
    pub(crate) token_percent: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) git_repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) git_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) git_branch: Option<String>,
    #[serde(default)]
    pub(crate) git_remotes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) project_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) package_manager: Option<String>,
    #[serde(default)]
    pub(crate) manifest_files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) running_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) detected_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) terminal_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SessionListEntry {
    Local {
        #[serde(flatten)]
        entry: LocalSessionListEntry,
    },
    Remote {
        #[serde(flatten)]
        entry: RemoteSessionListEntry,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionSemanticActions {
    pub(crate) queue: bool,
    pub(crate) steer: bool,
    pub(crate) stop_and_send: bool,
}

impl SessionSemanticActions {
    pub(crate) const NONE: Self = Self {
        queue: false,
        steer: false,
        stop_and_send: false,
    };
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(transparent)]
#[specta(transparent)]
pub(crate) struct SessionActivity(String);

impl PartialEq for SessionActivity {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for SessionActivity {}

impl From<String> for SessionActivity {
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalSessionListEntry {
    pub(crate) id: String,
    pub(crate) incarnation_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) create_request_id: Option<String>,
    pub(crate) name: String,
    pub(crate) project: String,
    pub(crate) mode: SessionMode,
    pub(crate) status: RuntimeSessionStatus,
    pub(crate) recovery: LocalSessionRecoveryState,
    pub(crate) scope: ShareScope,
    pub(crate) access: AccessLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) room_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) room_name: Option<String>,
    pub(crate) active_count: usize,
    pub(crate) entitled_count: usize,
    pub(crate) last_activity: SessionActivity,
    #[serde(default)]
    pub(crate) semantic_actions: SessionSemanticActions,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) backend_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) backend_incarnation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) meta: Option<Box<SessionListEntryMeta>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSessionListEntry {
    pub(crate) id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) incarnation_id: Option<String>,
    pub(crate) name: String,
    pub(crate) project: String,
    pub(crate) mode: SessionMode,
    pub(crate) status: RuntimeSessionStatus,
    pub(crate) scope: ShareScope,
    pub(crate) access: AccessLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) owner: Option<String>,
    pub(crate) owner_user_id: Option<String>,
    pub(crate) permissions: PermissionFlags,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) room_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) room_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) connection_state: Option<ConnectionState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) connection_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) access_state: Option<RemoteSessionAccessState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) access_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) access_issue: Option<RemoteSessionAccessIssue>,
    pub(crate) active_count: usize,
    pub(crate) entitled_count: usize,
    pub(crate) last_activity: SessionActivity,
    #[serde(default)]
    pub(crate) semantic_actions: SessionSemanticActions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(transparent)]
pub(crate) struct PermissionFlags(pub(crate) u32);

impl PermissionFlags {
    pub(crate) const VIEW: Self = Self(0x01);
    pub(crate) const SEND_INPUT: Self = Self(0x02);
    pub(crate) const RESIZE: Self = Self(0x04);
    pub(crate) const FOCUS_BLUR: Self = Self(0x08);
    pub(crate) const STOP: Self = Self(0x10);
    pub(crate) const RENAME: Self = Self(0x20);
    pub(crate) const DELETE: Self = Self(0x40);
    pub(crate) const SET_MODE: Self = Self(0x80);
    pub(crate) const APPROVE: Self = Self(0x100);

    pub(crate) const OWNER: Self = Self(
        Self::VIEW.0
            | Self::SEND_INPUT.0
            | Self::RESIZE.0
            | Self::FOCUS_BLUR.0
            | Self::STOP.0
            | Self::RENAME.0
            | Self::DELETE.0
            | Self::SET_MODE.0
            | Self::APPROVE.0,
    );

    pub(crate) const fn from_access(access: AccessLevel) -> Self {
        match access {
            AccessLevel::View | AccessLevel::Suggest => Self::VIEW,
            AccessLevel::Inject => {
                Self(Self::VIEW.0 | Self::SEND_INPUT.0 | Self::RESIZE.0 | Self::FOCUS_BLUR.0)
            }
            AccessLevel::Approve => Self(Self::VIEW.0 | Self::APPROVE.0),
        }
    }

    pub(crate) const fn bits(self) -> u32 {
        self.0
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

impl SessionListEntry {
    pub(crate) fn id(&self) -> &str {
        match self {
            Self::Local { entry } => &entry.id,
            Self::Remote { entry } => &entry.id,
        }
    }

    #[cfg(feature = "cli")]
    pub(crate) fn backend_session_id(&self) -> Option<&str> {
        match self {
            Self::Local { entry } => entry.backend_session_id.as_deref(),
            Self::Remote { entry } => Some(&entry.id),
        }
    }

    #[cfg(feature = "cli")]
    pub(crate) fn incarnation_id(&self) -> Option<&str> {
        match self {
            Self::Local { entry } => Some(&entry.incarnation_id),
            Self::Remote { entry } => entry.incarnation_id.as_deref(),
        }
    }

    #[cfg(feature = "cli")]
    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Local { entry } => &entry.name,
            Self::Remote { entry } => &entry.name,
        }
    }

    #[cfg(feature = "cli")]
    pub(crate) fn project(&self) -> &str {
        match self {
            Self::Local { entry } => &entry.project,
            Self::Remote { entry } => &entry.project,
        }
    }

    #[cfg(feature = "cli")]
    pub(crate) fn mode(&self) -> SessionMode {
        match self {
            Self::Local { entry } => entry.mode,
            Self::Remote { entry } => entry.mode,
        }
    }

    #[cfg(feature = "cli")]
    pub(crate) fn status(&self) -> RuntimeSessionStatus {
        match self {
            Self::Local { entry } => entry.status,
            Self::Remote { entry } => entry.status,
        }
    }

    #[cfg(feature = "cli")]
    pub(crate) fn scope(&self) -> ShareScope {
        match self {
            Self::Local { entry } => entry.scope,
            Self::Remote { entry } => entry.scope,
        }
    }

    #[cfg(feature = "cli")]
    pub(crate) fn room_id(&self) -> Option<&str> {
        match self {
            Self::Local { entry } => entry.room_id.as_deref(),
            Self::Remote { entry } => entry.room_id.as_deref(),
        }
    }

    #[cfg(feature = "cli")]
    pub(crate) fn room_name(&self) -> Option<&str> {
        match self {
            Self::Local { entry } => entry.room_name.as_deref(),
            Self::Remote { entry } => entry.room_name.as_deref(),
        }
    }

    #[cfg(feature = "cli")]
    pub(crate) fn active_count(&self) -> usize {
        match self {
            Self::Local { entry } => entry.active_count,
            Self::Remote { entry } => entry.active_count,
        }
    }

    #[cfg(feature = "cli")]
    pub(crate) fn entitled_count(&self) -> usize {
        match self {
            Self::Local { entry } => entry.entitled_count,
            Self::Remote { entry } => entry.entitled_count,
        }
    }

    #[cfg(feature = "cli")]
    pub(crate) fn create_request_id(&self) -> Option<&str> {
        match self {
            Self::Local { entry } => entry.create_request_id.as_deref(),
            Self::Remote { .. } => None,
        }
    }

    #[cfg(feature = "cli")]
    pub(crate) fn meta(&self) -> Option<&SessionListEntryMeta> {
        match self {
            Self::Local { entry } => entry.meta.as_deref(),
            Self::Remote { .. } => None,
        }
    }

    #[cfg(feature = "cli")]
    pub(crate) fn is_local(&self) -> bool {
        matches!(self, Self::Local { .. })
    }

    #[cfg(feature = "cli")]
    pub(crate) fn is_remote(&self) -> bool {
        matches!(self, Self::Remote { .. })
    }

    #[cfg(feature = "cli")]
    pub(crate) fn has_active_remote_relay(&self) -> bool {
        matches!(
            self,
            Self::Remote { entry }
                if matches!(
                    entry.connection_state,
                    Some(
                        ConnectionState::Connecting
                            | ConnectionState::Connected
                            | ConnectionState::Reconnecting
                    )
                )
        )
    }

    #[cfg(feature = "cli")]
    pub(crate) fn is_active_local(&self) -> bool {
        self.is_local()
            && matches!(
                self.status(),
                RuntimeSessionStatus::Active
                    | RuntimeSessionStatus::Waiting
                    | RuntimeSessionStatus::Reconnecting
            )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RoomListEntry {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) slug: String,
}

#[cfg(test)]
mod tests {
    use super::PermissionFlags;

    #[test]
    fn owner_includes_every_defined_verb() {
        let owner = PermissionFlags::OWNER;
        assert!(owner.contains(PermissionFlags::VIEW));
        assert!(owner.contains(PermissionFlags::SEND_INPUT));
        assert!(owner.contains(PermissionFlags::RESIZE));
        assert!(owner.contains(PermissionFlags::FOCUS_BLUR));
        assert!(owner.contains(PermissionFlags::STOP));
        assert!(owner.contains(PermissionFlags::RENAME));
        assert!(owner.contains(PermissionFlags::DELETE));
        assert!(owner.contains(PermissionFlags::SET_MODE));
        assert!(owner.contains(PermissionFlags::APPROVE));
    }

    #[test]
    fn contains_is_subset_check() {
        let inject = PermissionFlags(PermissionFlags::VIEW.0 | PermissionFlags::SEND_INPUT.0);
        assert!(inject.contains(PermissionFlags::VIEW));
        assert!(inject.contains(PermissionFlags::SEND_INPUT));
        assert!(!inject.contains(PermissionFlags::DELETE));
        assert!(!inject.contains(PermissionFlags::RENAME));
        assert!(inject.contains(PermissionFlags(0)));
    }

    #[test]
    fn every_verb_owns_a_distinct_power_of_two() {
        let bits = [
            PermissionFlags::VIEW.0,
            PermissionFlags::SEND_INPUT.0,
            PermissionFlags::RESIZE.0,
            PermissionFlags::FOCUS_BLUR.0,
            PermissionFlags::STOP.0,
            PermissionFlags::RENAME.0,
            PermissionFlags::DELETE.0,
            PermissionFlags::SET_MODE.0,
            PermissionFlags::APPROVE.0,
        ];
        for &bit in &bits {
            assert!(bit.is_power_of_two(), "bit {bit:#x} is not a single bit");
        }
        let mut all: u32 = 0;
        for &bit in &bits {
            assert_eq!(all & bit, 0, "bit {bit:#x} overlaps with another verb");
            all |= bit;
        }
    }
}
