//! The chat of one terminal.
//!
//! It takes what the agent's link reports, keeps the chat, sends each change to the people who
//! have the chat open, and carries what they send back to the agent.

use std::{collections::HashMap, time::Duration};

use serde_json::Value;
use tokio::{
    sync::{mpsc, oneshot, watch},
    time::Instant,
};

use super::{
    ChatFrame, ChatPerson, ChatRequest, MOST_MESSAGE,
    agent::{self, Agent, Stop},
    state::{Chat, ends_of},
};
use crate::{
    canvas::{PIPE_QUEUE, PipeEnds},
    terminal::ChatAgent,
    text::Text,
};

const JOBS: usize = 256;
/// The module of a link asks again at once after each answer, and each answer comes within the
/// time that a request waits. A link that asks for nothing during this time is gone.
const SILENCE: Duration = Duration::from_secs(45);
/// After the waiting request of a link broke, the module has this time to ask again.
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
    },
    Detach {
        viewer: u64,
    },
    /// The answer says if the reports come from the link that the chat follows.
    Events {
        link: String,
        /// The module counts its batches, and sends a batch again when it got no answer.
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
    /// The answer is nothing for a link that the chat does not follow.
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
    pub(super) fn spawn() -> Self {
        let (jobs, receiver) = mpsc::channel(JOBS);
        let (linked, linking) = watch::channel(None);
        let room = Room {
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
        tokio::spawn(room.run());
        Self {
            jobs,
            linked: linking,
        }
    }

    pub(super) fn jobs(&self) -> mpsc::WeakSender<Job> {
        self.jobs.downgrade()
    }

    /// The agent that reports to the chat, while one does.
    pub(crate) fn linked(&self) -> watch::Receiver<Option<ChatAgent>> {
        self.linked.clone()
    }

    /// The conversation that the agent of the chat shows, when the chat follows one that only the
    /// chat knows.
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
    jobs: mpsc::Receiver<Job>,
    own: mpsc::WeakSender<Job>,
    linked: watch::Sender<Option<ChatAgent>>,
    chat: Chat,
    /// The adapter of the agent whose link the chat follows.
    agent: Option<Box<dyn Agent>>,
    /// The link that the chat follows: the last that started in the terminal.
    link: Option<String>,
    /// The session of the terminal's own program, where an agent of the terminal runs.
    terminal: Option<u32>,
    /// The last batch of the followed module that the chat took.
    heard: u64,
    seen: Instant,
    gone: Option<Instant>,
    poll: Option<oneshot::Sender<Option<Vec<Value>>>>,
    commands: Vec<Value>,
    waits: HashMap<String, oneshot::Sender<Asked>>,
    /// Decisions of people that wait for the next request of their question.
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
            }
            self.tell();
        }
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
            Job::Request { viewer, request } => self.request(viewer, request),
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

    /// The chat follows a link that starts to report. A link of another kind of agent ends what
    /// the chat had from the agent before it.
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
            self.agent = Some(agent);
        }
        self.link = Some(link);
        self.heard = 0;
        self.poll = None;
    }

    fn is_linked(&self) -> bool {
        self.agent.as_ref().is_some_and(|agent| agent.is_linked())
    }

    /// True for a request of the link that the chat follows. A link that was silent is back: its
    /// agent was stopped for a time, not ended.
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

    /// The questions that ended with no decision from the chat stop their waiting requests.
    fn close(&mut self, asks: Vec<String>) {
        for ask in asks {
            self.decisions.remove(&ask);
            if let Some(wait) = self.waits.remove(&ask) {
                drop(wait.send(Asked::Closed));
            }
        }
    }

    fn request(&mut self, viewer: u64, request: ChatRequest) {
        let Some(agent) = &mut self.agent else {
            return;
        };
        match request {
            ChatRequest::Send { text } => {
                let text = text.trim();
                if text.is_empty() || !agent.is_linked() {
                    return;
                }
                let by = self
                    .viewers
                    .get(&viewer)
                    .map(|viewer| viewer.person.clone());
                if self.chat.queue(ends_of(text, MOST_MESSAGE), by).is_some() {
                    agent.retry();
                }
            }
            ChatRequest::Unqueue { id } => {
                if !agent.has(&id) {
                    self.chat.unqueue(&id);
                }
            }
            ChatRequest::Answer { ask, answer } => {
                let by = self.viewers.get(&viewer).map(|viewer| &viewer.person);
                if let Some(decision) = agent.decision(&ask, &answer, by) {
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
        }
    }

    /// The first decision of a question counts. It goes to the request that waits for it, or to
    /// the next one.
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
        tokio::spawn(async move {
            while let Some(frame) = requests.recv().await {
                let Some(jobs) = jobs.upgrade() else {
                    return;
                };
                if let Ok(request) = serde_json::from_slice::<ChatRequest>(&frame)
                    && jobs.send(Job::Request { viewer, request }).await.is_err()
                {
                    return;
                }
            }
            if let Some(jobs) = jobs.upgrade() {
                drop(jobs.send(Job::Detach { viewer }).await);
            }
        });
    }

    /// A viewer that does not take its frames loses the pipe, and gets the chat as it is when it
    /// opens the pipe again.
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
            self.commands.extend(agent.commands(&self.chat));
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
