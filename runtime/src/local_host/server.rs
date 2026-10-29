use super::*;

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

pub struct LocalServer {
    pub(super) cancel: CancellationToken,
    pub(super) task: Option<tokio::task::JoinHandle<()>>,
}
impl LocalServer {
    pub async fn shutdown(mut self) {
        self.cancel.cancel();
        if let Some(task) = self.task.take() {
            drop(task.await);
        }
    }
}
impl Drop for LocalServer {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

pub(super) fn root_identity(root: &Path) -> String {
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
pub(super) fn endpoint(root: &Path) -> Result<kodosi_os::ipc::Endpoint> {
    role_endpoint(root, "host")
}

pub(super) fn role_endpoint(root: &Path, role: &str) -> Result<kodosi_os::ipc::Endpoint> {
    Ok(kodosi_os::ipc::Endpoint::new(&namespace(root), role)?)
}

pub(super) fn lock_root(root: &Path) -> Result<File> {
    let file = kodosi_os::fs::open_private(&root.join("host.lock")).map_err(local_error)?;
    file.try_lock().map_err(|error| {
        Error::HostBusy(format!(
            "Kodosi is already running for this data root: {error}"
        ))
    })?;
    Ok(file)
}

pub(super) fn local_error(error: io::Error) -> Error {
    match error.kind() {
        io::ErrorKind::InvalidInput | io::ErrorKind::PermissionDenied => {
            Error::Invalid(error.to_string())
        }
        _ => error.into(),
    }
}

pub async fn serve(
    runtime: RuntimeHandle,
    root: PathBuf,
    host: HostDescription,
    clients: watch::Sender<usize>,
) -> Result<LocalServer> {
    let root = kodosi_os::fs::canonicalize(&root)?;
    let host_lock = lock_root(&root)?;
    let mut listener = kodosi_os::ipc::Listener::bind(&endpoint(&root)?)
        .await
        .map_err(|error| {
            if error.kind() == io::ErrorKind::AddrInUse {
                Error::HostBusy("another Kodosi host owns this endpoint".into())
            } else {
                local_error(error)
            }
        })?;
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
                    Ok(stream) => {
                        let Ok(permit) = std::sync::Arc::clone(&slots).try_acquire_owned() else { continue; };
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
    });
    Ok(LocalServer {
        cancel,
        task: Some(task),
    })
}

async fn serve_connection(
    mut stream: kodosi_os::ipc::Stream,
    runtime: RuntimeHandle,
    root: String,
    cancel: CancellationToken,
    host: HostDescription,
) -> Result<()> {
    let hello: Hello =
        tokio::time::timeout(HELLO_TIMEOUT, read_json(&mut FrameReader::new(&mut stream)))
            .await
            .map_err(|_| Error::Invalid("local handshake timed out".into()))??;
    if hello.root != root {
        let refusal = Welcome {
            version: VERSION,
            error: Some("local runtime data-root mismatch".into()),
            ..Welcome::default()
        };
        return write_json(&mut stream, &refusal).await;
    }
    if hello.version != VERSION {
        let refusal = Welcome {
            version: VERSION,
            error: Some("local runtime protocol mismatch".into()),
            host: Some(host),
            terminals: local_sessions(&runtime.snapshot().await?),
            ..Welcome::default()
        };
        write_json(&mut stream, &refusal).await?;
        let request = tokio::time::timeout(
            HELLO_TIMEOUT,
            read_json::<_, ClientRequest>(&mut FrameReader::new(&mut stream)),
        )
        .await;
        if let Ok(Ok(ClientRequest::Stop { force })) = request {
            stop(&mut stream, &runtime, host, force).await?;
        }
        return Ok(());
    }
    let Some(session) = hello.session_id else {
        return serve_commands(stream, runtime.command_client(), cancel, host).await;
    };
    let subscription = tokio::select! {
        biased;
        () = cancel.cancelled() => return Ok(()),
        result = runtime.subscribe_terminal(session) => result,
    };
    let subscription = match subscription {
        Ok(value) => value,
        Err(error) => {
            let refusal = Welcome {
                version: VERSION,
                error: Some(error.to_string()),
                refusal: Refusal::of(&error),
                ..Welcome::default()
            };
            return write_json(&mut stream, &refusal).await;
        }
    };
    let connection = subscription.connection_id;
    let result = async {
        let welcome = Welcome {
            version: VERSION,
            terminal: Some(TerminalWelcome {
                incarnation_id: subscription.incarnation_id,
            }),
            ..Welcome::default()
        };
        write_json(&mut stream, &welcome).await?;
        serve_terminal(stream, runtime.clone(), session, subscription, cancel).await
    }
    .await;
    runtime.unsubscribe_terminal(session, connection).await;
    result
}

async fn stop<W: AsyncWrite + Unpin + Send>(
    writer: &mut W,
    runtime: &RuntimeHandle,
    host: HostDescription,
    force: bool,
) -> Result<bool> {
    let open = local_sessions(&runtime.snapshot().await?);
    match stop_permission(host.kind, force, open) {
        Ok(()) => {
            write_json(writer, &json!({"kind":"stopping"})).await?;
            let stopping = runtime.clone();
            tokio::spawn(async move { stopping.shutdown().await });
            Ok(true)
        }
        Err(message) => {
            write_json(
                writer,
                &json!({"kind":"rejected","operation":"host.stop","message":message}),
            )
            .await?;
            Ok(false)
        }
    }
}

async fn serve_commands(
    mut stream: kodosi_os::ipc::Stream,
    runtime: RuntimeHandle,
    cancel: CancellationToken,
    host: HostDescription,
) -> Result<()> {
    let (snapshot, mut events) = runtime.observe().await?;
    let initial = snapshot
        .iter()
        .map(serde_json::to_value)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let welcome = Welcome {
        version: VERSION,
        terminals: local_sessions(&snapshot),
        events: initial,
        host: Some(host),
        ..Welcome::default()
    };
    write_json(&mut stream, &welcome).await?;
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
                        let request_id=command.command.request_id().to_owned();
                        let operation=command.command.operation();
                        if let Err(error)=runtime.try_send(command) {
                            write_json(&mut writer,&json!({"kind":"rejected","requestId":request_id,"operation":operation,"message":error.to_string()})).await?;
                        }
                    }
                    ClientRequest::Stop{force}=>{
                        if stop(&mut writer,&runtime,host,force).await? {
                            return Ok(());
                        }
                    }
                }
            }
            event=events.recv()=>match event {
                Ok(event) if event.reaches(runtime.consumer())=>write_event(&mut writer,&event).await?,
                Ok(_)=>{}
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

type Pending<T> = futures_util::stream::FuturesOrdered<futures_util::future::BoxFuture<'static, T>>;

async fn serve_terminal(
    stream: kodosi_os::ipc::Stream,
    runtime: RuntimeHandle,
    session: Uuid,
    mut subscription: terminal::Subscription,
    cancel: CancellationToken,
) -> Result<()> {
    const PENDING: usize = 32;
    let (reader, mut writer) = stream.into_split();
    let mut reader = FrameReader::new(reader);
    write_frame(
        &mut writer,
        &checkpoint_frame(&subscription.checkpoint, subscription.next_sequence),
    )
    .await?;
    let incarnation = subscription.incarnation_id;
    let connection = subscription.connection_id;
    let mut admissions = Pending::<Result<()>>::new();
    let mut controls = Pending::<Result<Option<Value>>>::new();
    let mut refreshing: Option<
        futures_util::future::BoxFuture<'static, Result<crate::network::CheckpointCut>>,
    > = None;
    let mut refresh_wanted = false;
    let mut retry: Option<tokio::time::Instant> = None;
    loop {
        if refresh_wanted && refreshing.is_none() && retry.is_none() && subscription.can_refresh() {
            refresh_wanted = false;
            let handle = runtime.clone();
            refreshing = Some(Box::pin(async move {
                handle.terminal_checkpoint(session, connection).await
            }));
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
            answer=controls.next(),if !controls.is_empty()=>{
                if let Some(answer)=answer.transpose()?.flatten() {
                    write_frame(&mut writer,&control_frame(&answer)?).await?;
                }
            }
            cut=async { match refreshing.as_mut() { Some(refresh)=>refresh.await, None=>std::future::pending().await } },if refreshing.is_some()=>{
                refreshing=None;
                match cut {
                    Ok(cut)=>subscription.refresh(cut),
                    Err(error)=>{
                        tracing::warn!(%session,%error,"terminal view refresh failed; it will be tried again");
                        refresh_wanted=true;
                        retry=Some(tokio::time::Instant::now()+REFRESH_RETRY);
                    }
                }
            }
            ()=async { tokio::time::sleep_until(retry.unwrap_or_else(tokio::time::Instant::now)).await },if retry.is_some()=>retry=None,
            frame=read_frame(&mut reader),if admissions.len()<PENDING && controls.len()<PENDING=>{
                let bytes=frame?;
                match bytes.first() {
                    Some(&INPUT)=>{
                        let handle=runtime.clone();
                        let input=Bytes::copy_from_slice(&bytes[1..]);
                        admissions.push_back(Box::pin(async move {handle.write_input(session,incarnation,connection,input).await}));
                    }
                    Some(&CONTROL)=>{
                        let command:CommandEnvelope=serde_json::from_slice(&bytes[1..])?;
                        let handle=runtime.clone();
                        controls.push_back(Box::pin(async move {handle.terminal_control(command,connection).await}));
                    }
                    Some(&REFRESH) if bytes.len()==1=>refresh_wanted=true,
                    Some(&RESIZE) if bytes.len()==5=>{
                        let cols=u16::from_be_bytes([bytes[1],bytes[2]]);let rows=u16::from_be_bytes([bytes[3],bytes[4]]);
                        if let Err(error)=runtime.resize(session,incarnation,connection,cols,rows).await {
                            tracing::warn!(%session,%error,"terminal view resize was not applied");
                        }
                    }
                    _=>return Err(Error::Invalid("unsupported terminal input frame".into())),
                }
            }
            frame=subscription.recv()=>match frame? {
                Some(terminal::Frame::Data(frame))=>{
                    let mut bytes=Vec::with_capacity(9+frame.bytes.len());bytes.push(DATA);bytes.extend(frame.sequence.to_be_bytes());bytes.extend(&frame.bytes);
                    write_frame(&mut writer,&bytes).await?;
                }
                Some(terminal::Frame::Control(terminal::ControlFrame::Resize{rows,cols,cell,at_sequence}))=>{
                    write_frame(&mut writer,&resize_frame(rows,cols,cell,at_sequence)?).await?;
                }
                Some(terminal::Frame::Control(terminal::ControlFrame::Snapshot{checkpoint,next_sequence}))=>{
                    retry=None;
                    refresh_wanted=false;
                    write_frame(&mut writer,&checkpoint_frame(&checkpoint,next_sequence)).await?;
                }
                Some(terminal::Frame::Control(terminal::ControlFrame::Closed{reason,final_sequence}))=>{
                    write_frame(&mut writer,&control_frame(&json!({"type":"closed","reason":reason,"finalSequence":final_sequence}))?).await?;
                    return Ok(());
                }
                None=>return Ok(()),
            }
        }
    }
}
