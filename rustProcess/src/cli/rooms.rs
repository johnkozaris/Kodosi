use std::env;

use kodosi_domain::{ids::SessionId, session::SessionState};
use serde::Serialize;
use uuid::Uuid;

use super::{
    args::{
        CliRoomAction, CliRoomChatAction, CliRoomChatListArgs, CliRoomChatPeersArgs,
        CliRoomChatPostArgs, CliRoomTaskAction, CliRoomTaskCreateArgs, CliRoomTaskDoneArgs,
        CliRoomTaskListArgs, CliRoomTasksArgs,
    },
    client::ensure_remote_command_access,
    output::OutputMode,
};
use crate::{AppError, Result, host_protocol::RoomCommand, rooms::mailbox_store::AgentRoomStore};

pub(in crate::cli) async fn run_room_command(
    command: CliRoomAction,
    output: OutputMode,
) -> Result<()> {
    match command {
        CliRoomAction::List => run_list(output).await,
        CliRoomAction::Chat(CliRoomChatAction::List(args)) => run_chat_list(args, output).await,
        CliRoomAction::Chat(CliRoomChatAction::Peers(args)) => run_chat_peers(args, output).await,
        CliRoomAction::Chat(CliRoomChatAction::Post(args)) => run_chat_post(args, output).await,
        CliRoomAction::Tasks(action) => run_room_task_command(action, output).await,
    }
}

async fn run_room_task_command(action: CliRoomTaskAction, output: OutputMode) -> Result<()> {
    match action {
        CliRoomTaskAction::Mine(args) => run_tasks_mine(args, output).await,
        CliRoomTaskAction::List(args) => run_tasks_list(args, output).await,
        CliRoomTaskAction::Create(args) => run_task_create(args, output).await,
        CliRoomTaskAction::Assign(args) => {
            run_task_assignment(
                args.room,
                args.task_id,
                Some(args.session_id),
                None,
                None,
                output,
            )
            .await
        }
        CliRoomTaskAction::Unassign(args) => {
            run_task_assignment(
                args.room,
                args.task_id,
                None,
                Some(args.expected_revision),
                args.request_id,
                output,
            )
            .await
        }
        CliRoomTaskAction::Claim(args) => {
            run_task_transition(
                args.room,
                args.task_id,
                args.expected_revision,
                "InProgress",
                None,
                args.request_id,
                output,
            )
            .await
        }
        CliRoomTaskAction::Submit(args) => {
            run_task_transition(
                args.room,
                args.task_id,
                args.expected_revision,
                "Review",
                None,
                args.request_id,
                output,
            )
            .await
        }
        CliRoomTaskAction::Done(args) => {
            let CliRoomTaskDoneArgs {
                room,
                task_id,
                result,
                expected_revision,
                request_id,
            } = args;
            run_task_transition(
                room,
                task_id,
                expected_revision,
                "Done",
                Some(result),
                request_id,
                output,
            )
            .await
        }
        CliRoomTaskAction::Archive(args) => {
            run_task_transition(
                args.room,
                args.task_id,
                args.expected_revision,
                "Archived",
                None,
                args.request_id,
                output,
            )
            .await
        }
        CliRoomTaskAction::Reopen(args) => {
            run_task_transition(
                args.room,
                args.task_id,
                args.expected_revision,
                "Open",
                None,
                args.request_id,
                output,
            )
            .await
        }
    }
}

async fn run_chat_list(args: CliRoomChatListArgs, output: OutputMode) -> Result<()> {
    let mut app = crate::runtime::one_shot::OneShotApp::load()?;
    ensure_remote_command_access(&mut app, "kodosi room chat list").await?;
    let rooms = crate::runtime::rooms::RoomApplication::new(app.runtime());
    let room = rooms.resolve_room(&args.room).await?;
    let messages = rooms
        .fetch_room_chat(&room.id, args.since, Some(args.limit))
        .await?;
    if output.json {
        return output.write_json(&messages);
    }
    if messages.is_empty() {
        return output.write_line("No matching messages in this room.");
    }
    let session_names = rooms
        .fetch_room_sessions(&room.id)
        .await?
        .into_iter()
        .map(|session| (session.id, session.title))
        .collect::<std::collections::HashMap<_, _>>();
    let member_names = rooms
        .fetch_room_members(&room.id)
        .await?
        .into_iter()
        .map(|member| {
            let label = member
                .display_name
                .or(member.username)
                .unwrap_or_else(|| "Room member".to_owned());
            (member.user_id, label)
        })
        .collect::<std::collections::HashMap<_, _>>();
    for message in messages {
        let author = message
            .author_session_id
            .as_ref()
            .and_then(|session_id| session_names.get(session_id))
            .or_else(|| member_names.get(&message.author_user_id))
            .map_or("Room member", String::as_str);
        let scope =
            if message.recipient_session_ids.is_empty() && message.recipient_user_ids.is_empty() {
                "Mission broadcast".to_owned()
            } else {
                format!(
                    "Directed sessions=[{}] users=[{}]",
                    message.recipient_session_ids.join(","),
                    message.recipient_user_ids.join(",")
                )
            };
        output.write_line(format!(
            "{}  {}  [{}]: {}",
            message.seq, author, scope, message.body
        ))?;
    }
    Ok(())
}

async fn run_list(output: OutputMode) -> Result<()> {
    let mut app = crate::runtime::one_shot::OneShotApp::load()?;
    ensure_remote_command_access(&mut app, "kodosi room list").await?;
    let rooms = crate::runtime::rooms::RoomApplication::new(app.runtime())
        .fetch_rooms()
        .await?;
    if output.json {
        return output.write_json(&rooms);
    }
    if rooms.is_empty() {
        output.write_line("No rooms found.")?;
        return Ok(());
    }
    for room in rooms {
        output.write_line(format!("{}  {}  ({})", room.slug, room.name, room.id))?;
    }
    Ok(())
}

async fn run_chat_post(args: CliRoomChatPostArgs, output: OutputMode) -> Result<()> {
    let mut app = crate::runtime::one_shot::OneShotApp::load()?;
    ensure_remote_command_access(&mut app, "kodosi room chat post").await?;
    let rooms = crate::runtime::rooms::RoomApplication::new(app.runtime());
    let room = rooms.resolve_room(&args.room).await?;
    let caller = current_agent_room_identity(&room.id).await?;
    let author_session_id = caller
        .as_ref()
        .map(|identity| identity.backend_session_id.clone());
    let author_kind = if author_session_id.is_some() {
        "Agent"
    } else {
        "Human"
    };
    RoomCommand::ChatPost {
        room_id: room.id.clone(),
        body: args.body.clone(),
        author_session_id: author_session_id.clone(),
        recipient_session_ids: args.recipient_session_ids.clone(),
        recipient_user_ids: args.recipient_user_ids.clone(),
        request_id: args.request_id.clone(),
    }
    .validate()
    .map_err(|error| AppError::Unsupported {
        reason: error.to_string(),
    })?;
    let message_id = args
        .request_id
        .unwrap_or_else(|| Uuid::now_v7().to_string());
    let mutation = rooms
        .post_room_chat(
            &message_id,
            &room.id,
            &args.body,
            author_session_id.as_deref(),
            author_kind,
            &args.recipient_session_ids,
            &args.recipient_user_ids,
        )
        .await?;
    match mutation.projection {
        Ok(posted) if output.json => output.write_json(&posted),
        Ok(posted) => output.write_line(format!("posted seq={} id={}", posted.seq, posted.id)),
        Err(error) => write_committed_unverified(output, &mutation.entity_id, &error),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatPeer {
    target_kind: &'static str,
    stable_id: String,
    label: String,
    session_status: Option<String>,
    agent_mailbox_delivery_available: bool,
}

async fn run_chat_peers(args: CliRoomChatPeersArgs, output: OutputMode) -> Result<()> {
    let mut app = crate::runtime::one_shot::OneShotApp::load()?;
    ensure_remote_command_access(&mut app, "kodosi room chat peers").await?;
    let rooms = crate::runtime::rooms::RoomApplication::new(app.runtime());
    let room = rooms.resolve_room(&args.room).await?;
    let sessions = rooms.fetch_room_sessions(&room.id).await?;
    let members = rooms.fetch_room_members(&room.id).await?;

    let local_profile_ids = app
        .current_user_id_via_auth()
        .map(|user_id| {
            AgentRoomStore::open()
                .and_then(|store| store.visible_to(user_id))
                .map(|profiles| {
                    profiles
                        .into_iter()
                        .map(|profile| profile.session_id.to_string())
                        .collect::<std::collections::HashSet<_>>()
                })
        })
        .transpose()
        .map_err(|error| super::msg::io_to_app(&error))?
        .unwrap_or_default();

    let mut peers = Vec::with_capacity(sessions.len() + members.len());
    for session in sessions {
        let mailbox_available = local_profile_ids.contains(&session.id)
            && !matches!(
                session.status,
                SessionState::Stopping | SessionState::Stopped | SessionState::Failed
            );
        peers.push(ChatPeer {
            target_kind: "session",
            stable_id: session.id,
            label: session.title,
            session_status: Some(format!("{:?}", session.status)),
            agent_mailbox_delivery_available: mailbox_available,
        });
    }
    for member in members {
        peers.push(ChatPeer {
            target_kind: "user",
            stable_id: member.user_id,
            label: member
                .display_name
                .or(member.username)
                .unwrap_or_else(|| "Room member".to_owned()),
            session_status: None,
            agent_mailbox_delivery_available: false,
        });
    }
    peers.sort_by(|left, right| {
        left.target_kind
            .cmp(right.target_kind)
            .then_with(|| left.stable_id.cmp(&right.stable_id))
    });

    if output.json {
        return output.write_json(&peers);
    }
    if peers.is_empty() {
        return output.write_line("No chat targets found in this room.");
    }
    for peer in peers {
        output.write_line(format!(
            "{}  {}  {}  status={}  agent-mailbox={}",
            peer.target_kind,
            peer.stable_id,
            peer.label,
            peer.session_status.as_deref().unwrap_or("n/a"),
            if peer.agent_mailbox_delivery_available {
                "available"
            } else {
                "unavailable"
            }
        ))?;
    }
    Ok(())
}

async fn run_tasks_mine(args: CliRoomTasksArgs, output: OutputMode) -> Result<()> {
    let mut app = crate::runtime::one_shot::OneShotApp::load()?;
    ensure_remote_command_access(&mut app, "kodosi room tasks mine").await?;
    let rooms = crate::runtime::rooms::RoomApplication::new(app.runtime());
    let room = rooms.resolve_room(&args.room).await?;
    let session_id = current_agent_room_identity(&room.id)
        .await?
        .ok_or_else(|| AppError::Unsupported {
            reason: "`room tasks mine` requires KODOSI_SESSION_ID and KODOSI_SESSION_INCARNATION_ID and must run inside a live room-attached Kodosi session".to_owned(),
        })?
        .backend_session_id;
    let tasks = rooms
        .fetch_room_tasks(&room.id, None, Some(session_id.as_str()))
        .await?;
    write_tasks(
        app.runtime(),
        &room.id,
        output,
        &tasks,
        "No tasks assigned to your session in this room.",
    )
    .await
}

async fn run_tasks_list(args: CliRoomTaskListArgs, output: OutputMode) -> Result<()> {
    let mut app = crate::runtime::one_shot::OneShotApp::load()?;
    ensure_remote_command_access(&mut app, "kodosi room tasks list").await?;
    let rooms = crate::runtime::rooms::RoomApplication::new(app.runtime());
    let room = rooms.resolve_room(&args.room).await?;
    let tasks = rooms
        .fetch_room_tasks(&room.id, args.status.as_deref(), args.assignee.as_deref())
        .await?;
    write_tasks(
        app.runtime(),
        &room.id,
        output,
        &tasks,
        "No matching tasks in this room.",
    )
    .await
}

async fn run_task_create(args: CliRoomTaskCreateArgs, output: OutputMode) -> Result<()> {
    let mut app = crate::runtime::one_shot::OneShotApp::load()?;
    ensure_remote_command_access(&mut app, "kodosi room tasks create").await?;
    let rooms = crate::runtime::rooms::RoomApplication::new(app.runtime());
    let room = rooms.resolve_room(&args.room).await?;
    let due_at = args
        .due_at
        .as_deref()
        .map(|value| {
            time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
                .map_err(|error| AppError::Unsupported {
                    reason: format!("--due-at must be RFC3339: {error}"),
                })
        })
        .transpose()?;
    let assigned_session_incarnation_id = match args.session.as_deref() {
        Some(session_id) => Some(
            rooms
                .resolve_room_session_incarnation(&room.id, session_id)
                .await?,
        ),
        None => None,
    };
    let task_id = Uuid::now_v7().to_string();
    let task = rooms
        .create_room_task(
            &task_id,
            &room.id,
            args.title,
            args.description,
            args.session.as_deref(),
            assigned_session_incarnation_id.as_deref(),
            due_at,
        )
        .await?
        .into_cli_result()?;
    write_task(output, &task)
}

async fn run_task_assignment(
    room: String,
    task_id: String,
    session_id: Option<String>,
    expected_revision: Option<i64>,
    request_id: Option<String>,
    output: OutputMode,
) -> Result<()> {
    let mut app = crate::runtime::one_shot::OneShotApp::load()?;
    ensure_remote_command_access(&mut app, "kodosi room tasks assign").await?;
    let rooms = crate::runtime::rooms::RoomApplication::new(app.runtime());
    let room = rooms.resolve_room(&room).await?;
    let expected_task_revision = match expected_revision {
        Some(revision) => revision,
        None => rooms.fetch_room_task(&room.id, &task_id).await?.revision,
    };
    let session_incarnation_id = match session_id.as_deref() {
        Some(session_id) => Some(
            rooms
                .resolve_room_session_incarnation(&room.id, session_id)
                .await?,
        ),
        None => None,
    };
    let request_id = request_id.unwrap_or_else(|| Uuid::now_v7().to_string());
    let command = RoomCommand::TaskAssign {
        room_id: room.id.clone(),
        task_id: task_id.clone(),
        expected_task_revision,
        session_id,
        session_incarnation_id,
        request_id,
    };
    let (mut host, _) = crate::headless_host::ensure_host_running().await?;
    host.send_room_action(command).await?;
    let task = rooms
        .fetch_room_tasks(&room.id, None, None)
        .await?
        .into_iter()
        .find(|task| task.id == task_id)
        .ok_or(AppError::NotFound)?;
    write_task(output, &task)
}

async fn run_task_transition(
    room: String,
    task_id: String,
    expected_revision: i64,
    to_status: &str,
    result: Option<String>,
    request_id: Option<String>,
    output: OutputMode,
) -> Result<()> {
    let mut app = crate::runtime::one_shot::OneShotApp::load()?;
    ensure_remote_command_access(&mut app, "kodosi room tasks").await?;
    let rooms = crate::runtime::rooms::RoomApplication::new(app.runtime());
    let room = rooms.resolve_room(&room).await?;
    let caller = current_agent_room_identity(&room.id).await?;
    let request_id = request_id.unwrap_or_else(|| Uuid::now_v7().to_string());
    let command = RoomCommand::TaskTransition {
        room_id: room.id.clone(),
        task_id: task_id.clone(),
        expected_task_revision: expected_revision,
        to_status: to_status.to_owned(),
        actor_session_id: caller
            .as_ref()
            .map(|identity| identity.backend_session_id.clone()),
        actor_session_incarnation_id: caller
            .as_ref()
            .map(|identity| identity.backend_incarnation_id.to_string()),
        result,
        request_id,
    };
    let (mut host, _) = crate::headless_host::ensure_host_running().await?;
    host.send_room_action(command).await?;
    match rooms.fetch_room_task(&room.id, &task_id).await {
        Ok(updated) => write_task(output, &updated),
        Err(error) => write_committed_unverified(output, &task_id, &error),
    }
}

fn write_committed_unverified(output: OutputMode, entity_id: &str, error: &AppError) -> Result<()> {
    let warning = format!("committed, but the local projection could not be verified: {error}");
    if output.json {
        return output.write_json(&serde_json::json!({
            "entityId": entity_id,
            "committed": true,
            "verified": false,
            "warning": warning,
        }));
    }
    output.write_line(format!("{entity_id}  [committed, projection unavailable]"))
}

async fn write_tasks(
    runtime: &crate::runtime::Runtime,
    room_id: &str,
    output: OutputMode,
    tasks: &[kodosi_backend_client::api::BackendRoomTask],
    empty_message: &str,
) -> Result<()> {
    if output.json {
        return output.write_json(&tasks);
    }
    if tasks.is_empty() {
        return output.write_line(empty_message);
    }
    let session_names = crate::runtime::rooms::RoomApplication::new(runtime)
        .fetch_room_sessions(room_id)
        .await?
        .into_iter()
        .map(|session| (session.id, session.title))
        .collect::<std::collections::HashMap<_, _>>();
    for task in tasks {
        let assignee = task
            .assigned_session_id
            .as_ref()
            .and_then(|session_id| session_names.get(session_id))
            .map_or("unassigned", String::as_str);
        output.write_line(format!(
            "{}  [{}]  rev={}  {}  -> {}",
            task.id, task.status, task.revision, task.title, assignee
        ))?;
    }
    Ok(())
}

fn write_task(
    output: OutputMode,
    task: &kodosi_backend_client::api::BackendRoomTask,
) -> Result<()> {
    if output.json {
        return output.write_json(task);
    }
    output.write_line(format!(
        "{}  [{}]  rev={}  {}",
        task.id, task.status, task.revision, task.title
    ))
}

#[derive(Debug)]
struct AgentRoomIdentity {
    backend_session_id: String,
    backend_incarnation_id: Uuid,
}

async fn current_agent_room_identity(room_id: &str) -> Result<Option<AgentRoomIdentity>> {
    let Some((session_id, local_incarnation_id)) = current_agent_environment()? else {
        return Ok(None);
    };
    let Some(mut host) = crate::headless_host::connect_existing_host().await? else {
        return Err(AppError::Unsupported {
            reason: "the calling agent session is no longer hosted by a live local Kodosi runtime"
                .to_owned(),
        });
    };
    let snapshot = host.snapshot().await?;
    resolve_agent_room_identity(
        &snapshot.sessions,
        session_id,
        local_incarnation_id,
        room_id,
    )
    .map(Some)
}

fn current_agent_environment() -> Result<Option<(SessionId, Uuid)>> {
    let session = env::var("KODOSI_SESSION_ID");
    let incarnation = env::var("KODOSI_SESSION_INCARNATION_ID");
    match (session, incarnation) {
        (Err(env::VarError::NotPresent), Err(env::VarError::NotPresent)) => Ok(None),
        (Err(env::VarError::NotUnicode(_)), _) => Err(AppError::Unsupported {
            reason: "KODOSI_SESSION_ID is not valid Unicode".to_owned(),
        }),
        (_, Err(env::VarError::NotUnicode(_))) => Err(AppError::Unsupported {
            reason: "KODOSI_SESSION_INCARNATION_ID is not valid Unicode".to_owned(),
        }),
        (Err(env::VarError::NotPresent), _) => Err(AppError::Unsupported {
            reason: "KODOSI_SESSION_ID is required when KODOSI_SESSION_INCARNATION_ID is set"
                .to_owned(),
        }),
        (_, Err(env::VarError::NotPresent)) => Err(AppError::Unsupported {
            reason: "KODOSI_SESSION_INCARNATION_ID is required when KODOSI_SESSION_ID is set"
                .to_owned(),
        }),
        (Ok(session), Ok(incarnation)) => {
            let session_id = SessionId::parse_field(&session, "KODOSI_SESSION_ID")?;
            let incarnation_id =
                Uuid::parse_str(&incarnation).map_err(|error| AppError::Unsupported {
                    reason: format!("KODOSI_SESSION_INCARNATION_ID is malformed: {error}"),
                })?;
            Ok(Some((session_id, incarnation_id)))
        }
    }
}

fn resolve_agent_room_identity(
    sessions: &[crate::SessionListEntry],
    local_session_id: SessionId,
    local_incarnation_id: Uuid,
    room_id: &str,
) -> Result<AgentRoomIdentity> {
    let expected_session_id = local_session_id.to_string();
    let local = sessions
        .iter()
        .find_map(|session| match session {
            crate::SessionListEntry::Local { entry } if entry.id == expected_session_id => {
                Some(entry)
            }
            _ => None,
        })
        .ok_or_else(|| AppError::Unsupported {
            reason: "the calling agent session is not a current local runtime record".to_owned(),
        })?;
    let current_local_incarnation =
        Uuid::parse_str(&local.incarnation_id).map_err(|error| AppError::InvalidBackendData {
            field: "sessions.incarnationId".to_owned(),
            reason: error.to_string(),
        })?;
    if current_local_incarnation != local_incarnation_id {
        return Err(AppError::Unsupported {
            reason: "the calling agent session incarnation is stale and has been replaced locally"
                .to_owned(),
        });
    }
    if matches!(
        local.status,
        crate::RuntimeSessionStatus::Stopping
            | crate::RuntimeSessionStatus::Stopped
            | crate::RuntimeSessionStatus::Blocked
    ) {
        return Err(AppError::Unsupported {
            reason: "the calling agent session is not live".to_owned(),
        });
    }
    if local.room_id.as_deref() != Some(room_id) {
        return Err(AppError::Unsupported {
            reason: "the calling agent session is not attached to the requested room".to_owned(),
        });
    }
    let backend_session_id =
        local
            .backend_session_id
            .as_deref()
            .ok_or_else(|| AppError::Unsupported {
                reason: "the calling agent session has no attached backend sharing identity"
                    .to_owned(),
            })?;
    let backend_session_id =
        SessionId::parse_field(backend_session_id, "backendSessionId")?.to_string();
    let backend_incarnation_id =
        local
            .backend_incarnation_id
            .as_deref()
            .ok_or_else(|| AppError::Unsupported {
                reason: "the calling agent session has no attached backend incarnation".to_owned(),
            })?;
    let backend_incarnation_id =
        Uuid::parse_str(backend_incarnation_id).map_err(|error| AppError::InvalidBackendData {
            field: "backendIncarnationId".to_owned(),
            reason: error.to_string(),
        })?;
    Ok(AgentRoomIdentity {
        backend_session_id,
        backend_incarnation_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        SessionListEntry,
        host_protocol::{LocalSessionListEntry, RuntimeSessionStatus, SessionSemanticActions},
    };
    use kodosi_domain::{
        permissions::{AccessLevel, ShareScope},
        session::{LocalSessionRecoveryState, SessionMode},
    };

    fn shared_local_session(
        local_session_id: SessionId,
        local_incarnation_id: Uuid,
        backend_session_id: SessionId,
        backend_incarnation_id: Uuid,
    ) -> SessionListEntry {
        SessionListEntry::Local {
            entry: LocalSessionListEntry {
                id: local_session_id.to_string(),
                incarnation_id: local_incarnation_id.to_string(),
                create_request_id: None,
                name: "Agent".to_owned(),
                project: "local".to_owned(),
                mode: SessionMode::Normal,
                status: RuntimeSessionStatus::Active,
                recovery: LocalSessionRecoveryState::Live,
                scope: ShareScope::Room,
                access: AccessLevel::Inject,
                room_id: Some("room-a".to_owned()),
                room_name: Some("Engineering".to_owned()),
                active_count: 1,
                entitled_count: 1,
                last_activity: "now".to_owned().into(),
                semantic_actions: SessionSemanticActions::default(),
                backend_session_id: Some(backend_session_id.to_string()),
                backend_incarnation_id: Some(backend_incarnation_id.to_string()),
                meta: None,
            },
        }
    }

    #[test]
    fn agent_room_identity_maps_local_to_backend_identity() {
        let local_session_id = SessionId::new();
        let local_incarnation_id = Uuid::now_v7();
        let backend_session_id = SessionId::new();
        let backend_incarnation_id = Uuid::now_v7();
        let sessions = [shared_local_session(
            local_session_id,
            local_incarnation_id,
            backend_session_id,
            backend_incarnation_id,
        )];

        let identity = resolve_agent_room_identity(
            &sessions,
            local_session_id,
            local_incarnation_id,
            "room-a",
        )
        .expect("current shared caller");

        assert_eq!(identity.backend_session_id, backend_session_id.to_string());
        assert_eq!(identity.backend_incarnation_id, backend_incarnation_id);
    }

    #[test]
    fn replaced_local_agent_incarnation_is_rejected_instead_of_remapped() {
        let local_session_id = SessionId::new();
        let current_incarnation_id = Uuid::now_v7();
        let stale_incarnation_id = Uuid::now_v7();
        let sessions = [shared_local_session(
            local_session_id,
            current_incarnation_id,
            SessionId::new(),
            Uuid::now_v7(),
        )];

        let error = resolve_agent_room_identity(
            &sessions,
            local_session_id,
            stale_incarnation_id,
            "room-a",
        )
        .expect_err("stale caller must not resolve to the replacement");

        assert!(error.to_string().contains("stale"));
    }
}
