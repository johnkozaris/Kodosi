use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const MAX_EXTERNAL_SESSION_ENTRIES: usize = 4_096;
const MAX_CLAUDE_SESSION_STATE_BYTES: u64 = 256 * 1024;
const MAX_COPILOT_HEAD_BYTES: u64 = 64 * 1024;
const MAX_COPILOT_TAIL_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExternalAgent {
    Claude,
    Copilot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LivenessStatus {
    Unknown,
    Alive,
    Dead,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalSession {
    pub agent: ExternalAgent,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,

    #[serde(default = "default_liveness")]
    pub liveness: LivenessStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub source_path: PathBuf,
}

pub async fn discover_external_sessions(
    home: &Path,
    agent_filter: Option<ExternalAgent>,
) -> Vec<ExternalSession> {
    let home = home.to_path_buf();
    tokio::task::spawn_blocking(move || discover_blocking(&home, agent_filter))
        .await
        .unwrap_or_default()
}

pub async fn ensure_resume_target_inactive(
    home: &Path,
    agent: ExternalAgent,
    session_id: &str,
    expected_cwd: &Path,
) -> Result<(), String> {
    let home = home.to_owned();
    let session_id = session_id.to_owned();
    let expected_cwd = expected_cwd.to_owned();
    tokio::task::spawn_blocking(move || match agent {
        ExternalAgent::Claude => ensure_claude_target_inactive(&home, &session_id, &expected_cwd),
        ExternalAgent::Copilot => ensure_copilot_target_inactive(&home, &session_id, &expected_cwd),
    })
    .await
    .map_err(|error| format!("provider liveness task failed: {error}"))?
}

fn ensure_claude_target_inactive(
    home: &Path,
    session_id: &str,
    expected_cwd: &Path,
) -> Result<(), String> {
    let sessions = home.join(".claude/sessions");
    let entries = match std::fs::read_dir(&sessions) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("could not inspect Claude liveness: {error}")),
    };
    for (index, entry) in entries.enumerate() {
        if index >= MAX_EXTERNAL_SESSION_ENTRIES {
            return Err("Claude liveness state exceeds its safety bound".to_owned());
        }
        let entry = entry.map_err(|error| format!("could not inspect Claude liveness: {error}"))?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let metadata = entry
            .metadata()
            .map_err(|error| format!("could not inspect Claude liveness: {error}"))?;
        if metadata.len() > MAX_CLAUDE_SESSION_STATE_BYTES {
            return Err("Claude liveness record exceeds its safety bound".to_owned());
        }
        let value: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&path)
                .map_err(|error| format!("could not read Claude liveness: {error}"))?,
        )
        .map_err(|error| format!("could not decode Claude liveness: {error}"))?;
        let candidate = value
            .get("sessionId")
            .or_else(|| value.get("session_id"))
            .and_then(serde_json::Value::as_str);
        if candidate != Some(session_id) {
            continue;
        }
        let cwd = value
            .get("cwd")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "Claude liveness record has no working directory".to_owned())?;
        let cwd = std::fs::canonicalize(cwd)
            .map_err(|_| "Claude liveness working directory is unavailable".to_owned())?;
        if cwd != expected_cwd {
            return Err("Claude liveness record belongs to another working directory".to_owned());
        }
        let pid = value
            .get("pid")
            .and_then(serde_json::Value::as_u64)
            .and_then(|pid| u32::try_from(pid).ok())
            .ok_or_else(|| "Claude conversation liveness is unknown".to_owned())?;
        match probe_pid_alive(pid) {
            LivenessStatus::Dead => {}
            LivenessStatus::Alive | LivenessStatus::Unknown => {
                return Err(
                    "Claude conversation appears to be open outside this session".to_owned(),
                );
            }
        }
    }
    Ok(())
}

fn ensure_copilot_target_inactive(
    home: &Path,
    session_id: &str,
    expected_cwd: &Path,
) -> Result<(), String> {
    let directory = home.join(".copilot/session-state").join(session_id);
    let metadata = std::fs::symlink_metadata(&directory)
        .map_err(|error| format!("could not inspect Copilot liveness: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Copilot liveness state must be a regular directory".to_owned());
    }
    let cwd = read_copilot_cwd(&directory)
        .ok_or_else(|| "Copilot conversation liveness has no working directory".to_owned())?;
    let cwd = std::fs::canonicalize(cwd)
        .map_err(|_| "Copilot liveness working directory is unavailable".to_owned())?;
    if cwd != expected_cwd {
        return Err("Copilot liveness record belongs to another working directory".to_owned());
    }
    match last_copilot_lifecycle(&directory)? {
        Some("session.shutdown") => Ok(()),
        Some("session.start") | None => {
            Err("Copilot conversation may still be open outside Kodosi".to_owned())
        }
        Some(_) => unreachable!("lifecycle helper returns only known tags"),
    }
}

fn last_copilot_lifecycle(session_dir: &Path) -> Result<Option<&'static str>, String> {
    use std::io::{Read, Seek, SeekFrom};

    let path = session_dir.join("events.jsonl");
    let mut file =
        std::fs::File::open(&path).map_err(|error| format!("open Copilot liveness: {error}"))?;
    let length = file
        .metadata()
        .map_err(|error| format!("inspect Copilot liveness: {error}"))?
        .len();
    let read_bytes = length.min(MAX_COPILOT_TAIL_BYTES);
    let start = length.saturating_sub(read_bytes);
    file.seek(SeekFrom::Start(start))
        .map_err(|error| format!("seek Copilot liveness: {error}"))?;
    let mut bytes = Vec::with_capacity(usize::try_from(read_bytes).unwrap_or(0));
    file.take(read_bytes)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read Copilot liveness: {error}"))?;
    let aligned = if start == 0 {
        bytes.as_slice()
    } else if let Some(newline) = bytes.iter().position(|byte| *byte == b'\n') {
        &bytes[newline + 1..]
    } else {
        return Err("Copilot liveness record exceeds its safety bound".to_owned());
    };
    let mut last = None;
    for line in aligned.split(|byte| *byte == b'\n') {
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(line) else {
            continue;
        };
        match value.get("type").and_then(serde_json::Value::as_str) {
            Some("session.start") => last = Some("session.start"),
            Some("session.shutdown") => last = Some("session.shutdown"),
            _ => {}
        }
    }
    Ok(last)
}

fn discover_blocking(home: &Path, agent_filter: Option<ExternalAgent>) -> Vec<ExternalSession> {
    let mut out = Vec::new();
    let want_claude = agent_filter.is_none_or(|f| f == ExternalAgent::Claude);
    let want_copilot = agent_filter.is_none_or(|f| f == ExternalAgent::Copilot);
    if want_claude {
        discover_claude(home, &mut out);
    }
    if want_copilot {
        discover_copilot(home, &mut out);
    }
    for entry in &mut out {
        entry.liveness = match entry.pid {
            Some(pid) => probe_pid_alive(pid),
            None => LivenessStatus::Unknown,
        };
    }
    out
}

const fn default_liveness() -> LivenessStatus {
    LivenessStatus::Unknown
}

fn probe_pid_alive(pid: u32) -> LivenessStatus {
    let Some(pid) = i32::try_from(pid)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    else {
        return LivenessStatus::Unknown;
    };
    match rustix::process::test_kill_process(pid) {
        Ok(()) => LivenessStatus::Alive,
        Err(rustix::io::Errno::SRCH) => LivenessStatus::Dead,
        Err(_) => LivenessStatus::Unknown,
    }
}

fn discover_claude(home: &Path, out: &mut Vec<ExternalSession>) {
    let sessions_dir = home.join(".claude").join("sessions");
    let Ok(entries) = std::fs::read_dir(&sessions_dir) else {
        return;
    };
    for entry in entries.flatten().take(MAX_EXTERNAL_SESSION_ENTRIES) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if entry
            .metadata()
            .is_ok_and(|metadata| metadata.len() > MAX_CLAUDE_SESSION_STATE_BYTES)
        {
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&contents) else {
            continue;
        };
        let session_id = value
            .get("sessionId")
            .or_else(|| value.get("session_id"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let Some(session_id) = session_id else {
            continue;
        };
        let pid = value
            .get("pid")
            .and_then(serde_json::Value::as_u64)
            .and_then(|v| u32::try_from(v).ok());
        let cwd = value
            .get("cwd")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let started_at = value
            .get("startedAt")
            .or_else(|| value.get("started_at"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let name = value
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        if pid.is_some_and(process_is_kodosi_owned) {
            continue;
        }
        out.push(ExternalSession {
            agent: ExternalAgent::Claude,
            session_id,
            pid,
            liveness: LivenessStatus::Unknown,
            cwd,
            started_at,
            name,
            source_path: path,
        });
    }
}

#[cfg(unix)]
fn process_is_kodosi_owned(pid: u32) -> bool {
    let output = std::process::Command::new("ps")
        .args(["eww", "-p", &pid.to_string(), "-o", "command="])
        .output();
    output
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .is_some_and(|command| command_line_marks_kodosi(&command))
}

#[cfg(not(unix))]
const fn process_is_kodosi_owned(_pid: u32) -> bool {
    false
}

fn command_line_marks_kodosi(command: &str) -> bool {
    command
        .split_whitespace()
        .filter_map(|token| token.strip_prefix("KODOSI_SESSION_ID="))
        .any(|value| !value.is_empty())
}

fn discover_copilot(home: &Path, out: &mut Vec<ExternalSession>) {
    let session_state = home.join(".copilot").join("session-state");
    let Ok(entries) = std::fs::read_dir(&session_state) else {
        return;
    };
    for entry in entries.flatten().take(MAX_EXTERNAL_SESSION_ENTRIES) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(session_id) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned) else {
            continue;
        };
        let cwd = read_copilot_cwd(&path);
        let (started_at, shutdown_seen) = read_copilot_first_and_last_events(&path);
        if shutdown_seen {
            continue;
        }
        out.push(ExternalSession {
            agent: ExternalAgent::Copilot,
            session_id,
            pid: None,
            liveness: LivenessStatus::Unknown,
            cwd,
            started_at,
            name: None,
            source_path: path,
        });
    }
}

fn read_copilot_cwd(session_dir: &Path) -> Option<String> {
    if let Ok(file) = std::fs::File::open(session_dir.join("events.jsonl"))
        && let Some(cwd) = read_session_start_cwd(file)
    {
        return Some(cwd);
    }

    let workspace = session_dir.join("workspace.yaml");
    if workspace.metadata().ok()?.len() > MAX_COPILOT_HEAD_BYTES {
        return None;
    }
    let raw = std::fs::read_to_string(&workspace).ok()?;
    for line in raw.lines().take(40) {
        if let Some(rest) = line.trim().strip_prefix("cwd:") {
            let cleaned = rest.trim().trim_matches('"').trim_matches('\'').to_owned();
            if !cleaned.is_empty() {
                return Some(cleaned);
            }
        }
    }
    None
}

fn read_session_start_cwd(file: std::fs::File) -> Option<String> {
    use std::io::{BufRead, BufReader, Read};
    let reader = BufReader::new(file.take(MAX_COPILOT_HEAD_BYTES));
    for line in reader.lines().take(50).map_while(Result::ok) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(serde_json::Value::as_str) == Some("session.start") {
            let cwd = value
                .get("data")
                .and_then(|d| d.get("context"))
                .and_then(|c| c.get("cwd"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
            if cwd.is_some() {
                return cwd;
            }
        }
    }
    None
}

fn read_copilot_first_and_last_events(session_dir: &Path) -> (Option<String>, bool) {
    use std::io::{BufRead, BufReader, Read};

    let start_at = std::fs::File::open(session_dir.join("events.jsonl"))
        .ok()
        .and_then(|file| {
            BufReader::new(file.take(MAX_COPILOT_HEAD_BYTES))
                .lines()
                .take(50)
                .map_while(Result::ok)
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(&line).ok())
                .find(|value| {
                    value.get("type").and_then(serde_json::Value::as_str) == Some("session.start")
                })
                .and_then(|value| {
                    value
                        .get("timestamp")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                })
        });
    let shutdown =
        last_copilot_lifecycle(session_dir).is_ok_and(|event| event == Some("session.shutdown"));
    (start_at, shutdown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Write as _};

    #[test]
    fn kodosi_process_markers_are_detected() {
        assert!(command_line_marks_kodosi(
            "KODOSI_SESSION_ID=abc claude --resume=def"
        ));
        assert!(!command_line_marks_kodosi("claude --session-id abc"));
        assert!(!command_line_marks_kodosi("claude --resume abc"));
        assert!(!command_line_marks_kodosi(
            "claude --prompt=KODOSI_SESSION_ID=abc"
        ));
        assert!(!command_line_marks_kodosi("claude --model sonnet"));
    }

    #[tokio::test]
    async fn discovers_claude_session_lockfile() {
        let temp = tempfile::tempdir().unwrap();
        let sessions = temp.path().join(".claude").join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        fs::write(
            sessions.join("abc.json"),
            r#"{
                "sessionId": "abc",
                "pid": 1234,
                "cwd": "/repo",
                "startedAt": "2026-05-16T10:00:00Z",
                "name": "research"
            }"#,
        )
        .unwrap();
        let found = discover_external_sessions(temp.path(), None).await;
        assert_eq!(found.len(), 1);
        let entry = &found[0];
        assert_eq!(entry.agent, ExternalAgent::Claude);
        assert_eq!(entry.session_id, "abc");
        assert_eq!(entry.pid, Some(1234));
        assert_eq!(entry.cwd.as_deref(), Some("/repo"));
        assert_eq!(entry.name.as_deref(), Some("research"));
    }

    #[tokio::test]
    async fn discovers_copilot_session_dir_with_cwd_from_events() {
        let temp = tempfile::tempdir().unwrap();
        let sid_dir = temp
            .path()
            .join(".copilot")
            .join("session-state")
            .join("uuid-1");
        fs::create_dir_all(&sid_dir).unwrap();
        fs::write(
            sid_dir.join("events.jsonl"),
            "{\"type\":\"session.start\",\"timestamp\":\"2026-05-16T10:00:00Z\",\"data\":{\"context\":{\"cwd\":\"/copilot-cwd\"}}}\n",
        )
        .unwrap();
        let found = discover_external_sessions(temp.path(), Some(ExternalAgent::Copilot)).await;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].session_id, "uuid-1");
        assert_eq!(found[0].cwd.as_deref(), Some("/copilot-cwd"));
        assert_eq!(found[0].started_at.as_deref(), Some("2026-05-16T10:00:00Z"));
    }

    #[tokio::test]
    async fn skips_copilot_sessions_with_shutdown_event() {
        let temp = tempfile::tempdir().unwrap();
        let sid_dir = temp
            .path()
            .join(".copilot")
            .join("session-state")
            .join("uuid-1");
        fs::create_dir_all(&sid_dir).unwrap();
        fs::write(
            sid_dir.join("events.jsonl"),
            "{\"type\":\"session.start\",\"data\":{\"context\":{\"cwd\":\"/c\"}}}\n\
             {\"type\":\"session.shutdown\"}\n",
        )
        .unwrap();
        let found = discover_external_sessions(temp.path(), Some(ExternalAgent::Copilot)).await;
        assert!(found.is_empty(), "shutdown sessions should be filtered out");
    }

    #[tokio::test]
    async fn exact_claude_liveness_blocks_a_matching_live_process() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = temp.path().join("repo");
        fs::create_dir_all(&cwd).unwrap();
        let cwd = cwd.canonicalize().unwrap();
        let sessions = temp.path().join(".claude/sessions");
        fs::create_dir_all(&sessions).unwrap();
        let session_id = "01900000-0000-4000-8000-000000000001";
        fs::write(
            sessions.join("state.json"),
            serde_json::to_vec(&serde_json::json!({
                "sessionId": session_id,
                "pid": std::process::id(),
                "cwd": cwd,
            }))
            .unwrap(),
        )
        .unwrap();

        assert!(
            ensure_resume_target_inactive(temp.path(), ExternalAgent::Claude, session_id, &cwd,)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn exact_copilot_liveness_requires_latest_shutdown() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = temp.path().join("repo");
        fs::create_dir_all(&cwd).unwrap();
        let cwd = cwd.canonicalize().unwrap();
        let session_id = "01900000-0000-4000-8000-000000000001";
        let state = temp.path().join(".copilot/session-state").join(session_id);
        fs::create_dir_all(&state).unwrap();
        let transcript = state.join("events.jsonl");
        fs::write(
            &transcript,
            format!(
                "{{\"type\":\"session.start\",\"data\":{{\"context\":{{\"cwd\":{}}}}}}}\n",
                serde_json::to_string(&cwd.to_string_lossy()).unwrap()
            ),
        )
        .unwrap();
        assert!(
            ensure_resume_target_inactive(temp.path(), ExternalAgent::Copilot, session_id, &cwd,)
                .await
                .is_err()
        );

        fs::OpenOptions::new()
            .append(true)
            .open(&transcript)
            .unwrap()
            .write_all(b"{\"type\":\"session.shutdown\"}\n")
            .unwrap();
        let result =
            ensure_resume_target_inactive(temp.path(), ExternalAgent::Copilot, session_id, &cwd)
                .await;
        assert!(result.is_ok(), "unexpected liveness result: {result:?}");
    }

    #[tokio::test]
    async fn copilot_falls_back_to_workspace_yaml_for_cwd() {
        let temp = tempfile::tempdir().unwrap();
        let sid_dir = temp
            .path()
            .join(".copilot")
            .join("session-state")
            .join("uuid-2");
        fs::create_dir_all(&sid_dir).unwrap();
        fs::write(
            sid_dir.join("workspace.yaml"),
            "cwd: /fallback-cwd\nother: x\n",
        )
        .unwrap();
        let found = discover_external_sessions(temp.path(), Some(ExternalAgent::Copilot)).await;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].cwd.as_deref(), Some("/fallback-cwd"));
    }

    #[tokio::test]
    async fn agent_filter_scopes_results() {
        let temp = tempfile::tempdir().unwrap();
        let sessions = temp.path().join(".claude").join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        fs::write(sessions.join("x.json"), r#"{"sessionId": "x"}"#).unwrap();
        let only_copilot =
            discover_external_sessions(temp.path(), Some(ExternalAgent::Copilot)).await;
        assert!(only_copilot.is_empty());
        let only_claude =
            discover_external_sessions(temp.path(), Some(ExternalAgent::Claude)).await;
        assert_eq!(only_claude.len(), 1);
    }
}
