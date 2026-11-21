//! Codex's link: the messages of its app-server as changes of the chat.
//!
//! The terminal's Codex works through an app-server that Kodosi starts for it, and Kodosi is a
//! second program of that server. The link carries each message of the server as it is, and makes
//! the calls that this adapter gives it, so what the server's protocol means is all here. A
//! person's answer goes back as the decision that Codex takes. A field can be absent or of another
//! type after an update of Codex, so a message gives what it has and never stops the chat.

use std::collections::{HashMap, VecDeque};

use serde_json::{Map, Value, json};

use super::{
    AskAlways, AskBody, ChatAnswer, ChatAsk, ChatBody, ChatContext, ChatMode, ChatNote, ChatNow,
    ChatOption, ChatPerson, ChatQuestion, ChatState, ChatTask, ChatTool, MOST_OUTPUT, MOST_TARGET,
    MOST_TEXT, PlanChoice, TaskState, ToolDetail, ToolKind, ToolState, TurnEnd,
    agent::{Agent, Stop, answer, file_name, list, person, text, words},
    diff,
    state::{Chat, ends_of, start_of},
};

/// The turns that the chat shows of a conversation that is older than the link.
const HISTORY: u64 = 20;
/// Codex's own words, which its terminal sends when a person chooses to carry out a plan.
const IMPLEMENT: &str = "Implement the plan.";
const MOST_ITEMS: usize = 2048;
const MOST_PASSED: usize = 64;

#[derive(Default)]
pub(super) struct Codex {
    linked: bool,
    /// The conversation of the Codex in the terminal.
    thread: Option<String>,
    follows: Follows,
    /// The conversations of the server that the chat looked at and does not show.
    passed: VecDeque<String>,
    turn: Option<Turn>,
    /// The chat's item of each item of Codex.
    items: HashMap<String, String>,
    asked: HashMap<String, Asked>,
    calls: VecDeque<Value>,
    handed: VecDeque<Handed>,
    sent: u64,
    /// What a person wrote to refuse a question. Codex gets it as the next message.
    said: Option<(String, Option<ChatPerson>)>,
    /// The next message carries out the plan, in Codex's default mode.
    implement: bool,
    /// The chat's item of the plan that the turn that runs wrote.
    plan: Option<String>,
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
    /// Codex's names of the questions of a request for the person's input, in order.
    questions: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AskKind {
    Command,
    Change,
    Question,
    /// Whether to carry out a plan, which Codex's terminal asks after its plan mode wrote one.
    Plan,
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
                    settings(chat, result);
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

    /// A message that Codex did not take stays in the queue with Codex's reason. The words of a
    /// refusal go there too.
    fn refused(&mut self, chat: &mut Chat, n: &str, event: &Value) {
        let n = n.parse::<u64>().ok();
        let Some(index) = self.handed.iter().position(|handed| Some(handed.n) == n) else {
            return;
        };
        let reason = event
            .pointer("/e/message")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Some(handed) = self.handed.remove(index) else {
            return;
        };
        let queued = handed.queued.or_else(|| chat.queue(handed.text, handed.by));
        if let Some(queued) = queued {
            chat.refuse(&queued, start_of(reason, MOST_TARGET).to_owned());
            self.held = true;
        }
    }

    /// The terminal's Codex has one conversation that a person writes in. A conversation that
    /// Codex makes for its own work is not it: the one that names a conversation, which Codex
    /// does not keep, and the one of a helper or of a review, which has a parent.
    fn consider(&mut self, chat: &mut Chat, thread: &Value) {
        let id = text(thread, "id");
        if id.is_empty() {
            return;
        }
        if thread.get("ephemeral").and_then(Value::as_bool) != Some(false)
            || thread
                .get("parentThreadId")
                .is_some_and(|parent| !parent.is_null())
            || thread.pointer("/source/subAgent").is_some()
        {
            self.pass(id);
            return;
        }
        self.passed.retain(|passed| passed != id);
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

    fn pass(&mut self, thread: &str) {
        if self.passed.iter().any(|passed| passed == thread) {
            return;
        }
        if self.passed.len() >= MOST_PASSED {
            self.passed.pop_front();
        }
        self.passed.push_back(thread.to_owned());
    }

    /// Codex works in a conversation that is not the chat's. When a person went to it in the
    /// terminal, the server tells no more than this, so the chat reads what the conversation is.
    fn look(&mut self, thread: &str) {
        if thread.is_empty() || self.passed.iter().any(|passed| passed == thread) {
            return;
        }
        self.pass(thread);
        self.call(
            "read",
            "thread/read",
            &json!({"threadId": thread, "includeTurns": false}),
        );
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
        self.implement = false;
        self.plan = None;
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
            "thread/status/changed" if !self.is_ours(params) => {
                if text(&params["status"], "type") == "active" {
                    self.look(text(params, "threadId"));
                }
            }
            // Codex resumed a conversation, as after a restart of its terminal: the server tells
            // the other programs only that the conversation has no goal now.
            "thread/goal/cleared" if !self.is_ours(params) => self.look(text(params, "threadId")),
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
                self.plan = None;
                // A person chose in the terminal, or wrote there.
                return self.close_plan(chat);
            }
            "turn/completed" => {
                let closed = self.end_turn(chat, &params["turn"], at);
                // Codex has each message that the chat gave it before the turn. One that Codex
                // did not write down in the chat's words leaves the queue here.
                for handed in std::mem::take(&mut self.handed) {
                    if let Some(queued) = handed.queued {
                        chat.unqueue(&queued);
                    }
                }
                self.offer_plan(chat, &params["turn"]);
                return closed;
            }
            "turn/plan/updated" => chat.set_tasks(list(params, "plan").map(|step| ChatTask {
                text: text(step, "step").to_owned(),
                state: match text(step, "status") {
                    "completed" => TaskState::Done,
                    "inProgress" => TaskState::Working,
                    _ => TaskState::Pending,
                },
            })),
            "item/started" => self.item(chat, params, at, false),
            "item/completed" => self.item(chat, params, at, true),
            "item/agentMessage/delta" | "item/plan/delta" | "item/commandExecution/outputDelta" => {
                self.piece(chat, text(event, "m"), params);
            }
            "serverRequest/resolved" => {
                let Some(ask) = Self::ask_name(&params["requestId"]) else {
                    return Vec::new();
                };
                // A person answered in the terminal, or the chat's stop ended the turn. What Codex
                // did with the answer comes with the item.
                if let Some(asked) = self.asked.remove(&ask) {
                    let answered = match asked.kind {
                        AskKind::Question
                            if self.turn.as_ref().is_some_and(|turn| turn.stopped) =>
                        {
                            ToolState::Stopped
                        }
                        AskKind::Question | AskKind::Plan => ToolState::Done,
                        AskKind::Command | AskKind::Change => ToolState::Running,
                    };
                    chat.change_tool(&asked.item, |tool| {
                        if tool.state == ToolState::Asking {
                            tool.state = answered;
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

    /// More of an item that grows: the answer that Codex writes, its plan, or what a command
    /// prints.
    fn piece(&mut self, chat: &mut Chat, method: &str, params: &Value) {
        let Some(item) = self.items.get(text(params, "itemId")).cloned() else {
            return;
        };
        let more = text(params, "delta");
        match method {
            "item/agentMessage/delta" => {
                chat.write(&item, more);
                self.now = Some(ChatNow::Writing);
            }
            "item/plan/delta" => chat.change_tool(&item, |tool| {
                if let Some(ToolDetail::Plan { text }) = &mut tool.detail {
                    let room = MOST_TEXT.saturating_sub(text.len());
                    text.push_str(start_of(more, room));
                }
            }),
            _ => chat.change_tool(&item, |tool| {
                if let Some(ToolDetail::Command { output, .. }) = &mut tool.detail {
                    *output = ends_of(&format!("{output}{more}"), MOST_OUTPUT);
                }
            }),
        }
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
        if done
            && let Some(shown) = &known
            && matches!(&self.now, Some(ChatNow::Tool { item }) if item == shown)
        {
            self.now = None;
        }
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
            ("plan", None) => Some(chat.push(
                at,
                ChatBody::Tool {
                    tool: Box::new(plan(item, done)),
                },
            )),
            // The pieces of a plan need not add up to the plan, so the whole plan replaces them.
            ("plan", Some(shown)) if done => {
                chat.put(
                    &shown,
                    ChatBody::Tool {
                        tool: Box::new(plan(item, true)),
                    },
                );
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
        if done && text(item, "type") == "plan" {
            self.plan = self.items.get(id).cloned();
        }
    }

    /// After a turn of its plan mode that wrote a plan, Codex's terminal asks whether to carry the
    /// plan out, unless a message waits, and the chat asks it too. The answer that comes first
    /// counts: the chat's starts the work, as the terminal's does, and the terminal's starts a
    /// turn, which ends the chat's question.
    fn offer_plan(&mut self, chat: &mut Chat, turn: &Value) {
        let Some(item) = self.plan.take() else {
            return;
        };
        if text(turn, "status") != "completed"
            || chat.facts().mode != Some(ChatMode::Plan)
            || !chat.queued().is_empty()
            || self.said.is_some()
        {
            return;
        }
        let Some(ToolDetail::Plan { text }) = chat.tool(&item).and_then(|tool| tool.detail.clone())
        else {
            return;
        };
        chat.change_tool(&item, |tool| tool.state = ToolState::Asking);
        let ask = format!("plan:{item}");
        chat.ask(ChatAsk {
            id: ask.clone(),
            item: Some(item.clone()),
            body: AskBody::Plan {
                text,
                choices: vec![PlanChoice::Go],
            },
        });
        self.asked.insert(
            ask,
            Asked {
                kind: AskKind::Plan,
                item,
                offered: Vec::new(),
                questions: Vec::new(),
            },
        );
    }

    fn close_plan(&mut self, chat: &mut Chat) -> Vec<String> {
        let planned = self
            .asked
            .iter()
            .filter(|(_, asked)| asked.kind == AskKind::Plan)
            .map(|(ask, asked)| (ask.clone(), asked.item.clone()))
            .collect::<Vec<_>>();
        for (ask, item) in &planned {
            self.asked.remove(ask);
            chat.change_tool(item, |tool| tool.state = ToolState::Done);
        }
        chat.close_asks(|open| planned.iter().any(|(ask, _)| *ask == open.id))
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
            "item/tool/requestUserInput" => return self.question(chat, event),
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
            AskKind::Question | AskKind::Plan => None,
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
                questions: Vec::new(),
            },
        );
    }

    /// Codex asks the person questions, as its plan mode does before a plan. The call has no item
    /// of Codex, so the chat keeps the questions as a step of its own. A question with a secret
    /// answer is for the terminal alone.
    fn question(&mut self, chat: &mut Chat, event: &Value) {
        let params = event.get("p").unwrap_or(&Value::Null);
        let Some(ask) = event.get("id").and_then(Self::ask_name) else {
            return;
        };
        let given = list(params, "questions").take(8).collect::<Vec<_>>();
        if !self.is_ours(params)
            || given.is_empty()
            || given
                .iter()
                .any(|question| question.get("isSecret").and_then(Value::as_bool) == Some(true))
        {
            return;
        }
        let questions = given
            .iter()
            .map(|question| ChatQuestion {
                question: start_of(text(question, "question"), MOST_TARGET * 2).to_owned(),
                header: start_of(text(question, "header"), MOST_TARGET).to_owned(),
                options: list(question, "options")
                    .take(16)
                    .map(|option| ChatOption {
                        label: start_of(text(option, "label"), MOST_TARGET).to_owned(),
                        description: start_of(text(option, "description"), MOST_TARGET * 2)
                            .to_owned(),
                    })
                    .collect(),
                multiple: false,
            })
            .collect::<Vec<_>>();
        let at = event.get("t").and_then(Value::as_u64).unwrap_or_default();
        let item = chat.push(
            at,
            ChatBody::Tool {
                tool: Box::new(ChatTool {
                    name: "request_user_input".to_owned(),
                    kind: ToolKind::Question,
                    state: ToolState::Asking,
                    target: questions[0].question.clone(),
                    detail: Some(ToolDetail::Question {
                        questions: questions.clone(),
                        answers: Vec::new(),
                    }),
                    ms: None,
                }),
            },
        );
        chat.ask(ChatAsk {
            id: ask.clone(),
            item: Some(item.clone()),
            body: AskBody::Question { questions },
        });
        self.asked.insert(
            ask,
            Asked {
                kind: AskKind::Question,
                item,
                offered: Vec::new(),
                questions: given
                    .iter()
                    .map(|question| text(question, "id").to_owned())
                    .collect(),
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
        let open = open(chat);
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
            stop_open(chat);
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
        self.implement = false;
        self.plan = None;
        self.handed.clear();
        self.calls.clear();
        self.stopped.clear();
        stop_open(chat);
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
    fn decision(
        &mut self,
        ask: &str,
        answer: &ChatAnswer,
        by: Option<&ChatPerson>,
    ) -> Option<Value> {
        let asked = self.asked.get(ask)?;
        match (asked.kind, answer) {
            (AskKind::Question, answer) => return answers_for(&asked.questions, answer),
            (
                AskKind::Plan,
                ChatAnswer::Plan {
                    choice: PlanChoice::Go,
                },
            ) => {
                self.said = Some((IMPLEMENT.to_owned(), by.cloned()));
                self.implement = true;
                return Some(json!({"implement": true}));
            }
            // As the terminal's "No, stay in Plan mode": words go to Codex as the next message.
            (AskKind::Plan, ChatAnswer::Deny { text }) => {
                self.said = text
                    .as_deref()
                    .map(str::trim)
                    .filter(|words| !words.is_empty())
                    .map(|words| (words.to_owned(), by.cloned()));
                return Some(json!({"implement": false}));
            }
            (AskKind::Plan, _) => return None,
            _ => {}
        }
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
                    .map(|words| (words.to_owned(), by.cloned()));
                json!("cancel")
            }
            _ => return None,
        };
        Some(json!({"decision": decision}))
    }

    fn answered(&mut self, chat: &mut Chat, ask: &str, decision: &Value) -> bool {
        let Some(asked) = self.asked.remove(ask) else {
            return true;
        };
        chat.close_asks(|open| open.id == ask);
        // The adapter asked about the plan for Codex, and the decision starts a turn.
        if asked.kind == AskKind::Plan {
            chat.change_tool(&asked.item, |tool| tool.state = ToolState::Done);
            self.show(chat);
            return false;
        }
        if asked.kind == AskKind::Question {
            let given = asked
                .questions
                .iter()
                .map(|id| {
                    list(&decision["answers"][id], "answers")
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .collect::<Vec<_>>();
            chat.change_tool(&asked.item, |tool| {
                tool.state = ToolState::Done;
                if let Some(ToolDetail::Question { answers, .. }) = &mut tool.detail {
                    *answers = given;
                }
            });
            self.show(chat);
            return true;
        }
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
        true
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
        let implement = std::mem::take(&mut self.implement) && self.said.is_some();
        let next = self
            .said
            .take()
            .map(|(said, by)| (None, said, by))
            .or_else(|| {
                let queued = chat.queued().first()?;
                Some((
                    Some(queued.id.clone()),
                    queued.text.clone(),
                    queued.by.clone(),
                ))
            });
        if let Some((queued, text, by)) = next {
            self.sent += 1;
            let mut start = json!({
                "threadId": thread,
                "input": [{"type": "text", "text": text, "text_elements": []}],
            });
            // As the terminal's own choice: Codex's default mode with the model and the effort of
            // the conversation, and that mode's own instructions.
            if let (true, Some(model)) = (implement, &chat.facts().model) {
                start["collaborationMode"] = json!({
                    "mode": "default",
                    "settings": {
                        "model": model,
                        "reasoning_effort": chat.facts().effort,
                        "developer_instructions": null,
                    },
                });
            }
            commands.push(json!({
                "do": "call",
                "tag": format!("prompt:{}", self.sent),
                "m": "turn/start",
                "p": start,
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
        let questions = self
            .asked
            .values()
            .any(|asked| asked.kind == AskKind::Question);
        if !chat.asks().is_empty() && !questions {
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

/// The answer that the agent wrote and each call that did not end stay as far as they came.
fn stop_open(chat: &mut Chat) {
    for item in open(chat) {
        match chat.body(&item).cloned() {
            Some(ChatBody::Text { text, .. }) => chat.put(&item, answer(&text, false)),
            _ => chat.change_tool(&item, |tool| tool.state = ToolState::Stopped),
        }
    }
}

fn open(chat: &Chat) -> Vec<String> {
    chat.items()
        .iter()
        .filter_map(|item| match &item.body {
            ChatBody::Tool { tool } if tool.state.is_open() => Some(item.id.clone()),
            ChatBody::Text { live: true, .. } => Some(item.id.clone()),
            _ => None,
        })
        .collect()
}

/// What a conversation tells of the agent that works in it.
fn describe(chat: &mut Chat, thread: &Value) {
    chat.change_facts(|facts| {
        facts.model = words(thread, "model").or_else(|| facts.model.take());
        facts.effort = words(thread, "reasoningEffort").or_else(|| facts.effort.take());
        facts.folder = words(thread, "cwd").or_else(|| facts.folder.take());
    });
}

/// The settings of a conversation, as a change of them gives them or as the answer to the
/// request to follow does, which has other names for two of them.
fn settings(chat: &mut Chat, settings: &Value) {
    let sandbox = settings
        .get("sandboxPolicy")
        .or_else(|| settings.get("sandbox"))
        .unwrap_or(&Value::Null);
    let effort = words(settings, "effort").or_else(|| words(settings, "reasoningEffort"));
    let mode = if text(&settings["collaborationMode"], "mode") == "plan" {
        Some(ChatMode::Plan)
    } else if text(settings, "approvalsReviewer") != "user"
        && !text(settings, "approvalsReviewer").is_empty()
    {
        Some(ChatMode::Auto)
    } else {
        match (text(settings, "approvalPolicy"), text(sandbox, "type")) {
            ("never", "dangerFullAccess") => Some(ChatMode::Unrestricted),
            ("never", _) => Some(ChatMode::Rules),
            (_, "readOnly") => Some(ChatMode::Ask),
            (_, "workspaceWrite") => Some(ChatMode::Edits),
            _ => None,
        }
    };
    chat.change_facts(|facts| {
        facts.model = words(settings, "model").or_else(|| facts.model.take());
        facts.effort = effort.or_else(|| facts.effort.take());
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
/// Codex's plan, as its plan mode writes it.
fn plan(item: &Value, done: bool) -> ChatTool {
    ChatTool {
        name: "plan".to_owned(),
        kind: ToolKind::Plan,
        state: if done {
            ToolState::Done
        } else {
            ToolState::Running
        },
        target: String::new(),
        detail: Some(ToolDetail::Plan {
            text: start_of(text(item, "text"), MOST_TEXT).to_owned(),
        }),
        ms: None,
    }
}

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

/// The answers to Codex's questions: for each question in order, the labels that the person chose,
/// or their own words. Words in place of the answers answer the first question, as Codex's own
/// "Other" does.
fn answers_for(questions: &[String], answer: &ChatAnswer) -> Option<Value> {
    let given = match answer {
        ChatAnswer::Answers { answers } if answers.len() == questions.len() => answers.clone(),
        ChatAnswer::Deny { text: Some(words) } if !words.trim().is_empty() => {
            let mut given = vec![Vec::new(); questions.len()];
            given[0] = vec![words.trim().to_owned()];
            given
        }
        _ => return None,
    };
    if given
        .iter()
        .take(if matches!(answer, ChatAnswer::Deny { .. }) {
            1
        } else {
            given.len()
        })
        .any(|labels| labels.iter().all(|label| label.trim().is_empty()))
    {
        return None;
    }
    let answers = questions
        .iter()
        .zip(given)
        .map(|(id, labels)| (id.clone(), json!({"answers": labels})))
        .collect::<Map<_, _>>();
    Some(json!({"answers": answers}))
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
