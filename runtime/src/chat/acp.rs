//! The terminal's side of an agent that has no add-on interface but a server for programs, the
//! Agent Client Protocol (ACP), such as Antigravity CLI.
//!
//! `kodosi acp -- <command>` runs the agent's own server in the terminal, with the terminal's
//! folder and environment, as a person would run it there. It makes Kodosi known to the server
//! and carries each message of the server to the terminal's chat as it is, and the calls and the
//! decisions of the chat's adapter back. What the messages mean in the chat is all in the adapter;
//! this side shows only what a program shows in its terminal: the agent's state and the title of
//! its session. Kodosi never talks to the agent's provider: the agent's own program does.

use std::{
    collections::{HashMap, HashSet},
    io::{self, Read as _, Write as _},
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufReader},
    net::UnixStream,
    process::{Child, ChildStdin, ChildStdout},
    signal::unix::{SignalKind, signal},
    sync::mpsc,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{agent::now, antigravity::Antigravity};
use crate::text::Text;

/// The time that a server has to answer its first message.
const START: Duration = Duration::from_secs(60);
/// The time that a server has to end by itself after its input closed.
const END: Duration = Duration::from_secs(5);
const MOST_MESSAGE: usize = 8 * 1024 * 1024;
const MOST_HEAD: usize = 16 * 1024;
const MOST_BATCH: usize = 64;
const MOST_WAITING: usize = 256;
const MOST_TITLE: usize = 120;
/// The JSON-RPC code of a method that the other side does not have.
const NO_METHOD: i64 = -32601;
/// What Kodosi's chat calls on the terminal's side itself: the agent's own sign-in, and the
/// state of an agent that asked the person a question in its answer and waits for the next message.
pub(super) const SIGN_IN: &str = "kodosi/signIn";
pub(super) const ASKING: &str = "kodosi/asking";

/// The choices of the session that the line of the terminal gives. The chat's adapter makes them.
pub(crate) struct Choices {
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) mode: Option<String>,
    pub(crate) resume: Option<String>,
}

/// Runs the agent's server until it ends, and gives the code to end with.
pub(crate) async fn run(command: Vec<String>, choices: Choices) -> Result<u8, Text> {
    let (Some(socket), Some(terminal)) = (
        std::env::var_os(super::LINK_VARIABLE).map(PathBuf::from),
        std::env::var("KODOSI_SESSION_ID")
            .ok()
            .filter(|id| Uuid::parse_str(id).is_ok()),
    ) else {
        return Err(text!(
            "Run it in a terminal of Kodosi, whose chat shows the agent."
        ));
    };
    let Some((agent, label)) = command.first().and_then(|program| agent_of(program)) else {
        return Err(text!(
            "Kodosi runs only the server of Antigravity CLI this way: kodosi acp -- agy acp."
        ));
    };
    let start = json!({
        "model": choices.model,
        "effort": choices.effort,
        "mode": choices.mode,
        "resume": choices.resume,
    });
    say(&text!(
        "{agent} runs in the chat of this terminal. Write to it there, or here and press Return.",
        agent = label
    ));
    let mut typed = Typed::default();
    loop {
        let link = Link {
            socket: socket.clone(),
            terminal: terminal.clone(),
            id: Uuid::now_v7().to_string(),
        };
        let server = Server::start(&command).await?;
        match relay(server, link, agent, &start, &mut typed).await? {
            Ended::Code(code) => return Ok(code),
            Ended::SignIn => {
                say(&text!(
                    "{agent} is not signed in. Sign in with its own steps below.",
                    agent = label
                ));
                report("state=blocked:kind=auth");
                if !sign_in(&command[0]).await {
                    return Err(text!("{agent} is not signed in.", agent = label));
                }
            }
        }
    }
}

/// The agent of the chat's adapter for a server's program, and the agent's name for people.
fn agent_of(program: &str) -> Option<(&'static str, &'static str)> {
    let name = Path::new(program).file_name()?.to_str()?;
    (name == "agy").then_some((Antigravity::NAME, "Antigravity CLI"))
}

fn say(text: &Text) {
    write_out(&format!("{text}\n"));
}

/// The agent's own sign-in in this terminal, which asks the person for a code.
async fn sign_in(program: &str) -> bool {
    tokio::process::Command::new(program)
        .args(["acp", "--login"])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .is_ok_and(|status| status.success())
}

enum Ended {
    Code(u8),
    /// The agent asks for a sign-in before its session can start.
    SignIn,
}

struct Server {
    child: Child,
    input: ChildStdin,
    output: Messages,
    /// The server's answer to Kodosi's first message: what it is and what it offers.
    started: Option<Value>,
}

impl Server {
    async fn start(command: &[String]) -> Result<Self, Text> {
        let failed = |error: io::Error| {
            text!(
                "{program} did not start: {error}",
                program = command[0].clone(),
                error = error.to_string()
            )
        };
        // The terminal's interrupt is for this side, which stops the turn, and not for the
        // server, which would end.
        let mut child = tokio::process::Command::new(&command[0])
            .args(&command[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .process_group(0)
            .kill_on_drop(true)
            .spawn()
            .map_err(failed)?;
        let (Some(input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
            return Err(failed(io::ErrorKind::BrokenPipe.into()));
        };
        let mut server = Self {
            child,
            input,
            output: Messages::new(output),
            started: None,
        };
        let hello = json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": 1,
                "clientCapabilities": {
                    "fs": {"readTextFile": false, "writeTextFile": false},
                    "terminal": false,
                },
                "clientInfo": {"name": "kodosi", "title": "Kodosi", "version": crate::PRODUCT_VERSION},
            },
        });
        server.say(&hello).await.map_err(failed)?;
        let answer = tokio::time::timeout(START, async {
            loop {
                match server.output.next().await {
                    Some(message) if message.get("method").is_none() && message["id"] == 0 => {
                        return Some(message);
                    }
                    Some(_) => {}
                    None => return None,
                }
            }
        })
        .await
        .ok()
        .flatten();
        match answer {
            Some(answer) if answer.get("result").is_some() => {
                server.started = answer.get("result").cloned();
                Ok(server)
            }
            Some(answer) => Err(text!(
                "{program} did not start: {error}",
                program = command[0].clone(),
                error = answer
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            )),
            None => Err(failed(io::ErrorKind::TimedOut.into())),
        }
    }

    async fn say(&mut self, message: &Value) -> io::Result<()> {
        let mut line = serde_json::to_vec(message)?;
        line.push(b'\n');
        self.input.write_all(&line).await?;
        self.input.flush().await
    }

    /// The server ends by itself when its input closes, so that it keeps what it saved.
    async fn end(mut self) -> u8 {
        drop(self.input);
        let status = if let Ok(status) = tokio::time::timeout(END, self.child.wait()).await {
            status.ok()
        } else {
            drop(self.child.start_kill());
            self.child.wait().await.ok()
        };
        status
            .and_then(|status| status.code())
            .and_then(|code| u8::try_from(code).ok())
            .unwrap_or(1)
    }
}

/// The messages of a server, one JSON value on each line.
struct Messages {
    reader: BufReader<ChildStdout>,
    line: Vec<u8>,
}

impl Messages {
    fn new(output: ChildStdout) -> Self {
        Self {
            reader: BufReader::new(output),
            line: Vec::new(),
        }
    }

    /// The next message, or none when the server ended. A line that is no JSON object, or that
    /// is too long, is passed over.
    async fn next(&mut self) -> Option<Value> {
        loop {
            self.line.clear();
            let mut over = false;
            loop {
                let buffer = self.reader.fill_buf().await.ok()?;
                if buffer.is_empty() {
                    return None;
                }
                let end = buffer.iter().position(|byte| *byte == b'\n');
                let part = &buffer[..end.map_or(buffer.len(), |end| end + 1)];
                if self.line.len() + part.len() > MOST_MESSAGE {
                    over = true;
                    self.line.clear();
                } else if !over {
                    self.line.extend_from_slice(part);
                }
                let used = part.len();
                self.reader.consume(used);
                if end.is_some() {
                    break;
                }
            }
            if over {
                continue;
            }
            if let Ok(message) = serde_json::from_slice::<Value>(&self.line)
                && message.is_object()
            {
                return Some(message);
            }
        }
    }
}

/// The terminal's chat, at the local endpoint where the agents of this computer report.
struct Link {
    socket: PathBuf,
    terminal: String,
    id: String,
}

impl Link {
    async fn call(&self, path: &str, mut body: Value) -> io::Result<(u16, Value)> {
        body["v"] = json!(super::claude::VERSION);
        body["terminal"] = json!(self.terminal);
        body["link"] = json!(self.id);
        let body = serde_json::to_vec(&body)?;
        let mut stream = UnixStream::connect(&self.socket).await?;
        let head = format!(
            "POST {path} HTTP/1.1\r\nhost: kodosi\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n",
            body.len()
        );
        stream.write_all(head.as_bytes()).await?;
        stream.write_all(&body).await?;
        stream.flush().await?;
        answer(&mut stream).await
    }
}

fn invalid(reason: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason)
}

/// The status and the JSON body of the chat's answer.
async fn answer(stream: &mut UnixStream) -> io::Result<(u16, Value)> {
    let mut kept = Vec::new();
    let (head, status, length) = loop {
        let mut headers = [httparse::EMPTY_HEADER; 16];
        let mut response = httparse::Response::new(&mut headers);
        match response.parse(&kept) {
            Ok(httparse::Status::Complete(head)) => {
                let length = response
                    .headers
                    .iter()
                    .find(|header| header.name.eq_ignore_ascii_case("content-length"))
                    .and_then(|header| std::str::from_utf8(header.value).ok())
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or_default();
                if length > MOST_MESSAGE {
                    return Err(invalid("the answer is too large"));
                }
                break (head, response.code.unwrap_or_default(), length);
            }
            Ok(httparse::Status::Partial) if kept.len() <= MOST_HEAD => {
                if stream.read_buf(&mut kept).await? == 0 {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
            }
            _ => return Err(invalid("the answer is not HTTP")),
        }
    };
    while kept.len() < head + length {
        if stream.read_buf(&mut kept).await? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
    }
    let body = &kept[head..head + length];
    let body = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(body)?
    };
    Ok((status, body))
}

/// The lines that people and Kodosi type into the terminal, as messages for the agent. The
/// terminal is read only once the session began, so that the agent's own sign-in before it
/// gets what the person types there.
#[derive(Default)]
struct Typed {
    lines: Option<mpsc::Receiver<String>>,
}

impl Typed {
    fn listen(&mut self) {
        if self.listens() {
            return;
        }
        let (sender, lines) = mpsc::channel(MOST_WAITING);
        self.lines = Some(lines);
        std::thread::spawn(move || {
            let mut input = io::stdin().lock();
            let mut kept = Vec::new();
            let mut buffer = [0u8; 4096];
            while let Ok(count) = input.read(&mut buffer) {
                if count == 0 {
                    break;
                }
                kept.extend_from_slice(&buffer[..count]);
                while let Some(end) = kept.iter().position(|byte| *byte == b'\n') {
                    let line = String::from_utf8_lossy(&kept[..end]).trim().to_owned();
                    kept.drain(..=end);
                    if !line.is_empty() && sender.blocking_send(line).is_err() {
                        return;
                    }
                }
                if kept.len() > MOST_MESSAGE {
                    kept.clear();
                }
            }
        });
    }

    async fn next(&mut self) -> Option<String> {
        match &mut self.lines {
            Some(lines) => lines.recv().await,
            None => std::future::pending().await,
        }
    }

    const fn listens(&self) -> bool {
        self.lines.is_some()
    }
}

/// The open calls and requests of the link, which give the state that the terminal shows.
#[derive(Default)]
struct Open {
    /// The tag of each call of the adapter that the server did not answer yet.
    calls: HashMap<u64, String>,
    /// The calls that send a person's message, which end with their turn.
    prompts: Vec<u64>,
    /// The requests of the server that wait for a person.
    requests: HashSet<String>,
    /// The agent asked the person a question and waits for the next message.
    asking: bool,
    /// How the last turn ended: done, or with an error.
    ended: Option<&'static str>,
    made: u64,
    shown: Option<&'static str>,
}

impl Open {
    fn busy(&self) -> bool {
        !self.prompts.is_empty() || !self.requests.is_empty()
    }

    /// Tells the terminal what the agent does, as an agent in a terminal reports it itself.
    fn show(&mut self) {
        let state = if !self.requests.is_empty() {
            "state=blocked:kind=permission"
        } else if !self.prompts.is_empty() {
            "state=working"
        } else if self.asking {
            "state=blocked:kind=question"
        } else {
            self.ended.unwrap_or("state=idle")
        };
        if self.shown == Some(state) {
            return;
        }
        self.shown = Some(state);
        report(state);
    }
}

fn report(state: &str) {
    write_out(&format!("\x1b]7501;{state}:app=antigravity\x07"));
}

fn write_out(text: &str) {
    let mut out = io::stdout().lock();
    drop(out.write_all(text.as_bytes()));
    drop(out.flush());
}

/// The name of the question that a request of the server makes, which the adapter gives too.
pub(super) fn ask_name(request: &Value) -> Option<String> {
    match request {
        Value::Number(number) => Some(format!("r{number}")),
        Value::String(name) if !name.is_empty() => Some(format!("r{name}")),
        _ => None,
    }
}

/// Carries the messages between the server and the chat until the server ends, the chat leaves,
/// or the agent asks for a sign-in.
async fn relay(
    mut server: Server,
    link: Link,
    agent: &str,
    start: &Value,
    typed: &mut Typed,
) -> Result<Ended, Text> {
    let link = Arc::new(link);
    let greeting = json!({
        "k": "hello",
        "t": now(),
        "agent": agent,
        "started": server.started.take(),
        "folder": std::env::current_dir().ok(),
        "start": start,
    });
    let followed = link
        .call("/v1/events", json!({"batch": 1, "events": [greeting]}))
        .await
        .is_ok_and(|(status, _)| status == 204);
    if !followed {
        server.end().await;
        return Err(text!("The chat of this terminal does not take the agent."));
    }
    let mut interrupts = signal(SignalKind::interrupt())
        .map_err(|error| Text::plain(format!("The interrupt is not taken: {error}")))?;
    let lost = CancellationToken::new();
    let (events, waiting) = mpsc::channel::<Value>(MOST_WAITING);
    let (commands_in, mut commands) = mpsc::channel::<Value>(MOST_WAITING);
    let (decided, mut decisions) = mpsc::channel::<(Value, Value)>(MOST_WAITING);
    let posting = tokio::spawn(post(Arc::clone(&link), waiting, decided, lost.clone()));
    let polling = tokio::spawn(poll(Arc::clone(&link), commands_in, lost.clone()));
    let mut relay = Relay {
        server,
        events,
        open: Open::default(),
    };
    relay.open.show();
    let ended = loop {
        let ended = tokio::select! {
            () = lost.cancelled() => Some(Ended::Code(0)),
            message = relay.server.output.next() => match message {
                Some(message) => relay.heard(&message, typed).await,
                None => Some(Ended::Code(0)),
            },
            command = commands.recv() => match command {
                Some(command) => relay.command(&command, typed).await,
                None => Some(Ended::Code(0)),
            },
            Some((id, decision)) = decisions.recv() => relay.decide(id, decision).await,
            written = typed.next() => match written {
                Some(words) => relay.tell(json!({"k": "typed", "t": now(), "text": words})).await,
                None => Some(Ended::Code(0)),
            },
            _ = interrupts.recv() => {
                if relay.open.busy() {
                    relay.tell(json!({"k": "interrupt", "t": now()})).await
                } else {
                    Some(Ended::Code(130))
                }
            }
        };
        relay.open.show();
        if let Some(ended) = ended {
            break ended;
        }
    };
    polling.abort();
    let Relay {
        server,
        events,
        mut open,
        ..
    } = relay;
    let code = server.end().await;
    if matches!(ended, Ended::Code(_)) {
        drop(events.send(json!({"k": "end", "t": now()})).await);
    }
    drop(events);
    drop(tokio::time::timeout(END, posting).await);
    open.prompts.clear();
    open.requests.clear();
    open.asking = false;
    open.ended = None;
    open.show();
    Ok(match ended {
        Ended::Code(0) => Ended::Code(code),
        ended => ended,
    })
}

struct Relay {
    server: Server,
    events: mpsc::Sender<Value>,
    open: Open,
}

impl Relay {
    /// Gives the chat an event. Nothing more can go when the chat does not take it.
    async fn tell(&self, event: Value) -> Option<Ended> {
        self.events
            .send(event)
            .await
            .is_err()
            .then_some(Ended::Code(1))
    }

    async fn say(&mut self, message: &Value) -> Option<Ended> {
        self.server
            .say(message)
            .await
            .is_err()
            .then_some(Ended::Code(1))
    }

    async fn heard(&mut self, message: &Value, typed: &mut Typed) -> Option<Ended> {
        let heard = hear(&mut self.open, message);
        if let Some(ask) = heard.ask.as_ref().and_then(ask_name) {
            self.open.requests.insert(ask);
        }
        if let Some(id) = heard.refuse {
            let refusal = json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": NO_METHOD, "message": "Kodosi does not offer this method."},
            });
            if let Some(ended) = self.say(&refusal).await {
                return Some(ended);
            }
        }
        if heard.began {
            typed.listen();
        }
        if let Some(title) = heard.title {
            write_out(&format!("\x1b]2;{title}\x07"));
        }
        match heard.event {
            Some(event) => self.tell(event).await,
            None => None,
        }
    }

    async fn command(&mut self, command: &Value, typed: &Typed) -> Option<Ended> {
        if command["m"] == SIGN_IN {
            // The terminal's lines go to the chat once the session began, so the agent's own
            // sign-in cannot read them then.
            return Some(if typed.listens() {
                Ended::Code(1)
            } else {
                Ended::SignIn
            });
        }
        if command["m"] == ASKING {
            self.open.asking = true;
            return None;
        }
        let message = call(&mut self.open, command);
        self.say(&message).await
    }

    async fn decide(&mut self, id: Value, decision: Value) -> Option<Ended> {
        if let Some(ask) = ask_name(&id) {
            self.open.requests.remove(&ask);
        }
        self.say(&json!({"jsonrpc": "2.0", "id": id, "result": decision}))
            .await
    }
}

/// What a message of the server is for the link.
#[derive(Default)]
struct Heard {
    event: Option<Value>,
    /// A request for a person's decision.
    ask: Option<Value>,
    /// A request for something that Kodosi does not offer.
    refuse: Option<Value>,
    /// The session began, so the terminal can take messages.
    began: bool,
    /// The title that the agent gave the session, which the terminal takes as its program's title.
    title: Option<String>,
}

fn hear(open: &mut Open, message: &Value) -> Heard {
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    match (message.get("method"), message.get("id")) {
        (Some(method), Some(id)) => {
            if method == "session/request_permission" {
                Heard {
                    event: Some(
                        json!({"k": "request", "t": now(), "id": id, "m": method, "p": params}),
                    ),
                    ask: Some(id.clone()),
                    ..Heard::default()
                }
            } else {
                Heard {
                    refuse: Some(id.clone()),
                    ..Heard::default()
                }
            }
        }
        (Some(method), None) => Heard {
            title: session_title(&params),
            event: Some(json!({"k": "note", "t": now(), "m": method, "p": params})),
            ..Heard::default()
        },
        (None, Some(id)) => {
            let Some(number) = id.as_u64() else {
                return Heard::default();
            };
            let Some(tag) = open.calls.remove(&number) else {
                return Heard::default();
            };
            let result = message.get("result").cloned();
            if let Some(at) = open.prompts.iter().position(|prompt| *prompt == number) {
                open.prompts.remove(at);
                open.ended = turn_end(result.as_ref());
            }
            let began = tag.starts_with("session") && result.is_some();
            Heard {
                event: Some(json!({
                    "k": "reply",
                    "t": now(),
                    "tag": tag,
                    "r": result,
                    "e": message.get("error"),
                })),
                began,
                ..Heard::default()
            }
        }
        (None, None) => Heard::default(),
    }
}

/// The title that the agent gave its session, as a terminal shows it.
fn session_title(params: &Value) -> Option<String> {
    let update = params
        .get("update")
        .filter(|update| update["sessionUpdate"] == "session_info_update")?;
    let title = update["title"].as_str()?;
    Some(
        title
            .chars()
            .filter(|character| !character.is_control())
            .take(MOST_TITLE)
            .collect(),
    )
}

/// The state that a turn leaves: done, none after a person's stop, or an error.
fn turn_end(result: Option<&Value>) -> Option<&'static str> {
    match result.and_then(|result| result["stopReason"].as_str()) {
        Some("end_turn") => Some("state=done"),
        Some("cancelled") => None,
        _ => Some("state=error"),
    }
}

/// A call or a notification of the adapter as the server takes it.
fn call(open: &mut Open, command: &Value) -> Value {
    let mut message = json!({"jsonrpc": "2.0", "method": command["m"]});
    if !command["p"].is_null() {
        message["params"] = command["p"].clone();
    }
    if command["notify"] == true {
        return message;
    }
    open.made += 1;
    let tag = command["tag"].as_str().unwrap_or_default().to_owned();
    if command["m"] == "session/prompt" {
        open.prompts.push(open.made);
        open.asking = false;
        open.ended = None;
    }
    open.calls.insert(open.made, tag);
    message["id"] = json!(open.made);
    message
}

/// Gives the chat what the server says, in order. A chat that no longer follows this link ends it.
/// The chat has a request of the server before the link waits for a person's decision of it.
async fn post(
    link: Arc<Link>,
    mut waiting: mpsc::Receiver<Value>,
    decided: mpsc::Sender<(Value, Value)>,
    lost: CancellationToken,
) {
    let mut batch = 1u64;
    while let Some(first) = waiting.recv().await {
        let mut events = vec![first];
        while events.len() < MOST_BATCH {
            match waiting.try_recv() {
                Ok(event) => events.push(event),
                Err(_) => break,
            }
        }
        let requests = events
            .iter()
            .filter(|event| event["k"] == "request")
            .filter_map(|event| Some((event["id"].clone(), ask_name(&event["id"])?)))
            .collect::<Vec<_>>();
        batch += 1;
        let heard = link
            .call("/v1/events", json!({"batch": batch, "events": events}))
            .await;
        if !heard.is_ok_and(|(status, _)| status == 204) {
            lost.cancel();
            return;
        }
        for (id, ask) in requests {
            tokio::spawn(wait_for_decision(
                Arc::clone(&link),
                id,
                ask,
                decided.clone(),
            ));
        }
    }
}

/// Asks the chat again and again for what the adapter wants done.
async fn poll(link: Arc<Link>, commands: mpsc::Sender<Value>, lost: CancellationToken) {
    loop {
        match link.call("/v1/next", json!({})).await {
            Ok((200, answer)) => {
                for command in answer["commands"].as_array().into_iter().flatten() {
                    if commands.send(command.clone()).await.is_err() {
                        return;
                    }
                }
            }
            Ok((204, _)) => {}
            _ => {
                lost.cancel();
                return;
            }
        }
    }
}

/// The decision of a person in the chat for a request of the server. A question that closed
/// with no decision, as when its turn ended, is cancelled.
async fn wait_for_decision(
    link: Arc<Link>,
    id: Value,
    ask: String,
    decisions: mpsc::Sender<(Value, Value)>,
) {
    loop {
        match link.call("/v1/ask", json!({"ask": ask})).await {
            Ok((200, answer)) => {
                let decision = answer
                    .get("decision")
                    .cloned()
                    .unwrap_or_else(|| json!({"outcome": {"outcome": "cancelled"}}));
                drop(decisions.send((id, decision)).await);
                return;
            }
            Ok((204, _)) => {}
            _ => return,
        }
    }
}
