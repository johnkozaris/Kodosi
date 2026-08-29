use crate::{
    AppError, Result,
    room_crypto::{RoomCryptoContext, RoomTaskPrivate},
};
use kodosi_backend_client::api::{
    BackendRoom, BackendRoomChatMessage, BackendRoomChatPage, BackendRoomTask,
};
#[cfg(feature = "cli")]
use kodosi_backend_client::api::{BackendRoomMember, BackendSessionCard};

use super::Runtime;

pub(crate) struct DurableRoomMutation<T> {
    pub(crate) entity_id: String,
    pub(crate) projection: Result<T>,
}

pub(crate) enum MailboxRoomChatMessage {
    Decrypted(BackendRoomChatMessage),
    Quarantined {
        id: String,
        room_id: String,
        seq: i64,
        reason: String,
    },
}

pub(crate) enum MailboxRoomTask {
    Decrypted(Box<BackendRoomTask>),
    Quarantined {
        id: String,
        room_id: String,
        updated_at: time::OffsetDateTime,
        reason: String,
    },
}

pub(crate) struct MailboxRoomChatPage {
    pub(crate) items: Vec<MailboxRoomChatMessage>,
    pub(crate) has_more: bool,
    pub(crate) next_since: Option<i64>,
}

pub(crate) struct MailboxRoomTaskPage {
    pub(crate) items: Vec<MailboxRoomTask>,
    pub(crate) has_more: bool,
    pub(crate) next_offset: Option<usize>,
}

const ROOM_TASK_MAX_SNAPSHOT_ITEMS: usize = 10_000;
const ROOM_TASK_PAGE_SIZE: usize = 500;
const ROOM_TASK_MAX_HTTP_PAGES: usize = ROOM_TASK_MAX_SNAPSHOT_ITEMS;

const ROOM_CHAT_DEFAULT_LIMIT: i32 = 100;
pub(crate) const ROOM_CHAT_MAX_PAGE_LIMIT: i32 = 1000;
const ROOM_CHAT_MAX_SNAPSHOT_ITEMS: usize = 10_000;
const ROOM_CHAT_MAX_AGGREGATE_BYTES: usize = 16 * 1024 * 1024;
const ROOM_CHAT_MAX_HTTP_PAGES: usize = ROOM_CHAT_MAX_SNAPSHOT_ITEMS;

impl MailboxRoomChatMessage {
    pub(crate) const fn seq(&self) -> i64 {
        match self {
            Self::Decrypted(message) => message.seq,
            Self::Quarantined { seq, .. } => *seq,
        }
    }
}

impl MailboxRoomTask {
    pub(crate) fn id(&self) -> &str {
        match self {
            Self::Decrypted(task) => &task.id,
            Self::Quarantined { id, .. } => id,
        }
    }

    pub(crate) fn updated_at(&self) -> time::OffsetDateTime {
        match self {
            Self::Decrypted(task) => task.updated_at,
            Self::Quarantined { updated_at, .. } => *updated_at,
        }
    }
}

impl<T> DurableRoomMutation<T> {
    #[cfg(any(test, feature = "cli"))]
    pub(crate) fn into_cli_result(self) -> Result<T> {
        self.projection.map_err(|error| AppError::Unsupported {
            reason: format!(
                "room mutation committed as {}, but its local response could not be verified: \
                 {error}; do not retry the mutation",
                self.entity_id
            ),
        })
    }
}

#[derive(Clone, Copy)]
pub(crate) struct RoomApplication<'a> {
    runtime: &'a Runtime,
}

impl<'a> RoomApplication<'a> {
    pub(crate) const fn new(runtime: &'a Runtime) -> Self {
        Self { runtime }
    }

    pub(crate) async fn fetch_rooms(&self) -> Result<Vec<BackendRoom>> {
        let rooms = self.runtime.backend.fetch_rooms().await?;
        let mut crypto = RoomCryptoContext::new(self.runtime).await?;
        for room in &rooms {
            crypto.verify_room(room).await?;
        }
        Ok(rooms)
    }

    #[cfg(feature = "cli")]
    pub(crate) async fn resolve_room(&self, room: &str) -> Result<BackendRoom> {
        self.fetch_rooms()
            .await?
            .into_iter()
            .find(|candidate| candidate.id == room || candidate.slug == room)
            .ok_or_else(|| AppError::Unsupported {
                reason: format!("room not found by id or slug: {room}"),
            })
    }

    #[cfg(feature = "cli")]
    pub(crate) async fn fetch_room_sessions(
        &self,
        room_id: &str,
    ) -> Result<Vec<BackendSessionCard>> {
        self.runtime
            .backend
            .fetch_room_feed(room_id)
            .await
            .map_err(Into::into)
    }

    #[cfg(feature = "cli")]
    pub(crate) async fn resolve_room_session_incarnation(
        &self,
        room_id: &str,
        session_id: &str,
    ) -> Result<String> {
        let session = self
            .runtime
            .backend
            .fetch_session_detail(session_id)
            .await?;
        if session.id != session_id
            || session.scope != kodosi_domain::permissions::ShareScope::Room
            || session.room_id.as_deref() != Some(room_id)
            || !matches!(
                session.status,
                kodosi_domain::session::SessionState::Running
                    | kodosi_domain::session::SessionState::Published
                    | kodosi_domain::session::SessionState::Reconnecting
            )
            || session.incarnation_id.is_nil()
        {
            return Err(AppError::Unsupported {
                reason: "task target must be the exact active room-session incarnation".to_owned(),
            });
        }
        Ok(session.incarnation_id.to_string())
    }

    #[cfg(feature = "cli")]
    pub(crate) async fn fetch_room_members(&self, room_id: &str) -> Result<Vec<BackendRoomMember>> {
        self.runtime
            .backend
            .fetch_room_members(room_id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn fetch_room_chat_tail(
        &self,
        room_id: &str,
        limit: i32,
    ) -> Result<Vec<BackendRoomChatMessage>> {
        let mut page = self
            .runtime
            .backend
            .fetch_room_chat_tail(room_id, None, Some(limit))
            .await?;
        let mut crypto = RoomCryptoContext::new(self.runtime).await?;
        for message in &mut page.items {
            decrypt_chat(&mut crypto, message).await?;
        }
        Ok(page.items)
    }

    pub(crate) async fn fetch_room_chat(
        &self,
        room_id: &str,
        since: Option<i64>,
        limit: Option<i32>,
    ) -> Result<Vec<BackendRoomChatMessage>> {
        let limit = normalize_room_chat_limit(limit);
        self.fetch_room_chat_collection(room_id, since, limit, false)
            .await
    }

    pub(crate) async fn fetch_room_chat_snapshot(
        &self,
        room_id: &str,
    ) -> Result<Vec<BackendRoomChatMessage>> {
        self.fetch_room_chat_collection(room_id, None, ROOM_CHAT_MAX_SNAPSHOT_ITEMS, true)
            .await
    }

    async fn fetch_room_chat_collection(
        &self,
        room_id: &str,
        since: Option<i64>,
        limit: usize,
        require_terminal_page: bool,
    ) -> Result<Vec<BackendRoomChatMessage>> {
        let mut collector = RoomChatCollector::new(limit, require_terminal_page);
        let mut cursor = since.unwrap_or(0);
        let mut crypto = RoomCryptoContext::new(self.runtime).await?;
        loop {
            let request_limit = collector.next_request_limit()?;
            let mut page = self
                .runtime
                .backend
                .fetch_room_chat(room_id, Some(cursor), Some(request_limit))
                .await?;
            for message in &mut page.items {
                decrypt_chat(&mut crypto, message).await?;
            }
            match collector.accept(page)? {
                Some(next_since) => cursor = next_since,
                None => return Ok(collector.into_items()),
            }
        }
    }

    pub(crate) async fn fetch_room_chat_for_mailbox(
        &self,
        room_id: &str,
        since: Option<i64>,
        limit: Option<i32>,
    ) -> Result<MailboxRoomChatPage> {
        let page = self
            .runtime
            .backend
            .fetch_room_chat(room_id, since, limit)
            .await?;
        let mut crypto = RoomCryptoContext::new(self.runtime).await?;
        let mut outcomes = Vec::with_capacity(page.items.len());
        for mut message in page.items {
            match decrypt_chat(&mut crypto, &mut message).await {
                Ok(()) => outcomes.push(MailboxRoomChatMessage::Decrypted(message)),
                Err(error) if is_terminal_room_chat_content_error(&error) => {
                    outcomes.push(MailboxRoomChatMessage::Quarantined {
                        id: message.id,
                        room_id: message.room_id,
                        seq: message.seq,
                        reason: error.to_string(),
                    });
                }
                Err(error) => return Err(error),
            }
        }
        Ok(MailboxRoomChatPage {
            items: outcomes,
            has_more: page.has_more,
            next_since: page.next_since,
        })
    }

    pub(crate) async fn fetch_room_chat_message(
        &self,
        room_id: &str,
        message_id: &str,
    ) -> Result<BackendRoomChatMessage> {
        let mut message = self
            .runtime
            .backend
            .fetch_room_chat_message(room_id, message_id)
            .await?;
        let mut crypto = RoomCryptoContext::new(self.runtime).await?;
        decrypt_chat(&mut crypto, &mut message).await?;
        Ok(message)
    }

    async fn resolve_room_chat_create(
        &self,
        message_id: &str,
        room_id: &str,
        body: &str,
        author_session_id: Option<&str>,
        author_kind: &str,
        recipient_session_ids: &[String],
        recipient_user_ids: &[String],
    ) -> Result<Option<BackendRoomChatMessage>> {
        let message = match self.fetch_room_chat_message(room_id, message_id).await {
            Ok(message) => message,
            Err(AppError::NotFound) => return Ok(None),
            Err(error) => return Err(error),
        };
        if !same_chat_create(
            &message,
            room_id,
            body,
            author_session_id,
            author_kind,
            recipient_session_ids,
            recipient_user_ids,
        ) {
            return Err(room_mutation_fingerprint_error("chat message"));
        }
        Ok(Some(message))
    }

    pub(crate) async fn post_room_chat(
        &self,
        message_id: &str,
        room_id: &str,
        body: &str,
        author_session_id: Option<&str>,
        author_kind: &str,
        recipient_session_ids: &[String],
        recipient_user_ids: &[String],
    ) -> Result<DurableRoomMutation<BackendRoomChatMessage>> {
        if let Some(message) = self
            .resolve_room_chat_create(
                message_id,
                room_id,
                body,
                author_session_id,
                author_kind,
                recipient_session_ids,
                recipient_user_ids,
            )
            .await?
        {
            return Ok(DurableRoomMutation {
                entity_id: message.id.clone(),
                projection: Ok(message),
            });
        }
        let mut crypto = RoomCryptoContext::new(self.runtime).await?;
        let encrypted = crypto
            .encrypt_json(room_id, message_id, "chat", &body)
            .await?;
        let posted = self
            .runtime
            .backend
            .post_room_chat(
                room_id,
                message_id,
                &encrypted,
                author_session_id,
                author_kind,
                recipient_session_ids,
                recipient_user_ids,
            )
            .await;
        let mut message = match posted {
            Ok(message) => message,
            Err(error) if is_conflict(&error) => self
                .resolve_room_chat_create(
                    message_id,
                    room_id,
                    body,
                    author_session_id,
                    author_kind,
                    recipient_session_ids,
                    recipient_user_ids,
                )
                .await?
                .ok_or_else(|| AppError::from(error))?,
            Err(error) => return Err(error.into()),
        };
        let entity_id = message.id.clone();
        let projection = decrypt_chat(&mut crypto, &mut message)
            .await
            .map(|()| message);
        Ok(DurableRoomMutation {
            entity_id,
            projection,
        })
    }

    pub(crate) async fn fetch_room_task(
        &self,
        room_id: &str,
        task_id: &str,
    ) -> Result<BackendRoomTask> {
        let mut task = self
            .runtime
            .backend
            .fetch_room_task(room_id, task_id)
            .await?;
        let mut crypto = RoomCryptoContext::new(self.runtime).await?;
        decrypt_task(&mut crypto, &mut task).await?;
        Ok(task)
    }

    pub(crate) async fn fetch_room_tasks(
        &self,
        room_id: &str,
        status: Option<&str>,
        assignee: Option<&str>,
    ) -> Result<Vec<BackendRoomTask>> {
        let mut items = Vec::new();
        let mut offset = 0_usize;
        let mut pages = 0_usize;
        let mut crypto: Option<RoomCryptoContext> = None;
        loop {
            let page = self
                .runtime
                .backend
                .fetch_room_tasks(
                    room_id,
                    status,
                    assignee,
                    Some(offset),
                    Some(ROOM_TASK_PAGE_SIZE),
                )
                .await?;
            if page.items.len() > ROOM_TASK_MAX_SNAPSHOT_ITEMS.saturating_sub(items.len()) {
                return Err(room_task_collection_capacity_error());
            }
            let mut page_items = page.items;
            if !page_items.is_empty() && crypto.is_none() {
                crypto = Some(RoomCryptoContext::new(self.runtime).await?);
            }
            if !page_items.is_empty() {
                let crypto = crypto.as_mut().ok_or_else(|| AppError::Unsupported {
                    reason: "room task decryption context was not initialized".to_owned(),
                })?;
                for task in &mut page_items {
                    decrypt_task(crypto, task).await?;
                }
            }
            items.extend(page_items);
            pages = pages.saturating_add(1);
            if !page.has_more {
                return Ok(items);
            }
            if pages >= ROOM_TASK_MAX_HTTP_PAGES {
                return Err(room_task_collection_capacity_error());
            }
            offset = page
                .next_offset
                .ok_or_else(|| AppError::InvalidBackendData {
                    field: "roomTasks.nextOffset".to_owned(),
                    reason: "hasMore page did not include a continuation offset".to_owned(),
                })?;
        }
    }

    pub(crate) async fn fetch_room_tasks_page(
        &self,
        room_id: &str,
        status: Option<&str>,
        assignee: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<kodosi_backend_client::api::BackendRoomTaskPage> {
        let mut page = self
            .runtime
            .backend
            .fetch_room_tasks(room_id, status, assignee, Some(offset), Some(limit))
            .await?;
        let mut crypto = RoomCryptoContext::new(self.runtime).await?;
        for task in &mut page.items {
            decrypt_task(&mut crypto, task).await?;
        }
        Ok(page)
    }

    pub(crate) async fn fetch_room_tasks_for_mailbox(
        &self,
        room_id: &str,
        status: Option<&str>,
        assignee: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<MailboxRoomTaskPage> {
        let page = self
            .runtime
            .backend
            .fetch_room_tasks(room_id, status, assignee, Some(offset), Some(limit))
            .await?;
        let mut crypto = RoomCryptoContext::new(self.runtime).await?;
        let mut outcomes = Vec::with_capacity(page.items.len());
        for mut task in page.items {
            match decrypt_task(&mut crypto, &mut task).await {
                Ok(()) => outcomes.push(MailboxRoomTask::Decrypted(Box::new(task))),
                Err(error) if is_terminal_room_task_content_error(&error) => {
                    outcomes.push(MailboxRoomTask::Quarantined {
                        id: task.id,
                        room_id: task.room_id,
                        updated_at: task.updated_at,
                        reason: error.to_string(),
                    });
                }
                Err(error) => return Err(error),
            }
        }
        Ok(MailboxRoomTaskPage {
            items: outcomes,
            has_more: page.has_more,
            next_offset: page.next_offset,
        })
    }

    async fn resolve_room_task_create(
        &self,
        task_id: &str,
        room_id: &str,
        title: &str,
        description: Option<&str>,
        assigned_session_id: Option<&str>,
        assigned_session_incarnation_id: Option<&str>,
        due_at: Option<time::OffsetDateTime>,
    ) -> Result<Option<BackendRoomTask>> {
        let task = match self.fetch_room_task(room_id, task_id).await {
            Ok(task) => task,
            Err(AppError::NotFound) => return Ok(None),
            Err(error) => return Err(error),
        };
        if !same_task_create(
            &task,
            room_id,
            title,
            description,
            assigned_session_id,
            assigned_session_incarnation_id,
            due_at,
        ) {
            return Err(room_mutation_fingerprint_error("room task"));
        }
        Ok(Some(task))
    }

    pub(crate) async fn create_room_task(
        &self,
        task_id: &str,
        room_id: &str,
        title: String,
        description: Option<String>,
        assigned_session_id: Option<&str>,
        assigned_session_incarnation_id: Option<&str>,
        due_at: Option<time::OffsetDateTime>,
    ) -> Result<DurableRoomMutation<BackendRoomTask>> {
        if let Some(task) = self
            .resolve_room_task_create(
                task_id,
                room_id,
                &title,
                description.as_deref(),
                assigned_session_id,
                assigned_session_incarnation_id,
                due_at,
            )
            .await?
        {
            return Ok(DurableRoomMutation {
                entity_id: task.id.clone(),
                projection: Ok(task),
            });
        }
        let private = RoomTaskPrivate { title, description };
        let mut crypto = RoomCryptoContext::new(self.runtime).await?;
        let encrypted = crypto
            .encrypt_json(room_id, task_id, "task", &private)
            .await?;
        let created = self
            .runtime
            .backend
            .create_room_task(
                room_id,
                task_id,
                &encrypted,
                None,
                assigned_session_id,
                assigned_session_incarnation_id,
                due_at,
            )
            .await;
        let mut task = match created {
            Ok(task) => task,
            Err(error) if is_conflict(&error) => self
                .resolve_room_task_create(
                    task_id,
                    room_id,
                    &private.title,
                    private.description.as_deref(),
                    assigned_session_id,
                    assigned_session_incarnation_id,
                    due_at,
                )
                .await?
                .ok_or_else(|| AppError::from(error))?,
            Err(error) => return Err(error.into()),
        };
        let entity_id = task.id.clone();
        let projection = decrypt_task(&mut crypto, &mut task).await.map(|()| task);
        Ok(DurableRoomMutation {
            entity_id,
            projection,
        })
    }

    pub(crate) async fn assign_room_task(
        &self,
        room_id: &str,
        task_id: &str,
        mutation_id: &uuid::Uuid,
        _fingerprint: &str,
        expected_task_revision: i64,
        session_id: Option<&str>,
        session_incarnation_id: Option<&str>,
    ) -> Result<DurableRoomMutation<BackendRoomTask>> {
        let response = self
            .runtime
            .backend
            .assign_room_task(
                room_id,
                task_id,
                mutation_id,
                expected_task_revision,
                session_id,
                session_incarnation_id,
            )
            .await?;
        let entity_id = response.entity_id.to_string();
        let projection = self.fetch_room_task(room_id, task_id).await;
        Ok(DurableRoomMutation {
            entity_id,
            projection,
        })
    }

    pub(crate) async fn prepare_task_result(
        &self,
        room_id: &str,
        task_id: &str,
        result: Option<&str>,
    ) -> Result<Option<String>> {
        let Some(result) = result else {
            return Ok(None);
        };
        let mut crypto = RoomCryptoContext::new(self.runtime).await?;
        crypto
            .encrypt_json(room_id, task_id, "taskResult", &result)
            .await
            .map(Some)
    }

    pub(crate) async fn transition_prepared_room_task(
        &self,
        room_id: &str,
        task_id: &str,
        mutation_id: &uuid::Uuid,
        expected_task_revision: i64,
        to_status: &str,
        actor: Option<(&str, &str)>,
        encrypted_result: Option<&str>,
    ) -> Result<DurableRoomMutation<BackendRoomTask>> {
        let response = self
            .runtime
            .backend
            .transition_room_task(
                room_id,
                task_id,
                mutation_id,
                expected_task_revision,
                to_status,
                actor,
                encrypted_result,
            )
            .await?;
        let entity_id = response.entity_id.to_string();
        let projection = self.fetch_room_task(room_id, task_id).await;
        Ok(DurableRoomMutation {
            entity_id,
            projection,
        })
    }
}

struct RoomChatCollector {
    items: Vec<BackendRoomChatMessage>,
    limit: usize,
    require_terminal_page: bool,
    body_bytes: usize,
    pages: usize,
}

impl RoomChatCollector {
    fn new(limit: usize, require_terminal_page: bool) -> Self {
        Self {
            items: Vec::new(),
            limit,
            require_terminal_page,
            body_bytes: 0,
            pages: 0,
        }
    }

    fn next_request_limit(&self) -> Result<i32> {
        let remaining = self.limit.saturating_sub(self.items.len());
        if remaining == 0 {
            return Err(room_chat_collection_capacity_error(self.limit, "items"));
        }
        i32::try_from(remaining.min(usize::try_from(ROOM_CHAT_MAX_PAGE_LIMIT).unwrap_or(remaining)))
            .map_err(|_| room_chat_collection_capacity_error(1000, "items"))
    }

    fn accept(&mut self, page: BackendRoomChatPage) -> Result<Option<i64>> {
        self.pages = self.pages.saturating_add(1);
        let added_bytes = page
            .items
            .iter()
            .try_fold(0_usize, |total, message| {
                total.checked_add(message.body.len())
            })
            .ok_or_else(|| {
                room_chat_collection_capacity_error(ROOM_CHAT_MAX_AGGREGATE_BYTES, "body bytes")
            })?;
        self.body_bytes = self.body_bytes.checked_add(added_bytes).ok_or_else(|| {
            room_chat_collection_capacity_error(ROOM_CHAT_MAX_AGGREGATE_BYTES, "body bytes")
        })?;
        if self.body_bytes > ROOM_CHAT_MAX_AGGREGATE_BYTES {
            return Err(room_chat_collection_capacity_error(
                ROOM_CHAT_MAX_AGGREGATE_BYTES,
                "body bytes",
            ));
        }
        self.items.extend(page.items);
        if !page.has_more || (!self.require_terminal_page && self.items.len() >= self.limit) {
            return Ok(None);
        }
        if self.items.len() >= self.limit {
            return Err(room_chat_collection_capacity_error(self.limit, "items"));
        }
        if self.pages >= ROOM_CHAT_MAX_HTTP_PAGES {
            return Err(room_chat_collection_capacity_error(
                ROOM_CHAT_MAX_HTTP_PAGES,
                "HTTP pages",
            ));
        }
        page.next_since
            .ok_or_else(|| AppError::InvalidBackendData {
                field: "roomChat.nextSince".to_owned(),
                reason: "hasMore page did not include a continuation cursor".to_owned(),
            })
            .map(Some)
    }

    fn into_items(self) -> Vec<BackendRoomChatMessage> {
        self.items
    }
}

fn normalize_room_chat_limit(limit: Option<i32>) -> usize {
    usize::try_from(
        limit
            .unwrap_or(ROOM_CHAT_DEFAULT_LIMIT)
            .clamp(1, ROOM_CHAT_MAX_PAGE_LIMIT),
    )
    .unwrap_or(100)
}

fn room_chat_collection_capacity_error(limit: usize, unit: &str) -> AppError {
    AppError::Unsupported {
        reason: format!(
            "room chat collection exceeds the explicit bound of {limit} {unit}; no partial snapshot was emitted"
        ),
    }
}

fn same_chat_create(
    message: &BackendRoomChatMessage,
    room_id: &str,
    body: &str,
    author_session_id: Option<&str>,
    author_kind: &str,
    recipient_session_ids: &[String],
    recipient_user_ids: &[String],
) -> bool {
    let mut expected_sessions = recipient_session_ids.to_vec();
    let mut expected_users = recipient_user_ids.to_vec();
    expected_sessions.sort();
    expected_users.sort();
    message.room_id == room_id
        && message.body == body
        && message.author_session_id.as_deref() == author_session_id
        && message.author_kind == author_kind
        && message.recipient_session_ids == expected_sessions
        && message.recipient_user_ids == expected_users
}

fn same_task_create(
    task: &BackendRoomTask,
    room_id: &str,
    title: &str,
    description: Option<&str>,
    assigned_session_id: Option<&str>,
    assigned_session_incarnation_id: Option<&str>,
    due_at: Option<time::OffsetDateTime>,
) -> bool {
    task.room_id == room_id
        && task.title == title
        && task.description.as_deref() == description
        && task.assigned_session_id.as_deref() == assigned_session_id
        && task.assigned_session_incarnation_id.as_deref() == assigned_session_incarnation_id
        && task.due_at == due_at
}

fn is_conflict(error: &kodosi_backend_client::BackendClientError) -> bool {
    matches!(
        error,
        kodosi_backend_client::BackendClientError::HttpProblem { status: 409, .. }
    )
}

fn room_mutation_fingerprint_error(kind: &str) -> AppError {
    AppError::Unsupported {
        reason: format!(
            "{kind} mutation ID is already in use with a different request fingerprint"
        ),
    }
}

fn room_task_collection_capacity_error() -> AppError {
    AppError::Unsupported {
        reason: format!(
            "room task collection exceeds the explicit bound of {ROOM_TASK_MAX_SNAPSHOT_ITEMS} items; no partial snapshot was emitted"
        ),
    }
}

async fn decrypt_chat(
    crypto: &mut RoomCryptoContext,
    message: &mut BackendRoomChatMessage,
) -> Result<()> {
    message.body = crypto
        .decrypt_json(
            &message.room_id,
            &message.id,
            "chat",
            &message.author_user_id,
            &message.body,
        )
        .await?;
    Ok(())
}

fn is_terminal_room_chat_content_error(error: &AppError) -> bool {
    match error {
        AppError::InvalidBackendData { field, .. } => {
            field == "room.encryptedContent" || field.starts_with("room.encryptedContent.")
        }
        AppError::Unsupported { reason } => {
            reason == "this device is not a recipient of the room content"
                || reason.contains("encrypted key blob")
                || reason.contains("ML-KEM-768 decapsulation failed")
                || reason.contains("wrapped key decryption failed")
                || reason.contains("frame decryption failed")
        }
        _ => false,
    }
}

fn is_terminal_room_task_content_error(error: &AppError) -> bool {
    matches!(
        error,
        AppError::InvalidBackendData { field, .. }
            if field == "roomTask.description" || field == "roomTask.result"
    ) || is_terminal_room_chat_content_error(error)
}

async fn decrypt_task(crypto: &mut RoomCryptoContext, task: &mut BackendRoomTask) -> Result<()> {
    if task.description.is_some() {
        return Err(AppError::InvalidBackendData {
            field: "roomTask.description".to_owned(),
            reason: "encrypted task content must be stored only in title".to_owned(),
        });
    }
    let content: RoomTaskPrivate = crypto
        .decrypt_json(
            &task.room_id,
            &task.id,
            "task",
            &task.created_by_user_id,
            &task.title,
        )
        .await?;
    task.title = content.title;
    task.description = content.description;
    task.result = match (&task.result, &task.result_author_user_id) {
        (Some(result), Some(author)) => Some(
            crypto
                .decrypt_json(&task.room_id, &task.id, "taskResult", author, result)
                .await?,
        ),
        (None, None) => None,
        _ => {
            return Err(AppError::InvalidBackendData {
                field: "roomTask.result".to_owned(),
                reason: "encrypted task result is missing signer attribution".to_owned(),
            });
        }
    };
    Ok(())
}

#[cfg(test)]
mod tests;
