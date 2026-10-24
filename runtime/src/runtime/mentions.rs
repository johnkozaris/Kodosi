use std::collections::HashMap;

use serde_json::Value;
use uuid::Uuid;

use crate::terminal::{ProgramState, ProgramStatus};

const MOST_WAITING: usize = 20;
const MOST_TEXT: usize = 500;
pub(super) const SPACING: std::time::Duration = std::time::Duration::from_secs(20);

/// A room message that names a terminal of this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Mention {
    pub room: String,
    pub author: String,
    pub text: String,
    pub sequence: u64,
}

/// The room messages that name terminals of this computer, kept until their agents wait for input.
#[derive(Default)]
pub(super) struct Mentions {
    seen: HashMap<String, u64>,
    pub waiting: HashMap<Uuid, Vec<Mention>>,
    pub told: HashMap<Uuid, tokio::time::Instant>,
    pub telling: std::collections::HashSet<Uuid>,
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

/// An agent takes typed text only while it says that it waits for input.
pub(super) fn waits(status: Option<&ProgramStatus>) -> bool {
    status.is_some_and(|status| {
        matches!(
            status.state,
            ProgramState::Idle | ProgramState::Done | ProgramState::Error
        )
    })
}

/// The line that Kodosi types into the agent: who wrote, how to answer in the room, and where
/// the agent learns how rooms work. It is one line, because Enter sends it.
pub(super) fn line(room: &str, mentions: &[Mention]) -> String {
    let room = plain(room, 80);
    let about = "Answer in the room with kodosi room post if you want; kodosi room skill tells how rooms work.";
    if let [mention] = mentions {
        return format!(
            "[Kodosi room \"{room}\"] {}: {} — {about}",
            plain(&mention.author, 80),
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
        let author = plain(&mention.author, 80);
        if !authors.contains(&author) {
            authors.push(author);
        }
    }
    format!(
        "[Kodosi room \"{room}\"] {} messages name you, from {}. Read them with kodosi room read --since {}. {about}",
        mentions.len(),
        authors.join(", "),
        first.saturating_sub(1)
    )
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
