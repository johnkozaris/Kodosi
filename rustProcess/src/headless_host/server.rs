use ::time::{OffsetDateTime, format_description::well_known::Rfc3339};
use kodosi_domain::ids::SessionId;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{broadcast, mpsc},
    task::JoinSet,
    time,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    AppError, HostCommand, HostEvent, Result, RuntimeCommandSink, SystemCommand, SystemEvent,
    TerminalCommand, shutdown as shutdown_budget, start_embedded_runtime,
    support::io::framed_json::{self, FramedJsonReader, FramedJsonWriter},
    terminal_transport::{
        TerminalCapability, TerminalControlFrame, TerminalSurface, hub::SubscriberHandle,
    },
};

use super::{
    device_rpc::DeviceRpcResponse,
    dispatch::dispatch_runtime_events,
    handshake::{
        ConnectionLane, ControlClientFrame, ControlServerFrame, HOST_PROTOCOL_VERSION,
        HostHelloRequest, HostHelloResponse, TerminalLaneCapability,
    },
    local_endpoint::{
        self, LocalListener, LocalWriteHalf, accept_local, acquire_host_lock, bind_local,
        socket_path_for_dir,
    },
    state_file::{
        HOST_CONTRACT_VERSION, HostStateFile, cleanup_host_state_file_in, host_runtime_dir,
        write_host_state_file,
    },
    terminal_lane::{TerminalLaneClientFrame, TerminalLaneServerFrame},
};

const HOST_EMPTY_GRACE: std::time::Duration = std::time::Duration::from_secs(90);

const HOST_HELLO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const HOST_EVENT_BUFFER: usize = 512;

fn client_lane_shutdown_token() -> CancellationToken {
    CancellationToken::new()
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct HeadlessHostActivity {
    pub(super) active_local_sessions: usize,
    pub(super) active_remote_relays: usize,
    pub(super) pending_auth: bool,
    pub(super) pending_device_link: bool,
    pub(super) connected_clients: usize,
}

impl HeadlessHostActivity {
    pub(super) const fn keeps_host_alive(&self) -> bool {
        self.active_local_sessions > 0
            || self.active_remote_relays > 0
            || self.pending_auth
            || self.pending_device_link
            || self.connected_clients > 0
    }
}

struct ClientActivityLease {
    activity_tx: tokio::sync::watch::Sender<HeadlessHostActivity>,
}

impl ClientActivityLease {
    fn new(activity_tx: tokio::sync::watch::Sender<HeadlessHostActivity>) -> Self {
        activity_tx.send_modify(|activity| {
            activity.connected_clients = activity.connected_clients.saturating_add(1);
        });
        Self { activity_tx }
    }
}

impl Drop for ClientActivityLease {
    fn drop(&mut self) {
        self.activity_tx.send_modify(|activity| {
            activity.connected_clients = activity.connected_clients.saturating_sub(1);
        });
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "server startup keeps lock, endpoint, state publication, runtime dispatch, and ordered teardown in one auditable lifecycle"
)]
pub(crate) async fn run_server() -> Result<()> {
    let runtime_dir = host_runtime_dir().ok_or_else(|| AppError::Unsupported {
        reason: "headless host runtime directory is unavailable".to_owned(),
    })?;
    let _host_lock = acquire_host_lock(&runtime_dir)?;
    let socket_path = socket_path_for_dir(&runtime_dir);
    let listener: LocalListener = bind_local(&socket_path).await?;

    let mut runtime = start_embedded_runtime().await?;
    let runtime_event_rx = runtime.take_events().ok_or_else(|| AppError::Unsupported {
        reason: "embedded runtime receivers were already taken".to_owned(),
    })?;
    let command_sink = runtime.command_sink();
    let (events_tx, _) = broadcast::channel::<HostEvent>(HOST_EVENT_BUFFER);
    let (activity_tx, activity_rx) = tokio::sync::watch::channel(HeadlessHostActivity::default());

    let shutdown = runtime.shutdown_token();

    let client_shutdown = client_lane_shutdown_token();

    let state = HostStateFile {
        version: HOST_CONTRACT_VERSION,
        pid: std::process::id(),
        socket_path: socket_path.to_string_lossy().into_owned(),
        token: Uuid::now_v7().simple().to_string(),
        config_identity: Some(crate::config::configuration_identity()?),
        started_at: OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|error| AppError::Unsupported {
                reason: format!("failed to render host start time: {error}"),
            })?,
    };
    write_host_state_file(&state)?;

    let shutdown_for_dispatch = shutdown.clone();
    let command_sink_for_clients = command_sink.clone();

    let runtime_finished = CancellationToken::new();
    let dispatch_handle = tokio::spawn(dispatch_runtime_events(
        runtime_event_rx,
        events_tx.clone(),
        activity_tx.clone(),
        shutdown_for_dispatch,
        runtime_finished.clone(),
    ));
    let idle_handle = tokio::spawn(idle_shutdown_monitor(
        command_sink.clone(),
        activity_rx,
        shutdown.clone(),
    ));
    let mut client_tasks = JoinSet::new();
    let mut listener_failure: Option<AppError> = None;

    loop {
        tokio::select! {
            () = shutdown.cancelled() => break,
            accept_result = accept_local(&listener) => {
                let stream = match accept_result {
                    Ok(stream) => stream,
                    Err(error) => {



                        listener_failure = Some(AppError::Io(error));
                        break;
                    }
                };
                let client_activity = ClientActivityLease::new(activity_tx.clone());
                let expected_token = state.token.clone();
                let client_command_sink = command_sink_for_clients.clone();
                let client_events = events_tx.subscribe();
                let client_shutdown = client_shutdown.clone();
                client_tasks.spawn(async move {
                    let _client_activity = client_activity;
                    handle_client(
                        stream,
                        expected_token,
                        client_command_sink,
                        client_events,
                        client_shutdown,
                    ).await;
                });
            }
            joined = client_tasks.join_next(), if !client_tasks.is_empty() => {
                if let Some(Err(error)) = joined {
                    if error.is_panic() {
                        tracing::warn!(%error, "headless host client task panicked");
                    } else {
                        tracing::debug!(%error, "headless host client task ended before join");
                    }
                }
            }
        }
    }

    let drain_deadline = time::Instant::now() + shutdown_budget::HOST_SHUTDOWN_BUDGET;

    stop_serving(&runtime_dir, &socket_path, listener);

    let runtime_result = runtime.join_before(drain_deadline).await;
    runtime_finished.cancel();
    shutdown_budget::join_within("headless host dispatch", dispatch_handle, drain_deadline).await;
    client_shutdown.cancel();

    while !client_tasks.is_empty() && time::Instant::now() < drain_deadline {
        let Some(remaining) = drain_deadline.checked_duration_since(time::Instant::now()) else {
            break;
        };
        match time::timeout(remaining, client_tasks.join_next()).await {
            Ok(Some(Err(error))) if error.is_panic() => {
                tracing::warn!(%error, "headless host client task panicked during shutdown");
            }
            Ok(Some(Err(error))) => {
                tracing::debug!(%error, "headless host client task ended before shutdown drain");
            }
            Ok(Some(Ok(()))) => {}
            Ok(None) | Err(_) => break,
        }
    }
    client_tasks.abort_all();
    while let Some(result) = client_tasks.join_next().await {
        if let Err(error) = result
            && error.is_panic()
        {
            tracing::warn!(%error, "headless host client task panicked during shutdown");
        }
    }

    shutdown_budget::join_within("headless host idle monitor", idle_handle, drain_deadline).await;

    listener_failure.map_or(runtime_result, Err)
}

pub(in crate::headless_host) fn stop_serving(
    runtime_dir: &std::path::Path,
    socket_path: &std::path::Path,
    listener: LocalListener,
) {
    cleanup_host_state_file_in(runtime_dir);
    local_endpoint::remove_socket_file(socket_path);
    drop(listener);
}

async fn idle_shutdown_monitor(
    command_sink: RuntimeCommandSink,
    mut activity_rx: tokio::sync::watch::Receiver<HeadlessHostActivity>,
    shutdown: CancellationToken,
) {
    loop {
        if activity_rx.borrow().keeps_host_alive() {
            if activity_rx.changed().await.is_err() {
                return;
            }
            continue;
        }

        tokio::select! {
            () = shutdown.cancelled() => return,
            changed = activity_rx.changed() => {
                if changed.is_err() {
                    return;
                }
            }
            () = time::sleep(HOST_EMPTY_GRACE) => {
                if !activity_rx.borrow().keeps_host_alive() {
                    let _ = command_sink.send_system(SystemCommand::Shutdown).await;
                    return;
                }
            }
        }
    }
}

enum DeviceRpcInput {
    Revoke {
        expected_account_user_id: String,
        device_id: String,
    },
    ApproveLink {
        expected_account_user_id: String,
        user_code: String,
    },
    StartSelfLink {
        expected_account_user_id: String,
        label: Option<String>,
    },
    CancelSelfLink {
        expected_account_user_id: String,
    },
}

async fn serve_device_rpc(
    writer: &mut FramedJsonWriter<LocalWriteHalf>,
    command_sink: &RuntimeCommandSink,
    input: DeviceRpcInput,
) {
    if framed_json::write_json(writer, &HostHelloResponse::accept())
        .await
        .is_err()
    {
        return;
    }
    let (reply, response) = tokio::sync::oneshot::channel();
    let request = match input {
        DeviceRpcInput::Revoke {
            expected_account_user_id,
            device_id,
        } => crate::DeviceRpcRequest::Revoke {
            expected_account_user_id,
            device_id,
            reply,
        },
        DeviceRpcInput::ApproveLink {
            expected_account_user_id,
            user_code,
        } => crate::DeviceRpcRequest::ApproveLink {
            expected_account_user_id,
            user_code,
            reply,
        },
        DeviceRpcInput::StartSelfLink {
            expected_account_user_id,
            label,
        } => crate::DeviceRpcRequest::StartSelfLink {
            expected_account_user_id,
            label,
            reply,
        },
        DeviceRpcInput::CancelSelfLink {
            expected_account_user_id,
        } => crate::DeviceRpcRequest::CancelSelfLink {
            expected_account_user_id,
            reply,
        },
    };
    let response = if command_sink.send_device_rpc(request).await.is_err() {
        DeviceRpcResponse::Error {
            message: "runtime stopped before device operation was accepted".to_owned(),
        }
    } else {
        match response.await {
            Ok(Ok(crate::runtime::identity::DeviceRpcOutcome::Revoked(outcome))) => {
                DeviceRpcResponse::Revoked {
                    history_warning: outcome.history_warning,
                    revoked_device_id: outcome.revoked_device_id,
                    new_generation: outcome.new_generation,
                }
            }
            Ok(Ok(crate::runtime::identity::DeviceRpcOutcome::LinkApproved(outcome))) => {
                DeviceRpcResponse::LinkApproved {
                    approved_user_code: outcome.approved_user_code,
                    approved_device_id: outcome.approved_device_id,
                    approved_device_label: outcome.approved_device_label,
                    new_generation: outcome.new_generation,
                }
            }
            Ok(Ok(crate::runtime::identity::DeviceRpcOutcome::SelfLinkStarted(outcome))) => {
                DeviceRpcResponse::SelfLinkStarted {
                    device_id: outcome.device_id,
                    user_code: outcome.user_code,
                    expires_at: outcome.expires_at,
                }
            }
            Ok(Ok(crate::runtime::identity::DeviceRpcOutcome::SelfLinkCancellationRequested)) => {
                DeviceRpcResponse::SelfLinkCancellationRequested
            }
            Ok(Err(error)) => DeviceRpcResponse::Error {
                message: error.to_string(),
            },
            Err(_) => DeviceRpcResponse::Error {
                message: "runtime stopped before device operation completed".to_owned(),
            },
        }
    };
    drop(framed_json::write_json(writer, &response).await);
}

#[expect(
    clippy::too_many_lines,
    reason = "client handshake and lane dispatch kept together for clarity"
)]
async fn handle_client(
    stream: local_endpoint::LocalStream,
    expected_token: String,
    command_sink: RuntimeCommandSink,
    events_rx: broadcast::Receiver<HostEvent>,
    shutdown: CancellationToken,
) {
    let (read_half, write_half) = stream.into_split();
    let mut framed_reader = framed_json::reader(read_half);
    let mut framed_writer = framed_json::writer(write_half);

    let hello_read = time::timeout(
        HOST_HELLO_TIMEOUT,
        framed_json::read_json::<_, HostHelloRequest>(&mut framed_reader),
    )
    .await;
    let hello = match hello_read {
        Ok(Ok(Some(request))) => request,
        Ok(Ok(None)) => return,
        Ok(Err(error)) => {
            tracing::warn!(%error, "failed to read headless host hello");
            return;
        }
        Err(_elapsed) => {
            tracing::warn!(
                timeout_secs = HOST_HELLO_TIMEOUT.as_secs(),
                "headless host hello timed out; dropping connection"
            );
            return;
        }
    };

    if hello.token != expected_token {
        drop(
            framed_json::write_json(
                &mut framed_writer,
                &HostHelloResponse::reject("invalid host token"),
            )
            .await,
        );
        return;
    }

    if hello.protocol_version != HOST_PROTOCOL_VERSION {
        let msg = format!(
            "protocol version mismatch: client={} server={}",
            hello.protocol_version, HOST_PROTOCOL_VERSION
        );
        drop(framed_json::write_json(&mut framed_writer, &HostHelloResponse::reject(msg)).await);
        return;
    }

    match hello.lane {
        ConnectionLane::DeviceRevoke {
            expected_account_user_id,
            device_id,
        } => {
            serve_device_rpc(
                &mut framed_writer,
                &command_sink,
                DeviceRpcInput::Revoke {
                    expected_account_user_id,
                    device_id,
                },
            )
            .await;
            return;
        }
        ConnectionLane::DeviceApproveLink {
            expected_account_user_id,
            user_code,
        } => {
            serve_device_rpc(
                &mut framed_writer,
                &command_sink,
                DeviceRpcInput::ApproveLink {
                    expected_account_user_id,
                    user_code,
                },
            )
            .await;
            return;
        }
        ConnectionLane::DeviceStartSelfLink {
            expected_account_user_id,
            label,
        } => {
            serve_device_rpc(
                &mut framed_writer,
                &command_sink,
                DeviceRpcInput::StartSelfLink {
                    expected_account_user_id,
                    label,
                },
            )
            .await;
            return;
        }
        ConnectionLane::DeviceCancelSelfLink {
            expected_account_user_id,
        } => {
            serve_device_rpc(
                &mut framed_writer,
                &command_sink,
                DeviceRpcInput::CancelSelfLink {
                    expected_account_user_id,
                },
            )
            .await;
            return;
        }
        ConnectionLane::Control | ConnectionLane::Terminal { .. } => {}
    }

    let control_status = match hello.lane {
        ConnectionLane::Control => Some(command_sink.remote_command_status()),
        ConnectionLane::Terminal {
            session_id,
            capture,
        } => {
            let session_id = match SessionId::try_from(session_id.as_str()) {
                Ok(session_id) => session_id,
                Err(error) => {
                    drop(
                        framed_json::write_json(
                            &mut framed_writer,
                            &HostHelloResponse::reject(format!("invalid session id: {error}")),
                        )
                        .await,
                    );
                    return;
                }
            };

            let (surface, capability) = if capture {
                (TerminalSurface::HeadlessCapture, TerminalCapability::Write)
            } else {
                (TerminalSurface::Cli, TerminalCapability::ReadOnly)
            };
            let handle = match command_sink
                .subscribe_terminal_hub(session_id, surface, capability)
                .await
            {
                Ok(handle) => handle,
                Err(error) => {
                    drop(
                        framed_json::write_json(
                            &mut framed_writer,
                            &HostHelloResponse::reject(format!(
                                "terminal attach rejected: {error}"
                            )),
                        )
                        .await,
                    );
                    return;
                }
            };
            let negotiated = if handle.capability.can_write() {
                TerminalLaneCapability::Write
            } else {
                TerminalLaneCapability::ReadOnly
            };
            if framed_json::write_json(
                &mut framed_writer,
                &HostHelloResponse::accept_terminal(negotiated),
            )
            .await
            .is_err()
            {
                return;
            }
            tracing::debug!(
                %session_id,
                capability = ?negotiated,
                "headless host client accepted (terminal lane)",
            );
            handle_terminal_lane(
                framed_reader,
                framed_writer,
                session_id,
                command_sink,
                handle,
                shutdown,
            )
            .await;
            return;
        }
        ConnectionLane::DeviceRevoke { .. }
        | ConnectionLane::DeviceApproveLink { .. }
        | ConnectionLane::DeviceStartSelfLink { .. }
        | ConnectionLane::DeviceCancelSelfLink { .. } => return,
    };

    let Some(control_status) = control_status else {
        return;
    };
    let expected_account = crate::AccountCommandContext {
        user_id: control_status.account_user_id.clone(),
        epoch: control_status.account_epoch,
    };
    if framed_json::write_json(
        &mut framed_writer,
        &HostHelloResponse::accept_control(control_status),
    )
    .await
    .is_err()
    {
        return;
    }
    tracing::debug!("headless host client accepted (control lane)");

    let (private_tx, private_rx) = mpsc::channel::<ControlServerFrame>(8);
    let writer_cancel = CancellationToken::new();
    let mut account_status = command_sink.subscribe_remote_command_status();
    let writer_handle = tokio::spawn(write_events_to_client(
        framed_writer,
        events_rx,
        private_rx,
        writer_cancel.clone(),
        expected_account.clone(),
        account_status.clone(),
    ));

    loop {
        let client_frame = tokio::select! {
            biased;
            changed = account_status.changed() => {
                if changed.is_err()
                    || command_account_context(&account_status) != expected_account
                {
                    break;
                }
                continue;
            }
            () = shutdown.cancelled() => break,
            () = writer_cancel.cancelled() => break,
            frame = framed_json::read_frame(&mut framed_reader) => frame,
        };
        let bytes = match client_frame {
            Ok(Some(bytes)) => bytes,
            Ok(None) => break,
            Err(error) => {
                tracing::debug!(%error, "headless host control read error; closing connection");
                break;
            }
        };

        let frame: ControlClientFrame = match framed_json::parse_json(&bytes) {
            Ok(frame) => frame,
            Err(error) => {
                tracing::warn!(
                    %error,
                    bytes = bytes.len(),
                    "headless host received unparseable control frame; ignoring"
                );
                let event = HostEvent::System(SystemEvent::Error {
                    message: format!("could not parse host control frame: {error}"),
                    context: Some(command_error_snippet(&bytes)),
                });
                if !try_send_private_diag(&private_tx, event, "parse-error") {
                    break;
                }
                continue;
            }
        };
        let message = match frame {
            ControlClientFrame::Snapshot { refresh_id } => {
                let Ok(response) = command_sink.snapshot_rpc(expected_account.clone()).await else {
                    break;
                };
                let snapshot = ControlServerFrame::Snapshot {
                    refresh_id,
                    account_user_id: response.account_user_id,
                    account_epoch: response.account_epoch,
                    remote_operations_ready: response.remote_operations_ready,
                    auth: response.auth,
                    sessions: response.sessions,
                    rooms: response.rooms,
                };
                if private_tx.send(snapshot).await.is_err() {
                    break;
                }
                continue;
            }
            ControlClientFrame::Command { command } => {
                match serde_json::from_value::<HostCommand>(command) {
                    Ok(message) => message,
                    Err(error) => {
                        let event = HostEvent::System(SystemEvent::Error {
                            message: format!("could not parse host command: {error}"),
                            context: None,
                        });
                        if !try_send_private_diag(&private_tx, event, "command-parse-error") {
                            break;
                        }
                        continue;
                    }
                }
            }
        };

        tracing::trace!(
            message_type = message.message_type(),
            "headless host command received"
        );

        if matches!(message, HostCommand::Terminal(_)) {
            tracing::warn!(
                message_type = message.message_type(),
                "control-lane client attempted to send a terminal command; rejecting (must use terminal lane)",
            );
            let event = HostEvent::System(SystemEvent::Error {
                message: format!(
                    "terminal commands must be sent over a terminal lane (use ConnectionLane::Terminal); rejected '{}' on control lane",
                    message.message_type()
                ),
                context: None,
            });
            if !try_send_private_diag(&private_tx, event, "control-lane-terminal-reject") {
                break;
            }
            continue;
        }
        if command_sink
            .dispatch_host(message, expected_account.clone())
            .await
            .is_err()
        {
            break;
        }
    }

    drop(private_tx);
    writer_cancel.cancel();
    if let Err(error) = writer_handle.await {
        tracing::warn!(%error, "headless host writer join failed");
    }
}

fn command_error_snippet(bytes: &[u8]) -> String {
    const MAX: usize = 256;
    let text = String::from_utf8_lossy(bytes);
    if text.len() <= MAX {
        text.into_owned()
    } else {
        let mut truncated: String = text.chars().take(MAX).collect();
        truncated.push('…');
        truncated
    }
}

fn try_send_private_diag(
    private_tx: &mpsc::Sender<ControlServerFrame>,
    event: HostEvent,
    kind: &'static str,
) -> bool {
    match private_tx.try_send(ControlServerFrame::Event {
        event: Box::new(event),
    }) {
        Ok(()) => true,
        Err(mpsc::error::TrySendError::Full(_)) => {
            tracing::warn!(
                kind,
                "private diagnostic channel full; dropping frame for this client"
            );
            true
        }
        Err(mpsc::error::TrySendError::Closed(_)) => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingTerminalClose {
    reason: crate::terminal_transport::TerminalCloseReason,
    final_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TerminalControlOutcome {
    Continue,
    ContinueFromSequence(u64),
    PendingClose(PendingTerminalClose),
    Stop,
}

async fn handle_terminal_lane<R, W>(
    framed_reader: FramedJsonReader<R>,
    framed_writer: FramedJsonWriter<W>,
    session_id: SessionId,
    command_sink: RuntimeCommandSink,
    handle: SubscriberHandle,
    shutdown: CancellationToken,
) where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    handle_terminal_lane_inner(
        framed_reader,
        framed_writer,
        session_id,
        &command_sink,
        handle,
        shutdown,
    )
    .await;
}

#[expect(
    clippy::too_many_lines,
    reason = "one task owns socket input, control, data, replacement receivers, and the sequence gate"
)]
async fn handle_terminal_lane_inner<R, W>(
    mut framed_reader: FramedJsonReader<R>,
    mut framed_writer: FramedJsonWriter<W>,
    session_id: SessionId,
    command_sink: &RuntimeCommandSink,
    mut handle: SubscriberHandle,
    shutdown: CancellationToken,
) where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let capability = handle.capability;
    let surface = handle.surface;
    'connection: {
        let initial_control = tokio::select! {
            biased;
            control = handle.control_rx.recv() => control,
            () = shutdown.cancelled() => break 'connection,
        };
        let mut next_data_sequence = match initial_control {
            Some(control) => match forward_terminal_control(&mut framed_writer, control).await {
                TerminalControlOutcome::Continue => None,
                TerminalControlOutcome::ContinueFromSequence(next_sequence) => Some(next_sequence),
                TerminalControlOutcome::PendingClose(close) => {
                    if close.final_sequence == 0 {
                        drop(write_terminal_close_frame(&mut framed_writer, close).await);
                    } else {
                        tracing::error!(
                            %session_id,
                            final_sequence = close.final_sequence,
                            "headless terminal close boundary arrived before snapshot cursor"
                        );
                    }
                    break 'connection;
                }
                TerminalControlOutcome::Stop => break 'connection,
            },
            None => break 'connection,
        };
        let mut pending_close: Option<PendingTerminalClose> = None;
        let session_id_text = session_id.to_string();
        loop {
            let close_ready = pending_close
                .as_ref()
                .is_some_and(|close| next_data_sequence.unwrap_or(0) == close.final_sequence);
            if close_ready {
                if let Some(close) = pending_close.take() {
                    drop(write_terminal_close_frame(&mut framed_writer, close).await);
                }
                break;
            }
            if pending_close.is_none()
                && ((handle.control_rx.is_closed() && handle.control_rx.is_empty())
                    || (handle.data_rx.is_closed() && handle.data_rx.is_empty()))
            {
                let previous_connection_id = handle.connection_id;
                let Some(replacement) = crate::terminal_transport::resubscribe_terminal_hub(
                    command_sink,
                    session_id,
                    surface,
                    capability,
                    &shutdown,
                )
                .await
                else {
                    break;
                };
                handle = replacement;
                let _ = command_sink
                    .unsubscribe_terminal_hub(session_id, previous_connection_id)
                    .await;
                let initial_control = tokio::select! {
                    biased;
                    control = handle.control_rx.recv() => control,
                    () = shutdown.cancelled() => break,
                };
                next_data_sequence = match initial_control {
                    Some(control) => {
                        match forward_terminal_control(&mut framed_writer, control).await {
                            TerminalControlOutcome::Continue => None,
                            TerminalControlOutcome::ContinueFromSequence(next_sequence) => {
                                Some(next_sequence)
                            }
                            TerminalControlOutcome::PendingClose(close) => {
                                pending_close = Some(close);
                                next_data_sequence
                            }
                            TerminalControlOutcome::Stop => break,
                        }
                    }
                    None => continue,
                };
            }
            tokio::select! {


                biased;
                maybe_control = handle.control_rx.recv(), if pending_close.is_none() => {
                    let Some(control) = maybe_control else {
                        continue;
                    };
                    match forward_terminal_control(&mut framed_writer, control).await {
                        TerminalControlOutcome::Continue => {}
                        TerminalControlOutcome::ContinueFromSequence(next_sequence) => {
                            next_data_sequence = Some(next_sequence);
                        }
                        TerminalControlOutcome::PendingClose(close) => {
                            pending_close = Some(close);
                        }
                        TerminalControlOutcome::Stop => break,
                    }
                }
                maybe_frame = handle.data_rx.recv() => {
                    let Some(frame) = maybe_frame else {
                        if pending_close.is_some() {
                            tracing::error!(
                                %session_id,
                                expected = ?next_data_sequence,
                                "headless terminal data channel closed before close boundary"
                            );
                            break;
                        }
                        continue;
                    };
                    match classify_data_frame(frame.sequence, &mut next_data_sequence) {
                        TerminalDataSequenceDecision::Stale => continue,
                        TerminalDataSequenceDecision::Exact => {}
                        TerminalDataSequenceDecision::Gap { expected, actual } => {
                            tracing::error!(
                                %session_id,
                                expected,
                                actual,
                                "headless terminal sequence gap before close boundary"
                            );
                            break;
                        }
                        TerminalDataSequenceDecision::Exhausted => {
                            tracing::error!(
                                %session_id,
                                sequence = frame.sequence,
                                "headless terminal sequence exhausted"
                            );
                            break;
                        }
                    }
                    let message = TerminalLaneServerFrame::Data {
                        sequence: frame.sequence,
                        bytes: frame.bytes,
                    };
                    let encoded = match message.encode() {
                        Ok(encoded) => encoded,
                        Err(error) => {
                            tracing::error!(%error, %session_id, "failed to encode terminal data frame");
                            break;
                        }
                    };
                    if framed_json::write_frame(&mut framed_writer, encoded).await.is_err() {
                        break;
                    }
                }
                client_frame = framed_json::read_frame(&mut framed_reader) => {
                    let bytes = match client_frame {
                        Ok(Some(bytes)) => bytes,
                        Ok(None) => break,
                        Err(error) => {
                            tracing::debug!(%error, %session_id, "terminal lane client read failed");
                            break;
                        }
                    };
                    let message = match TerminalLaneClientFrame::decode(&bytes) {
                        Ok(message) => message,
                        Err(error) => {
                            tracing::warn!(
                                %error,
                                %session_id,
                                bytes = bytes.len(),
                                "terminal lane received unparseable client message; ignoring frame"
                            );
                            continue;
                        }
                    };
                    if !dispatch_terminal_lane_client_msg(
                        command_sink,
                        &session_id_text,
                        handle.runtime_incarnation_id,
                        handle.connection_id,
                        capability,
                        message,
                    )
                    .await
                    {
                        break;
                    }
                }
                () = shutdown.cancelled() => break,
            }
        }
    }
    match command_sink
        .unsubscribe_terminal_hub(session_id, handle.connection_id)
        .await
    {
        Ok(()) => {}
        Err(error) => {
            tracing::debug!(%error, %session_id, connection_id = %handle.connection_id, "terminal lane unsubscribe failed");
        }
    }
}

async fn forward_terminal_control(
    framed_writer: &mut FramedJsonWriter<impl AsyncWrite + Unpin>,
    control: TerminalControlFrame,
) -> TerminalControlOutcome {
    let message = match control {
        TerminalControlFrame::SemanticCheckpoint {
            checkpoint,
            next_sequence,
        } => {
            let message = TerminalLaneServerFrame::LocalCheckpoint {
                checkpoint,
                next_sequence,
            };
            let Ok(encoded) = message.encode() else {
                return TerminalControlOutcome::Stop;
            };
            if framed_json::write_frame(framed_writer, encoded)
                .await
                .is_err()
            {
                return TerminalControlOutcome::Stop;
            }
            return TerminalControlOutcome::ContinueFromSequence(next_sequence);
        }
        TerminalControlFrame::Resize {
            rows,
            cols,
            at_sequence,
        } => TerminalLaneServerFrame::Resize {
            rows,
            cols,
            at_sequence,
        },
        TerminalControlFrame::Closed {
            reason,
            final_sequence,
        } => {
            return TerminalControlOutcome::PendingClose(PendingTerminalClose {
                reason,
                final_sequence,
            });
        }
    };
    let Ok(encoded) = message.encode() else {
        return TerminalControlOutcome::Stop;
    };
    if framed_json::write_frame(framed_writer, encoded)
        .await
        .is_err()
    {
        TerminalControlOutcome::Stop
    } else {
        TerminalControlOutcome::Continue
    }
}

async fn write_terminal_close_frame(
    framed_writer: &mut FramedJsonWriter<impl AsyncWrite + Unpin>,
    close: PendingTerminalClose,
) -> Result<()> {
    let message = TerminalLaneServerFrame::Closed {
        reason: close.reason.to_string(),
        final_sequence: close.final_sequence,
    };
    framed_json::write_frame(framed_writer, message.encode()?).await
}

use crate::terminal_transport::{TerminalDataSequenceDecision, classify_data_frame};

async fn dispatch_terminal_lane_client_msg(
    command_sink: &RuntimeCommandSink,
    session_id: &str,
    runtime_incarnation_id: Option<Uuid>,
    connection_id: crate::terminal_transport::TerminalConnectionId,
    capability: TerminalCapability,
    message: TerminalLaneClientFrame,
) -> bool {
    if !capability.can_write() {
        let op = match &message {
            TerminalLaneClientFrame::Input { .. } => "input",
            TerminalLaneClientFrame::Resize { .. } => "resize",
        };
        tracing::debug!(
            %session_id,
            op,
            "terminal lane: dropping {op} from read-only client (likely a \
             terminal auto-response); lane stays open",
        );
        return true;
    }

    let Some(runtime_incarnation_id) = runtime_incarnation_id else {
        tracing::debug!(%session_id, %connection_id, "terminal lane lost runtime incarnation");
        return false;
    };
    let subscription_id = format!("headless:{connection_id}");
    let command = match message {
        TerminalLaneClientFrame::Input { bytes } => TerminalCommand::InputBytes {
            session_id: session_id.to_owned(),
            bytes: bytes.to_vec(),
            expected_runtime_incarnation_id: runtime_incarnation_id.to_string(),
            subscription_id: Some(subscription_id),
            subscription_generation: Some(1),
        },
        TerminalLaneClientFrame::Resize { rows, cols } => TerminalCommand::HeadlessResize {
            session_id: session_id.to_owned(),
            cols,
            rows,
            expected_runtime_incarnation_id: runtime_incarnation_id.to_string(),
            subscription_id,
            subscription_generation: 1,
        },
    };

    match command_sink.send_terminal(command).await {
        Ok(()) => true,
        Err(error) => {
            tracing::debug!(%error, %session_id, "terminal lane runtime dispatch failed");
            false
        }
    }
}

fn command_account_context(
    status: &tokio::sync::watch::Receiver<crate::RemoteCommandStatus>,
) -> crate::AccountCommandContext {
    let status = status.borrow();
    crate::AccountCommandContext {
        user_id: status.account_user_id.clone(),
        epoch: status.account_epoch,
    }
}

async fn write_control_frame_or_cancel<W: AsyncWrite + Unpin>(
    writer: &mut FramedJsonWriter<W>,
    message: &ControlServerFrame,
    cancellation: &CancellationToken,
    expected_account: &crate::AccountCommandContext,
    account_status: &mut tokio::sync::watch::Receiver<crate::RemoteCommandStatus>,
) -> bool {
    let write = framed_json::write_json(writer, message);
    tokio::pin!(write);
    loop {
        tokio::select! {
            biased;
            changed = account_status.changed() => {
                if changed.is_err()
                    || command_account_context(account_status) != *expected_account
                {
                    return false;
                }
            }
            () = cancellation.cancelled() => return false,
            result = &mut write => return result.is_ok(),
        }
    }
}

async fn write_events_to_client(
    mut writer: FramedJsonWriter<LocalWriteHalf>,
    mut events_rx: broadcast::Receiver<HostEvent>,
    mut private_rx: mpsc::Receiver<ControlServerFrame>,
    cancellation: CancellationToken,
    expected_account: crate::AccountCommandContext,
    mut account_status: tokio::sync::watch::Receiver<crate::RemoteCommandStatus>,
) {
    let mut private_open = true;
    loop {
        tokio::select! {
            biased;
            changed = account_status.changed() => {
                if changed.is_err()
                    || command_account_context(&account_status) != expected_account
                {
                    break;
                }
            }
            () = cancellation.cancelled() => break,
            private = private_rx.recv(), if private_open => {
                match private {
                    Some(message) => {
                        if !write_control_frame_or_cancel(
                            &mut writer,
                            &message,
                            &cancellation,
                            &expected_account,
                            &mut account_status,
                        )
                        .await
                        {
                            break;
                        }
                    }
                    None => {
                        private_open = false;
                    }
                }
            }
            recv_result = events_rx.recv() => {
                match recv_result {
                    Ok(message) => {
                        if !write_control_frame_or_cancel(
                            &mut writer,
                            &ControlServerFrame::Event {
                                event: Box::new(message),
                            },
                            &cancellation,
                            &expected_account,
                            &mut account_status,
                        )
                        .await
                        {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "headless host client lagged behind runtime events");
                        break;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
    cancellation.cancel();
}

#[cfg(test)]
mod tests {
    use super::{
        ControlServerFrame, client_lane_shutdown_token, handle_terminal_lane_inner,
        write_control_frame_or_cancel, write_events_to_client,
    };
    use crate::{
        RuntimeCommandSink, SystemEvent,
        headless_host::terminal_lane::{TerminalLaneClientFrame, TerminalLaneServerFrame},
        host_protocol::HostEvent,
        support::io::framed_json,
        terminal_transport::{
            RESUBSCRIBE_DELAYS, TerminalCapability, TerminalConnectionId, TerminalControlFrame,
            TerminalDataFrame, TerminalDataSequenceDecision, TerminalHubCommand, TerminalSurface,
            classify_data_frame, hub::SubscriberHandle,
        },
    };
    use bytes::Bytes;
    use futures_util::{SinkExt, StreamExt};
    use kodosi_domain::{
        ids::SessionId,
        terminal::{TerminalCheckpointV2, TerminalScreen, TerminalSize},
    };
    use tokio::{
        io,
        sync::{broadcast, mpsc},
    };
    use tokio_util::sync::CancellationToken;

    fn checkpoint() -> TerminalCheckpointV2 {
        TerminalCheckpointV2::new(
            TerminalSize::new(24, 80).expect("size"),
            TerminalScreen::Primary,
            b"checkpoint".to_vec(),
            0,
            0,
            false,
        )
        .expect("checkpoint")
    }

    #[test]
    fn runtime_shutdown_does_not_preempt_client_lane_drain() {
        let runtime_shutdown = CancellationToken::new();
        let client_shutdown = client_lane_shutdown_token();

        runtime_shutdown.cancel();
        assert!(!client_shutdown.is_cancelled());

        client_shutdown.cancel();
        assert!(client_shutdown.is_cancelled());
    }

    async fn next_terminal_frame<T: io::AsyncRead + io::AsyncWrite + Unpin>(
        stream: &mut framed_json::FramedJsonStream<T>,
    ) -> crate::Result<Option<TerminalLaneServerFrame>> {
        let Some(bytes) = stream
            .next()
            .await
            .transpose()
            .map_err(crate::AppError::Io)?
        else {
            return Ok(None);
        };
        TerminalLaneServerFrame::decode(&bytes).map(Some)
    }

    async fn send_terminal_frame<T: io::AsyncRead + io::AsyncWrite + Unpin>(
        stream: &mut framed_json::FramedJsonStream<T>,
        frame: TerminalLaneClientFrame,
    ) {
        stream
            .send(frame.encode().expect("encode terminal frame"))
            .await
            .expect("write terminal frame");
    }

    fn test_command_sink() -> (
        RuntimeCommandSink,
        mpsc::Receiver<crate::TerminalCommand>,
        mpsc::Receiver<TerminalHubCommand>,
    ) {
        let (terminal_tx, terminal_rx) = mpsc::channel(8);
        let (system_tx, _system_rx) = mpsc::channel(1);
        let (auth_tx, _auth_rx) = mpsc::channel(1);
        let (friends_tx, _friends_rx) = mpsc::channel(1);
        let (devices_tx, _devices_rx) = mpsc::channel(1);
        let (trust_tx, _trust_rx) = mpsc::channel(1);
        let (room_tx, _room_rx) = mpsc::channel(1);
        let (sessions_tx, _sessions_rx) = mpsc::channel(1);
        let (agent_intel_tx, _agent_intel_rx) = mpsc::channel(1);
        let (hub_tx, hub_rx) = mpsc::channel(8);
        let (_remote_status_tx, remote_status) =
            tokio::sync::watch::channel(crate::RemoteCommandStatus {
                account_user_id: None,
                account_epoch: 1,
                remote_operations_ready: true,
            });
        let (snapshot_rpc, _snapshot_rpc_rx) = mpsc::channel(1);
        let (device_rpc, _device_rpc_rx) = mpsc::channel(1);
        (
            RuntimeCommandSink {
                terminal: terminal_tx,
                system: system_tx,
                auth: auth_tx,
                friends: friends_tx,
                devices: devices_tx,
                trust: trust_tx,
                room: room_tx,
                sessions: sessions_tx,
                agent_intel: agent_intel_tx,
                hub_subscribe: hub_tx,
                remote_status,
                snapshot_rpc,
                device_rpc,
            },
            terminal_rx,
            hub_rx,
        )
    }

    #[test]
    fn data_filter_drops_stale_frames_and_rejects_gaps() {
        let mut next_data_sequence = Some(3);

        assert_eq!(
            classify_data_frame(1, &mut next_data_sequence),
            TerminalDataSequenceDecision::Stale
        );
        assert_eq!(
            classify_data_frame(2, &mut next_data_sequence),
            TerminalDataSequenceDecision::Stale
        );
        assert_eq!(
            classify_data_frame(3, &mut next_data_sequence),
            TerminalDataSequenceDecision::Exact
        );
        assert_eq!(next_data_sequence, Some(4));
        assert_eq!(
            classify_data_frame(5, &mut next_data_sequence),
            TerminalDataSequenceDecision::Gap {
                expected: 4,
                actual: 5,
            }
        );
    }

    #[test]
    fn data_filter_arms_from_first_frame_without_snapshot_boundary() {
        let mut next_data_sequence = None;

        assert_eq!(
            classify_data_frame(7, &mut next_data_sequence),
            TerminalDataSequenceDecision::Exact
        );
        assert_eq!(next_data_sequence, Some(8));
    }

    #[tokio::test]
    async fn terminal_resize_is_forwarded_at_its_exclusive_data_boundary() {
        let (sink, _terminal_rx, mut hub_rx) = test_command_sink();
        let session_id = SessionId::new();
        let connection_id = TerminalConnectionId::new();
        let (data_tx, data_rx) = mpsc::channel(4);
        let (control_tx, control_rx) = mpsc::channel(4);
        control_tx
            .send(TerminalControlFrame::SemanticCheckpoint {
                checkpoint: checkpoint(),
                next_sequence: 7,
            })
            .await
            .expect("snapshot");
        data_tx
            .send(TerminalDataFrame::new(7, Bytes::from_static(b"before")))
            .await
            .expect("pre-resize data");
        control_tx
            .send(TerminalControlFrame::Resize {
                rows: 40,
                cols: 120,
                at_sequence: 8,
            })
            .await
            .expect("resize");
        data_tx
            .send(TerminalDataFrame::new(8, Bytes::from_static(b"after")))
            .await
            .expect("post-resize data");
        control_tx
            .send(TerminalControlFrame::Closed {
                reason: crate::terminal_transport::TerminalCloseReason::SessionEnded,
                final_sequence: 9,
            })
            .await
            .expect("close");
        drop((data_tx, control_tx));

        let (server, client) = io::duplex(64 * 1024);
        let (server_read, server_write) = io::split(server);
        let lane_sink = sink.clone();
        let lane = tokio::spawn(async move {
            handle_terminal_lane_inner(
                framed_json::reader(server_read),
                framed_json::writer(server_write),
                session_id,
                &lane_sink,
                SubscriberHandle {
                    connection_id,
                    runtime_incarnation_id: Some(uuid::Uuid::nil()),
                    surface: TerminalSurface::Cli,
                    capability: TerminalCapability::ReadOnly,
                    data_rx,
                    control_rx,
                },
                CancellationToken::new(),
            )
            .await;
        });
        let mut client = framed_json::framed(client);

        assert!(matches!(
            next_terminal_frame(&mut client)
                .await
                .expect("snapshot read")
                .expect("snapshot"),
            TerminalLaneServerFrame::LocalCheckpoint {
                next_sequence: 7,
                ..
            }
        ));
        let first = next_terminal_frame(&mut client)
            .await
            .expect("first post-snapshot read")
            .expect("first post-snapshot frame");
        let second = next_terminal_frame(&mut client)
            .await
            .expect("second post-snapshot read")
            .expect("second post-snapshot frame");
        assert!(
            matches!(first, TerminalLaneServerFrame::Data { sequence: 7, .. })
                || matches!(second, TerminalLaneServerFrame::Data { sequence: 7, .. })
        );
        assert!(
            matches!(
                first,
                TerminalLaneServerFrame::Resize {
                    rows: 40,
                    cols: 120,
                    at_sequence: 8
                }
            ) || matches!(
                second,
                TerminalLaneServerFrame::Resize {
                    rows: 40,
                    cols: 120,
                    at_sequence: 8
                }
            )
        );
        assert!(matches!(
            next_terminal_frame(&mut client)
                .await
                .expect("data 8 read")
                .expect("data 8"),
            TerminalLaneServerFrame::Data { sequence: 8, .. }
        ));
        assert!(matches!(
            next_terminal_frame(&mut client)
                .await
                .expect("close read")
                .expect("close"),
            TerminalLaneServerFrame::Closed {
                final_sequence: 9,
                ..
            }
        ));

        let cleanup = hub_rx.recv().await.expect("cleanup");
        let TerminalHubCommand::Unsubscribe(cleanup) = cleanup else {
            panic!("terminal lane must unsubscribe");
        };
        cleanup.reply.send(()).expect("cleanup reply");
        lane.await.expect("lane joins");
    }

    #[tokio::test]
    async fn terminal_close_waits_for_every_prequeued_data_frame() {
        let (sink, _terminal_rx, mut hub_rx) = test_command_sink();
        let session_id = SessionId::new();
        let connection_id = TerminalConnectionId::new();
        let (data_tx, data_rx) = mpsc::channel(4);
        let (control_tx, control_rx) = mpsc::channel(4);
        control_tx
            .send(TerminalControlFrame::SemanticCheckpoint {
                checkpoint: checkpoint(),
                next_sequence: 7,
            })
            .await
            .expect("snapshot");
        data_tx
            .send(TerminalDataFrame::new(7, Bytes::from_static(b"a")))
            .await
            .expect("data 7");
        data_tx
            .send(TerminalDataFrame::new(8, Bytes::from_static(b"b")))
            .await
            .expect("data 8");
        control_tx
            .send(TerminalControlFrame::Closed {
                reason: crate::terminal_transport::TerminalCloseReason::SessionEnded,
                final_sequence: 9,
            })
            .await
            .expect("close");
        drop((data_tx, control_tx));

        let (server, client) = io::duplex(64 * 1024);
        let (server_read, server_write) = io::split(server);
        let lane_sink = sink.clone();
        let lane = tokio::spawn(async move {
            handle_terminal_lane_inner(
                framed_json::reader(server_read),
                framed_json::writer(server_write),
                session_id,
                &lane_sink,
                SubscriberHandle {
                    connection_id,
                    runtime_incarnation_id: Some(uuid::Uuid::nil()),
                    surface: TerminalSurface::Cli,
                    capability: TerminalCapability::Write,
                    data_rx,
                    control_rx,
                },
                CancellationToken::new(),
            )
            .await;
        });
        let mut client = framed_json::framed(client);

        let snapshot = next_terminal_frame(&mut client)
            .await
            .expect("snapshot read")
            .expect("snapshot frame");
        let data_7 = next_terminal_frame(&mut client)
            .await
            .expect("data 7 read")
            .expect("data 7 frame");
        let data_8 = next_terminal_frame(&mut client)
            .await
            .expect("data 8 read")
            .expect("data 8 frame");
        let close = next_terminal_frame(&mut client)
            .await
            .expect("close read")
            .expect("close frame");

        assert!(matches!(
            snapshot,
            TerminalLaneServerFrame::LocalCheckpoint {
                next_sequence: 7,
                ..
            }
        ));
        assert!(matches!(
            data_7,
            TerminalLaneServerFrame::Data { sequence: 7, .. }
        ));
        assert!(matches!(
            data_8,
            TerminalLaneServerFrame::Data { sequence: 8, .. }
        ));
        assert!(matches!(
            close,
            TerminalLaneServerFrame::Closed {
                final_sequence: 9,
                ..
            }
        ));
        let cleanup = hub_rx.recv().await.expect("connection cleanup");
        let TerminalHubCommand::Unsubscribe(cleanup) = cleanup else {
            panic!("terminal lane must unsubscribe");
        };
        assert_eq!(cleanup.connection_id, connection_id);
        cleanup.reply.send(()).expect("cleanup reply");
        lane.await.expect("lane joins");
        assert!(
            next_terminal_frame(&mut client)
                .await
                .expect("EOF read")
                .is_none()
        );
    }

    #[tokio::test]
    async fn terminal_close_at_snapshot_boundary_needs_no_data_frame() {
        let (sink, _terminal_rx, mut hub_rx) = test_command_sink();
        let session_id = SessionId::new();
        let connection_id = TerminalConnectionId::new();
        let (_data_tx, data_rx) = mpsc::channel(1);
        let (control_tx, control_rx) = mpsc::channel(2);
        control_tx
            .send(TerminalControlFrame::SemanticCheckpoint {
                checkpoint: checkpoint(),
                next_sequence: 7,
            })
            .await
            .expect("snapshot");
        control_tx
            .send(TerminalControlFrame::Closed {
                reason: crate::terminal_transport::TerminalCloseReason::SessionEnded,
                final_sequence: 7,
            })
            .await
            .expect("close");

        let (server, client) = io::duplex(64 * 1024);
        let (server_read, server_write) = io::split(server);
        let lane_sink = sink.clone();
        let lane = tokio::spawn(async move {
            handle_terminal_lane_inner(
                framed_json::reader(server_read),
                framed_json::writer(server_write),
                session_id,
                &lane_sink,
                SubscriberHandle {
                    connection_id,
                    runtime_incarnation_id: Some(uuid::Uuid::nil()),
                    surface: TerminalSurface::Cli,
                    capability: TerminalCapability::ReadOnly,
                    data_rx,
                    control_rx,
                },
                CancellationToken::new(),
            )
            .await;
        });
        let mut client = framed_json::framed(client);
        assert!(matches!(
            next_terminal_frame(&mut client)
                .await
                .expect("snapshot read")
                .expect("snapshot"),
            TerminalLaneServerFrame::LocalCheckpoint {
                next_sequence: 7,
                ..
            }
        ));
        assert!(matches!(
            next_terminal_frame(&mut client)
                .await
                .expect("close read")
                .expect("close"),
            TerminalLaneServerFrame::Closed {
                final_sequence: 7,
                ..
            }
        ));
        let cleanup = hub_rx.recv().await.expect("cleanup");
        let TerminalHubCommand::Unsubscribe(cleanup) = cleanup else {
            panic!("terminal lane must unsubscribe");
        };
        cleanup.reply.send(()).expect("cleanup reply");
        lane.await.expect("lane joins");
    }

    #[tokio::test]
    async fn initial_exact_close_beats_simultaneous_shutdown() {
        let (sink, _terminal_rx, mut hub_rx) = test_command_sink();
        let session_id = SessionId::new();
        let connection_id = TerminalConnectionId::new();
        let (_data_tx, data_rx) = mpsc::channel(1);
        let (control_tx, control_rx) = mpsc::channel(1);
        control_tx
            .send(TerminalControlFrame::Closed {
                reason: crate::terminal_transport::TerminalCloseReason::SessionEnded,
                final_sequence: 0,
            })
            .await
            .expect("close");
        let shutdown = CancellationToken::new();
        shutdown.cancel();

        let (server, client) = io::duplex(64 * 1024);
        let (server_read, server_write) = io::split(server);
        let lane_sink = sink.clone();
        let lane = tokio::spawn(async move {
            handle_terminal_lane_inner(
                framed_json::reader(server_read),
                framed_json::writer(server_write),
                session_id,
                &lane_sink,
                SubscriberHandle {
                    runtime_incarnation_id: Some(uuid::Uuid::nil()),
                    connection_id,
                    surface: TerminalSurface::Cli,
                    capability: TerminalCapability::Write,
                    data_rx,
                    control_rx,
                },
                shutdown,
            )
            .await;
        });
        let mut client = framed_json::framed(client);

        assert!(matches!(
            next_terminal_frame(&mut client)
                .await
                .expect("close read")
                .expect("close frame"),
            TerminalLaneServerFrame::Closed {
                final_sequence: 0,
                ..
            }
        ));
        let cleanup = hub_rx.recv().await.expect("cleanup");
        let TerminalHubCommand::Unsubscribe(cleanup) = cleanup else {
            panic!("terminal lane must unsubscribe");
        };
        cleanup.reply.send(()).expect("cleanup reply");
        lane.await.expect("lane joins");
    }

    #[tokio::test(start_paused = true)]
    async fn closed_hub_channel_resubscribes_repaints_and_keeps_input_alive() {
        let (terminal_tx, mut terminal_rx) = mpsc::channel(8);
        let (system_tx, _system_rx) = mpsc::channel(1);
        let (auth_tx, _auth_rx) = mpsc::channel(1);
        let (friends_tx, _friends_rx) = mpsc::channel(1);
        let (devices_tx, _devices_rx) = mpsc::channel(1);
        let (trust_tx, _trust_rx) = mpsc::channel(1);
        let (room_tx, _room_rx) = mpsc::channel(1);
        let (sessions_tx, _sessions_rx) = mpsc::channel(1);
        let (agent_intel_tx, _agent_intel_rx) = mpsc::channel(1);
        let (hub_tx, mut hub_rx) = mpsc::channel(8);
        let (_remote_status_tx, remote_status) =
            tokio::sync::watch::channel(crate::RemoteCommandStatus {
                account_user_id: None,
                account_epoch: 1,
                remote_operations_ready: true,
            });
        let (snapshot_rpc, _snapshot_rpc_rx) = mpsc::channel(1);
        let (device_rpc, _device_rpc_rx) = mpsc::channel(1);
        let sink = RuntimeCommandSink {
            terminal: terminal_tx,
            system: system_tx,
            auth: auth_tx,
            friends: friends_tx,
            devices: devices_tx,
            trust: trust_tx,
            room: room_tx,
            sessions: sessions_tx,
            agent_intel: agent_intel_tx,
            hub_subscribe: hub_tx,
            remote_status,
            snapshot_rpc,
            device_rpc,
        };

        let session_id = SessionId::new();
        let initial_connection = TerminalConnectionId::new();
        let (initial_data_tx, initial_data_rx) = mpsc::channel(4);
        let (initial_control_tx, initial_control_rx) = mpsc::channel(4);
        initial_control_tx
            .send(TerminalControlFrame::SemanticCheckpoint {
                checkpoint: checkpoint(),
                next_sequence: 9,
            })
            .await
            .expect("initial snapshot queues");
        drop(initial_control_tx);

        let (server, client) = io::duplex(64 * 1024);
        let (server_read, server_write) = io::split(server);
        let shutdown = CancellationToken::new();
        let lane_sink = sink.clone();
        let lane_shutdown = shutdown.clone();
        let lane = tokio::spawn(async move {
            handle_terminal_lane_inner(
                framed_json::reader(server_read),
                framed_json::writer(server_write),
                session_id,
                &lane_sink,
                SubscriberHandle {
                    runtime_incarnation_id: Some(uuid::Uuid::nil()),
                    connection_id: initial_connection,
                    surface: TerminalSurface::Cli,
                    capability: TerminalCapability::Write,
                    data_rx: initial_data_rx,
                    control_rx: initial_control_rx,
                },
                lane_shutdown,
            )
            .await;
        });
        let mut client = framed_json::framed(client);

        let initial = next_terminal_frame(&mut client)
            .await
            .expect("read initial snapshot")
            .expect("initial snapshot exists");
        assert!(matches!(
            initial,
            TerminalLaneServerFrame::LocalCheckpoint {
                next_sequence: 9,
                ..
            }
        ));

        tokio::task::yield_now().await;
        assert!(hub_rx.try_recv().is_err(), "recovery waits before retrying");
        tokio::time::advance(RESUBSCRIBE_DELAYS[0]).await;
        let first = hub_rx.recv().await.expect("first retry");
        let TerminalHubCommand::Subscribe(first) = first else {
            panic!("first recovery command must subscribe");
        };
        assert_eq!(first.session_id, session_id);
        assert_eq!(first.surface, TerminalSurface::Cli);
        assert_eq!(first.capability, TerminalCapability::Write);
        assert!(first.reply.send(None).is_ok(), "first retry reply receiver");

        tokio::task::yield_now().await;
        tokio::time::advance(RESUBSCRIBE_DELAYS[1]).await;
        let second = hub_rx.recv().await.expect("second retry");
        let TerminalHubCommand::Subscribe(second) = second else {
            panic!("second recovery command must subscribe");
        };
        let replacement_connection = TerminalConnectionId::new();
        let (replacement_data_tx, replacement_data_rx) = mpsc::channel(4);
        let (replacement_control_tx, replacement_control_rx) = mpsc::channel(4);
        let replacement_checkpoint = TerminalCheckpointV2::new(
            TerminalSize::new(30, 100).expect("replacement size"),
            TerminalScreen::Alternate,
            b"replacement-checkpoint".to_vec(),
            0,
            0,
            false,
        )
        .expect("replacement checkpoint");
        replacement_control_tx
            .send(TerminalControlFrame::SemanticCheckpoint {
                checkpoint: replacement_checkpoint,
                next_sequence: 3,
            })
            .await
            .expect("replacement snapshot queues");
        replacement_data_tx
            .send(TerminalDataFrame::new(2, Bytes::from_static(b"stale")))
            .await
            .expect("stale data queues");
        replacement_data_tx
            .send(TerminalDataFrame::new(3, Bytes::from_static(b"fresh")))
            .await
            .expect("fresh data queues");
        assert!(
            second
                .reply
                .send(Some(SubscriberHandle {
                    runtime_incarnation_id: Some(uuid::Uuid::nil()),
                    connection_id: replacement_connection,
                    surface: TerminalSurface::Cli,
                    capability: TerminalCapability::Write,
                    data_rx: replacement_data_rx,
                    control_rx: replacement_control_rx,
                }))
                .is_ok(),
            "replacement reply receiver"
        );

        let old_cleanup = hub_rx.recv().await.expect("old connection cleanup");
        let TerminalHubCommand::Unsubscribe(old_cleanup) = old_cleanup else {
            panic!("old connection must unsubscribe");
        };
        assert_eq!(old_cleanup.connection_id, initial_connection);
        old_cleanup.reply.send(()).expect("old cleanup replies");

        let replacement = next_terminal_frame(&mut client)
            .await
            .expect("read replacement snapshot")
            .expect("replacement snapshot exists");
        assert!(matches!(
            replacement,
            TerminalLaneServerFrame::LocalCheckpoint {
                next_sequence: 3,
                ..
            }
        ));
        let data = next_terminal_frame(&mut client)
            .await
            .expect("read replacement data")
            .expect("replacement data exists");
        assert!(matches!(
            data,
            TerminalLaneServerFrame::Data { sequence: 3, bytes }
                if bytes.as_ref() == b"fresh"
        ));

        send_terminal_frame(
            &mut client,
            TerminalLaneClientFrame::Input {
                bytes: Bytes::from_static(b"input survives"),
            },
        )
        .await;
        assert!(matches!(
            terminal_rx.recv().await,
            Some(crate::TerminalCommand::InputBytes {
                session_id: actual,
                bytes,
                expected_runtime_incarnation_id: incarnation,
                subscription_id: Some(subscription),
                subscription_generation: Some(1),
            }) if actual == session_id.to_string()
                && bytes == b"input survives"
                && incarnation == uuid::Uuid::nil().to_string()
                && subscription == format!("headless:{replacement_connection}")
        ));

        drop(client);
        let final_cleanup = hub_rx.recv().await.expect("replacement cleanup");
        let TerminalHubCommand::Unsubscribe(final_cleanup) = final_cleanup else {
            panic!("replacement connection must unsubscribe");
        };
        assert_eq!(final_cleanup.connection_id, replacement_connection);
        final_cleanup
            .reply
            .send(())
            .expect("replacement cleanup replies");
        lane.await.expect("terminal lane joins");

        drop((initial_data_tx, replacement_data_tx, replacement_control_tx));
        assert!(!shutdown.is_cancelled());
    }

    #[tokio::test]
    async fn account_change_interrupts_a_backpressured_control_write() {
        let (server, _client) = io::duplex(64);
        let (_read_half, write_half) = io::split(server);
        let mut writer = framed_json::writer(write_half);
        let cancellation = CancellationToken::new();
        let expected_account = crate::AccountCommandContext {
            user_id: Some("account-a".to_owned()),
            epoch: 1,
        };
        let (status_tx, mut status_rx) = tokio::sync::watch::channel(crate::RemoteCommandStatus {
            account_user_id: expected_account.user_id.clone(),
            account_epoch: expected_account.epoch,
            remote_operations_ready: true,
        });
        let message = ControlServerFrame::Event {
            event: Box::new(HostEvent::System(SystemEvent::Error {
                message: "x".repeat(4096),
                context: None,
            })),
        };

        let write = write_control_frame_or_cancel(
            &mut writer,
            &message,
            &cancellation,
            &expected_account,
            &mut status_rx,
        );
        tokio::pin!(write);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), &mut write)
                .await
                .is_err()
        );
        status_tx
            .send(crate::RemoteCommandStatus {
                account_user_id: Some("account-b".to_owned()),
                account_epoch: 2,
                remote_operations_ready: true,
            })
            .expect("advance account");
        assert!(
            !tokio::time::timeout(std::time::Duration::from_secs(1), write)
                .await
                .expect("account crossover cancels blocked write")
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn private_snapshot_precedes_ready_broadcast() {
        let (events_tx, events_rx) = broadcast::channel(4);
        events_tx
            .send(HostEvent::System(SystemEvent::Heartbeat))
            .expect("queue broadcast");
        let (private_tx, private_rx) = mpsc::channel(1);
        let refresh_id = uuid::Uuid::now_v7();
        private_tx
            .send(ControlServerFrame::Snapshot {
                refresh_id,
                account_user_id: None,
                account_epoch: 1,
                remote_operations_ready: true,
                auth: crate::AuthEvent::Ready {
                    user_id: None,
                    account_epoch: 1,
                },
                sessions: Vec::new(),
                rooms: Vec::new(),
            })
            .await
            .expect("queue private snapshot");

        let (server, client) = tokio::net::UnixStream::pair().expect("socket pair");
        let (_read_half, write_half) = server.into_split();
        let writer_cancel = CancellationToken::new();
        let expected_account = crate::AccountCommandContext {
            user_id: None,
            epoch: 1,
        };
        let (_status_tx, status_rx) = tokio::sync::watch::channel(crate::RemoteCommandStatus {
            account_user_id: None,
            account_epoch: 1,
            remote_operations_ready: true,
        });
        let writer_task = tokio::spawn(write_events_to_client(
            framed_json::writer(write_half),
            events_rx,
            private_rx,
            writer_cancel.clone(),
            expected_account,
            status_rx,
        ));
        let (client_read, _client_write) = client.into_split();
        let mut reader = framed_json::reader(client_read);
        let first: ControlServerFrame = framed_json::read_json(&mut reader)
            .await
            .expect("read first frame")
            .expect("first frame exists");
        assert!(matches!(
            first,
            ControlServerFrame::Snapshot { refresh_id: actual, .. } if actual == refresh_id
        ));
        let second: ControlServerFrame = framed_json::read_json(&mut reader)
            .await
            .expect("read second frame")
            .expect("second frame exists");
        assert!(matches!(second, ControlServerFrame::Event { .. }));

        writer_cancel.cancel();
        writer_task.await.expect("writer joins");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn account_change_closes_writer_before_later_events() {
        let (events_tx, events_rx) = broadcast::channel(4);
        let (server, client) = tokio::net::UnixStream::pair().expect("socket pair");
        let (_read_half, write_half) = server.into_split();
        let writer = framed_json::writer(write_half);
        let (_private_tx, private_rx) = mpsc::channel(1);
        let expected_account = crate::AccountCommandContext {
            user_id: Some("account-a".to_owned()),
            epoch: 1,
        };
        let (status_tx, status_rx) = tokio::sync::watch::channel(crate::RemoteCommandStatus {
            account_user_id: expected_account.user_id.clone(),
            account_epoch: expected_account.epoch,
            remote_operations_ready: true,
        });
        let writer_cancel = CancellationToken::new();
        let writer_task = tokio::spawn(write_events_to_client(
            writer,
            events_rx,
            private_rx,
            writer_cancel.clone(),
            expected_account,
            status_rx,
        ));
        tokio::task::yield_now().await;

        status_tx
            .send(crate::RemoteCommandStatus {
                account_user_id: Some("account-b".to_owned()),
                account_epoch: 2,
                remote_operations_ready: true,
            })
            .expect("advance account");
        drop(events_tx.send(HostEvent::System(SystemEvent::Heartbeat)));
        tokio::time::timeout(std::time::Duration::from_secs(1), writer_task)
            .await
            .expect("writer closes on account change")
            .expect("writer task joins");
        assert!(writer_cancel.is_cancelled());

        let (client_read, _client_write) = client.into_split();
        let mut client_reader = framed_json::reader(client_read);
        assert!(
            framed_json::read_frame(&mut client_reader)
                .await
                .expect("closed stream")
                .is_none(),
            "no post-crossover event may reach the retired account connection"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn lagged_event_client_is_disconnected_for_resnapshot() {
        let (events_tx, events_rx) = broadcast::channel(1);
        let event = HostEvent::System(SystemEvent::Heartbeat);
        events_tx.send(event.clone()).expect("first event");
        events_tx.send(event).expect("second event");

        let (server, _client) = tokio::net::UnixStream::pair().expect("socket pair");
        let (_read_half, write_half) = server.into_split();
        let writer = framed_json::writer(write_half);
        let (_private_tx, private_rx) = mpsc::channel(1);

        let writer_cancel = CancellationToken::new();
        let expected_account = crate::AccountCommandContext {
            user_id: None,
            epoch: 1,
        };
        let (_status_tx, status_rx) = tokio::sync::watch::channel(crate::RemoteCommandStatus {
            account_user_id: None,
            account_epoch: 1,
            remote_operations_ready: true,
        });
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            write_events_to_client(
                writer,
                events_rx,
                private_rx,
                writer_cancel.clone(),
                expected_account,
                status_rx,
            ),
        )
        .await
        .expect("lagged writer should exit instead of continuing with a gap");
        assert!(writer_cancel.is_cancelled());
    }
}
