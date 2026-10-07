use aws_lc_rs::signature::{KeyPair as _, ML_DSA_65_SIGNING, PqdsaKeyPair};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::{Zeroize as _, Zeroizing};

use super::storage::Secrets;
use crate::network::{Result, invalid};

pub struct DeviceKeys {
    pub device_id: String,
    signing_pkcs8: Zeroizing<Vec<u8>>,
    signing_public: Vec<u8>,
    room_secret: Zeroizing<Vec<u8>>,
    room_public: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct StoredKeys {
    version: u8,
    device_id: String,
    signing_pkcs8: String,
    #[serde(default)]
    room_kem_secret: Option<String>,
    #[serde(default)]
    room_kem_public: Option<String>,
}

impl Drop for StoredKeys {
    fn drop(&mut self) {
        self.signing_pkcs8.zeroize();
        self.room_kem_secret.zeroize();
    }
}

impl DeviceKeys {
    pub fn load_or_create(store: &Secrets, user_id: &str) -> Result<Self> {
        let account = Uuid::parse_str(user_id).map_err(|_| invalid("Invalid account identity."))?;
        let label = format!("{account}.device");
        if let Some(payload) = store.load(&label)? {
            let keys = Self::decode(&payload)?;
            if serde_json::from_str::<StoredKeys>(&payload)?
                .room_kem_secret
                .is_none()
            {
                store.store(&label, &keys.encode()?)?;
            }
            return Ok(keys);
        }
        let key = Self::generate()?;
        store.store(&label, &key.encode()?)?;
        Ok(key)
    }

    pub(crate) fn replace_revoked(
        store: &Secrets,
        user_id: &str,
        expected_device: &str,
    ) -> Result<Self> {
        let account = Uuid::parse_str(user_id).map_err(|_| invalid("Invalid account identity."))?;
        let label = format!("{account}.device");
        let payload = store
            .load(&label)?
            .ok_or_else(|| invalid("The removed local device identity is missing."))?;
        let previous = Self::decode(&payload)?;
        if previous.device_id != expected_device {
            return Err(invalid(
                "The local device identity changed during reapproval.",
            ));
        }
        store.store(&format!("{account}.revoked.{expected_device}"), &payload)?;
        let next = Self::generate()?;
        store.store(&label, &next.encode()?)?;
        Ok(next)
    }

    pub fn generate() -> Result<Self> {
        let pair = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING)
            .map_err(|_| invalid("Cannot create device signing key."))?;
        let pkcs8 = pair
            .to_pkcs8v1()
            .map_err(|_| invalid("Cannot encode device signing key."))?;
        let (room_secret, room_public) = Self::new_room_key()?;
        Ok(Self {
            device_id: Uuid::now_v7().to_string(),
            signing_pkcs8: Zeroizing::new(pkcs8.as_ref().to_vec()),
            signing_public: pair.public_key().as_ref().to_vec(),
            room_secret,
            room_public,
        })
    }

    fn new_room_key() -> Result<(Zeroizing<Vec<u8>>, Vec<u8>)> {
        let key = aws_lc_rs::kem::DecapsulationKey::generate(&aws_lc_rs::kem::ML_KEM_768)
            .map_err(|_| invalid("Cannot create the room encryption key."))?;
        let private = key
            .key_bytes()
            .map_err(|_| invalid("Cannot save the room encryption key."))?;
        let public = key
            .encapsulation_key()
            .and_then(|key| key.key_bytes())
            .map_err(|_| invalid("Cannot encode the room encryption key."))?;
        Ok((
            Zeroizing::new(private.as_ref().to_vec()),
            public.as_ref().to_vec(),
        ))
    }

    pub(crate) fn room_public(&self) -> &[u8] {
        &self.room_public
    }

    pub(crate) fn room_key(&self) -> Result<aws_lc_rs::kem::DecapsulationKey> {
        aws_lc_rs::kem::DecapsulationKey::new(&aws_lc_rs::kem::ML_KEM_768, &self.room_secret)
            .map_err(|_| invalid("Stored room encryption key is invalid."))
    }

    pub fn signing_public(&self) -> &[u8] {
        &self.signing_public
    }
    pub fn signing_pkcs8(&self) -> &[u8] {
        &self.signing_pkcs8
    }
    pub fn signing_key(&self) -> Result<PqdsaKeyPair> {
        PqdsaKeyPair::from_pkcs8(&ML_DSA_65_SIGNING, &self.signing_pkcs8)
            .map_err(|_| invalid("Stored device signing key is invalid."))
    }

    fn encode(&self) -> Result<Zeroizing<String>> {
        Ok(Zeroizing::new(serde_json::to_string(&StoredKeys {
            version: 1,
            device_id: self.device_id.clone(),
            signing_pkcs8: BASE64.encode(self.signing_pkcs8.as_slice()),
            room_kem_secret: Some(BASE64.encode(self.room_secret.as_slice())),
            room_kem_public: Some(BASE64.encode(&self.room_public)),
        })?))
    }

    fn decode(payload: &str) -> Result<Self> {
        let stored: StoredKeys = serde_json::from_str(payload)?;
        if stored.version != 1 || Uuid::parse_str(&stored.device_id).is_err() {
            return Err(invalid(
                "Stored device identity is invalid; refusing to replace it.",
            ));
        }
        let signing_pkcs8 = Zeroizing::new(
            BASE64
                .decode(&stored.signing_pkcs8)
                .map_err(|_| invalid("Invalid stored signing key."))?,
        );
        let pair = PqdsaKeyPair::from_pkcs8(&ML_DSA_65_SIGNING, &signing_pkcs8)
            .map_err(|_| invalid("Invalid stored signing key."))?;
        let (room_secret, room_public) = match (&stored.room_kem_secret, &stored.room_kem_public) {
            (Some(secret), Some(public)) => {
                let secret = BASE64
                    .decode(secret)
                    .map_err(|_| invalid("Invalid stored room key."))?;
                let public = BASE64
                    .decode(public)
                    .map_err(|_| invalid("Invalid stored room public key."))?;
                let private =
                    aws_lc_rs::kem::DecapsulationKey::new(&aws_lc_rs::kem::ML_KEM_768, &secret)
                        .map_err(|_| invalid("Invalid stored room key."))?;
                let encapsulation =
                    aws_lc_rs::kem::EncapsulationKey::new(&aws_lc_rs::kem::ML_KEM_768, &public)
                        .map_err(|_| invalid("Invalid stored room public key."))?;
                let (ciphertext, expected) = encapsulation
                    .encapsulate()
                    .map_err(|_| invalid("Invalid stored room key pair."))?;
                let actual = private
                    .decapsulate(ciphertext)
                    .map_err(|_| invalid("Invalid stored room key pair."))?;
                if actual.as_ref() != expected.as_ref() {
                    return Err(invalid("Stored room keys do not match."));
                }
                (Zeroizing::new(secret), public)
            }
            (None, None) => Self::new_room_key()?,
            _ => {
                return Err(invalid(
                    "Stored room identity is incomplete; refusing to replace it.",
                ));
            }
        };
        Ok(Self {
            device_id: stored.device_id.clone(),
            signing_pkcs8,
            signing_public: pair.public_key().as_ref().to_vec(),
            room_secret,
            room_public,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_replacement_preserves_old_secret_and_does_not_reuse_removed_identity() {
        let root = tempfile::tempdir().unwrap();
        let store = Secrets::new(root.path().to_owned(), "test".into(), true);
        let user = Uuid::now_v7().to_string();
        let old = DeviceKeys::load_or_create(&store, &user).unwrap();
        assert!(DeviceKeys::replace_revoked(&store, &user, "unrelated-device").is_err());
        let replacement = DeviceKeys::replace_revoked(&store, &user, &old.device_id).unwrap();
        assert_ne!(replacement.device_id, old.device_id);
        assert_ne!(replacement.signing_public(), old.signing_public());
        assert_eq!(
            DeviceKeys::load_or_create(&store, &user).unwrap().device_id,
            replacement.device_id
        );
        let archive = store
            .load(&format!("{user}.revoked.{}", old.device_id))
            .unwrap()
            .unwrap();
        assert_eq!(
            DeviceKeys::decode(&archive).unwrap().device_id,
            old.device_id
        );
    }

    #[test]
    fn stored_keys_of_an_earlier_version_load_with_the_same_device_and_signing_key() {
        let keys = DeviceKeys::generate().unwrap();
        let mut stored: serde_json::Value = serde_json::from_str(&keys.encode().unwrap()).unwrap();
        stored["kem_secret"] = "AAAA".into();
        stored["kem_public"] = "AAAA".into();
        let loaded = DeviceKeys::decode(&stored.to_string()).unwrap();
        assert_eq!(loaded.device_id, keys.device_id);
        assert_eq!(loaded.signing_public(), keys.signing_public());
    }

    #[test]
    fn identity_round_trip_preserves_keys() {
        let first = DeviceKeys::generate().unwrap();
        let second = DeviceKeys::decode(&first.encode().unwrap()).unwrap();
        assert_eq!(first.device_id, second.device_id);
        assert_eq!(first.signing_public(), second.signing_public());
    }
}
