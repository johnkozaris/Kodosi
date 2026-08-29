use std::{fs, path::PathBuf};

use aws_lc_rs::{
    kem::{DecapsulationKey, ML_KEM_768},
    signature::{KeyPair, ML_DSA_65_SIGNING, PqdsaKeyPair},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::{
    AppError, Result,
    support::{
        platform::fs as support_fs,
        storage::atomic_file::{FileMode, atomic_write},
    },
};

use super::platform_secrets::{
    PlatformSecretLoadResult, PlatformSecretStore, PlatformSecretStoreResult,
};

pub(crate) struct DeviceKeys {
    pub(crate) device_id: String,
    kem_secret_bytes: Vec<u8>,
    kem_public: Vec<u8>,
    signing_pkcs8: Vec<u8>,
    signing_public: Vec<u8>,
}

impl DeviceKeys {
    pub(crate) fn kem_public_bytes(&self) -> &[u8] {
        &self.kem_public
    }

    pub(crate) fn signing_public_bytes(&self) -> &[u8] {
        &self.signing_public
    }

    pub(crate) fn signing_pkcs8_bytes(&self) -> &[u8] {
        &self.signing_pkcs8
    }

    pub(crate) fn kem_decapsulation_key(&self) -> Result<DecapsulationKey> {
        DecapsulationKey::new(&ML_KEM_768, &self.kem_secret_bytes).map_err(|_| {
            AppError::Unsupported {
                reason: "failed to reconstruct ML-KEM-768 decapsulation key".to_owned(),
            }
        })
    }

    pub(crate) fn signing_key(&self) -> Result<PqdsaKeyPair> {
        PqdsaKeyPair::from_pkcs8(&ML_DSA_65_SIGNING, &self.signing_pkcs8).map_err(|_| {
            AppError::Unsupported {
                reason: "failed to reconstruct ML-DSA-65 signing key".to_owned(),
            }
        })
    }

    pub(crate) fn kem_secret_bytes_raw(&self) -> &[u8] {
        &self.kem_secret_bytes
    }
}

impl Drop for DeviceKeys {
    fn drop(&mut self) {
        self.kem_secret_bytes.zeroize();
        self.signing_pkcs8.zeroize();
    }
}

#[derive(Serialize, Deserialize)]
struct StoredDeviceKeys {
    #[serde(default)]
    device_id: Option<String>,
    kem_secret: String,
    kem_public: String,
    signing_pkcs8: String,
}

impl Drop for StoredDeviceKeys {
    fn drop(&mut self) {
        self.kem_secret.zeroize();
        self.signing_pkcs8.zeroize();
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DeviceKeyStore {
    file_dir: PathBuf,
    platform_store: std::sync::Arc<PlatformSecretStore>,
}

impl DeviceKeyStore {
    pub(crate) fn new(service_name: &str) -> Result<Self> {
        let file_dir = crate::support::storage::paths::secrets_dir()?;
        let platform_store =
            std::sync::Arc::new(PlatformSecretStore::new(service_name, file_dir.clone()));
        if crate::support::storage::paths::isolated_root()?.is_some() {
            platform_store.disable_secure_store();
        }
        Ok(Self {
            file_dir,
            platform_store,
        })
    }

    pub(crate) fn at(file_dir: PathBuf, service_name: &str) -> Self {
        let platform_store =
            std::sync::Arc::new(PlatformSecretStore::new(service_name, file_dir.clone()));
        platform_store.disable_secure_store();
        Self {
            file_dir,
            platform_store,
        }
    }

    #[cfg(test)]
    pub(crate) fn save_for_test(&self, user_id: &str, keys: &DeviceKeys) -> Result<()> {
        self.save(user_id, keys)
    }

    pub(crate) fn load_if_present(&self, user_id: &str) -> Result<Option<DeviceKeys>> {
        self.load(user_id)
    }

    pub(crate) fn load_or_generate(&self, user_id: &str) -> Result<DeviceKeys> {
        match self.load(user_id) {
            Ok(Some(mut keys)) => {
                if keys.device_id.is_empty() {
                    keys.device_id = generate_device_id(user_id);
                    self.save(user_id, &keys)?;
                }
                Ok(keys)
            }
            Ok(None) => {
                let keys = generate_device_keys(user_id)?;
                self.save(user_id, &keys)?;
                Ok(keys)
            }
            Err(error) if is_corrupt_device_identity(&error) => {
                Err(AppError::IdentityRecoveryRequired {
                    reason: format!(
                        "stored device identity is unreadable; evidence was preserved: {error}"
                    ),
                })
            }
            Err(error) => Err(error),
        }
    }

    fn account_label(user_id: &str) -> String {
        format!("{user_id}.device_keys")
    }

    fn file_path(&self, user_id: &str) -> PathBuf {
        self.file_dir
            .join(format!("{}.device_keys.json", user_id.replace('/', "_")))
    }

    fn ensure_dir(&self) -> Result<()> {
        support_fs::ensure_dir(&self.file_dir)
    }

    fn load_from_file(&self, user_id: &str) -> Result<Option<DeviceKeys>> {
        let path = self.file_path(user_id);
        match fs::read_to_string(&path) {
            Ok(payload) => parse_stored_device_keys(&Zeroizing::new(payload)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(AppError::Io(e)),
        }
    }

    fn save_to_file(&self, user_id: &str, keys: &DeviceKeys) -> Result<()> {
        self.ensure_dir()?;
        let path = self.file_path(user_id);
        let payload = serialize_device_keys(keys)?;
        atomic_write(&path, payload.as_bytes(), FileMode::UserPrivate)
    }

    fn clear_file(&self, user_id: &str) -> Result<()> {
        let path = self.file_path(user_id);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AppError::Io(e)),
        }
    }
}

impl DeviceKeyStore {
    fn secure_store_unavailable_error() -> AppError {
        AppError::IdentityRecoveryRequired {
            reason: "platform device-key storage is unavailable and no file-backed identity exists; refusing to generate a replacement identity"
                .to_owned(),
        }
    }

    fn load(&self, user_id: &str) -> Result<Option<DeviceKeys>> {
        let label = Self::account_label(user_id);
        let mut promote_file_fallback = false;
        let mut requires_existing_file_fallback = false;
        match self.platform_store.load(&label)? {
            PlatformSecretLoadResult::Loaded(payload) => {
                return parse_stored_device_keys(payload.as_str());
            }
            PlatformSecretLoadResult::Missing => {
                promote_file_fallback = true;
            }
            PlatformSecretLoadResult::Unavailable {
                requires_existing_file_fallback: requires_fallback,
            } => {
                requires_existing_file_fallback = requires_fallback;
            }
        }

        let keys = self.load_from_file(user_id)?;
        if requires_existing_file_fallback {
            let Some(loaded_keys) = keys else {
                return Err(Self::secure_store_unavailable_error());
            };
            self.platform_store.delete(&label)?;
            self.platform_store.disable_secure_store();
            return Ok(Some(loaded_keys));
        }
        if promote_file_fallback
            && let Some(loaded_keys) = keys.as_ref()
            && let Err(error) = self.save(user_id, loaded_keys)
        {
            tracing::warn!(%error, "could not promote file-backed device keys to platform storage");
        }
        Ok(keys)
    }

    fn save(&self, user_id: &str, keys: &DeviceKeys) -> Result<()> {
        let label = Self::account_label(user_id);
        let payload = serialize_device_keys(keys)?;

        match self.platform_store.store(&label, &payload)? {
            PlatformSecretStoreResult::Stored => {
                self.clear_file(user_id)?;
                Ok(())
            }
            PlatformSecretStoreResult::Unavailable => {
                self.save_to_file(user_id, keys)?;
                self.platform_store.delete(&label)?;
                self.platform_store.disable_secure_store();
                Ok(())
            }
        }
    }

    pub(crate) fn clear(&self, user_id: &str) -> Result<()> {
        let label = Self::account_label(user_id);
        let secure_store_result = self.platform_store.delete(&label);
        let file_result = self.clear_file(user_id);
        secure_store_result?;
        file_result
    }
}

impl DeviceKeyStore {
    pub(crate) fn wipe(&self, user_id: &str) -> Result<()> {
        self.clear(user_id)
    }
}

fn parse_stored_device_keys(payload: &str) -> Result<Option<DeviceKeys>> {
    let stored: StoredDeviceKeys = serde_json::from_str(payload).map_err(AppError::Json)?;

    let kem_secret_bytes =
        BASE64
            .decode(&stored.kem_secret)
            .map_err(|e| AppError::Unsupported {
                reason: format!("invalid device KEM key: {e}"),
            })?;
    let kem_public = BASE64
        .decode(&stored.kem_public)
        .map_err(|e| AppError::Unsupported {
            reason: format!("invalid device KEM public key: {e}"),
        })?;
    let signing_pkcs8 =
        BASE64
            .decode(&stored.signing_pkcs8)
            .map_err(|e| AppError::Unsupported {
                reason: format!("invalid device signing key: {e}"),
            })?;

    DecapsulationKey::new(&ML_KEM_768, &kem_secret_bytes).map_err(|_| AppError::Unsupported {
        reason: "stored ML-KEM-768 key is invalid".to_owned(),
    })?;

    let signing_kp =
        PqdsaKeyPair::from_pkcs8(&ML_DSA_65_SIGNING, &signing_pkcs8).map_err(|_| {
            AppError::Unsupported {
                reason: "stored ML-DSA-65 key is invalid".to_owned(),
            }
        })?;
    let signing_public = signing_kp.public_key().as_ref().to_vec();

    let device_id = stored.device_id.clone().unwrap_or_default();

    Ok(Some(DeviceKeys {
        device_id,
        kem_secret_bytes,
        kem_public,
        signing_pkcs8,
        signing_public,
    }))
}

fn serialize_device_keys(keys: &DeviceKeys) -> Result<Zeroizing<String>> {
    let stored = StoredDeviceKeys {
        device_id: Some(keys.device_id.clone()),
        kem_secret: BASE64.encode(&keys.kem_secret_bytes),
        kem_public: BASE64.encode(&keys.kem_public),
        signing_pkcs8: BASE64.encode(&keys.signing_pkcs8),
    };
    serde_json::to_string(&stored)
        .map(Zeroizing::new)
        .map_err(AppError::Json)
}

fn is_corrupt_device_identity(error: &AppError) -> bool {
    matches!(error, AppError::Json(_) | AppError::Unsupported { .. })
}

fn generate_device_id(user_id: &str) -> String {
    format!("{user_id}-{}", uuid::Uuid::now_v7())
}

fn generate_device_keys(user_id: &str) -> Result<DeviceKeys> {
    let kem_dk = DecapsulationKey::generate(&ML_KEM_768).map_err(|_| AppError::Unsupported {
        reason: "ML-KEM-768 key generation failed".to_owned(),
    })?;
    let kem_secret_bytes = kem_dk
        .key_bytes()
        .map_err(|_| AppError::Unsupported {
            reason: "failed to serialize ML-KEM-768 decapsulation key".to_owned(),
        })?
        .as_ref()
        .to_vec();
    let kem_public = kem_dk
        .encapsulation_key()
        .map_err(|_| AppError::Unsupported {
            reason: "failed to derive ML-KEM-768 encapsulation key".to_owned(),
        })?
        .key_bytes()
        .map_err(|_| AppError::Unsupported {
            reason: "failed to serialize ML-KEM-768 public key".to_owned(),
        })?
        .as_ref()
        .to_vec();

    let signing_kp =
        PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).map_err(|_| AppError::Unsupported {
            reason: "ML-DSA-65 key generation failed".to_owned(),
        })?;
    let signing_pkcs8 = signing_kp
        .to_pkcs8v1()
        .map_err(|_| AppError::Unsupported {
            reason: "failed to serialize ML-DSA-65 keypair to PKCS#8".to_owned(),
        })?
        .as_ref()
        .to_vec();
    let signing_public = signing_kp.public_key().as_ref().to_vec();

    Ok(DeviceKeys {
        device_id: generate_device_id(user_id),
        kem_secret_bytes,
        kem_public,
        signing_pkcs8,
        signing_public,
    })
}

#[cfg(test)]
pub(crate) fn generate_device_keys_for_test(user_id: &str) -> Result<DeviceKeys> {
    generate_device_keys(user_id)
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "These cryptography round-trip tests assert infallible setup for fixed test inputs and fail fast if the fixture assumptions break."
)]
mod tests {
    use super::*;

    #[test]
    fn generate_produces_valid_keys_with_device_id() {
        let keys = generate_device_keys("user123").unwrap();
        assert!(keys.device_id.starts_with("user123-"));
        assert_eq!(keys.kem_public_bytes().len(), 1184);
        assert_eq!(keys.signing_public_bytes().len(), 1952);
    }

    #[test]
    fn different_calls_produce_different_device_ids() {
        let keys1 = generate_device_keys("user123").unwrap();
        let keys2 = generate_device_keys("user123").unwrap();
        assert_ne!(keys1.device_id, keys2.device_id);
    }

    #[test]
    fn kem_encapsulate_decapsulate_round_trip() {
        use aws_lc_rs::kem::EncapsulationKey;
        let keys = generate_device_keys("alice").unwrap();

        let encaps_key = EncapsulationKey::new(&ML_KEM_768, keys.kem_public_bytes()).unwrap();
        let (ciphertext, shared_secret_enc) = encaps_key.encapsulate().unwrap();

        let dk = keys.kem_decapsulation_key().unwrap();
        let shared_secret_dec = dk.decapsulate(ciphertext).unwrap();

        assert_eq!(shared_secret_enc.as_ref(), shared_secret_dec.as_ref());
    }

    #[test]
    fn signing_round_trip() {
        use aws_lc_rs::signature::{ML_DSA_65, VerificationAlgorithm};
        let keys = generate_device_keys("alice").unwrap();
        let message = b"test message for signing";

        let signing_key = keys.signing_key().unwrap();
        let mut signature_buf = vec![0u8; 3309];
        let sig_len = signing_key.sign(message, &mut signature_buf).unwrap();
        let signature = &signature_buf[..sig_len];

        ML_DSA_65
            .verify_sig(keys.signing_public_bytes(), message, signature)
            .unwrap();
    }

    #[test]
    fn key_serialization_round_trip() {
        use aws_lc_rs::kem::EncapsulationKey;
        let keys = generate_device_keys("user123").unwrap();

        let dk = DecapsulationKey::new(&ML_KEM_768, &keys.kem_secret_bytes).unwrap();
        let encaps_key = EncapsulationKey::new(&ML_KEM_768, &keys.kem_public).unwrap();
        let (ct, ss_enc) = encaps_key.encapsulate().unwrap();
        let ss_dec = dk.decapsulate(ct).unwrap();
        let enc_bytes: &[u8] = ss_enc.as_ref();
        let dec_bytes: &[u8] = ss_dec.as_ref();
        assert_eq!(enc_bytes, dec_bytes);

        let kp = PqdsaKeyPair::from_pkcs8(&ML_DSA_65_SIGNING, &keys.signing_pkcs8).unwrap();
        assert_eq!(kp.public_key().as_ref(), keys.signing_public.as_slice());
    }

    #[test]
    fn json_decode_failure_requires_recovery_without_regeneration() {
        let error = match serde_json::from_str::<StoredDeviceKeys>("not json") {
            Ok(_) => panic!("invalid json should fail"),
            Err(error) => AppError::Json(error),
        };

        assert!(is_corrupt_device_identity(&error));
    }

    #[test]
    fn keychain_access_failure_is_not_identity_corruption() {
        let error = AppError::Keychain {
            reason: "locked".to_owned(),
        };
        assert!(!is_corrupt_device_identity(&error));
    }

    #[test]
    fn corrupt_file_is_preserved_and_requires_recovery() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = DeviceKeyStore {
            file_dir: temp_dir.path().to_path_buf(),
            platform_store: std::sync::Arc::new(PlatformSecretStore::new(
                "kodosi-test",
                temp_dir.path().to_path_buf(),
            )),
        };
        store.platform_store.disable_secure_store();
        let path = store.file_path("alice");
        std::fs::write(&path, "not json").unwrap();

        let error = match store.load_or_generate("alice") {
            Ok(_) => panic!("corrupt enrolled identity must fail closed"),
            Err(error) => error,
        };

        std::assert_matches!(error, AppError::IdentityRecoveryRequired { .. });
        assert_eq!(std::fs::read_to_string(path).unwrap(), "not json");
    }

    #[test]
    fn load_or_generate_fails_when_persistence_is_unavailable() {
        let blocking_file = tempfile::NamedTempFile::new().unwrap();
        let store = DeviceKeyStore {
            file_dir: blocking_file.path().to_path_buf(),
            platform_store: std::sync::Arc::new(PlatformSecretStore::new(
                "kodosi-test",
                blocking_file.path().to_path_buf(),
            )),
        };
        store.platform_store.disable_secure_store();

        let error = match store.load_or_generate("alice") {
            Ok(_) => panic!("device keys should not load when persistence is unavailable"),
            Err(error) => error,
        };

        std::assert_matches!(error, AppError::Io(_));
    }
}
