use serde::Deserialize;

use super::*;
use crate::identity::{
    keys::RoomKeyPair,
    pins::decode,
    recovery::{self, RecoveryKey},
};

const DAY_MS: u64 = 24 * 60 * 60_000;
const SAVED_ROOM_KEY_BYTES: usize = 4096;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedRoomKey {
    device_id: String,
    room_key_box: String,
}

impl BackendClient {
    pub(super) async fn create_recovery_key(&self) -> Result<Vec<Value>> {
        let credentials = self.credentials()?;
        if !credentials.enrolled {
            return Err(Error::EnrollmentRequired);
        }
        let mut verified = self.fetch_identity(&credentials.user_id).await?;
        let earlier = verified
            .devices
            .keys()
            .find(|device| recovery::is_recovery_device(device))
            .cloned();
        if let Some(earlier) = earlier {
            self.revoke_device(&earlier).await?;
            verified = self.fetch_identity(&credentials.user_id).await?;
        }
        let text = recovery::new_text()?;
        let key = RecoveryKey::derive(&text, &credentials.user_id, &verified.root)?;
        let signer = credentials.keys.signing_key()?;
        let issued = verified.list.successor_issued_at(identity::now_ms())?;
        let cert = build_cert_for(
            &credentials.user_id,
            &key.device_id,
            recovery::LABEL,
            &key.signing_public(),
            &credentials.keys.device_id,
            &signer,
            issued,
            None,
        )?;
        let mut entries = verified.list.entries.clone();
        entries.push(DeviceListEntry {
            device_id: key.device_id.clone(),
            signer_device_id: credentials.keys.device_id.clone(),
        });
        let list = build_replacement_list(
            &credentials.user_id,
            verified.generation,
            &verified.list.entries,
            entries,
            &credentials.keys.device_id,
            &signer,
            issued,
            Some(issued + DAY_MS),
        )?;
        let room = RoomKeyPair::generate()?;
        let _response: Value = self
            .inner
            .http
            .device(
                Method::POST,
                "api/me/recovery-key",
                &credentials,
                Some(json!({
                    "deviceCertificate": BASE64.encode(cert.body_bytes),
                    "deviceCertificateSignature": BASE64.encode(cert.signature),
                    "signedDeviceList": BASE64.encode(list.body_bytes),
                    "signedDeviceListSignature": BASE64.encode(list.signature),
                    "roomKeyBox": BASE64.encode(key.seal(&room)?),
                })),
            )
            .await?;
        let proof = crypto::signed_fields(
            b"kodosi-room-recipient-v1",
            &[
                credentials.user_id.as_bytes(),
                key.device_id.as_bytes(),
                room.public(),
            ],
        )?;
        let _response: Value = self
            .inner
            .http
            .device(
                Method::PUT,
                "api/me/room-key",
                &credentials,
                Some(json!({
                    "publicKey": BASE64.encode(room.public()),
                    "signature": BASE64.encode(crypto::sign(key.signing_key(), &proof)?),
                    "recoveryDeviceId": key.device_id,
                })),
            )
            .await?;
        self.fetch_identity(&credentials.user_id).await?;
        let mut events = vec![json!({"type":"devices.recovery.created","key":text.as_str()})];
        events.extend(self.device_events().await?);
        Ok(events)
    }

    pub(super) async fn use_recovery_key(&self, typed: &str) -> Result<Vec<Value>> {
        let mut credentials = self.credentials()?;
        if credentials.enrolled {
            return Err(invalid("This device is already approved."));
        }
        let pins = Arc::clone(&self.inner.state.lock().await.pins);
        if pins
            .lock()
            .map_err(|_| Error::Closed)?
            .is_revoked(&credentials.user_id, &credentials.keys.device_id)
        {
            credentials = self.replace_local_keys(credentials).await?;
        }
        let bundle: IdentityBundle = self
            .inner
            .http
            .bearer(Method::GET, "api/me/identity", &credentials.token, None)
            .await?;
        self.check_credentials(&credentials)?;
        if bundle.user_id != credentials.user_id {
            return Err(Error::Trust(
                "The server sent the trusted devices of another account.".into(),
            ));
        }
        let identity_root = bundle.root()?;
        let key = RecoveryKey::derive(typed, &credentials.user_id, &identity_root)?;
        let known = Pins::without_time(&bundle, &identity_root)?;
        if known
            .devices
            .get(&key.device_id)
            .is_none_or(|entry| entry.sig_public_key != key.signing_public())
        {
            return Err(Error::Trust(
                "This is not the recovery key of this account.".into(),
            ));
        }
        let saved: SavedRoomKey = self
            .inner
            .http
            .bearer(Method::GET, "api/me/recovery-key", &credentials.token, None)
            .await?;
        if saved.device_id != key.device_id {
            return Err(Error::Trust(
                "This is not the recovery key of this account.".into(),
            ));
        }
        let room_keys = key.open(&decode(&saved.room_key_box, SAVED_ROOM_KEY_BYTES)?)?;
        self.approve_with_recovery_key(&credentials, &key, &known)
            .await?;
        let secrets = self.inner.state.lock().await.secrets.clone();
        let (user, device, entry) = (
            credentials.user_id.clone(),
            credentials.keys.device_id.clone(),
            key.device_id.clone(),
        );
        credentials.keys = Arc::new(
            secrets
                .run(credentials.cancel.clone(), move |store| {
                    DeviceKeys::keep_recovery(store, &user, &device, &entry, room_keys)
                })
                .await?,
        );
        self.check_credentials(&credentials)?;
        *self.inner.credentials.write().map_err(|_| Error::Closed)? = Some(credentials.clone());
        let current: IdentityBundle = self
            .inner
            .http
            .bearer(Method::GET, "api/me/identity", &credentials.token, None)
            .await?;
        let verified = self.verify_bundle(&current, Some(identity_root)).await?;
        if !device_identity::device_enrolled(&verified, &credentials) {
            return Err(Error::Trust(
                "The recovery key did not approve this device.".into(),
            ));
        }
        self.ensure_enrolled().await?;
        if !self.identity().is_some_and(|identity| identity.enrolled) {
            return Err(invalid(
                "This device could not be approved with the recovery key.",
            ));
        }
        let mut events = vec![self.ready_event(&credentials.user_id, true)];
        events.extend(self.device_events().await?);
        events.push(self.session_event().await?);
        Ok(events)
    }

    async fn approve_with_recovery_key(
        &self,
        credentials: &Credentials,
        key: &RecoveryKey,
        known: &VerifiedIdentity,
    ) -> Result<()> {
        let issued = known.list.successor_issued_at(identity::now_ms())?;
        let cert = build_cert_for(
            &credentials.user_id,
            &credentials.keys.device_id,
            &host_label(),
            credentials.keys.signing_public(),
            &key.device_id,
            key.signing_key(),
            issued,
            None,
        )?;
        let mut entries = known.list.entries.clone();
        entries.push(DeviceListEntry {
            device_id: credentials.keys.device_id.clone(),
            signer_device_id: key.device_id.clone(),
        });
        let list = build_replacement_list(
            &credentials.user_id,
            known.generation,
            &known.list.entries,
            entries,
            &key.device_id,
            key.signing_key(),
            issued,
            Some(issued + DAY_MS),
        )?;
        let challenge: Value = self
            .inner
            .http
            .bearer(
                Method::POST,
                "api/me/devices/challenge",
                &credentials.token,
                None,
            )
            .await?;
        let possession = crypto::sign_pop_challenge(
            &credentials.keys.signing_key()?,
            &wire::decode_b64(&challenge, "challengeBytes", 32)?,
        )?;
        let _response: Value = self
            .inner
            .http
            .bearer(
                Method::POST,
                "api/me/devices/recover",
                &credentials.token,
                Some(json!({
                    "deviceId": credentials.keys.device_id,
                    "signingPublicKey": BASE64.encode(credentials.keys.signing_public()),
                    "challengeId": challenge["challengeId"],
                    "popSignature": BASE64.encode(possession),
                    "deviceCertificate": BASE64.encode(cert.body_bytes),
                    "deviceCertificateSignature": BASE64.encode(cert.signature),
                    "signedDeviceList": BASE64.encode(list.body_bytes),
                    "signedDeviceListSignature": BASE64.encode(list.signature),
                })),
            )
            .await?;
        Ok(())
    }
}
