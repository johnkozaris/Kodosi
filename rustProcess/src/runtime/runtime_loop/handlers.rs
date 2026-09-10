use tokio_util::sync::CancellationToken;

use crate::{
    AgentIntelEvent, AppError, AuthCommand, AuthEvent, DeviceCommand, DeviceEvent, FriendsCommand,
    FriendsEvent, RelayActionStatus, Result, RoomCommand, SessionCommand, SessionEvent,
    SystemCommand, SystemEvent, TerminalCommand, TerminalEvent, TrustCommand,
    host_protocol::{
        RoomChatEntry, RoomEntry, RoomEvent, RoomInvitationEntry, RoomMemberEntry, RoomTaskEntry,
        SessionAccessMutationKind, SessionAccessMutationOutcome, TerminalResizeIdentity,
        TrustEvent, TrustPinEntry,
    },
    runtime::{self, session_catalog},
    runtime_event_bus::RuntimeEventSender,
    session_runtime::commands::SessionInput,
};
use kodosi_domain::{
    ids::SessionId,
    terminal::{TerminalPixelGeometry, TerminalSize},
};

use super::publish::{
    RuntimeFlushOptions, flush_runtime_outputs, publish_pending_room_events,
    publish_recovered_room_mutations,
};
#[cfg(feature = "cli")]
use crate::runtime::identity::DeviceRpcOutcome;

pub(crate) async fn apply_terminal_message(
    app: &mut runtime::Runtime,
    message: TerminalCommand,
    runtime_event_tx: &RuntimeEventSender,
    last_signal: &mut Option<session_catalog::RefreshFingerprint>,
    last_auth_event: &mut Option<AuthEvent>,
) -> Result<()> {
    let mut force_snapshot = false;
    let result: Result<()> = async {
        match message {
            TerminalCommand::InputBytes {
                session_id,
                bytes,
                expected_runtime_incarnation_id,
                subscription_id,
                subscription_generation,
            } => {
                let session_id = SessionId::parse_field(&session_id, "sessionId")?;
                let expected = validate_terminal_incarnation(
                    app,
                    session_id,
                    &expected_runtime_incarnation_id,
                    "input",
                )?;
                validate_headless_subscription(
                    app,
                    session_id,
                    subscription_id.as_deref(),
                    subscription_generation,
                )?;
                let input = SessionInput::new(bytes);
                app.send_input_to_session(session_id, expected, input)
                    .await?;
            }
            resize @ TerminalCommand::Resize { .. } => {
                apply_terminal_resize(app, resize, runtime_event_tx).await?;
            }
            TerminalCommand::HeadlessResize {
                session_id,
                cols,
                rows,
                expected_runtime_incarnation_id,
                subscription_id,
                subscription_generation,
            } => {
                let session_id = SessionId::parse_field(&session_id, "sessionId")?;
                validate_terminal_incarnation(
                    app,
                    session_id,
                    &expected_runtime_incarnation_id,
                    "resize",
                )?;
                validate_headless_subscription(
                    app,
                    session_id,
                    Some(&subscription_id),
                    Some(subscription_generation),
                )?;
                app.resize_session(session_id, TerminalSize::new(rows, cols)?, None)
                    .await?;
            }
            focus @ TerminalCommand::Focus { .. } => {
                apply_terminal_focus(app, focus, runtime_event_tx).await?;
            }
            blur @ TerminalCommand::Blur { .. } => {
                apply_terminal_blur(app, blur).await?;
            }
        }
        Ok(())
    }
    .await;

    if let Err(error) = result {
        runtime::auth::mark_expired_if_required(app, &error);
        runtime_event_tx
            .send_system(SystemEvent::Error {
                message: error.to_string(),
                context: None,
            })
            .await?;
    }

    force_snapshot |= app.drain_session_events().await;
    flush_runtime_outputs(
        app,
        runtime_event_tx,
        last_signal,
        last_auth_event,
        RuntimeFlushOptions::standard(force_snapshot),
    )
    .await
}

async fn apply_terminal_blur(app: &mut runtime::Runtime, command: TerminalCommand) -> Result<()> {
    let TerminalCommand::Blur {
        session_id,
        client_id,
        expected_runtime_incarnation_id,
    } = command
    else {
        return Err(AppError::Unsupported {
            reason: "terminal blur helper received another command".to_owned(),
        });
    };
    let session_id = SessionId::parse_field(&session_id, "sessionId")?;
    let expected = uuid::Uuid::parse_str(&expected_runtime_incarnation_id).map_err(|error| {
        AppError::InvalidBackendData {
            field: "expectedRuntimeIncarnationId".to_owned(),
            reason: error.to_string(),
        }
    })?;
    if current_session_incarnation(app, session_id) == Some(expected) {
        tracing::debug!(session_id = %session_id, "received session.blur");
        app.blur_session(session_id, client_id).await?;
    } else {
        tracing::debug!(session_id = %session_id, "ignored stale session.blur");
    }
    Ok(())
}

async fn apply_terminal_focus(
    app: &mut runtime::Runtime,
    command: TerminalCommand,
    runtime_event_tx: &RuntimeEventSender,
) -> Result<()> {
    let TerminalCommand::Focus {
        session_id,
        client_id,
        request_id,
        expected_runtime_incarnation_id,
    } = command
    else {
        return Err(AppError::Unsupported {
            reason: "terminal focus helper received another command".to_owned(),
        });
    };
    validate_terminal_request_id(&request_id, "requestId")?;
    let session_id = SessionId::parse_field(&session_id, "sessionId")?;
    let outcome = async {
        validate_terminal_incarnation(app, session_id, &expected_runtime_incarnation_id, "focus")?;
        tracing::debug!(session_id = %session_id, "received session.focus");
        app.focus_session(session_id, client_id).await
    }
    .await;
    let event = match outcome {
        Ok(()) => TerminalEvent::FocusApplied {
            session_id: session_id.to_string(),
            request_id,
            runtime_incarnation_id: expected_runtime_incarnation_id,
        },
        Err(error) => TerminalEvent::FocusRejected {
            session_id: session_id.to_string(),
            request_id,
            runtime_incarnation_id: expected_runtime_incarnation_id,
            reason: error.to_string(),
        },
    };
    runtime_event_tx.send_terminal_control(event).await
}

fn validate_headless_subscription(
    app: &runtime::Runtime,
    session_id: SessionId,
    subscription_id: Option<&str>,
    subscription_generation: Option<u64>,
) -> Result<()> {
    let Some(subscription_id) = subscription_id else {
        return Ok(());
    };
    let Some(raw) = subscription_id.strip_prefix("headless:") else {
        return Ok(());
    };
    if subscription_generation != Some(1) {
        return Err(AppError::Unsupported {
            reason: "terminal input targets a stale headless subscription generation".to_owned(),
        });
    }
    let connection_id =
        crate::terminal_transport::TerminalConnectionId::parse(raw).map_err(|_| {
            AppError::Unsupported {
                reason: "terminal input carries an invalid headless subscription".to_owned(),
            }
        })?;
    if !app.terminal_hub.has_connection(session_id, connection_id) {
        return Err(AppError::Unsupported {
            reason: "terminal input targets a retired headless subscription".to_owned(),
        });
    }
    Ok(())
}

fn validate_terminal_request_id(value: &str, field: &'static str) -> Result<()> {
    let id = uuid::Uuid::parse_str(value).map_err(|_| AppError::InvalidBackendData {
        field: field.to_owned(),
        reason: "must be a canonical UUIDv7".to_owned(),
    })?;
    if id.get_version_num() != 7 || id.hyphenated().to_string() != value {
        return Err(AppError::InvalidBackendData {
            field: field.to_owned(),
            reason: "must be a canonical UUIDv7".to_owned(),
        });
    }
    Ok(())
}

fn validate_terminal_incarnation(
    app: &runtime::Runtime,
    session_id: SessionId,
    expected: &str,
    operation: &str,
) -> Result<uuid::Uuid> {
    let expected =
        uuid::Uuid::parse_str(expected).map_err(|error| AppError::InvalidBackendData {
            field: "expectedRuntimeIncarnationId".to_owned(),
            reason: error.to_string(),
        })?;
    if current_session_incarnation(app, session_id) != Some(expected) {
        return Err(AppError::Unsupported {
            reason: format!("terminal {operation} targets a stale runtime incarnation"),
        });
    }
    Ok(expected)
}

fn current_session_incarnation(
    app: &runtime::Runtime,
    session_id: SessionId,
) -> Option<uuid::Uuid> {
    app.session_incarnation_id(session_id)
}

async fn reject_terminal_resize(
    runtime_event_tx: &RuntimeEventSender,
    session_id: SessionId,
    identity: TerminalResizeIdentity,
    reason: String,
) -> Result<()> {
    runtime_event_tx
        .send_terminal_control(TerminalEvent::ResizeRejected {
            session_id: session_id.to_string(),
            identity,
            reason,
        })
        .await
}

fn terminal_resize_geometry(
    identity: &TerminalResizeIdentity,
) -> std::result::Result<(TerminalSize, TerminalPixelGeometry), String> {
    let size =
        TerminalSize::new(identity.rows, identity.cols).map_err(|error| error.to_string())?;
    let geometry = TerminalPixelGeometry::new(
        identity.width_pixels,
        identity.height_pixels,
        identity.cell_width_pixels,
        identity.cell_height_pixels,
    )
    .and_then(|geometry| geometry.validate_for_size(size))
    .map_err(|error| error.to_string())?;
    Ok((size, geometry))
}

#[expect(
    clippy::too_many_lines,
    reason = "one resize transaction validates geometry and its correlated fence before emitting one authoritative outcome"
)]
async fn apply_terminal_resize(
    app: &mut runtime::Runtime,
    command: TerminalCommand,
    runtime_event_tx: &RuntimeEventSender,
) -> Result<()> {
    let TerminalCommand::Resize {
        session_id,
        identity,
        claim,
    } = command
    else {
        return Err(AppError::Unsupported {
            reason: "terminal resize handler received a different command".to_owned(),
        });
    };
    let session_id = SessionId::parse_field(&session_id, "sessionId")?;
    validate_terminal_request_id(&identity.request_id, "requestId")?;
    let (size, pixel_geometry) = match terminal_resize_geometry(&identity) {
        Ok(value) => value,
        Err(reason) => {
            return reject_terminal_resize(runtime_event_tx, session_id, identity, reason).await;
        }
    };
    tracing::debug!(
        session_id = %session_id,
        rows = size.rows(),
        cols = size.cols(),
        surface_generation = identity.surface_generation,
        "received session.resize"
    );
    let expected_incarnation_id =
        uuid::Uuid::parse_str(&identity.expected_runtime_incarnation_id).ok();
    if expected_incarnation_id.is_none()
        || expected_incarnation_id != current_session_incarnation(app, session_id)
    {
        return reject_terminal_resize(
            runtime_event_tx,
            session_id,
            identity,
            "terminal resize targets a stale runtime incarnation".to_owned(),
        )
        .await;
    }
    if let Err(error) = validate_headless_subscription(
        app,
        session_id,
        Some(&identity.subscription_id),
        Some(identity.subscription_generation),
    ) {
        return reject_terminal_resize(runtime_event_tx, session_id, identity, error.to_string())
            .await;
    }
    let pending_key = (session_id, identity.request_id.clone());
    if app.pending_remote_resizes.contains_key(&pending_key) {
        return reject_terminal_resize(
            runtime_event_tx,
            session_id,
            identity,
            "this terminal resize request is already pending".to_owned(),
        )
        .await;
    }
    if app.pending_remote_resizes.len() >= 128 {
        return reject_terminal_resize(
            runtime_event_tx,
            session_id,
            identity,
            "too many terminal resize requests are pending".to_owned(),
        )
        .await;
    }
    let outcome = if claim {
        app.claim_size_and_resize(
            session_id,
            identity.request_id.clone(),
            size,
            Some(pixel_geometry),
        )
        .await
    } else {
        app.resize_session(session_id, size, Some(pixel_geometry))
            .await
    };
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            return reject_terminal_resize(
                runtime_event_tx,
                session_id,
                identity,
                error.to_string(),
            )
            .await;
        }
    };
    if let crate::local_sessions::ops::LocalResizeOutcome::PendingRemote { relay_generation } =
        outcome
    {
        app.pending_remote_resizes.insert(
            pending_key,
            runtime::PendingRemoteResize {
                session_id,
                relay_generation,
                identity,
            },
        );
        return Ok(());
    }
    let event = match outcome {
        crate::local_sessions::ops::LocalResizeOutcome::Applied => TerminalEvent::ResizeApplied {
            session_id: session_id.to_string(),
            identity,
        },
        crate::local_sessions::ops::LocalResizeOutcome::RejectedByAuthority => {
            TerminalEvent::ResizeRejected {
                session_id: session_id.to_string(),
                identity,
                reason: "another terminal surface holds size authority".to_owned(),
            }
        }
        crate::local_sessions::ops::LocalResizeOutcome::PendingRemote { .. } => {
            unreachable!("pending remote resize returns before immediate receipt construction")
        }
        crate::local_sessions::ops::LocalResizeOutcome::RejectedRemoteOwnerOnly => {
            TerminalEvent::ResizeRejected {
                session_id: session_id.to_string(),
                identity,
                reason: "remote terminal size is owner-controlled".to_owned(),
            }
        }
    };
    runtime_event_tx.send_terminal_control(event).await
}

#[allow(
    clippy::too_many_lines,
    reason = "one `match` for all system commands keeps exhaustiveness at compile time; splitting would obscure that"
)]
pub(crate) async fn apply_system_message(
    app: &mut runtime::Runtime,
    message: SystemCommand,
    runtime_event_tx: &RuntimeEventSender,
    cancellation: &CancellationToken,
    last_signal: &mut Option<session_catalog::RefreshFingerprint>,
    last_auth_event: &mut Option<AuthEvent>,
) -> Result<bool> {
    let mut force_snapshot = false;
    if let Err(reason) = message.validate() {
        if let Some(request_id) = message.semantic_reply_request_id() {
            super::scoped_events::send_agent_intel(
                app,
                runtime_event_tx,
                AgentIntelEvent::Error {
                    request_id: request_id.to_owned(),
                    message: reason.to_string(),
                    failure_kind: crate::host_protocol::AgentIntelFailureKind::Deterministic,
                    mutation_id: None,
                    reconciliation_required: false,
                },
            )
            .await?;
        } else {
            runtime_event_tx
                .send_system(SystemEvent::Error {
                    message: reason.to_string(),
                    context: None,
                })
                .await?;
        }
        force_snapshot |= app.drain_session_events().await;
        flush_runtime_outputs(
            app,
            runtime_event_tx,
            last_signal,
            last_auth_event,
            RuntimeFlushOptions::maintenance(force_snapshot),
        )
        .await?;
        return Ok(false);
    }

    let result: Result<bool> = match message {
        SystemCommand::Shutdown => {
            cancellation.cancel();
            Ok(true)
        }
        SystemCommand::RefreshClaudeGlobal { cwd, generation } => {
            drop(
                crate::agent_intel::global_refresh::spawn_probed_global_status_refresh(
                    runtime_event_tx.clone(),
                    cwd,
                    generation,
                ),
            );
            Ok(false)
        }
        SystemCommand::SetHostTheme { dark } => {
            let ids = app.state.local.sessions.ids().to_vec();
            for message in app
                .state
                .local
                .owned_session_runtimes
                .try_notify_theme_changed(&ids, dark)
            {
                app.state.record_log(message);
            }
            Ok(false)
        }
        SystemCommand::QueryPendingPermissions { request_id } => {
            let snapshot = app
                .state
                .agent_intel
                .permission_decisions
                .snapshot_for_query()
                .ok_or_else(|| AppError::Unsupported {
                    reason: "could not advance pending-permission snapshot generation".to_owned(),
                })?;
            let payload = serde_json::to_value(snapshot).map_err(AppError::Json)?;
            super::scoped_events::send_agent_intel(
                app,
                runtime_event_tx,
                AgentIntelEvent::Reply {
                    request_id,
                    payload,
                },
            )
            .await?;
            Ok(false)
        }
        SystemCommand::QueryLiveAgentIntelSet { request_id } => {
            let current = app
                .state
                .agent_intel
                .live_authority
                .current(Some(request_id));
            super::scoped_events::send_agent_intel(app, runtime_event_tx, current).await?;
            Ok(false)
        }
        SystemCommand::AllowPendingPermissionRequest {
            session_id,
            session_incarnation_id,
            tool_use_id,
            request_generation,
        } => {
            resolve_pending_permission_decision(
                app,
                runtime_event_tx,
                &session_id,
                &session_incarnation_id,
                &tool_use_id,
                request_generation,
                crate::agent_intel::permission_decision_registry::PermissionDecision::Allow,
            )
            .await?;
            Ok(false)
        }
        SystemCommand::DenyPendingPermissionRequest {
            session_id,
            session_incarnation_id,
            tool_use_id,
            request_generation,
            reason,
        } => {
            resolve_pending_permission_decision(
                app,
                runtime_event_tx,
                &session_id,
                &session_incarnation_id,
                &tool_use_id,
                request_generation,
                crate::agent_intel::permission_decision_registry::PermissionDecision::Deny {
                    reason,
                },
            )
            .await?;
            Ok(false)
        }
        SystemCommand::SemanticSend {
            request_id,
            session_id,
            incarnation_id,
            mode,
            text,
        } => {
            let correlation_id = request_id.clone();
            let result = async {
                let request_id = uuid::Uuid::parse_str(&request_id).map_err(|_| {
                    AppError::InvalidBackendData {
                        field: "requestId".to_owned(),
                        reason: "must be UUIDv7".to_owned(),
                    }
                })?;
                let session_id = SessionId::parse_field(&session_id, "sessionId")?;
                let incarnation_id = uuid::Uuid::parse_str(&incarnation_id).map_err(|_| {
                    AppError::InvalidBackendData {
                        field: "incarnationId".to_owned(),
                        reason: "must be a UUID".to_owned(),
                    }
                })?;
                let entry = if runtime::remote_sessions::owned_remote_record(app, session_id)
                    .is_some()
                {
                    let relay_mode = match mode {
                        crate::host_protocol::SemanticSendMode::Queue => kodosi_backend_client::session_relay::wire::RelaySemanticMode::Queue,
                        crate::host_protocol::SemanticSendMode::Steer => kodosi_backend_client::session_relay::wire::RelaySemanticMode::Steer,
                        crate::host_protocol::SemanticSendMode::StopAndSend => kodosi_backend_client::session_relay::wire::RelaySemanticMode::StopAndSend,
                    };
                    runtime::remote_sessions::semantic_send(
                        app,
                        session_id,
                        request_id,
                        incarnation_id,
                        relay_mode,
                        text.clone(),
                    )?;
                    crate::host_protocol::SteerQueueEntry {
                        steer_id: request_id.to_string(),
                        account_user_id: app
                            .state
                            .identity
                            .auth
                            .subject_string()
                            .unwrap_or_default(),
                        request_id: request_id.to_string(),
                        session_incarnation_id: incarnation_id.to_string(),
                        mode,
                        session_id: session_id.to_string(),
                        text,
                        queued_at_ms: 0,
                        delivery_state: crate::host_protocol::SteerDeliveryState::Preparing,
                        at_tool_use_id: None,
                    }
                } else {
                    app.semantic_send(request_id, session_id, incarnation_id, mode, text)?
                };
                serde_json::to_value(entry).map_err(AppError::Json)
            }
            .await;
            send_steer_reply(app, runtime_event_tx, correlation_id, result).await?;
            Ok(false)
        }
        SystemCommand::CancelSteer {
            request_id,
            session_id,
            steer_id,
        } => {
            let result = SessionId::parse_field(&session_id, "sessionId")
                .map_err(AppError::from)
                .and_then(|id| {
                    if runtime::remote_sessions::owned_remote_record(app, id).is_some() {
                        let semantic_id = uuid::Uuid::parse_str(&steer_id).map_err(|_| {
                            AppError::InvalidBackendData {
                                field: "steerId".to_owned(),
                                reason: "must be UUIDv7".to_owned(),
                            }
                        })?;
                        runtime::remote_sessions::semantic_cancel(app, id, semantic_id)?;
                        runtime::remote_sessions::query_semantics(app, id, Some(&steer_id))
                            .into_iter()
                            .next()
                            .ok_or(AppError::NotFound)
                    } else {
                        app.cancel_steer(id, &steer_id)
                    }
                })
                .and_then(|entry| serde_json::to_value(entry).map_err(AppError::Json));
            send_steer_reply(app, runtime_event_tx, request_id, result).await?;
            Ok(false)
        }
        SystemCommand::QuerySteer {
            request_id,
            session_id,
            semantic_request_id,
        } => {
            let result = SessionId::parse_field(&session_id, "sessionId")
                .map_err(AppError::from)
                .and_then(|id| {
                    let entries =
                        if runtime::remote_sessions::owned_remote_record(app, id).is_some() {
                            runtime::remote_sessions::query_semantics(
                                app,
                                id,
                                semantic_request_id.as_deref(),
                            )
                        } else {
                            app.query_steers(id, semantic_request_id.as_deref())
                        };
                    serde_json::to_value(entries).map_err(AppError::Json)
                });
            send_steer_reply(app, runtime_event_tx, request_id, result).await?;
            Ok(false)
        }
    };

    let should_exit = match result {
        Ok(should_exit) => should_exit,
        Err(error) => {
            runtime::auth::mark_expired_if_required(app, &error);
            runtime_event_tx
                .send_system(SystemEvent::Error {
                    message: error.to_string(),
                    context: None,
                })
                .await?;
            false
        }
    };

    force_snapshot |= app.drain_session_events().await;
    flush_runtime_outputs(
        app,
        runtime_event_tx,
        last_signal,
        last_auth_event,
        RuntimeFlushOptions::maintenance(force_snapshot),
    )
    .await?;

    Ok(should_exit)
}

async fn send_steer_reply(
    app: &runtime::Runtime,
    runtime_event_tx: &RuntimeEventSender,
    request_id: String,
    result: Result<serde_json::Value>,
) -> Result<()> {
    let event = match result {
        Ok(payload) => AgentIntelEvent::Reply {
            request_id,
            payload,
        },
        Err(error) => AgentIntelEvent::Error {
            request_id,
            message: error.to_string(),
            failure_kind: crate::host_protocol::AgentIntelFailureKind::Deterministic,
            mutation_id: None,
            reconciliation_required: false,
        },
    };
    super::scoped_events::send_agent_intel(app, runtime_event_tx, event).await
}

pub(crate) fn apply_session_message_boxed<'a>(
    app: &'a mut runtime::Runtime,
    message: SessionCommand,
    runtime_event_tx: &'a RuntimeEventSender,
    last_signal: &'a mut Option<session_catalog::RefreshFingerprint>,
    last_auth_event: &'a mut Option<AuthEvent>,
) -> std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
    Box::pin(apply_session_message(
        app,
        message,
        runtime_event_tx,
        last_signal,
        last_auth_event,
    ))
}

fn scope_accepted_event(message: &SessionCommand) -> Option<SessionEvent> {
    let SessionCommand::SetShareScope {
        request_id,
        session_id,
        expected_runtime_incarnation_id,
        scope,
        room_id,
    } = message
    else {
        return None;
    };
    Some(SessionEvent::ScopeAccepted {
        request_id: request_id.clone(),
        session_id: session_id.clone(),
        expected_runtime_incarnation_id: expected_runtime_incarnation_id.clone(),
        scope: *scope,
        room_id: room_id.clone(),
        budget_ms: u64::try_from(runtime::sharing::scope_change_caller_budget(*scope).as_millis())
            .unwrap_or(u64::MAX),
    })
}

pub(crate) async fn apply_session_message(
    app: &mut runtime::Runtime,
    message: SessionCommand,
    runtime_event_tx: &RuntimeEventSender,
    last_signal: &mut Option<session_catalog::RefreshFingerprint>,
    last_auth_event: &mut Option<AuthEvent>,
) -> Result<()> {
    let operation = message.operation().to_owned();
    let session_id = message.session_id().map(ToOwned::to_owned);
    let request_id = message.request_id().map(ToOwned::to_owned);
    let scope_accepted = scope_accepted_event(&message);
    let is_scope_change = scope_accepted.is_some();
    let access_mutation = access_mutation_event_target(&message);
    let mut force_snapshot = false;
    let mut defer_catalog_replay = false;

    if let Err(reason) = message.validate() {
        if let Some(target) = access_mutation {
            app.state.runtime_outbox.queue_session(target.result_event(
                SessionAccessMutationOutcome::Rejected,
                Some(reason.to_string()),
            ));
        } else {
            app.state.runtime_outbox.queue_session(SessionEvent::Error {
                operation,
                session_id,
                request_id,
                message: reason.to_string(),
            });
        }
    } else {
        if let Some(accepted) = scope_accepted {
            super::scoped_events::send_session(app, runtime_event_tx, accepted).await?;
        }
        if let Some(target) = &access_mutation {
            super::scoped_events::send_session(app, runtime_event_tx, target.accepted_event())
                .await?;
        }
        match crate::host_protocol::sessions_command::apply_session_command(app, message).await {
            Ok(effects) => {
                force_snapshot |= effects.force_snapshot;
                defer_catalog_replay |= effects.defer_catalog_replay;
                for event in effects.events {
                    app.state.runtime_outbox.queue_session(event);
                }
            }
            Err(error) => {
                runtime::auth::mark_expired_if_required(app, &error);
                if let Some(target) = access_mutation {
                    let outcome = access_mutation_failure_outcome(app, &target);
                    app.state
                        .runtime_outbox
                        .queue_session(target.result_event(outcome, Some(error.to_string())));
                } else {
                    app.state.runtime_outbox.queue_session(SessionEvent::Error {
                        operation,
                        session_id,
                        request_id,
                        message: error.to_string(),
                    });
                }
            }
        }
    }

    if is_scope_change {
        force_snapshot |= app.drain_pending_session_events();
        tokio::time::timeout(
            runtime::sharing::scope_change_verdict_delivery_budget(),
            flush_runtime_outputs(
                app,
                runtime_event_tx,
                last_signal,
                last_auth_event,
                RuntimeFlushOptions::standard(force_snapshot),
            ),
        )
        .await
        .map_err(|_| AppError::Unsupported {
            reason: "scope verdict delivery exceeded its runtime-event deadline".to_owned(),
        })??;

        app.run_periodic_maintenance_throttled().await;
        let post_maintenance_snapshot = app.flush_pending_session_events().await;
        return flush_runtime_outputs(
            app,
            runtime_event_tx,
            last_signal,
            last_auth_event,
            RuntimeFlushOptions::standard(post_maintenance_snapshot),
        )
        .await;
    }

    force_snapshot |= app.drain_session_events().await;
    flush_runtime_outputs(
        app,
        runtime_event_tx,
        last_signal,
        last_auth_event,
        RuntimeFlushOptions::standard(force_snapshot),
    )
    .await?;
    if defer_catalog_replay {
        app.state.snapshot_refresh_pending = true;
        app.state.catalog_refresh = runtime::state::CatalogRefreshState::ReplayPending;
    }
    Ok(())
}

fn access_mutation_failure_outcome(
    app: &runtime::Runtime,
    target: &AccessMutationEventTarget,
) -> SessionAccessMutationOutcome {
    let Some(account) = app.state.identity.auth.subject_string() else {
        return SessionAccessMutationOutcome::Rejected;
    };
    let Ok(mutation_id) = uuid::Uuid::parse_str(&target.mutation_id) else {
        return SessionAccessMutationOutcome::Rejected;
    };
    let Ok(Some(prepared)) = app.access_mutations.get(&account, mutation_id) else {
        return SessionAccessMutationOutcome::Rejected;
    };
    if matches!(
        prepared.state,
        runtime::access_mutations::PreparedSessionAccessMutationState::OutcomeUnknown
            | runtime::access_mutations::PreparedSessionAccessMutationState::Retiring
            | runtime::access_mutations::PreparedSessionAccessMutationState::ReceiptConfirmed
            | runtime::access_mutations::PreparedSessionAccessMutationState::EffectPending
            | runtime::access_mutations::PreparedSessionAccessMutationState::RelayPending { .. }
    ) {
        SessionAccessMutationOutcome::Unknown
    } else {
        SessionAccessMutationOutcome::Rejected
    }
}

#[derive(Clone)]
struct AccessMutationEventTarget {
    mutation_id: String,
    session_id: String,
    expected_runtime_incarnation_id: String,
    kind: SessionAccessMutationKind,
    actor_user_id: Option<String>,
    access_level: Option<kodosi_domain::permissions::AccessLevel>,
    expires_at: Option<String>,
}

impl AccessMutationEventTarget {
    fn accepted_event(&self) -> SessionEvent {
        SessionEvent::AccessMutationAccepted {
            mutation_id: self.mutation_id.clone(),
            session_id: self.session_id.clone(),
            expected_runtime_incarnation_id: self.expected_runtime_incarnation_id.clone(),
            kind: self.kind,
            actor_user_id: self.actor_user_id.clone(),
            access_level: self.access_level,
            expires_at: self.expires_at.clone(),
        }
    }

    fn result_event(
        self,
        outcome: SessionAccessMutationOutcome,
        message: Option<String>,
    ) -> SessionEvent {
        SessionEvent::AccessMutationResult {
            mutation_id: self.mutation_id,
            session_id: self.session_id,
            expected_runtime_incarnation_id: self.expected_runtime_incarnation_id,
            kind: self.kind,
            actor_user_id: self.actor_user_id,
            access_level: self.access_level,
            expires_at: self.expires_at,
            outcome,
            message,
        }
    }
}

fn access_mutation_event_target(message: &SessionCommand) -> Option<AccessMutationEventTarget> {
    match message {
        SessionCommand::Leave {
            mutation_id,
            session_id,
            expected_runtime_incarnation_id,
        } => Some(AccessMutationEventTarget {
            mutation_id: mutation_id.clone(),
            session_id: session_id.clone(),
            expected_runtime_incarnation_id: expected_runtime_incarnation_id.clone(),
            kind: SessionAccessMutationKind::Leave,
            actor_user_id: None,
            access_level: None,
            expires_at: None,
        }),
        SessionCommand::GrantAccess {
            mutation_id,
            session_id,
            expected_runtime_incarnation_id,
            actor_user_id,
            access_level,
            expires_at,
        } => Some(AccessMutationEventTarget {
            mutation_id: mutation_id.clone(),
            session_id: session_id.clone(),
            expected_runtime_incarnation_id: expected_runtime_incarnation_id.clone(),
            kind: SessionAccessMutationKind::Grant,
            actor_user_id: Some(actor_user_id.clone()),
            access_level: Some(*access_level),
            expires_at: Some(expires_at.clone()),
        }),
        SessionCommand::RevokeAccess {
            mutation_id,
            session_id,
            expected_runtime_incarnation_id,
            actor_user_id,
        } => Some(AccessMutationEventTarget {
            mutation_id: mutation_id.clone(),
            session_id: session_id.clone(),
            expected_runtime_incarnation_id: expected_runtime_incarnation_id.clone(),
            kind: SessionAccessMutationKind::Revoke,
            actor_user_id: Some(actor_user_id.clone()),
            access_level: None,
            expires_at: None,
        }),
        _ => None,
    }
}

pub(crate) struct AuthMessageOutcome {
    force_auth: bool,
    result_failed: bool,
}

pub(crate) async fn apply_auth_message_without_flush(
    app: &mut runtime::Runtime,
    message: AuthCommand,
) -> AuthMessageOutcome {
    let operation = message.operation().to_owned();
    let force_auth = matches!(message, AuthCommand::Refresh | AuthCommand::Logout);
    let result = match message {
        AuthCommand::LoginStart => {
            tracing::debug!("received auth.login.start");
            super::super::auth::login(app).await
        }
        AuthCommand::Logout => {
            tracing::debug!("received auth.logout");
            super::super::auth::logout(app).await.map(drop)
        }
        AuthCommand::IdentityReset => {
            tracing::debug!("received auth.identity.reset");
            super::super::identity_reset::reset_identity(app).await
        }
        AuthCommand::Refresh => {
            tracing::debug!("received auth.refresh");
            super::super::auth::ensure_backend_access(app)
                .await
                .map(drop)
        }
    };

    let result_failed = result.is_err();
    if let Err(error) = result {
        app.state.runtime_outbox.queue_auth(AuthEvent::Error {
            operation,
            message: error.to_string(),
        });
    }
    AuthMessageOutcome {
        force_auth,
        result_failed,
    }
}

pub(crate) async fn flush_auth_message_outputs(
    app: &mut runtime::Runtime,
    outcome: AuthMessageOutcome,
    runtime_event_tx: &RuntimeEventSender,
    last_signal: &mut Option<session_catalog::RefreshFingerprint>,
    last_auth_event: &mut Option<AuthEvent>,
) -> Result<()> {
    let force_snapshot = app.drain_session_events().await;
    flush_runtime_outputs(
        app,
        runtime_event_tx,
        last_signal,
        last_auth_event,
        RuntimeFlushOptions::auth_command(
            force_snapshot,
            outcome.force_auth || force_snapshot,
            outcome.result_failed,
        ),
    )
    .await
}

#[cfg(test)]
pub(crate) async fn apply_auth_message(
    app: &mut runtime::Runtime,
    message: AuthCommand,
    runtime_event_tx: &RuntimeEventSender,
    last_signal: &mut Option<session_catalog::RefreshFingerprint>,
    last_auth_event: &mut Option<AuthEvent>,
) -> Result<()> {
    let outcome = apply_auth_message_without_flush(app, message).await;
    flush_auth_message_outputs(app, outcome, runtime_event_tx, last_signal, last_auth_event).await
}

pub(crate) async fn apply_friends_message(
    app: &mut runtime::Runtime,
    message: FriendsCommand,
    runtime_event_tx: &RuntimeEventSender,
    last_signal: &mut Option<session_catalog::RefreshFingerprint>,
    last_auth_event: &mut Option<AuthEvent>,
) -> Result<()> {
    if let Err(reason) = message.validate() {
        app.state.runtime_outbox.queue_friends(FriendsEvent::Error {
            operation: message.operation().to_owned(),
            message: reason.to_string(),
            request_id: match &message {
                FriendsCommand::RequestSend { request_id, .. } => Some(request_id.clone()),
                _ => None,
            },
        });
    } else if app.state.identity.auth.is_authenticated()
        && let Err(error) = runtime::auth::ensure_remote_operation_ready(app).await
    {
        app.state.runtime_outbox.queue_friends(FriendsEvent::Error {
            operation: message.operation().to_owned(),
            message: error.to_string(),
            request_id: match &message {
                FriendsCommand::RequestSend { request_id, .. } => Some(request_id.clone()),
                _ => None,
            },
        });
    } else {
        match message {
            FriendsCommand::Refresh => {
                tracing::debug!("received friends.refresh");
                if app.state.identity.auth.is_authenticated() {
                    app.friends_ctx().refresh_snapshot("refresh").await;
                } else {
                    app.state
                        .runtime_outbox
                        .queue_friends(FriendsEvent::Snapshot {
                            friends: Vec::new(),
                            incoming: Vec::new(),
                            outgoing: Vec::new(),
                            request_id: None,
                        });
                }
            }
            FriendsCommand::RequestSend {
                username,
                request_id,
            } => {
                tracing::debug!(target = %username, %request_id, "received friends.request.send");
                app.friends_ctx().send_request(&username, request_id).await;
            }
            FriendsCommand::RequestAccept { username } => {
                tracing::debug!(target = %username, "received friends.request.accept");
                app.friends_ctx().accept_request(&username).await;
            }
            FriendsCommand::RequestReject { username } => {
                tracing::debug!(target = %username, "received friends.request.reject");
                app.friends_ctx().reject_request(&username).await;
            }
            FriendsCommand::RequestCancel { username } => {
                tracing::debug!(target = %username, "received friends.request.cancel");
                app.friends_ctx().cancel_request(&username).await;
            }
            FriendsCommand::Remove { username } => {
                tracing::debug!(target = %username, "received friends.remove");
                app.friends_ctx().remove_friend(&username).await;
            }
        }
    }

    let force_snapshot = app.drain_session_events().await;
    flush_runtime_outputs(
        app,
        runtime_event_tx,
        last_signal,
        last_auth_event,
        RuntimeFlushOptions::standard(force_snapshot),
    )
    .await
}

#[cfg(feature = "cli")]
fn ensure_device_rpc_account(app: &runtime::Runtime, expected_account_user_id: &str) -> Result<()> {
    if app.state.identity.auth.subject_string().as_deref() == Some(expected_account_user_id) {
        Ok(())
    } else {
        Err(AppError::Unsupported {
            reason: "account changed before the device operation reached runtime authority"
                .to_owned(),
        })
    }
}

#[cfg(feature = "cli")]
pub(crate) async fn dispatch_device_rpc(
    app: &mut runtime::Runtime,
    request: crate::DeviceRpcRequest,
) {
    match request {
        crate::DeviceRpcRequest::Revoke {
            expected_account_user_id,
            device_id,
            reply,
        } => {
            let result = async {
                ensure_device_rpc_account(app, &expected_account_user_id)?;
                runtime::auth::ensure_remote_operation_ready(app).await?;
                let outcome = app.device_list_ctx().revoke(&device_id).await?;
                app.invalidate_hosted_session_keys();
                Ok(DeviceRpcOutcome::Revoked(outcome))
            }
            .await;
            drop(reply.send(result));
        }
        crate::DeviceRpcRequest::ApproveLink {
            expected_account_user_id,
            user_code,
            reply,
        } => {
            let result = async {
                ensure_device_rpc_account(app, &expected_account_user_id)?;
                runtime::auth::ensure_remote_operation_ready(app).await?;
                let outcome = app.device_link_ctx().approve(&user_code).await?;
                if let Err(error) = app.device_list_ctx().refresh().await {
                    app.state.record_log(format!(
                        "device list refresh after approve-link failed: {error}"
                    ));
                }
                Ok(DeviceRpcOutcome::LinkApproved(outcome))
            }
            .await;
            drop(reply.send(result));
        }
        crate::DeviceRpcRequest::StartSelfLink {
            expected_account_user_id,
            label,
            reply,
        } => {
            let result = async {
                ensure_device_rpc_account(app, &expected_account_user_id)?;
                runtime::auth::ensure_self_device_link_ready(app).await?;
                let outcome = app.device_link_ctx().start_self_with_label(label).await?;
                Ok(DeviceRpcOutcome::SelfLinkStarted(outcome))
            }
            .await;
            drop(reply.send(result));
        }
        crate::DeviceRpcRequest::CancelSelfLink {
            expected_account_user_id,
            reply,
        } => {
            let result = ensure_device_rpc_account(app, &expected_account_user_id).map(|()| {
                app.device_link_ctx().cancel_self();
                DeviceRpcOutcome::SelfLinkCancellationRequested
            });
            drop(reply.send(result));
        }
    }
}

pub(crate) async fn apply_devices_message(
    app: &mut runtime::Runtime,
    message: DeviceCommand,
    runtime_event_tx: &RuntimeEventSender,
    last_signal: &mut Option<session_catalog::RefreshFingerprint>,
    last_auth_event: &mut Option<AuthEvent>,
) -> Result<()> {
    let operation = message.operation().to_owned();
    let user_code = message.user_code_for_error();
    if let Err(reason) = message.validate() {
        app.state.runtime_outbox.queue_devices(DeviceEvent::Error {
            user_code,
            operation,
            message: reason.to_string(),
        });
    } else if let Err(error) = dispatch_devices_message(app, message).await {
        runtime::auth::mark_expired_if_required(app, &error);
        app.state.runtime_outbox.queue_devices(DeviceEvent::Error {
            user_code,
            operation,
            message: error.to_string(),
        });
    }

    let force_snapshot = app.drain_session_events().await;
    flush_runtime_outputs(
        app,
        runtime_event_tx,
        last_signal,
        last_auth_event,
        RuntimeFlushOptions::standard(force_snapshot),
    )
    .await
}

async fn dispatch_devices_message(
    app: &mut runtime::Runtime,
    message: DeviceCommand,
) -> Result<()> {
    let operation = message.operation();
    let needs_full_remote_readiness = matches!(
        message,
        DeviceCommand::Refresh | DeviceCommand::Revoke { .. } | DeviceCommand::LinkApprove { .. }
    );
    if needs_full_remote_readiness && app.state.identity.auth.is_authenticated() {
        runtime::auth::ensure_remote_operation_ready(app).await?;
    } else if matches!(message, DeviceCommand::LinkStartSelf)
        && app.state.identity.auth.is_authenticated()
    {
        runtime::auth::ensure_self_device_link_ready(app).await?;
    }
    match message {
        DeviceCommand::Refresh => {
            tracing::debug!("received devices.refresh");
            if !app.state.identity.auth.is_authenticated() {
                return Ok(());
            }
            app.device_list_ctx().refresh().await
        }
        DeviceCommand::Revoke { device_id } => {
            tracing::debug!(target_device = %device_id, "received devices.revoke");
            let outcome = app.device_list_ctx().revoke(&device_id).await?;
            app.invalidate_hosted_session_keys();
            let refreshed = app.device_list_ctx().refresh().await;
            if let Some(message) = outcome.history_warning {
                app.state.runtime_outbox.queue_devices(DeviceEvent::Error {
                    user_code: None,
                    operation: "revoke.history".to_owned(),
                    message,
                });
            }
            refreshed
        }
        DeviceCommand::LinkApprove { user_code } => {
            tracing::debug!(user_code = %user_code, "received devices.link.approve");
            let approve_result = app.device_link_ctx().approve(&user_code).await.map(drop);
            if approve_result.is_ok() {
                let refresh_result = app.device_list_ctx().refresh().await;
                if let Err(err) = refresh_result {
                    app.state.record_log(format!(
                        "device list refresh after approve-link failed: {err}"
                    ));
                }
            }
            approve_result
        }
        DeviceCommand::LinkStartSelf => {
            tracing::debug!("received devices.link.startSelf");
            app.device_link_ctx().start_self().await
        }
        DeviceCommand::LinkCancelSelf => {
            tracing::debug!("received devices.link.cancelSelf");
            app.device_link_ctx().cancel_self();
            Ok(())
        }
    }
    .map_err(|error| {
        tracing::warn!(%error, operation, "device command failed");
        error
    })
}

pub(crate) async fn apply_trust_message(
    app: &mut runtime::Runtime,
    message: TrustCommand,
    runtime_event_tx: &RuntimeEventSender,
    last_signal: &mut Option<session_catalog::RefreshFingerprint>,
    last_auth_event: &mut Option<AuthEvent>,
) -> Result<()> {
    let operation = message.operation().to_owned();
    let request_id = Some(message.request_id().to_owned());
    let user_id = message.user_id().map(str::to_owned);
    if let Err(reason) = message.validate() {
        app.state.runtime_outbox.queue_trust(TrustEvent::Error {
            request_id,
            user_id,
            operation,
            message: reason.to_string(),
        });
    } else if let Err(error) = dispatch_trust_message(app, message).await {
        runtime::auth::mark_expired_if_required(app, &error);
        app.state.runtime_outbox.queue_trust(TrustEvent::Error {
            request_id,
            user_id,
            operation,
            message: error.to_string(),
        });
    }

    let force_snapshot = app.drain_session_events().await;
    flush_runtime_outputs(
        app,
        runtime_event_tx,
        last_signal,
        last_auth_event,
        RuntimeFlushOptions::standard(force_snapshot),
    )
    .await
}

async fn dispatch_trust_message(app: &mut runtime::Runtime, message: TrustCommand) -> Result<()> {
    let operation = message.operation();
    match message {
        TrustCommand::Refresh { request_id } => {
            tracing::debug!(?request_id, "received trust.refresh");
            let pins = app.pin_store.list_pins().await?;
            let entries = pins
                .into_iter()
                .map(|pin| TrustPinEntry {
                    user_id: pin.user_id,
                    generation: pin.generation,
                    signer_device_id: pin.signer_device_id,
                    device_count: pin.device_count,
                    pinned_at_ms: pin.pinned_at_ms,
                })
                .collect();
            app.state.runtime_outbox.queue_trust(TrustEvent::Snapshot {
                request_id,
                pins: entries,
            });
            Ok(())
        }
        TrustCommand::Reset {
            request_id,
            user_id,
        } => {
            tracing::debug!(target_user = %user_id, "received trust.reset");
            let cleared =
                super::super::identity::reset_trust_and_refresh_discovery(app, &user_id).await?;
            app.state.runtime_outbox.queue_trust(TrustEvent::Reset {
                request_id,
                user_id,
                cleared,
            });
            Ok(())
        }
    }
    .map_err(|error: AppError| {
        tracing::warn!(%error, operation, "trust command failed");
        error
    })
}

async fn reconcile_existing_room_mutation(
    app: &mut runtime::Runtime,
    mutation_id: Option<uuid::Uuid>,
    account_user_id: Option<&str>,
    intent: Option<runtime::room_mutations::RoomMutationIntent>,
    operation: &str,
    room_id: Option<&str>,
) -> Result<bool> {
    let (Some(mutation_id), Some(account_user_id), Some(intent)) =
        (mutation_id, account_user_id, intent)
    else {
        return Ok(false);
    };
    let Some(existing) = app
        .room_mutations
        .get(account_user_id, mutation_id)?
        .cloned()
    else {
        return Ok(false);
    };
    if existing.intent == intent {
        if let runtime::room_mutations::PreparedRoomMutationState::Terminal(terminal) =
            existing.state
        {
            app.state
                .runtime_outbox
                .queue_room(RoomEvent::ActionResult {
                    request_id: mutation_id.to_string(),
                    operation: existing.target.kind().to_owned(),
                    room_id: Some(existing.target.room_id().to_owned()),
                    fingerprint: Some(existing.fingerprint),
                    status: terminal.status.action_status(),
                    entity_id: terminal.entity_id,
                    message: terminal.message,
                });
        } else {
            reconcile_room_mutation(app, account_user_id, mutation_id).await?;
        }
    } else {
        app.state
            .runtime_outbox
            .queue_room(RoomEvent::ActionResult {
                request_id: mutation_id.to_string(),
                operation: operation.to_owned(),
                room_id: room_id.map(str::to_owned),
                fingerprint: Some(existing.fingerprint),
                status: crate::host_protocol::RoomActionStatus::Conflict,
                entity_id: None,
                message: Some("mutationId already names a different prepared intent".to_owned()),
            });
    }
    Ok(true)
}

pub(crate) fn apply_room_message_boxed<'a>(
    app: &'a mut runtime::Runtime,
    message: RoomCommand,
    runtime_event_tx: &'a RuntimeEventSender,
    last_signal: &'a mut Option<session_catalog::RefreshFingerprint>,
    last_auth_event: &'a mut Option<AuthEvent>,
) -> std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
    Box::pin(apply_room_message(
        app,
        message,
        runtime_event_tx,
        last_signal,
        last_auth_event,
    ))
}

pub(crate) async fn apply_room_message(
    app: &mut runtime::Runtime,
    message: RoomCommand,
    runtime_event_tx: &RuntimeEventSender,
    last_signal: &mut Option<session_catalog::RefreshFingerprint>,
    last_auth_event: &mut Option<AuthEvent>,
) -> Result<()> {
    let operation = message.operation().to_owned();
    let room_id = message.room_id_for_error();
    let request_id = if matches!(&message, RoomCommand::ReconcileMutation { .. }) {
        None
    } else {
        message.request_id().map(str::to_owned)
    };
    let receipt_mutation_id = if message.is_receipt_authoritative() {
        request_id
            .as_deref()
            .map(parse_room_mutation_id)
            .transpose()?
    } else {
        None
    };
    let account_user_id = app.state.identity.auth.subject_string();
    let reconciliation = reconcile_existing_room_mutation(
        app,
        receipt_mutation_id,
        account_user_id.as_deref(),
        runtime::room_mutations::RoomMutationIntent::from_command(&message),
        &operation,
        room_id.as_deref(),
    )
    .await;
    let reconciled = match reconciliation {
        Ok(reconciled) => reconciled,
        Err(error) => {
            queue_unavailable_room_mutation_error(app, request_id, operation, room_id, &error);
            let force_snapshot = app.drain_session_events().await;
            return flush_runtime_outputs(
                app,
                runtime_event_tx,
                last_signal,
                last_auth_event,
                RuntimeFlushOptions::standard(force_snapshot),
            )
            .await;
        }
    };
    if reconciled {
        let force_snapshot = app.drain_session_events().await;
        return flush_runtime_outputs(
            app,
            runtime_event_tx,
            last_signal,
            last_auth_event,
            RuntimeFlushOptions::standard(force_snapshot),
        )
        .await;
    }
    let dispatch_result = if let Err(reason) = message.validate() {
        Err((reason.to_string(), None))
    } else if !app.state.identity.auth.is_authenticated() {
        Box::pin(dispatch_room_message(app, Some(runtime_event_tx), message))
            .await
            .map_err(|error| {
                let message = error.to_string();
                (message, Some(error))
            })
    } else if let Err(error) = runtime::auth::ensure_remote_operation_ready(app).await {
        let message = error.to_string();
        Err((message, Some(error)))
    } else {
        match reconcile_create_outcome(app, &message).await {
            Ok(Some(outcome)) => Ok(outcome),
            Ok(None) => dispatch_room_message(app, Some(runtime_event_tx), message)
                .await
                .map_err(|error| {
                    let message = error.to_string();
                    (message, Some(error))
                }),
            Err(error) => {
                let message = error.to_string();
                Err((message, Some(error)))
            }
        }
    };
    settle_room_dispatch_result(
        app,
        dispatch_result,
        receipt_mutation_id,
        account_user_id.as_deref(),
        request_id,
        operation,
        room_id,
    )?;

    let force_snapshot = drain_room_session_events(app).await;
    flush_runtime_outputs(
        app,
        runtime_event_tx,
        last_signal,
        last_auth_event,
        RuntimeFlushOptions::standard(force_snapshot),
    )
    .await
}

async fn drain_room_session_events(app: &mut runtime::Runtime) -> bool {
    if app.state.identity.auth.is_authenticated() {
        app.drain_session_events().await
    } else {
        app.drain_pending_session_events()
    }
}

fn queue_unavailable_room_mutation_error(
    app: &mut runtime::Runtime,
    request_id: Option<String>,
    operation: String,
    room_id: Option<String>,
    error: &AppError,
) {
    let message = error.to_string();
    app.state.runtime_outbox.queue_room(RoomEvent::Error {
        room_id: room_id.clone(),
        operation: operation.clone(),
        message: message.clone(),
    });
    if let Some(request_id) = request_id {
        app.state
            .runtime_outbox
            .queue_room(RoomEvent::ActionResult {
                request_id,
                operation,
                room_id,
                fingerprint: None,
                status: crate::host_protocol::RoomActionStatus::Failed,
                entity_id: None,
                message: Some(message),
            });
    }
}

fn settle_room_dispatch_result(
    app: &mut runtime::Runtime,
    dispatch_result: std::result::Result<RoomDispatchOutcome, (String, Option<AppError>)>,
    mutation_id: Option<uuid::Uuid>,
    account_user_id: Option<&str>,
    request_id: Option<String>,
    operation: String,
    room_id: Option<String>,
) -> Result<()> {
    match dispatch_result {
        Ok(outcome) => settle_room_dispatch_success(
            app,
            mutation_id,
            account_user_id,
            request_id,
            operation,
            room_id,
            outcome,
        ),
        Err((message, error)) => settle_room_dispatch_failure(
            app,
            mutation_id,
            account_user_id,
            request_id,
            operation,
            room_id,
            message,
            error.as_ref(),
        ),
    }
}

fn settle_room_dispatch_success(
    app: &mut runtime::Runtime,
    mutation_id: Option<uuid::Uuid>,
    account_user_id: Option<&str>,
    request_id: Option<String>,
    operation: String,
    room_id: Option<String>,
    outcome: RoomDispatchOutcome,
) -> Result<()> {
    let fingerprint = if let (Some(mutation_id), Some(account)) = (mutation_id, account_user_id) {
        app.room_mutations
            .get(account, mutation_id)?
            .map(|prepared| prepared.fingerprint.clone())
    } else {
        None
    };
    let entity_id = outcome.entity_id.clone();
    if let (Some(mutation_id), Some(account)) = (mutation_id, account_user_id)
        && let Some(mut prepared) = app.room_mutations.get(account, mutation_id)?.cloned()
    {
        prepared.state = runtime::room_mutations::PreparedRoomMutationState::Terminal(
            runtime::room_mutations::DurableRoomMutationTerminal {
                status: runtime::room_mutations::DurableRoomMutationTerminalStatus::Succeeded,
                entity_id,
                message: None,
            },
        );
        app.room_mutations.put(prepared)?;
    }
    queue_room_dispatch_success(app, request_id, operation, room_id, fingerprint, outcome);
    Ok(())
}

fn settle_room_dispatch_failure(
    app: &mut runtime::Runtime,
    mutation_id: Option<uuid::Uuid>,
    account_user_id: Option<&str>,
    request_id: Option<String>,
    operation: String,
    room_id: Option<String>,
    message: String,
    error: Option<&AppError>,
) -> Result<()> {
    if let Some(error) = error {
        runtime::auth::mark_expired_if_required(app, error);
    }
    let (status, fingerprint) =
        settle_failed_room_mutation(app, mutation_id, account_user_id, error)?;
    app.state.runtime_outbox.queue_room(RoomEvent::Error {
        room_id: room_id.clone(),
        operation: operation.clone(),
        message: message.clone(),
    });
    if let Some(request_id) = request_id {
        app.state
            .runtime_outbox
            .queue_room(RoomEvent::ActionResult {
                request_id,
                operation,
                room_id,
                fingerprint,
                status,
                entity_id: None,
                message: Some(message),
            });
    }
    Ok(())
}

fn settle_failed_room_mutation(
    app: &mut runtime::Runtime,
    mutation_id: Option<uuid::Uuid>,
    account_user_id: Option<&str>,
    error: Option<&AppError>,
) -> Result<(crate::host_protocol::RoomActionStatus, Option<String>)> {
    let Some(mutation_id) = mutation_id else {
        return Ok((crate::host_protocol::RoomActionStatus::Failed, None));
    };
    let fingerprint = if let Some(account) = account_user_id {
        app.room_mutations
            .get(account, mutation_id)?
            .map(|prepared| prepared.fingerprint.clone())
    } else {
        None
    };
    if error.is_some_and(is_indeterminate_room_write) {
        if let Some(account) = account_user_id
            && let Some(mut prepared) = app.room_mutations.get(account, mutation_id)?.cloned()
        {
            prepared.state = runtime::room_mutations::PreparedRoomMutationState::OutcomeUnknown;
            app.room_mutations.put(prepared)?;
        }
        return Ok((crate::host_protocol::RoomActionStatus::Unknown, fingerprint));
    }
    let status = error.map_or(
        crate::host_protocol::RoomActionStatus::Failed,
        classify_terminal_room_failure,
    );
    let terminal_status = match status {
        crate::host_protocol::RoomActionStatus::Conflict => {
            Some(runtime::room_mutations::DurableRoomMutationTerminalStatus::Conflict)
        }
        crate::host_protocol::RoomActionStatus::Failed => {
            Some(runtime::room_mutations::DurableRoomMutationTerminalStatus::Failed)
        }
        _ => None,
    };
    if let Some(terminal_status) = terminal_status
        && let Some(account) = account_user_id
        && let Some(mut prepared) = app.room_mutations.get(account, mutation_id)?.cloned()
    {
        prepared.state = runtime::room_mutations::PreparedRoomMutationState::Terminal(
            runtime::room_mutations::DurableRoomMutationTerminal {
                status: terminal_status,
                entity_id: None,
                message: error.map(ToString::to_string),
            },
        );
        app.room_mutations.put(prepared)?;
    }
    Ok((status, fingerprint))
}

fn is_indeterminate_room_write(error: &AppError) -> bool {
    matches!(
        error,
        AppError::Io(_) | AppError::Http(_) | AppError::Json(_)
    ) || matches!(
        error,
        AppError::InvalidBackendData { field, .. }
            if field == "roomMutation.response"
    ) || matches!(
        error,
        AppError::HttpProblem {
            status: 500..=599,
            ..
        }
    ) || matches!(
        error,
        AppError::Unsupported { reason }
            if reason.starts_with("backend operation timed out:")
    )
}

fn classify_terminal_room_failure(error: &AppError) -> crate::host_protocol::RoomActionStatus {
    match error {
        AppError::HttpProblem {
            status: 409,
            code: Some(code),
            ..
        } if code == "ROOM_MUTATION_TARGET_CONFLICT" || code == "CONCURRENT_MODIFICATION" => {
            crate::host_protocol::RoomActionStatus::Conflict
        }
        _ => crate::host_protocol::RoomActionStatus::Failed,
    }
}

#[cfg(test)]
mod room_mutation_failure_tests {
    use super::*;

    fn conflict(code: &str) -> AppError {
        AppError::HttpProblem {
            status: 409,
            code: Some(code.to_owned()),
            detail: "conflict".to_owned(),
        }
    }

    #[test]
    fn only_request_identity_rebinding_is_a_retained_conflict() {
        assert_eq!(
            classify_terminal_room_failure(&conflict("ROOM_MUTATION_TARGET_CONFLICT")),
            crate::host_protocol::RoomActionStatus::Conflict
        );
        assert_eq!(
            classify_terminal_room_failure(&conflict("INVITATION_EXPIRED")),
            crate::host_protocol::RoomActionStatus::Failed
        );
    }

    #[test]
    fn concurrent_modification_is_a_terminal_conflict() {
        let error = conflict("CONCURRENT_MODIFICATION");
        assert!(!is_indeterminate_room_write(&error));
        assert_eq!(
            classify_terminal_room_failure(&error),
            crate::host_protocol::RoomActionStatus::Conflict
        );
    }
}

fn queue_room_dispatch_success(
    app: &mut runtime::Runtime,
    request_id: Option<String>,
    operation: String,
    room_id: Option<String>,
    fingerprint: Option<String>,
    outcome: RoomDispatchOutcome,
) {
    let RoomDispatchOutcome {
        entity_id,
        follow_up_failures,
    } = outcome;
    if let Some(request_id) = request_id {
        app.state
            .runtime_outbox
            .queue_room(RoomEvent::ActionResult {
                request_id,
                operation,
                room_id,
                fingerprint,
                status: crate::host_protocol::RoomActionStatus::Succeeded,
                entity_id,
                message: None,
            });
    }
    for failure in follow_up_failures {
        runtime::auth::mark_expired_if_required(app, &failure.error);
        app.state.runtime_outbox.queue_room(RoomEvent::Error {
            room_id: failure.room_id,
            operation: failure.operation.to_owned(),
            message: format!(
                "room mutation succeeded, but follow-up refresh failed: {}",
                failure.error
            ),
        });
    }
}

#[derive(Default)]
struct RoomDispatchOutcome {
    entity_id: Option<String>,
    follow_up_failures: Vec<RoomFollowUpFailure>,
}

struct RoomFollowUpFailure {
    operation: &'static str,
    room_id: Option<String>,
    error: AppError,
}

impl RoomDispatchOutcome {
    fn record_follow_up(
        &mut self,
        operation: &'static str,
        room_id: Option<String>,
        result: Result<()>,
    ) {
        if let Err(error) = result {
            self.follow_up_failures.push(RoomFollowUpFailure {
                operation,
                room_id,
                error,
            });
        }
    }
}

async fn reconcile_room_create(
    app: &runtime::Runtime,
    room_id: &str,
    name: &str,
    slug: &str,
) -> Result<Option<RoomDispatchOutcome>> {
    let rooms = runtime::rooms::RoomApplication::new(app)
        .fetch_rooms()
        .await?;
    let Some(room) = rooms.into_iter().find(|room| room.id == room_id) else {
        return Ok(None);
    };
    if room.name != name || room.slug != slug {
        return Err(AppError::Unsupported {
            reason: "room mutation ID already exists with a different request fingerprint"
                .to_owned(),
        });
    }
    Ok(Some(RoomDispatchOutcome {
        entity_id: Some(room.id),
        follow_up_failures: Vec::new(),
    }))
}

async fn reconcile_create_outcome(
    app: &runtime::Runtime,
    message: &RoomCommand,
) -> Result<Option<RoomDispatchOutcome>> {
    match message {
        RoomCommand::Create {
            name,
            slug,
            request_id: Some(request_id),
        } => reconcile_room_create(app, request_id, name, slug).await,
        _ => Ok(None),
    }
}

fn mutation_entity_id(request_id: Option<&str>) -> Result<String> {
    request_id.map_or_else(
        || Ok(uuid::Uuid::now_v7().to_string()),
        |request_id| {
            uuid::Uuid::parse_str(request_id)
                .map(|id| id.to_string())
                .map_err(|_| AppError::Unsupported {
                    reason: "requestId must be a UUID for idempotent room creates".to_owned(),
                })
        },
    )
}

async fn queue_and_publish_room_mutation_accepted(
    app: &mut runtime::Runtime,
    runtime_event_tx: Option<&RuntimeEventSender>,
    request_id: String,
    operation: &str,
    room_id: String,
    fingerprint: String,
) -> Result<()> {
    app.state
        .runtime_outbox
        .queue_room(RoomEvent::ActionAccepted {
            request_id,
            operation: operation.to_owned(),
            room_id,
            fingerprint,
        });
    if let Some(runtime_event_tx) = runtime_event_tx {
        publish_pending_room_events(app, runtime_event_tx).await?;
    }
    Ok(())
}

#[expect(
    clippy::too_many_lines,
    reason = "single match on RoomCommand variants; per-variant split would force every arm into a tiny helper without clarifying flow"
)]
async fn dispatch_room_message(
    app: &mut runtime::Runtime,
    runtime_event_tx: Option<&RuntimeEventSender>,
    message: RoomCommand,
) -> Result<RoomDispatchOutcome> {
    let operation = message.operation();
    let result: Result<RoomDispatchOutcome> = Box::pin(async {
        let mut outcome = RoomDispatchOutcome::default();
        if !app.state.identity.auth.is_authenticated() {
            if matches!(&message, RoomCommand::Refresh) {
                return Ok(outcome);
            }
            return Err(AppError::Unsupported {
                reason: "not signed in".into(),
            });
        }
        runtime::auth::ensure_remote_operation_ready(app).await?;
        match message {
            RoomCommand::Refresh => push_room_snapshot(app).await?,
            RoomCommand::Create {
                name,
                slug,
                request_id,
            } => {
                let room_id = mutation_entity_id(request_id.as_deref())?;
                let roster = crate::room_crypto::create_roster(app, &room_id).await?;
                let created = app
                    .backend
                    .create_room(
                        &room_id,
                        &name,
                        &slug,
                        roster.generation,
                        &roster.body,
                        &roster.signature,
                        &roster.signer_device_id,
                    )
                    .await;
                let room = match created {
                    Ok(room) => room,
                    Err(error)
                        if matches!(
                            error,
                            kodosi_backend_client::BackendClientError::HttpProblem {
                                status: 409,
                                ..
                            }
                        ) =>
                    {
                        let reconciled = reconcile_room_create(app, &room_id, &name, &slug)
                            .await?
                            .ok_or_else(|| AppError::from(error))?;
                        outcome.entity_id = reconciled.entity_id;
                        return Ok(outcome);
                    }
                    Err(error) => return Err(error.into()),
                };
                tracing::debug!(room_id = %room.id, "room created");
                outcome.entity_id = Some(room.id.clone());
                outcome.record_follow_up(
                    "room.rosterPin.commit",
                    Some(room.id.clone()),
                    crate::room_crypto::commit_roster_pin(
                        app,
                        &room.id,
                        &room.owner_user_id,
                        &roster,
                    ),
                );
                outcome.record_follow_up(
                    "room.snapshot.refresh",
                    Some(room.id),
                    push_room_snapshot(app).await,
                );
            }
            RoomCommand::RemoveMember {
                room_id,
                user_id,
                expected_roster_generation,
                request_id,
            } => {
                let roster = crate::room_crypto::remove_roster_member(
                    app,
                    &room_id,
                    &user_id,
                    expected_roster_generation,
                )
                .await?;
                let mutation_id = parse_room_mutation_id(&request_id)?;
                let target = runtime::room_mutations::RoomMutationTarget::RemoveMember {
                    room_id: room_id.clone(),
                    user_id: user_id.clone(),
                    base_roster_generation: expected_roster_generation,
                    desired_roster_generation: roster.generation,
                    roster_body: roster.body.clone(),
                    roster_signature: roster.signature.clone(),
                    roster_signer_device_id: roster.signer_device_id.clone(),
                };
                let prepared = runtime::room_mutations::PreparedRoomMutation::new(
                    mutation_id,
                    app.state
                        .identity
                        .auth
                        .subject_string()
                        .ok_or(AppError::Unauthorized)?,
                    runtime::room_mutations::RoomMutationIntent::from_target(&target, None),
                    target,
                )?;
                app.room_mutations.put(prepared.clone())?;
                let mut attempting = prepared.clone();
                attempting.state = runtime::room_mutations::PreparedRoomMutationState::Attempting;
                app.room_mutations.put(attempting)?;
                queue_and_publish_room_mutation_accepted(
                    app,
                    runtime_event_tx,
                    request_id.clone(),
                    operation,
                    room_id.clone(),
                    prepared.fingerprint.clone(),
                )
                .await?;
                app.backend
                    .remove_room_member(
                        &room_id,
                        &user_id,
                        &prepared.mutation_id,
                        roster.generation,
                        &roster.body,
                        &roster.signature,
                        &roster.signer_device_id,
                    )
                    .await?;

                app.rotate_room_scoped_session_keys(&room_id);
                let owner_user_id = app
                    .state
                    .identity
                    .auth
                    .subject_string()
                    .ok_or(AppError::Unauthorized)?;
                outcome.entity_id = Some(user_id);
                outcome.record_follow_up(
                    "room.rosterPin.commit",
                    Some(room_id.clone()),
                    crate::room_crypto::commit_roster_pin(app, &room_id, &owner_user_id, &roster),
                );
                outcome.record_follow_up(
                    "room.members.refresh",
                    Some(room_id.clone()),
                    push_room_members(app, room_id, None).await,
                );
            }
            RoomCommand::Invite {
                room_id,
                invitee_user_id,
                request_id,
            } => {
                let invitation_id = mutation_entity_id(request_id.as_deref())?;
                let proposal = crate::room_crypto::create_invitation_proposal(
                    app,
                    &invitation_id,
                    &room_id,
                    &invitee_user_id,
                )
                .await?;
                let request = kodosi_backend_client::api::RoomInvitationProposalRequest {
                    invitation_id: &proposal.invitation_id,
                    invitee_user_id: &proposal.invitee_user_id,
                    proposal_body: &proposal.proposal_body,
                    proposal_signature: &proposal.proposal_signature,
                    proposal_signer_device_id: &proposal.proposal_signer_device_id,
                    proposed_roster_generation: proposal.proposed_roster.generation,
                    proposed_roster_body: &proposal.proposed_roster.body,
                    proposed_roster_signature: &proposal.proposed_roster.signature,
                    proposed_roster_signer_device_id: &proposal.proposed_roster.signer_device_id,
                };
                let invitation = app.backend.invite_room_member(&room_id, &request).await?;
                outcome.entity_id = Some(invitation.id);
                outcome.record_follow_up(
                    "room.invitations.refresh",
                    Some(room_id),
                    push_invitations(app).await,
                );
            }
            RoomCommand::AcceptInvitation {
                invitation_id,
                room_id,
                expected_roster_generation,
                request_id,
            } => {
                let pending = app
                    .backend
                    .fetch_incoming_room_invitations()
                    .await?
                    .into_iter()
                    .find(|invitation| {
                        invitation.id == invitation_id
                            && invitation.room_id == room_id
                            && invitation.base_roster_generation == expected_roster_generation
                    })
                    .ok_or(AppError::NotFound)?;
                let decision = {
                    let mut crypto = crate::room_crypto::RoomCryptoContext::new(app).await?;
                    crypto
                        .sign_invitation_decision(
                            &pending,
                            crate::room_crypto::RoomInvitationDecision::Accepted,
                        )
                        .await?
                };
                let mutation_id = parse_room_mutation_id(&request_id)?;
                let target = runtime::room_mutations::RoomMutationTarget::AcceptInvitation {
                    room_id: room_id.clone(),
                    invitation_id: invitation_id.clone(),
                    base_roster_generation: pending.base_roster_generation,
                    proposed_roster_generation: pending.proposed_roster_generation,
                    decision_body: decision.body.clone(),
                    decision_signature: decision.signature.clone(),
                    decision_signer_device_id: decision.signer_device_id.clone(),
                };
                let prepared = runtime::room_mutations::PreparedRoomMutation::new(
                    mutation_id,
                    app.state
                        .identity
                        .auth
                        .subject_string()
                        .ok_or(AppError::Unauthorized)?,
                    runtime::room_mutations::RoomMutationIntent::from_target(&target, None),
                    target,
                )?;
                app.room_mutations.put(prepared.clone())?;
                let mut attempting = prepared.clone();
                attempting.state = runtime::room_mutations::PreparedRoomMutationState::Attempting;
                app.room_mutations.put(attempting)?;
                queue_and_publish_room_mutation_accepted(
                    app,
                    runtime_event_tx,
                    request_id.clone(),
                    operation,
                    room_id.clone(),
                    prepared.fingerprint.clone(),
                )
                .await?;
                let request = kodosi_backend_client::api::RoomInvitationDecisionRequest {
                    request_id: &prepared.mutation_id,
                    decision_body: &decision.body,
                    decision_signature: &decision.signature,
                    decision_signer_device_id: &decision.signer_device_id,
                };
                let invitation = app
                    .backend
                    .accept_room_invitation(
                        &invitation_id,
                        &room_id,
                        pending.proposed_roster_generation,
                        &request,
                    )
                    .await?;
                outcome.entity_id = Some(invitation.entity_id.to_string());
                let accepted_room_id = invitation.room_id.to_string();
                outcome.record_follow_up(
                    "room.invitations.refresh",
                    Some(accepted_room_id.clone()),
                    push_invitations(app).await,
                );
                outcome.record_follow_up(
                    "room.snapshot.refresh",
                    Some(accepted_room_id),
                    push_room_snapshot(app).await,
                );
            }
            RoomCommand::DeclineInvitation {
                invitation_id,
                room_id,
                expected_roster_generation,
                request_id,
            } => {
                let pending = app
                    .backend
                    .fetch_incoming_room_invitations()
                    .await?
                    .into_iter()
                    .find(|invitation| {
                        invitation.id == invitation_id
                            && invitation.room_id == room_id
                            && invitation.base_roster_generation == expected_roster_generation
                    })
                    .ok_or(AppError::NotFound)?;
                let decision = {
                    let mut crypto = crate::room_crypto::RoomCryptoContext::new(app).await?;
                    crypto
                        .sign_invitation_decision(
                            &pending,
                            crate::room_crypto::RoomInvitationDecision::Declined,
                        )
                        .await?
                };
                let mutation_id = parse_room_mutation_id(&request_id)?;
                let target = runtime::room_mutations::RoomMutationTarget::DeclineInvitation {
                    room_id: room_id.clone(),
                    invitation_id: invitation_id.clone(),
                    base_roster_generation: pending.base_roster_generation,
                    decision_body: decision.body.clone(),
                    decision_signature: decision.signature.clone(),
                    decision_signer_device_id: decision.signer_device_id.clone(),
                };
                let prepared = runtime::room_mutations::PreparedRoomMutation::new(
                    mutation_id,
                    app.state
                        .identity
                        .auth
                        .subject_string()
                        .ok_or(AppError::Unauthorized)?,
                    runtime::room_mutations::RoomMutationIntent::from_target(&target, None),
                    target,
                )?;
                app.room_mutations.put(prepared.clone())?;
                let mut attempting = prepared.clone();
                attempting.state = runtime::room_mutations::PreparedRoomMutationState::Attempting;
                app.room_mutations.put(attempting)?;
                queue_and_publish_room_mutation_accepted(
                    app,
                    runtime_event_tx,
                    request_id.clone(),
                    operation,
                    room_id.clone(),
                    prepared.fingerprint.clone(),
                )
                .await?;
                let request = kodosi_backend_client::api::RoomInvitationDecisionRequest {
                    request_id: &prepared.mutation_id,
                    decision_body: &decision.body,
                    decision_signature: &decision.signature,
                    decision_signer_device_id: &decision.signer_device_id,
                };
                let invitation = app
                    .backend
                    .decline_room_invitation(
                        &invitation_id,
                        &room_id,
                        pending.base_roster_generation,
                        &request,
                    )
                    .await?;
                outcome.entity_id = Some(invitation.entity_id.to_string());
                outcome.record_follow_up(
                    "room.invitations.refresh",
                    Some(invitation.room_id.to_string()),
                    push_invitations(app).await,
                );
            }
            RoomCommand::CancelInvitation {
                invitation_id,
                room_id,
                expected_roster_generation,
                request_id,
            } => {
                let mutation_id = parse_room_mutation_id(&request_id)?;
                let target = runtime::room_mutations::RoomMutationTarget::CancelInvitation {
                    room_id: room_id.clone(),
                    invitation_id: invitation_id.clone(),
                    base_roster_generation: expected_roster_generation,
                };
                let prepared = runtime::room_mutations::PreparedRoomMutation::new(
                    mutation_id,
                    app.state
                        .identity
                        .auth
                        .subject_string()
                        .ok_or(AppError::Unauthorized)?,
                    runtime::room_mutations::RoomMutationIntent::from_target(&target, None),
                    target,
                )?;
                app.room_mutations.put(prepared.clone())?;
                let mut attempting = prepared.clone();
                attempting.state = runtime::room_mutations::PreparedRoomMutationState::Attempting;
                app.room_mutations.put(attempting)?;
                queue_and_publish_room_mutation_accepted(
                    app,
                    runtime_event_tx,
                    request_id.clone(),
                    operation,
                    room_id.clone(),
                    prepared.fingerprint.clone(),
                )
                .await?;
                app.backend
                    .cancel_room_invitation(
                        &invitation_id,
                        &room_id,
                        expected_roster_generation,
                        &prepared.mutation_id,
                    )
                    .await?;
                outcome.entity_id = Some(invitation_id);
                outcome.record_follow_up(
                    "room.invitations.refresh",
                    None,
                    push_invitations(app).await,
                );
            }
            RoomCommand::RefreshMembers {
                room_id,
                hydration_id,
            } => push_room_members(app, room_id, Some(hydration_id)).await?,
            RoomCommand::RefreshInvitations => {
                push_invitations(app).await?;
            }
            RoomCommand::RecoverMutations => {
                let runtime_event_tx = runtime_event_tx.ok_or_else(|| AppError::Unsupported {
                    reason: "room mutation recovery requires an event transport".to_owned(),
                })?;
                publish_pending_room_events(app, runtime_event_tx).await?;
                publish_recovered_room_mutations(app, runtime_event_tx).await?;
            }
            RoomCommand::ReconcileMutation { request_id } => {
                let mutation_id = parse_room_mutation_id(&request_id)?;
                let account = app
                    .state
                    .identity
                    .auth
                    .subject_string()
                    .ok_or(AppError::Unauthorized)?;
                reconcile_room_mutation(app, &account, mutation_id).await?;
            }
            RoomCommand::ChatList {
                room_id,
                since,
                limit,
                tail,
                hydration_id,
            } => {
                let entries = if tail == Some(true) {
                    runtime::rooms::RoomApplication::new(app)
                        .fetch_room_chat_tail(
                            &room_id,
                            limit.unwrap_or(runtime::rooms::ROOM_CHAT_MAX_PAGE_LIMIT),
                        )
                        .await?
                } else {
                    runtime::rooms::RoomApplication::new(app)
                        .fetch_room_chat(&room_id, since, limit)
                        .await?
                }
                .into_iter()
                .map(map_chat_entry)
                .collect();
                app.state
                    .runtime_outbox
                    .queue_room(RoomEvent::ChatSnapshot {
                        room_id,
                        messages: entries,
                        hydration_id: Some(hydration_id),
                    });
            }
            RoomCommand::ChatPost {
                room_id,
                body,
                author_session_id,
                recipient_session_ids,
                recipient_user_ids,
                request_id,
            } => {
                let message_id = mutation_entity_id(request_id.as_deref())?;
                let mutation = runtime::rooms::RoomApplication::new(app)
                    .post_room_chat(
                        &message_id,
                        &room_id,
                        &body,
                        author_session_id.as_deref(),
                        if author_session_id.is_some() {
                            "Agent"
                        } else {
                            "Human"
                        },
                        &recipient_session_ids,
                        &recipient_user_ids,
                    )
                    .await?;
                outcome.entity_id = Some(mutation.entity_id);
                match mutation.projection {
                    Ok(message) => app.state.runtime_outbox.queue_room(RoomEvent::ChatPosted {
                        room_id,
                        message: map_chat_entry(message),
                    }),
                    Err(error) => outcome.follow_up_failures.push(RoomFollowUpFailure {
                        operation: "room.chat.projection",
                        room_id: Some(room_id),
                        error,
                    }),
                }
            }
            RoomCommand::TasksList {
                room_id,
                status,
                assignee,
                offset,
                limit,
                hydration_id,
                snapshot,
            } => {
                if offset.is_none() && limit.is_none() {
                    let tasks = runtime::rooms::RoomApplication::new(app)
                        .fetch_room_tasks(&room_id, status.as_deref(), assignee.as_deref())
                        .await?
                        .into_iter()
                        .map(map_task_entry)
                        .collect();
                    app.state
                        .runtime_outbox
                        .queue_room(RoomEvent::TasksSnapshot { room_id, tasks });
                } else if offset.is_some_and(|offset| offset > 0) && snapshot.is_none() {
                    app.state
                        .runtime_outbox
                        .queue_room(RoomEvent::TasksInvalidated {
                            room_id,
                            hydration_id,
                            request_offset: offset.unwrap_or(0),
                        });
                } else {
                    let page = runtime::rooms::RoomApplication::new(app)
                        .fetch_room_tasks_page(
                            &room_id,
                            status.as_deref(),
                            assignee.as_deref(),
                            offset.unwrap_or(0),
                            limit.unwrap_or(100),
                            snapshot.as_deref(),
                        )
                        .await;
                    match page {
                        Ok(page) => app.state.runtime_outbox.queue_room(RoomEvent::TasksPage {
                            room_id,
                            tasks: page.items.into_iter().map(map_task_entry).collect(),
                            has_more: page.has_more,
                            next_offset: page.next_offset,
                            hydration_id,
                            request_offset: Some(offset.unwrap_or(0)),
                            snapshot: Some(page.snapshot),
                        }),
                        Err(error) if runtime::rooms::is_task_snapshot_conflict(&error) => {
                            app.state
                                .runtime_outbox
                                .queue_room(RoomEvent::TasksInvalidated {
                                    room_id,
                                    hydration_id,
                                    request_offset: offset.unwrap_or(0),
                                });
                        }
                        Err(error) => return Err(error),
                    }
                }
            }
            RoomCommand::TaskCreate {
                room_id,
                title,
                description,
                assigned_session_id,
                assigned_session_incarnation_id,
                due_at,
                request_id,
            } => {
                let task_id = mutation_entity_id(request_id.as_deref())?;
                let due_at_parsed = match due_at.as_deref() {
                    Some(s) => Some(
                        time::OffsetDateTime::parse(
                            s,
                            &time::format_description::well_known::Rfc3339,
                        )
                        .map_err(|e| AppError::Unsupported {
                            reason: format!("dueAt must be RFC3339: {e}"),
                        })?,
                    ),
                    None => None,
                };
                let mutation = runtime::rooms::RoomApplication::new(app)
                    .create_room_task(
                        &task_id,
                        &room_id,
                        title,
                        description,
                        assigned_session_id.as_deref(),
                        assigned_session_incarnation_id.as_deref(),
                        due_at_parsed,
                    )
                    .await?;
                outcome.entity_id = Some(mutation.entity_id);
                match mutation.projection {
                    Ok(task) => app
                        .state
                        .runtime_outbox
                        .queue_room(RoomEvent::TaskUpserted {
                            room_id,
                            task: map_task_entry(task),
                        }),
                    Err(error) => outcome.follow_up_failures.push(RoomFollowUpFailure {
                        operation: "room.tasks.projection",
                        room_id: Some(room_id),
                        error,
                    }),
                }
            }
            RoomCommand::TaskTransition {
                room_id,
                task_id,
                expected_task_revision,
                to_status,
                actor_session_id,
                actor_session_incarnation_id,
                result,
                request_id,
            } => {
                let mutation_id = parse_room_mutation_id(&request_id)?;
                let (encrypted_result, prepared) = {
                    let rooms = runtime::rooms::RoomApplication::new(app);
                    let encrypted_result = rooms
                        .prepare_task_result(&room_id, &task_id, result.as_deref())
                        .await?;
                    let target = runtime::room_mutations::RoomMutationTarget::TransitionTask {
                        room_id: room_id.clone(),
                        task_id: task_id.clone(),
                        expected_task_revision,
                        to_status: to_status.clone(),
                        actor_session_id: actor_session_id.clone(),
                        actor_session_incarnation_id: actor_session_incarnation_id.clone(),
                        encrypted_result: encrypted_result.clone(),
                    };
                    let intent = runtime::room_mutations::RoomMutationIntent::from_target(
                        &target,
                        result.as_deref(),
                    );
                    let prepared = runtime::room_mutations::PreparedRoomMutation::new(
                        mutation_id,
                        app.state
                            .identity
                            .auth
                            .subject_string()
                            .ok_or(AppError::Unauthorized)?,
                        intent,
                        target,
                    )?;
                    (encrypted_result, prepared)
                };
                app.room_mutations.put(prepared.clone())?;
                let mut attempting = prepared.clone();
                attempting.state = runtime::room_mutations::PreparedRoomMutationState::Attempting;
                app.room_mutations.put(attempting)?;
                queue_and_publish_room_mutation_accepted(
                    app,
                    runtime_event_tx,
                    request_id.clone(),
                    operation,
                    room_id.clone(),
                    prepared.fingerprint.clone(),
                )
                .await?;
                let mutation = runtime::rooms::RoomApplication::new(app)
                    .transition_prepared_room_task(
                        &room_id,
                        &task_id,
                        &prepared.mutation_id,
                        expected_task_revision,
                        &to_status,
                        actor_session_id
                            .as_deref()
                            .zip(actor_session_incarnation_id.as_deref()),
                        encrypted_result.as_deref(),
                    )
                    .await?;
                outcome.entity_id = Some(mutation.entity_id);
                match mutation.projection {
                    Ok(task) => app
                        .state
                        .runtime_outbox
                        .queue_room(RoomEvent::TaskUpserted {
                            room_id,
                            task: map_task_entry(task),
                        }),
                    Err(error) => outcome.follow_up_failures.push(RoomFollowUpFailure {
                        operation: "room.tasks.projection",
                        room_id: Some(room_id),
                        error,
                    }),
                }
            }
            RoomCommand::TaskAssign {
                room_id,
                task_id,
                expected_task_revision,
                session_id,
                session_incarnation_id,
                request_id,
            } => {
                let mutation_id = parse_room_mutation_id(&request_id)?;
                let target = runtime::room_mutations::RoomMutationTarget::AssignTask {
                    room_id: room_id.clone(),
                    task_id: task_id.clone(),
                    expected_task_revision,
                    session_id: session_id.clone(),
                    session_incarnation_id: session_incarnation_id.clone(),
                };
                let prepared = runtime::room_mutations::PreparedRoomMutation::new(
                    mutation_id,
                    app.state
                        .identity
                        .auth
                        .subject_string()
                        .ok_or(AppError::Unauthorized)?,
                    runtime::room_mutations::RoomMutationIntent::from_target(&target, None),
                    target,
                )?;
                app.room_mutations.put(prepared.clone())?;
                let mut attempting = prepared.clone();
                attempting.state = runtime::room_mutations::PreparedRoomMutationState::Attempting;
                app.room_mutations.put(attempting)?;
                queue_and_publish_room_mutation_accepted(
                    app,
                    runtime_event_tx,
                    request_id.clone(),
                    operation,
                    room_id.clone(),
                    prepared.fingerprint.clone(),
                )
                .await?;
                let mutation = runtime::rooms::RoomApplication::new(app)
                    .assign_room_task(
                        &room_id,
                        &task_id,
                        &prepared.mutation_id,
                        &prepared.fingerprint,
                        expected_task_revision,
                        session_id.as_deref(),
                        session_incarnation_id.as_deref(),
                    )
                    .await?;
                outcome.entity_id = Some(mutation.entity_id);
                match mutation.projection {
                    Ok(task) => app
                        .state
                        .runtime_outbox
                        .queue_room(RoomEvent::TaskUpserted {
                            room_id,
                            task: map_task_entry(task),
                        }),
                    Err(error) => outcome.follow_up_failures.push(RoomFollowUpFailure {
                        operation: "room.tasks.projection",
                        room_id: Some(room_id),
                        error,
                    }),
                }
            }
        }
        Ok(outcome)
    })
    .await;
    result.map_err(|error: AppError| {
        tracing::warn!(%error, operation, "room command failed");
        error
    })
}

pub(super) fn apply_recovered_room_mutation_security_effects(
    app: &mut runtime::Runtime,
    prepared: &runtime::room_mutations::PreparedRoomMutation,
) -> Result<Option<String>> {
    let runtime::room_mutations::RoomMutationTarget::RemoveMember {
        room_id,
        user_id,
        desired_roster_generation,
        roster_body,
        roster_signature,
        roster_signer_device_id,
        ..
    } = &prepared.target
    else {
        return Ok(None);
    };
    let owner_user_id = app
        .state
        .identity
        .auth
        .subject_string()
        .ok_or(AppError::Unauthorized)?;
    let roster = crate::room_crypto::RoomRosterSubmission::from_committed_removal_target(
        room_id,
        &owner_user_id,
        user_id,
        *desired_roster_generation,
        roster_body.clone(),
        roster_signature.clone(),
        roster_signer_device_id.clone(),
    )?;

    app.rotate_room_scoped_session_keys(room_id);
    crate::room_crypto::commit_roster_pin(app, room_id, &owner_user_id, &roster)?;
    Ok(Some(room_id.clone()))
}

async fn settle_room_mutation_receipt(
    app: &mut runtime::Runtime,
    mutation_id: uuid::Uuid,
    prepared: runtime::room_mutations::PreparedRoomMutation,
    receipt: &kodosi_backend_client::api::BackendRoomMutationReceipt,
) -> Result<()> {
    let outcome = prepared
        .target
        .match_receipt(mutation_id, &prepared.fingerprint, receipt);
    let (status, entity_id, message) = match outcome {
        Some(runtime::room_mutations::RoomMutationReceiptOutcome::Succeeded) => (
            crate::host_protocol::RoomActionStatus::Succeeded,
            Some(receipt.entity_id.to_string()),
            None,
        ),
        Some(runtime::room_mutations::RoomMutationReceiptOutcome::Failed(message)) => (
            crate::host_protocol::RoomActionStatus::Failed,
            None,
            Some(message.to_owned()),
        ),
        None => {
            app.state
                .runtime_outbox
                .queue_room(RoomEvent::ActionResult {
                    request_id: mutation_id.to_string(),
                    operation: prepared.target.kind().to_owned(),
                    room_id: Some(prepared.target.room_id().to_owned()),
                    fingerprint: Some(prepared.fingerprint),
                    status: crate::host_protocol::RoomActionStatus::Conflict,
                    entity_id: None,
                    message: Some(
                        "authoritative receipt conflicts with the prepared mutation".to_owned(),
                    ),
                });
            return Ok(());
        }
    };
    let recovered_room_id = match status {
        crate::host_protocol::RoomActionStatus::Succeeded => {
            let room_id = apply_recovered_room_mutation_security_effects(app, &prepared)?;
            (
                runtime::room_mutations::DurableRoomMutationTerminalStatus::Succeeded,
                room_id,
            )
        }
        _ => (
            runtime::room_mutations::DurableRoomMutationTerminalStatus::Failed,
            None,
        ),
    };
    let (terminal_status, recovered_room_id) = recovered_room_id;
    let mut terminal = prepared.clone();
    terminal.state = runtime::room_mutations::PreparedRoomMutationState::Terminal(
        runtime::room_mutations::DurableRoomMutationTerminal {
            status: terminal_status,
            entity_id: entity_id.clone(),
            message: message.clone(),
        },
    );
    app.room_mutations.put(terminal)?;
    if let Some(room_id) = recovered_room_id
        && let Err(error) = push_room_members(app, room_id.clone(), None).await
    {
        tracing::warn!(%error, %room_id, "room member projection refresh failed after receipt recovery");
    }
    app.state
        .runtime_outbox
        .queue_room(RoomEvent::ActionResult {
            request_id: mutation_id.to_string(),
            operation: prepared.target.kind().to_owned(),
            room_id: Some(prepared.target.room_id().to_owned()),
            fingerprint: Some(prepared.fingerprint),
            status,
            entity_id,
            message,
        });
    Ok(())
}

async fn reconcile_room_mutation(
    app: &mut runtime::Runtime,
    account: &str,
    mutation_id: uuid::Uuid,
) -> Result<()> {
    let Some(prepared) = app.room_mutations.get(account, mutation_id)?.cloned() else {
        return Ok(());
    };
    if matches!(
        prepared.state,
        runtime::room_mutations::PreparedRoomMutationState::Terminal(_)
    ) {
        app.room_mutations.remove(account, mutation_id)?;
        return Ok(());
    }
    let result = app
        .backend
        .fetch_room_mutation_receipt(&mutation_id, prepared.target.operation())
        .await;
    match result {
        Ok(Some(receipt)) => {
            settle_room_mutation_receipt(app, mutation_id, prepared, &receipt).await?;
        }
        Ok(None) => {
            let message =
                "no authoritative receipt is visible yet; the original mutation may still commit"
                    .to_owned();
            let mut unknown = prepared.clone();
            unknown.state = runtime::room_mutations::PreparedRoomMutationState::OutcomeUnknown;
            app.room_mutations.put(unknown)?;
            app.state
                .runtime_outbox
                .queue_room(RoomEvent::ActionResult {
                    request_id: mutation_id.to_string(),
                    operation: prepared.target.kind().to_owned(),
                    room_id: Some(prepared.target.room_id().to_owned()),
                    fingerprint: Some(prepared.fingerprint),
                    status: crate::host_protocol::RoomActionStatus::Unknown,
                    entity_id: None,
                    message: Some(message),
                });
        }
        Err(error) => {
            app.state
                .runtime_outbox
                .queue_room(RoomEvent::ActionResult {
                    request_id: mutation_id.to_string(),
                    operation: prepared.target.kind().to_owned(),
                    room_id: Some(prepared.target.room_id().to_owned()),
                    fingerprint: Some(prepared.fingerprint),
                    status: crate::host_protocol::RoomActionStatus::Unknown,
                    entity_id: None,
                    message: Some(format!("receipt reconciliation failed: {error}")),
                });
        }
    }
    Ok(())
}

fn parse_room_mutation_id(request_id: &str) -> Result<uuid::Uuid> {
    let mutation_id = uuid::Uuid::parse_str(request_id).map_err(|_| AppError::Unsupported {
        reason: "room mutationId must be a canonical UUIDv7".to_owned(),
    })?;
    if mutation_id.get_version_num() != 7 || mutation_id.to_string() != request_id.to_lowercase() {
        return Err(AppError::Unsupported {
            reason: "room mutationId must be a canonical UUIDv7".to_owned(),
        });
    }
    Ok(mutation_id)
}

#[cfg(all(test, feature = "cli"))]
pub(crate) async fn dispatch_room_message_for_test(
    app: &mut runtime::Runtime,
    message: RoomCommand,
) -> Result<()> {
    dispatch_room_message(app, None, message).await.map(drop)
}

pub(crate) async fn push_invitations(app: &mut runtime::Runtime) -> Result<()> {
    let (incoming, outgoing) = tokio::try_join!(
        app.backend.fetch_incoming_room_invitations(),
        app.backend.fetch_outgoing_room_invitations(),
    )?;
    app.state.runtime_outbox.queue_room(RoomEvent::Invitations {
        incoming: incoming.into_iter().map(map_invitation).collect(),
        outgoing: outgoing.into_iter().map(map_invitation).collect(),
    });
    Ok(())
}

pub(crate) async fn push_room_snapshot(app: &mut runtime::Runtime) -> Result<()> {
    let rooms = runtime::rooms::RoomApplication::new(app)
        .fetch_rooms()
        .await?
        .into_iter()
        .map(map_room)
        .collect();
    app.state
        .runtime_outbox
        .queue_room(RoomEvent::Snapshot { rooms });
    Ok(())
}

async fn push_room_members(
    app: &mut runtime::Runtime,
    room_id: String,
    hydration_id: Option<String>,
) -> Result<()> {
    let members = app.backend.fetch_room_members(&room_id).await?;
    app.state.runtime_outbox.queue_room(RoomEvent::Members {
        room_id,
        members: members.into_iter().map(map_member).collect(),
        hydration_id,
    });
    Ok(())
}

fn map_room(dto: kodosi_backend_client::api::BackendRoom) -> RoomEntry {
    RoomEntry {
        id: dto.id,
        name: dto.name,
        slug: dto.slug,
        owner_user_id: dto.owner_user_id,
        roster_generation: dto.roster_generation,
    }
}

fn map_member(dto: kodosi_backend_client::api::BackendRoomMember) -> RoomMemberEntry {
    RoomMemberEntry {
        room_id: dto.room_id,
        user_id: dto.user_id,
        role: dto.role,
        username: dto.username,
        display_name: dto.display_name,
    }
}

fn fmt_dt(dt: time::OffsetDateTime) -> String {
    dt.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| String::new())
}

fn map_invitation(dto: kodosi_backend_client::api::BackendRoomInvitation) -> RoomInvitationEntry {
    RoomInvitationEntry {
        id: dto.id,
        room_id: dto.room_id,
        room_name: dto.room_name,
        room_slug: dto.room_slug,
        invitee_user_id: dto.invitee_user_id,
        invitee_handle: dto.invitee_handle,
        invitee_display_name: dto.invitee_display_name,
        invited_by_user_id: dto.invited_by_user_id,
        invited_by_handle: dto.invited_by_handle,
        invited_by_display_name: dto.invited_by_display_name,
        status: dto.status,
        base_roster_generation: dto.base_roster_generation,
        proposed_roster_generation: dto.proposed_roster_generation,
        created_at: fmt_dt(dto.created_at),
    }
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
        posted_at: fmt_dt(dto.posted_at),
    }
}

fn map_task_entry(dto: kodosi_backend_client::api::BackendRoomTask) -> RoomTaskEntry {
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
        due_at: dto.due_at.map(fmt_dt),
        created_at: fmt_dt(dto.created_at),
        updated_at: fmt_dt(dto.updated_at),
        completed_at: dto.completed_at.map(fmt_dt),
        result: dto.result,
        result_author_user_id: dto.result_author_user_id,
        content_unavailable: dto.content_unavailable,
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "one authority boundary validates and transitions the exact local-or-remote permission tuple"
)]
async fn resolve_pending_permission_decision(
    app: &mut runtime::Runtime,
    runtime_event_tx: &RuntimeEventSender,
    session_id: &str,
    session_incarnation_id: &str,
    tool_use_id: &str,
    request_generation: u64,
    decision: crate::agent_intel::permission_decision_registry::PermissionDecision,
) -> Result<()> {
    let id = SessionId::parse_field(session_id, "sessionId")?;
    let incarnation_id = uuid::Uuid::parse_str(session_incarnation_id).map_err(|_| {
        AppError::InvalidBackendData {
            field: "sessionIncarnationId".to_owned(),
            reason: "must be a UUID".to_owned(),
        }
    })?;
    if request_generation == 0 {
        return Err(AppError::InvalidBackendData {
            field: "requestGeneration".to_owned(),
            reason: "must be nonzero".to_owned(),
        });
    }

    if app.state.discovery.session(id).is_some() {
        let verdict = match &decision {
            crate::agent_intel::permission_decision_registry::PermissionDecision::Allow => "allow",
            crate::agent_intel::permission_decision_registry::PermissionDecision::Deny {
                ..
            } => "deny",
        };
        let current_incarnation =
            app.session_incarnation_id(id)
                .ok_or_else(|| AppError::Unsupported {
                    reason: "remote permission decision has no current session incarnation"
                        .to_owned(),
                })?;
        if current_incarnation != incarnation_id {
            return Err(AppError::Unsupported {
                reason: "remote permission request belongs to a retired incarnation".to_owned(),
            });
        }
        let key = crate::agent_intel::permission_decision_registry::PendingKey {
            session_id: id,
            session_incarnation_id: incarnation_id,
            tool_use_id: tool_use_id.to_owned(),
        };
        if !app
            .state
            .agent_intel
            .permission_decisions
            .actionable_identity(&key, request_generation)
        {
            return Err(AppError::Unsupported {
                reason: "remote permission request is no longer actionable".to_owned(),
            });
        }
        let outcome = runtime::remote_sessions::permission_decision(
            app,
            id,
            tool_use_id,
            request_generation,
            verdict,
        );
        let failure_status = match outcome {
            runtime::SessionRelayCommandDispatchOutcome::Enqueued => None,
            runtime::SessionRelayCommandDispatchOutcome::Busy => Some(RelayActionStatus::Busy),
            runtime::SessionRelayCommandDispatchOutcome::Rejected => {
                Some(RelayActionStatus::Rejected)
            }
        };
        let mutation = if failure_status.is_none() {
            app.state
                .agent_intel
                .permission_decisions
                .mark_sending(&key, request_generation)
        } else {
            app.state
                .agent_intel
                .permission_decisions
                .mark_actionable(&key, request_generation)
        };
        if mutation.changed() {
            app.state.runtime_outbox.queue_pending_permissions_snapshot(
                app.state.agent_intel.permission_decisions.snapshot(),
            );
        }
        app.state.runtime_outbox.queue_agent_intel(
            AgentIntelEvent::RemotePermissionDecisionState {
                session_id: session_id.to_owned(),
                session_incarnation_id: session_incarnation_id.to_owned(),
                tool_use_id: tool_use_id.to_owned(),
                request_generation,
                phase: if failure_status.is_some() {
                    crate::RemotePermissionDecisionPhase::Failed
                } else {
                    crate::RemotePermissionDecisionPhase::Sending
                },
                status: failure_status,
                message: failure_status.map(|_| {
                    "session relay rejected the permission decision before delivery".to_owned()
                }),
            },
        );
        if let Some(status) = failure_status {
            app.state
                .runtime_outbox
                .queue_session(SessionEvent::ActionResult {
                    session_id: session_id.to_owned(),
                    action_id: tool_use_id.to_owned(),
                    status,
                });
            runtime_event_tx
                .send_system(SystemEvent::Error {
                    message: format!(
                        "could not send permission decision for sessionId={session_id} \
                         toolUseId={tool_use_id}: the session relay rejected the decision \
                         (not connected or queue full)"
                    ),
                    context: Some("agent.intel.resolvePendingPermissionRequest".to_owned()),
                })
                .await?;
        }
        return Ok(());
    }

    let key = crate::agent_intel::permission_decision_registry::PendingKey {
        session_id: id,
        session_incarnation_id: incarnation_id,
        tool_use_id: tool_use_id.to_owned(),
    };
    if app
        .state
        .agent_intel
        .permission_decisions
        .resolve_exact(&key, request_generation, decision)
    {
        app.state.runtime_outbox.queue_pending_permissions_snapshot(
            app.state.agent_intel.permission_decisions.snapshot(),
        );
        app.publish_pending_permissions(id, incarnation_id);
    } else {
        runtime_event_tx
            .send_system(SystemEvent::Error {
                message: format!(
                    "no pending permission decision for sessionId={session_id} toolUseId={tool_use_id} \
                     (already resolved or timed out)"
                ),
                context: Some("agent.intel.resolvePendingPermissionRequest".to_owned()),
            })
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod room_follow_up_tests {
    use super::{RoomDispatchOutcome, RoomFollowUpFailure, queue_room_dispatch_success};
    use crate::{
        AppError,
        config::AppConfig,
        host_protocol::{RoomActionStatus, RoomEvent},
        runtime::Runtime,
    };
    use tokio_util::sync::CancellationToken;

    fn assert_durable_success_survives_refresh_failure(
        operation: &str,
        expected_entity_id: &str,
        follow_up_operation: &'static str,
    ) {
        let mut config = AppConfig::default();
        config.auth.keyring_service = "kodosi.test".to_owned();
        let mut app = Runtime::with_dependencies(
            config.clone(),
            CancellationToken::new(),
            crate::runtime::RuntimeDependencies::isolated(&config)
                .expect("isolated runtime dependencies"),
        )
        .unwrap_or_else(|error| panic!("test app should construct: {error}"));

        queue_room_dispatch_success(
            &mut app,
            Some("request-1".to_owned()),
            operation.to_owned(),
            Some("room-1".to_owned()),
            None,
            RoomDispatchOutcome {
                entity_id: Some(expected_entity_id.to_owned()),
                follow_up_failures: vec![RoomFollowUpFailure {
                    operation: follow_up_operation,
                    room_id: Some("room-1".to_owned()),
                    error: AppError::Unsupported {
                        reason: "refresh unavailable".to_owned(),
                    },
                }],
            },
        );

        std::assert_matches!(
            app.state.runtime_outbox.drain_room().as_slice(),
            [
                RoomEvent::ActionResult {
                    request_id,
                    status: RoomActionStatus::Succeeded,
                    entity_id: Some(entity_id),
                    ..
                },
                RoomEvent::Error {
                    operation,
                    message,
                    ..
                },
            ] if request_id == "request-1"
                && entity_id == expected_entity_id
                && operation == follow_up_operation
                && message.contains("mutation succeeded")
        );
    }

    #[test]
    fn room_create_durable_success_survives_snapshot_refresh_failure() {
        assert_durable_success_survives_refresh_failure(
            "room.create",
            "room-1",
            "room.snapshot.refresh",
        );
    }

    #[test]
    fn room_invite_reuses_the_correlated_request_as_invitation_identity() {
        let request_id = uuid::Uuid::now_v7().to_string();
        assert_eq!(
            super::mutation_entity_id(Some(&request_id)).expect("UUID request identity"),
            request_id
        );
    }

    #[test]
    fn room_invite_durable_success_survives_invitation_refresh_failure() {
        assert_durable_success_survives_refresh_failure(
            "room.invite",
            "invitation-1",
            "room.invitations.refresh",
        );
    }

    #[test]
    fn room_accept_durable_success_survives_snapshot_refresh_failure() {
        assert_durable_success_survives_refresh_failure(
            "room.acceptInvitation",
            "invitation-1",
            "room.snapshot.refresh",
        );
    }
}
