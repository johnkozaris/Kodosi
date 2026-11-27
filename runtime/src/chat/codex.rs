//! Chat adapter for Codex via its `app-server` protocol.
//!
//! `codex_link` spawns and proxies the app-server and forwards raw JSON-RPC messages here; this
//! adapter tracks the terminal's active thread and translates updates, approvals, and plan flows.
//! Field parsing is lenient so schema changes across Codex updates degrade gracefully.

use std::collections::{HashMap, VecDeque};

use serde_json::{Map, Value, json};

use super::{
    AskAlways, AskBody, ChatAnswer, ChatAsk, ChatBody, ChatContext, ChatGiven, ChatMode, ChatNote,
    ChatNow, ChatPerson, ChatPicture, ChatTask, ChatTool, MOST_OUTPUT, MOST_QUESTIONS, MOST_TARGET,
    MOST_TEXT, PlanChoice, TaskState, ToolDetail, ToolKind, ToolState, TurnEnd,
    agent::{
        Agent, Places, Stop, answer, list, open_items, person, pictured, plan_command, question,
        remember, show_state, stop_open, text, words,
    },
    diff,
    mention::picture,
    state::{Chat, ends_of, start_of},
};

const HISTORY: u64 = 20;
/// Prompt Codex's terminal sends when the user chooses to implement a plan.
const IMPLEMENT: &str = "Implement the plan.";
const MOST_PASSED: usize = 64;

#[derive(Default)]
pub(super) struct Codex {
    linked: bool,
    thread: Option<String>,
    follows: Follows,
    /// Thread IDs already inspected and ignored (ephemeral, subagent, or already probed).
    passed: VecDeque<String>,
    turn: Option<Turn>,
    items: HashMap<String, String>,
    asked: HashMap<String, Asked>,
    calls: VecDeque<Value>,
    handed: VecDeque<Handed>,
    sent: u64,
    /// Follow-up prompt queued from a plan decision or denial reason, sent on the next turn.
    said: Option<(String, Option<ChatPerson>)>,
    /// When true, the next prompt switches back to default mode to execute the plan.
    implement: bool,
    /// Cached `(model, effort)` from default mode, restored when leaving plan mode to implement.
    default_mode: Option<(String, Option<String>)>,
    plan_mode: Option<Value>,
    plan: Option<String>,
    /// Item IDs of commands interrupted by a chat stop; Codex leaves them running as background
    /// terminals unless terminated explicitly.
    stopped: Vec<String>,
    now: Option<ChatNow>,
    held: bool,
}

/// Subscription state for `thread/resume`. Codex only streams thread events to subscribed
/// clients and rejects `thread/resume` until the thread's first turn has started.
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
    /// Codex did not take a message into this turn, so the next ones wait for its end.
    refuses_messages: bool,
}

struct Asked {
    kind: AskKind,
    item: String,
    offered: Vec<Value>,
    /// Codex question IDs in display order, used to key `requestUserInput` responses.
    questions: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AskKind {
    Command,
    Change,
    Question,
    Plan,
}

struct Handed {
    n: u64,
    queued: Option<String>,
    text: String,
    by: Option<ChatPerson>,
    /// Given to the running turn, and Codex has not yet answered whether the turn took it.
    steering: bool,
}

impl Codex {
    pub(super) const NAME: &'static str = "codex";

    pub(super) fn ask_name(request: &Value) -> Option<String> {
        match request {
            Value::Number(number) => Some(format!("r{number}")),
            Value::String(name) if !name.is_empty() => Some(format!("r{name}")),
            _ => None,
        }
    }

    fn show(&self, chat: &mut Chat) {
        show_state(chat, self.linked, self.turn.is_some(), self.now.clone());
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
        // `server` is a user-agent string such as "codex-tui/0.171.0 (Mac OS 27.2.1; arm64)".
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
        self.call("modes", "collaborationMode/list", &json!({}));
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
            ("limits", Some(result)) => limits(chat, result),
            ("modes", Some(result)) => {
                self.plan_mode = list(result, "data")
                    .find(|mask| text(mask, "mode") == "plan")
                    .cloned();
            }
            ("plan", Some(_)) => {
                let n = name.parse::<u64>().ok();
                if let Some(index) = self.handed.iter().position(|handed| Some(handed.n) == n)
                    && let Some(handed) = self.handed.remove(index)
                    && let Some(queued) = &handed.queued
                {
                    chat.unqueue(queued);
                }
            }
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
                    self.saw_settings(result);
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
            ("prompt" | "plan" | "compact", None) => self.refused(chat, name, event),
            ("compact", Some(_)) => self.compacts(chat, name, event),
            ("steer", Some(_)) => self.steered(chat, name),
            ("steer", None) => self.not_steered(name),
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

    /// Codex started to compact the thread for a command of the chat, which shows as its person's
    /// message.
    fn compacts(&mut self, chat: &mut Chat, n: &str, event: &Value) {
        let n = n.parse::<u64>().ok();
        if let Some(index) = self.handed.iter().position(|handed| Some(handed.n) == n)
            && let Some(handed) = self.handed.remove(index)
        {
            if let Some(queued) = &handed.queued {
                chat.unqueue(queued);
            }
            let at = event.get("t").and_then(Value::as_u64).unwrap_or_default();
            chat.push(at, person(&handed.text, handed.by));
        }
    }

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
        let queued = handed
            .queued
            .or_else(|| chat.queue(handed.text, handed.by, Vec::new()));
        if let Some(queued) = queued {
            chat.refuse(&queued, Some(start_of(reason, MOST_TARGET).to_owned()));
            self.held = true;
        }
    }

    /// Codex took a message into its turn. When the turn ended before that answer, Codex shows
    /// nothing more of the message, so it leaves the queue now.
    fn steered(&mut self, chat: &mut Chat, n: &str) {
        let n = n.parse::<u64>().ok();
        let Some(index) = self.handed.iter().position(|handed| Some(handed.n) == n) else {
            return;
        };
        if self.turn.is_some() {
            self.handed[index].steering = false;
            if let Some(queued) = &self.handed[index].queued {
                chat.give(queued, ChatGiven::Step);
            }
        } else if let Some(Handed {
            queued: Some(queued),
            ..
        }) = self.handed.remove(index)
        {
            chat.unqueue(&queued);
        }
    }

    /// Codex did not take a message into its turn, as when the turn ended meanwhile; the message
    /// stays in the queue for the next turn.
    fn not_steered(&mut self, n: &str) {
        let n = n.parse::<u64>().ok();
        if let Some(index) = self.handed.iter().position(|handed| Some(handed.n) == n) {
            self.handed.remove(index);
        }
        if let Some(turn) = &mut self.turn {
            turn.refuses_messages = true;
        }
    }

    /// The first queued message for the turn that runs, as Enter gives it in Codex's terminal. A
    /// command, and the follow-up of a plan decision, wait for the turn to end.
    fn steer(&mut self, chat: &Chat) -> Option<Value> {
        let turn = self
            .turn
            .as_ref()
            .filter(|turn| !turn.stopped && !turn.refuses_messages)?
            .id
            .clone()?;
        let thread = self.thread.clone()?;
        if self.said.is_some() {
            return None;
        }
        let queued = chat
            .queued()
            .iter()
            .find(|queued| !queued.terminal)
            .filter(|queued| !queued.said.trim_start().starts_with(['/', '!']))?;
        self.sent += 1;
        self.handed.push_back(Handed {
            n: self.sent,
            queued: Some(queued.id.clone()),
            text: queued.said.clone(),
            by: queued.by.clone(),
            steering: true,
        });
        Some(json!({
            "do": "call",
            "tag": format!("steer:{}", self.sent),
            "m": "turn/steer",
            "p": {
                "threadId": thread,
                "expectedTurnId": turn,
                "input": input(&queued.said, &queued.files),
            },
        }))
    }

    /// Adopts the terminal's main user thread, skipping internal threads Codex creates for title
    /// generation (`ephemeral`) and subagents or reviews (`parentThreadId` / `source.subAgent`).
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

    /// Probes an unknown thread ID via `thread/read` to check whether the user switched to it in
    /// the terminal.
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

    fn plan_settings(&self, chat: &Chat) -> Option<Value> {
        let facts = chat.facts();
        let mask = self.plan_mode.as_ref();
        let model = mask
            .and_then(|mask| words(mask, "model"))
            .or_else(|| facts.model.clone())?;
        let effort = mask
            .and_then(|mask| mask.get("reasoning_effort"))
            .cloned()
            .unwrap_or_else(|| json!(facts.effort));
        Some(json!({
            "mode": "plan",
            "settings": {"model": model, "reasoning_effort": effort, "developer_instructions": null},
        }))
    }

    fn saw_settings(&mut self, settings: &Value) {
        if text(&settings["collaborationMode"], "mode") == "plan" {
            return;
        }
        if let Some(model) = words(settings, "model") {
            let effort = words(settings, "effort").or_else(|| words(settings, "reasoningEffort"));
            self.default_mode = Some((model, effort));
        }
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
            // When the terminal resumes an existing thread, `thread/goal/cleared` is the only
            // notification other clients receive.
            "thread/goal/cleared" if !self.is_ours(params) => self.look(text(params, "threadId")),
            _ if !self.is_ours(params) => {}
            "thread/status/changed" => match text(&params["status"], "type") {
                "active" => {
                    self.turn.get_or_insert_default();
                    self.follow();
                }
                // Before `thread/resume` succeeds we do not receive `turn/completed`, so status
                // transitions are the only turn-end signal.
                _ if self.follows != Follows::Yes => self.turn = None,
                _ => {}
            },
            "turn/started" => {
                self.turn = Some(Turn {
                    id: words(&params["turn"], "id"),
                    ..Turn::default()
                });
                self.now = None;
                self.held = false;
                self.plan = None;
                // Starting a turn dismisses any pending chat plan prompt (e.g., answered in the
                // terminal).
                return self.close_plan(chat);
            }
            "turn/completed" => {
                let closed = self.end_turn(chat, &params["turn"], at);
                // Unqueue any handed prompts whose echoed text did not match verbatim in
                // `said_by_person`. A message given to the turn waits for Codex's answer, which
                // says whether the turn took it before it ended.
                let (steering, handed) = std::mem::take(&mut self.handed)
                    .into_iter()
                    .partition(|handed| handed.steering);
                self.handed = steering;
                for handed in handed {
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
                // Resolved outside chat (answered in the terminal or interrupted); the final
                // outcome arrives with the completed item.
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
            "thread/settings/updated" => {
                settings(chat, &params["threadSettings"]);
                self.saw_settings(&params["threadSettings"]);
            }
            _ => {}
        }
        Vec::new()
    }

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
        let places = &Places::of(chat);
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
            ("contextCompaction", _) => {
                self.compaction(chat, at, done);
                None
            }
            ("plan", None) => Some(chat.push(
                at,
                ChatBody::Tool {
                    tool: Box::new(plan(item, done)),
                },
            )),
            // Plan deltas do not always concatenate to the final text, so replace with the full
            // completed plan.
            ("plan", Some(shown)) if done => {
                chat.put(
                    &shown,
                    ChatBody::Tool {
                        tool: Box::new(plan(item, true)),
                    },
                );
                None
            }
            ("commandExecution" | "fileChange", None) => described(item, places).map(|mut tool| {
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
                    // Commands terminated after a chat stop report as failed; preserve `Stopped`.
                    let stopped = tool.state == ToolState::Stopped;
                    end(tool, item);
                    if stopped {
                        tool.state = ToolState::Stopped;
                    }
                });
                None
            }
            (kind, None) => other(kind, item, places).map(|tool| {
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
            if !done && chat.tool(&shown).is_some() {
                self.now = Some(ChatNow::Tool {
                    item: shown.clone(),
                });
            }
            remember(&mut self.items, chat, id, shown);
        }
        if done && text(item, "type") == "plan" {
            self.plan = self.items.get(id).cloned();
        }
    }

    /// `/compact` runs Codex's own compaction of the thread, as in its terminal.
    fn compact(
        &mut self,
        thread: &str,
        queued: Option<String>,
        text: String,
        by: Option<ChatPerson>,
    ) -> Value {
        self.handed.push_back(Handed {
            n: self.sent,
            queued,
            text,
            by,
            steering: false,
        });
        json!({
            "do": "call",
            "tag": format!("compact:{}", self.sent),
            "m": "thread/compact/start",
            "p": {"threadId": thread},
        })
    }

    fn compaction(&mut self, chat: &mut Chat, at: u64, done: bool) {
        self.now = (!done).then_some(ChatNow::Compacting);
        if done {
            chat.push(
                at,
                ChatBody::Note {
                    note: ChatNote::Compacted,
                },
            );
        }
    }

    /// Synthesizes a plan-approval ask after a plan-mode turn, matching Codex's terminal prompt
    /// (the app-server does not emit a server request for this). Whichever side answers first
    /// wins: a terminal response starts a new turn, which dismisses this ask in `close_plan`.
    fn offer_plan(&mut self, chat: &mut Chat, turn: &Value) {
        let Some(item) = self.plan.take() else {
            return;
        };
        if text(turn, "status") != "completed"
            || chat.facts().mode != Some(ChatMode::Plan)
            || chat.queued().iter().any(|queued| !queued.terminal)
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

    fn said_by_person(&mut self, chat: &mut Chat, item: &Value, at: u64) -> Option<String> {
        let said = list(item, "content")
            .filter(|part| text(part, "type") == "text")
            .map(|part| text(part, "text"))
            .collect::<Vec<_>>()
            .join("\n");
        let pictures = list(item, "content")
            .filter_map(|part| match text(part, "type") {
                "localImage" => Some(picture(text(part, "path"))),
                "image" => Some(ChatPicture {
                    name: String::new(),
                }),
                _ => None,
            })
            .collect::<Vec<_>>();
        if said.is_empty() && pictures.is_empty() {
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
        Some(chat.push(at, pictured(&said, by, pictures)))
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

    /// Handles `item/tool/requestUserInput`. Synthesizes a chat tool item because Codex creates
    /// none for user-input requests. Secret prompts (`isSecret`) are skipped so sensitive input
    /// stays in the terminal.
    fn question(&mut self, chat: &mut Chat, event: &Value) {
        let params = event.get("p").unwrap_or(&Value::Null);
        let Some(ask) = event.get("id").and_then(Self::ask_name) else {
            return;
        };
        let given = list(params, "questions")
            .take(MOST_QUESTIONS)
            .collect::<Vec<_>>();
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
            .map(|each| question(each, false))
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
        let open = open_items(chat);
        for item in &open {
            match chat.body(item).cloned() {
                Some(ChatBody::Text { text, .. }) => chat.put(item, answer(&text, false)),
                // Terminal interrupts leave running commands alive in the background (whereas a
                // chat stop terminates them below).
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

    fn conversation(&self) -> Option<&str> {
        self.thread.as_deref()
    }

    fn retry(&mut self) {
        self.held = false;
    }

    fn takes(&self, text: &str) -> bool {
        command_of(text).is_none_or(|name| matches!(name, "compact" | "plan"))
    }

    fn has(&self, queued: &str) -> bool {
        self.handed
            .iter()
            .any(|handed| handed.queued.as_deref() == Some(queued))
    }

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
            // "No, stay in Plan mode": follow-up text becomes the next prompt.
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
                // Codex's "cancel" carries no reason field, so send refusal text as the next
                // prompt (matching its terminal "No, and tell Codex what to do differently" flow).
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
        // Plan approval is synthesized locally; return false so no server reply is sent.
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

    fn commands(&mut self, chat: &mut Chat) -> Vec<Value> {
        let mut commands = self.calls.drain(..).collect::<Vec<_>>();
        let free = self.linked && !self.held && self.handed.is_empty() && chat.asks().is_empty();
        if free && let Some(steer) = self.steer(chat) {
            commands.push(steer);
            return commands;
        }
        let ready = free && self.turn.is_none();
        let (true, Some(thread)) = (ready, self.thread.clone()) else {
            return commands;
        };
        let implement = std::mem::take(&mut self.implement) && self.said.is_some();
        let next = self
            .said
            .take()
            .map(|(said, by)| (None, said, by, Vec::new()))
            .or_else(|| {
                let queued = chat.queued().iter().find(|queued| !queued.terminal)?;
                Some((
                    Some(queued.id.clone()),
                    queued.said.clone(),
                    queued.by.clone(),
                    queued.files.clone(),
                ))
            });
        if let Some((queued, text, by, files)) = next {
            self.sent += 1;
            if command_of(&text) == Some("compact") {
                commands.push(self.compact(&thread, queued, text, by));
                return commands;
            }
            let plan = plan_command(&text).map(str::to_owned);
            let text = plan.clone().unwrap_or(text);
            let plan_mode = plan.as_ref().and_then(|_| self.plan_settings(chat));
            if plan.as_deref() == Some("") {
                if let Some(mode) = plan_mode {
                    commands.push(json!({
                        "do": "call",
                        "tag": format!("plan:{}", self.sent),
                        "m": "thread/settings/update",
                        "p": {"threadId": thread, "collaborationMode": mode},
                    }));
                    self.handed.push_back(Handed {
                        n: self.sent,
                        queued,
                        text,
                        by,
                        steering: false,
                    });
                }
                return commands;
            }
            let mut start = json!({
                "threadId": thread,
                "input": input(&text, &files),
            });
            if let Some(mode) = plan_mode {
                start["collaborationMode"] = mode;
            }
            // Restore default mode's model, effort, and built-in instructions when implementing.
            let default_mode = self.default_mode.clone().or_else(|| {
                let facts = chat.facts();
                facts
                    .model
                    .clone()
                    .map(|model| (model, facts.effort.clone()))
            });
            if let (true, Some((model, effort))) = (implement, default_mode) {
                start["collaborationMode"] = json!({
                    "mode": "default",
                    "settings": {
                        "model": model,
                        "reasoning_effort": effort,
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
                steering: false,
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

/// A message as Codex takes it into a turn: its words, then each picture as a file of this
/// computer, which Codex shows its model itself.
fn input(text: &str, files: &[String]) -> Value {
    let words = (!text.is_empty() || files.is_empty())
        .then(|| json!({"type": "text", "text": text, "text_elements": []}));
    words
        .into_iter()
        .chain(
            files
                .iter()
                .map(|file| json!({"type": "localImage", "path": file})),
        )
        .collect()
}

/// The command that a message names, as Codex's composer reads a word after a slash.
fn command_of(text: &str) -> Option<&str> {
    let name = text
        .trim_start()
        .strip_prefix('/')?
        .split_whitespace()
        .next()?;
    name.bytes()
        .all(|letter| {
            letter.is_ascii_lowercase() || letter.is_ascii_digit() || matches!(letter, b'-' | b'_')
        })
        .then_some(name)
}

fn describe(chat: &mut Chat, thread: &Value) {
    chat.change_facts(|facts| {
        facts.model = words(thread, "model").or_else(|| facts.model.take());
        facts.effort = words(thread, "reasoningEffort").or_else(|| facts.effort.take());
        facts.folder = words(thread, "cwd").or_else(|| facts.folder.take());
    });
}

/// Extracts thread settings from either `thread/settings/updated` or `thread/resume`, which use
/// different field names for sandbox policy and reasoning effort.
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
    let limits = crate::agents::codex_limits(result);
    if !limits.is_empty() {
        chat.change_facts(|facts| facts.limits = limits);
    }
}

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

fn described(item: &Value, places: &Places) -> Option<ChatTool> {
    match text(item, "type") {
        "commandExecution" => Some(command(item, places)),
        "fileChange" => change(item, places),
        _ => None,
    }
}

/// Maps a Codex `commandExecution` item, preferring parsed `commandActions` over the raw shell
/// wrapper to classify pure reads and searches.
fn command(item: &Value, places: &Places) -> ChatTool {
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
            .map(|action| match text(action, "path") {
                "" => text(action, "name").to_owned(),
                path => places.file(path),
            })
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

fn change(item: &Value, places: &Places) -> Option<ChatTool> {
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
        .map(|change| places.file(text(change, "path")))
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
        detail: Some(lines.detail(text(first, "path").to_owned())),
        ms: None,
    })
}

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

fn other(kind: &str, item: &Value, places: &Places) -> Option<ChatTool> {
    let (shown, subject) = match kind {
        "webSearch" => (ToolKind::Web, words(item, "query")),
        "collabAgentToolCall" => (ToolKind::Agent, words(item, "prompt")),
        "mcpToolCall" | "dynamicToolCall" => (ToolKind::Other, words(item, "tool")),
        "imageView" => (
            ToolKind::Read,
            words(item, "path").map(|path| places.file(&path)),
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

/// Formats answers for `requestUserInput`. Free-text `Deny` input fills the first question,
/// matching Codex's "Other" option.
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
