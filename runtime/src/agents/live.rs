use serde::{Deserialize, Serialize};

#[cfg(feature = "host")]
use super::last_value;
use super::{Agent, Output, Permissions};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Permissions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Output>,
}

impl AgentSettings {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    pub fn is_valid(&self) -> bool {
        [&self.model, &self.effort]
            .into_iter()
            .flatten()
            .all(|value| is_value(value))
    }

    /// A new model without an effort runs at its own effort, which Kodosi doesn't know.
    #[must_use]
    pub fn changed(&self, change: &Self) -> Self {
        let effort = match (&change.model, &change.effort) {
            (_, Some(effort)) => Some(effort.clone()),
            (Some(_), None) => None,
            (None, None) => self.effort.clone(),
        };
        Self {
            model: change.model.clone().or_else(|| self.model.clone()),
            effort,
            permissions: change.permissions.or(self.permissions),
            output: change.output.or(self.output),
        }
    }
}

#[cfg(feature = "host")]
pub(crate) const NO_RUNNING_AGENT: &str = "No agent runs in front in this terminal.";

/// What Claude Code asks a person before it switches the model or effort of a conversation
/// whose cache is warm, without reporting that it waits.
#[cfg(feature = "host")]
const CLAUDE_QUESTIONS: [&str; 2] = ["Switch model?", "Change effort level?"];

#[cfg(feature = "host")]
pub(crate) fn asks(questions: &[&str], screen: &str) -> bool {
    screen.lines().any(|line| questions.contains(&line.trim()))
}

pub(crate) fn live_of(program: Option<&str>) -> Option<AgentLive> {
    program
        .and_then(Agent::named)
        .map(Agent::live)
        .filter(|live| !live.is_empty())
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentLive {
    pub model: bool,
    pub effort: bool,
    pub permissions: Vec<Permissions>,
    pub keeps: bool,
}

impl AgentLive {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

impl Agent {
    #[cfg(feature = "host")]
    pub(crate) const fn screen_questions(self) -> &'static [&'static str] {
        match self {
            Self::Claude => &CLAUDE_QUESTIONS,
            Self::Codex | Self::Copilot | Self::Gemini | Self::Cursor | Self::Opencode => &[],
        }
    }

    pub fn live(self) -> AgentLive {
        match self {
            Self::Claude => AgentLive {
                model: true,
                effort: true,
                keeps: true,
                ..AgentLive::default()
            },
            Self::Codex => AgentLive {
                model: true,
                effort: true,
                permissions: vec![Permissions::Ask, Permissions::Auto],
                ..AgentLive::default()
            },
            Self::Copilot => AgentLive {
                model: true,
                permissions: Permissions::ALL.to_vec(),
                ..AgentLive::default()
            },
            Self::Gemini => AgentLive {
                model: true,
                ..AgentLive::default()
            },
            Self::Cursor => AgentLive {
                permissions: vec![Permissions::Ask, Permissions::Unrestricted],
                ..AgentLive::default()
            },
            Self::Opencode => AgentLive::default(),
        }
    }

    #[cfg(feature = "host")]
    pub(crate) fn live_lines(
        self,
        change: &AgentSettings,
    ) -> Result<Vec<(String, AgentSettings)>, String> {
        let label = self.label();
        let can = self.live();
        if change.is_empty() {
            return Err("Choose what to change.".to_owned());
        }
        if !change.is_valid() {
            return Err(format!("{label} has no such model or effort."));
        }
        let unchanged = |what: &str| {
            format!(
                "{label} can't change its {what} while it runs. Choose it when you start {label}."
            )
        };
        if change.model.is_some() && !can.model {
            return Err(unchanged("model"));
        }
        if change.effort.is_some() && !can.effort {
            return Err(unchanged("effort"));
        }
        if change
            .permissions
            .is_some_and(|permissions| !can.permissions.contains(&permissions))
        {
            return Err(unchanged("permissions"));
        }
        if change.output.is_some() {
            return Err(unchanged("output"));
        }
        let part = |model: Option<&String>, effort: Option<&String>| AgentSettings {
            model: model.cloned(),
            effort: effort.cloned(),
            ..AgentSettings::default()
        };
        let mut lines = Vec::new();
        match (self, &change.model, &change.effort) {
            (Self::Codex, model, effort) if model.is_some() || effort.is_some() => {
                let line = [
                    Some("/model".to_owned()),
                    model.clone(),
                    effort
                        .as_ref()
                        .map(|effort| format!("--reasoning={effort}")),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
                lines.push((line, part(model.as_ref(), effort.as_ref())));
            }
            (Self::Gemini, Some(model), _) => {
                lines.push((format!("/model set {model}"), part(Some(model), None)));
            }
            (Self::Claude | Self::Copilot, model, effort) => {
                if let Some(model) = model {
                    lines.push((format!("/model {model}"), part(Some(model), None)));
                }
                if let Some(effort) = effort {
                    lines.push((format!("/effort {effort}"), part(None, Some(effort))));
                }
            }
            _ => {}
        }
        if let Some(permissions) = change.permissions {
            let line = match (self, permissions) {
                (Self::Codex, Permissions::Ask) => "/permissions :read-only",
                (Self::Codex, Permissions::Auto) => "/permissions :workspace",
                (Self::Copilot, Permissions::Ask) => "/permissions default",
                (Self::Copilot, Permissions::Auto) => "/permissions assisted",
                (Self::Copilot, Permissions::Unrestricted) => "/permissions allow-all",
                (Self::Cursor, Permissions::Ask) => "/run-everything off",
                (Self::Cursor, Permissions::Unrestricted) => "/run-everything on",
                _ => return Err(unchanged("permissions")),
            };
            lines.push((
                line.to_owned(),
                AgentSettings {
                    permissions: Some(permissions),
                    ..AgentSettings::default()
                },
            ));
        }
        Ok(lines)
    }

    #[cfg(feature = "host")]
    pub(crate) fn started_with(self, arguments: &[String]) -> AgentSettings {
        if arguments.is_empty() {
            return AgentSettings::default();
        }
        let has = |flags: &[&str]| {
            arguments
                .iter()
                .skip(1)
                .any(|argument| flags.contains(&argument.as_str()))
        };
        let value = |flag: &str| last_value(arguments, flag, "");
        let model = last_value(arguments, self.model_flag(), "")
            .filter(|model| is_value(model))
            .map(str::to_owned);
        let effort = self.effort_flag().and_then(|(flag, start)| {
            last_value(arguments, flag, start)
                .and_then(|value| value.strip_prefix(start))
                .filter(|effort| is_value(effort))
                .map(str::to_owned)
        });
        let permissions = match self {
            Self::Claude if has(&["--dangerously-skip-permissions"]) => {
                Some(Permissions::Unrestricted)
            }
            Self::Claude => match value("--permission-mode") {
                Some("manual" | "default") => Some(Permissions::Ask),
                Some("auto") => Some(Permissions::Auto),
                Some("bypassPermissions") => Some(Permissions::Unrestricted),
                _ => None,
            },
            Self::Codex if has(&["--dangerously-bypass-approvals-and-sandbox", "--yolo"]) => {
                Some(Permissions::Unrestricted)
            }
            Self::Codex if has(&["--full-auto"]) => Some(Permissions::Auto),
            Self::Codex => match value("-s").or_else(|| value("--sandbox")) {
                Some("read-only") => Some(Permissions::Ask),
                Some("workspace-write") => Some(Permissions::Auto),
                Some("danger-full-access") => Some(Permissions::Unrestricted),
                _ => None,
            },
            Self::Copilot if has(&["--yolo", "--allow-all"]) => Some(Permissions::Unrestricted),
            Self::Copilot if has(&["--assisted-approval"]) => Some(Permissions::Auto),
            Self::Gemini if has(&["--yolo", "-y"]) => Some(Permissions::Unrestricted),
            Self::Gemini => match value("--approval-mode") {
                Some("default") => Some(Permissions::Ask),
                Some("auto_edit") => Some(Permissions::Auto),
                Some("yolo") => Some(Permissions::Unrestricted),
                _ => None,
            },
            Self::Cursor if has(&["--force", "-f", "--yolo"]) => Some(Permissions::Unrestricted),
            Self::Opencode if has(&["--auto"]) => Some(Permissions::Auto),
            Self::Opencode if has(&["--yolo", "--dangerously-skip-permissions"]) => None,
            Self::Copilot | Self::Cursor | Self::Opencode => Some(Permissions::Ask),
        };
        let output = match self {
            Self::Claude => match last_value(arguments, "--settings", r#"{"viewMode""#) {
                Some(settings) if settings.contains(r#""focus""#) => Some(Output::Quiet),
                Some(settings) if settings.contains(r#""verbose""#) => Some(Output::Detailed),
                _ if has(&["--verbose"]) => Some(Output::Detailed),
                _ => Some(Output::Standard),
            },
            Self::Codex => {
                if last_value(arguments, "-c", "hide_agent_reasoning=true").is_some() {
                    Some(Output::Quiet)
                } else if last_value(arguments, "-c", "model_reasoning_summary=detailed").is_some()
                {
                    Some(Output::Detailed)
                } else {
                    Some(Output::Standard)
                }
            }
            Self::Copilot | Self::Cursor | Self::Gemini | Self::Opencode => None,
        };
        AgentSettings {
            model,
            effort,
            permissions,
            output,
        }
    }
}

fn is_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-/:@+[]".contains(&byte))
}
