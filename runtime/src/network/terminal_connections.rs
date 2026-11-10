use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
#[cfg(feature = "host")]
use std::{collections::BTreeSet, sync::atomic::AtomicBool};

use bytes::Bytes;
use futures_util::{SinkExt as _, StreamExt as _};
use reqwest::Method;
use serde_json::{Value, json};
#[cfg(feature = "host")]
use tokio::sync::{Mutex, RwLock, broadcast};
use tokio::{
    net::TcpStream,
    sync::{Notify, mpsc, oneshot},
};
use tokio_tungstenite::{
    Connector, MaybeTlsStream, WebSocketStream,
    tungstenite::{Message, client::IntoClientRequest as _, protocol::WebSocketConfig},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    BackendClient, Error, RemoteConnection, RemoteUpdate, Result, TerminalControl,
    http::Credentials,
    invalid,
    wire::{self, SessionDto},
};
#[cfg(feature = "host")]
use super::{HostRequest, LocalPublication, PublicationOutput, PublishedFrame};

mod calls;
mod channel;
mod direct;
#[cfg(feature = "host")]
mod host;
mod link;
#[cfg(feature = "host")]
mod pacer;
mod participant;

#[cfg(feature = "host")]
pub(crate) use calls::{Answering, answer as answer_calls};
pub(crate) use calls::{Lines, ask as ask_computer};
pub(crate) use channel::ChannelKeys;
pub(crate) use link::Link;

pub(crate) type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub(crate) const STABLE_CONNECTION: Duration = Duration::from_mins(1);
pub(crate) const CONTROL_TIMEOUT: Duration = Duration::from_secs(12);
pub(crate) fn input_not_sent() -> crate::text::Text {
    text!("Kodosi did not send some of your last input.")
}
const MESSAGE_LIMIT: usize = 256 * 1024 + 4096;
const TRUST_CHECK: Duration = Duration::from_mins(5);
const CHANNEL_SILENCE: Duration = Duration::from_secs(45);
static CONNECTION_FAILURE: std::sync::Mutex<crate::diagnostics::Repeats> =
    std::sync::Mutex::new(crate::diagnostics::Repeats::new());

#[cfg(feature = "host")]
pub(crate) struct Publication {
    pub(super) drained: CancellationToken,
    pub info: RwLock<LocalPublication>,
    pub dto: RwLock<SessionDto>,
    pub cancel: CancellationToken,
    pub refresh: Notify,
    pub changing: AtomicBool,
    pub(super) pending_shares: Mutex<Option<BTreeSet<String>>>,
    closing: AtomicBool,
    requests: mpsc::Sender<HostRequest>,
    output: Mutex<Option<PublicationOutput>>,
}
#[cfg(feature = "host")]
impl Publication {
    pub(crate) fn new(
        info: LocalPublication,
        requests: mpsc::Sender<HostRequest>,
        output: PublicationOutput,
        dto: SessionDto,
    ) -> Self {
        Self {
            info: RwLock::new(info),
            dto: RwLock::new(dto),
            cancel: CancellationToken::new(),
            refresh: Notify::new(),
            changing: AtomicBool::new(false),
            drained: CancellationToken::new(),
            pending_shares: Mutex::new(None),
            closing: AtomicBool::new(false),
            requests,
            output: Mutex::new(Some(output)),
        }
    }
    pub(crate) async fn clear_sharing(&self) {
        let mut info = self.info.write().await;
        info.shared_with.clear();
        info.mission_id = None;
        drop(info);
        *self.pending_shares.lock().await = None;
    }

    pub(crate) fn invalidate(&self) {
        self.refresh.notify_one();
    }
}

/// The live documents that one connection holds open.
#[cfg(feature = "host")]
const MOST_PIPES: usize = 16;

pub(crate) enum RemoteRequest {
    Control {
        control: TerminalControl,
        reply: oneshot::Sender<Result<Value>>,
    },
    Checkpoint,
    PipeOpen {
        pipe: Uuid,
        key: String,
        name: String,
        frames: mpsc::Sender<Vec<u8>>,
    },
    PipeData {
        pipe: Uuid,
        more: bool,
        part: Bytes,
    },
    PipeClose {
        pipe: Uuid,
    },
    DirectAsk {
        pipe: Uuid,
        reply: oneshot::Sender<(Arc<direct::Host>, Vec<std::net::SocketAddr>)>,
    },
    Picture {
        picture: crate::terminal::Picture,
        reply: oneshot::Sender<Result<()>>,
    },
}

/// Carries a pipe of a viewer until a side ends it.
pub(super) fn open_pipe(
    commands: mpsc::Sender<RemoteRequest>,
    cancel: CancellationToken,
    (id, key, name): (Uuid, String, String),
) -> crate::canvas::PipeEnds {
    let (to_view, receiver) = mpsc::channel(crate::canvas::PIPE_QUEUE);
    let (sender, from_view) = mpsc::channel(crate::canvas::PIPE_QUEUE);
    tokio::spawn(direct::run(direct::Pipe {
        commands,
        cancel,
        id,
        key,
        name,
        to_view,
        from_view,
    }));
    crate::canvas::PipeEnds { sender, receiver }
}

#[cfg(feature = "host")]
pub(crate) fn spawn_host(
    network: BackendClient,
    publication: Arc<Publication>,
    credentials: Credentials,
) {
    tokio::spawn(async move {
        let _completion = publication.drained.clone().drop_guard();
        let generation = credentials.generation;
        let Some(mut output) = publication.output.lock().await.take() else {
            return;
        };
        let mut delay = 1;
        loop {
            if publication.cancel.is_cancelled()
                || publication.requests.is_closed()
                || network.generation() != generation
                || network.inner.shutdown.is_cancelled()
            {
                break;
            }
            if publication.changing.load(Ordering::Acquire) {
                tokio::select! { ()=publication.cancel.cancelled()=>break, ()=publication.refresh.notified()=>{}, ()=tokio::time::sleep(Duration::from_millis(200))=>{} }
                continue;
            }
            let started = tokio::time::Instant::now();
            let result = tokio::select! {
                biased;
                ()=publication.cancel.cancelled()=>break,
                ()=credentials.cancel.cancelled()=>break,
                ()=network.inner.shutdown.cancelled()=>break,
                result=host::run(&network,&publication,&mut output,generation)=>result,
            };
            if publication.cancel.is_cancelled() {
                break;
            }
            if let Err(error) = result {
                tracing::warn!(%error, "remote access interrupted; reconnecting");
            }
            if started.elapsed() >= STABLE_CONNECTION {
                delay = 1;
            }
            tokio::select! {
                ()=publication.cancel.cancelled()=>break,
                ()=publication.requests.closed()=>break,
                ()=network.inner.shutdown.cancelled()=>break,
                ()=tokio::time::sleep(super::stagger(Duration::from_secs(delay)))=>{}
            }
            delay = (delay * 2).min(15);
        }
    });
}

pub(crate) async fn connect_remote(
    network: BackendClient,
    credentials: Credentials,
    id: Uuid,
) -> Result<RemoteConnection> {
    participant::connect(network, credentials, id).await
}

pub(crate) async fn run_link(
    network: &BackendClient,
    credentials: &Credentials,
    notices: &mpsc::UnboundedSender<wire::Notice>,
) -> Result<()> {
    link::run(network, credentials, notices).await
}

async fn socket(network: &BackendClient, credentials: &Credentials) -> Result<Socket> {
    network.check_credentials(credentials)?;
    network.inner.http.compatible().await?;
    let device_session = network.inner.http.device_session(credentials).await?;
    if let Some(admitted) = admitted(network, credentials, &device_session).await? {
        return Ok(admitted);
    }
    network
        .inner
        .http
        .forget_device_session(&device_session)
        .await;
    let device_session = network.inner.http.device_session(credentials).await?;
    admitted(network, credentials, &device_session)
        .await?
        .ok_or_else(|| Error::Text(text!("The connection service did not admit this device.")))
}

async fn admitted(
    network: &BackendClient,
    credentials: &Credentials,
    device_session: &str,
) -> Result<Option<Socket>> {
    let url = network.inner.http.websocket_url("ws/device")?;
    let mut request = url
        .as_str()
        .into_client_request()
        .map_err(|_| invalid("Invalid terminal connection service address."))?;
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {}", credentials.token.as_str())
            .parse()
            .map_err(|_| invalid("Invalid access token."))?,
    );
    let settings = WebSocketConfig::default()
        .max_message_size(Some(MESSAGE_LIMIT))
        .max_frame_size(Some(MESSAGE_LIMIT));
    let (mut socket, _) = tokio::time::timeout(
        Duration::from_secs(10),
        tokio_tungstenite::connect_async_tls_with_config(
            request,
            Some(settings),
            true,
            Some(Connector::Rustls(Arc::clone(
                &network.inner.http.websocket_tls,
            ))),
        ),
    )
    .await
    .map_err(|_| Error::Text(text!("The connection to the terminal timed out.")))?
    .map_err(|error| {
        if CONNECTION_FAILURE
            .lock()
            .is_ok_and(|mut failure| failure.new_failure(&error))
        {
            tracing::warn!(%error, "terminal connection failed");
        }
        Error::Text(text!(
            "Kodosi could not reach the terminal connection service."
        ))
    })?;
    if let Ok(mut failure) = CONNECTION_FAILURE.lock() {
        failure.worked();
    }
    send_json(&mut socket, json!({"type":"hello","protocolVersion":crate::protocol::TERMINAL_CONNECTION_VERSION,"deviceId":credentials.keys.device_id,"deviceSession":device_session,"hosts":cfg!(feature = "host")})).await?;
    let ready = read_json(&mut socket).await?;
    match wire::text(&ready, "type")? {
        "ready" => Ok(Some(socket)),
        "refused" if ready.get("reason").and_then(Value::as_str) == Some("version") => {
            let reported = |name: &str| ready.get(name).and_then(Value::as_u64).unwrap_or_default();
            super::http::supported(
                crate::protocol::TERMINAL_CONNECTION_VERSION,
                reported("oldest"),
                reported("current"),
            )?;
            Err(Error::Text(text!(
                "The connection service did not admit this device."
            )))
        }
        "refused" => Ok(None),
        _ => Err(Error::Text(text!(
            "The connection service did not admit this device."
        ))),
    }
}

async fn send_json(socket: &mut Socket, value: Value) -> Result<()> {
    let text = serde_json::to_string(&value)?;
    if text.len() > MESSAGE_LIMIT {
        return Err(invalid("Connection message exceeds its limit."));
    }
    tokio::time::timeout(
        Duration::from_secs(10),
        socket.send(Message::Text(text.into())),
    )
    .await
    .map_err(|_| Error::Closed)?
    .map_err(|error| {
        tracing::warn!(%error, "terminal connection send failed");
        Error::Text(text!("The connection to the host dropped."))
    })
}
async fn read_json(socket: &mut Socket) -> Result<Value> {
    let message = tokio::time::timeout(Duration::from_secs(10), socket.next())
        .await
        .map_err(|_| Error::Closed)?
        .ok_or(Error::Closed)?
        .map_err(|error| {
            tracing::warn!(%error, "terminal connection read failed");
            Error::Text(text!("The connection to the host dropped."))
        })?;
    match message {
        Message::Text(text) => serde_json::from_str(&text).map_err(Into::into),
        _ => Err(invalid("Expected a connection handshake message.")),
    }
}

pub(crate) fn check_generation(network: &BackendClient, generation: u64) -> Result<()> {
    if network.generation() != generation || network.inner.shutdown.is_cancelled() {
        Err(Error::Stale)
    } else {
        Ok(())
    }
}
