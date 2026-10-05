use std::sync::atomic::AtomicU64;

use super::{
    super::channel::tests::{Pipe, noise, pair},
    *,
};
use crate::{
    identity::keys::DeviceKeys,
    network::CheckpointCut,
    terminal::{Checkpoint, TerminalScreen, TerminalSize},
};

struct Terminal {
    frames: broadcast::Sender<PublishedFrame>,
    sequence: Arc<AtomicU64>,
    input: mpsc::UnboundedReceiver<Vec<u8>>,
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

async fn terminal(snapshot_bytes: usize) -> Terminal {
    let (host, viewer) = (
        DeviceKeys::generate().unwrap(),
        DeviceKeys::generate().unwrap(),
    );
    let (viewer, channel) = pair(&host, &viewer).await;
    let (frames, output) = broadcast::channel(512);
    let (requests, mut received) = mpsc::channel(64);
    let sequence = Arc::new(AtomicU64::new(0));
    let (typed, input) = mpsc::unbounded_channel();
    let (barrier, current) = (frames.clone(), Arc::clone(&sequence));
    tokio::spawn(async move {
        while let Some(request) = received.recv().await {
            match request {
                HostRequest::Bootstrap {
                    request_id,
                    history,
                    reply,
                } => {
                    let body = noise(if history { snapshot_bytes } else { VISIBLE });
                    let checkpoint = Checkpoint::new(
                        TerminalSize::new(24, 80).unwrap(),
                        TerminalScreen::Primary,
                        body,
                        0,
                        0,
                        false,
                    )
                    .unwrap();
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
                    drop(reply.send(Err("The terminal refused the size.".to_owned())));
                }
                _ => {}
            }
        }
    });
    let served = tokio::spawn(async move {
        let mut stream = Stream::new(
            channel,
            output,
            &requests,
            Controller {
                user: "user".into(),
                device: "device".into(),
                connection: Uuid::now_v7(),
                authorization: CancellationToken::new(),
            },
        );
        let result = stream.serve().await;
        stream.channel.close().await;
        result
    });
    Terminal {
        frames,
        sequence,
        input,
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
