use aws_lc_rs::signature::{ML_DSA_65_SIGNING, PqdsaKeyPair};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{Result, invalid};

const SIGNATURE_BYTES: usize = 3309;

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

#[cfg(test)]
mod tests {
    use super::*;
    use aws_lc_rs::signature::{KeyPair as _, ML_DSA_65, VerificationAlgorithm as _};

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
        let public = pair.public_key();
        ML_DSA_65
            .verify_sig(public.as_ref(), &preimage, &signature)
            .unwrap();
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
        assert!(
            ML_DSA_65
                .verify_sig(public.as_ref(), &changed, &signature)
                .is_err()
        );
    }

    #[test]
    fn device_proof_vectors_match_current_manifest() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../protocol/crypto-domains.json");
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
}
