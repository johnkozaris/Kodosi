use super::*;
use crate::network::{RemoteSession, TestRemoteRequest, test_remote_connection};

fn session() -> RemoteSession {
    RemoteSession {
        id: Uuid::now_v7(),
        incarnation_id: Uuid::now_v7(),
        name: "remote".to_owned(),
        owner_user_id: "owner".to_owned(),
        owner_name: "Owner".to_owned(),
        host_device_id: "host".to_owned(),
        host_name: "Host".to_owned(),
        room_id: None,
        room_name: None,
        shared_with: vec![],
        online: true,
    }
}

fn checkpoint(rows: u16, cols: u16, bytes: &[u8]) -> Checkpoint {
    let mut terminal = ghostty_vt::Terminal::new(cols, rows, ghostty_vt::TerminalPolicy::default())
        .expect("real terminal");
    terminal.write(bytes).expect("terminal output");
    let semantic = terminal
        .semantic_checkpoint(ghostty_vt::CheckpointLimits::default())
        .expect("checkpoint");
    let state = terminal.state().expect("state");
    Checkpoint::new(
        TerminalSize::new(rows, cols).expect("size"),
        super::super::TerminalScreen::Primary,
        semantic.into_bytes(),
        state.cursor_x,
        state.cursor_y,
        !state.cursor_visible,
    )
    .expect("metadata")
}

async fn next_request(requests: &mut mpsc::Receiver<TestRemoteRequest>) -> TestRemoteRequest {
    tokio::time::timeout(Duration::from_secs(2), requests.recv())
        .await
        .expect("request timeout")
        .expect("request")
}

async fn start() -> (
    RemoteTerminal,
    Subscription,
    mpsc::Receiver<TestRemoteRequest>,
    mpsc::Sender<RemoteUpdate>,
    mpsc::Receiver<SessionChange>,
) {
    let (connection, mut requests, updates) = test_remote_connection(session());
    let (changes, receiver) = mpsc::channel(16);
    let terminal = RemoteTerminal::spawn(connection, changes);
    let subscription = terminal.subscribe();
    assert!(matches!(
        next_request(&mut requests).await,
        TestRemoteRequest::Checkpoint
    ));
    updates
        .send(RemoteUpdate::Checkpoint {
            checkpoint: checkpoint(4, 20, b"initial"),
            next_sequence: 0,
            fresh: true,
        })
        .await
        .expect("initial cut");
    (
        terminal,
        subscription.await.expect("subscription"),
        requests,
        updates,
        receiver,
    )
}

#[tokio::test]
async fn ordinary_checkpoint_cannot_seed_an_unproven_connection() {
    let (connection, mut requests, updates) = test_remote_connection(session());
    let (changes, _receiver) = mpsc::channel(16);
    let terminal = RemoteTerminal::spawn(connection, changes);
    let pending = terminal.subscribe();
    assert!(matches!(
        next_request(&mut requests).await,
        TestRemoteRequest::Checkpoint
    ));
    updates
        .send(RemoteUpdate::Checkpoint {
            checkpoint: checkpoint(4, 20, b"old"),
            next_sequence: 0,
            fresh: false,
        })
        .await
        .expect("unproven cut");
    tokio::pin!(pending);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut pending)
            .await
            .is_err()
    );
    assert!(requests.try_recv().is_err());
    updates
        .send(RemoteUpdate::Checkpoint {
            checkpoint: checkpoint(4, 20, b"current"),
            next_sequence: 4,
            fresh: true,
        })
        .await
        .expect("proven cut");
    assert_eq!(pending.await.expect("fresh connection").next_sequence, 4);
    terminal.disconnect();
}

#[tokio::test]
async fn raw_after_an_unproven_checkpoint_does_not_connect() {
    let (connection, mut requests, updates) = test_remote_connection(session());
    let (changes, _receiver) = mpsc::channel(16);
    let terminal = RemoteTerminal::spawn(connection, changes);
    let pending = terminal.subscribe();
    assert!(matches!(
        next_request(&mut requests).await,
        TestRemoteRequest::Checkpoint
    ));
    updates
        .send(RemoteUpdate::Checkpoint {
            checkpoint: checkpoint(4, 20, b"old"),
            next_sequence: 0,
            fresh: false,
        })
        .await
        .expect("unproven cut");
    updates
        .send(RemoteUpdate::Raw {
            sequence: 0,
            bytes: Bytes::from_static(b"old continuation"),
        })
        .await
        .expect("unproven continuation");
    assert!(pending.await.is_err());
    assert!(terminal.is_closed());
    drop(terminal);
}

#[tokio::test]
async fn live_resize_does_not_satisfy_pending_capture_or_late_subscription() {
    let (terminal, mut original, mut requests, updates, _changes) = start().await;
    let late = terminal.subscribe();
    assert!(matches!(
        next_request(&mut requests).await,
        TestRemoteRequest::Checkpoint
    ));
    let capture = terminal.checkpoint(original.connection_id);
    updates
        .send(RemoteUpdate::Checkpoint {
            checkpoint: checkpoint(6, 30, b"live resize"),
            next_sequence: 0,
            fresh: false,
        })
        .await
        .expect("live checkpoint");
    assert!(matches!(
        original.control.recv().await,
        Some(ControlFrame::Resize {
            rows: 6,
            cols: 30,
            at_sequence: 0
        })
    ));
    tokio::pin!(late, capture);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut late)
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut capture)
            .await
            .is_err()
    );
    assert!(requests.try_recv().is_err());
    updates
        .send(RemoteUpdate::Checkpoint {
            checkpoint: checkpoint(6, 30, b"fresh capture"),
            next_sequence: 0,
            fresh: true,
        })
        .await
        .expect("fresh checkpoint");
    assert_eq!(
        late.await.expect("late fresh subscriber").checkpoint.rows(),
        6
    );
    assert_eq!(capture.await.expect("fresh capture").checkpoint.rows(), 6);
    assert!(original.control.try_recv().is_err());
    terminal.disconnect();
}

#[tokio::test]
async fn output_keeps_flowing_while_the_host_has_not_acknowledged_input() {
    let (terminal, mut subscriber, mut requests, updates, _changes) = start().await;
    let admitted = terminal.admit_input(subscriber.connection_id, Bytes::from_static(b"first"));
    tokio::pin!(admitted);
    let first = match next_request(&mut requests).await {
        TestRemoteRequest::Control {
            control: TerminalControl::Input { bytes },
            reply,
        } => {
            assert_eq!(bytes, b"first");
            reply
        }
        _ => panic!("input expected"),
    };
    terminal
        .input(subscriber.connection_id, Bytes::from_static(b"second"))
        .expect("second input");
    for sequence in 0..20 {
        updates
            .send(RemoteUpdate::Raw {
                sequence,
                bytes: Bytes::from_static(b"live"),
            })
            .await
            .expect("raw");
        let frame = tokio::time::timeout(Duration::from_secs(1), subscriber.data.recv())
            .await
            .expect("output not blocked")
            .expect("frame");
        assert_eq!(frame.sequence, sequence);
    }
    assert!(requests.try_recv().is_err());
    assert!(
        tokio::time::timeout(Duration::from_millis(10), &mut admitted)
            .await
            .is_err()
    );
    first.send(Ok(Value::Null)).expect("first result");
    admitted.await.expect("input admission acknowledged");
    match next_request(&mut requests).await {
        TestRemoteRequest::Control {
            control: TerminalControl::Input { bytes },
            reply,
        } => {
            assert_eq!(bytes, b"second");
            reply.send(Ok(Value::Null)).expect("second result");
        }
        _ => panic!("second input expected"),
    }
    terminal.disconnect();
}

#[tokio::test]
async fn a_late_view_receives_fresh_checkpoint_and_contiguous_suffix_without_resetting_the_old_view()
 {
    let (terminal, mut original, mut requests, updates, _changes) = start().await;
    let late = terminal.subscribe();
    assert!(matches!(
        next_request(&mut requests).await,
        TestRemoteRequest::Checkpoint
    ));
    for sequence in 0..3 {
        updates
            .send(RemoteUpdate::Raw {
                sequence,
                bytes: Bytes::from_static(b"x"),
            })
            .await
            .expect("raw");
        assert_eq!(
            original
                .data
                .recv()
                .await
                .expect("original continues")
                .sequence,
            sequence
        );
    }
    updates
        .send(RemoteUpdate::Checkpoint {
            checkpoint: checkpoint(4, 20, b"x"),
            next_sequence: 1,
            fresh: true,
        })
        .await
        .expect("fresh but lagging capture");
    let mut late = late.await.expect("late subscriber");
    assert_eq!(late.next_sequence, 1);
    assert_eq!(late.data.recv().await.expect("replay1").sequence, 1);
    assert_eq!(late.data.recv().await.expect("replay2").sequence, 2);
    assert!(original.control.try_recv().is_err());
    updates
        .send(RemoteUpdate::Raw {
            sequence: 3,
            bytes: Bytes::from_static(b"y"),
        })
        .await
        .expect("raw");
    assert_eq!(
        original.data.recv().await.expect("original live").sequence,
        3
    );
    assert_eq!(late.data.recv().await.expect("late live").sequence, 3);
    terminal.disconnect();
}

#[tokio::test]
async fn a_future_capture_waits_for_the_original_stream_instead_of_skipping_output() {
    let (terminal, mut original, mut requests, updates, _changes) = start().await;
    let late = terminal.subscribe();
    assert!(matches!(
        next_request(&mut requests).await,
        TestRemoteRequest::Checkpoint
    ));
    updates
        .send(RemoteUpdate::Checkpoint {
            checkpoint: checkpoint(4, 20, b"xx"),
            next_sequence: 2,
            fresh: true,
        })
        .await
        .expect("ahead capture");
    tokio::pin!(late);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut late)
            .await
            .is_err()
    );
    for sequence in 0..2 {
        updates
            .send(RemoteUpdate::Raw {
                sequence,
                bytes: Bytes::from_static(b"x"),
            })
            .await
            .expect("intermediate output");
        assert_eq!(
            original
                .data
                .recv()
                .await
                .expect("original receives every byte")
                .sequence,
            sequence
        );
    }
    let late = late.await.expect("late subscriber");
    assert_eq!(late.next_sequence, 2);
    assert!(original.control.try_recv().is_err());
    terminal.disconnect();
}

#[tokio::test]
async fn capture_refresh_preserves_existing_subscription_and_focus() {
    let (terminal, subscriber, mut requests, updates, _changes) = start().await;
    let focus = terminal.control(
        Some(subscriber.connection_id),
        TerminalControl::Focus { focused: true },
    );
    match next_request(&mut requests).await {
        TestRemoteRequest::Control {
            control: TerminalControl::Focus { focused: true },
            reply,
        } => {
            reply.send(Ok(Value::Null)).expect("focus");
        }
        _ => panic!("focus expected"),
    }
    focus.await.expect("focus applied");
    let capture = terminal.checkpoint(subscriber.connection_id);
    assert!(matches!(
        next_request(&mut requests).await,
        TestRemoteRequest::Checkpoint
    ));
    updates
        .send(RemoteUpdate::Checkpoint {
            checkpoint: checkpoint(4, 20, b"initial"),
            next_sequence: 0,
            fresh: true,
        })
        .await
        .expect("capture");
    assert_eq!(capture.await.expect("capture").next_sequence, 0);
    assert!(!terminal.is_closed());
    assert!(requests.try_recv().is_err());
    drop(subscriber);
    match next_request(&mut requests).await {
        TestRemoteRequest::Control {
            control: TerminalControl::Focus { focused: false },
            reply,
        } => {
            reply
                .send(Ok(Value::Null))
                .expect("blur after actual close");
        }
        _ => panic!("blur expected"),
    }
    terminal.disconnect();
}

#[tokio::test]
async fn same_cut_resize_reaches_existing_viewers_as_an_ordered_geometry_change() {
    let (terminal, mut subscriber, _requests, updates, _changes) = start().await;
    updates
        .send(RemoteUpdate::Checkpoint {
            checkpoint: checkpoint(6, 30, b"initial"),
            next_sequence: 0,
            fresh: false,
        })
        .await
        .expect("resized checkpoint");
    assert!(matches!(
        subscriber.control.recv().await,
        Some(ControlFrame::Resize {
            rows: 6,
            cols: 30,
            at_sequence: 0
        })
    ));
    updates
        .send(RemoteUpdate::Raw {
            sequence: 0,
            bytes: Bytes::from_static(b"next"),
        })
        .await
        .expect("raw");
    assert_eq!(
        subscriber
            .data
            .recv()
            .await
            .expect("raw after resize")
            .sequence,
        0
    );
    terminal.disconnect();
}

#[tokio::test]
async fn unknown_input_result_closes_once_and_reports_the_exact_remote_instance() {
    let (terminal, subscriber, mut requests, _updates, mut changes) = start().await;
    terminal
        .input(
            subscriber.connection_id,
            Bytes::from_static(b"never-replay"),
        )
        .expect("input");
    match next_request(&mut requests).await {
        TestRemoteRequest::Control { reply, .. } => {
            reply
                .send(Err(crate::network::Error::Closed))
                .expect("unknown result");
        }
        TestRemoteRequest::Checkpoint => panic!("input expected"),
    }
    let change = tokio::time::timeout(Duration::from_secs(1), changes.recv())
        .await
        .expect("closed event timeout")
        .expect("closed event");
    assert!(
        matches!(change, SessionChange::RemoteClosed { incarnation, instance_id, .. } if incarnation == terminal.incarnation_id && instance_id == terminal.instance_id)
    );
    assert!(terminal.is_closed());
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn disconnect_does_not_wait_behind_an_in_flight_control_or_full_queue() {
    let (terminal, subscriber, mut requests, _updates, mut changes) = start().await;
    terminal
        .input(subscriber.connection_id, Bytes::from_static(b"pending"))
        .expect("input");
    let _unconfirmed = next_request(&mut requests).await;
    for _ in 0..256 {
        drop(terminal.input(subscriber.connection_id, Bytes::from_static(b"queued")));
    }
    terminal.disconnect();
    assert!(terminal.is_closed());
    assert!(
        tokio::time::timeout(Duration::from_secs(1), changes.recv())
            .await
            .expect("timely closure")
            .is_some()
    );
}
