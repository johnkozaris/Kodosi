use super::{
    Runtime, SessionId, SessionRelayCommand, SessionRelayCommandDispatchOutcome,
    dispatch::{PermissionActionCorrelation, send_session_relay_command},
};
pub(crate) fn permission_decision(
    app: &mut Runtime,
    id: SessionId,
    request_id: &str,
    request_generation: u64,
    decision: &str,
) -> SessionRelayCommandDispatchOutcome {
    let Some(account_user_id) = app.state.identity.auth.subject_string() else {
        return SessionRelayCommandDispatchOutcome::Rejected;
    };
    let Some(incarnation_id) = app
        .state
        .discovery
        .session(id)
        .and_then(|record| record.incarnation_id)
    else {
        return SessionRelayCommandDispatchOutcome::Rejected;
    };
    let action = crate::runtime::permission_actions::RemotePermissionAction {
        account_user_id,
        session_id: id.to_string(),
        incarnation_id,
        action_id: uuid::Uuid::now_v7().to_string(),
        request_id: request_id.to_owned(),
        request_generation,
        decision: decision.to_owned(),
    };
    let action = match app.remote_permission_actions.admit_or_existing(action) {
        Ok(action) => action,
        Err(error) => {
            app.state.record_log(format!(
                "{} could not persist remote permission decision: {error}",
                id.short()
            ));
            return SessionRelayCommandDispatchOutcome::Rejected;
        }
    };
    let dispatch = dispatch_remote_permission_action(app, id, &action);
    if dispatch != SessionRelayCommandDispatchOutcome::Enqueued {
        app.state.record_log(format!(
            "{} retained remote permission decision {} for relay replay",
            id.short(),
            action.action_id
        ));
    }
    SessionRelayCommandDispatchOutcome::Enqueued
}

fn dispatch_remote_permission_action(
    app: &mut Runtime,
    id: SessionId,
    action: &crate::runtime::permission_actions::RemotePermissionAction,
) -> SessionRelayCommandDispatchOutcome {
    if app.state.identity.auth.subject_string().as_deref() != Some(action.account_user_id.as_str())
        || app
            .state
            .discovery
            .session(id)
            .and_then(|record| record.incarnation_id)
            != Some(action.incarnation_id)
    {
        app.state.record_log(format!(
            "{} ignored permission action for a retired remote incarnation",
            id.short()
        ));
        return SessionRelayCommandDispatchOutcome::Rejected;
    }
    let command = SessionRelayCommand::PermissionDecision {
        action_id: action.action_id.clone(),
        request_id: action.request_id.clone(),
        request_generation: action.request_generation,
        decision: action.decision.clone(),
    };
    send_session_relay_command(
        app,
        id,
        command,
        "participant.permissionDecision",
        Some(PermissionActionCorrelation {
            action_id: &action.action_id,
            tool_use_id: &action.request_id,
        }),
    )
}

pub(crate) fn replay_remote_permission_actions(app: &mut Runtime, id: SessionId) {
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
    let replayable = app
        .remote_permission_actions
        .replayable_for_incarnation(&account_user_id, id, active_incarnation_id)
        .into_iter()
        .filter(|action| {
            app.state.agent_intel.permission_decisions.visible_identity(
                &crate::agent_intel::permission_decision_registry::PendingKey {
                    session_id: id,
                    session_incarnation_id: active_incarnation_id,
                    tool_use_id: action.request_id.clone(),
                },
                action.request_generation,
            )
        })
        .collect::<Vec<_>>();
    for action in replayable {
        if dispatch_remote_permission_action(app, id, &action)
            != SessionRelayCommandDispatchOutcome::Enqueued
        {
            break;
        }
    }
}
