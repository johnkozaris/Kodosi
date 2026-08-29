use std::collections::{BTreeMap, BTreeSet, VecDeque};

use futures_util::{StreamExt, stream};
use time::OffsetDateTime;

use crate::{
    AppError, Result,
    host_protocol::RoomListEntry,
    runtime::{DiscoveryRefreshOutcome, state::push_log},
};
use kodosi_backend_client::{
    BackendClientError,
    api::{BackendRoom, BackendSessionCard, BackendSessionDetail, BackendUserSummary},
    http_client::BackendHttpClient,
};
use kodosi_domain::{
    ids::{SessionId, UserId},
    session::{SessionRole, SessionState, SessionSummary},
    terminal::TerminalSize,
    user::display_name_or_handle,
};

use crate::discovery::state::{DiscoveryPreservation, RemoteSessionRecord};

const MAX_DISCOVERY_ROOMS: usize = 256;
const MAX_DISCOVERY_CARDS_PER_SOURCE: usize = 2_000;
const MAX_DISCOVERY_IDENTITIES: usize = 4_000;
const DISCOVERY_CONCURRENCY: usize = 16;

pub(crate) struct DiscoveryFetchCtx<'a> {
    pub(crate) backend: &'a BackendHttpClient,
    pub(crate) logs: &'a mut VecDeque<String>,
}

impl DiscoveryFetchCtx<'_> {
    pub(crate) async fn fetch_outcome(
        &mut self,
        last_terminal_size: TerminalSize,
        current_user_id: Option<UserId>,
    ) -> Result<DiscoveryRefreshOutcome> {
        let (own_cards, rooms, preserve_owned, preserve_all_rooms) = self.load_sources().await?;
        let available_rooms = (!preserve_all_rooms)
            .then_some(rooms.as_ref())
            .flatten()
            .map(|items| {
                items
                    .iter()
                    .map(|room| RoomListEntry {
                        id: room.id.clone(),
                        name: room.name.clone(),
                        slug: room.slug.clone(),
                    })
                    .collect()
            });
        let rooms = rooms.unwrap_or_default();
        let (room_feeds, preserve_room_ids) = self.fetch_room_feeds(&rooms).await?;

        let mut all_cards = Vec::new();
        all_cards.extend(own_cards.iter().cloned());
        for cards in room_feeds.values() {
            all_cards.extend(cards.iter().cloned());
        }

        let session_ids: BTreeSet<String> = all_cards.iter().map(|card| card.id.clone()).collect();
        let (details_by_id, detail_partial_failure) =
            self.fetch_session_details(&session_ids).await?;

        let owner_ids: BTreeSet<String> = details_by_id
            .values()
            .map(|detail| detail.owner_user_id.clone())
            .collect();
        let (owners_by_id, owner_partial_failure) = self.fetch_owner_summaries(&owner_ids).await?;
        let rooms_by_id: BTreeMap<String, BackendRoom> = rooms
            .iter()
            .cloned()
            .map(|room| (room.id.clone(), room))
            .collect();
        let mut discovery_feeds: Vec<(&[BackendSessionCard], Option<&str>)> = Vec::new();
        for room in &rooms {
            if let Some(cards) = room_feeds.get(&room.id) {
                discovery_feeds.push((cards, Some(room.name.as_str())));
            }
        }
        discovery_feeds.push((&own_cards, None));

        let mut remote_sessions = BTreeMap::new();
        for (cards, room_name) in &discovery_feeds {
            for card in *cards {
                if let Some(record) = remote_record_from_backend(
                    card,
                    *room_name,
                    last_terminal_size,
                    &details_by_id,
                    &owners_by_id,
                    &rooms_by_id,
                    current_user_id,
                ) {
                    remote_sessions.insert(record.summary.id, record);
                }
            }
        }

        Ok(DiscoveryRefreshOutcome {
            remote_sessions: remote_sessions.into_values().collect(),
            preservation: DiscoveryPreservation {
                all: detail_partial_failure || owner_partial_failure,
                owned: preserve_owned,
                rooms: preserve_all_rooms,
                room_ids: preserve_room_ids,
            },
            available_rooms,
        })
    }

    async fn load_sources(
        &mut self,
    ) -> Result<(
        Vec<BackendSessionCard>,
        Option<Vec<BackendRoom>>,
        bool,
        bool,
    )> {
        let (own_result, rooms_result) =
            tokio::join!(self.backend.fetch_my_sessions(), self.backend.fetch_rooms(),);
        let mut loaded_sections = 0usize;
        let mut own_partial_failure = false;
        let mut room_partial_failure = false;
        let mut own_cards = self
            .accept_result(
                "own sessions",
                own_result,
                &mut loaded_sections,
                &mut own_partial_failure,
            )?
            .unwrap_or_default();

        own_cards.retain(is_live_discovery_card);
        if own_cards.len() > MAX_DISCOVERY_CARDS_PER_SOURCE {
            own_partial_failure = true;
            own_cards.truncate(MAX_DISCOVERY_CARDS_PER_SOURCE);
        }
        let mut rooms = self.accept_result(
            "rooms",
            rooms_result,
            &mut loaded_sections,
            &mut room_partial_failure,
        )?;
        if let Some(rooms) = rooms.as_mut()
            && rooms.len() > MAX_DISCOVERY_ROOMS
        {
            room_partial_failure = true;
            rooms.truncate(MAX_DISCOVERY_ROOMS);
        }

        if loaded_sections == 0 {
            return Err(AppError::Unsupported {
                reason: "backend discovery endpoints are temporarily unavailable".to_owned(),
            });
        }

        Ok((own_cards, rooms, own_partial_failure, room_partial_failure))
    }

    async fn fetch_room_feeds(
        &mut self,
        rooms: &[BackendRoom],
    ) -> Result<(BTreeMap<String, Vec<BackendSessionCard>>, BTreeSet<String>)> {
        let backend = self.backend.clone();
        let room_requests = rooms
            .iter()
            .enumerate()
            .map(|(index, room)| (index, room.id.clone()))
            .collect::<Vec<_>>();
        let room_feed_results = stream::iter(room_requests)
            .map(|(index, room_id)| {
                let backend = backend.clone();
                async move { (index, backend.fetch_room_feed(&room_id).await) }
            })
            .buffer_unordered(DISCOVERY_CONCURRENCY)
            .collect::<Vec<_>>()
            .await;
        let mut room_feeds = BTreeMap::new();
        let mut preserve_room_ids = BTreeSet::new();
        for (index, result) in room_feed_results {
            let room = &rooms[index];
            match result {
                Ok(mut cards) => {
                    if cards.len() > MAX_DISCOVERY_CARDS_PER_SOURCE {
                        preserve_room_ids.insert(room.id.clone());
                        cards.truncate(MAX_DISCOVERY_CARDS_PER_SOURCE);
                    }
                    room_feeds.insert(room.id.clone(), cards);
                }
                Err(BackendClientError::Unauthorized) => return Err(AppError::Unauthorized),
                Err(error) => {
                    preserve_room_ids.insert(room.id.clone());
                    push_log(
                        self.logs,
                        format!("room feed unavailable for {}: {error}", room.name),
                    );
                }
            }
        }
        Ok((room_feeds, preserve_room_ids))
    }

    async fn fetch_session_details(
        &mut self,
        session_ids: &BTreeSet<String>,
    ) -> Result<(BTreeMap<String, BackendSessionDetail>, bool)> {
        let backend = self.backend.clone();
        let ids = session_ids
            .iter()
            .take(MAX_DISCOVERY_IDENTITIES)
            .cloned()
            .collect::<Vec<_>>();
        let session_detail_results = stream::iter(ids)
            .map(|session_id| {
                let backend = backend.clone();
                async move {
                    let result = backend.fetch_session_detail(&session_id).await;
                    (session_id, result)
                }
            })
            .buffer_unordered(DISCOVERY_CONCURRENCY)
            .collect::<Vec<_>>()
            .await;
        let mut details_by_id = BTreeMap::new();
        let mut partial_failure = session_ids.len() > MAX_DISCOVERY_IDENTITIES;
        if partial_failure {
            push_log(
                self.logs,
                format!(
                    "session discovery returned {} identities; preserving cached sessions beyond the {}-identity detail limit",
                    session_ids.len(),
                    MAX_DISCOVERY_IDENTITIES
                ),
            );
        }
        for (session_id, result) in session_detail_results {
            match result {
                Ok(detail) => {
                    details_by_id.insert(session_id, detail);
                }
                Err(BackendClientError::Unauthorized) => return Err(AppError::Unauthorized),
                Err(error) => {
                    partial_failure = true;
                    push_log(
                        self.logs,
                        format!("session detail unavailable for {session_id}: {error}"),
                    );
                }
            }
        }
        Ok((details_by_id, partial_failure))
    }

    async fn fetch_owner_summaries(
        &mut self,
        owner_ids: &BTreeSet<String>,
    ) -> Result<(BTreeMap<String, BackendUserSummary>, bool)> {
        let backend = self.backend.clone();
        let ids = owner_ids
            .iter()
            .take(MAX_DISCOVERY_IDENTITIES)
            .cloned()
            .collect::<Vec<_>>();
        let owner_summary_results = stream::iter(ids)
            .map(|user_id| {
                let backend = backend.clone();
                async move {
                    let result = backend.fetch_user_summary(&user_id).await;
                    (user_id, result)
                }
            })
            .buffer_unordered(DISCOVERY_CONCURRENCY)
            .collect::<Vec<_>>()
            .await;
        let mut owners_by_id = BTreeMap::new();
        let mut partial_failure = false;
        for (user_id, result) in owner_summary_results {
            match result {
                Ok(summary) => {
                    owners_by_id.insert(user_id, summary);
                }
                Err(BackendClientError::Unauthorized) => return Err(AppError::Unauthorized),
                Err(error) => {
                    partial_failure = true;
                    push_log(
                        self.logs,
                        format!("owner lookup unavailable for {user_id}: {error}"),
                    );
                }
            }
        }
        Ok((owners_by_id, partial_failure))
    }

    fn accept_result<T>(
        &mut self,
        label: &str,
        result: std::result::Result<T, BackendClientError>,
        loaded_sections: &mut usize,
        partial_failure: &mut bool,
    ) -> Result<Option<T>> {
        match result {
            Ok(value) => {
                *loaded_sections += 1;
                Ok(Some(value))
            }
            Err(BackendClientError::Unauthorized) => Err(AppError::Unauthorized),
            Err(error) => {
                *partial_failure = true;
                push_log(
                    self.logs,
                    format!("{label} unavailable during discovery refresh: {error}"),
                );
                Ok(None)
            }
        }
    }
}

fn is_live_discovery_card(card: &BackendSessionCard) -> bool {
    matches!(
        card.status,
        SessionState::Starting
            | SessionState::Running
            | SessionState::Published
            | SessionState::Reconnecting
    )
}

fn remote_record_from_backend(
    card: &BackendSessionCard,
    room_name: Option<&str>,
    size: TerminalSize,
    details_by_id: &BTreeMap<String, BackendSessionDetail>,
    owners_by_id: &BTreeMap<String, BackendUserSummary>,
    rooms_by_id: &BTreeMap<String, BackendRoom>,
    current_user_id: Option<UserId>,
) -> Option<RemoteSessionRecord> {
    let id = SessionId::try_from(card.id.as_str()).ok()?;
    let detail = details_by_id.get(&card.id);
    detail?;
    let owner_id = detail.and_then(|detail| UserId::try_from(detail.owner_user_id.as_str()).ok());
    let is_owned = owner_id.is_some() && owner_id == current_user_id;
    if !is_owned && detail.is_some_and(|detail| !owners_by_id.contains_key(&detail.owner_user_id)) {
        return None;
    }
    let scope = detail.map_or(card.scope, |detail| detail.scope);
    let access = detail.map_or(card.access, |detail| {
        if is_owned {
            kodosi_domain::permissions::AccessLevel::Inject
        } else {
            detail.effective_access.unwrap_or(detail.default_access)
        }
    });
    let state = detail.map_or(card.status, |detail| detail.status);
    let resolved_room_name = room_name.map(str::to_owned).or_else(|| {
        detail
            .and_then(|detail| detail.room_id.as_ref())
            .and_then(|room_id| rooms_by_id.get(room_id))
            .map(|room| room.name.clone())
    });
    let owner_name = if is_owned {
        "You".to_owned()
    } else {
        detail
            .and_then(|detail| owners_by_id.get(&detail.owner_user_id))
            .map_or_else(
                || "Remote owner".to_owned(),
                |summary| display_name_or_handle(&summary.display_name, &summary.handle).to_owned(),
            )
    };

    let mut summary = SessionSummary::new_remote(
        id,
        detail.map_or_else(|| card.title.clone(), |detail| detail.title.clone()),
        owner_name,
        owner_id,
        scope,
        access,
        size,
    );
    if is_owned {
        summary.role = SessionRole::Owner;
    }
    summary.state = state;
    summary.room_name = resolved_room_name;
    summary.last_update = OffsetDateTime::now_utc();
    summary.created_at = OffsetDateTime::now_utc();

    Some(RemoteSessionRecord {
        summary,
        incarnation_id: detail.map(|detail| detail.incarnation_id),
        room_id: detail.and_then(|detail| detail.room_id.clone()),
        connection_state: None,
        connection_reason: None,
        access_state: None,
        access_reason: None,
        access_issue: None,
        viewer_blocked: false,
        viewer_hidden: false,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use kodosi_backend_client::{
        api::{BackendSessionCard, BackendSessionDetail, BackendUserSummary},
        labels::BackendToolKind,
    };
    use kodosi_domain::{
        ids::UserId,
        permissions::{AccessLevel, ShareScope},
        session::{SessionRole, SessionState},
        terminal::TerminalSize,
    };

    use super::{is_live_discovery_card, remote_record_from_backend};

    fn card(status: SessionState) -> BackendSessionCard {
        BackendSessionCard {
            id: "0197f18a-c1a0-7c90-a6dc-1f4dc53ea751".to_owned(),
            title: "Session".to_owned(),
            scope: ShareScope::Friends,
            access: AccessLevel::View,
            status,
        }
    }

    #[test]
    fn owner_history_only_enters_discovery_while_live() {
        for state in [
            SessionState::Starting,
            SessionState::Running,
            SessionState::Published,
            SessionState::Reconnecting,
        ] {
            assert!(is_live_discovery_card(&card(state)), "{state:?}");
        }
        for state in [
            SessionState::Stopping,
            SessionState::Stopped,
            SessionState::Failed,
        ] {
            assert!(!is_live_discovery_card(&card(state)), "{state:?}");
        }
    }

    #[test]
    fn owned_discovery_projection_uses_owner_access_not_audience_default() {
        let card = card(SessionState::Running);
        let current_user_id =
            UserId::try_from("11111111-1111-1111-1111-111111111111").expect("user id");
        let detail = BackendSessionDetail {
            id: card.id.clone(),
            incarnation_id: uuid::Uuid::from_u128(1),
            incarnation_generation: 1,
            incarnation_protocol_version: 2,
            owner_user_id: current_user_id.to_string(),
            title: card.title.clone(),
            tool_kind: BackendToolKind::Generic,
            scope: ShareScope::MyDevices,
            room_id: None,
            default_access: AccessLevel::View,
            effective_access: Some(AccessLevel::Suggest),
            status: SessionState::Running,
            started_at: "2026-08-05T00:00:00Z".to_owned(),
            ended_at: None,
            last_heartbeat_at: "2026-08-05T00:00:00Z".to_owned(),
        };
        let record = remote_record_from_backend(
            &card,
            None,
            TerminalSize::default(),
            &BTreeMap::from([(card.id.clone(), detail)]),
            &BTreeMap::<String, BackendUserSummary>::new(),
            &BTreeMap::new(),
            Some(current_user_id),
        )
        .expect("owned discovery record");

        assert_eq!(record.summary.role, SessionRole::Owner);
        assert_eq!(record.summary.access, AccessLevel::Inject);
    }
}
