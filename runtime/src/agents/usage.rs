use std::{path::Path, process::Stdio, time::Duration};

use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufReader},
    process::{Child, ChildStdin, ChildStdout},
};

use super::{Agent, Runner, UsageLevel, UsageLimit};

const READ_TIME: Duration = Duration::from_secs(20);
const END_GRACE: Duration = Duration::from_secs(2);
const MAX_MESSAGE: usize = 1024 * 1024;
const MAX_HEADER: u64 = 1024;
const MAX_SCOPE_CHARS: usize = 32;
const NEAR: u8 = 80;
const SESSION_HOURS: u32 = 5;
const WEEK_HOURS: u32 = 7 * 24;

pub(super) const fn reports(agent: Agent) -> bool {
    matches!(agent, Agent::Claude | Agent::Codex | Agent::Copilot)
}

/// Asks the agent's own program for the plan usage of one account. An empty list means that the
/// account has no plan limits, such as an account that pays by use.
pub(super) async fn read(
    agent: Agent,
    runner: &Runner,
    home: &Path,
) -> Result<Vec<UsageLimit>, String> {
    let mut process = Process::start(agent, runner, home)?;
    let limits = tokio::time::timeout(READ_TIME, process.ask(agent))
        .await
        .map_err(|_| format!("{} did not report its usage in time.", agent.label()))??;
    process.finish().await;
    Ok(headlined(limits))
}

struct Process {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
}

impl Process {
    fn start(agent: Agent, runner: &Runner, home: &Path) -> Result<Self, String> {
        let arguments: &[&str] = match agent {
            Agent::Claude => &[
                "--safe-mode",
                "-p",
                "/usage",
                "--output-format",
                "stream-json",
                "--verbose",
                "--no-session-persistence",
            ],
            Agent::Codex => &["app-server"],
            Agent::Copilot => &["--server", "--stdio", "--no-auto-update"],
            Agent::Cursor | Agent::Gemini | Agent::Opencode => {
                return Err(format!("{} reports no plan usage.", agent.label()));
            }
        };
        let mut command = tokio::process::Command::new(&runner.program);
        command
            .args(arguments)
            .current_dir(home)
            .env("NO_COLOR", "1")
            .env("TERM", "dumb")
            .env("COPILOT_AUTO_UPDATE", "false")
            .stdin(if agent == Agent::Claude {
                Stdio::null()
            } else {
                Stdio::piped()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        if let Some(path) = &runner.path {
            command.env("PATH", path);
        }
        if let Some((_, variable)) = agent.home() {
            command.env_remove(variable);
        }
        for (name, value) in &runner.variables {
            command.env(name, value);
        }
        kodosi_os::process::without_terminal(command.as_std_mut());
        let mut child = command
            .spawn()
            .map_err(|error| format!("{} did not start: {error}", agent.label()))?;
        let output = child
            .stdout
            .take()
            .map(BufReader::new)
            .ok_or_else(|| format!("{} gave no output.", agent.label()))?;
        Ok(Self {
            input: child.stdin.take(),
            child,
            output,
        })
    }

    async fn finish(mut self) {
        drop(self.input.take());
        drop(tokio::time::timeout(END_GRACE, self.child.wait()).await);
    }

    async fn ask(&mut self, agent: Agent) -> Result<Vec<UsageLimit>, String> {
        match agent {
            Agent::Claude => self.claude().await,
            Agent::Codex => self.codex().await,
            Agent::Copilot => self.copilot().await,
            Agent::Cursor | Agent::Gemini | Agent::Opencode => Ok(Vec::new()),
        }
    }

    async fn claude(&mut self) -> Result<Vec<UsageLimit>, String> {
        while let Some(message) = self.line().await? {
            if let Some(report) = message.get("usage_report") {
                return Ok(claude(report));
            }
            if message.get("type").and_then(Value::as_str) == Some("result") {
                return Ok(Vec::new());
            }
        }
        Err(ended(Agent::Claude))
    }

    async fn codex(&mut self) -> Result<Vec<UsageLimit>, String> {
        self.send_line(&json!({
            "method": "initialize",
            "id": 0,
            "params": {"clientInfo": {"name": "kodosi", "title": "Kodosi", "version": env!("CARGO_PKG_VERSION")}},
        }))
        .await?;
        self.line_answer(0).await?;
        self.send_line(&json!({"method": "initialized"})).await?;
        self.send_line(&json!({"method": "account/rateLimits/read", "id": 1}))
            .await?;
        Ok(codex(&self.line_answer(1).await?))
    }

    async fn copilot(&mut self) -> Result<Vec<UsageLimit>, String> {
        self.send_frame(
            &json!({"jsonrpc": "2.0", "id": 1, "method": "account.getQuota", "params": {}}),
        )
        .await?;
        loop {
            let message = self.frame().await?.ok_or_else(|| ended(Agent::Copilot))?;
            if let Some(result) = answer(&message, 1)? {
                return Ok(copilot(&result));
            }
        }
    }

    async fn line_answer(&mut self, id: u64) -> Result<Value, String> {
        loop {
            let message = self.line().await?.ok_or_else(|| ended(Agent::Codex))?;
            if let Some(result) = answer(&message, id)? {
                return Ok(result);
            }
        }
    }

    async fn line(&mut self) -> Result<Option<Value>, String> {
        loop {
            let mut bytes = Vec::new();
            let read = (&mut self.output)
                .take(MAX_MESSAGE as u64 + 1)
                .read_until(b'\n', &mut bytes)
                .await
                .map_err(|error| error.to_string())?;
            if read == 0 {
                return Ok(None);
            }
            if bytes.len() > MAX_MESSAGE {
                return Err("The agent sent a message that is too long.".to_owned());
            }
            if let Ok(value) = serde_json::from_slice(&bytes) {
                return Ok(Some(value));
            }
        }
    }

    async fn frame(&mut self) -> Result<Option<Value>, String> {
        let mut length = None;
        loop {
            let mut header = Vec::new();
            let read = (&mut self.output)
                .take(MAX_HEADER)
                .read_until(b'\n', &mut header)
                .await
                .map_err(|error| error.to_string())?;
            if read == 0 {
                return Ok(None);
            }
            let header = String::from_utf8_lossy(&header);
            let header = header.trim();
            if header.is_empty() {
                if length.is_some() {
                    break;
                }
                continue;
            }
            if let Some((name, value)) = header.split_once(':')
                && name.trim().eq_ignore_ascii_case("content-length")
            {
                length = value.trim().parse::<usize>().ok();
            }
        }
        let length = length
            .filter(|length| *length <= MAX_MESSAGE)
            .ok_or_else(|| "The agent sent a message that is too long.".to_owned())?;
        let mut body = vec![0; length];
        self.output
            .read_exact(&mut body)
            .await
            .map_err(|error| error.to_string())?;
        Ok(Some(serde_json::from_slice(&body).unwrap_or(Value::Null)))
    }

    async fn send_line(&mut self, message: &Value) -> Result<(), String> {
        self.send(format!("{message}\n").as_bytes()).await
    }

    async fn send_frame(&mut self, message: &Value) -> Result<(), String> {
        let body = message.to_string();
        self.send(format!("Content-Length: {}\r\n\r\n{body}", body.len()).as_bytes())
            .await
    }

    async fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        let input = self
            .input
            .as_mut()
            .ok_or_else(|| "The agent takes no input.".to_owned())?;
        input
            .write_all(bytes)
            .await
            .map_err(|error| error.to_string())?;
        input.flush().await.map_err(|error| error.to_string())
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let (Some(pid), Ok(None)) = (self.child.id(), self.child.try_wait()) {
            kodosi_os::process::end_group(pid, true);
        }
    }
}

fn ended(agent: Agent) -> String {
    format!("{} ended without its usage.", agent.label())
}

fn answer(message: &Value, id: u64) -> Result<Option<Value>, String> {
    if message.get("method").is_some() || message.get("id").and_then(Value::as_u64) != Some(id) {
        return Ok(None);
    }
    if let Some(error) = message.get("error") {
        return Err(error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("The agent refused the usage request.")
            .to_owned());
    }
    Ok(Some(message.get("result").cloned().unwrap_or(Value::Null)))
}

pub(super) fn claude(report: &Value) -> Vec<UsageLimit> {
    report
        .pointer("/rate_limits/limits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let text = |key: &str| row.get(key).and_then(Value::as_str).unwrap_or_default();
            let hours = match (text("kind"), text("group")) {
                ("session", _) | (_, "session") => Some(SESSION_HOURS),
                (_, "weekly") => Some(WEEK_HOURS),
                (_, "monthly") => None,
                _ => return None,
            };
            let scope = row
                .pointer("/scope/model/display_name")
                .or_else(|| row.pointer("/scope/surface/display_name"))
                .and_then(Value::as_str)
                .and_then(scope);
            limit(
                hours,
                scope,
                row.get("percent")?.as_f64()?,
                matches!(text("severity"), "warning" | "critical"),
                false,
                row.get("is_active").and_then(Value::as_bool) == Some(true),
            )
        })
        .collect()
}

pub(super) fn codex(result: &Value) -> Vec<UsageLimit> {
    let Some(snapshot) = result.get("rateLimits") else {
        return Vec::new();
    };
    ["primary", "secondary"]
        .into_iter()
        .filter_map(|window| {
            let window = snapshot.get(window)?;
            let minutes = window.get("windowDurationMins")?.as_u64()?;
            let hours = u32::try_from(minutes.div_ceil(60)).ok()?;
            limit(
                Some(hours),
                None,
                window.get("usedPercent")?.as_f64()?,
                false,
                false,
                false,
            )
        })
        .collect()
}

pub(super) fn copilot(result: &Value) -> Vec<UsageLimit> {
    let Some(quota) = result.pointer("/quotaSnapshots/premium_interactions") else {
        return Vec::new();
    };
    let flag = |key: &str| quota.get(key).and_then(Value::as_bool) == Some(true);
    if flag("isUnlimitedEntitlement") {
        return Vec::new();
    }
    quota
        .get("remainingPercentage")
        .and_then(Value::as_f64)
        .and_then(|remaining| {
            limit(
                None,
                None,
                100.0 - remaining,
                false,
                flag("usageAllowedWithExhaustedQuota"),
                true,
            )
        })
        .into_iter()
        .collect()
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the percentage is rounded and clamped to 0 through 100 first"
)]
fn limit(
    hours: Option<u32>,
    scope: Option<String>,
    percent: f64,
    warned: bool,
    continues: bool,
    headline: bool,
) -> Option<UsageLimit> {
    if !percent.is_finite() || hours == Some(0) {
        return None;
    }
    let used = percent.round().clamp(0.0, 100.0) as u8;
    let level = if used >= 100 && !continues {
        UsageLevel::Reached
    } else if warned || used >= NEAR {
        UsageLevel::Near
    } else {
        UsageLevel::Normal
    };
    Some(UsageLimit {
        hours,
        scope,
        used,
        level,
        headline,
    })
}

fn scope(name: &str) -> Option<String> {
    let name = name.trim();
    (!name.is_empty()
        && name.chars().count() <= MAX_SCOPE_CHARS
        && !name.chars().any(char::is_control))
    .then(|| name.to_owned())
}

/// Keeps one headline: the provider's own pick, or else the limit with the most use.
pub(super) fn headlined(mut limits: Vec<UsageLimit>) -> Vec<UsageLimit> {
    let chosen = limits.iter().position(|limit| limit.headline).or_else(|| {
        limits
            .iter()
            .enumerate()
            .max_by_key(|(index, limit)| (limit.used, std::cmp::Reverse(*index)))
            .map(|(index, _)| index)
    });
    for (index, limit) in limits.iter_mut().enumerate() {
        limit.headline = Some(index) == chosen;
    }
    limits
}

#[cfg(test)]
#[path = "usage_tests.rs"]
mod tests;
