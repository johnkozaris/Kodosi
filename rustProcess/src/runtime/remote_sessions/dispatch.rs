use super::{
    AppError, ConnectionState, Result, Runtime, SessionId, SessionRelayCommand,
    SessionRelayCommandDispatchOutcome, SessionRole, mpsc,
};
pub(crate) fn ensure_authenticated_remote_account(app: &Runtime, command_name: &str) -> Result<()> {
    if !app.state.identity.auth.is_authenticated() || app.state.identity.auth.subject().is_none() {
        return Err(AppError::Unsupported {
            reason: format!(
                "{command_name} is unavailable while authenticated account reconciliation is in progress"
            ),
        });
    }
    Ok(())
}

pub(super) fn ensure_participant_terminal_access(
    app: &Runtime,
    id: SessionId,
    command_name: &str,
) -> Result<()> {
    ensure_authenticated_remote_account(app, command_name)?;
    let record = app
        .state
        .discovery
        .session(id)
        .ok_or(AppError::NoActiveSession)?;
    if record.summary.role == SessionRole::Owner {
        return Err(AppError::Unsupported {
            reason: format!("{command_name} expected participant routing for remote session {id}"),
        });
    }
    if record.viewer_blocked
        || record.summary.access != kodosi_domain::permissions::AccessLevel::Inject
    {
        return Err(AppError::Unsupported {
            reason: format!(
                "{command_name} requires current Inject access for remote session {id}"
            ),
        });
    }
    Ok(())
}

pub(super) fn send_session_relay_control(
    app: &mut Runtime,
    id: SessionId,
    command: SessionRelayCommand,
    command_name: &str,
) -> Result<()> {
    match send_session_relay_command(app, id, command, command_name, None) {
        SessionRelayCommandDispatchOutcome::Enqueued => Ok(()),
        SessionRelayCommandDispatchOutcome::Busy => Err(AppError::Unsupported {
            reason: format!("{command_name} queue is busy for remote session {id}"),
        }),
        SessionRelayCommandDispatchOutcome::Rejected => Err(AppError::Unsupported {
            reason: format!("{command_name} relay is unavailable for remote session {id}"),
        }),
    }
}

#[derive(Clone, Copy)]
pub(super) struct PermissionActionCorrelation<'a> {
    pub(super) action_id: &'a str,
    pub(super) tool_use_id: &'a str,
}

fn required_relay_capability(command: &SessionRelayCommand) -> u16 {
    use kodosi_domain::permissions::SessionCapabilities;

    match command {
        SessionRelayCommand::SemanticSend { .. }
        | SessionRelayCommand::SemanticCancel { .. }
        | SessionRelayCommand::Suggest { .. } => SessionCapabilities::SUGGEST,
        SessionRelayCommand::Inject { .. } | SessionRelayCommand::OwnerInject { .. } => {
            SessionCapabilities::SEND_INPUT
        }
        SessionRelayCommand::PermissionDecision { .. } => SessionCapabilities::APPROVE_DENY,
        SessionRelayCommand::OwnerResize { .. } => SessionCapabilities::OWNER,
        SessionRelayCommand::OwnerFocusChanged { .. } => SessionCapabilities::FOCUS,
        SessionRelayCommand::OwnerStop | SessionRelayCommand::OwnerInterrupt => {
            SessionCapabilities::STOP
        }
    }
}

pub(super) fn send_session_relay_command(
    app: &mut Runtime,
    id: SessionId,
    command: SessionRelayCommand,
    command_name: &str,
    correlation: Option<PermissionActionCorrelation<'_>>,
) -> SessionRelayCommandDispatchOutcome {
    if let Err(error) = ensure_authenticated_remote_account(app, command_name) {
        app.state
            .record_log(format!("{} ignored {command_name}: {error}", id.short()));
        return SessionRelayCommandDispatchOutcome::Rejected;
    }
    let Some(record) = app.state.discovery.session(id) else {
        app.state.record_log(format!(
            "{} ignored {command_name} because the session is absent from the authenticated catalog",
            id.short()
        ));
        return SessionRelayCommandDispatchOutcome::Rejected;
    };
    let required_capability = required_relay_capability(&command);
    let capabilities = kodosi_domain::permissions::SessionCapabilities::from_access(
        record.summary.access,
        record.summary.role == SessionRole::Owner,
    );
    if record.viewer_blocked || !capabilities.allows(required_capability) {
        app.state.record_log(format!(
            "{} ignored {command_name} because the authenticated catalog does not grant it",
            id.short()
        ));
        return SessionRelayCommandDispatchOutcome::Rejected;
    }

    let Some(status) = app.state.remote.session_relays.status(id) else {
        app.state.record_log(format!(
            "{} ignored {command_name} because the session relay is not active",
            id.short()
        ));
        return SessionRelayCommandDispatchOutcome::Rejected;
    };

    if status != ConnectionState::Connected {
        app.state.record_log(format!(
            "{} ignored {command_name} while the session relay was {:?}",
            id.short(),
            status
        ));
        return SessionRelayCommandDispatchOutcome::Rejected;
    }

    let send_result = match correlation {
        Some(correlation) => app
            .state
            .remote
            .session_relays
            .try_send_permission_decision(
                id,
                correlation.action_id,
                correlation.tool_use_id,
                command,
            ),
        None => app.state.remote.session_relays.try_send(id, command),
    };
    match send_result {
        None => {
            app.state.record_log(format!(
                "{} ignored {command_name} because the session relay is not active",
                id.short()
            ));
            SessionRelayCommandDispatchOutcome::Rejected
        }
        Some(Ok(())) => SessionRelayCommandDispatchOutcome::Enqueued,
        Some(Err(mpsc::error::TrySendError::Full(_))) => {
            app.state.record_log(format!(
                "{} dropped {command_name} because the session relay command queue is full",
                id.short()
            ));
            SessionRelayCommandDispatchOutcome::Busy
        }
        Some(Err(mpsc::error::TrySendError::Closed(_))) => {
            app.state.record_log(format!(
                "{} ignored {command_name} because the session relay command queue closed",
                id.short()
            ));
            SessionRelayCommandDispatchOutcome::Rejected
        }
    }
}
