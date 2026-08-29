use super::*;
use crate::identity_core::{
    device_cert::build_self_cert,
    identity_bundle_view::{IdentityBundleInput, IdentityDeviceInput, IdentityDeviceListInput},
    signed_device_list::{
        DeviceListEntry, SignedDeviceList, build_bootstrap_list, build_replacement_list,
    },
    wire_codec::ML_DSA_65_SIGNATURE_LEN,
};
use aws_lc_rs::signature::{KeyPair, ML_DSA_65_SIGNING, PqdsaKeyPair, PqdsaSigningAlgorithm};

const ML_DSA_65_ALGO: &PqdsaSigningAlgorithm = &ML_DSA_65_SIGNING;
const TEST_USER_ID: &str = "01900000-0000-7000-8000-000000000001";
const SECOND_TEST_USER_ID: &str = "01900000-0000-7000-8000-000000000002";

#[derive(Clone)]
struct UserIdentityBundleDto {
    user_id: String,
    device_list: UserDeviceListDto,
    devices: Vec<UserDeviceCertificateDto>,
    historical_devices: Vec<UserDeviceCertificateDto>,
}

#[derive(Clone)]
struct UserDeviceListDto {
    generation: u64,
    signer_device_id: String,
    issued_at_ms: u64,
    expires_at_ms: Option<u64>,
    body: String,
    signature: String,
}

#[derive(Clone)]
struct UserDeviceCertificateDto {
    device_id: String,
    kem_public_key: String,
    signing_public_key: String,
    certificate: String,
    certificate_signature: String,
}

fn build_bundle_view(dto: &UserIdentityBundleDto) -> Result<IdentityBundleView> {
    let active = IdentityBundleInput {
        user_id: dto.user_id.clone(),
        device_list: IdentityDeviceListInput {
            generation: dto.device_list.generation,
            signer_device_id: dto.device_list.signer_device_id.clone(),
            issued_at_ms: dto.device_list.issued_at_ms,
            expires_at_ms: dto.device_list.expires_at_ms,
            body: dto.device_list.body.clone(),
            signature: dto.device_list.signature.clone(),
        },
        devices: dto.devices.iter().map(device_input).collect(),
        historical_devices: dto.historical_devices.iter().map(device_input).collect(),
    };
    let historical = dto
        .historical_devices
        .iter()
        .map(
            |device| crate::identity_core::identity_bundle_view::HistoricalIdentityDeviceInput {
                device: device_input(device),
            },
        )
        .collect::<Vec<_>>();
    crate::identity_core::identity_bundle_view::build_bundle_view_with_historical(
        &active,
        &historical,
    )
}

fn device_input(device: &UserDeviceCertificateDto) -> IdentityDeviceInput {
    IdentityDeviceInput {
        device_id: device.device_id.clone(),
        kem_public_key: device.kem_public_key.clone(),
        signing_public_key: device.signing_public_key.clone(),
        certificate: device.certificate.clone(),
        certificate_signature: device.certificate_signature.clone(),
    }
}

#[test]
fn persisted_identity_proof_shape_excludes_lifecycle_metadata() {
    let (bundle, _, _) = make_bootstrap_bundle("01900000-0000-7000-8000-000000000001");
    let input = IdentityBundleInput {
        user_id: bundle.user_id,
        device_list: IdentityDeviceListInput {
            generation: bundle.device_list.generation,
            signer_device_id: bundle.device_list.signer_device_id,
            issued_at_ms: bundle.device_list.issued_at_ms,
            expires_at_ms: bundle.device_list.expires_at_ms,
            body: bundle.device_list.body,
            signature: bundle.device_list.signature,
        },
        devices: bundle
            .devices
            .into_iter()
            .map(|device| IdentityDeviceInput {
                device_id: device.device_id,
                kem_public_key: device.kem_public_key,
                signing_public_key: device.signing_public_key,
                certificate: device.certificate,
                certificate_signature: device.certificate_signature,
            })
            .collect(),
        historical_devices: Vec::new(),
    };
    let value = serde_json::to_value(&input).unwrap();
    assert!(value.get("identityRevision").is_none());
    assert!(value.get("identityIncarnationId").is_none());
    assert_eq!(
        value
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["deviceList", "devices", "userId"]
    );
}

fn fresh_signing_keypair() -> PqdsaKeyPair {
    PqdsaKeyPair::generate(ML_DSA_65_ALGO).expect("generate ML-DSA-65 keypair")
}

fn fake_kem_pub() -> Vec<u8> {
    (0..1184u32).map(|i| (i % 251) as u8).collect()
}

fn sign_raw_list(list: SignedDeviceList, signer: &PqdsaKeyPair) -> (Vec<u8>, Vec<u8>) {
    let body = list.serialize_body().expect("serialize list body");
    let mut payload =
        Vec::with_capacity(kodosi_domain::domain_tags::DEVICE_LIST_V1.len() + body.len());
    payload.extend_from_slice(kodosi_domain::domain_tags::DEVICE_LIST_V1);
    payload.extend_from_slice(&body);
    let mut sig = vec![0u8; ML_DSA_65_SIGNATURE_LEN];
    let sig_len = signer.sign(&payload, &mut sig).expect("sign");
    sig.truncate(sig_len);
    (body, sig)
}

fn make_bootstrap_bundle(user_id: &str) -> (UserIdentityBundleDto, PqdsaKeyPair, String) {
    let keypair = fresh_signing_keypair();
    let kem_pub = fake_kem_pub();
    let sig_pub = keypair.public_key().as_ref().to_vec();
    let device_id = format!("{user_id}-dev-1");
    let issued = 1_700_000_000_000;
    let cert = build_self_cert(
        user_id,
        &device_id,
        "Test MacBook",
        &kem_pub,
        &keypair,
        issued,
        None,
    )
    .expect("build self cert");
    let list = build_bootstrap_list(user_id, &device_id, &keypair, issued, None)
        .expect("build bootstrap list");

    let dto = UserIdentityBundleDto {
        user_id: user_id.to_owned(),
        device_list: UserDeviceListDto {
            generation: 1,
            signer_device_id: device_id.clone(),
            issued_at_ms: issued,
            expires_at_ms: None,
            body: BASE64.encode(&list.body_bytes),
            signature: BASE64.encode(&list.signature),
        },
        devices: vec![UserDeviceCertificateDto {
            device_id: device_id.clone(),
            kem_public_key: BASE64.encode(&kem_pub),
            signing_public_key: BASE64.encode(&sig_pub),
            certificate: BASE64.encode(&cert.body_bytes),
            certificate_signature: BASE64.encode(&cert.signature),
        }],
        historical_devices: vec![],
    };
    (dto, keypair, device_id)
}

fn add_second_device(
    bundle: &mut UserIdentityBundleDto,
    existing_device_id: &str,
    existing_keypair: &PqdsaKeyPair,
) -> (PqdsaKeyPair, String) {
    let keypair = fresh_signing_keypair();
    let kem_pub = fake_kem_pub();
    let sig_pub = keypair.public_key().as_ref().to_vec();
    let new_id = format!("{}-dev-2", bundle.user_id);
    let issued = 1_700_000_100_000;

    let cert = crate::identity_core::device_cert::build_cert_for(
        &bundle.user_id,
        &new_id,
        "Test iPad",
        &kem_pub,
        &sig_pub,
        existing_device_id,
        existing_keypair,
        issued,
        None,
    )
    .expect("build cross-signed cert");

    let previous_entries = vec![DeviceListEntry {
        device_id: existing_device_id.to_owned(),
        signer_device_id: existing_device_id.to_owned(),
    }];
    let mut next_entries = previous_entries.clone();
    next_entries.push(DeviceListEntry {
        device_id: new_id.clone(),
        signer_device_id: existing_device_id.to_owned(),
    });
    let list = build_replacement_list(
        &bundle.user_id,
        1,
        &previous_entries,
        next_entries,
        existing_device_id,
        existing_keypair,
        issued,
        None,
    )
    .expect("build replacement list");

    bundle.device_list = UserDeviceListDto {
        generation: 2,
        signer_device_id: existing_device_id.to_owned(),
        issued_at_ms: issued,
        expires_at_ms: None,
        body: BASE64.encode(&list.body_bytes),
        signature: BASE64.encode(&list.signature),
    };
    bundle.devices.push(UserDeviceCertificateDto {
        device_id: new_id.clone(),
        kem_public_key: BASE64.encode(&kem_pub),
        signing_public_key: BASE64.encode(&sig_pub),
        certificate: BASE64.encode(&cert.body_bytes),
        certificate_signature: BASE64.encode(&cert.signature),
    });

    (keypair, new_id)
}

fn make_store() -> (tempfile::TempDir, DeviceListPinStore) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("device-list-pins.json");
    let mut store =
        DeviceListPinStore::load_from_with_clock(path, || 1_700_000_000_000).expect("load");
    store.bind_to_user("test-owner").expect("bind test store");
    (dir, store)
}

fn revoke_dev1_keep_dev2(
    bundle: &mut UserIdentityBundleDto,
    dev1_id: &str,
    dev2_id: &str,
    dev2_keypair: &PqdsaKeyPair,
) -> UserDeviceCertificateDto {
    let issued = 1_700_000_300_000;
    let list = SignedDeviceList {
        user_id: bundle.user_id.clone(),
        generation: 3,
        entries: vec![DeviceListEntry {
            device_id: dev2_id.to_owned(),
            signer_device_id: dev1_id.to_owned(),
        }],
        signer_device_id: dev2_id.to_owned(),
        issued_at_ms: issued,
        expires_at_ms: None,
    };
    let body = list.serialize_body().unwrap();
    let mut payload =
        Vec::with_capacity(kodosi_domain::domain_tags::DEVICE_LIST_V1.len() + body.len());
    payload.extend_from_slice(kodosi_domain::domain_tags::DEVICE_LIST_V1);
    payload.extend_from_slice(&body);
    let mut sig = vec![0u8; ML_DSA_65_SIGNATURE_LEN];
    let sig_len = dev2_keypair.sign(&payload, &mut sig).unwrap();
    sig.truncate(sig_len);

    bundle.device_list = UserDeviceListDto {
        generation: 3,
        signer_device_id: dev2_id.to_owned(),
        issued_at_ms: issued,
        expires_at_ms: None,
        body: BASE64.encode(&body),
        signature: BASE64.encode(&sig),
    };
    let historical = bundle
        .devices
        .iter()
        .find(|device| device.device_id == dev1_id)
        .expect("revoked signer proof")
        .clone();
    bundle.devices.retain(|device| device.device_id != dev1_id);
    bundle.historical_devices.push(historical.clone());
    historical
}

mod actor;
mod attacks;
mod bootstrap;
mod bundle_view;
mod persistence;
mod tombstones;
mod updates;
