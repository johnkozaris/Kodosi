use std::num::NonZeroU32;

use aws_lc_rs::{hkdf, hmac, pbkdf2, rand};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use zeroize::Zeroizing;

use super::{pins::Root, wire_codec::LpWriter};
use crate::network::{Error, Result};

const DOMAIN: &[u8] = b"kodosi-device-link-v2";
const SCANNED_DOMAIN: &[u8] = b"kodosi-device-link-scanned-v1";
const ROUNDS: NonZeroU32 = NonZeroU32::new(1 << 20).unwrap();
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const SYMBOLS: usize = 12;
const SCANNED_LEN: usize = 32;
const LINK: &str = "kodosi://device/";
pub(crate) const NONCE_LEN: usize = 16;
pub(crate) const PROOF_LEN: usize = 32;

pub(crate) struct LinkIdentity<'a> {
    pub(crate) user_id: &'a str,
    pub(crate) device_id: &'a str,
    pub(crate) label: &'a str,
    pub(crate) signing_public_key: &'a [u8],
}

pub(crate) fn new_code() -> Result<String> {
    let mut random = [0; 8];
    rand::fill(&mut random).map_err(|_| Error::Closed)?;
    let bits = u64::from_be_bytes(random);
    let symbols = (0..SYMBOLS)
        .map(|index| char::from(ALPHABET[((bits >> (59 - 5 * index)) & 31) as usize]))
        .collect::<String>();
    Ok(grouped(&symbols))
}

pub(crate) fn typed_code(text: &str) -> Option<String> {
    let symbols = text
        .chars()
        .filter(|symbol| !symbol.is_whitespace() && *symbol != '-')
        .map(|symbol| match symbol.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        })
        .collect::<String>();
    (symbols.len() == SYMBOLS && symbols.bytes().all(|symbol| ALPHABET.contains(&symbol)))
        .then(|| grouped(&symbols))
}

fn grouped(symbols: &str) -> String {
    format!("{}-{}-{}", &symbols[..4], &symbols[4..8], &symbols[8..])
}

pub(crate) enum LinkSecret {
    Typed(String),
    Scanned(Zeroizing<[u8; SCANNED_LEN]>),
}

impl LinkSecret {
    pub(crate) fn scanned() -> Result<Self> {
        let mut secret = Zeroizing::new([0; SCANNED_LEN]);
        rand::fill(secret.as_mut()).map_err(|_| Error::Closed)?;
        Ok(Self::Scanned(secret))
    }

    pub(crate) fn link(&self, user_id: &str) -> String {
        match self {
            Self::Typed(code) => format!("{LINK}approve/{user_id}#{code}"),
            Self::Scanned(secret) => format!(
                "{LINK}join/{user_id}#{}",
                URL_SAFE_NO_PAD.encode(secret.as_ref())
            ),
        }
    }
}

pub(crate) struct ScannedLink {
    pub(crate) user_id: String,
    pub(crate) secret: LinkSecret,
}

pub(crate) fn scanned_link(text: &str) -> Option<ScannedLink> {
    let (way, rest) = text.trim().strip_prefix(LINK)?.split_once('/')?;
    let (user_id, secret) = rest.split_once('#')?;
    let secret = match way {
        "approve" => LinkSecret::Typed(typed_code(secret)?),
        "join" => {
            let bytes = Zeroizing::new(URL_SAFE_NO_PAD.decode(secret).ok()?);
            LinkSecret::Scanned(Zeroizing::new(bytes.as_slice().try_into().ok()?))
        }
        _ => return None,
    };
    (!user_id.is_empty()).then(|| ScannedLink {
        user_id: user_id.to_owned(),
        secret,
    })
}

pub(crate) struct LinkKey(hmac::Key);

impl LinkKey {
    pub(crate) fn derive(secret: &LinkSecret, nonce: &[u8]) -> Result<Self> {
        match secret {
            LinkSecret::Typed(code) => {
                let mut salt = DOMAIN.to_vec();
                salt.extend_from_slice(nonce);
                let mut key = Zeroizing::new([0; 32]);
                pbkdf2::derive(
                    pbkdf2::PBKDF2_HMAC_SHA512,
                    ROUNDS,
                    &salt,
                    code.as_bytes(),
                    key.as_mut(),
                );
                Ok(Self(hmac::Key::new(hmac::HMAC_SHA256, key.as_ref())))
            }
            LinkSecret::Scanned(secret) => hkdf::Salt::new(hkdf::HKDF_SHA256, nonce)
                .extract(secret.as_ref())
                .expand(&[SCANNED_DOMAIN], hmac::HMAC_SHA256)
                .map(|key| Self(hmac::Key::from(key)))
                .map_err(|_| Error::Closed),
        }
    }

    pub(crate) fn request_proof(&self, identity: &LinkIdentity<'_>) -> Result<[u8; PROOF_LEN]> {
        Ok(self.proof(&request(identity)?))
    }

    pub(crate) fn proves_request(&self, identity: &LinkIdentity<'_>, proof: &[u8]) -> Result<bool> {
        Ok(hmac::verify(&self.0, &request(identity)?, proof).is_ok())
    }

    pub(crate) fn approval_proof(
        &self,
        user_id: &str,
        device_id: &str,
        root: &Root,
    ) -> Result<[u8; PROOF_LEN]> {
        Ok(self.proof(&approval(user_id, device_id, root)?))
    }

    pub(crate) fn proves_approval(
        &self,
        user_id: &str,
        device_id: &str,
        root: &Root,
        proof: &[u8],
    ) -> Result<bool> {
        Ok(hmac::verify(&self.0, &approval(user_id, device_id, root)?, proof).is_ok())
    }

    fn proof(&self, message: &[u8]) -> [u8; PROOF_LEN] {
        let mut proof = [0; PROOF_LEN];
        proof.copy_from_slice(hmac::sign(&self.0, message).as_ref());
        proof
    }
}

fn request(identity: &LinkIdentity<'_>) -> Result<Vec<u8>> {
    let mut fields = LpWriter::new("device link request");
    fields.write_lp_str("request")?;
    fields.write_lp_str(identity.user_id)?;
    fields.write_lp_str(identity.device_id)?;
    fields.write_lp_str(identity.label)?;
    fields.write_lp_bytes(identity.signing_public_key)?;
    Ok(fields.finish())
}

fn approval(user_id: &str, device_id: &str, root: &Root) -> Result<Vec<u8>> {
    let mut fields = LpWriter::new("device link approval");
    fields.write_lp_str("approval")?;
    fields.write_lp_str(user_id)?;
    fields.write_lp_str(device_id)?;
    fields.write_lp_bytes(root)?;
    Ok(fields.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER: &str = "11111111-1111-1111-1111-111111111111";

    fn identity(signing: &[u8]) -> LinkIdentity<'_> {
        LinkIdentity {
            user_id: USER,
            device_id: "device",
            label: "Laptop",
            signing_public_key: signing,
        }
    }

    fn typed(code: &str) -> LinkSecret {
        LinkSecret::Typed(code.to_owned())
    }

    fn scanned(byte: u8) -> LinkSecret {
        LinkSecret::Scanned(Zeroizing::new([byte; SCANNED_LEN]))
    }

    #[test]
    fn a_proof_holds_only_for_the_same_code_nonce_and_keys() {
        let key = LinkKey::derive(&typed("TAFX-E5HG-TN8E"), &[2; NONCE_LEN]).unwrap();
        let proof = key.request_proof(&identity(&[1; 1952])).unwrap();
        assert_eq!(
            proof[..4],
            [0x4c, 0x3d, 0xc6, 0x6e],
            "the derivation changed"
        );
        assert!(key.proves_request(&identity(&[1; 1952]), &proof).unwrap());
        let mut signing = [1; 1952];
        signing[1951] = 0;
        assert!(!key.proves_request(&identity(&signing), &proof).unwrap());
        for (code, nonce) in [("TAFX-E5HG-TN8F", 2), ("TAFX-E5HG-TN8E", 3)] {
            let other = LinkKey::derive(&typed(code), &[nonce; NONCE_LEN]).unwrap();
            assert!(!other.proves_request(&identity(&[1; 1952]), &proof).unwrap());
        }
    }

    #[test]
    fn a_scanned_secret_proves_only_with_the_same_secret_and_nonce() {
        let key = LinkKey::derive(&scanned(7), &[2; NONCE_LEN]).unwrap();
        let proof = key.request_proof(&identity(&[1; 1952])).unwrap();
        assert_eq!(
            proof[..4],
            [0xef, 0x37, 0x5c, 0xd5],
            "the derivation changed"
        );
        assert!(key.proves_request(&identity(&[1; 1952]), &proof).unwrap());
        for (secret, nonce) in [(8, 2), (7, 3)] {
            let other = LinkKey::derive(&scanned(secret), &[nonce; NONCE_LEN]).unwrap();
            assert!(!other.proves_request(&identity(&[1; 1952]), &proof).unwrap());
        }
    }

    #[test]
    fn an_approval_proof_names_one_device_and_one_identity() {
        let key = LinkKey::derive(&typed("TAFX-E5HG-TN8E"), &[2; NONCE_LEN]).unwrap();
        let proof = key.approval_proof(USER, "device", &[5; 32]).unwrap();
        assert!(
            key.proves_approval(USER, "device", &[5; 32], &proof)
                .unwrap()
        );
        assert!(
            !key.proves_approval(USER, "device", &[6; 32], &proof)
                .unwrap()
        );
        assert!(
            !key.proves_approval(USER, "other", &[5; 32], &proof)
                .unwrap()
        );
        let request = key.request_proof(&identity(&[1; 1952])).unwrap();
        assert!(
            !key.proves_approval(USER, "device", &[5; 32], &request)
                .unwrap()
        );
    }

    #[test]
    fn a_new_code_has_twelve_symbols_and_a_typed_code_ignores_case_spaces_and_look_alike_letters() {
        let code = new_code().unwrap();
        assert_eq!(
            typed_code(&code.to_lowercase().replace('-', " ")),
            Some(code)
        );
        assert_eq!(
            typed_code(" tafx e5hg-tn8e ").as_deref(),
            Some("TAFX-E5HG-TN8E")
        );
        assert_eq!(
            typed_code("oIl0-0000-0000").as_deref(),
            Some("0110-0000-0000")
        );
        assert_eq!(typed_code("TAFX-E5HG-TN8"), None);
        assert_eq!(typed_code("TAFX-E5HG-TN8U"), None);
        assert_eq!(typed_code("TAFX-E5HG-TN8é"), None);
    }

    #[test]
    fn a_link_gives_back_its_account_and_its_secret_and_tells_the_two_ways_apart() {
        let shown = typed("TAFX-E5HG-TN8E").link(USER);
        assert_eq!(
            shown,
            format!("kodosi://device/approve/{USER}#TAFX-E5HG-TN8E")
        );
        let read = scanned_link(&format!(" {shown}\n")).unwrap();
        assert_eq!(read.user_id, USER);
        assert!(matches!(read.secret, LinkSecret::Typed(code) if code == "TAFX-E5HG-TN8E"));

        let offered = LinkSecret::scanned().unwrap();
        let read = scanned_link(&offered.link(USER)).unwrap();
        assert_eq!(read.user_id, USER);
        let (LinkSecret::Scanned(made), LinkSecret::Scanned(got)) = (&offered, &read.secret) else {
            panic!("a join link must give a scanned secret");
        };
        assert_eq!(made.as_ref(), got.as_ref());
    }

    #[test]
    fn a_text_that_is_not_a_whole_device_link_is_refused() {
        let join = scanned(7).link(USER);
        for bad in [
            "TAFX-E5HG-TN8E".to_owned(),
            format!("kodosi:alice:{}", "ab".repeat(32)),
            format!("kodosi://device/approve/{USER}#TAFX-E5HG-TN8"),
            "kodosi://device/approve/#TAFX-E5HG-TN8E".to_owned(),
            format!("kodosi://device/other/{USER}#TAFX-E5HG-TN8E"),
            join[..join.len() - 1].to_owned(),
            format!("{join}A"),
            join.replace('#', "/"),
        ] {
            assert!(scanned_link(&bad).is_none(), "{bad}");
        }
    }
}
