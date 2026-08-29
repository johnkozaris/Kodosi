use std::env;

use kodosi_domain::ids::SessionId;
use time::OffsetDateTime;

use super::args::{CliMsgAckArgs, CliMsgAction, CliMsgInboxArgs, CliMsgSendArgs};
use super::output::OutputMode;
use crate::rooms::mailbox_store::{AgentProfile, AgentRoomStore, MailboxDestination, MailboxEntry};
use crate::{AppError, Result};

pub(in crate::cli) fn run_msg_command(action: CliMsgAction, output: OutputMode) -> Result<()> {
    match action {
        CliMsgAction::Send(args) => send(args, output),
        CliMsgAction::Inbox(args) => inbox(&args, output),
        CliMsgAction::Ack(args) => ack(&args, output),
    }
}

fn send(args: CliMsgSendArgs, output: OutputMode) -> Result<()> {
    let from = current_session_id()?;
    let target =
        SessionId::parse_field(&args.target_session_id, "targetSessionId").map_err(|e| {
            AppError::Unsupported {
                reason: format!("target session id is malformed: {e}"),
            }
        })?;
    let store = AgentRoomStore::open().map_err(|e| io_to_app(&e))?;
    let profiles = store.list_profiles().map_err(|e| io_to_app(&e))?;
    let from_description = profiles
        .iter()
        .find(|profile| profile.session_id == from)
        .map(|p| p.description.clone());
    let request_id = format!("req-{}", OffsetDateTime::now_utc().unix_timestamp_nanos());
    let entry = MailboxEntry::AgentMessage {
        request_id: request_id.clone(),
        from_session_id: from,
        from_description,
        body: args.body,
        in_reply_to: args.in_reply_to,
        at: OffsetDateTime::now_utc(),
    };
    let destination =
        described_mailbox_destination(&profiles, target).ok_or_else(|| AppError::Unsupported {
            reason: format!(
                "target session {target} has no current incarnation-scoped agent profile"
            ),
        })?;
    store
        .enqueue_for(destination, &entry)
        .map_err(|e| io_to_app(&e))?;
    if output.json {
        return output.write_json(&serde_json::json!({
            "requestId": request_id,
            "from": from.to_string(),
            "target": target.to_string(),
        }));
    }
    output.write_line(format!("Sent message {request_id} to {target}."))
}

fn inbox(args: &CliMsgInboxArgs, output: OutputMode) -> Result<()> {
    if output.quiet && !output.json {
        return Err(AppError::Unsupported {
            reason: "`msg inbox` cannot use --quiet because unread messages would be acknowledged without being shown; use --json for machine-readable delivery".to_owned(),
        });
    }
    let destination = current_mailbox_destination()?;
    let store = AgentRoomStore::open().map_err(|e| io_to_app(&e))?;

    let _read_guard = store
        .lock_mailbox_read(destination)
        .map_err(|e| io_to_app(&e))?;
    let limit = args.limit.clamp(1, 500);
    let (entries, cursor) = args
        .since
        .map_or_else(
            || store.drain_unread_page(destination, limit),
            |since| store.read_after_cursor_page(destination, since, limit),
        )
        .map_err(|e| io_to_app(&e))?;

    if output.json {
        output.write_json(&serde_json::json!({
            "sessionId": destination.session_id.to_string(),
            "sessionIncarnationId": destination.incarnation_id.to_string(),
            "cursor": cursor,
            "acknowledged": !args.peek,
            "entries": entries,
        }))?;
    } else if entries.is_empty() {
        output.write_line("No unread agent or room messages.")?;
    } else {
        for entry in &entries {
            match entry {
                MailboxEntry::AgentMessage {
                    request_id,
                    from_session_id,
                    from_description,
                    body,
                    ..
                } => {
                    let sender = from_description.as_deref().unwrap_or("Agent");
                    output.write_line(format!(
                        "{sender} ({from_session_id}, req={request_id}): {body}"
                    ))?;
                }
                MailboxEntry::RoomChat {
                    room_name,
                    recipient_session_ids,
                    recipient_user_ids,
                    body,
                    seq,
                    ..
                } => {
                    let scope = if recipient_session_ids.is_empty() && recipient_user_ids.is_empty()
                    {
                        "Mission broadcast".to_owned()
                    } else {
                        format!(
                            "Directed sessions=[{}] users=[{}]",
                            recipient_session_ids
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(","),
                            recipient_user_ids
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(",")
                        )
                    };
                    output.write_line(format!(
                        "{} chat #{} [{}]: {}",
                        room_name.as_deref().unwrap_or("Room"),
                        seq,
                        scope,
                        body
                    ))?;
                }
                MailboxEntry::RoomTask {
                    room_name,
                    title,
                    status,
                    ..
                } => {
                    output.write_line(format!(
                        "{} task [{}]: {}",
                        room_name.as_deref().unwrap_or("Room"),
                        status,
                        title
                    ))?;
                }
            }
        }
    }
    if args.peek {
        Ok(())
    } else {
        store
            .set_cursor(destination, cursor)
            .map_err(|e| io_to_app(&e))
    }
}

fn ack(args: &CliMsgAckArgs, output: OutputMode) -> Result<()> {
    let destination = current_mailbox_destination()?;
    let session = destination.session_id;
    let store = AgentRoomStore::open().map_err(|error| io_to_app(&error))?;
    let _read_guard = store
        .lock_mailbox_read(destination)
        .map_err(|error| io_to_app(&error))?;
    let (_, current) = store
        .drain_unread_page(destination, 0)
        .map_err(|error| io_to_app(&error))?;
    if args.cursor < current {
        return Err(AppError::Unsupported {
            reason: format!(
                "mailbox cursor {} is behind the committed cursor {current}",
                args.cursor
            ),
        });
    }
    store
        .read_after_cursor_page(destination, args.cursor, 0)
        .map_err(|error| io_to_app(&error))?;
    store
        .set_cursor(destination, args.cursor)
        .map_err(|error| io_to_app(&error))?;
    if output.json {
        return output.write_json(&serde_json::json!({
            "sessionId": session.to_string(),
            "sessionIncarnationId": destination.incarnation_id.to_string(),
            "cursor": args.cursor,
            "acknowledged": true,
        }));
    }
    output.write_line(format!(
        "Acknowledged mailbox through cursor {}.",
        args.cursor
    ))
}

fn described_mailbox_destination(
    profiles: &[AgentProfile],
    target: SessionId,
) -> Option<MailboxDestination> {
    profiles
        .iter()
        .find(|profile| profile.session_id == target)
        .and_then(|profile| profile.incarnation_id)
        .map(|incarnation_id| MailboxDestination::new(target, incarnation_id))
}

pub(in crate::cli) fn current_mailbox_destination() -> Result<MailboxDestination> {
    let session_id = current_session_id()?;
    let raw = env::var("KODOSI_SESSION_INCARNATION_ID").map_err(|_| AppError::Unsupported {
        reason: "KODOSI_SESSION_INCARNATION_ID env var is not set; this command must run inside the current Kodosi session incarnation".to_owned(),
    })?;
    let incarnation_id = uuid::Uuid::parse_str(&raw).map_err(|error| AppError::Unsupported {
        reason: format!("KODOSI_SESSION_INCARNATION_ID is malformed: {error}"),
    })?;
    Ok(MailboxDestination::new(session_id, incarnation_id))
}

pub(in crate::cli) fn current_session_id() -> Result<SessionId> {
    let raw = env::var("KODOSI_SESSION_ID").map_err(|_| AppError::Unsupported {
        reason:
            "KODOSI_SESSION_ID env var is not set; this command must run inside a kodosi session"
                .to_owned(),
    })?;
    SessionId::parse_field(&raw, "KODOSI_SESSION_ID").map_err(|e| AppError::Unsupported {
        reason: format!("KODOSI_SESSION_ID is malformed: {e}"),
    })
}

pub(in crate::cli) fn io_to_app(error: &std::io::Error) -> AppError {
    AppError::Unsupported {
        reason: format!("agent-room store I/O error: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::described_mailbox_destination;
    use crate::rooms::mailbox_store::AgentProfile;
    use kodosi_domain::ids::{SessionId, UserId};
    use time::OffsetDateTime;

    fn profile(session_id: SessionId, incarnation_id: Option<uuid::Uuid>) -> AgentProfile {
        AgentProfile {
            session_id,
            incarnation_id,
            owner_user_id: UserId::try_from("01900000-0000-7000-8000-000000000001")
                .expect("user id"),
            agent_kind: "Claude".to_owned(),
            cwd: None,
            description: "agent".to_owned(),
            last_described_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn direct_messages_use_described_incarnation_and_isolate_legacy_profiles() {
        let session_id = SessionId::new();
        let incarnation_id = uuid::Uuid::now_v7();
        assert_eq!(
            described_mailbox_destination(&[profile(session_id, Some(incarnation_id))], session_id),
            Some(crate::rooms::mailbox_store::MailboxDestination::new(
                session_id,
                incarnation_id
            ))
        );
        assert_eq!(
            described_mailbox_destination(&[profile(session_id, None)], session_id),
            None
        );
    }
}
