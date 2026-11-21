//! The link of the Codex in a terminal: its app-server, and Kodosi as a second program of it.
//!
//! Codex's own program for a terminal can work through an app-server at an address. Each
//! terminal has such an address. When a Codex connects there, Kodosi starts an app-server with
//! what that Codex started with, and carries the bytes between the two without a look at them. A
//! second connection of Kodosi's own to the server gets what Codex does and takes what the chat
//! asks for. The server ends with the last Codex of the terminal.

use std::{
    collections::HashMap,
    fmt::Write as _,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::UnixStream,
    sync::{mpsc, oneshot, watch},
    task::JoinSet,
};
use tokio_tungstenite::{WebSocketStream, tungstenite::Message};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    agent::now,
    codex::Codex,
    room::{Asked, Job},
};

/// The server takes the name of the first program that connects as the name of what works with
/// it. That is the terminal's Codex, so Kodosi connects after it.
const AFTER_CODEX: Duration = Duration::from_millis(600);
const START: Duration = Duration::from_secs(15);
const START_STEP: Duration = Duration::from_millis(50);
const END: Duration = Duration::from_secs(2);
/// A link that asks the chat for nothing during the chat's own limit counts as gone.
const ASK_AGAIN: Duration = Duration::from_secs(20);
const MOST_WAITING: usize = 64;

/// The address of a terminal's Codex server, as Codex takes it.
pub(super) fn address(socket: &Path) -> Option<String> {
    socket.to_str().map(|path| format!("unix://{path}"))
}

/// Serves the address of one terminal until its chat ends.
pub(super) async fn serve(
    endpoint: kodosi_os::ipc::Endpoint,
    server: PathBuf,
    room: mpsc::WeakSender<Job>,
    mut chat: watch::Receiver<bool>,
    stop: CancellationToken,
) {
    let mut listener = match kodosi_os::ipc::Listener::bind(&endpoint).await {
        Ok(listener) => listener,
        Err(error) => {
            tracing::warn!(%error, "a terminal has no address for Codex's server");
            return;
        }
    };
    loop {
        let first = tokio::select! {
            () = stop.cancelled() => return,
            gone = chat.wait_for(|_| false) => {
                drop(gone);
                return;
            }
            accepted = listener.accept() => match accepted {
                Ok(stream) => stream,
                Err(_) => return,
            },
        };
        let Some(room) = room.upgrade() else {
            return;
        };
        tokio::select! {
            () = stop.cancelled() => return,
            () = session(first, &mut listener, &server, room) => {}
        }
    }
}

/// One run of the server: from the first Codex that connects until the last one leaves. The
/// Codex of a terminal works through its server also when the chat does not follow it.
async fn session(
    first: kodosi_os::ipc::Stream,
    listener: &mut kodosi_os::ipc::Listener,
    socket: &Path,
    room: mpsc::Sender<Job>,
) {
    let Some(start) = first.peer_pid().and_then(kodosi_pty::process_start) else {
        return;
    };
    let server = match Server::start(&start, socket).await {
        Ok(server) => server,
        Err(error) => {
            tracing::warn!(%error, "Codex's server did not start for a terminal");
            return;
        }
    };
    let mut relays = JoinSet::new();
    relays.spawn(relay(first, socket.to_owned()));
    let left = CancellationToken::new();
    let mut ours = tokio::spawn(follow(socket.to_owned(), room, left.clone()));
    let mut listens = true;
    while !relays.is_empty() {
        tokio::select! {
            accepted = listener.accept(), if listens => match accepted {
                Ok(stream) => {
                    relays.spawn(relay(stream, socket.to_owned()));
                }
                Err(_) => listens = false,
            },
            _ = relays.join_next() => {}
        }
    }
    left.cancel();
    drop(tokio::time::timeout(END, &mut ours).await);
    ours.abort();
    server.stop().await;
}

async fn relay(mut codex: kodosi_os::ipc::Stream, socket: PathBuf) {
    let Ok(mut server) = UnixStream::connect(&socket).await else {
        return;
    };
    drop(tokio::io::copy_bidirectional(&mut codex, &mut server).await);
}

struct Server {
    child: tokio::process::Child,
}

impl Server {
    async fn start(start: &kodosi_pty::ProcessStart, socket: &Path) -> std::io::Result<Self> {
        if socket.symlink_metadata().is_ok() {
            std::fs::remove_file(socket)?;
        }
        let mut command = tokio::process::Command::new(&start.program);
        command
            .arg("app-server")
            .arg("--listen")
            .arg(address(socket).unwrap_or_default())
            .args(settings(&start.arguments))
            .arg("-c")
            .arg(format!(
                "developer_instructions={}",
                toml_string(crate::agents::CONTEXT.trim())
            ))
            // Codex keeps its list of the work only with its plan tool, which the chat shows.
            .args(["-c", "tools.update_plan.enabled=true"])
            .env_clear()
            .envs(start.variables.iter().map(|(name, value)| (name, value)))
            .current_dir(&start.directory)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .kill_on_drop(true);
        let mut server = Self {
            child: command.spawn()?,
        };
        let begun = tokio::time::Instant::now();
        loop {
            if UnixStream::connect(socket).await.is_ok() {
                return Ok(server);
            }
            if server.child.try_wait()?.is_some() || begun.elapsed() > START {
                return Err(std::io::Error::other("the server does not take programs"));
            }
            tokio::time::sleep(START_STEP).await;
        }
    }

    /// The server and the commands that it runs get the time to end by themselves.
    async fn stop(mut self) {
        let Some(group) = self.child.id() else {
            return;
        };
        kodosi_os::process::end_group(group, false);
        if tokio::time::timeout(END, self.child.wait()).await.is_err() {
            kodosi_os::process::end_group(group, true);
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let (Some(group), Ok(None)) = (self.child.id(), self.child.try_wait()) {
            kodosi_os::process::end_group(group, true);
        }
    }
}

/// The settings that a Codex took on its command line. With a server, Codex reads its settings
/// in the server, so the server gets the same ones.
fn settings(arguments: &[String]) -> Vec<&str> {
    let mut settings = Vec::new();
    let mut rest = arguments.iter().skip(1).map(String::as_str);
    while let Some(argument) = rest.next() {
        match argument {
            "-c" | "--config" | "--enable" | "--disable" => {
                if let Some(value) = rest.next() {
                    settings.extend([argument, value]);
                }
            }
            _ if ["--config=", "--enable=", "--disable="]
                .iter()
                .any(|flag| argument.starts_with(flag)) =>
            {
                settings.push(argument);
            }
            _ => {}
        }
    }
    settings
}

/// A text as Codex reads a value of its settings on its command line.
fn toml_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if control.is_control() => {
                let _ = write!(out, "\\u{:04X}", u32::from(control));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

async fn follow(socket: PathBuf, room: mpsc::Sender<Job>, left: CancellationToken) {
    tokio::time::sleep(AFTER_CODEX).await;
    let Ok(stream) = UnixStream::connect(&socket).await else {
        return;
    };
    report(stream, room, left).await;
}

/// What the chat's room hears from this link, in order.
struct Reports {
    room: mpsc::Sender<Job>,
    link: String,
    batch: u64,
}

impl Reports {
    /// False when the chat does not follow this link.
    async fn tell(&mut self, mut event: Value) -> bool {
        event["t"] = now().into();
        self.batch += 1;
        let (reply, heard) = oneshot::channel();
        let job = Job::Events {
            link: self.link.clone(),
            batch: self.batch,
            events: vec![event],
            reply,
        };
        self.room.send(job).await.is_ok() && heard.await == Ok(true)
    }
}

/// Reports what the server says to the chat, and gives the server what the chat's adapter asks
/// for, until Codex leaves the terminal or the server ends.
pub(super) async fn report<S>(stream: S, room: mpsc::Sender<Job>, left: CancellationToken)
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let Some((mut server, mut reports)) = greet(stream, room.clone()).await else {
        return;
    };
    let (commands, mut asked) = mpsc::channel::<Value>(MOST_WAITING);
    let (decisions, mut decided) = mpsc::channel::<(Value, Value)>(MOST_WAITING);
    let asking = tokio::spawn(ask_for_commands(
        room.clone(),
        reports.link.clone(),
        commands,
    ));
    let mut calls = Calls::default();
    loop {
        tokio::select! {
            () = left.cancelled() => break,
            message = server.next() => {
                let Some(Ok(message)) = message else {
                    break;
                };
                let Some((event, request)) = calls.heard(&message) else {
                    continue;
                };
                if !reports.tell(event).await {
                    break;
                }
                // The chat has the request before the link waits for a person's decision of it.
                if let Some(id) = request {
                    tokio::spawn(wait_for_decision(
                        room.clone(),
                        reports.link.clone(),
                        id,
                        decisions.clone(),
                    ));
                }
            }
            command = asked.recv() => {
                let Some(command) = command else {
                    break;
                };
                if say(&mut server, &calls.make(&command)).await.is_err() {
                    break;
                }
            }
            Some((id, decision)) = decided.recv() => {
                if say(&mut server, &json!({"id": id, "result": decision})).await.is_err() {
                    break;
                }
            }
        }
    }
    asking.abort();
    reports.tell(json!({"k": "end"})).await;
}

/// Makes Kodosi known to the server, and the server's Codex known to the chat.
async fn greet<S>(stream: S, room: mpsc::Sender<Job>) -> Option<(WebSocketStream<S>, Reports)>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (mut server, _) = tokio_tungstenite::client_async("ws://localhost/", stream)
        .await
        .ok()?;
    let hello = json!({
        "method": "initialize",
        "id": 0,
        "params": {
            "clientInfo": {"name": "kodosi", "title": "Kodosi", "version": env!("CARGO_PKG_VERSION")},
            "capabilities": {"experimentalApi": true},
        },
    });
    say(&mut server, &hello).await.ok()?;
    let started = answer_to(&mut server, 0).await?;
    say(&mut server, &json!({"method": "initialized"}))
        .await
        .ok()?;
    let mut reports = Reports {
        room,
        link: Uuid::now_v7().to_string(),
        batch: 0,
    };
    let greeting = json!({
        "k": "hello",
        "agent": Codex::NAME,
        "server": started.pointer("/result/userAgent").cloned().unwrap_or_default(),
    });
    reports.tell(greeting).await.then_some((server, reports))
}

/// The calls that the link made for the adapter and that the server did not answer yet.
#[derive(Default)]
struct Calls {
    made: u64,
    tags: HashMap<u64, String>,
}

impl Calls {
    fn make(&mut self, command: &Value) -> Value {
        self.made += 1;
        self.tags.insert(
            self.made,
            command["tag"].as_str().unwrap_or_default().to_owned(),
        );
        let mut call = json!({"method": command["m"], "id": self.made});
        if !command["p"].is_null() {
            call["params"] = command["p"].clone();
        }
        call
    }

    /// A message of the server as the chat's adapter takes it. A request of the server comes
    /// with its number, because it waits for an answer.
    fn heard(&mut self, message: &Message) -> Option<(Value, Option<Value>)> {
        let Message::Text(text) = message else {
            return None;
        };
        let message = serde_json::from_str::<Value>(text.as_str()).ok()?;
        let params = message.get("params");
        match (message.get("method"), message.get("id")) {
            (Some(method), Some(id)) => Some((
                json!({"k": "request", "id": id, "m": method, "p": params}),
                Some(id.clone()),
            )),
            (Some(method), None) => Some((json!({"k": "note", "m": method, "p": params}), None)),
            (None, Some(id)) => {
                let tag = self.tags.remove(&id.as_u64()?)?;
                let answer = json!({
                    "k": "reply",
                    "tag": tag,
                    "r": message.get("result"),
                    "e": message.get("error"),
                });
                Some((answer, None))
            }
            (None, None) => None,
        }
    }
}

async fn say<S>(server: &mut WebSocketStream<S>, message: &Value) -> Result<(), ()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    server
        .send(Message::text(message.to_string()))
        .await
        .map_err(|_| ())
}

/// The server's answer to the request with this number. What it says before the answer is of
/// no use to a program that the server does not know yet.
async fn answer_to<S>(server: &mut WebSocketStream<S>, id: u64) -> Option<Value>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let answer = async {
        while let Some(Ok(message)) = server.next().await {
            if let Message::Text(text) = message
                && let Ok(message) = serde_json::from_str::<Value>(text.as_str())
                && message.get("method").is_none()
                && message.get("id").and_then(Value::as_u64) == Some(id)
            {
                return Some(message);
            }
        }
        None
    };
    tokio::time::timeout(START, answer).await.ok().flatten()
}

/// Asks the chat again and again for what the adapter wants done, as the module of another
/// agent's link does over its own connection.
async fn ask_for_commands(room: mpsc::Sender<Job>, link: String, commands: mpsc::Sender<Value>) {
    loop {
        let (reply, next) = oneshot::channel();
        let job = Job::Next {
            link: link.clone(),
            reply,
        };
        if room.send(job).await.is_err() {
            return;
        }
        match tokio::time::timeout(ASK_AGAIN, next).await {
            Ok(Ok(Some(given))) => {
                for command in given {
                    if commands.send(command).await.is_err() {
                        return;
                    }
                }
            }
            Ok(Ok(None)) => return,
            Ok(Err(_)) | Err(_) => {}
        }
    }
}

/// The decision of a person in the chat for a request of the server, when the chat shows the
/// request as a question and a person decides it there.
async fn wait_for_decision(
    room: mpsc::Sender<Job>,
    link: String,
    id: Value,
    decisions: mpsc::Sender<(Value, Value)>,
) {
    let Some(ask) = Codex::ask_name(&id) else {
        return;
    };
    let (reply, asked) = oneshot::channel();
    if room.send(Job::Ask { link, ask, reply }).await.is_err() {
        return;
    }
    if let Ok(Asked::Decision(decision)) = asked.await {
        drop(decisions.send((id, decision)).await);
    }
}
