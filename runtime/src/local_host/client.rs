use super::*;

pub struct Client {
    events: Events,
    commands: Commands,
    pub initial_events: Vec<Value>,
    pub host: HostDescription,
}
pub struct Events(FrameReader<kodosi_os::ipc::ReadHalf>);
pub struct Commands(kodosi_os::ipc::WriteHalf);

impl Client {
    pub async fn connect(root: &Path) -> Result<Self> {
        let (stream, welcome) = connect(root, None).await?;
        let host = welcome
            .host
            .ok_or_else(|| Error::Invalid("the running host did not identify itself".into()))?;
        let (reader, writer) = stream.into_split();
        Ok(Self {
            events: Events(FrameReader::new(reader)),
            commands: Commands(writer),
            initial_events: welcome.events,
            host,
        })
    }
    pub async fn command(&mut self, command: Value) -> Result<()> {
        self.commands.command(command).await
    }
    pub async fn next(&mut self) -> Result<Value> {
        self.events.next().await
    }
    pub fn split(self) -> (Events, Commands) {
        (self.events, self.commands)
    }
}
impl Events {
    pub async fn next(&mut self) -> Result<Value> {
        read_json(&mut self.0).await
    }
}
impl Commands {
    pub async fn command(&mut self, command: Value) -> Result<()> {
        drop(serde_json::from_value::<CommandEnvelope>(command.clone())?);
        let bytes = serde_json::to_vec(&ClientRequest::Command { command })?;
        if bytes.len() > crate::protocol::MAX_COMMAND_BYTES {
            return Err(Error::Invalid("command exceeds 2 MiB".into()));
        }
        write_frame(&mut self.0, &bytes).await
    }
}

pub async fn host_status(root: &Path) -> Result<HostStatus> {
    let (_, welcome) = handshake(root, None).await?;
    let host = welcome.host.ok_or_else(|| {
        Error::Other(
            welcome
                .error
                .unwrap_or_else(|| "the running host did not identify itself".into()),
        )
    })?;
    Ok(HostStatus {
        host,
        local_sessions: welcome.terminals,
    })
}

pub async fn stop_other_host(root: &Path, force: bool) -> Result<()> {
    let (mut stream, welcome) = handshake(root, None).await?;
    if welcome.version == VERSION
        && let Some(error) = welcome.error
    {
        return Err(Error::Other(error));
    }
    write_json(&mut stream, &ClientRequest::Stop { force }).await?;
    let mut replies = FrameReader::new(&mut stream);
    loop {
        let reply: Value = read_json(&mut replies).await?;
        match reply.get("kind").and_then(Value::as_str) {
            Some("stopping") => break,
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
    drop(stream);
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
    pub incarnation_id: Uuid,
}
impl TerminalClient {
    pub async fn connect(root: &Path, session: Uuid) -> Result<Self> {
        let (stream, welcome) = connect(root, Some(session)).await?;
        let terminal = welcome
            .terminal
            .ok_or_else(|| Error::Invalid("the host did not name the terminal".into()))?;
        let (reader, writer) = stream.into_split();
        let mut reader = FrameReader::new(reader);
        reader.maximum = TERMINAL_FRAME_MAX;
        Ok(Self {
            reader,
            writer,
            incarnation_id: terminal.incarnation_id,
        })
    }
}

async fn connect(
    root: &Path,
    session_id: Option<Uuid>,
) -> Result<(kodosi_os::ipc::Stream, Welcome)> {
    let (stream, welcome) = handshake(root, session_id).await?;
    if welcome.version < VERSION {
        let host = welcome.host.ok_or_else(|| {
            Error::Invalid(
                "An earlier version of Kodosi still runs. Quit it, or end its `internal-host` process, and try again."
                    .into(),
            )
        })?;
        return Err(Error::HostOutdated {
            kind: host.kind,
            pid: host.pid,
            terminals: welcome.terminals,
        });
    }
    if welcome.version > VERSION {
        return Err(Error::Invalid(
            "The running Kodosi host is newer than this program. Use the updated Kodosi.".into(),
        ));
    }
    if let Some(refusal) = welcome.refusal {
        return Err(refusal.error());
    }
    if let Some(error) = welcome.error {
        return Err(Error::Other(error));
    }
    Ok((stream, welcome))
}

async fn handshake(
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
                    io::Error::new(io::ErrorKind::NotFound, "Kodosi is not running.").into()
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
    Ok((stream, welcome))
}

pub async fn connect_or_start(root: &Path) -> Result<Client> {
    let mut client = connect_or_spawn(root).await?;
    let deadline = tokio::time::Instant::now() + ACCOUNT_TIMEOUT;
    while finalizing(&client.initial_events) {
        let Ok(frame) = tokio::time::timeout_at(deadline, client.next()).await else {
            break;
        };
        if let Some(event) = frame?.get("event") {
            client.initial_events.push(event.clone());
        }
        if account_changed(&client.initial_events) {
            client = Client::connect(root).await?;
        }
    }
    Ok(client)
}

fn account_changed(events: &[Value]) -> bool {
    events.first().map(|event| &event["accountEpoch"])
        != events.last().map(|event| &event["accountEpoch"])
}

fn finalizing(events: &[Value]) -> bool {
    events
        .iter()
        .rev()
        .find_map(|event| match event["type"].as_str() {
            Some("auth.finalizing") => Some(true),
            Some("auth.ready" | "auth.required" | "auth.restoring") => Some(false),
            _ => None,
        })
        == Some(true)
}

async fn connect_or_spawn(root: &Path) -> Result<Client> {
    match Client::connect(root).await {
        Ok(client) => return Ok(client),
        Err(Error::Io(error))
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
            ) => {}
        Err(error) => return Err(error),
    }
    let mut command = std::process::Command::new(host_program()?);
    command
        .arg("internal-host")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if let Some(session) = kodosi_os::process::session_environment() {
        command
            .env_clear()
            .envs(host_environment(session, std::env::vars_os()));
    }
    kodosi_os::process::detach(&mut command);
    let mut child = command.spawn()?;
    let mut exited = None;
    for _ in 0..500 {
        tokio::time::sleep(Duration::from_millis(10)).await;
        match Client::connect(root).await {
            Ok(client) => {
                collect_exit(child);
                return Ok(client);
            }
            Err(Error::Io(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                ) => {}
            Err(error) => return Err(error),
        }
        if exited.is_none() {
            exited = child.try_wait()?;
        }
    }
    if let Some(status) = exited {
        return Err(Error::Other(format!(
            "The Kodosi host stopped while it started ({status})."
        )));
    }
    drop(child.kill());
    drop(child.wait());
    Err(Error::Other(
        "The Kodosi host did not start in time.".into(),
    ))
}

fn collect_exit(mut child: std::process::Child) {
    drop(
        std::thread::Builder::new()
            .name("kodosi-host-exit".into())
            .spawn(move || drop(child.wait())),
    );
}

fn host_program() -> Result<PathBuf> {
    let current = std::env::current_exe()?;
    if cfg!(target_os = "macos") {
        return Ok(current);
    }
    let program = current.with_file_name(if cfg!(windows) {
        "kodosi.exe"
    } else {
        "kodosi"
    });
    if program.is_file() {
        Ok(program)
    } else {
        Err(Error::Invalid(format!(
            "The kodosi program is missing from {}. Install Kodosi again.",
            current.parent().unwrap_or(&current).display()
        )))
    }
}

pub(super) fn host_environment(
    session: Vec<(std::ffi::OsString, std::ffi::OsString)>,
    own: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    let setting = |name: &std::ffi::OsString| {
        name.as_encoded_bytes()
            .get(..6)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"KODOSI"))
    };
    session
        .into_iter()
        .filter(|(name, _)| !setting(name))
        .chain(own.into_iter().filter(|(name, _)| setting(name)))
        .collect()
}
