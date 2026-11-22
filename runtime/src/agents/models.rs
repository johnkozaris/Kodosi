use std::{path::Path, process::Stdio, time::Duration};

use serde_json::Value;
use tokio::io::AsyncReadExt as _;

use super::{Agent, AgentModel, AgentModels, AgentOption, shell::Facts, shell_word};

const RUN_TIME: Duration = Duration::from_secs(10);
const MAX_OUTPUT: usize = 4 * 1024 * 1024;
const MAX_MODELS: usize = 500;

pub(super) async fn models(facts: &Facts, agent: Agent) -> AgentModels {
    let program = facts.executable(agent.program());
    let program = program.as_deref();
    let (models, efforts) = match agent {
        Agent::Claude => (
            fixed(
                &[
                    ("fable", "Fable"),
                    ("opus", "Opus"),
                    ("sonnet", "Sonnet"),
                    ("haiku", "Haiku"),
                ],
                agent,
            ),
            efforts(run(facts, program, &["--help"]).await.as_deref(), agent),
        ),
        Agent::Codex => codex(run(facts, program, &["debug", "models"]).await.as_deref()),
        Agent::Copilot => {
            let (config, help) = tokio::join!(
                run(facts, program, &["help", "config"]),
                run(facts, program, &["--help"])
            );
            (copilot(config.as_deref()), efforts(help.as_deref(), agent))
        }
        Agent::Cursor => (
            cursor(run(facts, program, &["models"]).await.as_deref()),
            Vec::new(),
        ),
        Agent::Opencode => (
            opencode(run(facts, program, &["models"]).await.as_deref()),
            Vec::new(),
        ),
        Agent::Antigravity => antigravity(run(facts, program, &["models"]).await.as_deref()),
    };
    AgentModels { models, efforts }
}

pub(super) async fn run(
    facts: &Facts,
    program: Option<&Path>,
    arguments: &[&str],
) -> Option<String> {
    let mut command = tokio::process::Command::new(program?);
    command
        .args(arguments)
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .env("COPILOT_AUTO_UPDATE", "false")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    if let Some(path) = &facts.path {
        command.env("PATH", path);
    }
    kodosi_os::process::without_terminal(command.as_std_mut());
    let mut child = command.spawn().ok()?;
    let stdout = child.stdout.take()?;
    let mut output = Vec::new();
    let success = tokio::time::timeout(RUN_TIME, async {
        stdout
            .take(MAX_OUTPUT as u64 + 1)
            .read_to_end(&mut output)
            .await
            .ok()?;
        if output.len() > MAX_OUTPUT {
            return None;
        }
        child.wait().await.ok().map(|status| status.success())
    })
    .await
    .ok()??;
    success.then(|| without_escapes(&String::from_utf8_lossy(&output)))
}

fn model(id: &str, label: &str, agent: Agent, efforts: Vec<String>) -> Option<AgentModel> {
    let label = label.trim();
    if id.len() > 128 || label.len() > 128 || label.chars().any(char::is_control) {
        return None;
    }
    Some(AgentModel {
        id: id.to_owned(),
        label: if label.is_empty() { id } else { label }.to_owned(),
        argument: format!("{} {}", agent.model_flag(), shell_word(id)?),
        efforts,
    })
}

fn fixed(models: &[(&str, &str)], agent: Agent) -> Vec<AgentModel> {
    models
        .iter()
        .filter_map(|(id, label)| model(id, label, agent, Vec::new()))
        .collect()
}

fn effort(agent: Agent, id: &str) -> Option<AgentOption> {
    let (flag, start) = agent.effort_flag()?;
    (!id.is_empty()
        && id.len() <= 32
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'))
    .then(|| AgentOption {
        id: id.to_owned(),
        argument: format!("{flag} {start}{id}"),
    })
}

pub(super) fn listed_values(help: &str, flag: &str) -> Vec<String> {
    let Some(start) = help
        .lines()
        .position(|line| line.trim_start().starts_with(&format!("{flag} ")))
    else {
        return Vec::new();
    };
    let lines = help.lines().skip(start).collect::<Vec<_>>();
    let end = lines
        .iter()
        .skip(1)
        .position(|line| line.trim_start().starts_with('-'))
        .map_or(lines.len(), |end| end + 1);
    let section = lines[..end].join(" ");
    let inner = section
        .split_once("possible values:")
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(inner, _)| inner)
        .or_else(|| {
            section
                .split_once('(')
                .and_then(|(_, rest)| rest.split_once(')'))
                .map(|(inner, _)| inner)
        });
    inner.map_or_default(|inner| {
        inner
            .split([',', '|'])
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect()
    })
}

fn efforts(help: Option<&str>, agent: Agent) -> Vec<AgentOption> {
    let Some((flag, _)) = agent.effort_flag() else {
        return Vec::new();
    };
    help.map_or_default(|help| listed_values(help, flag))
        .iter()
        .filter_map(|id| effort(agent, id))
        .collect()
}

pub(super) fn codex(catalog: Option<&str>) -> (Vec<AgentModel>, Vec<AgentOption>) {
    let Some(catalog) = catalog.and_then(|text| serde_json::from_str::<Value>(text).ok()) else {
        return (Vec::new(), Vec::new());
    };
    let mut entries = catalog
        .get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry
                .get("visibility")
                .and_then(Value::as_str)
                .is_none_or(|visibility| visibility == "list")
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| {
        entry
            .get("priority")
            .and_then(Value::as_i64)
            .unwrap_or(i64::MAX)
    });
    let mut levels: Vec<String> = Vec::new();
    let models = entries
        .into_iter()
        .take(MAX_MODELS)
        .filter_map(|entry| {
            let id = entry.get("slug").and_then(Value::as_str)?;
            let label = entry
                .get("display_name")
                .and_then(Value::as_str)
                .unwrap_or(id);
            let efforts = entry
                .get("supported_reasoning_levels")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|level| {
                    level
                        .get("effort")
                        .or(Some(level))
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .collect::<Vec<_>>();
            for level in &efforts {
                if !levels.contains(level) {
                    levels.push(level.clone());
                }
            }
            model(id, label, Agent::Codex, efforts)
        })
        .collect();
    let efforts = levels
        .iter()
        .filter_map(|level| effort(Agent::Codex, level))
        .collect();
    (models, efforts)
}

pub(super) fn copilot(config: Option<&str>) -> Vec<AgentModel> {
    let Some(config) = config else {
        return Vec::new();
    };
    config
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("`model`:"))
        .skip(1)
        .map(str::trim)
        .take_while(|line| line.starts_with("- \""))
        .filter_map(|line| line.strip_circumfix("- \"", '"'))
        .take(MAX_MODELS)
        .filter_map(|id| model(id, id, Agent::Copilot, Vec::new()))
        .collect()
}

pub(super) fn cursor(list: Option<&str>) -> Vec<AgentModel> {
    list.into_iter()
        .flat_map(str::lines)
        .filter_map(|line| {
            let (id, label) = line.trim().split_once(" - ")?;
            let label = ["(current)", "(default)"]
                .iter()
                .fold(label.trim(), |label, mark| {
                    label.trim_end_matches(mark).trim_end()
                });
            if id.contains(char::is_whitespace) {
                return None;
            }
            model(id, label, Agent::Cursor, Vec::new())
        })
        .take(MAX_MODELS)
        .collect()
}

pub(super) fn opencode(list: Option<&str>) -> Vec<AgentModel> {
    list.into_iter()
        .flat_map(str::lines)
        .map(str::trim)
        .filter(|line| line.contains('/') && !line.contains(char::is_whitespace))
        .filter_map(|id| model(id, id, Agent::Opencode, Vec::new()))
        .take(MAX_MODELS)
        .collect()
}

/// The thinking levels that end the identifiers of Antigravity CLI's models, lowest first.
const ANTIGRAVITY_LEVELS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// One model and thinking level on each line, its identifier, a tab and its name, as in
/// `gemini-4-neon-high` and `Gemini 4 Neon (High)`. The launcher shows each model once with its
/// levels.
pub(super) fn antigravity(list: Option<&str>) -> (Vec<AgentModel>, Vec<AgentOption>) {
    let mut models: Vec<AgentModel> = Vec::new();
    for line in list.into_iter().flat_map(str::lines) {
        let line = line.trim();
        let (id, label) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let slug = id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-._".contains(&byte)
        }) && id.contains('-');
        if !slug {
            continue;
        }
        let (id, level) = match id.rsplit_once('-') {
            Some((model, level)) if ANTIGRAVITY_LEVELS.contains(&level) => (model, Some(level)),
            _ => (id, None),
        };
        let label = label.trim();
        let name = level
            .and_then(|_| label.rsplit_once(" ("))
            .map_or(label, |(name, _)| name);
        let level = level.map(str::to_owned);
        if let Some(known) = models.iter_mut().find(|model| model.id == id) {
            known.efforts.extend(level);
        } else if models.len() < MAX_MODELS
            && let Some(model) = model(id, name, Agent::Antigravity, level.into_iter().collect())
        {
            models.push(model);
        }
    }
    let rank = |level: &String| ANTIGRAVITY_LEVELS.iter().position(|known| known == level);
    for model in &mut models {
        model.efforts.sort_by_key(rank);
        model.efforts.dedup();
    }
    let efforts = ANTIGRAVITY_LEVELS
        .iter()
        .filter(|level| {
            models
                .iter()
                .any(|model| model.efforts.iter().any(|known| known == *level))
        })
        .filter_map(|level| effort(Agent::Antigravity, level))
        .collect();
    (models, efforts)
}

pub(super) fn without_escapes(text: &str) -> String {
    let mut clean = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '\u{1b}' {
            clean.push(character);
            continue;
        }
        if characters.next_if_eq(&'[').is_some() {
            while characters
                .next()
                .is_some_and(|code| !code.is_ascii_alphabetic())
            {}
        }
    }
    clean
}
