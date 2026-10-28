#[cfg(feature = "host")]
use std::pin::Pin;

#[cfg(feature = "host")]
use futures_util::stream::FuturesUnordered;

use super::{channel::Channel, link::Pipe, wire::Frame, *};
#[cfg(feature = "host")]
use super::{link::Caller, wire::Refuse};
use crate::identity::pins::VerifiedIdentity;

const ANSWER_WAIT: Duration = Duration::from_secs(25);
const CALLER_IDLE: Duration = Duration::from_mins(1);
const WAITING_ASKS: usize = 16;
#[cfg(feature = "host")]
const HOST_IDLE: Duration = Duration::from_mins(2);
#[cfg(feature = "host")]
const AT_ONCE: usize = 4;
#[cfg(feature = "host")]
const CALLS: usize = crate::protocol::CALLS_MOST;

pub(crate) struct Ask {
    request: Value,
    reply: oneshot::Sender<Result<Value>>,
}

#[derive(Default)]
pub(crate) struct Lines(std::sync::Mutex<BTreeMap<String, mpsc::Sender<Ask>>>);

#[cfg(feature = "host")]
pub(crate) type Answering =
    Arc<dyn Fn(Value) -> Pin<Box<dyn Future<Output = Value> + Send>> + Send + Sync>;

pub(crate) async fn ask(network: &BackendClient, device: &str, request: Value) -> Result<Value> {
    let credentials = network.credentials()?;
    if !credentials.enrolled {
        return Err(Error::EnrollmentRequired);
    }
    let (reply, answer) = oneshot::channel();
    let mut ask = Ask { request, reply };
    for _ in 0..2 {
        let line = line(network, &credentials, device)?;
        match line.try_send(ask) {
            Ok(()) => {
                return tokio::time::timeout(ANSWER_WAIT, answer)
                    .await
                    .map_err(|_| invalid("The other computer did not answer in time."))?
                    .map_err(|_| invalid("The connection to the other computer was lost."))?;
            }
            Err(mpsc::error::TrySendError::Full(_)) => return Err(Error::Busy),
            Err(mpsc::error::TrySendError::Closed(returned)) => ask = returned,
        }
    }
    Err(Error::Closed)
}

fn line(
    network: &BackendClient,
    credentials: &Credentials,
    device: &str,
) -> Result<mpsc::Sender<Ask>> {
    let mut lines = network.inner.calls.0.lock().map_err(|_| Error::Closed)?;
    if let Some(line) = lines.get(device).filter(|line| !line.is_closed()) {
        return Ok(line.clone());
    }
    let (sender, asks) = mpsc::channel(WAITING_ASKS);
    lines.insert(device.to_owned(), sender.clone());
    drop(lines);
    tokio::spawn(run_line(
        network.clone(),
        credentials.clone(),
        device.to_owned(),
        asks,
    ));
    Ok(sender)
}

async fn run_line(
    network: BackendClient,
    credentials: Credentials,
    device: String,
    mut asks: mpsc::Receiver<Ask>,
) {
    let mut waiting = BTreeMap::<String, oneshot::Sender<Result<Value>>>::new();
    let result = async {
        let mut channel = tokio::select! {
            () = credentials.cancel.cancelled() => return Err(Error::Stale),
            channel = connect(&network, &credentials, &device) => channel?,
        };
        let mut open = true;
        loop {
            if !open && waiting.is_empty() {
                return Ok(());
            }
            tokio::select! {
                () = credentials.cancel.cancelled() => return Err(Error::Stale),
                () = network.inner.shutdown.cancelled() => return Err(Error::Closed),
                ask = asks.recv(), if open => {
                    let Some(Ask { request, reply }) = ask else {
                        open = false;
                        continue;
                    };
                    let id = request["requestId"].as_str().unwrap_or_default().to_owned();
                    if id.is_empty() || waiting.contains_key(&id) {
                        drop(reply.send(Err(invalid("That request is already in progress."))));
                        continue;
                    }
                    waiting.insert(id, reply);
                    channel.send(&Frame::Request(request)).await?;
                }
                frame = channel.receive() => match frame? {
                    Some((Frame::Answer(answer), _)) => {
                        if let Some(reply) = answer["requestId"].as_str().and_then(|id| waiting.remove(id)) {
                            drop(reply.send(Ok(answer)));
                        }
                    }
                    Some((Frame::Refuse(refuse), _)) => return Err(refused(refuse.code.as_str(), refuse.message)),
                    Some(_) => return Err(invalid("The other computer sent a frame that a call does not use.")),
                    None => return Err(Error::Closed),
                },
                () = tokio::time::sleep(CALLER_IDLE), if open && waiting.is_empty() => asks.close(),
                () = tokio::time::sleep(ANSWER_WAIT), if !open => return Ok(()),
            }
        }
    }
    .await;
    asks.close();
    let error = match result {
        Ok(()) => return,
        Err(Error::Closed) => "The connection to the other computer was lost.".to_owned(),
        Err(error) => error.to_string(),
    };
    for reply in waiting.into_values() {
        drop(reply.send(Err(invalid(error.clone()))));
    }
    while let Ok(ask) = asks.try_recv() {
        drop(ask.reply.send(Err(invalid(error.clone()))));
    }
}

fn refused(code: &str, message: String) -> Error {
    match code {
        "busy" => Error::Busy,
        _ => Error::Backend {
            status: 403,
            message,
        },
    }
}

async fn connect(
    network: &BackendClient,
    credentials: &Credentials,
    device: &str,
) -> Result<Channel<Pipe>> {
    if device == credentials.keys.device_id {
        return Err(invalid("This is the computer that you use now."));
    }
    let key = |identity: &VerifiedIdentity| {
        identity
            .devices
            .get(device)
            .filter(|_| !crate::identity::recovery::is_recovery_device(device))
            .map(|certificate| certificate.sig_public_key.clone())
    };
    let known = network
        .known_identity(credentials, &credentials.user_id)
        .await?;
    let key = match key(&known) {
        Some(key) => key,
        None => key(&network
            .fetch_identity_with(credentials, &credentials.user_id)
            .await?)
        .ok_or_else(|| Error::Trust("That computer is not an approved device of yours.".into()))?,
    };
    let pipe = network.inner.link.call(device)?;
    Channel::connect(pipe, &*network.channel_keys(credentials)?, device, &key).await
}

#[cfg(feature = "host")]
pub(crate) fn answer(network: BackendClient, answering: Answering) {
    let (callers, mut calls) = mpsc::channel(CALLS);
    network.inner.link.take_calls(callers);
    tokio::spawn(async move {
        let mut running = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                () = network.inner.shutdown.cancelled() => break,
                caller = calls.recv() => match caller {
                    Some(caller) if running.len() < CALLS => {
                        running.spawn(take_call(network.clone(), caller, Arc::clone(&answering)));
                    }
                    Some(caller) => drop(caller),
                    None => break,
                },
                Some(_) = running.join_next(), if !running.is_empty() => {}
            }
        }
    });
}

#[cfg(feature = "host")]
async fn take_call(network: BackendClient, caller: Caller, answering: Answering) {
    let Caller { pipe, user, device } = caller;
    let result = async {
        let credentials = network.credentials()?;
        let keys = network.channel_keys(&credentials)?;
        let mut channel = Channel::accept(pipe, &keys).await?;
        let key = channel.peer_key()?;
        if let Err(refuse) = admit(&network, &credentials, (&user, &device), &key).await {
            channel.send(&Frame::Refuse(refuse)).await?;
            channel.close().await;
            return Ok(());
        }
        let ended = serve(&mut channel, &answering, &credentials).await;
        if ended.is_ok() {
            channel.close().await;
        }
        ended
    }
    .await;
    if let Err(error) = result
        && !matches!(error, Error::Closed | Error::Stale)
    {
        tracing::warn!(%error, "a call from another device ended");
    }
}

#[cfg(feature = "host")]
async fn admit(
    network: &BackendClient,
    credentials: &Credentials,
    (user, device): (&str, &str),
    key: &[u8],
) -> std::result::Result<(), Refuse> {
    let refuse = |code: &str, message: &str| Refuse {
        code: code.to_owned(),
        message: message.to_owned(),
    };
    if user != credentials.user_id || crate::identity::recovery::is_recovery_device(device) {
        return Err(refuse(
            "access",
            "Only your own devices can start terminals on this computer.",
        ));
    }
    let approved = |identity: &VerifiedIdentity| {
        identity
            .devices
            .get(device)
            .is_some_and(|certificate| certificate.sig_public_key == key)
    };
    let mut identity = network.known_identity(credentials, user).await;
    if identity.as_ref().is_ok_and(|identity| !approved(identity)) {
        identity = network.fetch_identity_with(credentials, user).await;
    }
    match identity {
        Ok(identity) if approved(&identity) => Ok(()),
        Err(error) if error.unanswered() => Err(refuse(
            "busy",
            "The other computer could not check this device; try again.",
        )),
        Ok(_) | Err(_) => Err(refuse("access", "This device is not approved.")),
    }
}

#[cfg(feature = "host")]
async fn serve(
    channel: &mut Channel<Pipe>,
    answering: &Answering,
    credentials: &Credentials,
) -> Result<()> {
    let mut answers = FuturesUnordered::new();
    loop {
        tokio::select! {
            () = credentials.cancel.cancelled() => return Err(Error::Stale),
            frame = channel.receive(), if answers.len() < AT_ONCE => match frame? {
                Some((Frame::Request(request), _)) => answers.push(answering(request)),
                Some(_) => return Err(invalid("The other device sent a frame that a call does not use.")),
                None => return Ok(()),
            },
            Some(answer) = answers.next(), if !answers.is_empty() => {
                channel.send(&Frame::Answer(answer)).await?;
            }
            () = tokio::time::sleep(HOST_IDLE), if answers.is_empty() => return Ok(()),
        }
    }
}

#[cfg(all(test, feature = "host"))]
mod tests;
