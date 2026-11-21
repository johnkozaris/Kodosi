//! Codex's link: the messages of its app-server as changes of the chat.
//!
//! The terminal's Codex works through an app-server that Kodosi starts for it, and Kodosi is a
//! second program of that server. The link carries each message of the server as it is, and makes
//! the calls that this adapter gives it, so what the server's protocol means is all here. A
//! person's answer goes back as the decision that Codex takes. A field can be absent or of another
//! type after an update of Codex, so a message gives what it has and never stops the chat.

use std::collections::{HashMap, VecDeque};

use serde_json::{Value, json};

use super::{
    AskAlways, AskBody, ChatAnswer, ChatAsk, ChatBody, ChatContext, ChatMode, ChatNote, ChatNow,
    ChatPerson, ChatState, ChatTool, MOST_OUTPUT, MOST_TARGET, ToolDetail, ToolKind, ToolState,
    TurnEnd,
    agent::{Agent, Stop, answer, file_name, list, person, text, words},
    diff,
    state::{Chat, ends_of, start_of},
};

/// The turns that the chat shows of a conversation that is older than the link.
const HISTORY: u64 = 20;
const MOST_ITEMS: usize = 2048;

#[derive(Default)]
pub(super) struct Codex {
    linked: bool,
    /// The conversation of the Codex in the terminal.
    thread: Option<String>,
    follows: Follows,
    turn: Option<Turn>,
    /// The chat's item of each item of Codex.
    items: HashMap<String, String>,
    asked: HashMap<String, Asked>,
    calls: VecDeque<Value>,
    handed: VecDeque<Handed>,
    sent: u64,
    /// What a person wrote to refuse a question. Codex gets it as the next message.
    said: Option<String>,
    /// The commands that ran when a person of the chat stopped their turn. Codex lets them run
    /// on, so the adapter ends them.
    stopped: Vec<String>,
    now: Option<ChatNow>,
    /// Codex refused a message of the chat.
    held: bool,
}

/// The server sends the messages of a conversation only to a program that asked for it, and it
/// takes that request only after the conversation's first turn began.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Follows {
    #[default]
    No,
    Asked,
    Yes,
}

#[derive(Default)]
struct Turn {
    id: Option<String>,
    stopped: bool,
}

struct Asked {
    kind: AskKind,
    item: String,
    offered: Vec<Value>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AskKind {
    Command,
    Change,
}

struct Handed {
    n: u64,
    /// The message of the queue, or nothing for the words of a refusal.
    queued: Option<String>,
    text: String,
    by: Option<ChatPerson>,
}

impl Codex {
    pub(super) const NAME: &'static str = "codex";

    /// The name of the question that a request of the server makes.
    pub(super) fn ask_name(request: &Value) -> Option<String> {
        match request {
            Value::Number(number) => Some(format!("r{number}")),
            Value::String(name) if !name.is_empty() => Some(format!("r{name}")),
            _ => None,
        }
    }

    fn show(&self, chat: &mut Chat) {
        if !self.linked {
            chat.set_state(ChatState::Away, None);
        } else if !chat.asks().is_empty() {
            chat.set_state(ChatState::Asking, None);
        } else if self.turn.is_some() {
            chat.set_state(ChatState::Working, self.now.clone());
        } else {
            chat.set_state(ChatState::Idle, None);
        }
    }

    fn call(&mut self, tag: &str, method: &str, params: &Value) {
        self.calls
            .push_back(json!({"do": "call", "tag": tag, "m": method, "p": params}));
    }

    fn hello(&mut self, chat: &mut Chat, event: &Value) -> Vec<String> {
        let closed = chat.close_asks(|_| true);
        chat.reset();
        *self = Self {
            linked: true,
            ..Self::default()
        };
        // The server names the program that started it and that program's version, as in
        // "codex-tui/0.171.0 (Mac OS 27.2.1; arm64)".
        let version = text(event, "server")
            .split_once('/')
            .map(|(_, rest)| rest.split(' ').next().unwrap_or_default())
            .filter(|version| !version.is_empty())
            .map(|version| start_of(version, 32).to_owned());
        chat.change_facts(|facts| {
            *facts = super::ChatFacts {
                agent: Some(Self::NAME.to_owned()),
                version,
                ..super::ChatFacts::default()
            };
        });
        self.call("models", "model/list", &json!({}));
        self.call("limits", "account/rateLimits/read", &Value::Null);
        self.call("loaded", "thread/loaded/list", &json!({}));
        closed
    }

    fn reply(&mut self, chat: &mut Chat, event: &Value) {
        let (tag, name) = text(event, "tag")
            .split_once(':')
            .unwrap_or_else(|| (text(event, "tag"), ""));
        let result = event.get("r").filter(|result| !result.is_null());
        match (tag, result) {
            ("models", Some(result)) => {
                let models = list(result, "data")
                    .filter(|model| model.get("hidden").and_then(Value::as_bool) != Some(true))
                    .filter_map(|model| words(model, "id"))
                    .take(32)
                    .collect::<Vec<_>>();
                chat.change_facts(|facts| facts.models = models);
            }
            ("limits", Some(result)) => limits(chat, result),
            ("loaded", Some(result)) if self.thread.is_none() => {
                let loaded = list(result, "data")
                    .filter_map(Value::as_str)
                    .take(8)
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                for thread in loaded {
                    self.call(
                        "read",
                        "thread/read",
                        &json!({"threadId": thread, "includeTurns": false}),
                    );
                }
            }
            ("read", Some(result)) => {
                if let Some(thread) = result.get("thread") {
                    self.consider(chat, thread);
                }
            }
            ("follow", result) if self.thread.as_deref() == Some(name) => match result {
                Some(result) => {
                    self.follows = Follows::Yes;
                    if let Some(thread) = result.get("thread") {
                        describe(chat, thread);
                    }
                    if chat.items().is_empty() {
                        self.call(
                            &format!("history:{name}"),
                            "thread/turns/list",
                            &json!({
                                "threadId": name,
                                "limit": HISTORY,
                                "itemsView": "full",
                                "sortDirection": "desc",
                            }),
                        );
                    }
                }
                None => self.follows = Follows::No,
            },
            ("history", Some(result))
                if self.thread.as_deref() == Some(name) && chat.items().is_empty() =>
            {
                self.history(chat, result);
            }
            ("prompt", None) => self.refused(chat, name, event),
            ("background", Some(result)) => {
                let ended = std::mem::take(&mut self.stopped);
                let Some(thread) = self.thread.clone() else {
                    return;
                };
                for process in list(result, "data")
                    .filter(|process| ended.iter().any(|item| item == text(process, "itemId")))
                    .filter_map(|process| words(process, "processId"))
                {
                    self.call(
                        "end",
                        "thread/backgroundTerminals/terminate",
                        &json!({"threadId": thread, "processId": process}),
                    );
                }
            }
            _ => {}
        }
    }

    /// A message that Codex did not take stays in the queue with Codex's reason.
    fn refused(&mut self, chat: &mut Chat, n: &str, event: &Value) {
        let n = n.parse::<u64>().ok();
        let Some(index) = self.handed.iter().position(|handed| Some(handed.n) == n) else {
            return;
        };
        let reason = event
            .pointer("/e/message")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if let Some(handed) = self.handed.remove(index)
            && let Some(queued) = handed.queued
        {
            chat.refuse(&queued, start_of(reason, MOST_TARGET).to_owned());
            self.held = true;
        }
    }

    /// The terminal's Codex has one conversation that a person writes in. A conversation that
    /// Codex makes for its own work, such as the one that names a conversation, is not it.
    fn consider(&mut self, chat: &mut Chat, thread: &Value) {
        let id = text(thread, "id");
        if id.is_empty()
            || thread.get("ephemeral").and_then(Value::as_bool) != Some(false)
            || text(thread, "threadSource") != "user"
        {
            return;
        }
        if self.thread.as_deref() != Some(id) {
            if let Some(before) = self.thread.replace(id.to_owned()) {
                self.call("left", "thread/unsubscribe", &json!({"threadId": before}));
                self.forget(chat);
            }
            self.follows = Follows::No;
        }
        describe(chat, thread);
        self.follow();
    }

    /// The person began another conversation, so the chat begins again.
    fn forget(&mut self, chat: &mut Chat) {
        chat.close_asks(|_| true);
        chat.reset();
        self.turn = None;
        self.items.clear();
        self.asked.clear();
        self.handed.clear();
        self.stopped.clear();
        self.said = None;
        self.now = None;
        self.held = false;
    }

    fn follow(&mut self) {
        let (Follows::No, Some(thread)) = (self.follows, self.thread.clone()) else {
            return;
        };
        self.follows = Follows::Asked;
        self.call(
            &format!("follow:{thread}"),
            "thread/resume",
            &json!({"threadId": thread, "excludeTurns": true}),
        );
    }

    fn is_ours(&self, params: &Value) -> bool {
        self.thread
            .as_deref()
            .is_some_and(|thread| text(params, "threadId") == thread)
    }

    fn note(&mut self, chat: &mut Chat, event: &Value, at: u64) -> Vec<String> {
        let params = event.get("p").unwrap_or(&Value::Null);
        match text(event, "m") {
            "thread/started" => {
                if let Some(thread) = params.get("thread") {
                    self.consider(chat, thread);
                }
            }
            "account/rateLimits/updated" => limits(chat, params),
            _ if !self.is_ours(params) => {}
            "thread/status/changed" => match text(&params["status"], "type") {
                "active" => {
                    self.turn.get_or_insert_default();
                    self.follow();
                }
                // Without the conversation's messages, its state is all that tells the turn's end.
                _ if self.follows != Follows::Yes => self.turn = None,
                _ => {}
            },
            "turn/started" => {
                self.turn = Some(Turn {
                    id: words(&params["turn"], "id"),
                    stopped: false,
                });
                self.now = None;
                self.held = false;
            }
            "turn/completed" => return self.end_turn(chat, &params["turn"], at),
            "item/started" => self.item(chat, params, at, false),
            "item/completed" => self.item(chat, params, at, true),
            "item/agentMessage/delta" => {
                if let Some(item) = self.items.get(text(params, "itemId")) {
                    chat.write(item, text(params, "delta"));
                    self.now = Some(ChatNow::Writing);
                }
            }
            "item/commandExecution/outputDelta" => {
                if let Some(item) = self.items.get(text(params, "itemId")) {
                    let more = text(params, "delta");
                    chat.change_tool(item, |tool| {
                        if let Some(ToolDetail::Command { output, .. }) = &mut tool.detail {
                            *output = ends_of(&format!("{output}{more}"), MOST_OUTPUT);
                        }
                    });
                }
            }
            "serverRequest/resolved" => {
                let Some(ask) = Self::ask_name(&params["requestId"]) else {
                    return Vec::new();
                };
                // A person answered in the terminal. What Codex did with the answer comes with
                // the item.
                if let Some(asked) = self.asked.remove(&ask) {
                    chat.change_tool(&asked.item, |tool| {
                        if tool.state == ToolState::Asking {
                            tool.state = ToolState::Running;
                        }
                    });
                    return chat.close_asks(|open| open.id == ask);
                }
            }
            "thread/tokenUsage/updated" => {
                let usage = &params["tokenUsage"];
                if let (Some(tokens), Some(window)) = (
                    usage.pointer("/last/totalTokens").and_then(Value::as_u64),
                    usage.get("modelContextWindow").and_then(Value::as_u64),
                ) {
                    chat.change_facts(|facts| {
                        facts.context = Some(ChatContext { tokens, window });
                    });
                }
            }
            "thread/settings/updated" => settings(chat, &params["threadSettings"]),
            _ => {}
        }
        Vec::new()
    }

    fn item(&mut self, chat: &mut Chat, params: &Value, at: u64, done: bool) {
        let item = &params["item"];
        let id = text(item, "id");
        if id.is_empty() {
            return;
        }
        if !done && let Some(turn) = words(params, "turnId") {
            self.turn.get_or_insert_default().id = Some(turn);
        }
        let known = self.items.get(id).cloned();
        let shown = match (text(item, "type"), known) {
            ("userMessage", None) => self.said_by_person(chat, item, at),
            ("agentMessage", None) => Some(chat.push(at, answer(text(item, "text"), !done))),
            ("agentMessage", Some(shown)) if done => {
                chat.put(&shown, answer(text(item, "text"), false));
                if self.now == Some(ChatNow::Writing) {
                    self.now = None;
                }
                None
            }
            ("reasoning", _) => {
                if !done {
                    self.now = Some(ChatNow::Thinking);
                }
                None
            }
            ("commandExecution" | "fileChange", None) => described(item).map(|mut tool| {
                end(&mut tool, item);
                chat.push(
                    at,
                    ChatBody::Tool {
                        tool: Box::new(tool),
                    },
                )
            }),
            ("commandExecution" | "fileChange", Some(shown)) if done => {
                chat.change_tool(&shown, |tool| {
                    // The command of a turn that a person of the chat stopped ends as a failure.
                    let stopped = tool.state == ToolState::Stopped;
                    end(tool, item);
                    if stopped {
                        tool.state = ToolState::Stopped;
                    }
                });
                None
            }
            (kind, None) => other(kind, item).map(|tool| {
                chat.push(
                    at,
                    ChatBody::Tool {
                        tool: Box::new(tool),
                    },
                )
            }),
            (_, Some(shown)) if done => {
                let state = state_of(item);
                chat.change_tool(&shown, |tool| tool.state = state);
                None
            }
            _ => None,
        };
        if let Some(shown) = shown {
            if self.items.len() >= MOST_ITEMS {
                let live = chat
                    .items()
                    .iter()
                    .map(|item| item.id.as_str())
                    .collect::<std::collections::HashSet<_>>();
                self.items.retain(|_, item| live.contains(item.as_str()));
            }
            if !done && chat.tool(&shown).is_some() {
                self.now = Some(ChatNow::Tool {
                    item: shown.clone(),
                });
            }
            self.items.insert(id.to_owned(), shown);
        }
    }

    /// A message of a person: one that the chat gave Codex, or one that a person typed in the
    /// terminal.
    fn said_by_person(&mut self, chat: &mut Chat, item: &Value, at: u64) -> Option<String> {
        let said = list(item, "content")
            .filter(|part| text(part, "type") == "text")
            .map(|part| text(part, "text"))
            .collect::<Vec<_>>()
            .join("\n");
        if said.is_empty() {
            return None;
        }
        let by = match self.handed.front() {
            Some(handed) if handed.text == said => {
                let handed = self.handed.pop_front()?;
                if let Some(queued) = &handed.queued {
                    chat.unqueue(queued);
                }
                handed.by
            }
            _ => {
                self.held = false;
                None
            }
        };
        Some(chat.push(at, person(&said, by)))
    }

    fn request(&mut self, chat: &mut Chat, event: &Value) {
        let params = event.get("p").unwrap_or(&Value::Null);
        let kind = match text(event, "m") {
            "item/commandExecution/requestApproval" => AskKind::Command,
            "item/fileChange/requestApproval" => AskKind::Change,
            _ => return,
        };
        let (Some(ask), Some(item)) = (
            event.get("id").and_then(Self::ask_name),
            self.items.get(text(params, "itemId")).cloned(),
        ) else {
            return;
        };
        if !self.is_ours(params) {
            return;
        }
        chat.change_tool(&item, |tool| tool.state = ToolState::Asking);
        let Some(tool) = chat.tool(&item).cloned() else {
            return;
        };
        let offered = list(params, "availableDecisions")
            .cloned()
            .collect::<Vec<_>>();
        let always = match kind {
            AskKind::Command => offered
                .iter()
                .find_map(|decision| {
                    decision.pointer("/acceptWithExecpolicyAmendment/execpolicy_amendment")
                })
                .map(rule),
            AskKind::Change => Some(tool.target.clone()),
        }
        .filter(|text| !text.is_empty())
        .map(|text| AskAlways::Rule {
            tool: tool.name.clone(),
            text,
        });
        chat.ask(ChatAsk {
            id: ask.clone(),
            item: Some(item.clone()),
            body: AskBody::Permission {
                tool: Box::new(tool),
                always,
                helper: String::new(),
            },
        });
        self.asked.insert(
            ask,
            Asked {
                kind,
                item,
                offered,
            },
        );
    }

    fn end_turn(&mut self, chat: &mut Chat, turn: &Value, at: u64) -> Vec<String> {
        let stopped = self.turn.take().is_some_and(|turn| turn.stopped);
        self.now = None;
        let end = match text(turn, "status") {
            "interrupted" => TurnEnd::Stopped,
            "failed" => TurnEnd::Failed,
            _ => TurnEnd::Done,
        };
        let open = chat
            .items()
            .iter()
            .filter_map(|item| match &item.body {
                ChatBody::Tool { tool } if tool.state.is_open() => Some(item.id.clone()),
                ChatBody::Text { live: true, .. } => Some(item.id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        for item in &open {
            match chat.body(item).cloned() {
                Some(ChatBody::Text { text, .. }) => chat.put(item, answer(&text, false)),
                // After a stop in the terminal, Codex lets a command that runs go on.
                _ => chat.change_tool(item, |tool| match &mut tool.detail {
                    Some(ToolDetail::Command { background, .. })
                        if !stopped
                            && end == TurnEnd::Stopped
                            && tool.state == ToolState::Running =>
                    {
                        *background = true;
                    }
                    _ => tool.state = ToolState::Stopped,
                }),
            }
        }
        if stopped && end == TurnEnd::Stopped {
            self.stopped = self
                .items
                .iter()
                .filter(|(_, shown)| open.contains(shown))
                .map(|(item, _)| item.clone())
                .collect();
            if let (false, Some(thread)) = (self.stopped.is_empty(), self.thread.clone()) {
                self.call(
                    "background",
                    "thread/backgroundTerminals/list",
                    &json!({"threadId": thread}),
                );
            }
        }
        if end == TurnEnd::Failed {
            let error = turn.pointer("/error/message").and_then(Value::as_str);
            chat.push(
                at,
                ChatBody::Note {
                    note: ChatNote::Failed {
                        error: String::new(),
                        text: start_of(error.unwrap_or_default(), MOST_TARGET).to_owned(),
                    },
                },
            );
        }
        chat.push(
            at,
            ChatBody::Turn {
                end,
                ms: turn
                    .get("durationMs")
                    .and_then(Value::as_u64)
                    .unwrap_or_default(),
            },
        );
        self.asked.clear();
        chat.close_asks(|_| true)
    }

    /// The turns of a conversation that began before the link, the oldest first.
    fn history(&mut self, chat: &mut Chat, result: &Value) {
        let mut turns = list(result, "data").collect::<Vec<_>>();
        turns.reverse();
        for turn in turns {
            if text(turn, "status") == "inProgress" {
                continue;
            }
            let at = turn
                .get("startedAt")
                .and_then(Value::as_u64)
                .unwrap_or_default()
                .saturating_mul(1000);
            for item in list(turn, "items") {
                let at = item
                    .get("startedAtMs")
                    .and_then(Value::as_u64)
                    .unwrap_or(at);
                self.item(chat, &json!({"item": item}), at, true);
            }
            let ended = turn
                .get("completedAt")
                .and_then(Value::as_u64)
                .map_or(at, |seconds| seconds.saturating_mul(1000));
            let live = self.turn.take();
            self.end_turn(chat, turn, ended);
            self.turn = live;
        }
        self.now = None;
    }
}

impl Agent for Codex {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn apply(&mut self, chat: &mut Chat, event: &Value) -> Vec<String> {
        let at = event.get("t").and_then(Value::as_u64).unwrap_or_default();
        let closed = match text(event, "k") {
            "hello" => self.hello(chat, event),
            "note" => self.note(chat, event, at),
            "request" => {
                self.request(chat, event);
                Vec::new()
            }
            "reply" => {
                self.reply(chat, event);
                Vec::new()
            }
            "end" => return self.lost(chat),
            _ => Vec::new(),
        };
        self.show(chat);
        closed
    }

    fn lost(&mut self, chat: &mut Chat) -> Vec<String> {
        self.linked = false;
        self.turn = None;
        self.now = None;
        self.said = None;
        self.handed.clear();
        self.calls.clear();
        self.stopped.clear();
        let open = chat
            .items()
            .iter()
            .filter_map(|item| match &item.body {
                ChatBody::Tool { tool } if tool.state.is_open() => Some(item.id.clone()),
                ChatBody::Text { live: true, .. } => Some(item.id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        for item in open {
            match chat.body(&item).cloned() {
                Some(ChatBody::Text { text, .. }) => chat.put(&item, answer(&text, false)),
                _ => chat.change_tool(&item, |tool| tool.state = ToolState::Stopped),
            }
        }
        self.asked.clear();
        let closed = chat.close_asks(|_| true);
        self.show(chat);
        closed
    }

    fn back(&mut self, _: &mut Chat) {}

    fn is_linked(&self) -> bool {
        self.linked
    }

    fn retry(&mut self) {
        self.held = false;
    }

    fn has(&self, queued: &str) -> bool {
        self.handed
            .iter()
            .any(|handed| handed.queued.as_deref() == Some(queued))
    }

    /// Codex takes no words with a refusal. As its own "No, and tell Codex what to do
    /// differently", a refusal ends the turn, and the person's words are the next message.
    fn decision(&mut self, ask: &str, answer: &ChatAnswer) -> Option<Value> {
        let asked = self.asked.get(ask)?;
        let decision = match (asked.kind, answer) {
            (_, ChatAnswer::Allow) => json!("accept"),
            (AskKind::Command, ChatAnswer::Always) => asked
                .offered
                .iter()
                .find(|decision| decision.get("acceptWithExecpolicyAmendment").is_some())?
                .clone(),
            (AskKind::Change, ChatAnswer::Always) => json!("acceptForSession"),
            (_, ChatAnswer::Deny { text }) => {
                self.said = text
                    .as_deref()
                    .map(str::trim)
                    .filter(|words| !words.is_empty())
                    .map(str::to_owned);
                json!("cancel")
            }
            _ => return None,
        };
        Some(json!({"decision": decision}))
    }

    fn answered(&mut self, chat: &mut Chat, ask: &str, decision: &Value) {
        let Some(asked) = self.asked.remove(ask) else {
            return;
        };
        chat.close_asks(|open| open.id == ask);
        let refused = text(decision, "decision") == "cancel";
        chat.change_tool(&asked.item, |tool| {
            if tool.state == ToolState::Asking {
                tool.state = if refused {
                    ToolState::Denied
                } else {
                    ToolState::Running
                };
            }
        });
        self.show(chat);
    }

    /// The calls of the adapter, then the next message when Codex can start it: no turn runs, no
    /// question is open and Codex has no earlier message of the chat.
    fn commands(&mut self, chat: &Chat) -> Vec<Value> {
        let mut commands = self.calls.drain(..).collect::<Vec<_>>();
        let ready = self.linked
            && !self.held
            && self.turn.is_none()
            && self.handed.is_empty()
            && chat.asks().is_empty();
        let (true, Some(thread)) = (ready, self.thread.clone()) else {
            return commands;
        };
        let next = self.said.take().map(|said| (None, said, None)).or_else(|| {
            let queued = chat.queued().first()?;
            Some((
                Some(queued.id.clone()),
                queued.text.clone(),
                queued.by.clone(),
            ))
        });
        if let Some((queued, text, by)) = next {
            self.sent += 1;
            commands.push(json!({
                "do": "call",
                "tag": format!("prompt:{}", self.sent),
                "m": "turn/start",
                "p": {
                    "threadId": thread,
                    "input": [{"type": "text", "text": text, "text_elements": []}],
                },
            }));
            self.handed.push_back(Handed {
                n: self.sent,
                queued,
                text,
                by,
            });
        }
        commands
    }

    fn stop(&mut self, chat: &Chat) -> Stop {
        if !chat.asks().is_empty() {
            return Stop::Decisions(
                chat.asks()
                    .iter()
                    .map(|ask| (ask.id.clone(), json!({"decision": "cancel"})))
                    .collect(),
            );
        }
        let (Some(thread), Some(turn)) = (self.thread.clone(), &mut self.turn) else {
            return Stop::Nothing;
        };
        let Some(id) = turn.id.clone() else {
            return Stop::Nothing;
        };
        turn.stopped = true;
        Stop::Command(json!({
            "do": "call",
            "tag": "stop",
            "m": "turn/interrupt",
            "p": {"threadId": thread, "turnId": id},
        }))
    }
}

/// What a conversation tells of the agent that works in it.
fn describe(chat: &mut Chat, thread: &Value) {
    chat.change_facts(|facts| {
        facts.model = words(thread, "model").or_else(|| facts.model.take());
        facts.effort = words(thread, "reasoningEffort").or_else(|| facts.effort.take());
        facts.folder = words(thread, "cwd").or_else(|| facts.folder.take());
    });
}

fn settings(chat: &mut Chat, settings: &Value) {
    let mode = if text(&settings["collaborationMode"], "mode") == "plan" {
        Some(ChatMode::Plan)
    } else if text(settings, "approvalsReviewer") != "user"
        && !text(settings, "approvalsReviewer").is_empty()
    {
        Some(ChatMode::Auto)
    } else {
        match (
            text(settings, "approvalPolicy"),
            text(&settings["sandboxPolicy"], "type"),
        ) {
            ("never", "dangerFullAccess") => Some(ChatMode::Unrestricted),
            ("never", _) => Some(ChatMode::Rules),
            (_, "readOnly") => Some(ChatMode::Ask),
            (_, "workspaceWrite") => Some(ChatMode::Edits),
            _ => None,
        }
    };
    chat.change_facts(|facts| {
        facts.model = words(settings, "model").or_else(|| facts.model.take());
        facts.effort = words(settings, "effort").or_else(|| facts.effort.take());
        facts.folder = words(settings, "cwd").or_else(|| facts.folder.take());
        facts.mode = mode.or(facts.mode);
    });
}

fn limits(chat: &mut Chat, result: &Value) {
    let limits = crate::agents::codex_session(result);
    if !limits.is_empty() {
        chat.change_facts(|facts| facts.limits = limits);
    }
}

/// What an item of Codex tells of its tool now.
fn end(tool: &mut ChatTool, item: &Value) {
    tool.state = state_of(item);
    tool.ms = item.get("durationMs").and_then(Value::as_u64).or(tool.ms);
    if let Some(ToolDetail::Command {
        output, background, ..
    }) = &mut tool.detail
    {
        *background = false;
        if let Some(whole) = item.get("aggregatedOutput").and_then(Value::as_str) {
            *output = ends_of(whole, MOST_OUTPUT);
        }
    }
}

fn state_of(item: &Value) -> ToolState {
    match text(item, "status") {
        "completed" => ToolState::Done,
        "failed" => ToolState::Failed,
        "declined" => ToolState::Denied,
        "interrupted" => ToolState::Stopped,
        _ => ToolState::Running,
    }
}

/// A command or a change of files as the chat shows it.
fn described(item: &Value) -> Option<ChatTool> {
    match text(item, "type") {
        "commandExecution" => Some(command(item)),
        "fileChange" => change(item),
        _ => None,
    }
}

/// Codex gives a command as its shell runs it, and the parts that it understood: what the agent
/// asked for, and which of them read a file or search.
fn command(item: &Value) -> ChatTool {
    let actions = list(item, "commandActions").collect::<Vec<_>>();
    let asked = actions
        .iter()
        .map(|action| text(action, "command"))
        .filter(|command| !command.is_empty())
        .collect::<Vec<_>>()
        .join(" && ");
    let command = if asked.is_empty() {
        text(item, "command").to_owned()
    } else {
        asked
    };
    let all = |kinds: &[&str]| {
        !actions.is_empty()
            && actions
                .iter()
                .all(|action| kinds.contains(&text(action, "type")))
    };
    let (kind, target) = if all(&["read"]) {
        let names = actions
            .iter()
            .map(|action| text(action, "name"))
            .collect::<Vec<_>>()
            .join(", ");
        (ToolKind::Read, names)
    } else if all(&["search", "listFiles"]) {
        let subject = actions
            .iter()
            .find_map(|action| words(action, "query").or_else(|| words(action, "path")))
            .unwrap_or_default();
        (ToolKind::Search, subject)
    } else {
        (ToolKind::Command, command.clone())
    };
    ChatTool {
        name: "shell".to_owned(),
        kind,
        state: ToolState::Running,
        target: start_of(&target, MOST_TARGET).to_owned(),
        detail: Some(ToolDetail::Command {
            command: start_of(&command, MOST_OUTPUT).to_owned(),
            output: String::new(),
            background: false,
        }),
        ms: None,
    }
}

fn change(item: &Value) -> Option<ChatTool> {
    let changes = list(item, "changes").collect::<Vec<_>>();
    let first = changes.first()?;
    let lines = changes
        .iter()
        .map(|change| {
            let given = text(change, "diff");
            match text(&change["kind"], "type") {
                "add" if !given.contains("@@") => diff::written(given),
                "delete" if !given.contains("@@") => diff::between(given, ""),
                _ => diff::hunks(given),
            }
        })
        .reduce(diff::Change::then)?;
    let names = changes
        .iter()
        .map(|change| file_name(text(change, "path")))
        .collect::<Vec<_>>()
        .join(", ");
    Some(ChatTool {
        name: "apply_patch".to_owned(),
        kind: if text(&first["kind"], "type") == "add" {
            ToolKind::Write
        } else {
            ToolKind::Edit
        },
        state: ToolState::Running,
        target: start_of(&names, MOST_TARGET).to_owned(),
        detail: Some(ToolDetail::Change {
            path: text(first, "path").to_owned(),
            added: lines.added,
            removed: lines.removed,
            lines: lines.lines,
        }),
        ms: None,
    })
}

/// An item of a kind that the chat has no own view of. It shows with Codex's name of the kind.
fn other(kind: &str, item: &Value) -> Option<ChatTool> {
    let (shown, subject) = match kind {
        "webSearch" => (ToolKind::Web, words(item, "query")),
        "collabAgentToolCall" => (ToolKind::Agent, words(item, "prompt")),
        "mcpToolCall" | "dynamicToolCall" => (ToolKind::Other, words(item, "tool")),
        "imageView" => (
            ToolKind::Read,
            words(item, "path").map(|path| file_name(&path).to_owned()),
        ),
        "imageGeneration" => (ToolKind::Other, None),
        _ => return None,
    };
    Some(ChatTool {
        name: kind.to_owned(),
        kind: shown,
        state: state_of(item),
        target: subject.unwrap_or_default(),
        detail: None,
        ms: None,
    })
}

/// What Codex offers to allow from now on: commands that start as this one does. A command that
/// Codex runs through its shell shows without the shell.
fn rule(amendment: &Value) -> String {
    let words = amendment
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    match words.as_slice() {
        [_, "-lc" | "-c", command] => (*command).to_owned(),
        words => words.join(" "),
    }
}
