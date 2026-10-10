use aws_lc_rs::{
    agreement,
    encoding::{AsBigEndian as _, Curve25519SeedBin},
    kem,
    signature::{KeyPair as _, ML_DSA_65_SIGNING, PqdsaKeyPair},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::{Zeroize as _, Zeroizing};

use super::storage::Secrets;
use crate::network::{Result, invalid};

pub(crate) const ROOM_KEM_PUBLIC_BYTES: usize = 1184;
pub(crate) const ROOM_PUBLIC_BYTES: usize = ROOM_KEM_PUBLIC_BYTES + AGREEMENT_BYTES;
const ROOM_KEM_SECRET_BYTES: usize = 2400;
const AGREEMENT_BYTES: usize = 32;

pub(crate) struct RoomKeyPair {
    kem_secret: Zeroizing<Vec<u8>>,
    agreement_secret: Zeroizing<Vec<u8>>,
    public: Vec<u8>,
}

impl RoomKeyPair {
    pub(crate) fn generate() -> Result<Self> {
        let kem = kem::DecapsulationKey::generate(&kem::ML_KEM_768)
            .map_err(|_| invalid("Cannot create the room encryption key."))?;
        let kem_secret = kem
            .key_bytes()
            .map_err(|_| invalid("Cannot save the room encryption key."))?;
        let kem_public = kem
            .encapsulation_key()
            .and_then(|key| key.key_bytes())
            .map_err(|_| invalid("Cannot encode the room encryption key."))?;
        let agreement_secret: Curve25519SeedBin<'_> =
            agreement::PrivateKey::generate(&agreement::X25519)
                .and_then(|key| key.as_be_bytes())
                .map_err(|_| invalid("Cannot create the room encryption key."))?;
        Self::from_parts(
            kem_secret.as_ref(),
            kem_public.as_ref(),
            agreement_secret.as_ref(),
        )
    }

    fn from_parts(kem_secret: &[u8], kem_public: &[u8], agreement_secret: &[u8]) -> Result<Self> {
        let private = kem::DecapsulationKey::new(&kem::ML_KEM_768, kem_secret)
            .map_err(|_| invalid("Invalid stored room key."))?;
        let (ciphertext, expected) = kem::EncapsulationKey::new(&kem::ML_KEM_768, kem_public)
            .and_then(|key| key.encapsulate().map_err(Into::into))
            .map_err(|_| invalid("Invalid stored room public key."))?;
        let actual = private
            .decapsulate(ciphertext)
            .map_err(|_| invalid("Invalid stored room key pair."))?;
        if actual.as_ref() != expected.as_ref() {
            return Err(invalid("Stored room keys do not match."));
        }
        let agreement_public =
            agreement::PrivateKey::from_private_key(&agreement::X25519, agreement_secret)
                .ok()
                .and_then(|key| key.compute_public_key().ok())
                .ok_or_else(|| invalid("Invalid stored room key."))?;
        Ok(Self {
            kem_secret: Zeroizing::new(kem_secret.to_vec()),
            agreement_secret: Zeroizing::new(agreement_secret.to_vec()),
            public: [kem_public, agreement_public.as_ref()].concat(),
        })
    }

    pub(crate) fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != ROOM_KEM_SECRET_BYTES + ROOM_KEM_PUBLIC_BYTES + AGREEMENT_BYTES {
            return Err(invalid("Invalid stored room key."));
        }
        let (kem_secret, rest) = bytes.split_at(ROOM_KEM_SECRET_BYTES);
        let (kem_public, agreement_secret) = rest.split_at(ROOM_KEM_PUBLIC_BYTES);
        Self::from_parts(kem_secret, kem_public, agreement_secret)
    }

    pub(crate) fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        Zeroizing::new(
            [
                self.kem_secret.as_slice(),
                &self.public[..ROOM_KEM_PUBLIC_BYTES],
                self.agreement_secret.as_slice(),
            ]
            .concat(),
        )
    }

    pub(crate) fn public(&self) -> &[u8] {
        &self.public
    }

    pub(crate) fn kem(&self) -> Result<kem::DecapsulationKey> {
        kem::DecapsulationKey::new(&kem::ML_KEM_768, &self.kem_secret)
            .map_err(|_| invalid("Stored room encryption key is invalid."))
    }

    pub(crate) fn agreement(&self) -> Result<agreement::PrivateKey> {
        agreement::PrivateKey::from_private_key(&agreement::X25519, &self.agreement_secret)
            .map_err(|_| invalid("Stored room encryption key is invalid."))
    }
}

pub struct DeviceKeys {
    pub device_id: String,
    signing_pkcs8: Zeroizing<Vec<u8>>,
    signing_public: Vec<u8>,
    room: RoomKeyPair,
    recovery: Option<(String, RoomKeyPair)>,
}

#[derive(Serialize, Deserialize)]
struct StoredKeys {
    version: u8,
    device_id: String,
    signing_pkcs8: String,
    room_kem_secret: Option<String>,
    room_kem_public: Option<String>,
    room_agreement_secret: Option<String>,
    recovery_device_id: Option<String>,
    recovery_room: Option<String>,
}

impl Drop for StoredKeys {
    fn drop(&mut self) {
        self.signing_pkcs8.zeroize();
        self.room_kem_secret.zeroize();
        self.room_agreement_secret.zeroize();
        self.recovery_room.zeroize();
    }
}

impl DeviceKeys {
    fn account(user_id: &str) -> Result<Uuid> {
        Uuid::parse_str(user_id).map_err(|_| invalid("Invalid account identity."))
    }

    pub fn load_or_create(store: &Secrets, user_id: &str) -> Result<Self> {
        let label = format!("{}.device", Self::account(user_id)?);
        if let Some(payload) = store.load(&label)? {
            let keys = Self::decode(&payload)?;
            let current = keys.encode()?;
            if *current != *payload {
                store.store(&label, &current)?;
            }
            return Ok(keys);
        }
        let key = Self::generate()?;
        store.store(&label, &key.encode()?)?;
        Ok(key)
    }

    pub(crate) fn forget(store: &Secrets, user_id: &str) -> Result<()> {
        store.delete(&Self::label(user_id)?)
    }

    pub(crate) fn replace_revoked(
        store: &Secrets,
        user_id: &str,
        expected_device: &str,
    ) -> Result<Self> {
        let account = Self::account(user_id)?;
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

    pub(crate) fn set_recovery(
        store: &Secrets,
        user_id: &str,
        expected_device: &str,
        recovery: Option<(String, RoomKeyPair)>,
    ) -> Result<Self> {
        let label = format!("{}.device", Self::account(user_id)?);
        let payload = store
            .load(&label)?
            .ok_or_else(|| invalid("The local device identity is missing."))?;
        let mut keys = Self::decode(&payload)?;
        if keys.device_id != expected_device {
            return Err(invalid(
                "The local device identity changed during recovery.",
            ));
        }
        keys.recovery = recovery;
        store.store(&label, &keys.encode()?)?;
        Ok(keys)
    }

    pub fn generate() -> Result<Self> {
        let pair = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING)
            .map_err(|_| invalid("Cannot create device signing key."))?;
        let pkcs8 = pair
            .to_pkcs8v1()
            .map_err(|_| invalid("Cannot encode device signing key."))?;
        Ok(Self {
            device_id: Uuid::now_v7().to_string(),
            signing_pkcs8: Zeroizing::new(pkcs8.as_ref().to_vec()),
            signing_public: pair.public_key().as_ref().to_vec(),
            room: RoomKeyPair::generate()?,
            recovery: None,
        })
    }

    pub(crate) fn room_public(&self) -> &[u8] {
        self.room.public()
    }

    pub(crate) fn room(&self) -> &RoomKeyPair {
        &self.room
    }

    pub(crate) fn recovery(&self) -> Option<(&str, &RoomKeyPair)> {
        self.recovery
            .as_ref()
            .map(|(device, room)| (device.as_str(), room))
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
        let (kem_public, _) = self.room.public.split_at(ROOM_KEM_PUBLIC_BYTES);
        Ok(Zeroizing::new(serde_json::to_string(&StoredKeys {
            version: 1,
            device_id: self.device_id.clone(),
            signing_pkcs8: BASE64.encode(self.signing_pkcs8.as_slice()),
            room_kem_secret: Some(BASE64.encode(self.room.kem_secret.as_slice())),
            room_kem_public: Some(BASE64.encode(kem_public)),
            room_agreement_secret: Some(BASE64.encode(self.room.agreement_secret.as_slice())),
            recovery_device_id: self.recovery.as_ref().map(|(device, _)| device.clone()),
            recovery_room: self
                .recovery
                .as_ref()
                .map(|(_, room)| BASE64.encode(room.to_bytes().as_slice())),
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
        let bytes = |value: &str, reason: &'static str| {
            BASE64
                .decode(value)
                .map(Zeroizing::new)
                .map_err(|_| invalid(reason))
        };
        let room = match (
            &stored.room_kem_secret,
            &stored.room_kem_public,
            &stored.room_agreement_secret,
        ) {
            (Some(secret), Some(public), Some(agreement)) => RoomKeyPair::from_parts(
                &bytes(secret, "Invalid stored room key.")?,
                &bytes(public, "Invalid stored room public key.")?,
                &bytes(agreement, "Invalid stored room key.")?,
            )?,
            (_, _, None) => RoomKeyPair::generate()?,
            _ => {
                return Err(invalid(
                    "Stored room identity is incomplete; refusing to replace it.",
                ));
            }
        };
        let recovery = match (&stored.recovery_device_id, &stored.recovery_room) {
            (Some(device), Some(room)) => Some((
                device.clone(),
                RoomKeyPair::from_bytes(&bytes(room, "Invalid stored recovery key.")?)?,
            )),
            _ => None,
        };
        Ok(Self {
            device_id: stored.device_id.clone(),
            signing_pkcs8,
            signing_public: pair.public_key().as_ref().to_vec(),
            room,
            recovery,
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
        assert_eq!(first.signing_public(), second.signing_public());
        assert_eq!(first.room_public(), second.room_public());
        assert_eq!(first.room_public().len(), ROOM_PUBLIC_BYTES);
    }

    #[test]
    fn a_stored_identity_without_an_x25519_key_gets_a_new_room_key_and_other_gaps_are_refused() {
        let keys = DeviceKeys::generate().unwrap();
        let stored: serde_json::Value = serde_json::from_str(&keys.encode().unwrap()).unwrap();
        let without = |field: &str| {
            let mut changed = stored.clone();
            changed.as_object_mut().unwrap().remove(field);
            DeviceKeys::decode(&changed.to_string())
        };
        let loaded = without("room_agreement_secret").unwrap();
        assert_eq!(loaded.device_id, keys.device_id);
        assert_eq!(loaded.signing_public(), keys.signing_public());
        assert_ne!(loaded.room_public(), keys.room_public());
        assert!(without("room_kem_public").is_err());
        assert!(without("room_kem_secret").is_err());
    }

    #[test]
    fn the_device_keeps_the_room_key_of_a_recovery_key_until_it_deletes_it() {
        let folder = tempfile::tempdir().unwrap();
        let store = Secrets::new(folder.path().to_owned(), "test".into(), true);
        let user = Uuid::now_v7().to_string();
        let keys = DeviceKeys::load_or_create(&store, &user).unwrap();
        let kept = RoomKeyPair::generate().unwrap();
        let public = kept.public().to_vec();
        assert!(
            DeviceKeys::set_recovery(&store, &user, "other", Some(("recovery".into(), kept)))
                .is_err()
        );
        let kept = RoomKeyPair::from_bytes(&RoomKeyPair::generate().unwrap().to_bytes()).unwrap();
        let public_kept = kept.public().to_vec();
        assert_ne!(public, public_kept);
        DeviceKeys::set_recovery(
            &store,
            &user,
            &keys.device_id,
            Some(("recovery".into(), kept)),
        )
        .unwrap();
        let loaded = DeviceKeys::load_or_create(&store, &user).unwrap();
        let (device, room) = loaded.recovery().unwrap();
        assert_eq!(
            (device, room.public()),
            ("recovery", public_kept.as_slice())
        );
        assert_eq!(loaded.room_public(), keys.room_public());
        DeviceKeys::set_recovery(&store, &user, &keys.device_id, None).unwrap();
        let loaded = DeviceKeys::load_or_create(&store, &user).unwrap();
        assert!(loaded.recovery().is_none());
        assert_eq!(loaded.signing_public(), keys.signing_public());
    }
}
