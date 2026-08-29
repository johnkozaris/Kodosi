use crate::{
    AuthEvent, AuthRequiredReason, Result,
    runtime::{self, session_catalog},
    runtime_event_bus::RuntimeEventSender,
};
use kodosi_domain::auth::AuthState;

use super::catalog::publish_state_snapshot;

#[derive(Debug, Clone, Copy)]
pub(crate) enum AuthFlushPlacement {
    BeforeHooks,
    BeforeAuthState,
    AfterAuthState,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum CatalogFlush {
    Skip,
    Check,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RuntimeFlushOptions {
    pub(crate) force_snapshot: bool,
    pub(crate) force_auth_state: bool,
    pub(crate) catalog: CatalogFlush,
    pub(crate) auth_flush: AuthFlushPlacement,
}

impl RuntimeFlushOptions {
    pub(crate) const fn standard(force_snapshot: bool) -> Self {
        Self {
            force_snapshot,
            force_auth_state: force_snapshot,
            catalog: CatalogFlush::Check,
            auth_flush: AuthFlushPlacement::BeforeHooks,
        }
    }

    pub(crate) const fn maintenance(force_snapshot: bool) -> Self {
        Self {
            force_snapshot,
            force_auth_state: force_snapshot,
            catalog: if force_snapshot {
                CatalogFlush::Check
            } else {
                CatalogFlush::Skip
            },
            auth_flush: AuthFlushPlacement::BeforeHooks,
        }
    }

    pub(crate) const fn auth_command(
        force_snapshot: bool,
        force_auth_state: bool,
        result_failed: bool,
    ) -> Self {
        Self {
            force_snapshot,
            force_auth_state,
            catalog: CatalogFlush::Check,
            auth_flush: if result_failed {
                AuthFlushPlacement::AfterAuthState
            } else {
                AuthFlushPlacement::BeforeAuthState
            },
        }
    }
}

pub(crate) async fn flush_runtime_outputs(
    app: &mut runtime::Runtime,
    tx: &RuntimeEventSender,
    last_signal: &mut Option<session_catalog::RefreshFingerprint>,
    last_auth_event: &mut Option<AuthEvent>,
    options: RuntimeFlushOptions,
) -> Result<()> {
    publish_pending_runtime_events(app, tx).await?;
    publish_pending_session_events(app, tx).await?;
    publish_pending_friends_events(app, tx).await?;
    publish_pending_devices_events(app, tx).await?;
    publish_pending_trust_events(app, tx).await?;
    publish_pending_room_events(app, tx).await?;
    if matches!(options.auth_flush, AuthFlushPlacement::BeforeHooks) {
        publish_pending_auth_events(app, tx).await?;
    }
    publish_pending_agent_events(app, tx).await?;
    if matches!(options.auth_flush, AuthFlushPlacement::BeforeAuthState) {
        publish_pending_auth_events(app, tx).await?;
    }
    let auth_state_published =
        publish_auth_state(app, tx, last_auth_event, options.force_auth_state).await?;
    if auth_state_published {
        publish_recovered_room_mutations(app, tx).await?;
        publish_recovered_access_mutations(app, tx).await?;
    }
    if matches!(options.auth_flush, AuthFlushPlacement::AfterAuthState) {
        publish_pending_auth_events(app, tx).await?;
    }
    let catalog_refresh = app.state.catalog_refresh;
    let should_publish_catalog = catalog_refresh != runtime::state::CatalogRefreshState::Idle
        || match options.catalog {
            CatalogFlush::Skip => false,
            CatalogFlush::Check => true,
        };
    if should_publish_catalog {
        let replay_catalog = catalog_refresh == runtime::state::CatalogRefreshState::ReplayPending;
        publish_state_snapshot(app, tx, last_signal, options.force_snapshot, replay_catalog)
            .await?;
        if catalog_refresh != runtime::state::CatalogRefreshState::Idle {
            app.state.catalog_refresh = runtime::state::CatalogRefreshState::Idle;
        }
    }
    Ok(())
}

pub(crate) async fn publish_pending_runtime_events(
    app: &mut runtime::Runtime,
    tx: &RuntimeEventSender,
) -> Result<()> {
    for message in app.state.runtime_outbox.drain_terminal_control() {
        tx.send_terminal_control(message).await?;
    }
    Ok(())
}

pub(crate) async fn publish_pending_session_events(
    app: &mut runtime::Runtime,
    tx: &RuntimeEventSender,
) -> Result<()> {
    let (account_user_id, account_epoch) = app.state.identity.event_context();
    for message in app.state.runtime_outbox.drain_sessions() {
        tx.send_session(account_user_id.clone(), account_epoch, message)
            .await?;
    }
    Ok(())
}

pub(crate) async fn publish_pending_friends_events(
    app: &mut runtime::Runtime,
    tx: &RuntimeEventSender,
) -> Result<()> {
    let (account_user_id, account_epoch) = app.state.identity.event_context();
    for message in app.state.runtime_outbox.drain_friends() {
        tx.send_friends(account_user_id.clone(), account_epoch, message)
            .await?;
    }
    Ok(())
}

pub(crate) async fn publish_pending_devices_events(
    app: &mut runtime::Runtime,
    tx: &RuntimeEventSender,
) -> Result<()> {
    let (account_user_id, account_epoch) = app.state.identity.event_context();
    for message in app.state.runtime_outbox.drain_devices() {
        tx.send_devices(account_user_id.clone(), account_epoch, message)
            .await?;
    }
    Ok(())
}

pub(crate) async fn publish_pending_trust_events(
    app: &mut runtime::Runtime,
    tx: &RuntimeEventSender,
) -> Result<()> {
    let (account_user_id, account_epoch) = app.state.identity.event_context();
    for message in app.state.runtime_outbox.drain_trust() {
        tx.send_trust(account_user_id.clone(), account_epoch, message)
            .await?;
    }
    Ok(())
}

pub(crate) async fn publish_pending_room_events(
    app: &mut runtime::Runtime,
    tx: &RuntimeEventSender,
) -> Result<()> {
    let Some(origin) = app.state.identity.current_account_event_origin() else {
        if app.state.runtime_outbox.drain_room().is_empty() {
            return Ok(());
        }
        return Err(crate::AppError::Unauthorized);
    };
    for message in app.state.runtime_outbox.drain_room() {
        tx.send_room(
            Some(origin.account_user_id.clone()),
            origin.epoch.value(),
            message,
        )
        .await?;
    }
    Ok(())
}

pub(crate) async fn publish_pending_auth_events(
    app: &mut runtime::Runtime,
    tx: &RuntimeEventSender,
) -> Result<()> {
    for message in app.state.runtime_outbox.drain_auth() {
        tx.send_auth(message).await?;
    }
    Ok(())
}

async fn publish_pending_agent_intel_events(
    app: &mut runtime::Runtime,
    tx: &RuntimeEventSender,
) -> Result<()> {
    let (account_user_id, account_epoch) = app.state.identity.event_context();
    for message in app.state.runtime_outbox.drain_agent_intel() {
        tx.send_agent_intel(account_user_id.clone(), account_epoch, message)
            .await?;
    }
    Ok(())
}

pub(crate) async fn publish_pending_agent_events(
    app: &mut runtime::Runtime,
    tx: &RuntimeEventSender,
) -> Result<()> {
    publish_pending_agent_intel_events(app, tx).await
}

pub(crate) async fn publish_auth_state(
    app: &runtime::Runtime,
    tx: &RuntimeEventSender,
    last_auth_event: &mut Option<AuthEvent>,
    force: bool,
) -> Result<bool> {
    let event = auth_event_for_state(
        &app.state.identity.auth,
        app.state.identity.account_epoch().value(),
    );
    if force || last_auth_event.as_ref() != Some(&event) {
        tx.send_auth(event.clone()).await?;
        *last_auth_event = Some(event);
        return Ok(true);
    }
    Ok(false)
}

pub(crate) async fn publish_recovered_access_mutations(
    app: &mut runtime::Runtime,
    tx: &RuntimeEventSender,
) -> Result<()> {
    let Some(origin) = app.state.identity.current_account_event_origin() else {
        return Ok(());
    };
    let mutations = match app.access_mutations.entries(&origin.account_user_id) {
        Ok(mutations) => mutations,
        Err(error) => {
            tracing::error!(%error, "session access mutation recovery is unavailable");
            return Ok(());
        }
    };
    for mutation in mutations {
        let pending = !matches!(
            mutation.state,
            runtime::access_mutations::PreparedSessionAccessMutationState::Terminal(_)
        );
        let event = runtime::sharing::recovered_access_mutation_event(mutation.clone());
        tx.send_session(
            Some(origin.account_user_id.clone()),
            origin.epoch.value(),
            event,
        )
        .await?;
        if pending {
            runtime::sharing::queue_access_mutation_reconciliation(app, &mutation);
        }
    }
    Ok(())
}

pub(crate) async fn publish_recovered_room_mutations(
    app: &runtime::Runtime,
    tx: &RuntimeEventSender,
) -> Result<()> {
    let Some(origin) = app.state.identity.current_account_event_origin() else {
        return Ok(());
    };
    let mutations = match app.room_mutations.entries(&origin.account_user_id) {
        Ok(mutations) => mutations,
        Err(error) => {
            tracing::error!(%error, "room mutation recovery is unavailable");
            return Ok(());
        }
    };
    for mutation in mutations {
        let (status, entity_id, message) = match mutation.state {
            runtime::room_mutations::PreparedRoomMutationState::Terminal(terminal) => (
                Some(terminal.status.action_status()),
                terminal.entity_id,
                terminal.message,
            ),
            _ => (None, None, None),
        };
        tx.send_room(
            Some(origin.account_user_id.clone()),
            origin.epoch.value(),
            crate::RoomEvent::MutationRecovered {
                request_id: mutation.mutation_id.to_string(),
                operation: mutation.target.kind().to_owned(),
                room_id: mutation.target.room_id().to_owned(),
                fingerprint: mutation.fingerprint,
                status,
                entity_id,
                message,
            },
        )
        .await?;
    }
    Ok(())
}

pub(super) fn auth_event_for_state(auth: &AuthState, account_epoch: u64) -> AuthEvent {
    match auth {
        AuthState::Authenticated { subject, .. } => AuthEvent::Ready {
            user_id: subject.as_ref().map(ToString::to_string),
            account_epoch,
        },
        AuthState::WaitingForApproval {
            user_code,
            verification_uri,
        } => AuthEvent::DeviceCode {
            user_code: user_code.clone(),
            verification_uri: verification_uri.clone(),
        },
        AuthState::Finalizing { .. } => AuthEvent::Finalizing,
        AuthState::Expiring { .. } | AuthState::Expired { .. } => AuthEvent::Required {
            reason: AuthRequiredReason::Expired,
            account_epoch,
        },
        AuthState::SignedOut | AuthState::LoggingOut { .. } => AuthEvent::Required {
            reason: AuthRequiredReason::SignedOut,
            account_epoch,
        },
    }
}
