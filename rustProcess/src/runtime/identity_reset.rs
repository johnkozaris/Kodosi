use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};

use crate::{AppError, Result};
use kodosi_backend_client::{
    BackendClientError, api::IdentityResetProof, crypto::sign_pop_challenge,
};

use super::Runtime;

const INTENT_VERSION: u32 = 2;
const MAX_INTENT_BYTES: u64 = 4 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum IdentityResetPhase {
    DeleteRequired,
    LocalCleanupRequired,
    DurabilityUncertain,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IdentityResetIntent {
    version: u32,
    backend_origin: String,
    account_user_id: String,
    phase: IdentityResetPhase,
}

#[derive(Debug, Clone)]
pub(crate) struct IdentityResetIntentStore {
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IdentityResetResolution {
    Clear,
    Pending(String),
}

impl IdentityResetIntentStore {
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn load_default() -> Result<Self> {
        Ok(Self {
            path: crate::support::storage::paths::identity_reset_intent_path()?,
        })
    }

    pub(crate) fn at(path: PathBuf) -> Self {
        Self { path }
    }

    #[cfg(test)]
    fn seed_for_test(
        &self,
        backend_origin: &str,
        account_user_id: &str,
        phase: IdentityResetPhase,
    ) -> Result<()> {
        self.persist_new(&IdentityResetIntent {
            version: INTENT_VERSION,
            backend_origin: backend_origin.to_owned(),
            account_user_id: account_user_id.to_owned(),
            phase,
        })
    }

    #[cfg(test)]
    pub(crate) fn seed_local_cleanup_for_test(
        &self,
        backend_origin: &str,
        account_user_id: &str,
    ) -> Result<()> {
        self.seed_for_test(
            backend_origin,
            account_user_id,
            IdentityResetPhase::LocalCleanupRequired,
        )
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn seed_delete_required_for_test(
        &self,
        backend_origin: &str,
        account_user_id: &str,
    ) -> Result<()> {
        self.seed_for_test(
            backend_origin,
            account_user_id,
            IdentityResetPhase::DeleteRequired,
        )
    }

    #[cfg(test)]
    pub(crate) fn has_pending_for_test(&self) -> Result<bool> {
        self.load().map(|intent| intent.is_some())
    }

    fn load(&self) -> Result<Option<IdentityResetIntent>> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(AppError::Io(error)),
        };
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_INTENT_BYTES {
            return Err(AppError::InvalidBackendData {
                field: "identityResetIntent".to_owned(),
                reason: "local reset intent exceeds its size limit".to_owned(),
            });
        }
        let intent: IdentityResetIntent = serde_json::from_slice(&bytes)?;
        if intent.version != INTENT_VERSION {
            return Err(AppError::InvalidBackendData {
                field: "identityResetIntent.version".to_owned(),
                reason: format!("unsupported reset intent version {}", intent.version),
            });
        }
        kodosi_domain::ids::UserId::try_from(intent.account_user_id.as_str()).map_err(|error| {
            AppError::InvalidBackendData {
                field: "identityResetIntent.accountUserId".to_owned(),
                reason: error.to_string(),
            }
        })?;
        let origin: kodosi_backend_client::BackendOrigin =
            intent
                .backend_origin
                .parse()
                .map_err(|error: BackendClientError| AppError::InvalidBackendData {
                    field: "identityResetIntent.backendOrigin".to_owned(),
                    reason: error.to_string(),
                })?;
        if origin.to_string() != intent.backend_origin {
            return Err(AppError::InvalidBackendData {
                field: "identityResetIntent.backendOrigin".to_owned(),
                reason: "reset intent backend origin is not canonical".to_owned(),
            });
        }
        Ok(Some(intent))
    }

    fn begin(&self, backend_origin: &str, account_user_id: &str) -> Result<IdentityResetIntent> {
        if let Some(existing) = self.load()? {
            if existing.backend_origin != backend_origin
                || existing.account_user_id != account_user_id
            {
                return Err(AppError::Unsupported {
                    reason: "another backend account has an unfinished identity reset".to_owned(),
                });
            }
            if existing.phase != IdentityResetPhase::DurabilityUncertain {
                return Ok(existing);
            }
            let retry = IdentityResetIntent {
                phase: IdentityResetPhase::DeleteRequired,
                ..existing
            };
            self.replace(&retry)?;
            return Ok(retry);
        }
        let intent = IdentityResetIntent {
            version: INTENT_VERSION,
            backend_origin: backend_origin.to_owned(),
            account_user_id: account_user_id.to_owned(),
            phase: IdentityResetPhase::DeleteRequired,
        };
        self.persist_new(&intent)?;
        Ok(intent)
    }

    fn persist_new(&self, intent: &IdentityResetIntent) -> Result<()> {
        match self.replace(intent) {
            Ok(()) => Ok(()),
            Err(commit_error) => {
                let unresolved = IdentityResetIntent {
                    phase: IdentityResetPhase::DurabilityUncertain,
                    ..intent.clone()
                };
                if self.overwrite_visible_file(&unresolved).is_ok() {
                    return Err(AppError::Unsupported {
                        reason: format!(
                            "identity reset intent durability is unresolved; remote identity remains fenced: {commit_error}"
                        ),
                    });
                }
                match fs::remove_file(&self.path) {
                    Ok(()) => {
                        drop(sync_parent(&self.path));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => {}
                }
                Err(commit_error)
            }
        }
    }

    fn replace(&self, intent: &IdentityResetIntent) -> Result<()> {
        crate::support::storage::atomic_file::atomic_write_json(
            &self.path,
            intent,
            true,
            crate::support::storage::atomic_file::FileMode::UserPrivate,
        )
    }

    fn overwrite_visible_file(&self, intent: &IdentityResetIntent) -> Result<()> {
        let payload = serde_json::to_vec_pretty(intent)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&self.path)?;
        crate::support::platform::fs::set_file_permissions(&self.path)?;
        file.write_all(&payload)?;
        file.sync_all().map_err(AppError::Io)
    }

    fn transition(
        &self,
        expected: &IdentityResetIntent,
        phase: IdentityResetPhase,
    ) -> Result<IdentityResetIntent> {
        if self.load()?.as_ref() != Some(expected) {
            return Err(AppError::InvalidBackendData {
                field: "identityResetIntent".to_owned(),
                reason: "reset intent changed before phase transition".to_owned(),
            });
        }
        let next = IdentityResetIntent {
            phase,
            ..expected.clone()
        };
        self.replace(&next)?;
        Ok(next)
    }

    fn complete(&self, expected: &IdentityResetIntent) -> Result<()> {
        if self.load()?.as_ref() != Some(expected) {
            return Err(AppError::InvalidBackendData {
                field: "identityResetIntent".to_owned(),
                reason: "reset intent changed before completion".to_owned(),
            });
        }
        match fs::remove_file(&self.path) {
            Ok(()) => sync_parent(&self.path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(AppError::Io(error)),
        }
    }
}

fn sync_parent(path: &Path) -> Result<()> {
    let parent = path.parent().ok_or_else(|| AppError::Unsupported {
        reason: "identity reset intent path has no parent".to_owned(),
    })?;
    fs::File::open(parent)?.sync_all().map_err(AppError::Io)
}

pub(crate) async fn resolve_pending(app: &mut Runtime) -> Result<IdentityResetResolution> {
    let Some(mut intent) = app.identity_reset_intent.load()? else {
        return Ok(IdentityResetResolution::Clear);
    };
    let Some(configured_origin) = app.backend.backend_origin().map(ToString::to_string) else {
        return Ok(fence_pending(
            app,
            &intent.account_user_id,
            "Identity reset is pending, but the original backend is not configured.",
        ));
    };
    if configured_origin != intent.backend_origin {
        return Ok(fence_pending(
            app,
            &intent.account_user_id,
            format!(
                "Identity reset for {} is quarantined because the configured backend is {}.",
                intent.backend_origin, configured_origin
            ),
        ));
    }

    match intent.phase {
        IdentityResetPhase::DurabilityUncertain => {
            return Ok(fence_pending(
                app,
                &intent.account_user_id,
                "Identity reset intent durability is unresolved. Retry Reset identity explicitly.",
            ));
        }
        IdentityResetPhase::Cancelled => {
            app.identity_reset_intent.complete(&intent)?;
            return Ok(IdentityResetResolution::Clear);
        }
        IdentityResetPhase::DeleteRequired => {
            let access = install_reset_backend_access(app, &intent).await?;
            if let Some(reason) = access {
                return Ok(fence_pending(app, &intent.account_user_id, reason));
            }
            match dispatch_backend_delete(app, &intent).await {
                Ok(()) => {
                    intent = app
                        .identity_reset_intent
                        .transition(&intent, IdentityResetPhase::LocalCleanupRequired)?;
                }
                Err(ResetDeleteError::RetryRequired(reason)) => {
                    return Ok(fence_pending(
                        app,
                        &intent.account_user_id,
                        format!("Identity reset backend outcome is unresolved: {reason}"),
                    ));
                }
                Err(ResetDeleteError::Rejected(error)) => {
                    let cancelled = app
                        .identity_reset_intent
                        .transition(&intent, IdentityResetPhase::Cancelled)?;
                    app.identity_reset_intent.complete(&cancelled)?;
                    app.backend.set_access_token(None);
                    return Err(error.into());
                }
            }
        }
        IdentityResetPhase::LocalCleanupRequired => {}
    }

    match finish_local_reset(app, &intent).await {
        Ok(()) => {
            app.identity_reset_intent.complete(&intent)?;
            Ok(IdentityResetResolution::Clear)
        }
        Err(error) => Ok(fence_pending(
            app,
            &intent.account_user_id,
            format!("Identity reset local cleanup remains pending: {error}"),
        )),
    }
}

pub(crate) async fn reset_identity(app: &mut Runtime) -> Result<()> {
    if app.identity_reset_intent.load()?.is_some() {
        return match resolve_pending(app).await? {
            IdentityResetResolution::Clear => Ok(()),
            IdentityResetResolution::Pending(reason) => Err(AppError::Unsupported { reason }),
        };
    }
    super::auth::ensure_collaboration_cleanup_ready(app)?;
    let user_id = app
        .state
        .identity
        .auth
        .subject_string()
        .ok_or(AppError::Unauthorized)?;
    let backend_origin = app
        .backend
        .backend_origin()
        .ok_or_else(|| AppError::Unsupported {
            reason: "backend is not configured".to_owned(),
        })?
        .to_string();
    super::auth::ensure_remote_operation_access(app).await?;
    app.identity_reset_intent
        .begin(&backend_origin, &user_id)
        .map(drop)?;
    match resolve_pending(app).await? {
        IdentityResetResolution::Clear => Ok(()),
        IdentityResetResolution::Pending(reason) => Err(AppError::Unsupported { reason }),
    }
}

async fn install_reset_backend_access(
    app: &mut Runtime,
    intent: &IdentityResetIntent,
) -> Result<Option<String>> {
    use crate::identity_core::{
        stored_auth::{BackendAccessState, StoredAuthAccessor},
        token_store::TokenStore as _,
    };

    let Some(stored) = app.token_store.load(super::DEFAULT_TOKEN_SUBJECT)? else {
        return Ok(Some(
            "Identity reset is pending and requires the original stored sign-in.".to_owned(),
        ));
    };
    if stored.backend_origin.as_deref() != Some(intent.backend_origin.as_str())
        || stored.backend_user_id.as_deref() != Some(intent.account_user_id.as_str())
    {
        return Ok(Some(
            "Identity reset is pending, but stored credentials belong to another backend account."
                .to_owned(),
        ));
    }
    let access = {
        let mut accessor = StoredAuthAccessor::new(
            &mut app.backend,
            app.state.identity.device_flow.client(),
            &app.token_store,
            super::DEFAULT_TOKEN_SUBJECT,
            app.state.config.auth_refresh_skew_minutes,
        );
        accessor.ensure_backend_access().await?
    };
    match access {
        BackendAccessState::Ready { .. } | BackendAccessState::TemporarilyUnavailable { .. } => {
            Ok(None)
        }
        BackendAccessState::SignedOut => Ok(Some(
            "Identity reset is pending and no stored sign-in is available.".to_owned(),
        )),
        BackendAccessState::RequiresLogin(reason)
        | BackendAccessState::StorageUnavailable(reason) => Ok(Some(format!(
            "Identity reset is pending and backend access could not be restored: {reason}"
        ))),
    }
}

enum ResetDeleteError {
    RetryRequired(String),
    Rejected(BackendClientError),
}

async fn dispatch_backend_delete(
    app: &mut Runtime,
    intent: &IdentityResetIntent,
) -> std::result::Result<(), ResetDeleteError> {
    let proof = build_pop_request(app, &intent.account_user_id)
        .await
        .map_err(|error| ResetDeleteError::RetryRequired(error.to_string()))?;
    let worker = app.backend.clone();
    match worker.delete_my_identity(proof.as_ref()).await {
        Ok(()) => Ok(()),
        Err(BackendClientError::Unauthorized) => {
            let rejected = worker.into_access_token();
            match super::auth::refresh_access_token_after_rejection(
                app,
                rejected.as_deref().map(String::as_str),
            )
            .await
            .map_err(|error| ResetDeleteError::RetryRequired(error.to_string()))?
            {
                crate::identity_core::stored_auth::RefreshStoredAuthResult::Refreshed(_) => {
                    let proof = build_pop_request(app, &intent.account_user_id)
                        .await
                        .map_err(|error| ResetDeleteError::RetryRequired(error.to_string()))?;
                    app.backend
                        .delete_my_identity(proof.as_ref())
                        .await
                        .map_err(classify_delete_error)
                }
                crate::identity_core::stored_auth::RefreshStoredAuthResult::RequiresLogin(reason)
                | crate::identity_core::stored_auth::RefreshStoredAuthResult::TemporarilyUnavailable(
                    reason,
                ) => Err(ResetDeleteError::RetryRequired(reason.to_string())),
            }
        }
        Err(error) => Err(classify_delete_error(error)),
    }
}

fn classify_delete_error(error: BackendClientError) -> ResetDeleteError {
    if error.is_indeterminate_write() {
        ResetDeleteError::RetryRequired(error.to_string())
    } else {
        ResetDeleteError::Rejected(error)
    }
}

fn app_error_as_backend(error: &AppError) -> BackendClientError {
    BackendClientError::Protocol {
        reason: error.to_string(),
    }
}

async fn build_pop_request(
    app: &Runtime,
    user_id: &str,
) -> std::result::Result<Option<IdentityResetProof>, BackendClientError> {
    let keys = app
        .device_key_store
        .load_if_present(user_id)
        .map_err(|error| app_error_as_backend(&error))?;
    let Some(keys) = keys else {
        return Ok(None);
    };
    let challenge = app.backend.fetch_device_registration_challenge().await?;
    let challenge_bytes = BASE64.decode(&challenge.challenge_bytes).map_err(|_| {
        BackendClientError::InvalidBackendData {
            field: "deviceRegistration.challengeBytes".to_owned(),
            reason: "base64 decode failed".to_owned(),
        }
    })?;
    let signing_key = keys
        .signing_key()
        .map_err(|error| app_error_as_backend(&error))?;
    let signature = sign_pop_challenge(&signing_key, &challenge_bytes)?;
    Ok(Some(IdentityResetProof {
        challenge_id: challenge.challenge_id,
        signer_device_id: keys.device_id.clone(),
        pop_signature: BASE64.encode(&signature),
    }))
}

fn fence_pending(
    app: &mut Runtime,
    account_user_id: &str,
    reason: impl Into<String>,
) -> IdentityResetResolution {
    let reason = reason.into();
    super::auth::fence_account_for_pending_reset(app, account_user_id);
    app.state.record_log(reason.clone());
    IdentityResetResolution::Pending(reason)
}

async fn finish_local_reset(app: &mut Runtime, intent: &IdentityResetIntent) -> Result<()> {
    let user_id = &intent.account_user_id;
    let mut first_error =
        app.device_key_store
            .wipe(user_id)
            .err()
            .map(|error| AppError::Unsupported {
                reason: format!("identity reset: device key wipe failed: {error}"),
            });
    match app.pin_store.bind_to_user(user_id).await {
        Ok(_) => {
            if let Err(error) = app.pin_store.reset_all().await
                && first_error.is_none()
            {
                first_error = Some(AppError::Unsupported {
                    reason: format!("identity reset: pin store reset failed: {error}"),
                });
            }
        }
        Err(error) => {
            if first_error.is_none() {
                first_error = Some(error);
            }
        }
    }
    if let Err(error) =
        crate::room_crypto::reset_roster_pins_for_account_at(&app.room_roster_pins_path, user_id)
        && first_error.is_none()
    {
        first_error = Some(AppError::Unsupported {
            reason: format!("identity reset: room roster pin reset failed: {error}"),
        });
    }
    if let Err(error) = app.hidden_session_store.clear_user(user_id)
        && first_error.is_none()
    {
        first_error = Some(AppError::Unsupported {
            reason: format!("identity reset: hidden-session state wipe failed: {error}"),
        });
    }
    if let Err(error) = app.owner_action_results.clear_account(user_id)
        && first_error.is_none()
    {
        first_error = Some(AppError::Unsupported {
            reason: format!("identity reset: owner action-result ledger wipe failed: {error}"),
        });
    }
    if let Err(error) = app.remote_permission_actions.clear_account(user_id)
        && first_error.is_none()
    {
        first_error = Some(AppError::Unsupported {
            reason: format!("identity reset: remote permission-action ledger wipe failed: {error}"),
        });
    }
    if let Err(error) = app.remote_semantics.clear_account(user_id)
        && first_error.is_none()
    {
        first_error = Some(AppError::Unsupported {
            reason: format!("identity reset: remote semantic ledger wipe failed: {error}"),
        });
    }
    if let Err(error) = app.state.steering.clear_account(user_id)
        && first_error.is_none()
    {
        first_error = Some(AppError::Unsupported {
            reason: format!("identity reset: semantic-send ledger wipe failed: {error}"),
        });
    }

    super::auth::retire_local_collaboration_authority(app);
    app.state.discovery.drain_all();
    if let Err(error) = super::auth::logout(app).await {
        if first_error.is_none() {
            first_error = Some(error);
        }
        super::auth::set_signed_out(app)?;
    }
    first_error.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const ORIGIN_A: &str = "https://api-a.example.test:443/";
    const ORIGIN_B: &str = "https://api-b.example.test:443/";
    const ACCOUNT_A: &str = "11111111-1111-1111-1111-111111111111";
    const ACCOUNT_B: &str = "22222222-2222-2222-2222-222222222222";

    #[test]
    fn intent_round_trips_with_private_mode_and_no_secrets() {
        let dir = tempfile::tempdir().expect("intent directory");
        let path = dir.path().join("identity-reset-intent.json");
        let store = IdentityResetIntentStore::at(path.clone());

        let intent = store
            .begin(ORIGIN_A, ACCOUNT_A)
            .expect("persist reset intent");

        assert_eq!(store.load().expect("read reset intent"), Some(intent));
        let payload = fs::read_to_string(&path).expect("intent payload");
        assert!(payload.contains(ACCOUNT_A));
        assert!(payload.contains(ORIGIN_A));
        assert!(!payload.contains("token"));
        assert!(!payload.contains("secret"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path)
                    .expect("intent metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn intent_schema_rejects_legacy_unknown_and_oversize_payloads() {
        let dir = tempfile::tempdir().expect("intent directory");
        let path = dir.path().join("identity-reset-intent.json");
        let store = IdentityResetIntentStore::at(path.clone());

        fs::write(
            &path,
            format!(r#"{{"version":1,"accountUserId":"{ACCOUNT_A}"}}"#),
        )
        .expect("write legacy intent");
        assert!(store.load().is_err());

        fs::write(
            &path,
            format!(
                r#"{{"version":2,"backendOrigin":"{ORIGIN_A}","accountUserId":"{ACCOUNT_A}","phase":"deleteRequired","accessToken":"forbidden"}}"#
            ),
        )
        .expect("write unknown field");
        assert!(store.load().is_err());

        fs::write(
            &path,
            vec![b'x'; usize::try_from(MAX_INTENT_BYTES).unwrap() + 1],
        )
        .expect("write oversized intent");
        assert!(store.load().is_err());
    }

    #[test]
    fn unfinished_intent_is_scoped_to_exact_backend_account() {
        let dir = tempfile::tempdir().expect("intent directory");
        let path = dir.path().join("identity-reset-intent.json");
        let store = IdentityResetIntentStore::at(path.clone());

        let first = store.begin(ORIGIN_A, ACCOUNT_A).expect("first intent");
        assert_eq!(store.begin(ORIGIN_A, ACCOUNT_A).expect("same retry"), first);
        assert!(store.begin(ORIGIN_A, ACCOUNT_B).is_err());
        assert!(store.begin(ORIGIN_B, ACCOUNT_A).is_err());
    }

    #[test]
    fn only_confirmed_phase_authorizes_local_cleanup() {
        let dir = tempfile::tempdir().expect("intent directory");
        let store = IdentityResetIntentStore::at(dir.path().join("intent.json"));
        store
            .seed_for_test(ORIGIN_A, ACCOUNT_A, IdentityResetPhase::DeleteRequired)
            .expect("delete phase");
        assert_eq!(
            store.load().expect("load").expect("intent").phase,
            IdentityResetPhase::DeleteRequired
        );
        let current = store.load().expect("load").expect("intent");
        let local = store
            .transition(&current, IdentityResetPhase::LocalCleanupRequired)
            .expect("confirmed transition");
        assert_eq!(local.phase, IdentityResetPhase::LocalCleanupRequired);
    }
}
