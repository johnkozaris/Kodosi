use futures_util::StreamExt;
use serde::Deserialize;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::mpsc,
};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{Message, protocol::CloseFrame},
};
use tokio_util::sync::CancellationToken;

use crate::{
    BackendClientError, Result,
    auth::{BackendAccessTokenState, BackendAuthProvider},
    backoff::ReconnectBackoff,
    close_reasons::{self, CloseBehavior},
    user_events_ws::UserEventsWsClient,
};
pub use kodosi_domain::{
    device_link::DeviceLinkOutcome as UserDeviceLinkResolution, user::IdentityLifecycleState,
};

#[derive(Debug)]
pub struct UserEventsHandle {
    pub cancellation: CancellationToken,
    pub join_handle: tokio::task::JoinHandle<()>,
}

#[derive(Debug, Deserialize)]
struct UserEventsEnvelope {
    #[serde(rename = "type")]
    message_type: String,
}

#[derive(Debug, Deserialize)]
struct DiscoveryInvalidatedMessage {
    surfaces: Vec<DiscoverySurface>,
    #[serde(default, rename = "roomId")]
    room_id: Option<String>,
}

#[derive(Debug, Clone)]
pub enum UserEvent {
    DiscoveryInvalidated {
        surfaces: Vec<DiscoverySurface>,
        room_id: Option<String>,
    },
    DeviceListChanged {
        user_id: String,
        generation: u64,
    },
    IdentityLifecycleChanged {
        user_id: String,
        identity_revision: u64,
        state: IdentityLifecycleState,
    },
    DeviceLinkSnapshot {
        requests: Vec<UserDeviceLinkSnapshotEntry>,
    },
    DeviceLinkRequested {
        user_code: String,
        device_label: String,
        expires_at: String,
    },
    DeviceLinkResolved {
        user_code: String,
        outcome: UserDeviceLinkResolution,
    },
    BackendAccessInvalid {
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiscoverySurface {
    Friends,
    RoomCatalog,
    RoomFeed,
    RoomChat,
    RoomTasks,
    OwnSessions,
}

impl DiscoverySurface {
    pub const fn all() -> [Self; 6] {
        [
            Self::Friends,
            Self::RoomCatalog,
            Self::RoomFeed,
            Self::RoomChat,
            Self::RoomTasks,
            Self::OwnSessions,
        ]
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserDeviceListChangedMessage {
    user_id: String,
    generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum IdentityLifecycleStateWire {
    Enrolled,
    Withdrawn,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserIdentityLifecycleChangedMessage {
    user_id: String,
    identity_revision: u64,
    incarnation_id: Option<uuid::Uuid>,
    state: IdentityLifecycleStateWire,
    generation: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserDeviceLinkSnapshotEntry {
    pub user_code: String,
    pub device_label: String,
    pub expires_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserDeviceLinkSnapshotMessage {
    requests: Vec<UserDeviceLinkSnapshotEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserDeviceLinkRequestedMessage {
    user_code: String,
    device_label: String,
    expires_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserDeviceLinkResolvedMessage {
    user_code: String,
    outcome: UserDeviceLinkResolutionWire,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum UserDeviceLinkResolutionWire {
    Approved,
    Cancelled,
}

pub fn spawn(
    user_events_ws: UserEventsWsClient,
    auth_provider: BackendAuthProvider,
    device_id: String,
    user_id: String,
    signing_pkcs8: zeroize::Zeroizing<Vec<u8>>,
    user_events: mpsc::Sender<UserEvent>,
    cancellation: CancellationToken,
) -> UserEventsHandle {
    let task_cancellation = cancellation.clone();
    let join_handle = tokio::spawn(run_loop(
        user_events_ws,
        auth_provider,
        device_id,
        user_id,
        signing_pkcs8,
        user_events,
        task_cancellation,
    ));

    UserEventsHandle {
        cancellation,
        join_handle,
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "one reconnect loop owns token refresh, device proof, receive, and backoff"
)]
async fn run_loop(
    user_events_ws: UserEventsWsClient,
    auth_provider: BackendAuthProvider,
    device_id: String,
    user_id: String,
    signing_pkcs8: zeroize::Zeroizing<Vec<u8>>,
    user_events: mpsc::Sender<UserEvent>,
    cancellation: CancellationToken,
) {
    let mut reconnect_backoff = ReconnectBackoff::standard();
    let mut force_refresh_token: Option<zeroize::Zeroizing<String>> = None;

    loop {
        if cancellation.is_cancelled() {
            break;
        }

        let rejected_hint = force_refresh_token.take();
        let attempted_exact_refresh = rejected_hint.is_some();
        let token_state = auth_provider
            .access_token(rejected_hint.map(|token| token.to_string()))
            .await;
        match token_state {
            Ok(BackendAccessTokenState::Ready { access_token, .. }) => {
                match user_events_ws
                    .connect(&access_token, &device_id, &user_id, signing_pkcs8.as_ref())
                    .await
                {
                    Ok(mut stream) => {
                        reconnect_backoff.record_connected();
                        tracing::debug!("discovery event stream connected");

                        if !emit_discovery_invalidated(
                            &user_events,
                            DiscoverySurface::all(),
                            None,
                            &cancellation,
                        )
                        .await
                        {
                            break;
                        }

                        match receive_loop(&mut stream, &user_events, &cancellation).await {
                            Ok(ReceiveOutcome::Reconnect { force_refresh }) => {
                                reconnect_backoff.finish_connection();
                                if force_refresh {
                                    force_refresh_token = Some(access_token);
                                }
                            }
                            Ok(ReceiveOutcome::Terminal { reason }) => {
                                tracing::warn!(%reason, "discovery event stream closed with terminal reason");
                                break;
                            }
                            Err(error) => {
                                reconnect_backoff.finish_connection();
                                tracing::debug!(%error, "discovery event stream disconnected");
                            }
                        }
                    }
                    Err(error) if error.is_auth_rejected() && !attempted_exact_refresh => {
                        tracing::warn!(reason = %error.reason(), "discovery event stream auth rejected; refreshing exact token");
                        force_refresh_token = Some(access_token);
                        continue;
                    }
                    Err(error) if error.is_auth_rejected() => {
                        tracing::warn!(reason = %error.reason(), "discovery event stream auth rejected");
                        let _ = send_user_event(
                            &user_events,
                            UserEvent::BackendAccessInvalid {
                                reason: error.reason().to_owned(),
                            },
                            &cancellation,
                        )
                        .await;
                        break;
                    }
                    Err(error) if error.is_configuration() => {
                        tracing::warn!(reason = %error.reason(), "discovery event stream stopped");
                        break;
                    }
                    Err(error) => {
                        tracing::debug!(%error, "discovery event stream connect failed");
                    }
                }
            }
            Ok(BackendAccessTokenState::RequiresLogin { reason }) => {
                tracing::info!(%reason, "discovery event stream stopped until login");
                let _ = send_user_event(
                    &user_events,
                    UserEvent::BackendAccessInvalid {
                        reason: reason.to_string(),
                    },
                    &cancellation,
                )
                .await;
                break;
            }
            Ok(BackendAccessTokenState::TemporarilyUnavailable { reason }) => {
                tracing::debug!(%reason, "discovery event stream waiting for auth refresh");
            }
            Err(error) => {
                tracing::debug!(%error, "discovery event stream auth check failed");
            }
        }

        if cancellation.is_cancelled() {
            break;
        }

        if !reconnect_backoff.wait_after_failure(&cancellation).await {
            break;
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ReceiveOutcome {
    Reconnect { force_refresh: bool },
    Terminal { reason: String },
}

async fn receive_loop<S>(
    stream: &mut WebSocketStream<S>,
    user_events: &mpsc::Sender<UserEvent>,
    cancellation: &CancellationToken,
) -> Result<ReceiveOutcome>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        tokio::select! {
            () = cancellation.cancelled() => {
                if let Err(error) = stream.close(None).await {
                    tracing::debug!(%error, "user-events close handshake failed");
                }
                return Ok(ReceiveOutcome::Reconnect { force_refresh: false });
            }
            message = stream.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        if let Err(error) = handle_message_with_cancellation(
                            text.as_ref(),
                            user_events,
                            cancellation,
                        )
                        .await {
                            tracing::warn!(%error, "user-events protocol drift");
                            return Ok(ReceiveOutcome::Terminal {
                                reason: format!("user-events protocol drift: {error}"),
                            });
                        }
                    }
                    Some(Ok(Message::Close(frame))) => {
                        return Ok(receive_outcome_for_close(frame.as_ref()));
                    }
                    Some(Ok(Message::Binary(_) | Message::Frame(_))) => {
                        return Ok(ReceiveOutcome::Terminal {
                            reason: "user-events protocol drift: unsupported non-text frame".to_owned(),
                        });
                    }
                    None => {
                        return Ok(ReceiveOutcome::Reconnect { force_refresh: false });
                    }
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                    Some(Err(error)) => return Err(BackendClientError::WebSocket(error)),
                }
            }
        }
    }
}

fn receive_outcome_for_close(frame: Option<&CloseFrame>) -> ReceiveOutcome {
    match close_reasons::interpret(frame) {
        CloseBehavior::AuthRefresh => ReceiveOutcome::Reconnect {
            force_refresh: true,
        },
        CloseBehavior::Terminal { reason } => ReceiveOutcome::Terminal { reason },
        CloseBehavior::TerminalReplayGap => ReceiveOutcome::Terminal {
            reason: close_reasons::TERMINAL_REPLAY_GAP_REASON.to_owned(),
        },
        CloseBehavior::Retryable => ReceiveOutcome::Reconnect {
            force_refresh: false,
        },
    }
}

#[cfg(test)]
async fn handle_message(message: &str, user_events: &mpsc::Sender<UserEvent>) -> Result<()> {
    handle_message_with_cancellation(message, user_events, &CancellationToken::new()).await
}

#[expect(
    clippy::too_many_lines,
    reason = "one exhaustive match keeps the user-event wire surface auditable"
)]
async fn handle_message_with_cancellation(
    message: &str,
    user_events: &mpsc::Sender<UserEvent>,
    cancellation: &CancellationToken,
) -> Result<()> {
    let envelope: UserEventsEnvelope = serde_json::from_str(message)?;
    tracing::debug!(
        message_type = %envelope.message_type,
        "user-events message received"
    );
    match envelope.message_type.as_str() {
        "discovery.invalidated" => {
            let invalidated: DiscoveryInvalidatedMessage = serde_json::from_str(message)?;
            if invalidated.surfaces.is_empty() {
                return Err(BackendClientError::Protocol {
                    reason: "discovery.invalidated requires at least one surface".to_owned(),
                });
            }

            if !emit_discovery_invalidated(
                user_events,
                invalidated.surfaces,
                invalidated.room_id,
                cancellation,
            )
            .await
            {
                return Err(BackendClientError::Unsupported {
                    reason: "user-events delivery cancelled".to_owned(),
                });
            }
        }
        "user.deviceListChanged" => {
            let changed: UserDeviceListChangedMessage = serde_json::from_str(message)?;
            if changed.generation == 0 {
                return Err(BackendClientError::Protocol {
                    reason: "user.deviceListChanged generation must be positive".to_owned(),
                });
            }
            if !send_user_event(
                user_events,
                UserEvent::DeviceListChanged {
                    user_id: changed.user_id,
                    generation: changed.generation,
                },
                cancellation,
            )
            .await
            {
                return Err(delivery_cancelled());
            }
        }
        "user.identityLifecycleChanged" => {
            let changed: UserIdentityLifecycleChangedMessage = serde_json::from_str(message)?;
            let state = match (changed.state, changed.incarnation_id, changed.generation) {
                (IdentityLifecycleStateWire::Enrolled, Some(incarnation_id), generation)
                    if changed.identity_revision > 0
                        && !incarnation_id.is_nil()
                        && generation > 0 =>
                {
                    IdentityLifecycleState::Enrolled { incarnation_id }
                }
                (IdentityLifecycleStateWire::Withdrawn, None, 0)
                    if changed.identity_revision > 0 =>
                {
                    IdentityLifecycleState::Withdrawn
                }
                _ => {
                    return Err(BackendClientError::Protocol {
                        reason: "invalid user.identityLifecycleChanged state shape".to_owned(),
                    });
                }
            };
            if !send_user_event(
                user_events,
                UserEvent::IdentityLifecycleChanged {
                    user_id: changed.user_id,
                    identity_revision: changed.identity_revision,
                    state,
                },
                cancellation,
            )
            .await
            {
                return Err(delivery_cancelled());
            }
        }
        "user.deviceLinkSnapshot" => {
            let snapshot: UserDeviceLinkSnapshotMessage = serde_json::from_str(message)?;
            if !send_user_event(
                user_events,
                UserEvent::DeviceLinkSnapshot {
                    requests: snapshot.requests,
                },
                cancellation,
            )
            .await
            {
                return Err(delivery_cancelled());
            }
        }
        "user.deviceLinkRequested" => {
            let requested: UserDeviceLinkRequestedMessage = serde_json::from_str(message)?;
            if !send_user_event(
                user_events,
                UserEvent::DeviceLinkRequested {
                    user_code: requested.user_code,
                    device_label: requested.device_label,
                    expires_at: requested.expires_at,
                },
                cancellation,
            )
            .await
            {
                return Err(delivery_cancelled());
            }
        }
        "user.deviceLinkResolved" => {
            let resolved: UserDeviceLinkResolvedMessage = serde_json::from_str(message)?;
            let outcome = match resolved.outcome {
                UserDeviceLinkResolutionWire::Approved => UserDeviceLinkResolution::Approved,
                UserDeviceLinkResolutionWire::Cancelled => UserDeviceLinkResolution::Cancelled,
            };
            if !send_user_event(
                user_events,
                UserEvent::DeviceLinkResolved {
                    user_code: resolved.user_code,
                    outcome,
                },
                cancellation,
            )
            .await
            {
                return Err(delivery_cancelled());
            }
        }
        unknown_type => {
            tracing::warn!(
                message_type = unknown_type,
                "unknown user-event message type"
            );
            return Err(BackendClientError::Protocol {
                reason: format!("unknown user-event message type `{unknown_type}`"),
            });
        }
    }
    Ok(())
}

async fn emit_discovery_invalidated<I>(
    user_events: &mpsc::Sender<UserEvent>,
    surfaces: I,
    room_id: Option<String>,
    cancellation: &CancellationToken,
) -> bool
where
    I: IntoIterator<Item = DiscoverySurface>,
{
    let resolved = surfaces.into_iter().collect::<Vec<_>>();
    if resolved.is_empty() {
        return true;
    }

    send_user_event(
        user_events,
        UserEvent::DiscoveryInvalidated {
            surfaces: resolved,
            room_id,
        },
        cancellation,
    )
    .await
}

fn delivery_cancelled() -> BackendClientError {
    BackendClientError::Unsupported {
        reason: "user-events delivery cancelled".to_owned(),
    }
}

async fn send_user_event(
    user_events: &mpsc::Sender<UserEvent>,
    event: UserEvent,
    cancellation: &CancellationToken,
) -> bool {
    tokio::select! {
        () = cancellation.cancelled() => false,
        result = user_events.send(event) => result.is_ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::IdentityLifecycleState;
    use std::time::Duration;

    use futures_util::{SinkExt, StreamExt};
    use tokio::{io::duplex, sync::mpsc, time::timeout};
    use tokio_tungstenite::{
        WebSocketStream,
        tungstenite::{
            Message,
            protocol::{CloseFrame, Role, frame::coding::CloseCode},
        },
    };
    use tokio_util::sync::CancellationToken;

    use super::{
        DiscoverySurface, ReceiveOutcome, UserDeviceLinkResolution, UserEvent, handle_message,
        receive_loop, receive_outcome_for_close,
    };
    use crate::BackendClientError;

    const UNKNOWN_MESSAGE_POLICY: &str = "terminalClose";
    const MALFORMED_PAYLOAD_POLICY: &str = "terminalClose";
    use serde::Deserialize;

    fn frame(reason: &str) -> CloseFrame {
        CloseFrame {
            code: CloseCode::Policy,
            reason: reason.into(),
        }
    }

    #[tokio::test]
    async fn cancellation_sends_close_before_the_user_events_task_exits() {
        let (client_io, server_io) = duplex(4 * 1024);
        let mut client = WebSocketStream::from_raw_socket(client_io, Role::Client, None).await;
        let mut server = WebSocketStream::from_raw_socket(server_io, Role::Server, None).await;
        let (events_tx, _events_rx) = mpsc::channel(1);
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let receive_task =
            tokio::spawn(
                async move { receive_loop(&mut client, &events_tx, &task_cancellation).await },
            );

        cancellation.cancel();

        let received = timeout(Duration::from_secs(1), server.next())
            .await
            .unwrap_or_else(|error| panic!("close frame should arrive: {error}"))
            .unwrap_or_else(|| panic!("client should send one close frame"))
            .unwrap_or_else(|error| panic!("close frame should decode: {error}"));
        std::assert_matches!(received, Message::Close(_));

        let outcome = receive_task
            .await
            .unwrap_or_else(|error| panic!("receive task should join: {error}"))
            .unwrap_or_else(|error| panic!("receive loop should stop cleanly: {error}"));
        assert_eq!(
            outcome,
            ReceiveOutcome::Reconnect {
                force_refresh: false
            }
        );
    }

    #[tokio::test]
    async fn malformed_device_link_resolution_terminates_receive_loop() {
        let (client_io, server_io) = duplex(4 * 1024);
        let mut client = WebSocketStream::from_raw_socket(client_io, Role::Client, None).await;
        let mut server = WebSocketStream::from_raw_socket(server_io, Role::Server, None).await;
        let (events_tx, mut events_rx) = mpsc::channel(1);
        let cancellation = CancellationToken::new();

        server
            .send(Message::Text(
                r#"{"type":"user.deviceLinkResolved","userCode":"ABCD-EFGH","outcome":"expired"}"#
                    .into(),
            ))
            .await
            .expect("server should send malformed event");
        let outcome = receive_loop(&mut client, &events_tx, &cancellation)
            .await
            .expect("protocol drift should produce a terminal outcome");

        std::assert_matches!(
            outcome,
            ReceiveOutcome::Terminal { ref reason }
                if reason.contains("user-events protocol drift")
        );
        assert!(events_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn discovery_invalidated_messages_emit_user_event() {
        let (tx, mut rx) = mpsc::channel(1);

        handle_message(
            r#"{"type":"discovery.invalidated","surfaces":["friends","room-feed","own-sessions"]}"#,
            &tx,
        )
        .await
        .unwrap_or_else(|error| panic!("message should parse: {error}"));

        std::assert_matches!(
            rx.recv().await,
            Some(UserEvent::DiscoveryInvalidated { surfaces, room_id })
                if room_id.is_none() && surfaces == vec![
                    DiscoverySurface::Friends,
                    DiscoverySurface::RoomFeed,
                    DiscoverySurface::OwnSessions,
                ]
        );
    }

    #[tokio::test]
    async fn scoped_room_invalidation_preserves_room_id() {
        let (tx, mut rx) = mpsc::channel(1);

        handle_message(
            r#"{"type":"discovery.invalidated","surfaces":["room-chat","room-tasks"],"roomId":"room-7"}"#,
            &tx,
        )
        .await
        .unwrap_or_else(|error| panic!("message should parse: {error}"));

        std::assert_matches!(
            rx.recv().await,
            Some(UserEvent::DiscoveryInvalidated { surfaces, room_id })
                if room_id.as_deref() == Some("room-7")
                    && surfaces == vec![DiscoverySurface::RoomChat, DiscoverySurface::RoomTasks]
        );
    }

    #[tokio::test]
    async fn identity_lifecycle_message_validates_state_shape() {
        let (tx, mut rx) = mpsc::channel(1);
        let incarnation = uuid::Uuid::now_v7();
        handle_message(
            &format!(
                r#"{{"type":"user.identityLifecycleChanged","userId":"alice","identityRevision":3,"incarnationId":"{incarnation}","state":"enrolled","generation":1}}"#
            ),
            &tx,
        )
        .await
        .expect("valid lifecycle message");
        std::assert_matches!(
            rx.recv().await,
            Some(UserEvent::IdentityLifecycleChanged {
                user_id,
                identity_revision: 3,
                state: IdentityLifecycleState::Enrolled { incarnation_id },
            }) if user_id == "alice" && incarnation_id == incarnation
        );

        let invalid = handle_message(
            &format!(
                r#"{{"type":"user.identityLifecycleChanged","userId":"alice","identityRevision":4,"incarnationId":"{incarnation}","state":"withdrawn","generation":0}}"#
            ),
            &tx,
        )
        .await
        .expect_err("withdrawn lifecycle cannot name an incarnation");
        std::assert_matches!(invalid, BackendClientError::Protocol { .. });
    }

    #[tokio::test]
    async fn user_device_list_changed_emits_user_event() {
        let (tx, mut rx) = mpsc::channel(1);

        handle_message(
            r#"{"type":"user.deviceListChanged","userId":"alice","generation":7}"#,
            &tx,
        )
        .await
        .unwrap_or_else(|error| panic!("message should parse: {error}"));

        match rx.recv().await {
            Some(UserEvent::DeviceListChanged {
                user_id,
                generation,
            }) => {
                assert_eq!(user_id, "alice");
                assert_eq!(generation, 7);
            }
            other => panic!("expected UserDeviceListChanged, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn user_device_list_changed_requires_positive_generation() {
        let (tx, _rx) = mpsc::channel(1);

        let error = handle_message(
            r#"{"type":"user.deviceListChanged","userId":"alice","generation":0}"#,
            &tx,
        )
        .await
        .expect_err("generation zero is not a device-list change");

        std::assert_matches!(error, BackendClientError::Protocol { .. });
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct UserEventsAuthority {
        unknown_message_policy: String,
        malformed_payload_policy: String,
    }

    #[test]
    fn user_events_authority_declares_fail_closed_policies() {
        let authority: UserEventsAuthority = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../protocol/user-events-authority.json"
        )))
        .unwrap_or_else(|error| panic!("user-events-authority.json should deserialize: {error}"));

        assert_eq!(UNKNOWN_MESSAGE_POLICY, authority.unknown_message_policy);
        assert_eq!(MALFORMED_PAYLOAD_POLICY, authority.malformed_payload_policy);
    }

    #[tokio::test]
    async fn unknown_message_type_is_terminal_protocol_drift() {
        let (tx, mut rx) = mpsc::channel(1);

        let error = handle_message(r#"{"type":"some.future.event","foo":42}"#, &tx)
            .await
            .expect_err("unknown types must fail closed");

        std::assert_matches!(error, BackendClientError::Protocol { ref reason }
                if reason.contains("some.future.event"),
            "unexpected error: {error:?}"
        );
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn empty_discovery_invalidation_is_terminal_protocol_drift() {
        let (tx, mut rx) = mpsc::channel(1);

        let error = handle_message(r#"{"type":"discovery.invalidated","surfaces":[]}"#, &tx)
            .await
            .expect_err("empty invalidation must fail closed");

        std::assert_matches!(error, BackendClientError::Protocol { ref reason }
                if reason.contains("requires at least one surface"),
            "unexpected error: {error:?}"
        );
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn terminal_close_reasons_stop_user_events_reconnect() {
        assert_eq!(
            receive_outcome_for_close(Some(&frame("host_stopped"))),
            ReceiveOutcome::Terminal {
                reason: "host_stopped".to_owned()
            }
        );
        assert_eq!(
            receive_outcome_for_close(Some(&frame("session_timeout"))),
            ReceiveOutcome::Terminal {
                reason: "session_timeout".to_owned()
            }
        );
    }

    #[test]
    fn auth_revoked_refreshes_and_unknown_retries_user_events() {
        assert_eq!(
            receive_outcome_for_close(Some(&frame("auth_revoked"))),
            ReceiveOutcome::Reconnect {
                force_refresh: true,
            }
        );
        assert_eq!(
            receive_outcome_for_close(Some(&frame("closing"))),
            ReceiveOutcome::Reconnect {
                force_refresh: false,
            }
        );
    }

    #[tokio::test]
    async fn device_link_requested_emits_user_event() {
        let (tx, mut rx) = mpsc::channel(1);

        handle_message(
            r#"{"type":"user.deviceLinkRequested","userCode":"ABCD-EFGH","deviceLabel":"mbp","expiresAt":"2026-04-22T15:30:00Z"}"#,
            &tx,
        )
        .await
        .unwrap_or_else(|error| panic!("message should parse: {error}"));

        match rx.recv().await {
            Some(UserEvent::DeviceLinkRequested {
                user_code,
                device_label,
                expires_at,
            }) => {
                assert_eq!(user_code, "ABCD-EFGH");
                assert_eq!(device_label, "mbp");
                assert_eq!(expires_at, "2026-04-22T15:30:00Z");
            }
            other => panic!("expected DeviceLinkRequested, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn device_link_resolved_emits_user_event() {
        let (tx, mut rx) = mpsc::channel(1);

        handle_message(
            r#"{"type":"user.deviceLinkResolved","userCode":"ABCD-EFGH","outcome":"approved"}"#,
            &tx,
        )
        .await
        .unwrap_or_else(|error| panic!("message should parse: {error}"));

        match rx.recv().await {
            Some(UserEvent::DeviceLinkResolved { user_code, outcome }) => {
                assert_eq!(user_code, "ABCD-EFGH");
                assert_eq!(outcome, UserDeviceLinkResolution::Approved);
            }
            other => panic!("expected DeviceLinkResolved, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn device_link_resolved_rejects_unknown_outcome() {
        let (tx, mut rx) = mpsc::channel(1);

        let error = handle_message(
            r#"{"type":"user.deviceLinkResolved","userCode":"ABCD-EFGH","outcome":"expired"}"#,
            &tx,
        )
        .await
        .expect_err("unknown resolution must terminate the malformed event stream");
        assert!(matches!(error, BackendClientError::Json(_)));
        assert!(rx.try_recv().is_err());
    }
}
