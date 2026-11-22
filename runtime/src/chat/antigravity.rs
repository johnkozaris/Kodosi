//! Antigravity CLI's link: the messages of its server for programs (ACP) as changes of the chat.
//!
//! Antigravity CLI has no add-on interface that its terminal app reports through, so its chat
//! has a session of its own: `kodosi acp` runs the person's own `agy acp` in the terminal, and the
//! link carries each message of that server as it is. This adapter makes the calls of the
//! session, and a person's answer goes back as the decision that the agent takes. A field can be
//! absent or of another type after an update of the agent, so a message gives what it has and
//! never stops the chat.

use std::collections::{HashMap, VecDeque};

use serde_json::{Value, json};

use super::{
    AskAlways, AskBody, ChatAnswer, ChatAsk, ChatBody, ChatContext, ChatMode, ChatNote, ChatNow,
    ChatOption, ChatPerson, ChatQuestion, ChatState, ChatTask, ChatTool, MOST_OUTPUT, MOST_TARGET,
    MOST_TEXT, TaskState, ToolDetail, ToolKind, ToolState, TurnEnd,
    acp::{SIGN_IN, ask_name},
    agent::{Agent, Stop, answer, file_name, list, person, text, words},
    diff,
    state::{Chat, ends_of, start_of},
};

/// The JSON-RPC code of the agent's "Authentication required".
const SIGNED_OUT: i64 = -32000;
/// The start of the name of a call that is the agent's own question to the person.
const QUESTION: &str = "interaction_";
const MOST_ITEMS: usize = 2048;

#[derive(Default)]
pub(super) struct Antigravity {
    linked: bool,
    offers: Offers,
    folder: String,
    /// The choices of the terminal's line, for the session that starts.
    start: Value,
    session: Option<String>,
    /// The session is starting, and it may give its history first.
    starting: bool,
    turn: Option<Turn>,
    /// The chat's item of each tool call of the agent.
    items: HashMap<String, String>,
    /// The chat's item of the answer that the agent writes now.
    writing: Option<String>,
    /// The chat's item of a message of the person that the agent gives back with its history.
    told: Option<String>,
    asked: HashMap<String, Asked>,
    calls: VecDeque<Value>,
    sent: u64,
    /// What a person wrote to refuse a question. The agent gets it as the next message.
    said: Option<(String, Option<ChatPerson>)>,
    /// The session's options, such as its model and thinking level, as the agent gave them last.
    options: Vec<Value>,
    now: Option<ChatNow>,
}

/// What the agent's server does, as it said when it started and as the terminal's line chose.
#[derive(Default, Clone, Copy)]
struct Offers {
    /// The server gives the history of a session that it loads.
    loads: bool,
    /// The server adds Kodosi's note to the instructions of a session.
    appends: bool,
    /// The agent asks for no permission, as the line chose.
    unrestricted: bool,
}

struct Turn {
    /// When the person's message went to the agent.
    at: u64,
    stopped: bool,
    /// The message that starts the turn, until the chat shows it.
    handed: Option<Handed>,
}

struct Handed {
    /// The message of the queue, or nothing for the words of a refusal.
    queued: Option<String>,
    text: String,
    by: Option<ChatPerson>,
}

struct Asked {
    item: String,
    /// The answers that the agent offers, each with its name and kind.
    options: Vec<Value>,
    question: bool,
}

impl Antigravity {
    pub(super) const NAME: &'static str = "antigravity";

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

    fn call(&mut self, tag: &str, method: &str, params: Value) {
        let mut call = json!({"tag": tag, "m": method});
        call["p"] = params;
        self.calls.push_back(call);
    }

    /// The parameters that each start of a session carries: the folder, no servers of tools of
    /// Kodosi, and Kodosi's note where the agent takes it.
    fn session_params(&self, mut params: Value) -> Value {
        params["cwd"] = json!(self.folder);
        params["mcpServers"] = json!([]);
        if self.offers.appends {
            params["_meta"] = json!({
                "antigravity": {"systemPrompt": {"append": crate::agents::CONTEXT.trim()}},
            });
        }
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
                appends: capabilities
                    .pointer("/_meta/antigravity/systemPrompt/append")
                    .and_then(Value::as_bool)
                    == Some(true),
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
                self.choose();
            }
            ("session", None) => self.not_started(chat, name, event, at),
            ("prompt", result) => self.end_turn(chat, result, event, at),
            ("mode" | "option", Some(result)) => self.took(chat, result),
            ("mode" | "option", None) => failed(chat, event, at),
            _ => {}
        }
    }

    /// The session did not start. A session that is gone starts new; an agent that is signed out
    /// asks the terminal for its sign-in, and the server starts again after it.
    fn not_started(&mut self, chat: &mut Chat, name: &str, event: &Value, at: u64) {
        self.starting = false;
        if event.pointer("/e/code").and_then(Value::as_i64) == Some(SIGNED_OUT) {
            failed(chat, event, at);
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

    /// The model, the thinking level and the mode that the terminal's line chose, where the
    /// session has another.
    fn choose(&mut self) {
        let Some(session) = self.session.clone() else {
            return;
        };
        if let Some(mode) = self.start.get("mode").and_then(Value::as_str) {
            let mode = if mode.is_empty() { "default" } else { mode };
            self.call(
                "mode",
                "session/set_mode",
                json!({"sessionId": session, "modeId": mode}),
            );
        }
        for (wanted, option) in [
            ("model", self.model_option()),
            ("effort", self.effort_option()),
        ] {
            if let (Some(value), Some(option)) = (words(&self.start, wanted), option)
                && text(&option, "currentValue") != value
            {
                self.call(
                    "option",
                    "session/set_config_option",
                    json!({"sessionId": session, "configId": text(&option, "id"), "value": value}),
                );
            }
        }
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

    /// What an answer of the session tells of its mode and its options.
    fn took(&mut self, chat: &mut Chat, result: &Value) {
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
        let models = model.as_ref().map_or_default(|option| {
            list(option, "options")
                .flat_map(|entry| {
                    if entry.get("value").is_some() {
                        vec![entry]
                    } else {
                        list(entry, "options").collect()
                    }
                })
                .filter_map(|entry| words(entry, "value"))
                .take(64)
                .collect::<Vec<_>>()
        });
        chat.change_facts(|facts| {
            facts.model = model
                .as_ref()
                .and_then(|option| words(option, "currentValue"))
                .or_else(|| facts.model.take());
            facts.effort = effort
                .as_ref()
                .and_then(|option| words(option, "currentValue"))
                .or_else(|| facts.effort.take());
            if !models.is_empty() {
                facts.models = models;
            }
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

    /// The agent took the message that the chat sent, so the chat shows it.
    fn begin(&mut self, chat: &mut Chat, at: u64) {
        let Some(handed) = self.turn.as_mut().and_then(|turn| turn.handed.take()) else {
            return;
        };
        if let Some(queued) = &handed.queued {
            chat.unqueue(queued);
        }
        self.told = None;
        chat.push(at, person(&handed.text, handed.by));
    }

    /// Stops the turn that runs: the agent ends its open calls and its requests.
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

    /// A message of the person that the agent gives back, as it does with the history of a
    /// session that it loads. The chat has its own record of what it sent.
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
        self.told = None;
        if self.turn.is_some() {
            self.now = Some(ChatNow::Writing);
        }
        match &self.writing {
            Some(item) => chat.write(item, more),
            None => self.writing = Some(chat.push(at, answer(more, self.turn.is_some()))),
        }
    }

    /// The answer that the agent wrote is whole once a step or the end of its turn follows it.
    fn finish_writing(&mut self, chat: &mut Chat) {
        if let Some(item) = self.writing.take()
            && let Some(ChatBody::Text { text, .. }) = chat.body(&item).cloned()
        {
            chat.put(&item, answer(&text, false));
        }
    }

    /// A tool call of the agent, new or with news. The agent sends each field only when it is new.
    fn tool(&mut self, chat: &mut Chat, update: &Value, at: u64) -> Option<()> {
        let id = words(update, "toolCallId")?;
        self.told = None;
        self.finish_writing(chat);
        let item = self.item_of(chat, &id, update, at);
        chat.change_tool(&item, |tool| news(tool, update));
        if self.turn.is_some() && chat.tool(&item).is_some_and(|tool| tool.state.is_open()) {
            self.now = Some(ChatNow::Tool { item });
        }
        Some(())
    }

    /// The chat's item of a tool call, which the agent's first news of the call makes.
    fn item_of(&mut self, chat: &mut Chat, id: &str, update: &Value, at: u64) -> String {
        if let Some(item) = self.items.get(id) {
            return item.clone();
        }
        if self.items.len() >= MOST_ITEMS {
            self.items.clear();
        }
        let item = chat.push(
            at,
            ChatBody::Tool {
                tool: Box::new(described(update)),
            },
        );
        self.items.insert(id.to_owned(), item.clone());
        item
    }

    /// The agent asks before a call, or asks the person a question with the same request.
    fn request(&mut self, chat: &mut Chat, event: &Value) {
        let params = event.get("p").unwrap_or(&Value::Null);
        let Some(ask) = event.get("id").and_then(ask_name) else {
            return;
        };
        let call = params.get("toolCall").unwrap_or(&Value::Null);
        let options = list(params, "options")
            .take(16)
            .cloned()
            .collect::<Vec<_>>();
        let at = event.get("t").and_then(Value::as_u64).unwrap_or_default();
        self.finish_writing(chat);
        if text(call, "toolCallId").starts_with(QUESTION) {
            self.question(chat, ask, call, options, at);
            return;
        }
        let item = self.item_of(chat, text(call, "toolCallId"), call, at);
        chat.change_tool(&item, |tool| {
            news(tool, call);
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
                question: false,
            },
        );
    }

    /// The agent's own question with fixed answers. The chat keeps it as a step of its own.
    fn question(
        &mut self,
        chat: &mut Chat,
        ask: String,
        call: &Value,
        options: Vec<Value>,
        at: u64,
    ) {
        let question = ChatQuestion {
            question: start_of(
                words(call, "title").as_deref().unwrap_or_default(),
                MOST_TARGET * 2,
            )
            .to_owned(),
            header: String::new(),
            options: options
                .iter()
                .map(|option| ChatOption {
                    label: start_of(text(option, "name"), MOST_TARGET).to_owned(),
                    description: String::new(),
                })
                .collect(),
            multiple: false,
        };
        if question.options.is_empty() {
            return;
        }
        let item = chat.push(
            at,
            ChatBody::Tool {
                tool: Box::new(ChatTool {
                    name: "ask_question".to_owned(),
                    kind: ToolKind::Question,
                    state: ToolState::Asking,
                    target: question.question.clone(),
                    detail: Some(ToolDetail::Question {
                        questions: vec![question.clone()],
                        answers: Vec::new(),
                    }),
                    ms: None,
                }),
            },
        );
        chat.ask(ChatAsk {
            id: ask.clone(),
            item: Some(item.clone()),
            body: AskBody::Question {
                questions: vec![question],
            },
        });
        self.asked.insert(
            ask,
            Asked {
                item,
                options,
                question: true,
            },
        );
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
        for item in open(chat) {
            chat.change_tool(&item, |tool| {
                // The agent lets a command that runs go on after its turn, and wakes when it ends.
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
        self.asked.clear();
        chat.close_asks(|_| true);
    }

    /// A line typed into the terminal. The agent's own commands that change the session go to
    /// the session, and the rest is a message for the agent.
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
            self.call(
                "option",
                "session/set_config_option",
                json!({"sessionId": session, "configId": text(&option, "id"), "value": value.trim()}),
            );
            return;
        }
        chat.queue(line.to_owned(), None);
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
        for item in open(chat) {
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

    fn retry(&mut self) {}

    fn has(&self, queued: &str) -> bool {
        self.turn
            .as_ref()
            .and_then(|turn| turn.handed.as_ref())
            .is_some_and(|handed| handed.queued.as_deref() == Some(queued))
    }

    /// The agent offers its own answers: allow once, allow always and deny. A refusal with words
    /// denies, and the words are the next message, as the agent asks the person in its own terminal.
    fn decision(
        &mut self,
        ask: &str,
        answer: &ChatAnswer,
        by: Option<&ChatPerson>,
    ) -> Option<Value> {
        let asked = self.asked.get(ask)?;
        let chosen = |kind: &str| {
            asked
                .options
                .iter()
                .find(|option| text(option, "kind") == kind)
                .and_then(|option| option.get("optionId"))
                .cloned()
        };
        let option = match (asked.question, answer) {
            (true, ChatAnswer::Answers { answers }) => {
                let label = answers.first()?.first()?;
                asked
                    .options
                    .iter()
                    .find(|option| text(option, "name") == label)
                    .and_then(|option| option.get("optionId"))
                    .cloned()?
            }
            (true, _) => return Some(json!({"outcome": {"outcome": "cancelled"}})),
            (false, ChatAnswer::Allow) => chosen("allow_once")?,
            (false, ChatAnswer::Always) => chosen("allow_always")?,
            (false, ChatAnswer::Deny { text }) => {
                self.said = text
                    .as_deref()
                    .map(str::trim)
                    .filter(|words| !words.is_empty())
                    .map(|words| (words.to_owned(), by.cloned()));
                chosen("reject_once")?
            }
            (false, _) => return None,
        };
        Some(json!({"outcome": {"outcome": "selected", "optionId": option}}))
    }

    fn answered(&mut self, chat: &mut Chat, ask: &str, decision: &Value) -> bool {
        let Some(asked) = self.asked.remove(ask) else {
            return true;
        };
        chat.close_asks(|open| open.id == ask);
        let chosen = decision.pointer("/outcome/optionId");
        let option = asked
            .options
            .iter()
            .find(|option| Some(&option["optionId"]) == chosen);
        let refused = option.is_none_or(|option| text(option, "kind").starts_with("reject"));
        let label = option.map(|option| text(option, "name").to_owned());
        chat.change_tool(&asked.item, |tool| {
            if asked.question {
                tool.state = ToolState::Done;
                if let (Some(ToolDetail::Question { answers, .. }), Some(label)) =
                    (&mut tool.detail, label)
                {
                    *answers = vec![label];
                }
            } else if tool.state == ToolState::Asking {
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

    /// The calls of the adapter, then the next message when the agent can start it: the session
    /// began, no turn runs and no question is open.
    fn commands(&mut self, chat: &Chat) -> Vec<Value> {
        let mut commands = self.calls.drain(..).collect::<Vec<_>>();
        let ready = self.linked && !self.starting && self.turn.is_none() && chat.asks().is_empty();
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
                    queued.text.clone(),
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
            at: super::agent::now(),
            stopped: false,
            handed: Some(Handed { queued, text, by }),
        });
        commands
    }

    /// A stop cancels the turn, and each open request of the turn gets "cancelled", as the
    /// protocol asks of a program that cancels.
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

/// The steps that did not end and the answer that the agent still writes.
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

/// The agent could not do what the chat asked, in the agent's words.
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

/// The text of a message's content, which can come in pieces.
fn content_text(update: &Value) -> Option<&str> {
    let content = update.get("content")?;
    (text(content, "type") == "text").then(|| text(content, "text"))
}

/// A new tool call as the chat shows it, before its news.
fn described(update: &Value) -> ChatTool {
    let kind = match text(update, "kind") {
        "read" => ToolKind::Read,
        "edit" | "delete" | "move" => ToolKind::Edit,
        "search" => ToolKind::Search,
        "execute" => ToolKind::Command,
        "fetch" => ToolKind::Web,
        _ => ToolKind::Other,
    };
    // The agent names a call in its title, as in "Running run_command" or "Run invoke_subagent?".
    let title = text(update, "title");
    let named = title
        .strip_prefix("Running ")
        .or_else(|| title.strip_prefix("Run "))
        .map(|name| name.trim_end_matches('?'))
        .filter(|name| !name.is_empty() && !name.contains(char::is_whitespace));
    ChatTool {
        name: named.unwrap_or_else(|| text(update, "kind")).to_owned(),
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

/// The fields that a tool call's news gives: its state, its subject and what it did.
fn news(tool: &mut ChatTool, update: &Value) {
    tool.state = match text(update, "status") {
        "pending" => ToolState::Preparing,
        "in_progress" => ToolState::Running,
        "completed" => ToolState::Done,
        "failed" => ToolState::Failed,
        _ => tool.state,
    };
    let input = update.get("rawInput").unwrap_or(&Value::Null);
    let output = update.get("rawOutput").unwrap_or(&Value::Null);
    let command = ["CommandLine", "commandLine", "command_line", "command"]
        .iter()
        .find_map(|key| words(input, key));
    let place = list(update, "locations")
        .find_map(|location| words(location, "path"))
        .map(|path| file_name(&path).to_owned());
    let subject = command.clone().or(place).or_else(|| {
        ["Query", "query", "Pattern", "pattern", "Url", "url"]
            .iter()
            .find_map(|key| words(input, key))
    });
    if let Some(subject) = subject.or_else(|| words(update, "title")) {
        start_of(&subject, MOST_TARGET).clone_into(&mut tool.target);
    }
    let printed = ["combinedOutput", "combined_output", "output"]
        .iter()
        .find_map(|key| output.get(key).and_then(Value::as_str))
        .map(str::to_owned)
        .or_else(|| {
            let texts = list(update, "content")
                .filter(|content| text(content, "type") == "content")
                .filter_map(|content| content.pointer("/content/text").and_then(Value::as_str))
                .collect::<Vec<_>>();
            (!texts.is_empty()).then(|| texts.join("\n"))
        });
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
                *detail = Some(ToolDetail::Change {
                    path,
                    added: lines.added,
                    removed: lines.removed,
                    lines: lines.lines,
                });
            } else if let Some(printed) = printed.filter(|printed| !printed.trim().is_empty()) {
                *detail = Some(ToolDetail::Output {
                    text: ends_of(&printed, MOST_TEXT),
                });
            }
        }
    }
}
