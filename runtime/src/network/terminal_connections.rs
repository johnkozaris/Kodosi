use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use bytes::Bytes;
use futures_util::{SinkExt as _, StreamExt as _};
use reqwest::Method;
use serde_json::{Value, json};
use tokio::{
    net::TcpStream,
    sync::{Mutex, Notify, RwLock, broadcast, mpsc, oneshot},
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{Message, client::IntoClientRequest as _, protocol::WebSocketConfig},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    BackendClient, Error, HostRequest, LocalPublication, PublicationOutput, PublishedFrame,
    RemoteConnection, RemoteUpdate, Result, TerminalControl,
    http::Credentials,
    invalid,
    wire::{self, SessionDto},
};

mod channel;
mod host;
mod pacer;
mod participant;

pub(crate) type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub(crate) const STABLE_CONNECTION: Duration = Duration::from_mins(1);
pub(crate) const INPUT_NOT_SENT: &str = "Some of your last input was not sent.";
const MESSAGE_LIMIT: usize = 256 * 1024;
const TRUST_CHECK: Duration = Duration::from_mins(5);

pub(crate) struct Publication {
    pub(super) drained: CancellationToken,
    pub info: RwLock<LocalPublication>,
    pub dto: RwLock<SessionDto>,
    pub cancel: CancellationToken,
    pub refresh: Notify,
    pub changing: AtomicBool,
    pub(super) pending_shares: Mutex<Option<BTreeSet<String>>>,
    requests: mpsc::Sender<HostRequest>,
    output: Mutex<Option<PublicationOutput>>,
}
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

pub(crate) enum RemoteRequest {
    Control {
        control: TerminalControl,
        reply: oneshot::Sender<Result<Value>>,
    },
    Checkpoint,
}

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
                ()=tokio::time::sleep(Duration::from_secs(delay))=>{}
            }
            delay = (delay * 2).min(15);
        }
    });
}

pub(crate) async fn connect_remote(network: BackendClient, id: Uuid) -> Result<RemoteConnection> {
    participant::connect(network, id).await
}

pub(crate) async fn socket(
    network: &BackendClient,
    credentials: &Credentials,
    path: &str,
    session: Option<&SessionDto>,
) -> Result<(Socket, Value)> {
    network.check_credentials(credentials)?;
    network.inner.http.compatible().await?;
    let device_session = network.inner.http.device_session(credentials).await?;
    if let Some(admitted) = admitted(network, credentials, path, session, &device_session).await? {
        return Ok(admitted);
    }
    network
        .inner
        .http
        .forget_device_session(&device_session)
        .await;
    let device_session = network.inner.http.device_session(credentials).await?;
    admitted(network, credentials, path, session, &device_session)
        .await?
        .ok_or_else(|| invalid("The connection service did not admit this device."))
}

async fn admitted(
    network: &BackendClient,
    credentials: &Credentials,
    path: &str,
    session: Option<&SessionDto>,
    device_session: &str,
) -> Result<Option<(Socket, Value)>> {
    let url = network.inner.http.websocket_url(path)?;
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
        tokio_tungstenite::connect_async_with_config(request, Some(settings), true),
    )
    .await
    .map_err(|_| invalid("Terminal connection timed out."))?
    .map_err(|error| {
        tracing::warn!(%error, "terminal connection failed");
        invalid("Kodosi could not reach the terminal connection service.")
    })?;
    send_json(&mut socket, json!({"type":"hello","protocolVersion":crate::protocol::TERMINAL_CONNECTION_VERSION,"deviceId":credentials.keys.device_id,"deviceSession":device_session,"incarnationId":session.map(|s|s.incarnation_id)})).await?;
    let ready = read_json(&mut socket).await?;
    match wire::text(&ready, "type")? {
        "ready" => {}
        "refused" => return Ok(None),
        _ => return Err(invalid("The connection service did not admit this device.")),
    }
    if let Some(session) = session
        && wire::id(&ready, "incarnationId")? != session.incarnation_id
    {
        return Err(Error::Stale);
    }
    Ok(Some((socket, ready)))
}

pub(crate) async fn send_json(socket: &mut Socket, value: Value) -> Result<()> {
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
        invalid("The connection to the host dropped.")
    })
}
pub(crate) async fn read_json(socket: &mut Socket) -> Result<Value> {
    let message = tokio::time::timeout(Duration::from_secs(10), socket.next())
        .await
        .map_err(|_| Error::Closed)?
        .ok_or(Error::Closed)?
        .map_err(|error| {
            tracing::warn!(%error, "terminal connection read failed");
            invalid("The connection to the host dropped.")
        })?;
    match message {
        Message::Text(text) => serde_json::from_str(&text).map_err(Into::into),
        _ => Err(invalid("Expected a connection handshake message.")),
    }
}

pub(crate) async fn send_pong(socket: &mut Socket, payload: Bytes) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(5), socket.send(Message::Pong(payload)))
        .await
        .map_err(|_| Error::Closed)?
        .map_err(|error| invalid(error.to_string()))
}

pub(crate) fn check_generation(network: &BackendClient, generation: u64) -> Result<()> {
    if network.generation() != generation || network.inner.shutdown.is_cancelled() {
        Err(Error::Stale)
    } else {
        Ok(())
    }
}
