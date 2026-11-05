use futures_util::{SinkExt as _, StreamExt as _};
use shared_browser::link::{Address, EncodedFrame, FromEngine, ToEngine};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

/// The engine's way to its host. Any thread may send on it.
#[derive(Clone)]
pub(crate) struct Link {
    outgoing: mpsc::UnboundedSender<Message>,
}

impl Link {
    pub(crate) fn send(&self, report: &FromEngine) {
        if let Ok(text) = serde_json::to_string(report) {
            drop(self.outgoing.send(Message::text(text)));
        }
    }

    pub(crate) fn send_frame(&self, frame: &EncodedFrame) {
        drop(self.outgoing.send(Message::binary(frame.to_message())));
    }
}

/// Connects to the host on a thread of its own. The engine quits when the link ends, so it never
/// outlives its host.
pub(crate) fn connect(address: Address) -> Link {
    let (outgoing, waiting) = mpsc::unbounded_channel();
    let started = std::thread::Builder::new()
        .name("kodosi-link".to_owned())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            if let Ok(runtime) = runtime {
                runtime.block_on(carry(address, waiting));
            }
            crate::engine::post(ToEngine::Quit);
        });
    if started.is_err() {
        crate::engine::post(ToEngine::Quit);
    }
    Link { outgoing }
}

async fn carry(address: Address, mut waiting: mpsc::UnboundedReceiver<Message>) {
    let url = format!("ws://127.0.0.1:{}{}", address.port, address.path());
    let Ok((socket, _)) = tokio_tungstenite::connect_async(url).await else {
        return;
    };
    let (mut sink, mut source) = socket.split();
    loop {
        tokio::select! {
            message = waiting.recv() => {
                let Some(message) = message else { return };
                if sink.send(message).await.is_err() {
                    return;
                }
            }
            message = source.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    if let Ok(command) = serde_json::from_str::<ToEngine>(&text) {
                        crate::engine::post(command);
                    }
                }
                Some(Ok(Message::Close(_)) | Err(_)) | None => return,
                Some(Ok(_)) => {}
            },
        }
    }
}
