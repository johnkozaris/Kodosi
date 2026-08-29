use super::*;

fn store(root: &Path, capacity: usize) -> CollaborationTeardownObligationStore {
    CollaborationTeardownObligationStore::at(root.join(STORE_FILE_NAME), capacity)
        .expect("isolated store")
}

fn v7(value: u128) -> Uuid {
    let mut bytes = value.to_be_bytes();
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn origin(value: &str) -> BackendOrigin {
    value.parse().expect("canonical backend origin")
}

fn provision(
    store: &CollaborationTeardownObligationStore,
    session: &str,
    create_id: Uuid,
    end_id: Uuid,
) -> ProvisionOutcome {
    store
        .provision(
            &origin("https://example.com:443/"),
            "account-subject",
            session,
            create_id,
            end_id,
            1_700_000_000_000,
        )
        .expect("provision")
}

#[test]
fn pre_replace_failure_preserves_prior_durable_state() {
    let root = tempfile::tempdir().expect("isolated root");
    let store = store(root.path(), 8);
    provision(&store, "session-a", v7(4_000), v7(4_001));
    let prior = fs::read(&store.path).expect("prior durable state");
    let candidate = ObligationFile {
        version: FILE_SCHEMA_VERSION,
        records: vec![],
        quarantined_records: vec![],
    };

    let error = store
        .with_lock(|_| {
            store.persist_payload_with_recovery_under_lock(
                &candidate,
                |_| {
                    Err(AtomicWriteFailure::NotReplaced(AppError::Io(
                        io::Error::other("simulated pre-replace failure"),
                    )))
                },
                || panic!("a pre-replace failure must not retry parent sync"),
            )
        })
        .expect_err("pre-replace failure must remain an error");

    assert!(matches!(error, AppError::Io(_)));
    assert_eq!(fs::read(&store.path).expect("retained prior state"), prior);
    assert_eq!(store.durable_state(), DurableStoreState::Unavailable);
    let restarted = CollaborationTeardownObligationStore::at(root.path().join(STORE_FILE_NAME), 8)
        .expect("restart store");
    assert_eq!(
        restarted
            .list_for_account(&origin("https://example.com:443/"), "account-subject")
            .expect("restart reads prior state")
            .len(),
        1
    );
}

#[test]
fn post_replace_failure_reloads_visible_candidate_and_fails_closed() {
    let root = tempfile::tempdir().expect("isolated root");
    let store = store(root.path(), 8);
    let candidate_record = TeardownObligation {
        backend_origin: origin("https://example.com:443/"),
        account_subject: "account-subject".to_owned(),
        backend_session_id: "session-candidate".to_owned(),
        create_idempotency_id: v7(4_010),
        backend_incarnation_id: None,
        end_mutation_id: v7(4_011),
        created_at_ms: 1_700_000_000_000,
    };
    let mut candidate = ObligationFile {
        version: FILE_SCHEMA_VERSION,
        records: vec![candidate_record.clone()],
        quarantined_records: vec![],
    };
    candidate.validate_and_sort().expect("candidate");

    let error = store
        .with_lock(|_| {
            store.persist_payload_with_recovery_under_lock(
                &candidate,
                |payload| {
                    fs::write(&store.path, payload).expect("simulate visible replacement");
                    Err(AtomicWriteFailure::ReplacedDurabilityUncertain(
                        AppError::Io(io::Error::other("simulated post-replace failure")),
                    ))
                },
                || Err(AppError::Io(io::Error::other("simulated retry failure"))),
            )
        })
        .expect_err("uncertain replacement must fail closed");

    assert!(error.to_string().contains("candidate is visible"));
    assert_eq!(store.durable_state(), DurableStoreState::Unavailable);
    let restarted = CollaborationTeardownObligationStore::at(root.path().join(STORE_FILE_NAME), 8)
        .expect("restart store");
    assert_eq!(
        restarted
            .list_for_account(&origin("https://example.com:443/"), "account-subject")
            .expect("restart reads visible candidate"),
        vec![candidate_record]
    );
}

#[test]
fn post_replace_third_state_fails_closed_without_claiming_candidate() {
    let root = tempfile::tempdir().expect("isolated root");
    let store = store(root.path(), 8);
    let candidate = ObligationFile::default();
    let third_record = TeardownObligation {
        backend_origin: origin("https://example.com:443/"),
        account_subject: "account-subject".to_owned(),
        backend_session_id: "third-state".to_owned(),
        create_idempotency_id: v7(4_020),
        backend_incarnation_id: None,
        end_mutation_id: v7(4_021),
        created_at_ms: 1_700_000_000_000,
    };
    let mut third = ObligationFile {
        version: FILE_SCHEMA_VERSION,
        records: vec![third_record.clone()],
        quarantined_records: vec![],
    };
    third.validate_and_sort().expect("third state");

    let error = store
        .with_lock(|_| {
            store.persist_payload_with_recovery_under_lock(
                &candidate,
                |_| {
                    fs::write(
                        &store.path,
                        serde_json::to_vec_pretty(&third).expect("third payload"),
                    )
                    .expect("simulate third state");
                    Err(AtomicWriteFailure::ReplacedDurabilityUncertain(
                        AppError::Io(io::Error::other("simulated post-replace failure")),
                    ))
                },
                || Err(AppError::Io(io::Error::other("simulated retry failure"))),
            )
        })
        .expect_err("unexpected visible state must fail closed");

    assert!(
        error
            .to_string()
            .contains("unexpected prior or third state is visible")
    );
    assert_eq!(store.durable_state(), DurableStoreState::Unavailable);
    let restarted = CollaborationTeardownObligationStore::at(root.path().join(STORE_FILE_NAME), 8)
        .expect("restart store");
    assert_eq!(
        restarted
            .list_for_account(&origin("https://example.com:443/"), "account-subject")
            .expect("restart reads third state"),
        vec![third_record]
    );
}

#[test]
fn crash_reload_preserves_provisional_and_bound_records() {
    let root = tempfile::tempdir().expect("isolated root");
    let create_id = v7(1);
    let end_id = v7(2);
    let incarnation_id = v7(3);
    let first = store(root.path(), 8);
    assert!(matches!(
        provision(&first, "session-a", create_id, end_id),
        ProvisionOutcome::Inserted(_)
    ));
    drop(first);

    let reloaded = store(root.path(), 8);
    let provisional = reloaded
        .list_for_account(&origin("https://example.com:443/"), "account-subject")
        .expect("reload provisional");
    assert_eq!(provisional.len(), 1);
    assert_eq!(provisional[0].backend_incarnation_id, None);
    assert_eq!(
        reloaded
            .bind_incarnation(create_id, "session-a", incarnation_id)
            .expect("bind"),
        BindOutcome::Bound
    );
    drop(reloaded);

    let reloaded = store(root.path(), 8);
    assert_eq!(
        reloaded
            .list_for_account(&origin("https://example.com:443/"), "account-subject")
            .expect("reload bound")[0]
            .backend_incarnation_id,
        Some(incarnation_id)
    );
}

#[test]
fn duplicate_union_is_idempotent_and_capacity_counts_unique_records() {
    let root = tempfile::tempdir().expect("isolated root");
    let store = store(root.path(), 1);
    let create_id = v7(10);
    let end_id = v7(11);
    let first = provision(&store, "session-a", create_id, end_id);
    let duplicate = provision(&store, "session-a", create_id, end_id);
    assert!(matches!(first, ProvisionOutcome::Inserted(_)));
    assert!(matches!(duplicate, ProvisionOutcome::Existing(_)));
    assert_eq!(first.record(), duplicate.record());
    assert!(
        store
            .provision(
                &origin("https://example.com:443/"),
                "account-subject",
                "session-b",
                v7(12),
                v7(13),
                1_700_000_000_001,
            )
            .is_err()
    );
    assert_eq!(
        store
            .list_for_account(&origin("https://example.com:443/"), "account-subject")
            .expect("list")
            .len(),
        1
    );
}

#[test]
fn list_filters_and_orders_deterministically() {
    let root = tempfile::tempdir().expect("isolated root");
    let store = store(root.path(), 8);
    provision(&store, "session-z", v7(20), v7(21));
    provision(&store, "session-a", v7(22), v7(23));
    store
        .provision(
            &origin("https://other.example:443/"),
            "account-subject",
            "session-other",
            v7(24),
            v7(25),
            1,
        )
        .expect("other backend");
    store
        .provision(
            &origin("https://example.com:443/"),
            "other-account",
            "session-other-account",
            v7(26),
            v7(27),
            1,
        )
        .expect("other account");

    let sessions: Vec<_> = store
        .list_for_account(&origin("https://example.com:443/"), "account-subject")
        .expect("filtered list")
        .into_iter()
        .map(|record| record.backend_session_id)
        .collect();
    assert_eq!(sessions, ["session-a", "session-z"]);
}

#[test]
fn exact_ack_and_bind_mismatches_leave_record_intact() {
    let root = tempfile::tempdir().expect("isolated root");
    let store = store(root.path(), 8);
    let create_id = v7(30);
    let end_id = v7(31);
    let incarnation_id = v7(32);
    provision(&store, "session-a", create_id, end_id);

    assert!(
        store
            .bind_incarnation(create_id, "session-wrong", incarnation_id)
            .is_err()
    );
    assert_eq!(
        store
            .bind_incarnation(create_id, "session-a", incarnation_id)
            .expect("exact bind"),
        BindOutcome::Bound
    );
    assert_eq!(
        store
            .bind_incarnation(create_id, "session-a", incarnation_id)
            .expect("idempotent bind"),
        BindOutcome::AlreadyBound
    );
    assert!(
        store
            .bind_incarnation(create_id, "session-a", v7(33))
            .is_err()
    );
    assert!(
        store
            .acknowledge(create_id, Some(incarnation_id), v7(34))
            .is_err()
    );
    assert!(store.acknowledge(create_id, Some(v7(35)), end_id).is_err());
    assert_eq!(
        store
            .list_for_account(&origin("https://example.com:443/"), "account-subject")
            .expect("record remains")
            .len(),
        1
    );
    assert_eq!(
        store
            .acknowledge(create_id, Some(incarnation_id), end_id)
            .expect("exact ack"),
        AcknowledgeOutcome::Acknowledged
    );
    assert_eq!(
        store
            .acknowledge(create_id, Some(incarnation_id), end_id)
            .expect("missing ack"),
        AcknowledgeOutcome::NotFound
    );
}

#[test]
fn provisional_ack_is_exact_and_bound_records_reject_provisional_ack() {
    let root = tempfile::tempdir().expect("isolated root");
    let store = store(root.path(), 8);
    let provisional_create = v7(36);
    let provisional_end = v7(37);
    provision(
        &store,
        "session-provisional",
        provisional_create,
        provisional_end,
    );
    assert_eq!(
        store
            .acknowledge(provisional_create, None, provisional_end)
            .expect("exact provisional ack"),
        AcknowledgeOutcome::Acknowledged
    );

    let bound_create = v7(38);
    let bound_end = v7(39);
    let incarnation = v7(40);
    provision(&store, "session-bound", bound_create, bound_end);
    store
        .bind_incarnation(bound_create, "session-bound", incarnation)
        .expect("bind");
    assert!(store.acknowledge(bound_create, None, bound_end).is_err());
    assert_eq!(
        store
            .list_for_account(&origin("https://example.com:443/"), "account-subject")
            .expect("bound record remains")
            .len(),
        1
    );
}

#[test]
fn partition_and_quarantine_limits_preserve_fair_active_admission() {
    let root = tempfile::tempdir().expect("isolated root");
    let store = CollaborationTeardownObligationStore::with_capacities(
        root.path().join(STORE_FILE_NAME),
        3,
        1,
        1,
    )
    .expect("bounded store");
    let primary_origin = origin("https://example.com:443/");
    let other_origin = origin("https://other.example:443/");

    store
        .provision(&primary_origin, "account-a", "session-a", v7(80), v7(81), 1)
        .expect("first partition record");
    let partition_error = store
        .provision(
            &primary_origin,
            "account-a",
            "session-a-2",
            v7(82),
            v7(83),
            2,
        )
        .expect_err("one partition must not consume another partition's reserve");
    assert!(partition_error.to_string().contains("partition capacity 1"));
    store
        .provision(&primary_origin, "account-b", "session-b", v7(84), v7(85), 3)
        .expect("other account partition remains admissible");

    store
        .quarantine(v7(80), None, v7(81), QuarantineReason::MutationConflict)
        .expect("fill quarantine");
    store
        .provision(&other_origin, "account-c", "session-c", v7(86), v7(87), 4)
        .expect("quarantine does not consume active capacity");
    let quarantine = store
        .quarantine(v7(84), None, v7(85), QuarantineReason::MutationConflict)
        .expect("full quarantine returns a typed retained outcome");
    assert_eq!(quarantine, QuarantineOutcome::CapacityFull);

    let retained = store
        .list_for_account(&primary_origin, "account-b")
        .expect("retained active record");
    assert_eq!(retained.len(), 1);
    assert_eq!(retained[0].backend_session_id, "session-b");
    assert_eq!(
        store.validate_and_health().expect("health"),
        StoreHealth {
            active_count: 2,
            quarantined_count: 1,
            durable_state: DurableStoreState::Healthy,
        }
    );
}

#[test]
fn quarantine_preserves_identity_fence_without_consuming_active_capacity() {
    let root = tempfile::tempdir().expect("isolated root");
    let store = store(root.path(), 1);
    let create_id = v7(41);
    let end_id = v7(42);
    provision(&store, "session-a", create_id, end_id);
    assert!(store.contains_create_id(create_id).expect("active lookup"));
    assert_eq!(
        store
            .quarantine(create_id, None, end_id, QuarantineReason::MutationConflict,)
            .expect("quarantine"),
        QuarantineOutcome::Quarantined
    );
    assert!(
        store
            .contains_create_id(create_id)
            .expect("quarantined lookup")
    );
    assert!(
        store
            .provision(
                &origin("https://example.com:443/"),
                "account-subject",
                "session-a",
                v7(43),
                v7(44),
                2,
            )
            .is_err()
    );
    provision(&store, "session-b", v7(45), v7(46));
    assert_eq!(
        store.validate_and_health().expect("health"),
        StoreHealth {
            active_count: 1,
            quarantined_count: 1,
            durable_state: DurableStoreState::Healthy,
        }
    );
}

#[test]
fn exact_quarantine_moves_only_the_target_record() {
    let root = tempfile::tempdir().expect("isolated root");
    let store = store(root.path(), 8);
    let create_id = v7(40);
    let end_id = v7(41);
    provision(&store, "session-a", create_id, end_id);
    provision(&store, "session-b", v7(42), v7(43));
    assert!(
        store
            .quarantine(
                create_id,
                Some(v7(44)),
                end_id,
                QuarantineReason::InvalidRemoteIdentity,
            )
            .is_err()
    );
    assert_eq!(
        store
            .quarantine(
                create_id,
                None,
                end_id,
                QuarantineReason::OperatorIntervention,
            )
            .expect("exact quarantine"),
        QuarantineOutcome::Quarantined
    );
    let remaining = store
        .list_for_account(&origin("https://example.com:443/"), "account-subject")
        .expect("remaining");
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].backend_session_id, "session-b");
}

#[test]
fn concurrent_store_handles_do_not_lose_updates() {
    let root = tempfile::tempdir().expect("isolated root");
    let path = root.path().join(STORE_FILE_NAME);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let mut workers = Vec::new();
    for (session, create_id, end_id) in
        [("session-a", v7(47), v7(48)), ("session-b", v7(49), v7(50))]
    {
        let path = path.clone();
        let barrier = std::sync::Arc::clone(&barrier);
        workers.push(std::thread::spawn(move || {
            let store = CollaborationTeardownObligationStore::at(path, 2).expect("store");
            barrier.wait();
            provision(&store, session, create_id, end_id);
        }));
    }
    barrier.wait();
    for worker in workers {
        worker.join().expect("worker");
    }
    let store = store(root.path(), 2);
    assert_eq!(
        store.validate_and_health().expect("health"),
        StoreHealth {
            active_count: 2,
            quarantined_count: 0,
            durable_state: DurableStoreState::Healthy,
        }
    );
    assert!(
        store
            .provision(
                &origin("https://example.com:443/"),
                "account-subject",
                "session-c",
                v7(51),
                v7(52),
                3,
            )
            .is_err()
    );
}

#[test]
fn corruption_quarantine_blocks_every_mutator_under_the_store_lock() {
    let root = tempfile::tempdir().expect("isolated root");
    let store = store(root.path(), 8);
    let create_id = v7(90);
    let end_id = v7(91);
    provision(&store, "session-a", create_id, end_id);
    let primary_path = root.path().join(STORE_FILE_NAME);
    let primary_before = fs::read(&primary_path).expect("primary before sidecar");
    let sidecar_path = root
        .path()
        .join("collaboration-teardown-obligations.corrupt-guard.json");
    let sidecar_before = b"preserved corruption evidence";
    fs::write(&sidecar_path, sidecar_before).expect("sidecar");

    assert!(
        store
            .provision(
                &origin("https://example.com:443/"),
                "account-subject",
                "session-b",
                v7(92),
                v7(93),
                2,
            )
            .is_err()
    );
    assert!(
        store
            .bind_incarnation(create_id, "session-a", v7(94))
            .is_err()
    );
    assert!(store.acknowledge(create_id, None, end_id).is_err());
    assert!(
        store
            .quarantine(
                create_id,
                None,
                end_id,
                QuarantineReason::OperatorIntervention
            )
            .is_err()
    );
    assert_eq!(
        fs::read(primary_path).expect("primary preserved"),
        primary_before
    );
    assert_eq!(
        fs::read(sidecar_path).expect("sidecar preserved"),
        sidecar_before
    );
}

#[test]
fn oversized_valid_store_is_rejected_before_replacing_prior_bytes() {
    let root = tempfile::tempdir().expect("isolated root");
    let store = CollaborationTeardownObligationStore::with_capacities(
        root.path().join(STORE_FILE_NAME),
        DEFAULT_CAPACITY,
        DEFAULT_CAPACITY,
        DEFAULT_QUARANTINE_CAPACITY,
    )
    .expect("large test capacities");
    provision(&store, "baseline", v7(100), v7(101));
    let primary_path = root.path().join(STORE_FILE_NAME);
    let prior = fs::read(&primary_path).expect("prior store");
    let long_account = "a".repeat(MAX_IDENTIFIER_BYTES);
    let session_suffix = "s".repeat(MAX_IDENTIFIER_BYTES - 6);
    let backend_origin = origin("https://example.com:443/");
    let mut file = ObligationFile::default();
    for index in 0_u128..u128::try_from(DEFAULT_CAPACITY).expect("active capacity") {
        file.records.push(TeardownObligation {
            backend_origin: backend_origin.clone(),
            account_subject: long_account.clone(),
            backend_session_id: format!("a{index:04}-{session_suffix}"),
            create_idempotency_id: v7(1_000 + index * 2),
            backend_incarnation_id: None,
            end_mutation_id: v7(1_001 + index * 2),
            created_at_ms: i64::try_from(index).expect("timestamp"),
        });
    }
    for index in 0_u128..u128::try_from(DEFAULT_QUARANTINE_CAPACITY).expect("quarantine capacity") {
        file.quarantined_records
            .push(QuarantinedTeardownObligation {
                record: TeardownObligation {
                    backend_origin: backend_origin.clone(),
                    account_subject: long_account.clone(),
                    backend_session_id: format!("q{index:04}-{session_suffix}"),
                    create_idempotency_id: v7(10_000 + index * 2),
                    backend_incarnation_id: None,
                    end_mutation_id: v7(10_001 + index * 2),
                    created_at_ms: i64::try_from(index).expect("timestamp"),
                },
                reason: QuarantineReason::OperatorIntervention,
            });
    }

    let error = store
        .with_lock(|_| store.persist_under_lock(&mut file))
        .expect_err("writer must enforce its own reader limit");
    assert!(error.to_string().contains("exceeds"));
    assert_eq!(fs::read(primary_path).expect("prior remains"), prior);
}

#[test]
fn malformed_current_schema_is_quarantined_across_restart_until_explicit_reset() {
    let root = tempfile::tempdir().expect("isolated root");
    let path = root.path().join(STORE_FILE_NAME);
    let malformed = br#"{
      "version": 1,
      "records": [{"backendOrigin":"https://example.com:443/"}],
      "quarantinedRecords": []
    }"#;
    fs::write(&path, malformed).expect("malformed current-schema store");

    let first = store(root.path(), 8);
    assert_eq!(
        first.validate_and_health().expect("quarantine malformed"),
        StoreHealth {
            active_count: 0,
            quarantined_count: 0,
            durable_state: DurableStoreState::CorruptionQuarantined,
        }
    );
    let sidecar = corruption_quarantine_paths(root.path())
        .expect("quarantine paths")
        .pop()
        .expect("quarantine sidecar");
    assert_eq!(fs::read(&sidecar).expect("preserved bytes"), malformed);
    drop(first);

    let restarted = CollaborationTeardownObligationStore::at(root.path().join(STORE_FILE_NAME), 8)
        .expect("restart store");
    assert_eq!(
        restarted.durable_state(),
        DurableStoreState::CorruptionQuarantined
    );
    assert!(restarted.validate_and_health().is_ok());
    assert_eq!(
        restarted
            .reset_corruption_quarantine()
            .expect("explicit operator reset"),
        1
    );
    assert_eq!(restarted.durable_state(), DurableStoreState::Healthy);
    assert!(!sidecar.exists(), "active sidecar name is retired");
    let resolved = fs::read_dir(root.path())
        .expect("read root")
        .filter_map(std::result::Result::ok)
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("collaboration-teardown-obligations.resolved-")
        })
        .expect("preserved resolved evidence")
        .path();
    assert_eq!(fs::read(resolved).expect("resolved bytes"), malformed);
    assert_eq!(
        store(root.path(), 8).durable_state(),
        DurableStoreState::Healthy,
        "operator resolution persists across restart"
    );
}

#[test]
fn malformed_file_is_renamed_and_store_remains_usable() {
    let root = tempfile::tempdir().expect("isolated root");
    let unrelated = root.path().join("unrelated.txt");
    fs::write(&unrelated, b"keep me").expect("unrelated");
    let path = root.path().join(STORE_FILE_NAME);
    fs::write(&path, b"{not-json").expect("malformed store");
    let store = store(root.path(), 8);

    assert_eq!(
        store.validate_and_health().expect("quarantine malformed"),
        StoreHealth {
            active_count: 0,
            quarantined_count: 0,
            durable_state: DurableStoreState::CorruptionQuarantined,
        }
    );
    assert!(
        store
            .list_for_account(&origin("https://example.com:443/"), "account-subject")
            .expect("empty usable store")
            .is_empty()
    );
    assert!(!path.exists());
    assert_eq!(fs::read(&unrelated).expect("unrelated remains"), b"keep me");
    let corrupt_files = fs::read_dir(root.path())
        .expect("read root")
        .filter_map(std::result::Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("collaboration-teardown-obligations.corrupt-")
        })
        .count();
    assert_eq!(corrupt_files, 1);
    assert!(
        store
            .provision(
                &origin("https://example.com:443/"),
                "account-subject",
                "session-a",
                v7(50),
                v7(51),
                1,
            )
            .is_err(),
        "corruption quarantine must keep collaboration provisioning read-only"
    );
}

#[test]
fn failed_corruption_quarantine_never_silently_loses_the_file() {
    let root = tempfile::tempdir().expect("isolated root");
    let path = root.path().join(STORE_FILE_NAME);
    fs::write(&path, b"{not-json").expect("malformed store");
    let store = store(root.path(), 8);

    support_fs::set_dir_permissions(root.path()).expect("private root");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o500))
            .expect("read-only root");
        let result = store.list_for_account(&origin("https://example.com:443/"), "account-subject");
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).expect("restore root");

        if result.is_err() {
            assert_eq!(
                fs::read(&path).expect("malformed file retained"),
                b"{not-json"
            );
        }
    }
}

#[test]
fn future_schema_fails_closed_without_rewriting() {
    let root = tempfile::tempdir().expect("isolated root");
    let path = root.path().join(STORE_FILE_NAME);
    let payload = br#"{"version":999,"records":[{"unknown":"preserve"}]}"#;
    fs::write(&path, payload).expect("future store");
    let store = store(root.path(), 8);

    assert!(store.validate_and_health().is_err());
    assert!(
        store
            .list_for_account(&origin("https://example.com:443/"), "account-subject")
            .is_err()
    );
    assert!(
        store
            .provision(
                &origin("https://example.com:443/"),
                "account-subject",
                "session-a",
                v7(60),
                v7(61),
                1,
            )
            .is_err()
    );
    assert_eq!(fs::read(path).expect("future file preserved"), payload);
}

#[cfg(unix)]
#[test]
fn rejects_store_and_lock_symlink_attacks() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().expect("isolated root");
    let target = root.path().join("target");
    fs::write(&target, b"untouched").expect("target");
    let path = root.path().join(STORE_FILE_NAME);
    symlink(&target, &path).expect("store symlink");
    assert!(CollaborationTeardownObligationStore::at(path.clone(), 8).is_err());
    assert_eq!(fs::read(&target).expect("target untouched"), b"untouched");

    fs::remove_file(&path).expect("remove store symlink");
    let store = store(root.path(), 8);
    fs::remove_file(root.path().join(LOCK_FILE_NAME)).expect("remove regular lock");
    symlink(&target, root.path().join(LOCK_FILE_NAME)).expect("lock symlink");
    assert!(
        store
            .list_for_account(&origin("https://example.com:443/"), "account-subject")
            .is_err()
    );
    assert_eq!(
        fs::read(target).expect("lock target untouched"),
        b"untouched"
    );
}

#[cfg(unix)]
#[test]
fn persisted_store_lock_and_directory_are_private() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().expect("isolated root");
    let store = store(root.path(), 8);
    provision(&store, "session-a", v7(70), v7(71));

    let mode = |path: &Path| fs::metadata(path).expect("metadata").permissions().mode() & 0o777;
    assert_eq!(mode(root.path()), 0o700);
    assert_eq!(mode(&root.path().join(STORE_FILE_NAME)), 0o600);
    assert_eq!(mode(&root.path().join(LOCK_FILE_NAME)), 0o600);
}
