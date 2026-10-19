use std::collections::HashSet;

use serde_json::{Value, json};
use uuid::Uuid;

use crate::agents::Agent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Event {
    Session(String),
    TextDelta {
        block: String,
        text: String,
    },
    TextDone {
        block: String,
        text: String,
    },
    ReasoningDelta {
        block: String,
        text: String,
    },
    ReasoningDone {
        block: String,
        text: String,
    },
    ToolStart {
        id: String,
        name: String,
        input: Value,
    },
    ToolEnd {
        id: String,
        failed: bool,
        output: String,
    },
    TurnEnd {
        failed: bool,
        message: Option<String>,
    },
    Reply(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Launch {
    pub arguments: Vec<String>,
    pub input: Option<String>,
}

pub(super) struct Choice<'a> {
    pub session: Option<&'a str>,
    pub resume: bool,
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
}

pub(super) enum Adapter {
    Claude(Claude),
    Codex(Codex),
}

impl Adapter {
    pub(super) fn new(agent: Agent) -> Option<Self> {
        match agent {
            Agent::Claude => Some(Self::Claude(Claude::default())),
            Agent::Codex => Some(Self::Codex(Codex::default())),
            _ => None,
        }
    }

    pub(super) const fn per_turn(&self) -> bool {
        matches!(self, Self::Codex(_))
    }

    pub(super) fn launch(&self, choice: &Choice<'_>, message: &str) -> Launch {
        match self {
            Self::Claude(_) => Claude::launch(choice),
            Self::Codex(_) => Codex::launch(choice, message),
        }
    }

    pub(super) fn message(&self, text: &str) -> Option<String> {
        match self {
            Self::Claude(_) => {
                Some(json!({"type":"user","message":{"role":"user","content":text}}).to_string())
            }
            Self::Codex(_) => None,
        }
    }

    pub(super) fn interrupt(&self) -> Option<String> {
        match self {
            Self::Claude(_) => Some(
                json!({"type":"control_request","request_id":Uuid::now_v7().to_string(),"request":{"subtype":"interrupt"}})
                    .to_string(),
            ),
            Self::Codex(_) => None,
        }
    }

    pub(super) fn parse(&mut self, line: &str) -> Vec<Event> {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        match self {
            Self::Claude(claude) => claude.parse(&value),
            Self::Codex(codex) => codex.parse(&value),
        }
    }

    pub(super) fn ended(&mut self) -> Option<Event> {
        match self {
            Self::Claude(_) => None,
            Self::Codex(codex) => codex.ended(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Text,
    Thinking,
}

struct Block {
    message: String,
    index: u64,
    id: String,
    kind: Kind,
}

#[derive(Default)]
pub(super) struct Claude {
    message: Option<String>,
    open: Vec<Block>,
    finished: u64,
}

impl Claude {
    fn launch(choice: &Choice<'_>) -> Launch {
        let mut arguments = [
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
            "--permission-mode",
            "bypassPermissions",
            "--permission-prompts",
            "host",
            "--name",
            "Dosi",
        ]
        .map(str::to_owned)
        .to_vec();
        if let Some(session) = choice.session {
            arguments.push(
                if choice.resume {
                    "--resume"
                } else {
                    "--session-id"
                }
                .to_owned(),
            );
            arguments.push(session.to_owned());
        }
        if let Some(model) = choice.model {
            arguments.extend(["--model".to_owned(), model.to_owned()]);
        }
        if let Some(effort) = choice.effort {
            arguments.extend(["--effort".to_owned(), effort.to_owned()]);
        }
        Launch {
            arguments,
            input: None,
        }
    }

    fn parse(&mut self, value: &Value) -> Vec<Event> {
        let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
        match value.get("type").and_then(Value::as_str) {
            Some("system") if value.get("subtype").and_then(Value::as_str) == Some("init") => {
                text("session_id").map(Event::Session).into_iter().collect()
            }
            Some("stream_event") => self.stream(value.get("event").unwrap_or(&Value::Null)),
            Some("assistant") => self.assistant(value.get("message").unwrap_or(&Value::Null)),
            Some("user") => results(value.pointer("/message/content")),
            Some("result") => {
                let failed = value.get("is_error").and_then(Value::as_bool) == Some(true)
                    || value
                        .get("subtype")
                        .and_then(Value::as_str)
                        .is_some_and(|subtype| subtype != "success");
                self.open.clear();
                vec![Event::TurnEnd {
                    failed,
                    message: failed.then(|| text("result")).flatten(),
                }]
            }
            Some("control_request") => vec![Event::Reply(answer(value))],
            _ => Vec::new(),
        }
    }

    fn stream(&mut self, event: &Value) -> Vec<Event> {
        let index = event
            .get("index")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        match event.get("type").and_then(Value::as_str) {
            Some("message_start") => {
                self.message = event
                    .pointer("/message/id")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                Vec::new()
            }
            Some("content_block_start") => {
                let kind = match event.pointer("/content_block/type").and_then(Value::as_str) {
                    Some("text") => Kind::Text,
                    Some("thinking") => Kind::Thinking,
                    _ => return Vec::new(),
                };
                if let Some(message) = self.message.clone() {
                    self.open.push(Block {
                        id: format!("{message}:{index}"),
                        message,
                        index,
                        kind,
                    });
                }
                Vec::new()
            }
            Some("content_block_delta") => {
                let Some(block) = self.open.iter().find(|block| {
                    Some(&block.message) == self.message.as_ref() && block.index == index
                }) else {
                    return Vec::new();
                };
                let id = block.id.clone();
                match event.pointer("/delta/type").and_then(Value::as_str) {
                    Some("text_delta") => delta(event, "/delta/text")
                        .map(|text| Event::TextDelta { block: id, text })
                        .into_iter()
                        .collect(),
                    Some("thinking_delta") => delta(event, "/delta/thinking")
                        .map(|text| Event::ReasoningDelta { block: id, text })
                        .into_iter()
                        .collect(),
                    _ => Vec::new(),
                }
            }
            _ => Vec::new(),
        }
    }

    fn assistant(&mut self, message: &Value) -> Vec<Event> {
        let id = message
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let mut events = Vec::new();
        for content in message
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let field = |key: &str| {
                content
                    .get(key)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            };
            match content.get("type").and_then(Value::as_str) {
                Some("text") => events.push(Event::TextDone {
                    block: self.finish(&id, Kind::Text),
                    text: field("text"),
                }),
                Some("thinking") => events.push(Event::ReasoningDone {
                    block: self.finish(&id, Kind::Thinking),
                    text: field("thinking"),
                }),
                Some("tool_use") => events.push(Event::ToolStart {
                    id: field("id"),
                    name: field("name"),
                    input: content.get("input").cloned().unwrap_or(Value::Null),
                }),
                _ => {}
            }
        }
        events
    }

    fn finish(&mut self, message: &str, kind: Kind) -> String {
        if let Some(position) = self
            .open
            .iter()
            .position(|block| block.message == message && block.kind == kind)
        {
            return self.open.remove(position).id;
        }
        self.finished += 1;
        format!("{message}:{}", self.finished)
    }
}

fn delta(event: &Value, pointer: &str) -> Option<String> {
    event
        .pointer(pointer)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn results(content: Option<&Value>) -> Vec<Event> {
    content
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_result"))
        .map(|block| Event::ToolEnd {
            id: block
                .get("tool_use_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            failed: block.get("is_error").and_then(Value::as_bool) == Some(true),
            output: plain(block.get("content")),
        })
        .collect()
}

fn plain(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn answer(request: &Value) -> String {
    let id = request.get("request_id").cloned().unwrap_or(Value::Null);
    let body = request.get("request").unwrap_or(&Value::Null);
    let response = if body.get("subtype").and_then(Value::as_str) == Some("can_use_tool") {
        json!({"subtype":"success","request_id":id,"response":{"behavior":"allow","updatedInput":body.get("input").cloned().unwrap_or_else(|| json!({}))}})
    } else {
        json!({"subtype":"error","request_id":id,"error":"Kodosi does not answer this request."})
    };
    json!({"type":"control_response","response":response}).to_string()
}

#[derive(Default)]
pub(super) struct Codex {
    started: HashSet<String>,
    problem: Option<String>,
    finished: bool,
}

impl Codex {
    fn launch(choice: &Choice<'_>, message: &str) -> Launch {
        let mut arguments = vec!["exec".to_owned()];
        if let Some(session) = choice.session.filter(|_| choice.resume) {
            arguments.extend(["resume".to_owned(), session.to_owned()]);
        }
        arguments.extend(
            [
                "--json",
                "--skip-git-repo-check",
                "--dangerously-bypass-approvals-and-sandbox",
            ]
            .map(str::to_owned),
        );
        if let Some(model) = choice.model {
            arguments.extend(["-m".to_owned(), model.to_owned()]);
        }
        if let Some(effort) = choice.effort {
            arguments.extend(["-c".to_owned(), format!("model_reasoning_effort={effort}")]);
        }
        arguments.push("-".to_owned());
        Launch {
            arguments,
            input: Some(message.to_owned()),
        }
    }

    fn parse(&mut self, value: &Value) -> Vec<Event> {
        let text = |pointer: &str| {
            value
                .pointer(pointer)
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        match value.get("type").and_then(Value::as_str) {
            Some("thread.started") => text("/thread_id").map(Event::Session).into_iter().collect(),
            Some("item.started") => self.item(value.get("item").unwrap_or(&Value::Null), false),
            Some("item.completed") => self.item(value.get("item").unwrap_or(&Value::Null), true),
            Some("turn.completed") => {
                self.finished = true;
                vec![Event::TurnEnd {
                    failed: false,
                    message: None,
                }]
            }
            Some("turn.failed") => {
                self.finished = true;
                vec![Event::TurnEnd {
                    failed: true,
                    message: text("/error/message").or_else(|| self.problem.take()),
                }]
            }
            Some("error") => {
                self.problem = text("/message");
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn ended(&mut self) -> Option<Event> {
        (!self.finished).then(|| Event::TurnEnd {
            failed: true,
            message: self.problem.take(),
        })
    }

    fn item(&mut self, item: &Value, completed: bool) -> Vec<Event> {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let text = |key: &str| {
            item.get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let (name, input) = match item.get("type").and_then(Value::as_str) {
            Some("agent_message") if completed => {
                return vec![Event::TextDone {
                    block: id,
                    text: text("text"),
                }];
            }
            Some("reasoning") if completed => {
                return vec![Event::ReasoningDone {
                    block: id,
                    text: text("text"),
                }];
            }
            Some("command_execution") => (
                "shell".to_owned(),
                json!({"command":super::receipt::unwrap_shell(&text("command"))}),
            ),
            Some("file_change") => ("edit".to_owned(), json!({"changes":item.get("changes")})),
            Some("web_search") => ("web_search".to_owned(), json!({"query":text("query")})),
            Some("mcp_tool_call") => (
                format!("mcp__{}__{}", text("server"), text("tool")),
                item.get("arguments").cloned().unwrap_or(Value::Null),
            ),
            Some("todo_list") => ("plan".to_owned(), Value::Null),
            Some("error") if completed => {
                self.problem = Some(text("message"));
                return Vec::new();
            }
            _ => return Vec::new(),
        };
        let mut events = Vec::new();
        if self.started.insert(id.clone()) {
            events.push(Event::ToolStart {
                id: id.clone(),
                name,
                input,
            });
        }
        if completed {
            let failed = item.get("status").and_then(Value::as_str) == Some("failed")
                || item
                    .get("exit_code")
                    .and_then(Value::as_i64)
                    .is_some_and(|code| code != 0);
            events.push(Event::ToolEnd {
                id,
                failed,
                output: text("aggregated_output"),
            });
        }
        events
    }
}
