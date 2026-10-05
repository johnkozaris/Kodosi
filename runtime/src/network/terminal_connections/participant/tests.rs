use super::{
    super::channel::tests::{Pipe, noise, pair},
    *,
};
use crate::{
    identity::keys::DeviceKeys,
    terminal::{Checkpoint, TerminalScreen, TerminalSize},
};

struct View {
    host: Channel<Pipe>,
    commands: mpsc::Sender<RemoteRequest>,
    updates: mpsc::Receiver<RemoteUpdate>,
    ended: tokio::task::JoinHandle<(Result<()>, String)>,
}

async fn view() -> View {
    let (host, viewer) = (
        DeviceKeys::generate().unwrap(),
        DeviceKeys::generate().unwrap(),
    );
    let (channel, host) = pair(&host, &viewer).await;
    let (commands, requests) = mpsc::channel(128);
    let (updates_tx, updates) = mpsc::channel(128);
    let ended = tokio::spawn(async move {
        let mut viewer = Viewer::new(channel, requests, updates_tx);
        let mut deadlines = tokio::time::interval(Duration::from_millis(250));
        let mut trust = tokio::time::interval(Duration::from_hours(1));
        loop {
            if let Err(error) = viewer.step(&mut deadlines, &mut trust).await {
                let reason = viewer.unconfirmed("Lost.".into());
                return (Err(error), reason);
            }
        }
    });
    View {
        host,
        commands,
        updates,
        ended,
    }
}

impl View {
    async fn keyframe(&mut self, next_sequence: u64, bytes: usize) -> u64 {
        let checkpoint = Checkpoint::new(
            TerminalSize::new(24, 80).unwrap(),
            TerminalScreen::Primary,
            noise(bytes),
            0,
            0,
            false,
        )
        .unwrap();
        let parts = wire::snapshot_parts(&checkpoint).unwrap();
        let last = parts.len() - 1;
        let mut sent = 0;
        for (at, part) in parts.into_iter().enumerate() {
            sent += self
                .host
                .send(&Frame::Keyframe {
                    next_sequence,
                    more: at < last,
                    part,
                })
                .await
                .unwrap() as u64;
        }
        sent
    }

    async fn sent_by_view(&mut self) -> Frame {
        tokio::time::timeout(Duration::from_secs(5), self.host.receive())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .0
    }

    async fn input(&self, bytes: &[u8]) -> Result<Value> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(RemoteRequest::Control {
                control: TerminalControl::Input {
                    bytes: bytes.to_vec(),
                },
                reply,
            })
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), response)
            .await
            .unwrap()
            .unwrap()
    }
}

#[tokio::test]
async fn a_view_answers_each_heartbeat_so_that_the_relay_does_not_close_an_idle_view() {
    let mut view = view().await;
    for number in 1..=2 {
        view.host
            .send(&Frame::Heartbeat {
                number,
                next_sequence: 0,
            })
            .await
            .unwrap();
        assert!(matches!(
            view.sent_by_view().await,
            Frame::Ack { received: 0 }
        ));
    }
}

#[tokio::test]
async fn a_view_installs_each_whole_snapshot_and_acknowledges_what_it_received() {
    let mut view = view().await;
    let sent = view.keyframe(7, 100_000).await;
    let Some(RemoteUpdate::Checkpoint {
        checkpoint,
        next_sequence: 7,
        fresh: true,
    }) = view.updates.recv().await
    else {
        panic!("snapshot");
    };
    assert_eq!(checkpoint.semantic_checkpoint.len(), 100_000);
    let mut acknowledged = 0;
    while let Ok(frame) =
        tokio::time::timeout(Duration::from_millis(300), view.host.receive()).await
    {
        let Some((Frame::Ack { received }, _)) = frame.unwrap() else {
            panic!("acknowledgement");
        };
        assert!(received > acknowledged);
        acknowledged = received;
    }
    assert!(sent > 100_000);
    assert_eq!(acknowledged, sent);

    view.host
        .send(&Frame::Output {
            first_sequence: 7,
            chunks: vec![Bytes::from_static(b"a"), Bytes::from_static(b"b")],
        })
        .await
        .unwrap();
    assert!(matches!(
        view.updates.recv().await,
        Some(RemoteUpdate::Raw { sequence: 7, .. })
    ));
    assert!(matches!(
        view.updates.recv().await,
        Some(RemoteUpdate::Raw { sequence: 8, .. })
    ));
    assert!(matches!(view.sent_by_view().await, Frame::Ack { .. }));

    view.keyframe(40, 100).await;
    assert!(matches!(
        view.updates.recv().await,
        Some(RemoteUpdate::Resync)
    ));
    assert!(matches!(
        view.updates.recv().await,
        Some(RemoteUpdate::Checkpoint {
            next_sequence: 40,
            ..
        })
    ));
}

#[tokio::test]
async fn input_goes_at_once_in_order_and_waits_only_when_too_much_is_not_confirmed() {
    let mut view = view().await;
    assert!(view.input(b"early").await.is_err());
    view.keyframe(0, 100).await;
    view.updates.recv().await.unwrap();
    assert!(matches!(view.sent_by_view().await, Frame::Ack { .. }));
    view.host
        .send(&Frame::Heartbeat {
            number: 4,
            next_sequence: 0,
        })
        .await
        .unwrap();
    assert!(matches!(view.sent_by_view().await, Frame::Ack { .. }));

    view.input(b"ls").await.unwrap();
    view.input(b"\r").await.unwrap();
    assert!(matches!(
        view.sent_by_view().await,
        Frame::Input { offset: 0, heartbeat: 4, bytes } if &bytes[..] == b"ls"
    ));
    assert!(matches!(
        view.sent_by_view().await,
        Frame::Input { offset: 2, .. }
    ));

    let block = vec![b'x'; 32 * 1024];
    view.input(&block).await.unwrap();
    view.input(&block).await.unwrap();
    let (reply, mut held) = oneshot::channel();
    view.commands
        .send(RemoteRequest::Control {
            control: TerminalControl::Input {
                bytes: b"z".to_vec(),
            },
            reply,
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(held.try_recv().is_err());
    view.host
        .send(&Frame::InputAck {
            offset: 3 + 64 * 1024,
        })
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), held)
            .await
            .unwrap()
            .unwrap()
            .is_ok()
    );
}

#[tokio::test]
async fn a_lost_connection_says_that_input_was_not_sent_when_the_host_did_not_confirm_all_of_it() {
    let mut view = view().await;
    view.keyframe(0, 100).await;
    view.updates.recv().await.unwrap();
    view.input(b"12345").await.unwrap();
    view.host
        .send(&Frame::InputAck { offset: 2 })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    view.host.close().await;
    let (result, reason) = view.ended.await.unwrap();
    assert!(result.is_err());
    assert_eq!(reason, "Lost. Some of your last input was not sent.");
}

#[tokio::test]
async fn the_end_from_the_host_ends_the_view_with_all_input_confirmed() {
    let mut view = view().await;
    view.keyframe(0, 100).await;
    view.updates.recv().await.unwrap();
    view.input(b"exit\r").await.unwrap();
    view.host
        .send(&Frame::End(wire::End {
            final_sequence: 3,
            reason: "Terminal closed.".into(),
        }))
        .await
        .unwrap();
    assert!(matches!(
        view.updates.recv().await,
        Some(RemoteUpdate::Ended { final_sequence: 3 })
    ));
    let (_, reason) = view.ended.await.unwrap();
    assert_eq!(reason, "Lost.");
}
