use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::session_runtime::events::{AccountEventOrigin, DiscoverySurface, RuntimeSessionEvent};
use kodosi_backend_client::user_events::{self, UserEvent};

pub(crate) fn spawn(
    origin: AccountEventOrigin,
    mut user_events: mpsc::Receiver<UserEvent>,
    session_events: mpsc::Sender<RuntimeSessionEvent>,
    cancellation: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut last_dropped_kind: Option<&'static str> = None;
        loop {
            tokio::select! {
                () = cancellation.cancelled() => break,
                event = user_events.recv() => {
                    let Some(event) = event else {
                        break;
                    };
                    let runtime = runtime_event(&origin, event);
                    let kind = runtime_event_kind(&runtime);
                    let send_result = tokio::select! {
                        () = cancellation.cancelled() => break,
                        result = session_events.send(runtime) => result,
                    };
                    if let Err(error) = send_result {
                        if last_dropped_kind != Some(kind) {
                            tracing::warn!(
                                kind,
                                %error,
                                "user-event bridge dropping events; runtime session_events sink is closed or full"
                            );
                            last_dropped_kind = Some(kind);
                        }
                    } else {
                        last_dropped_kind = None;
                    }
                }
            }
        }
    })
}

const fn runtime_event_kind(event: &RuntimeSessionEvent) -> &'static str {
    match event {
        RuntimeSessionEvent::DiscoveryInvalidated { .. } => "discovery_invalidated",
        RuntimeSessionEvent::UserIdentityLifecycleChanged { .. } => {
            "user_identity_lifecycle_changed"
        }
        RuntimeSessionEvent::UserDeviceListChanged { .. } => "user_device_list_changed",
        RuntimeSessionEvent::DeviceLinkSnapshot { .. } => "device_link_snapshot",
        RuntimeSessionEvent::DeviceLinkRequested { .. } => "device_link_requested",
        RuntimeSessionEvent::DeviceLinkResolved { .. } => "device_link_resolved",
        RuntimeSessionEvent::BackendAccessInvalid { .. } => "backend_access_invalid",
        _ => "other",
    }
}

fn runtime_event(origin: &AccountEventOrigin, event: UserEvent) -> RuntimeSessionEvent {
    match event {
        UserEvent::DiscoveryInvalidated { surfaces, room_id } => {
            RuntimeSessionEvent::DiscoveryInvalidated {
                origin: origin.clone(),
                surfaces: surfaces.into_iter().map(discovery_surface).collect(),
                room_id,
            }
        }
        UserEvent::DeviceListChanged {
            user_id,
            generation,
        } => RuntimeSessionEvent::UserDeviceListChanged {
            origin: origin.clone(),
            user_id,
            generation,
        },
        UserEvent::IdentityLifecycleChanged {
            user_id,
            identity_revision,
            state,
        } => RuntimeSessionEvent::UserIdentityLifecycleChanged {
            origin: origin.clone(),
            user_id,
            identity_revision,
            state,
        },
        UserEvent::DeviceLinkSnapshot { requests } => RuntimeSessionEvent::DeviceLinkSnapshot {
            origin: origin.clone(),
            requests,
        },
        UserEvent::DeviceLinkRequested {
            user_code,
            device_label,
            expires_at,
        } => RuntimeSessionEvent::DeviceLinkRequested {
            origin: origin.clone(),
            user_code,
            device_label,
            expires_at,
        },
        UserEvent::DeviceLinkResolved { user_code, outcome } => {
            RuntimeSessionEvent::DeviceLinkResolved {
                origin: origin.clone(),
                user_code,
                outcome,
            }
        }
        UserEvent::BackendAccessInvalid { reason } => RuntimeSessionEvent::BackendAccessInvalid {
            origin: origin.clone(),
            reason,
        },
    }
}

const fn discovery_surface(surface: user_events::DiscoverySurface) -> DiscoverySurface {
    match surface {
        user_events::DiscoverySurface::Friends => DiscoverySurface::Friends,
        user_events::DiscoverySurface::RoomCatalog => DiscoverySurface::RoomCatalog,
        user_events::DiscoverySurface::RoomFeed => DiscoverySurface::RoomFeed,
        user_events::DiscoverySurface::RoomChat => DiscoverySurface::RoomChat,
        user_events::DiscoverySurface::RoomTasks => DiscoverySurface::RoomTasks,
        user_events::DiscoverySurface::OwnSessions => DiscoverySurface::OwnSessions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_runtime::events::AccountEpoch;
    use kodosi_backend_client::user_events::UserDeviceLinkResolution;

    fn test_origin() -> AccountEventOrigin {
        AccountEventOrigin {
            account_user_id: "account-a".to_owned(),
            epoch: AccountEpoch::INITIAL.next().expect("test account epoch"),
        }
    }

    #[test]
    fn maps_backend_user_events_to_runtime_session_events() {
        std::assert_matches!(
            runtime_event(&test_origin(), UserEvent::DiscoveryInvalidated {
                surfaces: vec![user_events::DiscoverySurface::RoomFeed],
                room_id: Some("room-1".to_owned()),
            }),
            RuntimeSessionEvent::DiscoveryInvalidated {
                origin,
                surfaces,
                room_id,
            }
                if origin.account_user_id == "account-a"
                    && origin.epoch
                        == AccountEpoch::INITIAL.next().expect("test account epoch")
                    && room_id.as_deref() == Some("room-1")
                    && surfaces == vec![DiscoverySurface::RoomFeed]
        );
        std::assert_matches!(
            runtime_event(&test_origin(), UserEvent::DeviceLinkResolved {
                user_code: "ABCD-EFGH".to_owned(),
                outcome: UserDeviceLinkResolution::Approved,
            }),
            RuntimeSessionEvent::DeviceLinkResolved { origin, user_code, outcome }
                if origin == test_origin()
                    && user_code == "ABCD-EFGH"
                    && outcome == UserDeviceLinkResolution::Approved
        );
    }
}
