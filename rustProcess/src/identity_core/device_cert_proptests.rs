use super::*;
use crate::identity_core::wire_codec::MAX_UNIX_TIME_MILLISECONDS;
use aws_lc_rs::signature::{KeyPair, ML_DSA_65_SIGNING};
use proptest::prelude::*;
use std::sync::OnceLock;

use kodosi_domain::domain_tags::DEVICE_LIST_V1;

const TEST_USER_ID: &str = "01900000-0000-7000-8000-000000000001";

static SHARED_KEYPAIR: OnceLock<PqdsaKeyPair> = OnceLock::new();
static WRONG_KEYPAIR: OnceLock<PqdsaKeyPair> = OnceLock::new();

fn fresh_keypair() -> PqdsaKeyPair {
    PqdsaKeyPair::generate(&ML_DSA_65_SIGNING)
        .unwrap_or_else(|err| panic!("ML-DSA-65 keypair should generate: {err:?}"))
}

fn shared_keypair() -> &'static PqdsaKeyPair {
    SHARED_KEYPAIR.get_or_init(fresh_keypair)
}

fn wrong_keypair() -> &'static PqdsaKeyPair {
    WRONG_KEYPAIR.get_or_init(fresh_keypair)
}

fn device_id_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9-]{0,23}"
}

fn device_label_strategy() -> impl Strategy<Value = String> {
    "[A-Za-z0-9][A-Za-z0-9_.-]{0,31}"
}

fn kem_public_key_strategy() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), ML_KEM_768_PUBLIC_KEY_LEN)
}

fn signing_public_key_strategy() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), ML_DSA_65_PUBLIC_KEY_LEN)
}

fn expiry_strategy() -> impl Strategy<Value = Option<u64>> {
    prop::option::of(1u64..=MAX_UNIX_TIME_MILLISECONDS)
}

fn device_certificate_strategy() -> BoxedStrategy<DeviceCertificate> {
    (
        device_id_strategy(),
        device_label_strategy(),
        kem_public_key_strategy(),
        signing_public_key_strategy(),
        0u64..=MAX_UNIX_TIME_MILLISECONDS,
        expiry_strategy(),
    )
        .prop_map(
            |(
                device_id,
                device_label,
                kem_public_key,
                sig_public_key,
                issued_at_ms,
                expires_at_ms,
            )| DeviceCertificate {
                user_id: TEST_USER_ID.to_owned(),
                signer_device_id: format!("{device_id}-signer"),
                device_id,
                device_label,
                kem_public_key,
                sig_public_key,
                issued_at_ms,
                expires_at_ms,
            },
        )
        .boxed()
}

fn self_signed_mismatch_certificate_strategy() -> BoxedStrategy<DeviceCertificate> {
    (
        device_id_strategy(),
        device_label_strategy(),
        kem_public_key_strategy(),
        signing_public_key_strategy(),
        0u64..=MAX_UNIX_TIME_MILLISECONDS,
        expiry_strategy(),
    )
        .prop_map(
            |(
                device_id,
                device_label,
                kem_public_key,
                sig_public_key,
                issued_at_ms,
                expires_at_ms,
            )| DeviceCertificate {
                user_id: TEST_USER_ID.to_owned(),
                device_id: device_id.clone(),
                device_label,
                signer_device_id: device_id,
                kem_public_key,
                sig_public_key,
                issued_at_ms,
                expires_at_ms,
            },
        )
        .boxed()
}

fn signed_certificate_strategy() -> BoxedStrategy<(DeviceCertificate, SignedDeviceCertificate)> {
    device_certificate_strategy()
        .prop_map(|expected| {
            let signed =
                sign_certificate(&expected, shared_keypair()).expect("generated cert signs");
            (expected, signed)
        })
        .boxed()
}

fn signed_certificate_with_body_index_strategy() -> BoxedStrategy<(SignedDeviceCertificate, usize)>
{
    signed_certificate_strategy()
        .prop_flat_map(|(_, certificate)| {
            let body_len = certificate.body_bytes.len();
            (Just(certificate), 0..body_len)
        })
        .boxed()
}

fn signed_certificate_with_signature_index_strategy()
-> BoxedStrategy<(SignedDeviceCertificate, usize)> {
    signed_certificate_strategy()
        .prop_flat_map(|(_, certificate)| {
            let signature_len = certificate.signature.len();
            (Just(certificate), 0..signature_len)
        })
        .boxed()
}

fn sign_body_with_domain(body: &[u8], domain_tag: &[u8], signer: &PqdsaKeyPair) -> Vec<u8> {
    let mut payload = Vec::with_capacity(domain_tag.len() + body.len());
    payload.extend_from_slice(domain_tag);
    payload.extend_from_slice(body);

    let mut signature = vec![0u8; ML_DSA_65_SIGNATURE_LEN];
    let sig_len = signer
        .sign(&payload, &mut signature)
        .expect("generated payload signs");
    signature.truncate(sig_len);
    signature
}

fn proptest_config() -> ProptestConfig {
    let mut config = ProptestConfig::with_cases(128);
    config.failure_persistence = None;
    config
}

proptest! {
    #![proptest_config(proptest_config())]

    #[test]
    fn generated_certs_round_trip_and_preserve_canonical_bytes(cert in device_certificate_strategy()) {
        let body = cert.serialize_body().expect("generated cert serializes");
        let parsed = DeviceCertificate::parse_body(&body).expect("generated body parses");

        prop_assert_eq!(&parsed, &cert);
        prop_assert_eq!(parsed.serialize_body().expect("parsed cert serializes"), body);
    }

    #[test]
    fn all_strict_prefixes_of_valid_cert_bodies_reject(cert in device_certificate_strategy()) {
        let body = cert.serialize_body().expect("generated cert serializes");

        for cut in 0..body.len() {
            prop_assert!(DeviceCertificate::parse_body(&body[..cut]).is_err());
        }
    }

    #[test]
    fn valid_cert_bodies_with_trailing_bytes_reject(
        cert in device_certificate_strategy(),
        trailing in prop::collection::vec(any::<u8>(), 1..=16),
    ) {
        let mut body = cert.serialize_body().expect("generated cert serializes");
        body.extend_from_slice(&trailing);

        prop_assert!(DeviceCertificate::parse_body(&body).is_err());
    }

    #[test]
    fn arbitrary_cert_body_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..=8192)) {
        drop(DeviceCertificate::parse_body(&bytes));
    }

    #[test]
    fn generated_certificates_verify((expected, certificate) in signed_certificate_strategy()) {
        prop_assert_eq!(
            DeviceCertificate::parse_body(&certificate.body_bytes)
                .expect("generated certificate body parses"),
            expected.clone()
        );
        prop_assert_eq!(
            verify_certificate(
                &certificate.body_bytes,
                &certificate.signature,
                shared_keypair().public_key().as_ref(),
            )
            .expect("generated certificate verifies"),
            expected
        );
    }

    #[test]
    fn tampered_generated_cert_body_fails_signature_verification(
        (certificate, index) in signed_certificate_with_body_index_strategy(),
    ) {
        let mut body = certificate.body_bytes.clone();
        body[index] ^= 0x01;

        prop_assert!(
            verify_certificate(
                &body,
                &certificate.signature,
                shared_keypair().public_key().as_ref(),
            )
            .is_err()
        );
    }

    #[test]
    fn tampered_generated_cert_signature_fails_verification(
        (certificate, index) in signed_certificate_with_signature_index_strategy(),
    ) {
        let mut signature = certificate.signature.clone();
        signature[index] ^= 0x01;

        prop_assert!(
            verify_certificate(
                &certificate.body_bytes,
                &signature,
                shared_keypair().public_key().as_ref(),
            )
            .is_err()
        );
    }

    #[test]
    fn wrong_signer_key_rejects_generated_certificates((_, certificate) in signed_certificate_strategy()) {
        prop_assert!(
            verify_certificate(
                &certificate.body_bytes,
                &certificate.signature,
                wrong_keypair().public_key().as_ref(),
            )
            .is_err()
        );
    }

    #[test]
    fn device_list_domain_signature_does_not_verify_as_certificate(cert in device_certificate_strategy()) {
        let body = cert.serialize_body().expect("generated cert serializes");
        let signature = sign_body_with_domain(&body, DEVICE_LIST_V1, shared_keypair());

        prop_assert!(
            verify_certificate(
                &body,
                &signature,
                shared_keypair().public_key().as_ref(),
            )
            .is_err()
        );
    }

    #[test]
    fn self_signed_cert_pubkey_mismatch_rejects(cert in self_signed_mismatch_certificate_strategy()) {
        let certificate =
            sign_certificate(&cert, shared_keypair()).expect("generated cert signs");

        prop_assert!(
            verify_certificate(
                &certificate.body_bytes,
                &certificate.signature,
                shared_keypair().public_key().as_ref(),
            )
            .is_err()
        );
    }
}
