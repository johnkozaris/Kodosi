//! Adapter for Copilot CLI session events forwarded by Kodosi's Copilot extension.
//!
//! Fields are read defensively from untyped JSON because Copilot updates may omit or change
//! field types; unrecognized or missing fields are ignored rather than failing the chat.

use std::collections::{HashMap, VecDeque};

use serde_json::{Map, Value, json};

use super::{
    AskAlways, AskBody, ChatAnswer, ChatAsk, ChatBody, ChatContext, ChatMode, ChatNote, ChatNow,
    ChatOption, ChatPerson, ChatQuestion, ChatStep, ChatTask, ChatTool, MOST_OUTPUT, MOST_TARGET,
    MOST_TEXT, PlanChoice, TaskState, ToolDetail, ToolKind, ToolState, TurnEnd,
    agent::{
        Agent, Places, Stop, answer, list, person, plan_command, remember, show_state, stop_open,
        text, words,
    },
    diff,
    state::{Chat, ends_of, start_of},
};

pub(crate) const EXTENSION: &str = include_str!("../../links/copilot.mjs");
const MOST_HELPERS: usize = 256;
const MOST_SAID: usize = 400;
const MOST_FIELDS: usize = 8;
const MOST_CHOICES: usize = 16;

#[derive(Default)]
pub(super) struct Copilot {
    linked: bool,
    turn: Option<Turn>,
    /// Maps Copilot message and tool call IDs to chat item IDs.
    items: HashMap<String, String>,
    helpers: HashMap<String, Helper>,
    /// Subagent tool calls (not top-level chat items), kept for permission prompts.
    helper_calls: HashMap<String, HelperCall>,
    asked: HashMap<String, Asked>,
    /// `(toolCallId, requestId)` pairs buffered when `elicitation.requested` precedes `ask`.
    named: VecDeque<(String, String)>,
    handed: VecDeque<Handed>,
    sent: u64,
    now: Option<ChatNow>,
    /// Pauses queued prompt delivery after Copilot rejects a prompt.
    held: bool,
    /// Pending refusal feedback stashed in `decision` and emitted as a user message in
    /// `answered`, since Copilot does not emit a `user.message` event for rejection feedback.
    said: Option<(String, Option<ChatPerson>)>,
    /// Copilot's plan mode, which the chat shows in place of its permission mode.
    planning: bool,
    permissions: Option<ChatMode>,
}

#[derive(Default)]
struct Turn {
    began: u64,
    stopped: bool,
    failed: bool,
    /// Copilot did not take a message into this turn, so the next ones wait for its end.
    refuses_messages: bool,
}

struct Helper {
    /// Parent `task` tool call ID.
    call: String,
    working: bool,
}

struct HelperCall {
    helper: String,
    tool: ChatTool,
}

struct Asked {
    call: String,
    /// Chat item ID for `call`; `None` for subagent calls.
    item: Option<String>,
    kind: AskKind,
    /// Copilot's `requestId`, matched against completion events when answered in the terminal.
    request: Option<String>,
}

enum AskKind {
    /// `commands` lists the command identifiers approved when the user chooses "Always".
    Permission {
        commands: Vec<String>,
    },
    Form {
        fields: Vec<Field>,
    },
    Plan {
        actions: Vec<(PlanChoice, String)>,
    },
}

struct Field {
    name: String,
    shape: Shape,
}

enum Shape {
    Text,
    Number {
        integer: bool,
    },
    /// `(display_label, wire_value)` pairs.
    One(Vec<(String, String)>),
    Many(Vec<(String, String)>),
}

struct Handed {
    n: u64,
    queued: String,
    text: String,
    by: Option<ChatPerson>,
    /// Given to the turn that runs, as Enter gives a message in Copilot's terminal.
    into_turn: bool,
}

impl Copilot {
    pub(super) const NAME: &'static str = "copilot";

    fn show(&self, chat: &mut Chat) {
        show_state(chat, self.linked, self.turn.is_some(), self.now.clone());
    }

    fn hello(&mut self, chat: &mut Chat, event: &Value) -> Vec<String> {
        let closed = chat.close_asks(|_| true);
        chat.reset();
        *self = Self {
            linked: true,
            planning: text(event, "mode") == "plan",
            permissions: permissions(text(event, "permissions")),
            ..Self::default()
        };
        chat.change_facts(|facts| {
            *facts = super::ChatFacts {
                agent: Some(Self::NAME.to_owned()),
                version: words(event, "version").map(|version| start_of(&version, 32).to_owned()),
                model: words(event, "model"),
                effort: words(event, "effort"),
                mode: self.mode(),
                folder: words(event, "cwd"),
                context: context(&event["context"]),
                ..super::ChatFacts::default()
            };
        });
        closed
    }

    const fn mode(&self) -> Option<ChatMode> {
        if self.planning {
            Some(ChatMode::Plan)
        } else {
            self.permissions
        }
    }

    /// Replays chunked pre-link session history. Copilot's history omits `session.idle`
    /// markers, so turns are closed at each subsequent `user.message` and on the final chunk.
    /// The facts stay as the hello gave them, since the history's changes of them are older.
    fn history(&mut self, chat: &mut Chat, event: &Value) {
        for earlier in list(event, "events") {
            if matches!(
                text(earlier, "type"),
                "session.model_change"
                    | "session.mode_changed"
                    | "session.permissions_changed"
                    | "session.usage_info"
            ) {
                continue;
            }
            let at = earlier
                .get("at")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            if text(earlier, "type") == "user.message"
                && text(earlier, "agent").is_empty()
                && self.turn.is_some()
            {
                self.end_turn(chat, at);
            }
            self.event(chat, earlier, at);
        }
        if self.turn.is_some() && event.get("more").and_then(Value::as_bool) != Some(true) {
            let at = chat.items().last().map_or(0, |item| item.at);
            self.end_turn(chat, at);
        }
    }

    fn event(&mut self, chat: &mut Chat, event: &Value, at: u64) -> Vec<String> {
        let data = event.get("data").unwrap_or(&Value::Null);
        let helper = text(event, "agent");
        match text(event, "type") {
            "permission.completed" => return self.decided(chat, data),
            "elicitation.requested" => {
                self.named(text(data, "toolCallId"), text(data, "requestId"));
            }
            "elicitation.completed" | "exit_plan_mode.completed" => {
                return self.settled(chat, data);
            }
            _ if !helper.is_empty() => self.helper_event(chat, helper, event),
            "user.message" => {
                self.turn.get_or_insert_with(|| Turn {
                    began: at,
                    ..Turn::default()
                });
                self.said_by_person(chat, data, at);
            }
            "assistant.turn_start" => {
                self.turn.get_or_insert_with(|| Turn {
                    began: at,
                    ..Turn::default()
                });
            }
            "assistant.message_start" => {
                let id = text(data, "messageId");
                if !id.is_empty() && !self.items.contains_key(id) {
                    let item = chat.push(at, answer("", true));
                    remember(&mut self.items, chat, id, item);
                }
                self.now = Some(ChatNow::Writing);
            }
            "assistant.message_delta" => {
                let id = text(data, "messageId");
                let more = text(data, "deltaContent");
                match self.items.get(id) {
                    Some(item) => chat.write(item, more),
                    None if !id.is_empty() => {
                        let item = chat.push(at, answer(more, true));
                        remember(&mut self.items, chat, id, item);
                    }
                    None => {}
                }
                self.now = Some(ChatNow::Writing);
            }
            "assistant.message" => self.message(chat, data, at),
            "tool.execution_start" => self.tool(chat, data, at),
            "tool.shell_output" => {
                if let Some(item) = self.items.get(text(data, "toolCallId")) {
                    let more = text(data, "text");
                    chat.change_tool(item, |tool| {
                        if let Some(ToolDetail::Command { output, .. }) = &mut tool.detail {
                            *output = ends_of(&format!("{output}{more}"), MOST_OUTPUT);
                        }
                    });
                }
            }
            "tool.execution_complete" => return self.complete(chat, data, at),
            "session.usage_info" => {
                if let Some(context) = context(data) {
                    chat.change_facts(|facts| facts.context = Some(context));
                }
            }
            "session.model_change" => chat.change_facts(|facts| {
                facts.model = words(data, "newModel").or_else(|| facts.model.take());
                facts.effort = words(data, "reasoningEffort");
            }),
            "session.mode_changed" => {
                self.planning = text(data, "newMode") == "plan";
                chat.change_facts(|facts| facts.mode = self.mode());
            }
            "session.permissions_changed" => {
                if let Some(changed) = permissions(text(data, "mode")) {
                    self.permissions = Some(changed);
                    chat.change_facts(|facts| facts.mode = self.mode());
                }
            }
            "abort" => {
                if let Some(turn) = &mut self.turn {
                    turn.stopped = true;
                }
            }
            "session.error" => {
                if let Some(turn) = &mut self.turn {
                    turn.failed = true;
                }
                chat.push(
                    at,
                    ChatBody::Note {
                        note: ChatNote::Failed {
                            error: start_of(text(data, "errorType"), MOST_TARGET).to_owned(),
                            text: start_of(text(data, "message"), MOST_TARGET).to_owned(),
                        },
                    },
                );
            }
            "session.idle" if self.turn.is_some() => return self.end_turn(chat, at),
            _ => {}
        }
        Vec::new()
    }

    fn helper_event(&mut self, chat: &mut Chat, helper: &str, event: &Value) {
        let data = event.get("data").unwrap_or(&Value::Null);
        match text(event, "type") {
            "subagent.started" => {
                let call = text(data, "toolCallId");
                if call.is_empty() {
                    return;
                }
                if self.helpers.len() >= MOST_HELPERS {
                    self.helpers.retain(|_, helper| helper.working);
                }
                self.helpers.insert(
                    helper.to_owned(),
                    Helper {
                        call: call.to_owned(),
                        working: true,
                    },
                );
                let name = words(data, "agentName")
                    .or_else(|| words(data, "agentDisplayName"))
                    .unwrap_or_default();
                if let Some(item) = self.items.get(call) {
                    chat.change_tool(item, |tool| {
                        if let Some(ToolDetail::Agent { name: shown, .. }) = &mut tool.detail {
                            *shown = name;
                        }
                    });
                }
            }
            "tool.execution_start" => {
                let call = text(data, "toolCallId");
                let tool = described(data, &Places::of(chat));
                let step = ChatStep {
                    kind: tool.kind,
                    target: tool.target.clone(),
                };
                if !call.is_empty() && self.helpers.contains_key(helper) {
                    self.helper_calls.insert(
                        call.to_owned(),
                        HelperCall {
                            helper: helper.to_owned(),
                            tool,
                        },
                    );
                }
                self.change_helper(chat, helper, |steps, doing, _| {
                    *steps += 1;
                    *doing = Some(step);
                });
            }
            "assistant.message" => {
                let said = text(data, "content");
                if !said.is_empty() {
                    self.change_helper(chat, helper, |_, _, shown| {
                        start_of(said, MOST_SAID).clone_into(shown);
                    });
                }
            }
            "subagent.completed" | "subagent.failed" => {
                if let Some(ended) = self.helpers.get_mut(helper) {
                    ended.working = false;
                }
                self.helper_calls.retain(|_, call| call.helper != helper);
            }
            _ => {}
        }
    }

    fn change_helper(
        &self,
        chat: &mut Chat,
        helper: &str,
        change: impl FnOnce(&mut u32, &mut Option<ChatStep>, &mut String),
    ) {
        let Some(item) = self
            .helpers
            .get(helper)
            .and_then(|helper| self.items.get(&helper.call))
        else {
            return;
        };
        chat.change_tool(item, |tool| {
            if let Some(ToolDetail::Agent {
                steps, doing, said, ..
            }) = &mut tool.detail
            {
                change(steps, doing, said);
            }
        });
    }

    fn said_by_person(&mut self, chat: &mut Chat, data: &Value, at: u64) {
        let said = text(data, "content");
        if said.is_empty() {
            return;
        }
        let by = match self.handed.front() {
            Some(handed) if handed.text == said => self.handed.pop_front().and_then(|handed| {
                chat.unqueue(&handed.queued);
                handed.by
            }),
            _ => {
                self.held = false;
                None
            }
        };
        chat.push(at, person(said, by));
    }

    /// Finalizes an assistant message, removing the empty placeholder of a message that only calls
    /// tools.
    fn message(&mut self, chat: &mut Chat, data: &Value, at: u64) {
        let id = text(data, "messageId");
        let said = text(data, "content");
        match (self.items.get(id).cloned(), said.is_empty()) {
            (Some(item), true) => chat.remove(&item),
            (Some(item), false) => chat.put(&item, answer(said, false)),
            (None, false) => {
                let item = chat.push(at, answer(said, false));
                remember(&mut self.items, chat, id, item);
            }
            (None, true) => {}
        }
        if self.now == Some(ChatNow::Writing) {
            self.now = None;
        }
    }

    fn tool(&mut self, chat: &mut Chat, data: &Value, at: u64) {
        let call = text(data, "toolCallId");
        if call.is_empty() || self.items.contains_key(call) {
            return;
        }
        let item = chat.push(
            at,
            ChatBody::Tool {
                tool: Box::new(described(data, &Places::of(chat))),
            },
        );
        self.now = Some(ChatNow::Tool { item: item.clone() });
        remember(&mut self.items, chat, call, item);
    }

    fn complete(&mut self, chat: &mut Chat, data: &Value, at: u64) -> Vec<String> {
        let call = text(data, "toolCallId");
        let Some(item) = self.items.get(call).cloned() else {
            return Vec::new();
        };
        if matches!(&self.now, Some(ChatNow::Tool { item: shown }) if *shown == item) {
            self.now = None;
        }
        let result = &data["result"];
        let shown = Some(text(result, "detailedContent"))
            .filter(|shown| !shown.is_empty())
            .unwrap_or_else(|| text(result, "content"));
        let began = chat
            .items()
            .iter()
            .find(|each| each.id == item)
            .map(|each| each.at);
        chat.change_tool(&item, |tool| {
            tool.ms = began.map(|began| at.saturating_sub(began)).or(tool.ms);
            if tool.state != ToolState::Denied {
                tool.state = if data.get("success").and_then(Value::as_bool) == Some(true) {
                    ToolState::Done
                } else {
                    ToolState::Failed
                };
            }
            match &mut tool.detail {
                Some(ToolDetail::Command { output, .. }) if output.is_empty() => {
                    *output = ends_of(shown, MOST_OUTPUT);
                }
                None if matches!(tool.kind, ToolKind::Edit | ToolKind::Write) => {
                    tool.detail = change(shown, "");
                }
                _ => {}
            }
        });
        // Close any prompt still open for this call (it was answered in the terminal).
        let ended = self
            .asked
            .iter()
            .filter(|(_, asked)| asked.call == call)
            .map(|(ask, _)| ask.clone())
            .collect::<Vec<_>>();
        if ended.is_empty() {
            return Vec::new();
        }
        self.asked.retain(|ask, _| !ended.contains(ask));
        chat.close_asks(|open| ended.contains(&open.id))
    }

    /// Finds the latest open tool call of `name` with no active prompt. Form and plan prompts
    /// omit `toolCallId`, and Copilot only opens one prompt of each kind at a time.
    fn waiting(&self, chat: &Chat, name: &str) -> Option<(String, String)> {
        let item = chat
            .items()
            .iter()
            .rev()
            .find_map(|item| match &item.body {
                ChatBody::Tool { tool } if tool.name == name && tool.state.is_open() => {
                    Some(&item.id)
                }
                _ => None,
            })?;
        let call = self
            .items
            .iter()
            .find(|(_, shown)| *shown == item)
            .map(|(call, _)| call.clone())?;
        (!self.asked.values().any(|asked| asked.call == call)).then(|| (call, item.clone()))
    }

    fn ask(&mut self, chat: &mut Chat, event: &Value) {
        let ask = text(event, "ask");
        let request = &event["request"];
        let call = text(request, "toolCallId");
        if ask.is_empty() || call.is_empty() {
            return;
        }
        let given = change(text(request, "diff"), text(request, "fileName"));
        let (item, tool, helper) = if let Some(item) = self.items.get(call).cloned() {
            chat.change_tool(&item, |tool| {
                tool.state = ToolState::Asking;
                if given.is_some() {
                    tool.detail.clone_from(&given);
                }
            });
            let Some(tool) = chat.tool(&item).cloned() else {
                return;
            };
            (Some(item), tool, String::new())
        } else if let Some(of_helper) = self.helper_calls.get(call) {
            let mut tool = of_helper.tool.clone();
            tool.state = ToolState::Asking;
            if given.is_some() {
                tool.detail = given;
            }
            let task = self
                .helpers
                .get(&of_helper.helper)
                .and_then(|helper| self.items.get(&helper.call))
                .and_then(|item| chat.tool(item))
                .map_or_default(|task| task.target.clone());
            (None, tool, task)
        } else {
            return;
        };
        let commands = if text(request, "kind") == "shell"
            && request
                .get("canOfferSessionApproval")
                .and_then(Value::as_bool)
                != Some(false)
        {
            list(request, "commands")
                .filter_map(|command| words(command, "identifier"))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let always = (!commands.is_empty()).then(|| AskAlways::Rule {
            tool: tool.name.clone(),
            text: commands.join(" "),
        });
        chat.ask(ChatAsk {
            id: ask.to_owned(),
            item: item.clone(),
            body: AskBody::Permission {
                tool: Box::new(tool),
                always,
                helper,
            },
        });
        self.asked.insert(
            ask.to_owned(),
            Asked {
                call: call.to_owned(),
                item,
                kind: AskKind::Permission { commands },
                request: None,
            },
        );
    }

    fn form(&mut self, chat: &mut Chat, event: &Value) {
        let ask = text(event, "ask");
        let Some((questions, fields)) = form_of(&event["form"]) else {
            return;
        };
        if ask.is_empty() {
            return;
        }
        let (call, item) = self
            .waiting(chat, "ask_user")
            .map_or((String::new(), None), |(call, item)| (call, Some(item)));
        if let Some(item) = &item {
            chat.change_tool(item, |tool| {
                tool.state = ToolState::Asking;
                tool.detail = Some(ToolDetail::Question {
                    questions: questions.clone(),
                    answers: Vec::new(),
                });
            });
        }
        let request = self
            .named
            .iter()
            .position(|(named, _)| *named == call)
            .and_then(|index| self.named.remove(index))
            .map(|(_, request)| request);
        chat.ask(ChatAsk {
            id: ask.to_owned(),
            item: item.clone(),
            body: AskBody::Question { questions },
        });
        self.asked.insert(
            ask.to_owned(),
            Asked {
                call,
                item,
                kind: AskKind::Form { fields },
                request,
            },
        );
    }

    /// Records the `requestId` for a form call, whether `elicitation.requested` arrives before
    /// or after the extension's `ask` event.
    fn named(&mut self, call: &str, request: &str) {
        if request.is_empty() {
            return;
        }
        if let Some(asked) = self.asked.values_mut().find(|asked| {
            matches!(asked.kind, AskKind::Form { .. })
                && asked.call == call
                && asked.request.is_none()
        }) {
            asked.request = Some(request.to_owned());
            return;
        }
        if self.named.len() >= MOST_FIELDS {
            self.named.pop_front();
        }
        self.named.push_back((call.to_owned(), request.to_owned()));
    }

    fn plan(&mut self, chat: &mut Chat, event: &Value) {
        let ask = text(event, "ask");
        let plan = &event["plan"];
        let shown = Some(text(plan, "planContent"))
            .filter(|shown| !shown.trim().is_empty())
            .unwrap_or_else(|| text(plan, "summary"));
        if ask.is_empty() || shown.trim().is_empty() {
            return;
        }
        let shown = start_of(shown, MOST_TEXT).to_owned();
        let recommended = text(plan, "recommendedAction");
        let mut actions = list(plan, "actions")
            .filter_map(Value::as_str)
            .filter_map(|action| Some((plan_choice(action)?, action.to_owned())))
            .collect::<Vec<_>>();
        actions.sort_by_key(|(_, action)| *action != recommended);
        let (call, item) = self
            .waiting(chat, "exit_plan_mode")
            .map_or((String::new(), None), |(call, item)| (call, Some(item)));
        if let Some(item) = &item {
            chat.change_tool(item, |tool| {
                tool.state = ToolState::Asking;
                tool.detail = Some(ToolDetail::Plan {
                    text: shown.clone(),
                });
            });
        }
        chat.ask(ChatAsk {
            id: ask.to_owned(),
            item: item.clone(),
            body: AskBody::Plan {
                text: shown,
                choices: actions.iter().map(|(choice, _)| *choice).collect(),
            },
        });
        self.asked.insert(
            ask.to_owned(),
            Asked {
                call,
                item,
                kind: AskKind::Plan { actions },
                request: words(plan, "requestId"),
            },
        );
    }

    fn decided(&mut self, chat: &mut Chat, data: &Value) -> Vec<String> {
        let call = text(data, "toolCallId");
        let result = text(&data["result"], "kind");
        let state = if result.starts_with("approved") {
            ToolState::Running
        } else if result == "cancelled" {
            ToolState::Stopped
        } else {
            ToolState::Denied
        };
        if let Some(item) = self.items.get(call) {
            chat.change_tool(item, |tool| {
                if tool.state.is_open() {
                    tool.state = state;
                }
            });
        }
        let Some(ask) = self
            .asked
            .iter()
            .find(|(_, asked)| {
                asked.call == call && matches!(asked.kind, AskKind::Permission { .. })
            })
            .map(|(ask, _)| ask.clone())
        else {
            return Vec::new();
        };
        self.asked.remove(&ask);
        chat.close_asks(|open| open.id == ask)
    }

    fn settled(&mut self, chat: &mut Chat, data: &Value) -> Vec<String> {
        let request = text(data, "requestId");
        let Some(ask) = self
            .asked
            .iter()
            .find(|(_, asked)| asked.request.as_deref() == Some(request))
            .map(|(ask, _)| ask.clone())
        else {
            return Vec::new();
        };
        let Some(asked) = self.asked.remove(&ask) else {
            return Vec::new();
        };
        if let Some(item) = &asked.item {
            let refused = data.get("approved").and_then(Value::as_bool) == Some(false)
                || matches!(text(data, "action"), "decline" | "cancel");
            let given = match &asked.kind {
                AskKind::Form { fields } if !refused => fields
                    .iter()
                    .map(|field| field.shown(&data["content"][&field.name]))
                    .collect(),
                _ => Vec::new(),
            };
            chat.change_tool(item, |tool| {
                if tool.state == ToolState::Asking {
                    tool.state = if refused {
                        ToolState::Denied
                    } else {
                        ToolState::Running
                    };
                }
                if let Some(ToolDetail::Question { answers, .. }) = &mut tool.detail
                    && !given.is_empty()
                {
                    *answers = given;
                }
            });
        }
        chat.close_asks(|open| open.id == ask)
    }

    fn end_turn(&mut self, chat: &mut Chat, at: u64) -> Vec<String> {
        let Some(turn) = self.turn.take() else {
            return Vec::new();
        };
        self.now = None;
        stop_open(chat);
        // Clear any handed prompts whose text Copilot didn't echo back verbatim.
        for handed in std::mem::take(&mut self.handed) {
            chat.unqueue(&handed.queued);
        }
        chat.push(
            at,
            ChatBody::Turn {
                end: if turn.stopped {
                    TurnEnd::Stopped
                } else if turn.failed {
                    TurnEnd::Failed
                } else {
                    TurnEnd::Done
                },
                ms: at.saturating_sub(turn.began),
            },
        );
        self.asked.clear();
        self.named.clear();
        self.helpers.retain(|_, helper| helper.working);
        chat.close_asks(|_| true)
    }

    /// Handles acknowledgement of a handed prompt. A bare `/plan` has empty text and only
    /// switches modes without emitting `user.message`, so it is unqueued immediately here. A
    /// message that Copilot gave back before a stop waits in the queue for the next turn.
    fn did(&mut self, chat: &mut Chat, event: &Value) {
        let n = event.get("n").and_then(Value::as_u64);
        let Some(index) = self.handed.iter().position(|handed| Some(handed.n) == n) else {
            return;
        };
        match words(event, "error") {
            Some(error) => {
                if let Some(handed) = self.handed.remove(index) {
                    if handed.into_turn {
                        if let Some(turn) = &mut self.turn {
                            turn.refuses_messages = true;
                        }
                    } else {
                        chat.refuse(&handed.queued, Some(error));
                        self.held = true;
                    }
                }
            }
            None if event.get("back").and_then(Value::as_bool) == Some(true) => {
                if let Some(handed) = self.handed.remove(index) {
                    chat.take_back(&handed.queued);
                }
            }
            None if self.handed[index].text.is_empty() => {
                if let Some(handed) = self.handed.remove(index) {
                    chat.unqueue(&handed.queued);
                }
            }
            None if self.handed[index].into_turn => chat.give(&self.handed[index].queued),
            None => {}
        }
    }
}

impl Agent for Copilot {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn apply(&mut self, chat: &mut Chat, event: &Value) -> Vec<String> {
        let at = event.get("t").and_then(Value::as_u64).unwrap_or_default();
        let closed = match text(event, "k") {
            "hello" => self.hello(chat, event),
            "history" => {
                self.history(chat, event);
                Vec::new()
            }
            "event" => {
                let at = event.get("at").and_then(Value::as_u64).unwrap_or(at);
                self.event(chat, event, at)
            }
            "ask" if event.get("form").is_some() => {
                self.form(chat, event);
                Vec::new()
            }
            "ask" if event.get("plan").is_some() => {
                self.plan(chat, event);
                Vec::new()
            }
            "ask" => {
                self.ask(chat, event);
                Vec::new()
            }
            "tasks" => {
                chat.set_tasks(list(event, "rows").map(|row| ChatTask {
                    text: text(row, "title").to_owned(),
                    state: match text(row, "status") {
                        "done" | "completed" => TaskState::Done,
                        "in_progress" => TaskState::Working,
                        _ => TaskState::Pending,
                    },
                }));
                Vec::new()
            }
            "did" => {
                self.did(chat, event);
                Vec::new()
            }
            "quota" => {
                let limits = crate::agents::copilot_session(&event["quota"]);
                chat.change_facts(|facts| facts.limits = limits);
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
        self.handed.clear();
        self.helpers.clear();
        self.helper_calls.clear();
        stop_open(chat);
        self.asked.clear();
        self.named.clear();
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
        self.handed.iter().any(|handed| handed.queued == queued)
    }

    fn decision(
        &mut self,
        ask: &str,
        answer: &ChatAnswer,
        by: Option<&ChatPerson>,
    ) -> Option<Value> {
        let asked = self.asked.get(ask)?;
        let refusal = match answer {
            ChatAnswer::Deny { text: Some(words) } if !words.trim().is_empty() => {
                Some(words.trim())
            }
            _ => None,
        };
        if let (Some(words), AskKind::Permission { .. } | AskKind::Plan { .. }) =
            (refusal, &asked.kind)
        {
            self.said = Some((words.to_owned(), by.cloned()));
        }
        Some(match (&asked.kind, answer) {
            (AskKind::Permission { .. }, ChatAnswer::Allow) => json!({"kind": "approve-once"}),
            (AskKind::Permission { commands }, ChatAnswer::Always) if !commands.is_empty() => {
                json!({
                    "kind": "approve-for-session",
                    "approval": {"kind": "commands", "commandIdentifiers": commands},
                })
            }
            (AskKind::Permission { .. }, ChatAnswer::Deny { .. }) => refusal.map_or_else(
                || json!({"kind": "reject"}),
                |words| json!({"kind": "reject", "feedback": words}),
            ),
            (AskKind::Form { fields }, ChatAnswer::Answers { answers })
                if answers.len() == fields.len() =>
            {
                let content = fields
                    .iter()
                    .zip(answers)
                    .map(|(field, given)| Some((field.name.clone(), field.value(given)?)))
                    .collect::<Option<Map<_, _>>>()?;
                json!({"action": "accept", "content": content})
            }
            // Free-text "Other" answers to forms arrive as `Deny { text: Some(..) }`.
            (AskKind::Form { fields }, ChatAnswer::Deny { .. }) => refusal
                .and_then(|words| {
                    let field = fields.iter().find(|field| field.takes_words())?;
                    let value = field.value(&[words.to_owned()])?;
                    Some(json!({"action": "accept", "content": {field.name.clone(): value}}))
                })
                .unwrap_or_else(|| json!({"action": "decline"})),
            // Autopilot modes expect session auto-approval to be enabled alongside the plan
            // decision, matching Copilot's terminal behavior.
            (AskKind::Plan { actions }, ChatAnswer::Plan { choice }) => {
                let action = &actions.iter().find(|(offered, _)| offered == choice)?.1;
                let mut decision = json!({"approved": true, "selectedAction": action});
                if matches!(choice, PlanChoice::Alone | PlanChoice::Helpers) {
                    decision["autoApproveEdits"] = json!(true);
                }
                decision
            }
            (AskKind::Plan { .. }, ChatAnswer::Deny { .. }) => refusal.map_or_else(
                || json!({"approved": false}),
                |words| json!({"approved": false, "feedback": words}),
            ),
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
        let refused = text(decision, "kind") == "reject"
            || decision.get("approved").and_then(Value::as_bool) == Some(false)
            || matches!(text(decision, "action"), "decline" | "cancel");
        let given = match &asked.kind {
            AskKind::Form { fields } if !refused => fields
                .iter()
                .map(|field| field.shown(&decision["content"][&field.name]))
                .collect(),
            _ => Vec::new(),
        };
        if let Some(item) = &asked.item {
            chat.change_tool(item, |tool| {
                if tool.state == ToolState::Asking {
                    tool.state = if refused {
                        ToolState::Denied
                    } else {
                        ToolState::Running
                    };
                }
                if let Some(ToolDetail::Question { answers, .. }) = &mut tool.detail
                    && !given.is_empty()
                {
                    *answers = given;
                }
            });
        }
        self.show(chat);
        true
    }

    /// The next queued message as a command for the link. While a turn runs, a message goes into
    /// it at once, as Enter gives it in Copilot's terminal, and Copilot reads it before its next
    /// model request. A slash command waits for the turn to end, as Copilot holds the commands
    /// that cannot run in a turn.
    fn commands(&mut self, chat: &Chat) -> Vec<Value> {
        if !self.linked || self.held || !self.handed.is_empty() || !chat.asks().is_empty() {
            return Vec::new();
        }
        let Some(queued) = chat.queued().first() else {
            return Vec::new();
        };
        if let Some(turn) = &self.turn
            && (turn.stopped || turn.refuses_messages || queued.said.trim_start().starts_with('/'))
        {
            return Vec::new();
        }
        let into_turn = self.turn.is_some();
        self.sent += 1;
        let plan = plan_command(&queued.said);
        let text = plan.unwrap_or(&queued.said);
        self.handed.push_back(Handed {
            n: self.sent,
            queued: queued.id.clone(),
            text: text.to_owned(),
            by: queued.by.clone(),
            into_turn,
        });
        let mut command = json!({"do": "prompt", "n": self.sent, "text": text});
        if plan.is_some() {
            command["mode"] = json!("plan");
        }
        if into_turn {
            command["steer"] = json!(true);
        }
        vec![command]
    }

    fn stop(&mut self, chat: &Chat) -> Stop {
        if !chat.asks().is_empty() {
            return Stop::Decisions(
                chat.asks()
                    .iter()
                    .filter_map(|ask| {
                        let refusal = match &self.asked.get(&ask.id)?.kind {
                            AskKind::Permission { .. } => json!({"kind": "reject"}),
                            AskKind::Form { .. } => json!({"action": "cancel"}),
                            AskKind::Plan { .. } => json!({"approved": false}),
                        };
                        Some((ask.id.clone(), refusal))
                    })
                    .collect(),
            );
        }
        match &mut self.turn {
            Some(turn) => {
                turn.stopped = true;
                // Copilot's abort clears the messages that it has not read yet, so the link
                // takes them back first.
                let back = self
                    .handed
                    .iter()
                    .filter(|handed| handed.into_turn)
                    .map(|handed| handed.n)
                    .collect::<Vec<_>>();
                Stop::Command(json!({"do": "stop", "back": back}))
            }
            None => Stop::Nothing,
        }
    }
}

fn plan_choice(action: &str) -> Option<PlanChoice> {
    match action {
        "interactive" => Some(PlanChoice::Go),
        "autopilot" => Some(PlanChoice::Alone),
        "autopilot_fleet" => Some(PlanChoice::Helpers),
        "exit_only" => Some(PlanChoice::Leave),
        _ => None,
    }
}

fn permissions(mode: &str) -> Option<ChatMode> {
    Some(match mode {
        "manual" => ChatMode::Ask,
        "assisted" => ChatMode::Auto,
        "allow-all" => ChatMode::Unrestricted,
        _ => return None,
    })
}

fn context(usage: &Value) -> Option<ChatContext> {
    Some(ChatContext {
        tokens: usage.get("currentTokens")?.as_u64()?,
        window: usage.get("tokenLimit")?.as_u64()?,
    })
}

fn described(data: &Value, places: &Places) -> ChatTool {
    let name = text(data, "toolName");
    let arguments = &data["arguments"];
    let file = || places.file(text(arguments, "path"));
    let (kind, target, detail) = match name {
        "bash" | "powershell" => {
            let command = text(arguments, "command");
            (
                ToolKind::Command,
                command.to_owned(),
                Some(ToolDetail::Command {
                    command: start_of(command, MOST_OUTPUT).to_owned(),
                    output: String::new(),
                    background: false,
                }),
            )
        }
        "view" => (ToolKind::Read, file(), None),
        "glob" | "grep" | "rg" => (
            ToolKind::Search,
            text(arguments, "pattern").to_owned(),
            None,
        ),
        "edit" => (
            ToolKind::Edit,
            file(),
            change_of(
                text(arguments, "old_str"),
                text(arguments, "new_str"),
                text(arguments, "path"),
            ),
        ),
        "create" => (
            ToolKind::Write,
            file(),
            Some(
                diff::written(text(arguments, "file_text"))
                    .detail(text(arguments, "path").to_owned()),
            ),
        ),
        "apply_patch" => {
            let patch = arguments.as_str().unwrap_or_default();
            let kind = if patch.contains("*** Add File:") {
                ToolKind::Write
            } else {
                ToolKind::Edit
            };
            (kind, patched(patch, places), None)
        }
        "web_fetch" => (ToolKind::Web, text(arguments, "url").to_owned(), None),
        "web_search" => (ToolKind::Web, text(arguments, "query").to_owned(), None),
        "task" => (
            ToolKind::Agent,
            words(arguments, "description")
                .or_else(|| words(arguments, "prompt"))
                .unwrap_or_default(),
            Some(ToolDetail::Agent {
                name: start_of(text(arguments, "agent_type"), MOST_TARGET).to_owned(),
                steps: 0,
                doing: None,
                said: String::new(),
            }),
        ),
        "ask_user" => (
            ToolKind::Question,
            text(arguments, "message").to_owned(),
            form_of(arguments).map(|(questions, _)| ToolDetail::Question {
                questions,
                answers: Vec::new(),
            }),
        ),
        "exit_plan_mode" => (
            ToolKind::Plan,
            String::new(),
            Some(text(arguments, "summary"))
                .filter(|summary| !summary.trim().is_empty())
                .map(|summary| ToolDetail::Plan {
                    text: start_of(summary, MOST_TEXT).to_owned(),
                }),
        ),
        "sql" => (
            ToolKind::Other,
            text(arguments, "description").to_owned(),
            None,
        ),
        _ => (ToolKind::Other, text(data, "toolTitle").to_owned(), None),
    };
    ChatTool {
        name: start_of(name, MOST_TARGET).to_owned(),
        kind,
        state: ToolState::Running,
        target: start_of(&target, MOST_TARGET).to_owned(),
        detail,
        ms: None,
    }
}

/// Converts a Copilot form schema into chat questions. Returns `None` (leaving the form to the
/// terminal) if any field type is unsupported or if a free-text/number field appears alongside
/// other fields, since the chat UI can only collect free-text input for a single field.
fn form_of(form: &Value) -> Option<(Vec<ChatQuestion>, Vec<Field>)> {
    let message = text(form, "message");
    let properties = form.pointer("/requestedSchema/properties")?.as_object()?;
    if properties.is_empty() || properties.len() > MOST_FIELDS {
        return None;
    }
    let mut questions = Vec::new();
    let mut fields = Vec::new();
    for (index, (name, schema)) in properties.iter().enumerate() {
        let shape = shape_of(schema)?;
        let title = text(schema, "title");
        let about = Some(text(schema, "description"))
            .filter(|about| !about.is_empty())
            .unwrap_or(title);
        let question = if index == 0 && !message.is_empty() {
            message
        } else if !about.is_empty() {
            about
        } else {
            name.as_str()
        };
        let options = match &shape {
            Shape::One(choices) | Shape::Many(choices) => choices
                .iter()
                .map(|(label, _)| ChatOption {
                    label: label.clone(),
                    description: String::new(),
                })
                .collect(),
            Shape::Text | Shape::Number { .. } => Vec::new(),
        };
        questions.push(ChatQuestion {
            question: start_of(question, MOST_TARGET * 2).to_owned(),
            header: start_of(if title == question { "" } else { title }, MOST_TARGET).to_owned(),
            multiple: matches!(shape, Shape::Many(_)),
            options,
        });
        fields.push(Field {
            name: name.clone(),
            shape,
        });
    }
    let words = fields
        .iter()
        .any(|field| matches!(field.shape, Shape::Text | Shape::Number { .. }));
    (!words || fields.len() == 1).then_some((questions, fields))
}

fn shape_of(schema: &Value) -> Option<Shape> {
    match text(schema, "type") {
        "string" => Some(schema.get("enum").and_then(Value::as_array).map_or_else(
            || {
                if schema.get("oneOf").is_some() {
                    Shape::One(titled(list(schema, "oneOf")))
                } else {
                    Shape::Text
                }
            },
            |values| {
                let names = schema.get("enumNames").and_then(Value::as_array);
                Shape::One(
                    values
                        .iter()
                        .enumerate()
                        .filter_map(|(index, value)| {
                            let value = value.as_str()?;
                            let label = names
                                .and_then(|names| names.get(index))
                                .and_then(Value::as_str)
                                .unwrap_or(value);
                            Some(choice(label, value))
                        })
                        .take(MOST_CHOICES)
                        .collect(),
                )
            },
        )),
        "array" => {
            let items = &schema["items"];
            let choices = items.get("enum").and_then(Value::as_array).map_or_else(
                || titled(list(items, "anyOf")),
                |values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(|value| choice(value, value))
                        .take(MOST_CHOICES)
                        .collect()
                },
            );
            (!choices.is_empty()).then_some(Shape::Many(choices))
        }
        "number" => Some(Shape::Number { integer: false }),
        "integer" => Some(Shape::Number { integer: true }),
        _ => None,
    }
}

fn titled<'a>(choices: impl Iterator<Item = &'a Value>) -> Vec<(String, String)> {
    choices
        .filter_map(|each| {
            let value = each.get("const")?.as_str()?;
            let label = Some(text(each, "title"))
                .filter(|title| !title.is_empty())
                .unwrap_or(value);
            Some(choice(label, value))
        })
        .take(MOST_CHOICES)
        .collect()
}

fn choice(label: &str, value: &str) -> (String, String) {
    (start_of(label, MOST_TARGET).to_owned(), value.to_owned())
}

impl Field {
    fn value(&self, given: &[String]) -> Option<Value> {
        let given = given
            .iter()
            .map(|part| part.trim())
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        let first = *given.first()?;
        Some(match &self.shape {
            Shape::Text => Value::String(given.join(", ")),
            Shape::Number { integer: true } => json!(first.parse::<i64>().ok()?),
            Shape::Number { integer: false } => json!(first.parse::<f64>().ok()?),
            Shape::One(choices) => Value::String(chosen(choices, first)),
            Shape::Many(choices) => given
                .iter()
                .map(|label| Value::String(chosen(choices, label)))
                .collect(),
        })
    }

    fn takes_words(&self) -> bool {
        matches!(
            self.shape,
            Shape::Text | Shape::Number { .. } | Shape::One(_)
        )
    }

    fn shown(&self, value: &Value) -> String {
        let label = |value: &str| match &self.shape {
            Shape::One(choices) | Shape::Many(choices) => choices
                .iter()
                .find(|(_, of)| of == value)
                .map_or(value, |(label, _)| label.as_str())
                .to_owned(),
            Shape::Text | Shape::Number { .. } => value.to_owned(),
        };
        match value {
            Value::String(value) => label(value),
            Value::Array(values) => values
                .iter()
                .filter_map(Value::as_str)
                .map(label)
                .collect::<Vec<_>>()
                .join(", "),
            Value::Number(number) => number.to_string(),
            _ => String::new(),
        }
    }
}

fn chosen(choices: &[(String, String)], label: &str) -> String {
    choices
        .iter()
        .find(|(shown, _)| shown == label)
        .map_or(label, |(_, value)| value.as_str())
        .to_owned()
}

fn patched(patch: &str, places: &Places) -> String {
    patch
        .lines()
        .filter_map(|line| {
            ["*** Update File: ", "*** Add File: ", "*** Delete File: "]
                .iter()
                .find_map(|start| line.strip_prefix(start))
        })
        .map(|path| places.file(path))
        .collect::<Vec<_>>()
        .join(", ")
}

fn change(given: &str, path: &str) -> Option<ToolDetail> {
    let mut named = None;
    let mut hunks = String::new();
    let mut in_hunk = false;
    for line in given.lines() {
        if line.starts_with("diff --git ") {
            in_hunk = false;
        } else if line.starts_with("@@") {
            in_hunk = true;
        } else if in_hunk {
            hunks.push_str(line);
            hunks.push('\n');
        } else if let Some(file) = line.strip_prefix("+++ b") {
            named.get_or_insert_with(|| file.to_owned());
        }
    }
    if hunks.is_empty() {
        return None;
    }
    let path = Some(path)
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .or(named)
        .unwrap_or_default();
    Some(diff::hunks(&hunks).detail(path))
}

fn change_of(old: &str, new: &str, path: &str) -> Option<ToolDetail> {
    if old.is_empty() && new.is_empty() {
        return None;
    }
    Some(diff::between(old, new).detail(path.to_owned()))
}
