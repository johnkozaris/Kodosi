use super::*;
use crate::runtime::share_transitions::{
    PreparedShareTransition, ShareAudience, ShareTransitionCleanupIdentity, ShareTransitionState,
};
use kodosi_domain::{ids::SessionId, permissions::ShareScope};

fn transition(account: &str) -> PreparedShareTransition {
    PreparedShareTransition::new(
        Uuid::now_v7(),
        1,
        account.to_owned(),
        7,
        SessionId::new(),
        Uuid::now_v7(),
        ShareAudience {
            scope: ShareScope::JustMe,
            room_id: None,
        },
        ShareAudience {
            scope: ShareScope::MyDevices,
            room_id: None,
        },
        Uuid::now_v7().to_string(),
        None,
        None,
        Some(ShareTransitionCleanupIdentity {
            backend_origin: "https://example.com:443/".to_owned(),
            create_idempotency_id: Uuid::now_v7(),
            end_mutation_id: Uuid::now_v7(),
            created_at_ms: 1,
        }),
    )
    .unwrap()
}

#[test]
fn ledger_survives_restart_and_partitions_accounts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.json");
    let entry = transition("alice");
    let id = entry.transition_id;
    let mut ledger = ShareTransitionLedger::load_at(path.clone()).unwrap();
    ledger.put(entry).unwrap();
    drop(ledger);

    let ledger = ShareTransitionLedger::load_at(path).unwrap();
    assert!(ledger.get("alice", id).unwrap().is_some());
    assert!(ledger.entries("bob").unwrap().is_empty());
}

#[test]
fn duplicate_id_rejects_rebound_intent() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = ShareTransitionLedger::load_at(dir.path().join("ledger.json")).unwrap();
    let first = transition("alice");
    let mut rebound = first.clone();
    rebound.target = ShareAudience {
        scope: ShareScope::Room,
        room_id: Some("room-a".to_owned()),
    };
    rebound.fingerprint = "0".repeat(64);
    ledger.put(first).unwrap();
    assert!(ledger.put(rebound).is_err());
}

#[test]
fn durable_phases_cannot_regress_or_skip_exposure_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = ShareTransitionLedger::load_at(dir.path().join("ledger.json")).unwrap();
    let mut entry = transition("alice");
    ledger.put(entry.clone()).unwrap();

    let mut skipped = entry.clone();
    skipped.state = ShareTransitionState::TargetObserved;
    assert!(ledger.put(skipped).is_err());

    entry.state = ShareTransitionState::ApplyingTarget;
    ledger.put(entry.clone()).unwrap();
    entry.state = ShareTransitionState::TargetObserved;
    ledger.put(entry.clone()).unwrap();
    entry.state = ShareTransitionState::KeyPreparing;
    ledger.put(entry.clone()).unwrap();

    let mut regressed = entry.clone();
    regressed.state = ShareTransitionState::ApplyingTarget;
    assert!(ledger.put(regressed).is_err());
}

#[test]
fn terminal_outcome_cannot_be_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = ShareTransitionLedger::load_at(dir.path().join("ledger.json")).unwrap();
    let mut entry = transition("alice");
    entry.state = ShareTransitionState::CommitReady;
    ledger.put(entry.clone()).unwrap();
    entry.state = ShareTransitionState::Terminal(
        crate::runtime::share_transitions::ShareTransitionTerminal {
            status: crate::runtime::share_transitions::ShareTransitionTerminalStatus::Applied,
            message: None,
        },
    );
    ledger.put(entry.clone()).unwrap();
    let mut rewritten = entry;
    rewritten.state = ShareTransitionState::Terminal(
        crate::runtime::share_transitions::ShareTransitionTerminal {
            status: crate::runtime::share_transitions::ShareTransitionTerminalStatus::Rejected,
            message: Some("different".to_owned()),
        },
    );
    assert!(ledger.put(rewritten).is_err());
}

#[test]
fn terminal_outcomes_are_evicted_before_active_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = ShareTransitionLedger::load_at(dir.path().join("ledger.json")).unwrap();
    let mut first = transition("alice");
    let first_id = first.transition_id;
    first.state = ShareTransitionState::CommitReady;
    ledger.put(first.clone()).unwrap();
    first.state = ShareTransitionState::Terminal(
        crate::runtime::share_transitions::ShareTransitionTerminal {
            status: crate::runtime::share_transitions::ShareTransitionTerminalStatus::Applied,
            message: None,
        },
    );
    ledger.put(first).unwrap();
    for _ in 1..MAX_ENTRIES {
        let mut terminal = transition("alice");
        terminal.state = ShareTransitionState::CommitReady;
        ledger.put(terminal.clone()).unwrap();
        terminal.state = ShareTransitionState::Terminal(
            crate::runtime::share_transitions::ShareTransitionTerminal {
                status: crate::runtime::share_transitions::ShareTransitionTerminalStatus::Applied,
                message: None,
            },
        );
        ledger.put(terminal).unwrap();
    }
    ledger.put(transition("alice")).unwrap();
    assert!(ledger.get("alice", first_id).unwrap().is_none());
    assert_eq!(ledger.entries("alice").unwrap().len(), MAX_ENTRIES);
}

#[test]
fn full_active_ledger_refuses_to_discard_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = ShareTransitionLedger::load_at(dir.path().join("ledger.json")).unwrap();
    for _ in 0..MAX_ENTRIES {
        ledger.put(transition("alice")).unwrap();
    }
    let error = ledger.put(transition("alice")).unwrap_err();
    assert!(error.to_string().contains("full of active evidence"));
}

#[test]
fn unavailable_reset_preserves_bytes_and_installs_empty_v2() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("share-transitions.json");
    let bytes = br#"{"version":1,"accounts":{"legacy":{}}}"#;
    std::fs::write(&path, bytes).unwrap();
    let mut ledger =
        ShareTransitionLedger::unavailable_for_test(path.clone(), "unsupported ledger version 1");

    let evidence = ledger
        .reset_unavailable_preserving_evidence()
        .expect("reset unavailable ledger");

    assert_eq!(evidence.len(), 1);
    assert_eq!(std::fs::read(&evidence[0]).unwrap(), bytes);
    assert!(ledger.ensure_available().is_ok());
    assert!(ledger.entries("legacy").unwrap().is_empty());
    let reloaded = ShareTransitionLedger::load_at(path).unwrap();
    assert!(reloaded.entries("legacy").unwrap().is_empty());
}

#[test]
fn reset_resumes_after_primary_was_already_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("share-transitions.json");
    let unresolved = dir.path().join(format!(
        "share-transitions-unresolved-{}.json",
        Uuid::now_v7()
    ));
    let bytes = b"preserved-before-crash";
    std::fs::write(&unresolved, bytes).unwrap();
    let mut ledger = ShareTransitionLedger::unavailable_for_test(
        path.clone(),
        "primary missing after evidence preservation",
    );

    let evidence = ledger
        .reset_unavailable_preserving_evidence()
        .expect("resume reset");

    assert_eq!(evidence.len(), 1);
    assert_eq!(std::fs::read(&evidence[0]).unwrap(), bytes);
    assert!(
        evidence[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("share-transitions-resolved-")
    );
    assert!(ShareTransitionLedger::load_at(path).is_ok());
}

#[test]
fn reset_resumes_visible_fresh_primary_without_preserving_it_again() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("share-transitions.json");
    std::fs::write(&path, br#"{"version":2,"accounts":{}}"#).unwrap();
    let unresolved = dir.path().join(format!(
        "share-transitions-unresolved-{}.json",
        Uuid::now_v7()
    ));
    let bytes = b"original evidence";
    std::fs::write(&unresolved, bytes).unwrap();
    let mut ledger = ShareTransitionLedger::unavailable_for_test(
        path.clone(),
        "fresh primary durability was interrupted",
    );

    let evidence = ledger
        .reset_unavailable_preserving_evidence()
        .expect("resume visible fresh stage");

    assert_eq!(evidence.len(), 1);
    assert_eq!(std::fs::read(&evidence[0]).unwrap(), bytes);
    assert_eq!(
        std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with("share-transitions-resolved-"))
            .count(),
        1
    );
    assert!(ShareTransitionLedger::load_at(path).is_ok());
}

#[test]
fn reset_resumes_partial_evidence_resolution_without_duplication() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("share-transitions.json");
    std::fs::write(&path, br#"{"version":2,"accounts":{}}"#).unwrap();
    let unresolved = dir.path().join(format!(
        "share-transitions-unresolved-{}.json",
        Uuid::now_v7()
    ));
    let resolved = dir.path().join(format!(
        "share-transitions-resolved-{}.json",
        Uuid::now_v7()
    ));
    std::fs::write(&unresolved, b"preserved-before-crash").unwrap();
    std::fs::hard_link(&unresolved, &resolved).unwrap();
    let historical = dir.path().join(format!(
        "share-transitions-resolved-{}.json",
        Uuid::now_v7()
    ));
    std::fs::write(&historical, b"historical").unwrap();
    let mut ledger =
        ShareTransitionLedger::unavailable_for_test(path.clone(), "partial resolution");

    let evidence = ledger
        .reset_unavailable_preserving_evidence()
        .expect("resume partial resolution");

    assert_eq!(evidence, vec![resolved]);
    assert_eq!(
        std::fs::read(&evidence[0]).unwrap(),
        b"preserved-before-crash"
    );
    assert_eq!(std::fs::read(historical).unwrap(), b"historical");
    assert!(!unresolved.exists());
    assert!(ShareTransitionLedger::load_at(path).is_ok());
}

#[test]
fn reset_preserves_and_reports_multiple_existing_sidecars() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("share-transitions.json");
    for bytes in [b"first".as_slice(), b"second".as_slice()] {
        let unresolved = dir.path().join(format!(
            "share-transitions-unresolved-{}.json",
            Uuid::now_v7()
        ));
        std::fs::write(unresolved, bytes).unwrap();
    }
    let mut ledger = ShareTransitionLedger::unavailable_for_test(path.clone(), "interrupted reset");

    let evidence = ledger
        .reset_unavailable_preserving_evidence()
        .expect("resume all evidence");

    assert_eq!(evidence.len(), 2);
    let mut contents = evidence
        .iter()
        .map(|path| std::fs::read(path).unwrap())
        .collect::<Vec<_>>();
    contents.sort();
    assert_eq!(contents, vec![b"first".to_vec(), b"second".to_vec()]);
    assert!(ShareTransitionLedger::load_at(path).is_ok());
}

#[test]
fn visible_primary_remains_fenced_while_unresolved_evidence_exists() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("share-transitions.json");
    std::fs::write(&path, br#"{"version":2,"accounts":{}}"#).unwrap();
    std::fs::write(
        dir.path().join(format!(
            "share-transitions-unresolved-{}.json",
            Uuid::now_v7()
        )),
        b"original evidence",
    )
    .unwrap();

    let error = ShareTransitionLedger::load_at(path).unwrap_err();

    assert!(error.to_string().contains("unresolved preserved evidence"));
}

#[cfg(unix)]
#[test]
fn primary_and_sidecar_symlinks_are_rejected_without_following() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target.json");
    std::fs::write(&target, br#"{"version":2,"accounts":{}}"#).unwrap();
    let primary = dir.path().join("share-transitions.json");
    symlink(&target, &primary).unwrap();
    assert!(ShareTransitionLedger::load_at(primary).is_err());

    let sidecar = dir.path().join(format!(
        "share-transitions-unresolved-{}.json",
        Uuid::now_v7()
    ));
    symlink(&target, &sidecar).unwrap();
    let missing = dir.path().join("missing-primary.json");
    assert!(ShareTransitionLedger::load_at(missing).is_err());
}

#[test]
fn healthy_ledger_cannot_be_reset() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("share-transitions.json");
    let mut ledger = ShareTransitionLedger::load_at(path).unwrap();
    assert!(ledger.reset_unavailable_preserving_evidence().is_err());
}

#[test]
fn missing_primary_with_unresolved_evidence_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let evidence = dir.path().join(format!(
        "share-transitions-unresolved-{}.json",
        Uuid::now_v7()
    ));
    std::fs::write(evidence, b"legacy").unwrap();
    let error =
        ShareTransitionLedger::load_at(dir.path().join("share-transitions.json")).unwrap_err();
    assert!(error.to_string().contains("unresolved preserved evidence"));
}

#[test]
fn unsupported_version_is_identified_before_row_decoding() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.json");
    std::fs::write(
        &path,
        br#"{"version":1,"accounts":{"alice":{"not-a-uuid":{"legacy":"shape"}}}}"#,
    )
    .unwrap();

    let error = ShareTransitionLedger::load_at(path).unwrap_err();
    assert!(error.to_string().contains("unsupported ledger version 1"));
    assert!(!error.to_string().contains("malformed ledger"));
}

#[test]
fn nonterminal_phase_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.json");
    let mut entry = transition("alice");
    entry.state = ShareTransitionState::CleanupPending(
        crate::runtime::share_transitions::ShareTransitionTerminal {
            status: crate::runtime::share_transitions::ShareTransitionTerminalStatus::Cancelled,
            message: Some("restart cleanup".to_owned()),
        },
    );
    let id = entry.transition_id;
    let mut ledger = ShareTransitionLedger::load_at(path.clone()).unwrap();
    ledger.put(entry).unwrap();
    drop(ledger);
    assert!(matches!(
        ShareTransitionLedger::load_at(path)
            .unwrap()
            .get("alice", id)
            .unwrap()
            .unwrap()
            .state,
        ShareTransitionState::CleanupPending(_)
    ));
}
