use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::{Duration, Instant};

use kodosi_domain::{
    ids::{SessionId, UserId},
    session::SessionState,
};

use crate::{
    AppError, Result,
    host_protocol::{RoomChatEntry, RoomEvent, RoomTaskEntry},
    rooms::mailbox_store::{
        AgentRoomStore, MAX_ROOM_DELIVERY_CURSORS, MAX_ROOM_TASK_DELIVERY_CURSORS,
        MailboxDestination, MailboxEntry, RoomTaskDeliveryCursor, RoomTaskDeliveryReason,
    },
    session_runtime::events::DiscoverySurface,
};

use super::{
    Runtime,
    rooms::{MailboxRoomChatMessage, MailboxRoomChatPage, MailboxRoomTask, RoomApplication},
};

const CHAT_PAGE_SIZE: i32 = 200;
const TASK_PAGE_SIZE: usize = 500;
const MAX_CHAT_DELIVERIES_PER_SYNC: usize = 512;
const ROOM_SYNC_BATCH: usize = 256;
const ROOM_MAILBOX_RETRY_BASE: Duration = Duration::from_millis(250);
const ROOM_MAILBOX_RETRY_MAX: Duration = Duration::from_secs(30);

#[derive(Debug, Default)]
pub(super) struct RoomMailboxRetryState {
    failures: u8,
    retry_not_before: Option<Instant>,
}

impl RoomMailboxRetryState {
    pub(super) fn is_due(&self, now: Instant) -> bool {
        self.retry_not_before.is_none_or(|deadline| now >= deadline)
    }

    pub(super) fn record_failure(&mut self, now: Instant) -> Duration {
        let multiplier = 1_u32 << u32::from(self.failures.min(7));
        let delay = ROOM_MAILBOX_RETRY_BASE
            .saturating_mul(multiplier)
            .min(ROOM_MAILBOX_RETRY_MAX);
        self.failures = self.failures.saturating_add(1);
        self.retry_not_before = now.checked_add(delay).or(Some(now));
        delay
    }

    pub(super) fn reset(&mut self) {
        self.failures = 0;
        self.retry_not_before = None;
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct RoomMailboxSyncOutcome {
    pub(super) chat_more: bool,
    pub(super) tasks_more: bool,
}

#[derive(Debug, Default)]
struct ChatSyncProgress {
    completed_rooms: usize,
    last_visited_room_id: Option<String>,
    hit_delivery_limit: bool,
}

struct ChatPageProgress {
    cursor: i64,
    processed: usize,
    has_more: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RoomMailboxTarget {
    mailbox: MailboxDestination,
    backend_incarnation_id: uuid::Uuid,
}

pub(super) async fn refresh_room_projections(
    app: &mut Runtime,
    room_id: &str,
    surfaces: &BTreeSet<DiscoverySurface>,
) -> Result<()> {
    let room_application = RoomApplication::new(app);
    let chat = if surfaces.contains(&DiscoverySurface::RoomChat) {
        Some(
            room_application
                .fetch_room_chat_snapshot(room_id)
                .await?
                .into_iter()
                .map(map_chat_entry)
                .collect(),
        )
    } else {
        None
    };
    let tasks = if surfaces.contains(&DiscoverySurface::RoomTasks) {
        Some(
            room_application
                .fetch_room_tasks(room_id, None, None)
                .await?
                .into_iter()
                .map(map_task_entry)
                .collect(),
        )
    } else {
        None
    };
    if let Some(messages) = chat {
        app.state
            .runtime_outbox
            .queue_room(RoomEvent::ChatSnapshot {
                room_id: room_id.to_owned(),
                messages,
                hydration_id: None,
            });
    }
    if let Some(tasks) = tasks {
        app.state
            .runtime_outbox
            .queue_room(RoomEvent::TasksSnapshot {
                room_id: room_id.to_owned(),
                tasks,
            });
    }
    Ok(())
}

fn map_chat_entry(dto: kodosi_backend_client::api::BackendRoomChatMessage) -> RoomChatEntry {
    RoomChatEntry {
        id: dto.id,
        room_id: dto.room_id,
        author_user_id: dto.author_user_id,
        author_session_id: dto.author_session_id,
        author_kind: dto.author_kind,
        body: dto.body,
        recipient_session_ids: dto.recipient_session_ids,
        recipient_user_ids: dto.recipient_user_ids,
        seq: dto.seq,
        posted_at: dto
            .posted_at
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| dto.posted_at.to_string()),
    }
}

fn map_task_entry(dto: kodosi_backend_client::api::BackendRoomTask) -> RoomTaskEntry {
    let format = |value: time::OffsetDateTime| {
        value
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| value.to_string())
    };
    RoomTaskEntry {
        id: dto.id,
        room_id: dto.room_id,
        created_by_user_id: dto.created_by_user_id,
        title: dto.title,
        description: dto.description,
        status: dto.status,
        revision: dto.revision,
        assigned_session_id: dto.assigned_session_id,
        assigned_session_incarnation_id: dto.assigned_session_incarnation_id,
        due_at: dto.due_at.map(&format),
        created_at: format(dto.created_at),
        updated_at: format(dto.updated_at),
        completed_at: dto.completed_at.map(&format),
        result: dto.result,
        result_author_user_id: dto.result_author_user_id,
        content_unavailable: dto.content_unavailable,
    }
}

pub(super) async fn sync_invalidated(
    app: &Runtime,
    surfaces: &BTreeSet<DiscoverySurface>,
) -> Result<RoomMailboxSyncOutcome> {
    let sync_chat = surfaces.contains(&DiscoverySurface::RoomChat);
    let sync_tasks = surfaces.contains(&DiscoverySurface::RoomTasks);
    if !sync_chat && !sync_tasks {
        return Ok(RoomMailboxSyncOutcome::default());
    }

    let account_user_id = authenticated_account_user_id(app)?;
    let store = AgentRoomStore::open().map_err(AppError::Io)?;
    let agent_sessions = active_agent_sessions(app, &store, account_user_id)?;
    if agent_sessions.is_empty() {
        return Ok(RoomMailboxSyncOutcome::default());
    }
    let agent_sessions_by_room = active_agent_sessions_by_room(app, &agent_sessions)?;

    let room_application = RoomApplication::new(app);
    let mut rooms = room_application.fetch_rooms().await?;
    rooms.sort_by(|left, right| left.id.cmp(&right.id));
    ensure_room_delivery_capacity(rooms.len())?;
    let room_ids = rooms.iter().map(|room| room.id.clone()).collect::<Vec<_>>();
    let room_id_set = room_ids.iter().cloned().collect::<BTreeSet<_>>();
    let room_names = rooms
        .iter()
        .map(|room| (room.id.clone(), room.name.clone()))
        .collect::<HashMap<_, _>>();
    let mut delivery = store
        .load_room_delivery_state(account_user_id)
        .map_err(AppError::Io)?;
    delivery.retain_rooms(&room_id_set);
    let mut outcome = RoomMailboxSyncOutcome::default();

    if sync_chat {
        let remaining = delivery
            .chat_rooms_remaining
            .unwrap_or(rooms.len())
            .min(rooms.len());
        let (selected_ids, next_room_id) = rotating_room_batch(
            &room_ids,
            delivery.next_chat_room_id.as_deref(),
            remaining.min(ROOM_SYNC_BATCH),
        );
        let selected_rooms = selected_ids
            .iter()
            .filter_map(|room_id| rooms.iter().find(|room| room.id == *room_id))
            .cloned()
            .collect::<Vec<_>>();
        let progress = sync_chat_messages(
            &room_application,
            &store,
            &agent_sessions_by_room,
            &room_names,
            &selected_rooms,
            account_user_id,
            &mut delivery,
        )
        .await?;
        let remaining = chat_rooms_remaining_after_pass(remaining, rooms.len(), &progress);
        outcome.chat_more = progress.hit_delivery_limit || remaining > 0;
        let next_room_id = progress
            .last_visited_room_id
            .as_deref()
            .and_then(|room_id| next_room_after(&room_ids, room_id))
            .or(next_room_id);
        delivery.next_chat_room_id = next_room_id.filter(|_| outcome.chat_more);
        delivery.chat_rooms_remaining = outcome.chat_more.then_some(remaining);
    }
    if sync_tasks {
        let remaining = delivery
            .task_rooms_remaining
            .unwrap_or(rooms.len())
            .min(rooms.len());
        let (selected_ids, next_room_id) = rotating_room_batch(
            &room_ids,
            delivery.next_task_room_id.as_deref(),
            remaining.min(ROOM_SYNC_BATCH),
        );
        let selected_rooms = selected_ids
            .iter()
            .filter_map(|room_id| rooms.iter().find(|room| room.id == *room_id))
            .cloned()
            .collect::<Vec<_>>();
        sync_assigned_tasks(
            &room_application,
            &store,
            &agent_sessions_by_room,
            &room_names,
            &selected_rooms,
            account_user_id,
            &mut delivery,
        )
        .await?;
        let remaining = remaining.saturating_sub(selected_rooms.len());
        outcome.tasks_more = remaining > 0;
        delivery.next_task_room_id = next_room_id.filter(|_| outcome.tasks_more);
        delivery.task_rooms_remaining = outcome.tasks_more.then_some(remaining);
    }

    store
        .save_room_delivery_state(account_user_id, &delivery)
        .map_err(AppError::Io)?;
    Ok(outcome)
}

fn authenticated_account_user_id(app: &Runtime) -> Result<UserId> {
    let user_id = app
        .state
        .identity
        .auth
        .subject_string()
        .ok_or(AppError::Unauthorized)?;
    UserId::try_from(user_id.as_str()).map_err(|error| AppError::InvalidBackendData {
        field: "auth.subject".to_owned(),
        reason: error.to_string(),
    })
}

fn ensure_room_delivery_capacity(room_count: usize) -> Result<()> {
    if room_count <= MAX_ROOM_DELIVERY_CURSORS {
        return Ok(());
    }
    Err(AppError::Unsupported {
        reason: format!(
            "room mailbox delivery state has {room_count} rooms; the explicit account limit is {MAX_ROOM_DELIVERY_CURSORS}"
        ),
    })
}

fn rotating_room_batch(
    room_ids: &[String],
    next_room_id: Option<&str>,
    limit: usize,
) -> (Vec<String>, Option<String>) {
    if room_ids.is_empty() || limit == 0 {
        return (Vec::new(), None);
    }
    let start = next_room_id
        .and_then(|marker| {
            room_ids
                .iter()
                .position(|room_id| room_id.as_str() >= marker)
        })
        .unwrap_or(0);
    let count = room_ids.len().min(limit);
    let selected = (0..count)
        .map(|offset| room_ids[(start + offset) % room_ids.len()].clone())
        .collect();
    let next = (room_ids.len() > count).then(|| room_ids[(start + count) % room_ids.len()].clone());
    (selected, next)
}

fn next_room_after(room_ids: &[String], current_room_id: &str) -> Option<String> {
    let current = room_ids
        .iter()
        .position(|room_id| room_id == current_room_id)?;
    Some(room_ids[(current + 1) % room_ids.len()].clone())
}

fn chat_rooms_remaining_after_pass(
    current: usize,
    total_rooms: usize,
    progress: &ChatSyncProgress,
) -> usize {
    if progress.hit_delivery_limit {
        total_rooms
    } else {
        current.saturating_sub(progress.completed_rooms)
    }
}

fn active_agent_sessions(
    app: &Runtime,
    store: &AgentRoomStore,
    account_user_id: UserId,
) -> Result<BTreeMap<SessionId, MailboxDestination>> {
    let described = store
        .list_profiles()
        .map_err(AppError::Io)?
        .into_iter()
        .filter(|profile| profile.owner_user_id == account_user_id)
        .filter_map(|profile| {
            profile
                .incarnation_id
                .map(|incarnation_id| MailboxDestination::new(profile.session_id, incarnation_id))
        })
        .collect::<BTreeSet<_>>();

    Ok(app
        .state
        .local
        .sessions
        .ids()
        .iter()
        .copied()
        .filter_map(|session_id| {
            let record = app.state.local.sessions.record(session_id)?;
            let destination = MailboxDestination::new(session_id, record.local_incarnation_id);
            (!matches!(
                record.summary.state,
                SessionState::Stopping | SessionState::Stopped | SessionState::Failed
            ) && (record.summary.detected_agent.is_some() || described.contains(&destination)))
            .then_some((session_id, destination))
        })
        .collect())
}

fn active_agent_sessions_by_room(
    app: &Runtime,
    agent_sessions: &BTreeMap<SessionId, MailboxDestination>,
) -> Result<HashMap<String, BTreeMap<SessionId, RoomMailboxTarget>>> {
    let mut by_room = HashMap::<String, BTreeMap<SessionId, RoomMailboxTarget>>::new();
    for (session_id, mailbox) in agent_sessions {
        let Some(shared) = app.state.sharing.shared_sessions.get(*session_id) else {
            continue;
        };
        let Some(room) = shared.room() else {
            continue;
        };
        let backend_session_id =
            SessionId::parse_field(shared.backend_session_id(), "backendSessionId")?;
        by_room.entry(room.id.clone()).or_default().insert(
            backend_session_id,
            RoomMailboxTarget {
                mailbox: *mailbox,
                backend_incarnation_id: *shared.backend_incarnation_id(),
            },
        );
    }
    Ok(by_room)
}

async fn sync_chat_messages(
    room_application: &RoomApplication<'_>,
    store: &AgentRoomStore,
    agent_sessions_by_room: &HashMap<String, BTreeMap<SessionId, RoomMailboxTarget>>,
    room_names: &HashMap<String, String>,
    rooms: &[kodosi_backend_client::api::BackendRoom],
    account_user_id: UserId,
    delivery: &mut crate::rooms::mailbox_store::RoomDeliveryState,
) -> Result<ChatSyncProgress> {
    let mut delivered = 0_usize;
    let mut progress = ChatSyncProgress::default();
    for room in rooms {
        progress.last_visited_room_id = Some(room.id.clone());
        let mut cursor = delivery.chat_cursors.get(&room.id).copied().unwrap_or(0);
        loop {
            let remaining = MAX_CHAT_DELIVERIES_PER_SYNC.saturating_sub(delivered);
            if remaining == 0 {
                progress.hit_delivery_limit = true;
                return Ok(progress);
            }
            let request_limit = usize::try_from(CHAT_PAGE_SIZE)
                .unwrap_or(remaining)
                .min(remaining);
            let request_limit = i32::try_from(request_limit).unwrap_or(CHAT_PAGE_SIZE);
            let page = room_application
                .fetch_room_chat_for_mailbox(&room.id, Some(cursor), Some(request_limit))
                .await?;
            let page = process_chat_page(
                page,
                cursor,
                |message| {
                    deliver_chat_message(
                        store,
                        &room.id,
                        room_names,
                        agent_sessions_by_room,
                        message,
                    )
                },
                |id, message_room_id, seq, reason| {
                    tracing::warn!(
                        message_id = %id,
                        room_id = %message_room_id,
                        requested_room_id = %room.id,
                        seq,
                        %reason,
                        "quarantined terminally malformed room chat; content was not delivered"
                    );
                },
                |next_cursor| {
                    delivery.chat_cursors.insert(room.id.clone(), next_cursor);
                    store
                        .save_room_delivery_state(account_user_id, delivery)
                        .map_err(AppError::Io)
                },
            )?;
            cursor = page.cursor;
            delivered = delivered.saturating_add(page.processed);
            if delivered >= MAX_CHAT_DELIVERIES_PER_SYNC {
                progress.hit_delivery_limit = true;
                return Ok(progress);
            }
            if !page.has_more {
                break;
            }
        }
        progress.completed_rooms = progress.completed_rooms.saturating_add(1);
    }
    Ok(progress)
}

fn process_chat_page(
    mut page: MailboxRoomChatPage,
    mut cursor: i64,
    mut deliver: impl FnMut(kodosi_backend_client::api::BackendRoomChatMessage) -> Result<()>,
    mut quarantine: impl FnMut(&str, &str, i64, &str),
    mut commit: impl FnMut(i64) -> Result<()>,
) -> Result<ChatPageProgress> {
    page.items.sort_by_key(MailboxRoomChatMessage::seq);
    let has_more = page.has_more;
    let next_since = page.next_since;
    let mut processed = 0_usize;
    for message in page.items {
        let seq = message.seq();
        if seq <= cursor {
            continue;
        }
        match message {
            MailboxRoomChatMessage::Decrypted(message) => deliver(message)?,
            MailboxRoomChatMessage::Quarantined {
                id,
                room_id,
                seq,
                reason,
            } => quarantine(&id, &room_id, seq, &reason),
        }
        commit(seq)?;
        cursor = seq;
        processed = processed.saturating_add(1);
    }
    if has_more && next_since != Some(cursor) {
        return Err(AppError::InvalidBackendData {
            field: "roomChat.nextSince".to_owned(),
            reason: "continuation cursor did not match the processed page".to_owned(),
        });
    }
    Ok(ChatPageProgress {
        cursor,
        processed,
        has_more,
    })
}

fn deliver_chat_message(
    store: &AgentRoomStore,
    room_id: &str,
    room_names: &HashMap<String, String>,
    agent_sessions_by_room: &HashMap<String, BTreeMap<SessionId, RoomMailboxTarget>>,
    message: kodosi_backend_client::api::BackendRoomChatMessage,
) -> Result<()> {
    let author_user_id = UserId::try_from(message.author_user_id.as_str()).map_err(|error| {
        AppError::InvalidBackendData {
            field: "roomChat.authorUserId".to_owned(),
            reason: error.to_string(),
        }
    })?;
    let author_session_id = message
        .author_session_id
        .as_deref()
        .map(|session_id| SessionId::parse_field(session_id, "authorSessionId"))
        .transpose()?;
    let recipient_session_ids = message
        .recipient_session_ids
        .iter()
        .map(|session_id| {
            SessionId::parse_field(session_id, "recipientSessionIds").map_err(AppError::from)
        })
        .collect::<Result<Vec<_>>>()?;
    let recipient_user_ids = message
        .recipient_user_ids
        .iter()
        .map(|user_id| {
            UserId::try_from(user_id.as_str()).map_err(|error| AppError::InvalidBackendData {
                field: "roomChat.recipientUserIds".to_owned(),
                reason: error.to_string(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let entry = MailboxEntry::RoomChat {
        room_id: message.room_id,
        room_name: room_names.get(room_id).cloned(),
        author_user_id,
        author_session_id,
        recipient_session_ids: recipient_session_ids.clone(),
        recipient_user_ids: recipient_user_ids.clone(),
        body: message.body,
        seq: message.seq,
        at: message.posted_at,
    };
    for target in room_chat_targets(
        room_id,
        author_session_id,
        &recipient_session_ids,
        &recipient_user_ids,
        agent_sessions_by_room,
    ) {
        store
            .enqueue_deduplicated(target, &entry)
            .map_err(AppError::Io)?;
    }
    Ok(())
}

fn room_chat_targets(
    room_id: &str,
    author_session_id: Option<SessionId>,
    recipient_session_ids: &[SessionId],
    recipient_user_ids: &[UserId],
    agent_sessions_by_room: &HashMap<String, BTreeMap<SessionId, RoomMailboxTarget>>,
) -> BTreeSet<MailboxDestination> {
    let Some(local_room_sessions) = agent_sessions_by_room.get(room_id) else {
        return BTreeSet::new();
    };
    let candidates: BTreeSet<SessionId> =
        if recipient_session_ids.is_empty() && recipient_user_ids.is_empty() {
            local_room_sessions.keys().copied().collect()
        } else {
            recipient_session_ids.iter().copied().collect()
        };
    candidates
        .into_iter()
        .filter(|target| Some(*target) != author_session_id)
        .filter_map(|target| {
            local_room_sessions
                .get(&target)
                .map(|target| target.mailbox)
        })
        .collect()
}

#[expect(
    clippy::too_many_lines,
    reason = "paged task delivery must update each durable cursor with its exact enqueue outcome"
)]
async fn sync_assigned_tasks(
    room_application: &RoomApplication<'_>,
    store: &AgentRoomStore,
    agent_sessions_by_room: &HashMap<String, BTreeMap<SessionId, RoomMailboxTarget>>,
    room_names: &HashMap<String, String>,
    rooms: &[kodosi_backend_client::api::BackendRoom],
    account_user_id: UserId,
    delivery: &mut crate::rooms::mailbox_store::RoomDeliveryState,
) -> Result<()> {
    let scanned_room_ids = rooms
        .iter()
        .map(|room| room.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut seen_relevant = BTreeSet::new();
    for room in rooms {
        let mut offset = 0_usize;
        let mut snapshot = None;
        loop {
            let page = room_application
                .fetch_room_tasks_for_mailbox(
                    &room.id,
                    None,
                    None,
                    offset,
                    TASK_PAGE_SIZE,
                    snapshot.as_deref(),
                )
                .await?;
            snapshot = Some(page.snapshot);
            process_task_batch(
                page.items,
                |task| {
                    let task_key = format!("{}/{}", task.room_id, task.id);
                    let current_assignee = match (
                        task.assigned_session_id.as_deref(),
                        task.assigned_session_incarnation_id.as_deref(),
                    ) {
                        (Some(session_id), Some(incarnation_id)) => Some((
                            SessionId::parse_field(session_id, "assignedSessionId")?,
                            uuid::Uuid::parse_str(incarnation_id).map_err(|error| {
                                AppError::InvalidBackendData {
                                    field: "assignedSessionIncarnationId".to_owned(),
                                    reason: error.to_string(),
                                }
                            })?,
                        )),
                        (None, None) => None,
                        _ => {
                            tracing::warn!(
                                task_id = %task.id,
                                room_id = %task.room_id,
                                "skipped room task with incomplete assignee incarnation identity"
                            );
                            delivery.task_cursors.remove(&task_key);
                            return Ok(());
                        }
                    };
                    let previous = delivery.task_cursors.get(&task_key).cloned();
                    let Some(room_targets) = agent_sessions_by_room.get(&task.room_id) else {
                        delivery.task_cursors.remove(&task_key);
                        return Ok(());
                    };
                    let targets =
                        task_delivery_targets(current_assignee, previous.as_ref(), room_targets);
                    if targets.is_empty() {
                        delivery.task_cursors.remove(&task_key);
                        return Ok(());
                    }
                    seen_relevant.insert(task_key.clone());

                    let fingerprint = format!(
                        "{}|{}|{}|{}",
                        task.updated_at.unix_timestamp_nanos(),
                        task.revision,
                        task.status,
                        current_assignee.map_or_else(
                            String::new,
                            |(session_id, incarnation_id)| {
                                format!("{session_id}:{incarnation_id}")
                            }
                        )
                    );
                    let current_mailbox =
                        current_assignee.and_then(|(session_id, incarnation_id)| {
                            room_targets
                                .get(&session_id)
                                .filter(|target| target.backend_incarnation_id == incarnation_id)
                                .map(|target| target.mailbox)
                        });
                    if previous.as_ref().is_some_and(|cursor| {
                        cursor.fingerprint == fingerprint
                            && cursor.mailbox_destination == current_mailbox
                    }) {
                        return Ok(());
                    }
                    for (target, delivery_reason) in targets {
                        let entry = MailboxEntry::RoomTask {
                            room_id: task.room_id.clone(),
                            room_name: room_names.get(&room.id).cloned(),
                            task_id: task.id.clone(),
                            title: task.title.clone(),
                            status: task.status.clone(),
                            revision: task.revision,
                            assigned_session_id: current_assignee.map(|(session_id, _)| session_id),
                            assigned_session_incarnation_id: current_assignee
                                .map(|(_, incarnation_id)| incarnation_id),
                            delivery_reason,
                            at: task.updated_at,
                        };
                        store
                            .enqueue_deduplicated(target, &entry)
                            .map_err(AppError::Io)?;
                    }

                    delivery.task_cursors.insert(
                        task_key,
                        RoomTaskDeliveryCursor {
                            fingerprint,
                            assigned_session_id: current_assignee.map(|(session_id, _)| session_id),
                            assigned_session_incarnation_id: current_assignee
                                .map(|(_, incarnation_id)| incarnation_id),
                            mailbox_destination: current_mailbox,
                        },
                    );
                    if delivery.task_cursors.len() > MAX_ROOM_TASK_DELIVERY_CURSORS {
                        return Err(AppError::Unsupported {
                            reason: "room task delivery cursor retention limit reached".to_owned(),
                        });
                    }
                    store
                        .save_room_delivery_state(account_user_id, delivery)
                        .map_err(AppError::Io)?;
                    Ok(())
                },
                |id, task_room_id, updated_at, reason| {
                    tracing::warn!(
                        task_id = %id,
                        room_id = %task_room_id,
                        requested_room_id = %room.id,
                        updated_at = %updated_at,
                        %reason,
                        "quarantined terminally malformed room task; content was not delivered"
                    );
                },
            )?;
            if !page.has_more {
                break;
            }
            offset = page
                .next_offset
                .ok_or_else(|| AppError::InvalidBackendData {
                    field: "roomTasks.nextOffset".to_owned(),
                    reason: "hasMore page did not include a continuation offset".to_owned(),
                })?;
        }
    }
    retain_scanned_task_cursors(delivery, &scanned_room_ids, &seen_relevant);
    Ok(())
}

fn process_task_batch(
    mut tasks: Vec<MailboxRoomTask>,
    mut deliver: impl FnMut(kodosi_backend_client::api::BackendRoomTask) -> Result<()>,
    mut quarantine: impl FnMut(&str, &str, time::OffsetDateTime, &str),
) -> Result<()> {
    tasks.sort_by(|left, right| {
        left.updated_at()
            .cmp(&right.updated_at())
            .then_with(|| left.id().cmp(right.id()))
    });
    for task in tasks {
        match task {
            MailboxRoomTask::Decrypted(task) => deliver(*task)?,
            MailboxRoomTask::Quarantined {
                id,
                room_id,
                updated_at,
                reason,
            } => quarantine(&id, &room_id, updated_at, &reason),
        }
    }
    Ok(())
}

fn retain_scanned_task_cursors(
    delivery: &mut crate::rooms::mailbox_store::RoomDeliveryState,
    scanned_room_ids: &BTreeSet<&str>,
    seen_relevant: &BTreeSet<String>,
) {
    delivery.task_cursors.retain(|task_key, _| {
        task_key.split_once('/').is_none_or(|(room_id, _)| {
            !scanned_room_ids.contains(room_id) || seen_relevant.contains(task_key)
        })
    });
}

fn task_delivery_targets(
    current: Option<(SessionId, uuid::Uuid)>,
    previous: Option<&RoomTaskDeliveryCursor>,
    local_agent_sessions: &BTreeMap<SessionId, RoomMailboxTarget>,
) -> BTreeMap<MailboxDestination, RoomTaskDeliveryReason> {
    let mut targets = BTreeMap::new();
    if let Some((session_id, incarnation_id)) = current
        && let Some(target) = local_agent_sessions
            .get(&session_id)
            .filter(|target| target.backend_incarnation_id == incarnation_id)
    {
        targets.insert(
            target.mailbox,
            RoomTaskDeliveryReason::CurrentAssignmentOrUpdate,
        );
    }
    if let Some(previous) = previous
        && let (Some(previous_session_id), Some(previous_incarnation_id), Some(previous_mailbox)) = (
            previous.assigned_session_id,
            previous.assigned_session_incarnation_id,
            previous.mailbox_destination,
        )
        && local_agent_sessions
            .get(&previous_session_id)
            .is_some_and(|target| {
                target.backend_incarnation_id == previous_incarnation_id
                    && target.mailbox == previous_mailbox
            })
        && current != Some((previous_session_id, previous_incarnation_id))
    {
        let reason = if current.is_some() {
            RoomTaskDeliveryReason::ReassignedPreviousAssignee
        } else {
            RoomTaskDeliveryReason::UnassignedPreviousAssignee
        };
        targets.insert(previous_mailbox, reason);
    }
    targets
}

#[cfg(test)]
mod tests;
