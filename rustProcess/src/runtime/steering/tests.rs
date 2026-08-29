use super::*;

fn execution_tombstone(
    account_user_id: &str,
    request_id: uuid::Uuid,
    session_id: SessionId,
    incarnation_id: uuid::Uuid,
    requester_device_id: &str,
) -> RelaySemanticExecutionTombstone {
    RelaySemanticExecutionTombstone {
        account_user_id: account_user_id.to_owned(),
        request_id: request_id.to_string(),
        session_id: session_id.to_string(),
        session_incarnation_id: incarnation_id.to_string(),
        mode: SemanticSendMode::Queue,
        payload_sha256: payload_sha256("once"),
        requester_device_id: requester_device_id.to_owned(),
        outcome: SteerDeliveryState::Injected,
    }
}

#[test]
fn tombstone_log_recovers_a_torn_final_record() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let steering_path = directory.path().join("pending-steers.json");
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let first_request_id = uuid::Uuid::now_v7();
    let second_request_id = uuid::Uuid::now_v7();
    let mut store = RelayExecutionTombstoneStore::load(&steering_path).expect("store");
    store
        .insert(execution_tombstone(
            "account",
            first_request_id,
            session_id,
            incarnation_id,
            "requester-device",
        ))
        .expect("first append");
    let valid_len = std::fs::metadata(&store.path).expect("metadata").len();
    let mut torn_record = Vec::new();
    write_tombstone_record(
        &mut torn_record,
        &RelayExecutionTombstoneRecord::Add {
            tombstone: execution_tombstone(
                "account",
                second_request_id,
                session_id,
                incarnation_id,
                "requester-device",
            ),
        },
    )
    .expect("encode torn record");
    let mut file = OpenOptions::new()
        .append(true)
        .open(&store.path)
        .expect("open log");
    file.write_all(&torn_record[..torn_record.len() / 2])
        .expect("write torn tail");
    file.sync_all().expect("sync torn tail");
    drop(file);
    drop(store);

    let recovered = RelayExecutionTombstoneStore::load(&steering_path).expect("recover");
    assert!(
        recovered
            .get("account", &first_request_id.to_string())
            .is_some()
    );
    assert!(
        recovered
            .get("account", &second_request_id.to_string())
            .is_none()
    );
    assert_eq!(
        std::fs::metadata(&recovered.path).expect("metadata").len(),
        valid_len
    );
}

#[test]
fn tombstone_log_rejects_oversized_records_before_append() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let steering_path = directory.path().join("pending-steers.json");
    let mut store = RelayExecutionTombstoneStore::load(&steering_path).expect("store");
    let tombstone = execution_tombstone(
        "account",
        uuid::Uuid::now_v7(),
        SessionId::new(),
        uuid::Uuid::now_v7(),
        &"d".repeat(MAX_TOMBSTONE_RECORD_BYTES),
    );

    let error = store.insert(tombstone).expect_err("oversized record");
    assert!(error.to_string().contains("record is oversized"));
    assert!(!store.path.exists());
    assert!(store.by_request.is_empty());
}

#[test]
fn tombstone_log_appends_linearly_without_rewriting_steering_json() {
    const RECORDS: usize = 1_024;
    let directory = tempfile::tempdir().expect("steering tempdir");
    let steering_path = directory.path().join("pending-steers.json");
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let mut store = RelayExecutionTombstoneStore::load(&steering_path).expect("store");
    let mut previous_len = 0;
    let mut largest_growth = 0;
    for _ in 0..RECORDS {
        store
            .insert(execution_tombstone(
                "account",
                uuid::Uuid::now_v7(),
                session_id,
                incarnation_id,
                "requester-device",
            ))
            .expect("append");
        let current_len = std::fs::metadata(&store.path).expect("metadata").len();
        largest_growth = largest_growth.max(current_len.saturating_sub(previous_len));
        previous_len = current_len;
    }
    assert_eq!(store.by_request.len(), RECORDS);
    assert!(largest_growth < u64::try_from(MAX_TOMBSTONE_RECORD_BYTES).unwrap_or(u64::MAX));
    assert!(!steering_path.exists());
    drop(store);
    let restarted = RelayExecutionTombstoneStore::load(&steering_path).expect("restart");
    assert_eq!(restarted.by_request.len(), RECORDS);
}

#[test]
fn tombstone_log_retirement_compacts_only_at_lifecycle_boundary() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let steering_path = directory.path().join("pending-steers.json");
    let retired_session = SessionId::new();
    let retained_session = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let retained_request_id = uuid::Uuid::now_v7();
    let mut store = RelayExecutionTombstoneStore::load(&steering_path).expect("store");
    for session_id in [retired_session, retained_session] {
        for _ in 0..32 {
            store
                .insert(execution_tombstone(
                    "account",
                    if session_id == retained_session {
                        retained_request_id
                    } else {
                        uuid::Uuid::now_v7()
                    },
                    session_id,
                    incarnation_id,
                    "requester-device",
                ))
                .expect("append");
            if session_id == retained_session {
                break;
            }
        }
    }
    let before_retirement = std::fs::metadata(&store.path).expect("metadata").len();

    store
        .retire_session(&retired_session.to_string())
        .expect("retire session");
    let after_retirement = std::fs::metadata(&store.path).expect("metadata").len();
    assert!(after_retirement < before_retirement);
    assert_eq!(store.by_request.len(), 1);
    assert!(
        store
            .get("account", &retained_request_id.to_string())
            .is_some()
    );
    drop(store);
    let restarted = RelayExecutionTombstoneStore::load(&steering_path).expect("restart");
    assert_eq!(restarted.by_request.len(), 1);
}

#[test]
fn turn_states_retire_with_account_and_session_lifecycle() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let first_session = SessionId::new();
    let second_session = SessionId::new();
    let incarnation = uuid::Uuid::now_v7();
    let mut state = SteeringState::load(path.clone()).expect("state");
    for (account, session) in [
        ("account-a", first_session),
        ("account-a", second_session),
        ("account-b", first_session),
    ] {
        state
            .note_semantic_turn_state(account, session, incarnation, SemanticTurnState::Idle)
            .expect("turn state persists");
    }

    state
        .cancel_session(first_session)
        .expect("session retirement persists");
    assert!(
        state
            .turn_states
            .iter()
            .all(|record| record.session_id != first_session.to_string())
    );
    state
        .clear_account("account-a")
        .expect("account retirement");
    assert!(state.turn_states.is_empty());
    assert!(
        SteeringState::load(path)
            .expect("restart")
            .turn_states
            .is_empty()
    );
}

#[test]
fn persisted_turn_states_are_bounded() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let mut state = SteeringState::load(path.clone()).expect("state");
    for index in 0..MAX_SEMANTIC_TURN_STATES {
        state
            .note_semantic_turn_state(
                "account",
                SessionId::new(),
                uuid::Uuid::now_v7(),
                SemanticTurnState::Running,
            )
            .unwrap_or_else(|error| panic!("turn state {index} should fit: {error}"));
    }
    assert!(
        state
            .note_semantic_turn_state(
                "account",
                SessionId::new(),
                uuid::Uuid::now_v7(),
                SemanticTurnState::Running,
            )
            .is_err()
    );
    let mut persisted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read persisted steering"))
            .expect("decode persisted steering");
    let extra = persisted["turnStates"][0].clone();
    persisted["turnStates"]
        .as_array_mut()
        .expect("turn states array")
        .push(extra);
    std::fs::write(&path, serde_json::to_vec(&persisted).expect("encode"))
        .expect("write oversized state");
    assert!(SteeringState::load(path).is_err());
}

#[test]
fn idle_turn_state_survives_restart_and_queues_at_immediate_boundary() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let mut state = SteeringState::load(path.clone()).expect("state");
    state
        .note_semantic_turn_state(
            "account",
            session_id,
            incarnation_id,
            SemanticTurnState::Idle,
        )
        .expect("idle state persists");
    drop(state);

    let mut restarted = SteeringState::load(path).expect("restart");
    assert_eq!(
        restarted.semantic_turn_state("account", session_id, incarnation_id),
        Some(SemanticTurnState::Idle)
    );
    let entry = restarted
        .queue_semantic(
            "account".to_owned(),
            uuid::Uuid::now_v7(),
            session_id,
            incarnation_id,
            SemanticSendMode::Queue,
            "deliver while idle".to_owned(),
        )
        .expect("queue admission");
    restarted
        .acknowledge_queued(&entry.steer_id)
        .expect("queue acknowledgement");
    let (claimed, boundary) = restarted
        .claim_ready("account")
        .expect("claim")
        .expect("idle Queue is immediately eligible");
    assert_eq!(claimed.steer_id, entry.steer_id);
    assert!(boundary.is_none());
}

#[test]
fn persisted_idle_transition_arms_queue_admitted_while_running() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let mut state = SteeringState::load(path.clone()).expect("state");
    state
        .note_semantic_turn_state(
            "account",
            session_id,
            incarnation_id,
            SemanticTurnState::Running,
        )
        .expect("running state persists");
    let entry = state
        .queue_semantic(
            "account".to_owned(),
            uuid::Uuid::now_v7(),
            session_id,
            incarnation_id,
            SemanticSendMode::Queue,
            "after this turn".to_owned(),
        )
        .expect("queue admission");
    state
        .acknowledge_queued(&entry.steer_id)
        .expect("queue acknowledgement");
    assert!(state.claim_ready("account").expect("claim check").is_none());

    state
        .note_semantic_turn_state(
            "account",
            session_id,
            incarnation_id,
            SemanticTurnState::Idle,
        )
        .expect("idle transition persists boundary");
    drop(state);
    let mut restarted = SteeringState::load(path).expect("restart");
    assert!(restarted.claim_ready("account").expect("claim").is_some());
}

#[test]
fn stale_incarnation_idle_state_never_arms_new_incarnation_queue() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let session_id = SessionId::new();
    let stale_incarnation = uuid::Uuid::now_v7();
    let current_incarnation = uuid::Uuid::now_v7();
    let mut state =
        SteeringState::load(directory.path().join("pending-steers.json")).expect("state");
    state
        .note_semantic_turn_state(
            "account",
            session_id,
            stale_incarnation,
            SemanticTurnState::Idle,
        )
        .expect("stale idle state persists");
    let entry = state
        .queue_semantic(
            "account".to_owned(),
            uuid::Uuid::now_v7(),
            session_id,
            current_incarnation,
            SemanticSendMode::Queue,
            "wait for current turn end".to_owned(),
        )
        .expect("queue admission");
    state
        .acknowledge_queued(&entry.steer_id)
        .expect("queue acknowledgement");

    assert_eq!(
        state.semantic_turn_state("account", session_id, stale_incarnation),
        Some(SemanticTurnState::Idle)
    );
    assert_eq!(
        state.semantic_turn_state("account", session_id, current_incarnation),
        None
    );
    assert!(
        state.claim_ready("account").expect("claim check").is_none(),
        "a prior incarnation cannot make current Queue work eligible"
    );
}

#[test]
fn relay_admission_reserves_receipt_capacity_before_injection() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let mut state =
        SteeringState::load(directory.path().join("pending-steers.json")).expect("state");
    for index in 0..MAX_COMPLETED_RELAY_SEMANTIC_SENDS {
        state.completed.push_back(SemanticSendReceipt {
            account_user_id: "account".to_owned(),
            request_id: format!("receipt-{index}"),
            session_id: session_id.to_string(),
            session_incarnation_id: incarnation_id.to_string(),
            mode: SemanticSendMode::Steer,
            payload_sha256: payload_sha256("done"),
            text: "done".to_owned(),
            queued_at_ms: index as u64,
            at_tool_use_id: None,
            outcome: SteerDeliveryState::Injected,
            origin: SemanticSendOrigin::Relay,
            relay_acknowledged: false,
        });
    }

    let result = state.admit_relay_semantic(
        "account".to_owned(),
        uuid::Uuid::now_v7(),
        session_id,
        incarnation_id,
        SemanticSendMode::Steer,
        "must not inject without receipt capacity".to_owned(),
        "requester-device",
    );

    assert!(matches!(result, Err(AppError::ChannelFull { .. })));
    assert!(state.entries.is_empty());
    assert!(state.relay_requesters.is_empty());
}

#[test]
fn relay_admission_persists_request_and_requester_in_one_transaction() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let request_id = uuid::Uuid::now_v7();
    let mut state = SteeringState::load(path.clone()).expect("state");

    let admitted = state
        .admit_relay_semantic(
            "account".to_owned(),
            request_id,
            session_id,
            incarnation_id,
            SemanticSendMode::Steer,
            "atomic relay request".to_owned(),
            "requester-device",
        )
        .expect("relay admission");
    assert!(matches!(admitted, SemanticSendAdmission::New(_)));
    drop(state);

    let mut restarted = SteeringState::load(path).expect("restart");
    assert_eq!(
        restarted.relay_requester_device("account", &request_id.to_string()),
        Some("requester-device")
    );
    assert!(
        restarted
            .pending_request("account", &request_id.to_string())
            .is_some()
    );
    assert!(
        restarted
            .admit_semantic(
                "account".to_owned(),
                request_id,
                session_id,
                incarnation_id,
                SemanticSendMode::Steer,
                "atomic relay request".to_owned(),
            )
            .is_err(),
        "a relay request ID must not be rebound as direct-local work"
    );
}

#[test]
fn relay_requester_and_mailbox_ack_survive_restart() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let request_id = uuid::Uuid::now_v7();
    let mut state = SteeringState::load(path.clone()).expect("state");
    let entry = match state
        .admit_relay_semantic(
            "account".to_owned(),
            request_id,
            session_id,
            incarnation_id,
            SemanticSendMode::Queue,
            "hello".to_owned(),
            "requester-device",
        )
        .expect("admit")
    {
        SemanticSendAdmission::New(entry) => entry,
        other => panic!("expected new relay admission, got {other:?}"),
    };
    state
        .complete_semantic(
            &entry,
            SteerDeliveryState::Injected,
            SemanticSendOrigin::Relay,
        )
        .expect("complete");

    let mut restarted = SteeringState::load(path.clone()).expect("restart");
    assert_eq!(
        restarted.relay_requester_device("account", &request_id.to_string()),
        Some("requester-device")
    );
    assert_eq!(
        restarted
            .pending_relay_receipts("account", session_id)
            .len(),
        1
    );
    assert!(
        restarted
            .acknowledge_relay_receipt("account", request_id, session_id, incarnation_id,)
            .expect("ack")
    );
    let restarted = SteeringState::load(path).expect("restart after ack");
    assert!(
        restarted
            .pending_relay_receipts("account", session_id)
            .is_empty()
    );
}

#[test]
fn acknowledged_relay_receipts_become_incarnation_lifetime_execution_tombstones() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let request_id = uuid::Uuid::now_v7();
    let mut state = SteeringState::load(path.clone()).expect("state");
    let entry = match state
        .admit_relay_semantic(
            "account".to_owned(),
            request_id,
            session_id,
            incarnation_id,
            SemanticSendMode::Queue,
            "inject exactly once".to_owned(),
            "requester-device",
        )
        .expect("admit")
    {
        SemanticSendAdmission::New(entry) => entry,
        other => panic!("expected new admission, got {other:?}"),
    };
    state
        .complete_semantic(
            &entry,
            SteerDeliveryState::Injected,
            SemanticSendOrigin::Relay,
        )
        .expect("complete");
    state
        .acknowledge_relay_receipt("account", request_id, session_id, incarnation_id)
        .expect("acknowledge");

    assert!(state.completed.is_empty());
    assert!(state.relay_requesters.is_empty());
    assert_eq!(state.relay_execution_tombstones.by_request.len(), 1);
    drop(state);

    let mut restarted = SteeringState::load(path).expect("restart");
    let exact = restarted
        .admit_relay_semantic(
            "account".to_owned(),
            request_id,
            session_id,
            incarnation_id,
            SemanticSendMode::Queue,
            "inject exactly once".to_owned(),
            "requester-device",
        )
        .expect("exact replay remains terminal");
    let SemanticSendAdmission::Tombstoned(exact) = exact else {
        panic!("exact replay must resolve from the execution tombstone")
    };
    assert_eq!(exact.delivery_state, SteerDeliveryState::Injected);
    assert!(restarted.entries.is_empty());
    assert!(
        restarted
            .admit_relay_semantic(
                "account".to_owned(),
                request_id,
                session_id,
                incarnation_id,
                SemanticSendMode::Queue,
                "changed payload".to_owned(),
                "requester-device",
            )
            .is_err()
    );
    assert!(
        restarted
            .admit_relay_semantic(
                "account".to_owned(),
                request_id,
                session_id,
                incarnation_id,
                SemanticSendMode::Queue,
                "inject exactly once".to_owned(),
                "replacement-device",
            )
            .is_err()
    );
}

#[test]
fn existing_tombstone_finishes_receipt_retirement_after_json_write_failure() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let request_id = uuid::Uuid::now_v7();
    let mut state = SteeringState::load(path.clone()).expect("state");
    let entry = match state
        .admit_relay_semantic(
            "account".to_owned(),
            request_id,
            session_id,
            incarnation_id,
            SemanticSendMode::Queue,
            "inject exactly once".to_owned(),
            "requester-device",
        )
        .expect("admit")
    {
        SemanticSendAdmission::New(entry) => entry,
        other => panic!("expected new admission, got {other:?}"),
    };
    state
        .complete_semantic(
            &entry,
            SteerDeliveryState::Injected,
            SemanticSendOrigin::Relay,
        )
        .expect("complete");
    let saved_path = state.path.clone();
    state.path = directory.path().join("missing/pending-steers.json");
    assert!(
        state
            .acknowledge_relay_receipt("account", request_id, session_id, incarnation_id)
            .is_err()
    );
    assert!(
        state
            .completed_receipt("account", &request_id.to_string())
            .is_some()
    );
    state.path = saved_path;

    assert!(
        state
            .acknowledge_relay_receipt("account", request_id, session_id, incarnation_id)
            .expect("retry retirement")
    );
    assert!(
        !state
            .acknowledge_relay_receipt("account", request_id, SessionId::new(), incarnation_id,)
            .expect("mismatched acknowledgement")
    );
    assert!(
        state
            .completed_receipt("account", &request_id.to_string())
            .is_none()
    );
    drop(state);
    let restarted = SteeringState::load(path).expect("restart");
    assert!(
        restarted
            .completed_receipt("account", &request_id.to_string())
            .is_none()
    );
    assert!(
        restarted
            .relay_execution_tombstone("account", &request_id.to_string())
            .is_some()
    );
}

#[test]
fn relay_execution_tombstones_retire_only_with_their_session_lifecycle() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let retired_session = SessionId::new();
    let retained_session = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let mut state = SteeringState::load(path.clone()).expect("state");
    for session_id in [retired_session, retained_session] {
        let request_id = uuid::Uuid::now_v7();
        let entry = match state
            .admit_relay_semantic(
                "account".to_owned(),
                request_id,
                session_id,
                incarnation_id,
                SemanticSendMode::Steer,
                "once".to_owned(),
                "requester-device",
            )
            .expect("admit")
        {
            SemanticSendAdmission::New(entry) => entry,
            other => panic!("expected new admission, got {other:?}"),
        };
        state
            .complete_semantic(
                &entry,
                SteerDeliveryState::Injected,
                SemanticSendOrigin::Relay,
            )
            .expect("complete");
        state
            .acknowledge_relay_receipt("account", request_id, session_id, incarnation_id)
            .expect("acknowledge");
    }

    state
        .cancel_session(retired_session)
        .expect("session retirement persists");
    assert_eq!(state.relay_execution_tombstones.by_request.len(), 1);
    assert_eq!(
        state
            .relay_execution_tombstones
            .by_request
            .values()
            .next()
            .expect("retained tombstone")
            .session_id,
        retained_session.to_string()
    );
    let restarted = SteeringState::load(path).expect("restart");
    assert_eq!(restarted.relay_execution_tombstones.by_request.len(), 1);
}

#[test]
fn direct_local_receipts_are_not_relayed_and_oldest_exhausts_at_capacity() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let mut state = SteeringState::load(path.clone()).expect("state");
    let mut first_request_id = None;

    for index in 0..=MAX_COMPLETED_DIRECT_SEMANTIC_SENDS {
        let request_id = uuid::Uuid::now_v7();
        first_request_id.get_or_insert(request_id);
        let entry = state
            .queue_semantic(
                "account".to_owned(),
                request_id,
                session_id,
                incarnation_id,
                SemanticSendMode::Queue,
                format!("direct local {index}"),
            )
            .expect("direct-local admission");
        state
            .complete_semantic(
                &entry,
                SteerDeliveryState::Injected,
                SemanticSendOrigin::DirectLocal,
            )
            .expect("direct-local completion must compact without relay ack");
    }

    assert_eq!(state.completed.len(), MAX_COMPLETED_DIRECT_SEMANTIC_SENDS);
    assert!(
        state
            .pending_relay_receipts("account", session_id)
            .is_empty()
    );
    assert!(
        state
            .completed_receipt("account", &first_request_id.expect("first id").to_string())
            .is_none(),
        "the oldest direct-local receipt is exhausted at the 256-entry bound"
    );
    let restarted = SteeringState::load(path).expect("restart");
    assert_eq!(
        restarted.completed.len(),
        MAX_COMPLETED_DIRECT_SEMANTIC_SENDS
    );
    assert!(
        restarted
            .pending_relay_receipts("account", session_id)
            .is_empty()
    );
}

#[test]
fn relay_pressure_cannot_evict_a_new_direct_local_receipt() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let mut state =
        SteeringState::load(directory.path().join("pending-steers.json")).expect("state");
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();

    for index in 0..MAX_COMPLETED_RELAY_SEMANTIC_SENDS {
        state.completed.push_back(SemanticSendReceipt {
            account_user_id: "account".to_owned(),
            request_id: format!("relay-{index}"),
            session_id: session_id.to_string(),
            session_incarnation_id: incarnation_id.to_string(),
            mode: SemanticSendMode::Queue,
            payload_sha256: payload_sha256("relay"),
            text: "relay".to_owned(),
            queued_at_ms: index as u64,
            at_tool_use_id: None,
            outcome: SteerDeliveryState::Injected,
            origin: SemanticSendOrigin::Relay,
            relay_acknowledged: false,
        });
    }
    let request_id = uuid::Uuid::now_v7();
    let entry = state
        .queue_semantic(
            "account".to_owned(),
            request_id,
            session_id,
            incarnation_id,
            SemanticSendMode::Queue,
            "direct".to_owned(),
        )
        .expect("direct admission");
    state
        .complete_semantic(
            &entry,
            SteerDeliveryState::Injected,
            SemanticSendOrigin::DirectLocal,
        )
        .expect("direct completion");

    assert!(
        state
            .completed_receipt("account", &request_id.to_string())
            .is_some()
    );
    assert_eq!(
        state.completed.len(),
        MAX_COMPLETED_RELAY_SEMANTIC_SENDS + 1
    );
}

#[test]
fn v2_receipts_migrate_as_direct_local_without_relay_identity() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let request_id = uuid::Uuid::now_v7();
    let persisted = serde_json::json!({
        "version": OLDER_STEER_FILE_VERSION,
        "entries": [],
        "completed": [{
            "accountUserId": "account",
            "requestId": request_id,
            "sessionId": session_id,
            "sessionIncarnationId": incarnation_id,
            "mode": "queue",
            "payloadSha256": payload_sha256("local"),
            "text": "local",
            "queuedAtMs": 1,
            "outcome": "injected",
            "relayAcknowledged": false
        }],
        "relayRequesters": [],
        "readyBoundaries": []
    });
    std::fs::write(&path, serde_json::to_vec_pretty(&persisted).expect("json"))
        .expect("persist v2 state");

    let state = SteeringState::load(path).expect("migrate v2");
    assert_eq!(
        state
            .completed_receipt("account", &request_id.to_string())
            .expect("receipt")
            .origin,
        SemanticSendOrigin::DirectLocal
    );
    assert!(
        state
            .pending_relay_receipts("account", session_id)
            .is_empty()
    );
}

#[test]
fn v3_receipt_origin_is_derived_from_persisted_relay_requester() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let relay_request_id = uuid::Uuid::now_v7();
    let local_request_id = uuid::Uuid::now_v7();
    let receipt = |request_id: uuid::Uuid, text: &str| {
        serde_json::json!({
            "accountUserId": "account",
            "requestId": request_id,
            "sessionId": session_id,
            "sessionIncarnationId": incarnation_id,
            "mode": "queue",
            "payloadSha256": payload_sha256(text),
            "text": text,
            "queuedAtMs": 1,
            "outcome": "injected",
            "relayAcknowledged": false
        })
    };
    let persisted = serde_json::json!({
        "version": RECEIPT_ORIGIN_FILE_VERSION,
        "entries": [],
        "completed": [
            receipt(relay_request_id, "relay"),
            receipt(local_request_id, "local")
        ],
        "relayRequesters": [{
            "accountUserId": "account",
            "requestId": relay_request_id,
            "requesterDeviceId": "requester-device"
        }],
        "readyBoundaries": []
    });
    std::fs::write(&path, serde_json::to_vec_pretty(&persisted).expect("json"))
        .expect("persist v3 state");

    let state = SteeringState::load(path).expect("migrate v3");
    assert_eq!(
        state
            .completed_receipt("account", &relay_request_id.to_string())
            .expect("relay receipt")
            .origin,
        SemanticSendOrigin::Relay
    );
    assert_eq!(
        state
            .completed_receipt("account", &local_request_id.to_string())
            .expect("local receipt")
            .origin,
        SemanticSendOrigin::DirectLocal
    );
    assert_eq!(state.pending_relay_receipts("account", session_id).len(), 1);
}

#[test]
fn request_ids_are_scoped_to_the_active_account() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let request_id = uuid::Uuid::now_v7();
    let mut state =
        SteeringState::load(directory.path().join("pending-steers.json")).expect("steering state");
    state
        .queue_semantic(
            "account-a".to_owned(),
            request_id,
            session_id,
            incarnation_id,
            SemanticSendMode::Queue,
            "account a".to_owned(),
        )
        .expect("account A request");
    state
        .queue_semantic(
            "account-b".to_owned(),
            request_id,
            session_id,
            incarnation_id,
            SemanticSendMode::Queue,
            "account b".to_owned(),
        )
        .expect("account B may independently reuse UUID");
    assert_ne!(
        state.entries(session_id)[0].steer_id,
        state.entries(session_id)[1].steer_id,
        "receipt correlation must stay unique when accounts reuse a request UUID"
    );
    let account_a = state
        .entries(session_id)
        .into_iter()
        .find(|entry| entry.account_user_id == "account-a")
        .expect("account A entry");
    state
        .complete_semantic(
            &account_a,
            SteerDeliveryState::Injected,
            SemanticSendOrigin::DirectLocal,
        )
        .expect("account A completion");
    let remaining = state.entries(session_id);
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].account_user_id, "account-b");
}

#[test]
fn delivery_unknown_semantic_send_cannot_be_cancelled() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let session_id = SessionId::new();
    let mut state =
        SteeringState::load(directory.path().join("pending-steers.json")).expect("steering state");
    let entry = state
        .queue_semantic(
            "account".to_owned(),
            uuid::Uuid::now_v7(),
            session_id,
            uuid::Uuid::now_v7(),
            SemanticSendMode::Steer,
            "maybe delivered".to_owned(),
        )
        .expect("semantic admission");
    state
        .acknowledge_queued(&entry.steer_id)
        .expect("queue acknowledgement");
    state
        .note_tool_boundary("account", session_id, Some("tool-1".to_owned()))
        .expect("boundary persists");
    state
        .claim_ready("account")
        .expect("claim")
        .expect("ready request");
    assert!(
        state
            .cancel_semantic(
                "account",
                session_id,
                &entry.steer_id,
                SemanticSendOrigin::DirectLocal,
            )
            .expect("cancel lookup")
            .is_none(),
        "delivery-unknown work cannot be represented as cancelled"
    );
}

#[test]
fn semantic_modes_become_ready_only_at_their_authoritative_boundaries() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let mut state =
        SteeringState::load(directory.path().join("pending-steers.json")).expect("steering state");
    let queue = state
        .queue_semantic(
            "account".to_owned(),
            uuid::Uuid::now_v7(),
            session_id,
            incarnation_id,
            SemanticSendMode::Queue,
            "after the turn".to_owned(),
        )
        .expect("queue admission");
    let steer = state
        .queue_semantic(
            "account".to_owned(),
            uuid::Uuid::now_v7(),
            session_id,
            incarnation_id,
            SemanticSendMode::Steer,
            "at the tool".to_owned(),
        )
        .expect("steer admission");
    let stop = state
        .queue_semantic(
            "account".to_owned(),
            uuid::Uuid::now_v7(),
            session_id,
            incarnation_id,
            SemanticSendMode::StopAndSend,
            "interrupt once".to_owned(),
        )
        .expect("stop admission");

    state
        .note_turn_end("account", session_id)
        .expect("turn boundary");
    state
        .note_tool_boundary("account", session_id, Some("tool-before-ack".to_owned()))
        .expect("pre-ack tool boundary");
    assert!(
        state
            .claim_ready("account")
            .expect("no premature claim")
            .is_none()
    );
    drop(state);
    let mut state = SteeringState::load(directory.path().join("pending-steers.json"))
        .expect("reload deferred boundaries");

    state
        .acknowledge_queued(&queue.steer_id)
        .expect("queue acknowledgement");
    state
        .acknowledge_queued(&steer.steer_id)
        .expect("steer acknowledgement");
    state
        .acknowledge_queued(&stop.steer_id)
        .expect("stop acknowledgement");
    assert!(
        state
            .claim_ready("other-account")
            .expect("wrong-account claim")
            .is_none(),
        "another account must not claim a retained semantic boundary"
    );

    let (claimed_queue, boundary) = state
        .claim_ready("account")
        .expect("retained queue claim")
        .expect("pre-ack turn boundary survives restart");
    assert_eq!(claimed_queue.steer_id, queue.steer_id);
    assert!(boundary.is_none());

    let (claimed_steer, boundary) = state
        .claim_ready("account")
        .expect("retained steer claim")
        .expect("pre-ack tool boundary survives restart");
    assert_eq!(claimed_steer.steer_id, steer.steer_id);
    assert_eq!(boundary.as_deref(), Some("tool-before-ack"));

    let (claimed_stop, boundary) = state
        .claim_ready("account")
        .expect("stop claim")
        .expect("stop is ready after its queue acknowledgement");
    assert_eq!(claimed_stop.steer_id, stop.steer_id);
    assert_eq!(claimed_stop.mode, SemanticSendMode::StopAndSend);
    assert!(boundary.is_none());
}

#[test]
fn exact_completed_retry_returns_terminal_receipt_without_requeue() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let request_id = uuid::Uuid::now_v7();
    let mut state = SteeringState::load(path.clone()).expect("steering state");
    let entry = match state
        .admit_semantic(
            "account-a".to_owned(),
            request_id,
            session_id,
            incarnation_id,
            SemanticSendMode::Queue,
            "run the focused tests".to_owned(),
        )
        .expect("new semantic send")
    {
        SemanticSendAdmission::New(entry) => entry,
        other => panic!("unexpected admission: {other:?}"),
    };
    state
        .complete_semantic(
            &entry,
            SteerDeliveryState::Injected,
            SemanticSendOrigin::DirectLocal,
        )
        .expect("completion persists");

    let mut restarted = SteeringState::load(path).expect("reload completed receipt");
    let completed = restarted
        .admit_semantic(
            "account-a".to_owned(),
            request_id,
            session_id,
            incarnation_id,
            SemanticSendMode::Queue,
            "run the focused tests".to_owned(),
        )
        .expect("exact retry reads receipt");
    let SemanticSendAdmission::Completed(completed) = completed else {
        panic!("exact retry must be completed");
    };
    assert_eq!(completed.delivery_state, SteerDeliveryState::Injected);
    assert_eq!(completed.text, "run the focused tests");
    assert!(restarted.entries(session_id).is_empty());
}

#[test]
fn unfiltered_query_rehydrates_bounded_completed_receipts_after_restart() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let request_id = uuid::Uuid::now_v7();
    let mut state = SteeringState::load(path.clone()).expect("steering state");
    let entry = state
        .queue_semantic(
            "account-a".to_owned(),
            request_id,
            session_id,
            incarnation_id,
            SemanticSendMode::Queue,
            "already injected".to_owned(),
        )
        .expect("queue semantic send");
    state
        .complete_semantic(
            &entry,
            SteerDeliveryState::Injected,
            SemanticSendOrigin::DirectLocal,
        )
        .expect("completion persists");

    let unknown_request_id = uuid::Uuid::now_v7();
    let unknown = state
        .queue_semantic(
            "account-a".to_owned(),
            unknown_request_id,
            session_id,
            incarnation_id,
            SemanticSendMode::Steer,
            "delivery unknown".to_owned(),
        )
        .expect("queue unknown semantic send");
    state
        .complete_semantic(
            &unknown,
            SteerDeliveryState::DeliveryUnknown,
            SemanticSendOrigin::DirectLocal,
        )
        .expect("unknown completion persists");

    let restarted = SteeringState::load(path).expect("reload completed receipt");
    let queried = restarted.query_semantic("account-a", session_id, None);
    assert_eq!(queried.len(), 2);
    assert!(queried.iter().any(|entry| {
        entry.request_id == request_id.to_string()
            && entry.delivery_state == SteerDeliveryState::Injected
    }));
    assert!(queried.iter().any(|entry| {
        entry.request_id == unknown_request_id.to_string()
            && entry.delivery_state == SteerDeliveryState::DeliveryUnknown
    }));
}

#[test]
fn completed_receipt_query_is_request_and_account_scoped() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let request_id = uuid::Uuid::now_v7();
    let mut state = SteeringState::load(path).expect("steering state");
    let entry = state
        .queue_semantic(
            "account-a".to_owned(),
            request_id,
            session_id,
            incarnation_id,
            SemanticSendMode::Steer,
            "preserve this payload".to_owned(),
        )
        .expect("queue semantic send");
    state
        .complete_semantic(
            &entry,
            SteerDeliveryState::Cancelled,
            SemanticSendOrigin::DirectLocal,
        )
        .expect("cancel receipt persists");

    let unfiltered = state.query_semantic("account-a", session_id, None);
    assert_eq!(unfiltered.len(), 1);
    assert_eq!(unfiltered[0].request_id, request_id.to_string());
    assert!(
        state
            .query_semantic("account-b", session_id, None)
            .is_empty()
    );
    assert!(
        state
            .query_semantic("account-b", session_id, Some(&request_id.to_string()))
            .is_empty()
    );
    let queried = state.query_semantic("account-a", session_id, Some(&request_id.to_string()));
    assert_eq!(queried.len(), 1);
    assert_eq!(queried[0].delivery_state, SteerDeliveryState::Cancelled);
    assert_eq!(queried[0].text, "preserve this payload");
}

#[test]
fn reused_request_id_with_changed_input_is_rejected_after_completion() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let request_id = uuid::Uuid::now_v7();
    let mut state =
        SteeringState::load(directory.path().join("pending-steers.json")).expect("steering state");
    let entry = state
        .queue_semantic(
            "account-a".to_owned(),
            request_id,
            session_id,
            incarnation_id,
            SemanticSendMode::Queue,
            "original".to_owned(),
        )
        .expect("queue semantic send");
    state
        .complete_semantic(
            &entry,
            SteerDeliveryState::Injected,
            SemanticSendOrigin::DirectLocal,
        )
        .expect("completion persists");

    let error = state
        .admit_semantic(
            "account-a".to_owned(),
            request_id,
            session_id,
            incarnation_id,
            SemanticSendMode::Queue,
            "changed".to_owned(),
        )
        .expect_err("changed payload must not reuse request ID");
    assert!(error.to_string().contains("reused with different input"));
}

#[test]
fn v1_migration_drops_unscoped_entries_instead_of_guessing_an_account() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let session_id = SessionId::new();
    let legacy = serde_json::json!({
        "version": 1,
        "entries": [{
            "steerId": "legacy-steer",
            "sessionId": session_id.to_string(),
            "text": "legacy text",
            "queuedAtMs": 1,
            "deliveryState": "queued"
        }]
    });
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&legacy).expect("legacy JSON"),
    )
    .expect("write legacy file");

    let state = SteeringState::load(path).expect("legacy migration");
    assert!(state.entries(session_id).is_empty());
}

#[test]
fn historical_versions_drop_entries_without_incarnation_and_keep_scoped_entries() {
    for version in [
        OLDER_STEER_FILE_VERSION,
        RECEIPT_ORIGIN_FILE_VERSION,
        PREVIOUS_STEER_FILE_VERSION,
    ] {
        let directory = tempfile::tempdir().expect("steering tempdir");
        let path = directory.path().join("pending-steers.json");
        let session_id = SessionId::new();
        let incarnation_id = uuid::Uuid::now_v7();
        let historical = serde_json::json!({
            "version": version,
            "entries": [
                {
                    "steerId": "unscoped",
                    "accountUserId": "account",
                    "requestId": uuid::Uuid::now_v7(),
                    "mode": "queue",
                    "sessionId": session_id,
                    "text": "must be discarded",
                    "queuedAtMs": 1,
                    "deliveryState": "queued"
                },
                {
                    "steerId": "scoped",
                    "accountUserId": "account",
                    "requestId": uuid::Uuid::now_v7(),
                    "sessionIncarnationId": incarnation_id,
                    "mode": "queue",
                    "sessionId": session_id,
                    "text": "keep",
                    "queuedAtMs": 2,
                    "deliveryState": "queued"
                }
            ]
        });
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&historical).expect("historical JSON"),
        )
        .expect("write historical file");

        let state = SteeringState::load(path).expect("historical migration");
        let entries = state.entries(session_id);
        assert_eq!(entries.len(), 1, "version {version}");
        assert_eq!(entries[0].steer_id, "scoped", "version {version}");
        assert_eq!(
            entries[0].session_incarnation_id,
            incarnation_id.to_string(),
            "version {version}"
        );
    }
}

#[test]
fn v5_receipts_migrate_without_fabricating_execution_tombstones() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let request_id = uuid::Uuid::now_v7();
    let persisted = serde_json::json!({
        "version": PREVIOUS_STEER_FILE_VERSION,
        "entries": [],
        "completed": [{
            "accountUserId": "account",
            "requestId": request_id,
            "sessionId": session_id,
            "sessionIncarnationId": incarnation_id,
            "mode": "queue",
            "payloadSha256": payload_sha256("relay"),
            "text": "relay",
            "queuedAtMs": 1,
            "outcome": "injected",
            "origin": "relay",
            "relayAcknowledged": true
        }],
        "relayRequesters": [{
            "accountUserId": "account",
            "requestId": request_id,
            "requesterDeviceId": "requester-device"
        }],
        "readyBoundaries": [],
        "turnStates": []
    });
    std::fs::write(&path, serde_json::to_vec_pretty(&persisted).expect("json"))
        .expect("persist v5 state");

    let state = SteeringState::load(path).expect("migrate v5");
    assert!(state.relay_execution_tombstones.by_request.is_empty());
    assert!(
        state
            .completed_receipt("account", &request_id.to_string())
            .is_some()
    );
}

#[test]
fn v6_tombstones_migrate_to_log_and_leave_v7_json_compact() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let request_id = uuid::Uuid::now_v7();
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let tombstone = execution_tombstone(
        "account",
        request_id,
        session_id,
        incarnation_id,
        "requester-device",
    );
    let persisted = serde_json::json!({
        "version": TOMBSTONE_JSON_FILE_VERSION,
        "entries": [],
        "relayExecutionTombstones": [tombstone]
    });
    std::fs::write(&path, serde_json::to_vec_pretty(&persisted).expect("json"))
        .expect("persist v6 state");

    let state = SteeringState::load(path.clone()).expect("migrate v6");
    assert!(
        state
            .relay_execution_tombstone("account", &request_id.to_string())
            .is_some()
    );
    let rewritten: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read v7 JSON"))
            .expect("decode v7 JSON");
    assert_eq!(rewritten["version"], STEER_FILE_VERSION);
    assert_eq!(
        rewritten["relayExecutionTombstones"]
            .as_array()
            .expect("tombstone array")
            .len(),
        0
    );
    drop(state);

    let restarted = SteeringState::load(path).expect("restart v7");
    assert!(
        restarted
            .relay_execution_tombstone("account", &request_id.to_string())
            .is_some()
    );
}

#[test]
fn v6_schema_rejects_conflicting_execution_tombstones() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let request_id = uuid::Uuid::now_v7();
    let tombstone = serde_json::json!({
        "accountUserId": "account",
        "requestId": request_id,
        "sessionId": SessionId::new(),
        "sessionIncarnationId": uuid::Uuid::now_v7(),
        "mode": "queue",
        "payloadSha256": payload_sha256("done"),
        "requesterDeviceId": "requester-device",
        "outcome": "injected"
    });
    let current = serde_json::json!({
        "version": TOMBSTONE_JSON_FILE_VERSION,
        "entries": [],
        "relayExecutionTombstones": [tombstone.clone(), tombstone]
    });
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&current).expect("current JSON"),
    )
    .expect("write current file");

    let error = SteeringState::load(path).expect_err("duplicate tombstones must fail closed");
    assert!(matches!(error, AppError::Unsupported { .. }));
    assert!(
        error
            .to_string()
            .contains("tombstone identity is duplicated")
    );
}

#[test]
fn current_version_requires_session_incarnation() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let current = serde_json::json!({
        "version": STEER_FILE_VERSION,
        "entries": [{
            "steerId": "unscoped",
            "accountUserId": "account",
            "requestId": uuid::Uuid::now_v7(),
            "mode": "queue",
            "sessionId": SessionId::new(),
            "text": "must be rejected",
            "queuedAtMs": 1,
            "deliveryState": "queued"
        }]
    });
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&current).expect("current JSON"),
    )
    .expect("write current file");

    let error = SteeringState::load(path).expect_err("current schema must stay strict");
    assert!(matches!(error, AppError::Json(_)));
    assert!(error.to_string().contains("sessionIncarnationId"));
}

#[test]
fn unsupported_version_is_rejected_before_entry_schema_validation() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let unsupported = serde_json::json!({
        "version": STEER_FILE_VERSION + 1,
        "entries": [{
            "steerId": "legacy-shaped"
        }]
    });
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&unsupported).expect("unsupported JSON"),
    )
    .expect("write unsupported file");

    let error = SteeringState::load(path).expect_err("unsupported schema");
    assert!(matches!(error, AppError::Unsupported { .. }));
    assert!(
        error
            .to_string()
            .contains("unsupported pending-steer file version")
    );
}

#[test]
fn clearing_one_account_preserves_other_account_receipts() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let session_id = SessionId::new();
    let incarnation_id = uuid::Uuid::now_v7();
    let mut state = SteeringState::load(path.clone()).expect("steering state");
    for account in ["account-a", "account-b"] {
        let entry = state
            .queue_semantic(
                account.to_owned(),
                uuid::Uuid::now_v7(),
                session_id,
                incarnation_id,
                SemanticSendMode::Queue,
                format!("message for {account}"),
            )
            .expect("queue semantic send");
        state
            .complete_semantic(
                &entry,
                SteerDeliveryState::Injected,
                SemanticSendOrigin::DirectLocal,
            )
            .expect("completion persists");
    }

    state.clear_account("account-a").expect("account clear");
    let restarted = SteeringState::load(path).expect("reload after clear");
    assert!(
        restarted
            .completed
            .iter()
            .all(|receipt| receipt.account_user_id == "account-b")
    );
    assert_eq!(restarted.completed.len(), 1);
}

#[test]
fn steering_queue_survives_restart_and_never_replays_unknown_delivery() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let session_id = SessionId::new();
    let mut state = SteeringState::load(path.clone()).expect("steering state");
    let entry = state
        .queue(session_id, "update the tests".to_owned())
        .expect("queue steer");

    let mut restarted = SteeringState::load(path.clone()).expect("reload queued steer");
    assert_eq!(restarted.entries(session_id), vec![entry.clone()]);
    restarted
        .note_tool_boundary("", session_id, Some("tool-1".to_owned()))
        .expect("first tool boundary");
    let (claimed, boundary) = restarted
        .claim_ready("")
        .expect("claim persists")
        .expect("queued steer is ready");
    assert_eq!(claimed.delivery_state, SteerDeliveryState::DeliveryUnknown);
    assert_eq!(boundary.as_deref(), Some("tool-1"));

    let mut after_crash = SteeringState::load(path.clone()).expect("reload claimed steer");
    assert_eq!(
        after_crash.entries(session_id)[0].delivery_state,
        SteerDeliveryState::DeliveryUnknown
    );
    after_crash
        .note_tool_boundary("", session_id, Some("tool-2".to_owned()))
        .expect("second tool boundary");
    assert!(
        after_crash.claim_ready("").expect("claim check").is_none(),
        "delivery-unknown steer must never be injected twice after restart"
    );
    after_crash
        .requeue(&entry.steer_id)
        .expect("explicit requeue");
    after_crash
        .note_tool_boundary("", session_id, Some("tool-3".to_owned()))
        .expect("third tool boundary");
    assert!(after_crash.claim_ready("").expect("second claim").is_some());
    after_crash
        .complete(&entry.steer_id)
        .expect("complete steer");
    assert!(
        SteeringState::load(path)
            .expect("final reload")
            .entries(session_id)
            .is_empty()
    );
}

#[test]
fn cancelled_steer_is_removed_durably() {
    let directory = tempfile::tempdir().expect("steering tempdir");
    let path = directory.path().join("pending-steers.json");
    let session_id = SessionId::new();
    let mut state = SteeringState::load(path.clone()).expect("steering state");
    let entry = state
        .queue(session_id, "do not send this".to_owned())
        .expect("queue steer");
    assert!(
        state
            .cancel(session_id, &entry.steer_id)
            .expect("cancel persists")
            .is_some()
    );
    assert!(
        SteeringState::load(path)
            .expect("reload")
            .entries(session_id)
            .is_empty()
    );
}
