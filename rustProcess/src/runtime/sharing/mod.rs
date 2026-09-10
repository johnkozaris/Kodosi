mod backend_session;
#[cfg(test)]
mod tests;

use std::{collections::HashMap, future::Future, pin::Pin};

use tokio::sync::mpsc;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{
    AppError, Result,
    identity_core::device_list_pin_store::{PinContext, PinVerdict},
    sharing::{
        scope::{SelectedRoom, SessionKeyDistributionResult},
        shared_session_registry::{
            FRAME_NONCE_BLOCK_SIZE, FRAME_REVISION_BLOCK_SIZE, FrameNonceBlock, FrameRevisionBlock,
            SharedRoom,
        },
    },
    terminal_transport::{
        TerminalCapability, TerminalControlFrame, TerminalSurface, hub::SubscriberHandle,
    },
};
use kodosi_backend_client::{
    api::{
        BackendSessionAccessMutationReceipt, SessionAccessMutationKindDto, SessionKeyBlobEntry,
        StoreSessionKeyBlobsRequest,
    },
    control::ControlTrustEntry,
    crypto,
    http_client::KeyGenerationClaimOutcome,
    relay,
    session_relay::{RemoteRelayMode, SessionRelayCommand},
};
use kodosi_domain::{
    ids::SessionId,
    lifecycle::ConnectionState,
    permissions::{AccessLevel, SessionCapabilities, ShareScope},
    session::{SessionRole, SessionState},
};

use super::Runtime;
use super::access_mutations::{
    PreparedSessionAccessMutation, PreparedSessionAccessMutationState, SessionAccessMutationTarget,
    SessionAccessMutationTerminal, SessionAccessMutationTerminalStatus,
};
use super::pending_work::BackendScopeRestore;
use backend_session::{
    resolve_local_shared_backend_session_identity, update_backend_session_scope,
};

pub(crate) use backend_session::patch_scope_id_reconciled;
pub(crate) use backend_session::{
    access_grant_entry_from_dto, prepare_backend_session_mutation, update_backend_session_title,
};

pub(crate) const RELAY_BRIDGE_CAPACITY: usize = 256;
const RELAY_BRIDGE_RESUBSCRIBE_DELAY: std::time::Duration = std::time::Duration::from_millis(50);
pub(crate) const SEMANTIC_RECEIPT_CAPACITY: usize = 32;
pub(crate) const ACTION_RESULT_CAPACITY: usize = 32;
pub(crate) const FENCE_COMPLETION_CAPACITY: usize = 32;

fn queue_relay_event(
    relay_event_tx: &mpsc::Sender<relay::HostRelayTerminalEvent>,
    event: relay::HostRelayTerminalEvent,
) -> bool {
    match relay_event_tx.try_send(event) {
        Ok(()) => true,
        Err(mpsc::error::TrySendError::Full(_) | mpsc::error::TrySendError::Closed(_)) => false,
    }
}

enum RelayHubSubscriptionOutcome {
    Detached,
    Stop,
}

async fn flush_closed_relay_stream(
    handle: &mut SubscriberHandle,
    relay_event_tx: &mpsc::Sender<relay::HostRelayTerminalEvent>,
    cancellation: &tokio_util::sync::CancellationToken,
    final_sequence: u64,
) -> RelayHubSubscriptionOutcome {
    while let Some(frame) = handle.data_rx.recv().await {
        let event = relay::HostRelayTerminalEvent::Raw {
            sequence: frame.sequence,
            bytes: frame.bytes,
        };
        if tokio::select! {
            biased;
            () = cancellation.cancelled() => return RelayHubSubscriptionOutcome::Stop,
            result = relay_event_tx.send(event) => result,
        }
        .is_err()
        {
            return RelayHubSubscriptionOutcome::Stop;
        }
    }
    let close = relay::HostRelayTerminalEvent::Closed { final_sequence };
    tokio::select! {
        biased;
        () = cancellation.cancelled() => {}
        _ = relay_event_tx.send(close) => {}
    }
    RelayHubSubscriptionOutcome::Stop
}

async fn drain_relay_hub_subscription(
    handle: &mut SubscriberHandle,
    relay_event_tx: &mpsc::Sender<relay::HostRelayTerminalEvent>,
    cancellation: &tokio_util::sync::CancellationToken,
) -> RelayHubSubscriptionOutcome {
    loop {
        tokio::select! {
            biased;
            () = cancellation.cancelled() => return RelayHubSubscriptionOutcome::Stop,
            maybe_control = handle.control_rx.recv() => {
                let Some(control) = maybe_control else {
                    return RelayHubSubscriptionOutcome::Detached;
                };
                let event = match control {
                    TerminalControlFrame::Closed { final_sequence, .. } => {
                        return flush_closed_relay_stream(
                            handle,
                            relay_event_tx,
                            cancellation,
                            final_sequence,
                        )
                        .await;
                    }
                    TerminalControlFrame::SemanticCheckpoint { .. } => {
                        relay::HostRelayTerminalEvent::ForceCheckpoint
                    }
                    TerminalControlFrame::Resize { .. } => {
                        relay::HostRelayTerminalEvent::ForcePresentation
                    }
                };
                let delivery_required = !matches!(
                    event,
                    relay::HostRelayTerminalEvent::ForcePresentation
                );
                if !queue_relay_event(relay_event_tx, event) && delivery_required {
                    return RelayHubSubscriptionOutcome::Detached;
                }
            }
            maybe_frame = handle.data_rx.recv() => {
                let Some(frame) = maybe_frame else {
                    return RelayHubSubscriptionOutcome::Detached;
                };
                if !queue_relay_event(
                    relay_event_tx,
                    relay::HostRelayTerminalEvent::Raw {
                        sequence: frame.sequence,
                        bytes: frame.bytes,
                    },
                ) {
                    return RelayHubSubscriptionOutcome::Detached;
                }
            }
        }
    }
}

pub(crate) async fn run_relay_hub_bridge(
    mut hub: crate::terminal_transport::hub::SessionHub,
    id: SessionId,
    mut handle: SubscriberHandle,
    relay_event_tx: mpsc::Sender<relay::HostRelayTerminalEvent>,
    cancellation: tokio_util::sync::CancellationToken,
) {
    loop {
        let connection_id = handle.connection_id;
        let outcome =
            drain_relay_hub_subscription(&mut handle, &relay_event_tx, &cancellation).await;
        hub.unregister(id, connection_id);
        if matches!(outcome, RelayHubSubscriptionOutcome::Stop) {
            return;
        }

        tracing::warn!(
            session_id = %id,
            connection = ?connection_id,
            "relay terminal hub subscriber detached; resubscribing"
        );
        tokio::select! {
            () = cancellation.cancelled() => return,
            () = tokio::time::sleep(RELAY_BRIDGE_RESUBSCRIBE_DELAY) => {}
        }

        let Some(next_handle) =
            hub.register(id, TerminalSurface::RelayHost, TerminalCapability::ReadOnly)
        else {
            return;
        };
        handle = next_handle;

        if !queue_relay_event(
            &relay_event_tx,
            relay::HostRelayTerminalEvent::ForceCheckpoint,
        ) {
            hub.unregister(id, handle.connection_id);
            return;
        }

        let _ = queue_relay_event(
            &relay_event_tx,
            relay::HostRelayTerminalEvent::ForcePresentation,
        );
    }
}

pub(crate) type SharingMaintenanceFuture<'a> =
    Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

pub(crate) trait SharingMaintenanceBackend: Sync {
    fn restore_scope<'a>(
        &'a self,
        app: &'a mut Runtime,
        repair: &'a BackendScopeRestore,
    ) -> SharingMaintenanceFuture<'a>;

    fn rotate_key<'a>(
        &'a self,
        app: &'a mut Runtime,
        id: SessionId,
    ) -> SharingMaintenanceFuture<'a>;
}

pub(crate) struct LiveSharingMaintenanceBackend;

impl SharingMaintenanceBackend for LiveSharingMaintenanceBackend {
    fn restore_scope<'a>(
        &'a self,
        app: &'a mut Runtime,
        repair: &'a BackendScopeRestore,
    ) -> SharingMaintenanceFuture<'a> {
        Box::pin(restore_backend_scope(app, repair))
    }

    fn rotate_key<'a>(
        &'a self,
        app: &'a mut Runtime,
        id: SessionId,
    ) -> SharingMaintenanceFuture<'a> {
        Box::pin(rotate_session_key(app, id))
    }
}

fn ensure_locally_hosted_sharing_mutation(
    app: &Runtime,
    id: SessionId,
    operation: &str,
) -> Result<()> {
    if app.state.local.sessions.record(id).is_some() {
        return Ok(());
    }
    if app.state.discovery.session(id).is_some() {
        return Err(AppError::Unsupported {
            reason: format!(
                "cannot {operation} for session {} because it is hosted by another device; use the host device",
                id.short()
            ),
        });
    }
    Err(AppError::NoActiveSession)
}

fn ensure_local_access_mutation_live(
    app: &Runtime,
    id: SessionId,
    expected_runtime_incarnation_id: Uuid,
    operation: &str,
) -> Result<()> {
    ensure_locally_hosted_sharing_mutation(app, id, operation)?;
    let record = app
        .state
        .local
        .sessions
        .record(id)
        .ok_or(AppError::NoActiveSession)?;
    if record.local_incarnation_id != expected_runtime_incarnation_id
        || matches!(
            record.summary.state,
            SessionState::Stopping | SessionState::Stopped | SessionState::Failed
        )
    {
        return Err(AppError::NoActiveSession);
    }
    Ok(())
}

pub(crate) fn scope_change_caller_budget(new_scope: ShareScope) -> std::time::Duration {
    if new_scope == ShareScope::JustMe {
        std::time::Duration::from_secs(5)
    } else {
        std::time::Duration::from_secs(505)
    }
}

pub(crate) const fn scope_change_verdict_delivery_budget() -> std::time::Duration {
    std::time::Duration::from_secs(5)
}

async fn claim_or_reconcile_key_generation(
    app: &mut Runtime,
    backend_session_id: &str,
    backend_incarnation_id: &Uuid,
    expected_current_generation: u32,
) -> Result<u32> {
    const MAX_RECONCILIATION_ATTEMPTS: usize = 4;
    let mut expected_generation = expected_current_generation;
    let mut last_failure = None;
    for _ in 0..MAX_RECONCILIATION_ATTEMPTS {
        match app
            .backend
            .claim_next_key_generation(
                backend_session_id,
                backend_incarnation_id,
                expected_generation,
            )
            .await
        {
            Ok(KeyGenerationClaimOutcome::Claimed(generation)) => return Ok(generation),
            Ok(KeyGenerationClaimOutcome::GenerationChanged(current_generation)) => {
                expected_generation = current_generation;
            }
            Err(claim_error) => {
                let claim_error: AppError = claim_error.into();
                let current_generation = app
                    .backend
                    .fetch_current_key_generation(backend_session_id, backend_incarnation_id)
                    .await
                    .map_err(|query_error| AppError::Unsupported {
                        reason: format!(
                            "session key generation claim was indeterminate ({claim_error}); \
                             current generation reconciliation failed: {query_error}"
                        ),
                    })?;
                app.state.record_log(format!(
                    "reconciled indeterminate key claim for {backend_session_id} at generation {current_generation}; claiming a fresh fence"
                ));
                expected_generation = current_generation;
                last_failure = Some(claim_error);
            }
        }
    }

    Err(AppError::Unsupported {
        reason: format!(
            "session key generation kept changing during reconciliation from generation \
             {expected_current_generation}; last indeterminate failure: {}",
            last_failure.map_or_else(|| "none".to_owned(), |error| error.to_string())
        ),
    })
}

pub(crate) fn retire_stale_backend_scope_repair(app: &mut Runtime, repair: &BackendScopeRestore) {
    let id = repair.session_id;
    let had_local_host = app.state.local.sessions.record(id).is_some();
    app.state.unshare_session_locally(id);
    app.state.record_log(if had_local_host {
        format!(
            "{} backend incarnation is stale; cleared obsolete local sharing state",
            id.short()
        )
    } else {
        format!("{} discarded obsolete nonlocal sharing repair", id.short())
    });
}

pub(crate) async fn restore_backend_scope(
    app: &mut Runtime,
    repair: &BackendScopeRestore,
) -> Result<()> {
    ensure_locally_hosted_sharing_mutation(app, repair.session_id, "restore sharing scope")?;
    update_backend_session_scope(
        app,
        &repair.backend_session_id,
        repair.incarnation_id,
        repair.scope,
        repair.room.as_ref(),
    )
    .await?;
    Ok(())
}

pub(crate) fn resume_after_backend_scope_restore(app: &mut Runtime, repair: &BackendScopeRestore) {
    let id = repair.session_id;
    if app.state.local.sessions.record(id).is_none() {
        app.state
            .record_log(format!("{} remote backend scope repaired", id.short()));
        return;
    }
    if repair.requires_key_rotation {
        app.state.pending_work.queue_host_key_rotation(id);
        app.state.record_log(format!(
            "{} backend scope repaired — rotating session key before relay restart",
            id.short()
        ));
        return;
    }
    if let Err(error) = app.start_maintenance_relay_prepare_worker(id) {
        app.state
            .record_log(format!("relay restart after scope repair failed: {error}"));
    }
}

fn ensure_scope_repair_complete(app: &Runtime, id: SessionId, operation: &str) -> Result<()> {
    if app.state.pending_work.backend_scope_restore(id).is_some() {
        return Err(AppError::Unsupported {
            reason: format!(
                "cannot {operation} for session {} while backend sharing scope repair is required",
                id.short()
            ),
        });
    }
    Ok(())
}

pub(super) fn persist_access_mutation_state(
    app: &mut Runtime,
    prepared: &PreparedSessionAccessMutation,
    state: PreparedSessionAccessMutationState,
) -> Result<PreparedSessionAccessMutation> {
    let mut updated = prepared.clone();
    updated.state = state;
    app.access_mutations.put(updated.clone())?;
    Ok(updated)
}

fn prepare_access_mutation(
    app: &mut Runtime,
    runtime_session_id: SessionId,
    expected_runtime_incarnation_id: Uuid,
    backend_session_id: String,
    backend_incarnation_id: Uuid,
    mutation_id: Uuid,
    target: SessionAccessMutationTarget,
) -> Result<(PreparedSessionAccessMutation, bool)> {
    let account_user_id = app
        .state
        .identity
        .auth
        .subject_string()
        .ok_or(AppError::Unauthorized)?;
    let prepared = PreparedSessionAccessMutation::new(
        mutation_id,
        account_user_id.clone(),
        app.state.identity.account_epoch().value(),
        runtime_session_id,
        expected_runtime_incarnation_id,
        backend_session_id,
        backend_incarnation_id,
        target,
    )?;
    if let Some(existing) = app
        .access_mutations
        .get(&account_user_id, mutation_id)?
        .cloned()
    {
        if existing.fingerprint != prepared.fingerprint
            || existing.runtime_session_id != prepared.runtime_session_id
            || existing.expected_runtime_incarnation_id != prepared.expected_runtime_incarnation_id
        {
            return Err(AppError::Unsupported {
                reason: "mutationId already names a different session access target".to_owned(),
            });
        }
        return Ok((existing, false));
    }
    let attempting = PreparedSessionAccessMutation {
        state: PreparedSessionAccessMutationState::Attempting,
        ..prepared
    };
    app.access_mutations.put(attempting.clone())?;
    Ok((attempting, true))
}

pub(super) fn mark_access_mutation_terminal(
    app: &mut Runtime,
    prepared: &PreparedSessionAccessMutation,
    status: SessionAccessMutationTerminalStatus,
    message: Option<String>,
) -> Result<()> {
    persist_access_mutation_state(
        app,
        prepared,
        PreparedSessionAccessMutationState::Terminal(SessionAccessMutationTerminal::new(
            status, message,
        )),
    )?;
    Ok(())
}

#[expect(
    clippy::too_many_lines,
    reason = "one target match resumes all exact local access effects under shared incarnation fences"
)]
pub(crate) async fn resume_access_mutation_effects(
    app: &mut Runtime,
    prepared: &PreparedSessionAccessMutation,
) -> Result<()> {
    let current_user = app
        .state
        .identity
        .auth
        .subject_string()
        .ok_or(AppError::Unauthorized)?;
    if current_user != prepared.account_user_id {
        return Err(AppError::Unsupported {
            reason: "session access mutation account changed during reconciliation".to_owned(),
        });
    }
    let current_runtime_incarnation = app
        .state
        .local
        .sessions
        .record(prepared.runtime_session_id)
        .map(|record| record.local_incarnation_id)
        .or_else(|| {
            app.state
                .discovery
                .session(prepared.runtime_session_id)
                .and_then(|record| record.incarnation_id)
        });
    if current_runtime_incarnation
        .is_some_and(|incarnation| incarnation != prepared.expected_runtime_incarnation_id)
    {
        app.state.record_log(format!(
            "retired recovered access mutation {} because its runtime incarnation was superseded",
            prepared.mutation_id
        ));
        return Ok(());
    }
    match &prepared.target {
        SessionAccessMutationTarget::Grant {
            actor_user_id,
            access_level,
            expires_at_unix_ms,
        } => {
            let Some(shared) = app
                .state
                .sharing
                .shared_sessions
                .get_mut(prepared.runtime_session_id)
            else {
                app.state.record_log(format!(
                    "retired recovered grant {} because no key-bearing shared authority remains",
                    prepared.mutation_id
                ));
                return Ok(());
            };
            let expires_at = time::OffsetDateTime::from_unix_timestamp_nanos(
                i128::from(*expires_at_unix_ms) * 1_000_000,
            )
            .map_err(|error| AppError::InvalidBackendData {
                field: "pendingSessionAccessMutations.expiresAt".to_owned(),
                reason: error.to_string(),
            })?;
            if *shared.backend_incarnation_id() != prepared.backend_incarnation_id
                || shared.backend_session_id() != prepared.backend_session_id
            {
                return Err(AppError::NoActiveSession);
            }
            shared.grant_user_at(actor_user_id.to_string(), *access_level, expires_at);
            redistribute_existing_session_key(app, prepared.runtime_session_id).await?;
        }
        SessionAccessMutationTarget::Revoke { actor_user_id } => {
            let Some(shared) = app
                .state
                .sharing
                .shared_sessions
                .get_mut(prepared.runtime_session_id)
            else {
                app.state.record_log(format!(
                    "retired recovered revoke {} because no key-bearing shared authority remains",
                    prepared.mutation_id
                ));
                return Ok(());
            };
            if *shared.backend_incarnation_id() != prepared.backend_incarnation_id
                || shared.backend_session_id() != prepared.backend_session_id
            {
                return Err(AppError::NoActiveSession);
            }
            shared.revoke_user(&actor_user_id.to_string());
            rotate_session_key(app, prepared.runtime_session_id).await?;
        }
        SessionAccessMutationTarget::Leave => {
            let current_incarnation = app
                .state
                .discovery
                .session(prepared.runtime_session_id)
                .and_then(|record| record.incarnation_id);
            if current_incarnation == Some(prepared.backend_incarnation_id) {
                if let Some(shutdown) = app
                    .state
                    .take_session_relay_for_shutdown(prepared.runtime_session_id)
                {
                    tokio::spawn(shutdown.run());
                }
                app.reject_pending_remote_resizes(
                    prepared.runtime_session_id,
                    None,
                    "remote terminal access ended before applying this size",
                );
                app.remote_terminal.forget(prepared.runtime_session_id);
                app.client_focus.forget(prepared.runtime_session_id);
                app.terminal_hub.end_session(
                    prepared.runtime_session_id,
                    &crate::terminal_transport::TerminalCloseReason::AuthRevoked,
                );
                let _ = app
                    .state
                    .discovery
                    .remove_session(prepared.runtime_session_id);
                let visible_remote_ids = app.state.discovery.non_hidden_remote_session_ids();
                app.state.shelf.sync_remote_sessions(&visible_remote_ids);
            }
            if let Err(error) = app.hidden_session_store.set_hidden(
                &prepared.account_user_id,
                prepared.runtime_session_id,
                false,
            ) {
                app.state.record_log(format!(
                    "{} recovered leave but failed to clear obsolete hidden state: {error}",
                    prepared.runtime_session_id.short()
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn recovered_access_mutation_event(
    prepared: PreparedSessionAccessMutation,
) -> crate::SessionEvent {
    let (outcome, message) = match prepared.state {
        PreparedSessionAccessMutationState::Terminal(terminal) => {
            (Some(terminal.status.outcome()), terminal.message)
        }
        _ => (None, None),
    };
    crate::SessionEvent::AccessMutationRecovered {
        mutation_id: prepared.mutation_id.to_string(),
        session_id: prepared.runtime_session_id.to_string(),
        expected_runtime_incarnation_id: prepared.expected_runtime_incarnation_id.to_string(),
        originating_account_epoch: prepared.originating_account_epoch,
        fingerprint: prepared.fingerprint,
        kind: prepared.target.kind(),
        actor_user_id: prepared.target.actor_user_id(),
        access_level: prepared.target.access_level(),
        expires_at: prepared.target.expires_at(),
        outcome,
        message,
    }
}

pub(crate) fn acknowledge_access_mutation(
    app: &mut Runtime,
    mutation_id: Uuid,
    fingerprint: &str,
) -> Result<bool> {
    let account = app
        .state
        .identity
        .auth
        .subject_string()
        .ok_or(AppError::Unauthorized)?;
    let Some(prepared) = app.access_mutations.get(&account, mutation_id)?.cloned() else {
        return Ok(false);
    };
    if prepared.fingerprint != fingerprint
        || !matches!(
            prepared.state,
            PreparedSessionAccessMutationState::Terminal(_)
        )
    {
        return Err(AppError::Unsupported {
            reason: "session access mutation acknowledgment does not match a terminal result"
                .to_owned(),
        });
    }
    app.access_mutations.remove(&account, mutation_id)?;
    Ok(true)
}

pub(crate) fn prepare_leave_access_mutation(
    app: &mut Runtime,
    runtime_session_id: SessionId,
    expected_runtime_incarnation_id: Uuid,
    backend_session_id: String,
    backend_incarnation_id: Uuid,
    mutation_id: Uuid,
) -> Result<(PreparedSessionAccessMutation, bool)> {
    ensure_access_mutation_ready(app)?;
    prepare_access_mutation(
        app,
        runtime_session_id,
        expected_runtime_incarnation_id,
        backend_session_id,
        backend_incarnation_id,
        mutation_id,
        SessionAccessMutationTarget::Leave,
    )
}

#[derive(Debug)]
pub(crate) enum AccessMutationSettlement {
    Applied,
    Unknown(String),
}

pub(crate) type AccessMutationTarget = SessionAccessMutationTarget;

fn access_mutation_write_is_indeterminate(
    error: &kodosi_backend_client::BackendClientError,
) -> bool {
    error.is_indeterminate_write()
        || matches!(
            error,
            kodosi_backend_client::BackendClientError::HttpProblem {
                status: 500..=599,
                ..
            }
        )
        || matches!(
            error,
            kodosi_backend_client::BackendClientError::HttpProblem {
                status: 409,
                code: Some(code),
                ..
            } if code == "CONCURRENT_MODIFICATION"
        )
}

pub(crate) async fn dispatch_access_mutation<F, Fut>(
    backend: &kodosi_backend_client::http_client::BackendHttpClient,
    backend_session_id: &str,
    backend_incarnation_id: Uuid,
    mutation_id: Uuid,
    target: &AccessMutationTarget,
    dispatch: F,
) -> Result<AccessMutationSettlement>
where
    F: Fn() -> Fut,
    Fut: Future<Output = kodosi_backend_client::Result<()>>,
{
    match dispatch().await {
        Ok(()) => Ok(AccessMutationSettlement::Applied),
        Err(first_error) if access_mutation_write_is_indeterminate(&first_error) => {
            match dispatch().await {
                Ok(()) => Ok(AccessMutationSettlement::Applied),
                Err(second_error) if access_mutation_write_is_indeterminate(&second_error) => {
                    reconcile_access_mutation_receipt(
                        backend,
                        backend_session_id,
                        backend_incarnation_id,
                        mutation_id,
                        target,
                        &second_error,
                    )
                    .await
                }
                Err(error) => Err(error.into()),
            }
        }
        Err(error) => Err(error.into()),
    }
}

async fn reconcile_access_mutation_receipt(
    backend: &kodosi_backend_client::http_client::BackendHttpClient,
    backend_session_id: &str,
    backend_incarnation_id: Uuid,
    mutation_id: Uuid,
    target: &AccessMutationTarget,
    write_error: &kodosi_backend_client::BackendClientError,
) -> Result<AccessMutationSettlement> {
    match backend
        .get_session_access_mutation_receipt(
            backend_session_id,
            &backend_incarnation_id,
            &mutation_id,
        )
        .await
    {
        Ok(receipt) => {
            validate_access_mutation_receipt(
                &receipt,
                backend_session_id,
                backend_incarnation_id,
                mutation_id,
                target,
            )?;
            Ok(AccessMutationSettlement::Applied)
        }
        Err(kodosi_backend_client::BackendClientError::NotFound) => {
            Ok(AccessMutationSettlement::Unknown(format!(
                "access mutation {mutation_id} remained indeterminate after exact retry ({write_error}); no receipt is visible yet"
            )))
        }
        Err(reconcile_error) => Ok(AccessMutationSettlement::Unknown(format!(
            "access mutation {mutation_id} remained indeterminate after exact retry ({write_error}); receipt reconciliation failed: {reconcile_error}"
        ))),
    }
}

pub(super) fn validate_access_mutation_receipt(
    receipt: &BackendSessionAccessMutationReceipt,
    backend_session_id: &str,
    backend_incarnation_id: Uuid,
    mutation_id: Uuid,
    target: &AccessMutationTarget,
) -> Result<()> {
    let session_id =
        Uuid::parse_str(backend_session_id).map_err(|error| AppError::InvalidBackendData {
            field: "sessionAccessMutation.sessionId".to_owned(),
            reason: error.to_string(),
        })?;
    let target_matches = match target {
        AccessMutationTarget::Grant {
            actor_user_id,
            access_level,
            expires_at_unix_ms,
        } => {
            let receipt_expiry = receipt.requested_expires_at.as_deref().and_then(|value| {
                time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
                    .ok()
            });
            receipt.kind == SessionAccessMutationKindDto::Grant
                && receipt.target_user_id == Some(*actor_user_id)
                && receipt.access_level == Some(*access_level)
                && receipt_expiry.is_some_and(|receipt_expiry| {
                    receipt_expiry.unix_timestamp_nanos() / 1_000_000
                        == i128::from(*expires_at_unix_ms)
                })
        }
        AccessMutationTarget::Revoke { actor_user_id } => {
            receipt.kind == SessionAccessMutationKindDto::Revoke
                && receipt.target_user_id == Some(*actor_user_id)
                && receipt.access_level.is_none()
                && receipt.requested_expires_at.is_none()
        }
        AccessMutationTarget::Leave => {
            receipt.kind == SessionAccessMutationKindDto::Leave
                && receipt.target_user_id.is_none()
                && receipt.access_level.is_none()
                && receipt.requested_expires_at.is_none()
        }
    };
    if receipt.mutation_id != mutation_id
        || receipt.session_id != session_id
        || receipt.incarnation_id != backend_incarnation_id
        || !target_matches
    {
        return Err(AppError::InvalidBackendData {
            field: "sessionAccessMutationReceipt".to_owned(),
            reason: "receipt does not match the exact requested mutation target".to_owned(),
        });
    }
    Ok(())
}

pub(crate) fn queue_access_mutation_reconciliation(
    app: &mut Runtime,
    prepared: &PreparedSessionAccessMutation,
) {
    app.access_mutation_retry_until
        .remove(&(prepared.account_user_id.clone(), prepared.mutation_id));
    app.last_maintenance_ran = None;
}

pub(crate) fn queue_access_mutation_dispatch(
    app: &mut Runtime,
    prepared: &PreparedSessionAccessMutation,
    dispatch: bool,
) {
    let identity = (prepared.account_user_id.clone(), prepared.mutation_id);
    if dispatch
        && app.access_mutation_in_flight.as_ref() != Some(&identity)
        && !app.access_mutation_dispatch_queue.contains(&identity)
    {
        app.access_mutation_dispatch_queue.push_back(identity);
    } else if !dispatch {
        match &prepared.state {
            PreparedSessionAccessMutationState::Terminal(terminal) => {
                app.state
                    .runtime_outbox
                    .queue_session(crate::SessionEvent::AccessMutationResult {
                        mutation_id: prepared.mutation_id.to_string(),
                        session_id: prepared.runtime_session_id.to_string(),
                        expected_runtime_incarnation_id: prepared
                            .expected_runtime_incarnation_id
                            .to_string(),
                        kind: prepared.target.kind(),
                        actor_user_id: prepared.target.actor_user_id(),
                        access_level: prepared.target.access_level(),
                        expires_at: prepared.target.expires_at(),
                        outcome: terminal.status.outcome(),
                        message: terminal.message.clone(),
                    });
                app.state
                    .runtime_outbox
                    .queue_session(recovered_access_mutation_event(prepared.clone()));
            }
            _ => {
                app.access_mutation_retry_until
                    .remove(&(prepared.account_user_id.clone(), prepared.mutation_id));
            }
        }
    }
    app.last_maintenance_ran = None;
}

pub(crate) fn grant_access(
    app: &mut Runtime,
    id: SessionId,
    expected_runtime_incarnation_id: Uuid,
    mutation_id: Uuid,
    actor_user_id: &str,
    access_level: AccessLevel,
    expires_at: &str,
) -> Result<()> {
    ensure_local_access_mutation_live(app, id, expected_runtime_incarnation_id, "grant access")?;
    let (backend_session_id, backend_incarnation_id) =
        resolve_local_shared_backend_session_identity(app, id)?;
    ensure_access_mutation_ready(app)?;
    let account = app
        .state
        .identity
        .auth
        .subject_string()
        .ok_or(AppError::Unauthorized)?;
    if app.share_transition_active_for_session(&account, id) {
        return Err(AppError::Unsupported {
            reason: format!(
                "session {} already has a sharing change in flight",
                id.short()
            ),
        });
    }
    let actor_user_uuid =
        Uuid::parse_str(actor_user_id).map_err(|error| AppError::InvalidBackendData {
            field: "actorUserId".to_owned(),
            reason: error.to_string(),
        })?;
    let expiry =
        time::OffsetDateTime::parse(expires_at, &time::format_description::well_known::Rfc3339)
            .map_err(|error| AppError::InvalidBackendData {
                field: "expiresAt".to_owned(),
                reason: error.to_string(),
            })?;
    let expires_at_unix_ms =
        i64::try_from(expiry.unix_timestamp_nanos() / 1_000_000).map_err(|_| {
            AppError::InvalidBackendData {
                field: "expiresAt".to_owned(),
                reason: "timestamp is outside millisecond range".to_owned(),
            }
        })?;
    let (prepared, dispatch) = prepare_access_mutation(
        app,
        id,
        expected_runtime_incarnation_id,
        backend_session_id,
        backend_incarnation_id,
        mutation_id,
        AccessMutationTarget::Grant {
            actor_user_id: actor_user_uuid,
            access_level,
            expires_at_unix_ms,
        },
    )?;
    queue_access_mutation_dispatch(app, &prepared, dispatch);
    Ok(())
}

pub(crate) fn revoke_access(
    app: &mut Runtime,
    id: SessionId,
    expected_runtime_incarnation_id: Uuid,
    mutation_id: Uuid,
    actor_user_id: &str,
) -> Result<()> {
    ensure_local_access_mutation_live(app, id, expected_runtime_incarnation_id, "revoke access")?;
    let (backend_session_id, backend_incarnation_id) =
        resolve_local_shared_backend_session_identity(app, id)?;
    ensure_access_mutation_ready(app)?;
    let account = app
        .state
        .identity
        .auth
        .subject_string()
        .ok_or(AppError::Unauthorized)?;
    if app.share_transition_active_for_session(&account, id) {
        return Err(AppError::Unsupported {
            reason: format!(
                "session {} already has a sharing change in flight",
                id.short()
            ),
        });
    }
    let actor_user_id =
        Uuid::parse_str(actor_user_id).map_err(|error| AppError::InvalidBackendData {
            field: "actorUserId".to_owned(),
            reason: error.to_string(),
        })?;
    let (prepared, dispatch) = prepare_access_mutation(
        app,
        id,
        expected_runtime_incarnation_id,
        backend_session_id,
        backend_incarnation_id,
        mutation_id,
        AccessMutationTarget::Revoke { actor_user_id },
    )?;
    queue_access_mutation_dispatch(app, &prepared, dispatch);
    Ok(())
}

fn ensure_access_mutation_ready(app: &Runtime) -> Result<()> {
    if !app.backend.is_configured() {
        return Err(AppError::Unsupported {
            reason: "backend.api is not configured — cannot mutate session access".to_owned(),
        });
    }
    if !app.remote_surfaces_ready() {
        return Err(AppError::Unsupported {
            reason: "remote operations are not ready; retry after reconciliation".to_owned(),
        });
    }
    Ok(())
}

fn ensure_runtime_incarnation(app: &Runtime, id: SessionId, expected: Uuid) -> Result<()> {
    let current = app
        .state
        .local
        .sessions
        .record(id)
        .map(|record| record.local_incarnation_id)
        .or_else(|| {
            app.state
                .discovery
                .session(id)
                .and_then(|record| record.incarnation_id)
        });
    if current == Some(expected) {
        Ok(())
    } else {
        Err(AppError::NoActiveSession)
    }
}

pub(crate) async fn list_access(
    app: &mut Runtime,
    id: SessionId,
    expected_runtime_incarnation_id: Uuid,
) -> Result<(Uuid, Vec<crate::AccessGrantEntry>)> {
    ensure_runtime_incarnation(app, id, expected_runtime_incarnation_id)?;
    let expected_identity = resolve_local_shared_backend_session_identity(app, id)?;
    prepare_backend_session_mutation(app).await?;
    ensure_runtime_incarnation(app, id, expected_runtime_incarnation_id)?;
    let current_identity = resolve_local_shared_backend_session_identity(app, id)?;
    if current_identity != expected_identity {
        return Err(AppError::NoActiveSession);
    }
    let response = app
        .backend
        .list_access(&expected_identity.0, expected_identity.1)
        .await?;
    ensure_runtime_incarnation(app, id, expected_runtime_incarnation_id)?;
    if resolve_local_shared_backend_session_identity(app, id)? != expected_identity
        || response.incarnation_id != expected_identity.1
    {
        return Err(AppError::NoActiveSession);
    }
    Ok((
        expected_runtime_incarnation_id,
        response
            .grants
            .into_iter()
            .map(access_grant_entry_from_dto)
            .collect(),
    ))
}

pub(crate) async fn rotate_session_key(app: &mut Runtime, id: SessionId) -> Result<()> {
    ensure_scope_repair_complete(app, id, "rotate session key")?;
    let record = app
        .state
        .local
        .sessions
        .record(id)
        .ok_or(AppError::NoActiveSession)?;

    let (backend_session_id, backend_incarnation_id, previous_generation) = app
        .state
        .sharing
        .shared_sessions
        .get(id)
        .map(|shared| {
            (
                shared.backend_session_id().to_owned(),
                *shared.backend_incarnation_id(),
                shared.session_key_generation().unwrap_or(0),
            )
        })
        .ok_or_else(|| AppError::Unsupported {
            reason: "cannot rotate key for unshared session".to_owned(),
        })?;
    let _ = record;

    app.state.sharing.host_relays.cancel(id);
    app.state
        .sharing
        .shared_sessions
        .set_session_key(id, None, None);

    let new_key = Zeroizing::new(crypto::generate_session_key()?);
    let key_gen = claim_or_reconcile_key_generation(
        app,
        &backend_session_id,
        &backend_incarnation_id,
        previous_generation,
    )
    .await?;

    let new_key_ref: &crypto::SessionKey = &new_key;
    app.state
        .sharing
        .shared_sessions
        .set_session_key(id, Some(*new_key_ref), Some(key_gen));
    if let Err(error) = distribute_session_key(app, &backend_session_id, &new_key, key_gen).await {
        app.state
            .sharing
            .shared_sessions
            .set_session_key(id, None, None);
        return Err(error);
    }

    if let Err(error) = app.start_maintenance_relay_prepare_worker(id) {
        app.state
            .record_log(format!("relay restart after key rotation failed: {error}"));
    }

    app.state
        .record_log(format!("session key rotated for {}", id.short()));
    Ok(())
}

pub(crate) async fn redistribute_existing_session_key(
    app: &mut Runtime,
    id: SessionId,
) -> Result<SessionKeyDistributionResult> {
    let record = app
        .state
        .local
        .sessions
        .record(id)
        .ok_or(AppError::NoActiveSession)?;
    let _ = record;
    let shared_state =
        app.state
            .sharing
            .shared_sessions
            .get(id)
            .ok_or_else(|| AppError::Unsupported {
                reason: "cannot redistribute key for an unshared session".to_owned(),
            })?;
    let backend_session_id = shared_state.backend_session_id().to_owned();
    let session_key =
        Zeroizing::new(
            *shared_state
                .session_key()
                .ok_or_else(|| AppError::Unsupported {
                    reason: "cannot redistribute a missing session key".to_owned(),
                })?,
        );
    let key_generation =
        shared_state
            .session_key_generation()
            .ok_or_else(|| AppError::Unsupported {
                reason: "shared session is missing current key generation".to_owned(),
            })?;

    let result =
        distribute_session_key(app, &backend_session_id, &session_key, key_generation).await?;

    if result == SessionKeyDistributionResult::Distributed {
        app.state
            .record_log(format!("session keys redistributed for {}", id.short()));
    }
    Ok(result)
}

pub(crate) async fn distribute_session_key(
    app: &mut Runtime,
    backend_session_id: &str,
    session_key: &crypto::SessionKey,
    key_gen: u32,
) -> Result<SessionKeyDistributionResult> {
    let id = app
        .state
        .sharing
        .shared_sessions
        .local_id_for_backend(backend_session_id)
        .ok_or(AppError::NoActiveSession)?;
    ensure_scope_repair_complete(app, id, "distribute session key")?;
    let backend_incarnation_id = shared_backend_incarnation_id(app, id)?;

    let owner_keys = super::identity::device_keys::load_registered_device_keys_required(
        &app.state.identity.auth,
        &app.backend,
        &app.device_key_store,
        &app.pin_store,
    )
    .await?;
    let sender_device_id = owner_keys.device_id.clone();
    let signing_key = owner_keys.signing_key()?;

    let authorized_recipients = owner_authorized_recipients(app, backend_session_id).await?;
    let owner_user_id = app
        .state
        .identity
        .auth
        .subject_string()
        .ok_or(AppError::Unauthorized)?;
    let authorized_devices =
        authorized_devices_for_distribution(app, backend_session_id, &authorized_recipients)
            .await?;
    let (blobs, control_keys) = build_key_distribution_batch(
        app,
        &authorized_devices,
        &authorized_recipients,
        KeyDistributionContext {
            backend_session_id,
            backend_incarnation_id: &backend_incarnation_id,
            session_key,
            key_generation: key_gen,
            sender_device_id: &sender_device_id,
            owner_user_id: &owner_user_id,
            signing_key: &signing_key,
            issued_at_ms: current_epoch_ms(),
        },
    )
    .await?;

    if blobs.is_empty() {
        app.state
            .record_log("no viewer device keys found — session key not distributed yet".to_owned());
        return Ok(SessionKeyDistributionResult::PendingRecipients);
    }

    finalize_key_distribution(
        app,
        backend_session_id,
        backend_incarnation_id,
        blobs,
        control_keys,
        sender_device_id,
    )
    .await?;
    app.state
        .record_log("session keys distributed to viewer devices".to_owned());
    Ok(SessionKeyDistributionResult::Distributed)
}

struct KeyDistributionContext<'a> {
    backend_session_id: &'a str,
    backend_incarnation_id: &'a Uuid,
    session_key: &'a crypto::SessionKey,
    key_generation: u32,
    sender_device_id: &'a str,
    owner_user_id: &'a str,
    signing_key: &'a aws_lc_rs::signature::PqdsaKeyPair,
    issued_at_ms: u64,
}

type KeyDistributionBatch = (
    Vec<SessionKeyBlobEntry>,
    HashMap<(String, String), ControlTrustEntry>,
);

async fn build_key_distribution_batch(
    app: &Runtime,
    authorized_devices: &[kodosi_backend_client::api::AuthorizedDeviceDto],
    authorized_recipients: &HashMap<String, AccessLevel>,
    context: KeyDistributionContext<'_>,
) -> Result<KeyDistributionBatch> {
    use base64::{Engine, engine::general_purpose::STANDARD as BASE64};

    let mut verified_identities = HashMap::new();
    let mut blobs = Vec::new();
    let mut control_keys = HashMap::new();
    for authorized in authorized_devices {
        let identity =
            verified_recipient_identity(app, &authorized.user_id, &mut verified_identities).await?;
        let (viewer_kem_public, signing_public) =
            verified_device_key_material(identity, &authorized.user_id, &authorized.device_id)?;

        let capabilities = SessionCapabilities::from_access(
            authorized_recipients
                .get(&authorized.user_id)
                .copied()
                .unwrap_or(AccessLevel::View),
            authorized.user_id == context.owner_user_id,
        );
        control_keys.insert(
            (authorized.user_id.clone(), authorized.device_id.clone()),
            ControlTrustEntry::new(signing_public, capabilities),
        );

        let encrypted = crypto::wrap_session_key(
            &viewer_kem_public,
            context.session_key,
            context.backend_session_id,
            &authorized.device_id,
            context.key_generation,
        )?;
        let signature = crypto::sign_key_blob_v2(
            context.signing_key,
            context.backend_session_id,
            context.backend_incarnation_id,
            &authorized.device_id,
            &encrypted,
            context.key_generation,
            context.issued_at_ms,
        )?;
        blobs.push(SessionKeyBlobEntry {
            recipient_device_id: authorized.device_id.clone(),
            encrypted_session_key: BASE64.encode(&encrypted),
            sender_device_id: context.sender_device_id.to_owned(),
            key_generation: context.key_generation,
            issued_at_ms: context.issued_at_ms,
            signature: BASE64.encode(&signature),
            signature_version: crypto::CURRENT_KEY_BLOB_SIGNATURE_VERSION,
        });
    }
    Ok((blobs, control_keys))
}

fn shared_backend_incarnation_id(app: &Runtime, id: SessionId) -> Result<Uuid> {
    app.state
        .sharing
        .shared_sessions
        .get(id)
        .map(|shared| *shared.backend_incarnation_id())
        .ok_or(AppError::NoActiveSession)
}

async fn authorized_devices_for_distribution(
    app: &Runtime,
    backend_session_id: &str,
    authorized_recipients: &HashMap<String, AccessLevel>,
) -> Result<Vec<kodosi_backend_client::api::AuthorizedDeviceDto>> {
    let authorized_user_ids = authorized_recipients.keys().cloned().collect();

    Ok(filter_authorized_devices(
        backend_session_id,
        app.backend
            .fetch_authorized_devices(backend_session_id)
            .await?,
        &authorized_user_ids,
    ))
}

fn verified_device_key_material(
    identity: &crate::identity_core::device_list_pin_store::IdentityBundleView,
    user_id: &str,
    device_id: &str,
) -> Result<(Vec<u8>, Vec<u8>)> {
    let device = identity
        .devices
        .get(device_id)
        .ok_or_else(|| AppError::InvalidBackendData {
            field: "authorizedDevices.deviceId".to_owned(),
            reason: format!(
                "authorized device {device_id} is not in user {user_id}'s signed device list"
            ),
        })?;
    Ok((
        device.certificate.kem_public_key.clone(),
        device.sig_public_key.clone(),
    ))
}

pub(crate) fn filter_authorized_devices(
    backend_session_id: &str,
    devices: Vec<kodosi_backend_client::api::AuthorizedDeviceDto>,
    authorized_user_ids: &std::collections::HashSet<String>,
) -> Vec<kodosi_backend_client::api::AuthorizedDeviceDto> {
    devices
        .into_iter()
        .filter(|device| {
            let authorized = authorized_user_ids.contains(&device.user_id);
            if !authorized {
                tracing::warn!(
                    backend_session_id,
                    user_id = %device.user_id,
                    device_id = %device.device_id,
                    "backend returned device outside owner-authorized audience"
                );
            }
            authorized
        })
        .collect()
}

pub(crate) async fn owner_authorized_recipients(
    app: &Runtime,
    backend_session_id: &str,
) -> Result<HashMap<String, AccessLevel>> {
    let owner_user_id = app
        .state
        .identity
        .auth
        .subject_string()
        .ok_or(AppError::Unauthorized)?;
    let local_id = app
        .state
        .sharing
        .shared_sessions
        .local_id_for_backend(backend_session_id)
        .ok_or(AppError::NoActiveSession)?;

    let (scope, room_id, explicit_grantees) = {
        let shared = app
            .state
            .sharing
            .shared_sessions
            .get(local_id)
            .ok_or(AppError::NoActiveSession)?;
        (
            shared.scope(),
            shared.room().map(|room| room.id.clone()),
            shared.explicit_grantee_access().clone(),
        )
    };

    let mut allowed = HashMap::new();
    match scope {
        ShareScope::JustMe | ShareScope::MyDevices => {}
        ShareScope::Room => {
            let room_id = room_id.ok_or_else(|| AppError::InvalidBackendData {
                field: "session.roomId".to_owned(),
                reason: "room-scoped session has no selected room".to_owned(),
            })?;

            for member in crate::room_crypto::verified_room_members(app, &room_id).await? {
                allowed.insert(member, AccessLevel::View);
            }
        }
        ShareScope::Friends => {
            return Err(AppError::Unsupported {
                reason: "friends-scope encrypted sharing requires an owner-signed audience manifest; use room or explicit grants until the backend persists that manifest".to_owned(),
            });
        }
    }

    let now = time::OffsetDateTime::now_utc();
    allowed.extend(
        explicit_grantees
            .into_iter()
            .filter(|(_, grant)| grant.expires_at > now)
            .map(|(user_id, grant)| (user_id, grant.access)),
    );
    allowed.insert(owner_user_id, AccessLevel::Approve);
    Ok(allowed)
}

async fn finalize_key_distribution(
    app: &mut Runtime,
    backend_session_id: &str,
    backend_incarnation_id: Uuid,
    blobs: Vec<SessionKeyBlobEntry>,
    control_keys: HashMap<(String, String), ControlTrustEntry>,
    sender_device_id: String,
) -> Result<()> {
    app.backend
        .store_session_key_blobs(
            backend_session_id,
            &StoreSessionKeyBlobsRequest {
                incarnation_id: backend_incarnation_id,
                blobs,
            },
        )
        .await?;
    if !app
        .state
        .sharing
        .shared_sessions
        .replace_control_trust_for_backend(backend_session_id, control_keys)
    {
        return Err(AppError::NoActiveSession);
    }
    if !app
        .state
        .sharing
        .shared_sessions
        .set_host_device_for_backend(backend_session_id, sender_device_id)
    {
        return Err(AppError::NoActiveSession);
    }

    Ok(())
}

async fn verified_recipient_identity<'a>(
    app: &Runtime,
    user_id: &str,
    identities: &'a mut HashMap<
        String,
        crate::identity_core::device_list_pin_store::IdentityBundleView,
    >,
) -> Result<&'a crate::identity_core::device_list_pin_store::IdentityBundleView> {
    if !identities.contains_key(user_id) {
        let bundle = app.backend.fetch_user_identity(user_id).await?;
        let view = crate::runtime::identity::backend_adapters::identity_bundle_view(&bundle)?;
        match app
            .pin_store
            .verify_or_pin(view.clone(), PinContext::ExplicitShare)
            .await?
        {
            PinVerdict::FirstShare | PinVerdict::AcceptUpdate | PinVerdict::AlreadyPinned => {}
            PinVerdict::Reject { reason } => {
                return Err(AppError::PeerIdentityChanged {
                    user_id: user_id.to_owned(),
                    detail: format!("{reason:?}"),
                });
            }
        }
        identities.insert(user_id.to_owned(), view);
    }
    identities
        .get(user_id)
        .ok_or_else(|| AppError::InvalidBackendData {
            field: "authorizedDevices.userId".to_owned(),
            reason: "verified identity disappeared during key distribution".to_owned(),
        })
}

fn current_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

pub(crate) struct ProvisionalRelayBridge {
    pub(crate) hub: crate::terminal_transport::hub::SessionHub,
    pub(crate) session_id: SessionId,
    pub(crate) connection_id: crate::terminal_transport::TerminalConnectionId,
    pub(crate) cancellation: tokio_util::sync::CancellationToken,
    pub(crate) armed: bool,
}

impl ProvisionalRelayBridge {
    pub(crate) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ProvisionalRelayBridge {
    fn drop(&mut self) {
        if self.armed {
            self.cancellation.cancel();
            self.hub.unregister(self.session_id, self.connection_id);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
enum HostRelayRangeReservationError {
    #[error("encrypted frame nonce space exhausted")]
    NonceExhausted,
    #[error("terminal frame revision space exhausted")]
    RevisionExhausted,
    #[error("{0}")]
    Unavailable(String),
}

pub(crate) fn reserve_host_relay_ranges_or_dispose(
    app: &mut Runtime,
    id: SessionId,
    frame_key_generation: u32,
) -> Result<(FrameRevisionBlock, FrameNonceBlock)> {
    match reserve_host_relay_ranges(app, id, frame_key_generation) {
        Ok(ranges) => Ok(ranges),
        Err(HostRelayRangeReservationError::NonceExhausted) => {
            app.state.pending_work.queue_host_key_rotation(id);
            app.state
                .local
                .sessions
                .update_state(id, SessionState::Reconnecting);
            app.state.record_log(format!(
                "{} exhausted encrypted frame nonces; rotating its session key",
                id.short()
            ));
            app.state.sync_host_relay_status();
            Err(AppError::Unsupported {
                reason: "encrypted frame nonce space exhausted; key rotation queued".to_owned(),
            })
        }
        Err(HostRelayRangeReservationError::RevisionExhausted) => {
            app.state.sharing.host_relays.teardown(id);
            app.state
                .local
                .sessions
                .update_state(id, SessionState::Failed);
            app.state.record_log(format!(
                "{} exhausted terminal frame revisions; sharing is permanently failed for this session incarnation",
                id.short()
            ));
            app.state.sync_host_relay_status();
            Err(AppError::Unsupported {
                reason: "terminal frame revision space exhausted for this session incarnation"
                    .to_owned(),
            })
        }
        Err(HostRelayRangeReservationError::Unavailable(reason)) => {
            Err(AppError::Unsupported { reason })
        }
    }
}

#[tracing::instrument(skip_all, fields(session_id = %id, frame_key_generation), err)]
fn reserve_host_relay_ranges(
    app: &mut Runtime,
    id: SessionId,
    frame_key_generation: u32,
) -> std::result::Result<(FrameRevisionBlock, FrameNonceBlock), HostRelayRangeReservationError> {
    use crate::sharing::shared_session_registry::{
        FrameNonceReservationError, FrameRevisionReservationError,
    };

    let nonce_block = app
        .state
        .sharing
        .shared_sessions
        .reserve_frame_nonce_block(id, FRAME_NONCE_BLOCK_SIZE)
        .map_err(|error| match error {
            FrameNonceReservationError::Exhausted => HostRelayRangeReservationError::NonceExhausted,
            _ => HostRelayRangeReservationError::Unavailable(format!(
                "cannot reserve encrypted frame nonce block: {error}"
            )),
        })?;
    if nonce_block.generation != frame_key_generation {
        return Err(HostRelayRangeReservationError::Unavailable(
            "reserved encrypted frame nonce block generation did not match session key generation"
                .to_owned(),
        ));
    }

    let revision_block = app
        .state
        .sharing
        .shared_sessions
        .reserve_frame_revision_block(id, FRAME_REVISION_BLOCK_SIZE)
        .map_err(|error| match error {
            FrameRevisionReservationError::Exhausted => {
                HostRelayRangeReservationError::RevisionExhausted
            }
            FrameRevisionReservationError::MissingSession => {
                HostRelayRangeReservationError::Unavailable(format!(
                    "cannot reserve terminal frame revision block: {error}"
                ))
            }
        })?;

    Ok((revision_block, nonce_block))
}

pub(crate) fn send_remote_owner_input_payload(
    app: &mut Runtime,
    id: SessionId,
    payload: Vec<u8>,
    command_name: &str,
) -> Result<()> {
    dispatch_remote_owner_command(
        app,
        id,
        command_name,
        SessionRelayCommand::OwnerInject { payload },
    )
}

pub(crate) fn send_remote_owner_control(
    app: &mut Runtime,
    id: SessionId,
    command: SessionRelayCommand,
    command_name: &str,
) -> Result<()> {
    dispatch_remote_owner_command(app, id, command_name, command)
}

pub(crate) fn send_remote_owner_resize_claim_payload(
    app: &mut Runtime,
    id: SessionId,
    action_id: String,
    size: kodosi_domain::terminal::TerminalSize,
    pixel_geometry: Option<kodosi_domain::terminal::TerminalPixelGeometry>,
    command_name: &str,
) -> Result<()> {
    dispatch_remote_owner_command(
        app,
        id,
        command_name,
        SessionRelayCommand::OwnerResize {
            action_id,
            rows: size.rows(),
            cols: size.cols(),
            pixel_geometry,
            claim: true,
        },
    )
}

pub(crate) fn send_remote_owner_focus_changed_payload(
    app: &mut Runtime,
    id: SessionId,
    focused: bool,
    command_name: &str,
) -> Result<()> {
    dispatch_remote_owner_command(
        app,
        id,
        command_name,
        SessionRelayCommand::OwnerFocusChanged { focused },
    )
}

fn dispatch_remote_owner_command(
    app: &mut Runtime,
    id: SessionId,
    command_name: &str,
    command: SessionRelayCommand,
) -> Result<()> {
    crate::runtime::remote_sessions::ensure_authenticated_remote_account(app, command_name)?;
    if app
        .state
        .discovery
        .session(id)
        .is_none_or(|record| record.summary.role != SessionRole::Owner || record.viewer_blocked)
    {
        return Err(AppError::Unsupported {
            reason: format!(
                "{command_name} requires a current authenticated owner catalog entry for session {}",
                id.short()
            ),
        });
    }
    if !app.state.remote.session_relays.contains(id) {
        app.state.record_log(format!(
            "{} ignored {command_name} because the remote host relay is not active",
            id.short()
        ));
        return Err(AppError::Unsupported {
            reason: format!("remote owner session {} is not connected yet", id.short()),
        });
    }

    if app.state.remote.session_relays.relay_mode(id) != Some(RemoteRelayMode::OwnerParticipant) {
        return Err(AppError::Unsupported {
            reason: format!(
                "session {} is not connected with remote owner controls",
                id.short()
            ),
        });
    }

    let status = app
        .state
        .remote
        .session_relays
        .status(id)
        .unwrap_or(ConnectionState::Offline);
    if status != ConnectionState::Connected {
        app.state.record_log(format!(
            "{} ignored {command_name} while the remote host relay was {:?}",
            id.short(),
            status
        ));
        return Err(AppError::Unsupported {
            reason: format!("remote owner session {} is still reconnecting", id.short()),
        });
    }

    match app.state.remote.session_relays.try_send(id, command) {
        Some(Ok(())) => Ok(()),
        None => Err(AppError::Unsupported {
            reason: format!("remote owner session {} is not connected yet", id.short()),
        }),
        Some(Err(mpsc::error::TrySendError::Full(_))) => {
            app.state.record_log(format!(
                "{} dropped {command_name} because the remote owner command queue is full",
                id.short()
            ));
            Err(AppError::ChannelFull {
                session: id.to_string(),
            })
        }
        Some(Err(mpsc::error::TrySendError::Closed(_))) => {
            app.state.record_log(format!(
                "{} ignored {command_name} because the remote owner command queue closed",
                id.short()
            ));
            Err(AppError::ChannelClosed {
                session: id.to_string(),
            })
        }
    }
}

pub(crate) fn resolve_room_selection(app: &Runtime, room_id: Option<&str>) -> Result<SelectedRoom> {
    let room_id = room_id.ok_or_else(|| AppError::Unsupported {
        reason: "select a room before sharing there".to_owned(),
    })?;
    let room = app
        .state
        .available_rooms
        .iter()
        .find(|room| room.id == room_id)
        .ok_or_else(|| AppError::Unsupported {
            reason: "the selected room is no longer available".to_owned(),
        })?;
    Ok(SelectedRoom {
        id: room.id.clone(),
        name: room.name.clone(),
    })
}

pub(crate) fn record_shared_audience(
    app: &mut Runtime,
    id: SessionId,
    scope: ShareScope,
    room: Option<&SelectedRoom>,
) -> Result<()> {
    app.state
        .sharing
        .shared_sessions
        .set_audience(id, scope, room.map(SharedRoom::from))
        .then_some(())
        .ok_or(AppError::NoActiveSession)
}
