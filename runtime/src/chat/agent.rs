//! What a chat needs from one kind of agent.
//!
//! Each agent has its own interface for programs that work with it, and its own words for what it
//! does. An adapter turns the reports of one agent's link into changes of the chat, and a person's
//! answer, message or stop into what that agent takes.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::{
    ChatAnswer, ChatBody, ChatNow, ChatOption, ChatPerson, ChatQuestion, ChatState, MOST_CALLS,
    MOST_OPTIONS, MOST_TARGET, MOST_TEXT, ToolState,
    antigravity::Antigravity,
    claude::Claude,
    codex::Codex,
    copilot::Copilot,
    state::{Chat, ends_of, start_of},
};
use crate::terminal::TerminalFace;

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

    /// The conversation that the agent shows in its terminal, when only the chat knows it: Codex
    /// keeps its conversations in the server of the terminal, outside the terminal's programs.
    fn conversation(&self) -> Option<&str> {
        None
    }

    /// Where the agent works: in its terminal, which the chat follows, or in the chat itself.
    fn works_in(&self) -> TerminalFace {
        TerminalFace::Terminal
    }
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

/// A question that waits for a person comes before the agent's work.
pub(super) fn show_state(chat: &mut Chat, linked: bool, working: bool, now: Option<ChatNow>) {
    let (state, now) = if !linked {
        (ChatState::Away, None)
    } else if !chat.asks().is_empty() {
        (ChatState::Asking, None)
    } else if working {
        (ChatState::Working, now)
    } else {
        (ChatState::Idle, None)
    };
    chat.set_state(state, now);
}

/// A call whose item the chat still shows stays known, so that the news of a call that runs
/// changes its own item.
pub(super) fn remember(items: &mut HashMap<String, String>, chat: &Chat, call: &str, item: String) {
    if items.len() >= MOST_CALLS {
        let shown = chat
            .items()
            .iter()
            .map(|item| item.id.as_str())
            .collect::<HashSet<_>>();
        items.retain(|_, item| shown.contains(item.as_str()));
    }
    items.insert(call.to_owned(), item);
}

/// A question as Claude Code and Codex give it: its words, a short header, and the options with
/// their labels and descriptions.
pub(super) fn question(question: &Value, multiple: bool) -> ChatQuestion {
    ChatQuestion {
        question: start_of(text(question, "question"), MOST_TARGET * 2).to_owned(),
        header: start_of(text(question, "header"), MOST_TARGET).to_owned(),
        options: list(question, "options")
            .take(MOST_OPTIONS)
            .map(|option| ChatOption {
                label: start_of(text(option, "label"), MOST_TARGET).to_owned(),
                description: start_of(text(option, "description"), MOST_TARGET * 2).to_owned(),
            })
            .collect(),
        multiple,
    }
}

/// The items that did not end: each call that runs or waits, and the answer that the agent writes.
pub(super) fn open_items(chat: &Chat) -> Vec<String> {
    chat.items()
        .iter()
        .filter_map(|item| match &item.body {
            ChatBody::Tool { tool } if tool.state.is_open() => Some(item.id.clone()),
            ChatBody::Text { live: true, .. } => Some(item.id.clone()),
            _ => None,
        })
        .collect()
}

/// The answer that the agent wrote and each call that did not end stay as far as they came.
pub(super) fn stop_open(chat: &mut Chat) {
    for item in open_items(chat) {
        match chat.body(&item).cloned() {
            Some(ChatBody::Text { text, .. }) => chat.put(&item, answer(&text, false)),
            _ => chat.change_tool(&item, |tool| tool.state = ToolState::Stopped),
        }
    }
}

const SEPARATORS: [char; 2] = ['/', '\\'];

/// The places from which a chat names a file: the terminal's folder that the agent reported for
/// this moment, and the home folder of this computer.
pub(super) struct Places {
    folder: Option<String>,
    home: Option<String>,
}

impl Places {
    pub(super) fn of(chat: &Chat) -> Self {
        Self {
            folder: chat.facts().folder.clone(),
            home: chat.home().map(str::to_owned),
        }
    }

    /// A file in the terminal's folder or below it shows by its name, as a person expects there.
    /// A file elsewhere shows its place, with the home folder as `~`, so that a person who allows
    /// a call sees which file it is.
    pub(super) fn file(&self, path: &str) -> String {
        if path.is_empty() {
            return String::new();
        }
        let folder = self
            .folder
            .as_deref()
            .and_then(|folder| self.place(folder, None));
        let Some(file) = self.place(path, folder.clone()) else {
            return path.to_owned();
        };
        if folder.is_some_and(|folder| file.is_in(&folder)) {
            return file
                .names
                .last()
                .map_or_else(|| path.to_owned(), |name| (*name).to_owned());
        }
        let separator = if path.contains('\\') { "\\" } else { "/" };
        match self.home().filter(|home| file.is_in(home)) {
            Some(home) => std::iter::once("~")
                .chain(file.names[home.names.len()..].iter().copied())
                .collect::<Vec<_>>()
                .join(separator),
            None => file.shown(separator),
        }
    }

    /// A rule for files names the folders before its pattern as a file shows them, and the
    /// pattern as it is: `src/**` in the terminal's folder, `~/Repos/other/**` elsewhere.
    pub(super) fn rule(&self, rule: &str) -> String {
        let (folders, pattern) = match rule.find(['*', '?', '[', '{']) {
            None => (rule, ""),
            Some(glob) => {
                let Some(end) = rule[..glob].rfind(SEPARATORS) else {
                    return rule.to_owned();
                };
                (&rule[..end], &rule[end + 1..])
            }
        };
        if !is_home(folders) && rooted(folders).is_none() {
            return rule.to_owned();
        }
        let shown = self.file(folders);
        if pattern.is_empty() {
            shown
        } else if shown.ends_with(SEPARATORS) {
            format!("{shown}{pattern}")
        } else {
            format!("{shown}/{pattern}")
        }
    }

    fn home(&self) -> Option<Place<'_>> {
        let (start, rest) = rooted(self.home.as_deref()?)?;
        Some(Place::at(start).along(rest))
    }

    /// Where `path` is: an absolute path, a path from the home folder, or a path from `from`.
    fn place<'a>(&'a self, path: &'a str, from: Option<Place<'a>>) -> Option<Place<'a>> {
        if is_home(path) {
            return Some(self.home()?.along(&path[1..]));
        }
        match rooted(path) {
            Some((start, rest)) => Some(Place::at(start).along(rest)),
            None => Some(from?.along(path)),
        }
    }
}

fn is_home(path: &str) -> bool {
    path.strip_prefix('~')
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(SEPARATORS))
}

/// How an absolute path starts, and the rest: `/`, a drive such as `C:`, or the `\\` of a share.
fn rooted(path: &str) -> Option<(&str, &str)> {
    let bytes = path.as_bytes();
    if path.starts_with(r"\\") {
        Some(path.split_at(2))
    } else if path.starts_with(SEPARATORS) {
        Some(path.split_at(1))
    } else if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        Some(path.split_at(2))
    } else {
        None
    }
}

/// An absolute place: how its path starts, and its folders and name.
#[derive(Clone)]
struct Place<'a> {
    start: &'a str,
    names: Vec<&'a str>,
}

impl<'a> Place<'a> {
    const fn at(start: &'a str) -> Self {
        Self {
            start,
            names: Vec::new(),
        }
    }

    /// The place that `path` leads to from here: into a folder at each name, out of one at `..`.
    fn along(mut self, path: &'a str) -> Self {
        for name in path.split(SEPARATORS) {
            match name {
                "" | "." => {}
                ".." => {
                    self.names.pop();
                }
                name => self.names.push(name),
            }
        }
        self
    }

    /// Windows does not tell letters of another case apart in a path.
    fn is_in(&self, folder: &Place<'_>) -> bool {
        let windows = self.start != "/";
        let same = |one: &str, other: &str| {
            if windows {
                one.to_lowercase() == other.to_lowercase()
            } else {
                one == other
            }
        };
        same(self.start, folder.start)
            && self.names.len() >= folder.names.len()
            && self
                .names
                .iter()
                .zip(&folder.names)
                .all(|(name, folder)| same(name, folder))
    }

    fn shown(&self, separator: &str) -> String {
        let names = self.names.join(separator);
        if self.start.ends_with(SEPARATORS) {
            format!("{}{names}", self.start)
        } else {
            format!("{}{separator}{names}", self.start)
        }
    }
}

/// A message that starts with the agents' own `/plan` command goes on in plan mode, with the rest
/// as the message, as the agents' terminals take it.
pub(super) fn plan_command(text: &str) -> Option<&str> {
    let rest = text.trim_start().strip_prefix("/plan")?;
    (rest.is_empty() || rest.starts_with(char::is_whitespace)).then(|| rest.trim())
}
