use super::*;
use crate::room_crypto::tests::signing_identity;

const USER: &str = "11111111-1111-4111-8111-111111111111";

fn signed_endorsement(
    identity: &IdentityBundleView,
    key: &[u8],
    digest: &str,
    device: &str,
) -> ArtifactEndorsement {
    let preimage = endorsement_preimage(
        &identity.user_id,
        &identity.identity_incarnation_id,
        digest,
        device,
    )
    .unwrap();
    ArtifactEndorsement {
        user_id: identity.user_id.clone(),
        identity_incarnation_id: identity.identity_incarnation_id,
        artifact_digest: digest.to_owned(),
        endorser_device_id: device.to_owned(),
        signature: BASE64
            .encode(kodosi_backend_client::crypto::sign_control_message(key, &preimage).unwrap()),
    }
}

#[test]
fn endorsement_accepts_only_current_original_author_and_exact_artifact() {
    let (identity, key) = signing_identity(USER, "surviving-device");
    let digest = artifact_digest(
        USER,
        &identity.identity_incarnation_id,
        "revoked-device",
        b"roster-body",
        b"signature",
    )
    .unwrap();
    let endorsement = signed_endorsement(&identity, &key, &digest, "surviving-device");
    verify_endorsement(&identity, &digest, &endorsement).unwrap();

    let changed = artifact_digest(
        USER,
        &identity.identity_incarnation_id,
        "revoked-device",
        b"roster-body",
        b"new-signature",
    )
    .unwrap();
    assert!(verify_endorsement(&identity, &changed, &endorsement).is_err());
    let mut reset = identity.clone();
    reset.identity_incarnation_id = uuid::Uuid::now_v7();
    assert!(verify_endorsement(&reset, &digest, &endorsement).is_err());
    let mut revoked = identity.clone();
    revoked.historical_devices = std::mem::take(&mut revoked.devices);
    assert!(verify_endorsement(&revoked, &digest, &endorsement).is_err());
    let mut wrong_user = endorsement.clone();
    wrong_user.user_id = "22222222-2222-4222-8222-222222222222".to_owned();
    assert!(verify_endorsement(&identity, &digest, &wrong_user).is_err());
    let mut forged = endorsement;
    forged.signature = BASE64.encode(vec![0; 3309]);
    assert!(verify_endorsement(&identity, &digest, &forged).is_err());
}

#[test]
fn owner_cannot_endorse_an_invitees_consent() {
    let (invitee, _) = signing_identity(USER, "invitee-device");
    let (owner, owner_key) =
        signing_identity("22222222-2222-4222-8222-222222222222", "owner-device");
    let digest = artifact_digest(
        USER,
        &invitee.identity_incarnation_id,
        "old-invitee",
        b"decision",
        b"signature",
    )
    .unwrap();
    let endorsement = signed_endorsement(&owner, &owner_key, &digest, "owner-device");
    assert!(verify_endorsement(&invitee, &digest, &endorsement).is_err());
}
