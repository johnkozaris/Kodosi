use std::{
    collections::HashSet,
    sync::{Arc, Mutex, PoisonError},
};

use futures_util::{SinkExt as _, StreamExt as _};
use shared_browser::link::{Address, EncodedFrame, FromEngine, ToEngine};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

/// Reports that wait for the link. A host that leaves this many unread is gone.
const REPORTS_WAITING: usize = 1024;
/// Pictures that wait for the link. One that finds no room is lost, so a slow host never piles
/// up pictures.
const FRAMES_WAITING: usize = 4;

/// The engine's way to its host. Any thread may send on it.
#[derive(Clone)]
pub(crate) struct Link {
    reports: mpsc::Sender<Message>,
    frames: mpsc::Sender<Message>,
    /// The windows that lost a picture. The pictures after it need it, so a window sends
    /// nothing more until its next key frame.
    broken: Arc<Mutex<HashSet<i64>>>,
}

impl Link {
    pub(crate) fn send(&self, report: &FromEngine) {
        if let Ok(text) = serde_json::to_string(report) {
            drop(self.reports.try_send(Message::text(text)));
        }
    }

    /// Sends a picture unless the link is full, and says whether its window now needs a key
    /// frame.
    pub(crate) fn send_frame(&self, frame: &EncodedFrame) -> bool {
        let mut broken = self.broken.lock().unwrap_or_else(PoisonError::into_inner);
        if !frame.key && broken.contains(&frame.window) {
            return true;
        }
        let Ok(room) = self.frames.try_reserve() else {
            broken.insert(frame.window);
            return true;
        };
        room.send(Message::binary(frame.to_message()));
        broken.remove(&frame.window);
        false
    }
}

/// Connects to the host on a thread of its own. The engine quits when the link ends, so it never
/// outlives its host.
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
