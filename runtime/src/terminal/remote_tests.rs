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
        mission_id: None,
        mission_name: None,
        shared_with: vec![],
        online: true,
        status: None,
    }
}

const CELL: CellPixels = CellPixels {
    width: 8,
    height: 16,
};

fn checkpoint(rows: u16, cols: u16, bytes: &[u8]) -> Checkpoint {
    let mut terminal = ghostty_vt::Terminal::new(cols, rows, ghostty_vt::TerminalPolicy::default())
        .expect("real terminal");
    terminal
        .resize(cols, rows, CELL.width, CELL.height)
        .expect("host cells");
    terminal.write(bytes).expect("terminal output");
    let snapshot = terminal
        .snapshot(crate::terminal::snapshot_limits(u64::MAX))
        .expect("snapshot");
    Checkpoint::new(snapshot, false).expect("checkpoint")
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
async fn raw_before_a_checkpoint_does_not_connect() {
    let (connection, mut requests, updates) = test_remote_connection(session());
    let (changes, _receiver) = mpsc::channel(16);
    let terminal = RemoteTerminal::spawn(connection, changes);
    let pending = terminal.subscribe();
    assert!(matches!(
        next_request(&mut requests).await,
        TestRemoteRequest::Checkpoint
    ));
    updates
        .send(RemoteUpdate::Raw {
            sequence: 0,
            bytes: Bytes::from_static(b"old continuation"),
        })
        .await
        .expect("continuation");
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
        .send(RemoteUpdate::Resize {
            rows: 6,
            cols: 30,
            cell: CELL,
            at_sequence: 0,
        })
        .await
        .expect("live resize");
    assert!(matches!(
        original.control.recv().await,
        Some(ControlFrame::Resize {
            rows: 6,
            cols: 30,
            cell: CELL,
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
async fn resync_gives_an_open_view_a_new_snapshot_without_closing_it() {
    let (terminal, mut view, _requests, updates, _changes) = start().await;
    updates
        .send(RemoteUpdate::Raw {
            sequence: 0,
            bytes: Bytes::from_static(b"before"),
        })
        .await
        .expect("output");
    updates.send(RemoteUpdate::Resync).await.expect("resync");
    updates
        .send(RemoteUpdate::Checkpoint {
            checkpoint: checkpoint(4, 20, b"after the gap"),
            next_sequence: 40,
        })
        .await
        .expect("new cut");
    updates
        .send(RemoteUpdate::Raw {
            sequence: 40,
            bytes: Bytes::from_static(b"after"),
        })
        .await
        .expect("continuation");
    assert!(matches!(
        view.control.recv().await,
        Some(ControlFrame::Snapshot {
            next_sequence: 40,
            ..
        })
    ));
    assert_eq!(view.data.recv().await.expect("old output").sequence, 0);
    assert_eq!(view.data.recv().await.expect("new output").sequence, 40);
    assert!(!terminal.is_closed());
    terminal.disconnect();
}

async fn interrupted(changes: &mut mpsc::Receiver<SessionChange>, terminal: &RemoteTerminal) {
    let change = tokio::time::timeout(Duration::from_secs(1), changes.recv())
        .await
        .expect("interruption timeout")
        .expect("interruption");
    assert!(
        matches!(change, SessionChange::RemoteInterrupted { incarnation, instance_id, .. } if incarnation == terminal.incarnation_id && instance_id == terminal.instance_id)
    );
    assert!(terminal.is_interrupted() && !terminal.is_closed());
}

#[tokio::test]
async fn a_lost_connection_keeps_the_view_and_a_later_close_keeps_its_output_position() {
    let (terminal, mut view, _requests, updates, mut changes) = start().await;
    updates
        .send(RemoteUpdate::Raw {
            sequence: 0,
            bytes: Bytes::from_static(b"before"),
        })
        .await
        .expect("output");
    updates
        .send(RemoteUpdate::Closed {
            reason: "link lost".to_owned(),
        })
        .await
        .expect("close");
    interrupted(&mut changes, &terminal).await;
    assert!(view.control.try_recv().is_err());
    assert!(
        terminal
            .write_input(view.connection_id, Bytes::from_static(b"typed in the gap"))
            .await
            .is_err()
    );
    terminal.disconnect();
    assert!(matches!(
        view.control.recv().await,
        Some(ControlFrame::Closed {
            final_sequence: 1,
            ..
        })
    ));
}

#[tokio::test]
async fn a_new_connection_gives_an_open_view_a_snapshot_in_place() {
    let (terminal, mut view, _requests, updates, mut changes) = start().await;
    updates
        .send(RemoteUpdate::Raw {
            sequence: 0,
            bytes: Bytes::from_static(b"before"),
        })
        .await
        .expect("output");
    drop(updates);
    interrupted(&mut changes, &terminal).await;
    let (connection, mut requests, updates) = test_remote_connection(RemoteSession {
        incarnation_id: terminal.incarnation_id,
        ..session()
    });
    assert!(terminal.resume(connection));
    updates
        .send(RemoteUpdate::Checkpoint {
            checkpoint: checkpoint(4, 20, b"after the gap"),
            next_sequence: 70,
        })
        .await
        .expect("new cut");
    updates
        .send(RemoteUpdate::Raw {
            sequence: 70,
            bytes: Bytes::from_static(b"after"),
        })
        .await
        .expect("continuation");
    assert!(matches!(
        view.control.recv().await,
        Some(ControlFrame::Snapshot {
            next_sequence: 70,
            ..
        })
    ));
    assert_eq!(view.data.recv().await.expect("old output").sequence, 0);
    assert_eq!(view.data.recv().await.expect("new output").sequence, 70);
    assert!(!terminal.is_interrupted());
    terminal
        .input(view.connection_id, Bytes::from_static(b"typed after"))
        .expect("input");
    assert!(matches!(
        next_request(&mut requests).await,
        TestRemoteRequest::Control { .. }
    ));
    terminal.disconnect();
}

#[tokio::test]
async fn a_control_before_the_first_snapshot_waits_for_it() {
    let (connection, mut requests, updates) = test_remote_connection(session());
    let (changes, _receiver) = mpsc::channel(16);
    let terminal = RemoteTerminal::spawn(connection, changes);
    let view = terminal.subscribe();
    assert!(matches!(
        next_request(&mut requests).await,
        TestRemoteRequest::Checkpoint
    ));
    let close = tokio::spawn(terminal.control(None, TerminalControl::Close));
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(requests.try_recv().is_err());
    updates
        .send(RemoteUpdate::Checkpoint {
            checkpoint: checkpoint(4, 20, b"current"),
            next_sequence: 0,
        })
        .await
        .expect("first cut");
    match next_request(&mut requests).await {
        TestRemoteRequest::Control {
            control: TerminalControl::Close,
            reply,
        } => reply.send(Ok(Value::Null)).expect("close result"),
        _ => panic!("close expected"),
    }
    close.await.expect("close task").expect("close confirmed");
    view.await.expect("view");
    terminal.disconnect();
}

#[tokio::test]
async fn command_input_goes_out_without_a_view_but_waits_while_a_person_types() {
    let (terminal, view, mut requests, _updates, _changes) = start().await;
    let sent = terminal.send_input(Bytes::from_static(b"ls\r"));
    match next_request(&mut requests).await {
        TestRemoteRequest::Control {
            control: TerminalControl::Input { bytes },
            reply,
        } => {
            assert_eq!(bytes, b"ls\r");
            reply.send(Ok(Value::Null)).expect("input result");
        }
        _ => panic!("command input expected"),
    }
    sent.await.expect("command input");
    terminal
        .input(view.connection_id, Bytes::from_static(b"x"))
        .expect("a person types");
    match next_request(&mut requests).await {
        TestRemoteRequest::Control { reply, .. } => {
            reply.send(Ok(Value::Null)).expect("input result");
        }
        TestRemoteRequest::Checkpoint => panic!("view input expected"),
    }
    let refused = terminal.send_input(Bytes::from_static(b"y")).await;
    assert_eq!(
        refused.expect_err("typing").to_string(),
        crate::terminal::SOMEONE_TYPING
    );
    terminal.disconnect();
}

#[tokio::test]
async fn focus_brings_the_size_the_view_asked_for() {
    let (terminal, view, mut requests, _updates, _changes) = start().await;
    let resize = TerminalControl::Resize {
        rows: 10,
        cols: 40,
        width_pixels: 0,
        height_pixels: 0,
        cell_width_pixels: 0,
        cell_height_pixels: 0,
        claim: false,
    };
    let refused = terminal.control(Some(view.connection_id), resize);
    match next_request(&mut requests).await {
        TestRemoteRequest::Control {
            control: TerminalControl::Resize { claim: false, .. },
            reply,
        } => reply
            .send(Err(crate::network::Error::Invalid {
                reason: "Another device controls the size.".to_owned(),
            }))
            .expect("refusal"),
        _ => panic!("resize expected"),
    }
    assert!(refused.await.is_err());
    let focused = terminal.control(
        Some(view.connection_id),
        TerminalControl::Focus { focused: true },
    );
    match next_request(&mut requests).await {
        TestRemoteRequest::Control {
            control: TerminalControl::Focus { focused: true },
            reply,
        } => reply.send(Ok(Value::Null)).expect("focus result"),
        _ => panic!("focus expected"),
    }
    focused.await.expect("focus");
    match next_request(&mut requests).await {
        TestRemoteRequest::Control {
            control:
                TerminalControl::Resize {
                    rows: 10,
                    cols: 40,
                    claim: true,
                    ..
                },
            reply,
        } => reply.send(Ok(Value::Null)).expect("resize result"),
        _ => panic!("the asked size expected"),
    }
    terminal.disconnect();
}

#[tokio::test]
async fn input_that_waits_for_the_host_goes_out_as_one_ordered_control() {
    let (terminal, subscriber, mut requests, _updates, _changes) = start().await;
    terminal
        .input(subscriber.connection_id, Bytes::from_static(b"a"))
        .expect("first input");
    let first = match next_request(&mut requests).await {
        TestRemoteRequest::Control {
            control: TerminalControl::Input { bytes },
            reply,
        } => {
            assert_eq!(bytes, b"a");
            reply
        }
        _ => panic!("input expected"),
    };
    let second = terminal.write_input(subscriber.connection_id, Bytes::from_static(b"b"));
    let third = terminal.write_input(subscriber.connection_id, Bytes::from_static(b"c"));
    tokio::pin!(second, third);
    assert!(
        tokio::time::timeout(Duration::from_millis(10), &mut second)
            .await
            .is_err()
    );
    assert!(requests.try_recv().is_err());
    first.send(Ok(Value::Null)).expect("first result");
    match next_request(&mut requests).await {
        TestRemoteRequest::Control {
            control: TerminalControl::Input { bytes },
            reply,
        } => {
            assert_eq!(bytes, b"bc");
            reply.send(Ok(Value::Null)).expect("merged result");
        }
        _ => panic!("merged input expected"),
    }
    second.await.expect("second input confirmed");
    third.await.expect("third input confirmed");
    assert!(requests.try_recv().is_err());
    terminal.disconnect();
}

#[tokio::test]
async fn output_keeps_flowing_while_the_host_has_not_acknowledged_input() {
    let (terminal, mut subscriber, mut requests, updates, _changes) = start().await;
    let admitted = terminal.write_input(subscriber.connection_id, Bytes::from_static(b"first"));
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
        .send(RemoteUpdate::Resize {
            rows: 6,
            cols: 30,
            cell: CELL,
            at_sequence: 0,
        })
        .await
        .expect("resize notice");
    assert!(matches!(
        subscriber.control.recv().await,
        Some(ControlFrame::Resize {
            rows: 6,
            cols: 30,
            cell: CELL,
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
async fn unknown_input_result_is_not_retried_and_makes_the_terminal_connect_again() {
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
    interrupted(&mut changes, &terminal).await;
    assert!(requests.try_recv().is_err());
    terminal.disconnect();
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

#[tokio::test]
async fn the_end_from_the_host_ends_a_view_that_did_not_get_all_output() {
    let (terminal, mut view, _requests, updates, mut changes) = start().await;
    updates.send(RemoteUpdate::Ended).await.expect("end");
    assert!(matches!(
        view.control.recv().await,
        Some(ControlFrame::Closed { .. })
    ));
    loop {
        match changes.recv().await {
            Some(SessionChange::RemoteEnded { .. }) => break,
            Some(_) => {}
            None => panic!("the terminal did not report its end"),
        }
    }
    assert!(terminal.is_closed());
}
