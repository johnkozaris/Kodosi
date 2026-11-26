use std::{
    collections::HashSet,
    sync::{Arc, Mutex, PoisonError},
};

use futures_util::{SinkExt as _, StreamExt as _};
use shared_browser::link::{Address, EncodedFrame, FromEngine, ToEngine};
use stream::{Encoded, FrameKind};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

const REPORTS_WAITING: usize = 1024;
/// Small frame queue cap so a slow host drops frames instead of building latency.
const FRAMES_WAITING: usize = 4;

#[derive(Clone)]
pub(crate) struct Link {
    reports: mpsc::Sender<Message>,
    frames: mpsc::Sender<Message>,
    /// Windows that dropped a frame; subsequent non-keyframes are skipped until the next
    /// keyframe since decoders cannot apply them cleanly.
    broken: Arc<Mutex<HashSet<i64>>>,
}

impl Link {
    pub(crate) fn send(&self, report: &FromEngine) {
        if let Ok(text) = serde_json::to_string(report) {
            drop(self.reports.try_send(Message::text(text)));
        }
    }

    /// Returns false when the frame was not queued. Stills are independent of the video
    /// stream, so they bypass and never alter `broken`.
    fn send_frame(&self, frame: &EncodedFrame) -> bool {
        if frame.still {
            return self
                .frames
                .try_send(Message::binary(frame.to_message()))
                .is_ok();
        }
        let mut broken = self.broken.lock().unwrap_or_else(PoisonError::into_inner);
        if frame.picture.kind != FrameKind::Key && broken.contains(&frame.window) {
            return false;
        }
        let Ok(room) = self.frames.try_reserve() else {
            broken.insert(frame.window);
            return false;
        };
        room.send(Message::binary(frame.to_message()));
        broken.remove(&frame.window);
        true
    }
}

/// Carries one window's pictures to the host. A failed still stays silent, as views keep the
/// previous one.
pub(crate) struct Pictures {
    pub(crate) window: i64,
    pub(crate) still: bool,
    pub(crate) link: Link,
}

impl stream_encode::Output for Pictures {
    fn frame(&self, picture: Encoded) -> bool {
        self.link.send_frame(&EncodedFrame {
            window: self.window,
            still: self.still,
            picture,
        })
    }

    fn failed(&self, problem: String) {
        if !self.still {
            self.link.send(&FromEngine::Failed {
                window: self.window,
                problem,
            });
        }
    }
}

/// Connects to the host and posts `ToEngine::Quit` when the link closes or fails to start, so
/// the engine never outlives its host.
pub(crate) fn connect(address: Address) -> Link {
    let (reports, waiting_reports) = mpsc::channel(REPORTS_WAITING);
    let (frames, waiting_frames) = mpsc::channel(FRAMES_WAITING);
    let started = std::thread::Builder::new()
        .name("kodosi-link".to_owned())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            if let Ok(runtime) = runtime {
                runtime.block_on(carry(address, waiting_reports, waiting_frames));
            }
            crate::engine::post(ToEngine::Quit);
        });
    if started.is_err() {
        crate::engine::post(ToEngine::Quit);
    }
    Link {
        reports,
        frames,
        broken: Arc::default(),
    }
}

async fn carry(
    address: Address,
    mut reports: mpsc::Receiver<Message>,
    mut frames: mpsc::Receiver<Message>,
) {
    let url = format!("ws://127.0.0.1:{}{}", address.port, address.path());
    let Ok((socket, _)) = tokio_tungstenite::connect_async(url).await else {
        return;
    };
    let (mut sink, mut source) = socket.split();
    loop {
        let outgoing = tokio::select! {
            biased;
            incoming = source.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    if let Ok(command) = serde_json::from_str::<ToEngine>(&text) {
                        crate::engine::post(command);
                    }
                    continue;
                }
                Some(Ok(Message::Close(_)) | Err(_)) | None => return,
                Some(Ok(_)) => continue,
            },
            report = reports.recv() => report,
            frame = frames.recv() => frame,
        };
        let Some(outgoing) = outgoing else { return };
        if sink.send(outgoing).await.is_err() {
            return;
        }
    }
}
