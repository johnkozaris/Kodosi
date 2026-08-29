use kodosi_domain::{ids::SessionId, permissions::ShareScope, session::SessionState};
use uuid::Uuid;

use crate::{AppError, Result};

use super::{
    Runtime,
    access_effect_worker::{AccessEffectAudience, AccessEffectCommit, AccessEffectServices},
    relay_prepare_worker::{RelayPrepareCommit, RelayPrepareFence, RelayPrepareOwner},
    share_transition_worker::{
        ShareTransitionCompletion, ShareTransitionWork, ShareTransitionWorkerInput,
        ShareTransitionWorkerMode, ShareTransitionWorkerOutcome,
    },
    share_transitions::{
        DeferredShareSettlement, DeferredShareVerdict, PreparedShareTransition, ShareAudience,
        ShareTransitionCleanupIdentity, ShareTransitionState, ShareTransitionTerminal,
        ShareTransitionTerminalStatus,
    },
};

const REMOTE_SHARE_EXECUTION_BUDGET: std::time::Duration = std::time::Duration::from_secs(500);
const LOCAL_SHARE_EXECUTION_BUDGET: std::time::Duration = std::time::Duration::from_secs(4);

fn share_execution_budget(target_scope: ShareScope) -> std::time::Duration {
    if target_scope == ShareScope::JustMe {
        LOCAL_SHARE_EXECUTION_BUDGET
    } else {
        REMOTE_SHARE_EXECUTION_BUDGET
    }
}

impl Runtime {
    pub(crate) fn begin_share_transition(
        &mut self,
        transition_id: Uuid,
        session_id: SessionId,
        expected_runtime_incarnation_id: Uuid,
        target_scope: ShareScope,
        target_room_id: Option<&str>,
    ) -> Result<PreparedShareTransition> {
        if target_scope == ShareScope::Friends {
            return Err(AppError::Unsupported {
                reason: "friends sharing is decode-only and cannot be selected".to_owned(),
            });
        }
        self.share_transitions.ensure_available()?;
        let account_user_id = self
            .state
            .identity
            .auth
            .subject_string()
            .ok_or(AppError::Unauthorized)?;
        let record = self
            .state
            .local
            .sessions
            .record(session_id)
            .ok_or(AppError::NoActiveSession)?;
        if record.local_incarnation_id != expected_runtime_incarnation_id {
            return Err(AppError::NoActiveSession);
        }
        let previous_scope = record.summary.scope;
        let previous_room_id = self
            .state
            .sharing
            .shared_sessions
            .get(session_id)
            .and_then(|shared| shared.room().map(|room| room.id.clone()));
        let target_room = if target_scope == ShareScope::Room {
            Some(super::sharing::resolve_room_selection(
                self,
                target_room_id,
            )?)
        } else {
            None
        };
        let target = ShareAudience {
            scope: target_scope,
            room_id: target_room.as_ref().map(|room| room.id.clone()),
        };
        if let Some(existing) = self
            .share_transitions
            .get(&account_user_id, transition_id)?
            .cloned()
        {
            if existing.runtime_session_id != session_id
                || existing.expected_runtime_incarnation_id != expected_runtime_incarnation_id
                || existing.target != target
            {
                return Err(AppError::Unsupported {
                    reason: "requestId already names a different share transition".to_owned(),
                });
            }
            self.replay_share_transition_terminal(&existing);
            return Ok(existing);
        }
        let active_collaboration =
            self.collaboration_change_active(&account_user_id, session_id)?;
        if target_scope == ShareScope::JustMe
            && (previous_scope != ShareScope::JustMe || active_collaboration)
        {
            let prepared = self.prepare_local_unshare_receipt(
                transition_id,
                session_id,
                expected_runtime_incarnation_id,
                account_user_id,
            )?;
            self.preempt_collaboration_for_unshare(session_id);
            self.state.unshare_session_locally(session_id);
            let persist_result = self
                .state
                .local
                .sessions
                .record(session_id)
                .ok_or(AppError::NoActiveSession)
                .and_then(|record| self.local_catalog.persist_record(record));
            if let Err(error) = persist_result {
                self.fail_share_transition(&prepared, error.to_string());
                return Err(error);
            }
            self.last_maintenance_ran = None;
            self.share_transition_in_flight
                .insert(session_id, transition_id);
            return self.start_prepared_share_transition(prepared, None);
        }
        let prepared = self.prepare_new_share_transition(
            transition_id,
            session_id,
            expected_runtime_incarnation_id,
            account_user_id,
            ShareAudience {
                scope: previous_scope,
                room_id: previous_room_id,
            },
            target,
        )?;
        self.start_prepared_share_transition(prepared, target_room.as_ref())
    }

    fn prepare_local_unshare_receipt(
        &mut self,
        transition_id: Uuid,
        session_id: SessionId,
        expected_runtime_incarnation_id: Uuid,
        account_user_id: String,
    ) -> Result<PreparedShareTransition> {
        let transition_epoch = self
            .share_transitions
            .entries(&account_user_id)?
            .iter()
            .filter(|entry| entry.runtime_session_id == session_id)
            .map(|entry| entry.transition_epoch)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| AppError::Unsupported {
                reason: "share transition epoch space exhausted".to_owned(),
            })?;
        let local = ShareAudience {
            scope: ShareScope::JustMe,
            room_id: None,
        };
        let prepared = PreparedShareTransition::new(
            transition_id,
            transition_epoch,
            account_user_id,
            self.state.identity.account_epoch().value(),
            session_id,
            expected_runtime_incarnation_id,
            local.clone(),
            local,
            session_id.to_string(),
            None,
            None,
            None,
        )?;
        self.share_transitions.put(prepared.clone())?;
        self.share_transition_deadlines.insert(
            transition_id,
            std::time::Instant::now() + share_execution_budget(ShareScope::JustMe),
        );
        Ok(prepared)
    }

    fn prepare_new_share_transition(
        &mut self,
        transition_id: Uuid,
        session_id: SessionId,
        expected_runtime_incarnation_id: Uuid,
        account_user_id: String,
        previous: ShareAudience,
        target: ShareAudience,
    ) -> Result<PreparedShareTransition> {
        self.ensure_collaboration_operation_available(&account_user_id, session_id)?;
        if !self.remote_surfaces_ready()
            && previous.scope != target.scope
            && target.scope != ShareScope::JustMe
        {
            return Err(AppError::Unsupported {
                reason: "remote operations are not ready; retry after reconciliation".to_owned(),
            });
        }
        let backend_identity = self
            .state
            .sharing
            .shared_sessions
            .get(session_id)
            .map(|shared| {
                (
                    shared.backend_session_id().to_owned(),
                    *shared.backend_incarnation_id(),
                    shared.session_key_generation(),
                )
            });
        let backend_session_id = backend_identity
            .as_ref()
            .map_or_else(|| session_id.to_string(), |value| value.0.clone());
        let cleanup = if previous.scope == ShareScope::JustMe && target.scope == ShareScope::JustMe
        {
            None
        } else {
            Some(self.prepare_share_cleanup_identity(
                &account_user_id,
                &backend_session_id,
                backend_identity.as_ref().map(|value| value.1),
                transition_id,
            )?)
        };
        let transition_epoch = self
            .share_transitions
            .entries(&account_user_id)?
            .iter()
            .filter(|entry| entry.runtime_session_id == session_id)
            .map(|entry| entry.transition_epoch)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| AppError::Unsupported {
                reason: "share transition epoch space exhausted".to_owned(),
            })?;
        let prepared = PreparedShareTransition::new(
            transition_id,
            transition_epoch,
            account_user_id,
            self.state.identity.account_epoch().value(),
            session_id,
            expected_runtime_incarnation_id,
            previous,
            target,
            backend_session_id,
            backend_identity.as_ref().map(|value| value.1),
            backend_identity.and_then(|value| value.2),
            cleanup,
        )?;
        self.share_transitions.put(prepared.clone())?;
        self.share_transition_in_flight
            .insert(session_id, transition_id);
        self.share_transition_deadlines.insert(
            transition_id,
            std::time::Instant::now() + share_execution_budget(prepared.target.scope),
        );
        Ok(prepared)
    }

    fn start_prepared_share_transition(
        &mut self,
        prepared: PreparedShareTransition,
        target_room: Option<&crate::sharing::scope::SelectedRoom>,
    ) -> Result<PreparedShareTransition> {
        let session_id = prepared.runtime_session_id;
        if prepared.previous == prepared.target {
            let commit_ready =
                self.persist_share_transition_state(&prepared, ShareTransitionState::CommitReady)?;
            self.finish_share_transition(
                &commit_ready,
                ShareTransitionTerminalStatus::Applied,
                None,
                DeferredShareVerdict::ScopeChanged,
            )?;
            return Ok(prepared);
        }

        self.cancel_relay_prepare_for_session(session_id);
        self.state.sharing.host_relays.cancel(session_id);
        self.state
            .local
            .sessions
            .update_state(session_id, SessionState::Reconnecting);
        let applying =
            self.persist_share_transition_state(&prepared, ShareTransitionState::ApplyingTarget)?;
        let (create_request, owner_secret) = if applying.is_initial_share() {
            let owner_secret = crate::sharing::keys::generate_owner_secret()?;
            let record = self
                .state
                .local
                .sessions
                .record(session_id)
                .ok_or(AppError::NoActiveSession)?;
            let request = crate::sharing::backend_adapters::create_session_request(
                applying.backend_session_id.clone(),
                applying
                    .cleanup
                    .as_ref()
                    .ok_or_else(|| AppError::Unsupported {
                        reason: "initial share is missing cleanup identity".to_owned(),
                    })?
                    .create_idempotency_id,
                record.summary.title.clone(),
                record.summary.access,
                applying.target.scope,
                owner_secret.clone(),
                target_room,
            );
            (Some(request), Some(owner_secret))
        } else {
            (None, None)
        };
        if let Err(error) = self.spawn_share_transition_worker(
            applying,
            ShareTransitionWorkerInput::ApplyTarget {
                create_request,
                owner_secret,
            },
        ) {
            self.fail_share_transition(&prepared, error.to_string());
            return Err(error);
        }
        Ok(prepared)
    }

    fn prepare_share_cleanup_identity(
        &self,
        account_user_id: &str,
        backend_session_id: &str,
        backend_incarnation_id: Option<Uuid>,
        transition_id: Uuid,
    ) -> Result<ShareTransitionCleanupIdentity> {
        let backend_origin = self
            .backend
            .backend_origin()
            .ok_or(AppError::MissingConfig { key: "backend.api" })?
            .clone();
        if let Some(existing) = self
            .collaboration_teardown
            .list_for_account(&backend_origin, account_user_id)?
            .into_iter()
            .find(|entry| {
                entry.backend_session_id == backend_session_id
                    && entry.backend_incarnation_id == backend_incarnation_id
            })
        {
            return Ok(ShareTransitionCleanupIdentity {
                backend_origin: existing.backend_origin.to_string(),
                create_idempotency_id: existing.create_idempotency_id,
                end_mutation_id: existing.end_mutation_id,
                created_at_ms: existing.created_at_ms,
            });
        }
        if backend_incarnation_id.is_some() {
            return Err(AppError::Unsupported {
                reason: "shared session is missing its exact durable cleanup obligation".to_owned(),
            });
        }
        let end_mutation_id = Uuid::now_v7();
        let created_at_ms = time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000;
        let created_at_ms = i64::try_from(created_at_ms).map_err(|_| AppError::Unsupported {
            reason: "collaboration teardown creation time is out of range".to_owned(),
        })?;
        self.collaboration_teardown.provision(
            &backend_origin,
            account_user_id,
            backend_session_id,
            transition_id,
            end_mutation_id,
            created_at_ms,
        )?;
        Ok(ShareTransitionCleanupIdentity {
            backend_origin: backend_origin.to_string(),
            create_idempotency_id: transition_id,
            end_mutation_id,
            created_at_ms,
        })
    }

    pub(super) fn spawn_share_transition_worker(
        &mut self,
        prepared: PreparedShareTransition,
        input: ShareTransitionWorkerInput,
    ) -> Result<()> {
        let transition_id = prepared.transition_id;
        if self.share_transition_tasks.contains_key(&transition_id)
            || self.share_transition_tasks.len() >= 32
        {
            return Err(AppError::ChannelFull {
                session: prepared.runtime_session_id.to_string(),
            });
        }
        let deadline = self
            .share_transition_deadlines
            .get(&transition_id)
            .copied()
            .ok_or_else(|| AppError::Unsupported {
                reason: "share transition execution deadline is unavailable".to_owned(),
            })?;
        if std::time::Instant::now() >= deadline {
            return Err(AppError::Unsupported {
                reason: "share transition exceeded its execution deadline".to_owned(),
            });
        }
        let completion_tx = self.share_transition_completion_tx.clone();
        let fallback = prepared.clone();
        let account_epoch = self.state.identity.account_epoch().value();
        let input_mode = input.mode();
        let work = ShareTransitionWork {
            prepared,
            account_epoch,
            backend: self.backend.clone(),
            input,
        };
        let timeout_fallback = fallback.clone();
        self.share_transition_tasks.insert(
            transition_id,
            tokio::spawn(async move {
                use futures_util::FutureExt as _;
                let completion = tokio::time::timeout_at(
                    tokio::time::Instant::from_std(deadline),
                    std::panic::AssertUnwindSafe(super::share_transition_worker::execute(work))
                        .catch_unwind(),
                )
                .await
                .map_or_else(
                    |_| ShareTransitionCompletion {
                        prepared: timeout_fallback,
                        account_epoch,
                        mode: input_mode,
                        outcome: Err(AppError::Unsupported {
                            reason: "share transition exceeded its execution deadline".to_owned(),
                        }),
                    },
                    |result| {
                        result.unwrap_or_else(|_| ShareTransitionCompletion {
                            prepared: fallback,
                            account_epoch,
                            mode: input_mode,
                            outcome: Err(AppError::Unsupported {
                                reason: "share transition worker panicked".to_owned(),
                            }),
                        })
                    },
                );
                drop(completion_tx.send(completion).await);
            }),
        );
        Ok(())
    }

    pub(crate) fn has_share_transition_workers(&self) -> bool {
        !self.share_transition_tasks.is_empty()
    }

    pub(crate) fn apply_received_share_transition_completion(
        &mut self,
        completion: ShareTransitionCompletion,
    ) {
        self.share_transition_tasks
            .remove(&completion.prepared.transition_id);
        self.apply_share_transition_completion(completion);
    }

    fn apply_share_transition_completion(&mut self, completion: ShareTransitionCompletion) {
        let prepared = completion.prepared;
        if !self.share_completion_matches(&prepared, completion.account_epoch, completion.mode) {
            self.state.record_log(format!(
                "ignored stale share transition completion {}",
                prepared.transition_id
            ));
            return;
        }
        let outcome = match completion.outcome {
            Ok(outcome) => outcome,
            Err(error) => {
                self.fail_share_transition(&prepared, error.to_string());
                return;
            }
        };
        let failure_context = prepared.clone();
        let result = match (completion.mode, outcome) {
            (
                ShareTransitionWorkerMode::ApplyTarget,
                ShareTransitionWorkerOutcome::TargetObserved {
                    detail,
                    owner_secret,
                    source_key_generation,
                },
            ) => self.apply_share_target_observed(
                prepared,
                &detail,
                owner_secret,
                source_key_generation,
            ),
            (
                ShareTransitionWorkerMode::PrepareKey,
                ShareTransitionWorkerOutcome::KeyPrepared(key),
            ) => self.apply_share_key_prepared(&prepared, key),
            (
                ShareTransitionWorkerMode::ClaimGeneration,
                ShareTransitionWorkerOutcome::GenerationClaimed(claimed),
            ) => self.apply_share_generation_claimed(prepared, claimed),
            (
                ShareTransitionWorkerMode::PublishBlobs,
                ShareTransitionWorkerOutcome::BlobsPublished(commit),
            ) => self.apply_share_blobs_published(prepared, commit),
            _ => Err(AppError::Unsupported {
                reason: "share transition worker returned an outcome for the wrong phase"
                    .to_owned(),
            }),
        };
        if let Err(error) = result {
            self.fail_share_transition(&failure_context, error.to_string());
        }
    }

    pub(super) fn share_completion_matches(
        &self,
        prepared: &PreparedShareTransition,
        completion_account_epoch: u64,
        mode: ShareTransitionWorkerMode,
    ) -> bool {
        let expected_state = match mode {
            ShareTransitionWorkerMode::ApplyTarget => ShareTransitionState::ApplyingTarget,
            ShareTransitionWorkerMode::PrepareKey => ShareTransitionState::KeyPreparing,
            ShareTransitionWorkerMode::ClaimGeneration => ShareTransitionState::GenerationClaiming,
            ShareTransitionWorkerMode::PublishBlobs => {
                let Some(key_generation) = prepared.claimed_key_generation else {
                    return false;
                };
                ShareTransitionState::BlobsPublishing { key_generation }
            }
        };
        self.share_transitions
            .get(&prepared.account_user_id, prepared.transition_id)
            .ok()
            .flatten()
            .is_some_and(|current| {
                current.fingerprint == prepared.fingerprint
                    && current.state == expected_state
                    && self.state.identity.auth.subject_string().as_deref()
                        == Some(prepared.account_user_id.as_str())
                    && completion_account_epoch == prepared.originating_account_epoch
                    && self.state.identity.account_epoch().value() == completion_account_epoch
                    && self
                        .state
                        .local
                        .sessions
                        .record(prepared.runtime_session_id)
                        .is_some_and(|record| {
                            record.local_incarnation_id == prepared.expected_runtime_incarnation_id
                        })
                    && self
                        .share_transition_in_flight
                        .get(&prepared.runtime_session_id)
                        == Some(&prepared.transition_id)
                    && current.backend_incarnation_id == prepared.backend_incarnation_id
                    && current.source_key_generation == prepared.source_key_generation
                    && current.claimed_key_generation == prepared.claimed_key_generation
                    && current.relay_generation == prepared.relay_generation
            })
    }

    fn apply_share_target_observed(
        &mut self,
        mut prepared: PreparedShareTransition,
        detail: &kodosi_backend_client::api::BackendSessionDetail,
        owner_secret: Option<zeroize::Zeroizing<String>>,
        source_key_generation: Option<u32>,
    ) -> Result<()> {
        if detail.id != prepared.backend_session_id
            || !super::share_transition_worker::detail_matches_audience(detail, &prepared.target)
        {
            return Err(AppError::NoActiveSession);
        }
        if prepared.is_initial_share() {
            prepared.bind_backend_incarnation(detail.incarnation_id)?;
            prepared.bind_source_key_generation(source_key_generation.ok_or_else(|| {
                AppError::Unsupported {
                    reason: "initial share did not reconcile a source generation".to_owned(),
                }
            })?)?;
            let cleanup = prepared
                .cleanup
                .as_ref()
                .ok_or_else(|| AppError::Unsupported {
                    reason: "initial share lost its cleanup identity".to_owned(),
                })?;
            self.collaboration_teardown.bind_incarnation(
                cleanup.create_idempotency_id,
                &prepared.backend_session_id,
                detail.incarnation_id,
            )?;
        } else if prepared.backend_incarnation_id != Some(detail.incarnation_id) {
            return Err(AppError::NoActiveSession);
        }
        prepared.state = ShareTransitionState::TargetObserved;
        self.share_transitions.put(prepared.clone())?;
        let room =
            prepared.target.room_id.as_deref().and_then(|room_id| {
                super::sharing::resolve_room_selection(self, Some(room_id)).ok()
            });
        if prepared.is_initial_share() {
            let owner_secret = owner_secret.ok_or_else(|| AppError::Unsupported {
                reason: "initial share lost its owner secret before actor commit".to_owned(),
            })?;
            self.state.sharing.shared_sessions.insert(
                prepared.runtime_session_id,
                crate::sharing::shared_session_registry::SharedSessionState::new(
                    prepared.backend_session_id.clone(),
                    detail.incarnation_id,
                    owner_secret,
                    prepared.target.scope,
                    room.as_ref()
                        .map(crate::sharing::shared_session_registry::SharedRoom::from),
                    None,
                    None,
                ),
            );
        } else {
            super::sharing::record_shared_audience(
                self,
                prepared.runtime_session_id,
                prepared.target.scope,
                room.as_ref(),
            )?;
        }
        let key_preparing =
            self.persist_share_transition_state(&prepared, ShareTransitionState::KeyPreparing)?;
        let shared = self
            .state
            .sharing
            .shared_sessions
            .get(prepared.runtime_session_id)
            .ok_or(AppError::NoActiveSession)?;
        let services = AccessEffectServices {
            backend: self.backend.clone(),
            auth: self.state.identity.auth.clone(),
            device_key_store: self.device_key_store.clone(),
            pin_store: self.pin_store.clone(),
            roster_pins_path: self.room_roster_pins_path.clone(),
        };
        let audience = AccessEffectAudience {
            scope: shared.scope(),
            room_id: shared.room().map(|room| room.id.clone()),
            explicit_grants: shared.explicit_grantee_access().clone(),
            owner_user_id: prepared.account_user_id.clone(),
        };
        self.spawn_share_transition_worker(
            key_preparing,
            ShareTransitionWorkerInput::PrepareKey {
                services,
                audience,
                previous_generation: prepared.source_key_generation.unwrap_or(0),
            },
        )
    }

    fn apply_share_key_prepared(
        &mut self,
        prepared: &PreparedShareTransition,
        key: super::access_effect_worker::PreparedShareKeyDistribution,
    ) -> Result<()> {
        if key.backend_session_id != prepared.backend_session_id
            || Some(key.backend_incarnation_id) != prepared.backend_incarnation_id
            || Some(key.previous_generation) != prepared.source_key_generation
        {
            return Err(AppError::NoActiveSession);
        }
        let claiming = self
            .persist_share_transition_state(prepared, ShareTransitionState::GenerationClaiming)?;
        self.spawn_share_transition_worker(
            claiming,
            ShareTransitionWorkerInput::ClaimGeneration { prepared: key },
        )
    }

    fn apply_share_generation_claimed(
        &mut self,
        mut prepared: PreparedShareTransition,
        claimed: super::access_effect_worker::ClaimedShareKeyDistribution,
    ) -> Result<()> {
        if claimed.prepared.backend_session_id != prepared.backend_session_id
            || Some(claimed.prepared.backend_incarnation_id) != prepared.backend_incarnation_id
        {
            return Err(AppError::NoActiveSession);
        }
        prepared.bind_claimed_key_generation(claimed.key_generation)?;
        prepared.state = ShareTransitionState::BlobsPublishing {
            key_generation: claimed.key_generation,
        };
        self.share_transitions.put(prepared.clone())?;
        self.spawn_share_transition_worker(
            prepared,
            ShareTransitionWorkerInput::PublishBlobs { claimed },
        )
    }

    fn apply_share_blobs_published(
        &mut self,
        mut prepared: PreparedShareTransition,
        commit: AccessEffectCommit,
    ) -> Result<()> {
        let AccessEffectCommit::Distributed {
            session_key,
            key_generation,
            control_keys,
            sender_device_id,
            sender_signing_pkcs8,
            distributed,
        } = commit
        else {
            return Err(AppError::Unsupported {
                reason: "share transition received relay identity instead of key distribution"
                    .to_owned(),
            });
        };
        if !distributed || prepared.claimed_key_generation != Some(key_generation) {
            return Err(AppError::Unsupported {
                reason: "share key distribution did not cover its exact claimed generation"
                    .to_owned(),
            });
        }
        let registry = &mut self.state.sharing.shared_sessions;
        if !registry.set_session_key(
            prepared.runtime_session_id,
            Some(session_key),
            Some(key_generation),
        ) || !registry
            .replace_control_trust_for_backend(&prepared.backend_session_id, control_keys)
            || !registry.set_host_device_for_backend(&prepared.backend_session_id, sender_device_id)
        {
            return Err(AppError::NoActiveSession);
        }
        let relay_generation = self.state.sharing.host_relays.allocate_generation()?;
        prepared.bind_relay_generation(relay_generation)?;
        prepared.state = ShareTransitionState::RelayPending {
            key_generation,
            relay_generation,
        };
        prepared.validate()?;
        self.share_transitions.put(prepared.clone())?;
        self.start_relay_prepare_worker(
            RelayPrepareOwner::Share { prepared },
            Some(key_generation),
            sender_signing_pkcs8,
        )
    }

    fn share_relay_completion_matches(
        &self,
        prepared: &PreparedShareTransition,
        account_epoch: u64,
        source_key_generation: Option<u32>,
        relay_generation: u64,
        fence: Option<&RelayPrepareFence>,
    ) -> bool {
        let ledger_matches = self
            .share_transitions
            .get(&prepared.account_user_id, prepared.transition_id)
            .ok()
            .flatten()
            .is_some_and(|current| {
                current.fingerprint == prepared.fingerprint
                    && account_epoch == prepared.originating_account_epoch
                    && matches!(
                        current.state,
                        ShareTransitionState::RelayPending {
                            key_generation,
                            relay_generation: expected_relay,
                        } if Some(key_generation) == source_key_generation
                            && expected_relay == relay_generation
                    )
            });
        let shared = self
            .state
            .sharing
            .shared_sessions
            .get(prepared.runtime_session_id);
        let session_live = self
            .state
            .local
            .sessions
            .record(prepared.runtime_session_id)
            .is_some_and(|record| {
                !matches!(
                    record.summary.state,
                    SessionState::Stopping | SessionState::Stopped | SessionState::Failed
                )
            });
        ledger_matches
            && session_live
            && fence.is_some_and(|fence| {
                crate::runtime::relay_prepare_worker::exact_fence_matches(
                    fence,
                    self.state.identity.auth.subject_string().as_deref(),
                    self.state.identity.account_epoch().value(),
                    self.state
                        .local
                        .sessions
                        .record(prepared.runtime_session_id)
                        .map(|record| record.local_incarnation_id),
                    shared.map(
                        crate::sharing::shared_session_registry::SharedSessionState::backend_session_id,
                    ),
                    shared.map(|state| *state.backend_incarnation_id()),
                    shared.and_then(
                        crate::sharing::shared_session_registry::SharedSessionState::session_key_generation,
                    ),
                    self.state
                        .sharing
                        .host_relays
                        .generation(prepared.runtime_session_id),
                )
            })
    }

    pub(crate) fn apply_share_relay_prepare_completion(
        &mut self,
        prepared: &PreparedShareTransition,
        account_epoch: u64,
        source_key_generation: Option<u32>,
        relay_generation: u64,
        fence: Option<&RelayPrepareFence>,
        outcome: Result<RelayPrepareCommit>,
    ) {
        if !self.share_relay_completion_matches(
            prepared,
            account_epoch,
            source_key_generation,
            relay_generation,
            fence,
        ) {
            self.state.record_log(format!(
                "ignored stale share relay preparation completion {}",
                prepared.transition_id
            ));
            return;
        }
        let commit = match outcome {
            Ok(commit) => commit,
            Err(error) => {
                self.fail_share_transition(prepared, error.to_string());
                return;
            }
        };
        if let Err(error) =
            self.activate_prepared_host_relay(prepared.runtime_session_id, relay_generation, commit)
        {
            self.fail_share_transition(prepared, error.to_string());
            return;
        }
        let commit_ready = match self
            .persist_share_transition_state(prepared, ShareTransitionState::CommitReady)
        {
            Ok(value) => value,
            Err(error) => {
                self.state
                    .sharing
                    .host_relays
                    .teardown(prepared.runtime_session_id);
                self.fail_share_transition(prepared, error.to_string());
                return;
            }
        };
        if let Err(error) = self.apply_share_target_locally(&commit_ready) {
            self.state
                .sharing
                .host_relays
                .teardown(prepared.runtime_session_id);
            self.fail_share_transition(prepared, error.to_string());
            return;
        }
        if let Err(error) = self.finish_share_transition(
            &commit_ready,
            ShareTransitionTerminalStatus::Applied,
            None,
            DeferredShareVerdict::ScopeChanged,
        ) {
            self.state.record_log(error.to_string());
        }
    }

    fn apply_share_target_locally(&mut self, prepared: &PreparedShareTransition) -> Result<()> {
        let session_id = prepared.runtime_session_id;
        let previous_scope = self
            .state
            .local
            .sessions
            .record(session_id)
            .ok_or(AppError::NoActiveSession)?
            .summary
            .scope;
        let previous_room_name = self
            .state
            .local
            .sessions
            .record(session_id)
            .and_then(|record| record.summary.room_name.clone());
        self.state
            .apply_session_scope_locally(session_id, prepared.target.scope);
        if let Some(record) = self.state.local.sessions.record_mut(session_id) {
            record.summary.room_name = prepared.target.room_id.as_ref().and_then(|room_id| {
                self.state
                    .available_rooms
                    .iter()
                    .find(|room| room.id == *room_id)
                    .map(|room| room.name.clone())
            });
        }
        let persist_result = self
            .state
            .local
            .sessions
            .record(session_id)
            .ok_or(AppError::NoActiveSession)
            .and_then(|record| self.local_catalog.persist_record(record));
        if let Err(error) = persist_result {
            self.state
                .apply_session_scope_locally(session_id, previous_scope);
            if let Some(record) = self.state.local.sessions.record_mut(session_id) {
                record.summary.room_name = previous_room_name;
            }
            return Err(error);
        }
        self.state.publish_session_if_host_relay_active(session_id);
        self.state.snapshot_refresh_pending = true;
        Ok(())
    }

    fn fail_share_transition(&mut self, prepared: &PreparedShareTransition, reason: String) {
        self.settle_failed_share_transition(
            prepared,
            ShareTransitionTerminalStatus::Rejected,
            reason,
        );
    }

    fn cancel_share_transition(&mut self, prepared: &PreparedShareTransition, reason: String) {
        self.settle_failed_share_transition(
            prepared,
            ShareTransitionTerminalStatus::Cancelled,
            reason,
        );
    }

    fn settle_failed_share_transition(
        &mut self,
        prepared: &PreparedShareTransition,
        status: ShareTransitionTerminalStatus,
        reason: String,
    ) {
        let message = bounded_transition_message(reason);
        let terminal = ShareTransitionTerminal {
            status,
            message: Some(message),
        };
        let mut cleanup = self
            .share_transitions
            .get(&prepared.account_user_id, prepared.transition_id)
            .ok()
            .flatten()
            .cloned()
            .unwrap_or_else(|| prepared.clone());
        cleanup.state = ShareTransitionState::CleanupPending(terminal);
        let persisted = match self.share_transitions.put(cleanup.clone()) {
            Ok(()) => true,
            Err(error) => {
                self.share_transition_settlement_retry.insert(
                    prepared.transition_id,
                    DeferredShareSettlement {
                        entry: cleanup.clone(),
                        verdict: DeferredShareVerdict::Error,
                    },
                );
                self.state.record_log(error.to_string());
                false
            }
        };
        self.state
            .unshare_session_locally(prepared.runtime_session_id);
        self.last_maintenance_ran = None;
        if persisted {
            self.share_transition_deadlines
                .remove(&prepared.transition_id);
            if self
                .share_transition_in_flight
                .get(&prepared.runtime_session_id)
                == Some(&prepared.transition_id)
            {
                self.share_transition_in_flight
                    .remove(&prepared.runtime_session_id);
            }
            self.emit_deferred_share_verdict(&cleanup, DeferredShareVerdict::Error);
        }
    }

    fn persist_share_transition_state(
        &mut self,
        prepared: &PreparedShareTransition,
        state: ShareTransitionState,
    ) -> Result<PreparedShareTransition> {
        let mut updated = prepared.clone();
        updated.state = state;
        self.share_transitions.put(updated.clone())?;
        Ok(updated)
    }

    pub(super) fn finish_share_transition(
        &mut self,
        prepared: &PreparedShareTransition,
        status: ShareTransitionTerminalStatus,
        message: Option<String>,
        verdict: DeferredShareVerdict,
    ) -> Result<()> {
        let mut terminal = prepared.clone();
        terminal.state = ShareTransitionState::Terminal(ShareTransitionTerminal {
            status,
            message: message.map(bounded_transition_message),
        });
        if let Err(error) = self.share_transitions.put(terminal.clone()) {
            self.share_transition_settlement_retry.insert(
                prepared.transition_id,
                DeferredShareSettlement {
                    entry: terminal,
                    verdict,
                },
            );
            self.last_maintenance_ran = None;
            return Err(error);
        }
        self.share_transition_settlement_retry
            .remove(&prepared.transition_id);
        self.share_transition_deadlines
            .remove(&prepared.transition_id);
        if self
            .share_transition_in_flight
            .get(&prepared.runtime_session_id)
            == Some(&prepared.transition_id)
        {
            self.share_transition_in_flight
                .remove(&prepared.runtime_session_id);
        }
        self.emit_deferred_share_verdict(prepared, verdict);
        Ok(())
    }

    fn emit_deferred_share_verdict(
        &mut self,
        prepared: &PreparedShareTransition,
        verdict: DeferredShareVerdict,
    ) {
        match verdict {
            DeferredShareVerdict::ScopeChanged => self.queue_share_scope_changed(prepared),
            DeferredShareVerdict::Error => {
                let message = match &prepared.state {
                    ShareTransitionState::CleanupPending(terminal)
                    | ShareTransitionState::Terminal(terminal) => terminal
                        .message
                        .clone()
                        .unwrap_or_else(|| "share transition did not apply".to_owned()),
                    _ => "share transition did not apply".to_owned(),
                };
                self.state
                    .runtime_outbox
                    .queue_session(crate::SessionEvent::Error {
                        operation: "session.scope".to_owned(),
                        session_id: Some(prepared.runtime_session_id.to_string()),
                        request_id: Some(prepared.transition_id.to_string()),
                        message,
                    });
                self.state.snapshot_refresh_pending = true;
            }
            DeferredShareVerdict::None => {}
        }
    }

    fn replay_share_transition_terminal(&mut self, prepared: &PreparedShareTransition) {
        match &prepared.state {
            ShareTransitionState::Terminal(ShareTransitionTerminal {
                status: ShareTransitionTerminalStatus::Applied,
                ..
            }) => self.queue_share_scope_changed(prepared),
            ShareTransitionState::CleanupPending(terminal)
            | ShareTransitionState::Terminal(terminal) => {
                self.state
                    .runtime_outbox
                    .queue_session(crate::SessionEvent::Error {
                        operation: "session.scope".to_owned(),
                        session_id: Some(prepared.runtime_session_id.to_string()),
                        request_id: Some(prepared.transition_id.to_string()),
                        message: terminal
                            .message
                            .clone()
                            .unwrap_or_else(|| "share transition did not apply".to_owned()),
                    });
            }
            _ => {}
        }
    }

    fn queue_share_scope_changed(&mut self, prepared: &PreparedShareTransition) {
        self.state
            .runtime_outbox
            .queue_session(crate::SessionEvent::ScopeChanged {
                request_id: prepared.transition_id.to_string(),
                session_id: prepared.runtime_session_id.to_string(),
                expected_runtime_incarnation_id: prepared
                    .expected_runtime_incarnation_id
                    .to_string(),
                scope: prepared.target.scope,
                room_id: prepared.target.room_id.clone(),
            });
        self.state.snapshot_refresh_pending = true;
    }

    pub(crate) fn share_transition_active_for_session(
        &self,
        account_user_id: &str,
        session_id: SessionId,
    ) -> bool {
        self.share_transition_in_flight.contains_key(&session_id)
            || self
                .share_transitions
                .entries(account_user_id)
                .map_or(true, |entries| {
                    entries.into_iter().any(|entry| {
                        entry.runtime_session_id == session_id
                            && !matches!(entry.state, ShareTransitionState::Terminal(_))
                    })
                })
    }

    fn preempt_collaboration_for_unshare(&mut self, session_id: SessionId) {
        self.cancel_share_transition_for_session(
            session_id,
            "sharing was turned off during another share transition",
        );
        if matches!(
            self.relay_prepare_owners.get(&session_id),
            Some(
                crate::runtime::relay_prepare_worker::RelayPrepareOwnerKind::Access
                    | crate::runtime::relay_prepare_worker::RelayPrepareOwnerKind::Maintenance
            )
        ) {
            if let Some(task) = self.relay_prepare_tasks.remove(&session_id) {
                task.abort();
            }
            self.relay_prepare_owners.remove(&session_id);
        }
        self.retire_access_mutations_for_session(
            session_id,
            "sharing was turned off before this access change settled",
        );
    }

    pub(super) fn collaboration_change_active(
        &self,
        account_user_id: &str,
        session_id: SessionId,
    ) -> Result<bool> {
        let active_share = self
            .share_transitions
            .entries(account_user_id)?
            .into_iter()
            .any(|entry| {
                entry.runtime_session_id == session_id
                    && !matches!(entry.state, ShareTransitionState::Terminal(_))
            });
        let active_access = self
            .access_mutations
            .entries(account_user_id)?
            .into_iter()
            .any(|entry| {
                entry.runtime_session_id == session_id && entry.state.holds_local_authority()
            });
        Ok(active_share || active_access)
    }

    fn ensure_collaboration_operation_available(
        &self,
        account_user_id: &str,
        session_id: SessionId,
    ) -> Result<()> {
        if self.collaboration_change_active(account_user_id, session_id)? {
            return Err(AppError::Unsupported {
                reason: format!(
                    "session {} already has a collaboration change in flight",
                    session_id.short()
                ),
            });
        }
        Ok(())
    }

    pub(crate) fn share_transition_holds_cleanup(&self, create_id: Uuid) -> bool {
        let Some(account) = self.state.identity.auth.subject_string() else {
            return false;
        };
        self.share_transitions
            .entries(&account)
            .map_or(true, |entries| {
                entries.into_iter().any(|entry| {
                    entry
                        .cleanup
                        .as_ref()
                        .is_some_and(|cleanup| cleanup.create_idempotency_id == create_id)
                        && !matches!(
                            entry.state,
                            ShareTransitionState::CleanupPending(_)
                                | ShareTransitionState::Terminal(_)
                        )
                })
            })
    }

    pub(crate) fn expire_share_transition_deadlines(&mut self) {
        let now = std::time::Instant::now();
        let expired = self
            .share_transition_deadlines
            .iter()
            .filter_map(|(transition_id, deadline)| (now >= *deadline).then_some(*transition_id))
            .collect::<Vec<_>>();
        for transition_id in expired {
            let prepared = self
                .state
                .identity
                .auth
                .subject_string()
                .and_then(|account| {
                    self.share_transitions
                        .get(&account, transition_id)
                        .ok()
                        .flatten()
                        .cloned()
                });
            let Some(prepared) = prepared else {
                self.share_transition_deadlines.remove(&transition_id);
                continue;
            };
            if matches!(prepared.state, ShareTransitionState::Terminal(_)) {
                self.share_transition_deadlines.remove(&transition_id);
                continue;
            }
            self.share_transition_settlement_retry
                .remove(&transition_id);
            if let Some(task) = self.share_transition_tasks.remove(&transition_id) {
                task.abort();
            }
            if self.relay_prepare_owners.get(&prepared.runtime_session_id)
                == Some(&crate::runtime::relay_prepare_worker::RelayPrepareOwnerKind::Share)
            {
                if let Some(task) = self
                    .relay_prepare_tasks
                    .remove(&prepared.runtime_session_id)
                {
                    task.abort();
                }
                self.relay_prepare_owners
                    .remove(&prepared.runtime_session_id);
            }
            self.fail_share_transition(
                &prepared,
                "share transition exceeded its execution deadline".to_owned(),
            );
        }
    }

    pub(crate) fn retry_share_transition_settlements(&mut self) {
        let retries = self
            .share_transition_settlement_retry
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for retry in retries {
            if self.share_transitions.put(retry.entry.clone()).is_err() {
                continue;
            }
            self.share_transition_settlement_retry
                .remove(&retry.entry.transition_id);
            self.share_transition_deadlines
                .remove(&retry.entry.transition_id);
            if self
                .share_transition_in_flight
                .get(&retry.entry.runtime_session_id)
                == Some(&retry.entry.transition_id)
            {
                self.share_transition_in_flight
                    .remove(&retry.entry.runtime_session_id);
            }
            self.emit_deferred_share_verdict(&retry.entry, retry.verdict);
        }
    }

    pub(crate) fn recover_share_transitions(&mut self) {
        let Some(account) = self.state.identity.auth.subject_string() else {
            return;
        };
        let entries = match self.share_transitions.entries(&account) {
            Ok(entries) => entries,
            Err(error) => {
                self.state.record_log(error.to_string());
                return;
            }
        };
        for entry in entries {
            if matches!(entry.state, ShareTransitionState::Terminal(_)) {
                continue;
            }
            self.share_transition_in_flight
                .insert(entry.runtime_session_id, entry.transition_id);
            let message = "share transition was interrupted and is being cleaned up".to_owned();
            if entry.cleanup.is_none() {
                if let Err(error) = self.finish_share_transition(
                    &entry,
                    ShareTransitionTerminalStatus::Cancelled,
                    Some(message),
                    DeferredShareVerdict::None,
                ) {
                    self.state.record_log(error.to_string());
                }
                continue;
            }
            let mut cleanup = entry.clone();
            cleanup.state = ShareTransitionState::CleanupPending(ShareTransitionTerminal {
                status: ShareTransitionTerminalStatus::Cancelled,
                message: Some(message),
            });
            if let Err(error) = self.share_transitions.put(cleanup) {
                self.state.record_log(error.to_string());
                continue;
            }
            self.state.unshare_session_locally(entry.runtime_session_id);
        }
        self.last_maintenance_ran = None;
    }

    pub(crate) fn process_share_transition_cleanup(&mut self) {
        let Some(account) = self.state.identity.auth.subject_string() else {
            return;
        };
        let entries = match self.share_transitions.entries(&account) {
            Ok(entries) => entries,
            Err(error) => {
                self.state.record_log(error.to_string());
                return;
            }
        };
        for entry in entries {
            let ShareTransitionState::CleanupPending(terminal) = &entry.state else {
                continue;
            };
            let retained_cleanup = entry.cleanup.as_ref().is_some_and(|cleanup| {
                self.collaboration_teardown
                    .contains_create_id(cleanup.create_idempotency_id)
                    .unwrap_or(true)
            });
            if retained_cleanup {
                continue;
            }
            if let Err(error) = self.finish_share_transition(
                &entry,
                terminal.status,
                terminal.message.clone(),
                DeferredShareVerdict::None,
            ) {
                self.state.record_log(error.to_string());
            }
        }
    }

    pub(crate) fn cancel_share_transition_for_session(
        &mut self,
        session_id: SessionId,
        reason: &str,
    ) {
        let Some(transition_id) = self.share_transition_in_flight.get(&session_id).copied() else {
            return;
        };
        if self.relay_prepare_owners.get(&session_id)
            == Some(&crate::runtime::relay_prepare_worker::RelayPrepareOwnerKind::Share)
        {
            if let Some(task) = self.relay_prepare_tasks.remove(&session_id) {
                task.abort();
            }
            self.relay_prepare_owners.remove(&session_id);
        }
        if let Some(task) = self.share_transition_tasks.remove(&transition_id) {
            task.abort();
        }
        let Some(account) = self.state.identity.auth.subject_string() else {
            return;
        };
        let prepared = self
            .share_transitions
            .get(&account, transition_id)
            .ok()
            .flatten()
            .cloned();
        if let Some(prepared) = prepared {
            self.cancel_share_transition(&prepared, reason.to_owned());
        }
    }
}

fn bounded_transition_message(mut message: String) -> String {
    const LIMIT: usize = 16 * 1024;
    if message.len() <= LIMIT {
        return message;
    }
    let mut boundary = LIMIT;
    while !message.is_char_boundary(boundary) {
        boundary -= 1;
    }
    message.truncate(boundary);
    message
}
