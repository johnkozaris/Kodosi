use super::*;
use bytes::Bytes;

fn make_hub() -> SessionHub {
    SessionHub::new()
}

fn revive(hub: &mut SessionHub, session_id: SessionId) {
    assert!(hub.install_semantic_checkpoint(session_id, checkpoint(), 0));
}

fn checkpoint() -> kodosi_domain::terminal::TerminalCheckpointV2 {
    kodosi_domain::terminal::TerminalCheckpointV2::new(
        kodosi_domain::terminal::TerminalSize::new(24, 80).expect("size"),
        kodosi_domain::terminal::TerminalScreen::Primary,
        vec![1],
        0,
        0,
        false,
    )
    .expect("checkpoint")
}

#[test]
fn bootstrap_pending_connection_receives_checkpoint_before_controls_and_data() {
    let mut hub = make_hub();
    let sid = SessionId::new();
    assert!(hub.install_semantic_checkpoint(sid, checkpoint(), 0));
    let mut handle = hub
        .register_bootstrap_pending(sid, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("live session");

    hub.broadcast_control(
        sid,
        &TerminalControlFrame::Resize {
            rows: 40,
            cols: 120,
            at_sequence: 0,
        },
    );
    assert_eq!(
        hub.publish(sid, Bytes::from_static(b"before-checkpoint")),
        Some(0)
    );
    assert!(handle.control_rx.try_recv().is_err());
    assert!(handle.data_rx.try_recv().is_err());

    assert!(hub.send_control(
        sid,
        handle.connection_id,
        TerminalControlFrame::SemanticCheckpoint {
            checkpoint: checkpoint(),
            next_sequence: 1,
        },
    ));
    assert_eq!(
        hub.publish(sid, Bytes::from_static(b"after-checkpoint")),
        Some(1)
    );

    std::assert_matches!(
        handle.control_rx.try_recv().unwrap(),
        TerminalControlFrame::SemanticCheckpoint {
            next_sequence: 1,
            ..
        }
    );
    let frame = handle.data_rx.try_recv().unwrap();
    assert_eq!(frame.sequence, 1);
    assert_eq!(frame.bytes, Bytes::from_static(b"after-checkpoint"));
}

#[test]
fn semantic_checkpoint_cannot_regress_terminal_sequence() {
    let mut hub = make_hub();
    let sid = SessionId::new();

    assert!(hub.install_semantic_checkpoint(sid, checkpoint(), 100));
    assert!(!hub.install_semantic_checkpoint(sid, checkpoint(), 99));
    assert_eq!(hub.next_sequence(sid), Some(100));
    assert!(hub.install_semantic_checkpoint(sid, checkpoint(), 100));
}

#[test]
fn semantic_checkpoint_reviving_tombstone_releases_retired_local_authority() {
    let mut hub = make_hub();
    let session_id = SessionId::new();
    let retired = Uuid::now_v7();
    let replacement = Uuid::now_v7();

    assert!(hub.open_local_incarnation(session_id, retired, 100));
    assert!(hub.end_local_incarnation(
        crate::session_runtime::events::LocalCoordinatorOrigin {
            session_id,
            local_incarnation_id: retired,
        },
        &TerminalCloseReason::SessionEnded,
    ));
    assert!(hub.install_semantic_checkpoint(session_id, checkpoint(), 100));

    assert!(hub.open_local_incarnation(session_id, replacement, 100));
    assert_eq!(
        hub.publish_local(
            crate::session_runtime::events::LocalCoordinatorOrigin {
                session_id,
                local_incarnation_id: retired,
            },
            Bytes::from_static(b"retired"),
        ),
        None
    );
    assert_eq!(
        hub.publish_local(
            crate::session_runtime::events::LocalCoordinatorOrigin {
                session_id,
                local_incarnation_id: replacement,
            },
            Bytes::from_static(b"replacement"),
        ),
        Some(100)
    );
}

#[test]
fn two_connections_receive_same_frames_in_order() {
    let mut hub = make_hub();
    let sid = SessionId::new();
    revive(&mut hub, sid);

    let mut h1 = hub
        .register(sid, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("session should be live");
    let mut h2 = hub
        .register(sid, TerminalSurface::Cli, TerminalCapability::Write)
        .expect("session should be live");

    hub.publish(sid, Bytes::from_static(b"hello"));
    hub.publish(sid, Bytes::from_static(b"world"));

    for handle in [&mut h1, &mut h2] {
        let f0 = handle.data_rx.try_recv().unwrap();
        let f1 = handle.data_rx.try_recv().unwrap();
        assert_eq!(f0.sequence, 0);
        assert_eq!(f0.bytes, Bytes::from_static(b"hello"));
        assert_eq!(f1.sequence, 1);
        assert_eq!(f1.bytes, Bytes::from_static(b"world"));
        assert!(handle.data_rx.try_recv().is_err());
    }
}

#[tokio::test]
async fn cloned_hub_publishes_without_runtime_loop_ownership() {
    let mut hub = make_hub();
    let sid = SessionId::new();
    revive(&mut hub, sid);
    let mut subscriber = hub
        .register(sid, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("session should be live");
    let mut publisher = hub.clone();

    tokio::spawn(async move {
        publisher.publish(sid, Bytes::from_static(b"local-first"));
    })
    .await
    .expect("publisher task");

    let frame = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        subscriber.data_rx.recv(),
    )
    .await
    .expect("subscriber must not wait on runtime state loop")
    .expect("frame");
    assert_eq!(frame.bytes, Bytes::from_static(b"local-first"));
}

#[test]
fn closed_data_channel_detaches_only_that_connection() {
    let mut hub = make_hub();
    let sid = SessionId::new();
    revive(&mut hub, sid);

    let h1 = hub
        .register(sid, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("session should be live");
    let mut h2 = hub
        .register(sid, TerminalSurface::Cli, TerminalCapability::Write)
        .expect("session should be live");

    drop(h1);

    hub.publish(sid, Bytes::from_static(b"after-close"));

    assert_eq!(hub.connection_count(sid), 1);

    let frame = h2.data_rx.try_recv().unwrap();
    assert_eq!(frame.bytes, Bytes::from_static(b"after-close"));
}

#[test]
fn full_data_channel_detaches_only_that_connection() {
    let mut hub = make_hub();
    let sid = SessionId::new();
    revive(&mut hub, sid);

    let _h1 = hub
        .register(sid, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("session should be live");
    let mut h2 = hub
        .register(sid, TerminalSurface::Cli, TerminalCapability::Write)
        .expect("session should be live");

    for i in 0..DATA_CHANNEL_CAPACITY {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "test loop index fits in u8"
        )]
        hub.publish(sid, Bytes::from(vec![i as u8]));
        while h2.data_rx.try_recv().is_ok() {}
    }

    hub.publish(sid, Bytes::from_static(b"overflow"));
    while h2.data_rx.try_recv().is_ok() {}

    assert_eq!(hub.connection_count(sid), 1);

    hub.publish(sid, Bytes::from_static(b"still-alive"));
    let frame = h2.data_rx.try_recv().unwrap();
    assert_eq!(frame.bytes, Bytes::from_static(b"still-alive"));
}

#[test]
fn unregister_stops_delivery() {
    let mut hub = make_hub();
    let sid = SessionId::new();
    revive(&mut hub, sid);

    let h1 = hub
        .register(sid, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("session should be live");
    let mut h2 = hub
        .register(sid, TerminalSurface::Cli, TerminalCapability::Write)
        .expect("session should be live");

    hub.publish(sid, Bytes::from_static(b"before-unreg"));
    hub.unregister(sid, h1.connection_id);
    hub.publish(sid, Bytes::from_static(b"after-unreg"));

    assert_eq!(
        h2.data_rx.try_recv().unwrap().bytes,
        Bytes::from_static(b"before-unreg")
    );
    assert_eq!(
        h2.data_rx.try_recv().unwrap().bytes,
        Bytes::from_static(b"after-unreg")
    );
    assert_eq!(hub.connection_count(sid), 1);
}

#[test]
fn sequence_is_per_session_not_global() {
    let mut hub = make_hub();
    let sid_a = SessionId::new();
    revive(&mut hub, sid_a);
    let sid_b = SessionId::new();
    revive(&mut hub, sid_b);

    let mut ha = hub
        .register(sid_a, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("session should be live");
    let mut hb = hub
        .register(sid_b, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("session should be live");

    hub.publish(sid_a, Bytes::from_static(b"a0"));
    hub.publish(sid_a, Bytes::from_static(b"a1"));
    hub.publish(sid_b, Bytes::from_static(b"b0"));

    let fa0 = ha.data_rx.try_recv().unwrap();
    let fa1 = ha.data_rx.try_recv().unwrap();
    assert_eq!(fa0.sequence, 0);
    assert_eq!(fa1.sequence, 1);

    let fb0 = hb.data_rx.try_recv().unwrap();
    assert_eq!(fb0.sequence, 0);
}

#[test]
fn late_publish_after_end_does_not_resurrect_or_reset_sequence() {
    let mut hub = make_hub();
    let sid = SessionId::new();
    revive(&mut hub, sid);

    assert_eq!(hub.publish(sid, Bytes::from_static(b"a")), Some(0));
    assert_eq!(hub.publish(sid, Bytes::from_static(b"b")), Some(1));
    hub.end_session(sid, &TerminalCloseReason::SessionEnded);

    assert_eq!(
        hub.publish(sid, Bytes::from_static(b"late")),
        None,
        "a publish for an ended session is dropped, not sequenced"
    );
    assert!(
        hub.next_sequence(sid).is_none(),
        "an ended session must not expose a live sequence counter"
    );
    std::assert_matches!(
        hub.close_reason(sid),
        Some(TerminalCloseReason::SessionEnded)
    );
}

#[test]
fn register_after_end_is_refused_rather_than_resurrecting() {
    let mut hub = make_hub();
    let sid = SessionId::new();
    revive(&mut hub, sid);

    hub.publish(sid, Bytes::from_static(b"a"));
    hub.end_session(sid, &TerminalCloseReason::AuthRevoked);

    assert!(
        hub.register(sid, TerminalSurface::Desktop, TerminalCapability::Write)
            .is_none()
    );
    assert_eq!(hub.connection_count(sid), 0);
}

#[test]
fn end_session_for_unknown_session_still_tombstones() {
    let mut hub = make_hub();
    let sid = SessionId::new();

    hub.end_session(sid, &TerminalCloseReason::AuthRevoked);

    assert_eq!(hub.publish(sid, Bytes::from_static(b"late")), None);
    std::assert_matches!(
        hub.close_reason(sid),
        Some(TerminalCloseReason::AuthRevoked)
    );
}

#[test]
fn expired_tombstone_cannot_be_recreated_by_stale_publish_or_attach() {
    let mut hub = make_hub();
    let sid = SessionId::new();
    assert!(hub.install_semantic_checkpoint(sid, checkpoint(), 3));
    hub.end_session(sid, &TerminalCloseReason::SessionEnded);
    if let Some(SessionState {
        lifecycle: Lifecycle::Ended { at, .. },
        ..
    }) = hub.lock().get_mut(&sid)
    {
        *at = std::time::Instant::now()
            .checked_sub(TOMBSTONE_TTL + std::time::Duration::from_secs(1))
            .expect("test instant should support one minute subtraction");
    }

    assert_eq!(hub.publish(sid, Bytes::from_static(b"late")), None);
    assert!(
        hub.register(sid, TerminalSurface::Desktop, TerminalCapability::Write)
            .is_none()
    );
    assert_eq!(hub.next_sequence(sid), None);
    assert_eq!(hub.connection_count(sid), 0);
}

#[test]
fn overlapping_local_incarnation_is_rejected_without_disturbing_current_authority() {
    let hub = make_hub();
    let session_id = SessionId::new();
    let current = Uuid::now_v7();
    let overlap = Uuid::now_v7();

    assert!(hub.open_local_incarnation(session_id, current, 0));
    assert!(!hub.open_local_incarnation(session_id, overlap, 0));
    assert_eq!(hub.next_sequence(session_id), Some(0));
    assert_eq!(
        hub.publish_local(
            crate::session_runtime::events::LocalCoordinatorOrigin {
                session_id,
                local_incarnation_id: overlap,
            },
            Bytes::from_static(b"stale"),
        ),
        None
    );
    assert_eq!(
        hub.publish_local(
            crate::session_runtime::events::LocalCoordinatorOrigin {
                session_id,
                local_incarnation_id: current,
            },
            Bytes::from_static(b"current"),
        ),
        Some(0)
    );
    assert!(!hub.end_local_incarnation(
        crate::session_runtime::events::LocalCoordinatorOrigin {
            session_id,
            local_incarnation_id: overlap,
        },
        &TerminalCloseReason::SessionEnded,
    ));
    assert_eq!(hub.next_sequence(session_id), Some(1));
}

#[test]
fn semantic_checkpoint_restores_absent_authority_and_rejects_regression() {
    let mut hub = make_hub();
    let restored = SessionId::new();
    assert!(hub.install_semantic_checkpoint(restored, checkpoint(), 41));
    assert_eq!(hub.next_sequence(restored), Some(41));
    assert_eq!(hub.publish(restored, Bytes::from_static(b"next")), Some(41));

    assert!(
        !hub.install_semantic_checkpoint(restored, checkpoint(), 9),
        "an existing authority cannot be overwritten by a conflicting persisted baseline"
    );
    assert_eq!(hub.next_sequence(restored), Some(42));

    hub.end_session(restored, &TerminalCloseReason::SessionEnded);
    assert!(hub.install_semantic_checkpoint(restored, checkpoint(), 42));
    assert_eq!(hub.next_sequence(restored), Some(42));
}

#[test]
fn revive_clears_the_tombstone_without_resetting_the_sequence() {
    let mut hub = make_hub();
    let sid = SessionId::new();
    revive(&mut hub, sid);

    assert_eq!(hub.publish(sid, Bytes::from_static(b"a")), Some(0));
    assert_eq!(hub.publish(sid, Bytes::from_static(b"b")), Some(1));
    hub.end_session(sid, &TerminalCloseReason::SessionEnded);
    assert!(hub.publish(sid, Bytes::from_static(b"late")).is_none());

    assert!(hub.install_semantic_checkpoint(sid, checkpoint(), 2));

    assert!(
        hub.close_reason(sid).is_none(),
        "revive must clear the tombstone"
    );
    assert_eq!(
        hub.next_sequence(sid),
        Some(2),
        "the sequence counter must continue, not restart, across a reopen"
    );
    let mut reopened = hub
        .register(sid, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("a revived session must accept subscribers again");
    assert_eq!(
        hub.publish(sid, Bytes::from_static(b"after-reopen")),
        Some(2),
        "the first post-reopen frame must not reuse a sequence already handed out"
    );
    hub.end_session(sid, &TerminalCloseReason::SessionEnded);
    std::assert_matches!(
        reopened.control_rx.try_recv().unwrap(),
        TerminalControlFrame::Closed {
            final_sequence: 3,
            ..
        }
    );
}

#[test]
fn a_tombstone_that_was_not_revived_is_still_refused() {
    let mut hub = make_hub();
    let revived = SessionId::new();
    let untouched = SessionId::new();
    revive(&mut hub, revived);
    revive(&mut hub, untouched);

    hub.publish(revived, Bytes::from_static(b"a"));
    hub.publish(untouched, Bytes::from_static(b"a"));
    hub.end_session(revived, &TerminalCloseReason::SessionEnded);
    hub.end_session(untouched, &TerminalCloseReason::SessionEnded);

    assert!(hub.install_semantic_checkpoint(revived, checkpoint(), 1));

    assert!(
        hub.publish(untouched, Bytes::from_static(b"late"))
            .is_none()
    );
    assert!(
        hub.register(
            untouched,
            TerminalSurface::Desktop,
            TerminalCapability::Write
        )
        .is_none()
    );
}

#[test]
fn same_sequence_checkpoint_keeps_live_subscribers() {
    let mut hub = make_hub();
    let sid = SessionId::new();
    revive(&mut hub, sid);

    hub.publish(sid, Bytes::from_static(b"a"));
    let mut handle = hub
        .register(sid, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("session should be live");

    assert!(hub.install_semantic_checkpoint(sid, checkpoint(), 1));

    assert_eq!(hub.next_sequence(sid), Some(1));
    assert_eq!(hub.connection_count(sid), 1);
    assert_eq!(hub.publish(sid, Bytes::from_static(b"b")), Some(1));
    assert!(
        handle.data_rx.try_recv().is_ok(),
        "an existing subscriber must survive a no-op revive"
    );
}

#[test]
fn end_session_sends_exclusive_boundary_after_queued_data() {
    let mut hub = make_hub();
    let sid = SessionId::new();
    revive(&mut hub, sid);

    let mut h1 = hub
        .register(sid, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("session should be live");
    let mut h2 = hub
        .register(sid, TerminalSurface::Cli, TerminalCapability::Write)
        .expect("session should be live");
    assert_eq!(hub.publish(sid, Bytes::from_static(b"a")), Some(0));
    assert_eq!(hub.publish(sid, Bytes::from_static(b"b")), Some(1));

    hub.end_session(sid, &TerminalCloseReason::SessionEnded);

    for handle in [&mut h1, &mut h2] {
        let ctrl = handle.control_rx.try_recv().unwrap();
        std::assert_matches!(
            ctrl,
            TerminalControlFrame::Closed {
                reason: TerminalCloseReason::SessionEnded,
                final_sequence: 2,
            }
        );
        assert_eq!(handle.data_rx.try_recv().unwrap().sequence, 0);
        assert_eq!(handle.data_rx.try_recv().unwrap().sequence, 1);
    }

    assert_eq!(hub.publish(sid, Bytes::from_static(b"late")), None);
    assert!(!hub.send_control(
        sid,
        h1.connection_id,
        TerminalControlFrame::Resize {
            rows: 24,
            cols: 80,
            at_sequence: 2
        },
    ));
    assert_eq!(hub.connection_count(sid), 0);
    assert!(hub.next_sequence(sid).is_none());
}

#[test]
fn end_session_without_output_uses_zero_boundary() {
    let mut hub = make_hub();
    let sid = SessionId::new();
    revive(&mut hub, sid);
    let mut handle = hub
        .register(sid, TerminalSurface::Desktop, TerminalCapability::ReadOnly)
        .expect("session should be live");

    hub.end_session(sid, &TerminalCloseReason::Detached);

    std::assert_matches!(
        handle.control_rx.try_recv().unwrap(),
        TerminalControlFrame::Closed {
            reason: TerminalCloseReason::Detached,
            final_sequence: 0,
        }
    );
}

#[test]
fn sequence_exhaustion_closes_instead_of_wrapping() {
    let mut hub = make_hub();
    let sid = SessionId::new();
    revive(&mut hub, sid);
    let mut handle = hub
        .register(sid, TerminalSurface::Desktop, TerminalCapability::ReadOnly)
        .expect("session should be live");
    hub.lock()
        .get_mut(&sid)
        .expect("session state")
        .next_sequence = u64::MAX;

    assert_eq!(hub.publish(sid, Bytes::from_static(b"overflow")), None);
    std::assert_matches!(
        handle.control_rx.try_recv().unwrap(),
        TerminalControlFrame::Closed {
            reason: TerminalCloseReason::IoError(ref message),
            final_sequence: u64::MAX,
        } if message == "terminal sequence exhausted"
    );
    assert!(hub.next_sequence(sid).is_none());
}

#[test]
fn close_is_delivered_after_the_ordinary_control_queue_saturates() {
    let mut hub = make_hub();
    let sid = SessionId::new();
    revive(&mut hub, sid);
    let mut handle = hub
        .register(sid, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("session should be live");

    for at_sequence in 0..CONTROL_CHANNEL_CAPACITY as u64 {
        assert!(hub.send_control(
            sid,
            handle.connection_id,
            TerminalControlFrame::Resize {
                rows: 24,
                cols: 80,
                at_sequence
            },
        ));
    }
    assert_eq!(hub.connection_count(sid), 1);
    hub.end_session(sid, &TerminalCloseReason::SessionEnded);
    for _ in 0..CONTROL_CHANNEL_CAPACITY {
        std::assert_matches!(
            handle.control_rx.try_recv().unwrap(),
            TerminalControlFrame::Resize { .. }
        );
    }
    std::assert_matches!(
        handle.control_rx.try_recv().unwrap(),
        TerminalControlFrame::Closed {
            reason: TerminalCloseReason::SessionEnded,
            final_sequence: 0,
        }
    );
}

#[test]
fn broadcast_control_reaches_all_connections() {
    let mut hub = make_hub();
    let sid = SessionId::new();
    revive(&mut hub, sid);

    let mut h1 = hub
        .register(sid, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("session should be live");
    let mut h2 = hub
        .register(sid, TerminalSurface::Cli, TerminalCapability::ReadOnly)
        .expect("session should be live");

    hub.broadcast_control(
        sid,
        &TerminalControlFrame::Resize {
            rows: 40,
            cols: 120,
            at_sequence: 0,
        },
    );

    for handle in [&mut h1, &mut h2] {
        std::assert_matches!(
            handle.control_rx.try_recv().unwrap(),
            TerminalControlFrame::Resize {
                rows: 40,
                cols: 120,
                at_sequence: 0
            }
        );
    }
}

#[test]
fn next_sequence_reflects_publish_count() {
    let mut hub = make_hub();
    let sid = SessionId::new();
    revive(&mut hub, sid);

    hub.register(sid, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("session should be live");

    assert_eq!(hub.next_sequence(sid), Some(0));
    hub.publish(sid, Bytes::from_static(b"x"));
    assert_eq!(hub.next_sequence(sid), Some(1));
    hub.publish(sid, Bytes::from_static(b"y"));
    assert_eq!(hub.next_sequence(sid), Some(2));
}

#[test]
fn publish_without_subscribers_still_advances_session_sequence() {
    let mut hub = make_hub();
    let sid = SessionId::new();
    revive(&mut hub, sid);

    assert_eq!(hub.publish(sid, Bytes::from_static(b"pre-attach")), Some(0));
    assert_eq!(hub.next_sequence(sid), Some(1));

    let mut handle = hub
        .register(sid, TerminalSurface::Desktop, TerminalCapability::Write)
        .expect("session should be live");
    assert_eq!(
        hub.publish(sid, Bytes::from_static(b"post-attach")),
        Some(1)
    );

    let frame = handle
        .data_rx
        .try_recv()
        .expect("subscriber should receive frame");
    assert_eq!(frame.sequence, 1);
    assert_eq!(frame.bytes, Bytes::from_static(b"post-attach"));
}
