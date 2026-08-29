use crate::{
    AppError, AuthRequiredReason, Result, SessionListEntry,
    headless_host::{self, HeadlessHostAuthState},
    runtime::{
        one_shot::{OneShotApp, OneShotBackendAccess},
        rooms::RoomApplication,
    },
};
use kodosi_backend_client::{
    api::{BackendRoom, BackendSessionCard, BackendUserProfile},
    labels,
};
use kodosi_domain::permissions::ShareScope;

use serde::Serialize;

use super::{
    OWNER_DASHBOARD_URL, OWNER_SESSION_URL_PREFIX, auth::best_effort_auth_enrichment,
    output::OutputMode,
};

#[derive(Debug, Serialize)]
pub(in crate::cli) struct HostStatusOutput {
    pub(in crate::cli) running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::cli) pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::cli) started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::cli) auth: Option<HeadlessHostAuthState>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(in crate::cli) sessions: Vec<SessionListEntry>,
}

#[derive(Debug, Serialize)]
pub(in crate::cli) struct AuthStatusOutput {
    pub(in crate::cli) signed_in: bool,
    pub(in crate::cli) state: AuthStatusState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::cli) reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::cli) user: Option<CliUserOutput>,
    pub(in crate::cli) host_running: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(in crate::cli) rooms: Vec<CliRoomOutput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(in crate::cli) owned_remote_sessions: Vec<CliRemoteSessionOutput>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::cli) struct CliUserOutput {
    pub(in crate::cli) id: String,
    pub(in crate::cli) handle: String,
    pub(in crate::cli) display_name: String,
    pub(in crate::cli) email: Option<String>,
    pub(in crate::cli) avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::cli) struct CliRoomOutput {
    pub(in crate::cli) id: String,
    pub(in crate::cli) name: String,
    pub(in crate::cli) slug: String,
    pub(in crate::cli) owner_user_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::cli) struct CliRemoteSessionOutput {
    pub(in crate::cli) id: String,
    pub(in crate::cli) title: String,
    pub(in crate::cli) scope: String,
    pub(in crate::cli) access: String,
    pub(in crate::cli) status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::cli) enum AuthStatusState {
    Ready,
    RequiresLogin,
    WaitingForApproval,
    SignedOut,
    Expired,
    StorageUnavailable,
    CollaborationQuarantined,
    BackendUnconfigured,
}

impl AuthStatusState {
    pub(in crate::cli) const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::RequiresLogin => "requires_login",
            Self::WaitingForApproval => "waiting_for_approval",
            Self::SignedOut => "signed_out",
            Self::Expired => "expired",
            Self::StorageUnavailable => "storage_unavailable",
            Self::CollaborationQuarantined => "collaboration_quarantined",
            Self::BackendUnconfigured => "backend_unconfigured",
        }
    }
}

pub(in crate::cli) async fn collect_auth_status() -> Result<AuthStatusOutput> {
    if let Some(mut host) = headless_host::connect_existing_host().await? {
        let snapshot = host.snapshot().await?;
        return collect_host_auth_status(snapshot.auth).await;
    }

    let mut app = OneShotApp::load()?;
    let access_state = app.remote_command_access().await?;

    match access_state {
        OneShotBackendAccess::Ready => {
            let user = app.fetch_current_user().await.ok().map(CliUserOutput::from);
            let rooms = RoomApplication::new(app.runtime())
                .fetch_rooms()
                .await
                .unwrap_or_default()
                .into_iter()
                .map(CliRoomOutput::from)
                .collect();
            let owned_remote_sessions = app
                .fetch_my_sessions()
                .await
                .unwrap_or_default()
                .into_iter()
                .map(CliRemoteSessionOutput::from)
                .collect();
            Ok(AuthStatusOutput {
                signed_in: true,
                state: AuthStatusState::Ready,
                reason: None,
                user,
                host_running: false,
                rooms,
                owned_remote_sessions,
            })
        }
        OneShotBackendAccess::RequiresLogin(reason) => Ok(AuthStatusOutput {
            signed_in: false,
            state: AuthStatusState::RequiresLogin,
            reason: Some(reason),
            user: None,
            host_running: false,
            rooms: Vec::new(),
            owned_remote_sessions: Vec::new(),
        }),
        OneShotBackendAccess::SignedOut => Ok(AuthStatusOutput {
            signed_in: false,
            state: AuthStatusState::SignedOut,
            reason: None,
            user: None,
            host_running: false,
            rooms: Vec::new(),
            owned_remote_sessions: Vec::new(),
        }),
        OneShotBackendAccess::StorageUnavailable(reason) => Ok(AuthStatusOutput {
            signed_in: false,
            state: AuthStatusState::StorageUnavailable,
            reason: Some(reason),
            user: None,
            host_running: false,
            rooms: Vec::new(),
            owned_remote_sessions: Vec::new(),
        }),
        OneShotBackendAccess::CollaborationQuarantined(reason) => Ok(AuthStatusOutput {
            signed_in: true,
            state: AuthStatusState::CollaborationQuarantined,
            reason: Some(reason),
            user: None,
            host_running: false,
            rooms: Vec::new(),
            owned_remote_sessions: Vec::new(),
        }),
        OneShotBackendAccess::BackendUnconfigured => {
            Ok(AuthStatusOutput {
                signed_in: false,
                state: AuthStatusState::BackendUnconfigured,
                reason: Some(
                    "backend.api is not configured — local sessions still work, but login, rooms, and remote pickup need backend.api"
                        .to_owned(),
                ),
                user: None,
                host_running: false,
                rooms: Vec::new(),
                owned_remote_sessions: Vec::new(),
            })
        }
    }
}

pub(in crate::cli) async fn collect_host_auth_status(
    auth: HeadlessHostAuthState,
) -> Result<AuthStatusOutput> {
    match auth {
        HeadlessHostAuthState::Ready => {
            let (user, rooms, owned_remote_sessions) = best_effort_auth_enrichment().await;
            Ok(AuthStatusOutput {
                signed_in: true,
                state: AuthStatusState::Ready,
                reason: None,
                user,
                host_running: true,
                rooms,
                owned_remote_sessions,
            })
        }
        HeadlessHostAuthState::WaitingForApproval {
            user_code,
            verification_uri,
        } => Ok(AuthStatusOutput {
            signed_in: false,
            state: AuthStatusState::WaitingForApproval,
            reason: Some(format!(
                "device login pending at {verification_uri} with code {user_code}"
            )),
            user: None,
            host_running: true,
            rooms: Vec::new(),
            owned_remote_sessions: Vec::new(),
        }),
        HeadlessHostAuthState::RequiresLogin { reason } => {
            let (state, detail) = match reason {
                AuthRequiredReason::SignedOut => (AuthStatusState::SignedOut, None),
                AuthRequiredReason::Expired => (
                    AuthStatusState::Expired,
                    Some("the running host's backend session expired".to_owned()),
                ),
            };
            Ok(AuthStatusOutput {
                signed_in: false,
                state,
                reason: detail,
                user: None,
                host_running: true,
                rooms: Vec::new(),
                owned_remote_sessions: Vec::new(),
            })
        }
        HeadlessHostAuthState::Unknown => Ok(AuthStatusOutput {
            signed_in: false,
            state: AuthStatusState::RequiresLogin,
            reason: Some("the running host has not published auth state yet".to_owned()),
            user: None,
            host_running: true,
            rooms: Vec::new(),
            owned_remote_sessions: Vec::new(),
        }),
    }
}

pub(in crate::cli) async fn collect_host_status() -> Result<HostStatusOutput> {
    let running_info = headless_host::host_info().await?;
    let Some(info) = running_info else {
        return Ok(HostStatusOutput {
            running: false,
            pid: None,
            started_at: None,
            auth: None,
            sessions: Vec::new(),
        });
    };

    let mut client = headless_host::connect_existing_host()
        .await?
        .ok_or_else(|| AppError::Unsupported {
            reason: "host state exists but the host is unreachable".to_owned(),
        })?;
    let snapshot = client.snapshot().await?;
    let active_sessions = snapshot
        .sessions
        .iter()
        .filter(|session| session.is_active_local())
        .cloned()
        .collect::<Vec<_>>();
    Ok(HostStatusOutput {
        running: true,
        pid: Some(info.pid),
        started_at: Some(info.started_at),
        auth: Some(snapshot.auth),
        sessions: active_sessions,
    })
}

pub(in crate::cli) fn render_auth_status_human(
    output: OutputMode,
    status: &AuthStatusOutput,
) -> Result<()> {
    if status.signed_in {
        let name = status
            .user
            .as_ref()
            .map_or_else(|| "your account".to_owned(), display_name);
        output.write_line(format!("Signed in as {name}."))?;
        output.write_line(if status.host_running {
            "Local host: running"
        } else {
            "Local host: stopped"
        })?;
        if !output.quiet {
            output.write_line(format!(
                "Owned remote sessions: {}",
                status.owned_remote_sessions.len()
            ))?;
            output.write_line(format!("Rooms: {}", status.rooms.len()))?;
            write_signed_in_guidance(
                output,
                status.host_running,
                &status.rooms,
                &status.owned_remote_sessions,
            )?;
        }
        return Ok(());
    }

    if status.state == AuthStatusState::BackendUnconfigured {
        output.write_line("Backend API is not configured.")?;
        if let Some(reason) = &status.reason {
            output.write_line(reason)?;
        }
        return write_local_only_guidance(output);
    }

    if status.state == AuthStatusState::WaitingForApproval {
        output.write_line("Login is waiting for device approval.")?;
        if let Some(reason) = &status.reason {
            output.write_line(reason)?;
        }
        return Ok(());
    }

    output.write_line("Not signed in.")?;
    if let Some(reason) = &status.reason {
        output.write_line(reason)?;
    }
    output.write_line("Run `kodosi auth login` to sign in.")?;
    if !output.quiet {
        output.write_line(
            "Start a local session without login: kodosi session start --name <name>",
        )?;
    }
    Ok(())
}

pub(in crate::cli) fn render_host_status_human(
    output: OutputMode,
    status: &HostStatusOutput,
) -> Result<()> {
    if !status.running {
        output.write_line("Headless host is not running.")?;
        return Ok(());
    }

    output.write_line("Headless host is running.")?;
    if let Some(pid) = status.pid {
        output.write_line(format!("PID: {pid}"))?;
    }
    if let Some(started_at) = &status.started_at {
        output.write_line(format!("Started at: {started_at}"))?;
    }
    output.write_line(format!("Local sessions: {}", status.sessions.len()))?;
    if let Some(auth) = &status.auth {
        output.write_line(match auth {
            HeadlessHostAuthState::Ready => "Auth: ready".to_owned(),
            HeadlessHostAuthState::RequiresLogin {
                reason: AuthRequiredReason::SignedOut,
            } => "Auth: signed out".to_owned(),
            HeadlessHostAuthState::RequiresLogin {
                reason: AuthRequiredReason::Expired,
            } => "Auth: expired".to_owned(),
            HeadlessHostAuthState::WaitingForApproval { .. } => {
                "Auth: waiting for device approval".to_owned()
            }
            HeadlessHostAuthState::Unknown => "Auth: unknown".to_owned(),
        })?;
    }
    if !output.quiet && !status.sessions.is_empty() {
        output.write_line(
            "Inspect sessions with `kodosi session list` or `kodosi session show <id>`.",
        )?;
    }
    Ok(())
}

pub(in crate::cli) fn write_signed_in_guidance(
    output: OutputMode,
    host_running: bool,
    rooms: &[CliRoomOutput],
    owned_remote_sessions: &[CliRemoteSessionOutput],
) -> Result<()> {
    output.write_line("")?;
    output.write_line(if host_running {
        "A local headless host is already running."
    } else {
        "No local headless host is running yet."
    })?;
    output.write_line(
        "Start a cross-device session: kodosi session start --name server --scope my-devices",
    )?;
    output.write_line("List your own remote sessions: kodosi session list --owned-remote")?;
    output.write_line("List your rooms: kodosi room list")?;
    output.write_line(format!("Open your dashboard: {OWNER_DASHBOARD_URL}"))?;
    if !rooms.is_empty() {
        output.write_line("")?;
        output.write_line("Available rooms:")?;
        for room in rooms {
            output.write_line(format!("  {}  {} ({})", room.id, room.name, room.slug))?;
        }
        output.write_line(
            "Use one when you start a session: kodosi session start --scope room --room <id>",
        )?;
    }
    if !owned_remote_sessions.is_empty() {
        output.write_line("")?;
        output.write_line("Owned remote sessions:")?;
        for session in owned_remote_sessions.iter().take(5) {
            output.write_line(format!(
                "  {}  {} [{} / {}]",
                session.id, session.title, session.scope, session.status
            ))?;
        }
        output.write_line("Pick one up on the web or in the signed-in desktop app:")?;
        output.write_line(format!("  Web: {OWNER_SESSION_URL_PREFIX}/<session-id>"))?;
        output.write_line("  Desktop: open the session from the signed-in sidebar")?;
    }
    Ok(())
}

pub(in crate::cli) fn write_local_only_guidance(output: OutputMode) -> Result<()> {
    output.write_line("Local sessions still work with `kodosi session start --name <name>`.")?;
    if !output.quiet {
        output.write_line("Check the local host: kodosi host status")?;
        output.write_line("Inspect local sessions: kodosi session list")?;
    }
    Ok(())
}

pub(in crate::cli) fn write_session_follow_up_guidance(
    output: OutputMode,
    session: &SessionListEntry,
) -> Result<()> {
    output.write_line(format!(
        "Inspect this session: kodosi session show {}",
        session.id()
    ))?;
    if session.is_active_local() {
        output.write_line(format!("Stop it: kodosi session stop {}", session.id()))?;
    }
    match session.scope() {
        ShareScope::JustMe => {
            output.write_line(format!(
                "Share across your devices: kodosi share set {} --scope my-devices",
                session.id()
            ))?;
            output.write_line(format!(
                "Share to a room: kodosi share set {} --scope room --room <id>",
                session.id()
            ))?;
        }
        ShareScope::MyDevices => {
            output.write_line(format!(
                "Share to a room: kodosi share set {} --scope room --room <id>",
                session.id()
            ))?;
            output.write_line(format!(
                "Stop sharing across devices: kodosi share set {} --scope just-me",
                session.id()
            ))?;
        }
        ShareScope::Friends | ShareScope::Room => {
            output.write_line(format!(
                "Disable sharing: kodosi share off {}",
                session.id()
            ))?;
        }
    }
    Ok(())
}

pub(in crate::cli) fn display_name(profile: &CliUserOutput) -> String {
    if profile.display_name.trim().is_empty() {
        profile.handle.clone()
    } else {
        profile.display_name.clone()
    }
}

impl From<BackendUserProfile> for CliUserOutput {
    fn from(dto: BackendUserProfile) -> Self {
        Self {
            id: dto.id,
            handle: dto.handle,
            display_name: dto.display_name,
            email: dto.email,
            avatar_url: dto.avatar_url,
        }
    }
}

impl From<BackendRoom> for CliRoomOutput {
    fn from(dto: BackendRoom) -> Self {
        Self {
            id: dto.id,
            name: dto.name,
            slug: dto.slug,
            owner_user_id: dto.owner_user_id,
        }
    }
}

impl From<BackendSessionCard> for CliRemoteSessionOutput {
    fn from(dto: BackendSessionCard) -> Self {
        Self {
            id: dto.id,
            title: dto.title,
            scope: labels::share_scope_label(dto.scope).to_owned(),
            access: labels::access_level_label(dto.access).to_owned(),
            status: labels::session_state_label(dto.status).to_owned(),
        }
    }
}
