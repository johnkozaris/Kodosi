use serde::Deserialize;

use super::*;
use crate::identity::{
    keys::RoomKeyPair,
    pins::decode,
    recovery::{self, RecoveryKey},
};

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
        let room_keys = RoomKeyPair::generate()?;
        let mut body = device_identity::sign_addition(
            &verified,
            &device_identity::NewDevice {
                device_id: &key.device_id,
                label: recovery::LABEL,
                signing_public: &key.signing_public(),
            },
            &credentials.keys.device_id,
            &credentials.keys.signing_key()?,
        )?;
        let proof =
            rooms::room_key_proof(&credentials.user_id, &key.device_id, room_keys.public())?;
        body["roomKeyBox"] = json!(BASE64.encode(key.seal(&room_keys)?));
        body["roomKey"] = json!({
            "publicKey": BASE64.encode(room_keys.public()),
            "signature": BASE64.encode(crypto::sign(key.signing_key(), &proof)?),
        });
        let _response: Value = self
            .inner
            .http
            .device(
                Method::POST,
                "api/me/recovery-key",
                &credentials,
                Some(body),
            )
            .await?;
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
        if !known.devices.contains_key(&credentials.keys.device_id) {
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
            credentials.keys = self
                .keep_recovery_room_key(&credentials, Some((key.device_id.clone(), room_keys)))
                .await?;
            self.approve_with_recovery_key(&credentials, &key, &known)
                .await?;
        }
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

    pub(super) async fn keep_recovery_room_key(
        &self,
        credentials: &Credentials,
        recovery: Option<(String, RoomKeyPair)>,
    ) -> Result<Arc<DeviceKeys>> {
        let secrets = self.inner.state.lock().await.secrets.clone();
        let (user, device) = (
            credentials.user_id.clone(),
            credentials.keys.device_id.clone(),
        );
        let keys = Arc::new(
            secrets
                .run(credentials.cancel.clone(), move |store| {
                    DeviceKeys::set_recovery(store, &user, &device, recovery)
                })
                .await?,
        );
        self.check_credentials(credentials)?;
        if let Some(current) = self
            .inner
            .credentials
            .write()
            .map_err(|_| Error::Closed)?
            .as_mut()
            && current.keys.device_id == keys.device_id
        {
            current.keys = Arc::clone(&keys);
        }
        Ok(keys)
    }

    async fn approve_with_recovery_key(
        &self,
        credentials: &Credentials,
        key: &RecoveryKey,
        known: &VerifiedIdentity,
    ) -> Result<()> {
        let mut body = device_identity::sign_addition(
            known,
            &device_identity::NewDevice {
                device_id: &credentials.keys.device_id,
                label: &host_label(),
                signing_public: credentials.keys.signing_public(),
            },
            &key.device_id,
            key.signing_key(),
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
        body["deviceId"] = json!(credentials.keys.device_id);
        body["signingPublicKey"] = json!(BASE64.encode(credentials.keys.signing_public()));
        body["challengeId"] = challenge["challengeId"].clone();
        body["popSignature"] = json!(BASE64.encode(possession));
        let _response: Value = self
            .inner
            .http
            .bearer(
                Method::POST,
                "api/me/devices/recover",
                &credentials.token,
                Some(body),
            )
            .await?;
        Ok(())
    }
}
