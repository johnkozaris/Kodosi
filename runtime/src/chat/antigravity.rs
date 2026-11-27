//! Chat adapter for Antigravity CLI over the Agent Client Protocol (ACP).
//!
//! Antigravity CLI has no extension interface for its terminal app to report through, so its chat
//! runs a session of its own: `kodosi acp` runs `agy acp` in the terminal and forwards raw ACP
//! messages here; this adapter drives the session lifecycle and translates updates and permission
//! responses. Field parsing is lenient so schema changes across agent updates degrade gracefully
//! instead of breaking chat.

use std::collections::{HashMap, VecDeque};

use serde_json::{Value, json};

use super::{
    AskAlways, AskBody, ChatAnswer, ChatAsk, ChatBody, ChatContext, ChatMode, ChatNote, ChatNow,
    ChatOption, ChatPerson, ChatQuestion, ChatTask, ChatTool, MOST_OPTIONS, MOST_OUTPUT,
    MOST_QUESTIONS, MOST_TARGET, MOST_TEXT, TaskState, ToolDetail, ToolKind, ToolState, TurnEnd,
    agent::{
        Agent, Places, Stop, answer, list, open_items, person, remember, show_state, stop_open,
        text, words,
    },
    diff,
    state::{Chat, ends_of, start_of},
};
use crate::terminal::TerminalFace;

/// ACP JSON-RPC error code for "Authentication required".
const SIGNED_OUT: i64 = -32000;
/// Tool name for agent questions, which end the turn and expect the user's next message as the
/// answer rather than a JSON-RPC reply.
const ASK_QUESTION: &str = "ask_question";
/// Synthetic commands handled by `acp.rs` rather than forwarded to the ACP server.
pub(super) const SIGN_IN: &str = "kodosi/signIn";
pub(super) const ASKING: &str = "kodosi/asking";

#[derive(Default)]
pub(super) struct Antigravity {
    linked: bool,
    offers: Offers,
    folder: String,
    start: Value,
    session: Option<String>,
    starting: bool,
    turn: Option<Turn>,
    items: HashMap<String, String>,
    writing: Option<String>,
    told: Option<String>,
    asked: HashMap<String, Asked>,
    calls: VecDeque<Value>,
    sent: u64,
    /// Follow-up text from a question answer or denial reason, sent as the next prompt.
    said: Option<(String, Option<ChatPerson>)>,
    options: Vec<Value>,
    modes: Vec<String>,
    models: Vec<String>,
    /// First prompt of an open `ask_question` call, used to suppress the duplicate text the agent
    /// echoes in `agent_message_chunk`.
    questioned: Option<String>,
    now: Option<ChatNow>,
}

#[derive(Default, Clone, Copy)]
struct Offers {
    loads: bool,
    unrestricted: bool,
}

struct Turn {
    at: u64,
    stopped: bool,
    handed: Option<Handed>,
}

struct Handed {
    queued: Option<String>,
    text: String,
    by: Option<ChatPerson>,
}

struct Asked {
    item: String,
    options: Vec<Value>,
    questions: Vec<String>,
}

impl Antigravity {
    pub(super) const NAME: &'static str = "antigravity";

    fn show(&self, chat: &mut Chat) {
        show_state(chat, self.linked, self.turn.is_some(), self.now.clone());
    }

    fn call(&mut self, tag: &str, method: &str, params: Value) {
        let mut call = json!({"tag": tag, "m": method});
        call["p"] = params;
        self.calls.push_back(call);
    }

    fn session_params(&self, mut params: Value) -> Value {
        params["cwd"] = json!(self.folder);
        params["mcpServers"] = json!([]);
        params
    }

    fn hello(&mut self, chat: &mut Chat, event: &Value) -> Vec<String> {
        let closed = chat.close_asks(|_| true);
        chat.reset();
        let started = event.get("started").unwrap_or(&Value::Null);
        let capabilities = started.get("agentCapabilities").unwrap_or(&Value::Null);
        *self = Self {
            linked: true,
            offers: Offers {
                loads: capabilities.get("loadSession").and_then(Value::as_bool) == Some(true),
                unrestricted: list(event, "arguments")
                    .any(|argument| argument == "--dangerously-skip-permissions"),
            },
            folder: text(event, "folder").to_owned(),
            start: event.get("start").cloned().unwrap_or(Value::Null),
            starting: true,
            ..Self::default()
        };
        let version = started
            .pointer("/agentInfo/version")
            .and_then(Value::as_str)
            .map(|version| start_of(version, 32).to_owned());
        let folder = Some(self.folder.clone()).filter(|folder| !folder.is_empty());
        chat.change_facts(|facts| {
            *facts = super::ChatFacts {
                agent: Some(Self::NAME.to_owned()),
                version,
                folder,
                ..super::ChatFacts::default()
            };
        });
        match words(&self.start, "resume") {
            Some(session) if self.offers.loads => {
                let params = self.session_params(json!({"sessionId": session}));
                self.session = Some(session);
                self.call("session:load", "session/load", params);
            }
            _ => {
                let params = self.session_params(json!({}));
                self.call("session:new", "session/new", params);
            }
        }
        closed
    }

    fn reply(&mut self, chat: &mut Chat, event: &Value) {
        let (tag, name) = text(event, "tag")
            .split_once(':')
            .unwrap_or_else(|| (text(event, "tag"), ""));
        let result = event.get("r").filter(|result| !result.is_null());
        let at = event.get("t").and_then(Value::as_u64).unwrap_or_default();
        match (tag, result) {
            ("session", Some(result)) => {
                if let Some(session) = words(result, "sessionId") {
                    self.session = Some(session);
                }
                self.starting = false;
                self.finish_writing(chat);
                self.took(chat, result);
                self.choose(result);
            }
            ("session", None) => self.not_started(chat, name, event, at),
            ("prompt", result) => self.end_turn(chat, result, event, at),
            ("mode" | "option", Some(result)) => {
                self.took(chat, result);
                // Available effort levels depend on the selected model, so apply effort only after
                // the model change returns updated config options.
                if name == "model" {
                    self.choose_effort();
                }
            }
            ("mode" | "option", None) => failed(chat, event, at),
            _ => {}
        }
    }

    fn not_started(&mut self, chat: &mut Chat, name: &str, event: &Value, at: u64) {
        self.starting = false;
        if event.pointer("/e/code").and_then(Value::as_i64) == Some(SIGNED_OUT) {
            chat.push(
                at,
                ChatBody::Note {
                    note: ChatNote::SignedOut,
                },
            );
            self.session = None;
            self.calls.push_back(json!({"m": SIGN_IN}));
            return;
        }
        if name == "load" {
            chat.reset();
            let params = self.session_params(json!({}));
            self.session = None;
            self.starting = true;
            self.call("session:new", "session/new", params);
            return;
        }
        failed(chat, event, at);
        self.session = None;
    }

    /// Applies CLI startup choices (`mode`, `model`, `effort`) that differ from the session's
    /// current values and are offered by this account. Unavailable modes (such as `auto`) are
    /// ignored rather than failing startup.
    fn choose(&mut self, result: &Value) {
        let Some(session) = self.session.clone() else {
            return;
        };
        if let Some(mode) = self.start.get("mode").and_then(Value::as_str) {
            let mode = if mode.is_empty() { "default" } else { mode };
            if result.pointer("/modes/currentModeId") != Some(&json!(mode))
                && self.modes.iter().any(|offered| offered == mode)
            {
                self.call(
                    "mode",
                    "session/set_mode",
                    json!({"sessionId": session, "modeId": mode}),
                );
            }
        }
        if let (Some(model), Some(option)) = (words(&self.start, "model"), self.model_option())
            && text(&option, "currentValue") != model
            && (offers(&option, &model) || self.models.contains(&model))
        {
            self.set("option:model", &session, &option, &model);
            return;
        }
        self.choose_effort();
    }

    fn choose_effort(&mut self) {
        if let (Some(session), Some(effort), Some(option)) = (
            self.session.clone(),
            words(&self.start, "effort"),
            self.effort_option(),
        ) && text(&option, "currentValue") != effort
            && offers(&option, &effort)
        {
            self.set("option:effort", &session, &option, &effort);
        }
    }

    fn set(&mut self, tag: &str, session: &str, option: &Value, value: &str) {
        self.call(
            tag,
            "session/set_config_option",
            json!({"sessionId": session, "configId": text(option, "id"), "value": value}),
        );
    }

    fn model_option(&self) -> Option<Value> {
        self.options
            .iter()
            .find(|option| text(option, "id") == "model" || text(option, "category") == "model")
            .cloned()
    }

    fn effort_option(&self) -> Option<Value> {
        self.options
            .iter()
            .find(|option| text(option, "category") == "thought_level")
            .cloned()
    }

    fn took(&mut self, chat: &mut Chat, result: &Value) {
        if let Some(modes) = result.pointer("/modes/availableModes") {
            self.modes = list_of(modes, "id");
        }
        if let Some(models) = result.pointer("/models/availableModels") {
            self.models = list_of(models, "modelId");
        }
        if let Some(options) = result.get("configOptions").and_then(Value::as_array) {
            self.options.clone_from(options);
            self.show_options(chat);
        }
        if let Some(mode) = result
            .pointer("/modes/currentModeId")
            .and_then(Value::as_str)
        {
            self.show_mode(chat, mode);
        }
    }

    fn show_options(&self, chat: &mut Chat) {
        let model = self.model_option();
        let effort = self.effort_option();
        chat.change_facts(|facts| {
            facts.model = model
                .as_ref()
                .and_then(|option| words(option, "currentValue"))
                .or_else(|| facts.model.take());
            facts.effort = effort
                .as_ref()
                .and_then(|option| words(option, "currentValue"))
                .or_else(|| facts.effort.take());
        });
    }

    fn show_mode(&self, chat: &mut Chat, mode: &str) {
        let mode = match mode {
            _ if self.offers.unrestricted => Some(ChatMode::Unrestricted),
            "default" => Some(ChatMode::Ask),
            "accept-edits" => Some(ChatMode::Edits),
            "plan" => Some(ChatMode::Plan),
            "auto" => Some(ChatMode::Auto),
            _ => None,
        };
        chat.change_facts(|facts| facts.mode = mode.or(facts.mode));
    }

    /// Commits a prompt sent in `commands()` to the chat once the server responds.
    fn begin(&mut self, chat: &mut Chat, at: u64) {
        let Some(handed) = self.turn.as_mut().and_then(|turn| turn.handed.take()) else {
            return;
        };
        if let Some(queued) = &handed.queued {
            chat.unqueue(queued);
        }
        self.told = None;
        self.answer_questions(chat, &handed.text);
        chat.push(at, person(&handed.text, handed.by));
    }

    fn cancel(&mut self) -> Option<Value> {
        let session = self.session.clone()?;
        self.turn.as_mut()?.stopped = true;
        Some(json!({
            "tag": "stop",
            "m": "session/cancel",
            "p": {"sessionId": session},
            "notify": true,
        }))
    }

    fn note(&mut self, chat: &mut Chat, event: &Value, at: u64) -> Vec<String> {
        let params = event.get("p").unwrap_or(&Value::Null);
        if text(event, "m") != "session/update"
            || self
                .session
                .as_deref()
                .is_some_and(|session| text(params, "sessionId") != session)
        {
            return Vec::new();
        }
        let update = params.get("update").unwrap_or(&Value::Null);
        match text(update, "sessionUpdate") {
            "user_message_chunk" => self.told(chat, update, at),
            "agent_message_chunk" => self.write(chat, update, at),
            "agent_thought_chunk" => {
                if self.turn.is_some() {
                    self.now = Some(ChatNow::Thinking);
                }
            }
            "tool_call" | "tool_call_update" => {
                self.tool(chat, update, at);
            }
            "plan" => chat.set_tasks(list(update, "entries").map(|entry| ChatTask {
                text: text(entry, "content").to_owned(),
                state: match text(entry, "status") {
                    "completed" => TaskState::Done,
                    "in_progress" => TaskState::Working,
                    _ => TaskState::Pending,
                },
            })),
            "current_mode_update" => self.show_mode(chat, text(update, "currentModeId")),
            "config_option_update" => self.took(chat, update),
            "usage_update" => {
                if let (Some(tokens), Some(window)) = (
                    update.get("used").and_then(Value::as_u64),
                    update.get("size").and_then(Value::as_u64),
                ) {
                    chat.change_facts(|facts| facts.context = Some(ChatContext { tokens, window }));
                }
            }
            _ => {}
        }
        Vec::new()
    }

    /// Appends `user_message_chunk` history during session load. Ignored during an active turn
    /// because `begin()` already recorded the outgoing user message.
    fn told(&mut self, chat: &mut Chat, update: &Value, at: u64) {
        if self.turn.is_some() {
            return;
        }
        let Some(more) = content_text(update) else {
            return;
        };
        self.finish_writing(chat);
        if let Some(item) = self.told.clone()
            && let Some(ChatBody::Person { text, .. }) = chat.body(&item)
        {
            let words = format!("{text}{more}");
            chat.put(&item, person(&words, None));
        } else {
            self.told = Some(chat.push(at, person(more, None)));
        }
    }

    fn write(&mut self, chat: &mut Chat, update: &Value, at: u64) {
        let Some(more) = content_text(update) else {
            return;
        };
        if self
            .questioned
            .take_if(|question| more.contains(question.as_str()))
            .is_some()
        {
            return;
        }
        self.told = None;
        if self.turn.is_some() {
            self.now = Some(ChatNow::Writing);
        }
        match &self.writing {
            Some(item) => chat.write(item, more),
            None => self.writing = Some(chat.push(at, answer(more, self.turn.is_some()))),
        }
    }

    fn finish_writing(&mut self, chat: &mut Chat) {
        if let Some(item) = self.writing.take()
            && let Some(ChatBody::Text { text, .. }) = chat.body(&item).cloned()
        {
            chat.put(&item, answer(&text, false));
        }
    }

    fn tool(&mut self, chat: &mut Chat, update: &Value, at: u64) -> Option<()> {
        let id = words(update, "toolCallId")?;
        self.told = None;
        self.finish_writing(chat);
        let item = self.item_of(chat, &id, update, at);
        let stopped = self.turn.as_ref().is_some_and(|turn| turn.stopped);
        let asking = self.asked.contains_key(&question_of(&id));
        let places = Places::of(chat);
        chat.change_tool(&item, |tool| {
            news(tool, update, &places);
            if stopped && tool.state == ToolState::Failed {
                tool.state = ToolState::Stopped;
            } else if asking && tool.state.is_open() {
                tool.state = ToolState::Asking;
            }
        });
        // Skip re-opening the ask when the agent sends a follow-up update after it was answered.
        if text(update, "name") == ASK_QUESTION
            && self.turn.is_some()
            && chat
                .tool(&item)
                .is_some_and(|tool| tool.state.is_open() && !has_answers(tool))
        {
            self.question(chat, &id, &item, update);
        }
        if self.turn.is_some() && chat.tool(&item).is_some_and(|tool| tool.state.is_open()) {
            self.now = Some(ChatNow::Tool { item });
        }
        Some(())
    }

    fn item_of(&mut self, chat: &mut Chat, id: &str, update: &Value, at: u64) -> String {
        if let Some(item) = self.items.get(id) {
            return item.clone();
        }
        let item = chat.push(
            at,
            ChatBody::Tool {
                tool: Box::new(described(update)),
            },
        );
        remember(&mut self.items, chat, id, item.clone());
        item
    }

    fn request(&mut self, chat: &mut Chat, event: &Value) {
        let params = event.get("p").unwrap_or(&Value::Null);
        let Some(ask) = event.get("id").and_then(ask_name) else {
            return;
        };
        let call = params.get("toolCall").unwrap_or(&Value::Null);
        let options = list(params, "options")
            .take(MOST_OPTIONS)
            .cloned()
            .collect::<Vec<_>>();
        let at = event.get("t").and_then(Value::as_u64).unwrap_or_default();
        self.finish_writing(chat);
        let item = self.item_of(chat, text(call, "toolCallId"), call, at);
        let places = Places::of(chat);
        chat.change_tool(&item, |tool| {
            news(tool, call, &places);
            tool.state = ToolState::Asking;
        });
        let Some(tool) = chat.tool(&item).cloned() else {
            return;
        };
        let always = options
            .iter()
            .any(|option| text(option, "kind") == "allow_always")
            .then(|| AskAlways::Rule {
                tool: tool.name.clone(),
                text: if tool.target.is_empty() {
                    tool.name.clone()
                } else {
                    tool.target.clone()
                },
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
                item,
                options,
                questions: Vec::new(),
            },
        );
    }

    fn question(&mut self, chat: &mut Chat, id: &str, item: &str, update: &Value) {
        let ask = question_of(id);
        if self.asked.contains_key(&ask) {
            return;
        }
        let input = update.get("rawInput").unwrap_or(&Value::Null);
        let questions = list(input, "questions")
            .take(MOST_QUESTIONS)
            .filter_map(|entry| {
                Some(ChatQuestion {
                    question: start_of(&words(entry, "question")?, MOST_TARGET * 2).to_owned(),
                    header: String::new(),
                    options: list(entry, "options")
                        .take(MOST_OPTIONS)
                        .filter_map(Value::as_str)
                        .map(|label| ChatOption {
                            label: start_of(label, MOST_TARGET).to_owned(),
                            description: String::new(),
                        })
                        .collect(),
                    multiple: entry.get("is_multi_select").and_then(Value::as_bool) == Some(true),
                })
            })
            .filter(|question| !question.options.is_empty())
            .collect::<Vec<_>>();
        let Some(first) = questions.first().map(|question| question.question.clone()) else {
            return;
        };
        chat.change_tool(item, |tool| {
            tool.kind = ToolKind::Question;
            tool.state = ToolState::Asking;
            first.clone_into(&mut tool.target);
            tool.detail = Some(ToolDetail::Question {
                questions: questions.clone(),
                answers: Vec::new(),
            });
        });
        chat.ask(ChatAsk {
            id: ask.clone(),
            item: Some(item.to_owned()),
            body: AskBody::Question {
                questions: questions.clone(),
            },
        });
        self.questioned = Some(first);
        self.calls.push_back(json!({"m": ASKING}));
        self.asked.insert(
            ask,
            Asked {
                item: item.to_owned(),
                options: Vec::new(),
                questions: questions
                    .into_iter()
                    .map(|question| question.question)
                    .collect(),
            },
        );
    }

    fn answer_questions(&mut self, chat: &mut Chat, words: &str) {
        let open = self
            .asked
            .iter()
            .filter(|(_, asked)| !asked.questions.is_empty())
            .map(|(ask, asked)| (ask.clone(), asked.item.clone()))
            .collect::<Vec<_>>();
        for (ask, item) in open {
            self.asked.remove(&ask);
            chat.close_asks(|open| open.id == ask);
            chat.change_tool(&item, |tool| {
                tool.state = ToolState::Done;
                if let Some(ToolDetail::Question { answers, .. }) = &mut tool.detail {
                    *answers = vec![start_of(words, MOST_TARGET).to_owned()];
                }
            });
        }
        self.questioned = None;
    }

    fn end_turn(&mut self, chat: &mut Chat, result: Option<&Value>, event: &Value, at: u64) {
        let Some(turn) = self.turn.take() else {
            return;
        };
        self.now = None;
        self.finish_writing(chat);
        let end = match result.map(|result| text(result, "stopReason")) {
            Some("end_turn") => TurnEnd::Done,
            Some("cancelled") => TurnEnd::Stopped,
            Some("refusal") => TurnEnd::Refused,
            Some(_) => TurnEnd::Failed,
            None if turn.stopped => TurnEnd::Stopped,
            None => {
                failed(chat, event, at);
                TurnEnd::Failed
            }
        };
        self.questioned = None;
        let asking = self
            .asked
            .values()
            .filter(|asked| !asked.questions.is_empty())
            .map(|asked| asked.item.clone())
            .collect::<Vec<_>>();
        for item in open_items(chat)
            .into_iter()
            .filter(|item| !asking.contains(item))
        {
            chat.change_tool(&item, |tool| {
                // Commands still running when a turn finishes normally continue in the background.
                if let (ToolState::Running, Some(ToolDetail::Command { background, .. })) =
                    (tool.state, &mut tool.detail)
                    && end != TurnEnd::Stopped
                {
                    *background = true;
                } else {
                    tool.state = ToolState::Stopped;
                }
            });
        }
        chat.push(
            at,
            ChatBody::Turn {
                end,
                ms: at.saturating_sub(turn.at),
            },
        );
        self.asked.retain(|_, asked| !asked.questions.is_empty());
        let open = self.asked.keys().cloned().collect::<Vec<_>>();
        chat.close_asks(|ask| !open.contains(&ask.id));
    }

    fn typed(&mut self, chat: &mut Chat, event: &Value) {
        let line = text(event, "text").trim();
        if line.is_empty() {
            return;
        }
        let (command, value) = line.split_once(' ').unwrap_or((line, ""));
        let option = match command {
            "/model" => self.model_option(),
            "/effort" => self.effort_option(),
            _ => None,
        };
        if let (Some(option), Some(session), false) =
            (option, self.session.clone(), value.trim().is_empty())
        {
            self.set("option", &session, &option, value.trim());
            return;
        }
        chat.queue(line.to_owned(), None, Vec::new());
    }
}

impl Agent for Antigravity {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn apply(&mut self, chat: &mut Chat, event: &Value) -> Vec<String> {
        let at = event.get("t").and_then(Value::as_u64).unwrap_or_default();
        if matches!(text(event, "k"), "note" | "request" | "reply") {
            self.begin(chat, at);
        }
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
            "typed" => {
                self.typed(chat, event);
                Vec::new()
            }
            "interrupt" => {
                if let Some(command) = self.cancel() {
                    self.calls.push_back(command);
                }
                self.asked.clear();
                chat.close_asks(|_| true)
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
        self.writing = None;
        self.told = None;
        self.calls.clear();
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
        self.session.as_deref()
    }

    fn works_in(&self) -> TerminalFace {
        TerminalFace::Chat
    }

    fn retry(&mut self) {}

    fn has(&self, queued: &str) -> bool {
        self.turn
            .as_ref()
            .and_then(|turn| turn.handed.as_ref())
            .is_some_and(|handed| handed.queued.as_deref() == Some(queued))
    }

    fn decision(
        &mut self,
        ask: &str,
        answer: &ChatAnswer,
        by: Option<&ChatPerson>,
    ) -> Option<Value> {
        let asked = self.asked.get(ask)?;
        if !asked.questions.is_empty() {
            let ChatAnswer::Answers { answers } = answer else {
                return Some(json!({"answers": []}));
            };
            let said = answers_words(&asked.questions, answers)?;
            self.said = Some((said, by.cloned()));
            return Some(json!({"answers": answers}));
        }
        let chosen = |kind: &str| {
            asked
                .options
                .iter()
                .find(|option| text(option, "kind") == kind)
                .and_then(|option| option.get("optionId"))
                .cloned()
        };
        let option = match answer {
            ChatAnswer::Allow => chosen("allow_once")?,
            ChatAnswer::Always => chosen("allow_always")?,
            ChatAnswer::Deny { text } => {
                // ACP rejection carries no reason field, so send refusal text as the next prompt.
                self.said = text
                    .as_deref()
                    .map(str::trim)
                    .filter(|words| !words.is_empty())
                    .map(|words| (words.to_owned(), by.cloned()));
                chosen("reject_once")?
            }
            ChatAnswer::Answers { .. } | ChatAnswer::Plan { .. } => return None,
        };
        Some(json!({"outcome": {"outcome": "selected", "optionId": option}}))
    }

    fn answered(&mut self, chat: &mut Chat, ask: &str, decision: &Value) -> bool {
        let Some(asked) = self.asked.remove(ask) else {
            return true;
        };
        chat.close_asks(|open| open.id == ask);
        if !asked.questions.is_empty() {
            let chosen = list(decision, "answers")
                .flat_map(|labels| list_of(labels, ""))
                .collect::<Vec<_>>();
            chat.change_tool(&asked.item, |tool| {
                tool.state = if chosen.is_empty() {
                    ToolState::Stopped
                } else {
                    ToolState::Done
                };
                if let Some(ToolDetail::Question { answers, .. }) = &mut tool.detail {
                    *answers = chosen;
                }
            });
            self.questioned = None;
            self.show(chat);
            return false;
        }
        let chosen = decision.pointer("/outcome/optionId");
        let refused = asked
            .options
            .iter()
            .find(|option| Some(&option["optionId"]) == chosen)
            .is_none_or(|option| text(option, "kind").starts_with("reject"));
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
        // Permission requests block the next prompt, whereas open `ask_question` asks are
        // answered by sending the next prompt.
        let waits = chat.asks().iter().any(|ask| {
            self.asked
                .get(&ask.id)
                .is_none_or(|asked| asked.questions.is_empty())
        });
        let ready = self.linked && !self.starting && self.turn.is_none() && !waits;
        let (true, Some(session)) = (ready, self.session.clone()) else {
            return commands;
        };
        let next = self
            .said
            .take()
            .map(|(said, by)| (None, said, by))
            .or_else(|| {
                let queued = chat.queued().first()?;
                Some((
                    Some(queued.id.clone()),
                    queued.said.clone(),
                    queued.by.clone(),
                ))
            });
        let Some((queued, text, by)) = next else {
            return commands;
        };
        self.sent += 1;
        commands.push(json!({
            "tag": format!("prompt:{}", self.sent),
            "m": "session/prompt",
            "p": {"sessionId": session, "prompt": [{"type": "text", "text": text}]},
        }));
        self.turn = Some(Turn {
            at: crate::identity::now_ms(),
            stopped: false,
            handed: Some(Handed { queued, text, by }),
        });
        commands
    }

    /// Cancels the turn and replies `"cancelled"` to any open permission requests, as required by
    /// ACP.
    fn stop(&mut self, chat: &Chat) -> Stop {
        let Some(command) = self.cancel() else {
            return Stop::Nothing;
        };
        if chat.asks().is_empty() {
            return Stop::Command(command);
        }
        self.calls.push_back(command);
        Stop::Decisions(
            chat.asks()
                .iter()
                .map(|ask| (ask.id.clone(), json!({"outcome": {"outcome": "cancelled"}})))
                .collect(),
        )
    }
}

fn failed(chat: &mut Chat, event: &Value, at: u64) {
    let error = event
        .pointer("/e/message")
        .and_then(Value::as_str)
        .unwrap_or_default();
    chat.push(
        at,
        ChatBody::Note {
            note: ChatNote::Failed {
                error: start_of(error, MOST_TARGET).to_owned(),
                text: event
                    .pointer("/e/data/message")
                    .or_else(|| event.pointer("/e/data"))
                    .and_then(Value::as_str)
                    .map_or_default(|data| start_of(data, MOST_TARGET).to_owned()),
            },
        },
    );
}

fn has_answers(tool: &ChatTool) -> bool {
    matches!(&tool.detail, Some(ToolDetail::Question { answers, .. }) if !answers.is_empty())
}

fn question_of(call: &str) -> String {
    format!("q{call}")
}

fn list_of(entries: &Value, key: &str) -> Vec<String> {
    entries
        .as_array()
        .into_iter()
        .flatten()
        .take(64)
        .filter_map(|entry| {
            if key.is_empty() {
                entry
                    .as_str()
                    .map(|text| start_of(text, MOST_TARGET).to_owned())
            } else {
                words(entry, key)
            }
        })
        .collect()
}

fn offers(option: &Value, value: &str) -> bool {
    list(option, "options")
        .flat_map(|entry| {
            if entry.get("value").is_some() {
                vec![entry]
            } else {
                list(entry, "options").collect()
            }
        })
        .any(|entry| text(entry, "value") == value)
}

fn answers_words(questions: &[String], answers: &[Vec<String>]) -> Option<String> {
    if let ([_], [chosen]) = (questions, answers) {
        return Some(chosen.join(", ")).filter(|words| !words.is_empty());
    }
    let lines = questions
        .iter()
        .zip(answers)
        .filter(|(_, chosen)| !chosen.is_empty())
        .map(|(question, chosen)| format!("{question} {}", chosen.join(", ")))
        .collect::<Vec<_>>();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// Strips the metadata header (timing and exit code) Antigravity prepends to command output.
fn command_output(printed: &str) -> String {
    let printed = printed
        .split_once("\nCommand output:\n")
        .map_or(printed, |(_, output)| output);
    printed.replace("\r\n", "\n").trim_end().to_owned()
}

fn content_text(update: &Value) -> Option<&str> {
    let content = update.get("content")?;
    (text(content, "type") == "text").then(|| text(content, "text"))
}

fn described(update: &Value) -> ChatTool {
    let kind = match text(update, "kind") {
        "read" => ToolKind::Read,
        "edit" | "delete" | "move" => ToolKind::Edit,
        "search" => ToolKind::Search,
        "execute" => ToolKind::Command,
        "fetch" => ToolKind::Web,
        _ => ToolKind::Other,
    };
    ChatTool {
        name: words(update, "name").unwrap_or_else(|| text(update, "kind").to_owned()),
        kind,
        state: ToolState::Preparing,
        target: String::new(),
        detail: (kind == ToolKind::Command).then(|| ToolDetail::Command {
            command: String::new(),
            output: String::new(),
            background: false,
        }),
        ms: None,
    }
}

/// Applies incremental `tool_call_update` fields (ACP only sends changed fields). Preserves
/// `Denied` when the agent follows up with `"failed"`, and marks non-zero exit codes as `Failed`
/// even when the tool call status is `"completed"`.
fn news(tool: &mut ChatTool, update: &Value, places: &Places) {
    let input = update.get("rawInput").unwrap_or(&Value::Null);
    let output = update.get("rawOutput").unwrap_or(&Value::Null);
    let exited = output.get("exit_code").and_then(Value::as_i64);
    tool.state = match text(update, "status") {
        "pending" => ToolState::Preparing,
        "in_progress" => ToolState::Running,
        "completed" if exited.is_some_and(|code| code != 0) => ToolState::Failed,
        "completed" => ToolState::Done,
        "failed" if tool.state == ToolState::Denied => ToolState::Denied,
        "failed" => ToolState::Failed,
        _ => tool.state,
    };
    let command = words(input, "CommandLine");
    let place = list(update, "locations")
        .find_map(|location| words(location, "path"))
        .map(|path| places.file(&path));
    let subject = command
        .clone()
        .or(place)
        .or_else(|| words(input, "query"))
        .or_else(|| words(update, "title"));
    if let Some(subject) = subject {
        start_of(&subject, MOST_TARGET).clone_into(&mut tool.target);
    }
    let printed = output
        .get("output")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            let texts = list(update, "content")
                .filter(|content| text(content, "type") == "content")
                .filter_map(|content| content.pointer("/content/text").and_then(Value::as_str))
                .collect::<Vec<_>>();
            (!texts.is_empty()).then(|| texts.join("\n"))
        })
        .map(|printed| command_output(&printed));
    let change = list(update, "content")
        .filter(|content| text(content, "type") == "diff")
        .map(|content| {
            let old = content.get("oldText").and_then(Value::as_str);
            let new = text(content, "newText");
            (
                text(content, "path").to_owned(),
                old.map_or_else(|| diff::written(new), |old| diff::between(old, new)),
            )
        })
        .reduce(|(path, first), (_, next)| (path, first.then(next)));
    match &mut tool.detail {
        Some(ToolDetail::Command {
            command: shown,
            output,
            ..
        }) => {
            if let Some(command) = command {
                start_of(&command, MOST_OUTPUT).clone_into(shown);
            }
            if let Some(printed) = printed {
                *output = ends_of(&printed, MOST_OUTPUT);
            }
        }
        detail => {
            if let Some((path, lines)) = change {
                if tool.kind == ToolKind::Edit && lines.removed == 0 {
                    tool.kind = ToolKind::Write;
                }
                *detail = Some(lines.detail(path));
            } else if let Some(printed) = printed.filter(|printed| !printed.trim().is_empty()) {
                *detail = Some(ToolDetail::Output {
                    text: ends_of(&printed, MOST_TEXT),
                });
            }
        }
    }
}

pub(super) fn ask_name(request: &Value) -> Option<String> {
    match request {
        Value::Number(number) => Some(format!("r{number}")),
        Value::String(name) if !name.is_empty() => Some(format!("r{name}")),
        _ => None,
    }
}
