use aws_lc_rs::{
    aead, rand,
    signature::{ML_DSA_65_SIGNING, PqdsaKeyPair},
};
use uuid::Uuid;
use zeroize::Zeroizing;

use super::{Result, invalid};

const SIGNATURE_BYTES: usize = 3309;

pub fn signed_fields(domain: &[u8], fields: &[&[u8]]) -> Result<Vec<u8>> {
    let mut result = domain.to_vec();
    for field in fields {
        append_field(&mut result, field)?;
    }
    Ok(result)
}

fn append_field(out: &mut Vec<u8>, field: &[u8]) -> Result<()> {
    let len =
        u32::try_from(field.len()).map_err(|_| invalid("Signed field exceeds the wire limit."))?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(field);
    Ok(())
}

pub(crate) const NONCE_BYTES: usize = 12;

fn sealing_key(key: &[u8]) -> Option<aead::LessSafeKey> {
    Some(aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_256_GCM, key).ok()?,
    ))
}

pub(crate) fn seal(
    key: &[u8],
    context: &[u8],
    plain: &[u8],
) -> Option<([u8; NONCE_BYTES], Vec<u8>)> {
    let mut nonce = [0; NONCE_BYTES];
    rand::fill(&mut nonce).ok()?;
    let mut ciphertext = plain.to_vec();
    sealing_key(key)?
        .seal_in_place_append_tag(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(context),
            &mut ciphertext,
        )
        .ok()?;
    Some((nonce, ciphertext))
}

pub(crate) fn open(
    key: &[u8],
    context: &[u8],
    nonce: &[u8],
    ciphertext: Vec<u8>,
) -> Option<Zeroizing<Vec<u8>>> {
    let nonce: [u8; NONCE_BYTES] = nonce.try_into().ok()?;
    let mut bytes = Zeroizing::new(ciphertext);
    let len = sealing_key(key)?
        .open_in_place(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(context),
            bytes.as_mut(),
        )
        .ok()?
        .len();
    bytes.truncate(len);
    Some(bytes)
}

pub fn sign_control_message(pkcs8: &[u8], preimage: &[u8]) -> Result<Vec<u8>> {
    let key = PqdsaKeyPair::from_pkcs8(&ML_DSA_65_SIGNING, pkcs8)
        .map_err(|_| invalid("Stored device signing key is invalid."))?;
    sign(&key, preimage)
}

pub fn sign(key: &PqdsaKeyPair, preimage: &[u8]) -> Result<Vec<u8>> {
    let mut signature = vec![0; SIGNATURE_BYTES];
    let len = key
        .sign(preimage, &mut signature)
        .map_err(|_| invalid("Device signature failed."))?;
    signature.truncate(len);
    Ok(signature)
}

pub fn sign_pop_challenge(key: &PqdsaKeyPair, challenge: &[u8]) -> Result<Vec<u8>> {
    if challenge.len() != 32 {
        return Err(invalid("Device challenge must be 32 bytes."));
    }
    let mut bytes = b"kodosi-device-pop-v1".to_vec();
    bytes.extend_from_slice(challenge);
    sign(key, &bytes)
}

pub fn device_session_proof_preimage(
    user_id: &str,
    device_id: &str,
    challenge_id: &Uuid,
    challenge: &[u8],
) -> Result<Vec<u8>> {
    if user_id.is_empty() || device_id.is_empty() || challenge_id.is_nil() || challenge.len() != 32
    {
        return Err(invalid("Device session proof is incomplete."));
    }
    let id = challenge_id.to_string();
    let mut bytes = signed_fields(
        b"kodosi-device-session-v1",
        &[user_id.as_bytes(), device_id.as_bytes(), id.as_bytes()],
    )?;
    bytes.extend_from_slice(challenge);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_lc_rs::signature::{KeyPair as _, ML_DSA_65, VerificationAlgorithm as _};

    #[test]
    fn a_device_session_proof_holds_for_one_device_and_one_challenge() {
        let pair = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).unwrap();
        let id = Uuid::now_v7();
        let preimage = device_session_proof_preimage("user", "device", &id, &[2; 32]).unwrap();
        let signature = sign(&pair, &preimage).unwrap();
        let public = pair.public_key();
        ML_DSA_65
            .verify_sig(public.as_ref(), &preimage, &signature)
            .unwrap();
        for changed in [
            device_session_proof_preimage("user", "other", &id, &[2; 32]).unwrap(),
            device_session_proof_preimage("user", "device", &id, &[3; 32]).unwrap(),
            device_session_proof_preimage("user", "device", &Uuid::now_v7(), &[2; 32]).unwrap(),
        ] {
            assert!(
                ML_DSA_65
                    .verify_sig(public.as_ref(), &changed, &signature)
                    .is_err()
            );
        }
    }

    #[test]
    fn the_device_session_proof_vector_matches_the_current_manifest() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../protocol/crypto-domains.json");
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let vector = &manifest["deviceProofPreimageVectors"]["sessionV1"];
        let text = |key: &str| vector[key].as_str().unwrap().to_owned();
        let hex = |value: String| {
            value
                .as_bytes()
                .chunks_exact(2)
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect::<Vec<_>>()
        };
        let bytes = device_session_proof_preimage(
            &text("userId"),
            &text("deviceId"),
            &Uuid::parse_str(&text("challengeId")).unwrap(),
            &hex(text("challengeHex")),
        )
        .unwrap();
        assert_eq!(bytes, hex(text("preimageHex")));
    }
}
