//! What a chat needs from one kind of agent.
//!
//! Each agent has its own interface for programs that work with it, and its own words for what it
//! does. An adapter turns the reports of one agent's link into changes of the chat, and a person's
//! answer, message or stop into what that agent takes.

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use super::{
    ChatAnswer, ChatBody, ChatPerson, MOST_TARGET, MOST_TEXT,
    antigravity::Antigravity,
    claude::Claude,
    codex::Codex,
    copilot::Copilot,
    state::{Chat, ends_of, start_of},
};

pub(super) trait Agent: Send {
    /// The name that the agent's link gives when it starts to report.
    fn name(&self) -> &'static str;

    /// Takes one report of the link. It gives the questions that closed with no decision from the
    /// chat: a person answered them in the terminal, or their turn ended.
    fn apply(&mut self, chat: &mut Chat, event: &Value) -> Vec<String>;

    /// The agent ended, or its link does not report.
    fn lost(&mut self, chat: &mut Chat) -> Vec<String>;

    /// The link reports again after a silence.
    fn back(&mut self, chat: &mut Chat);

    fn is_linked(&self) -> bool;

    /// A person sent a new message, so the chat tries the queue again.
    fn retry(&mut self);

    /// True for a queued message that the agent has, which a person can no longer take back.
    fn has(&self, queued: &str) -> bool;

    /// The decision that the agent takes for the answer of the person `by`, or nothing when the
    /// answer does not fit the question.
    fn decision(
        &mut self,
        ask: &str,
        answer: &ChatAnswer,
        by: Option<&ChatPerson>,
    ) -> Option<Value>;

    /// A person of the chat decided a question. The chat shows the decision at once, and the
    /// agent's own record of it follows. It gives true when the agent's link waits for the
    /// decision; a question that the adapter asks for the agent takes the decision itself.
    fn answered(&mut self, chat: &mut Chat, ask: &str, decision: &Value) -> bool;

    /// What the link does next for the chat: the next message of the queue when the agent can
    /// start it, and the calls that the adapter makes.
    fn commands(&mut self, chat: &Chat) -> Vec<Value>;

    /// How the turn stops for a person in the chat.
    fn stop(&mut self, chat: &Chat) -> Stop;
}

pub(super) enum Stop {
    /// The open questions get these decisions, which end the turn.
    Decisions(Vec<(String, Value)>),
    /// The link ends the turn and the commands of the turn.
    Command(Value),
    Nothing,
}

/// The adapter of the agent that a link names.
pub(super) fn named(name: &str) -> Option<Box<dyn Agent>> {
    match name {
        Claude::NAME => Some(Box::new(Claude::default())),
        Codex::NAME => Some(Box::new(Codex::default())),
        Copilot::NAME => Some(Box::new(Copilot::default())),
        Antigravity::NAME => Some(Box::new(Antigravity::default())),
        _ => None,
    }
}

pub(super) fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

pub(super) fn words(value: &Value, key: &str) -> Option<String> {
    Some(text(value, key))
        .filter(|words| !words.is_empty())
        .map(|words| start_of(words, MOST_TARGET).to_owned())
}

pub(super) fn list<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> + use<'a> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

/// The time of this computer, in milliseconds since 1970.
pub(super) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

pub(super) fn person(words: &str, by: Option<ChatPerson>) -> ChatBody {
    ChatBody::Person {
        text: ends_of(words, MOST_TEXT),
        by,
    }
}

pub(super) fn answer(words: &str, live: bool) -> ChatBody {
    ChatBody::Text {
        text: start_of(words, MOST_TEXT).to_owned(),
        live,
    }
}

pub(super) fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// A message that starts with the agents' own `/plan` command goes on in plan mode, with the rest
/// as the message, as the agents' terminals take it.
pub(super) fn plan_command(text: &str) -> Option<&str> {
    let rest = text.trim_start().strip_prefix("/plan")?;
    (rest.is_empty() || rest.starts_with(char::is_whitespace)).then(|| rest.trim())
}
