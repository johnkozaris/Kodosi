use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;
use tokio::io::AsyncReadExt;

const QUERY_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_OUTPUT_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeAgentRecord {
    pid: u32,
    cwd: String,
    session_id: String,
    name: String,
    status: String,
}

pub(crate) async fn query(cwd: &str) -> agent_intel::AgentIntelSnapshot {
    match query_records(cwd).await {
        Ok(records) if records.len() == 1 => snapshot_from_record(&records[0]),
        Ok(records) if records.is_empty() => degraded_snapshot(
            cwd,
            "Claude is running, but `claude agents --json` did not report this session",
        ),
        Ok(records) => degraded_snapshot(
            cwd,
            &format!(
                "`claude agents --json` reported {} sessions for this workspace; exact identity is ambiguous",
                records.len()
            ),
        ),
        Err(reason) => degraded_snapshot(cwd, &reason),
    }
}

async fn query_records(cwd: &str) -> Result<Vec<ClaudeAgentRecord>, String> {
    let mut child = tokio::process::Command::new("claude")
        .args(["agents", "--json", "--cwd", cwd])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("Claude agent discovery is unavailable: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Claude agent discovery did not expose stdout".to_owned())?;
    let read = async {
        let mut bytes = Vec::new();
        stdout
            .take(MAX_OUTPUT_BYTES + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|error| format!("Could not read Claude agent discovery: {error}"))?;
        let status = child
            .wait()
            .await
            .map_err(|error| format!("Could not wait for Claude agent discovery: {error}"))?;
        if !status.success() {
            return Err(format!(
                "Claude agent discovery exited with {}",
                status
                    .code()
                    .map_or_else(|| "a signal".to_owned(), |code| code.to_string())
            ));
        }
        if bytes.len() as u64 > MAX_OUTPUT_BYTES {
            return Err("Claude agent discovery exceeded its output limit".to_owned());
        }
        serde_json::from_slice(&bytes)
            .map_err(|error| format!("Claude agent discovery returned invalid JSON: {error}"))
    };
    tokio::time::timeout(QUERY_TIMEOUT, read)
        .await
        .map_err(|_| "Claude agent discovery timed out".to_owned())?
}

fn snapshot_from_record(record: &ClaudeAgentRecord) -> agent_intel::AgentIntelSnapshot {
    agent_intel::AgentIntelSnapshot {
        identity: agent_intel::domain::LiveAgentIdentity {
            agent_type: "claude".to_owned(),
            version: None,
            model: None,
            title: nonempty(&record.name),
            cwd: nonempty(&record.cwd),
            vendor_session_id: nonempty(&record.session_id),
            process_id: Some(record.pid),
        },
        lifecycle: lifecycle(&record.status),
        attention: None,
        pending_interaction: None,
        current_activity: None,
        workers: agent_intel::domain::ChildAgentSummary::default(),
        outcome: None,
        exceptional_state: None,
        source: agent_intel::domain::AgentSource {
            kind: agent_intel::domain::AgentSourceKind::Command,
            degraded: false,
            detail: None,
        },
    }
}

fn degraded_snapshot(cwd: &str, detail: &str) -> agent_intel::AgentIntelSnapshot {
    agent_intel::AgentIntelSnapshot {
        identity: agent_intel::domain::LiveAgentIdentity {
            agent_type: "claude".to_owned(),
            version: None,
            model: None,
            title: None,
            cwd: nonempty(cwd),
            vendor_session_id: None,
            process_id: None,
        },
        lifecycle: agent_intel::domain::AgentLifecycle::Starting,
        attention: None,
        pending_interaction: None,
        current_activity: None,
        workers: agent_intel::domain::ChildAgentSummary::default(),
        outcome: None,
        exceptional_state: None,
        source: agent_intel::domain::AgentSource {
            kind: agent_intel::domain::AgentSourceKind::Runtime,
            degraded: true,
            detail: Some(detail.chars().take(240).collect()),
        },
    }
}

fn lifecycle(status: &str) -> agent_intel::domain::AgentLifecycle {
    match status {
        "busy" | "working" | "running" => agent_intel::domain::AgentLifecycle::Working,
        "waiting" | "blocked" | "needs_input" => agent_intel::domain::AgentLifecycle::Waiting,
        "completed" | "done" => agent_intel::domain::AgentLifecycle::Completed,
        "failed" | "error" => agent_intel::domain::AgentLifecycle::Failed,
        "stopped" => agent_intel::domain::AgentLifecycle::Stopped,
        "offline" => agent_intel::domain::AgentLifecycle::Offline,
        _ => agent_intel::domain::AgentLifecycle::Idle,
    }
}

fn nonempty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.chars().take(1_024).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_record_maps_to_bounded_live_state() {
        let record: ClaudeAgentRecord = serde_json::from_value(serde_json::json!({
            "pid": 42,
            "cwd": "/repo",
            "kind": "interactive",
            "startedAt": 123,
            "sessionId": "session",
            "name": "worker",
            "status": "busy"
        }))
        .expect("record");

        let snapshot = snapshot_from_record(&record);

        assert_eq!(
            snapshot.lifecycle,
            agent_intel::domain::AgentLifecycle::Working
        );
        assert_eq!(snapshot.identity.title.as_deref(), Some("worker"));
        assert_eq!(
            snapshot.source.kind,
            agent_intel::domain::AgentSourceKind::Command
        );
        assert!(!snapshot.source.degraded);
    }

    #[test]
    fn unknown_status_does_not_claim_activity() {
        assert_eq!(
            lifecycle("future"),
            agent_intel::domain::AgentLifecycle::Idle
        );
    }
}
