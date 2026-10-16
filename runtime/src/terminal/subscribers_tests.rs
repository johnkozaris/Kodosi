use super::super::{TerminalScreen, TerminalSize};
use super::*;

fn checkpoint() -> Checkpoint {
    Checkpoint::new(
        TerminalSize::new(4, 10).expect("valid size"),
        TerminalScreen::Primary,
        b"not-restored-in-subscriber-tests".to_vec(),
        0,
        0,
        false,
    )
    .expect("metadata checkpoint")
}

#[tokio::test]
async fn dropping_subscription_immediately_retires_its_authorization() {
    let mut subscribers = Subscribers::default();
    let subscription = subscribers
        .subscribe(Uuid::now_v7(), checkpoint(), 0)
        .expect("subscriber");
    let id = subscription.connection_id;
    let authorization = subscribers.authorization(id).expect("live authorization");
    assert!(!authorization.is_cancelled());
    drop(subscription);
    assert!(authorization.is_cancelled());
    assert!(!subscribers.contains(id));
    subscribers.prune();
    assert!(subscribers.entries.is_empty());
}

#[tokio::test]
async fn a_view_that_falls_behind_stays_open_and_gets_a_snapshot_after_it_reads_its_queue() {
    let mut subscribers = Subscribers::default();
    let mut slow = subscribers
        .subscribe(Uuid::now_v7(), checkpoint(), 0)
        .expect("slow");
    let mut fast = subscribers
        .subscribe(Uuid::now_v7(), checkpoint(), 0)
        .expect("fast");
    let total = DATA_CAPACITY as u64 + 20;
    for sequence in 0..total {
        subscribers.data(&DataFrame {
            sequence,
            bytes: Bytes::from_static(b"x"),
        });
        assert_eq!(
            fast.data.recv().await.expect("live data").sequence,
            sequence
        );
    }
    subscribers.control(&ControlFrame::Resize {
        rows: 6,
        cols: 30,
        at_sequence: total,
    });
    assert!(subscribers.contains(slow.connection_id));
    assert!(!subscribers.caught_up());
    subscribers.refresh(&checkpoint(), total);
    assert!(slow.control.try_recv().is_err());
    for expected in 0..DATA_CAPACITY as u64 {
        assert_eq!(
            slow.data.recv().await.expect("queued prefix").sequence,
            expected
        );
    }
    assert!(subscribers.caught_up());
    subscribers.refresh(&checkpoint(), total);
    assert!(!subscribers.caught_up());
    assert!(matches!(
        slow.control.recv().await,
        Some(ControlFrame::Snapshot { next_sequence, .. }) if next_sequence == total
    ));
    subscribers.data(&DataFrame {
        sequence: total,
        bytes: Bytes::from_static(b"y"),
    });
    assert_eq!(slow.data.recv().await.expect("resumed").sequence, total);
    assert!(matches!(
        fast.control.recv().await,
        Some(ControlFrame::Resize { .. })
    ));
    assert!(fast.control.try_recv().is_err());
}

#[tokio::test]
async fn replay_preflights_capacity_without_partially_enqueuing() {
    let mut subscribers = Subscribers::default();
    let mut subscriber = subscribers
        .subscribe(Uuid::now_v7(), checkpoint(), 0)
        .expect("subscriber");
    let frames = (0..=DATA_CAPACITY as u64)
        .map(|sequence| DataFrame {
            sequence,
            bytes: Bytes::from_static(b"x"),
        })
        .collect::<Vec<_>>();
    assert!(!subscribers.replay(subscriber.connection_id, &frames));
    assert!(subscriber.data.try_recv().is_err());
    assert!(subscribers.replay(subscriber.connection_id, &frames[..DATA_CAPACITY]));
    for sequence in 0..DATA_CAPACITY as u64 {
        assert_eq!(
            subscriber
                .data
                .recv()
                .await
                .expect("replayed frame")
                .sequence,
            sequence
        );
    }
}

#[tokio::test]
async fn a_view_with_a_full_control_queue_stays_open_and_gets_a_snapshot_when_it_has_room() {
    let mut subscribers = Subscribers::default();
    let mut subscriber = subscribers
        .subscribe(Uuid::now_v7(), checkpoint(), 0)
        .expect("subscriber");
    let authorization = subscribers
        .authorization(subscriber.connection_id)
        .expect("authorization");
    for at_sequence in 0..=CONTROL_CAPACITY as u64 {
        subscribers.control(&ControlFrame::Resize {
            rows: 4,
            cols: 10,
            at_sequence,
        });
    }
    assert!(!authorization.is_cancelled());
    assert!(subscribers.contains(subscriber.connection_id));

    subscribers.refresh(&checkpoint(), 20);
    assert!(subscribers.caught_up());
    assert!(matches!(
        subscriber.control.recv().await,
        Some(ControlFrame::Resize { at_sequence: 0, .. })
    ));
    subscribers.refresh(&checkpoint(), 20);
    assert!(!subscribers.caught_up());
    let mut last = None;
    while let Ok(frame) = subscriber.control.try_recv() {
        last = Some(frame);
    }
    assert!(matches!(
        last,
        Some(ControlFrame::Snapshot {
            next_sequence: 20,
            ..
        })
    ));
}

#[tokio::test]
async fn stopped_session_drains_accepted_output_before_final_cut() {
    let mut subscribers = Subscribers::default();
    let mut subscriber = subscribers
        .subscribe(Uuid::now_v7(), checkpoint(), 12)
        .expect("subscriber");
    subscribers.data(&DataFrame {
        sequence: 12,
        bytes: Bytes::from_static(b"final"),
    });
    subscribers.close("ended", 13);
    assert!(matches!(
        subscriber.control.recv().await,
        Some(ControlFrame::Closed {
            final_sequence: 13,
            ..
        })
    ));
    assert_eq!(
        subscriber.data.recv().await.expect("last output").sequence,
        12
    );
    assert!(subscriber.data.recv().await.is_none());
}

fn output(subscribers: &mut Subscribers, sequence: u64) {
    subscribers.data(&DataFrame {
        sequence,
        bytes: Bytes::from_static(b"x"),
    });
}

fn resize(subscribers: &mut Subscribers, at_sequence: u64) {
    subscribers.control(&ControlFrame::Resize {
        rows: 6,
        cols: 30,
        at_sequence,
    });
}

async fn next(subscription: &mut Subscription) -> String {
    match subscription.recv().await {
        Ok(Some(Frame::Data(frame))) => format!("data {}", frame.sequence),
        Ok(Some(Frame::Control(ControlFrame::Resize { at_sequence, .. }))) => {
            format!("resize {at_sequence}")
        }
        Ok(Some(Frame::Control(ControlFrame::Snapshot { next_sequence, .. }))) => {
            format!("snapshot {next_sequence}")
        }
        Ok(Some(Frame::Control(ControlFrame::Closed {
            reason,
            final_sequence,
        }))) => format!("closed {final_sequence} {reason}"),
        Ok(None) => "ended".to_owned(),
        Err(error) => format!("failed {error}"),
    }
}

#[tokio::test]
async fn a_view_gets_each_resize_and_its_close_at_their_place_in_the_output() {
    let mut subscribers = Subscribers::default();
    let mut view = subscribers
        .subscribe(Uuid::now_v7(), checkpoint(), 0)
        .expect("view");
    resize(&mut subscribers, 1);
    output(&mut subscribers, 0);
    output(&mut subscribers, 1);
    subscribers.close("ended", 2);
    for expected in ["data 0", "resize 1", "data 1", "closed 2 ended"] {
        assert_eq!(next(&mut view).await, expected);
    }
}

#[tokio::test]
async fn a_snapshot_replaces_older_output_and_waiting_resizes() {
    let mut subscribers = Subscribers::default();
    let mut view = subscribers
        .subscribe(Uuid::now_v7(), checkpoint(), 0)
        .expect("view");
    resize(&mut subscribers, 4);
    output(&mut subscribers, 0);
    assert_eq!(next(&mut view).await, "data 0");
    subscribers.control(&ControlFrame::Snapshot {
        checkpoint: checkpoint(),
        next_sequence: 5,
    });
    assert_eq!(next(&mut view).await, "snapshot 5");
    output(&mut subscribers, 3);
    output(&mut subscribers, 5);
    assert_eq!(next(&mut view).await, "data 5");
}

#[tokio::test]
async fn a_refreshed_snapshot_waits_for_the_output_before_it() {
    let mut subscribers = Subscribers::default();
    let mut view = subscribers
        .subscribe(Uuid::now_v7(), checkpoint(), 0)
        .expect("view");
    assert!(view.can_refresh());
    view.refresh(CheckpointCut {
        checkpoint: checkpoint(),
        next_sequence: 2,
    });
    assert!(!view.can_refresh());
    resize(&mut subscribers, 2);
    for sequence in 0..3 {
        output(&mut subscribers, sequence);
    }
    for expected in ["data 0", "data 1", "snapshot 2", "resize 2", "data 2"] {
        assert_eq!(next(&mut view).await, expected);
    }
    view.refresh(CheckpointCut {
        checkpoint: checkpoint(),
        next_sequence: 1,
    });
    assert_eq!(
        next(&mut view).await,
        "failed terminal refresh snapshot regressed"
    );
}

#[tokio::test]
async fn a_view_fails_on_lost_output_and_reports_a_close_before_its_last_output() {
    let mut subscribers = Subscribers::default();
    let mut view = subscribers
        .subscribe(Uuid::now_v7(), checkpoint(), 0)
        .expect("view");
    output(&mut subscribers, 1);
    assert_eq!(
        next(&mut view).await,
        "failed terminal output needs a fresh snapshot"
    );

    let mut view = subscribers
        .subscribe(Uuid::now_v7(), checkpoint(), 0)
        .expect("view");
    output(&mut subscribers, 0);
    subscribers.close("ended", 3);
    assert_eq!(next(&mut view).await, "data 0");
    assert_eq!(next(&mut view).await, "closed 3 ended");

    let mut view = subscribers
        .subscribe(Uuid::now_v7(), checkpoint(), 0)
        .expect("view");
    subscribers.remove(view.connection_id);
    assert_eq!(next(&mut view).await, "ended");
}
