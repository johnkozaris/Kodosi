use super::{
    CancellationToken, Deserialize, Duration, Error, File, FrameReader, HELLO_TIMEOUT, JoinSet,
    LocalServer, Path, PathBuf, Result, Semaphore, Serialize, Value, io, json, local_error,
    read_json, role_endpoint, write_json,
};

/// The largest activation a launch may hand to the running app.
pub const MAX_ACTIVATION_BYTES: usize = 16 * 1024;
const ACTIVATION_VERSION: u32 = 1;
const MAX_PENDING_ACTIVATIONS: usize = 16;
const FORWARD_RETRY: Duration = Duration::from_millis(50);

/// The app instance's claim on a data folder. Dropping it lets the next
/// launch claim the folder.
pub struct AppClaim {
    root: PathBuf,
    _lock: File,
}

/// Claims the data folder at `root` for this app instance. `None` means a
/// running app already holds it.
pub fn claim_app(root: &Path) -> Result<Option<AppClaim>> {
    let root = root.canonicalize()?;
    let lock = kodosi_os::fs::open_private(&root.join("app.lock")).map_err(local_error)?;
    match lock.try_lock() {
        Ok(()) => Ok(Some(AppClaim { root, _lock: lock })),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(error)) => Err(error.into()),
    }
}

#[derive(Debug, Deserialize)]
struct ActivationRequest {
    version: u32,
    activation: Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct ActivationReply {
    version: u32,
    accepted: bool,
}

/// Serves activations from later launches until shut down, then releases
/// `claim`. `deliver` receives each activation as a JSON object; returning
/// `false` leaves the request unanswered so its launch tries again.
pub async fn serve_activations(
    claim: AppClaim,
    deliver: impl Fn(Vec<u8>) -> bool + Send + Sync + 'static,
) -> Result<LocalServer> {
    let mut listener = kodosi_os::ipc::Listener::bind(&role_endpoint(&claim.root, "app")?)
        .await
        .map_err(|error| {
            if error.kind() == io::ErrorKind::AddrInUse {
                Error::HostBusy("another process owns the Kodosi app endpoint".into())
            } else {
                local_error(error)
            }
        })?;
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    let deliver = std::sync::Arc::new(deliver);
    let task = tokio::spawn(async move {
        let slots = std::sync::Arc::new(Semaphore::new(MAX_PENDING_ACTIVATIONS));
        let mut requests = JoinSet::new();
        loop {
            tokio::select! {
                biased;
                () = stop.cancelled() => break,
                accepted = listener.accept() => match accepted {
                    Ok(stream) => {
                        let Ok(permit) = std::sync::Arc::clone(&slots).try_acquire_owned() else {
                            continue;
                        };
                        let deliver = std::sync::Arc::clone(&deliver);
                        requests.spawn(async move {
                            let _permit = permit;
                            if let Err(error) = receive(stream, &*deliver).await {
                                tracing::debug!(%error, "activation request failed");
                            }
                        });
                    }
                    Err(error) => {
                        tracing::warn!(%error, "app endpoint accept failed");
                        break;
                    }
                },
                _ = requests.join_next(), if !requests.is_empty() => {}
            }
        }
        requests.abort_all();
        while requests.join_next().await.is_some() {}
        drop(listener);
        drop(claim);
    });
    Ok(LocalServer {
        cancel,
        task: Some(task),
    })
}

async fn receive(
    stream: kodosi_os::ipc::Stream,
    deliver: &(impl Fn(Vec<u8>) -> bool + Sync),
) -> Result<()> {
    let mut reader = FrameReader::new(stream);
    reader.maximum = MAX_ACTIVATION_BYTES + 64;
    let request: ActivationRequest = tokio::time::timeout(HELLO_TIMEOUT, read_json(&mut reader))
        .await
        .map_err(|_| Error::Other("activation request timed out".into()))??;
    let accepted = request.version == ACTIVATION_VERSION && request.activation.is_object();
    if accepted && !deliver(serde_json::to_vec(&request.activation)?) {
        return Ok(());
    }
    write_json(
        &mut reader.input,
        &ActivationReply {
            version: ACTIVATION_VERSION,
            accepted,
        },
    )
    .await
}

/// What became of an activation handed to the running app.
pub enum Forwarded {
    /// The running app accepted it.
    Delivered,
    /// The running app exited first; this launch now owns the data folder.
    Claimed(AppClaim),
}

/// Hands `activation`, a JSON object, to the app that owns `root`. Waits up
/// to `within` for an app that is still starting, and claims the folder if
/// the owner exits meanwhile.
pub async fn forward_activation(
    root: &Path,
    activation: &Value,
    within: Duration,
) -> Result<Forwarded> {
    if !activation.is_object() || serde_json::to_vec(activation)?.len() > MAX_ACTIVATION_BYTES {
        return Err(Error::Invalid(
            "an activation must be a JSON object within its bound".into(),
        ));
    }
    let root = root.canonicalize()?;
    let endpoint = role_endpoint(&root, "app")?;
    let deadline = tokio::time::Instant::now() + within;
    loop {
        if let Some(claim) = claim_app(&root)? {
            return Ok(Forwarded::Claimed(claim));
        }
        match tokio::time::timeout(HELLO_TIMEOUT, send(&endpoint, activation)).await {
            Ok(Ok(true)) => return Ok(Forwarded::Delivered),
            Ok(Ok(false)) => {
                return Err(Error::Invalid(
                    "Another version of Kodosi is running. Quit it, then open Kodosi again.".into(),
                ));
            }
            Ok(Err(Error::Invalid(message))) => return Err(Error::Invalid(message)),
            Ok(Err(_)) | Err(_) => {}
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(Error::Other(
                "The running Kodosi app did not respond.".into(),
            ));
        }
        tokio::time::sleep(FORWARD_RETRY).await;
    }
}

async fn send(endpoint: &kodosi_os::ipc::Endpoint, activation: &Value) -> Result<bool> {
    let mut stream = kodosi_os::ipc::connect(endpoint)
        .await
        .map_err(local_error)?;
    if let Some(pid) = stream.peer_pid() {
        kodosi_os::process::allow_foreground(pid);
    }
    write_json(
        &mut stream,
        &json!({ "version": ACTIVATION_VERSION, "activation": activation }),
    )
    .await?;
    let mut reader = FrameReader::new(stream);
    reader.maximum = 256;
    let reply: ActivationReply = read_json(&mut reader).await?;
    Ok(reply.accepted)
}
