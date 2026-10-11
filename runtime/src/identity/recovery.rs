use aws_lc_rs::{
    aead, hkdf, rand,
    signature::{KeyPair as _, ML_DSA_65_SIGNING, PqdsaKeyPair},
};
use uuid::Uuid;
use zeroize::Zeroizing;

use super::{keys::RoomKeyPair, pins::Root};
use crate::network::{Error, Result, crypto, invalid};

const DOMAIN: &[u8] = b"kodosi-recovery-v1";
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const SYMBOLS: usize = 32;
const SECRET_BYTES: usize = SYMBOLS * 5 / 8;
pub(crate) const LABEL: &str = "Recovery key";

pub(crate) fn new_text() -> Result<Zeroizing<String>> {
    let mut secret = Zeroizing::new([0; SECRET_BYTES]);
    rand::fill(secret.as_mut()).map_err(|_| Error::Closed)?;
    Ok(text_of(&secret))
}

fn text_of(secret: &[u8; SECRET_BYTES]) -> Zeroizing<String> {
    let mut text = Zeroizing::new(String::with_capacity(SYMBOLS + SYMBOLS / 4));
    for index in 0..SYMBOLS {
        if index > 0 && index % 4 == 0 {
            text.push('-');
        }
        let bit = index * 5;
        let pair = u16::from_be_bytes([
            secret[bit / 8],
            secret.get(bit / 8 + 1).copied().unwrap_or(0),
        ]);
        text.push(char::from(
            ALPHABET[usize::from((pair >> (11 - bit % 8)) & 31)],
        ));
    }
    text
}

fn secret_of(text: &str) -> Option<Zeroizing<[u8; SECRET_BYTES]>> {
    let mut secret = Zeroizing::new([0; SECRET_BYTES]);
    let mut count = 0;
    for symbol in text.chars().filter(|c| !c.is_whitespace() && *c != '-') {
        let symbol = match symbol.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        };
        let value = ALPHABET
            .iter()
            .position(|known| char::from(*known) == symbol)?;
        if count == SYMBOLS {
            return None;
        }
        let bit = count * 5;
        let shifted = u16::try_from(value).ok()? << (11 - bit % 8);
        let [high, low] = shifted.to_be_bytes();
        secret[bit / 8] |= high;
        if let Some(next) = secret.get_mut(bit / 8 + 1) {
            *next |= low;
        }
        count += 1;
    }
    (count == SYMBOLS).then_some(secret)
}

pub(crate) fn is_recovery_device(device_id: &str) -> bool {
    Uuid::parse_str(device_id).is_ok_and(|id| id.get_version_num() == 8)
}

pub(crate) struct RecoveryKey {
    pub(crate) device_id: String,
    signing: PqdsaKeyPair,
    room_key: Zeroizing<[u8; 32]>,
    context: Vec<u8>,
}

impl RecoveryKey {
    pub(crate) fn derive(text: &str, user_id: &str, root: &Root) -> Result<Self> {
        let secret = secret_of(text)
            .ok_or_else(|| invalid("Type the 32 characters of your recovery key."))?;
        let context = crypto::signed_fields(DOMAIN, &[user_id.as_bytes(), root])?;
        let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, &context).extract(secret.as_ref());
        let part = |label: &[u8]| {
            let mut bytes = Zeroizing::new([0; 32]);
            prk.expand(&[label], &aead::AES_256_GCM)
                .and_then(|expanded| expanded.fill(bytes.as_mut()))
                .map_err(|_| invalid("Cannot read the recovery key."))?;
            Ok::<_, Error>(bytes)
        };
        let signing = PqdsaKeyPair::from_seed(&ML_DSA_65_SIGNING, part(b"signing")?.as_ref())
            .map_err(|_| invalid("Cannot read the recovery key."))?;
        let mut device = [0; 16];
        device.copy_from_slice(&part(b"device")?[..16]);
        Ok(Self {
            device_id: uuid::Builder::from_custom_bytes(device)
                .into_uuid()
                .to_string(),
            signing,
            room_key: part(b"room-key")?,
            context,
        })
    }

    pub(crate) fn signing_key(&self) -> &PqdsaKeyPair {
        &self.signing
    }

    pub(crate) fn signing_public(&self) -> Vec<u8> {
        self.signing.public_key().as_ref().to_vec()
    }

    pub(crate) fn seal(&self, room: &RoomKeyPair) -> Result<Vec<u8>> {
        let (nonce, ciphertext) =
            crypto::seal(self.room_key.as_ref(), &self.context, &room.to_bytes())
                .ok_or_else(|| invalid("Cannot protect the recovery key."))?;
        Ok([nonce.as_slice(), &ciphertext].concat())
    }

    pub(crate) fn open(&self, sealed: &[u8]) -> Result<RoomKeyPair> {
        let (nonce, ciphertext) = sealed
            .split_at_checked(crypto::NONCE_BYTES)
            .ok_or_else(|| invalid("The saved recovery data is not valid."))?;
        let bytes = crypto::open(
            self.room_key.as_ref(),
            &self.context,
            nonce,
            ciphertext.to_vec(),
        )
        .ok_or_else(|| Error::Trust("This is not the recovery key of this account.".into()))?;
        RoomKeyPair::from_bytes(&bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER: &str = "11111111-1111-1111-1111-111111111111";

    #[test]
    fn a_typed_key_gives_the_same_keys_for_one_account_and_identity_only() {
        let text = new_text().unwrap();
        assert_eq!(text.len(), 39);
        let typed = text.to_lowercase().replace('-', " ");
        let key = RecoveryKey::derive(&text, USER, &[1; 32]).unwrap();
        let again = RecoveryKey::derive(&typed, USER, &[1; 32]).unwrap();
        assert_eq!(key.device_id, again.device_id);
        assert_eq!(key.signing_public(), again.signing_public());
        assert!(is_recovery_device(&key.device_id));
        assert!(!is_recovery_device(&Uuid::now_v7().to_string()));
        for other in [
            RecoveryKey::derive(&text, USER, &[2; 32]).unwrap(),
            RecoveryKey::derive(&text, "22222222-2222-2222-2222-222222222222", &[1; 32]).unwrap(),
            RecoveryKey::derive(&new_text().unwrap(), USER, &[1; 32]).unwrap(),
        ] {
            assert_ne!(other.device_id, key.device_id);
            assert_ne!(other.signing_public(), key.signing_public());
        }
    }

    #[test]
    fn the_key_text_keeps_all_its_bits_and_refuses_other_text() {
        let secret: [u8; SECRET_BYTES] =
            std::array::from_fn(|index| u8::try_from(index * 13 + 7).unwrap_or(0));
        let text = text_of(&secret);
        assert_eq!(secret_of(&text).unwrap().as_ref(), &secret);
        assert_eq!(
            text_of(&[0xff; SECRET_BYTES]).replace('-', ""),
            "Z".repeat(SYMBOLS)
        );
        assert_eq!(
            secret_of(&"o".repeat(SYMBOLS)).unwrap().as_ref(),
            &[0; SECRET_BYTES]
        );
        assert!(secret_of(&text[..text.len() - 1]).is_none());
        assert!(secret_of(&format!("{}0", text.as_str())).is_none());
        assert!(secret_of(&text.replace(|c: char| c.is_ascii_alphanumeric(), "U")).is_none());
    }

    #[test]
    fn the_sealed_room_key_opens_only_with_the_same_recovery_key() {
        let key = RecoveryKey::derive(&new_text().unwrap(), USER, &[1; 32]).unwrap();
        let room = RoomKeyPair::generate().unwrap();
        let sealed = key.seal(&room).unwrap();
        assert_eq!(key.open(&sealed).unwrap().public(), room.public());
        let other = RecoveryKey::derive(&new_text().unwrap(), USER, &[1; 32]).unwrap();
        assert!(matches!(other.open(&sealed), Err(Error::Trust(_))));
        let mut changed = sealed;
        changed[20] ^= 1;
        assert!(key.open(&changed).is_err());
    }
}
