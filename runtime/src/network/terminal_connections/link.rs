use std::sync::atomic::AtomicUsize;

use super::{channel::Transport, *};
use crate::rooms::crypto::SealedStatus;

const SEND: Duration = Duration::from_secs(10);
const WAITING_FRAMES: usize = 256;
const PIPE_WAITING_BYTES: usize = 16 * 1024 * 1024;
const PIPE_ID: usize = 16;
const SILENCE: Duration = Duration::from_secs(45);

#[cfg_attr(
    not(feature = "host"),
    expect(
        dead_code,
        reason = "only a host registers terminals on the device link"
    )
)]
pub(crate) enum HostEvent {
    Refused(Error),
    AccessChanged,
    Viewer {
        pipe: Pipe,
        user: String,
        device: String,
    },
}

enum Incoming {
    Data(Vec<u8>),
    Closed(Option<Error>),
}

struct Route {
    incoming: mpsc::UnboundedSender<Incoming>,
    waiting: Arc<AtomicUsize>,
}

#[cfg_attr(
    not(feature = "host"),
    expect(
        dead_code,
        reason = "only a host registers terminals on the device link"
    )
)]
struct Hosting {
    registration: Uuid,
    incarnation: Uuid,
    events: mpsc::Sender<HostEvent>,
    announced: bool,
    status: Option<SealedStatus>,
}

#[derive(Clone)]
struct Outgoing {
    control: mpsc::UnboundedSender<Message>,
    frames: mpsc::Sender<Message>,
}

#[derive(Default)]
struct Shared {
    outgoing: Option<Outgoing>,
    routes: BTreeMap<Uuid, Route>,
    hosting: BTreeMap<Uuid, Hosting>,
}

#[derive(Default)]
pub(crate) struct Link {
    shared: std::sync::Mutex<Shared>,
    changed: Notify,
}

#[cfg(feature = "host")]
pub(crate) struct Hosted {
    link: Arc<Link>,
    session: Uuid,
    registration: Uuid,
    pub(super) events: mpsc::Receiver<HostEvent>,
}

#[cfg(feature = "host")]
impl Hosted {
    pub(super) fn share(&self, status: Option<SealedStatus>) {
        let Ok(mut shared) = self.link.shared.lock() else {
            return;
        };
        let outgoing = shared.outgoing.clone();
        let Some(hosting) = shared
            .hosting
            .get_mut(&self.session)
            .filter(|hosting| hosting.registration == self.registration)
        else {
            return;
        };
        if let Some(outgoing) = outgoing.filter(|_| hosting.announced) {
            drop(outgoing.control.send(status_message(
                self.session,
                hosting.incarnation,
                status.as_ref(),
            )));
        }
        hosting.status = status;
    }
}

#[cfg(feature = "host")]
impl Drop for Hosted {
    fn drop(&mut self) {
        let Ok(mut shared) = self.link.shared.lock() else {
            return;
        };
        if shared
            .hosting
            .get(&self.session)
            .is_some_and(|hosting| hosting.registration == self.registration)
        {
            shared.hosting.remove(&self.session);
            if let Some(outgoing) = &shared.outgoing {
                drop(
                    outgoing
                        .control
                        .send(text(&json!({"type":"unhost","sessionId":self.session}))),
                );
            }
        }
    }
}

pub(crate) struct Pipe {
    id: Uuid,
    link: Arc<Link>,
    frames: mpsc::Sender<Message>,
    incoming: mpsc::UnboundedReceiver<Incoming>,
    waiting: Arc<AtomicUsize>,
}

#[cfg(feature = "host")]
impl Pipe {
    pub(super) const fn id(&self) -> Uuid {
        self.id
    }
}

impl Transport for Pipe {
    async fn send(&mut self, bytes: Vec<u8>) -> Result<()> {
        let mut frame = Vec::with_capacity(PIPE_ID + bytes.len());
        frame.extend_from_slice(self.id.as_bytes());
        frame.extend_from_slice(&bytes);
        tokio::time::timeout(SEND, self.frames.send(Message::Binary(frame.into())))
            .await
            .map_err(|_| Error::Closed)?
            .map_err(|_| Error::Closed)
    }

    async fn receive(&mut self) -> Result<Option<Vec<u8>>> {
        match self.incoming.recv().await {
            Some(Incoming::Data(bytes)) => {
                self.waiting.fetch_sub(bytes.len(), Ordering::AcqRel);
                Ok(Some(bytes))
            }
            Some(Incoming::Closed(Some(error))) => Err(error),
            Some(Incoming::Closed(None)) | None => Ok(None),
        }
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        let open = self
            .link
            .shared
            .lock()
            .is_ok_and(|mut shared| shared.routes.remove(&self.id).is_some());
        if open {
            let (frames, close) = (
                self.frames.clone(),
                text(&json!({"type":"close","pipe":self.id})),
            );
            tokio::spawn(async move {
                let _sent = tokio::time::timeout(SEND, frames.send(close)).await;
            });
        }
    }
}

fn text(value: &Value) -> Message {
    Message::Text(value.to_string().into())
}

fn status_message(session: Uuid, incarnation: Uuid, status: Option<&SealedStatus>) -> Message {
    text(&json!({"type":"status","sessionId":session,"incarnationId":incarnation,"status":status}))
}

impl Link {
    #[cfg(feature = "host")]
    pub(crate) fn host(self: &Arc<Self>, session: Uuid, incarnation: Uuid) -> Hosted {
        let (events, received) = mpsc::channel(128);
        let registration = Uuid::now_v7();
        if let Ok(mut shared) = self.shared.lock() {
            shared.hosting.insert(
                session,
                Hosting {
                    registration,
                    incarnation,
                    events,
                    announced: false,
                    status: None,
                },
            );
        }
        self.changed.notify_one();
        Hosted {
            link: Arc::clone(self),
            session,
            registration,
            events: received,
        }
    }

    pub(crate) fn open(self: &Arc<Self>, session: Uuid, incarnation: Uuid) -> Result<Pipe> {
        let id = Uuid::now_v7();
        let mut shared = self.shared.lock().map_err(|_| Error::Closed)?;
        let outgoing = shared
            .outgoing
            .clone()
            .ok_or_else(|| invalid("Kodosi could not reach the terminal connection service."))?;
        let pipe = self.route(&mut shared, id, &outgoing);
        drop(shared);
        outgoing
            .control
            .send(text(
                &json!({"type":"open","pipe":id,"sessionId":session,"incarnationId":incarnation}),
            ))
            .map_err(|_| Error::Closed)?;
        Ok(pipe)
    }

    fn route(self: &Arc<Self>, shared: &mut Shared, id: Uuid, outgoing: &Outgoing) -> Pipe {
        let (incoming, received) = mpsc::unbounded_channel();
        let waiting = Arc::new(AtomicUsize::new(0));
        shared.routes.insert(
            id,
            Route {
                incoming,
                waiting: Arc::clone(&waiting),
            },
        );
        Pipe {
            id,
            link: Arc::clone(self),
            frames: outgoing.frames.clone(),
            incoming: received,
            waiting,
        }
    }

    fn connected(&self, outgoing: Outgoing) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.outgoing = Some(outgoing);
            for hosting in shared.hosting.values_mut() {
                hosting.announced = false;
            }
        }
        self.announce();
    }

    fn disconnected(&self) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.outgoing = None;
            for route in std::mem::take(&mut shared.routes).into_values() {
                drop(route.incoming.send(Incoming::Closed(None)));
            }
        }
    }

    fn announce(&self) {
        let Ok(mut shared) = self.shared.lock() else {
            return;
        };
        let Some(outgoing) = shared.outgoing.clone() else {
            return;
        };
        for (session, hosting) in &mut shared.hosting {
            if !std::mem::replace(&mut hosting.announced, true) {
                drop(outgoing.control.send(text(
                    &json!({"type":"host","sessionId":session,"incarnationId":hosting.incarnation}),
                )));
                if let Some(status) = &hosting.status {
                    drop(outgoing.control.send(status_message(
                        *session,
                        hosting.incarnation,
                        Some(status),
                    )));
                }
            }
        }
    }

    fn frame(&self, frame: &[u8]) {
        let Some((id, payload)) = frame
            .split_at_checked(PIPE_ID)
            .and_then(|(id, payload)| Some((Uuid::from_slice(id).ok()?, payload)))
        else {
            return;
        };
        let Ok(mut shared) = self.shared.lock() else {
            return;
        };
        let Some(route) = shared.routes.get(&id) else {
            return;
        };
        if route.waiting.fetch_add(payload.len(), Ordering::AcqRel) + payload.len()
            > PIPE_WAITING_BYTES
            || route
                .incoming
                .send(Incoming::Data(payload.to_vec()))
                .is_err()
        {
            shared.routes.remove(&id);
            if let Some(outgoing) = &shared.outgoing {
                drop(
                    outgoing
                        .control
                        .send(text(&json!({"type":"close","pipe":id}))),
                );
            }
        }
    }

    fn message(self: &Arc<Self>, value: &Value) -> Result<Option<wire::Notice>> {
        let mut shared = self.shared.lock().map_err(|_| Error::Closed)?;
        let outgoing = shared.outgoing.clone().ok_or(Error::Closed)?;
        match wire::text(value, "type")? {
            "ping" => drop(outgoing.control.send(text(&json!({"type":"pong"})))),
            "pong" | "hosted" => {}
            "changed" => return Ok(Some(wire::Notice::Changed(wire::Change::read(value)?))),
            "terminal" => {
                return Ok(Some(wire::Notice::Terminal(wire::TerminalNotice::read(
                    value,
                )?)));
            }
            "unhosted" => {
                if let Some(hosting) = shared.hosting.get_mut(&wire::id(value, "sessionId")?) {
                    hosting.announced = false;
                    drop(hosting.events.try_send(HostEvent::Refused(refusal(value))));
                }
            }
            "accessChanged" => {
                if let Some(hosting) = shared.hosting.get(&wire::id(value, "sessionId")?) {
                    drop(hosting.events.try_send(HostEvent::AccessChanged));
                }
            }
            "viewer" => {
                let (id, session) = (wire::id(value, "pipe")?, wire::id(value, "sessionId")?);
                let pipe = self.route(&mut shared, id, &outgoing);
                let event = HostEvent::Viewer {
                    pipe,
                    user: wire::text(value, "userId")?.to_owned(),
                    device: wire::text(value, "deviceId")?.to_owned(),
                };
                let refused = match shared.hosting.get(&session) {
                    Some(hosting) => hosting
                        .events
                        .try_send(event)
                        .err()
                        .map(mpsc::error::TrySendError::into_inner),
                    None => Some(event),
                };
                if refused.is_some() {
                    shared.routes.remove(&id);
                    drop(
                        outgoing
                            .control
                            .send(text(&json!({"type":"close","pipe":id}))),
                    );
                }
                drop(shared);
                drop(refused);
                return Ok(None);
            }
            "closed" => {
                if let Some(route) = shared.routes.remove(&wire::id(value, "pipe")?) {
                    let error =
                        (value["status"].as_u64().unwrap_or(0) != 0).then(|| refusal(value));
                    drop(route.incoming.send(Incoming::Closed(error)));
                }
            }
            _ => return Err(invalid("Unsupported device connection message.")),
        }
        Ok(None)
    }
}

fn refusal(value: &Value) -> Error {
    Error::Backend {
        status: value["status"]
            .as_u64()
            .and_then(|status| u16::try_from(status).ok())
            .unwrap_or(500),
        message: value["message"].as_str().unwrap_or_default().to_owned(),
    }
}

struct Disconnect<'a>(&'a Link);

impl Drop for Disconnect<'_> {
    fn drop(&mut self) {
        self.0.disconnected();
    }
}

pub(crate) async fn run(
    network: &BackendClient,
    credentials: &Credentials,
    notices: &mpsc::UnboundedSender<wire::Notice>,
) -> Result<()> {
    let socket = socket(network, credentials).await?;
    let (mut sink, mut stream) = socket.split();
    let (control, mut controls) = mpsc::unbounded_channel();
    let (frames, mut waiting) = mpsc::channel(WAITING_FRAMES);
    let link = Arc::clone(&network.inner.link);
    link.connected(Outgoing {
        control: control.clone(),
        frames,
    });
    let _disconnect = Disconnect(&link);
    for surface in ["sessions", "friends", "devices", "missions"] {
        drop(notices.send(wire::Notice::Changed(wire::Change::of(surface))));
    }
    let writer = async {
        loop {
            let message = tokio::select! {
                biased;
                Some(message) = controls.recv() => message,
                Some(message) = waiting.recv() => message,
                else => return Err(Error::Closed),
            };
            tokio::time::timeout(SEND, sink.send(message))
                .await
                .map_err(|_| Error::Closed)?
                .map_err(|_| Error::Closed)?;
        }
    };
    let reader = async {
        let mut heartbeat = tokio::time::interval(Duration::from_secs(20));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut heard = tokio::time::Instant::now();
        loop {
            tokio::select! {
                _ = heartbeat.tick() => {
                    if heard.elapsed() > SILENCE {
                        return Err(Error::Closed);
                    }
                    drop(control.send(text(&json!({"type":"ping"}))));
                }
                () = link.changed.notified() => link.announce(),
                () = network.inner.friends_changed.notified() => drop(notices.send(wire::Notice::Changed(wire::Change::of("friends")))),
                incoming = stream.next() => {
                    let message = incoming.ok_or(Error::Closed)?.map_err(|error| invalid(error.to_string()))?;
                    heard = tokio::time::Instant::now();
                    match message {
                        Message::Binary(frame) => link.frame(&frame),
                        Message::Text(message) => {
                            if message.len() > 16 * 1024 {
                                return Err(invalid("Device connection message exceeds its size limit."));
                            }
                            if let Some(notice) = link.message(&serde_json::from_str(&message)?)? {
                                drop(notices.send(notice));
                            }
                        }
                        Message::Ping(payload) => drop(control.send(Message::Pong(payload))),
                        Message::Pong(_) => {}
                        Message::Close(_) => return Err(Error::Closed),
                        Message::Frame(_) => return Err(invalid("Unexpected device connection payload.")),
                    }
                }
            }
        }
    };
    tokio::select! {
        result = writer => result,
        result = reader => result,
    }
}

#[cfg(all(test, feature = "host"))]
mod tests {
    use super::*;

    fn connect(link: &Arc<Link>) -> mpsc::UnboundedReceiver<Message> {
        let (control, controls) = mpsc::unbounded_channel();
        let (frames, _waiting) = mpsc::channel(1);
        link.connected(Outgoing { control, frames });
        controls
    }

    fn messages(controls: &mut mpsc::UnboundedReceiver<Message>) -> Vec<Value> {
        let mut messages = Vec::new();
        while let Ok(Message::Text(text)) = controls.try_recv() {
            messages.push(serde_json::from_str(&text).unwrap());
        }
        messages
    }

    #[test]
    fn each_new_device_connection_hears_the_terminal_and_then_its_status() {
        let link = Arc::new(Link::default());
        let hosted = link.host(Uuid::now_v7(), Uuid::now_v7());
        let status = SealedStatus {
            key_version: 2,
            epoch: 1,
            nonce: "bm9uY2U=".to_owned(),
            ciphertext: "c2VhbGVk".to_owned(),
        };
        hosted.share(Some(status.clone()));
        let mut first = connect(&link);
        let sent = messages(&mut first);
        assert_eq!(
            sent.iter()
                .map(|message| message["type"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["host", "status"]
        );
        assert_eq!(
            serde_json::from_value::<SealedStatus>(sent[1]["status"].clone()).unwrap(),
            status
        );
        hosted.share(None);
        assert_eq!(messages(&mut first)[0]["status"], Value::Null);
        link.disconnected();
        let mut second = connect(&link);
        assert_eq!(messages(&mut second).len(), 1);
    }
}
