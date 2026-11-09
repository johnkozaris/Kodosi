use std::{
    collections::BTreeSet,
    path::Path,
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use tokio::sync::mpsc;
use uuid::Uuid;

use super::{
    DosiAsk, DosiBody, DosiComputer, DosiConversation, DosiEntry, DosiItem, DosiLog, DosiPage,
    DosiRecord, DosiRemote, SharedInstructions,
    conversation::{self, Conversation},
    shared::Shared,
    store::{Book, Marks, Store},
};
use crate::{Error, Result};

const REMOTE_LIFE: i64 = 10 * 60 * 1000;
const REMOTE_STOP: Duration = Duration::from_secs(8);
const MESSAGES: usize = 256;
const MAX_ASKS: usize = 64;
pub(super) const MAX_WAITING: usize = 8;

pub(crate) struct Failure {
    pub retry: bool,
    pub message: String,
}

pub(crate) enum Message {
    Written {
        generation: u64,
        result: std::result::Result<u64, Failure>,
    },
    Synced {
        generation: u64,
        result: std::result::Result<DosiLog, Failure>,
    },
    Retry {
        generation: u64,
    },
    Gone {
        generation: u64,
        turn: String,
    },
    #[cfg(feature = "host")]
    Saved {
        generation: u64,
        instructions: SharedInstructions,
        result: std::result::Result<(), Failure>,
    },
    #[cfg(feature = "host")]
    Run(super::engine::Run),
}

#[derive(Default)]
pub(crate) struct Output {
    pub events: Vec<Value>,
    pub snapshot: bool,
    pub write: Option<DosiRecord>,
    pub sync: Option<u64>,
    #[cfg(feature = "host")]
    pub typing: Vec<super::engine::Typing>,
    #[cfg(feature = "host")]
    pub save: Option<SharedInstructions>,
}

impl Output {
    pub(super) fn item(&mut self, item: &DosiItem) {
        self.events.push(json!({"type":"dosi.item","item":item}));
    }
}

/// The account of the person and this device in it, whose devices share one conversation.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Account {
    pub user: Uuid,
    pub device: String,
    pub computer: String,
}

/// What another device did that the Dosi of this device acts on.
pub(super) enum Heard {
    /// The server lost the records after the last one that this device knew.
    Reset,
    /// A newer conversation replaced the one that this device worked in.
    Adopted,
    /// Another device stopped the turn that runs here.
    #[cfg_attr(
        not(feature = "host"),
        expect(dead_code, reason = "only a computer runs turns")
    )]
    Stopped(String),
    /// Another device answered a question that Dosi asked here.
    #[cfg_attr(
        not(feature = "host"),
        expect(dead_code, reason = "only a computer asks")
    )]
    Answered(DosiItem),
}

enum Log {
    Local { next: u64 },
    Shared(Shared),
}

pub(super) struct Remote {
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

    pub(super) const fn stopping(&self) -> bool {
        self.stopping.is_some()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Asked {
    Waiting,
    Taken,
    Withdrawn,
}

pub(super) struct Ask {
    pub turn: String,
    pub item: String,
    pub computer: DosiComputer,
    from: String,
    #[cfg_attr(
        not(feature = "host"),
        expect(dead_code, reason = "only a computer answers")
    )]
    pub text: String,
    #[cfg_attr(
        not(feature = "host"),
        expect(dead_code, reason = "only a computer answers")
    )]
    pub context: Option<String>,
    state: Asked,
}

/// Where a record is in the room, and whether this device wrote it.
#[derive(Clone, Copy)]
struct Seen {
    sequence: u64,
    mine: bool,
    restoring: bool,
}

/// Dosi's one conversation as this device knows it: the records that the person's devices
/// wrote in the Dosi room, in the room's order, and the records that this device writes there.
pub(super) struct Chat {
    store: Store,
    messages: mpsc::Sender<Message>,
    log: Log,
    generation: u64,
    device: String,
    conversation: Option<Conversation>,
    remote: Option<Remote>,
    asks: Vec<Ask>,
    last: Option<DosiComputer>,
    marks: Marks,
    marks_unsaved: bool,
    holding: bool,
    devices: Option<BTreeSet<String>>,
}

impl Chat {
    pub(super) fn load(data_root: &Path) -> (Self, mpsc::Receiver<Message>) {
        let (messages, receiver) = mpsc::channel(MESSAGES);
        let mut chat = Self {
            store: Store::open(data_root),
            messages,
            log: Log::Local { next: 1 },
            generation: 0,
            device: String::new(),
            conversation: None,
            remote: None,
            asks: Vec::new(),
            last: None,
            marks: Marks::default(),
            marks_unsaved: false,
            holding: false,
            devices: None,
        };
        chat.open(None);
        (chat, receiver)
    }

    pub(super) fn sender(&self) -> mpsc::Sender<Message> {
        self.messages.clone()
    }

    pub(super) const fn generation(&self) -> u64 {
        self.generation
    }

    pub(super) fn later(&self, delay: Duration, message: Message) {
        let messages = self.messages.clone();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            drop(messages.send(message).await);
        });
    }

    /// Whether `account` is another account than the one that this device shares now.
    pub(super) fn differs(&self, account: Option<&Account>) -> bool {
        match (&self.log, account) {
            (Log::Local { .. }, None) => false,
            (Log::Shared(shared), Some(account)) => {
                shared.user != account.user || self.device != account.device
            }
            _ => true,
        }
    }

    pub(super) fn share(&mut self, account: Option<Account>, out: &mut Output) {
        self.generation += 1;
        self.open(account);
        self.request_sync(out);
        out.snapshot = true;
    }

    fn open(&mut self, account: Option<Account>) {
        let book = account
            .as_ref()
            .map_or(Book::Local, |account| Book::Shared(account.user));
        let log = self.store.log(book);
        self.marks = account
            .as_ref()
            .map_or_else(Marks::default, |account| self.store.marks(account.user));
        self.marks_unsaved = false;
        self.devices = None;
        self.device = account
            .as_ref()
            .map(|account| account.device.clone())
            .unwrap_or_default();
        self.log = match account {
            Some(account) => Log::Shared(Shared::new(
                account.user,
                account.computer,
                (log.sequence > 0).then_some(log.sequence),
            )),
            None => Log::Local {
                next: log.sequence + 1,
            },
        };
        self.conversation = None;
        self.remote = None;
        self.asks.clear();
        self.last = None;
        let mut out = Output::default();
        self.hold(|chat| {
            for entry in log.entries {
                chat.record(entry, true, &mut out);
            }
        });
    }

    fn hold<T>(&mut self, work: impl FnOnce(&mut Self) -> T) -> T {
        self.holding = true;
        let result = work(self);
        self.holding = false;
        self.keep_marks();
        result
    }

    fn marked(&mut self) {
        self.marks_unsaved = true;
        if !self.holding {
            self.keep_marks();
        }
    }

    fn keep_marks(&mut self) {
        if !std::mem::take(&mut self.marks_unsaved) {
            return;
        }
        let waiting = self
            .asks
            .iter()
            .filter(|ask| {
                ask.state == Asked::Waiting
                    && !self.device.is_empty()
                    && ask.computer.device == self.device
            })
            .map(|ask| ask.turn.clone())
            .min();
        self.marks.trim(waiting.as_deref());
        if let Log::Shared(shared) = &self.log {
            self.store.set_marks(shared.user, &self.marks);
        }
    }

    fn settle_turn(&mut self, turn: &str) {
        if self.marks.settle(turn) {
            self.marked();
        }
    }

    fn raise_floor(&mut self, conversation: &str) {
        if self.marks.raise(conversation) {
            self.marked();
        }
    }

    /// Whether `device` is one of the person's devices now.
    fn current_device(&self, device: &str) -> bool {
        self.devices
            .as_ref()
            .is_some_and(|devices| devices.contains(device))
    }

    /// An id for a new conversation, which sorts after every conversation that this device
    /// knows, also when its clock is behind.
    fn fresh_id(&self) -> String {
        let id = Uuid::now_v7().to_string();
        let Some(floor) = self.marks.floor().filter(|floor| *floor >= id.as_str()) else {
            return id;
        };
        let after = conversation::time_of(floor).unwrap_or_else(conversation::now) + 1;
        let after = u64::try_from(after).unwrap_or_default();
        Uuid::new_v7(uuid::Timestamp::from_unix(
            uuid::NoContext,
            after / 1000,
            u32::try_from(after % 1000).unwrap_or_default() * 1_000_000,
        ))
        .to_string()
    }

    pub(super) const fn ready(&self) -> bool {
        match &self.log {
            Log::Local { .. } => true,
            Log::Shared(shared) => shared.synced().is_some(),
        }
    }

    pub(super) const fn shared(&self) -> bool {
        matches!(self.log, Log::Shared(_))
    }

    #[cfg(feature = "host")]
    /// The name of this computer in the records that it writes.
    pub(super) fn computer(&self) -> String {
        match &self.log {
            Log::Local { .. } => String::new(),
            Log::Shared(shared) => shared.computer.clone(),
        }
    }

    #[cfg(feature = "host")]
    pub(super) fn device(&self) -> &str {
        &self.device
    }

    #[cfg(feature = "host")]
    pub(super) const fn conversation(&self) -> Option<&Conversation> {
        self.conversation.as_ref()
    }

    #[cfg(feature = "host")]
    pub(super) const fn conversation_mut(&mut self) -> Option<&mut Conversation> {
        self.conversation.as_mut()
    }

    /// The conversation, after a new one begins when there is none.
    pub(super) fn current(&mut self, out: &mut Output) -> &mut Conversation {
        if self.conversation.is_none() {
            self.renew(out);
        }
        self.conversation
            .get_or_insert_with(|| Conversation::begun(Uuid::now_v7().to_string()))
    }

    pub(super) fn renew(&mut self, out: &mut Output) {
        let id = self.fresh_id();
        self.raise_floor(&id);
        self.conversation = Some(Conversation::begun(id.clone()));
        self.remote = None;
        self.asks.clear();
        self.append(DosiRecord::Begin { conversation: id }, out);
        out.snapshot = true;
    }

    pub(super) fn remote(&self) -> Option<&Remote> {
        self.remote.as_ref().filter(|remote| remote.live())
    }

    pub(super) fn remote_snapshot(&self) -> Option<DosiRemote> {
        self.remote().map(|remote| DosiRemote {
            computer: remote.computer.clone(),
            since: remote.since,
        })
    }

    pub(super) fn conversation_snapshot(&self) -> (Option<DosiConversation>, Vec<DosiItem>, bool) {
        let Some(conversation) = &self.conversation else {
            return (None, Vec::new(), false);
        };
        let (items, earlier) = conversation.window();
        let started = DosiConversation {
            id: conversation.id.clone(),
            started_at: conversation.started_at,
        };
        (Some(started), items, earlier)
    }

    pub(super) fn waiting(&self) -> Vec<DosiAsk> {
        self.asks
            .iter()
            .filter(|ask| ask.state == Asked::Waiting)
            .map(|ask| DosiAsk {
                item: ask.item.clone(),
                computer: ask.computer.clone(),
            })
            .collect()
    }

    pub(super) fn last(&self) -> Option<DosiComputer> {
        self.last.clone()
    }

    #[cfg(feature = "host")]
    /// The oldest message that waits for this device to answer it.
    pub(super) fn asked_here(&self) -> Option<&Ask> {
        self.asks.iter().find(|ask| {
            ask.state == Asked::Waiting
                && !self.device.is_empty()
                && ask.computer.device == self.device
                && self.current_device(&ask.from)
        })
    }

    /// Marks the message for the turn `turn` as answered here, before its turn opens.
    #[cfg(feature = "host")]
    pub(super) fn take(&mut self, turn: &str) {
        self.settle_turn(turn);
        if let Some(ask) = self.asks.iter_mut().find(|ask| ask.turn == turn) {
            ask.state = Asked::Taken;
        }
    }

    /// A turn opened for the message of `turn`, also when the person took it back while its
    /// computer started it.
    fn opened(&mut self, turn: &str) {
        self.settle_turn(turn);
        let Some(ask) = self.asks.iter_mut().find(|ask| ask.turn == turn) else {
            return;
        };
        if ask.state == Asked::Withdrawn
            && let Some(conversation) = self.conversation.as_mut()
        {
            conversation.reveal(turn);
        }
        ask.state = Asked::Taken;
    }

    /// Writes a message of the person for `computer`, which answers it in a turn of its own.
    pub(super) fn ask(
        &mut self,
        text: &str,
        context: Option<String>,
        computer: DosiComputer,
        out: &mut Output,
    ) -> Result<()> {
        if !self.shared() {
            return Err(Error::Refused(text!(
                "Sign in on this device to talk with Dosi on your computers."
            )));
        }
        if computer.device.is_empty() || computer.device == self.device {
            return Err(Error::Refused(text!(
                "Choose one of your computers for Dosi."
            )));
        }
        if !self.ready() || self.waiting().len() >= MAX_WAITING {
            return Err(Error::Busy);
        }
        let turn = Uuid::now_v7().to_string();
        let conversation = self.current(out);
        let item = DosiItem {
            id: Uuid::now_v7().to_string(),
            turn: turn.clone(),
            at: conversation::now(),
            body: DosiBody::Person {
                text: text.to_owned(),
            },
        };
        conversation.place(item.clone());
        let id = conversation.id.clone();
        self.asks.push(Ask {
            turn,
            item: item.id.clone(),
            computer: computer.clone(),
            from: self.device.clone(),
            text: text.to_owned(),
            context: context.clone(),
            state: Asked::Waiting,
        });
        self.bound_asks();
        out.item(&item);
        self.append(
            DosiRecord::Ask {
                conversation: id,
                computer,
                context,
                item,
            },
            out,
        );
        out.snapshot = true;
        Ok(())
    }

    /// Takes back the message `item` that waits for its computer. It answers whether there was
    /// such a message.
    pub(super) fn take_back(&mut self, item: &str, out: &mut Output) -> bool {
        let Some(turn) = self
            .asks
            .iter()
            .find(|ask| ask.item == item && ask.state == Asked::Waiting)
            .map(|ask| ask.turn.clone())
        else {
            return false;
        };
        self.withdrawn(&turn, out);
        self.settle_turn(&turn);
        self.append(DosiRecord::Stop { turn }, out);
        true
    }

    /// Answers a question in the conversation. It answers the answered question, and whether
    /// Dosi asked it on this device, which then goes on with the answer.
    pub(super) fn answer(
        &mut self,
        item: &str,
        choices: Vec<String>,
        out: &mut Output,
    ) -> Result<(DosiItem, bool)> {
        let conversation = self
            .conversation
            .as_mut()
            .ok_or_else(|| Error::Refused(text!("Dosi has no conversation.")))?;
        let here = conversation.mine(item);
        let answered = conversation.answer(item, choices).map_err(Error::Text)?;
        out.item(&answered);
        self.write(answered.clone(), out);
        Ok((answered, here))
    }

    /// Closes the questions that Dosi asked here and that wait for an answer.
    #[cfg(feature = "host")]
    pub(super) fn close_own_questions(&mut self, out: &mut Output) {
        let Some(conversation) = self.conversation.as_mut() else {
            return;
        };
        for item in conversation.close_questions(true) {
            out.item(&item);
            self.write(item, out);
        }
    }

    /// Stops the turn that runs on another computer, and answers whether one ran.
    pub(super) fn stop_remote(&mut self, out: &mut Output) -> bool {
        let Some(remote) = self
            .remote
            .as_mut()
            .filter(|remote| remote.live() && remote.stopping.is_none())
        else {
            return false;
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
        true
    }

    pub(super) fn page(&self, before: &str) -> DosiPage {
        self.conversation.as_ref().map_or_else(
            || DosiPage {
                items: Vec::new(),
                earlier: false,
            },
            |conversation| conversation.page(before),
        )
    }

    pub(super) fn write(&mut self, item: DosiItem, out: &mut Output) {
        let Some(conversation) = &self.conversation else {
            return;
        };
        let record = DosiRecord::Item {
            conversation: conversation.id.clone(),
            item,
        };
        self.append(record, out);
    }

    pub(super) fn append(&mut self, record: DosiRecord, out: &mut Output) {
        match &mut self.log {
            Log::Local { next } => {
                let entry = DosiEntry {
                    sequence: *next,
                    mine: true,
                    device: String::new(),
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
        match entry.record {
            DosiRecord::Item { conversation, item }
            | DosiRecord::Ask {
                conversation, item, ..
            } => {
                if let Some(current) = self
                    .conversation
                    .as_mut()
                    .filter(|current| current.id == conversation)
                {
                    current.upsert(item, entry.sequence, true, false);
                }
            }
            DosiRecord::Turn {
                open: true,
                computer,
                ..
            } => {
                if !self.device.is_empty() {
                    self.last = Some(DosiComputer {
                        device: self.device.clone(),
                        name: computer,
                    });
                }
            }
            DosiRecord::Begin { .. } | DosiRecord::Turn { .. } | DosiRecord::Stop { .. } => {}
        }
    }

    pub(super) fn request_sync(&mut self, out: &mut Output) {
        if let Log::Shared(shared) = &mut self.log
            && let Some(after) = shared.sync()
        {
            out.sync = Some(after);
        }
    }

    pub(super) fn nudge(&mut self, sequence: Option<u64>, out: &mut Output) {
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

    pub(super) fn retry(&mut self, generation: u64, out: &mut Output) {
        if generation != self.generation {
            return;
        }
        let Log::Shared(shared) = &mut self.log else {
            return;
        };
        let again = shared.retry();
        if again.write.is_some() {
            out.write = again.write;
        }
        if again.sync.is_some() {
            out.sync = again.sync;
        }
        #[cfg(feature = "host")]
        if again.save.is_some() {
            out.save = again.save;
        }
    }

    pub(super) fn written(
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
                        device: self.device.clone(),
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

    /// Takes the records that the server holds after the last one that this device knows. It
    /// answers what other devices did, and the instructions that the server holds.
    pub(super) fn synced(
        &mut self,
        generation: u64,
        result: std::result::Result<DosiLog, Failure>,
        out: &mut Output,
    ) -> Option<(Vec<Heard>, Option<SharedInstructions>)> {
        if generation != self.generation {
            return None;
        }
        let Log::Shared(shared) = &mut self.log else {
            return None;
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
                return None;
            }
        };
        let book = Book::Shared(shared.user);
        if log.reset {
            shared.reset();
            self.store.clear(book);
            self.conversation = None;
            self.remote = None;
            self.asks.clear();
            out.snapshot = true;
            self.request_sync(out);
            return Some((vec![Heard::Reset], None));
        }
        let restoring = shared.synced().is_none();
        let known = shared.synced().unwrap_or_default();
        let again = shared.synced_to(log.sequence);
        #[cfg(feature = "host")]
        let instructions = log.instructions.filter(|_| !shared.saving());
        #[cfg(not(feature = "host"))]
        let instructions = None;
        self.devices = Some(log.devices).filter(|devices| devices.contains(&self.device));
        if self
            .last
            .as_ref()
            .is_some_and(|last| !self.current_device(&last.device))
        {
            self.last = None;
        }
        let entries = log
            .entries
            .into_iter()
            .filter(|entry| {
                entry.sequence > known
                    && (entry.mine
                        || self.current_device(&entry.device)
                        || !matches!(
                            entry.record,
                            DosiRecord::Ask { .. } | DosiRecord::Item { .. }
                        ))
            })
            .collect::<Vec<_>>();
        self.store.append(book, &entries);
        let heard = self.hold(|chat| {
            let mut heard = Vec::new();
            for entry in entries {
                heard.extend(chat.record(entry, restoring, out));
            }
            heard
        });
        self.drop_strangers(out);
        if again {
            self.request_sync(out);
        }
        out.snapshot = true;
        Some((heard, instructions))
    }

    #[cfg(feature = "host")]
    pub(super) fn save(&mut self, instructions: SharedInstructions) -> Option<SharedInstructions> {
        match &mut self.log {
            Log::Shared(shared) => shared.save(instructions),
            Log::Local { .. } => None,
        }
    }

    #[cfg(feature = "host")]
    pub(super) fn saved(
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

    /// Forgets the turn on another computer once it stopped telling that it runs.
    pub(super) fn gone(&mut self, generation: u64, turn: &str, out: &mut Output) -> bool {
        if generation != self.generation
            || !self
                .remote
                .as_ref()
                .is_some_and(|remote| remote.turn == turn && !remote.live())
        {
            return false;
        }
        self.remote = None;
        out.snapshot = true;
        true
    }

    /// Takes back the messages for this device from devices that the person no longer has.
    fn drop_strangers(&mut self, out: &mut Output) {
        let Some(devices) = &self.devices else {
            return;
        };
        let strangers = self
            .asks
            .iter()
            .filter(|ask| {
                ask.state == Asked::Waiting
                    && ask.computer.device == self.device
                    && !devices.contains(&ask.from)
            })
            .map(|ask| ask.turn.clone())
            .collect::<Vec<_>>();
        for turn in strangers {
            self.withdrawn(&turn, out);
            self.settle_turn(&turn);
            self.append(DosiRecord::Stop { turn }, out);
        }
    }

    fn record(&mut self, entry: DosiEntry, restoring: bool, out: &mut Output) -> Vec<Heard> {
        let seen = Seen {
            sequence: entry.sequence,
            mine: entry.mine,
            restoring,
        };
        let mut heard = Vec::new();
        match entry.record {
            DosiRecord::Begin { conversation } => {
                let newer = self
                    .conversation
                    .as_ref()
                    .is_none_or(|current| current.id < conversation);
                if newer && !self.marks.below(&conversation) {
                    self.adopt(conversation.clone(), out, &mut heard);
                }
                if let Some(current) = self
                    .conversation
                    .as_mut()
                    .filter(|current| current.id == conversation)
                {
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
                if self.belongs(&conversation, out, &mut heard) {
                    let writer = DosiComputer {
                        device: entry.device,
                        name: computer,
                    };
                    self.record_turn(turn, open, writer, at, seen, out);
                }
            }
            DosiRecord::Stop { turn } => {
                self.settle_turn(&turn);
                if !self.withdrawn(&turn, out) && !seen.mine {
                    heard.push(Heard::Stopped(turn));
                }
            }
            DosiRecord::Item { conversation, item } => {
                if self.belongs(&conversation, out, &mut heard)
                    && let Some(answered) = self.record_item(item, seen, &entry.device, out)
                {
                    heard.push(Heard::Answered(answered));
                }
            }
            DosiRecord::Ask {
                conversation,
                computer,
                context,
                item,
            } => {
                if self.belongs(&conversation, out, &mut heard) {
                    let from = entry.device;
                    self.record_ask(computer, from, context, item, seen, out);
                }
            }
        }
        heard
    }

    fn record_turn(
        &mut self,
        turn: String,
        open: bool,
        writer: DosiComputer,
        at: i64,
        seen: Seen,
        out: &mut Output,
    ) {
        if open {
            self.opened(&turn);
            if !writer.device.is_empty()
                && (self.devices.is_none() || self.current_device(&writer.device))
            {
                self.last = Some(writer.clone());
            }
        }
        if seen.mine {
            return;
        }
        if open {
            self.remote_turn(turn, writer.name, at, out);
        } else if self
            .remote
            .as_ref()
            .is_some_and(|remote| remote.turn == turn)
        {
            self.remote = None;
            out.snapshot = true;
        }
    }

    /// Takes back the message of `turn` when it still waits, and answers whether it did.
    fn withdrawn(&mut self, turn: &str, out: &mut Output) -> bool {
        let Some(ask) = self
            .asks
            .iter_mut()
            .find(|ask| ask.turn == turn && ask.state == Asked::Waiting)
        else {
            return false;
        };
        ask.state = Asked::Withdrawn;
        if let Some(conversation) = self.conversation.as_mut() {
            conversation.withdraw(turn);
        }
        if self.marks.withdraw(turn) {
            self.marked();
        }
        out.snapshot = true;
        true
    }

    /// Takes an item into the conversation. Of an answer from another of the person's devices
    /// to a question that Dosi asked here, it takes only the answer, once, as this device's
    /// question takes it, and answers the answered question; a question closed there closes.
    fn record_item(
        &mut self,
        item: DosiItem,
        seen: Seen,
        writer: &str,
        out: &mut Output,
    ) -> Option<DosiItem> {
        if let Some(remote) = self
            .remote
            .as_mut()
            .filter(|remote| remote.turn == item.turn)
        {
            remote.last = remote.last.max(item.at);
        }
        let trusted = self.current_device(writer) && !self.marks.answered(&item.id);
        let current = self.conversation.as_mut()?;
        if current.answered(&item.id) {
            return None;
        }
        if !seen.restoring && !seen.mine && current.mine(&item.id) {
            let DosiBody::Question {
                answer: Some(choices),
                ..
            } = item.body
            else {
                return None;
            };
            if !trusted {
                return None;
            }
            if choices.is_empty() {
                if let Some(closed) = current.close(&item.id) {
                    out.item(&closed);
                }
                return None;
            }
            let answered = current.answer(&item.id, choices).ok()?;
            out.item(&answered);
            if self.marks.answer(&answered.id) {
                self.marked();
            }
            return Some(answered);
        }
        self.upsert(item, seen, out);
        None
    }

    fn record_ask(
        &mut self,
        computer: DosiComputer,
        from: String,
        context: Option<String>,
        item: DosiItem,
        seen: Seen,
        out: &mut Output,
    ) {
        let DosiBody::Person { text } = &item.body else {
            return;
        };
        if !self.asks.iter().any(|ask| ask.turn == item.turn) {
            let state = if self.marks.withdrawn(&item.turn) {
                if let Some(conversation) = self.conversation.as_mut() {
                    conversation.withdraw(&item.turn);
                }
                Asked::Withdrawn
            } else if self.marks.settled(&item.turn) {
                Asked::Taken
            } else {
                Asked::Waiting
            };
            self.asks.push(Ask {
                turn: item.turn.clone(),
                item: item.id.clone(),
                computer,
                from,
                text: text.clone(),
                context,
                state,
            });
            self.bound_asks();
            out.snapshot = true;
        }
        self.upsert(item, seen, out);
    }

    fn upsert(&mut self, item: DosiItem, seen: Seen, out: &mut Output) {
        let Some(current) = self.conversation.as_mut() else {
            return;
        };
        let id = item.id.clone();
        if current.upsert(item, seen.sequence, seen.mine, seen.restoring)
            && let Some(item) = current.item(&id)
        {
            out.item(item);
        }
    }

    fn belongs(&mut self, conversation: &str, out: &mut Output, heard: &mut Vec<Heard>) -> bool {
        match &self.conversation {
            Some(current) if current.id == conversation => true,
            Some(current) if current.confirmed || current.id.as_str() > conversation => false,
            _ if self.marks.below(conversation) => false,
            _ => {
                self.adopt(conversation.to_owned(), out, heard);
                true
            }
        }
    }

    fn adopt(&mut self, id: String, out: &mut Output, heard: &mut Vec<Heard>) {
        self.raise_floor(&id);
        heard.push(Heard::Adopted);
        self.remote = None;
        self.asks.clear();
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

    fn bound_asks(&mut self) {
        while self.asks.len() > MAX_ASKS {
            let done = self
                .asks
                .iter()
                .position(|ask| ask.state != Asked::Waiting)
                .unwrap_or(0);
            self.asks.remove(done);
        }
    }
}
