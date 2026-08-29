use super::{
    AppError, ConnectionState, FocusTransition, OffsetDateTime, RemoteSessionRecord, Result,
    Runtime, SessionId, SessionInput, SessionProvenance, SessionRelayCommand, SessionRole,
    TerminalSize,
    dispatch::{ensure_participant_terminal_access, send_session_relay_control},
    session_input_bytes, session_mode_label, shift_tab_bytes, shift_tab_count_for_agent,
};
pub(crate) fn owned_remote_record(app: &Runtime, id: SessionId) -> Option<&RemoteSessionRecord> {
    let record = app.state.discovery.session(id)?;
    is_owned_remote_summary(&record.summary).then_some(record)
}

pub(crate) fn owned_remote_record_mut(
    app: &mut Runtime,
    id: SessionId,
) -> Option<&mut RemoteSessionRecord> {
    let record = app.state.discovery.session_mut(id)?;
    is_owned_remote_summary(&record.summary).then_some(record)
}

fn is_owned_remote_summary(summary: &kodosi_domain::session::SessionSummary) -> bool {
    summary.provenance == SessionProvenance::Remote && summary.role == SessionRole::Owner
}

pub(crate) async fn rename(app: &mut Runtime, id: SessionId, title: String) -> Result<()> {
    let current_title = owned_remote_record(app, id)
        .map(|record| record.summary.title.clone())
        .ok_or(AppError::NoActiveSession)?;
    if current_title == title {
        return Ok(());
    }

    crate::runtime::sharing::update_backend_session_title(app, id, &title).await?;
    app.state.record_log(format!(
        "{} renamed remote session to {}",
        id.short(),
        title
    ));
    Ok(())
}

pub(crate) fn set_mode(
    app: &mut Runtime,
    id: SessionId,
    expected_runtime_incarnation_id: uuid::Uuid,
    mode: kodosi_domain::session::SessionMode,
) -> Result<()> {
    let Some(record) = owned_remote_record(app, id)
        .filter(|record| record.incarnation_id == Some(expected_runtime_incarnation_id))
    else {
        return Err(AppError::NoActiveSession);
    };
    let current_mode = record.summary.mode;
    if current_mode == mode {
        return Ok(());
    }

    let shift_tab_count = shift_tab_count_for_agent(
        record.summary.detected_agent.as_deref(),
        current_mode,
        mode,
        id,
    )?;

    crate::runtime::sharing::send_remote_owner_input_payload(
        app,
        id,
        shift_tab_bytes(shift_tab_count),
        "session.mode",
    )?;
    let Some(record) = owned_remote_record_mut(app, id)
        .filter(|record| record.incarnation_id == Some(expected_runtime_incarnation_id))
    else {
        return Err(AppError::NoActiveSession);
    };
    record.summary.mode = mode;
    record.summary.last_update = OffsetDateTime::now_utc();
    app.state.record_log(format!(
        "{} switched remote mode from {} to {}",
        id.short(),
        session_mode_label(current_mode),
        session_mode_label(mode)
    ));
    Ok(())
}

pub(crate) fn send_input(
    app: &mut Runtime,
    id: SessionId,
    input: &SessionInput,
    command_name: &'static str,
) -> Result<()> {
    crate::runtime::sharing::send_remote_owner_input_payload(
        app,
        id,
        session_input_bytes(input).into_owned(),
        command_name,
    )
}

pub(crate) fn stop(app: &mut Runtime, id: SessionId) -> Result<()> {
    crate::runtime::sharing::send_remote_owner_control(
        app,
        id,
        SessionRelayCommand::OwnerStop,
        "session.stop",
    )?;
    if let Some(record) = owned_remote_record_mut(app, id) {
        record.connection_state = Some(ConnectionState::Reconnecting);
        record.connection_reason = Some("Stopping session".to_owned());
        record.summary.last_update = OffsetDateTime::now_utc();
    }
    app.state
        .record_log(format!("{} requested remote session stop", id.short()));
    Ok(())
}

pub(crate) fn interrupt(app: &mut Runtime, id: SessionId) -> Result<()> {
    crate::runtime::sharing::send_remote_owner_control(
        app,
        id,
        SessionRelayCommand::OwnerInterrupt,
        "session.interrupt",
    )?;
    app.state
        .record_log(format!("{} requested remote session interrupt", id.short()));
    Ok(())
}

pub(crate) fn claim_size(
    app: &mut Runtime,
    id: SessionId,
    action_id: String,
    size: TerminalSize,
    pixel_geometry: Option<kodosi_domain::terminal::TerminalPixelGeometry>,
) -> Result<()> {
    crate::runtime::sharing::send_remote_owner_resize_claim_payload(
        app,
        id,
        action_id,
        size,
        pixel_geometry,
        "session.resize",
    )
}

pub(crate) fn focus(app: &mut Runtime, id: SessionId, client_id: &str) -> Result<()> {
    match app.client_focus.note_focus(id, client_id.to_owned()) {
        FocusTransition::Send { focused } => {
            let result = crate::runtime::sharing::send_remote_owner_focus_changed_payload(
                app,
                id,
                focused,
                "session.focus",
            );
            if result.is_err() {
                let _ = app.client_focus.note_blur(id, client_id);
            }
            result
        }
        FocusTransition::Skip => Ok(()),
    }
}

pub(crate) fn blur(app: &mut Runtime, id: SessionId, client_id: &str) -> Result<()> {
    match app.client_focus.note_blur(id, client_id) {
        FocusTransition::Send { focused } => {
            let result = crate::runtime::sharing::send_remote_owner_focus_changed_payload(
                app,
                id,
                focused,
                "session.blur",
            );
            if result.is_err() {
                let _ = app.client_focus.note_focus(id, client_id.to_owned());
            }
            result
        }
        FocusTransition::Skip => Ok(()),
    }
}

pub(crate) fn reassert_focus(app: &mut Runtime, id: SessionId) {
    if app.client_focus.clients(id).is_empty() {
        return;
    }
    if let Err(error) = send_session_relay_control(
        app,
        id,
        SessionRelayCommand::OwnerFocusChanged { focused: true },
        "session.focus.replay",
    ) {
        app.state.record_log(format!(
            "{} could not replay terminal focus after relay reconnect: {error}",
            id.short()
        ));
    }
}

pub(crate) fn send_participant_input(
    app: &mut Runtime,
    id: SessionId,
    input: &SessionInput,
    command_name: &'static str,
) -> Result<()> {
    ensure_participant_terminal_access(app, id, command_name)?;
    send_session_relay_control(
        app,
        id,
        SessionRelayCommand::OwnerInject {
            payload: session_input_bytes(input).into_owned(),
        },
        command_name,
    )
}

pub(crate) fn focus_participant(app: &mut Runtime, id: SessionId, client_id: &str) -> Result<()> {
    ensure_participant_terminal_access(app, id, "session.focus")?;
    match app.client_focus.note_focus(id, client_id.to_owned()) {
        FocusTransition::Send { focused } => {
            let result = send_session_relay_control(
                app,
                id,
                SessionRelayCommand::OwnerFocusChanged { focused },
                "session.focus",
            );
            if result.is_err() {
                let _ = app.client_focus.note_blur(id, client_id);
            }
            result
        }
        FocusTransition::Skip => Ok(()),
    }
}

pub(crate) fn blur_participant(app: &mut Runtime, id: SessionId, client_id: &str) -> Result<()> {
    ensure_participant_terminal_access(app, id, "session.blur")?;
    match app.client_focus.note_blur(id, client_id) {
        FocusTransition::Send { focused } => {
            let result = send_session_relay_control(
                app,
                id,
                SessionRelayCommand::OwnerFocusChanged { focused },
                "session.blur",
            );
            if result.is_err() {
                let _ = app.client_focus.note_focus(id, client_id.to_owned());
            }
            result
        }
        FocusTransition::Skip => Ok(()),
    }
}
