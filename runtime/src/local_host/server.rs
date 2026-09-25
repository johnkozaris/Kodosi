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

pub struct HostServer {
    cancel: CancellationToken,
    task: Option<tokio::task::JoinHandle<()>>,
}
impl HostServer {
    pub async fn shutdown(mut self) {
        self.cancel.cancel();
        if let Some(task) = self.task.take() {
            drop(task.await);
        }
    }
}
impl Drop for HostServer {
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
pub(super) fn socket_dir(root: &Path) -> Result<PathBuf> {
    let temporary = Path::new("/tmp").canonicalize()?;
    Ok(temporary.join(format!(
        "kodosi-{}-{}",
        rustix::process::geteuid().as_raw(),
        &root_identity(root)[..24]
    )))
}
pub(super) fn existing_owned_file(path: &Path, socket: bool) -> Result<bool> {
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
pub(super) fn lock_root(root: &Path) -> Result<File> {
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
) -> Result<HostServer> {
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
    Ok(HostServer {
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
