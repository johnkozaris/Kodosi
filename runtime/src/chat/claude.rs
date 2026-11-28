//! Chat adapter for Claude Code via its hooks module (`links/claude.js`).
//!
//! Field parsing is lenient so schema changes across Claude Code updates degrade gracefully
//! instead of breaking chat.

use std::collections::{HashMap, HashSet, VecDeque};

use serde_json::{Value, json};

use super::{
    AskAlways, AskBody, ChatAnswer, ChatAsk, ChatBody, ChatCommand, ChatContext, ChatControl,
    ChatGiven, ChatMode, ChatNote, ChatNow, ChatPerson, ChatPicture, ChatQuestion, ChatSetting,
    ChatSettingKind, ChatStep, ChatTool, MOST_CALLS, MOST_CHOICES, MOST_OUTPUT, MOST_QUESTIONS,
    MOST_QUEUED, MOST_TARGET, MOST_TEXT, PlanChoice, ToolDetail, ToolKind, ToolState, TurnEnd,
    agent::{
        Agent, Places, Stop, answer, list, person, pictured, question, show_state, stop_open, text,
        words,
    },
    diff,
    mention::with_pictures,
    state::{Chat, ends_of, start_of},
};

pub(crate) const MODULE: &str = include_str!("../../links/claude.js");
pub(super) const VERSION: u64 = 1;
const REFUSED_WITH_WORDS: &str = "The user doesn't want to proceed with this tool use. The tool \
    use was rejected (eg. if it was a file edit, the new_string was NOT written to the file). To \
    tell you how to proceed, the user said:\n";
const MOST_ROWS: usize = 4096;
/// Claude Code writes this text for a message of the agent or the output of a command that has no
/// text.
const NO_CONTENT: &str = "(no content)";
const MOST_SAID: usize = 400;
/// A stop in Claude Code's terminal drops the prompts of plugins that wait in its queue, and it
/// takes none until a person writes to it there.
const STOPPED: [&str; 2] = [
    "the prompt was removed from the queue before it ran",
    "the session was stopped and nobody has sent input since",
];

#[derive(Default)]
pub(super) struct Claude {
    calls: HashMap<String, Call>,
    helpers: HashMap<String, Helper>,
    asked: HashMap<String, Asked>,
    rows: Rows,
    writing: Option<Writing>,
    turn: Option<Turn>,
    now: Option<ChatNow>,
    compacting: bool,
    handed: VecDeque<Handed>,
    /// The handed command whose picker can stay open in the terminal after Claude Code ran it. A
    /// person can close the picker there with no word to the module, so the next message goes on.
    picker: Option<u64>,
    /// The last command of the chat, which the chat showed when it ended. Claude Code writes the
    /// row of some commands only after their end, and of others none.
    ran: Option<String>,
    /// The changes of the model, the effort or fast mode that Kodosi's controls gave the module,
    /// whose rows Claude Code has not written yet.
    changes: VecDeque<String>,
    sent: u64,
    /// Set when Claude Code drops a chat prompt, as after a stop in its terminal; pauses queue
    /// delivery until a turn starts or a person writes again.
    held: bool,
    /// Set when Claude Code lets its plugins change the settings of its /config view: the values
    /// of the settings that the chat changed before Claude Code took the change, which a refusal
    /// puts back.
    changing: Option<HashMap<String, String>>,
    linked: bool,
    session: String,
    ways: Ways,
    /// Refusal text to show as a chat message. Claude Code passes denial text to the model as the
    /// tool result without recording a user message row.
    said: Option<(String, Option<ChatPerson>)>,
}

/// Tracks session permission capabilities to mirror the exit-plan choices Claude Code offers.
#[derive(Default, Clone)]
struct Ways {
    bypass: bool,
    bypass_off: bool,
    auto_off: bool,
    before_plan: String,
}

impl Ways {
    /// Parses CLI flags for the initial permission mode, since Claude Code does not report it
    /// until the first mode change or message.
    fn started(&mut self, arguments: &[&str]) {
        let (mut skip, mut allow, mut mode) = (false, false, None);
        let mut rest = arguments.iter();
        while let Some(argument) = rest.next() {
            match *argument {
                "--dangerously-skip-permissions" => skip = true,
                "--allow-dangerously-skip-permissions" => allow = true,
                "--permission-mode" => mode = rest.next().copied(),
                other => mode = other.strip_prefix("--permission-mode=").or(mode),
            }
        }
        let mode = mode.map(|mode| if mode == "manual" { "default" } else { mode });
        let bypass = !self.bypass_off && (skip || allow || mode == Some("bypassPermissions"));
        self.bypass |= bypass;
        let start = if bypass && skip {
            Some("bypassPermissions")
        } else {
            mode
        };
        if let (true, Some(start)) = (
            self.before_plan.is_empty(),
            start.filter(|start| *start != "plan"),
        ) {
            start.clone_into(&mut self.before_plan);
        }
    }

    fn choices(&self) -> Vec<PlanChoice> {
        let first = if self.bypass {
            PlanChoice::Alone
        } else if !self.auto_off || self.before_plan == "auto" {
            PlanChoice::Auto
        } else {
            PlanChoice::Edits
        };
        vec![first, PlanChoice::Ask]
    }

    fn without_plan(&self) -> Vec<PlanChoice> {
        vec![if self.before_plan == "auto" && !self.auto_off {
            PlanChoice::Auto
        } else {
            PlanChoice::Ask
        }]
    }

    fn saw(&mut self, mode: &str, from: &str) {
        if mode == "bypassPermissions" || from == "bypassPermissions" {
            self.bypass = true;
        }
        if mode != "plan" && !mode.is_empty() {
            mode.clone_into(&mut self.before_plan);
        } else if mode == "plan" && !from.is_empty() && from != "plan" {
            from.clone_into(&mut self.before_plan);
        }
    }
}

struct Call {
    item: Option<String>,
    helper: Option<String>,
}

#[derive(Default)]
struct Helper {
    call: Option<String>,
    task: String,
    working: bool,
}

struct Asked {
    kind: AskKind,
    call: Option<String>,
    input: Value,
    suggestions: Vec<Value>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AskKind {
    Permission,
    Question,
    Plan,
}

struct Writing {
    block: u64,
    item: String,
}

struct Turn {
    stopped: bool,
}

struct Handed {
    n: u64,
    queued: String,
    text: String,
    by: Option<ChatPerson>,
}

#[derive(Default)]
struct Rows {
    seen: HashSet<String>,
    order: VecDeque<String>,
}

impl Rows {
    fn is_new(&mut self, uuid: &str) -> bool {
        if uuid.is_empty() {
            return true;
        }
        if !self.seen.insert(uuid.to_owned()) {
            return false;
        }
        self.order.push_back(uuid.to_owned());
        if self.order.len() > MOST_ROWS
            && let Some(oldest) = self.order.pop_front()
        {
            self.seen.remove(&oldest);
        }
        true
    }
}

impl Claude {
    pub(super) const NAME: &'static str = "claude";

    fn show(&self, chat: &mut Chat) {
        let now = if self.compacting {
            Some(ChatNow::Compacting)
        } else {
            self.now.clone()
        };
        show_state(
            chat,
            self.linked,
            self.compacting || self.turn.is_some(),
            now,
        );
    }

    fn helper(&mut self, id: &str) -> &mut Helper {
        self.helpers.entry(id.to_owned()).or_default()
    }

    fn saw_mode(&mut self, chat: &mut Chat, event: &Value) {
        let name = text(event, "mode");
        self.ways.saw(name, text(event, "from"));
        chat.change_facts(|facts| facts.mode = mode(name).or(facts.mode));
    }

    fn forget(&mut self, chat: &Chat) {
        if self.calls.len() <= MOST_CALLS {
            return;
        }
        let shown = chat
            .items()
            .iter()
            .map(|item| item.id.as_str())
            .collect::<HashSet<_>>();
        let helpers = &self.helpers;
        self.calls.retain(|_, call| {
            let of_item = call
                .item
                .as_deref()
                .is_some_and(|item| shown.contains(item));
            let of_helper = call
                .helper
                .as_ref()
                .and_then(|helper| helpers.get(helper))
                .is_some_and(|helper| helper.working);
            of_item || of_helper
        });
        let calls = &self.calls;
        self.helpers.retain(|_, helper| {
            helper.working
                || helper
                    .call
                    .as_ref()
                    .is_some_and(|call| calls.contains_key(call))
        });
    }

    /// Preserves chat history if the hooks module reloads for the same session; resets when a
    /// new Claude Code session starts, or when a rewind takes the conversation back.
    fn hello(&mut self, chat: &mut Chat, event: &Value) -> Vec<String> {
        let session = text(event, "session");
        let other = !self.session.is_empty() && !session.is_empty() && self.session != session;
        let closed = if other || event.get("rewound").and_then(Value::as_bool) == Some(true) {
            self.renew(chat)
        } else {
            Vec::new()
        };
        session.clone_into(&mut self.session);
        self.linked = true;
        self.ways.bypass |= event.get("bypass").and_then(Value::as_bool) == Some(true);
        self.ways.bypass_off = event.get("bypassOff").and_then(Value::as_bool) == Some(true);
        self.ways.auto_off = event.get("autoOff").and_then(Value::as_bool) == Some(true);
        self.ways.started(
            &list(event, "arguments")
                .filter_map(Value::as_str)
                .collect::<Vec<_>>(),
        );
        self.saw_mode(chat, event);
        chat.change_facts(|facts| {
            facts.agent = Some(Self::NAME.to_owned());
            facts.version = words(event, "version").or_else(|| facts.version.take());
            facts.model = words(event, "model").or_else(|| facts.model.take());
            facts.folder = words(event, "cwd").or_else(|| facts.folder.take());
        });
        if let Some(usage) = event.get("usage") {
            measure(chat, usage);
        }
        self.changing =
            (event.get("settings").and_then(Value::as_bool) == Some(true)).then(HashMap::new);
        let offers_settings = self.changing.is_some();
        chat.set_commands(list(event, "commands").filter_map(|command| {
            let name = words(command, "name")?;
            let opens = opens(&name, offers_settings);
            Some(ChatCommand {
                terminal: opens.is_none() && shows_view(&name),
                opens,
                description: start_of(text(command, "description"), MOST_TARGET).to_owned(),
                name,
            })
        }));
        closed
    }

    fn renew(&mut self, chat: &mut Chat) -> Vec<String> {
        let closed = self.asked.drain().map(|(ask, _)| ask).collect();
        *self = Self {
            handed: std::mem::take(&mut self.handed),
            sent: self.sent,
            session: std::mem::take(&mut self.session),
            linked: true,
            ways: self.ways.clone(),
            ..Self::default()
        };
        chat.reset();
        closed
    }

    /// Replays transcript rows from before the hooks module loaded. Claude Code stores synthetic
    /// records (such as XML command tags and interrupt markers) under the `"user"` role with no
    /// separate kind field, so those prefixes are filtered out.
    fn history(&mut self, chat: &mut Chat, event: &Value) {
        for row in list(event, "rows") {
            if !self.rows.is_new(text(row, "uuid")) {
                continue;
            }
            let words = text(row, "text");
            if text(row, "role") == "user" {
                if !words.is_empty()
                    && !words.starts_with('<')
                    && !words.starts_with("[Request interrupted")
                {
                    chat.push(0, person(words, None));
                }
                continue;
            }
            if !words.is_empty() && words != NO_CONTENT {
                chat.push(0, answer(words, false));
            }
            for used in list(row, "toolUses") {
                let (name, input) = (
                    text(used, "tool"),
                    used.get("input").unwrap_or(&Value::Null),
                );
                if is_hidden(name, input) {
                    continue;
                }
                let mut tool = described(name, input, &Places::of(chat));
                let failed = used.get("isError").and_then(Value::as_bool) == Some(true);
                tool.state = if failed {
                    ToolState::Failed
                } else {
                    ToolState::Done
                };
                tool.ms = used.get("durationMs").and_then(Value::as_u64);
                finish(&mut tool, text(used, "text"));
                chat.push(
                    0,
                    ChatBody::Tool {
                        tool: Box::new(tool),
                    },
                );
            }
        }
    }

    fn row(&mut self, chat: &mut Chat, event: &Value, at: u64) -> Vec<String> {
        if !self.rows.is_new(text(event, "uuid")) {
            return Vec::new();
        }
        let helper = text(event, "agent");
        let door = text(event, "door");
        let mut closed = Vec::new();
        // Claude Code keeps a picture that its terminal took as a block of the prompt beside its
        // words.
        let mut pictures = list(event, "content")
            .filter(|block| text(block, "type") == "image")
            .map(|_| ChatPicture {
                name: String::new(),
            })
            .collect::<Vec<_>>();
        for block in list(event, "content") {
            match (door, text(block, "type")) {
                ("prompt", "text") if helper.is_empty() => {
                    self.prompt(chat, event, block, std::mem::take(&mut pictures), at);
                }
                ("command", "text") if helper.is_empty() => {
                    self.command(chat, event, text(block, "text"), at);
                }
                ("response", "text") if helper.is_empty() => {
                    self.end_writing(chat, Some(text(block, "text")));
                    if self.writing.take().is_none() && text(block, "text") != NO_CONTENT {
                        chat.push(at, answer(text(block, "text"), false));
                    }
                }
                ("response", "text") => self.said(chat, helper, text(block, "text")),
                ("response", "tool_use") if helper.is_empty() => self.call(chat, block, at),
                ("response", "tool_use") => self.step(chat, helper, block),
                ("tool-result", "tool_result") => closed.extend(self.result(chat, block)),
                _ => {}
            }
        }
        closed
    }

    /// Accepts only composer and chat-injected user prompts; other `"prompt"` rows are internal
    /// Claude Code notifications (such as subagent completions).
    fn prompt(
        &mut self,
        chat: &mut Chat,
        event: &Value,
        block: &Value,
        pictures: Vec<ChatPicture>,
        at: u64,
    ) {
        let origin = event.get("origin").unwrap_or(&Value::Null);
        let from_chat = text(origin, "kind") == "plugin"
            && origin.get("asUser").and_then(Value::as_bool) == Some(true);
        if from_chat || text(origin, "kind") == "composer" {
            self.person_said(chat, text(block, "text"), from_chat, pictures, at);
        }
    }

    fn command(&mut self, chat: &mut Chat, event: &Value, row: &str, at: u64) {
        if let Some(output) = between(row, "<local-command-stdout>", "</local-command-stdout>") {
            let output = without_escapes(output);
            let output = output.trim();
            if !output.is_empty() && output != NO_CONTENT {
                chat.push(at, answer(output, false));
            }
            return;
        }
        if let Some(error) = between(row, "<local-command-stderr>", "</local-command-stderr>") {
            chat.push(
                at,
                ChatBody::Note {
                    note: ChatNote::Failed {
                        error: start_of(without_escapes(error).trim(), MOST_TARGET).to_owned(),
                        text: String::new(),
                    },
                },
            );
            return;
        }
        let Some(name) = between(row, "<command-name>", "</command-name>") else {
            return;
        };
        let arguments = between(row, "<command-args>", "</command-args>").unwrap_or_default();
        let words = format!("{} {}", name.trim(), arguments.trim());
        let words = words.trim_end();
        if self.ran.take_if(|ran| ran == words).is_some() {
            return;
        }
        let origin = event.get("origin").unwrap_or(&Value::Null);
        // Claude Code writes a command that only shows something as a row of its own, whoever ran
        // it.
        let from_chat = (text(origin, "kind") == "plugin"
            && text(origin, "name") == crate::agents::PLUGIN_NAME)
            || (text(origin, "kind") == "engine"
                && self.handed.iter().any(|handed| handed.text == words));
        if !from_chat && text(origin, "kind") != "composer" {
            return;
        }
        if from_chat && self.changes.front().is_some_and(|change| change == words) {
            self.changes.pop_front();
            return;
        }
        self.person_said(chat, words, from_chat, Vec::new(), at);
    }

    /// Claude Code shows the recap of a conversation when a person comes back to it.
    fn recap(&mut self, chat: &mut Chat, event: &Value, at: u64) {
        if !self.rows.is_new(text(event, "uuid")) {
            return;
        }
        let words = list(event, "content")
            .filter(|block| text(block, "type") == "text")
            .map(|block| text(block, "text").trim())
            .collect::<Vec<_>>()
            .join("\n");
        if !words.is_empty() {
            chat.push(
                at,
                ChatBody::Note {
                    note: ChatNote::Recap {
                        text: start_of(&words, MOST_OUTPUT).to_owned(),
                    },
                },
            );
        }
    }

    /// A command that Claude Code shows in a view of its terminal ends when a person closes the
    /// view there, and its message stays in the queue until then.
    fn waits(&mut self, chat: &mut Chat, event: &Value) {
        let n = event.get("n").and_then(Value::as_u64);
        if self.turn.is_some() || self.compacting {
            return;
        }
        if let Some(handed) = self.handed.iter().find(|handed| Some(handed.n) == n) {
            chat.in_terminal(&handed.queued);
            chat.give(&handed.queued, ChatGiven::Turn);
            if event.get("picker").and_then(Value::as_bool) == Some(true) {
                self.picker = n;
            }
        }
    }

    fn person_said(
        &mut self,
        chat: &mut Chat,
        words: &str,
        from_chat: bool,
        pictures: Vec<ChatPicture>,
        at: u64,
    ) {
        self.held &= from_chat;
        self.end_writing(chat, None);
        // Claude Code processes injected prompts FIFO and may rewrite their text, so fall back to
        // the oldest pending entry if there is no exact match.
        let handed = from_chat
            .then(|| {
                let same = self.handed.iter().position(|handed| handed.text == words);
                self.handed.remove(same.unwrap_or_default())
            })
            .flatten();
        let by = handed.and_then(|handed| {
            chat.unqueue(&handed.queued);
            handed.by
        });
        chat.push(at, pictured(words, by, pictures));
    }

    fn write(&mut self, chat: &mut Chat, event: &Value, at: u64) {
        let block = event
            .get("block")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let more = text(event, "text");
        match &self.writing {
            Some(writing) if writing.block == block => chat.write(&writing.item, more),
            _ => {
                self.end_writing(chat, None);
                let item = chat.push(at, answer(more, true));
                self.writing = Some(Writing { block, item });
            }
        }
        self.now = Some(ChatNow::Writing);
    }

    fn end_writing(&mut self, chat: &mut Chat, whole: Option<&str>) {
        let Some(writing) = &self.writing else {
            return;
        };
        let written = match (whole, chat.body(&writing.item)) {
            (Some(whole), _) => whole.to_owned(),
            (None, Some(ChatBody::Text { text, .. })) => text.clone(),
            _ => String::new(),
        };
        chat.put(&writing.item, answer(&written, false));
        if whole.is_none() {
            self.writing = None;
        }
    }

    fn begin(&mut self, chat: &mut Chat, event: &Value, at: u64) {
        let (id, name) = (text(event, "id"), text(event, "name"));
        if id.is_empty() || is_hidden(name, &Value::Null) {
            return;
        }
        self.end_writing(chat, None);
        let tool = described(name, &Value::Null, &Places::of(chat));
        let item = chat.push(
            at,
            ChatBody::Tool {
                tool: Box::new(tool),
            },
        );
        self.now = Some(ChatNow::Tool { item: item.clone() });
        self.calls.insert(
            id.to_owned(),
            Call {
                item: Some(item),
                helper: None,
            },
        );
    }

    fn call(&mut self, chat: &mut Chat, block: &Value, at: u64) {
        self.forget(chat);
        let (id, name) = (text(block, "id"), text(block, "name"));
        let input = block.get("input").unwrap_or(&Value::Null);
        let known = self.calls.get(id).and_then(|call| call.item.clone());
        if is_hidden(name, input) {
            if let Some(item) = known {
                chat.remove(&item);
            }
            self.calls.insert(
                id.to_owned(),
                Call {
                    item: None,
                    helper: None,
                },
            );
            return;
        }
        let tool = described(name, input, &Places::of(chat));
        if let Some(item) = known {
            let state = chat.tool(&item).map_or(tool.state, |tool| tool.state);
            chat.put(
                &item,
                ChatBody::Tool {
                    tool: Box::new(ChatTool { state, ..tool }),
                },
            );
            return;
        }
        let item = chat.push(
            at,
            ChatBody::Tool {
                tool: Box::new(tool),
            },
        );
        self.calls.insert(
            id.to_owned(),
            Call {
                item: Some(item),
                helper: None,
            },
        );
    }

    fn step(&mut self, chat: &mut Chat, helper: &str, block: &Value) {
        self.forget(chat);
        self.calls.insert(
            text(block, "id").to_owned(),
            Call {
                item: None,
                helper: Some(helper.to_owned()),
            },
        );
        let step = described(
            text(block, "name"),
            block.get("input").unwrap_or(&Value::Null),
            &Places::of(chat),
        );
        self.change_helper(chat, helper, |steps, doing, _| {
            *steps = steps.saturating_add(1);
            *doing = Some(ChatStep {
                kind: step.kind,
                target: step.target,
            });
        });
    }

    fn said(&self, chat: &mut Chat, helper: &str, words: &str) {
        self.change_helper(chat, helper, |_, _, said| {
            start_of(words, MOST_SAID).clone_into(said);
        });
    }

    fn change_helper(
        &self,
        chat: &mut Chat,
        helper: &str,
        change: impl FnOnce(&mut u32, &mut Option<ChatStep>, &mut String),
    ) {
        let item = self
            .helpers
            .get(helper)
            .and_then(|helper| helper.call.as_ref())
            .and_then(|call| self.calls.get(call))
            .and_then(|call| call.item.clone());
        if let Some(item) = item {
            chat.change_tool(&item, |tool| {
                if let Some(ToolDetail::Agent {
                    steps, doing, said, ..
                }) = &mut tool.detail
                {
                    change(steps, doing, said);
                }
            });
        }
    }

    fn started_helper(&mut self, chat: &mut Chat, event: &Value) {
        let (id, call) = (text(event, "id"), text(event, "call"));
        if id.is_empty() || call.is_empty() {
            return;
        }
        let task = start_of(text(event, "description"), MOST_TARGET).to_owned();
        let helper = self.helper(id);
        helper.call = Some(call.to_owned());
        helper.task.clone_from(&task);
        helper.working = true;
        if let Some(item) = self.calls.get(call).and_then(|call| call.item.clone()) {
            chat.change_tool(&item, |tool| {
                tool.state = ToolState::Running;
                if tool.target.is_empty() {
                    tool.target = task;
                }
            });
        }
    }

    fn end_helper(&mut self, chat: &mut Chat, id: &str, reason: &str) {
        let helper = self.helper(id);
        helper.working = false;
        let item = helper
            .call
            .clone()
            .and_then(|call| self.calls.get(&call))
            .and_then(|call| call.item.clone());
        if let Some(item) = item {
            chat.change_tool(&item, |tool| {
                if tool.state.is_open() {
                    tool.state = match reason {
                        "aborted" => ToolState::Stopped,
                        "error" | "refusal" => ToolState::Failed,
                        _ => ToolState::Done,
                    };
                }
            });
        }
    }

    fn works(&self, call: &str) -> bool {
        self.helpers
            .values()
            .any(|helper| helper.working && helper.call.as_deref() == Some(call))
    }

    fn result(&mut self, chat: &mut Chat, block: &Value) -> Vec<String> {
        let id = text(block, "tool_use_id");
        let Some(item) = self.calls.get(id).and_then(|call| call.item.clone()) else {
            return self.close(chat, id);
        };
        let failed = block.get("is_error").and_then(Value::as_bool) == Some(true);
        let output = list(block, "content")
            .filter(|part| text(part, "type") == "text")
            .map(|part| text(part, "text"))
            .collect::<Vec<_>>()
            .join("\n");
        let stopped = self.turn.as_ref().is_some_and(|turn| turn.stopped);
        let works = self.works(id);
        chat.change_tool(&item, |tool| {
            if failed {
                tool.state = match tool.state {
                    ToolState::Denied => ToolState::Denied,
                    _ if stopped => ToolState::Stopped,
                    _ => ToolState::Failed,
                };
                if tool.state == ToolState::Failed {
                    fail(tool, &output);
                }
            } else if !works {
                tool.state = ToolState::Done;
                finish(tool, &output);
            }
        });
        self.close(chat, id)
    }

    fn close(&mut self, chat: &mut Chat, call: &str) -> Vec<String> {
        let over = self
            .asked
            .iter()
            .filter(|(_, asked)| asked.call.as_deref() == Some(call))
            .map(|(ask, _)| ask.clone())
            .collect::<Vec<_>>();
        for ask in &over {
            self.asked.remove(ask);
        }
        chat.close_asks(|ask| over.contains(&ask.id))
    }

    fn ask(&mut self, chat: &mut Chat, event: &Value) {
        let (id, name) = (text(event, "ask"), text(event, "tool"));
        if id.is_empty() {
            return;
        }
        let input = event.get("input").cloned().unwrap_or(Value::Null);
        let suggestions = list(event, "suggestions").cloned().collect::<Vec<_>>();
        let call = words(event, "call");
        let item = call
            .as_ref()
            .and_then(|call| self.calls.get(call))
            .and_then(|call| call.item.clone());
        let places = Places::of(chat);
        let asking = described(name, &input, &places);
        let (kind, body) = match asking.detail {
            Some(ToolDetail::Question { questions, .. }) => {
                (AskKind::Question, AskBody::Question { questions })
            }
            Some(ToolDetail::Plan { text }) => (
                AskKind::Plan,
                AskBody::Plan {
                    choices: if text.trim().is_empty() {
                        self.ways.without_plan()
                    } else {
                        self.ways.choices()
                    },
                    text,
                },
            ),
            _ => (
                AskKind::Permission,
                AskBody::Permission {
                    tool: Box::new(ChatTool {
                        state: ToolState::Asking,
                        ..asking
                    }),
                    always: suggestions
                        .first()
                        .and_then(|suggestion| always(suggestion, &places)),
                    helper: self
                        .helpers
                        .get(text(event, "agent"))
                        .map_or_default(|helper| helper.task.clone()),
                },
            ),
        };
        if let Some(item) = &item {
            chat.change_tool(item, |tool| tool.state = ToolState::Asking);
        }
        self.saw_mode(chat, event);
        self.asked.insert(
            id.to_owned(),
            Asked {
                kind,
                call,
                input,
                suggestions,
            },
        );
        chat.ask(ChatAsk {
            id: id.to_owned(),
            item,
            body,
        });
    }

    fn decided(&mut self, chat: &mut Chat, event: &Value) -> Vec<String> {
        let id = text(event, "id");
        self.saw_mode(chat, event);
        let Some(call) = self.calls.get(id) else {
            return Vec::new();
        };
        if let Some(item) = call.item.clone() {
            let accepted = text(event, "decision") == "accept";
            chat.change_tool(&item, |tool| {
                if tool.state.is_open() {
                    tool.state = if accepted {
                        ToolState::Running
                    } else {
                        ToolState::Denied
                    };
                }
            });
            if accepted {
                self.now = Some(ChatNow::Tool { item });
            }
        }
        self.close(chat, id)
    }

    /// Supplements the transcript `tool_result` row with hook-only metadata (`ms`, background task
    /// status, and structured question answers).
    fn done(&self, chat: &mut Chat, event: &Value) {
        let id = text(event, "id");
        let Some(item) = self.calls.get(id).and_then(|call| call.item.clone()) else {
            return;
        };
        let result = event.get("result").unwrap_or(&Value::Null);
        let works = self.works(id);
        chat.change_tool(&item, |tool| {
            tool.ms = event.get("ms").and_then(Value::as_u64).or(tool.ms);
            if tool.state.is_open() && !works {
                tool.state = ToolState::Done;
            }
            match &mut tool.detail {
                // For backgrounded commands, the tool result text is Claude Code's notice to the
                // model rather than actual command output.
                Some(ToolDetail::Command {
                    background, output, ..
                }) => {
                    *background = result.get("backgroundTaskId").is_some_and(Value::is_string);
                    if *background {
                        output.clear();
                    }
                }
                Some(ToolDetail::Question { questions, answers }) => {
                    let given = questions
                        .iter()
                        .map(|question| {
                            let given = result
                                .get("answers")
                                .and_then(|answers| answers.get(&question.question));
                            start_of(
                                given.and_then(Value::as_str).unwrap_or_default(),
                                MOST_TARGET,
                            )
                            .to_owned()
                        })
                        .collect::<Vec<_>>();
                    if given.iter().any(|answer| !answer.is_empty()) {
                        *answers = given;
                    }
                }
                _ => {}
            }
        });
    }

    fn end_turn(&mut self, chat: &mut Chat, event: &Value, at: u64) -> Vec<String> {
        let stopped = self.turn.take().is_some_and(|turn| turn.stopped);
        self.end_writing(chat, None);
        self.now = None;
        let open = chat
            .items()
            .iter()
            .filter_map(|item| match &item.body {
                ChatBody::Tool { tool } if tool.state.is_open() => Some(item.id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        for item in open {
            let call = self
                .calls
                .iter()
                .find(|(_, call)| call.item.as_ref() == Some(&item))
                .map(|(id, _)| id.clone());
            if call.as_deref().is_some_and(|call| self.works(call)) {
                continue;
            }
            let unwritten = chat
                .tool(&item)
                .is_some_and(|tool| tool.state == ToolState::Preparing && tool.target.is_empty());
            if unwritten {
                chat.remove(&item);
            } else {
                chat.change_tool(&item, |tool| tool.state = ToolState::Stopped);
            }
        }
        let end = match text(event, "reason") {
            "aborted" => TurnEnd::Stopped,
            "error" => TurnEnd::Failed,
            "refusal" => TurnEnd::Refused,
            _ if stopped => TurnEnd::Stopped,
            _ => TurnEnd::Done,
        };
        chat.push(
            at,
            ChatBody::Turn {
                end,
                ms: event.get("ms").and_then(Value::as_u64).unwrap_or_default(),
            },
        );
        let helpers = &self.helpers;
        let calls = &self.calls;
        let of_main = |asked: &Asked| {
            asked
                .call
                .as_ref()
                .and_then(|call| calls.get(call))
                .is_none_or(|call| {
                    call.helper
                        .as_ref()
                        .and_then(|helper| helpers.get(helper))
                        .is_none_or(|helper| !helper.working)
                })
        };
        let over = self
            .asked
            .iter()
            .filter(|(_, asked)| of_main(asked))
            .map(|(ask, _)| ask.clone())
            .collect::<Vec<_>>();
        for ask in &over {
            self.asked.remove(ask);
        }
        chat.close_asks(|ask| over.contains(&ask.id))
    }

    fn end(&mut self, chat: &mut Chat, reason: &str) -> Vec<String> {
        if matches!(reason, "clear" | "resume") {
            return self.renew(chat);
        }
        self.relinked(chat);
        self.lost(chat)
    }

    /// Claude Code can start a command now: no turn runs, no question is open and no earlier
    /// message waits in Claude Code, other than a command whose picker can be open in the
    /// terminal.
    fn can_start(&self, chat: &Chat) -> bool {
        self.turn.is_none()
            && !self.compacting
            && chat.asks().is_empty()
            && self
                .handed
                .iter()
                .all(|handed| Some(handed.n) == self.picker)
    }

    /// Handles completion of an injected prompt or slash command.
    ///
    /// Dropped prompts stay queued, taken back, and set `held`. A prompt that a stop dropped
    /// waits as before; any other drop is a refusal, shown with its reason. Successful
    /// non-command prompts remain in `handed` until their transcript row arrives, whereas slash
    /// commands such as `/clear` may not emit a row in the current session, so they are shown as
    /// the person's message and unqueued here.
    fn did(&mut self, chat: &mut Chat, event: &Value, at: u64) {
        let (error, drop) = (text(event, "error"), text(event, "drop"));
        let n = event.get("n").and_then(Value::as_u64);
        let Some(index) = self.handed.iter().position(|handed| Some(handed.n) == n) else {
            return;
        };
        if error.is_empty() && drop.is_empty() && !self.handed[index].text.starts_with('/') {
            return;
        }
        let Some(handed) = self.handed.remove(index) else {
            return;
        };
        if self.picker == Some(handed.n) {
            self.picker = None;
        }
        if !drop.is_empty() {
            let refusal =
                (!STOPPED.contains(&drop)).then(|| start_of(drop, MOST_TARGET).to_owned());
            chat.take_back(&handed.queued);
            chat.refuse(&handed.queued, refusal);
            self.held = true;
            return;
        }
        if handed.text.starts_with('/') {
            self.ran_command(chat, handed, at);
        } else {
            chat.unqueue(&handed.queued);
        }
        if !error.is_empty() {
            chat.push(
                at,
                ChatBody::Note {
                    note: ChatNote::Failed {
                        error: start_of(error, MOST_TARGET).to_owned(),
                        text: String::new(),
                    },
                },
            );
        }
    }

    /// A command of the chat shows as its person's message when it ends, or when its compaction
    /// starts.
    fn ran_command(&mut self, chat: &mut Chat, handed: Handed, at: u64) {
        chat.unqueue(&handed.queued);
        self.end_writing(chat, None);
        chat.push(at, person(&handed.text, handed.by));
        self.ran = Some(handed.text);
    }

    /// Claude Code took a change of a setting, or refused it and the setting goes back.
    fn was_set(&mut self, chat: &mut Chat, event: &Value) {
        let key = text(event, "key");
        let before = self
            .changing
            .as_mut()
            .and_then(|changing| changing.remove(key));
        match words(event, "deny") {
            Some(refusal) => chat.change_setting(key, |setting| {
                if let Some(before) = before {
                    setting.value = before;
                }
                setting.refusal = Some(refusal);
            }),
            None => {
                if let Some(value) = event.get("value").and_then(setting_words) {
                    chat.change_setting(key, |setting| setting.value = value);
                }
            }
        }
    }

    /// The module stopped a command that the end of the turn had moved to the background.
    fn halted(&self, chat: &mut Chat, event: &Value) {
        if event.get("ok").and_then(Value::as_bool) != Some(true) {
            return;
        }
        let item = self
            .calls
            .get(text(event, "id"))
            .and_then(|call| call.item.clone());
        if let Some(item) = item {
            chat.change_tool(&item, |tool| {
                tool.state = ToolState::Stopped;
                if let Some(ToolDetail::Command { background, .. }) = &mut tool.detail {
                    *background = false;
                }
            });
        }
    }
}

impl Agent for Claude {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn apply(&mut self, chat: &mut Chat, event: &Value) -> Vec<String> {
        let at = event.get("t").and_then(Value::as_u64).unwrap_or_default();
        let helper = text(event, "agent");
        let kind = text(event, "k");
        // Synthesize an active turn if the hooks module attached mid-turn and missed `"turn"`.
        if helper.is_empty() && matches!(kind, "step" | "think" | "text" | "tool") {
            self.turn.get_or_insert(Turn { stopped: false });
        }
        let mut closed = Vec::new();
        match kind {
            "hello" => closed = self.hello(chat, event),
            "history" => self.history(chat, event),
            "row" => closed = self.row(chat, event, at),
            "turn" if helper.is_empty() => {
                self.turn = Some(Turn { stopped: false });
                self.now = None;
                self.held = false;
            }
            "turn" => self.helper(helper).working = true,
            "step" if helper.is_empty() => {
                self.end_writing(chat, None);
                chat.change_facts(|facts| {
                    facts.model = words(event, "model").or_else(|| facts.model.take());
                    facts.effort = words(event, "effort").or_else(|| facts.effort.take());
                });
            }
            "think" if helper.is_empty() => self.now = Some(ChatNow::Thinking),
            "text" if helper.is_empty() => self.write(chat, event, at),
            "tool" if helper.is_empty() => self.begin(chat, event, at),
            "stepEnd" if helper.is_empty() => self.end_writing(chat, None),
            "turnEnd" if helper.is_empty() => closed = self.end_turn(chat, event, at),
            "turnEnd" => self.end_helper(chat, helper, text(event, "reason")),
            "ask" => self.ask(chat, event),
            "decided" => closed = self.decided(chat, event),
            "done" => self.done(chat, event),
            "halted" => self.halted(chat, event),
            "agent" => self.started_helper(chat, event),
            "mode" => {
                self.saw_mode(chat, event);
                if let Some(background) = event.get("background").and_then(Value::as_u64) {
                    chat.change_facts(|facts| {
                        facts.background = u32::try_from(background).unwrap_or(u32::MAX);
                    });
                }
            }
            "measure" => measure(chat, event),
            "model" => chat.change_facts(|facts| {
                facts.model = words(event, "model").or_else(|| facts.model.take());
            }),
            "cwd" => chat.change_facts(|facts| {
                facts.folder = words(event, "cwd").or_else(|| facts.folder.take());
            }),
            "compact" => {
                self.compacting = text(event, "phase") == "start";
                let compact = self
                    .handed
                    .iter()
                    .position(|handed| handed.text.split_whitespace().next() == Some("/compact"));
                if self.compacting
                    && let Some(handed) = compact.and_then(|index| self.handed.remove(index))
                {
                    self.ran_command(chat, handed, at);
                }
                if !self.compacting {
                    chat.push(
                        at,
                        ChatBody::Note {
                            note: ChatNote::Compacted,
                        },
                    );
                }
            }
            "failure" => {
                chat.push(
                    at,
                    ChatBody::Note {
                        note: ChatNote::Failed {
                            error: start_of(text(event, "error"), MOST_TARGET).to_owned(),
                            text: ends_of(text(event, "details"), MOST_OUTPUT),
                        },
                    },
                );
            }
            "stop" => {
                if let Some(turn) = &mut self.turn {
                    turn.stopped = true;
                }
            }
            "end" => closed = self.end(chat, text(event, "reason")),
            "did" => self.did(chat, event, at),
            "recap" => self.recap(chat, event, at),
            "waits" => self.waits(chat, event),
            "settings" => chat.set_settings(list(event, "rows").filter_map(setting)),
            "set" => self.was_set(chat, event),
            _ => {}
        }
        self.show(chat);
        closed
    }

    fn lost(&mut self, chat: &mut Chat) -> Vec<String> {
        self.linked = false;
        self.end_writing(chat, None);
        self.turn = None;
        self.now = None;
        self.compacting = false;
        // A command that Claude Code showed in a view of its terminal ended with that view.
        let shown = chat
            .queued()
            .iter()
            .filter(|queued| queued.terminal)
            .map(|queued| queued.id.clone())
            .collect::<Vec<_>>();
        for queued in shown {
            chat.unqueue(&queued);
        }
        // A prompt that Claude Code holds stays with it while its link is silent; the session's
        // end or another link takes it back.
        self.handed.retain(|handed| !handed.text.starts_with('/'));
        self.picker = None;
        self.changes.clear();
        for helper in self.helpers.values_mut() {
            helper.working = false;
        }
        stop_open(chat);
        self.asked.clear();
        let closed = chat.close_asks(|_| true);
        self.show(chat);
        closed
    }

    fn back(&mut self, chat: &mut Chat) {
        if !self.linked {
            self.linked = true;
            self.show(chat);
        }
    }

    /// The prompts that Claude Code held wait in the chat's queue again.
    fn relinked(&mut self, chat: &mut Chat) {
        for handed in self.handed.drain(..) {
            chat.take_back(&handed.queued);
        }
        self.picker = None;
    }

    fn is_linked(&self) -> bool {
        self.linked
    }

    fn retry(&mut self) {
        self.held = false;
    }

    fn has(&self, queued: &str) -> bool {
        self.handed.iter().any(|handed| handed.queued == queued)
    }

    fn decision(
        &mut self,
        ask: &str,
        answer: &ChatAnswer,
        by: Option<&ChatPerson>,
    ) -> Option<Value> {
        let asked = self.asked.get(ask)?;
        Some(match (asked.kind, answer) {
            (AskKind::Permission, ChatAnswer::Allow) => json!({"behavior": "allow"}),
            (AskKind::Permission, ChatAnswer::Always) => json!({
                "behavior": "allow",
                "updatedPermissions": [asked.suggestions.first()?],
            }),
            // Claude Code sends `message` verbatim to the model as the tool result, so match its
            // built-in rejection prefix.
            (_, ChatAnswer::Deny { text: Some(words) }) if !words.trim().is_empty() => {
                self.said = Some((words.trim().to_owned(), by.cloned()));
                json!({
                    "behavior": "deny",
                    "message": format!("{REFUSED_WITH_WORDS}{}", words.trim()),
                })
            }
            (_, ChatAnswer::Deny { .. }) => refusal("The person did not allow this."),
            (AskKind::Question, ChatAnswer::Answers { answers }) => {
                let questions = list(&asked.input, "questions").collect::<Vec<_>>();
                if questions.len() != answers.len()
                    || answers
                        .iter()
                        .any(|answer| answer.iter().all(|part| part.trim().is_empty()))
                {
                    return None;
                }
                let given = questions
                    .iter()
                    .zip(answers)
                    .map(|(question, answer)| {
                        (
                            text(question, "question").to_owned(),
                            Value::String(answer.join(", ")),
                        )
                    })
                    .collect::<serde_json::Map<_, _>>();
                let mut input = asked.input.clone();
                input
                    .as_object_mut()?
                    .insert("answers".to_owned(), given.into());
                json!({"behavior": "allow", "updatedInput": input})
            }
            (AskKind::Plan, ChatAnswer::Plan { choice }) => {
                let mode = match choice {
                    PlanChoice::Alone => "bypassPermissions",
                    // Omit `setMode` when returning to `auto`: Claude Code restores the pre-plan
                    // mode itself, which runs its full auto-mode transition logic.
                    PlanChoice::Auto if self.ways.before_plan == "auto" => {
                        return Some(json!({"behavior": "allow", "updatedInput": asked.input}));
                    }
                    PlanChoice::Auto => "auto",
                    PlanChoice::Edits => "acceptEdits",
                    PlanChoice::Ask => "default",
                    _ => return None,
                };
                json!({
                    "behavior": "allow",
                    "updatedInput": asked.input,
                    "updatedPermissions": [{"type": "setMode", "mode": mode, "destination": "session"}],
                })
            }
            _ => return None,
        })
    }

    fn answered(&mut self, chat: &mut Chat, ask: &str, decision: &Value) -> bool {
        let said = self.said.take();
        let Some(asked) = self.asked.remove(ask) else {
            return true;
        };
        chat.close_asks(|open| open.id == ask);
        if let Some((words, by)) = said {
            chat.push(crate::identity::now_ms(), person(&words, by));
        }
        let allowed = text(decision, "behavior") == "allow";
        let item = asked
            .call
            .as_ref()
            .and_then(|call| self.calls.get(call))
            .and_then(|call| call.item.clone());
        if let Some(item) = item {
            chat.change_tool(&item, |tool| {
                if tool.state == ToolState::Asking {
                    tool.state = if allowed {
                        ToolState::Running
                    } else {
                        ToolState::Denied
                    };
                }
            });
        }
        self.show(chat);
        true
    }

    /// The messages of the queue, in order, as commands for the module, each as soon as Claude
    /// Code takes it. Claude Code keeps a plugin's prompt in its own queue while it works, as it
    /// does a prompt typed in its terminal, but gives it to the model only once it is idle, not at
    /// its next step; so a prompt goes to it at once, given. A command waits until Claude Code can
    /// start it: no turn runs, no question is open and no earlier message waits in Claude Code,
    /// other than a command whose picker can be open in the terminal.
    fn commands(&mut self, chat: &mut Chat) -> Vec<Value> {
        if !self.linked || self.held {
            return Vec::new();
        }
        let mut commands = Vec::new();
        let mut given = Vec::new();
        for queued in chat.queued() {
            if self.handed.iter().any(|handed| handed.queued == queued.id) {
                continue;
            }
            let command = queued.said.trim_start().starts_with('/');
            if command && (!commands.is_empty() || !self.can_start(chat)) {
                break;
            }
            self.sent += 1;
            let text = if command {
                queued.said.clone()
            } else {
                given.push(queued.id.clone());
                with_pictures(&queued.said, &queued.files)
            };
            commands.push(json!({"do": "prompt", "n": self.sent, "text": text}));
            self.handed.push_back(Handed {
                n: self.sent,
                queued: queued.id.clone(),
                text,
                by: queued.by.clone(),
            });
            if command {
                break;
            }
        }
        for queued in given {
            chat.give(&queued, ChatGiven::Turn);
        }
        commands
    }

    fn change(&mut self, line: &str) -> Option<Value> {
        if !self.linked {
            return None;
        }
        self.sent += 1;
        self.changes.push_back(line.to_owned());
        if self.changes.len() > MOST_QUEUED {
            self.changes.pop_front();
        }
        Some(json!({"do": "change", "n": self.sent, "text": line}))
    }

    fn settings(&mut self, chat: &mut Chat) -> Option<Value> {
        if !self.linked || self.changing.is_none() {
            return None;
        }
        chat.forget_refusals();
        Some(json!({"do": "settings"}))
    }

    /// The change shows at once, and goes back if Claude Code refuses it.
    fn set(&mut self, chat: &mut Chat, key: &str, value: &str) -> Option<Value> {
        if !self.linked {
            return None;
        }
        let changing = self.changing.as_mut()?;
        let setting = chat
            .settings()
            .iter()
            .find(|setting| setting.key == key && !setting.locked)?;
        let typed = setting_value(setting, value)?;
        changing
            .entry(key.to_owned())
            .or_insert_with(|| setting.value.clone());
        chat.change_setting(key, |setting| {
            value.clone_into(&mut setting.value);
            setting.refusal = None;
        });
        Some(json!({"do": "set", "key": key, "value": typed}))
    }

    /// How the turn of the main conversation stops for a person in the chat.
    fn stop(&mut self, chat: &Chat) -> Stop {
        if !chat.asks().is_empty() {
            return Stop::Decisions(
                chat.asks()
                    .iter()
                    .map(|ask| (ask.id.clone(), refusal("The person stopped the turn.")))
                    .collect(),
            );
        }
        if self.turn.is_some() {
            Stop::Command(json!({"do": "stop"}))
        } else {
            Stop::Nothing
        }
    }
}

/// The commands whose own view in the terminal a control of the chat takes the place of.
fn opens(name: &str, offers_settings: bool) -> Option<ChatControl> {
    match name {
        "model" | "effort" => Some(ChatControl::Model),
        "usage" | "context" => Some(ChatControl::Usage),
        "config" | "settings" if offers_settings => Some(ChatControl::Settings),
        _ => None,
    }
}

/// A row of Claude Code's /config view.
fn setting(row: &Value) -> Option<ChatSetting> {
    let key = words(row, "key")?;
    let kind = match text(row, "kind") {
        "boolean" => ChatSettingKind::Toggle,
        "choice" => ChatSettingKind::Choice,
        "text" => ChatSettingKind::Text,
        "number" => ChatSettingKind::Number,
        _ => return None,
    };
    Some(ChatSetting {
        label: words(row, "label").unwrap_or_else(|| key.clone()),
        description: start_of(text(row, "description"), MOST_TARGET).to_owned(),
        kind,
        value: setting_words(row.get("value")?)?,
        options: list(row, "options")
            .filter_map(Value::as_str)
            .take(MOST_CHOICES)
            .map(|option| start_of(option, MOST_TARGET).to_owned())
            .collect(),
        locked: row.get("isLocked").and_then(Value::as_bool) == Some(true),
        refusal: None,
        key,
    })
}

fn setting_words(value: &Value) -> Option<String> {
    let words = match value {
        Value::Bool(on) => on.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(words) => start_of(words, MOST_TARGET).to_owned(),
        _ => return None,
    };
    Some(words)
}

/// The value that Claude Code takes for a setting, from its words.
fn setting_value(setting: &ChatSetting, words: &str) -> Option<Value> {
    match setting.kind {
        ChatSettingKind::Toggle => words.parse::<bool>().ok().map(Value::Bool),
        ChatSettingKind::Number => {
            let words = words.trim();
            words
                .parse::<i64>()
                .map(Value::from)
                .or_else(|_| words.parse::<f64>().map(Value::from))
                .ok()
                .filter(Value::is_number)
        }
        ChatSettingKind::Choice => setting
            .options
            .iter()
            .any(|option| option == words)
            .then(|| Value::from(words)),
        ChatSettingKind::Text => Some(Value::from(words)),
    }
}

/// The commands that, given without words, open a view of Claude Code's terminal for a person to
/// use there. Claude Code does not tell its plugins which commands do.
fn shows_view(name: &str) -> bool {
    matches!(
        name,
        "add-dir"
            | "config"
            | "doctor"
            | "export"
            | "help"
            | "hooks"
            | "ide"
            | "login"
            | "logout"
            | "mcp"
            | "memory"
            | "permissions"
            | "plugin"
            | "privacy-settings"
            | "resume"
            | "rewind"
            | "skills"
            | "status"
            | "tasks"
            | "theme"
    )
}

/// The text between two marks that Claude Code puts around a part of a row.
fn between<'a>(row: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let after = &row[row.find(start)? + start.len()..];
    Some(&after[..after.find(end)?])
}

/// A command prints its output for a terminal, with the escape codes of its colors and styles,
/// which the chat does not draw.
fn without_escapes(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut letters = text.chars().peekable();
    while let Some(letter) = letters.next() {
        if letter != '\u{1b}' {
            plain.push(letter);
            continue;
        }
        match letters.next() {
            Some('[') => {
                for code in letters.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&code) {
                        break;
                    }
                }
            }
            Some(']' | 'P' | '_' | '^') => {
                while let Some(code) = letters.next() {
                    if code == '\u{7}' || (code == '\u{1b}' && letters.next_if_eq(&'\\').is_some())
                    {
                        break;
                    }
                }
            }
            Some(code) if ('\u{20}'..='\u{2f}').contains(&code) => {
                letters.next();
            }
            _ => {}
        }
    }
    plain
}

fn refusal(words: &str) -> Value {
    json!({"behavior": "deny", "message": words, "interrupt": true})
}

fn measure(chat: &mut Chat, usage: &Value) {
    chat.change_facts(|facts| {
        if let Some(window) = usage.pointer("/context/window").and_then(Value::as_u64) {
            facts.context = Some(ChatContext {
                tokens: usage
                    .pointer("/context/tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or_default(),
                window,
            });
        }
        let limits = usage.get("limits").or_else(|| usage.get("rateLimits"));
        if let Some(limits) =
            limits.filter(|limits| limits.as_array().is_some_and(|l| !l.is_empty()))
        {
            facts.limits = crate::agents::claude_session(limits);
        }
        facts.cost = usage
            .pointer("/cost/usd")
            .and_then(Value::as_f64)
            .filter(|cost| cost.is_finite() && *cost >= 0.0)
            .or(facts.cost);
    });
}

fn mode(name: &str) -> Option<ChatMode> {
    Some(match name {
        "default" => ChatMode::Ask,
        "acceptEdits" => ChatMode::Edits,
        "plan" => ChatMode::Plan,
        "auto" => ChatMode::Auto,
        "dontAsk" => ChatMode::Rules,
        "bypassPermissions" => ChatMode::Unrestricted,
        _ => return None,
    })
}

fn kind(name: &str) -> ToolKind {
    match name {
        "Bash" => ToolKind::Command,
        "Read" | "NotebookRead" => ToolKind::Read,
        "Edit" | "MultiEdit" | "NotebookEdit" => ToolKind::Edit,
        "Write" => ToolKind::Write,
        "Grep" | "Glob" | "LS" => ToolKind::Search,
        "WebFetch" | "WebSearch" => ToolKind::Web,
        "Agent" | "Task" => ToolKind::Agent,
        "Skill" => ToolKind::Skill,
        "AskUserQuestion" => ToolKind::Question,
        "ExitPlanMode" => ToolKind::Plan,
        _ => ToolKind::Other,
    }
}

/// Hides internal tool discovery and plan-file writes (the plan is shown via `ExitPlanMode`).
fn is_hidden(name: &str, input: &Value) -> bool {
    name == "ToolSearch"
        || (matches!(kind(name), ToolKind::Write | ToolKind::Edit)
            && text(input, "file_path")
                .replace('\\', "/")
                .contains("/.claude/plans/"))
}

fn described(name: &str, input: &Value, places: &Places) -> ChatTool {
    let kind = kind(name);
    let path = [text(input, "file_path"), text(input, "notebook_path")]
        .into_iter()
        .find(|path| !path.is_empty())
        .unwrap_or_default();
    let file = places.file(path);
    let first = |keys: &[&str]| {
        keys.iter()
            .map(|key| text(input, key))
            .find(|value| !value.is_empty())
            .unwrap_or_default()
    };
    let (target, detail) = match kind {
        ToolKind::Command => {
            let command = text(input, "command");
            (
                command.lines().next().unwrap_or_default(),
                (!command.is_empty()).then(|| ToolDetail::Command {
                    command: ends_of(command, MOST_OUTPUT),
                    output: String::new(),
                    background: false,
                }),
            )
        }
        ToolKind::Read => (file.as_str(), None),
        ToolKind::Edit | ToolKind::Write if !path.is_empty() => {
            let change = if kind == ToolKind::Write {
                diff::written(text(input, "content"))
            } else if input.get("edits").is_some_and(Value::is_array) {
                list(input, "edits")
                    .map(|edit| diff::between(text(edit, "old_string"), text(edit, "new_string")))
                    .reduce(diff::Change::then)
                    .unwrap_or_else(|| diff::between("", ""))
            } else {
                diff::between(text(input, "old_string"), text(input, "new_string"))
            };
            (
                file.as_str(),
                Some(change.detail(start_of(path, MOST_TARGET * 4).to_owned())),
            )
        }
        ToolKind::Search => (first(&["pattern", "query", "path"]), None),
        ToolKind::Web => (first(&["url", "query"]), None),
        ToolKind::Agent => (
            text(input, "description"),
            Some(ToolDetail::Agent {
                name: start_of(text(input, "subagent_type"), MOST_TARGET).to_owned(),
                steps: 0,
                doing: None,
                said: String::new(),
            }),
        ),
        ToolKind::Skill => (text(input, "skill"), None),
        ToolKind::Question => {
            let questions = questions(input);
            (
                list(input, "questions")
                    .next()
                    .map_or_default(|question| text(question, "question")),
                (!questions.is_empty()).then_some(ToolDetail::Question {
                    questions,
                    answers: Vec::new(),
                }),
            )
        }
        // Always emit `ToolDetail::Plan`, even when empty: Claude Code also uses `ExitPlanMode`
        // to exit plan mode without a plan.
        ToolKind::Plan => (
            "",
            Some(ToolDetail::Plan {
                text: start_of(text(input, "plan"), MOST_TEXT).to_owned(),
            }),
        ),
        ToolKind::Edit | ToolKind::Write | ToolKind::Other => ("", None),
    };
    ChatTool {
        name: start_of(name, MOST_TARGET).to_owned(),
        kind,
        state: ToolState::Preparing,
        target: start_of(target, MOST_TARGET).to_owned(),
        detail,
        ms: None,
    }
}

fn questions(input: &Value) -> Vec<ChatQuestion> {
    list(input, "questions")
        .take(MOST_QUESTIONS)
        .map(|each| {
            question(
                each,
                each.get("multiSelect").and_then(Value::as_bool) == Some(true),
            )
        })
        .collect()
}

fn always(suggestion: &Value, places: &Places) -> Option<AskAlways> {
    match text(suggestion, "type") {
        "addRules" if text(suggestion, "behavior") == "allow" => {
            let rule = list(suggestion, "rules").next()?;
            let (tool, content) = (text(rule, "toolName"), text(rule, "ruleContent"));
            let shown = match kind(tool) {
                ToolKind::Read | ToolKind::Edit | ToolKind::Write | ToolKind::Search => {
                    places.rule(content)
                }
                _ => content.to_owned(),
            };
            Some(AskAlways::Rule {
                tool: start_of(tool, MOST_TARGET).to_owned(),
                text: start_of(&shown, MOST_TARGET).to_owned(),
            })
        }
        "setMode" => Some(AskAlways::Mode {
            mode: mode(text(suggestion, "mode"))?,
        }),
        "addDirectories" => Some(AskAlways::Folder {
            name: start_of(
                &places.file(list(suggestion, "directories").next()?.as_str()?),
                MOST_TARGET,
            )
            .to_owned(),
        }),
        _ => None,
    }
}

fn finish(tool: &mut ChatTool, output: &str) {
    match &mut tool.detail {
        Some(ToolDetail::Command {
            output: shown,
            background: false,
            ..
        }) => *shown = ends_of(output, MOST_OUTPUT),
        Some(ToolDetail::Agent { said, .. }) if said.is_empty() => {
            start_of(output, MOST_SAID).clone_into(said);
        }
        None if matches!(
            tool.kind,
            ToolKind::Search | ToolKind::Web | ToolKind::Other
        ) && !output.is_empty() =>
        {
            tool.detail = Some(ToolDetail::Output {
                text: ends_of(output, MOST_OUTPUT),
            });
        }
        _ => {}
    }
}

fn fail(tool: &mut ChatTool, error: &str) {
    match &mut tool.detail {
        Some(ToolDetail::Command { output, .. }) => *output = ends_of(error, MOST_OUTPUT),
        _ if !error.is_empty() => {
            tool.detail = Some(ToolDetail::Output {
                text: ends_of(error, MOST_OUTPUT),
            });
        }
        _ => {}
    }
}
