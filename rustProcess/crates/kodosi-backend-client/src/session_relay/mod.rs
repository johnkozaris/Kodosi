mod connect;
mod connected;
pub mod cursor;
pub mod events;
pub mod incoming;
pub mod wire;
pub mod ws;

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use zeroize::{Zeroize, Zeroizing};

use crate::{
    auth::BackendAuthProvider,
    backoff::{ReconnectBackoff, wait_for_reauth},
    crypto,
    http_client::BackendHttpClient,
    session_key_service::SessionKeyTrustAnchor,
};
use kodosi_domain::{ids::SessionId, lifecycle::ConnectionState};

use self::{
    connect::{ConnectAttemptOutcome, attempt_connect_cycle},
    connected::{ConnectedSessionOutcome, reject_queued_commands, run_connected_session},
    cursor::ReplayCursor,
    events::SessionRelayEventSink,
    ws::SessionRelayWsClient,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteRelayMode {
    SharedParticipant,
    OwnerParticipant,
}

pub struct SessionRelayClientSpec {
    pub id: SessionId,
    pub backend_session_id: String,
    pub backend_incarnation_id: uuid::Uuid,
    pub cursor: ReplayCursor,
    pub relay_mode: RemoteRelayMode,
    pub session_key: Option<crypto::SessionKey>,
    pub viewer_kem_secret_bytes: Option<Vec<u8>>,
    pub viewer_device_id: Option<String>,
    pub viewer_user_id: Option<String>,
    pub viewer_signing_pkcs8: Option<Vec<u8>>,
    pub owner_user_id: Option<String>,
}

impl Drop for SessionRelayClientSpec {
    fn drop(&mut self) {
        self.viewer_kem_secret_bytes.zeroize();
        self.viewer_signing_pkcs8.zeroize();
        self.session_key.zeroize();
    }
}

#[derive(Debug)]
pub struct SessionRelayClientHandle {
    pub cancellation: CancellationToken,
    pub command_tx: mpsc::Sender<SessionRelayCommand>,
    pub join_handle: tokio::task::JoinHandle<()>,
}

#[derive(Debug)]
pub enum SessionRelayCommand {
    SemanticSend {
        request_id: uuid::Uuid,
        incarnation_id: uuid::Uuid,
        mode: wire::RelaySemanticMode,
        payload_sha256: String,
        text: String,
    },
    SemanticCancel {
        request_id: uuid::Uuid,
        incarnation_id: uuid::Uuid,
        mode: wire::RelaySemanticMode,
        payload_sha256: String,
    },
    Suggest {
        action_id: String,
        body: String,
    },
    Inject {
        action_id: String,
        payload: Vec<u8>,
    },
    PermissionDecision {
        action_id: String,
        request_id: String,
        request_generation: u64,
        decision: String,
    },
    OwnerInject {
        payload: Vec<u8>,
    },
    OwnerResize {
        action_id: String,
        rows: u16,
        cols: u16,
        pixel_geometry: Option<kodosi_domain::terminal::TerminalPixelGeometry>,

        claim: bool,
    },
    OwnerFocusChanged {
        focused: bool,
    },
    OwnerStop,
    OwnerInterrupt,
}

pub fn spawn(
    spec: SessionRelayClientSpec,
    backend: BackendHttpClient,
    trust_anchor: Arc<dyn SessionKeyTrustAnchor>,
    session_relay_ws: SessionRelayWsClient,
    auth_provider: BackendAuthProvider,
    session_events: mpsc::Sender<events::SessionRelayEvent>,
    cancellation: CancellationToken,
) -> SessionRelayClientHandle {
    let task_cancellation = cancellation.clone();
    let (command_tx, command_rx) = mpsc::channel(32);

    let join_handle = tokio::spawn(run_loop(
        spec,
        backend,
        trust_anchor,
        session_relay_ws,
        auth_provider,
        session_events,
        command_rx,
        task_cancellation,
    ));

    SessionRelayClientHandle {
        cancellation,
        command_tx,
        join_handle,
    }
}

fn reset_terminal_cursor_for_recovery(mut cursor: ReplayCursor) -> ReplayCursor {
    cursor.reset_terminal_replay();
    cursor
}

fn should_retry_terminal_recovery(attempted: bool, cursor: ReplayCursor) -> bool {
    !attempted && !cursor.has_fresh_terminal_replay()
}

const PARTICIPANT_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

#[expect(
    clippy::too_many_lines,
    reason = "one relay state loop keeps reconnect, recovery, and terminal exit transitions serialized"
)]
async fn run_loop(
    spec: SessionRelayClientSpec,
    mut backend: BackendHttpClient,
    trust_anchor: Arc<dyn SessionKeyTrustAnchor>,
    session_relay_ws: SessionRelayWsClient,
    auth_provider: BackendAuthProvider,
    session_events: mpsc::Sender<events::SessionRelayEvent>,
    mut command_rx: mpsc::Receiver<SessionRelayCommand>,
    cancellation: CancellationToken,
) {
    let mut reconnect_backoff = ReconnectBackoff::standard();
    let mut cursor = spec.cursor;
    let mut connected_once = false;
    let mut owner_pin_established = false;
    let mut terminal_recovery_attempted = false;
    let mut awaiting_reauthentication = false;
    let mut force_refresh_token: Option<Zeroizing<String>> = None;
    let events = SessionRelayEventSink::new(session_events, spec.id, cancellation.clone());

    loop {
        if cancellation.is_cancelled() {
            break;
        }

        events
            .connection(
                if connected_once {
                    ConnectionState::Reconnecting
                } else {
                    ConnectionState::Connecting
                },
                None,
            )
            .await;

        let force_refresh = force_refresh_token.take();
        let connect_attempt = attempt_connect_cycle(
            &spec,
            &mut backend,
            trust_anchor.as_ref(),
            &session_relay_ws,
            &auth_provider,
            &events,
            cursor,
            &mut awaiting_reauthentication,
            !owner_pin_established,
            force_refresh.as_deref().map(String::as_str),
        );
        let connect_outcome = tokio::select! {
            () = cancellation.cancelled() => break,
            outcome = tokio::time::timeout(PARTICIPANT_CONNECT_TIMEOUT, connect_attempt) => {
                if let Ok(outcome) = outcome {
                    outcome
                } else {
                    events.log_error("participant relay connect timed out".to_owned()).await;
                    ConnectAttemptOutcome::Waiting
                }
            }
        };
        match connect_outcome {
            ConnectAttemptOutcome::Connected(connected) => {
                owner_pin_established |= connected.owner_pin_established;
                connected_once = true;
                reconnect_backoff.record_connected();
                match run_connected_session(
                    &spec,
                    *connected,
                    &events,
                    &mut command_rx,
                    &cancellation,
                    cursor,
                )
                .await
                {
                    ConnectedSessionOutcome::Reconnect {
                        cursor: next_cursor,
                        force_refresh_token: next_force_refresh_token,
                    } => {
                        reconnect_backoff.finish_connection();
                        cursor = next_cursor;
                        force_refresh_token = next_force_refresh_token;
                    }
                    ConnectedSessionOutcome::TerminalReplayGap { cursor: gap_cursor } => {
                        reconnect_backoff.finish_connection();
                        if !should_retry_terminal_recovery(terminal_recovery_attempted, gap_cursor)
                        {
                            events
                                .log_error(
                                    "terminal replay recovery failed after a fresh cursor"
                                        .to_owned(),
                                )
                                .await;
                            break;
                        }
                        cursor = reset_terminal_cursor_for_recovery(gap_cursor);
                        terminal_recovery_attempted = true;
                    }
                    ConnectedSessionOutcome::Exit => {
                        reconnect_backoff.finish_connection();
                        break;
                    }
                }
            }
            ConnectAttemptOutcome::RetryWithRejectedToken(rejected) => {
                force_refresh_token = Some(rejected);
                continue;
            }
            ConnectAttemptOutcome::Waiting => {}
        }

        if cancellation.is_cancelled() {
            break;
        }

        events.connection(ConnectionState::Reconnecting, None).await;

        let kept_alive = if awaiting_reauthentication {
            reconnect_backoff.reset();
            wait_for_reauth(&cancellation).await
        } else {
            reconnect_backoff.wait_after_failure(&cancellation).await
        };
        if !kept_alive {
            break;
        }
    }

    reject_queued_commands(&mut command_rx, &events).await;
    events.connection(ConnectionState::Offline, None).await;
    events.exited().await;
}

#[cfg(test)]
mod tests {
    use super::{ReplayCursor, reset_terminal_cursor_for_recovery, should_retry_terminal_recovery};

    #[test]
    fn terminal_gap_reset_preserves_pending_snapshot_and_key_for_fresh_join() {
        let reset = reset_terminal_cursor_for_recovery(ReplayCursor {
            checkpoint_revision: 7,
            presentation_revision: 9,
            next_sequence: Some(12),
            key_generation: 3,
            checkpoint_counter: Some(4),
            raw_counter: Some(5),
            presentation_counter: Some(6),
            pending_permissions_counter: Some(8),
            pending_permissions_generation: 41,
        });

        assert!(reset.has_fresh_terminal_replay());
        assert_eq!(reset.key_generation, 3);
        assert_eq!(reset.pending_permissions_generation, 41);
        assert_eq!(reset.pending_permissions_counter, Some(8));
    }

    #[test]
    fn terminal_gap_recovery_is_one_shot_and_rejects_already_fresh_cursor() {
        let stale = ReplayCursor {
            checkpoint_revision: 1,
            next_sequence: Some(2),
            ..ReplayCursor::default()
        };
        assert!(should_retry_terminal_recovery(false, stale));
        assert!(!should_retry_terminal_recovery(true, stale));
        assert!(!should_retry_terminal_recovery(
            false,
            ReplayCursor::default()
        ));
    }
}
