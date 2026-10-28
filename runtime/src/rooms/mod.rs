use serde::{Deserialize, Serialize};
use specta_macros::Type;

pub(crate) mod crypto;
pub(crate) mod providers;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Action {
    Read {
        #[serde(default)]
        before: Option<u64>,
    },
    Post {
        text: String,
        #[serde(default)]
        terminal_id: Option<String>,
        #[serde(default)]
        agent: Option<String>,
    },
    CreateTask {
        title: String,
        #[serde(default)]
        description: String,
        #[serde(default)]
        repository_ids: Vec<String>,
        #[serde(default)]
        terminal_id: Option<String>,
        #[serde(default)]
        issue_repository_id: Option<String>,
    },
    UpdateTask {
        task_id: String,
        change: TaskChange,
        #[serde(default)]
        note: Option<String>,
        #[serde(default)]
        terminal_id: Option<String>,
    },
    AddRepository {
        url: String,
        #[serde(default)]
        provider: Option<String>,
    },
    Issues {
        repository_id: String,
    },
    ImportIssue {
        repository_id: String,
        number: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum TaskChange {
    Claim,
    Release,
    Close,
    Reopen,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub sequence: u64,
    pub author_id: String,
    pub author_name: String,
    pub agent: Option<String>,
    pub terminal_id: Option<String>,
    pub text: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub version: u64,
    pub title: String,
    pub description: String,
    pub closed: bool,
    pub assigned_to: Option<String>,
    pub assigned_name: Option<String>,
    pub terminal_id: Option<String>,
    pub repository_ids: Vec<String>,
    pub note: Option<String>,
    pub issue: Option<Issue>,
}

impl Task {
    pub(crate) fn claim(
        &mut self,
        user_id: &str,
        name: String,
        terminal_id: Option<String>,
        live: impl Fn(&str) -> bool,
    ) -> Result<(), String> {
        if self.closed {
            return Err("This task is done. Open it again before you take it.".to_owned());
        }
        match &self.assigned_to {
            Some(holder) if holder != user_id => {
                return Err(format!(
                    "{} has this task. Take another task, or ask them to release it.",
                    self.assigned_name.as_deref().unwrap_or("Another member")
                ));
            }
            Some(_)
                if self.terminal_id != terminal_id
                    && self.terminal_id.as_deref().is_some_and(live) =>
            {
                return Err(
                    "You have this task in another terminal. Release it there first.".to_owned(),
                );
            }
            _ => {}
        }
        self.assigned_to = Some(user_id.to_owned());
        self.assigned_name = Some(name);
        self.terminal_id = terminal_id;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub id: String,
    pub name: String,
    pub url: String,
    pub host: String,
    pub owner: String,
    pub repository: String,
    pub provider: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub repository_id: String,
    pub number: u64,
    pub title: String,
    pub body: String,
    pub url: String,
    pub closed: bool,
    pub assignees: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub room_id: String,
    pub messages: Vec<Message>,
    pub tasks: Vec<Task>,
    pub repositories: Vec<Repository>,
    pub sequence: u64,
    pub has_older: bool,
    pub history_trimmed: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Payload {
    Message {
        text: String,
        #[serde(rename = "authorName")]
        author_name: String,
        agent: Option<String>,
        #[serde(rename = "terminalId")]
        terminal_id: Option<String>,
    },
    Task {
        task: Task,
    },
    Repository {
        repository: Repository,
    },
    Dosi {
        record: crate::dosi::DosiRecord,
    },
    DosiInstructions {
        text: Option<String>,
    },
}

impl Action {
    pub(crate) fn validate(&self) -> crate::Result<()> {
        let text = |value: &str, maximum: usize| {
            if value.trim().is_empty() || value.len() > maximum || value.contains('\0') {
                Err(crate::Error::Invalid(
                    "Room text is empty or too long.".into(),
                ))
            } else {
                Ok(())
            }
        };
        match self {
            Self::Post {
                text: value,
                agent,
                terminal_id,
            } => {
                text(value, 16_384)?;
                if let Some(agent) = agent {
                    text(agent, 128)?;
                }
                if let Some(id) = terminal_id {
                    crate::protocol::parse_id(id)?;
                }
            }
            Self::CreateTask {
                title,
                description,
                repository_ids,
                terminal_id,
                issue_repository_id,
            } => {
                text(title, 256)?;
                if description.len() > 16_384 || repository_ids.len() > 64 {
                    return Err(crate::Error::Invalid("Task details are too long.".into()));
                }
                for id in repository_ids
                    .iter()
                    .chain(terminal_id)
                    .chain(issue_repository_id)
                {
                    crate::protocol::parse_id(id)?;
                }
            }
            Self::UpdateTask {
                task_id,
                note,
                terminal_id,
                ..
            } => {
                crate::protocol::parse_id(task_id)?;
                if let Some(note) = note {
                    text(note, 16_384)?;
                }
                if let Some(id) = terminal_id {
                    crate::protocol::parse_id(id)?;
                }
            }
            Self::AddRepository { url, provider } => {
                text(url, 2048)?;
                if provider
                    .as_deref()
                    .is_some_and(|p| !matches!(p, "github" | "gitea"))
                {
                    return Err(crate::Error::Invalid("Choose GitHub or Gitea.".into()));
                }
            }
            Self::Issues { repository_id } | Self::ImportIssue { repository_id, .. } => {
                crate::protocol::parse_id(repository_id)?;
            }
            Self::Read { .. } => {}
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod crypto_tests;

#[cfg(test)]
mod tests;
