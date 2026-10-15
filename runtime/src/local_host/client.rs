use super::*;

pub struct Client {
    reader: FrameReader<kodosi_os::ipc::ReadHalf>,
    writer: kodosi_os::ipc::WriteHalf,
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
    let events = client
        .initial_events
        .into_iter()
        .map(serde_json::from_value)
        .collect::<std::result::Result<Vec<Event>, _>>()?;
    Ok(HostStatus {
        host,
        local_sessions: local_sessions(&events),
    })
}

pub async fn stop_other_host(root: &Path, force: bool) -> Result<()> {
    let mut client = Client::connect(root).await?;
    client.stop_host(force).await?;
    drop(client);
    let root = kodosi_os::fs::canonicalize(root)?;
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
    pub reader: FrameReader<kodosi_os::ipc::ReadHalf>,
    pub writer: kodosi_os::ipc::WriteHalf,
}
impl TerminalClient {
    pub async fn connect(root: &Path, session: Uuid) -> Result<Self> {
        let (stream, _) = connect(root, Some(session)).await?;
        let (reader, writer) = stream.into_split();
        Ok(Self {
            reader: FrameReader::new(reader),
            writer,
        })
    }
}
async fn connect(
    root: &Path,
    session_id: Option<Uuid>,
) -> Result<(kodosi_os::ipc::Stream, Welcome)> {
    let root = kodosi_os::fs::canonicalize(root)?;
    let mut stream =
        tokio::time::timeout(HELLO_TIMEOUT, kodosi_os::ipc::connect(&endpoint(&root)?))
            .await
            .map_err(|_| Error::Other("local connection timed out".into()))?
            .map_err(|error| {
                if error.kind() == io::ErrorKind::NotFound {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        "Kodosi is not running. Start the app or `kodosi host`.",
                    )
                    .into()
                } else {
                    local_error(error)
                }
            })?;
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
    if welcome.version < VERSION {
        return Err(Error::Invalid(format!(
            "Kodosi was updated to {}. Restart Kodosi to use it. This ends its terminals.",
            env!("KODOSI_VERSION")
        )));
    }
    if welcome.version > VERSION {
        return Err(Error::Invalid(
            "This kodosi command is older than the running Kodosi. Use the kodosi command of the installed app.".into(),
        ));
    }
    if let Some(error) = &welcome.error {
        return Err(Error::Other(error.clone()));
    }
    Ok((stream, welcome))
}
