//! How a chat tells its agent about the agents of other terminals that a person names with `@` or
//! hands the work of to it, and about the pictures of an agent that takes them only as files, and
//! how the chat shows such a message again: the person's words apart from the lines that Kodosi
//! added for the agent.

use std::sync::Arc;

use super::{ChatMention, ChatPeer, ChatPicture};
use crate::agents::Agent;

const NAMED: &str = "[Kodosi] @";
const DETAILS: &str = " in a Kodosi terminal. Details: kodosi terminal info \"";
const HANDED: &str = "[Kodosi] Hand-off: continue the work of @";
const READ_IT: &str = " in a Kodosi terminal. Read its conversation: kodosi terminal read \"";
const READ_END: &str = "\" --conversation";
const PICTURE: &str = "[Kodosi] The person sent a picture. Read this file to see it: ";

/// The agents that the agents of a computer's terminals can reach.
pub(crate) type Peers = Arc<Vec<Peer>>;

/// An agent of another terminal that this terminal's agent can reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Peer {
    pub(crate) shown: ChatPeer,
    /// The terminal's name as the lines for the agent spell it.
    pub(crate) name: String,
    pub(crate) agent: Agent,
}

impl Peer {
    fn mention(&self) -> ChatMention {
        ChatMention {
            name: self.name.clone(),
            agent: self.agent.name().to_owned(),
        }
    }
}

/// A message of a person for the agent: what the chat shows of it, and the whole text that the
/// agent gets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Message {
    pub(super) words: String,
    pub(super) mentions: Vec<ChatMention>,
    pub(super) handoff: Option<ChatMention>,
    pub(super) pictures: Vec<ChatPicture>,
    pub(super) said: String,
}

impl From<String> for Message {
    fn from(words: String) -> Self {
        Self {
            said: words.clone(),
            words,
            mentions: Vec::new(),
            handoff: None,
            pictures: Vec::new(),
        }
    }
}

impl Message {
    /// The person's words, then a line for each peer that they name. A command for the agent keeps
    /// its words alone, because the agent reads what follows its name as the command's arguments.
    pub(super) fn naming(words: String, peers: &[Peer]) -> Self {
        let named = named(&words, peers);
        if named.is_empty() || words.starts_with('/') {
            return words.into();
        }
        let lines = named
            .iter()
            .map(|peer| {
                format!(
                    "{NAMED}{name} is {agent}{DETAILS}{name}\"",
                    name = peer.name,
                    agent = peer.agent.label()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        Self {
            said: format!("{words}\n\n{lines}"),
            mentions: named.iter().map(|peer| peer.mention()).collect(),
            words,
            handoff: None,
            pictures: Vec::new(),
        }
    }

    /// The message that hands this agent the work of `peer`.
    pub(super) fn handing(peer: &Peer) -> Self {
        Self {
            words: String::new(),
            mentions: Vec::new(),
            handoff: Some(peer.mention()),
            pictures: Vec::new(),
            said: format!(
                "{HANDED}{name}, {agent}{READ_IT}{name}{READ_END}",
                name = peer.name,
                agent = peer.agent.label()
            ),
        }
    }

    /// A message as the agent got it, apart: the person's words, and the agents and the pictures
    /// that Kodosi's lines after them name.
    pub(super) fn heard(said: &str) -> Self {
        let mut words = said.trim_end();
        let mut mentions = Vec::new();
        let mut handoff = None;
        let mut pictures = Vec::new();
        loop {
            let (before, line) = words.rsplit_once('\n').unwrap_or(("", words));
            if let Some(mention) = mention_in(line) {
                mentions.push(mention);
            } else if let Some(handed) = handoff_in(line).filter(|_| handoff.is_none()) {
                handoff = Some(handed);
            } else if let Some(file) = line.strip_prefix(PICTURE).filter(|file| !file.is_empty()) {
                pictures.push(picture(file));
            } else {
                break;
            }
            words = before.trim_end();
        }
        mentions.reverse();
        pictures.reverse();
        Self {
            words: words.to_owned(),
            mentions,
            handoff,
            pictures,
            said: said.to_owned(),
        }
    }
}

/// The message for an agent that sees a picture only when it reads its file: `said`, then a line
/// for each file.
pub(super) fn with_pictures(said: &str, files: &[String]) -> String {
    if files.is_empty() {
        return said.to_owned();
    }
    let lines = files
        .iter()
        .map(|file| format!("{PICTURE}{file}"))
        .collect::<Vec<_>>()
        .join("\n");
    if said.is_empty() {
        lines
    } else {
        format!("{said}\n\n{lines}")
    }
}

/// The picture of a file on the terminal's computer, by the name of the file.
pub(super) fn picture(file: &str) -> ChatPicture {
    ChatPicture {
        name: file
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or_default()
            .to_owned(),
    }
}

/// The peers that `words` name, each once, in the order of their first `@`.
fn named<'peers>(words: &str, peers: &'peers [Peer]) -> Vec<&'peers Peer> {
    let terminals = peers
        .iter()
        .map(|peer| (peer, peer.shown.name.as_str()))
        .collect::<Vec<_>>();
    let mut found = Vec::<&Peer>::new();
    for peer in crate::names::named(words, &terminals) {
        if !found.iter().any(|known| std::ptr::eq(*known, peer)) {
            found.push(peer);
        }
    }
    found
}

fn mention_in(line: &str) -> Option<ChatMention> {
    let (head, name) = line
        .strip_prefix(NAMED)?
        .strip_suffix('"')?
        .rsplit_once(DETAILS)?;
    let label = head.strip_prefix(name)?.strip_prefix(" is ")?;
    Some(ChatMention {
        name: name.to_owned(),
        agent: labeled(label),
    })
}

fn handoff_in(line: &str) -> Option<ChatMention> {
    let (head, name) = line
        .strip_prefix(HANDED)?
        .strip_suffix(READ_END)?
        .rsplit_once(READ_IT)?;
    let label = head.strip_prefix(name)?.strip_prefix(", ")?;
    Some(ChatMention {
        name: name.to_owned(),
        agent: labeled(label),
    })
}

fn labeled(label: &str) -> String {
    Agent::ALL
        .into_iter()
        .find(|agent| agent.label() == label)
        .map_or_else(String::new, |agent| agent.name().to_owned())
}
