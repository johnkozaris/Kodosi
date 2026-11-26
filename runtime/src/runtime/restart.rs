use std::collections::HashMap;
use std::time::Duration;

use serde_json::json;
use tokio::time::Instant;
use uuid::Uuid;

use super::{Completion, Runtime, TerminalTarget, messages};
use crate::Result;
use crate::protocol::SessionEntry;
use crate::terminal::{ProgramState, Waiting};

/// An agent that comes back after an update but never says it is ready gets the key instead.
const READY_WITHIN: Duration = Duration::from_secs(120);
/// How long a computer stays quiet before its Kodosi restarts by itself for an update, on top of
/// the time its person needs to count as away.
const QUIET_FOR: Duration = Duration::from_secs(120);

#[derive(Debug, Default)]
pub(super) struct Restart {
    /// Agents that went on after an update and continue once they say they are ready.
    continuing: HashMap<Uuid, Instant>,
    quiet_since: Option<Instant>,
    told: Option<(bool, u32)>,
}

impl Restart {
    pub(super) fn continue_when_ready(&mut self, id: Uuid, now: Instant) {
        self.continuing.insert(id, now + READY_WITHIN);
    }

    /// The apps of another account hear the state again.
    pub(super) fn retell(&mut self) {
        self.told = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Next {
    Wait,
    GoOn,
    Offer,
    Forget,
}

/// What an agent that came back after an update does at its newest state: it goes on once it
/// takes messages at rest, and offers the key when it never gets there. An agent may report work
/// while it starts, so only one that still works at the end went on by itself.
fn next(state: Option<ProgramState>, takes_messages: bool, late: bool) -> Next {
    match state {
        Some(ProgramState::Error) => Next::Forget,
        Some(ProgramState::Idle | ProgramState::Done) if takes_messages => Next::GoOn,
        Some(ProgramState::Working) if late => Next::Forget,
        _ if late => Next::Offer,
        _ => Next::Wait,
    }
}

/// A terminal that a restart costs a running program: the program is neither the shell at its
/// prompt nor an agent, which comes back.
fn stops(prompt: bool, program: Option<&str>) -> bool {
    !prompt && !program.is_some_and(crate::agents::is_agent_name)
}

/// The terminal's program is in the middle of a task: it or its helpers work, or it waits for an
/// answer.
pub(super) fn busy(entry: &SessionEntry) -> bool {
    super::works(entry) || entry.helpers > 0 || super::waits(entry) == Some(Waiting::Blocked)
}

/// Someone other than this computer's person has the terminal open, and may type in it.
fn joined(entry: &SessionEntry, me: Option<&str>) -> bool {
    entry
        .connected_users
        .iter()
        .any(|user| Some(user.as_str()) != me)
}

/// What a restart now costs: how many terminals lose a running program, and whether it cuts off
/// an agent's task or someone else in a terminal.
fn cost<'a>(entries: impl IntoIterator<Item = &'a SessionEntry>, me: Option<&str>) -> (u32, bool) {
    entries
        .into_iter()
        .fold((0u32, false), |(stopping, used), entry| {
            (
                stopping.saturating_add(u32::from(stops(entry.prompt, entry.program.as_deref()))),
                used || busy(entry) || joined(entry, me),
            )
        })
}

impl Runtime {
    pub(super) fn continue_restored(&mut self) {
        let now = Instant::now();
        let (mut going_on, mut offered) = (Vec::new(), Vec::new());
        let locals = &self.local;
        self.restart.continuing.retain(|id, until| {
            let Some(local) = locals.get(id) else {
                return false;
            };
            let status = local.entry.program_status.as_ref();
            let takes = messages::takes_messages(local.entry.program.as_deref(), status);
            match next(status.map(|status| status.state), takes, now >= *until) {
                Next::Wait => true,
                Next::GoOn => {
                    going_on.push(*id);
                    false
                }
                Next::Offer => {
                    offered.push(*id);
                    false
                }
                Next::Forget => false,
            }
        });
        for id in offered {
            self.offer_continue(id);
        }
        for id in going_on {
            let terminal = TerminalTarget::Local(self.local[&id].terminal.clone());
            self.spawn(async move {
                let result = terminal
                    .type_line(super::commands::GO_ON.to_owned(), true)
                    .await;
                Completion::WentOn { id, result }
            });
        }
    }

    pub(super) fn went_on(&self, id: Uuid, result: Result<()>) {
        if let Err(error) = result {
            tracing::debug!(%error, "an agent waits for someone to let it go on");
            self.offer_continue(id);
        }
    }

    pub(super) fn offer_continue(&self, id: Uuid) {
        if let Some(local) = self.local.get(&id)
            && local.terminal.offer_continue().is_err()
        {
            tracing::debug!("a terminal closed before it offered to go on");
        }
    }

    /// Tells the apps whether a restart now loses nothing while nobody uses Kodosi, and how many
    /// terminals would lose a program that does not come back.
    pub(super) fn tell_restart(&mut self) {
        let now = Instant::now();
        let (stopping, used) = cost(
            self.local
                .values()
                .filter(|local| local.comes_back())
                .map(|local| &local.entry),
            self.scope.user.as_deref(),
        );
        let quiet = if stopping == 0 && !used && self.attention.away() {
            let since = *self.restart.quiet_since.get_or_insert(now);
            now.duration_since(since) >= QUIET_FOR
        } else {
            self.restart.quiet_since = None;
            false
        };
        if self.restart.told == Some((quiet, stopping)) {
            return;
        }
        self.restart.told = Some((quiet, stopping));
        let event = json!({"type":"system.restart", "quiet":quiet, "stops":stopping});
        self.cached_events
            .insert("system.restart".to_owned(), event.clone());
        self.emit(event);
    }
}

#[cfg(test)]
#[path = "restart_tests.rs"]
mod tests;
