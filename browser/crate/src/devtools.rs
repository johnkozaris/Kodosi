use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use futures_util::{SinkExt as _, StreamExt as _};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

use crate::Error;

const CALL_LIMIT: Duration = Duration::from_secs(10);
const OUTGOING: usize = 1024;

/// One event of the browser or of a page.
#[derive(Debug)]
pub(crate) struct Event {
    pub session: Option<String>,
    pub method: String,
    pub params: Value,
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, Error>>>>>;

/// The browser's debug connection.
#[derive(Clone)]
pub(crate) struct Devtools {
    outgoing: mpsc::Sender<String>,
    pending: Pending,
    next: Arc<AtomicU64>,
}

#[derive(Deserialize)]
struct Reply {
    id: Option<u64>,
    result: Option<Value>,
    error: Option<ReplyError>,
    method: Option<String>,
    params: Option<Value>,
    #[serde(rename = "sessionId")]
    session: Option<String>,
}

#[derive(Deserialize)]
struct ReplyError {
    message: String,
}

impl Devtools {
    /// The reader takes each answer at once and queues the events without a limit, so a slow
    /// reader of events never holds up the answers that it may wait for.
    pub(crate) async fn connect(
        address: &str,
    ) -> Result<(Self, mpsc::UnboundedReceiver<Event>), Error> {
        let (socket, _) = tokio_tungstenite::connect_async(address)
            .await
            .map_err(|error| Error::Engine(format!("the debug port refused: {error}")))?;
        let (mut sink, mut source) = socket.split();
        let (outgoing, mut messages) = mpsc::channel::<String>(OUTGOING);
        let (events_in, events) = mpsc::unbounded_channel();
        let pending = Pending::default();
        tokio::spawn(async move {
            while let Some(message) = messages.recv().await {
                if sink.send(Message::text(message)).await.is_err() {
                    return;
                }
            }
        });
        let replies = Arc::clone(&pending);
        tokio::spawn(async move {
            while let Some(Ok(message)) = source.next().await {
                let Message::Text(text) = message else {
                    continue;
                };
                let Ok(reply) = serde_json::from_str::<Reply>(&text) else {
                    continue;
                };
                if !answer(&replies, reply.id, reply.result, reply.error)
                    && let Some(method) = reply.method
                    && events_in
                        .send(Event {
                            session: reply.session,
                            method,
                            params: reply.params.unwrap_or(Value::Null),
                        })
                        .is_err()
                {
                    break;
                }
            }
            let waiting =
                std::mem::take(&mut *replies.lock().unwrap_or_else(PoisonError::into_inner));
            for (_, waiter) in waiting {
                drop(waiter.send(Err(Error::Stopped)));
            }
        });
        Ok((
            Self {
                outgoing,
                pending,
                next: Arc::new(AtomicU64::new(1)),
            },
            events,
        ))
    }

    /// Sends a command and waits for its answer.
    pub(crate) async fn call(
        &self,
        method: &str,
        params: Value,
        session: Option<&str>,
    ) -> Result<Value, Error> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (reply, answered) = oneshot::channel();
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, reply);
        self.outgoing
            .send(command(method, &params, session, id))
            .await
            .map_err(|_| Error::Stopped)?;
        let answer = tokio::time::timeout(CALL_LIMIT, answered).await;
        if answer.is_err() {
            self.pending
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&id);
        }
        answer
            .map_err(|_| Error::Engine(format!("{method} did not answer")))?
            .map_err(|_| Error::Stopped)?
    }

    /// Sends a command in order with the others, with no wait for its answer. A person's input
    /// must not wait for the page to take the event before it.
    pub(crate) fn send(&self, method: &str, params: &Value, session: Option<&str>) {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let message = command(method, params, session, id);
        if self.outgoing.try_send(message).is_err() {
            tracing::debug!(method, "the debug connection is full");
        }
    }
}

fn command(method: &str, params: &Value, session: Option<&str>, id: u64) -> String {
    let mut message = json!({ "id": id, "method": method, "params": params });
    if let Some(session) = session {
        message["sessionId"] = json!(session);
    }
    message.to_string()
}

fn answer(
    replies: &Pending,
    id: Option<u64>,
    result: Option<Value>,
    error: Option<ReplyError>,
) -> bool {
    let Some(id) = id else {
        return false;
    };
    let waiter = replies
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&id);
    if let Some(waiter) = waiter {
        let outcome = match error {
            Some(error) => Err(Error::Engine(error.message)),
            None => Ok(result.unwrap_or(Value::Null)),
        };
        drop(waiter.send(outcome));
    }
    true
}
