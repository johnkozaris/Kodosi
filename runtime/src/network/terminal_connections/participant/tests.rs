use super::{
    super::channel::tests::{Pipe, pair, stand_in_snapshot},
    *,
};
use crate::{identity::keys::DeviceKeys, terminal::Checkpoint};

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
        let checkpoint = Checkpoint::new(stand_in_snapshot(bytes), false).unwrap();
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
    }) = view.updates.recv().await
    else {
        panic!("snapshot");
    };
    assert_eq!(checkpoint.snapshot().len(), 100_000);
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

    view.keyframe(40, 200).await;
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
async fn a_view_refuses_a_snapshot_that_does_not_end() {
    let (host, viewer) = (
        DeviceKeys::generate().unwrap(),
        DeviceKeys::generate().unwrap(),
    );
    let (channel, _host) = pair(&host, &viewer).await;
    let (_commands, requests) = mpsc::channel(1);
    let (updates, _updates) = mpsc::channel(1);
    let mut viewer = Viewer::new(channel, requests, updates);
    viewer.snapshot_sequence = Some(1);
    viewer.snapshot = vec![0; wire::PACKED_SNAPSHOT_LIMIT - wire::KEYFRAME_PART / 2];
    let part = Frame::Keyframe {
        next_sequence: 1,
        more: true,
        part: Bytes::from(vec![0; wire::KEYFRAME_PART]),
    };
    assert!(viewer.frame(part, 0).await.is_err());
}

#[tokio::test]
async fn input_goes_at_once_in_order_and_waits_only_when_too_much_is_not_confirmed() {
    let mut view = view().await;
    assert!(view.input(b"early").await.is_err());
    view.keyframe(0, 200).await;
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
    view.keyframe(0, 200).await;
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
    assert_eq!(reason, "Lost. Kodosi did not send some of your last input.");
}

#[tokio::test]
async fn the_end_from_the_host_ends_the_view_with_all_input_confirmed() {
    let mut view = view().await;
    view.keyframe(0, 200).await;
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
        Some(RemoteUpdate::Ended)
    ));
    let (_, reason) = view.ended.await.unwrap();
    assert_eq!(reason, "Lost.");
}

#[tokio::test]
async fn a_view_sends_the_parts_of_a_picture_only_when_the_host_asks_and_ends_when_it_is_pasted() {
    let mut view = view().await;
    view.keyframe(0, 200).await;
    view.updates.recv().await.unwrap();
    assert!(matches!(view.sent_by_view().await, Frame::Ack { .. }));
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.resize(3 * PICTURE_PART + 10, 7);
    let picture = crate::terminal::Picture::new(Bytes::from(bytes.clone())).unwrap();
    let (reply, pasted) = oneshot::channel();
    view.commands
        .send(RemoteRequest::Picture {
            picture: picture.clone(),
            reply,
        })
        .await
        .unwrap();
    let Frame::Picture(offer) = view.sent_by_view().await else {
        panic!("picture offer");
    };
    assert_eq!(
        (offer.size, offer.sha256.as_str()),
        (bytes.len() as u64, picture.sha256().as_str())
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(200), view.host.receive())
            .await
            .is_err()
    );
    let answer = |received, step| {
        Frame::PictureState(PictureState {
            id: offer.id,
            received,
            at: 0,
            step,
            message: String::new(),
        })
    };
    view.host
        .send(&answer(0, PictureStep::Receiving))
        .await
        .unwrap();
    let mut arrived = Vec::new();
    while arrived.len() < bytes.len() {
        let Frame::PicturePart { id, offset, bytes } = view.sent_by_view().await else {
            panic!("picture part");
        };
        assert_eq!((id, offset), (offer.id, arrived.len() as u64));
        arrived.extend_from_slice(&bytes);
        view.host
            .send(&answer(arrived.len() as u64, PictureStep::Receiving))
            .await
            .unwrap();
    }
    assert_eq!(arrived, bytes);
    view.host
        .send(&answer(bytes.len() as u64, PictureStep::Pasted))
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), pasted)
            .await
            .unwrap()
            .unwrap()
            .is_ok()
    );
    let mut last = None;
    while let Ok(Some(RemoteUpdate::Picture { sent, total })) =
        tokio::time::timeout(Duration::from_millis(100), view.updates.recv()).await
    {
        last = Some((sent, total));
    }
    assert_eq!(last, Some((bytes.len() as u64, bytes.len() as u64)));
}

#[tokio::test(start_paused = true)]
async fn a_late_answer_for_a_picture_that_the_view_stopped_waiting_for_leaves_the_view_open() {
    let mut view = view().await;
    view.keyframe(0, 200).await;
    view.updates.recv().await.unwrap();
    assert!(matches!(view.sent_by_view().await, Frame::Ack { .. }));
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.resize(100, 1);
    let (reply, given_up) = oneshot::channel();
    view.commands
        .send(RemoteRequest::Picture {
            picture: crate::terminal::Picture::new(Bytes::from(bytes)).unwrap(),
            reply,
        })
        .await
        .unwrap();
    let Frame::Picture(late) = view.sent_by_view().await else {
        panic!("picture offer");
    };
    assert!(given_up.await.unwrap().is_err());
    view.host
        .send(&Frame::PictureState(PictureState {
            id: late.id,
            received: 0,
            at: 0,
            step: PictureStep::Refused,
            message: "Late.".to_owned(),
        }))
        .await
        .unwrap();
    view.host
        .send(&Frame::Heartbeat {
            number: 1,
            next_sequence: 0,
        })
        .await
        .unwrap();
    assert!(matches!(view.sent_by_view().await, Frame::Ack { .. }));
}

#[test]
fn a_picture_fills_the_link_and_input_waits_behind_its_parts_for_about_one_round_trip() {
    let (speed, one_way, burst) = (
        1_000_000.0,
        Duration::from_millis(40),
        Duration::from_millis(100),
    );
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.resize(4_000_000, 7);
    let picture = crate::terminal::Picture::new(Bytes::from(bytes)).unwrap();
    let start = tokio::time::Instant::now();
    let mut upload = Upload::new(Uuid::now_v7(), picture, oneshot::channel().0, start);
    upload.asked = true;
    let mut now = start + one_way;
    upload.received(0, 0, now).unwrap();
    let (mut free, mut waited) = (now, Duration::ZERO);
    let mut answers = VecDeque::new();
    loop {
        while upload.wants_part() {
            let Frame::PicturePart { bytes, .. } = upload.part(now).unwrap() else {
                panic!("picture part");
            };
            let leaves = free.max(now);
            waited = waited.max(leaves - now);
            let length = f64::from(u32::try_from(bytes.len()).unwrap());
            free = leaves + Duration::from_secs_f64(length / speed);
            let arrived = free + one_way - start;
            let bursts = (arrived + one_way).as_micros().div_ceil(burst.as_micros());
            answers.push_back((
                start + burst * u32::try_from(bursts).unwrap(),
                upload.sent,
                u64::try_from(arrived.as_micros()).unwrap(),
            ));
        }
        let Some((at, received, host)) = answers.pop_front() else {
            break;
        };
        now = at;
        upload.received(received, host, now).unwrap();
    }
    assert_eq!(upload.received, 4_000_000);
    let took = now - start;
    assert!(took < Duration::from_secs(5), "{took:?}");
    assert!(waited < 2 * upload.round_trip, "{waited:?}");
}
