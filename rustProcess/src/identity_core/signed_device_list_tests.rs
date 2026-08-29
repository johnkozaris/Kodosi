use super::super::wire_codec::{LpWriter, MAX_UNIX_TIME_MILLISECONDS};
use super::*;
use aws_lc_rs::signature::ML_DSA_65_SIGNING;
use kodosi_domain::domain_tags::DEVICE_CERT_V2;
use proptest::prelude::*;
use std::{collections::BTreeSet, sync::OnceLock};

const PROPTEST_MAX_ENTRIES: usize = 8;
const TEST_USER_ID: &str = "01900000-0000-7000-8000-000000000001";

static SHARED_KEYPAIR: OnceLock<PqdsaKeyPair> = OnceLock::new();

fn fresh_keypair() -> PqdsaKeyPair {
    PqdsaKeyPair::generate(&ML_DSA_65_SIGNING)
        .unwrap_or_else(|err| panic!("ML-DSA-65 keypair should generate: {err:?}"))
}

fn shared_keypair() -> &'static PqdsaKeyPair {
    SHARED_KEYPAIR.get_or_init(fresh_keypair)
}

#[test]
fn bootstrap_list_round_trips_and_verifies() {
    let kp = fresh_keypair();
    let envelope = build_bootstrap_list(
        "01900000-0000-7000-8000-000000000001",
        "device-alice-1",
        &kp,
        1_700_000_000_000,
        None,
    )
    .expect("bootstrap list builds");

    assert_eq!(envelope.list.generation, 1);
    assert_eq!(envelope.list.entries.len(), 1);
    assert_eq!(envelope.list.entries[0].device_id, "device-alice-1");
    assert_eq!(envelope.list.entries[0].signer_device_id, "device-alice-1");

    let verified = verify_signed_device_list(
        &envelope.body_bytes,
        &envelope.signature,
        kp.public_key().as_ref(),
    )
    .expect("bootstrap list verifies");
    assert_eq!(verified, envelope.list);
}

#[test]
fn extended_list_chains_to_previous() {
    let mac = fresh_keypair();
    let bootstrap = build_bootstrap_list(
        "01900000-0000-7000-8000-000000000001",
        "device-alice-1",
        &mac,
        100,
        None,
    )
    .unwrap();

    let envelope = build_replacement_list(
        "01900000-0000-7000-8000-000000000001",
        bootstrap.list.generation,
        &bootstrap.list.entries,
        {
            let mut entries = bootstrap.list.entries.clone();
            entries.push(DeviceListEntry {
                device_id: "device-alice-2".to_owned(),
                signer_device_id: "device-alice-1".to_owned(),
            });
            entries
        },
        "device-alice-1",
        &mac,
        200,
        Some(300),
    )
    .expect("extended list builds");

    assert_eq!(envelope.list.generation, 2);
    assert_eq!(envelope.list.entries.len(), 2);

    let verified = verify_signed_device_list(
        &envelope.body_bytes,
        &envelope.signature,
        mac.public_key().as_ref(),
    )
    .unwrap();
    assert_eq!(verified.entries[1].signer_device_id, "device-alice-1");
}

#[test]
fn extended_list_rejects_signer_not_in_previous() {
    let mac = fresh_keypair();
    let interloper = fresh_keypair();
    let bootstrap = build_bootstrap_list(
        "01900000-0000-7000-8000-000000000001",
        "device-1",
        &mac,
        0,
        None,
    )
    .unwrap();

    let result = build_replacement_list(
        "01900000-0000-7000-8000-000000000001",
        bootstrap.list.generation,
        &bootstrap.list.entries,
        {
            let mut entries = bootstrap.list.entries.clone();
            entries.push(DeviceListEntry {
                device_id: "device-2".to_owned(),
                signer_device_id: "device-not-in-list".to_owned(),
            });
            entries
        },
        "device-not-in-list",
        &interloper,
        0,
        None,
    );
    assert!(result.is_err());
}

#[test]
fn extended_list_rejects_duplicate_device_id() {
    let mac = fresh_keypair();
    let bootstrap = build_bootstrap_list(
        "01900000-0000-7000-8000-000000000001",
        "device-1",
        &mac,
        0,
        None,
    )
    .unwrap();

    let result = build_replacement_list(
        "01900000-0000-7000-8000-000000000001",
        1,
        &bootstrap.list.entries,
        {
            let mut entries = bootstrap.list.entries.clone();
            entries.push(DeviceListEntry {
                device_id: "device-1".to_owned(),
                signer_device_id: "device-1".to_owned(),
            });
            entries
        },
        "device-1",
        &mac,
        0,
        None,
    );
    assert!(result.is_err());
}

#[test]
fn parse_rejects_signer_not_in_entries() {
    let list = SignedDeviceList {
        user_id: "01900000-0000-7000-8000-000000000001".to_owned(),
        generation: 1,
        entries: vec![DeviceListEntry {
            device_id: "device-1".to_owned(),
            signer_device_id: "device-1".to_owned(),
        }],
        signer_device_id: "device-impostor".to_owned(),
        issued_at_ms: 0,
        expires_at_ms: None,
    };
    let body = list.serialize_body().unwrap();
    let parsed = SignedDeviceList::parse_body(&body);
    assert!(parsed.is_err(), "parser must reject signer not in entries");
}

#[test]
fn parse_rejects_duplicate_device_ids() {
    let list = SignedDeviceList {
        user_id: "01900000-0000-7000-8000-000000000001".to_owned(),
        generation: 2,
        entries: vec![
            DeviceListEntry {
                device_id: "device-1".to_owned(),
                signer_device_id: "device-1".to_owned(),
            },
            DeviceListEntry {
                device_id: "device-1".to_owned(),
                signer_device_id: "device-1".to_owned(),
            },
        ],
        signer_device_id: "device-1".to_owned(),
        issued_at_ms: 1,
        expires_at_ms: None,
    };
    let mut writer = LpWriter::with_capacity(DEVICE_LIST_CONTEXT, 128);
    writer.write_lp_str(&list.user_id).unwrap();
    writer.write_u64_be(list.generation);
    writer.write_u32_be(2);
    for entry in &list.entries {
        writer.write_lp_str(&entry.device_id).unwrap();
        writer.write_lp_str(&entry.signer_device_id).unwrap();
    }
    writer.write_lp_str(&list.signer_device_id).unwrap();
    writer.write_u64_be(list.issued_at_ms);
    writer.write_u64_be(0);

    let parsed = SignedDeviceList::parse_body(&writer.finish());
    assert!(parsed.is_err(), "parser must reject duplicate device IDs");
}

#[test]
fn build_replacement_list_rejects_duplicate_device_ids() {
    let mac = fresh_keypair();
    let bootstrap = build_bootstrap_list(
        "01900000-0000-7000-8000-000000000001",
        "device-1",
        &mac,
        0,
        None,
    )
    .unwrap();
    let duplicate = bootstrap.list.entries[0].clone();

    let result = build_replacement_list(
        "01900000-0000-7000-8000-000000000001",
        bootstrap.list.generation,
        &bootstrap.list.entries,
        vec![duplicate.clone(), duplicate],
        "device-1",
        &mac,
        1,
        None,
    );

    assert!(result.is_err(), "builder must reject duplicate device IDs");
}

#[test]
fn parse_preserves_historical_certificate_signer() {
    let list = SignedDeviceList {
        user_id: "01900000-0000-7000-8000-000000000001".to_owned(),
        generation: 2,
        entries: vec![DeviceListEntry {
            device_id: "device-2".to_owned(),
            signer_device_id: "device-retired".to_owned(),
        }],
        signer_device_id: "device-2".to_owned(),
        issued_at_ms: 1,
        expires_at_ms: None,
    };
    let body = list.serialize_body().unwrap();
    let parsed = SignedDeviceList::parse_body(&body).unwrap();
    assert_eq!(parsed, list);
}

#[test]
fn parse_rejects_too_many_entries() {
    let mut writer = LpWriter::with_capacity(
        DEVICE_LIST_CONTEXT,
        4 + "01900000-0000-7000-8000-000000000001".len() + 8 + 4,
    );
    writer
        .write_lp_str("01900000-0000-7000-8000-000000000001")
        .unwrap();
    writer.write_u64_be(1);
    writer.write_u32_be(MAX_ENTRIES + 1);
    let body = writer.finish();
    let parsed = SignedDeviceList::parse_body(&body);
    assert!(parsed.is_err());
}

#[test]
fn build_rejects_zero_expiry_sentinel() {
    let kp = fresh_keypair();

    let result = build_bootstrap_list(
        "01900000-0000-7000-8000-000000000001",
        "device-1",
        &kp,
        0,
        Some(0),
    );
    assert!(result.is_err(), "Some(0) collides with no-expiry sentinel");

    let bootstrap = build_bootstrap_list(
        "01900000-0000-7000-8000-000000000001",
        "device-1",
        &kp,
        0,
        None,
    )
    .unwrap();
    let result = build_replacement_list(
        "01900000-0000-7000-8000-000000000001",
        1,
        &bootstrap.list.entries,
        {
            let mut entries = bootstrap.list.entries.clone();
            entries.push(DeviceListEntry {
                device_id: "device-2".to_owned(),
                signer_device_id: "device-1".to_owned(),
            });
            entries
        },
        "device-1",
        &kp,
        0,
        Some(0),
    );
    assert!(
        result.is_err(),
        "build_replacement_list must also reject Some(0)"
    );

    let result = build_replacement_list(
        "01900000-0000-7000-8000-000000000001",
        1,
        &bootstrap.list.entries,
        bootstrap.list.entries.clone(),
        "device-1",
        &kp,
        0,
        Some(0),
    );
    assert!(
        result.is_err(),
        "build_replacement_list must also reject Some(0)"
    );
}

#[test]
fn build_rejects_missing_identity_fields() {
    let kp = fresh_keypair();

    assert!(build_bootstrap_list("", "device-1", &kp, 0, None).is_err());
    assert!(
        build_bootstrap_list("01900000-0000-7000-8000-000000000001", "", &kp, 0, None).is_err()
    );

    let bootstrap = build_bootstrap_list(
        "01900000-0000-7000-8000-000000000001",
        "device-1",
        &kp,
        0,
        None,
    )
    .unwrap();
    assert!(
        build_replacement_list(
            "01900000-0000-7000-8000-000000000001",
            1,
            &bootstrap.list.entries,
            {
                let mut entries = bootstrap.list.entries.clone();
                entries.push(DeviceListEntry {
                    device_id: String::new(),
                    signer_device_id: "device-1".to_owned(),
                });
                entries
            },
            "device-1",
            &kp,
            0,
            None,
        )
        .is_err()
    );
    assert!(
        build_replacement_list(
            "01900000-0000-7000-8000-000000000001",
            1,
            &bootstrap.list.entries,
            vec![DeviceListEntry {
                device_id: "device-1".to_owned(),
                signer_device_id: String::new(),
            }],
            "device-1",
            &kp,
            0,
            None,
        )
        .is_err()
    );
}

#[test]
fn parse_rejects_trailing_bytes() {
    let kp = fresh_keypair();
    let envelope = build_bootstrap_list(
        "01900000-0000-7000-8000-000000000001",
        "device-1",
        &kp,
        0,
        None,
    )
    .unwrap();

    let mut padded = envelope.body_bytes.clone();
    padded.push(0x00);
    let result = SignedDeviceList::parse_body(&padded);
    assert!(
        result.is_err(),
        "trailing bytes after a valid body must reject"
    );
}

#[test]
fn parse_rejects_truncated_body() {
    let kp = fresh_keypair();
    let envelope = build_bootstrap_list(
        "01900000-0000-7000-8000-000000000001",
        "device-1",
        &kp,
        0,
        None,
    )
    .unwrap();

    let truncated = &envelope.body_bytes[..envelope.body_bytes.len() - 1];
    let result = SignedDeviceList::parse_body(truncated);
    assert!(result.is_err(), "truncated body must reject");
}

#[test]
fn parse_rejects_invalid_utf8_in_string_field() {
    let kp = fresh_keypair();
    let envelope = build_bootstrap_list(
        "01900000-0000-7000-8000-000000000001",
        "device-1",
        &kp,
        0,
        None,
    )
    .unwrap();

    let mut tampered = envelope.body_bytes.clone();
    tampered[4] = 0xC0;
    tampered[5] = 0xC0;

    let result = SignedDeviceList::parse_body(&tampered);
    assert!(result.is_err(), "invalid UTF-8 must reject");
}

#[test]
fn parse_rejects_field_exceeding_max_field_len() {
    let mut body = Vec::with_capacity(8);
    body.extend_from_slice(&(MAX_FIELD_LEN + 1).to_be_bytes());

    let result = SignedDeviceList::parse_body(&body);
    assert!(result.is_err(), "field above MAX_FIELD_LEN must reject");
}

#[test]
fn build_replacement_list_supports_revocation() {
    let mac = fresh_keypair();
    let bootstrap = build_bootstrap_list(
        "01900000-0000-7000-8000-000000000001",
        "device-mac",
        &mac,
        100,
        None,
    )
    .unwrap();
    let two_device_list = build_replacement_list(
        "01900000-0000-7000-8000-000000000001",
        1,
        &bootstrap.list.entries,
        {
            let mut entries = bootstrap.list.entries.clone();
            entries.push(DeviceListEntry {
                device_id: "device-ipad".to_owned(),
                signer_device_id: "device-mac".to_owned(),
            });
            entries
        },
        "device-mac",
        &mac,
        200,
        None,
    )
    .unwrap();

    let revoked = build_replacement_list(
        "01900000-0000-7000-8000-000000000001",
        two_device_list.list.generation,
        &two_device_list.list.entries,
        vec![DeviceListEntry {
            device_id: "device-mac".to_owned(),
            signer_device_id: "device-mac".to_owned(),
        }],
        "device-mac",
        &mac,
        300,
        None,
    )
    .expect("revocation list builds");

    assert_eq!(revoked.list.generation, 3);
    assert_eq!(revoked.list.entries.len(), 1);
    assert!(
        !revoked
            .list
            .entries
            .iter()
            .any(|e| e.device_id == "device-ipad"),
        "iPad must be absent from the revocation list"
    );

    let verified = verify_signed_device_list(
        &revoked.body_bytes,
        &revoked.signature,
        mac.public_key().as_ref(),
    )
    .expect("revocation list must verify");
    assert_eq!(verified.entries.len(), 1);
}

#[test]
fn build_replacement_list_rejects_signer_not_in_new_set() {
    let mac = fresh_keypair();
    let bootstrap = build_bootstrap_list(
        "01900000-0000-7000-8000-000000000001",
        "device-mac",
        &mac,
        0,
        None,
    )
    .unwrap();

    let result = build_replacement_list(
        "01900000-0000-7000-8000-000000000001",
        1,
        &bootstrap.list.entries,
        vec![DeviceListEntry {
            device_id: "device-other".to_owned(),
            signer_device_id: "device-other".to_owned(),
        }],
        "device-mac",
        &mac,
        0,
        None,
    );
    assert!(result.is_err());
}

#[test]
fn build_replacement_list_preserves_retained_certificate_signer() {
    let mac = fresh_keypair();
    let ipad = fresh_keypair();
    let bootstrap = build_bootstrap_list(
        "01900000-0000-7000-8000-000000000001",
        "device-mac",
        &mac,
        0,
        None,
    )
    .unwrap();
    let two_device_list = build_replacement_list(
        "01900000-0000-7000-8000-000000000001",
        1,
        &bootstrap.list.entries,
        {
            let mut entries = bootstrap.list.entries.clone();
            entries.push(DeviceListEntry {
                device_id: "device-ipad".to_owned(),
                signer_device_id: "device-mac".to_owned(),
            });
            entries
        },
        "device-mac",
        &mac,
        1,
        None,
    )
    .unwrap();

    let result = build_replacement_list(
        "01900000-0000-7000-8000-000000000001",
        two_device_list.list.generation,
        &two_device_list.list.entries,
        vec![DeviceListEntry {
            device_id: "device-ipad".to_owned(),
            signer_device_id: "device-mac".to_owned(),
        }],
        "device-ipad",
        &ipad,
        2,
        None,
    );

    let replacement = result.expect("retained certificate signer may be historical");
    assert_eq!(
        replacement.list.entries,
        vec![DeviceListEntry {
            device_id: "device-ipad".to_owned(),
            signer_device_id: "device-mac".to_owned(),
        }]
    );
    assert_eq!(replacement.list.signer_device_id, "device-ipad");
}

#[test]
fn build_replacement_list_rejects_empty_set() {
    let mac = fresh_keypair();
    let bootstrap = build_bootstrap_list(
        "01900000-0000-7000-8000-000000000001",
        "device-mac",
        &mac,
        0,
        None,
    )
    .unwrap();

    let result = build_replacement_list(
        "01900000-0000-7000-8000-000000000001",
        1,
        &bootstrap.list.entries,
        Vec::new(),
        "device-mac",
        &mac,
        0,
        None,
    );
    assert!(result.is_err(), "empty replacement list must reject");
}

#[test]
fn verify_rejects_signature_from_wrong_signer() {
    let mac = fresh_keypair();
    let attacker = fresh_keypair();
    let envelope = build_bootstrap_list(
        "01900000-0000-7000-8000-000000000001",
        "device-1",
        &mac,
        0,
        None,
    )
    .unwrap();

    let result = verify_signed_device_list(
        &envelope.body_bytes,
        &envelope.signature,
        attacker.public_key().as_ref(),
    );
    assert!(result.is_err());
}

#[test]
fn verify_rejects_wrong_signature_length() {
    let mac = fresh_keypair();
    let envelope = build_bootstrap_list(TEST_USER_ID, "device-1", &mac, 0, None).unwrap();

    let result = verify_signed_device_list(
        &envelope.body_bytes,
        &envelope.signature[..ML_DSA_65_SIGNATURE_LEN - 1],
        mac.public_key().as_ref(),
    );
    assert!(result.is_err(), "wrong signature length must reject");
}

fn device_id_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9-]{0,23}"
}

fn signed_device_list_strategy() -> BoxedStrategy<SignedDeviceList> {
    prop::collection::btree_set(device_id_strategy(), 1..=PROPTEST_MAX_ENTRIES)
        .prop_flat_map(|device_ids: BTreeSet<String>| {
            let device_ids: Vec<String> = device_ids.into_iter().collect();
            let entry_count = device_ids.len();
            (
                Just(device_ids),
                1u64..=u64::MAX,
                0u64..=MAX_UNIX_TIME_MILLISECONDS,
                prop::option::of(1u64..=MAX_UNIX_TIME_MILLISECONDS),
                0..entry_count,
                prop::collection::vec(0..entry_count, entry_count.saturating_sub(1)),
            )
        })
        .prop_map(
            |(
                device_ids,
                generation,
                issued_at_ms,
                expires_at_ms,
                list_signer_index,
                non_root_signer_indexes,
            )| {
                let mut entries = Vec::with_capacity(device_ids.len());
                entries.push(DeviceListEntry {
                    device_id: device_ids[0].clone(),
                    signer_device_id: device_ids[0].clone(),
                });
                entries.extend(device_ids.iter().enumerate().skip(1).map(
                    |(entry_index, device_id)| {
                        let signer_index = non_root_signer_indexes[entry_index - 1] % entry_index;
                        DeviceListEntry {
                            device_id: device_id.clone(),
                            signer_device_id: device_ids[signer_index].clone(),
                        }
                    },
                ));

                SignedDeviceList {
                    user_id: TEST_USER_ID.to_owned(),
                    generation,
                    entries,
                    signer_device_id: device_ids[list_signer_index].clone(),
                    issued_at_ms,
                    expires_at_ms,
                }
            },
        )
        .boxed()
}

fn signed_envelope_strategy() -> BoxedStrategy<SignedDeviceListEnvelope> {
    signed_device_list_strategy()
        .prop_map(|list| sign_list(&list, shared_keypair()).expect("generated list signs"))
        .boxed()
}

fn signed_envelope_with_body_index_strategy() -> BoxedStrategy<(SignedDeviceListEnvelope, usize)> {
    signed_envelope_strategy()
        .prop_flat_map(|envelope| {
            let body_len = envelope.body_bytes.len();
            (Just(envelope), 0..body_len)
        })
        .boxed()
}

fn signed_envelope_with_signature_index_strategy()
-> BoxedStrategy<(SignedDeviceListEnvelope, usize)> {
    signed_envelope_strategy()
        .prop_flat_map(|envelope| {
            let signature_len = envelope.signature.len();
            (Just(envelope), 0..signature_len)
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
    fn generated_lists_round_trip_and_preserve_canonical_bytes(list in signed_device_list_strategy()) {
        let body = list.serialize_body().expect("generated list serializes");
        let parsed = SignedDeviceList::parse_body(&body).expect("generated body parses");

        prop_assert_eq!(&parsed, &list);
        prop_assert_eq!(parsed.serialize_body().expect("parsed list serializes"), body);
    }

    #[test]
    fn all_strict_prefixes_of_valid_bodies_reject(list in signed_device_list_strategy()) {
        let body = list.serialize_body().expect("generated list serializes");

        for cut in 0..body.len() {
            prop_assert!(SignedDeviceList::parse_body(&body[..cut]).is_err());
        }
    }

    #[test]
    fn arbitrary_body_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..=8192)) {
        drop(SignedDeviceList::parse_body(&bytes));
    }

    #[test]
    fn generated_envelopes_verify(envelope in signed_envelope_strategy()) {
        prop_assert_eq!(
            verify_signed_device_list(
                &envelope.body_bytes,
                &envelope.signature,
                shared_keypair().public_key().as_ref(),
            )
            .expect("generated envelope verifies"),
            envelope.list
        );
    }

    #[test]
    fn tampered_generated_body_fails_signature_verification(
        (envelope, index) in signed_envelope_with_body_index_strategy(),
    ) {
        let mut body = envelope.body_bytes.clone();
        body[index] ^= 0x01;

        prop_assert!(
            verify_signed_device_list(
                &body,
                &envelope.signature,
                shared_keypair().public_key().as_ref(),
            )
            .is_err()
        );
    }

    #[test]
    fn tampered_generated_signature_fails_verification(
        (envelope, index) in signed_envelope_with_signature_index_strategy(),
    ) {
        let mut signature = envelope.signature.clone();
        signature[index] ^= 0x01;

        prop_assert!(
            verify_signed_device_list(
                &envelope.body_bytes,
                &signature,
                shared_keypair().public_key().as_ref(),
            )
            .is_err()
        );
    }

    #[test]
    fn certificate_domain_signature_does_not_verify_as_list(list in signed_device_list_strategy()) {
        let body = list.serialize_body().expect("generated list serializes");
        let signature = sign_body_with_domain(&body, DEVICE_CERT_V2, shared_keypair());

        prop_assert!(
            verify_signed_device_list(
                &body,
                &signature,
                shared_keypair().public_key().as_ref(),
            )
            .is_err()
        );
    }
}
