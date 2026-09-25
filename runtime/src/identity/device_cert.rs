use aws_lc_rs::signature::{KeyPair, ML_DSA_65, PqdsaKeyPair, VerificationAlgorithm};

use super::wire_codec::{
    DEVICE_LABEL_MAX_UTF16_CODE_UNITS, LpReader, LpWriter, MAX_DEVICE_CERTIFICATE_BODY_LEN,
    ML_DSA_65_PUBLIC_KEY_LEN, ML_DSA_65_SIGNATURE_LEN, ML_KEM_768_PUBLIC_KEY_LEN, decode_expiry,
    encode_expiry, validate_canonical_device_id, validate_canonical_user_id, validate_timestamps,
};
use crate::network::{Error, Result};
const DEVICE_CERT_V2: &[u8] = b"kodosi-device-cert-v2";

const DEVICE_CERT_CONTEXT: &str = "device cert";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeviceCertificate {
    pub(crate) user_id: String,
    pub(crate) device_id: String,
    pub(crate) device_label: String,
    pub(crate) signer_device_id: String,
    pub(crate) kem_public_key: Vec<u8>,
    pub(crate) sig_public_key: Vec<u8>,
    pub(crate) issued_at_ms: u64,
    pub(crate) expires_at_ms: Option<u64>,
}

impl DeviceCertificate {
    pub(crate) fn is_self_signed(&self) -> bool {
        self.signer_device_id == self.device_id
    }

    pub(crate) fn serialize_body(&self) -> Result<Vec<u8>> {
        validate_certificate(self)?;
        let mut writer = LpWriter::with_capacity(
            DEVICE_CERT_CONTEXT,
            6 * 4
                + 2 * 8
                + self.user_id.len()
                + self.device_id.len()
                + self.device_label.len()
                + self.signer_device_id.len()
                + self.kem_public_key.len()
                + self.sig_public_key.len(),
        );
        writer.write_lp_str(&self.user_id)?;
        writer.write_lp_str(&self.device_id)?;
        writer.write_lp_str(&self.device_label)?;
        writer.write_lp_str(&self.signer_device_id)?;
        writer.write_lp_bytes(&self.kem_public_key)?;
        writer.write_lp_bytes(&self.sig_public_key)?;
        writer.write_u64_be(self.issued_at_ms);
        writer.write_u64_be(encode_expiry(self.expires_at_ms));
        let body = writer.finish();
        if body.len() > MAX_DEVICE_CERTIFICATE_BODY_LEN {
            return Err(Error::Invalid {
                reason: format!(
                    "{DEVICE_CERT_CONTEXT}: body length {} exceeds MAX_DEVICE_CERTIFICATE_BODY_LEN ({MAX_DEVICE_CERTIFICATE_BODY_LEN})",
                    body.len()
                ),
            });
        }
        Ok(body)
    }

    pub(crate) fn parse_body(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_DEVICE_CERTIFICATE_BODY_LEN {
            return Err(Error::Invalid {
                reason: format!(
                    "{DEVICE_CERT_CONTEXT}: body length {} exceeds MAX_DEVICE_CERTIFICATE_BODY_LEN ({MAX_DEVICE_CERTIFICATE_BODY_LEN})",
                    bytes.len()
                ),
            });
        }
        let mut cursor = LpReader::new(DEVICE_CERT_CONTEXT, bytes);
        let user_id = cursor.read_lp_str()?;
        let device_id = cursor.read_lp_str()?;
        let device_label = cursor.read_lp_str()?;
        let signer_device_id = cursor.read_lp_str()?;
        let kem_public_key = cursor.read_lp_bytes()?;
        let sig_public_key = cursor.read_lp_bytes()?;
        let issued_at_ms = cursor.read_u64_be()?;
        let expires_raw = cursor.read_u64_be()?;
        cursor.expect_consumed()?;

        let certificate = Self {
            user_id,
            device_id,
            device_label,
            signer_device_id,
            kem_public_key,
            sig_public_key,
            issued_at_ms,
            expires_at_ms: decode_expiry(expires_raw),
        };
        validate_certificate(&certificate)?;
        Ok(certificate)
    }

    pub(crate) fn is_valid_at(&self, now_ms: u64) -> bool {
        self.expires_at_ms.is_none_or(|expires| now_ms < expires)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SignedDeviceCertificate {
    pub(crate) body_bytes: Vec<u8>,
    pub(crate) signature: Vec<u8>,
}

pub(crate) fn build_self_cert(
    user_id: &str,
    own_device_id: &str,
    own_label: &str,
    own_kem_pub: &[u8],
    own_signing_keypair: &PqdsaKeyPair,
    issued_at_ms: u64,
    expires_at_ms: Option<u64>,
) -> Result<SignedDeviceCertificate> {
    let sig_pub = own_signing_keypair.public_key().as_ref().to_vec();
    let cert = DeviceCertificate {
        user_id: user_id.to_owned(),
        device_id: own_device_id.to_owned(),
        device_label: own_label.to_owned(),
        signer_device_id: own_device_id.to_owned(),
        kem_public_key: own_kem_pub.to_vec(),
        sig_public_key: sig_pub,
        issued_at_ms,
        expires_at_ms,
    };
    sign_certificate(&cert, own_signing_keypair)
}

#[expect(clippy::too_many_arguments, reason = "flat signed tuple")]
pub(crate) fn build_cert_for(
    user_id: &str,
    new_device_id: &str,
    new_device_label: &str,
    new_kem_pub: &[u8],
    new_sig_pub: &[u8],
    signer_device_id: &str,
    signer_keypair: &PqdsaKeyPair,
    issued_at_ms: u64,
    expires_at_ms: Option<u64>,
) -> Result<SignedDeviceCertificate> {
    validate_canonical_device_id(DEVICE_CERT_CONTEXT, "new device_id", new_device_id)?;
    validate_canonical_device_id(DEVICE_CERT_CONTEXT, "signer_device_id", signer_device_id)?;
    if signer_device_id == new_device_id {
        return Err(Error::Invalid {
            reason: "use build_self_cert for self-signed bootstrap".to_owned(),
        });
    }
    let cert = DeviceCertificate {
        user_id: user_id.to_owned(),
        device_id: new_device_id.to_owned(),
        device_label: new_device_label.to_owned(),
        signer_device_id: signer_device_id.to_owned(),
        kem_public_key: new_kem_pub.to_vec(),
        sig_public_key: new_sig_pub.to_vec(),
        issued_at_ms,
        expires_at_ms,
    };
    sign_certificate(&cert, signer_keypair)
}

pub(crate) fn verify_certificate(
    body_bytes: &[u8],
    signature: &[u8],
    signer_sig_pubkey: &[u8],
) -> Result<DeviceCertificate> {
    if signature.len() != ML_DSA_65_SIGNATURE_LEN {
        return Err(Error::Invalid {
            reason: format!(
                "{DEVICE_CERT_CONTEXT}: signature length {} does not equal ML_DSA_65_SIGNATURE_LEN ({ML_DSA_65_SIGNATURE_LEN})",
                signature.len()
            ),
        });
    }
    let mut payload = Vec::with_capacity(DEVICE_CERT_V2.len() + body_bytes.len());
    payload.extend_from_slice(DEVICE_CERT_V2);
    payload.extend_from_slice(body_bytes);

    ML_DSA_65
        .verify_sig(signer_sig_pubkey, &payload, signature)
        .map_err(|_| Error::Invalid {
            reason: "device certificate signature verification failed".to_owned(),
        })?;

    let cert = DeviceCertificate::parse_body(body_bytes)?;

    if cert.is_self_signed() && cert.sig_public_key != signer_sig_pubkey {
        return Err(Error::Invalid {
            reason: "self-signed cert pubkey must match signer pubkey".to_owned(),
        });
    }

    Ok(cert)
}

fn sign_certificate(
    cert: &DeviceCertificate,
    signer: &PqdsaKeyPair,
) -> Result<SignedDeviceCertificate> {
    let body_bytes = cert.serialize_body()?;
    let mut payload = Vec::with_capacity(DEVICE_CERT_V2.len() + body_bytes.len());
    payload.extend_from_slice(DEVICE_CERT_V2);
    payload.extend_from_slice(&body_bytes);

    let mut signature = vec![0u8; ML_DSA_65_SIGNATURE_LEN];
    let sig_len = signer
        .sign(&payload, &mut signature)
        .map_err(|_| Error::Invalid {
            reason: "device certificate signing failed".to_owned(),
        })?;
    signature.truncate(sig_len);

    Ok(SignedDeviceCertificate {
        body_bytes,
        signature,
    })
}

fn validate_certificate(cert: &DeviceCertificate) -> Result<()> {
    validate_timestamps(DEVICE_CERT_CONTEXT, cert.issued_at_ms, cert.expires_at_ms)?;
    validate_canonical_user_id(DEVICE_CERT_CONTEXT, &cert.user_id)?;
    validate_canonical_device_id(DEVICE_CERT_CONTEXT, "device_id", &cert.device_id)?;
    reject_bounded_str(
        DEVICE_CERT_CONTEXT,
        "device_label",
        &cert.device_label,
        DEVICE_LABEL_MAX_UTF16_CODE_UNITS,
    )?;
    validate_canonical_device_id(
        DEVICE_CERT_CONTEXT,
        "signer_device_id",
        &cert.signer_device_id,
    )?;
    require_exact_bytes(
        DEVICE_CERT_CONTEXT,
        "KEM public key",
        &cert.kem_public_key,
        ML_KEM_768_PUBLIC_KEY_LEN,
    )?;
    require_exact_bytes(
        DEVICE_CERT_CONTEXT,
        "signature public key",
        &cert.sig_public_key,
        ML_DSA_65_PUBLIC_KEY_LEN,
    )
}

fn reject_bounded_str(
    context: &str,
    field: &str,
    value: &str,
    max_utf16_code_units: usize,
) -> Result<()> {
    if value.trim() != value
        || value.is_empty()
        || value.encode_utf16().count() > max_utf16_code_units
    {
        return Err(Error::Invalid {
            reason: format!(
                "{context}: {field} must be canonical non-blank text of at most {max_utf16_code_units} UTF-16 code units"
            ),
        });
    }
    Ok(())
}

fn require_exact_bytes(
    context: &str,
    field: &str,
    value: &[u8],
    required_length: usize,
) -> Result<()> {
    if value.len() != required_length {
        return Err(Error::Invalid {
            reason: format!("{context}: {field} must be exactly {required_length} bytes"),
        });
    }
    Ok(())
}
