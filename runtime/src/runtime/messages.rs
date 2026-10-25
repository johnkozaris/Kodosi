use std::collections::{HashMap, HashSet};

use serde_json::Value;
use uuid::Uuid;

use crate::terminal::{ProgramState, ProgramStatus};

const MOST_WAITING: usize = 20;
const MOST_TEXT: usize = 500;
const MOST_NAME: usize = 80;
const MOST_FOLDER: usize = 200;
pub(super) const SPACING: std::time::Duration = std::time::Duration::from_secs(10);
const JUDGMENT: &str = "Use your own judgment and your current work to decide if and how to act on";

/// Who wrote a message, as the agent that gets it reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Sender {
    Person(String),
    Agent {
        person: String,
        agent: String,
        terminal: Option<Place>,
    },
}

/// The terminal that an agent writes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Place {
    pub name: String,
    pub folder: Option<String>,
    pub title: Option<String>,
}

/// A room message that names a terminal of this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Mention {
    pub room: String,
    pub from: Sender,
    pub text: String,
    pub sequence: u64,
}

/// The room messages that name terminals of this computer, kept until their agents can take them.
#[derive(Default)]
pub(super) struct Mentions {
    seen: HashMap<String, u64>,
    pub waiting: HashMap<Uuid, Vec<Mention>>,
    pub told: HashMap<Uuid, tokio::time::Instant>,
    pub telling: HashSet<Uuid>,
}

impl Mentions {
    /// The messages of a room that this computer has not looked at yet. The first look at a
    /// room only remembers where it is, so older messages never reach an agent.
    pub(super) fn new_messages(&mut self, room: &str, messages: &[Value]) -> Vec<Value> {
        let latest = messages
            .iter()
            .filter_map(|message| message["sequence"].as_u64())
            .max()
            .unwrap_or_default();
        let Some(seen) = self.seen.insert(room.to_owned(), latest) else {
            return Vec::new();
        };
        if latest < seen {
            self.seen.insert(room.to_owned(), seen);
        }
        messages
            .iter()
            .filter(|message| {
                message["sequence"]
                    .as_u64()
                    .is_some_and(|sequence| sequence > seen)
            })
            .cloned()
            .collect()
    }

    pub(super) fn keep(&mut self, terminal: Uuid, mention: Mention) {
        let waiting = self.waiting.entry(terminal).or_default();
        if waiting
            .iter()
            .any(|kept| kept.sequence == mention.sequence && kept.room == mention.room)
        {
            return;
        }
        waiting.push(mention);
        let extra = waiting.len().saturating_sub(MOST_WAITING);
        waiting.drain(..extra);
    }

    pub(super) fn forget(&mut self, terminal: Uuid) {
        self.waiting.remove(&terminal);
        self.told.remove(&terminal);
        self.telling.remove(&terminal);
    }

    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }
}

/// The terminals that a message names with `@` and the terminal's name. Where names start
/// alike, as `Build` and `Build 2`, the longest name that the text spells wins.
pub(super) fn named<T: Copy>(text: &str, terminals: &[(T, &str)]) -> Vec<T> {
    let mut found = Vec::new();
    for (index, _) in text.match_indices('@') {
        if text[..index]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
        {
            continue;
        }
        let rest = &text[index + 1..];
        let best = terminals
            .iter()
            .filter(|(_, name)| {
                !name.is_empty()
                    && rest.starts_with(*name)
                    && rest[name.len()..]
                        .chars()
                        .next()
                        .is_none_or(|next| !next.is_alphanumeric() && next != '_' && next != '-')
            })
            .max_by_key(|(_, name)| name.len());
        if let Some((terminal, _)) = best {
            found.push(*terminal);
        }
    }
    found
}

/// An agent takes a message while it reports its state, also while it works, but not while it
/// waits for a person's answer: typed text would answer that question.
pub(super) fn takes_messages(program: Option<&str>, status: Option<&ProgramStatus>) -> bool {
    program.is_some_and(crate::agents::is_agent_name)
        && status.is_some_and(|status| status.state != ProgramState::Blocked)
}

/// The line for messages of one room that name the agent's terminal. One line, because Enter
/// sends it.
pub(super) fn mention_line(room: &str, mentions: &[Mention]) -> String {
    let room = plain(room, MOST_NAME);
    let answer = "You can answer there with kodosi room post.";
    if let [mention] = mentions {
        return format!(
            "[Kodosi] {} mentioned you in the chat of the room \"{room}\": \"{}\" {answer} {JUDGMENT} it.",
            describe(&mention.from),
            plain(&mention.text, MOST_TEXT)
        );
    }
    let first = mentions
        .iter()
        .map(|mention| mention.sequence)
        .min()
        .unwrap_or_default();
    let mut authors = Vec::new();
    for mention in mentions {
        let author = short(&mention.from);
        if !authors.contains(&author) {
            authors.push(author);
        }
    }
    format!(
        "[Kodosi] {} messages in the chat of the room \"{room}\" mention you, from {}. Read them with kodosi room read --since {}. {answer} {JUDGMENT} them.",
        mentions.len(),
        listed(&authors),
        first.saturating_sub(1)
    )
}

/// The line for a message that a person or another agent sends to this agent's terminal.
pub(super) fn message_line(from: &Sender, text: &str) -> String {
    let answer = match from {
        Sender::Agent {
            terminal: Some(place),
            ..
        } => format!(
            " You can answer with kodosi terminal message \"{}\" \"your answer\".",
            plain(&place.name, MOST_NAME)
        ),
        _ => String::new(),
    };
    format!(
        "[Kodosi] {} sent you a message: \"{}\"{answer} {JUDGMENT} it.",
        describe(from),
        plain(text, crate::protocol::MESSAGE_MAX_BYTES)
    )
}

/// The sender, and for an agent the terminal that it writes from: its name, folder and title.
fn describe(from: &Sender) -> String {
    let Sender::Agent {
        person,
        agent,
        terminal,
    } = from
    else {
        return short(from);
    };
    let Some(place) = terminal else {
        return format!(
            "{}, an agent of {},",
            plain(agent, MOST_NAME),
            plain(person, MOST_NAME)
        );
    };
    let details = [
        place
            .folder
            .as_deref()
            .map(|folder| format!("folder {}", plain(folder, MOST_FOLDER))),
        place
            .title
            .as_deref()
            .map(|title| plain(title, MOST_NAME))
            .filter(|title| !title.is_empty())
            .map(|title| format!("title \"{title}\"")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    let details = if details.is_empty() {
        String::new()
    } else {
        format!(" ({})", details.join(", "))
    };
    format!(
        "{}, an agent of {} in the terminal \"{}\"{details},",
        plain(agent, MOST_NAME),
        plain(person, MOST_NAME),
        plain(&place.name, MOST_NAME)
    )
}

fn short(from: &Sender) -> String {
    match from {
        Sender::Person(name) => plain(name, MOST_NAME),
        Sender::Agent { person, agent, .. } => format!(
            "{} (an agent of {})",
            plain(agent, MOST_NAME),
            plain(person, MOST_NAME)
        ),
    }
}

fn listed(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn plain(text: &str, most: usize) -> String {
    let words = text
        .split(|character: char| character.is_whitespace() || character.is_control())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if words.chars().count() <= most {
        return words;
    }
    let mut clipped = words.chars().take(most).collect::<String>();
    clipped.push('…');
    clipped
}
