use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
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
use zeroize::Zeroizing;

use super::{
    BackendClient, CheckpointCut, Error, HostRequest, LocalPublication, PublicationOutput,
    PublishedFrame, RemoteConnection, RemoteUpdate, Result, TerminalControl, crypto,
    http::Credentials,
    invalid,
    wire::{self, ControlIdentity, SessionDto},
};
use crate::identity;

mod host;
mod participant;

pub(crate) type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub(crate) struct Publication {
    pub(super) drained: CancellationToken,
    pub info: RwLock<LocalPublication>,
    pub dto: RwLock<SessionDto>,
    pub cancel: CancellationToken,
    pub refresh: Notify,
    pub changing: AtomicBool,
    pub authorization: Mutex<CancellationToken>,
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
            authorization: Mutex::new(CancellationToken::new()),
            drained: CancellationToken::new(),
            pending_shares: Mutex::new(None),
            requests,
            output: Mutex::new(Some(output)),
        }
    }
    pub(crate) async fn expire_sharing(&self) {
        let mut info = self.info.write().await;
        info.shared_with.clear();
        info.mission_id = None;
        drop(info);
        *self.pending_shares.lock().await = None;
    }

    pub(crate) async fn invalidate(&self) {
        self.authorization.lock().await.cancel();
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
                || network.generation() != generation
                || network.inner.shutdown.is_cancelled()
            {
                break;
            }
            if publication.changing.load(Ordering::Acquire) {
                tokio::select! { ()=publication.cancel.cancelled()=>break, ()=publication.refresh.notified()=>{}, ()=tokio::time::sleep(Duration::from_millis(200))=>{} }
                continue;
            }
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
            tokio::select! {
                ()=publication.cancel.cancelled()=>break,
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
    purpose: &str,
    session: Option<&SessionDto>,
    checkpoint_challenge: Option<&[u8; 32]>,
) -> Result<(Socket, Value)> {
    network.check_credentials(credentials)?;
    network.inner.http.compatible().await?;
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
        .max_message_size(Some(wire::FRAME_LIMIT * 2))
        .max_frame_size(Some(wire::FRAME_LIMIT * 2));
    let (mut socket, _) = tokio::time::timeout(
        Duration::from_secs(10),
        tokio_tungstenite::connect_async_with_config(request, Some(settings), false),
    )
    .await
    .map_err(|_| invalid("Terminal connection timed out."))?
    .map_err(|error| {
        tracing::warn!(%error, "terminal connection failed");
        invalid("Kodosi could not reach the terminal connection service.")
    })?;
    let mut hello = json!({"type":"hello","protocolVersion":crate::protocol::TERMINAL_CONNECTION_VERSION,"deviceId":credentials.keys.device_id,"incarnationId":session.map(|s|s.incarnation_id)});
    if let Some(challenge) = checkpoint_challenge {
        hello["checkpointChallenge"] = json!(BASE64.encode(challenge));
    }
    send_json(&mut socket, hello).await?;
    let challenge = read_json(&mut socket).await?;
    if wire::text(&challenge, "type")? != "challenge" {
        return Err(invalid(
            "The connection service did not issue a device challenge.",
        ));
    }
    let connection = wire::text(&challenge, "connectionId")?;
    let bytes = wire::decode_b64(&challenge, "challenge", 32)?;
    let session_id = session.map(|s| s.id.to_string());
    let preimage = crypto::device_connection_proof_preimage(
        &credentials.user_id,
        &credentials.keys.device_id,
        connection,
        purpose,
        session_id.as_deref(),
        session.map(|s| &s.incarnation_id),
        &bytes,
    )?;
    let signature = crypto::sign_control_message(credentials.keys.signing_pkcs8(), &preimage)?;
    send_json(
        &mut socket,
        json!({"type":"authenticate","signature":BASE64.encode(signature)}),
    )
    .await?;
    let ready = read_json(&mut socket).await?;
    if wire::text(&ready, "type")? != "ready" || wire::text(&ready, "connectionId")? != connection {
        return Err(invalid("The connection service did not admit this device."));
    }
    if let Some(session) = session
        && wire::id(&ready, "incarnationId")? != session.incarnation_id
    {
        return Err(Error::Stale);
    }
    Ok((socket, ready))
}

pub(crate) async fn send_json(socket: &mut Socket, value: Value) -> Result<()> {
    let text = serde_json::to_string(&value)?;
    if text.len() > wire::FRAME_LIMIT * 2 {
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
pub(crate) async fn send_binary(socket: &mut Socket, bytes: Vec<u8>) -> Result<()> {
    tokio::time::timeout(
        Duration::from_secs(10),
        socket.send(Message::Binary(bytes.into())),
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

pub(crate) async fn bootstrap(
    publication: &Publication,
    request_id: Uuid,
) -> Result<CheckpointCut> {
    let (reply, response) = oneshot::channel();
    publication
        .requests
        .try_send(HostRequest::Bootstrap { request_id, reply })
        .map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => Error::Busy,
            mpsc::error::TrySendError::Closed(_) => Error::Closed,
        })?;
    tokio::time::timeout(Duration::from_secs(5), response)
        .await
        .map_err(|_| Error::Closed)?
        .map_err(|_| Error::Closed)?
        .map_err(invalid)
}

pub(crate) fn check_generation(network: &BackendClient, generation: u64) -> Result<()> {
    if network.generation() != generation || network.inner.shutdown.is_cancelled() {
        Err(Error::Stale)
    } else {
        Ok(())
    }
}
