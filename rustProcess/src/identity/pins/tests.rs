use super::*;
use crate::identity::{
    device_cert::{SignedDeviceCertificate, build_cert_for, build_self_cert},
    keys::DeviceKeys,
    signed_device_list::{
        DeviceListEntry, SignedDeviceListEnvelope, build_bootstrap_list, build_replacement_list,
    },
};

fn certificate(signed: SignedDeviceCertificate) -> CertificateEnvelope {
    CertificateEnvelope {
        certificate: BASE64.encode(signed.body_bytes),
        certificate_signature: BASE64.encode(signed.signature),
    }
}
fn list(signed: &SignedDeviceListEnvelope) -> DeviceListEnvelope {
    DeviceListEnvelope {
        body: BASE64.encode(&signed.body_bytes),
        signature: BASE64.encode(&signed.signature),
    }
}
struct Fixture {
    root: tempfile::TempDir,
    user: String,
    a: DeviceKeys,
    b: DeviceKeys,
    cert_a: CertificateEnvelope,
    cert_b: CertificateEnvelope,
    initial: SignedDeviceListEnvelope,
    both: SignedDeviceListEnvelope,
    bundle: IdentityBundle,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let user = Uuid::now_v7().to_string();
        let a = DeviceKeys::generate().unwrap();
        let b = DeviceKeys::generate().unwrap();
        let cert_a = certificate(
            build_self_cert(
                &user,
                &a.device_id,
                "a",
                a.kem_public(),
                &a.signing_key().unwrap(),
                1000,
                None,
            )
            .unwrap(),
        );
        let cert_b = certificate(
            build_cert_for(
                &user,
                &b.device_id,
                "b",
                b.kem_public(),
                b.signing_public(),
                &a.device_id,
                &a.signing_key().unwrap(),
                1100,
                None,
            )
            .unwrap(),
        );
        let initial = build_bootstrap_list(
            &user,
            &a.device_id,
            &a.signing_key().unwrap(),
            1000,
            Some(20_000),
        )
        .unwrap();
        let mut entries = initial.list.entries.clone();
        entries.push(DeviceListEntry {
            device_id: b.device_id.clone(),
            signer_device_id: a.device_id.clone(),
        });
        let both = build_replacement_list(
            &user,
            1,
            &initial.list.entries,
            entries,
            &a.device_id,
            &a.signing_key().unwrap(),
            1100,
            Some(20_000),
        )
        .unwrap();
        let bundle = IdentityBundle {
            user_id: user.clone(),
            identity_revision: 1,
            identity_incarnation_id: Uuid::now_v7(),
            device_list: list(&initial),
            devices: vec![cert_a.clone()],
            certificate_chain: vec![],
        };
        Self {
            root,
            user,
            a,
            b,
            cert_a,
            cert_b,
            initial,
            both,
            bundle,
        }
    }
    fn pins(&self) -> Pins {
        Pins::load(self.root.path().join("pins.json")).unwrap()
    }
    fn with_both(&self) -> IdentityBundle {
        IdentityBundle {
            device_list: list(&self.both),
            devices: vec![self.cert_a.clone(), self.cert_b.clone()],
            ..self.bundle.clone()
        }
    }
    fn only_b(&self) -> IdentityBundle {
        let signed = build_replacement_list(
            &self.user,
            2,
            &self.both.list.entries,
            vec![self.both.list.entries[1].clone()],
            &self.b.device_id,
            &self.b.signing_key().unwrap(),
            1200,
            Some(20_000),
        )
        .unwrap();
        IdentityBundle {
            device_list: list(&signed),
            devices: vec![self.cert_b.clone()],
            certificate_chain: vec![self.cert_a.clone()],
            ..self.bundle.clone()
        }
    }
}

#[test]
fn inactive_ancestor_verifies_but_never_receives_a_key() {
    let fixture = Fixture::new();
    let verified = fixture
        .pins()
        .verify(&fixture.only_b(), true, 2000)
        .unwrap();
    assert_eq!(verified.devices.len(), 1);
    assert!(verified.devices.contains_key(&fixture.b.device_id));
    assert!(!verified.devices.contains_key(&fixture.a.device_id));
}

#[test]
fn revocation_and_generation_survive_reload() {
    let fixture = Fixture::new();
    let mut pins = fixture.pins();
    pins.verify(&fixture.with_both(), true, 2000).unwrap();
    let removed = fixture.only_b();
    pins.verify(&removed, false, 2000).unwrap();
    let mut pins = fixture.pins();
    assert!(pins.verify(&fixture.with_both(), false, 2000).is_err());
    let previous =
        SignedDeviceList::parse_body(&decode(&removed.device_list.body, 527_432).unwrap()).unwrap();
    let readded = build_replacement_list(
        &fixture.user,
        previous.generation,
        &previous.entries,
        fixture.both.list.entries.clone(),
        &fixture.b.device_id,
        &fixture.b.signing_key().unwrap(),
        1300,
        Some(20_000),
    )
    .unwrap();
    let bad = IdentityBundle {
        device_list: list(&readded),
        devices: vec![fixture.cert_a, fixture.cert_b],
        certificate_chain: vec![],
        ..removed
    };
    assert!(pins.verify(&bad, false, 2000).is_err());
}

#[test]
fn skipped_enrollment_can_chain_to_retained_pin() {
    let fixture = Fixture::new();
    let mut pins = fixture.pins();
    pins.verify(&fixture.bundle, true, 2000).unwrap();
    let verified = pins.verify(&fixture.only_b(), false, 2000).unwrap();
    assert_eq!(verified.devices.len(), 1);
}

#[test]
fn identity_replacement_and_same_generation_equivocation_fail_closed() {
    let fixture = Fixture::new();
    let mut pins = fixture.pins();
    pins.verify(&fixture.bundle, true, 2000).unwrap();
    let mut changed = fixture.bundle.clone();
    changed.identity_incarnation_id = Uuid::now_v7();
    assert!(pins.verify(&changed, true, 2000).is_err());
    let changed_list = build_bootstrap_list(
        &fixture.user,
        &fixture.a.device_id,
        &fixture.a.signing_key().unwrap(),
        1200,
        Some(30_000),
    )
    .unwrap();
    let changed = IdentityBundle {
        device_list: list(&changed_list),
        ..fixture.bundle.clone()
    };
    assert!(pins.verify(&changed, true, 2000).is_err());
    assert_eq!(
        pins.verify(&fixture.bundle, false, 2000)
            .unwrap()
            .generation,
        1
    );
}

#[test]
fn expired_pin_is_renewal_only_and_first_use_is_explicit() {
    let fixture = Fixture::new();
    let mut pins = fixture.pins();
    assert!(pins.verify(&fixture.bundle, false, 2000).is_err());
    assert!(pins.verify_for_renewal(&fixture.bundle, 30_000).is_err());
    pins.verify(&fixture.bundle, true, 2000).unwrap();
    assert!(pins.verify(&fixture.bundle, false, 30_000).is_err());
    assert_eq!(
        pins.verify_for_renewal(&fixture.bundle, 30_000)
            .unwrap()
            .generation,
        1
    );
    assert_eq!(fixture.initial.list.generation, 1);
}

#[test]
fn malformed_and_foreign_ancestry_is_rejected() {
    let fixture = Fixture::new();
    let mut bundle = fixture.only_b();
    bundle.certificate_chain[0].certificate_signature = BASE64.encode([0; 3309]);
    assert!(fixture.pins().verify(&bundle, true, 2000).is_err());
    let mut bundle = fixture.only_b();
    bundle.user_id = Uuid::now_v7().to_string();
    assert!(fixture.pins().verify(&bundle, true, 2000).is_err());
}

#[test]
fn own_identity_replacement_needs_an_explicit_forget_before_first_use() {
    let fixture = Fixture::new();
    let now = 1_500;
    let mut pins = fixture.pins();
    pins.verify(&fixture.bundle, true, now).unwrap();
    let mut replaced = fixture.bundle.clone();
    replaced.identity_incarnation_id = Uuid::now_v7();
    assert!(!pins.own_identity_replaced(&fixture.bundle));
    assert!(pins.own_identity_replaced(&replaced));
    assert!(matches!(
        pins.verify_for_renewal(&replaced, now),
        Err(Error::Trust(_))
    ));
    pins.forget(&fixture.user).unwrap();
    assert!(!pins.contains(&fixture.user));
    pins.verify(&replaced, true, now).unwrap();
    let reloaded = fixture.pins();
    assert!(reloaded.contains(&fixture.user));
    assert!(!reloaded.own_identity_replaced(&replaced));
    assert!(reloaded.own_identity_replaced(&fixture.bundle));
}
