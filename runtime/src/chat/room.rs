use std::{collections::HashMap, path::PathBuf, time::Duration};

use base64::Engine as _;
use serde_json::Value;
use tokio::{
    sync::{mpsc, oneshot, watch},
    time::Instant,
};

use super::{
    ChatFrame, ChatPerson, ChatRequest, MOST_MESSAGE, MOST_PICTURES,
    agent::{self, Agent, Stop},
    mention::{Message, Peer, Peers},
    state::{Chat, ends_of},
};
use crate::{
    canvas::{PIPE_QUEUE, PipeEnds},
    terminal::{ChatAgent, Picture, TERMINAL_PICTURE_MAX_BYTES},
    text::Text,
};

const JOBS: usize = 256;
/// Disconnect timeout: an active link immediately re-issues long-poll requests (which return
/// within `link::HOLD`), so 45 seconds of silence means the link is gone.
const SILENCE: Duration = Duration::from_secs(45);
/// Grace period for a link to reconnect after its active long-poll connection drops.
const GRACE: Duration = Duration::from_secs(3);
const MOST_DECISIONS: usize = 32;

pub(super) enum Job {
    Attach {
        person: ChatPerson,
        reply: oneshot::Sender<PipeEnds>,
    },
    Request {
        viewer: u64,
        request: ChatRequest,
        /// The files of the pictures that came with a message, saved on this computer.
        files: Vec<String>,
    },
    Detach {
        viewer: u64,
    },
    /// Replies `false` if `link` is not the active link for this chat.
    Events {
        link: String,
        /// Monotonic batch sequence number used to ignore retried batches.
        batch: u64,
        events: Vec<Value>,
        /// The session of the process that sent them.
        session: Option<u32>,
        reply: oneshot::Sender<bool>,
    },
    /// The session of the terminal's own program.
    #[cfg(unix)]
    Terminal {
        session: u32,
    },
    Ask {
        link: String,
        ask: String,
        reply: oneshot::Sender<Asked>,
    },
    /// Replies `None` if `link` is not the active link for this chat.
    Next {
        link: String,
        reply: oneshot::Sender<Option<Vec<Value>>>,
    },
    Gone {
        link: String,
    },
    Conversation {
        reply: oneshot::Sender<Option<String>>,
    },
    Change {
        line: String,
        reply: oneshot::Sender<bool>,
    },
}

pub(super) enum Asked {
    Decision(Value),
    Closed,
}

#[derive(Clone)]
pub(crate) struct ChatHandle {
    jobs: mpsc::Sender<Job>,
    linked: watch::Receiver<Option<ChatAgent>>,
}

impl ChatHandle {
    /// The chat of the terminal `id`, whose agent can reach the agents of `peers` but its own,
    /// and which keeps the pictures of its messages in `pictures`.
    pub(super) fn spawn(
        id: String,
        peers: Option<watch::Receiver<Peers>>,
        pictures: Option<PathBuf>,
    ) -> Self {
        let (jobs, receiver) = mpsc::channel(JOBS);
        let (linked, linking) = watch::channel(None);
        let mut room = Room {
            id,
            peers,
            pictures,
            jobs: receiver,
            own: jobs.downgrade(),
            linked,
            chat: Chat::at_home(
                kodosi_os::fs::home_dir()
                    .ok()
                    .and_then(|home| home.into_os_string().into_string().ok()),
            ),
            agent: None,
            link: None,
            terminal: None,
            heard: 0,
            seen: Instant::now(),
            gone: None,
            poll: None,
            commands: Vec::new(),
            waits: HashMap::new(),
            decisions: HashMap::new(),
            viewers: HashMap::new(),
            made: 0,
        };
        room.know_peers();
        tokio::spawn(room.run());
        Self {
            jobs,
            linked: linking,
        }
    }

    pub(super) fn jobs(&self) -> mpsc::WeakSender<Job> {
        self.jobs.downgrade()
    }

    /// Reports the agent linked to the chat, or `None` when no agent is linked.
    pub(crate) fn linked(&self) -> watch::Receiver<Option<ChatAgent>> {
        self.linked.clone()
    }

    /// Returns the active conversation ID for agents whose session is tracked only by the chat
    /// adapter (such as Codex and Antigravity).
    pub(crate) async fn conversation(&self) -> Option<String> {
        let (reply, conversation) = oneshot::channel();
        self.jobs.send(Job::Conversation { reply }).await.ok()?;
        conversation.await.ok().flatten()
    }

    /// The terminal's own program started with this session.
    #[cfg(unix)]
    pub(crate) async fn runs_in(&self, session: u32) {
        self.jobs.send(Job::Terminal { session }).await.ok();
    }

    /// Gives the link of the agent a change of its model, effort or fast mode, as the line that a
    /// person would type. False when no link that takes it reports, and the line goes to the
    /// terminal.
    pub(crate) async fn change(&self, line: String) -> bool {
        let (reply, taken) = oneshot::channel();
        if self.jobs.send(Job::Change { line, reply }).await.is_err() {
            return false;
        }
        taken.await.unwrap_or(false)
    }

    /// Opens the chat for a view of `person`, who is named `name`.
    pub(crate) async fn attach(&self, person: String, name: String) -> Result<PipeEnds, Text> {
        let closed = || text!("The terminal of this chat closed.");
        let (reply, ends) = oneshot::channel();
        let person = ChatPerson { id: person, name };
        self.jobs
            .send(Job::Attach { person, reply })
            .await
            .map_err(|_| closed())?;
        ends.await.map_err(|_| closed())
    }
}

struct Viewer {
    person: ChatPerson,
    frames: mpsc::Sender<Vec<u8>>,
}

struct Room {
    /// The terminal's ID.
    id: String,
    peers: Option<watch::Receiver<Peers>>,
    pictures: Option<PathBuf>,
    jobs: mpsc::Receiver<Job>,
    own: mpsc::WeakSender<Job>,
    linked: watch::Sender<Option<ChatAgent>>,
    chat: Chat,
    agent: Option<Box<dyn Agent>>,
    link: Option<String>,
    /// The session of the terminal's own program, where an agent of the terminal runs.
    terminal: Option<u32>,
    /// Last processed batch sequence number for the active link.
    heard: u64,
    seen: Instant,
    gone: Option<Instant>,
    poll: Option<oneshot::Sender<Option<Vec<Value>>>>,
    commands: Vec<Value>,
    waits: HashMap<String, oneshot::Sender<Asked>>,
    /// User decisions buffered until the agent's `/v1/ask` request arrives.
    decisions: HashMap<String, Value>,
    viewers: HashMap<u64, Viewer>,
    made: u64,
}

impl Room {
    async fn run(mut self) {
        loop {
            let check = self.check_at();
            tokio::select! {
                job = self.jobs.recv() => match job {
                    Some(job) => self.handle(job),
                    None => return,
                },
                () = async {
                    match check {
                        Some(at) => tokio::time::sleep_until(at).await,
                        None => std::future::pending().await,
                    }
                } => self.lose(),
                changed = changed(self.peers.as_mut()) => {
                    if changed {
                        self.know_peers();
                    } else {
                        self.peers = None;
                    }
                }
            }
            self.tell();
        }
    }

    /// Shows the agents of other terminals that this terminal's agent can reach now.
    fn know_peers(&mut self) {
        let Some(peers) = &mut self.peers else {
            return;
        };
        let shown = peers
            .borrow_and_update()
            .iter()
            .filter(|peer| peer.shown.terminal != self.id)
            .map(|peer| peer.shown.clone())
            .collect();
        self.chat.set_peers(shown);
    }

    fn peers(&self) -> Vec<Peer> {
        self.peers.as_ref().map_or_else(Vec::new, |peers| {
            peers
                .borrow()
                .iter()
                .filter(|peer| peer.shown.terminal != self.id)
                .cloned()
                .collect()
        })
    }

    fn check_at(&self) -> Option<Instant> {
        if self.link.is_none() || !self.is_linked() {
            return None;
        }
        let silent = self.seen + SILENCE;
        Some(self.gone.map_or(silent, |gone| silent.min(gone + GRACE)))
    }

    fn handle(&mut self, job: Job) {
        match job {
            Job::Attach { person, reply } => self.attach(person, reply),
            Job::Request {
                viewer,
                request,
                files,
            } => self.request(viewer, request, files),
            Job::Detach { viewer } => {
                self.viewers.remove(&viewer);
            }
            Job::Events {
                link,
                batch,
                events,
                session,
                reply,
            } => {
                let greeting = events
                    .iter()
                    .find(|event| event.get("k").and_then(Value::as_str) == Some("hello"));
                // A program can move its session into a background job that a daemon runs. The
                // job keeps the terminal's environment, but the terminal no longer shows it.
                let in_terminal = self
                    .terminal
                    .zip(session)
                    .is_none_or(|(terminal, session)| terminal == session);
                if let Some(greeting) = greeting
                    && self.link.as_deref() != Some(&link)
                    && in_terminal
                {
                    self.follow(link.clone(), greeting);
                }
                let ours = self.hears(&link);
                let _ = reply.send(ours);
                if !ours || batch <= self.heard {
                    return;
                }
                self.heard = batch;
                for event in &events {
                    let closed = self
                        .agent
                        .as_mut()
                        .map_or_default(|agent| agent.apply(&mut self.chat, event));
                    self.close(closed);
                }
                if !self.is_linked() {
                    self.link = None;
                    self.poll = None;
                }
            }
            Job::Ask { link, ask, reply } => {
                self.hears(&link);
                if let Some(decision) = self.decisions.remove(&ask) {
                    drop(reply.send(Asked::Decision(decision)));
                } else if self.chat.asks().iter().any(|open| open.id == ask) {
                    self.waits.insert(ask, reply);
                } else {
                    drop(reply.send(Asked::Closed));
                }
            }
            Job::Next { link, reply } => {
                if self.hears(&link) {
                    self.poll = Some(reply);
                } else {
                    drop(reply.send(None));
                }
            }
            Job::Gone { link } => {
                if self.link.as_deref() == Some(&link) {
                    self.gone = Some(Instant::now());
                }
            }
            Job::Conversation { reply } => {
                let conversation = self
                    .agent
                    .as_ref()
                    .filter(|agent| agent.is_linked())
                    .and_then(|agent| agent.conversation())
                    .map(str::to_owned);
                drop(reply.send(conversation));
            }
            #[cfg(unix)]
            Job::Terminal { session } => self.terminal = Some(session),
            Job::Change { line, reply } => {
                let command = self
                    .agent
                    .as_mut()
                    .filter(|agent| agent.is_linked())
                    .and_then(|agent| agent.change(&line));
                let _ = reply.send(command.is_some());
                self.commands.extend(command);
            }
        }
    }

    fn follow(&mut self, link: String, greeting: &Value) {
        let name = greeting
            .get("agent")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if self.agent.as_ref().is_none_or(|agent| agent.name() != name) {
            let Some(agent) = agent::named(name) else {
                return;
            };
            self.lose();
            self.chat.reset();
            self.chat.set_commands([]);
            self.chat.set_settings([]);
            self.agent = Some(agent);
        } else if let Some(agent) = &mut self.agent {
            agent.relinked(&mut self.chat);
        }
        self.link = Some(link);
        self.heard = 0;
        self.poll = None;
    }

    fn is_linked(&self) -> bool {
        self.agent.as_ref().is_some_and(|agent| agent.is_linked())
    }

    /// Records activity from `link` and returns `true` if it is the active link.
    /// Restores the agent via `back` in case it timed out while suspended rather than exiting.
    fn hears(&mut self, link: &str) -> bool {
        if self.link.as_deref() != Some(link) {
            return false;
        }
        self.seen = Instant::now();
        self.gone = None;
        if let Some(agent) = &mut self.agent {
            agent.back(&mut self.chat);
        }
        true
    }

    fn lose(&mut self) {
        self.poll = None;
        let closed = self
            .agent
            .as_mut()
            .map_or_default(|agent| agent.lost(&mut self.chat));
        self.close(closed);
    }

    fn close(&mut self, asks: Vec<String>) {
        for ask in asks {
            self.decisions.remove(&ask);
            if let Some(wait) = self.waits.remove(&ask) {
                drop(wait.send(Asked::Closed));
            }
        }
    }

    fn request(&mut self, viewer: u64, request: ChatRequest, files: Vec<String>) {
        let peers = match &request {
            ChatRequest::Send { .. } | ChatRequest::HandOff { .. } => self.peers(),
            _ => Vec::new(),
        };
        let Some(agent) = &mut self.agent else {
            return;
        };
        let by = self
            .viewers
            .get(&viewer)
            .map(|viewer| viewer.person.clone());
        match request {
            ChatRequest::Send { text, .. } => {
                let text = text.trim();
                if (text.is_empty() && files.is_empty()) || !agent.is_linked() {
                    return;
                }
                let message = Message::naming(ends_of(text, MOST_MESSAGE), &peers);
                if let Some(id) = self.chat.queue(message, by, files) {
                    if !agent.takes(text) {
                        self.chat.in_terminal(&id);
                    }
                    agent.retry();
                }
            }
            ChatRequest::HandOff { terminal } => {
                let Some(peer) = peers.iter().find(|peer| peer.shown.terminal == terminal) else {
                    return;
                };
                if agent.is_linked()
                    && self
                        .chat
                        .queue(Message::handing(peer), by, Vec::new())
                        .is_some()
                {
                    agent.retry();
                }
            }
            ChatRequest::Unqueue { id } => {
                if !agent.has(&id) {
                    self.chat.unqueue(&id);
                }
            }
            ChatRequest::Answer { ask, answer } => {
                if let Some(decision) = agent.decision(&ask, &answer, by.as_ref()) {
                    self.decide(ask, decision);
                }
            }
            ChatRequest::Stop => match agent.stop(&self.chat) {
                Stop::Decisions(decisions) => {
                    for (ask, decision) in decisions {
                        self.decide(ask, decision);
                    }
                }
                Stop::Command(command) => self.commands.push(command),
                Stop::Nothing => {}
            },
            ChatRequest::Settings => {
                let command = agent.settings(&mut self.chat);
                self.commands.extend(command);
            }
            ChatRequest::Set { key, value } => {
                let command = agent.set(&mut self.chat, &key, &value);
                self.commands.extend(command);
            }
        }
    }

    fn decide(&mut self, ask: String, decision: Value) {
        let waits = self
            .agent
            .as_mut()
            .is_some_and(|agent| agent.answered(&mut self.chat, &ask, &decision));
        if !waits {
            return;
        }
        match self.waits.remove(&ask) {
            Some(wait) => {
                if let Err(Asked::Decision(decision)) = wait.send(Asked::Decision(decision)) {
                    self.keep(ask, decision);
                }
            }
            None => self.keep(ask, decision),
        }
    }

    fn keep(&mut self, ask: String, decision: Value) {
        if self.decisions.len() < MOST_DECISIONS {
            self.decisions.insert(ask, decision);
        }
    }

    fn attach(&mut self, person: ChatPerson, reply: oneshot::Sender<PipeEnds>) {
        self.publish();
        let (frames, receiver) = mpsc::channel(PIPE_QUEUE);
        let (sender, mut requests) = mpsc::channel::<Vec<u8>>(PIPE_QUEUE);
        let snapshot = encoded(&ChatFrame::Snapshot(Box::new(self.chat.snapshot())));
        if frames.try_send(snapshot).is_err() || reply.send(PipeEnds { sender, receiver }).is_err()
        {
            return;
        }
        self.made += 1;
        let viewer = self.made;
        self.viewers.insert(viewer, Viewer { person, frames });
        let jobs = self.own.clone();
        let pictures = self.pictures.clone();
        tokio::spawn(async move {
            while let Some(frame) = requests.recv().await {
                let Ok(request) = serde_json::from_slice::<ChatRequest>(&frame) else {
                    continue;
                };
                drop(frame);
                let (request, files) = saved(request, pictures.clone()).await;
                let Some(jobs) = jobs.upgrade() else {
                    return;
                };
                if jobs
                    .send(Job::Request {
                        viewer,
                        request,
                        files,
                    })
                    .await
                    .is_err()
                {
                    return;
                }
            }
            if let Some(jobs) = jobs.upgrade() {
                drop(jobs.send(Job::Detach { viewer }).await);
            }
        });
    }

    /// Broadcasts pending updates, dropping any slow viewer whose queue is full so it
    /// disconnects and resyncs via a fresh snapshot instead of blocking the room.
    fn publish(&mut self) {
        let Some(update) = self.chat.take() else {
            return;
        };
        if self.viewers.is_empty() {
            return;
        }
        let frame = encoded(&ChatFrame::Update(update));
        self.viewers
            .retain(|_, viewer| viewer.frames.try_send(frame.clone()).is_ok());
    }

    fn tell(&mut self) {
        if let Some(agent) = &mut self.agent {
            self.commands.extend(agent.commands(&mut self.chat));
        }
        if !self.commands.is_empty()
            && let Some(poll) = self.poll.take()
            && let Err(Some(commands)) = poll.send(Some(std::mem::take(&mut self.commands)))
        {
            self.commands = commands;
        }
        let linked = self
            .agent
            .as_ref()
            .filter(|agent| agent.is_linked())
            .map(|agent| ChatAgent {
                face: agent.works_in(),
                helpers: self.chat.helpers(),
            });
        self.linked.send_if_modified(|current| {
            let changed = *current != linked;
            *current = linked;
            changed
        });
        self.publish();
    }
}

fn encoded(frame: &ChatFrame) -> Vec<u8> {
    serde_json::to_vec(frame).unwrap_or_default()
}

/// Saves the pictures of a message in `folder` for the agent, and gives the request without their
/// bytes, with the files that hold them.
async fn saved(request: ChatRequest, folder: Option<PathBuf>) -> (ChatRequest, Vec<String>) {
    let ChatRequest::Send { text, pictures } = request else {
        return (request, Vec::new());
    };
    let files = match folder {
        Some(folder) if !pictures.is_empty() => {
            tokio::task::spawn_blocking(move || save_pictures(&folder, &pictures))
                .await
                .unwrap_or_default()
        }
        _ => Vec::new(),
    };
    let request = ChatRequest::Send {
        text,
        pictures: Vec::new(),
    };
    (request, files)
}

/// The pictures of one message together take up to the size of one picture that a terminal takes.
fn save_pictures(folder: &std::path::Path, pictures: &[String]) -> Vec<String> {
    let mut left = TERMINAL_PICTURE_MAX_BYTES;
    let mut files = Vec::new();
    for data in pictures.iter().take(MOST_PICTURES) {
        let saved = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                if bytes.len() > left {
                    return Err("the pictures of the message are too large".to_owned());
                }
                let picture = Picture::new(bytes.into()).map_err(|error| error.to_string())?;
                left -= picture.bytes.len();
                crate::terminal::save_picture(folder, &picture).map_err(|error| error.to_string())
            });
        match saved.map(|path| path.into_os_string().into_string()) {
            Ok(Ok(file)) => files.push(file),
            Ok(Err(_)) => tracing::warn!("a picture of the chat has a path that is not UTF-8"),
            Err(error) => tracing::warn!(%error, "a picture of the chat was not saved"),
        }
    }
    files
}

/// Whether the agents of `peers` changed; a room without them never hears of a change.
async fn changed(peers: Option<&mut watch::Receiver<Peers>>) -> bool {
    match peers {
        Some(peers) => peers.changed().await.is_ok(),
        None => std::future::pending().await,
    }
}
