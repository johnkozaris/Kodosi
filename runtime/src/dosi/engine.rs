use std::{
    collections::VecDeque,
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use tokio::sync::mpsc;
use uuid::Uuid;

use super::{
    AGENTS, DosiAgent, DosiBody, DosiConversation, DosiEntry, DosiInstructions, DosiItem, DosiLog,
    DosiPage, DosiQueued, DosiRecord, DosiRemote, DosiSnapshot, DosiState, DosiTone,
    DosiToolStatus, SharedInstructions, TOKEN_VARIABLE,
    adapters::{Adapter, Choice, Event},
    context,
    conversation::{self, Conversation},
    folder,
    process::{self, Child, Spawn},
    shared::Shared,
    store::{Book, ProviderSession, Saved, Store},
};
use crate::{
    Error, Result,
    agents::{self, Agent, AgentChoice, AgentChoiceChange, Launcher},
};

const IDLE_LIFE: Duration = Duration::from_mins(10);
const STOP_GRACE: Duration = Duration::from_secs(6);
const FLUSH_DELAY: Duration = Duration::from_millis(80);
const BEAT: Duration = Duration::from_mins(3);
const REMOTE_LIFE: i64 = 10 * 60 * 1000;
const REMOTE_STOP: Duration = Duration::from_secs(8);
const MAX_QUEUED: usize = 8;
const MAX_SESSIONS: usize = 8;
const MESSAGES: usize = 256;

pub(crate) struct Failure {
    pub retry: bool,
    pub message: String,
}

pub(crate) enum Message {
    Prepared {
        process: u64,
        result: std::result::Result<agents::Runner, String>,
    },
    Line {
        process: u64,
        line: String,
    },
    Exited {
        process: u64,
        error: Option<String>,
    },
    Idle {
        process: u64,
        turns: u64,
    },
    Late {
        process: u64,
        turns: u64,
    },
    Flush,
    Agents(Vec<DosiAgent>),
    Beat {
        generation: u64,
        turns: u64,
    },
    Written {
        generation: u64,
        result: std::result::Result<u64, Failure>,
    },
    Synced {
        generation: u64,
        result: std::result::Result<DosiLog, Failure>,
    },
    Saved {
        generation: u64,
        instructions: SharedInstructions,
        result: std::result::Result<(), Failure>,
    },
    Retry {
        generation: u64,
    },
    Gone {
        generation: u64,
        turn: String,
    },
}

pub(crate) struct Typing {
    pub terminal: String,
    pub text: String,
}

#[derive(Default)]
pub(crate) struct Output {
    pub events: Vec<Value>,
    pub snapshot: bool,
    pub typing: Vec<Typing>,
    pub write: Option<DosiRecord>,
    pub sync: Option<u64>,
    pub save: Option<SharedInstructions>,
}

impl Output {
    fn item(&mut self, item: &DosiItem) {
        self.events.push(json!({"type":"dosi.item","item":item}));
    }
}

pub(crate) type Answer<T> = futures_util::future::BoxFuture<'static, T>;

pub(crate) trait Runners: Send + Sync {
    fn installed(&self) -> Answer<Option<Vec<DosiAgent>>>;
    fn runner(
        &self,
        agent: Agent,
        account: Option<String>,
    ) -> Answer<std::result::Result<agents::Runner, String>>;
}

pub(crate) struct Installed {
    pub launcher: Arc<Launcher>,
    pub shell: PathBuf,
    pub home: PathBuf,
}

impl Runners for Installed {
    fn installed(&self) -> Answer<Option<Vec<DosiAgent>>> {
        let launcher = Arc::clone(&self.launcher);
        Box::pin(async move {
            let catalog = launcher.catalog().await.ok()?;
            Some(installed(&catalog))
        })
    }

    fn runner(
        &self,
        agent: Agent,
        account: Option<String>,
    ) -> Answer<std::result::Result<agents::Runner, String>> {
        let launcher = Arc::clone(&self.launcher);
        let (shell, home) = (self.shell.clone(), self.home.clone());
        Box::pin(async move { prepare(&launcher, &shell, &home, agent, account.as_deref()).await })
    }
}

fn installed(catalog: &agents::AgentCatalog) -> Vec<DosiAgent> {
    AGENTS
        .into_iter()
        .map(|agent| DosiAgent {
            agent,
            installed: catalog
                .agents
                .iter()
                .any(|info| info.agent == agent && info.installed),
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Working,
    Stopping,
}

enum Log {
    Local { next: u64 },
    Shared(Shared),
}

struct Remote {
    turn: String,
    computer: String,
    since: i64,
    last: i64,
    stopping: Option<Instant>,
}

impl Remote {
    fn live(&self) -> bool {
        conversation::now() - self.last < REMOTE_LIFE
            && self
                .stopping
                .is_none_or(|stopping| stopping.elapsed() < REMOTE_STOP)
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Key {
    conversation: String,
    agent: Agent,
    account: String,
}

impl Key {
    fn matches(&self, session: &ProviderSession) -> bool {
        session.conversation == self.conversation
            && session.agent == self.agent
            && session.account == self.account
    }
}

struct Running {
    child: Child,
    adapter: Adapter,
    key: Key,
    message: String,
    text: String,
    resuming: bool,
    heard: bool,
}

pub(crate) struct Engine {
    runners: Arc<dyn Runners>,
    store: Store,
    folder: PathBuf,
    messages: mpsc::Sender<Message>,
    setup: AgentChoice,
    sessions: Vec<ProviderSession>,
    agents: Vec<DosiAgent>,
    log: Log,
    generation: u64,
    conversation: Option<Conversation>,
    remote: Option<Remote>,
    phase: Phase,
    turns: u64,
    queue: VecDeque<DosiQueued>,
    process: Option<Running>,
    preparing: Option<(u64, Key, String, String)>,
    processes: u64,
    notes: Vec<String>,
    wake: bool,
    token: String,
    edited: bool,
    restart: bool,
    dirty: Vec<String>,
}

impl Engine {
    pub(crate) fn load(
        data_root: &Path,
        home: &Path,
        runners: Arc<dyn Runners>,
    ) -> (Self, mpsc::Receiver<Message>) {
        let (messages, receiver) = mpsc::channel(MESSAGES);
        let store = Store::open(data_root);
        let saved = store.load();
        let mut engine = Self {
            runners,
            folder: folder::path(home),
            messages,
            setup: saved.setup,
            sessions: saved.sessions,
            agents: Vec::new(),
            log: Log::Local { next: 1 },
            generation: 0,
            conversation: None,
            remote: None,
            phase: Phase::Idle,
            turns: 0,
            queue: VecDeque::new(),
            process: None,
            preparing: None,
            processes: 0,
            notes: Vec::new(),
            wake: false,
            token: Uuid::now_v7().to_string(),
            edited: store.instructions().is_some(),
            restart: false,
            dirty: Vec::new(),
            store,
        };
        engine.open(None);
        (engine, receiver)
    }

    pub(crate) fn sender(&self) -> mpsc::Sender<Message> {
        self.messages.clone()
    }

    pub(crate) const fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn share(
        &mut self,
        user: Option<Uuid>,
        computer: impl FnOnce() -> String,
        out: &mut Output,
    ) {
        let current = match &self.log {
            Log::Local { .. } => None,
            Log::Shared(shared) => Some(shared.user),
        };
        if current == user {
            return;
        }
        self.quiet();
        self.generation += 1;
        self.open(user.map(|user| (user, computer())));
        self.request_sync(out);
        out.snapshot = true;
    }

    fn open(&mut self, account: Option<(Uuid, String)>) {
        let book = account
            .as_ref()
            .map_or(Book::Local, |(user, _)| Book::Shared(*user));
        let log = self.store.log(book);
        self.log = match account {
            Some((user, computer)) => Log::Shared(Shared::new(
                user,
                computer,
                (log.sequence > 0).then_some(log.sequence),
            )),
            None => Log::Local {
                next: log.sequence + 1,
            },
        };
        self.conversation = None;
        let mut out = Output::default();
        for entry in log.entries {
            self.record(entry, true, &mut out);
        }
    }

    fn quiet(&mut self) {
        self.end_process();
        self.phase = Phase::Idle;
        self.preparing = None;
        self.queue.clear();
        self.remote = None;
        self.notes.clear();
        self.wake = false;
        self.dirty.clear();
        self.restart = false;
    }

    pub(crate) fn learn(&mut self, catalog: &agents::AgentCatalog, out: &mut Output) {
        let agents = installed(catalog);
        if agents != self.agents {
            self.agents = agents;
            out.snapshot = true;
        }
    }

    pub(crate) fn refresh_agents(&self) {
        let installed = self.runners.installed();
        let messages = self.messages.clone();
        tokio::spawn(async move {
            if let Some(agents) = installed.await {
                drop(messages.send(Message::Agents(agents)).await);
            }
        });
    }

    fn later(&self, delay: Duration, message: Message) {
        let messages = self.messages.clone();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            drop(messages.send(message).await);
        });
    }

    fn agent(&self) -> Option<Agent> {
        let installed = |agent: Agent| {
            self.agents.is_empty()
                || self
                    .agents
                    .iter()
                    .any(|known| known.agent == agent && known.installed)
        };
        self.setup
            .agent
            .filter(|agent| installed(*agent))
            .or_else(|| AGENTS.into_iter().find(|agent| installed(*agent)))
    }

    fn account(&self, agent: Agent) -> String {
        self.setup
            .pick(agent)
            .account
            .unwrap_or_else(|| "default".to_owned())
    }

    fn remote(&self) -> Option<&Remote> {
        self.remote.as_ref().filter(|remote| remote.live())
    }

    const fn ready(&self) -> bool {
        match &self.log {
            Log::Local { .. } => true,
            Log::Shared(shared) => shared.synced().is_some(),
        }
    }

    fn free(&self) -> bool {
        self.phase == Phase::Idle && self.remote().is_none() && self.ready()
    }

    fn computer(&self) -> String {
        match &self.log {
            Log::Local { .. } => String::new(),
            Log::Shared(shared) => shared.computer.clone(),
        }
    }

    pub(crate) fn snapshot(&self) -> Value {
        let state = match (self.phase, self.remote()) {
            (Phase::Stopping, _)
            | (
                Phase::Idle,
                Some(Remote {
                    stopping: Some(_), ..
                }),
            ) => DosiState::Stopping,
            (Phase::Working, _) | (Phase::Idle, Some(_)) => DosiState::Working,
            (Phase::Idle, None) if self.agent().is_none() => DosiState::Unavailable,
            (Phase::Idle, None) if !self.ready() => DosiState::Connecting,
            (Phase::Idle, None) => DosiState::Idle,
        };
        let (items, earlier) = self
            .conversation
            .as_ref()
            .map(Conversation::window)
            .unwrap_or_default();
        let snapshot = DosiSnapshot {
            state,
            setup: AgentChoice {
                agent: self.agent(),
                picks: AGENTS.map(|agent| self.setup.pick(agent)).to_vec(),
            },
            agents: self.agents.clone(),
            instructions_edited: self.edited,
            folder: self.folder.to_string_lossy().into_owned(),
            shared: matches!(self.log, Log::Shared(_)),
            remote: self.remote().map(|remote| DosiRemote {
                computer: remote.computer.clone(),
                since: remote.since,
            }),
            conversation: self
                .conversation
                .as_ref()
                .map(|conversation| DosiConversation {
                    id: conversation.id.clone(),
                    started_at: conversation.started_at,
                }),
            items,
            earlier,
            queued: self.queue.iter().cloned().collect(),
        };
        json!({"type":"dosi.snapshot","dosi":snapshot})
    }

    fn save(&self) {
        self.store.save(&Saved {
            setup: self.setup.clone(),
            sessions: self.sessions.clone(),
        });
    }

    fn instructions(&self) -> String {
        self.store
            .instructions()
            .unwrap_or_else(|| folder::DEFAULT_INSTRUCTIONS.to_owned())
    }

    pub(crate) fn send(
        &mut self,
        text: &str,
        context: Option<String>,
        out: &mut Output,
    ) -> Result<()> {
        let text = text.trim();
        if self.agent().is_none() {
            return Err(Error::Invalid(
                "Install Claude Code or Codex, then choose it for Dosi in Settings.".to_owned(),
            ));
        }
        if !self.free() {
            if self.queue.len() >= MAX_QUEUED {
                return Err(Error::Busy);
            }
            self.queue.push_back(DosiQueued {
                id: Uuid::now_v7().to_string(),
                text: text.to_owned(),
            });
            out.snapshot = true;
            return Ok(());
        }
        let message = self.compose(text, context);
        self.begin(Some(text), message, out);
        Ok(())
    }

    fn compose(&mut self, text: &str, context: Option<String>) -> String {
        self.wake = false;
        let mut parts = std::mem::take(&mut self.notes);
        parts.extend(context);
        parts.push(text.to_owned());
        parts.join("\n\n")
    }

    fn advance(&mut self, out: &mut Output) {
        if !self.free() {
            return;
        }
        if let Some(next) = self.queue.pop_front() {
            let message = self.compose(&next.text, None);
            self.begin(Some(&next.text), message, out);
        } else if self.wake {
            self.wake = false;
            let message = std::mem::take(&mut self.notes).join("\n\n");
            self.begin(None, message, out);
        }
    }

    fn begin(&mut self, person: Option<&str>, text: String, out: &mut Output) {
        let Some(agent) = self.agent() else {
            return;
        };
        if self.conversation.is_none() {
            let id = Uuid::now_v7().to_string();
            self.conversation = Some(Conversation::begun(id.clone()));
            self.append(DosiRecord::Begin { conversation: id }, out);
        }
        let computer = self.computer();
        let Some(conversation) = self.conversation.as_mut() else {
            return;
        };
        let turn = Uuid::now_v7().to_string();
        let closed = conversation.close_questions();
        let person = conversation.begin(turn.clone(), person);
        let id = conversation.id.clone();
        self.turns += 1;
        self.append(
            DosiRecord::Turn {
                conversation: id.clone(),
                turn,
                open: true,
                computer,
                at: conversation::now(),
            },
            out,
        );
        for item in closed.into_iter().chain(person) {
            out.item(&item);
            self.write(item, out);
        }
        self.phase = Phase::Working;
        self.beat_later();
        out.snapshot = true;
        let key = Key {
            conversation: id,
            agent,
            account: self.account(agent),
        };
        let message = self.message(&key, &text);
        let reused = self.process.as_ref().is_some_and(|running| {
            running.key == key
                && !running.adapter.per_turn()
                && running
                    .adapter
                    .message(&message)
                    .is_some_and(|line| running.child.write(line))
        });
        if reused {
            if let Some(running) = self.process.as_mut() {
                running.message = message;
                running.text = text;
            }
        } else {
            self.end_process();
            self.launch(key, message, text, out);
        }
    }

    fn beat_later(&self) {
        if matches!(self.log, Log::Shared(_)) {
            self.later(
                BEAT,
                Message::Beat {
                    generation: self.generation,
                    turns: self.turns,
                },
            );
        }
    }

    fn message(&mut self, key: &Key, text: &str) -> String {
        let Some(conversation) = &self.conversation else {
            return text.to_owned();
        };
        let latest = conversation.latest();
        let known = self
            .sessions
            .iter()
            .find(|session| key.matches(session))
            .filter(|session| session.session.is_some())
            .map(|session| session.seen);
        let items = known.map_or_else(
            || {
                conversation
                    .items()
                    .filter(|item| Some(item.turn.as_str()) != conversation.turn())
                    .collect()
            },
            |seen| conversation.foreign_since(seen),
        );
        let catch_up = context::catch_up(&items, known.is_none());
        if let Some(session) = self
            .sessions
            .iter_mut()
            .find(|session| key.matches(session))
        {
            session.seen = latest;
        } else {
            self.sessions.push(ProviderSession {
                conversation: key.conversation.clone(),
                agent: key.agent,
                account: key.account.clone(),
                session: None,
                seen: latest,
            });
            if self.sessions.len() > MAX_SESSIONS {
                self.sessions.remove(0);
            }
        }
        self.save();
        catch_up.map_or_else(
            || text.to_owned(),
            |catch_up| format!("{catch_up}\n\n{text}"),
        )
    }

    fn launch(&mut self, key: Key, message: String, text: String, out: &mut Output) {
        if let Err(error) = folder::prepare(&self.folder, &self.instructions()) {
            self.finish(
                true,
                Some(format!("Dosi's folder is not ready: {error}")),
                out,
            );
            return;
        }
        self.processes += 1;
        let process = self.processes;
        let account = self.setup.pick(key.agent).account;
        let runner = self.runners.runner(key.agent, account);
        self.preparing = Some((process, key, message, text));
        let messages = self.messages.clone();
        tokio::spawn(async move {
            let result = runner.await;
            drop(messages.send(Message::Prepared { process, result }).await);
        });
    }

    fn start(
        &mut self,
        runner: agents::Runner,
        key: Key,
        message: String,
        text: String,
        out: &mut Output,
    ) {
        let Some(adapter) = Adapter::new(key.agent) else {
            self.finish(true, None, out);
            return;
        };
        let session = self
            .sessions
            .iter_mut()
            .find(|session| key.matches(session));
        let known = session.as_ref().and_then(|session| session.session.clone());
        let resuming = known.is_some();
        let session_id = known.or_else(|| {
            (!adapter.per_turn()).then(|| {
                let id = Uuid::now_v7().to_string();
                if let Some(session) = session {
                    session.session = Some(id.clone());
                }
                id
            })
        });
        self.save();
        let pick = self.setup.pick(key.agent);
        let launch = adapter.launch(
            &Choice {
                session: session_id.as_deref(),
                resume: resuming,
                model: pick.model.as_deref(),
                effort: pick.effort.as_deref(),
            },
            &message,
        );
        let mut variables = runner
            .variables
            .into_iter()
            .map(|(name, value)| (name, OsString::from(value)))
            .collect::<Vec<_>>();
        variables.push((TOKEN_VARIABLE.to_owned(), OsString::from(&self.token)));
        let path = runner
            .path
            .or_else(|| std::env::var_os("PATH"))
            .map(|path| {
                let mut folders = std::env::split_paths(&path).collect::<Vec<_>>();
                if let Some(kodosi) = process::executable_folder()
                    && !folders.contains(&kodosi)
                {
                    folders.push(kodosi);
                }
                std::env::join_paths(folders).unwrap_or(path)
            });
        self.processes += 1;
        let spawned = process::spawn(
            self.processes,
            Spawn {
                program: &runner.program,
                arguments: &launch.arguments,
                directory: &self.folder,
                path,
                variables,
                input: launch.input,
            },
            self.messages.clone(),
        );
        match spawned {
            Ok(child) => {
                if let Some(line) = adapter.message(&message) {
                    child.write(line);
                }
                self.process = Some(Running {
                    child,
                    adapter,
                    key,
                    message,
                    text,
                    resuming,
                    heard: false,
                });
            }
            Err(error) => self.finish(true, Some(error.to_string()), out),
        }
    }

    pub(crate) fn handle(&mut self, message: Message, out: &mut Output) {
        match message {
            Message::Agents(agents) => {
                if agents != self.agents {
                    self.agents = agents;
                    out.snapshot = true;
                }
            }
            Message::Prepared { process, result } => self.prepared(process, result, out),
            Message::Line { process, line } => self.line(process, &line, out),
            Message::Exited { process, error } => self.exited(process, error, out),
            Message::Idle { process, turns } => {
                if self.phase == Phase::Idle && self.current(process) && self.turns == turns {
                    self.end_process();
                }
            }
            Message::Late { process, turns } => {
                if self.phase == Phase::Stopping
                    && self.current(process)
                    && self.turns == turns
                    && let Some(running) = self.process.as_mut()
                {
                    running.child.end();
                }
            }
            Message::Flush => self.flush(out),
            Message::Beat { generation, turns } => self.beat(generation, turns, out),
            Message::Written { generation, result } => self.written(generation, result, out),
            Message::Synced { generation, result } => self.synced(generation, result, out),
            Message::Saved {
                generation,
                instructions,
                result,
            } => self.saved(generation, instructions, result, out),
            Message::Retry { generation } => self.retry(generation, out),
            Message::Gone { generation, turn } => {
                if generation == self.generation
                    && self
                        .remote
                        .as_ref()
                        .is_some_and(|remote| remote.turn == turn && !remote.live())
                {
                    self.remote = None;
                    out.snapshot = true;
                    self.advance(out);
                }
            }
        }
    }

    fn prepared(
        &mut self,
        process: u64,
        result: std::result::Result<agents::Runner, String>,
        out: &mut Output,
    ) {
        if self.preparing.as_ref().map(|(id, ..)| *id) != Some(process) {
            return;
        }
        let Some((_, key, message, text)) = self.preparing.take() else {
            return;
        };
        if self.phase != Phase::Working {
            return;
        }
        match result {
            Ok(runner) => self.start(runner, key, message, text, out),
            Err(error) => self.finish(true, Some(error), out),
        }
    }

    fn flush(&mut self, out: &mut Output) {
        let Some(conversation) = &self.conversation else {
            return;
        };
        for id in self.dirty.drain(..) {
            if let Some(item) = conversation.item(&id) {
                out.item(item);
            }
        }
    }

    fn beat(&mut self, generation: u64, turns: u64, out: &mut Output) {
        if generation != self.generation || turns != self.turns || self.phase == Phase::Idle {
            return;
        }
        let computer = self.computer();
        let Some(conversation) = &self.conversation else {
            return;
        };
        let Some(turn) = conversation.turn() else {
            return;
        };
        let record = DosiRecord::Turn {
            conversation: conversation.id.clone(),
            turn: turn.to_owned(),
            open: true,
            computer,
            at: conversation::now(),
        };
        self.append(record, out);
        self.beat_later();
    }

    fn current(&self, process: u64) -> bool {
        self.process
            .as_ref()
            .is_some_and(|running| running.child.id == process)
    }

    fn line(&mut self, process: u64, line: &str, out: &mut Output) {
        let Some(running) = self
            .process
            .as_mut()
            .filter(|running| running.child.id == process)
        else {
            return;
        };
        running.heard = true;
        let events = running.adapter.parse(line);
        for event in events {
            self.event(event, out);
        }
    }

    fn event(&mut self, event: Event, out: &mut Output) {
        match event {
            Event::Session(session) => {
                let Some(key) = self.process.as_ref().map(|running| running.key.clone()) else {
                    return;
                };
                if let Some(known) = self
                    .sessions
                    .iter_mut()
                    .find(|known| key.matches(known))
                    .filter(|known| known.session.as_deref() != Some(session.as_str()))
                {
                    known.session = Some(session);
                    self.save();
                }
            }
            Event::Reply(line) => {
                if let Some(running) = &self.process {
                    running.child.write(line);
                }
            }
            Event::TurnEnd { failed, message } => self.finish(failed, message, out),
            event => {
                if self.phase == Phase::Idle {
                    return;
                }
                let Some(item) = self
                    .conversation
                    .as_mut()
                    .and_then(|conversation| conversation.apply(event))
                else {
                    return;
                };
                if matches!(
                    item.body,
                    DosiBody::Text { live: true, .. } | DosiBody::Reasoning { live: true, .. }
                ) {
                    self.mark(item.id);
                    return;
                }
                self.dirty.retain(|id| *id != item.id);
                out.item(&item);
                if !matches!(
                    item.body,
                    DosiBody::Tool {
                        status: DosiToolStatus::Running,
                        ..
                    }
                ) {
                    self.write(item, out);
                }
            }
        }
    }

    fn mark(&mut self, id: String) {
        if self.dirty.contains(&id) {
            return;
        }
        self.dirty.push(id);
        if self.dirty.len() == 1 {
            self.later(FLUSH_DELAY, Message::Flush);
        }
    }

    fn finish(&mut self, failed: bool, message: Option<String>, out: &mut Output) {
        if self.phase == Phase::Idle {
            return;
        }
        let interrupted = self.phase == Phase::Stopping;
        self.phase = Phase::Idle;
        self.preparing = None;
        let computer = self.computer();
        let Some(conversation) = self.conversation.as_mut() else {
            return;
        };
        let mut settled = conversation.settle(interrupted);
        self.dirty
            .retain(|id| !settled.iter().any(|item| item.id == *id));
        let notice = if interrupted {
            Some(DosiBody::Notice {
                tone: DosiTone::Stopped,
                text: None,
            })
        } else if failed {
            Some(DosiBody::Notice {
                tone: message
                    .as_deref()
                    .map_or(DosiTone::Failed, conversation::tone),
                text: message.map(|text| conversation::clip(text.trim(), 2000)),
            })
        } else {
            None
        };
        if let Some(body) = notice {
            settled.push(conversation.push(body));
        }
        let closing = conversation.turn().map(|turn| DosiRecord::Turn {
            conversation: conversation.id.clone(),
            turn: turn.to_owned(),
            open: false,
            computer,
            at: conversation::now(),
        });
        conversation.end();
        for item in settled {
            out.item(&item);
            self.write(item, out);
        }
        if let Some(closing) = closing {
            self.append(closing, out);
        }
        out.snapshot = true;
        if std::mem::take(&mut self.restart) {
            self.end_process();
        }
        if !self.queue.is_empty() || self.wake {
            self.advance(out);
        } else if let Some(running) = self
            .process
            .as_ref()
            .filter(|running| !running.adapter.per_turn())
        {
            self.later(
                IDLE_LIFE,
                Message::Idle {
                    process: running.child.id,
                    turns: self.turns,
                },
            );
        }
    }

    fn exited(&mut self, process: u64, error: Option<String>, out: &mut Output) {
        if !self.current(process) {
            return;
        }
        let Some(mut running) = self.process.take() else {
            return;
        };
        if self.phase == Phase::Idle {
            return;
        }
        let missing = error.as_deref().is_some_and(|error| {
            let lower = error.to_ascii_lowercase();
            lower.contains("no conversation found") || lower.contains("session not found")
        });
        if running.resuming && !running.heard && missing && self.phase == Phase::Working {
            if let Some(session) = self
                .sessions
                .iter_mut()
                .find(|session| running.key.matches(session))
            {
                session.session = None;
            }
            let message = self.message(&running.key, &running.text);
            self.launch(running.key, message, running.text, out);
            return;
        }
        match running.adapter.ended() {
            Some(Event::TurnEnd { failed, message }) => {
                self.finish(failed, message.or(error), out);
            }
            _ => self.finish(true, error, out),
        }
    }

    pub(crate) fn stop(&mut self, out: &mut Output) {
        if self.phase == Phase::Idle {
            self.stop_remote(out);
            return;
        }
        if self.phase != Phase::Working {
            return;
        }
        self.phase = Phase::Stopping;
        out.snapshot = true;
        if self.preparing.take().is_some() {
            self.finish(false, None, out);
            return;
        }
        let turns = self.turns;
        let Some(running) = self.process.as_mut() else {
            self.finish(false, None, out);
            return;
        };
        let asked = running
            .adapter
            .interrupt()
            .is_some_and(|line| running.child.write(line));
        if asked {
            let process = running.child.id;
            self.later(STOP_GRACE, Message::Late { process, turns });
        } else {
            running.child.end();
        }
    }

    fn stop_remote(&mut self, out: &mut Output) {
        let Some(remote) = self
            .remote
            .as_mut()
            .filter(|remote| remote.live() && remote.stopping.is_none())
        else {
            return;
        };
        remote.stopping = Some(Instant::now());
        let turn = remote.turn.clone();
        self.append(DosiRecord::Stop { turn: turn.clone() }, out);
        self.later(
            REMOTE_STOP,
            Message::Gone {
                generation: self.generation,
                turn,
            },
        );
        out.snapshot = true;
    }

    pub(crate) fn unqueue(&mut self, id: &str, out: &mut Output) -> Result<()> {
        let before = self.queue.len();
        self.queue.retain(|queued| queued.id != id);
        if self.queue.len() == before {
            return Err(Error::Invalid("That message was already sent.".to_owned()));
        }
        out.snapshot = true;
        Ok(())
    }

    pub(crate) fn answer(
        &mut self,
        item: &str,
        choices: Vec<String>,
        out: &mut Output,
    ) -> Result<()> {
        let conversation = self
            .conversation
            .as_mut()
            .ok_or_else(|| Error::Invalid("Dosi has no conversation.".to_owned()))?;
        let answered = conversation
            .answer(item, choices)
            .map_err(|reason| Error::Invalid(reason.to_owned()))?;
        out.item(&answered);
        self.write(answered.clone(), out);
        let DosiBody::Question {
            question,
            options,
            keys,
            terminal,
            answer: Some(choices),
            ..
        } = answered.body
        else {
            return Ok(());
        };
        let (picked, own): (Vec<_>, Vec<_>) = choices
            .into_iter()
            .partition(|choice| options.contains(choice));
        let with = if picked.is_empty() {
            String::new()
        } else {
            format!(" with {}", picked.join(", "))
        };
        let words = own.first().map_or_else(String::new, |own| {
            let and = if picked.is_empty() { "" } else { " and" };
            format!("{and} in their own words: \"{own}\"")
        });
        let note = format!("The person answered \"{question}\"{with}{words}");
        if let Some(terminal) = terminal {
            let typed = picked
                .iter()
                .map(|choice| {
                    let index = options.iter().position(|option| option == choice);
                    index.and_then(|index| keys.get(index)).unwrap_or(choice)
                })
                .collect::<Vec<_>>();
            for text in &typed {
                out.typing.push(Typing {
                    terminal: terminal.clone(),
                    text: format!("{text}\r"),
                });
            }
            let typed = if typed.is_empty() {
                "nothing".to_owned()
            } else {
                typed
                    .iter()
                    .map(|text| format!("\"{text}\" and Enter"))
                    .collect::<Vec<_>>()
                    .join(", then ")
            };
            self.notes.push(format!(
                "{note}. Kodosi typed {typed} in the terminal {terminal}."
            ));
            if own.is_empty() {
                return Ok(());
            }
        } else {
            self.notes.push(format!("{note}."));
        }
        self.wake = true;
        self.advance(out);
        Ok(())
    }

    pub(crate) fn ask(
        &mut self,
        token: &str,
        question: String,
        options: Vec<String>,
        keys: Vec<String>,
        multiple: bool,
        terminal: Option<String>,
        out: &mut Output,
    ) -> Result<()> {
        if token != self.token || self.phase != Phase::Working {
            return Err(Error::Invalid(
                "Only Dosi in the Kodosi app asks with buttons. Ask in your answer.".to_owned(),
            ));
        }
        let conversation = self
            .conversation
            .as_mut()
            .ok_or_else(|| Error::Invalid("Dosi has no conversation.".to_owned()))?;
        let item = conversation.push(DosiBody::Question {
            question,
            options,
            keys,
            multiple,
            terminal,
            answer: None,
        });
        out.item(&item);
        self.write(item, out);
        Ok(())
    }

    pub(crate) fn renew(&mut self, out: &mut Output) {
        self.quiet();
        let id = Uuid::now_v7().to_string();
        self.conversation = Some(Conversation::begun(id.clone()));
        self.append(DosiRecord::Begin { conversation: id }, out);
        out.snapshot = true;
    }

    pub(crate) fn configure(&mut self, change: &AgentChoiceChange, out: &mut Output) -> Result<()> {
        let agent = match change {
            AgentChoiceChange::Tab { agent } => *agent,
            AgentChoiceChange::Pick(pick) => Some(pick.agent),
        };
        if agent.is_some_and(|agent| !AGENTS.contains(&agent)) {
            return Err(Error::Invalid("Dosi cannot run on that agent.".to_owned()));
        }
        let used = |engine: &Self| engine.agent().map(|agent| engine.setup.pick(agent));
        let before = used(self);
        self.setup.apply(change);
        if used(self) != before {
            self.restart_process();
        }
        self.save();
        self.refresh_agents();
        out.snapshot = true;
        Ok(())
    }

    fn restart_process(&mut self) {
        if self.phase == Phase::Idle {
            self.end_process();
        } else {
            self.restart = true;
        }
    }

    pub(crate) fn read_instructions(&self) -> DosiInstructions {
        DosiInstructions {
            text: self.instructions(),
            edited: self.edited,
        }
    }

    pub(crate) fn write_instructions(
        &mut self,
        text: Option<&str>,
        out: &mut Output,
    ) -> Result<DosiInstructions> {
        let text = text
            .map(str::trim)
            .filter(|text| !text.is_empty() && *text != folder::DEFAULT_INSTRUCTIONS.trim())
            .map(str::to_owned);
        self.adopt_instructions(text.as_deref(), out)?;
        if let Log::Shared(shared) = &mut self.log {
            out.save = shared.save(SharedInstructions { text });
        }
        Ok(self.read_instructions())
    }

    fn adopt_instructions(&mut self, text: Option<&str>, out: &mut Output) -> Result<()> {
        if self.store.instructions().as_deref() == text {
            return Ok(());
        }
        self.store.set_instructions(text)?;
        self.edited = text.is_some();
        folder::prepare(&self.folder, &self.instructions())?;
        self.restart_process();
        out.snapshot = true;
        Ok(())
    }

    pub(crate) fn page(&self, before: &str) -> DosiPage {
        self.conversation.as_ref().map_or_else(
            || DosiPage {
                items: Vec::new(),
                earlier: false,
            },
            |conversation| conversation.page(before),
        )
    }

    fn end_process(&mut self) {
        if let Some(mut running) = self.process.take() {
            running.child.end();
        }
    }

    pub(crate) fn shutdown(&mut self) {
        if let Some(running) = self.process.take() {
            running.child.end_now();
        }
    }

    fn write(&mut self, item: DosiItem, out: &mut Output) {
        let Some(conversation) = &self.conversation else {
            return;
        };
        let record = DosiRecord::Item {
            conversation: conversation.id.clone(),
            item,
        };
        self.append(record, out);
    }

    fn append(&mut self, record: DosiRecord, out: &mut Output) {
        match &mut self.log {
            Log::Local { next } => {
                let entry = DosiEntry {
                    sequence: *next,
                    mine: true,
                    record,
                };
                *next += 1;
                self.store.append(Book::Local, std::slice::from_ref(&entry));
                self.settle(entry);
            }
            Log::Shared(shared) => {
                if let Some(record) = shared.push(record) {
                    out.write = Some(record);
                }
            }
        }
    }

    fn settle(&mut self, entry: DosiEntry) {
        if let DosiRecord::Item { conversation, item } = entry.record
            && let Some(current) = self
                .conversation
                .as_mut()
                .filter(|current| current.id == conversation)
        {
            current.upsert(item, entry.sequence, true, false);
        }
    }

    fn request_sync(&mut self, out: &mut Output) {
        if let Log::Shared(shared) = &mut self.log
            && let Some(after) = shared.sync()
        {
            out.sync = Some(after);
        }
    }

    pub(crate) fn nudge(&mut self, sequence: Option<u64>, out: &mut Output) {
        if let Log::Shared(shared) = &mut self.log
            && let Some(after) = shared.hear(sequence)
        {
            out.sync = Some(after);
        }
    }

    fn retry_later(&mut self) {
        let Log::Shared(shared) = &mut self.log else {
            return;
        };
        let delay = shared.backoff();
        self.later(
            delay,
            Message::Retry {
                generation: self.generation,
            },
        );
    }

    fn retry(&mut self, generation: u64, out: &mut Output) {
        if generation != self.generation {
            return;
        }
        let Log::Shared(shared) = &mut self.log else {
            return;
        };
        let (write, sync, save) = shared.retry();
        if write.is_some() {
            out.write = write;
        }
        if sync.is_some() {
            out.sync = sync;
        }
        if save.is_some() {
            out.save = save;
        }
    }

    fn written(
        &mut self,
        generation: u64,
        result: std::result::Result<u64, Failure>,
        out: &mut Output,
    ) {
        if generation != self.generation {
            return;
        }
        let Log::Shared(shared) = &mut self.log else {
            return;
        };
        match result {
            Ok(sequence) => {
                let written = shared.written(sequence);
                let book = Book::Shared(shared.user);
                if written.write.is_some() {
                    out.write = written.write;
                }
                if written.sync.is_some() {
                    out.sync = written.sync;
                }
                if let Some(record) = written.settled {
                    let entry = DosiEntry {
                        sequence,
                        mine: true,
                        record,
                    };
                    self.store.append(book, std::slice::from_ref(&entry));
                    self.settle(entry);
                }
            }
            Err(failure) => {
                tracing::debug!(
                    message = failure.message,
                    "a Dosi record waits for the server"
                );
                if let Some(next) = shared.write_failed(failure.retry) {
                    out.write = Some(next);
                }
                if failure.retry {
                    self.retry_later();
                }
            }
        }
    }

    fn synced(
        &mut self,
        generation: u64,
        result: std::result::Result<DosiLog, Failure>,
        out: &mut Output,
    ) {
        if generation != self.generation {
            return;
        }
        let Log::Shared(shared) = &mut self.log else {
            return;
        };
        let log = match result {
            Ok(log) => log,
            Err(failure) => {
                tracing::debug!(
                    message = failure.message,
                    "Dosi's conversation waits for the server"
                );
                shared.sync_failed();
                self.retry_later();
                return;
            }
        };
        let book = Book::Shared(shared.user);
        if log.reset {
            shared.reset();
            self.store.clear(book);
            self.quiet();
            self.conversation = None;
            out.snapshot = true;
            self.request_sync(out);
            return;
        }
        let known = shared.synced().unwrap_or_default();
        let again = shared.synced_to(log.sequence);
        let saving = shared.saving();
        let entries = log
            .entries
            .into_iter()
            .filter(|entry| entry.sequence > known)
            .collect::<Vec<_>>();
        self.store.append(book, &entries);
        for entry in entries {
            self.record(entry, false, out);
        }
        if let Some(instructions) = log.instructions.filter(|_| !saving)
            && let Err(error) = self.adopt_instructions(instructions.text.as_deref(), out)
        {
            tracing::warn!(%error, "Dosi's instructions from another computer were not kept");
        }
        if again {
            self.request_sync(out);
        }
        out.snapshot = true;
        self.advance(out);
    }

    fn saved(
        &mut self,
        generation: u64,
        instructions: SharedInstructions,
        result: std::result::Result<(), Failure>,
        out: &mut Output,
    ) {
        if generation != self.generation {
            return;
        }
        let Log::Shared(shared) = &mut self.log else {
            return;
        };
        match result {
            Ok(()) => out.save = shared.saved(None),
            Err(failure) if failure.retry => {
                shared.saved(Some(instructions));
                self.retry_later();
            }
            Err(failure) => {
                tracing::warn!(
                    message = failure.message,
                    "Dosi's instructions were not shared"
                );
                out.save = shared.saved(None);
            }
        }
    }

    fn record(&mut self, entry: DosiEntry, restoring: bool, out: &mut Output) {
        let DosiEntry {
            sequence,
            mine,
            record,
        } = entry;
        match record {
            DosiRecord::Begin { conversation } => {
                if self
                    .conversation
                    .as_ref()
                    .is_none_or(|current| current.id != conversation)
                {
                    self.adopt(conversation, out);
                }
                if let Some(current) = self.conversation.as_mut() {
                    current.confirmed = true;
                }
            }
            DosiRecord::Turn {
                conversation,
                turn,
                open,
                computer,
                at,
            } => {
                if !self.belongs(&conversation, out) || mine {
                    return;
                }
                if open {
                    self.remote_turn(turn, computer, at, out);
                } else if self
                    .remote
                    .as_ref()
                    .is_some_and(|remote| remote.turn == turn)
                {
                    self.remote = None;
                    out.snapshot = true;
                }
            }
            DosiRecord::Stop { turn } => {
                let running =
                    self.conversation.as_ref().and_then(Conversation::turn) == Some(turn.as_str());
                if !mine && running && self.phase == Phase::Working {
                    self.stop(out);
                }
            }
            DosiRecord::Item { conversation, item } => {
                if !self.belongs(&conversation, out) {
                    return;
                }
                if let Some(remote) = self
                    .remote
                    .as_mut()
                    .filter(|remote| remote.turn == item.turn)
                {
                    remote.last = remote.last.max(item.at);
                }
                let id = item.id.clone();
                if let Some(current) = self.conversation.as_mut()
                    && current.upsert(item, sequence, mine, restoring)
                    && let Some(item) = current.item(&id)
                {
                    out.item(item);
                }
            }
        }
    }

    fn belongs(&mut self, conversation: &str, out: &mut Output) -> bool {
        match &self.conversation {
            Some(current) if current.id == conversation => true,
            Some(current) if current.confirmed || current.id.as_str() > conversation => false,
            _ => {
                self.adopt(conversation.to_owned(), out);
                true
            }
        }
    }

    fn adopt(&mut self, id: String, out: &mut Output) {
        if self.phase != Phase::Idle {
            self.end_process();
            self.phase = Phase::Idle;
            self.preparing = None;
        }
        self.remote = None;
        let started = conversation::time_of(&id).unwrap_or_else(conversation::now);
        self.conversation = Some(Conversation::new(id, started));
        out.snapshot = true;
    }

    fn remote_turn(&mut self, turn: String, computer: String, at: i64, out: &mut Output) {
        match self.remote.as_mut().filter(|remote| remote.turn == turn) {
            Some(remote) => remote.last = remote.last.max(at),
            None => {
                self.remote = Some(Remote {
                    since: conversation::time_of(&turn).unwrap_or(at),
                    turn: turn.clone(),
                    computer,
                    last: at,
                    stopping: None,
                });
            }
        }
        let left = self
            .remote
            .as_ref()
            .filter(|remote| remote.live())
            .map(|remote| REMOTE_LIFE - (conversation::now() - remote.last));
        match left {
            Some(left) => {
                let wait = Duration::from_millis(u64::try_from(left).unwrap_or_default())
                    + Duration::from_secs(1);
                self.later(
                    wait,
                    Message::Gone {
                        generation: self.generation,
                        turn,
                    },
                );
            }
            None => self.remote = None,
        }
        out.snapshot = true;
    }
}

async fn prepare(
    launcher: &Launcher,
    shell: &Path,
    home: &Path,
    agent: Agent,
    account: Option<&str>,
) -> std::result::Result<agents::Runner, String> {
    let catalog = launcher.catalog().await?;
    let info = catalog
        .agents
        .iter()
        .find(|info| info.agent == agent)
        .filter(|info| info.installed)
        .ok_or_else(|| format!("{} is not installed on this computer.", agent.label()))?;
    let account = account
        .and_then(|name| info.accounts.iter().find(|account| account.name == name))
        .or_else(|| info.accounts.first())
        .ok_or_else(|| format!("{} has no account.", agent.label()))?;
    agents::runner(shell, home, agent, account)
        .await
        .ok_or_else(|| format!("{} is not installed on this computer.", agent.label()))
}
