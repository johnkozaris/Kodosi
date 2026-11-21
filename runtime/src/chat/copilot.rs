//! Copilot CLI's link: the events of its session, as the extension in Kodosi's plugin gives them, as
//! changes of the chat.
//!
//! The extension joins the session of the Copilot in the terminal and gives each event that the chat
//! reads with the fields that the chat reads, so what the events mean is all here. A person's answer
//! goes back as the decision that Copilot takes. A field can be absent or of another type after an
//! update of Copilot, so an event gives what it has and never stops the chat.

use std::collections::{HashMap, VecDeque};

use serde_json::{Value, json};

use super::{
    AskAlways, AskBody, ChatAnswer, ChatAsk, ChatBody, ChatContext, ChatNote, ChatNow, ChatPerson,
    ChatState, ChatTool, MOST_OUTPUT, MOST_TARGET, ToolDetail, ToolKind, ToolState, TurnEnd,
    agent::{Agent, Stop, answer, file_name, list, person, text, words},
    diff,
    state::{Chat, ends_of, start_of},
};

pub(crate) const EXTENSION: &str = include_str!("../../links/copilot.mjs");
const MOST_ITEMS: usize = 2048;

#[derive(Default)]
pub(super) struct Copilot {
    linked: bool,
    /// The work on a person's message, from the message until Copilot is idle again.
    turn: Option<Turn>,
    /// The chat's item of each message and each tool call of Copilot.
    items: HashMap<String, String>,
    asked: HashMap<String, Asked>,
    handed: VecDeque<Handed>,
    sent: u64,
    now: Option<ChatNow>,
    /// Copilot refused a message of the chat.
    held: bool,
}

#[derive(Default)]
struct Turn {
    began: u64,
    stopped: bool,
    failed: bool,
}

struct Asked {
    /// The tool call that the question is about.
    call: String,
    item: String,
    /// The commands that "always" allows for the session.
    commands: Vec<String>,
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

    fn hello(&mut self, chat: &mut Chat, event: &Value) -> Vec<String> {
        let closed = chat.close_asks(|_| true);
        *self = Self {
            linked: true,
            ..Self::default()
        };
        chat.change_facts(|facts| {
            *facts = super::ChatFacts {
                agent: Some(Self::NAME.to_owned()),
                version: words(event, "version").map(|version| start_of(&version, 32).to_owned()),
                model: words(event, "model"),
                folder: words(event, "cwd"),
                models: list(event, "models")
                    .filter_map(Value::as_str)
                    .take(32)
                    .map(str::to_owned)
                    .collect(),
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
            if text(earlier, "type") == "user.message" && self.turn.is_some() {
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
        match text(event, "type") {
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
                    self.remember(chat, id, item);
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
                        self.remember(chat, id, item);
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
            "tool.execution_complete" => self.complete(chat, data, at),
            "permission.completed" => return self.decided(chat, data),
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

    fn remember(&mut self, chat: &Chat, id: &str, item: String) {
        if self.items.len() >= MOST_ITEMS {
            let live = chat
                .items()
                .iter()
                .map(|item| item.id.as_str())
                .collect::<std::collections::HashSet<_>>();
            self.items.retain(|_, item| live.contains(item.as_str()));
        }
        self.items.insert(id.to_owned(), item);
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
                self.remember(chat, id, item);
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
                tool: Box::new(described(data)),
            },
        );
        self.now = Some(ChatNow::Tool { item: item.clone() });
        self.remember(chat, call, item);
    }

    fn complete(&mut self, chat: &mut Chat, data: &Value, at: u64) {
        let Some(item) = self.items.get(text(data, "toolCallId")).cloned() else {
            return;
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
            if data.get("success").and_then(Value::as_bool) == Some(true) {
                tool.state = ToolState::Done;
            } else if tool.state != ToolState::Denied {
                tool.state = ToolState::Failed;
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
    }

    /// Copilot asks the person, and it asks the extension at the same time.
    fn ask(&mut self, chat: &mut Chat, event: &Value) {
        let ask = text(event, "ask");
        let request = &event["request"];
        let call = text(request, "toolCallId");
        if ask.is_empty() || call.is_empty() {
            return;
        }
        let Some(item) = self.items.get(call).cloned() else {
            return;
        };
        let given = change(text(request, "diff"), text(request, "fileName"));
        chat.change_tool(&item, |tool| {
            tool.state = ToolState::Asking;
            if given.is_some() {
                tool.detail = given;
            }
        });
        let Some(tool) = chat.tool(&item).cloned() else {
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
            item: Some(item.clone()),
            body: AskBody::Permission {
                tool: Box::new(tool),
                always,
                helper: String::new(),
            },
        });
        self.asked.insert(
            ask.to_owned(),
            Asked {
                call: call.to_owned(),
                item,
                commands,
            },
        );
    }

    /// Copilot's record of a decision: the chat's own, one that a person took in the terminal, or
    /// one of the session before the link.
    fn decided(&mut self, chat: &mut Chat, data: &Value) -> Vec<String> {
        let call = text(data, "toolCallId");
        let state = match text(&data["result"], "kind") {
            "approved" => ToolState::Running,
            "cancelled" => ToolState::Stopped,
            _ => ToolState::Denied,
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
            .find(|(_, asked)| asked.call == call)
            .map(|(ask, _)| ask.clone())
        else {
            return Vec::new();
        };
        self.asked.remove(&ask);
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
        chat.close_asks(|_| true)
    }

    /// Copilot took a message of the chat, or refused it with its reason.
    fn did(&mut self, chat: &mut Chat, event: &Value) {
        let Some(error) = words(event, "error") else {
            return;
        };
        let n = event.get("n").and_then(Value::as_u64);
        let Some(index) = self.handed.iter().position(|handed| Some(handed.n) == n) else {
            return;
        };
        if let Some(handed) = self.handed.remove(index) {
            chat.refuse(&handed.queued, error);
            self.held = true;
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
            "ask" => {
                self.ask(chat, event);
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
        self.handed.iter().any(|handed| handed.queued == queued)
    }

    /// Copilot gives the words of a refusal to the agent with its own text, as for a refusal in
    /// its terminal.
    fn decision(
        &mut self,
        ask: &str,
        answer: &ChatAnswer,
        _: Option<&ChatPerson>,
    ) -> Option<Value> {
        let asked = self.asked.get(ask)?;
        Some(match answer {
            ChatAnswer::Allow => json!({"kind": "approve-once"}),
            ChatAnswer::Always if !asked.commands.is_empty() => json!({
                "kind": "approve-for-session",
                "approval": {"kind": "commands", "commandIdentifiers": asked.commands},
            }),
            ChatAnswer::Deny { text: Some(words) } if !words.trim().is_empty() => {
                json!({"kind": "reject", "feedback": words.trim()})
            }
            ChatAnswer::Deny { .. } => json!({"kind": "reject"}),
            _ => return None,
        })
    }

    fn answered(&mut self, chat: &mut Chat, ask: &str, decision: &Value) {
        let Some(asked) = self.asked.remove(ask) else {
            return;
        };
        chat.close_asks(|open| open.id == ask);
        let refused = text(decision, "kind") == "reject";
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
        self.handed.push_back(Handed {
            n: self.sent,
            queued: queued.id.clone(),
            text: queued.text.clone(),
            by: queued.by.clone(),
        });
        vec![json!({"do": "prompt", "n": self.sent, "text": queued.text})]
    }

    fn stop(&mut self, chat: &Chat) -> Stop {
        if !chat.asks().is_empty() {
            return Stop::Decisions(
                chat.asks()
                    .iter()
                    .map(|ask| (ask.id.clone(), json!({"kind": "reject"})))
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

/// The answer that the agent wrote and each call that did not end stay as far as they came.
fn stop_open(chat: &mut Chat) {
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
}

/// A tool call of Copilot as the chat shows it, by Copilot's name of the tool.
fn described(data: &Value) -> ChatTool {
    let name = text(data, "toolName");
    let arguments = &data["arguments"];
    let file = || file_name(text(arguments, "path")).to_owned();
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
        "glob" | "grep" => (
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
            Some(written(
                text(arguments, "file_text"),
                text(arguments, "path"),
            )),
        ),
        "apply_patch" => {
            let patch = arguments.as_str().unwrap_or_default();
            let kind = if patch.contains("*** Add File:") {
                ToolKind::Write
            } else {
                ToolKind::Edit
            };
            (kind, patched(patch), None)
        }
        "web_fetch" => (ToolKind::Web, text(arguments, "url").to_owned(), None),
        "web_search" => (ToolKind::Web, text(arguments, "query").to_owned(), None),
        "task" => (
            ToolKind::Agent,
            words(arguments, "description")
                .or_else(|| words(arguments, "prompt"))
                .unwrap_or_default(),
            None,
        ),
        _ => (ToolKind::Other, text(data, "toolTitle").to_owned(), None),
    };
    ChatTool {
        name: name.to_owned(),
        kind,
        state: ToolState::Running,
        target: start_of(&target, MOST_TARGET).to_owned(),
        detail,
        ms: None,
    }
}

/// The names of the files that a patch of Copilot changes.
fn patched(patch: &str) -> String {
    patch
        .lines()
        .filter_map(|line| {
            ["*** Update File: ", "*** Add File: ", "*** Delete File: "]
                .iter()
                .find_map(|start| line.strip_prefix(start))
        })
        .map(file_name)
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
    let lines = diff::hunks(&hunks);
    Some(ToolDetail::Change {
        path: Some(path)
            .filter(|path| !path.is_empty())
            .map(str::to_owned)
            .or(named)
            .unwrap_or_default(),
        added: lines.added,
        removed: lines.removed,
        lines: lines.lines,
    })
}

fn change_of(old: &str, new: &str, path: &str) -> Option<ToolDetail> {
    if old.is_empty() && new.is_empty() {
        return None;
    }
    let lines = diff::between(old, new);
    Some(ToolDetail::Change {
        path: path.to_owned(),
        added: lines.added,
        removed: lines.removed,
        lines: lines.lines,
    })
}

fn written(text: &str, path: &str) -> ToolDetail {
    let lines = diff::written(text);
    ToolDetail::Change {
        path: path.to_owned(),
        added: lines.added,
        removed: lines.removed,
        lines: lines.lines,
    }
}
