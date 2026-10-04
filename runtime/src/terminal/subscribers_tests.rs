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
