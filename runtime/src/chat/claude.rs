//! Claude Code's link: the reports of its hooks module (`links/claude.js`) as changes of the chat.
//!
//! A person's answer goes back as the decision that Claude Code takes. Each field of a report can
//! be absent or of another type after an update of Claude Code, so a report gives what it has and
//! never stops the chat.

use std::collections::{HashMap, HashSet, VecDeque};

use serde_json::{Value, json};

use super::{
    AskAlways, AskBody, ChatAnswer, ChatAsk, ChatBody, ChatContext, ChatMode, ChatNote, ChatNow,
    ChatOption, ChatPerson, ChatQuestion, ChatState, ChatStep, ChatTool, MOST_OUTPUT, MOST_TARGET,
    MOST_TEXT, PlanChoice, ToolDetail, ToolKind, ToolState, TurnEnd,
    agent::{
        Agent, Stop, answer, file_name, list, now, person, shown_rule, stop_open, text, words,
    },
    diff,
    state::{Chat, ends_of, start_of},
};

pub(crate) const MODULE: &str = include_str!("../../links/claude.js");
pub(super) const VERSION: u64 = 1;
const REFUSED_WITH_WORDS: &str = "The user doesn't want to proceed with this tool use. The tool \
    use was rejected (eg. if it was a file edit, the new_string was NOT written to the file). To \
    tell you how to proceed, the user said:\n";
const MOST_ROWS: usize = 4096;
const MOST_CALLS: usize = 2048;
const MOST_SAID: usize = 400;

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
    sent: u64,
    /// Claude Code refused a message of the chat. After a person's stop in the terminal it takes
    /// the next message only there.
    held: bool,
    linked: bool,
    /// Claude Code's name of the conversation that the chat shows.
    session: String,
    ways: Ways,
    /// The words of a person's refusal. Claude Code gives them to the model as the result of the
    /// call and keeps no message of the person.
    said: Option<(String, Option<ChatPerson>)>,
}

/// What decides the ways on after a plan that Claude Code offers in its terminal.
#[derive(Default, Clone)]
struct Ways {
    /// The session can go on without permission checks: its command line allows that, or it had
    /// that mode.
    bypass: bool,
    bypass_off: bool,
    auto_off: bool,
    /// The mode that the session had before plan mode.
    before_plan: String,
}

impl Ways {
    /// The command line also gives the mode that the session starts in, which Claude Code reports
    /// first with a change of the mode or with a message.
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

    /// The least asking mode that the session can take, then the mode that asks before each change.
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

    /// Without a plan, Claude Code asks only whether to leave plan mode: back to auto mode when
    /// the session had it, or else to the mode that asks before each change.
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
    /// A call of the main conversation has an item.
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

/// The identifiers of the rows that the chat has, the oldest forgotten first.
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

    /// The state that the reports so far give.
    fn show(&self, chat: &mut Chat) {
        let helpers = self
            .helpers
            .values()
            .filter(|helper| helper.working && helper.call.is_some())
            .count();
        chat.change_facts(|facts| facts.helpers = u32::try_from(helpers).unwrap_or(u32::MAX));
        if !self.linked {
            chat.set_state(ChatState::Away, None);
        } else if !chat.asks().is_empty() {
            chat.set_state(ChatState::Asking, None);
        } else if self.compacting {
            chat.set_state(ChatState::Working, Some(ChatNow::Compacting));
        } else if self.turn.is_some() {
            chat.set_state(ChatState::Working, self.now.clone());
        } else {
            chat.set_state(ChatState::Idle, None);
        }
    }

    fn helper(&mut self, id: &str) -> &mut Helper {
        self.helpers.entry(id.to_owned()).or_default()
    }

    /// The permission mode that a report names, and the mode that it changed from.
    fn saw_mode(&mut self, chat: &mut Chat, event: &Value) {
        let name = text(event, "mode");
        self.ways.saw(name, text(event, "from"));
        chat.change_facts(|facts| facts.mode = mode(name).or(facts.mode));
    }

    /// A long session makes more calls than the chat keeps items. The calls of the items that
    /// left the chat, and the helpers that ended, are forgotten.
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

    /// A module that loads again in its session keeps the chat. Claude Code that starts again in
    /// the terminal has another conversation, and the chat starts again with it.
    fn hello(&mut self, chat: &mut Chat, event: &Value) -> Vec<String> {
        let session = text(event, "session");
        let closed = if self.session.is_empty() || session.is_empty() || self.session == session {
            Vec::new()
        } else {
            self.renew(chat)
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

    /// The rows from before the module loaded. Claude Code gives no kind for a row of a person's
    /// side, so a text that starts as its own records do is not shown as the person's message.
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
            if !words.is_empty() {
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
                let mut tool = described(name, input);
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
        for block in list(event, "content") {
            match (door, text(block, "type")) {
                ("prompt", "text") if helper.is_empty() => self.prompt(chat, event, block, at),
                ("command", "text") if helper.is_empty() => {
                    self.command(chat, event, text(block, "text"), at);
                }
                ("response", "text") if helper.is_empty() => {
                    self.end_writing(chat, Some(text(block, "text")));
                    if self.writing.take().is_none() {
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

    /// Only the composer of Claude Code and the chat give a person's own words. Other rows of
    /// this door are records that Claude Code makes, such as the end of a helper.
    fn prompt(&mut self, chat: &mut Chat, event: &Value, block: &Value, at: u64) {
        let origin = event.get("origin").unwrap_or(&Value::Null);
        let from_chat = text(origin, "kind") == "plugin"
            && origin.get("asUser").and_then(Value::as_bool) == Some(true);
        if from_chat || text(origin, "kind") == "composer" {
            self.person_said(chat, text(block, "text"), from_chat, at);
        }
    }

    /// A slash command that a person ran in the terminal or the chat, or what it printed.
    fn command(&mut self, chat: &mut Chat, event: &Value, row: &str, at: u64) {
        if let Some(output) = between(row, "<local-command-stdout>", "</local-command-stdout>") {
            if !output.trim().is_empty() {
                chat.push(at, answer(output.trim(), false));
            }
            return;
        }
        if let Some(error) = between(row, "<local-command-stderr>", "</local-command-stderr>") {
            chat.push(
                at,
                ChatBody::Note {
                    note: ChatNote::Failed {
                        error: start_of(error.trim(), MOST_TARGET).to_owned(),
                        text: String::new(),
                    },
                },
            );
            return;
        }
        let Some(name) = between(row, "<command-name>", "</command-name>") else {
            return;
        };
        let origin = event.get("origin").unwrap_or(&Value::Null);
        let from_chat =
            text(origin, "kind") == "plugin" && text(origin, "name") == crate::agents::PLUGIN_NAME;
        if !from_chat && text(origin, "kind") != "composer" {
            return;
        }
        let arguments = between(row, "<command-args>", "</command-args>").unwrap_or_default();
        let words = format!("{} {}", name.trim(), arguments.trim());
        self.person_said(chat, words.trim_end(), from_chat, at);
    }

    fn person_said(&mut self, chat: &mut Chat, words: &str, from_chat: bool, at: u64) {
        self.held &= from_chat;
        self.end_writing(chat, None);
        // Claude Code starts the messages of the chat in their order, so a message whose text it
        // changed is the oldest that waits.
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
        chat.push(at, person(words, by));
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

    /// The answer that the agent wrote is whole: as its row has it, or as it came.
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
        let tool = described(name, &Value::Null);
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
        let tool = described(name, input);
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

    /// The questions about a call are over when Claude Code decided the call or has its result.
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
        let asking = described(name, &input);
        let (kind, body) = match asking.detail {
            Some(ToolDetail::Question { questions, .. }) => {
                (AskKind::Question, AskBody::Question { questions })
            }
            Some(ToolDetail::Plan { text }) => (
                AskKind::Plan,
                // The ways on that Claude Code's terminal offers when the conversation goes on.
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
                    always: suggestions.first().and_then(always),
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

    /// The record of a call that ended well. The row of the result ends the call too, so this
    /// only adds what the row does not have.
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
                // The result of a command that runs on is Claude Code's notice for the model.
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

    /// After `/clear` and `/resume` Claude Code goes on with another conversation. Each other end
    /// is the end of Claude Code in this terminal.
    fn end(&mut self, chat: &mut Chat, reason: &str) -> Vec<String> {
        if matches!(reason, "clear" | "resume") {
            return self.renew(chat);
        }
        self.lost(chat)
    }

    /// Claude Code holds a message after a stop in the terminal until a person types there, and
    /// the message waits in the queue with Claude Code's reason. A command that it cannot run ends
    /// there, as in its terminal. A command that ran writes its row before it ends, except one
    /// such as `/clear` that writes it in the next conversation, so a command still handed then
    /// is done.
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
        if !drop.is_empty() {
            chat.refuse(&handed.queued, start_of(drop, MOST_TARGET).to_owned());
            self.held = true;
            return;
        }
        chat.unqueue(&handed.queued);
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
        // A module that loaded in the middle of a turn did not report the turn's start.
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
            _ => {}
        }
        self.show(chat);
        closed
    }

    /// Claude Code ended, or its module does not report.
    fn lost(&mut self, chat: &mut Chat) -> Vec<String> {
        self.linked = false;
        self.end_writing(chat, None);
        self.turn = None;
        self.now = None;
        self.compacting = false;
        self.handed.clear();
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

    fn is_linked(&self) -> bool {
        self.linked
    }

    fn retry(&mut self) {
        self.held = false;
    }

    fn has(&self, queued: &str) -> bool {
        self.handed.iter().any(|handed| handed.queued == queued)
    }

    /// The decision that Claude Code takes for an answer, or nothing when the answer does not fit
    /// the question.
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
            // Claude Code gives the model these words as the result of the call as they are, so
            // they start as Claude Code's own refusal with words does in its terminal.
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
            // Claude Code's own ways on after a plan set the mode of the session.
            (AskKind::Plan, ChatAnswer::Plan { choice }) => {
                let mode = match choice {
                    PlanChoice::Alone => "bypassPermissions",
                    // Claude Code goes back by itself to the mode that it had before the plan, and
                    // only so does it take auto mode with each step that its own switch to it takes.
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

    /// A person of the chat decided a question. The chat shows the decision at once, and Claude
    /// Code's own record of it follows.
    fn answered(&mut self, chat: &mut Chat, ask: &str, decision: &Value) -> bool {
        let said = self.said.take();
        let Some(asked) = self.asked.remove(ask) else {
            return true;
        };
        chat.close_asks(|open| open.id == ask);
        if let Some((words, by)) = said {
            chat.push(now(), person(&words, by));
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

    /// The next message of the queue, as a command for the module, when Claude Code can start it:
    /// no turn runs, no question is open and no earlier message waits in Claude Code.
    fn commands(&mut self, chat: &Chat) -> Vec<Value> {
        if !self.linked
            || self.held
            || self.turn.is_some()
            || self.compacting
            || !self.handed.is_empty()
            || !chat.asks().is_empty()
        {
            return Vec::new();
        }
        let Some(queued) = chat.queued().first() else {
            return Vec::new();
        };
        self.sent += 1;
        self.handed.push_back(Handed {
            n: self.sent,
            queued: queued.id.clone(),
            text: queued.text.clone(),
            by: queued.by.clone(),
        });
        vec![json!({"do": "prompt", "n": self.sent, "text": queued.text})]
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

/// The text between two marks that Claude Code puts around a part of a row.
fn between<'a>(row: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let after = &row[row.find(start)? + start.len()..];
    Some(&after[..after.find(end)?])
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

/// Calls that Claude Code makes for its own work: the search for a tool, and the file of a plan
/// that it then shows for approval.
fn is_hidden(name: &str, input: &Value) -> bool {
    name == "ToolSearch"
        || (matches!(kind(name), ToolKind::Write | ToolKind::Edit)
            && text(input, "file_path")
                .replace('\\', "/")
                .contains("/.claude/plans/"))
}

/// A call as the chat shows it before it ran, from the tool's name and what the agent gave it.
fn described(name: &str, input: &Value) -> ChatTool {
    let kind = kind(name);
    let path = [text(input, "file_path"), text(input, "notebook_path")]
        .into_iter()
        .find(|path| !path.is_empty())
        .unwrap_or_default();
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
        ToolKind::Read => (file_name(path), None),
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
                file_name(path),
                Some(ToolDetail::Change {
                    path: start_of(path, MOST_TARGET * 4).to_owned(),
                    added: change.added,
                    removed: change.removed,
                    lines: change.lines,
                }),
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
        // Claude Code also asks to leave plan mode when it found no plan.
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
        .take(8)
        .map(|question| ChatQuestion {
            question: start_of(text(question, "question"), MOST_TARGET * 2).to_owned(),
            header: start_of(text(question, "header"), MOST_TARGET).to_owned(),
            options: list(question, "options")
                .take(16)
                .map(|option| ChatOption {
                    label: start_of(text(option, "label"), MOST_TARGET).to_owned(),
                    description: start_of(text(option, "description"), MOST_TARGET * 2).to_owned(),
                })
                .collect(),
            multiple: question.get("multiSelect").and_then(Value::as_bool) == Some(true),
        })
        .collect()
}

/// What Claude Code offers to allow from now on, in its first suggestion.
fn always(suggestion: &Value) -> Option<AskAlways> {
    match text(suggestion, "type") {
        "addRules" if text(suggestion, "behavior") == "allow" => {
            let rule = list(suggestion, "rules").next()?;
            Some(AskAlways::Rule {
                tool: start_of(text(rule, "toolName"), MOST_TARGET).to_owned(),
                text: shown_rule(start_of(text(rule, "ruleContent"), MOST_TARGET)),
            })
        }
        "setMode" => Some(AskAlways::Mode {
            mode: mode(text(suggestion, "mode"))?,
        }),
        "addDirectories" => Some(AskAlways::Folder {
            name: start_of(
                file_name(
                    list(suggestion, "directories")
                        .next()?
                        .as_str()?
                        .trim_end_matches(['/', '\\']),
                ),
                MOST_TARGET,
            )
            .to_owned(),
        }),
        _ => None,
    }
}

/// The result of a call that ended well, where the chat shows a result for the kind of tool.
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
