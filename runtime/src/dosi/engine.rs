use std::{
    collections::VecDeque,
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use serde_json::{Value, json};
use tokio::sync::mpsc;
use uuid::Uuid;

use super::{
    AGENTS, DosiAgent, DosiBody, DosiComputer, DosiInstructions, DosiItem, DosiPage, DosiQueued,
    DosiRecord, DosiSnapshot, DosiState, DosiTone, DosiToolStatus, SharedInstructions,
    TOKEN_VARIABLE,
    adapters::{Adapter, Choice, Event},
    chat::{Account, Chat, Heard, Message, Output},
    context, conversation, folder,
    process::{self, Child, Spawn},
    store::{ProviderSession, Saved, Store},
};
use crate::{
    Error, Result,
    agents::{self, Agent, AgentChoice, AgentChoiceChange, Launcher},
};

const IDLE_LIFE: Duration = Duration::from_mins(10);
const STOP_GRACE: Duration = Duration::from_secs(6);
const FLUSH_DELAY: Duration = Duration::from_millis(80);
const BEAT: Duration = Duration::from_mins(3);
const MAX_QUEUED: usize = super::chat::MAX_WAITING;
const MAX_SESSIONS: usize = 8;

/// What the work of this computer's Dosi tells it: its agent's process and its timers.
pub(crate) enum Run {
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
}

pub(crate) struct Typing {
    pub terminal: String,
    pub text: String,
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
    chat: Chat,
    store: Store,
    folder: PathBuf,
    saved: Saved,
    agents: Vec<DosiAgent>,
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
        let (chat, receiver) = Chat::load(data_root);
        let store = Store::open(data_root);
        let engine = Self {
            runners,
            chat,
            folder: folder::path(home),
            saved: store.load(),
            agents: Vec::new(),
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
        (engine, receiver)
    }

    pub(crate) fn sender(&self) -> mpsc::Sender<Message> {
        self.chat.sender()
    }

    pub(crate) const fn generation(&self) -> u64 {
        self.chat.generation()
    }

    pub(crate) fn share(
        &mut self,
        account: Option<(Uuid, String)>,
        computer: impl FnOnce() -> String,
        out: &mut Output,
    ) {
        let account = account.map(|(user, device)| Account {
            user,
            device,
            computer: String::new(),
        });
        if !self.chat.differs(account.as_ref()) {
            return;
        }
        self.quiet();
        let account = account.map(|account| Account {
            computer: computer(),
            ..account
        });
        self.chat.share(account, out);
    }

    fn quiet(&mut self) {
        self.end_process();
        self.phase = Phase::Idle;
        self.preparing = None;
        self.queue.clear();
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
        let messages = self.chat.sender();
        tokio::spawn(async move {
            if let Some(agents) = installed.await {
                drop(messages.send(Message::Run(Run::Agents(agents))).await);
            }
        });
    }

    fn later(&self, delay: Duration, run: Run) {
        self.chat.later(delay, Message::Run(run));
    }

    fn agent(&self) -> Option<Agent> {
        let installed = |agent: Agent| {
            self.agents.is_empty()
                || self
                    .agents
                    .iter()
                    .any(|known| known.agent == agent && known.installed)
        };
        self.saved
            .setup
            .agent
            .filter(|agent| installed(*agent))
            .or_else(|| AGENTS.into_iter().find(|agent| installed(*agent)))
    }

    fn account(&self, agent: Agent) -> String {
        self.saved
            .setup
            .pick(agent)
            .account
            .unwrap_or_else(|| "default".to_owned())
    }

    fn free(&self) -> bool {
        self.phase == Phase::Idle && self.chat.remote().is_none() && self.chat.ready()
    }

    pub(crate) fn snapshot(&self) -> Value {
        let state = match (self.phase, self.chat.remote()) {
            _ if !self.saved.on => DosiState::Off,
            (Phase::Stopping, _) => DosiState::Stopping,
            (Phase::Idle, Some(remote)) if remote.stopping() => DosiState::Stopping,
            (Phase::Working, _) | (Phase::Idle, Some(_)) => DosiState::Working,
            (Phase::Idle, None) if self.agent().is_none() => DosiState::Unavailable,
            (Phase::Idle, None) if !self.chat.ready() => DosiState::Connecting,
            (Phase::Idle, None) => DosiState::Idle,
        };
        let (conversation, items, earlier) = self.chat.conversation_snapshot();
        let snapshot = DosiSnapshot {
            state,
            setup: AgentChoice {
                agent: self.agent(),
                picks: AGENTS.map(|agent| self.saved.setup.pick(agent)).to_vec(),
            },
            agents: self.agents.clone(),
            instructions_edited: self.edited,
            folder: self.folder.to_string_lossy().into_owned(),
            shared: self.chat.shared(),
            remote: self.chat.remote_snapshot(),
            conversation,
            items,
            earlier,
            queued: self.queue.iter().cloned().collect(),
            asks: self.chat.waiting(),
            last: self.chat.last(),
        };
        json!({"type":"dosi.snapshot","dosi":snapshot})
    }

    fn save(&self) {
        self.store.save(&self.saved);
    }

    fn instructions(&self) -> String {
        self.store
            .instructions()
            .unwrap_or_else(|| folder::DEFAULT_INSTRUCTIONS.to_owned())
    }

    /// Sends a message to Dosi here, or to Dosi on `computer` when it is another computer.
    pub(crate) fn send(
        &mut self,
        text: &str,
        context: Option<String>,
        computer: Option<DosiComputer>,
        out: &mut Output,
    ) -> Result<()> {
        let text = text.trim();
        if let Some(computer) = computer.filter(|computer| computer.device != self.chat.device()) {
            return self.chat.ask(text, context, computer, out);
        }
        if !self.saved.on {
            return Err(Error::Refused(text!(
                "Dosi is off on this computer. Turn it on in Settings."
            )));
        }
        if self.agent().is_none() {
            return Err(Error::Refused(text!(
                "Install Claude Code or Codex, then choose it for Dosi in Settings."
            )));
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
        self.begin(None, Some(text), message, out);
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
        while self.free() {
            if self.saved.on
                && let Some(next) = self.queue.pop_front()
            {
                let message = self.compose(&next.text, None);
                self.begin(None, Some(&next.text), message, out);
            } else if let Some(ask) = self.chat.asked_here() {
                let (turn, text, context) =
                    (ask.turn.clone(), ask.text.clone(), ask.context.clone());
                self.chat.take(&turn);
                if let Some(reason) = self.refusal() {
                    self.refuse(&turn, &reason, out);
                    continue;
                }
                let message = self.compose(&text, context);
                self.begin(Some(turn), None, message, out);
            } else if self.saved.on && self.wake {
                self.wake = false;
                let message = std::mem::take(&mut self.notes).join("\n\n");
                self.begin(None, None, message, out);
            }
            return;
        }
    }

    fn has_work(&self) -> bool {
        (self.saved.on && (!self.queue.is_empty() || self.wake)) || self.chat.asked_here().is_some()
    }

    /// Why Dosi here does not answer a message that another device sent to this computer.
    fn refusal(&self) -> Option<crate::text::Text> {
        if !self.saved.on {
            return Some(text!(
                "Dosi is off on {computer}. Turn it on in Settings there.",
                computer = self.chat.computer()
            ));
        }
        self.agent()
            .is_none()
            .then(|| text!("Install Claude Code or Codex, then choose it for Dosi in Settings."))
    }

    /// Answers a message for this computer with why Dosi here does not run it.
    fn refuse(&mut self, turn: &str, reason: &crate::text::Text, out: &mut Output) {
        let computer = self.chat.computer();
        let conversation = self.chat.current(out);
        let id = conversation.id.clone();
        let notice = DosiItem {
            id: Uuid::now_v7().to_string(),
            turn: turn.to_owned(),
            at: conversation::now(),
            body: DosiBody::Notice {
                tone: DosiTone::Failed,
                text: Some(reason.to_string()),
            },
        };
        conversation.place(notice.clone());
        for open in [true, false] {
            if !open {
                out.item(&notice);
                self.chat.write(notice.clone(), out);
            }
            self.chat.append(
                DosiRecord::Turn {
                    conversation: id.clone(),
                    turn: turn.to_owned(),
                    open,
                    computer: computer.clone(),
                    at: conversation::now(),
                },
                out,
            );
        }
        out.snapshot = true;
    }

    fn begin(
        &mut self,
        turn: Option<String>,
        person: Option<&str>,
        text: String,
        out: &mut Output,
    ) {
        let Some(agent) = self.agent().filter(|_| self.saved.on) else {
            return;
        };
        let computer = self.chat.computer();
        let conversation = self.chat.current(out);
        let turn = turn.unwrap_or_else(|| Uuid::now_v7().to_string());
        let closed = conversation.close_questions(false);
        let person = conversation.begin(turn.clone(), person);
        let id = conversation.id.clone();
        self.turns += 1;
        self.chat.append(
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
            self.chat.write(item, out);
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
        if self.chat.shared() {
            self.later(
                BEAT,
                Run::Beat {
                    generation: self.chat.generation(),
                    turns: self.turns,
                },
            );
        }
    }

    fn message(&mut self, key: &Key, text: &str) -> String {
        let Some(conversation) = self.chat.conversation() else {
            return text.to_owned();
        };
        let latest = conversation.latest();
        let known = self
            .saved
            .sessions
            .iter()
            .find(|session| key.matches(session))
            .filter(|session| session.session.is_some())
            .map(|session| session.seen);
        let earlier = |item: &&DosiItem| Some(item.turn.as_str()) != conversation.turn();
        let items: Vec<_> = known.map_or_else(
            || conversation.items().filter(earlier).collect(),
            |seen| {
                conversation
                    .foreign_since(seen)
                    .into_iter()
                    .filter(earlier)
                    .collect()
            },
        );
        let catch_up = context::catch_up(&items, known.is_none());
        if let Some(session) = self
            .saved
            .sessions
            .iter_mut()
            .find(|session| key.matches(session))
        {
            session.seen = latest;
        } else {
            self.saved.sessions.push(ProviderSession {
                conversation: key.conversation.clone(),
                agent: key.agent,
                account: key.account.clone(),
                session: None,
                seen: latest,
            });
            if self.saved.sessions.len() > MAX_SESSIONS {
                self.saved.sessions.remove(0);
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
        let account = self.saved.setup.pick(key.agent).account;
        let runner = self.runners.runner(key.agent, account);
        self.preparing = Some((process, key, message, text));
        let messages = self.chat.sender();
        tokio::spawn(async move {
            let result = runner.await;
            drop(
                messages
                    .send(Message::Run(Run::Prepared { process, result }))
                    .await,
            );
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
            .saved
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
        let pick = self.saved.setup.pick(key.agent);
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
            self.chat.sender(),
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
            Message::Written { generation, result } => self.chat.written(generation, result, out),
            Message::Synced { generation, result } => {
                let Some((heard, instructions)) = self.chat.synced(generation, result, out) else {
                    return;
                };
                self.heard(heard, out);
                if let Some(instructions) = instructions
                    && let Err(error) = self.adopt_instructions(instructions.text.as_deref(), out)
                {
                    tracing::warn!(%error, "Dosi's instructions from another computer were not kept");
                }
                self.advance(out);
            }
            Message::Saved {
                generation,
                instructions,
                result,
            } => self.chat.saved(generation, instructions, result, out),
            Message::Retry { generation } => self.chat.retry(generation, out),
            Message::Gone { generation, turn } => {
                if self.chat.gone(generation, &turn, out) {
                    self.advance(out);
                }
            }
            Message::Run(run) => self.run(run, out),
        }
    }

    fn heard(&mut self, heard: Vec<Heard>, out: &mut Output) {
        for heard in heard {
            match heard {
                Heard::Reset => self.quiet(),
                Heard::Adopted => {
                    if self.phase != Phase::Idle {
                        self.end_process();
                        self.phase = Phase::Idle;
                        self.preparing = None;
                    }
                }
                Heard::Stopped(turn) => {
                    let running = self
                        .chat
                        .conversation()
                        .and_then(conversation::Conversation::turn)
                        == Some(turn.as_str());
                    if running && self.phase == Phase::Working {
                        self.stop(out);
                    }
                }
                Heard::Answered(item) => self.go_on(item, out),
            }
        }
    }

    fn run(&mut self, run: Run, out: &mut Output) {
        match run {
            Run::Agents(agents) => {
                if agents != self.agents {
                    self.agents = agents;
                    out.snapshot = true;
                }
            }
            Run::Prepared { process, result } => self.prepared(process, result, out),
            Run::Line { process, line } => self.line(process, &line, out),
            Run::Exited { process, error } => self.exited(process, error, out),
            Run::Idle { process, turns } => {
                if self.phase == Phase::Idle && self.current(process) && self.turns == turns {
                    self.end_process();
                }
            }
            Run::Late { process, turns } => {
                if self.phase == Phase::Stopping
                    && self.current(process)
                    && self.turns == turns
                    && let Some(running) = self.process.as_mut()
                {
                    running.child.end();
                }
            }
            Run::Flush => self.flush(out),
            Run::Beat { generation, turns } => self.beat(generation, turns, out),
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
        let Some(conversation) = self.chat.conversation() else {
            return;
        };
        for id in self.dirty.drain(..) {
            if let Some(item) = conversation.item(&id) {
                out.item(item);
            }
        }
    }

    fn beat(&mut self, generation: u64, turns: u64, out: &mut Output) {
        if generation != self.chat.generation() || turns != self.turns || self.phase == Phase::Idle
        {
            return;
        }
        let computer = self.chat.computer();
        let Some(conversation) = self.chat.conversation() else {
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
        self.chat.append(record, out);
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
                    .saved
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
                    .chat
                    .conversation_mut()
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
                    self.chat.write(item, out);
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
            self.later(FLUSH_DELAY, Run::Flush);
        }
    }

    fn finish(&mut self, failed: bool, message: Option<String>, out: &mut Output) {
        if self.phase == Phase::Idle {
            return;
        }
        let interrupted = self.phase == Phase::Stopping;
        self.phase = Phase::Idle;
        self.preparing = None;
        let computer = self.chat.computer();
        let Some(conversation) = self.chat.conversation_mut() else {
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
            self.chat.write(item, out);
        }
        if let Some(closing) = closing {
            self.chat.append(closing, out);
        }
        out.snapshot = true;
        if std::mem::take(&mut self.restart) {
            self.end_process();
        }
        if self.has_work() {
            self.advance(out);
        } else if let Some(running) = self
            .process
            .as_ref()
            .filter(|running| !running.adapter.per_turn())
        {
            self.later(
                IDLE_LIFE,
                Run::Idle {
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
                .saved
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
            self.chat.stop_remote(out);
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
            self.later(STOP_GRACE, Run::Late { process, turns });
        } else {
            running.child.end();
        }
    }

    /// Takes back a message that waits here or for another computer.
    pub(crate) fn unqueue(&mut self, id: &str, out: &mut Output) -> Result<()> {
        let before = self.queue.len();
        self.queue.retain(|queued| queued.id != id);
        if self.queue.len() < before {
            out.snapshot = true;
            return Ok(());
        }
        if self.chat.take_back(id, out) {
            return Ok(());
        }
        Err(Error::Refused(text!("You sent that message already.")))
    }

    /// Answers a question. When Dosi asked it here, Dosi goes on with the answer here; when it
    /// asked on another computer, Dosi goes on there.
    pub(crate) fn answer(
        &mut self,
        item: &str,
        choices: Vec<String>,
        out: &mut Output,
    ) -> Result<()> {
        let (answered, here) = self.chat.answer(item, choices, out)?;
        if here {
            self.go_on(answered, out);
            self.advance(out);
        }
        Ok(())
    }

    /// Takes the answer to a question that Dosi asked here into Dosi's next turn, and types the
    /// chosen keys in the terminal that asked.
    fn go_on(&mut self, answered: DosiItem, out: &mut Output) {
        if !self.saved.on {
            return;
        }
        let DosiBody::Question {
            question,
            options,
            keys,
            terminal,
            answer: Some(choices),
            ..
        } = answered.body
        else {
            return;
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
                return;
            }
        } else {
            self.notes.push(format!("{note}."));
        }
        self.wake = true;
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
            .chat
            .conversation_mut()
            .ok_or_else(|| Error::Refused(text!("Dosi has no conversation.")))?;
        let item = conversation.push(DosiBody::Question {
            question,
            options,
            keys,
            multiple,
            terminal,
            answer: None,
        });
        out.item(&item);
        self.chat.write(item, out);
        Ok(())
    }

    pub(crate) fn renew(&mut self, out: &mut Output) {
        self.quiet();
        self.chat.renew(out);
    }

    /// Turns Dosi on this computer on or off. Off, Dosi here stops its turn and closes its
    /// questions, and answers messages from other devices with a note that it is off here.
    pub(crate) fn turn(&mut self, on: bool, out: &mut Output) {
        if self.saved.on == on {
            return;
        }
        self.saved.on = on;
        self.save();
        if !on {
            self.queue.clear();
            self.notes.clear();
            self.wake = false;
            if self.phase == Phase::Working {
                self.stop(out);
            }
            self.chat.close_own_questions(out);
        }
        out.snapshot = true;
        self.advance(out);
    }

    pub(crate) fn configure(&mut self, change: &AgentChoiceChange, out: &mut Output) -> Result<()> {
        let agent = match change {
            AgentChoiceChange::Tab { agent } => *agent,
            AgentChoiceChange::Pick(pick) => Some(pick.agent),
        };
        if agent.is_some_and(|agent| !AGENTS.contains(&agent)) {
            return Err(Error::Refused(text!("Dosi cannot run on that agent.")));
        }
        let used = |engine: &Self| engine.agent().map(|agent| engine.saved.setup.pick(agent));
        let before = used(self);
        self.saved.setup.apply(change);
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
        out.save = self.chat.save(SharedInstructions { text });
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
        self.chat.page(before)
    }

    pub(crate) fn nudge(&mut self, sequence: Option<u64>, out: &mut Output) {
        self.chat.nudge(sequence, out);
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
