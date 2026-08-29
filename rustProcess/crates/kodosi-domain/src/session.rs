use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

pub const REMOTE_SEMANTIC_TEXT_MAX_BYTES: usize = 8 * 1024;

use crate::{
    ids::{SessionId, UserId},
    permissions::{AccessLevel, ShareScope},
    terminal::TerminalSize,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    Starting,
    Running,
    Published,
    Reconnecting,

    Stopping,
    Stopped,
    Failed,
}

impl SessionState {
    pub fn merge_local_with_incoming(local: Self, incoming: Self) -> Self {
        match (local, incoming) {
            (Self::Stopped, _) => Self::Stopped,
            (Self::Published, Self::Running) => Self::Published,
            (Self::Stopping, Self::Starting | Self::Running | Self::Reconnecting) => Self::Stopping,
            (
                Self::Failed,
                Self::Starting | Self::Running | Self::Reconnecting | Self::Published,
            ) => Self::Failed,
            (_, incoming) => incoming,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionRole {
    Owner,
    Viewer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionProvenance {
    Kodosi,
    Remote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SessionMode {
    Normal,
    Plan,
    Autopilot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum LocalSessionRecoveryState {
    Live,
    Recoverable,
    Crashed,
    Quarantined,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: SessionId,
    pub title: String,
    pub owner_id: Option<UserId>,
    pub owner_name: String,
    pub runtime_name: String,
    pub working_dir: Option<String>,
    pub running_command: Option<String>,
    pub detected_agent: Option<String>,
    pub mode: SessionMode,
    pub role: SessionRole,
    pub provenance: SessionProvenance,
    pub state: SessionState,
    pub scope: ShareScope,
    pub access: AccessLevel,
    pub room_name: Option<String>,

    pub active_count: usize,

    pub entitled_count: usize,
    pub pending_suggestion_count: usize,
    pub size: TerminalSize,
    pub created_at: OffsetDateTime,
    pub last_update: OffsetDateTime,
}

impl SessionSummary {
    pub fn new_owned(
        id: SessionId,
        title: String,
        runtime_name: String,
        size: TerminalSize,
        owner_id: Option<UserId>,
    ) -> Self {
        let now = OffsetDateTime::now_utc();
        Self {
            id,
            title,
            owner_id,
            owner_name: "You".to_owned(),
            runtime_name,
            working_dir: None,
            running_command: None,
            detected_agent: None,
            mode: SessionMode::Normal,
            role: SessionRole::Owner,
            provenance: SessionProvenance::Kodosi,
            state: SessionState::Starting,
            scope: ShareScope::JustMe,
            access: AccessLevel::Inject,
            room_name: None,
            active_count: 0,
            entitled_count: 1,
            pending_suggestion_count: 0,
            size,
            created_at: now,
            last_update: now,
        }
    }

    pub fn new_remote(
        id: SessionId,
        title: String,
        owner_name: String,
        owner_id: Option<UserId>,
        scope: ShareScope,
        access: AccessLevel,
        size: TerminalSize,
    ) -> Self {
        let now = OffsetDateTime::now_utc();
        Self {
            id,
            title,
            owner_id,
            owner_name,
            runtime_name: "remote".to_owned(),
            working_dir: None,
            running_command: None,
            detected_agent: None,
            mode: SessionMode::Normal,
            role: SessionRole::Viewer,
            provenance: SessionProvenance::Remote,
            state: SessionState::Running,
            scope,
            access,
            room_name: None,
            active_count: 0,
            entitled_count: 0,
            pending_suggestion_count: 0,
            size,
            created_at: now,
            last_update: now,
        }
    }
}
