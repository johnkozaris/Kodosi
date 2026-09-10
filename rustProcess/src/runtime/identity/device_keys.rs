use std::collections::VecDeque;

use time::OffsetDateTime;

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};

use crate::{
    AppError, Result,
    host_protocol::AuthEvent,
    identity_core::{
        device_cert,
        device_keys::DeviceKeys,
        device_list_pin_store::{
            DeviceListPinStoreHandle, IdentityBundleView, PinContext, PinVerdict,
        },
        signed_device_list,
    },
    runtime::{runtime_event_outbox::RuntimeEventOutbox, state::push_log},
};
use kodosi_backend_client::{
    BackendClientError, api::DeviceRegistrationRequest, crypto::sign_pop_challenge,
    http_client::BackendHttpClient,
};
use kodosi_domain::auth::AuthState;

pub(crate) struct DeviceKeysCtx<'a> {
    pub(crate) auth: &'a AuthState,
    pub(crate) backend: &'a BackendHttpClient,
    pub(crate) pin_store: &'a DeviceListPinStoreHandle,
    pub(crate) device_key_store: &'a crate::identity_core::device_keys::DeviceKeyStore,
    pub(crate) outbox: &'a mut RuntimeEventOutbox,
    pub(crate) logs: &'a mut VecDeque<String>,
}

impl DeviceKeysCtx<'_> {
    pub(crate) async fn ensure_registered(&mut self) -> Result<()> {
        if !matches!(
            self.auth,
            AuthState::Authenticated {
                subject: Some(_),
                ..
            }
        ) {
            return Ok(());
        }

        match register_with_backend(
            self.auth,
            self.backend,
            self.device_key_store,
            self.pin_store,
        )
        .await
        {
            Ok(DeviceKeyRegistrationState::Registered) => {
                push_log(self.logs, "device encryption keys registered".to_owned());
                self.outbox.queue_auth(AuthEvent::IdentityHealth {
                    state: crate::host_protocol::IdentityHealthState::Healthy,
                    message: None,
                });
                Ok(())
            }
            Ok(DeviceKeyRegistrationState::AlreadyRegistered) => {
                self.outbox.queue_auth(AuthEvent::IdentityHealth {
                    state: crate::host_protocol::IdentityHealthState::Healthy,
                    message: None,
                });
                Ok(())
            }
            Err(error) => {
                let message = registration_failure_message(&error);
                push_log(self.logs, message.clone());
                if matches!(error, AppError::IdentityRecoveryRequired { .. }) {
                    self.outbox.queue_auth(AuthEvent::IdentityHealth {
                        state: crate::host_protocol::IdentityHealthState::RecoveryRequired,
                        message: Some(message),
                    });
                } else {
                    self.outbox.queue_auth(AuthEvent::Notice {
                        message: Some(message),
                    });
                }
                Err(error)
            }
        }
    }
}

pub(crate) async fn ensure_device_keys_registered_required(
    auth: &AuthState,
    backend: &BackendHttpClient,
    device_key_store: &crate::identity_core::device_keys::DeviceKeyStore,
    pin_store: &DeviceListPinStoreHandle,
) -> Result<()> {
    match register_with_backend(auth, backend, device_key_store, pin_store).await? {
        DeviceKeyRegistrationState::Registered | DeviceKeyRegistrationState::AlreadyRegistered => {
            Ok(())
        }
    }
}

pub(crate) async fn load_registered_device_keys_required(
    auth: &AuthState,
    backend: &BackendHttpClient,
    device_key_store: &crate::identity_core::device_keys::DeviceKeyStore,
    pin_store: &DeviceListPinStoreHandle,
) -> Result<DeviceKeys> {
    ensure_device_keys_registered_required(auth, backend, device_key_store, pin_store).await?;
    DeviceKeyAccess::new(auth, device_key_store).load_authenticated_device_keys()
}

pub(crate) async fn verify_own_identity_bundle(
    auth: &AuthState,
    local_keys: &DeviceKeys,
    pin_store: &DeviceListPinStoreHandle,
    bundle: &kodosi_backend_client::api::UserIdentityBundleDto,
) -> Result<IdentityBundleView> {
    let user_id = auth.subject_string().ok_or(AppError::Unauthorized)?;
    verify_own_identity_bundle_for_user(&user_id, local_keys, pin_store, bundle).await
}

async fn verify_own_identity_bundle_for_user(
    user_id: &str,
    local_keys: &DeviceKeys,
    pin_store: &DeviceListPinStoreHandle,
    bundle: &kodosi_backend_client::api::UserIdentityBundleDto,
) -> Result<IdentityBundleView> {
    let view = super::backend_adapters::identity_bundle_view(bundle)?;
    if view.user_id != user_id {
        return Err(AppError::InvalidBackendData {
            field: "identity.userId".to_owned(),
            reason: "authenticated user's identity bundle names a different user".to_owned(),
        });
    }
    let own_device =
        view.devices
            .get(&local_keys.device_id)
            .ok_or_else(|| AppError::InvalidBackendData {
                field: "identity.devices".to_owned(),
                reason: "authenticated device is absent from its signed device list".to_owned(),
            })?;
    if own_device.sig_public_key != local_keys.signing_public_bytes()
        || own_device.certificate.kem_public_key != local_keys.kem_public_bytes()
    {
        return Err(AppError::InvalidBackendData {
            field: "identity.devices".to_owned(),
            reason: "authenticated device keys do not match the signed identity bundle".to_owned(),
        });
    }

    let has_pin = pin_store.identity_bundle(user_id).await?.is_some();
    if !has_pin && view.signed_list.signer_device_id != local_keys.device_id {
        return Err(AppError::InvalidBackendData {
            field: "identity.deviceList.signerDeviceId".to_owned(),
            reason: "cannot bootstrap self trust from a list signed by an unpinned device"
                .to_owned(),
        });
    }
    match pin_store
        .verify_or_pin_for_owner(user_id, view.clone(), PinContext::ExplicitShare)
        .await?
    {
        PinVerdict::FirstShare | PinVerdict::AcceptUpdate | PinVerdict::AlreadyPinned => Ok(view),
        PinVerdict::Reject { reason } => Err(AppError::InvalidBackendData {
            field: "identity.deviceList".to_owned(),
            reason: format!("signed device list failed self-pin verification: {reason:?}"),
        }),
    }
}

fn registration_failure_message(error: &AppError) -> String {
    match error {
        AppError::Io(source) => {
            format!("Signed in, but this desktop could not persist its encryption keys: {source}")
        }
        AppError::Keychain { reason } => format!(
            "Signed in, but this desktop could not finish encrypted sharing setup: {reason}"
        ),
        _ => {
            format!("Signed in, but this desktop could not finish encrypted sharing setup: {error}")
        }
    }
}

async fn register_with_backend(
    auth: &AuthState,
    backend: &BackendHttpClient,
    device_key_store: &crate::identity_core::device_keys::DeviceKeyStore,
    pin_store: &DeviceListPinStoreHandle,
) -> Result<DeviceKeyRegistrationState> {
    let keys = DeviceKeyAccess::new(auth, device_key_store).load_authenticated_device_keys()?;
    let user_id = auth.subject().ok_or(AppError::Unauthorized)?;

    if let Some(state) =
        preflight_existing_enrollment(auth, backend, pin_store, &keys, &user_id.to_string()).await?
    {
        return state;
    }

    let challenge = backend.fetch_device_registration_challenge().await?;
    let challenge_bytes =
        BASE64
            .decode(&challenge.challenge_bytes)
            .map_err(|_| AppError::Unsupported {
                reason: "backend returned malformed PoP challenge".to_owned(),
            })?;
    let signing_key = keys.signing_key()?;
    let pop_signature = sign_pop_challenge(&signing_key, &challenge_bytes)?;

    #[expect(
        clippy::cast_sign_loss,
        reason = "now_ms is non-negative within Unix epoch range"
    )]
    let now_ms = OffsetDateTime::now_utc().unix_timestamp() as u64 * 1000;
    let device_label = std::env::var("HOSTNAME")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Unknown device".to_owned());

    let signed_cert = device_cert::build_self_cert(
        &user_id.to_string(),
        &keys.device_id,
        device_label.as_str(),
        keys.kem_public_bytes(),
        &signing_key,
        now_ms,
        None,
    )?;
    let bootstrap_list = signed_device_list::build_bootstrap_list(
        &user_id.to_string(),
        &keys.device_id,
        &signing_key,
        now_ms,
        None,
    )?;

    let request = DeviceRegistrationRequest {
        device_id: keys.device_id.clone(),
        kem_public_key: BASE64.encode(keys.kem_public_bytes()),
        signing_public_key: BASE64.encode(keys.signing_public_bytes()),
        challenge_id: challenge.challenge_id,
        pop_signature: BASE64.encode(&pop_signature),
        device_certificate: BASE64.encode(&signed_cert.body_bytes),
        device_certificate_signature: BASE64.encode(&signed_cert.signature),
        signed_device_list: BASE64.encode(&bootstrap_list.body_bytes),
        signed_device_list_signature: BASE64.encode(&bootstrap_list.signature),
    };

    let outcome = match backend.register_device_keys(&request).await {
        Ok(()) => DeviceKeyRegistrationState::Registered,
        Err(BackendClientError::HttpProblem {
            status: 409, code, ..
        }) if matches!(
            code.as_deref(),
            Some("DEVICE_ALREADY_ENROLLED" | "DEVICE_LIST_GENERATION_COLLISION")
        ) =>
        {
            return Err(AppError::Unsupported {
                reason: "device enrollment race: refresh identity state and retry".to_owned(),
            });
        }
        Err(error) => return Err(error.into()),
    };

    if let Err(pin_err) = install_self_pin(backend, pin_store, &user_id.to_string(), &keys).await {
        tracing::warn!(
            %pin_err,
            "self-pin after device registration failed; subsequent OWNED attaches will TOFU"
        );
    }
    Ok(outcome)
}

async fn preflight_existing_enrollment(
    auth: &AuthState,
    backend: &BackendHttpClient,
    pin_store: &DeviceListPinStoreHandle,
    local_keys: &DeviceKeys,
    user_id: &str,
) -> Result<Option<Result<DeviceKeyRegistrationState>>> {
    let bundle = match backend.fetch_user_identity(user_id).await {
        Ok(bundle) => bundle,
        Err(BackendClientError::NotFound) => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    let view = super::backend_adapters::identity_bundle_view(&bundle)?;
    if view.devices.contains_key(&local_keys.device_id) {
        verify_own_identity_bundle(auth, local_keys, pin_store, &bundle).await?;
        return Ok(Some(Ok(DeviceKeyRegistrationState::AlreadyRegistered)));
    }
    let device_count = view.signed_list.entries.len();
    let device_word = if device_count == 1 {
        "device"
    } else {
        "devices"
    };
    Ok(Some(Err(AppError::Unsupported {
        reason: format!(
            "This machine isn't in your device list yet (you already have {device_count} enrolled {device_word}). \
            On a device you've already used, open Settings → Devices → Add device and enter the code this machine shows. \
            From a terminal: run `kodosi device link` here, then approve from another device with `kodosi device approve-link CODE`."
        ),
    })))
}

pub(crate) async fn install_self_pin(
    backend: &BackendHttpClient,
    pin_store: &DeviceListPinStoreHandle,
    user_id: &str,
    local_keys: &DeviceKeys,
) -> Result<()> {
    let dto = backend.fetch_user_identity(user_id).await?;
    verify_own_identity_bundle_for_user(user_id, local_keys, pin_store, &dto)
        .await
        .map(drop)
}

enum DeviceKeyRegistrationState {
    Registered,
    AlreadyRegistered,
}

pub(crate) struct DeviceKeyAccess<'a> {
    auth: &'a AuthState,
    store: &'a crate::identity_core::device_keys::DeviceKeyStore,
}

impl<'a> DeviceKeyAccess<'a> {
    pub(crate) const fn new(
        auth: &'a AuthState,
        store: &'a crate::identity_core::device_keys::DeviceKeyStore,
    ) -> Self {
        Self { auth, store }
    }

    pub(crate) fn load_authenticated_device_keys(&self) -> Result<DeviceKeys> {
        let user_id = self.auth.subject_string().ok_or(AppError::Unauthorized)?;
        self.store.load_or_generate(&user_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity_core::{
        device_cert::build_self_cert,
        device_keys::generate_device_keys_for_test,
        device_list_pin_store::DeviceListPinStore,
        signed_device_list::{DeviceListEntry, build_replacement_list},
    };
    use kodosi_domain::ids::UserId;
    use serde_json::json;

    #[tokio::test]
    async fn rejects_unpinned_backend_list_signed_by_another_device() {
        let user_id = "11111111-1111-1111-1111-111111111111";
        let local = generate_device_keys_for_test(user_id).expect("local keys");
        let attacker = generate_device_keys_for_test(user_id).expect("attacker keys");
        let local_signer = local.signing_key().expect("local signer");
        let attacker_signer = attacker.signing_key().expect("attacker signer");
        let now_ms = u64::try_from(OffsetDateTime::now_utc().unix_timestamp())
            .expect("positive timestamp")
            * 1_000;

        let local_cert = build_self_cert(
            user_id,
            &local.device_id,
            "local",
            local.kem_public_bytes(),
            &local_signer,
            now_ms,
            None,
        )
        .expect("local certificate");
        let attacker_cert = build_self_cert(
            user_id,
            &attacker.device_id,
            "attacker",
            attacker.kem_public_bytes(),
            &attacker_signer,
            now_ms,
            None,
        )
        .expect("attacker certificate");
        let local_certificate = device_cert::DeviceCertificate::parse_body(&local_cert.body_bytes)
            .expect("local certificate body");
        let attacker_certificate =
            device_cert::DeviceCertificate::parse_body(&attacker_cert.body_bytes)
                .expect("attacker certificate body");
        let attacker_entry = DeviceListEntry {
            device_id: attacker.device_id.clone(),
            signer_device_id: attacker.device_id.clone(),
        };
        let list = build_replacement_list(
            user_id,
            1,
            std::slice::from_ref(&attacker_entry),
            vec![
                attacker_entry.clone(),
                DeviceListEntry {
                    device_id: local.device_id.clone(),
                    signer_device_id: local.device_id.clone(),
                },
            ],
            &attacker.device_id,
            &attacker_signer,
            now_ms,
            None,
        )
        .expect("attacker-signed list");

        let bundle = serde_json::from_value(json!({
            "userId": user_id,
            "identityRevision": 1,
            "identityIncarnationId": "01900000-0000-7000-8000-000000000001",
            "deviceList": {
                "generation": list.list.generation,
                "signerDeviceId": attacker.device_id.clone(),
                "issuedAtMs": now_ms,
                "expiresAtMs": null,
                "body": BASE64.encode(&list.body_bytes),
                "signature": BASE64.encode(&list.signature),
            },
            "devices": [
                {
                    "deviceId": local.device_id.clone(),
                    "deviceLabel": "local",
                    "kemPublicKey": BASE64.encode(local.kem_public_bytes()),
                    "signingPublicKey": BASE64.encode(local.signing_public_bytes()),
                    "certificate": BASE64.encode(&local_cert.body_bytes),
                    "certificateSignature": BASE64.encode(&local_cert.signature),
                    "certSignerDeviceId": local_certificate.signer_device_id.clone(),
                    "certIssuedAtMs": now_ms,
                    "certExpiresAtMs": null,
                },
                {
                    "deviceId": attacker.device_id.clone(),
                    "deviceLabel": "attacker",
                    "kemPublicKey": BASE64.encode(attacker.kem_public_bytes()),
                    "signingPublicKey": BASE64.encode(attacker.signing_public_bytes()),
                    "certificate": BASE64.encode(&attacker_cert.body_bytes),
                    "certificateSignature": BASE64.encode(&attacker_cert.signature),
                    "certSignerDeviceId": attacker_certificate.signer_device_id.clone(),
                    "certIssuedAtMs": now_ms,
                    "certExpiresAtMs": null,
                }
            ],
        }))
        .expect("identity bundle DTO");

        let dir = tempfile::tempdir().expect("tempdir");
        let pin_store = DeviceListPinStoreHandle::from_store(
            DeviceListPinStore::load_from(dir.path().join("pins.json")).expect("pin store"),
        )
        .expect("pin-store actor");
        let auth = AuthState::Authenticated {
            subject: Some(UserId::try_from(user_id).expect("user id")),
            expires_at: OffsetDateTime::now_utc(),
        };

        let error = verify_own_identity_bundle(&auth, &local, &pin_store, &bundle)
            .await
            .expect_err("an unpinned backend device must not establish self trust");
        std::assert_matches!(
            error,
            AppError::InvalidBackendData { ref field, .. }
                if field == "identity.deviceList.signerDeviceId"
        );
        assert!(
            pin_store
                .identity_bundle(user_id)
                .await
                .expect("pin lookup")
                .is_none()
        );
        pin_store.shutdown_blocking();
    }
}
