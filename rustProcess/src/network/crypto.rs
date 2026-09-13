use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use aws_lc_rs::{
    kem::{DecapsulationKey, EncapsulationKey, ML_KEM_768},
    signature::{ML_DSA_65, ML_DSA_65_SIGNING, PqdsaKeyPair, VerificationAlgorithm},
};
use hkdf::Hkdf;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zeroize::Zeroizing;

use super::{Result, invalid};

pub type SessionKey = [u8; 32];
const SIGNATURE_BYTES: usize = 3309;
const KEM_CIPHERTEXT_BYTES: usize = 1088;

pub fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes)
        .map_err(|_| invalid("The operating system random generator is unavailable."))?;
    Ok(bytes)
}

pub fn generate_session_key() -> Result<SessionKey> {
    random_bytes()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut result = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(result, "{byte:02x}");
    }
    result
}

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

pub fn verify_control_message(public: &[u8], preimage: &[u8], signature: &[u8]) -> Result<()> {
    if signature.len() != SIGNATURE_BYTES {
        return Err(invalid("Device signature has the wrong length."));
    }
    ML_DSA_65
        .verify_sig(public, preimage, signature)
        .map_err(|_| invalid("Device signature did not verify."))
}

pub fn sign_pop_challenge(key: &PqdsaKeyPair, challenge: &[u8]) -> Result<Vec<u8>> {
    if challenge.len() != 32 {
        return Err(invalid("Device challenge must be 32 bytes."));
    }
    let mut bytes = b"kodosi-device-pop-v1".to_vec();
    bytes.extend_from_slice(challenge);
    sign(key, &bytes)
}

pub fn device_http_request_proof_preimage(
    user_id: &str,
    device_id: &str,
    challenge_id: &Uuid,
    method: &str,
    target: &str,
    body_sha256: &str,
    challenge: &[u8],
) -> Result<Vec<u8>> {
    if user_id.is_empty()
        || device_id.is_empty()
        || challenge_id.is_nil()
        || method.is_empty()
        || !target.starts_with('/')
        || challenge.len() != 32
        || body_sha256.len() != 64
        || !body_sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(invalid("Device request proof is incomplete."));
    }
    let id = challenge_id.to_string();
    let method = method.to_ascii_uppercase();
    let mut bytes = signed_fields(
        b"kodosi-device-http-request-proof-v1",
        &[
            user_id.as_bytes(),
            device_id.as_bytes(),
            id.as_bytes(),
            method.as_bytes(),
            target.as_bytes(),
            body_sha256.as_bytes(),
        ],
    )?;
    bytes.extend_from_slice(challenge);
    Ok(bytes)
}

pub fn device_connection_proof_preimage(
    user_id: &str,
    device_id: &str,
    connection_id: &str,
    purpose: &str,
    session_id: Option<&str>,
    incarnation_id: Option<&Uuid>,
    challenge: &[u8],
) -> Result<Vec<u8>> {
    if user_id.is_empty()
        || device_id.is_empty()
        || connection_id.is_empty()
        || purpose.is_empty()
        || challenge.len() != 32
    {
        return Err(invalid("Device connection proof is incomplete."));
    }
    let incarnation = incarnation_id.map(ToString::to_string).unwrap_or_default();
    let mut bytes = signed_fields(
        b"kodosi-device-connection-proof-v1",
        &[
            user_id.as_bytes(),
            device_id.as_bytes(),
            connection_id.as_bytes(),
            purpose.as_bytes(),
            session_id.unwrap_or_default().as_bytes(),
            incarnation.as_bytes(),
        ],
    )?;
    bytes.extend_from_slice(challenge);
    Ok(bytes)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayStream {
    Checkpoint,
    TerminalRaw,
    Control,
}

pub fn derive_stream_key(key: &SessionKey, stream: RelayStream) -> Result<Zeroizing<SessionKey>> {
    let info: &[u8] = match stream {
        RelayStream::Checkpoint => b"kodosi-terminal-checkpoint-v1",
        RelayStream::TerminalRaw => b"kodosi-terminal-raw-v1",
        RelayStream::Control => b"kodosi-control-v1",
    };
    let mut result = Zeroizing::new([0; 32]);
    Hkdf::<Sha256>::new(None, key)
        .expand(info, result.as_mut())
        .map_err(|_| invalid("Cannot derive terminal encryption key."))?;
    Ok(result)
}

pub fn encrypt_frame(
    key: &SessionKey,
    generation: u32,
    counter: u64,
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|_| invalid("Invalid terminal encryption key."))?;
    let nonce = frame_nonce(generation, counter);
    cipher
        .encrypt(
            &Nonce::from(nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| invalid("Terminal encryption failed."))
}

pub fn decrypt_frame(
    key: &SessionKey,
    generation: u32,
    counter: u64,
    aad: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|_| invalid("Invalid terminal encryption key."))?;
    let nonce = frame_nonce(generation, counter);
    cipher
        .decrypt(
            &Nonce::from(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| invalid("Terminal content could not be authenticated."))
}

fn frame_nonce(generation: u32, counter: u64) -> [u8; 12] {
    let mut nonce = [0; 12];
    nonce[..4].copy_from_slice(&generation.to_be_bytes());
    nonce[4..].copy_from_slice(&counter.to_be_bytes());
    nonce
}

pub fn encrypt_control_payload(
    key: &SessionKey,
    aad: &[u8],
    plaintext: &[u8],
) -> Result<([u8; 12], Vec<u8>)> {
    let key = derive_stream_key(key, RelayStream::Control)?;
    let cipher =
        Aes256Gcm::new_from_slice(key.as_ref()).map_err(|_| invalid("Invalid control key."))?;
    let nonce = random_bytes()?;
    let ciphertext = cipher
        .encrypt(
            &Nonce::from(nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| invalid("Terminal control encryption failed."))?;
    Ok((nonce, ciphertext))
}

pub fn decrypt_control_payload(
    key: &SessionKey,
    aad: &[u8],
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>> {
    if nonce.len() != 12 {
        return Err(invalid("Invalid control nonce."));
    }
    let key = derive_stream_key(key, RelayStream::Control)?;
    let cipher =
        Aes256Gcm::new_from_slice(key.as_ref()).map_err(|_| invalid("Invalid control key."))?;
    let nonce = Nonce::try_from(nonce).map_err(|_| invalid("Invalid control nonce."))?;
    cipher
        .decrypt(
            &nonce,
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| invalid("Terminal control could not be authenticated."))
}

pub fn wrap_session_key(
    public: &[u8],
    key: &SessionKey,
    session_id: &str,
    recipient_device: &str,
    generation: u32,
) -> Result<Vec<u8>> {
    let public = EncapsulationKey::new(&ML_KEM_768, public)
        .map_err(|_| invalid("Recipient encryption key is invalid."))?;
    let (kem_ciphertext, secret) = public
        .encapsulate()
        .map_err(|_| invalid("Session key encapsulation failed."))?;
    let wrapping_key = derive_wrap_key(secret.as_ref(), session_id)?;
    let nonce = random_bytes::<12>()?;
    let aad = wrap_aad(session_id, recipient_device, generation)?;
    let cipher = Aes256Gcm::new_from_slice(wrapping_key.as_ref())
        .map_err(|_| invalid("Invalid wrapping key."))?;
    let ciphertext = cipher
        .encrypt(
            &Nonce::from(nonce),
            Payload {
                msg: key,
                aad: &aad,
            },
        )
        .map_err(|_| invalid("Session key encryption failed."))?;
    let mut result = vec![1];
    result.extend_from_slice(kem_ciphertext.as_ref());
    result.extend_from_slice(&nonce);
    result.extend_from_slice(&ciphertext);
    Ok(result)
}

pub fn unwrap_session_key(
    secret: &DecapsulationKey,
    blob: &[u8],
    session_id: &str,
    recipient_device: &str,
    generation: u32,
) -> Result<Zeroizing<SessionKey>> {
    if blob.len() != 1 + KEM_CIPHERTEXT_BYTES + 12 + 32 + 16 || blob[0] != 1 {
        return Err(invalid("Wrapped session key has the wrong format."));
    }
    let shared = secret
        .decapsulate(aws_lc_rs::kem::Ciphertext::from(
            &blob[1..=KEM_CIPHERTEXT_BYTES],
        ))
        .map_err(|_| invalid("Session key decapsulation failed."))?;
    let wrapping_key = derive_wrap_key(shared.as_ref(), session_id)?;
    let aes = &blob[1 + KEM_CIPHERTEXT_BYTES..];
    let nonce = Nonce::try_from(&aes[..12]).map_err(|_| invalid("Invalid wrapped-key nonce."))?;
    let cipher = Aes256Gcm::new_from_slice(wrapping_key.as_ref())
        .map_err(|_| invalid("Invalid wrapping key."))?;
    let aad = wrap_aad(session_id, recipient_device, generation)?;
    let plaintext = Zeroizing::new(
        cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: &aes[12..],
                    aad: &aad,
                },
            )
            .map_err(|_| invalid("Wrapped session key could not be authenticated."))?,
    );
    let mut key = Zeroizing::new([0; 32]);
    key.copy_from_slice(&plaintext);
    Ok(key)
}

fn wrap_aad(session_id: &str, recipient_device: &str, generation: u32) -> Result<Vec<u8>> {
    let mut aad = signed_fields(b"", &[session_id.as_bytes(), recipient_device.as_bytes()])?;
    aad.extend_from_slice(&generation.to_be_bytes());
    Ok(aad)
}

fn derive_wrap_key(secret: &[u8], session_id: &str) -> Result<Zeroizing<SessionKey>> {
    let mut key = Zeroizing::new([0; 32]);
    Hkdf::<Sha256>::new(Some(session_id.as_bytes()), secret)
        .expand(b"kodosi-session-key-wrap", key.as_mut())
        .map_err(|_| invalid("Cannot derive wrapping key."))?;
    Ok(key)
}

pub fn key_blob_digest(
    session_id: &str,
    incarnation: &Uuid,
    recipient: &str,
    blob: &[u8],
    generation: u32,
    issued_ms: u64,
) -> Result<Vec<u8>> {
    let incarnation = incarnation.to_string();
    let bytes = signed_fields(
        b"kodosi-session-key-blob-v2",
        &[
            &issued_ms.to_be_bytes(),
            &generation.to_be_bytes(),
            session_id.as_bytes(),
            incarnation.as_bytes(),
            recipient.as_bytes(),
            blob,
        ],
    )?;
    Ok(Sha256::digest(bytes).to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_lc_rs::signature::KeyPair as _;

    #[test]
    fn encrypted_streams_are_independent_and_tamper_evident() {
        let key = [7; 32];
        let raw = derive_stream_key(&key, RelayStream::TerminalRaw).unwrap();
        let checkpoint = derive_stream_key(&key, RelayStream::Checkpoint).unwrap();
        let encrypted = encrypt_frame(&raw, 1, 0, b"session", b"output").unwrap();
        assert_eq!(
            decrypt_frame(&raw, 1, 0, b"session", &encrypted).unwrap(),
            b"output"
        );
        assert!(decrypt_frame(&checkpoint, 1, 0, b"session", &encrypted).is_err());
        assert!(decrypt_frame(&raw, 1, 0, b"other", &encrypted).is_err());
        assert!(decrypt_frame(&raw, 2, 0, b"session", &encrypted).is_err());
    }

    #[test]
    fn device_proof_binds_request_not_just_account() {
        let pair = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).unwrap();
        let challenge = [2; 32];
        let id = Uuid::now_v7();
        let preimage = device_http_request_proof_preimage(
            "user",
            "device",
            &id,
            "POST",
            "/api/sessions",
            &sha256_hex(b"a"),
            &challenge,
        )
        .unwrap();
        let signature = sign(&pair, &preimage).unwrap();
        verify_control_message(pair.public_key().as_ref(), &preimage, &signature).unwrap();
        let changed = device_http_request_proof_preimage(
            "user",
            "device",
            &id,
            "DELETE",
            "/api/sessions",
            &sha256_hex(b"a"),
            &challenge,
        )
        .unwrap();
        assert!(verify_control_message(pair.public_key().as_ref(), &changed, &signature).is_err());
    }

    #[test]
    fn device_proof_vectors_match_current_manifest() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../protocol/crypto-domain-tags.json");
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let vectors = &manifest["deviceProofPreimageVectors"];
        let connection = &vectors["connectionV1"];
        let text = |value: &serde_json::Value, key: &str| value[key].as_str().unwrap().to_owned();
        let hex = |value: String| {
            value
                .as_bytes()
                .chunks_exact(2)
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect::<Vec<_>>()
        };
        let bytes = device_connection_proof_preimage(
            &text(connection, "userId"),
            &text(connection, "deviceId"),
            &text(connection, "connectionId"),
            &text(connection, "purpose"),
            Some(&text(connection, "sessionId")),
            Some(&Uuid::parse_str(&text(connection, "incarnationId")).unwrap()),
            &hex(text(connection, "challengeHex")),
        )
        .unwrap();
        assert_eq!(bytes, hex(text(connection, "preimageHex")));
        let http = &vectors["httpV1"];
        let bytes = device_http_request_proof_preimage(
            &text(http, "userId"),
            &text(http, "deviceId"),
            &Uuid::parse_str(&text(http, "challengeId")).unwrap(),
            &text(http, "method"),
            &text(http, "pathAndQuery"),
            &text(http, "bodySha256"),
            &hex(text(http, "challengeHex")),
        )
        .unwrap();
        assert_eq!(bytes, hex(text(http, "preimageHex")));
    }

    #[test]
    fn key_envelope_cannot_move_to_another_recipient() {
        let secret = DecapsulationKey::generate(&ML_KEM_768).unwrap();
        let public = secret.encapsulation_key().unwrap();
        let blob = wrap_session_key(
            public.key_bytes().unwrap().as_ref(),
            &[3; 32],
            "session",
            "device",
            1,
        )
        .unwrap();
        assert_eq!(
            *unwrap_session_key(&secret, &blob, "session", "device", 1).unwrap(),
            [3; 32]
        );
        assert!(unwrap_session_key(&secret, &blob, "session", "other", 1).is_err());
    }
}
