use crate::domain::{SubAgentInfo, SubAgentStatus, SubAgentTree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskNotificationStatus {
    Completed,
    Failed,
    Stopped,
}

impl TaskNotificationStatus {
    pub fn from_str_ci(value: &str) -> Option<Self> {
        match value {
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "stopped" => Some(Self::Stopped),
            _ => None,
        }
    }

    const fn as_agent_status(self) -> SubAgentStatus {
        match self {
            Self::Completed => SubAgentStatus::Completed,
            Self::Failed => SubAgentStatus::Failed,
            Self::Stopped => SubAgentStatus::Stopped,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SubAgentSpawn<'a> {
    pub agent_id: &'a str,
}

#[derive(Debug, Default)]
pub struct SubAgentTracker {
    agents: Vec<(String, SubAgentStatus)>,
    tasks: Vec<(String, usize)>,
    dirty: bool,
}

const WORKER_CAP: usize = 256;

impl SubAgentTracker {
    pub fn reset(&mut self) {
        let had_agents = !self.agents.is_empty();
        *self = Self::default();
        self.dirty = had_agents;
    }

    pub fn record_spawn(&mut self, spawn: SubAgentSpawn<'_>) {
        if spawn.agent_id.is_empty()
            || self.index_for_agent(spawn.agent_id).is_some()
            || self.agents.len() >= WORKER_CAP
        {
            return;
        }
        self.agents
            .push((spawn.agent_id.to_owned(), SubAgentStatus::Running));
        self.dirty = true;
    }

    pub fn record_completion(&mut self, agent_id: &str) {
        self.transition(agent_id, SubAgentStatus::Completed);
    }

    pub fn record_failure(&mut self, agent_id: &str) {
        self.transition(agent_id, SubAgentStatus::Failed);
    }

    pub fn record_task_started(&mut self, task_id: &str, tool_use_id: Option<&str>) {
        if self.tasks.iter().any(|(id, _)| id == task_id) || self.tasks.len() >= WORKER_CAP {
            return;
        }
        let index = tool_use_id
            .and_then(|id| self.index_for_agent(id))
            .unwrap_or_else(|| {
                if self.agents.len() >= WORKER_CAP {
                    return WORKER_CAP;
                }
                let id = tool_use_id.unwrap_or(task_id);
                self.agents.push((id.to_owned(), SubAgentStatus::Running));
                self.agents.len() - 1
            });
        if index == WORKER_CAP {
            return;
        }
        self.tasks.push((task_id.to_owned(), index));
        self.dirty = true;
    }

    pub fn record_task_notification(&mut self, task_id: &str, status: TaskNotificationStatus) {
        let Some(index) = self
            .tasks
            .iter()
            .find_map(|(id, index)| (id == task_id).then_some(*index))
        else {
            return;
        };
        self.transition_at(index, status.as_agent_status());
    }

    pub fn take_tree(&mut self) -> Option<SubAgentTree> {
        if !std::mem::take(&mut self.dirty) {
            return None;
        }
        Some(SubAgentTree {
            agents: self
                .agents
                .iter()
                .map(|(_, status)| SubAgentInfo { status: *status })
                .collect(),
        })
    }

    fn index_for_agent(&self, agent_id: &str) -> Option<usize> {
        self.agents
            .iter()
            .position(|(existing, _)| existing == agent_id)
    }

    fn transition(&mut self, agent_id: &str, next: SubAgentStatus) {
        if let Some(index) = self.index_for_agent(agent_id) {
            self.transition_at(index, next);
        }
    }

    fn transition_at(&mut self, index: usize, next: SubAgentStatus) {
        let Some((_, current)) = self.agents.get_mut(index) else {
            return;
        };
        if matches!(*current, SubAgentStatus::Failed | SubAgentStatus::Stopped)
            || (*current == SubAgentStatus::Completed
                && !matches!(next, SubAgentStatus::Failed | SubAgentStatus::Stopped))
        {
            return;
        }
        if *current != next {
            *current = next;
            self.dirty = true;
        }
    }
}

pub fn truncate_chars(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_owned();
    }
    let mut output: String = value.chars().take(max).collect();
    output.push('…');
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_current_worker_status_only() {
        let mut tracker = SubAgentTracker::default();
        tracker.record_spawn(SubAgentSpawn { agent_id: "tool-1" });
        assert_eq!(
            tracker.take_tree().expect("spawn").agents[0].status,
            SubAgentStatus::Running
        );

        tracker.record_completion("tool-1");
        assert_eq!(
            tracker.take_tree().expect("completion").agents[0].status,
            SubAgentStatus::Completed
        );
    }

    #[test]
    fn failure_can_correct_provisional_completion() {
        let mut tracker = SubAgentTracker::default();
        tracker.record_spawn(SubAgentSpawn { agent_id: "tool-1" });
        tracker.record_task_started("task-1", Some("tool-1"));
        tracker.record_completion("tool-1");
        drop(tracker.take_tree());

        tracker.record_task_notification("task-1", TaskNotificationStatus::Failed);
        assert_eq!(
            tracker.take_tree().expect("failure").agents[0].status,
            SubAgentStatus::Failed
        );
    }

    #[test]
    fn current_worker_state_is_bounded() {
        let mut tracker = SubAgentTracker::default();
        for index in 0..(WORKER_CAP + 50) {
            let id = format!("worker-{index}");
            tracker.record_spawn(SubAgentSpawn { agent_id: &id });
        }
        assert_eq!(
            tracker.take_tree().expect("workers").agents.len(),
            WORKER_CAP
        );
    }
}
