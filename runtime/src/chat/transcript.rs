//! A terminal's conversation as text for another agent, such as one that takes over its work: the
//! messages, the agent's answers and one line for each step, oldest first.

use std::fmt::Write as _;

use super::{ChatBody, ChatNote, ChatSnapshot, ChatTool, ToolDetail, ToolKind, ToolState, TurnEnd};

/// The most of a conversation that an agent reads at once: its newest part.
const MOST: usize = 64 * 1024;
/// The most of a step's subject on its line.
const MOST_TARGET: usize = 200;

/// The conversation that the chat shows, or `None` when the chat has none.
pub(crate) fn transcript(chat: &ChatSnapshot) -> Option<String> {
    let agent = chat
        .facts
        .agent
        .as_deref()
        .and_then(crate::agents::Agent::named)
        .map_or("Agent", crate::agents::Agent::label);
    let mut text = String::new();
    for item in &chat.items {
        match &item.body {
            ChatBody::Person {
                text: words,
                by,
                handoff,
                ..
            } => {
                let person = by.as_ref().map_or("User", |by| by.name.as_str());
                match handoff {
                    Some(handoff) if words.is_empty() => paragraph(
                        &mut text,
                        &format!("{person} handed over the work of {}.", handoff.name),
                    ),
                    _ => paragraph(&mut text, &format!("{person}: {words}")),
                }
            }
            ChatBody::Text { text: words, .. } => {
                paragraph(&mut text, &format!("{agent}: {words}"));
            }
            ChatBody::Tool { tool } => match &tool.detail {
                Some(ToolDetail::Plan { text: plan }) if !plan.is_empty() => {
                    paragraph(&mut text, &format!("{agent}, plan: {plan}"));
                }
                _ => step(&mut text, tool),
            },
            ChatBody::Turn { end, .. } => match end {
                TurnEnd::Done => {}
                TurnEnd::Stopped => step_words(&mut text, "(stopped)"),
                TurnEnd::Failed => step_words(&mut text, "(failed)"),
                TurnEnd::Refused => step_words(&mut text, "(not allowed)"),
            },
            ChatBody::Note { note } => match note {
                ChatNote::Compacted => step_words(&mut text, "(it compacted the conversation)"),
                ChatNote::SignedOut => step_words(&mut text, "(signed out)"),
                ChatNote::Recap { text: recap } => {
                    paragraph(&mut text, &format!("{agent}, recap: {recap}"));
                }
                ChatNote::Failed { error, .. } => {
                    step_words(&mut text, &format!("(failed: {error})"));
                }
            },
        }
    }
    if text.is_empty() {
        return None;
    }
    if text.len() <= MOST {
        return Some(text);
    }
    let start = text.ceil_char_boundary(text.len() - MOST);
    Some(format!(
        "(The earlier part is left out.)\n\n{}",
        &text[start..]
    ))
}

fn paragraph(text: &mut String, words: &str) {
    if !text.is_empty() {
        text.push_str("\n\n");
    }
    text.push_str(words.trim_end());
}

fn step_words(text: &mut String, words: &str) {
    if !text.is_empty() {
        text.push('\n');
    }
    text.push_str("  ");
    text.push_str(words);
}

fn step(text: &mut String, tool: &ChatTool) {
    let verb = match tool.kind {
        ToolKind::Command => "ran",
        ToolKind::Read => "read",
        ToolKind::Edit => "changed",
        ToolKind::Write => "wrote",
        ToolKind::Search => "searched",
        ToolKind::Web => "fetched",
        ToolKind::Agent => "helper",
        ToolKind::Skill => "skill",
        ToolKind::Question => "asked",
        ToolKind::Plan => "plan",
        ToolKind::Other => tool.name.as_str(),
    };
    let target = match &tool.detail {
        Some(ToolDetail::Command { command, .. }) => command.lines().next().unwrap_or_default(),
        _ => tool.target.as_str(),
    };
    let mut line = format!(
        "{verb} {}",
        &target[..target.floor_char_boundary(MOST_TARGET)]
    );
    if let Some(ToolDetail::Change { added, removed, .. }) = &tool.detail {
        let _ = write!(line, " (+{added} -{removed})");
    }
    match tool.state {
        ToolState::Failed => line.push_str(" (failed)"),
        ToolState::Denied => line.push_str(" (not allowed)"),
        ToolState::Stopped => line.push_str(" (stopped)"),
        ToolState::Asking => line.push_str(" (waits for an answer)"),
        ToolState::Preparing | ToolState::Running | ToolState::Done => {}
    }
    step_words(text, line.trim_end());
}
