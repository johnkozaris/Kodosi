use std::collections::HashMap;

use kodosi_backend_client::{
    api::{SessionKeyBlobEntry, StoreSessionKeyBlobsRequest},
    control::ControlTrustEntry,
    crypto,
    http_client::{BackendHttpClient, KeyGenerationClaimOutcome},
};
use kodosi_domain::{
    auth::AuthState,
    permissions::{AccessLevel, SessionCapabilities, ShareScope},
};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{
    AppError, Result,
    identity_core::{
        device_keys::DeviceKeyStore,
        device_list_pin_store::{DeviceListPinStoreHandle, PinContext, PinVerdict},
    },
    runtime::{access_mutations::PreparedSessionAccessMutation, identity::backend_adapters},
    sharing::shared_session_registry::ExplicitGrant,
};

#[derive(Debug, Clone)]
pub(crate) struct AccessEffectServices {
    pub(crate) backend: BackendHttpClient,
    pub(crate) auth: AuthState,
    pub(crate) device_key_store: DeviceKeyStore,
    pub(crate) pin_store: DeviceListPinStoreHandle,
    pub(crate) roster_pins_path: std::path::PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct AccessEffectAudience {
    pub(crate) scope: ShareScope,
    pub(crate) room_id: Option<String>,
    pub(crate) explicit_grants: HashMap<String, ExplicitGrant>,
    pub(crate) owner_user_id: String,
}

#[derive(Debug)]
pub(crate) enum AccessEffectKind {
    Redistribute {
        session_key: crypto::SessionKey,
        key_generation: u32,
    },
    Rotate {
        previous_generation: u32,
    },
    LoadRelayIdentity {
        expected_host_device_id: String,
    },
}

#[derive(Debug)]
pub(crate) struct AccessEffectWork {
    pub(crate) prepared: PreparedSessionAccessMutation,
    pub(crate) account_epoch: u64,
    pub(crate) source_key_generation: Option<u32>,
    pub(crate) mode: crate::runtime::access_mutation_worker::SessionAccessMutationWorkerMode,
    pub(crate) access_snapshot: Option<Result<kodosi_backend_client::api::BackendAccessGrants>>,
    pub(crate) audience: AccessEffectAudience,
    pub(crate) services: AccessEffectServices,
    pub(crate) kind: AccessEffectKind,
}

#[derive(Debug)]
pub(crate) enum AccessEffectCommit {
    Distributed {
        session_key: crypto::SessionKey,
        key_generation: u32,
        control_keys: HashMap<(String, String), ControlTrustEntry>,
        sender_device_id: String,
        sender_signing_pkcs8: Zeroizing<Vec<u8>>,
        distributed: bool,
    },
    RelayIdentity {
        sender_signing_pkcs8: Zeroizing<Vec<u8>>,
    },
}

#[derive(Debug)]
pub(crate) struct AccessEffectCompletion {
    pub(crate) prepared: PreparedSessionAccessMutation,
    pub(crate) account_epoch: u64,
    pub(crate) source_key_generation: Option<u32>,
    pub(crate) mode: crate::runtime::access_mutation_worker::SessionAccessMutationWorkerMode,
    pub(crate) access_snapshot: Option<Result<kodosi_backend_client::api::BackendAccessGrants>>,
    pub(crate) outcome: Result<AccessEffectCommit>,
}

#[derive(Clone, Copy)]
struct DistributionContext<'a> {
    backend_session_id: &'a str,
    backend_incarnation_id: &'a Uuid,
    session_key: &'a crypto::SessionKey,
    key_generation: u32,
    sender_device_id: &'a str,
    owner_user_id: &'a str,
    signing_key: &'a aws_lc_rs::signature::PqdsaKeyPair,
    issued_at_ms: u64,
}

#[derive(Debug)]
pub(crate) struct ShareKeyEffectWork {
    pub(crate) backend_session_id: String,
    pub(crate) backend_incarnation_id: Uuid,
    pub(crate) previous_generation: u32,
    pub(crate) audience: AccessEffectAudience,
    pub(crate) services: AccessEffectServices,
}

#[derive(Debug)]
pub(crate) struct PreparedShareKeyDistribution {
    pub(crate) backend_session_id: String,
    pub(crate) backend_incarnation_id: Uuid,
    pub(crate) previous_generation: u32,
    pub(crate) session_key: crypto::SessionKey,
    pub(crate) sender_device_id: String,
    pub(crate) owner_user_id: String,
    pub(crate) sender_signing_pkcs8: Zeroizing<Vec<u8>>,
    pub(crate) authorized_devices: Vec<kodosi_backend_client::api::AuthorizedDeviceDto>,
    pub(crate) authorized_recipients: HashMap<String, AccessLevel>,
    pub(crate) verified_identities:
        HashMap<String, crate::identity_core::device_list_pin_store::IdentityBundleView>,
}

#[derive(Debug)]
pub(crate) struct ClaimedShareKeyDistribution {
    pub(crate) prepared: PreparedShareKeyDistribution,
    pub(crate) key_generation: u32,
}

pub(crate) async fn prepare_share_key_distribution(
    work: ShareKeyEffectWork,
) -> Result<PreparedShareKeyDistribution> {
    let owner_keys = crate::runtime::identity::device_keys::load_registered_device_keys_required(
        &work.services.auth,
        &work.services.backend,
        &work.services.device_key_store,
        &work.services.pin_store,
    )
    .await?;
    let sender_device_id = owner_keys.device_id.clone();
    let sender_signing_pkcs8 = Zeroizing::new(owner_keys.signing_pkcs8_bytes().to_vec());
    let authorized_recipients =
        authorized_recipients_for(&work.services, &work.audience, owner_keys).await?;
    let authorized_user_ids = authorized_recipients.keys().cloned().collect();
    let authorized_devices = crate::runtime::sharing::filter_authorized_devices(
        &work.backend_session_id,
        work.services
            .backend
            .fetch_authorized_devices(&work.backend_session_id)
            .await?,
        &authorized_user_ids,
    );
    if authorized_devices.is_empty() {
        return Err(AppError::Unsupported {
            reason: "share audience has no registered recipient devices".to_owned(),
        });
    }
    let mut verified_identities = HashMap::new();
    for authorized in &authorized_devices {
        let identity = verified_identity(
            &work.services,
            &authorized.user_id,
            &mut verified_identities,
        )
        .await?;
        if !identity.devices.contains_key(&authorized.device_id) {
            return Err(AppError::InvalidBackendData {
                field: "authorizedDevices.deviceId".to_owned(),
                reason: format!(
                    "authorized device {} is not in user {}'s signed device list",
                    authorized.device_id, authorized.user_id
                ),
            });
        }
    }
    Ok(PreparedShareKeyDistribution {
        backend_session_id: work.backend_session_id,
        backend_incarnation_id: work.backend_incarnation_id,
        previous_generation: work.previous_generation,
        session_key: crypto::generate_session_key()?,
        sender_device_id,
        owner_user_id: work.audience.owner_user_id,
        sender_signing_pkcs8,
        authorized_devices,
        authorized_recipients,
        verified_identities,
    })
}

pub(crate) async fn claim_share_key_generation(
    backend: &BackendHttpClient,
    prepared: PreparedShareKeyDistribution,
) -> Result<ClaimedShareKeyDistribution> {
    let key_generation = claim_or_reconcile_key_generation(
        backend,
        &prepared.backend_session_id,
        &prepared.backend_incarnation_id,
        prepared.previous_generation,
    )
    .await?;
    Ok(ClaimedShareKeyDistribution {
        prepared,
        key_generation,
    })
}

pub(crate) async fn publish_share_key_distribution(
    backend: &BackendHttpClient,
    claimed: ClaimedShareKeyDistribution,
) -> Result<AccessEffectCommit> {
    let signing_key = aws_lc_rs::signature::PqdsaKeyPair::from_pkcs8(
        &aws_lc_rs::signature::ML_DSA_65_SIGNING,
        &claimed.prepared.sender_signing_pkcs8,
    )
    .map_err(|_| AppError::Unsupported {
        reason: "failed to reconstruct ML-DSA-65 signing key".to_owned(),
    })?;
    let (blobs, control_keys) = build_distribution_batch_from_verified(
        &claimed.prepared.authorized_devices,
        &claimed.prepared.authorized_recipients,
        &claimed.prepared.verified_identities,
        DistributionContext {
            backend_session_id: &claimed.prepared.backend_session_id,
            backend_incarnation_id: &claimed.prepared.backend_incarnation_id,
            session_key: &claimed.prepared.session_key,
            key_generation: claimed.key_generation,
            sender_device_id: &claimed.prepared.sender_device_id,
            owner_user_id: &claimed.prepared.owner_user_id,
            signing_key: &signing_key,
            issued_at_ms: current_epoch_ms(),
        },
    )?;
    backend
        .store_session_key_blobs(
            &claimed.prepared.backend_session_id,
            &StoreSessionKeyBlobsRequest {
                incarnation_id: claimed.prepared.backend_incarnation_id,
                blobs,
            },
        )
        .await?;
    Ok(AccessEffectCommit::Distributed {
        session_key: claimed.prepared.session_key,
        key_generation: claimed.key_generation,
        control_keys,
        sender_device_id: claimed.prepared.sender_device_id,
        sender_signing_pkcs8: claimed.prepared.sender_signing_pkcs8,
        distributed: true,
    })
}

pub(crate) async fn execute(work: AccessEffectWork) -> AccessEffectCompletion {
    let outcome = execute_inner(&work).await;
    AccessEffectCompletion {
        prepared: work.prepared,
        account_epoch: work.account_epoch,
        source_key_generation: work.source_key_generation,
        mode: work.mode,
        access_snapshot: work.access_snapshot,
        outcome,
    }
}

pub(crate) async fn load_relay_signing_identity(
    services: &AccessEffectServices,
    expected_host_device_id: &str,
) -> Result<Zeroizing<Vec<u8>>> {
    let owner_keys = crate::runtime::identity::device_keys::load_registered_device_keys_required(
        &services.auth,
        &services.backend,
        &services.device_key_store,
        &services.pin_store,
    )
    .await?;
    if owner_keys.device_id != expected_host_device_id {
        return Err(AppError::Unsupported {
            reason: "host relay device identity changed during restart".to_owned(),
        });
    }
    Ok(Zeroizing::new(owner_keys.signing_pkcs8_bytes().to_vec()))
}

async fn execute_inner(work: &AccessEffectWork) -> Result<AccessEffectCommit> {
    let services = &work.services;
    if let AccessEffectKind::LoadRelayIdentity {
        expected_host_device_id,
    } = &work.kind
    {
        return load_relay_signing_identity(services, expected_host_device_id)
            .await
            .map(|sender_signing_pkcs8| AccessEffectCommit::RelayIdentity {
                sender_signing_pkcs8,
            });
    }
    execute_distribution(
        &work.services,
        &work.audience,
        &work.prepared.backend_session_id,
        &work.prepared.backend_incarnation_id,
        match &work.kind {
            AccessEffectKind::Redistribute {
                session_key,
                key_generation,
            } => AccessEffectKind::Redistribute {
                session_key: *session_key,
                key_generation: *key_generation,
            },
            AccessEffectKind::Rotate {
                previous_generation,
            } => AccessEffectKind::Rotate {
                previous_generation: *previous_generation,
            },
            AccessEffectKind::LoadRelayIdentity { .. } => unreachable!(),
        },
    )
    .await
}

async fn execute_distribution(
    services: &AccessEffectServices,
    audience: &AccessEffectAudience,
    backend_session_id: &str,
    backend_incarnation_id: &Uuid,
    kind: AccessEffectKind,
) -> Result<AccessEffectCommit> {
    let owner_keys = crate::runtime::identity::device_keys::load_registered_device_keys_required(
        &services.auth,
        &services.backend,
        &services.device_key_store,
        &services.pin_store,
    )
    .await?;
    let sender_device_id = owner_keys.device_id.clone();
    let sender_signing_pkcs8 = Zeroizing::new(owner_keys.signing_pkcs8_bytes().to_vec());
    let signing_key = owner_keys.signing_key()?;
    let (session_key, key_generation) = match kind {
        AccessEffectKind::Redistribute {
            session_key,
            key_generation,
        } => (session_key, key_generation),
        AccessEffectKind::Rotate {
            previous_generation,
        } => (
            crypto::generate_session_key()?,
            claim_or_reconcile_key_generation(
                &services.backend,
                backend_session_id,
                backend_incarnation_id,
                previous_generation,
            )
            .await?,
        ),
        AccessEffectKind::LoadRelayIdentity { .. } => unreachable!(),
    };

    let authorized_recipients = authorized_recipients_for(services, audience, owner_keys).await?;
    let authorized_user_ids = authorized_recipients.keys().cloned().collect();
    let authorized_devices = crate::runtime::sharing::filter_authorized_devices(
        backend_session_id,
        services
            .backend
            .fetch_authorized_devices(backend_session_id)
            .await?,
        &authorized_user_ids,
    );
    let (blobs, control_keys) = build_distribution_batch(
        services,
        &authorized_devices,
        &authorized_recipients,
        DistributionContext {
            backend_session_id,
            backend_incarnation_id,
            session_key: &session_key,
            key_generation,
            sender_device_id: &sender_device_id,
            owner_user_id: &audience.owner_user_id,
            signing_key: &signing_key,
            issued_at_ms: current_epoch_ms(),
        },
    )
    .await?;
    let distributed = !blobs.is_empty();
    if distributed {
        services
            .backend
            .store_session_key_blobs(
                backend_session_id,
                &StoreSessionKeyBlobsRequest {
                    incarnation_id: *backend_incarnation_id,
                    blobs,
                },
            )
            .await?;
    }

    Ok(AccessEffectCommit::Distributed {
        session_key,
        key_generation,
        control_keys,
        sender_device_id,
        sender_signing_pkcs8,
        distributed,
    })
}

async fn authorized_recipients_for(
    services: &AccessEffectServices,
    audience: &AccessEffectAudience,
    owner_keys: crate::identity_core::device_keys::DeviceKeys,
) -> Result<HashMap<String, AccessLevel>> {
    let mut allowed = HashMap::new();
    match audience.scope {
        ShareScope::JustMe | ShareScope::MyDevices => {}
        ShareScope::Room => {
            let room_id =
                audience
                    .room_id
                    .as_deref()
                    .ok_or_else(|| AppError::InvalidBackendData {
                        field: "session.roomId".to_owned(),
                        reason: "room-scoped session has no selected room".to_owned(),
                    })?;
            for member in crate::room_crypto::verified_room_members_with_services(
                services.backend.clone(),
                services.pin_store.clone(),
                services.roster_pins_path.clone(),
                audience.owner_user_id.clone(),
                owner_keys,
                room_id,
            )
            .await?
            {
                allowed.insert(member, AccessLevel::View);
            }
        }
        ShareScope::Friends => {
            return Err(AppError::Unsupported {
                reason:
                    "friends-scope encrypted sharing requires an owner-signed audience manifest"
                        .to_owned(),
            });
        }
    }
    let now = time::OffsetDateTime::now_utc();
    allowed.extend(
        audience
            .explicit_grants
            .iter()
            .filter(|(_, grant)| grant.expires_at > now)
            .map(|(user_id, grant)| (user_id.clone(), grant.access)),
    );
    allowed.insert(audience.owner_user_id.clone(), AccessLevel::Approve);
    Ok(allowed)
}

async fn build_distribution_batch(
    services: &AccessEffectServices,
    authorized_devices: &[kodosi_backend_client::api::AuthorizedDeviceDto],
    authorized_recipients: &HashMap<String, AccessLevel>,
    context: DistributionContext<'_>,
) -> Result<(
    Vec<SessionKeyBlobEntry>,
    HashMap<(String, String), ControlTrustEntry>,
)> {
    let mut verified_identities = HashMap::new();
    for authorized in authorized_devices {
        verified_identity(services, &authorized.user_id, &mut verified_identities).await?;
    }
    build_distribution_batch_from_verified(
        authorized_devices,
        authorized_recipients,
        &verified_identities,
        context,
    )
}

fn build_distribution_batch_from_verified(
    authorized_devices: &[kodosi_backend_client::api::AuthorizedDeviceDto],
    authorized_recipients: &HashMap<String, AccessLevel>,
    verified_identities: &HashMap<
        String,
        crate::identity_core::device_list_pin_store::IdentityBundleView,
    >,
    context: DistributionContext<'_>,
) -> Result<(
    Vec<SessionKeyBlobEntry>,
    HashMap<(String, String), ControlTrustEntry>,
)> {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

    let mut blobs = Vec::with_capacity(authorized_devices.len());
    let mut control_keys = HashMap::with_capacity(authorized_devices.len());
    for authorized in authorized_devices {
        let identity = verified_identities
            .get(&authorized.user_id)
            .ok_or_else(|| AppError::InvalidBackendData {
                field: "authorizedDevices.userId".to_owned(),
                reason: format!(
                    "verified identity for authorized user {} is unavailable",
                    authorized.user_id
                ),
            })?;
        let device = identity.devices.get(&authorized.device_id).ok_or_else(|| {
            AppError::InvalidBackendData {
                field: "authorizedDevices.deviceId".to_owned(),
                reason: format!(
                    "authorized device {} is not in user {}'s signed device list",
                    authorized.device_id, authorized.user_id
                ),
            }
        })?;
        let capabilities = SessionCapabilities::from_access(
            authorized_recipients
                .get(&authorized.user_id)
                .copied()
                .unwrap_or(AccessLevel::View),
            authorized.user_id == context.owner_user_id,
        );
        let encrypted = crypto::wrap_session_key(
            &device.certificate.kem_public_key,
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
        control_keys.insert(
            (authorized.user_id.clone(), authorized.device_id.clone()),
            ControlTrustEntry::new(device.sig_public_key.clone(), capabilities),
        );
        blobs.push(SessionKeyBlobEntry {
            recipient_device_id: authorized.device_id.clone(),
            encrypted_session_key: BASE64.encode(encrypted),
            sender_device_id: context.sender_device_id.to_owned(),
            key_generation: context.key_generation,
            issued_at_ms: context.issued_at_ms,
            signature: BASE64.encode(signature),
            signature_version: crypto::CURRENT_KEY_BLOB_SIGNATURE_VERSION,
        });
    }
    Ok((blobs, control_keys))
}

async fn verified_identity<'a>(
    services: &AccessEffectServices,
    user_id: &str,
    identities: &'a mut HashMap<
        String,
        crate::identity_core::device_list_pin_store::IdentityBundleView,
    >,
) -> Result<&'a crate::identity_core::device_list_pin_store::IdentityBundleView> {
    if !identities.contains_key(user_id) {
        let bundle = services.backend.fetch_user_identity(user_id).await?;
        let view = backend_adapters::identity_bundle_view(&bundle)?;
        match services
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

async fn claim_or_reconcile_key_generation(
    backend: &BackendHttpClient,
    backend_session_id: &str,
    backend_incarnation_id: &Uuid,
    expected_current_generation: u32,
) -> Result<u32> {
    const MAX_RECONCILIATION_ATTEMPTS: usize = 4;
    let mut expected_generation = expected_current_generation;
    let mut last_failure = None;
    for _ in 0..MAX_RECONCILIATION_ATTEMPTS {
        match backend
            .claim_next_key_generation(
                backend_session_id,
                backend_incarnation_id,
                expected_generation,
            )
            .await
        {
            Ok(KeyGenerationClaimOutcome::Claimed(generation)) => return Ok(generation),
            Ok(KeyGenerationClaimOutcome::GenerationChanged(current)) => {
                expected_generation = current;
            }
            Err(error) => {
                let error: AppError = error.into();
                expected_generation = backend
                    .fetch_current_key_generation(backend_session_id, backend_incarnation_id)
                    .await
                    .map_err(|query_error| AppError::Unsupported {
                        reason: format!(
                            "session key generation claim was indeterminate ({error}); current generation reconciliation failed: {query_error}"
                        ),
                    })?;
                last_failure = Some(error);
            }
        }
    }
    Err(AppError::Unsupported {
        reason: format!(
            "session key generation kept changing from {expected_current_generation}; last failure: {}",
            last_failure.map_or_else(|| "none".to_owned(), |error| error.to_string())
        ),
    })
}

fn current_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}
