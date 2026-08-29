use super::{
    AppError, DeviceKeyAccess, Result, Runtime, SessionId, SessionRelayCommand,
    dispatch::send_session_relay_control, session_relay,
};
pub(crate) fn semantic_send(
    app: &mut Runtime,
    id: SessionId,
    request_id: uuid::Uuid,
    incarnation_id: uuid::Uuid,
    mode: session_relay::wire::RelaySemanticMode,
    text: String,
) -> Result<()> {
    let account_user_id = app
        .state
        .identity
        .auth
        .subject_string()
        .ok_or(AppError::Unauthorized)?;
    let requester_device_id = DeviceKeyAccess::new(&app.state.identity.auth, &app.device_key_store)
        .load_authenticated_device_keys()?
        .device_id
        .clone();
    let signer = app
        .remote_semantics
        .signer_snapshot(&account_user_id, id, incarnation_id)
        .cloned()
        .ok_or_else(|| AppError::Unsupported {
            reason: "remote semantic owner signer is not durably established".to_owned(),
        })?;
    let request = crate::runtime::remote_semantics::RemoteSemanticRequest {
        account_user_id,
        requester_device_id,
        session_id: id.to_string(),
        incarnation_id,
        request_id,
        mode,
        payload_sha256: kodosi_backend_client::crypto::sha256_hex(text.as_bytes()),
        text,
        signer: Some(signer),
    };
    let request = app.remote_semantics.admit(request)?;
    dispatch_remote_semantic(app, id, &request)
}

pub(crate) fn semantic_cancel(
    app: &mut Runtime,
    id: SessionId,
    request_id: uuid::Uuid,
) -> Result<()> {
    let account_user_id = app
        .state
        .identity
        .auth
        .subject_string()
        .ok_or(AppError::Unauthorized)?;
    let request = app
        .remote_semantics
        .pending_for(&account_user_id, id)
        .into_iter()
        .find(|request| request.request_id == request_id)
        .ok_or(AppError::NotFound)?;
    if app
        .state
        .discovery
        .session(id)
        .and_then(|record| record.incarnation_id)
        != Some(request.incarnation_id)
    {
        return Err(AppError::Unsupported {
            reason: "cannot cancel a retired remote session incarnation".to_owned(),
        });
    }
    send_session_relay_control(
        app,
        id,
        SessionRelayCommand::SemanticCancel {
            request_id: request.request_id,
            incarnation_id: request.incarnation_id,
            mode: request.mode,
            payload_sha256: request.payload_sha256,
        },
        "agent.intel.semanticCancel",
    )
}

fn dispatch_remote_semantic(
    app: &mut Runtime,
    id: SessionId,
    request: &crate::runtime::remote_semantics::RemoteSemanticRequest,
) -> Result<()> {
    send_session_relay_control(
        app,
        id,
        SessionRelayCommand::SemanticSend {
            request_id: request.request_id,
            incarnation_id: request.incarnation_id,
            mode: request.mode,
            payload_sha256: request.payload_sha256.clone(),
            text: request.text.clone(),
        },
        "agent.intel.semanticSend",
    )
}

pub(crate) fn query_semantics(
    app: &Runtime,
    id: SessionId,
    request_id: Option<&str>,
) -> Vec<crate::host_protocol::SteerQueueEntry> {
    app.state
        .identity
        .auth
        .subject_string()
        .map_or_else(Vec::new, |account| {
            app.remote_semantics.query(&account, id, request_id)
        })
}

pub(crate) fn replay_remote_semantics(app: &mut Runtime, id: SessionId) {
    let Some(account_user_id) = app.state.identity.auth.subject_string() else {
        return;
    };
    let Some(active_incarnation_id) = app
        .state
        .discovery
        .session(id)
        .and_then(|record| record.incarnation_id)
    else {
        return;
    };
    for request in app
        .remote_semantics
        .pending_for(&account_user_id, id)
        .into_iter()
        .filter(|request| request.incarnation_id == active_incarnation_id)
    {
        if let Err(error) = dispatch_remote_semantic(app, id, &request) {
            app.state.record_log(format!(
                "{} remote semantic replay deferred: {error}",
                id.short()
            ));
            break;
        }
    }
}
