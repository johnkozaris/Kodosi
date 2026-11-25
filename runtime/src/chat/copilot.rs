//! Copilot CLI's link: the events of its session, as the extension in Kodosi's plugin gives them, as
//! changes of the chat.
//!
//! The extension joins the session of the Copilot in the terminal and gives each event that the chat
//! reads with the fields that the chat reads, so what the events mean is all here. A person's answer
//! goes back as the decision that Copilot takes. A field can be absent or of another type after an
//! update of Copilot, so an event gives what it has and never stops the chat.

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
    /// The work on a person's message, from the message until Copilot is idle again.
    turn: Option<Turn>,
    /// The chat's item of each message and each tool call of Copilot.
    items: HashMap<String, String>,
    /// Each helper that a call of Copilot's task tool started, by the helper's identifier.
    helpers: HashMap<String, Helper>,
    /// The tool calls of the helpers, which have no items of their own.
    helper_calls: HashMap<String, HelperCall>,
    asked: HashMap<String, Asked>,
    /// The requests of forms that Copilot named before the extension asked them, by their calls.
    named: VecDeque<(String, String)>,
    handed: VecDeque<Handed>,
    sent: u64,
    now: Option<ChatNow>,
    /// Copilot refused a message of the chat.
    held: bool,
    /// The words of a person's refusal. Copilot gives them to the agent with the refusal and keeps
    /// no message of the person.
    said: Option<(String, Option<ChatPerson>)>,
}

#[derive(Default)]
struct Turn {
    began: u64,
    stopped: bool,
    failed: bool,
}

struct Helper {
    /// The call of the task tool that started the helper.
    call: String,
    working: bool,
}

struct HelperCall {
    helper: String,
    tool: ChatTool,
}

struct Asked {
    /// The tool call that the question is about.
    call: String,
    /// The chat's item of that call. A call of a helper has none.
    item: Option<String>,
    kind: AskKind,
    /// Copilot's name of the request, which its record of the answer gives.
    request: Option<String>,
}

enum AskKind {
    /// With the commands that "always" allows for the session.
    Permission {
        commands: Vec<String>,
    },
    Form {
        fields: Vec<Field>,
    },
    /// With Copilot's names of the ways on that it offers.
    Plan {
        actions: Vec<(PlanChoice, String)>,
    },
}

/// A field of a form of Copilot, which the chat asks as one question.
struct Field {
    name: String,
    shape: Shape,
}

enum Shape {
    Text,
    Number {
        integer: bool,
    },
    /// The choices by their labels, each with the value that Copilot takes.
    One(Vec<(String, String)>),
    Many(Vec<(String, String)>),
}

struct Handed {
    n: u64,
    queued: String,
    text: String,
    by: Option<ChatPerson>,
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
            ..Self::default()
        };
        chat.change_facts(|facts| {
            *facts = super::ChatFacts {
                agent: Some(Self::NAME.to_owned()),
                version: words(event, "version").map(|version| start_of(&version, 32).to_owned()),
                model: words(event, "model"),
                mode: mode(text(event, "mode")),
                folder: words(event, "cwd"),
                ..super::ChatFacts::default()
            };
        });
        closed
    }

    /// The events of the session before the link, in parts. Copilot keeps no mark of the end of
    /// a turn, so each turn ends where the next message begins, and the last with the last part.
    fn history(&mut self, chat: &mut Chat, event: &Value) {
        for earlier in list(event, "events") {
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
                if let (Some(tokens), Some(window)) = (
                    data.get("currentTokens").and_then(Value::as_u64),
                    data.get("tokenLimit").and_then(Value::as_u64),
                ) {
                    chat.change_facts(|facts| {
                        facts.context = Some(ChatContext { tokens, window });
                    });
                }
            }
            "session.model_change" => chat.change_facts(|facts| {
                facts.model = words(data, "newModel").or_else(|| facts.model.take());
                facts.effort = words(data, "reasoningEffort").or_else(|| facts.effort.take());
            }),
            "session.mode_changed" => {
                chat.change_facts(|facts| facts.mode = mode(text(data, "newMode")));
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

    /// What a helper does shows in the step of the call that started it.
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

    /// A message of a person: one that the chat gave Copilot, or one that a person typed in the
    /// terminal.
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

    /// The whole message: its text, which an empty answer that only calls tools does not keep.
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
        // The question of a call that ended was answered in the terminal.
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

    /// The call of a tool of this name that runs now and has no open question of the chat:
    /// Copilot shows one question of a kind at a time.
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

    /// Copilot asks the person for a permission, and it asks the extension at the same time.
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

    /// A form of Copilot, as its ask tool gives it: one question for each field.
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

    /// Copilot names the request of a form of a call, before or after the extension asks it.
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

    /// Copilot's plan, which waits for the person's approval.
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
        // Copilot's terminal offers the recommended way first.
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

    /// Copilot's record of a decision: the chat's own, one that a person took in the terminal, or
    /// one of the session before the link.
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

    /// Copilot's record of the answer to a form or a plan, from whichever side gave it first.
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
        // Copilot has each message that the chat gave it before the end. One that Copilot did not
        // write down in the chat's words leaves the queue here.
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

    /// Copilot took a message of the chat, or refused it with its reason. A `/plan` without words
    /// only changed the mode, and Copilot has no message of it.
    fn did(&mut self, chat: &mut Chat, event: &Value) {
        let n = event.get("n").and_then(Value::as_u64);
        let Some(index) = self.handed.iter().position(|handed| Some(handed.n) == n) else {
            return;
        };
        match words(event, "error") {
            Some(error) => {
                if let Some(handed) = self.handed.remove(index) {
                    chat.refuse(&handed.queued, error);
                    self.held = true;
                }
            }
            None if self.handed[index].text.is_empty() => {
                if let Some(handed) = self.handed.remove(index) {
                    chat.unqueue(&handed.queued);
                }
            }
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

    /// Copilot gives the words of a refusal to the agent with its own text, as for a refusal in
    /// its terminal. Words in place of an answer to a form are the person's own answer, as the
    /// terminal's "Other" takes them.
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
            (AskKind::Form { fields }, ChatAnswer::Deny { .. }) => refusal
                .and_then(|words| {
                    let field = fields.iter().find(|field| field.takes_words())?;
                    let value = field.value(&[words.to_owned()])?;
                    Some(json!({"action": "accept", "content": {field.name.clone(): value}}))
                })
                .unwrap_or_else(|| json!({"action": "decline"})),
            // Copilot's own autopilot also allows each tool for the session, which its terminal
            // asks for at once, so the extension turns that on as it gives the decision.
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

    /// The next message when Copilot can start it: no turn runs, no question is open and Copilot
    /// has no earlier message of the chat.
    fn commands(&mut self, chat: &Chat) -> Vec<Value> {
        if !self.linked
            || self.held
            || self.turn.is_some()
            || !self.handed.is_empty()
            || !chat.asks().is_empty()
        {
            return Vec::new();
        }
        let Some(queued) = chat.queued().first() else {
            return Vec::new();
        };
        self.sent += 1;
        let plan = plan_command(&queued.text);
        let text = plan.unwrap_or(&queued.text);
        self.handed.push_back(Handed {
            n: self.sent,
            queued: queued.id.clone(),
            text: text.to_owned(),
            by: queued.by.clone(),
        });
        let mut command = json!({"do": "prompt", "n": self.sent, "text": text});
        if plan.is_some() {
            command["mode"] = json!("plan");
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
        if self.turn.is_some() {
            Stop::Command(json!({"do": "stop"}))
        } else {
            Stop::Nothing
        }
    }
}

/// A way on after a plan, by Copilot's name of it.
fn plan_choice(action: &str) -> Option<PlanChoice> {
    match action {
        "interactive" => Some(PlanChoice::Go),
        "autopilot" => Some(PlanChoice::Alone),
        "autopilot_fleet" => Some(PlanChoice::Helpers),
        "exit_only" => Some(PlanChoice::Leave),
        _ => None,
    }
}

/// Copilot's way of work as a mode of the chat. Its other ways ask as its permissions say.
fn mode(name: &str) -> Option<ChatMode> {
    (name == "plan").then_some(ChatMode::Plan)
}

/// A tool call of Copilot as the chat shows it, by Copilot's name of the tool.
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

/// The questions of a form of Copilot: its message, and one question for each field. A form
/// with a field that the chat cannot ask is for the terminal alone, and so is a form with a
/// field for words beside other fields: a person's words in the chat answer one field.
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
    /// What Copilot takes for the labels that a person chose, or for the words that they wrote.
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

    /// An answer that Copilot took, in the labels that the chat shows.
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

/// A choice by its label, or a person's own words, which Copilot takes as they are.
fn chosen(choices: &[(String, String)], label: &str) -> String {
    choices
        .iter()
        .find(|(shown, _)| shown == label)
        .map_or(label, |(_, value)| value.as_str())
        .to_owned()
}

/// The files that a patch of Copilot changes.
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

/// A change as a diff of Copilot gives it: a diff of git, which names each file before its hunks.
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
