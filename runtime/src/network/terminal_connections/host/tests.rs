use std::sync::atomic::AtomicU64;

use super::{
    super::channel::tests::{Pipe, channel_keys, noise, pair, stand_in_snapshot},
    *,
};
use crate::{
    identity::keys::DeviceKeys,
    network::CheckpointCut,
    terminal::{
        Checkpoint, TERMINAL_REMOTE_IMAGE_DATA_MAX_BYTES, TERMINAL_REMOTE_IMAGE_DATA_MIN_BYTES,
    },
};

struct Terminal {
    frames: broadcast::Sender<PublishedFrame>,
    sequence: Arc<AtomicU64>,
    input: mpsc::UnboundedReceiver<Vec<u8>>,
    pictures: mpsc::UnboundedReceiver<Bytes>,
    image_budgets: mpsc::UnboundedReceiver<u64>,
    viewer: Channel<Pipe>,
    received: u64,
    heartbeat: u32,
    delay: Duration,
    served: tokio::task::JoinHandle<Result<()>>,
}

impl Terminal {
    fn write(&self, bytes: &[u8]) {
        let sequence = self.sequence.fetch_add(1, Ordering::SeqCst);
        drop(self.frames.send(PublishedFrame::Raw {
            sequence,
            bytes: Bytes::copy_from_slice(bytes),
        }));
    }

    async fn next(&mut self) -> Frame {
        loop {
            let (frame, size) = tokio::time::timeout(Duration::from_secs(5), self.viewer.receive())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            if frame.carries_output() {
                self.received += size as u64;
            }
            match frame {
                Frame::Heartbeat { number, .. } => self.heartbeat = number,
                frame => return frame,
            }
        }
    }

    async fn acknowledge(&mut self) {
        let received = self.received;
        self.viewer.send(&Frame::Ack { received }).await.unwrap();
    }

    async fn keyframe(&mut self) -> (u64, usize) {
        let mut size = 0;
        loop {
            match self.next().await {
                Frame::Keyframe {
                    next_sequence,
                    more,
                    part,
                } => {
                    size += part.len();
                    tokio::time::sleep(self.delay).await;
                    self.acknowledge().await;
                    if !more {
                        return (next_sequence, size);
                    }
                }
                other => panic!("expected a keyframe, got {other:?}"),
            }
        }
    }

    async fn flood(&self) {
        let line = noise(4096);
        for _ in 0..200 {
            self.write(&line);
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

const VISIBLE: usize = 300;

fn echo_pipe() -> crate::canvas::PipeEnds {
    let (sender, mut sent) = mpsc::channel(8);
    let (answers, answered) = mpsc::channel(8);
    tokio::spawn(async move {
        while let Some(frame) = sent.recv().await {
            drop(answers.send(frame).await);
        }
    });
    crate::canvas::PipeEnds {
        sender,
        receiver: answered,
    }
}

async fn terminal(snapshot_bytes: usize) -> Terminal {
    let (keys, viewer) = (
        DeviceKeys::generate().unwrap(),
        DeviceKeys::generate().unwrap(),
    );
    let (viewer, channel) = pair(&keys, &viewer).await;
    let (frames, output) = broadcast::channel(512);
    let (requests, mut received) = mpsc::channel(64);
    let door = Door::new(
        Arc::new(channel_keys(&keys)),
        Vec::new(),
        requests.clone(),
        Uuid::now_v7(),
        CancellationToken::new(),
    );
    let sequence = Arc::new(AtomicU64::new(0));
    let (typed, input) = mpsc::unbounded_channel();
    let (pasted, pictures) = mpsc::unbounded_channel();
    let (image_budget, image_budgets) = mpsc::unbounded_channel();
    let (barrier, current) = (frames.clone(), Arc::clone(&sequence));
    tokio::spawn(async move {
        let mut saved = std::collections::HashSet::new();
        while let Some(request) = received.recv().await {
            match request {
                HostRequest::Bootstrap {
                    request_id,
                    history,
                    image_data_bytes,
                    reply,
                } => {
                    let _ = image_budget.send(image_data_bytes);
                    let body = stand_in_snapshot(if history { snapshot_bytes } else { VISIBLE });
                    let checkpoint = Checkpoint::new(body, false).unwrap();
                    drop(barrier.send(PublishedFrame::BootstrapBarrier { request_id }));
                    drop(reply.send(Ok(CheckpointCut {
                        checkpoint,
                        next_sequence: current.load(Ordering::SeqCst),
                    })));
                }
                HostRequest::Control {
                    control: TerminalControl::Input { bytes },
                    reply,
                    ..
                } => {
                    drop(typed.send(bytes));
                    drop(reply.send(Ok(json!({"accepted":true}))));
                }
                HostRequest::Control { reply, .. } => {
                    drop(reply.send(Err("The terminal refused the size.".into())));
                }
                HostRequest::Document { reply, .. } => drop(reply.send(Ok(echo_pipe()))),
                HostRequest::Picture { picture, reply, .. } => {
                    let answer = match picture {
                        HostPicture::Known { digest, .. } if !saved.contains(&digest) => {
                            json!({"missing": true})
                        }
                        HostPicture::Known { .. } => json!({"accepted": true}),
                        HostPicture::Whole(picture) => {
                            saved.insert(picture.digest);
                            drop(pasted.send(picture.bytes));
                            json!({"accepted": true})
                        }
                    };
                    drop(reply.send(Ok(answer)));
                }
                _ => {}
            }
        }
    });
    let served = tokio::spawn(async move {
        let (result, channel) = {
            let mut stream = Stream::new(
                channel,
                output,
                &requests,
                Controller {
                    connection: Uuid::now_v7(),
                    authorization: CancellationToken::new(),
                },
                door,
            );
            (stream.serve().await, stream.channel)
        };
        channel.close().await;
        result
    });
    Terminal {
        frames,
        sequence,
        input,
        pictures,
        image_budgets,
        viewer,
        received: 0,
        heartbeat: 0,
        delay: Duration::ZERO,
        served,
    }
}

#[tokio::test]
async fn a_view_starts_with_the_current_screen_and_then_gets_output_in_order() {
    let mut terminal = terminal(100).await;
    terminal.write(b"before the view");
    let (next_sequence, _) = terminal.keyframe().await;
    assert!(next_sequence <= 1);
    let mut expected = next_sequence;
    let mut received = Vec::new();
    for word in [&b"one "[..], b"two ", b"three"] {
        terminal.write(word);
    }
    while received.len() < 3 {
        let Frame::Output {
            first_sequence,
            chunks,
        } = terminal.next().await
        else {
            panic!("output");
        };
        assert_eq!(first_sequence, expected);
        expected += chunks.len() as u64;
        received.extend(chunks.iter().map(|chunk| chunk.to_vec()));
    }
    assert_eq!(&received[received.len() - 3..].concat(), b"one two three");
}

#[tokio::test]
async fn terminal_output_goes_before_the_frames_of_a_document() {
    let mut terminal = terminal(100).await;
    terminal.keyframe().await;
    let lines = 8;
    for _ in 0..lines {
        terminal.write(&noise(1024));
    }
    let pipe = Uuid::now_v7();
    let opening = Frame::PipeOpen {
        pipe,
        key: "notes.docx".to_owned(),
        name: "Ana".to_owned(),
    };
    terminal.viewer.send(&opening).await.unwrap();
    let frame = Frame::PipeData {
        pipe,
        more: false,
        part: Bytes::from_static(b"join"),
    };
    terminal.viewer.send(&frame).await.unwrap();
    let mut seen = 0;
    loop {
        let frame = terminal.next().await;
        terminal.acknowledge().await;
        match frame {
            Frame::Output {
                first_sequence,
                chunks,
            } => seen = first_sequence + chunks.len() as u64,
            Frame::PipeData { part, .. } => {
                assert_eq!(part, Bytes::from_static(b"join"));
                break;
            }
            _ => {}
        }
    }
    assert_eq!(seen, lines);
}

#[tokio::test]
async fn a_view_that_falls_behind_gets_the_current_screen_and_not_the_backlog() {
    let mut terminal = terminal(100).await;
    terminal.delay = Duration::from_millis(100);
    let (first, _) = terminal.keyframe().await;
    assert_eq!(first, 0);
    terminal.flood().await;
    let mut expected = first;
    loop {
        let frame = terminal.next().await;
        terminal.acknowledge().await;
        match frame {
            Frame::Output {
                first_sequence,
                chunks,
            } => {
                assert_eq!(first_sequence, expected);
                expected += chunks.len() as u64;
            }
            Frame::Keyframe { next_sequence, .. } => {
                assert!(expected < 100, "{expected} chunks were delivered");
                assert_eq!(next_sequence, 200);
                expected = next_sequence;
                break;
            }
            _ => {}
        }
    }
    terminal.write(b"last");
    let Frame::Output {
        first_sequence,
        chunks,
    } = terminal.next().await
    else {
        panic!("output");
    };
    assert_eq!((first_sequence, &chunks[0][..]), (expected, &b"last"[..]));
}

#[tokio::test]
async fn a_view_gets_the_visible_screen_first_and_the_history_when_output_is_quiet() {
    let mut terminal = terminal(400_000).await;
    terminal.delay = Duration::from_millis(50);
    let (first, visible) = terminal.keyframe().await;
    assert!(visible < 1024, "the first snapshot had {visible} bytes");
    terminal.viewer.send(&Frame::Refresh).await.unwrap();
    assert_eq!(terminal.keyframe().await, (first, visible));
    terminal.flood().await;
    let mut expected = first;
    let screen = loop {
        let frame = terminal.next().await;
        terminal.acknowledge().await;
        match frame {
            Frame::Output {
                first_sequence,
                chunks,
            } => {
                assert_eq!(first_sequence, expected);
                expected += chunks.len() as u64;
            }
            Frame::Keyframe {
                next_sequence,
                more: false,
                part,
            } => {
                assert_eq!(part.len(), visible);
                break next_sequence;
            }
            other => panic!("a slow view got {other:?} in place of the visible screen"),
        }
    };
    assert_eq!(screen, 200);
    terminal.write(b"after the screen");
    let Frame::Output { first_sequence, .. } = terminal.next().await else {
        panic!("output");
    };
    assert_eq!(first_sequence, screen);
    terminal.acknowledge().await;
    let (next_sequence, exact) = terminal.keyframe().await;
    assert_eq!(next_sequence, screen + 1);
    assert!(exact > 300_000, "the exact snapshot had {exact} bytes");
}

#[tokio::test(start_paused = true)]
async fn a_snapshot_holds_the_image_pixels_that_the_link_of_the_view_carries() {
    for (delay, image_data_bytes) in [
        (
            Duration::from_millis(1),
            TERMINAL_REMOTE_IMAGE_DATA_MAX_BYTES,
        ),
        (Duration::from_secs(1), TERMINAL_REMOTE_IMAGE_DATA_MIN_BYTES),
    ] {
        let mut terminal = terminal(400_000).await;
        terminal.delay = delay;
        terminal.keyframe().await;
        let (_, exact) = terminal.keyframe().await;
        assert!(exact > 300_000, "the exact snapshot had {exact} bytes");
        terminal.viewer.send(&Frame::Refresh).await.unwrap();
        terminal.keyframe().await;
        let mut budgets = Vec::new();
        while let Ok(budget) = terminal.image_budgets.try_recv() {
            budgets.push(budget);
        }
        assert_eq!(
            budgets,
            [
                TERMINAL_REMOTE_IMAGE_DATA_MIN_BYTES,
                TERMINAL_REMOTE_IMAGE_DATA_MIN_BYTES,
                image_data_bytes
            ]
        );
    }
}

#[tokio::test]
async fn a_large_snapshot_waits_for_the_viewer_between_its_parts() {
    let mut terminal = terminal(400_000).await;
    terminal.delay = Duration::from_millis(100);
    terminal.keyframe().await;
    let mut parts = 0;
    while let Ok(Ok(Some((frame, _)))) =
        tokio::time::timeout(Duration::from_millis(1500), terminal.viewer.receive()).await
    {
        parts += usize::from(matches!(frame, Frame::Keyframe { .. }));
    }
    assert!(
        (1..=2).contains(&parts),
        "{parts} parts went without an acknowledgement"
    );
}

#[tokio::test]
async fn input_is_confirmed_in_order_and_input_out_of_order_ends_the_channel() {
    let mut terminal = terminal(100).await;
    terminal.keyframe().await;
    terminal.write(b"prompt");
    assert!(matches!(terminal.next().await, Frame::Output { .. }));
    let heartbeat = terminal.heartbeat;
    for (offset, bytes) in [(0, &b"ls"[..]), (2, b"\r")] {
        terminal
            .viewer
            .send(&Frame::Input {
                offset,
                heartbeat,
                bytes: Bytes::copy_from_slice(bytes),
            })
            .await
            .unwrap();
    }
    assert!(matches!(
        terminal.next().await,
        Frame::InputAck { offset: 2 }
    ));
    assert!(matches!(
        terminal.next().await,
        Frame::InputAck { offset: 3 }
    ));
    assert_eq!(terminal.input.recv().await.unwrap(), b"ls");
    assert_eq!(terminal.input.recv().await.unwrap(), b"\r");

    let request_id = Uuid::now_v7();
    terminal
        .viewer
        .send(&Frame::Control {
            request_id,
            control: TerminalControl::Interrupt,
        })
        .await
        .unwrap();
    let Frame::ControlResult(result) = terminal.next().await else {
        panic!("control result");
    };
    assert_eq!((result.request_id, result.accepted), (request_id, false));

    terminal
        .viewer
        .send(&Frame::Input {
            offset: 9,
            heartbeat,
            bytes: Bytes::from_static(b"x"),
        })
        .await
        .unwrap();
    assert!(terminal.served.await.unwrap().is_err());
    assert!(terminal.input.try_recv().is_err());
}

#[tokio::test(start_paused = true)]
async fn a_view_stays_while_it_answers_and_ends_once_it_falls_silent() {
    let mut terminal = terminal(100).await;
    terminal.keyframe().await;
    let mut answered = 0;
    while answered < 4 {
        let (frame, size) = terminal.viewer.receive().await.unwrap().unwrap();
        if frame.carries_output() {
            terminal.received += size as u64;
        }
        if matches!(frame, Frame::Heartbeat { .. }) {
            answered += 1;
        }
        terminal.acknowledge().await;
    }
    assert!(!terminal.served.is_finished());
    let ended = tokio::time::timeout(Duration::from_mins(2), &mut terminal.served)
        .await
        .expect("the host keeps a silent view");
    assert!(ended.unwrap().is_err());
}

#[tokio::test]
async fn the_end_reaches_the_view_after_the_output_that_waits() {
    let mut terminal = terminal(100).await;
    terminal.keyframe().await;
    terminal.write(b"goodbye");
    drop(terminal.frames.send(PublishedFrame::Closed {
        reason: "Terminal closed.".into(),
        final_sequence: 1,
    }));
    assert!(matches!(
        terminal.next().await,
        Frame::Output {
            first_sequence: 0,
            ..
        }
    ));
    let Frame::End(end) = terminal.next().await else {
        panic!("end");
    };
    assert_eq!(end.final_sequence, 1);
    assert!(terminal.served.await.unwrap().is_ok());
    assert!(terminal.viewer.receive().await.unwrap().is_none());
}

impl Terminal {
    async fn picture_state(&mut self) -> PictureState {
        loop {
            match self.next().await {
                Frame::PictureState(state) => return state,
                Frame::Keyframe { .. } | Frame::Output { .. } => self.acknowledge().await,
                _ => {}
            }
        }
    }

    async fn offer(&mut self, picture: &Picture, sha256: String) -> PictureState {
        let id = Uuid::now_v7();
        self.viewer
            .send(&Frame::Picture(PictureOffer {
                id,
                size: picture.bytes.len() as u64,
                sha256,
                format: picture.format,
            }))
            .await
            .unwrap();
        let state = self.picture_state().await;
        assert_eq!(state.id, id);
        state
    }

    async fn send_parts(&mut self, id: Uuid, bytes: &[u8]) -> PictureState {
        let mut offset = 0;
        for part in bytes.chunks(wire::PICTURE_PART) {
            self.viewer
                .send(&Frame::PicturePart {
                    id,
                    offset,
                    bytes: Bytes::copy_from_slice(part),
                })
                .await
                .unwrap();
            offset += part.len() as u64;
            let state = self.picture_state().await;
            assert_eq!(
                (state.step, state.received),
                (PictureStep::Receiving, offset)
            );
        }
        self.picture_state().await
    }
}

fn picture(fill: u8) -> Picture {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.resize(2 * wire::PICTURE_PART + 99, fill);
    Picture::new(Bytes::from(bytes)).unwrap()
}

#[tokio::test]
async fn a_picture_is_pasted_once_all_its_parts_arrive_and_later_again_from_its_file() {
    let mut terminal = terminal(100).await;
    terminal.keyframe().await;
    let first = picture(1);
    let asked = terminal.offer(&first, first.sha256()).await;
    assert_eq!((asked.step, asked.received), (PictureStep::Receiving, 0));
    let done = terminal.send_parts(asked.id, &first.bytes).await;
    assert_eq!(done.step, PictureStep::Pasted);
    assert_eq!(terminal.pictures.recv().await.unwrap(), first.bytes);

    let again = terminal.offer(&first, first.sha256()).await;
    assert_eq!(again.step, PictureStep::Pasted);
    assert!(terminal.pictures.try_recv().is_err());

    let damaged = picture(2);
    let asked = terminal.offer(&damaged, picture(3).sha256()).await;
    assert_eq!(asked.step, PictureStep::Receiving);
    let refused = terminal.send_parts(asked.id, &damaged.bytes).await;
    assert_eq!(
        (refused.step, refused.message.map(|message| message.to_string())),
        (
            PictureStep::Refused,
            Some(text!("The picture arrived damaged. Paste it again.").to_string())
        )
    );
    assert!(terminal.pictures.try_recv().is_err());
}

#[tokio::test]
async fn a_picture_takes_the_place_of_one_whose_parts_stopped() {
    let mut terminal = terminal(100).await;
    terminal.keyframe().await;
    let stopped = picture(1);
    let asked = terminal.offer(&stopped, stopped.sha256()).await;
    terminal
        .viewer
        .send(&Frame::PicturePart {
            id: asked.id,
            offset: 0,
            bytes: stopped.bytes.slice(..wire::PICTURE_PART),
        })
        .await
        .unwrap();
    assert_eq!(
        terminal.picture_state().await.received,
        wire::PICTURE_PART as u64
    );
    let next = picture(2);
    let asked = terminal.offer(&next, next.sha256()).await;
    assert_eq!((asked.step, asked.received), (PictureStep::Receiving, 0));
    let done = terminal.send_parts(asked.id, &next.bytes).await;
    assert_eq!(done.step, PictureStep::Pasted);
    assert_eq!(terminal.pictures.recv().await.unwrap(), next.bytes);
}
