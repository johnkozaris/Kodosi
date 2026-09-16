use futures_util::StreamExt as _;
use std::{
    fs::{File, OpenOptions},
    io,
    os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::UnixStream,
    sync::{Semaphore, broadcast, watch},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    CommandEnvelope, Error, Event, EventBody, HostKind, Result, RuntimeHandle,
    protocol::SessionKind, terminal,
};

const VERSION: u32 = 17;
const STOP_WAIT: Duration = Duration::from_secs(5);
const MAX_FRAME: usize = 8 * 1024 * 1024 + 64 * 1024;
const MAX_CONNECTIONS: usize = 64;
const HELLO_TIMEOUT: Duration = Duration::from_secs(3);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Hello {
    version: u32,
    root: String,
    session_id: Option<Uuid>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Welcome {
    version: u32,
    error: Option<String>,
    events: Vec<Value>,
    incarnation_id: Option<Uuid>,
    host: Option<HostDescription>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum ClientRequest {
    Command { command: Value },
    Snapshot,
    Stop { force: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostDescription {
    pub pid: u32,
    pub kind: HostKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostStatus {
    pub host: HostDescription,
    pub local_sessions: usize,
}

pub(crate) fn local_sessions(events: &[Event]) -> usize {
    events
        .iter()
        .rev()
        .find_map(|event| match &event.event {
            EventBody::SessionsSnapshot { sessions } => Some(
                sessions
                    .iter()
                    .filter(|session| session.kind == SessionKind::Local)
                    .count(),
            ),
            _ => None,
        })
        .unwrap_or(0)
}

fn local_sessions_in(events: &[Value]) -> usize {
    events
        .iter()
        .rev()
        .find(|event| event.get("type").and_then(Value::as_str) == Some("sessions.snapshot"))
        .and_then(|event| event.get("sessions").and_then(Value::as_array))
        .map_or(0, |sessions| {
            sessions
                .iter()
                .filter(|session| session.get("kind").and_then(Value::as_str) == Some("local"))
                .count()
        })
}

fn stop_permission(
    kind: HostKind,
    force: bool,
    local_sessions: usize,
) -> std::result::Result<(), String> {
    if kind == HostKind::App {
        return Err("Kodosi is running as the app. Quit the app to stop this host.".into());
    }
    if force {
        return Ok(());
    }
    if kind == HostKind::Foreground {
        return Err("This host was started with `kodosi host`. Stop it from its terminal.".into());
    }
    if local_sessions > 0 {
        return Err(format!(
            "This host still runs {local_sessions} terminal(s). Close them first."
        ));
    }
    Ok(())
}

struct ClientSlot(std::sync::Arc<watch::Sender<usize>>);
impl ClientSlot {
    fn enter(clients: &std::sync::Arc<watch::Sender<usize>>) -> Self {
        clients.send_modify(|count| *count += 1);
        Self(std::sync::Arc::clone(clients))
    }
}
impl Drop for ClientSlot {
    fn drop(&mut self) {
        self.0.send_modify(|count| *count = count.saturating_sub(1));
    }
}

pub struct HeadlessServer {
    cancel: CancellationToken,
    task: Option<tokio::task::JoinHandle<()>>,
}
impl HeadlessServer {
    pub async fn shutdown(mut self) {
        self.cancel.cancel();
        if let Some(task) = self.task.take() {
            drop(task.await);
        }
    }
}
impl Drop for HeadlessServer {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

fn root_identity(root: &Path) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let hash = aws_lc_rs::digest::digest(
        &aws_lc_rs::digest::SHA256,
        root.as_os_str().as_encoded_bytes(),
    );
    let mut value = String::with_capacity(hash.as_ref().len() * 2);
    for byte in hash.as_ref() {
        value.push(char::from(HEX[usize::from(byte >> 4)]));
        value.push(char::from(HEX[usize::from(byte & 0xf)]));
    }
    value
}
fn socket_dir(root: &Path) -> Result<PathBuf> {
    let temporary = Path::new("/tmp").canonicalize()?;
    Ok(temporary.join(format!(
        "kodosi-{}-{}",
        rustix::process::geteuid().as_raw(),
        &root_identity(root)[..24]
    )))
}
fn existing_owned_file(path: &Path, socket: bool) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            if meta.uid() != rustix::process::geteuid().as_raw()
                || (meta.mode() & 0o077) != 0
                || meta.file_type().is_symlink()
                || if socket {
                    !meta.file_type().is_socket()
                } else {
                    !meta.is_file()
                }
            {
                return Err(Error::Invalid(format!(
                    "unsafe local endpoint: {}",
                    path.display()
                )));
            }
            Ok(true)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
fn lock_root(root: &Path) -> Result<File> {
    let path = root.join("host.lock");
    let _ = existing_owned_file(&path, false)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits().cast_signed())
        .open(path)?;
    if file.metadata()?.uid() != rustix::process::geteuid().as_raw() {
        return Err(Error::Invalid("host lock belongs to another user".into()));
    }
    file.try_lock().map_err(|error| {
        Error::HostBusy(format!(
            "Kodosi is already running for this data root: {error}"
        ))
    })?;
    Ok(file)
}

pub async fn serve(
    runtime: RuntimeHandle,
    root: PathBuf,
    host: HostDescription,
    clients: watch::Sender<usize>,
) -> Result<HeadlessServer> {
    let root = root.canonicalize()?;
    let host_lock = lock_root(&root)?;
    let dir = socket_dir(&root)?;
    crate::config::secure_directory(&dir)?;
    let path = dir.join("host.sock");
    if existing_owned_file(&path, true)? {
        match UnixStream::connect(&path).await {
            Ok(_) => {
                return Err(Error::HostBusy(
                    "another Kodosi host owns this endpoint".into(),
                ));
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
                ) =>
            {
                std::fs::remove_file(&path)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    let listener = tokio::net::UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    let identity = root_identity(&root);
    let clients = std::sync::Arc::new(clients);
    let task = tokio::spawn(async move {
        let _host_lock = host_lock;
        let slots = std::sync::Arc::new(Semaphore::new(MAX_CONNECTIONS));
        let mut connections = JoinSet::new();
        loop {
            tokio::select! {
                biased;
                () = stop.cancelled() => break,
                connection = listener.accept() => match connection {
                    Ok((stream,_)) => {
                        let Ok(permit) = std::sync::Arc::clone(&slots).try_acquire_owned() else { continue; };
                        if !stream.peer_cred().is_ok_and(|peer| peer.uid() == rustix::process::geteuid().as_raw()) { continue; }
                        let handle = runtime.clone(); let expected = identity.clone(); let cancel = stop.child_token();
                        let slot = ClientSlot::enter(&clients);
                        connections.spawn(async move {
                            let _permit = permit;
                            let _slot = slot;
                            tokio::select! {
                                biased;
                                () = cancel.cancelled() => {},
                                result = serve_connection(stream, handle, expected, cancel.clone(), host) => {
                                    if let Err(error) = result {
                                        tracing::debug!(%error, "local client disconnected");
                                    }
                                }
                            }
                        });
                    }
                    Err(error) => { tracing::warn!(%error,"local endpoint accept failed"); break; }
                },
                _ = connections.join_next(), if !connections.is_empty() => {}
            }
        }
        stop.cancel();
        while connections.join_next().await.is_some() {}
        drop(listener);
        if existing_owned_file(&path, true).unwrap_or(false) {
            drop(std::fs::remove_file(&path));
        }
    });
    Ok(HeadlessServer {
        cancel,
        task: Some(task),
    })
}

async fn serve_connection(
    mut stream: UnixStream,
    runtime: RuntimeHandle,
    root: String,
    cancel: CancellationToken,
    host: HostDescription,
) -> Result<()> {
    let hello: Hello =
        tokio::time::timeout(HELLO_TIMEOUT, read_json(&mut FrameReader::new(&mut stream)))
            .await
            .map_err(|_| Error::Invalid("local handshake timed out".into()))??;
    if hello.version != VERSION || hello.root != root {
        write_json(
            &mut stream,
            &Welcome {
                version: VERSION,
                error: Some("local runtime protocol or data-root mismatch".into()),
                events: vec![],
                incarnation_id: None,
                host: None,
            },
        )
        .await?;
        return Ok(());
    }
    if let Some(session) = hello.session_id {
        let subscription = tokio::select! {
            biased;
            ()=cancel.cancelled()=>return Ok(()),
            result=runtime.subscribe_terminal(session)=>result,
        };
        let subscription = match subscription {
            Ok(value) => value,
            Err(error) => {
                write_json(
                    &mut stream,
                    &Welcome {
                        version: VERSION,
                        error: Some(error.to_string()),
                        events: vec![],
                        incarnation_id: None,
                        host: None,
                    },
                )
                .await?;
                return Ok(());
            }
        };
        let connection = subscription.connection_id;
        let incarnation = subscription.incarnation_id;
        let result = async {
            write_json(
                &mut stream,
                &Welcome {
                    version: VERSION,
                    error: None,
                    events: vec![],
                    incarnation_id: Some(incarnation),
                    host: Some(host),
                },
            )
            .await?;
            serve_terminal(stream, runtime.clone(), session, subscription, cancel).await
        }
        .await;
        runtime.unsubscribe_terminal(session, connection).await;
        result
    } else {
        serve_commands(stream, runtime.command_client(), cancel, host).await
    }
}

async fn serve_commands(
    mut stream: UnixStream,
    runtime: RuntimeHandle,
    cancel: CancellationToken,
    host: HostDescription,
) -> Result<()> {
    let (snapshot, mut events) = runtime.observe().await?;
    let initial = snapshot
        .iter()
        .map(serde_json::to_value)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    write_json(
        &mut stream,
        &Welcome {
            version: VERSION,
            error: None,
            events: initial,
            incarnation_id: None,
            host: Some(host),
        },
    )
    .await?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = FrameReader::new(reader);
    reader.maximum = crate::protocol::MAX_COMMAND_BYTES;
    loop {
        tokio::select! {
            biased;
            ()=cancel.cancelled()=>return Ok(()),
            request=read_json::<_,ClientRequest>(&mut reader)=>{
                match request? {
                    ClientRequest::Command{command}=>{
                        let command:CommandEnvelope=serde_json::from_value(command)?;
                        let request_id=command.command.request_id().map(str::to_owned);
                        let operation=command.command.operation();
                        if let Err(error)=runtime.try_send(command) {
                            write_json(&mut writer,&json!({"kind":"rejected","requestId":request_id,"operation":operation,"message":error.to_string()})).await?;
                        }
                    }
                    ClientRequest::Snapshot=>{
                        let (snapshot, receiver) = runtime.observe().await?;
                        events = receiver;
                        for event in snapshot { write_event(&mut writer,&event).await?; }
                    }
                    ClientRequest::Stop{force}=>{
                        let open = local_sessions(&runtime.snapshot().await?);
                        match stop_permission(host.kind, force, open) {
                            Ok(())=>{
                                write_json(&mut writer,&json!({"kind":"stopping"})).await?;
                                let stopping = runtime.clone();
                                tokio::spawn(async move { stopping.shutdown().await; });
                                return Ok(());
                            }
                            Err(message)=>write_json(&mut writer,&json!({"kind":"rejected","operation":"host.stop","message":message})).await?,
                        }
                    }
                }
            }
            event=events.recv()=>match event {
                Ok(event)=>write_event(&mut writer,&event).await?,
                Err(broadcast::error::RecvError::Lagged(_))=>{
                    write_json(&mut writer,&json!({"kind":"resync"})).await?;
                    let (snapshot, receiver) = runtime.observe().await?;
                    events = receiver;
                    for event in snapshot { write_event(&mut writer,&event).await?; }
                }
                Err(broadcast::error::RecvError::Closed)=>return Ok(()),
            }
        }
    }
}
async fn write_event<W: AsyncWrite + Unpin + Send>(writer: &mut W, event: &Event) -> Result<()> {
    write_json(writer, &json!({"kind":"event","event":event})).await
}

const CHECKPOINT: u8 = 1;
const DATA: u8 = 2;
const CONTROL: u8 = 3;
const INPUT: u8 = 4;
const RESIZE: u8 = 5;
const INPUT_ACK: u8 = 6;

#[derive(Debug)]
pub enum TerminalFrame {
    InputAck {
        accepted: bool,
        message: Option<String>,
    },
    Checkpoint {
        bytes: Vec<u8>,
        rows: u16,
        cols: u16,
        next_sequence: u64,
    },
    Data {
        bytes: Bytes,
        sequence: u64,
    },
    Resize {
        rows: u16,
        cols: u16,
        at_sequence: u64,
    },
    Closed {
        reason: String,
        final_sequence: u64,
    },
}
fn checkpoint_frame(checkpoint: &terminal::Checkpoint, next: u64) -> Result<Vec<u8>> {
    if checkpoint.semantic_checkpoint.len() > 8 * 1024 * 1024 {
        return Err(Error::Invalid("checkpoint exceeds bound".into()));
    }
    let mut bytes = Vec::with_capacity(13 + checkpoint.semantic_checkpoint.len());
    bytes.push(CHECKPOINT);
    bytes.extend(checkpoint.rows().to_be_bytes());
    bytes.extend(checkpoint.cols().to_be_bytes());
    bytes.extend(next.to_be_bytes());
    bytes.extend(&checkpoint.semantic_checkpoint);
    Ok(bytes)
}
fn control_frame(value: &Value) -> Result<Vec<u8>> {
    let mut bytes = vec![CONTROL];
    bytes.extend(serde_json::to_vec(value)?);
    Ok(bytes)
}
async fn serve_terminal(
    stream: UnixStream,
    runtime: RuntimeHandle,
    session: Uuid,
    mut subscription: terminal::Subscription,
    cancel: CancellationToken,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = FrameReader::new(reader);
    write_frame(
        &mut writer,
        &checkpoint_frame(&subscription.checkpoint, subscription.next_sequence)?,
    )
    .await?;
    let mut next = subscription.next_sequence;
    let mut controls = std::collections::VecDeque::new();
    let mut closing: Option<(String, u64)> = None;
    let mut admissions = futures_util::stream::FuturesOrdered::<
        futures_util::future::BoxFuture<'static, Result<()>>,
    >::new();
    loop {
        while let Some(&(rows, cols, at_sequence)) = controls.front() {
            if at_sequence > next {
                break;
            }
            controls.pop_front();
            if at_sequence != next {
                return Err(Error::Invalid(
                    "terminal resize crossed output boundary".into(),
                ));
            }
            write_frame(
                &mut writer,
                &control_frame(
                    &json!({"type":"resize","rows":rows,"cols":cols,"atSequence":at_sequence}),
                )?,
            )
            .await?;
        }
        if let Some((reason, last)) = &closing {
            if *last == next {
                write_frame(
                    &mut writer,
                    &control_frame(&json!({"type":"closed","reason":reason,"finalSequence":last}))?,
                )
                .await?;
                return Ok(());
            }
            if *last < next {
                return Err(Error::Invalid("terminal close boundary regressed".into()));
            }
        }
        tokio::select! {
            biased;
            ()=cancel.cancelled()=>return Ok(()),
            result=admissions.next(),if !admissions.is_empty()=>{
                if let Some(result)=result {
                    let mut ack=vec![INPUT_ACK];
                    ack.extend(serde_json::to_vec(&json!({"accepted":result.is_ok(),"message":result.err().map(|error:Error|error.to_string())}))?);
                    write_frame(&mut writer,&ack).await?;
                }
            }
            frame=read_frame(&mut reader),if admissions.len()<32=>{
                let bytes=frame?;
                match bytes.first() {
                    Some(&INPUT)=>{
                        let handle=runtime.clone();
                        let incarnation=subscription.incarnation_id;
                        let connection=subscription.connection_id;
                        let input=Bytes::copy_from_slice(&bytes[1..]);
                        admissions.push_back(Box::pin(async move {handle.write_input(session,incarnation,connection,input).await}));
                    },
                    Some(&RESIZE) if bytes.len()==5=>{
                        let cols=u16::from_be_bytes([bytes[1],bytes[2]]);let rows=u16::from_be_bytes([bytes[3],bytes[4]]);
                        runtime.resize(session,subscription.incarnation_id,subscription.connection_id,cols,rows).await?;
                    }
                    _=>return Err(Error::Invalid("unsupported terminal input frame".into())),
                }
            }
            control=subscription.control.recv(),if closing.is_none()=>match control {
                Some(terminal::ControlFrame::Resize{rows,cols,at_sequence})=>{
                    if controls.len()>=32 || controls.back().is_some_and(|&(_,_,at)|at>at_sequence){return Err(Error::Invalid("too many terminal resize boundaries".into()));}
                    controls.push_back((rows,cols,at_sequence));
                }
                Some(terminal::ControlFrame::Closed{reason,final_sequence})=>closing=Some((reason,final_sequence)),
                None=>return Ok(()),
            },
            frame=subscription.data.recv()=>match frame {
                Some(frame) if frame.sequence<next=>{},
                Some(frame) if frame.sequence==next=>{
                    let mut bytes=Vec::with_capacity(9+frame.bytes.len());bytes.push(DATA);bytes.extend(frame.sequence.to_be_bytes());bytes.extend(&frame.bytes);
                    write_frame(&mut writer,&bytes).await?;next=next.checked_add(1).ok_or_else(||Error::Invalid("terminal sequence exhausted".into()))?;
                }
                Some(_)=>return Err(Error::Invalid("terminal output needs a fresh snapshot".into())),
                None=>return Ok(()),
            }
        }
    }
}

pub struct Client {
    reader: FrameReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
    pub initial_events: Vec<Value>,
    pub host: Option<HostDescription>,
}
impl Client {
    pub async fn connect(root: &Path) -> Result<Self> {
        let (stream, welcome) = connect(root, None).await?;
        let (reader, writer) = stream.into_split();
        Ok(Self {
            reader: FrameReader::new(reader),
            writer,
            initial_events: welcome.events,
            host: welcome.host,
        })
    }
    pub async fn command(&mut self, command: Value) -> Result<()> {
        drop(serde_json::from_value::<CommandEnvelope>(command.clone())?);
        let bytes = serde_json::to_vec(&ClientRequest::Command { command })?;
        if bytes.len() > crate::protocol::MAX_COMMAND_BYTES {
            return Err(Error::Invalid("command exceeds 2 MiB".into()));
        }
        write_frame(&mut self.writer, &bytes).await
    }
    pub async fn snapshot(&mut self) -> Result<()> {
        write_json(&mut self.writer, &ClientRequest::Snapshot).await
    }
    pub async fn next(&mut self) -> Result<Value> {
        read_json(&mut self.reader).await
    }
    pub async fn stop_host(&mut self, force: bool) -> Result<()> {
        write_json(&mut self.writer, &ClientRequest::Stop { force }).await?;
        loop {
            let reply = self.next().await?;
            match reply.get("kind").and_then(Value::as_str) {
                Some("stopping") => return Ok(()),
                Some("rejected")
                    if reply.get("operation").and_then(Value::as_str) == Some("host.stop") =>
                {
                    return Err(Error::HostBusy(
                        reply
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("The running host refused to stop.")
                            .to_owned(),
                    ));
                }
                _ => {}
            }
        }
    }
}

pub async fn host_status(root: &Path) -> Result<HostStatus> {
    let client = Client::connect(root).await?;
    let host = client
        .host
        .ok_or_else(|| Error::Invalid("the running host did not identify itself".into()))?;
    Ok(HostStatus {
        host,
        local_sessions: local_sessions_in(&client.initial_events),
    })
}

pub async fn stop_other_host(root: &Path, force: bool) -> Result<()> {
    let mut client = Client::connect(root).await?;
    client.stop_host(force).await?;
    drop(client);
    let root = root.canonicalize()?;
    let deadline = tokio::time::Instant::now() + STOP_WAIT;
    while lock_root(&root).is_err() {
        if tokio::time::Instant::now() >= deadline {
            return Err(Error::Other(
                "The running host did not release the data root in time.".into(),
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Ok(())
}

pub struct TerminalClient {
    pub reader: FrameReader<tokio::net::unix::OwnedReadHalf>,
    pub writer: tokio::net::unix::OwnedWriteHalf,
    pub incarnation_id: Uuid,
}
impl TerminalClient {
    pub async fn connect(root: &Path, session: Uuid) -> Result<Self> {
        let (stream, welcome) = connect(root, Some(session)).await?;
        let incarnation_id = welcome
            .incarnation_id
            .ok_or_else(|| Error::Invalid("terminal handshake omitted incarnation".into()))?;
        let (reader, writer) = stream.into_split();
        Ok(Self {
            reader: FrameReader::new(reader),
            writer,
            incarnation_id,
        })
    }
}
async fn connect(root: &Path, session_id: Option<Uuid>) -> Result<(UnixStream, Welcome)> {
    let root = root.canonicalize()?;
    let path = socket_dir(&root)?.join("host.sock");
    if !existing_owned_file(&path, true)? {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Kodosi is not running. Start the app or `kodosi host`.",
        )
        .into());
    }
    let mut stream = tokio::time::timeout(HELLO_TIMEOUT, UnixStream::connect(path))
        .await
        .map_err(|_| Error::Other("local connection timed out".into()))??;
    if stream.peer_cred()?.uid() != rustix::process::geteuid().as_raw() {
        return Err(Error::Invalid("local host belongs to another user".into()));
    }
    write_json(
        &mut stream,
        &Hello {
            version: VERSION,
            root: root_identity(&root),
            session_id,
        },
    )
    .await?;
    let welcome: Welcome =
        tokio::time::timeout(HELLO_TIMEOUT, read_json(&mut FrameReader::new(&mut stream)))
            .await
            .map_err(|_| Error::Other("local handshake timed out".into()))??;
    if welcome.version != VERSION {
        return Err(Error::Invalid(
            "local host protocol differs; restart Kodosi".into(),
        ));
    }
    if let Some(error) = &welcome.error {
        return Err(Error::Other(error.clone()));
    }
    Ok((stream, welcome))
}

pub async fn read_terminal<R: AsyncRead + Unpin>(
    reader: &mut FrameReader<R>,
) -> Result<TerminalFrame> {
    let bytes = read_frame(reader).await?;
    match bytes.first() {
        Some(&CHECKPOINT) if bytes.len() > 13 => Ok(TerminalFrame::Checkpoint {
            rows: u16::from_be_bytes([bytes[1], bytes[2]]),
            cols: u16::from_be_bytes([bytes[3], bytes[4]]),
            next_sequence: u64::from_be_bytes(
                bytes[5..13]
                    .try_into()
                    .map_err(|_| Error::Invalid("invalid checkpoint sequence".into()))?,
            ),
            bytes: bytes[13..].to_vec(),
        }),
        Some(&DATA) if bytes.len() >= 9 => Ok(TerminalFrame::Data {
            sequence: u64::from_be_bytes(
                bytes[1..9]
                    .try_into()
                    .map_err(|_| Error::Invalid("invalid terminal sequence".into()))?,
            ),
            bytes: Bytes::copy_from_slice(&bytes[9..]),
        }),
        Some(&INPUT_ACK) => {
            let value: Value = serde_json::from_slice(&bytes[1..])?;
            Ok(TerminalFrame::InputAck {
                accepted: value
                    .get("accepted")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| Error::Invalid("invalid input acknowledgement".into()))?,
                message: value
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })
        }
        Some(&CONTROL) => {
            let value: Value = serde_json::from_slice(&bytes[1..])?;
            match value.get("type").and_then(Value::as_str) {
                Some("resize") => Ok(TerminalFrame::Resize {
                    rows: number(&value, "rows")?,
                    cols: number(&value, "cols")?,
                    at_sequence: number(&value, "atSequence")?,
                }),
                Some("closed") => Ok(TerminalFrame::Closed {
                    reason: value
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("terminal closed")
                        .to_owned(),
                    final_sequence: number(&value, "finalSequence")?,
                }),
                _ => Err(Error::Invalid("unsupported terminal control".into())),
            }
        }
        _ => Err(Error::Invalid("malformed terminal frame".into())),
    }
}
fn number<T: TryFrom<u64>>(value: &Value, key: &str) -> Result<T> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|n| T::try_from(n).ok())
        .ok_or_else(|| Error::Invalid(format!("invalid {key}")))
}
pub async fn write_input<W: AsyncWrite + Unpin>(writer: &mut W, bytes: &[u8]) -> Result<()> {
    if bytes.len() > 1024 * 1024 {
        return Err(Error::Invalid("terminal input exceeds 1 MiB".into()));
    }
    let mut frame = vec![INPUT];
    frame.extend(bytes);
    write_frame(writer, &frame).await
}
pub async fn write_resize<W: AsyncWrite + Unpin>(
    writer: &mut W,
    cols: u16,
    rows: u16,
) -> Result<()> {
    if cols == 0 || rows == 0 {
        return Err(Error::Invalid("terminal size must be nonzero".into()));
    }
    let mut frame = vec![RESIZE];
    frame.extend(cols.to_be_bytes());
    frame.extend(rows.to_be_bytes());
    write_frame(writer, &frame).await
}
async fn read_json<R: AsyncRead + Unpin, T: serde::de::DeserializeOwned>(
    reader: &mut FrameReader<R>,
) -> Result<T> {
    serde_json::from_slice(&read_frame(reader).await?).map_err(Into::into)
}
async fn write_json<W: AsyncWrite + Unpin + Send, T: Serialize + Sync>(
    writer: &mut W,
    value: &T,
) -> Result<()> {
    write_frame(writer, &serde_json::to_vec(value)?).await
}
pub struct FrameReader<R> {
    input: R,
    prefix: [u8; 4],
    prefix_read: usize,
    body: Vec<u8>,
    body_read: usize,
    maximum: usize,
}
impl<R> FrameReader<R> {
    pub fn new(input: R) -> Self {
        Self {
            input,
            prefix: [0; 4],
            prefix_read: 0,
            body: Vec::new(),
            body_read: 0,
            maximum: MAX_FRAME,
        }
    }
}
async fn read_frame<R: AsyncRead + Unpin>(reader: &mut FrameReader<R>) -> Result<Vec<u8>> {
    while reader.prefix_read < 4 {
        let count = reader
            .input
            .read(&mut reader.prefix[reader.prefix_read..])
            .await?;
        if count == 0 {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
        }
        reader.prefix_read += count;
    }
    if reader.body.is_empty() {
        let size = u32::from_be_bytes(reader.prefix) as usize;
        if size == 0 || size > reader.maximum {
            return Err(Error::Invalid("local frame exceeds its bound".into()));
        }
        reader.body = vec![0; size];
    }
    while reader.body_read < reader.body.len() {
        let count = reader
            .input
            .read(&mut reader.body[reader.body_read..])
            .await?;
        if count == 0 {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
        }
        reader.body_read += count;
    }
    reader.prefix_read = 0;
    reader.body_read = 0;
    Ok(std::mem::take(&mut reader.body))
}
async fn write_frame<W: AsyncWrite + Unpin>(writer: &mut W, bytes: &[u8]) -> Result<()> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err(Error::Invalid("local frame exceeds its bound".into()));
    }
    let size = u32::try_from(bytes.len())
        .map_err(|_| Error::Invalid("local frame length overflow".into()))?;
    tokio::time::timeout(WRITE_TIMEOUT, async {
        writer.write_u32(size).await?;
        writer.write_all(bytes).await?;
        writer.flush().await
    })
    .await
    .map_err(|_| Error::Other("local client stopped receiving".into()))??;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn command_reader_rejects_large_prefix_before_allocating() {
        let (mut writer, stream) = tokio::io::duplex(32);
        let mut reader = FrameReader::new(stream);
        reader.maximum = crate::protocol::MAX_COMMAND_BYTES;
        writer
            .write_u32(u32::try_from(reader.maximum + 1).unwrap())
            .await
            .unwrap();
        assert!(read_frame(&mut reader).await.is_err());
        assert!(reader.body.is_empty());
    }

    #[tokio::test]
    async fn frames_roundtrip_and_oversized_prefix_is_rejected() {
        let (mut a, b) = tokio::io::duplex(512);
        let mut b = FrameReader::new(b);
        write_frame(&mut a, b"hello").await.unwrap();
        assert_eq!(read_frame(&mut b).await.unwrap(), b"hello");
        a.write_u32(u32::try_from(MAX_FRAME + 1).unwrap())
            .await
            .unwrap();
        assert!(read_frame(&mut b).await.is_err());
    }
    #[tokio::test]
    async fn interrupted_frame_read_keeps_consumed_prefix_and_body() {
        let (mut writer, input) = tokio::io::duplex(64);
        let mut reader = FrameReader::new(input);
        writer.write_all(&[0, 0]).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(5), read_frame(&mut reader))
                .await
                .is_err()
        );
        writer.write_all(&[0, 5, b'h', b'e']).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(5), read_frame(&mut reader))
                .await
                .is_err()
        );
        writer.write_all(b"llo").await.unwrap();
        assert_eq!(read_frame(&mut reader).await.unwrap(), b"hello");
    }
    #[test]
    fn endpoint_rejects_foreign_file_and_symlink_shapes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("host.sock");
        std::fs::write(&path, b"not a socket").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(existing_owned_file(&path, true).is_err());
        let link = directory.path().join("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(existing_owned_file(&link, false).is_err());
    }
    #[test]
    fn host_lock_prevents_competing_local_owners() {
        let directory = tempfile::tempdir().unwrap();
        let first = lock_root(directory.path()).unwrap();
        assert!(lock_root(directory.path()).is_err());
        drop(first);
        assert!(lock_root(directory.path()).is_ok());
    }
    #[test]
    fn root_identity_and_socket_namespace_are_stable_and_separate() {
        assert_eq!(
            root_identity(Path::new("/a")),
            root_identity(Path::new("/a"))
        );
        assert_ne!(
            root_identity(Path::new("/a")),
            root_identity(Path::new("/b"))
        );
    }
    #[test]
    fn local_protocol_requires_exact_version_and_root() {
        assert!(
            serde_json::from_str::<Hello>(r#"{"version":17,"root":"a","sessionId":null}"#).is_ok()
        );
        assert!(serde_json::from_str::<Hello>(r#"{"version":17}"#).is_err());
        assert!(
            serde_json::from_str::<Hello>(r#"{"version":17,"root":"a","legacy":true}"#).is_err()
        );
    }
    #[test]
    fn stop_permission_protects_the_app_and_open_terminals() {
        assert!(stop_permission(HostKind::App, true, 0).is_err());
        assert!(stop_permission(HostKind::Foreground, false, 0).is_err());
        assert!(stop_permission(HostKind::Foreground, true, 2).is_ok());
        assert!(stop_permission(HostKind::Background, false, 1).is_err());
        assert!(stop_permission(HostKind::Background, false, 0).is_ok());
        assert!(stop_permission(HostKind::Background, true, 1).is_ok());
    }
    #[test]
    fn local_session_counts_ignore_remote_entries() {
        let events = [
            json!({"type":"sessions.snapshot","sessions":[{"kind":"local"},{"kind":"remote"},{"kind":"local"}]}),
            json!({"type":"auth.required"}),
        ];
        assert_eq!(local_sessions_in(&events), 2);
        assert_eq!(local_sessions_in(&[]), 0);
    }
    async fn hosted(kind: HostKind) -> (tempfile::TempDir, PathBuf, RuntimeHandle) {
        let storage = tempfile::tempdir().unwrap();
        let mut config = crate::Config::isolated(&storage.path().canonicalize().unwrap()).unwrap();
        config.host = kind;
        let root = config.data_root.clone();
        let handle = crate::start(config).await.unwrap();
        (storage, root, handle)
    }
    #[tokio::test]
    async fn idle_background_hosts_identify_themselves_and_yield() {
        let (_storage, root, handle) = hosted(HostKind::Background).await;
        let status = host_status(&root).await.unwrap();
        assert_eq!(
            status.host,
            HostDescription {
                pid: std::process::id(),
                kind: HostKind::Background
            }
        );
        assert_eq!(status.local_sessions, 0);
        stop_other_host(&root, false).await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), handle.stopped())
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn foreground_hosts_only_stop_when_forced() {
        let (_storage, root, handle) = hosted(HostKind::Foreground).await;
        assert!(matches!(
            stop_other_host(&root, false).await,
            Err(Error::HostBusy(_))
        ));
        stop_other_host(&root, true).await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), handle.stopped())
            .await
            .unwrap();
    }
}
