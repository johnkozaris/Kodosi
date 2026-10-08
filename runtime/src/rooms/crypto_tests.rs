use super::crypto::*;
use crate::identity::keys::DeviceKeys;
use crate::identity::{
    device_cert::build_self_cert,
    pins::{CertificateEnvelope, DeviceListEnvelope, IdentityBundle},
    signed_device_list::build_bootstrap_list,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use std::collections::BTreeMap;
use uuid::Uuid;

pub(crate) fn person() -> (DeviceKeys, IdentityBundle) {
    let user = Uuid::now_v7().to_string();
    let keys = DeviceKeys::generate().unwrap();
    let certificate = build_self_cert(
        &user,
        &keys.device_id,
        "Room device",
        &keys.signing_key().unwrap(),
        1000,
        None,
    )
    .unwrap();
    let list = build_bootstrap_list(
        &user,
        &keys.device_id,
        &keys.signing_key().unwrap(),
        1000,
        None,
    )
    .unwrap();
    let bundle = IdentityBundle {
        user_id: user,
        identity_revision: 1,
        identity_incarnation_id: Uuid::now_v7(),
        device_list: DeviceListEnvelope {
            body: BASE64.encode(list.body_bytes),
            signature: BASE64.encode(list.signature),
        },
        devices: vec![CertificateEnvelope {
            certificate: BASE64.encode(certificate.body_bytes),
            certificate_signature: BASE64.encode(certificate.signature),
        }],
        certificate_chain: vec![],
    };
    (keys, bundle)
}

fn signed(state: &State, keys: &DeviceKeys) -> SignedState {
    let (body, signature) = sign(STATE_DOMAIN, keys, state).unwrap();
    SignedState {
        version: state.version,
        body,
        signature,
    }
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one encrypted history membership transition"
)]
fn late_members_read_the_full_history_and_departed_members_cannot_open_future_content() {
    let (alice, alice_identity) = person();
    let (bob, bob_identity) = person();
    let room = Uuid::now_v7();
    let owner = alice_identity.user_id.clone();
    let anchor = alice_identity.root().unwrap();
    let first_key = secret().unwrap();
    let mut first = State {
        room_id: room,
        owner_user_id: owner.clone(),
        author_id: owner.clone(),
        device_id: alice.device_id.clone(),
        version: 1,
        epoch: 1,
        created_at_ms: 2000,
        previous_hash: String::new(),
        members: BTreeMap::from([(owner.clone(), alice_identity)]),
        recipients: vec![],
        previous_key: None,
    };
    first.recipients.push(
        wrap(
            &first,
            &owner,
            &alice.device_id,
            alice.room_public(),
            &first_key,
        )
        .unwrap(),
    );
    let first_verified =
        verify_state(&signed(&first, &alice), room, &owner, &anchor, None).unwrap();
    let mut message = Content {
        room_id: room,
        id: Uuid::now_v7(),
        kind: "message".into(),
        version: 1,
        key_version: 1,
        epoch: 1,
        author_id: owner.clone(),
        device_id: alice.device_id.clone(),
        nonce: String::new(),
        ciphertext: String::new(),
    };
    message
        .encrypt(&first_key, b"A message posted before Bob joined")
        .unwrap();
    assert!(!message.ciphertext.contains("A message"));
    let mut joined = first.clone();
    joined.version = 2;
    joined.previous_hash = first_verified.hash.clone();
    joined
        .members
        .insert(bob_identity.user_id.clone(), bob_identity.clone());
    joined.recipients = vec![
        wrap(
            &joined,
            &owner,
            &alice.device_id,
            alice.room_public(),
            &first_key,
        )
        .unwrap(),
        wrap(
            &joined,
            &bob_identity.user_id,
            &bob.device_id,
            bob.room_public(),
            &first_key,
        )
        .unwrap(),
    ];
    let joined_verified = verify_state(
        &signed(&joined, &alice),
        room,
        &owner,
        &anchor,
        Some(&first_verified),
    )
    .unwrap();
    let bob_key = unwrap(&joined, &bob_identity.user_id, &bob).unwrap();
    assert_eq!(
        message.decrypt(&bob_key).unwrap().as_slice(),
        b"A message posted before Bob joined"
    );
    let second_key = secret().unwrap();
    let mut departed = joined.clone();
    departed.version = 3;
    departed.epoch = 2;
    departed.previous_hash = joined_verified.hash.clone();
    departed.members.remove(&bob_identity.user_id);
    departed.previous_key = Some(seal_previous(room, 2, &second_key, &first_key).unwrap());
    departed.recipients = vec![
        wrap(
            &departed,
            &owner,
            &alice.device_id,
            alice.room_public(),
            &second_key,
        )
        .unwrap(),
    ];
    verify_state(
        &signed(&departed, &alice),
        room,
        &owner,
        &anchor,
        Some(&joined_verified),
    )
    .unwrap();
    assert!(unwrap(&departed, &bob_identity.user_id, &bob).is_err());
    assert_eq!(
        open_previous(&departed, &second_key).unwrap().as_ref(),
        first_key.as_ref()
    );
    message.epoch = 2;
    message.key_version = 3;
    message.encrypt(&second_key, b"New room content").unwrap();
    assert!(message.decrypt(&bob_key).is_err());
    assert_eq!(
        message.decrypt(&second_key).unwrap().as_slice(),
        b"New room content"
    );
    message.room_id = Uuid::now_v7();
    assert!(message.decrypt(&second_key).is_err());
}

#[test]
fn member_cannot_insert_another_identity_or_rewrite_the_room_history() {
    let (alice, alice_identity) = person();
    let (bob, bob_identity) = person();
    let (_, charlie_identity) = person();
    let owner = alice_identity.user_id.clone();
    let anchor = alice_identity.root().unwrap();
    let key = secret().unwrap();
    let mut state = State {
        room_id: Uuid::now_v7(),
        owner_user_id: owner.clone(),
        author_id: owner.clone(),
        device_id: alice.device_id.clone(),
        version: 1,
        epoch: 1,
        created_at_ms: 2000,
        previous_hash: String::new(),
        members: BTreeMap::from([
            (owner.clone(), alice_identity),
            (bob_identity.user_id.clone(), bob_identity.clone()),
        ]),
        recipients: vec![],
        previous_key: None,
    };
    state
        .recipients
        .push(wrap(&state, &owner, &alice.device_id, alice.room_public(), &key).unwrap());
    let verified = verify_state(
        &signed(&state, &alice),
        state.room_id,
        &owner,
        &anchor,
        None,
    )
    .unwrap();
    state.version = 2;
    state.previous_hash = verified.hash.clone();
    state.author_id = bob_identity.user_id;
    state.device_id = bob.device_id.clone();
    state
        .members
        .insert(charlie_identity.user_id.clone(), charlie_identity);
    assert!(
        verify_state(
            &signed(&state, &bob),
            state.room_id,
            &owner,
            &anchor,
            Some(&verified)
        )
        .is_err()
    );
    let mut tampered = signed(&verified.state, &alice);
    tampered.body = BASE64.encode(b"{}");
    assert!(verify_state(&tampered, state.room_id, &owner, &anchor, None).is_err());
}
