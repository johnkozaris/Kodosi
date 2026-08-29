use std::collections::BTreeSet;

use aws_lc_rs::signature::{KeyPair, ML_DSA_65_SIGNING, PqdsaKeyPair};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

use crate::{
    AppError, Result,
    identity_core::{
        device_cert::DeviceCertificate,
        identity_bundle_view::{IdentityBundleView, VerifiedDevice},
        signed_device_list::SignedDeviceList,
    },
};

use super::{
    ROOM_INVITATION_DECISION_VERSION, ROOM_INVITATION_PROPOSAL_VERSION, ROOM_ROSTER_VERSION,
    RoomInvitationDecision, RoomRosterPin, RoomRosterSubmission, RosterPinTransition,
    SignedRoomInvitationDecision, SignedRoomInvitationProposal, SignedRoomRoster, domain_preimage,
    ensure_expected_roster_generation, ensure_sender_is_room_member, load_roster_pin_file_locked,
    migrate_v1_roster_pins, persist_roster_pin_at, plan_roster_history,
    require_roster_activation_proof, reset_roster_pins_for_account_at, sha256,
    validate_roster_activation_metadata, validate_roster_pin_transition, verify_identity_signature,
    verify_roster_activation_proof, with_roster_pin_lock,
};

fn committed_removal_roster(owner: &str, members: Vec<&str>) -> SignedRoomRoster {
    SignedRoomRoster {
        version: ROOM_ROSTER_VERSION,
        room_id: "room-1".to_owned(),
        generation: 8,
        owner_user_id: owner.to_owned(),
        member_user_ids: members.into_iter().map(str::to_owned).collect(),
        signer_device_id: "owner-device".to_owned(),
        issued_at_ms: 1,
    }
}

fn reconstruct_committed_removal(
    owner: &str,
    removed: &str,
    members: Vec<&str>,
) -> Result<RoomRosterSubmission> {
    let body =
        BASE64.encode(serde_json::to_vec(&committed_removal_roster(owner, members)).unwrap());
    RoomRosterSubmission::from_committed_removal_target(
        "room-1",
        "owner",
        removed,
        8,
        body,
        "signature".to_owned(),
        "owner-device".to_owned(),
    )
}

#[test]
fn committed_removal_target_reconstructs_exact_roster() {
    let roster = reconstruct_committed_removal("owner", "removed", vec!["owner", "remaining"])
        .expect("valid retained roster");
    assert_eq!(roster.generation, 8);
    assert_eq!(
        roster.member_user_ids,
        BTreeSet::from(["owner".to_owned(), "remaining".to_owned()])
    );
}

#[test]
fn committed_removal_target_rejects_membership_or_owner_tampering() {
    assert!(reconstruct_committed_removal("owner", "removed", vec!["owner", "removed"],).is_err());
    assert!(
        reconstruct_committed_removal("other-owner", "removed", vec!["other-owner", "remaining"],)
            .is_err()
    );
    assert!(
        reconstruct_committed_removal("owner", "removed", vec!["owner", "remaining", "remaining"],)
            .is_err()
    );
}

#[test]
fn roster_removal_requires_the_reviewed_generation() {
    assert!(ensure_expected_roster_generation(7, 7).is_ok());
    std::assert_matches!(
        ensure_expected_roster_generation(8, 7),
        Err(AppError::HttpProblem {
            status: 409,
            code: Some(code),
            ..
        }) if code == "CONCURRENT_MODIFICATION"
    );
}

#[test]
fn roster_pin_rejects_rollback_and_same_generation_rewrite() {
    let initial = RoomRosterPin {
        owner_user_id: "owner".to_owned(),
        generation: 2,
        body: "body-2".to_owned(),
        signature: "signature-2".to_owned(),
        member_user_ids: BTreeSet::from(["owner".to_owned(), "member".to_owned()]),
    };
    assert_eq!(
        validate_roster_pin_transition(None, &initial).expect("first roster pins"),
        RosterPinTransition::FirstSeen
    );
    assert_eq!(
        validate_roster_pin_transition(Some(&initial), &initial)
            .expect("identical roster is idempotent"),
        RosterPinTransition::Unchanged
    );

    let rollback = RoomRosterPin {
        owner_user_id: "owner".to_owned(),
        generation: 1,
        body: "body-1".to_owned(),
        signature: "signature-1".to_owned(),
        member_user_ids: BTreeSet::from(["owner".to_owned()]),
    };
    assert!(validate_roster_pin_transition(Some(&initial), &rollback).is_err());

    let rewrite = RoomRosterPin {
        owner_user_id: "owner".to_owned(),
        generation: 2,
        body: "forged".to_owned(),
        signature: "forged".to_owned(),
        member_user_ids: initial.member_user_ids.clone(),
    };
    assert!(validate_roster_pin_transition(Some(&initial), &rewrite).is_err());
}

#[test]
fn roster_pin_reports_member_additions() {
    let initial = RoomRosterPin {
        owner_user_id: "owner".to_owned(),
        generation: 2,
        body: "body-2".to_owned(),
        signature: "signature-2".to_owned(),
        member_user_ids: BTreeSet::from(["owner".to_owned()]),
    };
    let changed = RoomRosterPin {
        owner_user_id: "owner".to_owned(),
        generation: 3,
        body: "body-3".to_owned(),
        signature: "signature-3".to_owned(),
        member_user_ids: BTreeSet::from(["owner".to_owned(), "invitee".to_owned()]),
    };
    assert_eq!(
        validate_roster_pin_transition(Some(&initial), &changed)
            .expect("newer roster is inspected"),
        RosterPinTransition::Changed {
            added_members: BTreeSet::from(["invitee".to_owned()])
        }
    );
}

#[test]
fn v1_roster_pin_migration_preserves_members() {
    let roster = SignedRoomRoster {
        version: ROOM_ROSTER_VERSION,
        room_id: "room-1".to_owned(),
        generation: 2,
        owner_user_id: "owner".to_owned(),
        member_user_ids: vec!["owner".to_owned(), "member".to_owned()],
        signer_device_id: "owner-device".to_owned(),
        issued_at_ms: 1,
    };
    let body = BASE64.encode(serde_json::to_vec(&roster).expect("serialize roster"));
    let legacy = serde_json::json!({
        "version": 1,
        "accountUserId": "viewer",
        "pins": {
            "room-1": {
                "ownerUserId": "owner",
                "generation": 2,
                "body": body,
                "signature": "signature"
            }
        }
    });
    let migrated =
        migrate_v1_roster_pins(&serde_json::to_vec(&legacy).expect("serialize legacy pin file"))
            .expect("legacy pins migrate");
    assert_eq!(
        migrated.accounts["viewer"].pins["room-1"].member_user_ids,
        BTreeSet::from(["member".to_owned(), "owner".to_owned()])
    );

    let directory = tempfile::tempdir().expect("pin directory");
    let path = directory.path().join("room-roster-pins.json");
    std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    let loaded = with_roster_pin_lock(&path, || load_roster_pin_file_locked(&path)).unwrap();
    assert!(loaded.accounts["viewer"].pins.contains_key("room-1"));
    let on_disk: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(on_disk["version"], 3);
}

fn roster_pin(room_id: &str, owner_user_id: &str) -> RoomRosterPin {
    RoomRosterPin {
        owner_user_id: owner_user_id.to_owned(),
        generation: 1,
        body: format!("body-{room_id}"),
        signature: format!("signature-{room_id}"),
        member_user_ids: BTreeSet::from([owner_user_id.to_owned()]),
    }
}

#[test]
fn v2_roster_pin_migration_and_a_b_a_preserve_account_buckets() {
    let directory = tempfile::tempdir().expect("pin directory");
    let path = directory.path().join("room-roster-pins.json");
    let legacy = serde_json::json!({
        "version": 2,
        "accountUserId": "account-a",
        "pins": { "room-a": roster_pin("room-a", "owner-a") }
    });
    std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();

    let migrated = with_roster_pin_lock(&path, || load_roster_pin_file_locked(&path)).unwrap();
    assert!(migrated.accounts["account-a"].pins.contains_key("room-a"));
    let on_disk: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(on_disk["version"], 3);

    persist_roster_pin_at(
        &path,
        "account-b",
        "room-b",
        roster_pin("room-b", "owner-b"),
    )
    .unwrap();
    persist_roster_pin_at(
        &path,
        "account-a",
        "room-a-2",
        roster_pin("room-a-2", "owner-a"),
    )
    .unwrap();
    let file = with_roster_pin_lock(&path, || load_roster_pin_file_locked(&path)).unwrap();
    assert_eq!(file.accounts["account-a"].pins.len(), 2);
    assert_eq!(file.accounts["account-b"].pins.len(), 1);
}

#[test]
fn roster_identity_reset_only_removes_target_account_bucket() {
    let directory = tempfile::tempdir().expect("pin directory");
    let path = directory.path().join("room-roster-pins.json");
    for (account, room) in [("account-a", "room-a"), ("account-b", "room-b")] {
        persist_roster_pin_at(&path, account, room, roster_pin(room, account)).unwrap();
    }

    reset_roster_pins_for_account_at(&path, "account-a").unwrap();
    let file = with_roster_pin_lock(&path, || load_roster_pin_file_locked(&path)).unwrap();
    assert!(!file.accounts.contains_key("account-a"));
    assert!(file.accounts["account-b"].pins.contains_key("room-b"));
}

#[test]
fn concurrent_roster_v2_migration_and_other_account_update_preserve_both() {
    let directory = tempfile::tempdir().expect("pin directory");
    let path = directory.path().join("room-roster-pins.json");
    let legacy = serde_json::json!({
        "version": 2,
        "accountUserId": "account-a",
        "pins": { "room-a": roster_pin("room-a", "owner-a") }
    });
    std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));

    let migration_path = path.clone();
    let migration_barrier = std::sync::Arc::clone(&barrier);
    let migration = std::thread::spawn(move || {
        migration_barrier.wait();
        with_roster_pin_lock(&migration_path, || {
            load_roster_pin_file_locked(&migration_path).map(drop)
        })
        .unwrap();
    });
    let update_path = path.clone();
    let update_barrier = std::sync::Arc::clone(&barrier);
    let update = std::thread::spawn(move || {
        update_barrier.wait();
        persist_roster_pin_at(
            &update_path,
            "account-b",
            "room-b",
            roster_pin("room-b", "owner-b"),
        )
        .unwrap();
    });
    migration.join().unwrap();
    update.join().unwrap();

    let file = with_roster_pin_lock(&path, || load_roster_pin_file_locked(&path)).unwrap();
    assert!(file.accounts["account-a"].pins.contains_key("room-a"));
    assert!(file.accounts["account-b"].pins.contains_key("room-b"));
}

#[test]
fn roster_pin_file_rejects_corruption_and_future_versions() {
    let directory = tempfile::tempdir().expect("pin directory");
    let path = directory.path().join("room-roster-pins.json");
    std::fs::write(&path, "{ not json").unwrap();
    std::assert_matches!(
        with_roster_pin_lock(&path, || load_roster_pin_file_locked(&path)),
        Err(AppError::Json(_))
    );

    std::fs::write(&path, r#"{"version": 99, "accounts": {}}"#).unwrap();
    std::assert_matches!(
        with_roster_pin_lock(&path, || load_roster_pin_file_locked(&path)),
        Err(AppError::InvalidBackendData { field, .. })
            if field == "roomRosterPins.version"
    );
}

#[test]
fn concurrent_roster_pin_updates_preserve_distinct_rooms() {
    let directory = tempfile::tempdir().expect("pin directory");
    let path = directory.path().join("room-roster-pins.json");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut workers = Vec::new();
    for room_id in ["room-a", "room-b"] {
        let path = path.clone();
        let barrier = std::sync::Arc::clone(&barrier);
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            persist_roster_pin_at(
                &path,
                "viewer",
                room_id,
                RoomRosterPin {
                    owner_user_id: "owner".to_owned(),
                    generation: 1,
                    body: format!("body-{room_id}"),
                    signature: format!("signature-{room_id}"),
                    member_user_ids: BTreeSet::from(["owner".to_owned(), "viewer".to_owned()]),
                },
            )
            .expect("concurrent pin update");
        }));
    }
    for worker in workers {
        worker.join().expect("pin worker");
    }

    let file = super::load_roster_pin_file_locked(&path).expect("load pins");
    let pins = &file.accounts["viewer"].pins;
    assert_eq!(pins.len(), 2);
    assert!(pins.contains_key("room-a"));
    assert!(pins.contains_key("room-b"));
}

#[test]
fn encrypted_content_sender_must_be_a_verified_room_member() {
    let members = BTreeSet::from(["member".to_owned()]);
    ensure_sender_is_room_member(&members, "member").expect("member is accepted");
    assert!(ensure_sender_is_room_member(&members, "attacker").is_err());
}

#[test]
fn composite_activation_proof_matches_pinned_transition() {
    let fixture = activation_fixture();
    validate_roster_activation_metadata(
        &fixture.room,
        fixture
            .room
            .roster_activation_proof
            .as_ref()
            .expect("fixture has proof"),
        &fixture.proposal,
        &fixture.proposal_body,
        &fixture.decision,
        &fixture.roster_body,
        &fixture.roster_members,
        Some(&fixture.previous),
        &fixture.added_members,
        "member",
        fixture.now,
    )
    .expect("valid composite proof");
}

#[test]
fn member_addition_rejects_omitted_activation_proof() {
    let mut fixture = activation_fixture();
    fixture.room.roster_activation_proof = None;
    assert!(require_roster_activation_proof(&fixture.room).is_err());
}

#[test]
fn composite_activation_rejects_hash_drift_and_direct_add_relabel() {
    let mut fixture = activation_fixture();
    fixture.proposal.proposed_roster_body_hash = BASE64.encode([0u8; 32]);
    assert!(validate_activation(&fixture).is_err());

    let mut fixture = activation_fixture();
    fixture.added_members.insert("attacker".to_owned());
    assert!(validate_activation(&fixture).is_err());
}

#[test]
fn composite_activation_rejects_base_drift_wrong_signer_and_expired_decision() {
    let mut fixture = activation_fixture();
    fixture.previous.generation = 0;
    assert!(validate_activation(&fixture).is_err());

    let mut fixture = activation_fixture();
    fixture
        .room
        .roster_activation_proof
        .as_mut()
        .expect("fixture has proof")
        .proposal_signer_device_id = "wrong-device".to_owned();
    assert!(validate_activation(&fixture).is_err());

    let mut fixture = activation_fixture();
    fixture.decision.issued_at_ms = fixture.proposal.expires_at_ms.saturating_add(1);
    assert!(validate_activation(&fixture).is_err());
}

#[test]
fn composite_activation_rejects_declined_decision() {
    let mut fixture = activation_fixture();
    fixture.decision.decision = RoomInvitationDecision::Declined;
    assert!(validate_activation(&fixture).is_err());
}

#[test]
fn invitation_signature_rejects_forgery_and_wrong_device() {
    let keypair = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).expect("generate ML-DSA keypair");
    let pkcs8 = keypair.to_pkcs8v1().expect("serialize ML-DSA keypair");
    let public_key = keypair.public_key().as_ref().to_vec();
    let device_id = "owner-device";
    let certificate = DeviceCertificate {
        user_id: "owner".to_owned(),
        device_id: device_id.to_owned(),
        device_label: "Owner device".to_owned(),
        signer_device_id: device_id.to_owned(),
        kem_public_key: Vec::new(),
        sig_public_key: public_key.clone(),
        issued_at_ms: 1,
        expires_at_ms: None,
    };
    let identity = IdentityBundleView {
        user_id: "owner".to_owned(),
        identity_revision: 1,
        identity_incarnation_id: uuid::Uuid::from_u128(0x0190_0000_0000_7000_8000_0000_0000_0001),
        signed_list: SignedDeviceList {
            user_id: "owner".to_owned(),
            generation: 1,
            entries: Vec::new(),
            signer_device_id: device_id.to_owned(),
            issued_at_ms: 1,
            expires_at_ms: None,
        },
        list_body: Vec::new(),
        list_signature: Vec::new(),
        devices: std::collections::BTreeMap::from([(
            device_id.to_owned(),
            VerifiedDevice {
                certificate,
                sig_public_key: public_key,
                certificate_body: Vec::new(),
                certificate_signature: Vec::new(),
            },
        )]),
        historical_devices: std::collections::BTreeMap::new(),
    };
    let body = b"proposal";
    let signature = kodosi_backend_client::crypto::sign_control_message(
        pkcs8.as_ref(),
        &domain_preimage(
            kodosi_domain::domain_tags::ROOM_INVITATION_PROPOSAL_V1,
            body,
        ),
    )
    .expect("sign proposal");
    verify_identity_signature(
        &identity,
        device_id,
        kodosi_domain::domain_tags::ROOM_INVITATION_PROPOSAL_V1,
        body,
        &BASE64.encode(&signature),
        "proposal",
    )
    .expect("valid proposal signature");

    let mut forged = signature;
    forged[0] ^= 1;
    assert!(
        verify_identity_signature(
            &identity,
            device_id,
            kodosi_domain::domain_tags::ROOM_INVITATION_PROPOSAL_V1,
            body,
            &BASE64.encode(forged),
            "proposal",
        )
        .is_err()
    );
    assert!(
        verify_identity_signature(
            &identity,
            "wrong-device",
            kodosi_domain::domain_tags::ROOM_INVITATION_PROPOSAL_V1,
            body,
            &BASE64.encode([0]),
            "proposal",
        )
        .is_err()
    );
}

fn validate_activation(fixture: &ActivationFixture) -> super::Result<()> {
    validate_roster_activation_metadata(
        &fixture.room,
        fixture
            .room
            .roster_activation_proof
            .as_ref()
            .expect("fixture has proof"),
        &fixture.proposal,
        &fixture.proposal_body,
        &fixture.decision,
        &fixture.roster_body,
        &fixture.roster_members,
        Some(&fixture.previous),
        &fixture.added_members,
        "member",
        fixture.now,
    )
}

struct ActivationFixture {
    room: kodosi_backend_client::api::BackendRoom,
    proposal: SignedRoomInvitationProposal,
    proposal_body: Vec<u8>,
    decision: SignedRoomInvitationDecision,
    roster_body: Vec<u8>,
    roster_members: BTreeSet<String>,
    previous: RoomRosterPin,
    added_members: BTreeSet<String>,
    now: u64,
}

fn activation_fixture() -> ActivationFixture {
    let now = 2_000_000;
    let roster_body = b"signed-roster-generation-2".to_vec();
    let proposal = SignedRoomInvitationProposal {
        version: ROOM_INVITATION_PROPOSAL_VERSION,
        invitation_id: "invitation".to_owned(),
        room_id: "room".to_owned(),
        owner_user_id: "owner".to_owned(),
        invitee_user_id: "member".to_owned(),
        base_roster_generation: 1,
        proposed_roster_generation: 2,
        proposed_roster_body_hash: BASE64.encode(sha256(&roster_body)),
        expires_at_ms: 3_000_000,
        signer_device_id: "owner-device".to_owned(),
        issued_at_ms: 1_000_000,
    };
    let proposal_body = serde_json::to_vec(&proposal).expect("serialize proposal");
    let proposal_hash = sha256(&proposal_body);
    let decision = SignedRoomInvitationDecision {
        version: ROOM_INVITATION_DECISION_VERSION,
        invitation_id: "invitation".to_owned(),
        proposal_hash: BASE64.encode(&proposal_hash),
        room_id: "room".to_owned(),
        invitee_user_id: "member".to_owned(),
        decision: RoomInvitationDecision::Accepted,
        signer_device_id: "member-device".to_owned(),
        issued_at_ms: 1_500_000,
    };
    let proof = kodosi_backend_client::api::BackendRoomInvitationProof {
        invitation_id: "invitation".to_owned(),
        invitee_user_id: "member".to_owned(),
        proposal_body: BASE64.encode(&proposal_body),
        proposal_signature: BASE64.encode([1]),
        proposal_signer_device_id: "owner-device".to_owned(),
        proposal_hash: BASE64.encode(proposal_hash),
        decision_body: BASE64.encode(serde_json::to_vec(&decision).expect("serialize decision")),
        decision_signature: BASE64.encode([2]),
        decision_signer_device_id: "member-device".to_owned(),
        expires_at: time::OffsetDateTime::from_unix_timestamp_nanos(3_000_000_000_000)
            .expect("valid expiry"),
    };
    ActivationFixture {
        room: kodosi_backend_client::api::BackendRoom {
            id: "room".to_owned(),
            name: "Room".to_owned(),
            slug: "room".to_owned(),
            owner_user_id: "owner".to_owned(),
            roster_generation: 2,
            roster_body: BASE64.encode(&roster_body),
            roster_signature: BASE64.encode([3]),
            roster_signer_device_id: "owner-device".to_owned(),
            roster_activation_proof: Some(proof),
            admission_proofs: Vec::new(),
            roster_transitions: Vec::new(),
        },
        proposal,
        proposal_body,
        decision,
        roster_body,
        roster_members: BTreeSet::from(["member".to_owned(), "owner".to_owned()]),
        previous: RoomRosterPin {
            owner_user_id: "owner".to_owned(),
            generation: 1,
            body: "base".to_owned(),
            signature: "base-signature".to_owned(),
            member_user_ids: BTreeSet::from(["owner".to_owned()]),
        },
        added_members: BTreeSet::from(["member".to_owned()]),
        now,
    }
}

fn roster_entry_body(generation: i64, members: &[&str]) -> Vec<u8> {
    serde_json::to_vec(&SignedRoomRoster {
        version: ROOM_ROSTER_VERSION,
        room_id: "room".to_owned(),
        generation,
        owner_user_id: "owner".to_owned(),
        member_user_ids: members.iter().map(|id| (*id).to_owned()).collect(),
        signer_device_id: "owner-device".to_owned(),
        issued_at_ms: 1_000,
    })
    .expect("serialize roster")
}

fn history_room(
    generation: i64,
    transitions: Vec<(i64, Vec<&str>)>,
    proof_invitees: Vec<&str>,
) -> kodosi_backend_client::api::BackendRoom {
    let roster_transitions = transitions
        .into_iter()
        .map(|(entry_generation, members)| {
            let body = roster_entry_body(entry_generation, &members);
            kodosi_backend_client::api::BackendRoomRosterTransition {
                generation: entry_generation,
                roster_body: BASE64.encode(&body),
                roster_signature: BASE64.encode([1]),
                roster_signer_device_id: "owner-device".to_owned(),
            }
        })
        .collect::<Vec<_>>();
    let active = roster_transitions
        .last()
        .expect("at least one transition")
        .clone();
    let admission_proofs = proof_invitees
        .into_iter()
        .map(
            |invitee| kodosi_backend_client::api::BackendRoomInvitationProof {
                invitation_id: format!("invitation-{invitee}"),
                invitee_user_id: invitee.to_owned(),
                proposal_body: BASE64.encode(b"proposal"),
                proposal_signature: BASE64.encode([1]),
                proposal_signer_device_id: "owner-device".to_owned(),
                proposal_hash: BASE64.encode(sha256(b"proposal")),
                decision_body: BASE64.encode(b"decision"),
                decision_signature: BASE64.encode([2]),
                decision_signer_device_id: format!("{invitee}-device"),
                expires_at: time::OffsetDateTime::from_unix_timestamp_nanos(3_000_000_000_000)
                    .expect("valid expiry"),
            },
        )
        .collect();
    kodosi_backend_client::api::BackendRoom {
        id: "room".to_owned(),
        name: "Room".to_owned(),
        slug: "room".to_owned(),
        owner_user_id: "owner".to_owned(),
        roster_generation: generation,
        roster_body: active.roster_body,
        roster_signature: active.roster_signature,
        roster_signer_device_id: active.roster_signer_device_id,
        roster_activation_proof: None,
        admission_proofs,
        roster_transitions,
    }
}

fn pin_for(generation: i64, members: &[&str]) -> RoomRosterPin {
    let body = roster_entry_body(generation, members);
    RoomRosterPin {
        owner_user_id: "owner".to_owned(),
        generation,
        body: BASE64.encode(&body),
        signature: BASE64.encode([1]),
        member_user_ids: members.iter().map(|id| (*id).to_owned()).collect(),
    }
}

#[test]
fn roster_history_rejects_omitted_history() {
    let mut room = history_room(
        2,
        vec![(1, vec!["owner"]), (2, vec!["owner", "member"])],
        vec!["member"],
    );
    room.roster_transitions.clear();
    assert!(plan_roster_history(&room, None).is_err());
}

#[test]
fn roster_history_rejects_active_roster_spliced_from_another_proposal() {
    let mut body_splice = history_room(
        2,
        vec![(1, vec!["owner"]), (2, vec!["owner", "member"])],
        vec!["member"],
    );
    body_splice.roster_body = BASE64.encode(roster_entry_body(2, &["owner", "attacker"]));
    assert!(plan_roster_history(&body_splice, None).is_err());

    let mut signature_splice = history_room(
        2,
        vec![(1, vec!["owner"]), (2, vec!["owner", "member"])],
        vec!["member"],
    );
    signature_splice.roster_signature = BASE64.encode([9]);
    assert!(plan_roster_history(&signature_splice, None).is_err());

    let mut signer_splice = history_room(
        2,
        vec![(1, vec!["owner"]), (2, vec!["owner", "member"])],
        vec!["member"],
    );
    signer_splice.roster_signer_device_id = "other-owner-device".to_owned();
    assert!(plan_roster_history(&signer_splice, None).is_err());
}

#[test]
fn roster_history_rejects_unanchored_suffix_chain() {
    let room = history_room(
        3,
        vec![
            (2, vec!["owner", "member"]),
            (3, vec!["owner", "member", "other"]),
        ],
        vec!["member", "other"],
    );
    assert!(plan_roster_history(&room, None).is_err());

    let pin = pin_for(2, &["owner", "member"]);
    plan_roster_history(&room, Some(&pin)).expect("suffix anchored at the pin verifies");
}

#[test]
fn roster_history_rejects_anchor_that_contradicts_the_pin() {
    let room = history_room(
        3,
        vec![
            (2, vec!["owner", "member"]),
            (3, vec!["owner", "member", "other"]),
        ],
        vec!["member", "other"],
    );
    let pin = RoomRosterPin {
        owner_user_id: "owner".to_owned(),
        generation: 2,
        body: "other-body".to_owned(),
        signature: "other-signature".to_owned(),
        member_user_ids: BTreeSet::from(["owner".to_owned(), "attacker".to_owned()]),
    };
    assert!(plan_roster_history(&room, Some(&pin)).is_err());
}

#[test]
fn roster_history_rejects_full_chain_that_forks_at_the_pin() {
    let pin = pin_for(2, &["owner", "member"]);

    let forked_members = history_room(
        3,
        vec![
            (1, vec!["owner"]),
            (2, vec!["owner", "attacker"]),
            (3, vec!["owner", "attacker", "member"]),
        ],
        vec!["attacker", "member"],
    );
    assert!(plan_roster_history(&forked_members, Some(&pin)).is_err());

    let mut forked_body = history_room(
        3,
        vec![
            (1, vec!["owner"]),
            (2, vec!["owner", "member"]),
            (3, vec!["owner", "member", "other"]),
        ],
        vec!["member", "other"],
    );
    forked_body.roster_transitions[1].roster_body = BASE64.encode(b"re-signed-body");
    assert!(plan_roster_history(&forked_body, Some(&pin)).is_err());

    let mut forked_signature = history_room(
        3,
        vec![
            (1, vec!["owner"]),
            (2, vec!["owner", "member"]),
            (3, vec!["owner", "member", "other"]),
        ],
        vec!["member", "other"],
    );
    forked_signature.roster_transitions[1].roster_signature = BASE64.encode([9]);
    assert!(plan_roster_history(&forked_signature, Some(&pin)).is_err());

    let honest = history_room(
        3,
        vec![
            (1, vec!["owner"]),
            (2, vec!["owner", "member"]),
            (3, vec!["owner", "member", "other"]),
        ],
        vec!["member", "other"],
    );
    plan_roster_history(&honest, Some(&pin)).expect("honest full chain verifies");
}

#[test]
fn roster_history_rejects_chain_that_omits_the_pinned_generation() {
    let pin = pin_for(2, &["owner", "member"]);
    let room = history_room(1, vec![(1, vec!["owner"])], vec![]);
    assert!(plan_roster_history(&room, Some(&pin)).is_err());
}

#[test]
fn roster_history_rejects_pin_with_a_different_owner() {
    let mut pin = pin_for(2, &["owner", "member"]);
    pin.owner_user_id = "someone-else".to_owned();
    let room = history_room(
        2,
        vec![(1, vec!["owner"]), (2, vec!["owner", "member"])],
        vec!["member"],
    );
    assert!(plan_roster_history(&room, Some(&pin)).is_err());
}

#[test]
fn roster_history_rejects_generation_one_with_non_owner_members() {
    let room = history_room(1, vec![(1, vec!["owner", "attacker"])], vec!["attacker"]);
    assert!(plan_roster_history(&room, None).is_err());

    let honest = history_room(1, vec![(1, vec!["owner"])], vec![]);
    let plan = plan_roster_history(&honest, None).expect("owner-only genesis verifies");
    assert!(plan.admissions.is_empty());
    assert_eq!(plan.final_members, BTreeSet::from(["owner".to_owned()]));
}

#[test]
fn roster_history_rejects_member_added_without_admission_proof() {
    let room = history_room(
        2,
        vec![(1, vec!["owner"]), (2, vec!["owner", "member"])],
        vec![],
    );
    assert!(plan_roster_history(&room, None).is_err());

    let proven = history_room(
        2,
        vec![(1, vec!["owner"]), (2, vec!["owner", "member"])],
        vec!["member"],
    );
    let plan = plan_roster_history(&proven, None).expect("proven addition verifies");
    assert_eq!(plan.admissions.len(), 1);
    assert_eq!(plan.admissions[0].user_id, "member");
    assert_eq!(plan.admissions[0].generation, 2);
}

#[test]
fn roster_history_rejects_proof_for_a_different_member() {
    let room = history_room(
        2,
        vec![(1, vec!["owner"]), (2, vec!["owner", "member"])],
        vec!["someone-else"],
    );
    assert!(plan_roster_history(&room, None).is_err());
}

#[test]
fn roster_history_rejects_duplicate_admission_proofs() {
    let room = history_room(
        2,
        vec![(1, vec!["owner"]), (2, vec!["owner", "member"])],
        vec!["member", "member"],
    );
    assert!(plan_roster_history(&room, None).is_err());
}

#[test]
fn roster_history_allows_removed_member_without_retained_proof() {
    let room = history_room(
        3,
        vec![
            (1, vec!["owner"]),
            (2, vec!["owner", "member"]),
            (3, vec!["owner"]),
        ],
        vec![],
    );
    let plan = plan_roster_history(&room, None).expect("removed member needs no proof");
    assert!(plan.admissions.is_empty());
    assert_eq!(plan.final_members, BTreeSet::from(["owner".to_owned()]));
}

#[test]
fn roster_history_uses_latest_proof_after_member_readmission() {
    let room = history_room(
        4,
        vec![
            (1, vec!["owner"]),
            (2, vec!["owner", "member"]),
            (3, vec!["owner"]),
            (4, vec!["owner", "member"]),
        ],
        vec!["member"],
    );
    let plan = plan_roster_history(&room, None).expect("readmitted member uses current proof");
    assert_eq!(plan.admissions.len(), 1);
    assert_eq!(plan.admissions[0].user_id, "member");
    assert_eq!(plan.admissions[0].generation, 4);
}

#[test]
fn roster_history_rejects_non_terminating_and_gapped_chains() {
    let mut room = history_room(
        2,
        vec![(1, vec!["owner"]), (2, vec!["owner", "member"])],
        vec!["member"],
    );
    room.roster_generation = 3;
    assert!(plan_roster_history(&room, None).is_err());

    let gapped = history_room(
        3,
        vec![(1, vec!["owner"]), (3, vec!["owner", "member"])],
        vec!["member"],
    );
    assert!(plan_roster_history(&gapped, None).is_err());
}

#[test]
fn roster_history_rejects_entries_from_another_room_or_owner() {
    let mut room = history_room(1, vec![(1, vec!["owner"])], vec![]);
    room.id = "different-room".to_owned();
    assert!(plan_roster_history(&room, None).is_err());

    let mut room = history_room(1, vec![(1, vec!["owner"])], vec![]);
    room.owner_user_id = "different-owner".to_owned();
    assert!(plan_roster_history(&room, None).is_err());
}

#[test]
fn roster_history_rejects_duplicate_members_in_an_entry() {
    let room = history_room(
        2,
        vec![(1, vec!["owner"]), (2, vec!["owner", "member", "member"])],
        vec!["member"],
    );
    assert!(plan_roster_history(&room, None).is_err());
}

fn signing_identity(user_id: &str, device_id: &str) -> (IdentityBundleView, Vec<u8>) {
    let keypair = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).expect("generate ML-DSA keypair");
    let pkcs8 = keypair.to_pkcs8v1().expect("serialize ML-DSA keypair");
    let public_key = keypair.public_key().as_ref().to_vec();
    let certificate = DeviceCertificate {
        user_id: user_id.to_owned(),
        device_id: device_id.to_owned(),
        device_label: format!("{user_id} device"),
        signer_device_id: device_id.to_owned(),
        kem_public_key: Vec::new(),
        sig_public_key: public_key.clone(),
        issued_at_ms: 1,
        expires_at_ms: None,
    };
    let identity = IdentityBundleView {
        user_id: user_id.to_owned(),
        identity_revision: 1,
        identity_incarnation_id: uuid::Uuid::from_u128(0x0190_0000_0000_7000_8000_0000_0000_0001),
        signed_list: SignedDeviceList {
            user_id: user_id.to_owned(),
            generation: 1,
            entries: Vec::new(),
            signer_device_id: device_id.to_owned(),
            issued_at_ms: 1,
            expires_at_ms: None,
        },
        list_body: Vec::new(),
        list_signature: Vec::new(),
        devices: std::collections::BTreeMap::from([(
            device_id.to_owned(),
            VerifiedDevice {
                certificate,
                sig_public_key: public_key,
                certificate_body: Vec::new(),
                certificate_signature: Vec::new(),
            },
        )]),
        historical_devices: std::collections::BTreeMap::new(),
    };
    (identity, pkcs8.as_ref().to_vec())
}

fn sign_with(pkcs8: &[u8], domain_tag: &[u8], body: &[u8]) -> String {
    BASE64.encode(
        kodosi_backend_client::crypto::sign_control_message(
            pkcs8,
            &domain_preimage(domain_tag, body),
        )
        .expect("sign body"),
    )
}

struct SignedActivationFixture {
    inner: ActivationFixture,
    decision_body: Vec<u8>,
    owner: IdentityBundleView,
    invitee: IdentityBundleView,
    owner_pkcs8: Vec<u8>,
    invitee_pkcs8: Vec<u8>,
}

fn signed_activation_fixture() -> SignedActivationFixture {
    let mut inner = activation_fixture();
    let (owner, owner_pkcs8) = signing_identity("owner", "owner-device");
    let (invitee, invitee_pkcs8) = signing_identity("member", "member-device");
    let decision_body = serde_json::to_vec(&inner.decision).expect("serialize decision");
    let proof = inner
        .room
        .roster_activation_proof
        .as_mut()
        .expect("fixture has proof");
    proof.proposal_signature = sign_with(
        &owner_pkcs8,
        kodosi_domain::domain_tags::ROOM_INVITATION_PROPOSAL_V1,
        &inner.proposal_body,
    );
    proof.decision_signature = sign_with(
        &invitee_pkcs8,
        kodosi_domain::domain_tags::ROOM_INVITATION_DECISION_V1,
        &decision_body,
    );
    SignedActivationFixture {
        inner,
        decision_body,
        owner,
        invitee,
        owner_pkcs8,
        invitee_pkcs8,
    }
}

fn verify_signed_activation(fixture: &SignedActivationFixture) -> super::Result<()> {
    verify_roster_activation_proof(
        &fixture.inner.room,
        fixture
            .inner
            .room
            .roster_activation_proof
            .as_ref()
            .expect("fixture has proof"),
        &fixture.inner.proposal,
        &fixture.inner.proposal_body,
        &fixture.inner.decision,
        &fixture.decision_body,
        &fixture.inner.roster_body,
        &fixture.inner.roster_members,
        Some(&fixture.inner.previous),
        &fixture.inner.added_members,
        "member",
        fixture.inner.now,
        &fixture.owner,
        &fixture.invitee,
    )
}

#[test]
fn signed_activation_proof_verifies_with_real_signatures() {
    let fixture = signed_activation_fixture();
    verify_signed_activation(&fixture).expect("authentically signed proof verifies");
}

#[test]
fn signed_activation_rejects_forged_proposal_signature() {
    let mut fixture = signed_activation_fixture();
    let proof = fixture
        .inner
        .room
        .roster_activation_proof
        .as_mut()
        .expect("fixture has proof");
    let mut signature = BASE64
        .decode(&proof.proposal_signature)
        .expect("decode signature");
    signature[0] ^= 1;
    proof.proposal_signature = BASE64.encode(signature);
    assert!(verify_signed_activation(&fixture).is_err());
}

#[test]
fn signed_activation_rejects_forged_decision_signature() {
    let mut fixture = signed_activation_fixture();
    let proof = fixture
        .inner
        .room
        .roster_activation_proof
        .as_mut()
        .expect("fixture has proof");
    let mut signature = BASE64
        .decode(&proof.decision_signature)
        .expect("decode signature");
    signature[0] ^= 1;
    proof.decision_signature = BASE64.encode(signature);
    assert!(verify_signed_activation(&fixture).is_err());
}

#[test]
fn signed_activation_rejects_owner_signed_decision() {
    let mut fixture = signed_activation_fixture();
    let signature = sign_with(
        &fixture.owner_pkcs8,
        kodosi_domain::domain_tags::ROOM_INVITATION_DECISION_V1,
        &fixture.decision_body,
    );
    fixture
        .inner
        .room
        .roster_activation_proof
        .as_mut()
        .expect("fixture has proof")
        .decision_signature = signature;
    assert!(verify_signed_activation(&fixture).is_err());
}

#[test]
fn signed_activation_rejects_invitee_signed_proposal() {
    let mut fixture = signed_activation_fixture();
    let signature = sign_with(
        &fixture.invitee_pkcs8,
        kodosi_domain::domain_tags::ROOM_INVITATION_PROPOSAL_V1,
        &fixture.inner.proposal_body,
    );
    fixture
        .inner
        .room
        .roster_activation_proof
        .as_mut()
        .expect("fixture has proof")
        .proposal_signature = signature;
    assert!(verify_signed_activation(&fixture).is_err());
}

#[test]
fn signed_activation_rejects_cross_domain_signature_reuse() {
    let mut fixture = signed_activation_fixture();
    let signature = sign_with(
        &fixture.owner_pkcs8,
        kodosi_domain::domain_tags::ROOM_INVITATION_DECISION_V1,
        &fixture.inner.proposal_body,
    );
    fixture
        .inner
        .room
        .roster_activation_proof
        .as_mut()
        .expect("fixture has proof")
        .proposal_signature = signature;
    assert!(verify_signed_activation(&fixture).is_err());
}

#[test]
fn signed_activation_rejects_signature_bound_to_a_different_body() {
    let mut fixture = signed_activation_fixture();
    fixture.inner.decision.decision = RoomInvitationDecision::Declined;
    fixture.decision_body =
        serde_json::to_vec(&fixture.inner.decision).expect("serialize decision");
    assert!(verify_signed_activation(&fixture).is_err());
}

#[test]
fn signed_activation_rejects_valid_signatures_over_mismatched_metadata() {
    let mut fixture = signed_activation_fixture();
    fixture.inner.previous.generation = 0;
    assert!(verify_signed_activation(&fixture).is_err());

    let mut fixture = signed_activation_fixture();
    fixture.inner.added_members.insert("attacker".to_owned());
    assert!(verify_signed_activation(&fixture).is_err());
}
