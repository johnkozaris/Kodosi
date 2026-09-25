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
async fn a_full_data_queue_closes_only_the_slow_viewer() {
    let mut subscribers = Subscribers::default();
    let mut slow = subscribers
        .subscribe(Uuid::now_v7(), checkpoint(), 0)
        .expect("slow");
    let mut fast = subscribers
        .subscribe(Uuid::now_v7(), checkpoint(), 0)
        .expect("fast");
    let slow_authorization = subscribers
        .authorization(slow.connection_id)
        .expect("authorization");
    for sequence in 0..=DATA_CAPACITY as u64 {
        subscribers.data(&DataFrame {
            sequence,
            bytes: Bytes::from_static(b"x"),
        });
        assert_eq!(
            fast.data.recv().await.expect("live data").sequence,
            sequence
        );
    }
    assert!(slow_authorization.is_cancelled());
    assert!(!subscribers.contains(slow.connection_id));
    assert!(subscribers.contains(fast.connection_id));
    assert!(
        matches!(slow.control.recv().await, Some(ControlFrame::Closed { final_sequence, .. }) if final_sequence == DATA_CAPACITY as u64)
    );
    for expected in 0..DATA_CAPACITY as u64 {
        assert_eq!(
            slow.data.recv().await.expect("bounded prefix").sequence,
            expected
        );
    }
    assert!(slow.data.recv().await.is_none());
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
async fn full_control_queue_retires_focus_and_input_lease() {
    let mut subscribers = Subscribers::default();
    let subscriber = subscribers
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
    assert!(authorization.is_cancelled());
    assert!(!subscribers.contains(subscriber.connection_id));
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
