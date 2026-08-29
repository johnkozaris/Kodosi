use crate::{
    Result, RoomListEntry, SessionEvent, SessionListEntry, SystemEvent,
    host_protocol::{room_catalog_event, session_catalog_snapshot_event},
    runtime::{self, session_catalog},
    runtime_event_bus::RuntimeEventSender,
};

pub(crate) async fn publish_state_snapshot(
    app: &runtime::Runtime,
    tx: &RuntimeEventSender,
    last_signal: &mut Option<session_catalog::RefreshFingerprint>,
    force: bool,
    replay_catalog: bool,
) -> Result<()> {
    let signal =
        session_catalog::build_refresh_fingerprint(app, app.collaboration_cleanup_health());
    let context_changed = last_signal.as_ref().is_none_or(|previous| {
        previous.account_user_id != signal.account_user_id
            || previous.account_epoch != signal.account_epoch
    });
    let sessions_changed = replay_catalog
        || force
        || context_changed
        || last_signal
            .as_ref()
            .is_none_or(|previous| previous.sessions != signal.sessions);
    let rooms_changed = replay_catalog
        || force
        || context_changed
        || last_signal
            .as_ref()
            .is_none_or(|previous| previous.rooms != signal.rooms);
    let runtime_health_changed = last_signal
        .as_ref()
        .is_none_or(|previous| previous.runtime_health != signal.runtime_health);
    if !sessions_changed && !rooms_changed && !runtime_health_changed {
        return Ok(());
    }

    let account_user_id = signal.account_user_id.clone();
    let account_epoch = signal.account_epoch;
    if sessions_changed {
        if replay_catalog || context_changed || last_signal.is_none() {
            publish_session_catalog_snapshot(
                tx,
                account_user_id.clone(),
                account_epoch,
                signal.sessions.clone(),
            )
            .await?;
        } else if let Some(previous) = last_signal.as_ref() {
            publish_session_catalog_changes(
                tx,
                account_user_id.clone(),
                account_epoch,
                &previous.sessions,
                &signal.sessions,
            )
            .await?;
        }
    }

    if rooms_changed {
        if let Some(rooms) = signal.rooms.as_deref() {
            publish_room_catalog(tx, account_user_id, account_epoch, rooms).await?;
        } else if last_signal
            .as_ref()
            .and_then(|previous| previous.rooms.as_ref())
            .is_some()
        {
            publish_room_catalog(tx, account_user_id, account_epoch, &[]).await?;
        }
    }

    if runtime_health_changed {
        tx.send_system(SystemEvent::RuntimeHealth {
            collaboration_cleanup: signal.runtime_health.collaboration_cleanup.clone(),
        })
        .await?;
    }

    *last_signal = Some(signal);

    Ok(())
}

async fn publish_session_catalog_snapshot(
    tx: &RuntimeEventSender,
    account_user_id: Option<String>,
    account_epoch: u64,
    sessions: Vec<SessionListEntry>,
) -> Result<()> {
    tracing::debug!(session_count = sessions.len(), "publishing session.list");
    tx.send_session(
        account_user_id,
        account_epoch,
        session_catalog_snapshot_event(sessions),
    )
    .await
}

async fn publish_session_catalog_changes(
    tx: &RuntimeEventSender,
    account_user_id: Option<String>,
    account_epoch: u64,
    previous: &[SessionListEntry],
    current: &[SessionListEntry],
) -> Result<()> {
    let mut previous_index = 0;
    let mut current_index = 0;
    while previous_index < previous.len() || current_index < current.len() {
        match (previous.get(previous_index), current.get(current_index)) {
            (Some(before), Some(after)) => match before.id().cmp(after.id()) {
                std::cmp::Ordering::Less => {
                    publish_session_change(
                        tx,
                        account_user_id.clone(),
                        account_epoch,
                        SessionEvent::Removed {
                            session_id: before.id().to_owned(),
                        },
                    )
                    .await?;
                    previous_index += 1;
                }
                std::cmp::Ordering::Greater => {
                    publish_session_change(
                        tx,
                        account_user_id.clone(),
                        account_epoch,
                        SessionEvent::Upsert {
                            session: Box::new(after.clone()),
                        },
                    )
                    .await?;
                    current_index += 1;
                }
                std::cmp::Ordering::Equal => {
                    if before != after {
                        publish_session_change(
                            tx,
                            account_user_id.clone(),
                            account_epoch,
                            SessionEvent::Upsert {
                                session: Box::new(after.clone()),
                            },
                        )
                        .await?;
                    }
                    previous_index += 1;
                    current_index += 1;
                }
            },
            (Some(before), None) => {
                publish_session_change(
                    tx,
                    account_user_id.clone(),
                    account_epoch,
                    SessionEvent::Removed {
                        session_id: before.id().to_owned(),
                    },
                )
                .await?;
                previous_index += 1;
            }
            (None, Some(after)) => {
                publish_session_change(
                    tx,
                    account_user_id.clone(),
                    account_epoch,
                    SessionEvent::Upsert {
                        session: Box::new(after.clone()),
                    },
                )
                .await?;
                current_index += 1;
            }
            (None, None) => break,
        }
    }
    Ok(())
}

async fn publish_session_change(
    tx: &RuntimeEventSender,
    account_user_id: Option<String>,
    account_epoch: u64,
    event: SessionEvent,
) -> Result<()> {
    tracing::debug!("publishing session catalog change");
    tx.send_session(account_user_id, account_epoch, event).await
}

async fn publish_room_catalog(
    tx: &RuntimeEventSender,
    account_user_id: Option<String>,
    account_epoch: u64,
    rooms: &[RoomListEntry],
) -> Result<()> {
    tracing::debug!(room_count = rooms.len(), "publishing room.list");
    tx.send_session(account_user_id, account_epoch, room_catalog_event(rooms))
        .await
}
