use std::collections::{HashMap, HashSet};

use serde_json::Value;
use uuid::Uuid;

use crate::terminal::{ProgramState, ProgramStatus};

const MOST_WAITING: usize = 20;
const MOST_UNCHECKED: usize = 64;
pub(super) const CHECK_PATIENCE: std::time::Duration = std::time::Duration::from_mins(10);
const MOST_TEXT: usize = 500;
const MOST_NAME: usize = 80;
const MOST_FOLDER: usize = 200;
pub(super) const SPACING: std::time::Duration = std::time::Duration::from_secs(10);
const JUDGMENT: &str = "Use your own judgment and your current work to decide if and how to act on";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Sender {
    Person(String),
    Agent {
        person: String,
        agent: String,
        terminal: Option<Place>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Place {
    pub name: String,
    pub folder: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Mention {
    pub project: String,
    pub project_id: Uuid,
    pub from: Sender,
    pub text: String,
    pub position: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Unchecked {
    pub project: Uuid,
    pub author: String,
    pub terminal: Uuid,
    pub mention: Mention,
    pub since: tokio::time::Instant,
}

#[derive(Default)]
pub(super) struct Mentions {
    seen: HashMap<String, u64>,
    pub waiting: HashMap<Uuid, Vec<Mention>>,
    pub told: HashMap<Uuid, tokio::time::Instant>,
    pub telling: HashSet<Uuid>,
    pub unchecked: Vec<Unchecked>,
}

impl Mentions {
    pub(super) fn new_messages(&mut self, project: &str, messages: &[Value]) -> Vec<Value> {
        let latest = messages
            .iter()
            .filter_map(|message| message["position"].as_u64())
            .max()
            .unwrap_or_default();
        let Some(seen) = self.seen.insert(project.to_owned(), latest) else {
            return Vec::new();
        };
        if latest < seen {
            self.seen.insert(project.to_owned(), seen);
        }
        messages
            .iter()
            .filter(|message| {
                message["deleted"] != true
                    && message["position"]
                        .as_u64()
                        .is_some_and(|position| position > seen)
            })
            .cloned()
            .collect()
    }

    pub(super) fn keep(&mut self, terminal: Uuid, mention: Mention) {
        let waiting = self.waiting.entry(terminal).or_default();
        if waiting
            .iter()
            .any(|kept| kept.position == mention.position && kept.project_id == mention.project_id)
        {
            return;
        }
        waiting.push(mention);
        let extra = waiting.len().saturating_sub(MOST_WAITING);
        waiting.drain(..extra);
    }

    pub(super) fn forget_deleted(&mut self, project: Uuid, positions: &HashSet<u64>) {
        let deleted = |mention: &Mention| {
            mention.project_id == project && positions.contains(&mention.position)
        };
        for waiting in self.waiting.values_mut() {
            waiting.retain(|mention| !deleted(mention));
        }
        self.waiting.retain(|_, waiting| !waiting.is_empty());
        self.unchecked
            .retain(|unchecked| !deleted(&unchecked.mention));
    }

    pub(super) fn wait_for_check(&mut self, unchecked: Unchecked) {
        self.unchecked.push(unchecked);
        let extra = self.unchecked.len().saturating_sub(MOST_UNCHECKED);
        self.unchecked.drain(..extra);
    }

    pub(super) fn take_unchecked(&mut self, now: tokio::time::Instant) -> Vec<Unchecked> {
        let mut unchecked = std::mem::take(&mut self.unchecked);
        unchecked.retain(|waiting| now.duration_since(waiting.since) < CHECK_PATIENCE);
        unchecked
    }

    pub(super) fn forget(&mut self, terminal: Uuid) {
        self.waiting.remove(&terminal);
        self.told.remove(&terminal);
        self.telling.remove(&terminal);
        self.unchecked
            .retain(|waiting| waiting.terminal != terminal);
    }

    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }
}

/// The apps mark a name after an at sign in any case, and end it where a word ends. A name that an
/// app marks and the runtime does not find leaves its person waiting for an agent that got nothing.
pub(super) fn named<T: Copy>(text: &str, terminals: &[(T, &str)]) -> Vec<T> {
    let mut found = Vec::new();
    for (index, _) in text.match_indices('@') {
        if text[..index].chars().next_back().is_some_and(in_a_word) {
            continue;
        }
        let rest = &text[index + 1..];
        let best = terminals
            .iter()
            .filter(|(_, name)| {
                !name.is_empty()
                    && after_name(rest, name).is_some_and(|next| !next.is_some_and(in_a_word))
            })
            .max_by_key(|(_, name)| name.len());
        if let Some((terminal, _)) = best {
            found.push(*terminal);
        }
    }
    found
}

fn in_a_word(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn after_name(text: &str, name: &str) -> Option<Option<char>> {
    let mut rest = text.chars();
    for expected in name.chars() {
        if !rest.next()?.to_lowercase().eq(expected.to_lowercase()) {
            return None;
        }
    }
    Some(rest.next())
}

pub(super) fn takes_messages(program: Option<&str>, status: Option<&ProgramStatus>) -> bool {
    program.is_some_and(crate::agents::is_agent_name)
        && status.is_some_and(|status| status.state != ProgramState::Blocked)
}

pub(super) fn mention_line(project: &str, mentions: &[Mention]) -> String {
    let project = plain(project, MOST_NAME);
    let answer = "If you answer: kodosi project post \"text\".";
    if let [mention] = mentions {
        return format!(
            "[Kodosi] Message from {} in the project \"{project}\": \"{}\" {answer} {JUDGMENT} it.",
            describe(&mention.from),
            plain(&mention.text, MOST_TEXT)
        );
    }
    let first = mentions
        .iter()
        .map(|mention| mention.position)
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
        "[Kodosi] {} messages in the project \"{project}\" name your terminal, from {}. Read them with kodosi project read --since {}. {answer} {JUDGMENT} them.",
        mentions.len(),
        listed(&authors),
        first.saturating_sub(1)
    )
}

pub(super) fn message_line(from: &Sender, text: &str) -> String {
    let answer = match from {
        Sender::Agent {
            terminal: Some(place),
            ..
        } => format!(
            " If you answer: kodosi terminal message \"{}\" \"text\".",
            plain(&place.name, MOST_NAME)
        ),
        _ => String::new(),
    };
    format!(
        "[Kodosi] Message from {}: \"{}\"{answer} {JUDGMENT} it.",
        describe(from),
        plain(text, crate::protocol::MESSAGE_MAX_BYTES)
    )
}

fn describe(from: &Sender) -> String {
    let Sender::Agent {
        person,
        agent,
        terminal,
    } = from
    else {
        return short(from);
    };
    let mut details = vec![format!("agent of {}", plain(person, MOST_NAME))];
    if let Some(place) = terminal {
        details.push(format!("terminal \"{}\"", plain(&place.name, MOST_NAME)));
        if let Some(folder) = &place.folder {
            details.push(format!("folder {}", plain(folder, MOST_FOLDER)));
        }
    }
    format!("{} ({})", plain(agent, MOST_NAME), details.join(", "))
}

fn short(from: &Sender) -> String {
    match from {
        Sender::Person(name) => plain(name, MOST_NAME),
        Sender::Agent { person, agent, .. } => format!(
            "{} (agent of {})",
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
        .filter_map(|word| crate::terminal::display_text(word, word.len()))
        .collect::<Vec<_>>()
        .join(" ");
    if words.chars().count() <= most {
        return words;
    }
    let mut clipped = words.chars().take(most).collect::<String>();
    clipped.push('…');
    clipped
}
