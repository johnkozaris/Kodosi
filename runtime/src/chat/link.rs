use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use serde_json::{Value, json};
use tokio::{
    io::AsyncReadExt as _,
    sync::{Semaphore, mpsc, oneshot, watch},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    claude, http,
    mention::{Peer, Peers},
    room::{Asked, ChatHandle, Job},
};

const ROLE: &str = "agents";
/// Long-poll timeout; kept below the agent link client's 30-second request timeout.
const HOLD: Duration = Duration::from_secs(20);
const MOST_CONNECTIONS: usize = 256;
const MOST_EVENTS: usize = 4096;
const MOST_NAME: usize = 64;
const MOST_KEPT: usize = 9 * 1024 * 1024;

type Rooms = Arc<Mutex<HashMap<Uuid, mpsc::WeakSender<Job>>>>;
type Variables = Vec<(&'static str, String)>;

pub(crate) struct TerminalChat {
    pub(crate) handle: ChatHandle,
    /// Environment variables passed to the terminal so agents know where to report.
    pub(crate) variables: Variables,
}

impl TerminalChat {
    #[cfg(test)]
    pub(crate) fn alone() -> Self {
        Self {
            handle: ChatHandle::spawn(String::new(), None),
            variables: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct Links {
    rooms: Rooms,
    peers: Arc<watch::Sender<Peers>>,
    addresses: Arc<[(&'static str, String)]>,
    codex: Option<CodexAddresses>,
    _served: Arc<tokio_util::sync::DropGuard>,
}

#[derive(Clone)]
struct CodexAddresses {
    namespace: Arc<str>,
    stop: CancellationToken,
}

impl Links {
    pub(crate) async fn serve(root: &Path) -> Self {
        let rooms = Rooms::default();
        let stop = CancellationToken::new();
        let (addresses, codex) = match listen(root, Arc::clone(&rooms), stop.clone()).await {
            Ok((addresses, namespace)) => (addresses, CodexAddresses::new(namespace, &stop)),
            Err(error) => {
                tracing::warn!(%error, "agents cannot report to the chats of their terminals");
                (Vec::new(), None)
            }
        };
        Self {
            rooms,
            peers: Arc::new(watch::Sender::new(Peers::default())),
            addresses: addresses.into(),
            codex,
            _served: Arc::new(stop.drop_guard()),
        }
    }

    #[cfg(test)]
    pub(crate) fn unserved() -> Self {
        let stop = CancellationToken::new();
        Self {
            rooms: Rooms::default(),
            peers: Arc::new(watch::Sender::new(Peers::default())),
            addresses: Arc::new([]),
            codex: None,
            _served: Arc::new(stop.drop_guard()),
        }
    }

    pub(crate) fn room(&self, terminal: Uuid) -> TerminalChat {
        let handle = ChatHandle::spawn(terminal.to_string(), Some(self.peers.subscribe()));
        {
            let mut rooms = self.rooms.lock().unwrap_or_else(PoisonError::into_inner);
            rooms.retain(|_, room| room.upgrade().is_some());
            rooms.insert(terminal, handle.jobs());
        }
        let codex = self
            .codex
            .as_ref()
            .and_then(|codex| codex.serve(terminal, &handle))
            .map(|address| (super::CODEX_VARIABLE, address));
        TerminalChat {
            variables: self.addresses.iter().cloned().chain(codex).collect(),
            handle,
        }
    }

    /// Gives the chat of each terminal the agents that its agent can reach: those of the other
    /// terminals that this computer lists.
    pub(crate) fn reach(&self, peers: Vec<Peer>) {
        self.peers.send_if_modified(|known| {
            let changed = **known != peers;
            if changed {
                *known = Arc::new(peers);
            }
            changed
        });
    }

    /// Returns the common address prefix shared by all per-terminal Codex servers.
    pub(crate) fn codex_servers(&self) -> Option<String> {
        let folder = kodosi_os::ipc::private_dir(&self.codex.as_ref()?.namespace).ok()?;
        super::codex_link::address(&folder.join(super::codex_link::NAME))
    }
}

impl CodexAddresses {
    /// Probes once with a dummy UUID to verify socket paths fit within OS path-length limits,
    /// since every terminal produces endpoints of the same length.
    fn new(namespace: Arc<str>, stop: &CancellationToken) -> Option<Self> {
        Self::endpoints(&namespace, Uuid::max())?;
        Some(Self {
            namespace,
            stop: stop.clone(),
        })
    }

    fn serve(&self, terminal: Uuid, chat: &ChatHandle) -> Option<String> {
        let (endpoint, server) = Self::endpoints(&self.namespace, terminal)?;
        let address = super::codex_link::address(endpoint.address())?;
        tokio::spawn(super::codex_link::serve(
            endpoint,
            server.address().to_owned(),
            chat.jobs(),
            chat.linked(),
            self.stop.clone(),
        ));
        Some(address)
    }

    /// Returns the proxy and server socket endpoints for a terminal's Codex session.
    /// Uses the trailing 16 hex chars of the UUID v7 (the random suffix) to stay within
    /// Unix socket path length limits.
    fn endpoints(
        namespace: &str,
        terminal: Uuid,
    ) -> Option<(kodosi_os::ipc::Endpoint, kodosi_os::ipc::Endpoint)> {
        let name = terminal.simple().to_string();
        let name = &name[name.len() - 16..];
        let start = super::codex_link::NAME;
        Some((
            kodosi_os::ipc::Endpoint::socket_file(namespace, &format!("{start}{name}")).ok()?,
            kodosi_os::ipc::Endpoint::socket_file(namespace, &format!("{start}{name}-s")).ok()?,
        ))
    }
}

async fn listen(
    root: &Path,
    rooms: Rooms,
    stop: CancellationToken,
) -> std::io::Result<(Variables, Arc<str>)> {
    let root = kodosi_os::fs::canonicalize(root)?;
    let namespace = crate::local_host::namespace(&root);
    let slots = Arc::new(Semaphore::new(MOST_CONNECTIONS));
    let mut addresses = Vec::new();
    for (variable, endpoint) in endpoints(&namespace)? {
        let address = endpoint
            .address()
            .to_str()
            .ok_or_else(|| std::io::Error::other("the address of the agents' links is not UTF-8"))?
            .to_owned();
        let listener = kodosi_os::ipc::Listener::bind(&endpoint).await?;
        tokio::spawn(accept(
            listener,
            Arc::clone(&rooms),
            Arc::clone(&slots),
            stop.clone(),
        ));
        addresses.push((variable, address));
    }
    // Binding succeeded, so this host owns the namespace and any remaining Codex servers are
    // orphans from a previous host.
    #[cfg(unix)]
    if let Ok(folder) = kodosi_os::ipc::private_dir(&namespace) {
        super::codex_link::end_left_servers(&folder).await;
    }
    Ok((addresses, namespace.into()))
}

/// Claude Code's link uses a Unix socket file, while Node-based links on Windows require
/// a named pipe for local IPC.
fn endpoints(namespace: &str) -> std::io::Result<Vec<(&'static str, kodosi_os::ipc::Endpoint)>> {
    let mut endpoints = vec![(
        super::LINK_VARIABLE,
        kodosi_os::ipc::Endpoint::socket_file(namespace, ROLE)?,
    )];
    if cfg!(windows) {
        endpoints.push((
            super::PIPE_VARIABLE,
            kodosi_os::ipc::Endpoint::new(namespace, ROLE)?,
        ));
    }
    Ok(endpoints)
}

async fn accept(
    mut listener: kodosi_os::ipc::Listener,
    rooms: Rooms,
    slots: Arc<Semaphore>,
    stop: CancellationToken,
) {
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
}

async fn connection(mut stream: kodosi_os::ipc::Stream, rooms: Rooms) {
    let peer = stream.peer_pid();
    let mut kept = Vec::new();
    while let Ok(Some(request)) = http::read(&mut stream, &mut kept).await {
        let Some((status, body)) = respond(&mut stream, &mut kept, &rooms, peer, request).await
        else {
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

/// Handles one HTTP request, returning `None` if the peer disconnected while waiting.
async fn respond(
    stream: &mut kodosi_os::ipc::Stream,
    kept: &mut Vec<u8>,
    rooms: &Rooms,
    peer: Option<u32>,
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
            started_with(&mut events, peer);
            let (reply, heard) = oneshot::channel();
            let job = Job::Events {
                link,
                batch: body
                    .get("batch")
                    .and_then(Value::as_u64)
                    .unwrap_or_default(),
                events,
                session: peer.and_then(session_of),
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

#[cfg(unix)]
fn session_of(pid: u32) -> Option<u32> {
    kodosi_pty::process_session(pid)
}

#[cfg(not(unix))]
const fn session_of(_pid: u32) -> Option<u32> {
    None
}

/// Attaches the peer process's launch arguments to its `hello` event. Some session settings,
/// such as Claude Code's permission flags, appear only on the command line and nowhere else.
fn started_with(events: &mut [Value], peer: Option<u32>) {
    let Some(hello) = events
        .iter_mut()
        .find(|event| event.get("k").and_then(Value::as_str) == Some("hello"))
    else {
        return;
    };
    if let Some(start) = peer.and_then(kodosi_pty::process_start) {
        hello["arguments"] = json!(start.arguments);
    }
}

/// Waits up to `HOLD` for `answer` while monitoring `stream` for peer disconnect.
/// Returns `Some(Some(val))` on reply, `Some(None)` on timeout or dropped channel,
/// and `None` if the peer closed the connection or exceeded `MOST_KEPT`.
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
