use std::env;

use time::OffsetDateTime;

use super::args::{CliAgentAction, CliAgentDescribeArgs, CliAgentsArgs};
use super::client::{ensure_remote_command_access, resolve_local_command_identity};
use super::output::OutputMode;
use crate::rooms::mailbox_store::{AgentProfile, AgentRoomStore};
use crate::runtime::one_shot::OneShotApp;
use crate::{AppError, Result};
use kodosi_domain::ids::{SessionId, UserId};

pub(in crate::cli) async fn run_agent_command(
    action: CliAgentAction,
    output: OutputMode,
) -> Result<()> {
    match action {
        CliAgentAction::Describe(args) => describe(args, output).await,
    }
}

pub(in crate::cli) async fn run_agents_command(
    args: CliAgentsArgs,
    output: OutputMode,
) -> Result<()> {
    let mut app = OneShotApp::load()?;
    let store = AgentRoomStore::open().map_err(|e| super::msg::io_to_app(&e))?;
    let cached_user_id = cached_current_session_owner(&store)?;
    let viewer = if agents_require_remote_access(&args) {
        ensure_remote_command_access(&mut app, "kodosi agents").await?;
        app.current_user_id_via_auth()
            .ok_or_else(|| AppError::Unsupported {
                reason: "the signed-in account identity is unavailable for `kodosi agents`"
                    .to_owned(),
            })?
    } else {
        resolve_local_command_identity(&mut app, "kodosi agents", cached_user_id).await?
    };
    let mut profiles = store
        .visible_to(viewer)
        .map_err(|e| super::msg::io_to_app(&e))?;
    let mut room_sessions = Vec::new();
    if let Some(room_name) = args.room {
        let rooms = crate::runtime::rooms::RoomApplication::new(app.runtime());
        let room = rooms.resolve_room(&room_name).await?;
        room_sessions = rooms.fetch_room_sessions(&room.id).await?;
        let room_session_ids = room_sessions
            .iter()
            .map(|session| session.id.clone())
            .collect::<std::collections::HashSet<_>>();
        profiles.retain(|profile| room_session_ids.contains(&profile.session_id.to_string()));
    }
    if output.json {
        return output.write_json(&serde_json::json!({
            "profiles": profiles,
            "roomSessions": room_sessions,
        }));
    }
    if profiles.is_empty() && room_sessions.is_empty() {
        return output.write_line("No reachable agents found.");
    }
    let described_session_ids = profiles
        .iter()
        .map(|profile| profile.session_id.to_string())
        .collect::<std::collections::HashSet<_>>();
    for profile in &profiles {
        output.write_line(format!(
            "{}  {}  {}",
            profile.session_id, profile.agent_kind, profile.description
        ))?;
    }
    for session in room_sessions {
        if !described_session_ids.contains(&session.id) {
            output.write_line(format!("{}  Session  {}", session.id, session.title))?;
        }
    }
    Ok(())
}

async fn describe(args: CliAgentDescribeArgs, output: OutputMode) -> Result<()> {
    let destination = super::msg::current_mailbox_destination()?;
    let session_id = destination.session_id;
    let store = AgentRoomStore::open().map_err(|e| super::msg::io_to_app(&e))?;
    let cached_user_id = cached_owner_for_session(&store, session_id)?;
    let mut app = OneShotApp::load()?;
    let owner_user_id =
        resolve_local_command_identity(&mut app, "kodosi agent describe", cached_user_id).await?;
    let profile = AgentProfile {
        session_id,
        incarnation_id: Some(destination.incarnation_id),
        owner_user_id,
        agent_kind: detect_agent_kind(),
        cwd: env::current_dir().ok(),
        description: args.description,
        last_described_at: OffsetDateTime::now_utc(),
    };
    store
        .put_profile(&profile)
        .map_err(|e| super::msg::io_to_app(&e))?;
    if output.json {
        return output.write_json(&profile);
    }
    output.write_line(format!(
        "Published description for session {}.",
        profile.session_id
    ))
}

const fn agents_require_remote_access(args: &CliAgentsArgs) -> bool {
    args.room.is_some()
}

fn cached_current_session_owner(store: &AgentRoomStore) -> Result<Option<UserId>> {
    let session_id =
        match env::var("KODOSI_SESSION_ID") {
            Ok(raw) => Some(SessionId::parse_field(&raw, "KODOSI_SESSION_ID").map_err(
                |error| AppError::Unsupported {
                    reason: format!("KODOSI_SESSION_ID is malformed: {error}"),
                },
            )?),
            Err(env::VarError::NotPresent) => None,
            Err(env::VarError::NotUnicode(_)) => {
                return Err(AppError::Unsupported {
                    reason: "KODOSI_SESSION_ID is not valid Unicode".to_owned(),
                });
            }
        };
    session_id
        .map(|session_id| cached_owner_for_session(store, session_id))
        .transpose()
        .map(Option::flatten)
}

fn cached_owner_for_session(
    store: &AgentRoomStore,
    session_id: SessionId,
) -> Result<Option<UserId>> {
    Ok(store
        .list_profiles()
        .map_err(|error| super::msg::io_to_app(&error))?
        .into_iter()
        .find(|profile| profile.session_id == session_id)
        .map(|profile| profile.owner_user_id))
}

fn detect_agent_kind() -> String {
    if env::var("CLAUDE_SESSION_ID").is_ok() {
        "Claude".to_owned()
    } else if env::var("COPILOT_AGENT_PROMPT").is_ok() || env::var("COPILOT_HOME").is_ok() {
        "Copilot".to_owned()
    } else {
        "Unknown".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::agents_require_remote_access;
    use crate::cli::args::CliAgentsArgs;

    #[test]
    fn only_room_filtered_agent_listing_requires_remote_access() {
        assert!(!agents_require_remote_access(&CliAgentsArgs { room: None }));
        assert!(agents_require_remote_access(&CliAgentsArgs {
            room: Some("mission".to_owned()),
        }));
    }
}
