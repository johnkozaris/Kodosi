use aws_lc_rs::{
    kem::{DecapsulationKey, ML_KEM_768},
    signature::{KeyPair as _, ML_DSA_65_SIGNING, PqdsaKeyPair},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::{Zeroize as _, Zeroizing};

use super::storage::Secrets;
use crate::network::{Result, invalid};

pub struct DeviceKeys {
    pub device_id: String,
    kem_secret: Zeroizing<Vec<u8>>,
    kem_public: Vec<u8>,
    signing_pkcs8: Zeroizing<Vec<u8>>,
    signing_public: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredKeys {
    version: u8,
    device_id: String,
    kem_secret: String,
    kem_public: String,
    signing_pkcs8: String,
}

impl Drop for StoredKeys {
    fn drop(&mut self) {
        self.kem_secret.zeroize();
        self.signing_pkcs8.zeroize();
    }
}

impl DeviceKeys {
    pub fn load_or_create(store: &Secrets, user_id: &str) -> Result<Self> {
        let account = Uuid::parse_str(user_id).map_err(|_| invalid("Invalid account identity."))?;
        let label = format!("{account}.device");
        if let Some(payload) = store.load(&label)? {
            return Self::decode(&payload);
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
        let kem = DecapsulationKey::generate(&ML_KEM_768)
            .map_err(|_| invalid("Cannot create device encryption key."))?;
        let public = kem
            .encapsulation_key()
            .map_err(|_| invalid("Cannot obtain device encryption public key."))?;
        let pair = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING)
            .map_err(|_| invalid("Cannot create device signing key."))?;
        let pkcs8 = pair
            .to_pkcs8v1()
            .map_err(|_| invalid("Cannot encode device signing key."))?;
        Ok(Self {
            device_id: Uuid::now_v7().to_string(),
            kem_secret: Zeroizing::new(
                kem.key_bytes()
                    .map_err(|_| invalid("Cannot encode device encryption key."))?
                    .as_ref()
                    .to_vec(),
            ),
            kem_public: public
                .key_bytes()
                .map_err(|_| invalid("Cannot encode device encryption public key."))?
                .as_ref()
                .to_vec(),
            signing_pkcs8: Zeroizing::new(pkcs8.as_ref().to_vec()),
            signing_public: pair.public_key().as_ref().to_vec(),
        })
    }

    pub fn kem_public(&self) -> &[u8] {
        &self.kem_public
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
    pub fn kem_key(&self) -> Result<DecapsulationKey> {
        DecapsulationKey::new(&ML_KEM_768, &self.kem_secret)
            .map_err(|_| invalid("Stored device encryption key is invalid."))
    }

    fn encode(&self) -> Result<Zeroizing<String>> {
        Ok(Zeroizing::new(serde_json::to_string(&StoredKeys {
            version: 1,
            device_id: self.device_id.clone(),
            kem_secret: BASE64.encode(self.kem_secret.as_slice()),
            kem_public: BASE64.encode(&self.kem_public),
            signing_pkcs8: BASE64.encode(self.signing_pkcs8.as_slice()),
        })?))
    }

    fn decode(payload: &str) -> Result<Self> {
        let stored: StoredKeys = serde_json::from_str(payload)?;
        if stored.version != 1 || Uuid::parse_str(&stored.device_id).is_err() {
            return Err(invalid(
                "Stored device identity is invalid; refusing to replace it.",
            ));
        }
        let kem_secret = Zeroizing::new(
            BASE64
                .decode(&stored.kem_secret)
                .map_err(|_| invalid("Invalid stored encryption key."))?,
        );
        let kem_public = BASE64
            .decode(&stored.kem_public)
            .map_err(|_| invalid("Invalid stored public key."))?;
        let signing_pkcs8 = Zeroizing::new(
            BASE64
                .decode(&stored.signing_pkcs8)
                .map_err(|_| invalid("Invalid stored signing key."))?,
        );
        let kem = DecapsulationKey::new(&ML_KEM_768, &kem_secret)
            .map_err(|_| invalid("Invalid stored encryption key."))?;
        let public = aws_lc_rs::kem::EncapsulationKey::new(&ML_KEM_768, &kem_public)
            .map_err(|_| invalid("Invalid stored encryption public key."))?;
        let (ciphertext, expected) = public
            .encapsulate()
            .map_err(|_| invalid("Cannot validate stored encryption keypair."))?;
        let actual = kem
            .decapsulate(ciphertext)
            .map_err(|_| invalid("Stored encryption keypair does not match."))?;
        aws_lc_rs::constant_time::verify_slices_are_equal(actual.as_ref(), expected.as_ref())
            .map_err(|_| invalid("Stored encryption keypair does not match."))?;
        let pair = PqdsaKeyPair::from_pkcs8(&ML_DSA_65_SIGNING, &signing_pkcs8)
            .map_err(|_| invalid("Invalid stored signing key."))?;
        Ok(Self {
            device_id: stored.device_id.clone(),
            kem_secret,
            kem_public,
            signing_pkcs8,
            signing_public: pair.public_key().as_ref().to_vec(),
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
    fn identity_round_trip_preserves_keys() {
        let first = DeviceKeys::generate().unwrap();
        let second = DeviceKeys::decode(&first.encode().unwrap()).unwrap();
        assert_eq!(first.device_id, second.device_id);
        assert_eq!(first.kem_public(), second.kem_public());
        assert_eq!(first.signing_public(), second.signing_public());
    }
}
