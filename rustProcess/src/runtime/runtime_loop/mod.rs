mod catalog;
mod handlers;
mod heartbeat;
mod publish;
mod scoped_events;

pub(crate) use heartbeat::heartbeat_loop;

use std::time::{Duration, Instant};

use tokio::{
    sync::mpsc,
    time::{self, MissedTickBehavior},
};
use tokio_util::sync::CancellationToken;

const IDLE_MAINTENANCE_INTERVAL: Duration = Duration::from_secs(1);

const MAINTENANCE_ACTIVE_WINDOW: Duration = Duration::from_secs(2);

fn next_maintenance_interval(
    since_activity: Duration,
    active: Duration,
    idle: Duration,
) -> Duration {
    if since_activity < MAINTENANCE_ACTIVE_WINDOW {
        active
    } else {
        idle
    }
}

use crate::{
    AuthCommand, AuthEvent, DeviceCommand, FriendsCommand, Result, RoomCommand, SessionCommand,
    SystemCommand, SystemEvent, TerminalCommand, TrustCommand,
    local_sessions::runtime_registry::LocalInputRouter,
    runtime::{self, session_catalog},
    runtime_event_bus::RuntimeEventSender,
    terminal_transport::{self, TerminalCloseReason, TerminalControlFrame},
};

use self::{
    catalog::publish_state_snapshot,
    handlers::{
        apply_devices_message, apply_friends_message, apply_room_message_boxed,
        apply_session_message_boxed, apply_system_message, apply_terminal_message,
        apply_trust_message,
    },
    publish::{
        RuntimeFlushOptions, flush_runtime_outputs, publish_auth_state,
        publish_recovered_access_mutations, publish_recovered_room_mutations,
    },
};

#[cfg(all(test, feature = "cli"))]
pub(crate) use handlers::dispatch_room_message_for_test;
#[cfg(test)]
pub(crate) use handlers::{apply_auth_message, apply_room_message, apply_session_message};
pub(crate) use handlers::{push_invitations, push_room_snapshot};

pub(crate) struct RuntimeLaneReceivers {
    pub(crate) terminal: mpsc::Receiver<TerminalCommand>,
    pub(crate) system: mpsc::Receiver<crate::AccountScopedCommand<SystemCommand>>,
    pub(crate) auth: mpsc::Receiver<crate::AccountScopedCommand<AuthCommand>>,
    pub(crate) friends: mpsc::Receiver<crate::AccountScopedCommand<FriendsCommand>>,
    pub(crate) devices: mpsc::Receiver<crate::AccountScopedCommand<DeviceCommand>>,
    pub(crate) trust: mpsc::Receiver<crate::AccountScopedCommand<TrustCommand>>,
    pub(crate) room: mpsc::Receiver<crate::AccountScopedCommand<RoomCommand>>,
    pub(crate) sessions: mpsc::Receiver<crate::AccountScopedCommand<SessionCommand>>,
    pub(crate) agent_intel: mpsc::Receiver<crate::AccountScopedCommand<crate::AgentIntelCommand>>,
    pub(crate) hub_subscribe: mpsc::Receiver<terminal_transport::TerminalHubCommand>,
    pub(crate) remote_status: tokio::sync::watch::Sender<crate::RemoteCommandStatus>,
    pub(crate) headless: HeadlessRuntimeLaneReceivers,
}

#[cfg(feature = "cli")]
pub(crate) struct HeadlessRuntimeLaneReceivers {
    pub(crate) snapshot_rpc: mpsc::Receiver<crate::SnapshotRpcRequest>,
    pub(crate) device_rpc: mpsc::Receiver<crate::DeviceRpcRequest>,
}

#[cfg(not(feature = "cli"))]
#[derive(Debug, Clone, Copy)]
pub(crate) struct HeadlessRuntimeLaneReceivers;

#[cfg(feature = "cli")]
enum HeadlessRuntimeRequest {
    Snapshot(crate::SnapshotRpcRequest),
    Device(crate::DeviceRpcRequest),
    Closed,
}

#[cfg(not(feature = "cli"))]
enum HeadlessRuntimeRequest {}

impl HeadlessRuntimeLaneReceivers {
    #[cfg(feature = "cli")]
    async fn recv(&mut self) -> HeadlessRuntimeRequest {
        tokio::select! {
            request = self.snapshot_rpc.recv() => request.map_or(
                HeadlessRuntimeRequest::Closed,
                HeadlessRuntimeRequest::Snapshot,
            ),
            request = self.device_rpc.recv() => request.map_or(
                HeadlessRuntimeRequest::Closed,
                HeadlessRuntimeRequest::Device,
            ),
        }
    }

    #[cfg(not(feature = "cli"))]
    async fn recv(&self) -> HeadlessRuntimeRequest {
        std::future::pending().await
    }
}

fn publish_remote_status(
    app: &runtime::Runtime,
    remote_status: &tokio::sync::watch::Sender<crate::RemoteCommandStatus>,
) {
    let (account_user_id, account_epoch) = app.state.identity.event_context();
    remote_status.send_if_modified(|status| {
        let current = crate::RemoteCommandStatus {
            account_user_id,
            account_epoch,
            #[cfg(feature = "cli")]
            remote_operations_ready: app.remote_surfaces_ready(),
        };
        if *status == current {
            false
        } else {
            *status = current;
            true
        }
    });
}

fn accepts_account_command<T>(
    app: &runtime::Runtime,
    envelope: crate::AccountScopedCommand<T>,
) -> Option<T> {
    if let Some(expected) = envelope.expected {
        let (user_id, epoch) = app.state.identity.event_context();
        if expected.user_id != user_id || expected.epoch != epoch {
            tracing::debug!(
                expected_user_id = ?expected.user_id,
                expected_epoch = expected.epoch,
                current_user_id = ?user_id,
                current_epoch = epoch,
                "dropping command admitted for a stale account context"
            );
            return None;
        }
    }
    Some(envelope.command)
}

#[allow(
    clippy::too_many_lines,
    reason = "single tokio::select! loop; arms cannot be hoisted without losing cancellation safety"
)]
pub(crate) async fn runtime_loop(
    mut app: runtime::Runtime,
    mut commands: RuntimeLaneReceivers,
    runtime_event_tx: RuntimeEventSender,
    cancellation: CancellationToken,
) -> Result<()> {
    tracing::debug!("runtime loop started");
    let active_interval = app.tick_interval();
    let idle_interval = active_interval.max(IDLE_MAINTENANCE_INTERVAL);
    let mut last_activity = Instant::now();
    let mut tick = time::interval(active_interval);
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut last_signal: Option<session_catalog::RefreshFingerprint> = None;
    let mut last_auth_event: Option<AuthEvent> = None;
    drop(
        crate::agent_intel::global_refresh::spawn_probed_global_status_refresh(
            runtime_event_tx.clone(),
            None,
            None,
        ),
    );
    let auth_state_published =
        publish_auth_state(&app, &runtime_event_tx, &mut last_auth_event, true).await?;
    if auth_state_published {
        publish_recovered_room_mutations(&app, &runtime_event_tx).await?;
        publish_recovered_access_mutations(&mut app, &runtime_event_tx).await?;
    }
    publish_state_snapshot(&app, &runtime_event_tx, &mut last_signal, true, true).await?;
    let local_input_router = app.state.local.owned_session_runtimes.input_router();
    let terminal_dispatch_cancel = cancellation.child_token();
    let (terminal_control_tx, mut terminal_control_rx) = mpsc::channel(64);
    let terminal_dispatch = tokio::spawn(run_terminal_dispatch(
        commands.terminal,
        terminal_control_tx,
        local_input_router,
        runtime_event_tx.clone(),
        terminal_dispatch_cancel.clone(),
    ));
    let mut terminal_control_open = true;

    loop {
        let mut maintenance_fired = false;
        if app
            .last_maintenance_ran
            .is_none_or(|last| last.elapsed() >= app.tick_interval())
        {
            maintenance_fired = true;
            app.run_periodic_maintenance_force().await;
            let force_snapshot = std::mem::take(&mut app.state.snapshot_refresh_pending);
            flush_runtime_outputs(
                &mut app,
                &runtime_event_tx,
                &mut last_signal,
                &mut last_auth_event,
                RuntimeFlushOptions::maintenance(force_snapshot),
            )
            .await?;
        }

        let relay_prepare_notified = std::sync::Arc::clone(&app.relay_prepare_notify);
        let relay_prepare_notified = relay_prepare_notified.notified();
        let access_effect_notified = std::sync::Arc::clone(&app.access_effect_notify);
        let access_effect_notified = access_effect_notified.notified();
        let access_mutation_notified = std::sync::Arc::clone(&app.access_mutation_notify);
        let access_mutation_notified = access_mutation_notified.notified();
        tokio::select! {
            biased;
            () = cancellation.cancelled() => break,
            maybe_message = terminal_control_rx.recv(), if terminal_control_open => {
                if let Some(message) = maybe_message {
                    apply_terminal_message(&mut app, message, &runtime_event_tx, &mut last_signal, &mut last_auth_event).await?;
                } else {
                    terminal_control_open = false;
                }
            }
            completion = app.share_transition_completion_rx.recv(), if app.has_share_transition_workers() => {
                let Some(completion) = completion else { continue };
                app.apply_received_share_transition_completion(completion);
                let force_snapshot = app.flush_pending_session_events().await
                    | std::mem::take(&mut app.state.snapshot_refresh_pending);
                flush_runtime_outputs(
                    &mut app,
                    &runtime_event_tx,
                    &mut last_signal,
                    &mut last_auth_event,
                    RuntimeFlushOptions::standard(force_snapshot),
                )
                .await?;
            }
            () = relay_prepare_notified => {
                Box::pin(app.pump_relay_prepare_worker()).await;
                let force_snapshot = app.flush_pending_session_events().await
                    | std::mem::take(&mut app.state.snapshot_refresh_pending);
                flush_runtime_outputs(
                    &mut app,
                    &runtime_event_tx,
                    &mut last_signal,
                    &mut last_auth_event,
                    RuntimeFlushOptions::standard(force_snapshot),
                )
                .await?;
            }
            () = access_effect_notified => {
                Box::pin(app.pump_access_effect_worker()).await;
                let force_snapshot = app.flush_pending_session_events().await
                    | std::mem::take(&mut app.state.snapshot_refresh_pending);
                flush_runtime_outputs(
                    &mut app,
                    &runtime_event_tx,
                    &mut last_signal,
                    &mut last_auth_event,
                    RuntimeFlushOptions::standard(force_snapshot),
                )
                .await?;
            }
            () = access_mutation_notified => {
                Box::pin(app.pump_session_access_mutations()).await;
                let force_snapshot = app.flush_pending_session_events().await
                    | std::mem::take(&mut app.state.snapshot_refresh_pending);
                flush_runtime_outputs(
                    &mut app,
                    &runtime_event_tx,
                    &mut last_signal,
                    &mut last_auth_event,
                    RuntimeFlushOptions::standard(force_snapshot),
                )
                .await?;
            }
            headless_request = commands.headless.recv() => {
                match headless_request {
                    #[cfg(feature = "cli")]
                    HeadlessRuntimeRequest::Snapshot(request) => {
                        let current = app.state.identity.event_context();
                        if request.expected.user_id == current.0
                            && request.expected.epoch == current.1
                        {
                            let signal = session_catalog::build_refresh_fingerprint(
                                &app,
                                app.collaboration_cleanup_health(),
                            );
                            drop(request.reply.send(crate::SnapshotRpcResponse {
                                account_user_id: current.0,
                                account_epoch: current.1,
                                remote_operations_ready: app.remote_surfaces_ready(),
                                auth: publish::auth_event_for_state(&app.state.identity.auth, current.1),
                                sessions: signal.sessions,
                                rooms: signal.rooms.unwrap_or_default(),
                            }));
                            app.state.snapshot_refresh_pending = true;
                        }
                    }
                    #[cfg(feature = "cli")]
                    HeadlessRuntimeRequest::Device(request) => {
                        handlers::dispatch_device_rpc(&mut app, request).await;
                        let force_snapshot = app.drain_session_events().await;
                        flush_runtime_outputs(
                            &mut app,
                            &runtime_event_tx,
                            &mut last_signal,
                            &mut last_auth_event,
                            RuntimeFlushOptions::standard(force_snapshot),
                        )
                        .await?;
                    }
                    #[cfg(feature = "cli")]
                    HeadlessRuntimeRequest::Closed => break,
                }
            }
            maybe_message = commands.system.recv() => {
                let Some(envelope) = maybe_message else {
                    break;
                };
                let Some(message) = accepts_account_command(&app, envelope) else { continue };
                if apply_system_message(&mut app, message, &runtime_event_tx, &cancellation, &mut last_signal, &mut last_auth_event).await? {
                    break;
                }
            }
            maybe_message = commands.auth.recv() => {
                let Some(envelope) = maybe_message else {
                    break;
                };
                let Some(message) = accepts_account_command(&app, envelope) else { continue };
                let outcome = handlers::apply_auth_message_without_flush(&mut app, message).await;
                publish_remote_status(&app, &commands.remote_status);
                handlers::flush_auth_message_outputs(
                    &mut app,
                    outcome,
                    &runtime_event_tx,
                    &mut last_signal,
                    &mut last_auth_event,
                )
                .await?;
            }
            maybe_message = commands.friends.recv() => {
                let Some(envelope) = maybe_message else {
                    break;
                };
                let Some(message) = accepts_account_command(&app, envelope) else { continue };
                apply_friends_message(&mut app, message, &runtime_event_tx, &mut last_signal, &mut last_auth_event).await?;
            }
            maybe_message = commands.devices.recv() => {
                let Some(envelope) = maybe_message else {
                    break;
                };
                let Some(message) = accepts_account_command(&app, envelope) else { continue };
                apply_devices_message(&mut app, message, &runtime_event_tx, &mut last_signal, &mut last_auth_event).await?;
            }
            maybe_message = commands.trust.recv() => {
                let Some(envelope) = maybe_message else {
                    break;
                };
                let Some(message) = accepts_account_command(&app, envelope) else { continue };
                apply_trust_message(&mut app, message, &runtime_event_tx, &mut last_signal, &mut last_auth_event).await?;
            }
            maybe_message = commands.room.recv() => {
                let Some(envelope) = maybe_message else {
                    break;
                };
                let Some(message) = accepts_account_command(&app, envelope) else { continue };
                apply_room_message_boxed(
                    &mut app,
                    message,
                    &runtime_event_tx,
                    &mut last_signal,
                    &mut last_auth_event,
                )
                .await?;
            }
            maybe_message = commands.sessions.recv() => {
                let Some(envelope) = maybe_message else {
                    break;
                };
                let Some(message) = accepts_account_command(&app, envelope) else { continue };
                apply_session_message_boxed(
                    &mut app,
                    message,
                    &runtime_event_tx,
                    &mut last_signal,
                    &mut last_auth_event,
                )
                .await?;
            }
            maybe_message = commands.agent_intel.recv() => {
                let Some(envelope) = maybe_message else {
                    break;
                };
                let Some(message) = accepts_account_command(&app, envelope) else { continue };
                let (account_user_id, account_epoch) = app.state.identity.event_context();
                crate::host_protocol::agent_intel_dispatch::handle_runtime(
                    message,
                    &app.state.local.sessions,
                    &app.state.agent_intel.registry,
                    runtime_event_tx.agent_intel_event_sender().clone(),
                    account_user_id,
                    account_epoch,
                )
                .await;
            }
            maybe_req = commands.hub_subscribe.recv() => {
                match maybe_req {
                    Some(terminal_transport::TerminalHubCommand::Subscribe(req)) => {
                        register_terminal_hub_subscriber(&mut app, req).await;
                    }
                    Some(terminal_transport::TerminalHubCommand::Unsubscribe(req)) => {
                        app.terminal_hub
                            .unregister(req.session_id, req.connection_id);
                        app.release_focus_if_disconnected(req.session_id).await;
                        if req.reply.send(()).is_err() {
                            tracing::debug!(
                                session_id = %req.session_id,
                                connection_id = ?req.connection_id,
                                "terminal unsubscribe caller dropped before acknowledgement",
                            );
                        }
                    }
                    None => break,
                }
            }
            maybe_event = app.session_events_rx.recv() => {
                let Some(event) = maybe_event else {
                    break;
                };
                let force_snapshot = app.handle_session_event(event);
                if let Err(error) = app.inject_ready_steer().await {
                    app.state
                        .record_log(format!("runtime steer injection failed: {error}"));
                }
                let force_snapshot = app.flush_pending_session_events().await | force_snapshot;
                let force_snapshot = force_snapshot | std::mem::take(&mut app.state.snapshot_refresh_pending);
                flush_runtime_outputs(
                    &mut app,
                    &runtime_event_tx,
                    &mut last_signal,
                    &mut last_auth_event,
                    RuntimeFlushOptions::standard(force_snapshot),
                )
                .await?;
            }
            _ = tick.tick() => {
                maintenance_fired = true;
                app.run_periodic_maintenance_force().await;
                let force_snapshot = std::mem::take(&mut app.state.snapshot_refresh_pending);
                flush_runtime_outputs(
                    &mut app,
                    &runtime_event_tx,
                    &mut last_signal,
                    &mut last_auth_event,
                    RuntimeFlushOptions::maintenance(force_snapshot),
                )
                .await?;
            }
        }

        publish_remote_status(&app, &commands.remote_status);

        if maintenance_fired {
            let next =
                next_maintenance_interval(last_activity.elapsed(), active_interval, idle_interval);
            tick.reset_after(next);
        } else {
            last_activity = Instant::now();
        }
    }

    let shutdown_deadline = time::Instant::now()
        + crate::shutdown::TASK_DRAIN_BUDGET
        + crate::shutdown::SESSION_DRAIN_BUDGET;
    let session_deadline = shutdown_deadline;
    let terminal_deadline = session_deadline - crate::shutdown::SESSION_DRAIN_BUDGET;
    let account_shutdown = super::identity::account_runtimes::begin_shutdown_account_runtimes(
        &mut app.state.identity.device_flow,
        &mut app.state.identity.account_runtimes,
        &mut app.state.pending_discovery_surfaces,
    );

    terminal_dispatch_cancel.cancel();
    crate::shutdown::join_within(
        "local terminal input dispatcher",
        terminal_dispatch,
        terminal_deadline,
    )
    .await;

    super::auth::retire_local_collaboration_authority(&mut app);
    app.state.cancel_all_session_relays_immediate();
    app.clear_pending_remote_resizes();

    drain_owned_sessions_for_shutdown(
        &mut app,
        &runtime_event_tx,
        &mut last_signal,
        &mut last_auth_event,
        session_deadline,
    )
    .await;

    account_shutdown.join_before(shutdown_deadline).await;
    tracing::debug!("runtime loop stopped");

    Ok(())
}

async fn run_terminal_dispatch(
    mut terminal_rx: mpsc::Receiver<TerminalCommand>,
    terminal_control_tx: mpsc::Sender<TerminalCommand>,
    local_input_router: LocalInputRouter,
    runtime_event_tx: RuntimeEventSender,
    cancellation: CancellationToken,
) {
    loop {
        let message = tokio::select! {
            biased;
            () = cancellation.cancelled() => break,
            message = terminal_rx.recv() => {
                let Some(message) = message else { break };
                message
            }
        };
        let forwarded = match message {
            TerminalCommand::InputBytes {
                session_id,
                bytes,
                expected_runtime_incarnation_id,
                subscription_id,
                subscription_generation,
            } => match kodosi_domain::ids::SessionId::parse_field(&session_id, "sessionId") {
                Ok(id) => {
                    let expected = match uuid::Uuid::parse_str(&expected_runtime_incarnation_id) {
                        Ok(value) => value,
                        Err(error) => {
                            publish_terminal_dispatch_error(
                                &runtime_event_tx,
                                format!("invalid expectedRuntimeIncarnationId: {error}"),
                            )
                            .await;
                            continue;
                        }
                    };
                    if subscription_id
                        .as_deref()
                        .is_some_and(|id| id.starts_with("headless:"))
                    {
                        Some(TerminalCommand::InputBytes {
                            session_id,
                            bytes,
                            expected_runtime_incarnation_id,
                            subscription_id,
                            subscription_generation,
                        })
                    } else {
                        match local_input_router
                            .send_input(
                                id,
                                Some(expected),
                                crate::session_runtime::commands::SessionInput::new(bytes.clone()),
                            )
                            .await
                        {
                            Some(Ok(())) => None,
                            Some(Err(error)) => {
                                publish_terminal_dispatch_error(
                                    &runtime_event_tx,
                                    error.to_string(),
                                )
                                .await;
                                None
                            }
                            None => Some(TerminalCommand::InputBytes {
                                session_id,
                                bytes,
                                expected_runtime_incarnation_id,
                                subscription_id,
                                subscription_generation,
                            }),
                        }
                    }
                }
                Err(error) => {
                    publish_terminal_dispatch_error(&runtime_event_tx, error.to_string()).await;
                    None
                }
            },
            control => Some(control),
        };
        if let Some(control) = forwarded {
            let sent = tokio::select! {
                biased;
                () = cancellation.cancelled() => break,
                sent = terminal_control_tx.send(control) => sent,
            };
            if sent.is_err() {
                break;
            }
        }
    }
}

async fn publish_terminal_dispatch_error(runtime_event_tx: &RuntimeEventSender, message: String) {
    drop(
        runtime_event_tx
            .send_system(SystemEvent::Error {
                message,
                context: None,
            })
            .await,
    );
}

async fn drain_owned_sessions_for_shutdown(
    app: &mut runtime::Runtime,
    runtime_event_tx: &RuntimeEventSender,
    last_signal: &mut Option<session_catalog::RefreshFingerprint>,
    last_auth_event: &mut Option<AuthEvent>,
    deadline: time::Instant,
) {
    app.state.local.owned_session_runtimes.cancel_all();
    while !app.state.local.owned_session_runtimes.is_empty() && time::Instant::now() < deadline {
        tokio::select! {
            maybe_event = app.session_events_rx.recv() => {
                let Some(event) = maybe_event else { break };
                let force_snapshot = app.handle_session_event(event);
                let force_snapshot = app.flush_pending_session_events().await | force_snapshot;
                if let Err(error) = flush_runtime_outputs(
                    app,
                    runtime_event_tx,
                    last_signal,
                    last_auth_event,
                    RuntimeFlushOptions::maintenance(force_snapshot),
                )
                .await
                {


                    tracing::debug!(
                        %error,
                        "runtime shutdown drain stopped publishing; continuing to stop state"
                    );
                    break;
                }
            }
            () = time::sleep(Duration::from_millis(25)) => {}
        }
    }

    if !app.state.local.owned_session_runtimes.is_empty() {
        let remaining = app.state.local.owned_session_runtimes.ids();
        tracing::warn!(sessions = ?remaining, "graceful runtime shutdown timed out");
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "subscriber admission atomically authorizes capability and captures an exact semantic checkpoint"
)]
async fn register_terminal_hub_subscriber(
    app: &mut runtime::Runtime,
    req: terminal_transport::TerminalHubRequest,
) {
    let session_id = req.session_id;
    let local_owned = app
        .state
        .local
        .owned_session_runtimes
        .runtime_handle(session_id)
        .is_some();
    let remote_record = app.state.discovery.session(session_id);

    if !local_owned && remote_record.is_none() {
        app.remote_terminal.forget(session_id);
        drop(req.reply.send(None));
        return;
    }
    let capability = if local_owned {
        req.capability
    } else {
        let can_inject = remote_record.is_some_and(|record| {
            let owner = record.summary.role == kodosi_domain::session::SessionRole::Owner;
            let capabilities = kodosi_domain::permissions::SessionCapabilities::from_access(
                record.summary.access,
                owner,
            );
            capabilities.0 & kodosi_domain::permissions::SessionCapabilities::SEND_INPUT != 0
        });
        if req.capability.can_write() && can_inject {
            terminal_transport::TerminalCapability::Write
        } else {
            terminal_transport::TerminalCapability::ReadOnly
        }
    };
    let runtime_incarnation_id = app
        .state
        .local
        .sessions
        .record(session_id)
        .map(|record| record.local_incarnation_id)
        .or_else(|| remote_record.and_then(|record| record.incarnation_id));
    let Some(mut handle) =
        app.terminal_hub
            .register_bootstrap_pending(session_id, req.surface, capability)
    else {
        drop(req.reply.send(None));
        return;
    };
    handle.runtime_incarnation_id = runtime_incarnation_id;
    let connection_id = handle.connection_id;

    if app.drain_pending_session_events() {
        app.state.snapshot_refresh_pending = true;
    }

    let control_frame = if let Some(runtime) = app
        .state
        .local
        .owned_session_runtimes
        .runtime_handle(session_id)
    {
        match runtime.capture_checkpoint_data().await {
            Ok(captured) => TerminalControlFrame::SemanticCheckpoint {
                checkpoint: captured.checkpoint,
                next_sequence: captured.applied_sequence,
            },
            Err(error) => {
                let reason = TerminalCloseReason::IoError(error.to_string());
                app.terminal_hub.end_session(session_id, &reason);
                drop(req.reply.send(None));
                return;
            }
        }
    } else if let Some(cached) = app.remote_terminal.cached(session_id) {
        let Some(checkpoint) = cached.checkpoint else {
            app.terminal_hub.unregister(session_id, connection_id);
            drop(req.reply.send(None));
            return;
        };
        let current_sequence = app.terminal_hub.next_sequence(session_id).unwrap_or(0);
        if current_sequence != cached.next_sequence {
            app.terminal_hub.unregister(session_id, connection_id);
            drop(req.reply.send(None));
            return;
        }
        TerminalControlFrame::SemanticCheckpoint {
            checkpoint,
            next_sequence: cached.next_sequence,
        }
    } else {
        app.terminal_hub.unregister(session_id, connection_id);
        drop(req.reply.send(None));
        return;
    };

    if !app
        .terminal_hub
        .send_control(session_id, connection_id, control_frame)
    {
        app.terminal_hub.unregister(session_id, connection_id);
        drop(req.reply.send(None));
        return;
    }

    if req.reply.send(Some(handle)).is_err() {
        app.terminal_hub.unregister(session_id, connection_id);
    }
}

#[cfg(test)]
mod tests;
