use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use uuid::Uuid;

use crate::{BackendClientError, Result};

use kodosi_domain::domain_tags::{
    ARTIFACT_ENDORSEMENT_V1 as ENDORSEMENT_DOMAIN, AUTHENTICATED_ARTIFACT_V1 as ARTIFACT_DOMAIN,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactEndorsement {
    pub user_id: String,
    pub identity_incarnation_id: Uuid,
    pub artifact_digest: String,
    pub endorser_device_id: String,
    pub signature: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PutArtifactEndorsement<'a> {
    pub identity_incarnation_id: Uuid,
    pub artifact_digest: &'a str,
    pub endorser_device_id: &'a str,
    pub signature: &'a str,
}

pub fn artifact_digest(
    user_id: &str,
    identity_incarnation_id: &Uuid,
    signer_device_id: &str,
    signed_preimage: &[u8],
    signature: &[u8],
) -> Result<String> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut bytes = ARTIFACT_DOMAIN.to_vec();
    append_field(&mut bytes, user_id.as_bytes())?;
    bytes.extend_from_slice(identity_incarnation_id.as_bytes());
    append_field(&mut bytes, signer_device_id.as_bytes())?;
    append_field(&mut bytes, signed_preimage)?;
    append_field(&mut bytes, signature)?;
    let hash = Sha256::digest(&bytes);
    let mut encoded = String::with_capacity(64);
    for byte in hash {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 15)]));
    }
    Ok(encoded)
}

pub fn endorsement_preimage(
    user_id: &str,
    identity_incarnation_id: &Uuid,
    artifact_digest: &str,
    endorser_device_id: &str,
) -> Result<Vec<u8>> {
    if !Uuid::parse_str(user_id).is_ok_and(|id| !id.is_nil() && id.to_string() == user_id)
        || identity_incarnation_id.is_nil()
        || artifact_digest.len() != 64
        || !artifact_digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || endorser_device_id.is_empty()
        || endorser_device_id.len() > 256
        || endorser_device_id.chars().any(char::is_control)
    {
        return Err(BackendClientError::InvalidBackendData {
            field: "artifactEndorsement".to_owned(),
            reason: "invalid artifact endorsement identity or digest".to_owned(),
        });
    }
    let mut bytes = ENDORSEMENT_DOMAIN.to_vec();
    append_field(&mut bytes, user_id.as_bytes())?;
    bytes.extend_from_slice(identity_incarnation_id.as_bytes());
    append_field(&mut bytes, artifact_digest.as_bytes())?;
    append_field(&mut bytes, endorser_device_id.as_bytes())?;
    Ok(bytes)
}

fn append_field(bytes: &mut Vec<u8>, field: &[u8]) -> Result<()> {
    let length =
        u32::try_from(field.len()).map_err(|_| BackendClientError::InvalidBackendData {
            field: "artifactEndorsement".to_owned(),
            reason: "artifact field exceeds the wire limit".to_owned(),
        })?;
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(field);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_artifact_binds_identity_original_signer_body_and_signature() {
        let incarnation = Uuid::from_u128(1);
        let original =
            artifact_digest("user", &incarnation, "device", b"domain:body", b"signature").unwrap();
        for altered in [
            artifact_digest(
                "other",
                &incarnation,
                "device",
                b"domain:body",
                b"signature",
            ),
            artifact_digest(
                "user",
                &Uuid::from_u128(2),
                "device",
                b"domain:body",
                b"signature",
            ),
            artifact_digest("user", &incarnation, "other", b"domain:body", b"signature"),
            artifact_digest("user", &incarnation, "device", b"other:body", b"signature"),
            artifact_digest("user", &incarnation, "device", b"domain:body", b"other"),
        ] {
            assert_ne!(original, altered.unwrap());
        }
    }

    #[test]
    fn endorsement_uses_big_endian_uuid_and_length_prefixes() {
        let user = "11111111-1111-4111-8111-111111111111";
        let incarnation = Uuid::parse_str("01900000-0000-7000-8000-000000000001").unwrap();
        let digest = "a".repeat(64);
        let bytes = endorsement_preimage(user, &incarnation, &digest, "device-1").unwrap();
        let mut expected = ENDORSEMENT_DOMAIN.to_vec();
        expected.extend_from_slice(&36_u32.to_be_bytes());
        expected.extend_from_slice(user.as_bytes());
        expected.extend_from_slice(incarnation.as_bytes());
        expected.extend_from_slice(&64_u32.to_be_bytes());
        expected.extend_from_slice(digest.as_bytes());
        expected.extend_from_slice(&8_u32.to_be_bytes());
        expected.extend_from_slice(b"device-1");
        assert_eq!(bytes, expected);
        assert!(endorsement_preimage(user, &incarnation, &"A".repeat(64), "device-1").is_err());
        assert!(endorsement_preimage(user, &Uuid::nil(), &digest, "device-1").is_err());
    }
}
