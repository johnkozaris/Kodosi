use tokio::sync::mpsc;

use crate::session_runtime::events::{AccountEventOrigin, RuntimeSessionEvent};
use kodosi_backend_client::session_relay::events::SessionRelayEvent;

pub(crate) fn spawn_session_relay_event_bridge(
    account_origin: AccountEventOrigin,
    mut relay_events: mpsc::Receiver<SessionRelayEvent>,
    session_events: mpsc::Sender<RuntimeSessionEvent>,
    relay_generation: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(event) = relay_events.recv().await {
            drop(
                session_events
                    .send(runtime_event(&account_origin, event, relay_generation))
                    .await,
            );
        }
    })
}

fn terminal_runtime_event(
    event: &SessionRelayEvent,
    relay_generation: u64,
) -> Option<RuntimeSessionEvent> {
    match event {
        SessionRelayEvent::RemoteCheckpoint {
            id,
            next_sequence,
            checkpoint,
            application,
        } => Some(RuntimeSessionEvent::RemoteCheckpoint {
            id: *id,
            next_sequence: *next_sequence,
            checkpoint: checkpoint.clone(),
            application: application.clone(),
            relay_generation,
        }),
        SessionRelayEvent::RemoteRawBatch {
            id,
            first_sequence,
            next_sequence,
            chunks,
        } => Some(RuntimeSessionEvent::RemoteRawBatch {
            id: *id,
            first_sequence: *first_sequence,
            next_sequence: *next_sequence,
            chunks: chunks.clone(),
            relay_generation,
        }),
        SessionRelayEvent::RemotePlainPresentation { id, presentation } => {
            Some(RuntimeSessionEvent::RemotePlainPresentation {
                id: *id,
                presentation: presentation.clone(),
                relay_generation,
            })
        }
        SessionRelayEvent::RemotePendingPermissionsSnapshot {
            id,
            incarnation_id,
            generation,
            snapshot,
        } => Some(RuntimeSessionEvent::RemotePendingPermissionsSnapshot {
            id: *id,
            incarnation_id: *incarnation_id,
            generation: *generation,
            snapshot: snapshot.clone(),
            relay_generation,
        }),
        _ => None,
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "exhaustive wire-event to runtime-event mapping stays auditable in one match"
)]
fn runtime_event(
    account_origin: &AccountEventOrigin,
    event: SessionRelayEvent,
    relay_generation: u64,
) -> RuntimeSessionEvent {
    if let Some(terminal) = terminal_runtime_event(&event, relay_generation) {
        return terminal;
    }
    match event {
        SessionRelayEvent::RemoteCheckpoint { .. }
        | SessionRelayEvent::RemoteRawBatch { .. }
        | SessionRelayEvent::RemotePlainPresentation { .. }
        | SessionRelayEvent::RemotePendingPermissionsSnapshot { .. } => unreachable!(),
        SessionRelayEvent::RemoteStateChanged { id, state } => {
            RuntimeSessionEvent::RemoteStateChanged {
                id,
                state,
                relay_generation,
            }
        }
        SessionRelayEvent::RemoteAccessChanged { id, access } => {
            RuntimeSessionEvent::RemoteAccessChanged {
                id,
                access,
                relay_generation,
            }
        }
        SessionRelayEvent::RemoteActionResult {
            id,
            action_id,
            request_id,
            request_generation,
            status,
        } => RuntimeSessionEvent::RemoteActionResult {
            id,
            action_id,
            request_id,
            request_generation,
            status,
            relay_generation,
        },
        SessionRelayEvent::RemoteSessionConnectionChanged { id, status, reason } => {
            RuntimeSessionEvent::RemoteSessionConnectionChanged {
                id,
                status,
                reason,
                relay_generation,
            }
        }
        SessionRelayEvent::RemoteAccessStateChanged {
            id,
            state,
            reason,
            issue,
        } => RuntimeSessionEvent::RemoteAccessStateChanged {
            id,
            state,
            reason,
            issue,
            relay_generation,
        },
        SessionRelayEvent::RemoteAccessRevoked { id } => RuntimeSessionEvent::RemoteAccessRevoked {
            id,
            relay_generation,
        },
        SessionRelayEvent::RemoteSessionRelayExited { id } => {
            RuntimeSessionEvent::RemoteSessionRelayExited {
                id,
                relay_generation,
            }
        }
        SessionRelayEvent::Info { id, message } => RuntimeSessionEvent::RemoteInfo {
            id,
            message,
            relay_generation,
        },
        SessionRelayEvent::LogError { id, message } => RuntimeSessionEvent::RemoteLogError {
            id,
            message,
            relay_generation,
        },
        SessionRelayEvent::BackendAccessInvalid { reason } => {
            RuntimeSessionEvent::BackendAccessInvalid {
                origin: account_origin.clone(),
                reason,
            }
        }
        SessionRelayEvent::RemoteSemanticReceipt {
            id,
            receipt,
            persistence,
        } => RuntimeSessionEvent::RemoteSemanticReceipt {
            id,
            receipt,
            persistence,
            relay_generation,
        },
        SessionRelayEvent::RemoteControlTrustEstablished {
            id,
            incarnation_id,
            owner_user_id,
            signer_device_id,
            signer_public_key,
            device_list_generation,
            identity_fingerprint,
        } => RuntimeSessionEvent::RemoteControlTrustEstablished {
            id,
            incarnation_id,
            owner_user_id,
            signer_device_id,
            signer_public_key,
            device_list_generation,
            identity_fingerprint,
            relay_generation,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_runtime::events::AccountEpoch;

    fn account_origin() -> AccountEventOrigin {
        AccountEventOrigin {
            account_user_id: "11111111-1111-1111-1111-111111111111".to_owned(),
            epoch: AccountEpoch::INITIAL.next().expect("test account epoch"),
        }
    }

    #[test]
    fn backend_access_invalid_keeps_session_relay_account_origin() {
        let origin = account_origin();
        let event = runtime_event(
            &origin,
            SessionRelayEvent::BackendAccessInvalid {
                reason: "expired".to_owned(),
            },
            7,
        );

        std::assert_matches!(
            event,
            RuntimeSessionEvent::BackendAccessInvalid {
                origin: event_origin,
                reason,
            } if event_origin == origin && reason == "expired"
        );
    }
}
