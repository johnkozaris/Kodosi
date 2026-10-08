use std::collections::{BTreeMap, BTreeSet};

use aws_lc_rs::{
    aead, hkdf, kem, rand,
    signature::{ML_DSA_65, VerificationAlgorithm as _},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{
    identity::{
        keys::DeviceKeys,
        pins::{IdentityBundle, Pins, Root, VerifiedIdentity},
    },
    network::{Result, crypto, invalid},
};

pub(crate) type Secret = Zeroizing<[u8; 32]>;
pub(crate) const STATE_DOMAIN: &[u8] = b"kodosi-room-state-v1";
pub(crate) const CONTENT_DOMAIN: &[u8] = b"kodosi-room-content-v1";

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Sealed {
    pub nonce: String,
    pub ciphertext: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Recipient {
    pub user_id: String,
    pub device_id: String,
    pub kem_ciphertext: String,
    pub sealed: Sealed,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct State {
    pub room_id: Uuid,
    pub owner_user_id: String,
    pub author_id: String,
    pub device_id: String,
    pub version: u64,
    pub epoch: u64,
    pub created_at_ms: u64,
    pub previous_hash: String,
    pub members: BTreeMap<String, IdentityBundle>,
    pub recipients: Vec<Recipient>,
    pub previous_key: Option<Sealed>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SignedState {
    pub version: u64,
    pub body: String,
    pub signature: String,
}

#[derive(Clone)]
pub(crate) struct VerifiedState {
    pub state: State,
    pub hash: String,
    pub identities: BTreeMap<String, VerifiedIdentity>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Content {
    pub room_id: Uuid,
    pub id: Uuid,
    pub kind: String,
    pub version: u64,
    pub key_version: u64,
    pub epoch: u64,
    pub author_id: String,
    pub device_id: String,
    pub nonce: String,
    pub ciphertext: String,
}

impl Content {
    fn context(&self) -> Result<Vec<u8>> {
        crypto::signed_fields(
            b"kodosi-room-content-aead-v1",
            &[
                self.room_id.to_string().as_bytes(),
                self.id.to_string().as_bytes(),
                self.kind.as_bytes(),
                &self.version.to_be_bytes(),
                &self.key_version.to_be_bytes(),
                &self.epoch.to_be_bytes(),
                self.author_id.as_bytes(),
                self.device_id.as_bytes(),
            ],
        )
    }
    pub(crate) fn encrypt(&mut self, key: &Secret, plain: &[u8]) -> Result<()> {
        let encrypted = seal(key.as_ref(), &self.context()?, plain)?;
        self.nonce = encrypted.nonce;
        self.ciphertext = encrypted.ciphertext;
        Ok(())
    }
    pub(crate) fn decrypt(&self, key: &Secret) -> Result<Zeroizing<Vec<u8>>> {
        open(
            key.as_ref(),
            &self.context()?,
            &Sealed {
                nonce: self.nonce.clone(),
                ciphertext: self.ciphertext.clone(),
            },
        )
    }
}

pub(crate) fn secret() -> Result<Secret> {
    let mut key = Zeroizing::new([0; 32]);
    rand::fill(key.as_mut()).map_err(|_| invalid("Cannot create room keys."))?;
    Ok(key)
}

pub(crate) fn sign<T: Serialize>(
    domain: &[u8],
    keys: &DeviceKeys,
    value: &T,
) -> Result<(String, String)> {
    let body = serde_json::to_vec(value)?;
    let mut signed = domain.to_vec();
    signed.extend_from_slice(&body);
    Ok((
        BASE64.encode(&body),
        BASE64.encode(crypto::sign_control_message(keys.signing_pkcs8(), &signed)?),
    ))
}

pub(crate) fn verify_signature(
    domain: &[u8],
    body: &[u8],
    signature: &str,
    public: &[u8],
) -> Result<()> {
    let mut signed = domain.to_vec();
    signed.extend_from_slice(body);
    let signature = decode(signature, 3309)?;
    ML_DSA_65
        .verify_sig(public, &signed, &signature)
        .map_err(|_| invalid("The room signature is invalid."))
}

pub(crate) fn verify_state(
    signed: &SignedState,
    room: Uuid,
    owner: &str,
    owner_root: &Root,
    previous: Option<&VerifiedState>,
) -> Result<VerifiedState> {
    let bytes = decode(&signed.body, 3 * 1024 * 1024)?;
    let state: State = serde_json::from_slice(&bytes)?;
    if state.room_id != room
        || state.owner_user_id != owner
        || state.version != signed.version
        || state.version != previous.map_or(1, |p| p.state.version + 1)
        || state.members.is_empty()
        || state.members.len() > 129
        || !state.members.contains_key(owner)
        || state.created_at_ms > crate::identity::now_ms().saturating_add(300_000)
        || state.recipients.is_empty()
        || state.recipients.len() > 1024
    {
        return Err(invalid("The room key history is invalid."));
    }
    let mut identities = BTreeMap::new();
    for (user, bundle) in &state.members {
        if user != &bundle.user_id {
            return Err(invalid("A room identity belongs to another user."));
        }
        let anchor = if user == owner {
            *owner_root
        } else {
            bundle.root()?
        };
        let older = previous.and_then(|p| {
            p.state
                .members
                .get(user)
                .map(|bundle| (bundle, p.state.created_at_ms))
        });
        if older.is_some_and(|(bundle, _)| bundle.root().is_ok_and(|prior| prior != anchor)) {
            return Err(invalid("A room member's identity changed."));
        }
        identities.insert(
            user.clone(),
            Pins::historical(bundle, &anchor, state.created_at_ms, older)?,
        );
    }
    if let Some(previous) = previous {
        if state.previous_hash != previous.hash
            || state.epoch < previous.state.epoch
            || state.epoch > previous.state.epoch + 1
            || !previous.state.members.contains_key(&state.author_id)
        {
            return Err(invalid("The room update does not follow its history."));
        }
        let prior = previous.state.members.keys().collect::<BTreeSet<_>>();
        let current = state.members.keys().collect::<BTreeSet<_>>();
        let removed = prior.difference(&current).copied().collect::<Vec<_>>();
        if state.author_id != owner
            && (current.difference(&prior).next().is_some()
                || removed.iter().any(|user| *user != &state.author_id))
        {
            return Err(invalid("The room membership update has no valid author."));
        }
        if !removed.is_empty() && state.epoch != previous.state.epoch + 1 {
            return Err(invalid("Room keys did not change with membership."));
        }
        if state.epoch == previous.state.epoch && state.previous_key != previous.state.previous_key
        {
            return Err(invalid("Room history keys changed within an epoch."));
        }
    } else if state.author_id != owner
        || state.epoch != 1
        || !state.previous_hash.is_empty()
        || state.previous_key.is_some()
    {
        return Err(invalid("The room has no valid starting keys."));
    }
    let signer_bundle = state
        .members
        .get(&state.author_id)
        .or_else(|| previous.and_then(|p| p.state.members.get(&state.author_id)))
        .ok_or_else(|| invalid("The room update has no member identity."))?;
    let signer_root = previous
        .and_then(|p| p.state.members.get(&state.author_id))
        .map_or_else(|| signer_bundle.root(), IdentityBundle::root)?;
    let identity = Pins::historical(signer_bundle, &signer_root, state.created_at_ms, None)?;
    let public = &identity
        .devices
        .get(&state.device_id)
        .ok_or_else(|| invalid("The room update has no approved signing device."))?
        .sig_public_key;
    verify_signature(STATE_DOMAIN, &bytes, &signed.signature, public)?;
    for recipient in &state.recipients {
        if !identities
            .get(&recipient.user_id)
            .is_some_and(|identity| identity.devices.contains_key(&recipient.device_id))
        {
            return Err(invalid("Room keys include a device outside the room."));
        }
    }
    Ok(VerifiedState {
        hash: digest(&bytes),
        state,
        identities,
    })
}

fn wrap_context(room: Uuid, version: u64, epoch: u64, user: &str, device: &str) -> Result<Vec<u8>> {
    crypto::signed_fields(
        b"kodosi-room-key-wrap-v1",
        &[
            room.to_string().as_bytes(),
            &version.to_be_bytes(),
            &epoch.to_be_bytes(),
            user.as_bytes(),
            device.as_bytes(),
        ],
    )
}

fn wrapping_key(shared: &[u8], context: &[u8]) -> Result<Secret> {
    let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, b"kodosi-room-kem-v1");
    let prk = salt.extract(shared);
    let info = [context];
    let expanded = prk
        .expand(&info, &aead::AES_256_GCM)
        .map_err(|_| invalid("Cannot derive the room wrapping key."))?;
    let mut key = Zeroizing::new([0; 32]);
    expanded
        .fill(key.as_mut())
        .map_err(|_| invalid("Cannot derive the room wrapping key."))?;
    Ok(key)
}

pub(crate) fn wrap(
    state: &State,
    user: &str,
    device: &str,
    public: &[u8],
    key: &Secret,
) -> Result<Recipient> {
    let encapsulation = kem::EncapsulationKey::new(&kem::ML_KEM_768, public)
        .map_err(|_| invalid("Invalid room recipient key."))?;
    let (ciphertext, shared) = encapsulation
        .encapsulate()
        .map_err(|_| invalid("Cannot encrypt room keys."))?;
    let context = wrap_context(state.room_id, state.version, state.epoch, user, device)?;
    let derived = wrapping_key(shared.as_ref(), &context)?;
    Ok(Recipient {
        user_id: user.to_owned(),
        device_id: device.to_owned(),
        kem_ciphertext: BASE64.encode(ciphertext.as_ref()),
        sealed: seal(derived.as_ref(), &context, key.as_ref())?,
    })
}

pub(crate) fn unwrap(state: &State, user: &str, keys: &DeviceKeys) -> Result<Secret> {
    let recipient = state
        .recipients
        .iter()
        .find(|recipient| recipient.user_id == user && recipient.device_id == keys.device_id)
        .ok_or_else(|| invalid("Room keys are still arriving on this device."))?;
    let ciphertext = decode(&recipient.kem_ciphertext, 1088)?;
    let shared = keys
        .room_key()?
        .decapsulate(kem::Ciphertext::from(ciphertext.as_slice()))
        .map_err(|_| invalid("Cannot open room keys."))?;
    let context = wrap_context(
        state.room_id,
        state.version,
        state.epoch,
        user,
        &keys.device_id,
    )?;
    let derived = wrapping_key(shared.as_ref(), &context)?;
    key_bytes(&open(derived.as_ref(), &context, &recipient.sealed)?)
}

fn history_context(room: Uuid, epoch: u64) -> Result<Vec<u8>> {
    crypto::signed_fields(
        b"kodosi-room-history-v1",
        &[room.to_string().as_bytes(), &epoch.to_be_bytes()],
    )
}

pub(crate) fn seal_previous(
    room: Uuid,
    epoch: u64,
    current: &Secret,
    previous: &Secret,
) -> Result<Sealed> {
    seal(
        current.as_ref(),
        &history_context(room, epoch)?,
        previous.as_ref(),
    )
}

pub(crate) fn open_previous(state: &State, current: &Secret) -> Result<Secret> {
    key_bytes(&open(
        current.as_ref(),
        &history_context(state.room_id, state.epoch)?,
        state
            .previous_key
            .as_ref()
            .ok_or_else(|| invalid("Room history keys are incomplete."))?,
    )?)
}

fn key_bytes(bytes: &[u8]) -> Result<Secret> {
    Ok(Zeroizing::new(
        bytes
            .try_into()
            .map_err(|_| invalid("Invalid room key size."))?,
    ))
}

fn seal(key: &[u8], context: &[u8], bytes: &[u8]) -> Result<Sealed> {
    let key = aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_256_GCM, key)
            .map_err(|_| invalid("Invalid room encryption key."))?,
    );
    let mut nonce = [0; 12];
    rand::fill(&mut nonce).map_err(|_| invalid("Cannot create an encryption nonce."))?;
    let mut ciphertext = bytes.to_vec();
    key.seal_in_place_append_tag(
        aead::Nonce::assume_unique_for_key(nonce),
        aead::Aad::from(context),
        &mut ciphertext,
    )
    .map_err(|_| invalid("Cannot encrypt room content."))?;
    Ok(Sealed {
        nonce: BASE64.encode(nonce),
        ciphertext: BASE64.encode(ciphertext),
    })
}

fn open(key: &[u8], context: &[u8], sealed: &Sealed) -> Result<Zeroizing<Vec<u8>>> {
    let key = aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_256_GCM, key)
            .map_err(|_| invalid("Invalid room encryption key."))?,
    );
    let nonce: [u8; 12] = decode(&sealed.nonce, 12)?
        .try_into()
        .map_err(|_| invalid("Invalid room nonce."))?;
    let mut bytes = Zeroizing::new(decode(&sealed.ciphertext, 256 * 1024)?);
    let plain = key
        .open_in_place(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(context),
            bytes.as_mut(),
        )
        .map_err(|_| invalid("Room content could not be authenticated."))?;
    let len = plain.len();
    bytes.truncate(len);
    Ok(bytes)
}

pub(crate) fn decode(value: &str, maximum: usize) -> Result<Vec<u8>> {
    if value.len() > maximum.div_ceil(3) * 4 {
        return Err(invalid("Room data exceeds its limit."));
    }
    let bytes = BASE64
        .decode(value)
        .map_err(|_| invalid("Invalid room data encoding."))?;
    if bytes.len() > maximum {
        return Err(invalid("Room data exceeds its limit."));
    }
    Ok(bytes)
}

fn digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    Sha256::digest(bytes)
        .iter()
        .flat_map(|byte| {
            [
                char::from(HEX[usize::from(byte >> 4)]),
                char::from(HEX[usize::from(byte & 15)]),
            ]
        })
        .collect()
}
