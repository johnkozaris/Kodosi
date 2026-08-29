use aws_lc_rs::signature::{KeyPair, ML_DSA_65, PqdsaKeyPair, VerificationAlgorithm};

#[cfg(test)]
use aws_lc_rs::signature::ML_DSA_65_SIGNING;

#[cfg(test)]
use super::wire_codec::MAX_FIELD_LEN;
use super::wire_codec::{
    DEVICE_LABEL_MAX_UTF16_CODE_UNITS, LpReader, LpWriter, MAX_DEVICE_CERTIFICATE_BODY_LEN,
    ML_DSA_65_PUBLIC_KEY_LEN, ML_DSA_65_SIGNATURE_LEN, ML_KEM_768_PUBLIC_KEY_LEN, decode_expiry,
    encode_expiry, validate_canonical_device_id, validate_canonical_user_id, validate_timestamps,
};
use crate::{AppError, Result};
use kodosi_domain::domain_tags::DEVICE_CERT_V2;

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
            return Err(AppError::Unsupported {
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
            return Err(AppError::Unsupported {
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
        return Err(AppError::Unsupported {
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
        return Err(AppError::Unsupported {
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
        .map_err(|_| AppError::Unsupported {
            reason: "device certificate signature verification failed".to_owned(),
        })?;

    let cert = DeviceCertificate::parse_body(body_bytes)?;

    if cert.is_self_signed() && cert.sig_public_key != signer_sig_pubkey {
        return Err(AppError::Unsupported {
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
        .map_err(|_| AppError::Unsupported {
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
        return Err(AppError::Unsupported {
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
        return Err(AppError::Unsupported {
            reason: format!("{context}: {field} must be exactly {required_length} bytes"),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_USER_ID: &str = "01900000-0000-7000-8000-000000000001";

    fn fresh_keypair() -> PqdsaKeyPair {
        PqdsaKeyPair::generate(&ML_DSA_65_SIGNING)
            .unwrap_or_else(|err| panic!("ML-DSA-65 keypair should generate: {err:?}"))
    }

    fn fake_kem_pub() -> Vec<u8> {
        (0..1184u32).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn self_cert_round_trips_and_verifies() {
        let kp = fresh_keypair();
        let kem = fake_kem_pub();
        let expected = DeviceCertificate {
            user_id: TEST_USER_ID.to_owned(),
            device_id: "device-alice-1".to_owned(),
            device_label: "Alice's MacBook".to_owned(),
            signer_device_id: "device-alice-1".to_owned(),
            kem_public_key: kem.clone(),
            sig_public_key: kp.public_key().as_ref().to_vec(),
            issued_at_ms: 1_700_000_000_000,
            expires_at_ms: Some(1_710_000_000_000),
        };
        let signed = build_self_cert(
            &expected.user_id,
            &expected.device_id,
            &expected.device_label,
            &expected.kem_public_key,
            &kp,
            expected.issued_at_ms,
            expected.expires_at_ms,
        )
        .expect("self cert builds");

        assert!(expected.is_self_signed());

        let parsed = DeviceCertificate::parse_body(&signed.body_bytes).expect("body should parse");
        assert_eq!(parsed, expected);

        let pub_bytes = kp.public_key().as_ref();
        let verified = verify_certificate(&signed.body_bytes, &signed.signature, pub_bytes)
            .expect("self cert should verify");
        assert_eq!(verified, expected);
    }

    #[test]
    fn cross_cert_round_trips_and_verifies() {
        let signer = fresh_keypair();
        let new_signing = fresh_keypair();
        let new_kem = fake_kem_pub();

        let expected = DeviceCertificate {
            user_id: TEST_USER_ID.to_owned(),
            device_id: "device-alice-2".to_owned(),
            device_label: "Alice's iPad".to_owned(),
            signer_device_id: "device-alice-1".to_owned(),
            kem_public_key: new_kem,
            sig_public_key: new_signing.public_key().as_ref().to_vec(),
            issued_at_ms: 1_700_000_500_000,
            expires_at_ms: Some(1_703_000_000_000),
        };
        let signed = build_cert_for(
            &expected.user_id,
            &expected.device_id,
            &expected.device_label,
            &expected.kem_public_key,
            &expected.sig_public_key,
            &expected.signer_device_id,
            &signer,
            expected.issued_at_ms,
            expected.expires_at_ms,
        )
        .expect("cross cert builds");

        assert!(!expected.is_self_signed());

        let parsed = DeviceCertificate::parse_body(&signed.body_bytes).expect("body should parse");
        assert_eq!(parsed, expected);

        let signer_pub = signer.public_key().as_ref();
        let verified = verify_certificate(&signed.body_bytes, &signed.signature, signer_pub)
            .expect("cross cert should verify with signer's pubkey");
        assert_eq!(verified, expected);
    }

    #[test]
    fn cross_cert_rejects_self_signer_id() {
        let kp = fresh_keypair();
        let kem = fake_kem_pub();
        let other_signing = fresh_keypair();

        let result = build_cert_for(
            TEST_USER_ID,
            "device-x",
            "Whatever",
            &kem,
            other_signing.public_key().as_ref(),
            "device-x",
            &kp,
            0,
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn build_rejects_empty_user_id() {
        let kp = fresh_keypair();
        let kem = fake_kem_pub();
        let result = build_self_cert("", "device-1", "Mac", &kem, &kp, 0, None);
        assert!(result.is_err(), "empty user_id must reject");

        let other = fresh_keypair();
        let result = build_cert_for(
            "",
            "device-2",
            "iPad",
            &kem,
            other.public_key().as_ref(),
            "device-1",
            &kp,
            0,
            None,
        );
        assert!(result.is_err(), "cross cert empty user_id must reject");
    }

    #[test]
    fn build_rejects_missing_identity_and_key_fields() {
        let kp = fresh_keypair();
        let kem = fake_kem_pub();
        let other = fresh_keypair();
        assert!(build_self_cert(TEST_USER_ID, "", "Mac", &kem, &kp, 0, None).is_err());
        assert!(build_self_cert(TEST_USER_ID, "device-1", "Mac", &[], &kp, 0, None).is_err());
        assert!(
            build_cert_for(
                TEST_USER_ID,
                "device-2",
                "iPad",
                &kem,
                other.public_key().as_ref(),
                "",
                &kp,
                0,
                None,
            )
            .is_err()
        );
        assert!(
            build_cert_for(
                TEST_USER_ID,
                "device-2",
                "iPad",
                &kem,
                &[],
                "device-1",
                &kp,
                0,
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn verify_rejects_signature_from_wrong_signer() {
        let signer = fresh_keypair();
        let attacker = fresh_keypair();
        let new_signing = fresh_keypair();
        let kem = fake_kem_pub();

        let signed = build_cert_for(
            TEST_USER_ID,
            "device-alice-2",
            "iPad",
            &kem,
            new_signing.public_key().as_ref(),
            "device-alice-1",
            &signer,
            0,
            None,
        )
        .unwrap();

        let result = verify_certificate(
            &signed.body_bytes,
            &signed.signature,
            attacker.public_key().as_ref(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn verify_rejects_tampered_body() {
        let kp = fresh_keypair();
        let kem = fake_kem_pub();
        let signed = build_self_cert(
            TEST_USER_ID,
            "device-alice-1",
            "MacBook",
            &kem,
            &kp,
            1_700_000_000_000,
            None,
        )
        .unwrap();

        let mut tampered = signed.body_bytes.clone();
        tampered[100] ^= 0xFF;

        let result = verify_certificate(&tampered, &signed.signature, kp.public_key().as_ref());
        assert!(result.is_err());
    }

    #[test]
    fn verify_rejects_wrong_signature_length() {
        let kp = fresh_keypair();
        let signed = build_self_cert(
            TEST_USER_ID,
            "device-alice-1",
            "MacBook",
            &fake_kem_pub(),
            &kp,
            1_700_000_000_000,
            None,
        )
        .unwrap();

        let result = verify_certificate(
            &signed.body_bytes,
            &signed.signature[..ML_DSA_65_SIGNATURE_LEN - 1],
            kp.public_key().as_ref(),
        );
        assert!(result.is_err(), "wrong signature length must reject");
    }

    #[test]
    fn verify_rejects_self_signed_cert_with_wrong_pubkey_field() {
        let kp = fresh_keypair();
        let kem = fake_kem_pub();
        let cert = DeviceCertificate {
            user_id: TEST_USER_ID.to_owned(),
            device_id: "device-evil".to_owned(),
            device_label: "Pretender".to_owned(),
            signer_device_id: "device-evil".to_owned(),
            kem_public_key: kem,
            sig_public_key: vec![0xDE; 1952],
            issued_at_ms: 0,
            expires_at_ms: None,
        };
        let signed = sign_certificate(&cert, &kp).unwrap();

        let result = verify_certificate(
            &signed.body_bytes,
            &signed.signature,
            kp.public_key().as_ref(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn parse_rejects_truncated_body() {
        let kp = fresh_keypair();
        let kem = fake_kem_pub();
        let signed = build_self_cert(TEST_USER_ID, "device-1", "Mac", &kem, &kp, 0, None).unwrap();

        let truncated = &signed.body_bytes[..signed.body_bytes.len() - 1];
        let result = DeviceCertificate::parse_body(truncated);
        assert!(result.is_err());
    }

    #[test]
    fn parse_rejects_trailing_bytes() {
        let kp = fresh_keypair();
        let kem = fake_kem_pub();
        let signed = build_self_cert(TEST_USER_ID, "device-1", "Mac", &kem, &kp, 0, None).unwrap();

        let mut padded = signed.body_bytes.clone();
        padded.push(0x00);
        let result = DeviceCertificate::parse_body(&padded);
        assert!(result.is_err());
    }

    #[test]
    fn parse_rejects_invalid_utf8_in_string_field() {
        let kp = fresh_keypair();
        let kem = fake_kem_pub();
        let signed = build_self_cert(TEST_USER_ID, "device-1", "Mac", &kem, &kp, 0, None).unwrap();

        let mut tampered = signed.body_bytes.clone();
        tampered[4] = 0xC0;
        tampered[5] = 0xC0;

        let result = DeviceCertificate::parse_body(&tampered);
        assert!(result.is_err(), "non-UTF-8 string field must reject");
    }

    #[test]
    fn parse_rejects_field_exceeding_max_field_len() {
        let mut body = Vec::with_capacity(8);
        body.extend_from_slice(&(MAX_FIELD_LEN + 1).to_be_bytes());
        let result = DeviceCertificate::parse_body(&body);
        assert!(
            result.is_err(),
            "field length above MAX_FIELD_LEN must reject"
        );
    }

    #[test]
    fn parse_rejects_body_exceeding_max_body_len() {
        let result = DeviceCertificate::parse_body(&vec![0; MAX_DEVICE_CERTIFICATE_BODY_LEN + 1]);
        assert!(result.is_err(), "oversized certificate body must reject");
    }

    #[test]
    fn build_rejects_zero_expiry_sentinel() {
        let kp = fresh_keypair();
        let kem = fake_kem_pub();

        let result = build_self_cert(TEST_USER_ID, "device-1", "Mac", &kem, &kp, 0, Some(0));
        assert!(result.is_err(), "Some(0) collides with no-expiry sentinel");

        let signer = fresh_keypair();
        let other = fresh_keypair();
        let result = build_cert_for(
            TEST_USER_ID,
            "device-2",
            "iPad",
            &kem,
            other.public_key().as_ref(),
            "device-1",
            &signer,
            0,
            Some(0),
        );
        assert!(result.is_err(), "build_cert_for must also reject Some(0)");
    }

    #[test]
    fn expiry_check_distinguishes_no_expiry_from_zero_timestamp() {
        let kp = fresh_keypair();
        let kem = fake_kem_pub();
        let signed = build_self_cert(TEST_USER_ID, "device-1", "Mac", &kem, &kp, 0, None).unwrap();
        let parsed = DeviceCertificate::parse_body(&signed.body_bytes).unwrap();
        assert!(parsed.expires_at_ms.is_none());
        assert!(parsed.is_valid_at(u64::MAX));

        let with_expiry =
            build_self_cert(TEST_USER_ID, "device-2", "Mac", &kem, &kp, 100, Some(200)).unwrap();
        let parsed = DeviceCertificate::parse_body(&with_expiry.body_bytes).unwrap();
        assert_eq!(parsed.expires_at_ms, Some(200));
        assert!(parsed.is_valid_at(150));
        assert!(!parsed.is_valid_at(200));
        assert!(!parsed.is_valid_at(300));
    }
}

#[cfg(test)]
#[path = "device_cert_proptests.rs"]
mod proptests;
