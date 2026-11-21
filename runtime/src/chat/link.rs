//! The local endpoint where the agents in this computer's terminals report to their chats.

use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use serde_json::{Value, json};
use tokio::{
    io::AsyncReadExt as _,
    sync::{Semaphore, mpsc, oneshot},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    claude, http,
    room::{Asked, ChatHandle, Job},
};

const ROLE: &str = "agents";
/// The module of a link gives each of its requests 30 seconds.
const HOLD: Duration = Duration::from_secs(20);
const MOST_CONNECTIONS: usize = 256;
const MOST_EVENTS: usize = 4096;
const MOST_NAME: usize = 64;
const MOST_KEPT: usize = 9 * 1024 * 1024;

type Rooms = Arc<Mutex<HashMap<Uuid, mpsc::WeakSender<Job>>>>;

/// What a new terminal gets for its chat.
pub(crate) struct TerminalChat {
    pub(crate) handle: ChatHandle,
    /// Where the terminal's agents report, on a platform that has such an address.
    pub(crate) socket: Option<String>,
    /// Where a Codex of the terminal finds its app-server, on such a platform.
    pub(crate) codex: Option<String>,
}

impl TerminalChat {
    #[cfg(test)]
    pub(crate) fn alone() -> Self {
        Self {
            handle: ChatHandle::spawn(),
            socket: None,
            codex: None,
        }
    }
}

/// The chats of this computer's terminals, and the address where their agents reach them.
#[derive(Clone)]
pub(crate) struct Links {
    rooms: Rooms,
    socket: Option<Arc<str>>,
    #[cfg(unix)]
    codex: Option<CodexAddresses>,
    _served: Arc<tokio_util::sync::DropGuard>,
}

/// What gives each terminal its own address for Codex's server.
#[cfg(unix)]
#[derive(Clone)]
struct CodexAddresses {
    /// The name of this host's private folder of local addresses.
    namespace: Arc<str>,
    stop: CancellationToken,
}

impl Links {
    pub(crate) async fn serve(root: &Path) -> Self {
        let rooms = Rooms::default();
        let stop = CancellationToken::new();
        let served = listen(root, Arc::clone(&rooms), stop.clone())
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "agents cannot report to the chats of their terminals");
                None
            });
        Self {
            rooms,
            socket: served.as_ref().map(|(socket, _)| Arc::clone(socket)),
            #[cfg(unix)]
            codex: served.map(|(_, namespace)| CodexAddresses {
                namespace,
                stop: stop.clone(),
            }),
            _served: Arc::new(stop.drop_guard()),
        }
    }

    #[cfg(test)]
    pub(crate) fn unserved() -> Self {
        let stop = CancellationToken::new();
        Self {
            rooms: Rooms::default(),
            socket: None,
            #[cfg(unix)]
            codex: None,
            _served: Arc::new(stop.drop_guard()),
        }
    }

    pub(crate) fn room(&self, terminal: Uuid) -> TerminalChat {
        let handle = ChatHandle::spawn();
        {
            let mut rooms = self.rooms.lock().unwrap_or_else(PoisonError::into_inner);
            rooms.retain(|_, room| room.upgrade().is_some());
            rooms.insert(terminal, handle.jobs());
        }
        TerminalChat {
            #[cfg(unix)]
            codex: self
                .codex
                .as_ref()
                .and_then(|codex| codex.serve(terminal, &handle)),
            // Codex reaches its app-server at a socket file, and a Windows endpoint is a named pipe.
            #[cfg(not(unix))]
            codex: None,
            handle,
            socket: self.socket.as_deref().map(str::to_owned),
        }
    }
}

#[cfg(unix)]
impl Links {
    /// How the address of each terminal's Codex server starts.
    pub(crate) fn codex_servers(&self) -> Option<String> {
        let folder = kodosi_os::ipc::private_dir(&self.codex.as_ref()?.namespace).ok()?;
        super::codex_link::address(&folder.join("codex-"))
    }
}

#[cfg(unix)]
impl CodexAddresses {
    /// Serves the address of one terminal, and gives it as Codex takes it.
    fn serve(&self, terminal: Uuid, chat: &ChatHandle) -> Option<String> {
        // The end of a terminal's name is what tells it from the others of its time, and an
        // address has room for few characters.
        let name = terminal.simple().to_string();
        let name = &name[name.len() - 16..];
        let endpoint =
            kodosi_os::ipc::Endpoint::new(&self.namespace, &format!("codex-{name}")).ok()?;
        let server = endpoint
            .socket()
            .with_file_name(format!("codex-{name}-s.sock"));
        let address = super::codex_link::address(endpoint.socket())?;
        tokio::spawn(super::codex_link::serve(
            endpoint,
            server,
            chat.jobs(),
            chat.linked(),
            self.stop.clone(),
        ));
        Some(address)
    }
}

/// The address where the agents report, with the name of the host's folder of local addresses.
async fn listen(
    root: &Path,
    rooms: Rooms,
    stop: CancellationToken,
) -> std::io::Result<Option<(Arc<str>, Arc<str>)>> {
    let root = kodosi_os::fs::canonicalize(root)?;
    let namespace = crate::local_host::namespace(&root);
    let endpoint = kodosi_os::ipc::Endpoint::new(&namespace, ROLE)?;
    let Some(socket) = socket_of(&endpoint) else {
        return Ok(None);
    };
    let mut listener = kodosi_os::ipc::Listener::bind(&endpoint).await?;
    tokio::spawn(async move {
        let slots = Arc::new(Semaphore::new(MOST_CONNECTIONS));
        loop {
            let stream = tokio::select! {
                () = stop.cancelled() => return,
                accepted = listener.accept() => match accepted {
                    Ok(stream) => stream,
                    Err(error) => {
                        tracing::warn!(%error, "the endpoint of the agents' links closed");
                        return;
                    }
                },
            };
            let Ok(slot) = Arc::clone(&slots).try_acquire_owned() else {
                continue;
            };
            let (rooms, stop) = (Arc::clone(&rooms), stop.clone());
            tokio::spawn(async move {
                let _slot = slot;
                tokio::select! {
                    () = stop.cancelled() => {}
                    () = connection(stream, rooms) => {}
                }
            });
        }
    });
    Ok(Some((socket, namespace.into())))
}

#[cfg(unix)]
fn socket_of(endpoint: &kodosi_os::ipc::Endpoint) -> Option<Arc<str>> {
    endpoint.socket().to_str().map(Arc::from)
}

/// The module of an agent's link reaches a socket file, and a Windows endpoint is a named pipe.
#[cfg(not(unix))]
fn socket_of(_: &kodosi_os::ipc::Endpoint) -> Option<Arc<str>> {
    None
}

async fn connection(mut stream: kodosi_os::ipc::Stream, rooms: Rooms) {
    let mut kept = Vec::new();
    while let Ok(Some(request)) = http::read(&mut stream, &mut kept).await {
        let Some((status, body)) = respond(&mut stream, &mut kept, &rooms, request).await else {
            return;
        };
        let body = body.map_or_default(|body| body.to_string());
        if http::answer(&mut stream, status, body.as_bytes())
            .await
            .is_err()
        {
            return;
        }
    }
}

/// The answer to one request, or nothing when the other side left while the request waited.
async fn respond(
    stream: &mut kodosi_os::ipc::Stream,
    kept: &mut Vec<u8>,
    rooms: &Rooms,
    request: http::Request,
) -> Option<(u16, Option<Value>)> {
    let Ok(mut body) = serde_json::from_slice::<Value>(&request.body) else {
        return Some((400, None));
    };
    if body.get("v").and_then(Value::as_u64) != Some(claude::VERSION) {
        return Some((409, None));
    }
    let name = |key: &str| {
        body.get(key)
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty() && name.len() <= MOST_NAME)
            .map(str::to_owned)
    };
    let room = name("terminal")
        .and_then(|terminal| Uuid::parse_str(&terminal).ok())
        .and_then(|terminal| {
            let rooms = rooms.lock().unwrap_or_else(PoisonError::into_inner);
            rooms.get(&terminal).and_then(mpsc::WeakSender::upgrade)
        });
    let (Some(room), Some(link)) = (room, name("link")) else {
        return Some((404, None));
    };
    Some(match request.path.as_str() {
        "/v1/events" => {
            let Some(Value::Array(mut events)) = body.get_mut("events").map(Value::take) else {
                return Some((400, None));
            };
            events.truncate(MOST_EVENTS);
            let (reply, heard) = oneshot::channel();
            let job = Job::Events {
                link,
                batch: body
                    .get("batch")
                    .and_then(Value::as_u64)
                    .unwrap_or_default(),
                events,
                reply,
            };
            if room.send(job).await.is_ok() && heard.await == Ok(true) {
                (204, None)
            } else {
                (410, None)
            }
        }
        "/v1/ask" => {
            let Some(ask) = name("ask") else {
                return Some((400, None));
            };
            let (reply, asked) = oneshot::channel();
            if room.send(Job::Ask { link, ask, reply }).await.is_err() {
                return Some((404, None));
            }
            match wait(stream, kept, asked).await? {
                Some(Asked::Decision(decision)) => (200, Some(json!({"decision": decision}))),
                Some(Asked::Closed) => (200, Some(json!({"closed": true}))),
                None => (204, None),
            }
        }
        "/v1/next" => {
            let (reply, next) = oneshot::channel();
            let job = Job::Next {
                link: link.clone(),
                reply,
            };
            if room.send(job).await.is_err() {
                return Some((404, None));
            }
            match wait(stream, kept, next).await {
                Some(Some(Some(commands))) => (200, Some(json!({"commands": commands}))),
                Some(Some(None)) => (410, None),
                Some(None) => (204, None),
                None => {
                    drop(room.send(Job::Gone { link }).await);
                    return None;
                }
            }
        }
        _ => (404, None),
    })
}

/// The chat's answer to a request that waits: the answer, nothing when the time of the request
/// ended, and no value at all when the other side left.
async fn wait<T>(
    stream: &mut kodosi_os::ipc::Stream,
    kept: &mut Vec<u8>,
    mut answer: oneshot::Receiver<T>,
) -> Option<Option<T>> {
    let hold = tokio::time::sleep(HOLD);
    tokio::pin!(hold);
    loop {
        tokio::select! {
            answer = &mut answer => return Some(answer.ok()),
            () = &mut hold => return Some(None),
            read = stream.read_buf(kept) => match read {
                Ok(more) if more > 0 && kept.len() <= MOST_KEPT => {}
                _ => return None,
            },
        }
    }
}
