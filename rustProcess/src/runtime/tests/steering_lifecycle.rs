use super::*;
use crate::{SemanticSendMode, SteerDeliveryState};

#[test]
fn semantic_queue_becomes_durable_without_an_audit_witness() {
    let directory = tempfile::tempdir().expect("state directory");
    let path = directory.path().join("pending-steers.json");
    let account = "account-a";
    let request_id = uuid::Uuid::now_v7();
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let mut state =
        crate::runtime::steering::SteeringState::load_at(path.clone()).expect("steering state");
    let admission = state
        .admit_semantic(
            account.to_owned(),
            request_id,
            session_id,
            incarnation_id,
            SemanticSendMode::Steer,
            "update the tests".to_owned(),
        )
        .expect("admit");
    let crate::runtime::steering::SemanticSendAdmission::New(entry) = admission else {
        panic!("new request must be admitted once");
    };
    let queued = state
        .acknowledge_queued(&entry.steer_id)
        .expect("persist queued")
        .expect("queued entry");
    assert_eq!(queued.delivery_state, SteerDeliveryState::Queued);

    let restarted = crate::runtime::steering::SteeringState::load_at(path).expect("restart state");
    assert_eq!(
        restarted
            .pending_request(account, &request_id.to_string())
            .map(|entry| entry.delivery_state),
        Some(SteerDeliveryState::Queued)
    );
}

#[test]
fn pty_effect_window_is_persisted_as_delivery_unknown() {
    let directory = tempfile::tempdir().expect("state directory");
    let path = directory.path().join("pending-steers.json");
    let account = "account-a";
    let request_id = uuid::Uuid::now_v7();
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let mut state =
        crate::runtime::steering::SteeringState::load_at(path.clone()).expect("steering state");
    let admission = state
        .admit_semantic(
            account.to_owned(),
            request_id,
            session_id,
            incarnation_id,
            SemanticSendMode::Steer,
            "apply now".to_owned(),
        )
        .expect("admit");
    let crate::runtime::steering::SemanticSendAdmission::New(entry) = admission else {
        panic!("new request");
    };
    state.acknowledge_queued(&entry.steer_id).expect("queue");
    state
        .note_boundary(account, session_id, SemanticSendMode::Steer, None)
        .expect("boundary");
    let (claimed, _) = state.claim_ready(account).expect("claim").expect("ready");
    assert_eq!(claimed.delivery_state, SteerDeliveryState::DeliveryUnknown);

    let restarted = crate::runtime::steering::SteeringState::load_at(path).expect("restart state");
    assert_eq!(
        restarted
            .pending_request(account, &request_id.to_string())
            .map(|entry| entry.delivery_state),
        Some(SteerDeliveryState::DeliveryUnknown)
    );
}
