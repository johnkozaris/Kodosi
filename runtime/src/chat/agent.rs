//! Adapter interface between a chat room and a specific coding agent's link protocol.

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
    fn name(&self) -> &'static str;

    /// Applies one link event to `chat`. Returns IDs of asks closed without a chat decision
    /// (for example, answered in the terminal or dropped when the turn ended).
    fn apply(&mut self, chat: &mut Chat, event: &Value) -> Vec<String>;

    /// Called when the agent exits or its link disconnects; returns closed ask IDs.
    fn lost(&mut self, chat: &mut Chat) -> Vec<String>;

    /// Called when the link resumes reporting after going silent.
    fn back(&mut self, chat: &mut Chat);

    fn is_linked(&self) -> bool;

    /// Clears any send hold so queued messages can be retried after a new user message.
    fn retry(&mut self);

    /// Whether the agent runs this message from the chat, which it does not for a command that
    /// only its terminal has.
    fn takes(&self, _text: &str) -> bool {
        true
    }

    /// Returns true if the queued message has already been handed to the agent and cannot be
    /// recalled.
    fn has(&self, queued: &str) -> bool;

    /// Converts a user's answer into the agent's protocol payload, or `None` if it does not
    /// match the pending ask.
    fn decision(
        &mut self,
        ask: &str,
        answer: &ChatAnswer,
        by: Option<&ChatPerson>,
    ) -> Option<Value>;

    /// Records a chat decision immediately in `chat`. Returns true if the link is waiting for
    /// `decision`, or false if the adapter synthesized the ask and handles the response itself.
    fn answered(&mut self, chat: &mut Chat, ask: &str, decision: &Value) -> bool;

    /// Returns pending adapter calls and, when the agent is ready, the next queued prompt.
    fn commands(&mut self, chat: &Chat) -> Vec<Value>;

    fn stop(&mut self, chat: &Chat) -> Stop;

    /// The command for the link that changes the agent's model, effort or fast mode by the line
    /// that a person would type, when the link runs it as the agent's own command.
    fn change(&mut self, _line: &str) -> Option<Value> {
        None
    }

    /// The command for the link that lists the agent's own settings, when the agent has them.
    fn settings(&mut self, _chat: &mut Chat) -> Option<Value> {
        None
    }

    /// The command for the link that changes one of the agent's settings to `value`, in the words
    /// of `ChatSetting::value`.
    fn set(&mut self, _chat: &mut Chat, _key: &str, _value: &str) -> Option<Value> {
        None
    }

    /// Returns the active conversation ID when only the adapter knows it, such as Codex threads
    /// (kept in an app-server outside the terminal's process tree) or ACP sessions.
    fn conversation(&self) -> Option<&str> {
        None
    }

    fn works_in(&self) -> TerminalFace {
        TerminalFace::Terminal
    }
}

pub(super) enum Stop {
    Decisions(Vec<(String, Value)>),
    Command(Value),
    Nothing,
}

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

pub(super) fn stop_open(chat: &mut Chat) {
    for item in open_items(chat) {
        match chat.body(&item).cloned() {
            Some(ChatBody::Text { text, .. }) => chat.put(&item, answer(&text, false)),
            _ => chat.change_tool(&item, |tool| tool.state = ToolState::Stopped),
        }
    }
}

const SEPARATORS: [char; 2] = ['/', '\\'];

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

    /// Formats a path for display: basename only when inside the working directory, or full path
    /// (with `~` for home) for external paths so permission prompts make external access clear.
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

    /// Formats the directory prefix of a file-glob rule via `file()`, keeping the glob suffix
    /// intact (e.g., `src/**` inside the working directory, `~/Repos/other/**` outside it).
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

pub(super) fn plan_command(text: &str) -> Option<&str> {
    let rest = text.trim_start().strip_prefix("/plan")?;
    (rest.is_empty() || rest.starts_with(char::is_whitespace)).then(|| rest.trim())
}
