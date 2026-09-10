use time::OffsetDateTime;

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};

use crate::{
    AppError, Result,
    host_protocol::{DeviceEvent, MyDeviceEntry},
    identity_core::{
        device_keys::DeviceKeyStore,
        device_list_pin_store::{BreakReason, DeviceListPinStoreHandle, PinContext, PinVerdict},
        signed_device_list,
    },
    runtime::runtime_event_outbox::RuntimeEventOutbox,
};
use kodosi_backend_client::{api::SignedDeviceListSubmission, http_client::BackendHttpClient};
use kodosi_domain::auth::AuthState;

use super::{DeviceKeyAccess, backend_adapters, device_keys::verify_own_identity_bundle};

#[derive(Debug, Clone)]
pub(crate) struct DeviceRevocationOutcome {
    pub(crate) history_warning: Option<String>,
    #[cfg(feature = "cli")]
    pub(crate) revoked_device_id: String,
    #[cfg(feature = "cli")]
    pub(crate) new_generation: u64,
}

pub(crate) struct DeviceListCtx<'a> {
    pub(crate) auth: &'a AuthState,
    pub(crate) backend: &'a BackendHttpClient,
    pub(crate) device_key_store: &'a DeviceKeyStore,
    pub(crate) pin_store: &'a DeviceListPinStoreHandle,
    pub(crate) room_roster_pins_path: &'a std::path::Path,
    pub(crate) outbox: &'a mut RuntimeEventOutbox,
}

impl DeviceListCtx<'_> {
    #[tracing::instrument(skip_all, err)]
    pub(crate) async fn refresh(&mut self) -> Result<()> {
        let user_id = self.auth.subject_string().ok_or(AppError::Unauthorized)?;
        let local_keys = DeviceKeyAccess::new(self.auth, self.device_key_store)
            .load_authenticated_device_keys()?;

        let bundle = self.backend.fetch_user_identity(&user_id).await?;
        let view = backend_adapters::identity_bundle_view(&bundle)?;
        if view.user_id != user_id {
            return Err(AppError::InvalidBackendData {
                field: "identity.userId".to_owned(),
                reason: "authenticated user's identity bundle names a different user".to_owned(),
            });
        }

        let local_device_enrolled = view
            .devices
            .get(&local_keys.device_id)
            .is_some_and(|device| {
                device.sig_public_key == local_keys.signing_public_bytes()
                    && device.certificate.kem_public_key == local_keys.kem_public_bytes()
            });
        let pin_context = if local_device_enrolled {
            PinContext::ExplicitShare
        } else {
            PinContext::BackgroundFetch
        };
        let verdict = self
            .pin_store
            .verify_or_pin_for_owner(&user_id, view.clone(), pin_context)
            .await?;
        match verdict {
            PinVerdict::FirstShare | PinVerdict::AcceptUpdate | PinVerdict::AlreadyPinned => {}
            PinVerdict::Reject {
                reason: BreakReason::NoExplicitShareContext,
            } if !local_device_enrolled => {}
            PinVerdict::Reject { reason } => {
                return Err(AppError::InvalidBackendData {
                    field: "identity.deviceList".to_owned(),
                    reason: format!("signed device list failed self-pin verification: {reason:?}"),
                });
            }
        }

        let devices = view
            .devices
            .values()
            .map(|device| MyDeviceEntry {
                device_id: device.certificate.device_id.clone(),
                label: device.certificate.device_label.clone(),
                cert_signer_device_id: device.certificate.signer_device_id.clone(),
                cert_issued_at_ms: device.certificate.issued_at_ms,
            })
            .collect();

        self.outbox.queue_devices(DeviceEvent::List {
            self_device_id: local_keys.device_id.clone(),
            local_device_enrolled,
            devices,
        });
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(target_device_id = %target_device_id), err)]
    pub(crate) async fn revoke(
        &mut self,
        target_device_id: &str,
    ) -> Result<DeviceRevocationOutcome> {
        let user_id = self.auth.subject_string().ok_or(AppError::Unauthorized)?;
        let local_keys = DeviceKeyAccess::new(self.auth, self.device_key_store)
            .load_authenticated_device_keys()?;
        if target_device_id == local_keys.device_id {
            return Err(AppError::Unsupported {
                reason: "Cannot revoke the device you're currently using. Sign in on another enrolled device and revoke this one from there, or use Reset identity to wipe everything.".to_owned(),
            });
        }

        let bundle = self.backend.fetch_user_identity(&user_id).await?;
        let view =
            verify_own_identity_bundle(self.auth, &local_keys, self.pin_store, &bundle).await?;
        let previous_entries = view.signed_list.entries.clone();

        if !previous_entries
            .iter()
            .any(|e| e.device_id == target_device_id)
        {
            return Err(AppError::Unsupported {
                reason: "Target device is not in the current signed list.".to_owned(),
            });
        }

        let next_entries: Vec<_> = previous_entries
            .iter()
            .filter(|e| e.device_id != target_device_id)
            .cloned()
            .collect();

        let mut history = crate::room_crypto::RoomCryptoContext::from_parts(
            self.backend.clone(),
            self.pin_store.clone(),
            self.room_roster_pins_path.to_owned(),
            user_id.clone(),
            self.device_key_store
                .load_if_present(&user_id)?
                .ok_or(AppError::Unauthorized)?,
        );
        let preservation = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            Box::pin(history.preserve_history_before_revocation(target_device_id)),
        )
        .await;
        let history_warning = match preservation {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(format!("Device revoked. Some Mission history could not be preserved: {error}")),
            Err(_) => Some("Device revoked. Preserving Mission history timed out; some older content may be unavailable.".to_owned()),
        };

        let signing_key = local_keys.signing_key()?;
        #[expect(
            clippy::cast_sign_loss,
            reason = "now_ms is non-negative within Unix epoch range"
        )]
        let now_ms = OffsetDateTime::now_utc().unix_timestamp() as u64 * 1000;
        let envelope = signed_device_list::build_replacement_list(
            &user_id,
            view.signed_list.generation,
            &previous_entries,
            next_entries,
            &local_keys.device_id,
            &signing_key,
            now_ms,
            None,
        )?;

        self.backend
            .submit_device_list(&SignedDeviceListSubmission {
                signed_device_list: BASE64.encode(&envelope.body_bytes),
                signed_device_list_signature: BASE64.encode(&envelope.signature),
            })
            .await?;

        self.outbox.queue_devices(DeviceEvent::List {
            self_device_id: local_keys.device_id.clone(),
            local_device_enrolled: true,
            devices: view
                .devices
                .values()
                .filter(|device| device.certificate.device_id != target_device_id)
                .map(|device| MyDeviceEntry {
                    device_id: device.certificate.device_id.clone(),
                    label: device.certificate.device_label.clone(),
                    cert_signer_device_id: device.certificate.signer_device_id.clone(),
                    cert_issued_at_ms: device.certificate.issued_at_ms,
                })
                .collect(),
        });
        Ok(DeviceRevocationOutcome {
            history_warning,
            #[cfg(feature = "cli")]
            revoked_device_id: target_device_id.to_owned(),
            #[cfg(feature = "cli")]
            new_generation: view.signed_list.generation.saturating_add(1),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity_core::{
        device_cert::build_self_cert,
        device_keys::{DeviceKeys, generate_device_keys_for_test},
        device_list_pin_store::DeviceListPinStore,
        signed_device_list::build_bootstrap_list,
    };
    use kodosi_backend_client::{
        api::{UserDeviceCertificateDto, UserDeviceListDto, UserIdentityBundleDto},
        config::BackendClientConfig,
        http_client::BackendHttpClient,
    };

    #[tokio::test]
    async fn first_run_existing_account_inventory_is_visible_without_establishing_tofu() {
        let user_id = "11111111-1111-1111-1111-111111111111";
        let existing = generate_device_keys_for_test(user_id).expect("existing keys");
        let local = generate_device_keys_for_test(user_id).expect("new-device keys");
        let server = IdentityServer::spawn(existing_account_bundle(user_id, &existing)).await;
        let pin_dir = tempfile::tempdir().expect("pin directory");
        let pin_store = DeviceListPinStoreHandle::from_store(
            DeviceListPinStore::load_from(pin_dir.path().join("pins.json")).expect("empty pins"),
        )
        .expect("pin actor");
        pin_store
            .bind_to_user(user_id)
            .await
            .expect("pin store should bind to the authenticated account");
        let (auth, device_key_store) = authenticated_device(user_id, &local);
        let mut outbox = RuntimeEventOutbox::default();
        let mut context = DeviceListCtx {
            auth: &auth,
            backend: &server.client(),
            device_key_store: &device_key_store,
            pin_store: &pin_store,
            room_roster_pins_path: &pin_dir.path().join("rosters.json"),
            outbox: &mut outbox,
        };

        context
            .refresh()
            .await
            .expect("existing-account inventory should remain reachable");

        let event = outbox
            .drain_devices()
            .into_iter()
            .next()
            .expect("devices.list");
        std::assert_matches!(
            event,
            DeviceEvent::List {
                ref self_device_id,
                local_device_enrolled: false,
                ref devices,
            } if self_device_id == &local.device_id
                && devices.len() == 1
                && devices[0].device_id == existing.device_id
        );
        assert!(
            pin_store
                .identity_bundle(user_id)
                .await
                .expect("pin lookup")
                .is_none(),
            "background inventory refresh must not establish TOFU"
        );
        device_key_store
            .clear(user_id)
            .expect("remove test device keys");
    }

    #[tokio::test]
    async fn enrolled_device_refresh_remains_actionable_and_pins_normally() {
        let user_id = "22222222-2222-2222-2222-222222222222";
        let local = generate_device_keys_for_test(user_id).expect("local keys");
        let server = IdentityServer::spawn(existing_account_bundle(user_id, &local)).await;
        let pin_dir = tempfile::tempdir().expect("pin directory");
        let pin_store = DeviceListPinStoreHandle::from_store(
            DeviceListPinStore::load_from(pin_dir.path().join("pins.json")).expect("empty pins"),
        )
        .expect("pin actor");
        pin_store
            .bind_to_user(user_id)
            .await
            .expect("pin store should bind to the authenticated account");
        let (auth, device_key_store) = authenticated_device(user_id, &local);
        let mut outbox = RuntimeEventOutbox::default();
        let backend = server.client();
        let mut context = DeviceListCtx {
            auth: &auth,
            backend: &backend,
            device_key_store: &device_key_store,
            pin_store: &pin_store,
            room_roster_pins_path: &pin_dir.path().join("rosters.json"),
            outbox: &mut outbox,
        };

        context.refresh().await.expect("enrolled inventory refresh");

        let event = outbox
            .drain_devices()
            .into_iter()
            .next()
            .expect("devices.list");
        std::assert_matches!(
            event,
            DeviceEvent::List {
                local_device_enrolled: true,
                ..
            }
        );
        assert!(
            pin_store
                .identity_bundle(user_id)
                .await
                .expect("pin lookup")
                .is_some(),
            "an enrolled device keeps the verified self-pin path"
        );
        device_key_store
            .clear(user_id)
            .expect("remove test device keys");
    }

    fn authenticated_device(user_id: &str, keys: &DeviceKeys) -> (AuthState, DeviceKeyStore) {
        let auth = AuthState::Authenticated {
            subject: Some(user_id.try_into().expect("user id")),
            expires_at: OffsetDateTime::now_utc(),
        };
        let device_key_store =
            DeviceKeyStore::new(&format!("kodosi.device-list-test.{}", uuid::Uuid::now_v7()))
                .expect("device key store");
        device_key_store
            .save_for_test(user_id, keys)
            .expect("store device keys");
        (auth, device_key_store)
    }

    fn existing_account_bundle(user_id: &str, keys: &DeviceKeys) -> UserIdentityBundleDto {
        let signer = keys.signing_key().expect("signer");
        let now_ms =
            u64::try_from(OffsetDateTime::now_utc().unix_timestamp()).expect("timestamp") * 1000;
        let cert = build_self_cert(
            user_id,
            &keys.device_id,
            "Existing Mac",
            keys.kem_public_bytes(),
            &signer,
            now_ms,
            None,
        )
        .expect("certificate");
        let list = build_bootstrap_list(user_id, &keys.device_id, &signer, now_ms, None)
            .expect("device list");
        UserIdentityBundleDto {
            user_id: user_id.to_owned(),
            identity_revision: 1,
            identity_incarnation_id: uuid::Uuid::from_u128(
                0x0190_0000_0000_7000_8000_0000_0000_0001,
            ),
            device_list: UserDeviceListDto {
                body: BASE64.encode(list.body_bytes),
                signature: BASE64.encode(list.signature),
            },
            devices: vec![UserDeviceCertificateDto {
                certificate: BASE64.encode(cert.body_bytes),
                certificate_signature: BASE64.encode(cert.signature),
            }],
            historical_devices: vec![],
        }
    }

    struct IdentityServer {
        base_url: String,
        task: tokio::task::AbortHandle,
    }

    impl IdentityServer {
        async fn spawn(bundle: UserIdentityBundleDto) -> Self {
            use axum::{Json, Router, routing::get};
            use tokio::net::TcpListener;

            let router = Router::new().route(
                "/api/users/{user_id}/identity",
                get(move || {
                    let bundle = bundle.clone();
                    async move { Json(bundle) }
                }),
            );
            let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
            let address = listener.local_addr().expect("address");
            let task = tokio::spawn(async move {
                axum::serve(listener, router)
                    .await
                    .expect("identity server");
            });
            Self {
                base_url: format!("http://{address}"),
                task: task.abort_handle(),
            }
        }

        fn client(&self) -> BackendHttpClient {
            BackendHttpClient::new(&BackendClientConfig::new(
                Some(self.base_url.clone()),
                None,
                None,
                None,
            ))
            .expect("backend client")
        }
    }

    impl Drop for IdentityServer {
        fn drop(&mut self) {
            self.task.abort();
        }
    }
}
