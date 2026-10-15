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
    person_with(Uuid::now_v7().to_string(), None)
}

fn person_with(user: String, list_expires_at_ms: Option<u64>) -> (DeviceKeys, IdentityBundle) {
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
        list_expires_at_ms,
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

fn room_of(
    owner: &(DeviceKeys, IdentityBundle),
    member: &IdentityBundle,
) -> (State, VerifiedState) {
    let (keys, identity) = owner;
    let mut state = State {
        room_id: Uuid::now_v7(),
        owner_user_id: identity.user_id.clone(),
        author_id: identity.user_id.clone(),
        device_id: keys.device_id.clone(),
        version: 1,
        epoch: 1,
        created_at_ms: 2000,
        previous_hash: String::new(),
        members: BTreeMap::from([
            (identity.user_id.clone(), identity.clone()),
            (member.user_id.clone(), member.clone()),
        ]),
        recipients: vec![],
        previous_key: None,
    };
    state.recipients.push(
        wrap(
            &state,
            &identity.user_id,
            &keys.device_id,
            keys.room_public(),
            &secret().unwrap(),
        )
        .unwrap(),
    );
    let verified = verify_state(
        &signed(&state, keys),
        state.room_id,
        &identity.user_id,
        &identity.root().unwrap(),
        None,
    )
    .unwrap();
    (state, verified)
}

#[test]
fn a_room_changes_while_the_device_list_of_an_absent_member_is_past_its_time() {
    let alice = person();
    let (_, bob) = person_with(Uuid::now_v7().to_string(), Some(3000));
    let (first, verified) = room_of(&alice, &bob);
    let (owner, anchor) = (alice.1.user_id.clone(), alice.1.root().unwrap());
    let mut later = first;
    later.version = 2;
    later.created_at_ms = 5000;
    later.previous_hash = verified.hash.clone();
    let second = verify_state(
        &signed(&later, &alice.0),
        later.room_id,
        &owner,
        &anchor,
        Some(&verified),
    )
    .unwrap();
    assert!(second.identities.contains_key(&bob.user_id));

    let (carol_keys, carol) = person_with(Uuid::now_v7().to_string(), Some(4000));
    let mut joined = later;
    joined.version = 3;
    joined.previous_hash = second.hash.clone();
    joined.members.insert(carol.user_id.clone(), carol);
    drop(carol_keys);
    assert!(
        verify_state(
            &signed(&joined, &alice.0),
            joined.room_id,
            &owner,
            &anchor,
            Some(&second)
        )
        .is_err(),
        "a new member needs a current device list"
    );
}

#[test]
fn only_the_owner_gives_a_member_a_new_identity_and_only_with_new_room_keys() {
    let alice = person();
    let (bob_keys, bob) = person();
    let (first, verified) = room_of(&alice, &bob);
    let (owner, anchor) = (alice.1.user_id.clone(), alice.1.root().unwrap());
    let (_, bob_again) = person_with(bob.user_id.clone(), None);
    assert_ne!(bob_again.root().unwrap(), bob.root().unwrap());
    let mut replaced = first;
    replaced.version = 2;
    replaced.previous_hash = verified.hash.clone();
    replaced.members.insert(bob.user_id.clone(), bob_again);
    let check = |state: &State, keys: &DeviceKeys| {
        verify_state(
            &signed(state, keys),
            state.room_id,
            &owner,
            &anchor,
            Some(&verified),
        )
    };
    assert!(
        check(&replaced, &alice.0).is_err(),
        "the room keys did not change"
    );
    replaced.epoch = 2;
    let mut by_member = replaced.clone();
    by_member.author_id.clone_from(&bob.user_id);
    by_member.device_id.clone_from(&bob_keys.device_id);
    assert!(check(&by_member, &bob_keys).is_err());
    let accepted = check(&replaced, &alice.0).unwrap();
    assert_eq!(
        accepted.identities[&bob.user_id].root,
        replaced.members[&bob.user_id].root().unwrap()
    );
}

#[test]
fn a_room_key_wrap_opens_only_with_both_parts_and_for_its_device() {
    let (alice, identity) = person();
    let user = identity.user_id.clone();
    let key = secret().unwrap();
    let state = State {
        room_id: Uuid::now_v7(),
        owner_user_id: user.clone(),
        author_id: user.clone(),
        device_id: alice.device_id.clone(),
        version: 1,
        epoch: 1,
        created_at_ms: 2000,
        previous_hash: String::new(),
        members: BTreeMap::from([(user.clone(), identity)]),
        recipients: vec![],
        previous_key: None,
    };
    let mut current = state;
    current
        .recipients
        .push(wrap(&current, &user, &alice.device_id, alice.room_public(), &key).unwrap());
    assert_eq!(
        unwrap(&current, &user, &alice).unwrap().as_ref(),
        key.as_ref()
    );
    let bytes = BASE64
        .decode(&current.recipients[0].kem_ciphertext)
        .unwrap();
    assert_eq!(bytes.len(), 1088 + 32);
    for part in [0, 1088] {
        let mut changed = bytes.clone();
        changed[part] ^= 1;
        let mut other = current.clone();
        other.recipients[0].kem_ciphertext = BASE64.encode(changed);
        assert!(unwrap(&other, &user, &alice).is_err());
    }
    let (other, _) = person();
    let mut renamed = current;
    renamed.recipients[0].device_id.clone_from(&other.device_id);
    assert!(unwrap(&renamed, &user, &other).is_err());
}
