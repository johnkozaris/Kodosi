use kodosi_backend_client::{BackendClientError, http_client::BackendHttpClient};

use zeroize::Zeroizing;

use crate::sharing::collaboration_teardown_obligations::{
    CollaborationTeardownObligationStore, QuarantineReason, TeardownObligation,
};

#[derive(Debug)]
pub(super) enum CollaborationTeardownTaskResult {
    Settled(String),
    Retry,
    Pause,
    Quarantined(String),
    Deferred {
        create_idempotency_id: uuid::Uuid,
        message: String,
    },
    StoreError(String),
}

#[derive(Debug)]
pub(super) struct CollaborationTeardownTaskCompletion {
    pub(super) create_idempotency_id: uuid::Uuid,
    pub(super) result: CollaborationTeardownTaskResult,
    pub(super) rejected_access_token: Option<Zeroizing<String>>,
}

pub(super) async fn run(
    backend: BackendHttpClient,
    store: CollaborationTeardownObligationStore,
    mut obligation: TeardownObligation,
) -> CollaborationTeardownTaskCompletion {
    let result = run_with_backend(&backend, &store, &mut obligation).await;
    let rejected_access_token = matches!(result, CollaborationTeardownTaskResult::Pause)
        .then(|| backend.into_access_token())
        .flatten();
    CollaborationTeardownTaskCompletion {
        create_idempotency_id: obligation.create_idempotency_id,
        result,
        rejected_access_token,
    }
}

async fn run_with_backend(
    backend: &BackendHttpClient,
    store: &CollaborationTeardownObligationStore,
    obligation: &mut TeardownObligation,
) -> CollaborationTeardownTaskResult {
    if obligation.backend_incarnation_id.is_none() {
        match backend
            .fetch_session_creation_receipt(
                &obligation.backend_session_id,
                &obligation.create_idempotency_id,
            )
            .await
        {
            Ok(receipt) => {
                if let Err(error) = store.bind_incarnation(
                    obligation.create_idempotency_id,
                    &obligation.backend_session_id,
                    receipt.incarnation_id,
                ) {
                    return CollaborationTeardownTaskResult::StoreError(error.to_string());
                }
                obligation.backend_incarnation_id = Some(receipt.incarnation_id);
            }
            Err(BackendClientError::NotFound) => {
                return match store.acknowledge(
                    obligation.create_idempotency_id,
                    None,
                    obligation.end_mutation_id,
                ) {
                    Ok(_) => CollaborationTeardownTaskResult::Settled(format!(
                        "confirmed provisional backend session {} was never created",
                        obligation.backend_session_id
                    )),
                    Err(error) => CollaborationTeardownTaskResult::StoreError(error.to_string()),
                };
            }
            Err(error) if is_retryable(&error) => {
                return CollaborationTeardownTaskResult::Retry;
            }
            Err(BackendClientError::Unauthorized) => {
                return CollaborationTeardownTaskResult::Pause;
            }
            Err(error) => {
                return quarantine(
                    store,
                    obligation,
                    None,
                    QuarantineReason::RemoteTargetMismatch,
                    format!(
                        "provisional backend session {} reconciliation requires operator attention: {error}",
                        obligation.backend_session_id
                    ),
                );
            }
        }
    }

    let Some(incarnation_id) = obligation.backend_incarnation_id else {
        return CollaborationTeardownTaskResult::StoreError(
            "bound collaboration cleanup obligation lost its incarnation".to_owned(),
        );
    };
    let attempt_id = uuid::Uuid::now_v7();
    match classify(
        backend
            .end_session(
                &obligation.backend_session_id,
                &incarnation_id,
                &obligation.end_mutation_id,
                &attempt_id,
            )
            .await,
    ) {
        DispatchOutcome::Acknowledge => match store.acknowledge(
            obligation.create_idempotency_id,
            Some(incarnation_id),
            obligation.end_mutation_id,
        ) {
            Ok(_) => CollaborationTeardownTaskResult::Settled(format!(
                "confirmed backend session {} ended",
                obligation.backend_session_id
            )),
            Err(error) => CollaborationTeardownTaskResult::StoreError(error.to_string()),
        },
        DispatchOutcome::Pause => CollaborationTeardownTaskResult::Pause,
        DispatchOutcome::Retry => CollaborationTeardownTaskResult::Retry,
        DispatchOutcome::Quarantine(reason) => quarantine(
            store,
            obligation,
            Some(incarnation_id),
            reason,
            format!(
                "backend session {} cleanup requires operator attention",
                obligation.backend_session_id
            ),
        ),
    }
}

fn quarantine(
    store: &CollaborationTeardownObligationStore,
    obligation: &TeardownObligation,
    incarnation_id: Option<uuid::Uuid>,
    reason: QuarantineReason,
    message: String,
) -> CollaborationTeardownTaskResult {
    match store.quarantine(
        obligation.create_idempotency_id,
        incarnation_id,
        obligation.end_mutation_id,
        reason,
    ) {
        Ok(crate::sharing::collaboration_teardown_obligations::QuarantineOutcome::Quarantined) => {
            CollaborationTeardownTaskResult::Quarantined(message)
        }
        Ok(crate::sharing::collaboration_teardown_obligations::QuarantineOutcome::CapacityFull) => {
            CollaborationTeardownTaskResult::Deferred {
                create_idempotency_id: obligation.create_idempotency_id,
                message: format!(
                    "{message}; quarantine capacity is full and the active obligation was retained"
                ),
            }
        }
        Ok(crate::sharing::collaboration_teardown_obligations::QuarantineOutcome::NotFound) => {
            CollaborationTeardownTaskResult::Settled(format!(
                "collaboration cleanup obligation {} was already resolved",
                obligation.backend_session_id
            ))
        }
        Err(error) => CollaborationTeardownTaskResult::StoreError(error.to_string()),
    }
}

fn is_retryable(error: &BackendClientError) -> bool {
    match error {
        BackendClientError::Io(_)
        | BackendClientError::Http(_)
        | BackendClientError::Timeout { .. }
        | BackendClientError::Json(_) => true,
        BackendClientError::HttpProblem { status, .. } => {
            matches!(*status, 408 | 425 | 429) || *status >= 500
        }
        _ => false,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum DispatchOutcome {
    Acknowledge,
    Pause,
    Retry,
    Quarantine(QuarantineReason),
}

pub(super) fn classify(result: kodosi_backend_client::Result<()>) -> DispatchOutcome {
    match result {
        Ok(()) | Err(BackendClientError::NotFound) => DispatchOutcome::Acknowledge,
        Err(BackendClientError::Unauthorized) => DispatchOutcome::Pause,
        Err(error) if is_retryable(&error) => DispatchOutcome::Retry,
        Err(BackendClientError::HttpProblem {
            status: 409,
            code: Some(code),
            ..
        }) if code == "SESSION_END_MUTATION_TARGET_CONFLICT" => {
            DispatchOutcome::Quarantine(QuarantineReason::MutationConflict)
        }
        Err(BackendClientError::HttpProblem { status: 409, .. }) => DispatchOutcome::Retry,
        Err(BackendClientError::HttpProblem { .. }) => {
            DispatchOutcome::Quarantine(QuarantineReason::NonRetryableClientError)
        }
        Err(_) => DispatchOutcome::Quarantine(QuarantineReason::StoreInvariantViolation),
    }
}
