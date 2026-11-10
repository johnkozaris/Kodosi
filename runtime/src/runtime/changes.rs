use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use tokio::time::Instant;
use uuid::Uuid;

use crate::agents::AgentSettings;
use crate::terminal::{ProgramState, ProgramStatus};

/// The pause after a command that Kodosi typed into an agent, before the next one.
pub(super) const SPACING: Duration = Duration::from_millis(1500);

/// The agents' own commands that wait to be typed into their terminals, each with the part of
/// the change that it makes.
#[derive(Debug, Default)]
pub(super) struct AgentChanges {
    waiting: HashMap<Uuid, VecDeque<(String, AgentSettings)>>,
    typing: HashMap<Uuid, AgentSettings>,
    typed: HashMap<Uuid, Instant>,
}

impl AgentChanges {
    /// Adds the commands of a change. A newer command replaces one that still waits to change
    /// the same thing, and an effort that waits follows a new model, which would reset it.
    pub(super) fn add(&mut self, id: Uuid, lines: Vec<(String, AgentSettings)>) {
        let waiting = self.waiting.entry(id).or_default();
        for (line, part) in lines {
            waiting.retain(|(_, earlier)| !overlaps(earlier, &part));
            let efforts = if part.model.is_some() {
                let (efforts, rest): (VecDeque<_>, VecDeque<_>) = waiting
                    .drain(..)
                    .partition(|(_, earlier)| earlier.effort.is_some() && earlier.model.is_none());
                *waiting = rest;
                efforts
            } else {
                VecDeque::new()
            };
            waiting.push_back((line, part));
            waiting.extend(efforts);
        }
    }

    pub(super) fn forget(&mut self, id: Uuid) {
        self.waiting.remove(&id);
        self.typing.remove(&id);
        self.typed.remove(&id);
    }

    /// The change that still waits for the agent of a terminal, with the part that Kodosi
    /// types now.
    pub(super) fn pending(&self, id: Uuid) -> Option<AgentSettings> {
        let parts = self.typing.get(&id).into_iter().chain(
            self.waiting
                .get(&id)
                .into_iter()
                .flatten()
                .map(|(_, part)| part),
        );
        let change = parts.fold(AgentSettings::default(), |change, part| AgentSettings {
            model: part.model.clone().or(change.model),
            effort: part.effort.clone().or(change.effort),
            permissions: part.permissions.or(change.permissions),
            output: part.output.or(change.output),
        });
        (!change.is_empty()).then_some(change)
    }

    /// The terminals that have a command waiting, and no command typed or tried in the last
    /// moment.
    pub(super) fn due(&self, now: Instant) -> Vec<Uuid> {
        self.waiting
            .iter()
            .filter(|(id, lines)| {
                !lines.is_empty()
                    && !self.typing.contains_key(*id)
                    && self
                        .typed
                        .get(*id)
                        .is_none_or(|at| now.duration_since(*at) >= SPACING)
            })
            .map(|(id, _)| *id)
            .collect()
    }

    pub(super) fn take(&mut self, id: Uuid) -> Option<(String, AgentSettings)> {
        let next = self.waiting.get_mut(&id)?.pop_front();
        if let Some((_, part)) = &next {
            self.typing.insert(id, part.clone());
        }
        next
    }

    /// Notes the end of typing `line`: done, or back in front of the others to try again after
    /// the pause.
    pub(super) fn typed(
        &mut self,
        id: Uuid,
        line: (String, AgentSettings),
        done: bool,
        now: Instant,
    ) {
        if self.typing.remove(&id).is_none() {
            return;
        }
        self.typed.insert(id, now);
        if done {
            if self.waiting.get(&id).is_some_and(VecDeque::is_empty) {
                self.waiting.remove(&id);
            }
        } else if let Some(waiting) = self.waiting.get_mut(&id) {
            if !waiting.iter().any(|(_, later)| overlaps(later, &line.1)) {
                waiting.push_front(line);
            }
        } else {
            self.waiting.insert(id, VecDeque::from([line]));
        }
    }
}

/// Whether the agent waits for its next prompt, so that it takes one of its own commands.
pub(super) fn takes_commands(program: Option<&str>, status: Option<&ProgramStatus>) -> bool {
    program.is_some_and(crate::agents::is_agent_name)
        && status.is_some_and(|status| {
            matches!(
                status.state,
                ProgramState::Idle | ProgramState::Done | ProgramState::Error
            )
        })
}

fn overlaps(earlier: &AgentSettings, later: &AgentSettings) -> bool {
    (earlier.model.is_some() && later.model.is_some())
        || (earlier.effort.is_some() && later.effort.is_some())
        || (earlier.permissions.is_some() && later.permissions.is_some())
        || (earlier.output.is_some() && later.output.is_some())
}
