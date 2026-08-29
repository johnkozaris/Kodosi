use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;

const TTL: Duration = Duration::from_hours(1);

const SPAWN_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_VERSION_OUTPUT_BYTES: u64 = 64 * 1024;
const PROCESS_REAP_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Clone)]
struct Entry {
    cached_at: Instant,
    version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ProbeKey {
    binary: String,
    args: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VersionProbe {
    binary: &'static str,
    args: &'static [&'static str],
}

const CLAUDE_PROBES: &[VersionProbe] = &[VersionProbe {
    binary: "claude",
    args: &["--version"],
}];
const COPILOT_PROBES: &[VersionProbe] = &[
    VersionProbe {
        binary: "copilot",
        args: &["--version"],
    },
    VersionProbe {
        binary: "gh",
        args: &["copilot", "--version"],
    },
];

static CACHE: LazyLock<Mutex<HashMap<ProbeKey, Entry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

async fn query_cli_version_args(binary: &str, args: &[&str]) -> Option<String> {
    if let Some(cached) = look_up_cache(binary, args) {
        return cached;
    }
    let fresh = spawn_version(binary, args, SPAWN_TIMEOUT).await;
    insert_cache(binary, args, fresh.clone());
    fresh
}

pub async fn query_install_version(agent: kodosi_session::AgentKind) -> Option<String> {
    for probe in install_version_probes(agent) {
        if let Some(version) = query_cli_version_args(probe.binary, probe.args).await {
            return Some(version);
        }
    }
    None
}

const fn install_version_probes(agent: kodosi_session::AgentKind) -> &'static [VersionProbe] {
    match agent {
        kodosi_session::AgentKind::Claude => CLAUDE_PROBES,
        kodosi_session::AgentKind::Copilot => COPILOT_PROBES,
    }
}

type CacheLookup = Option<Option<String>>;

fn look_up_cache(binary: &str, args: &[&str]) -> CacheLookup {
    let guard = CACHE.lock().ok()?;
    let entry = guard.get(&probe_key(binary, args))?;
    let result = if entry.cached_at.elapsed() < TTL {
        Some(entry.version.clone())
    } else {
        None
    };

    drop(guard);
    result
}

fn insert_cache(binary: &str, args: &[&str], version: Option<String>) {
    if let Ok(mut guard) = CACHE.lock() {
        guard.insert(
            probe_key(binary, args),
            Entry {
                cached_at: Instant::now(),
                version,
            },
        );
    }
}

fn probe_key(binary: &str, args: &[&str]) -> ProbeKey {
    ProbeKey {
        binary: binary.to_owned(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
    }
}

async fn spawn_version(binary: &str, args: &[&str], timeout: Duration) -> Option<String> {
    let executable = crate::runtime::executable::resolve(binary)?;
    let mut command = tokio::process::Command::new(executable);
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            tracing::debug!(binary, %err, "version probe: spawn failed");
            return None;
        }
    };
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_task = tokio::spawn(async move {
        let mut output = Vec::new();
        if let Some(stdout) = stdout {
            drop(
                stdout
                    .take(MAX_VERSION_OUTPUT_BYTES)
                    .read_to_end(&mut output)
                    .await,
            );
        }
        output
    });
    let stderr_task = tokio::spawn(async move {
        let mut output = Vec::new();
        if let Some(stderr) = stderr {
            drop(
                stderr
                    .take(MAX_VERSION_OUTPUT_BYTES)
                    .read_to_end(&mut output)
                    .await,
            );
        }
        output
    });
    let status = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(err)) => {
            tracing::debug!(binary, %err, "version probe: wait failed");
            terminate_process_tree(&mut child, binary).await;
            reap_child(&mut child, binary).await;
            drain_output_tasks(stdout_task, stderr_task).await;
            return None;
        }
        Err(_) => {
            tracing::debug!(binary, "version probe: timed out");
            terminate_process_tree(&mut child, binary).await;
            reap_child(&mut child, binary).await;
            drain_output_tasks(stdout_task, stderr_task).await;
            return None;
        }
    };

    let (stdout, _stderr) =
        tokio::join!(join_output_task(stdout_task), join_output_task(stderr_task));
    if !status.success() {
        tracing::debug!(
            binary,
            code = ?status.code(),
            "version probe: non-zero exit"
        );
        return None;
    }

    let stdout = String::from_utf8_lossy(&stdout);
    extract_version(&stdout)
}

async fn reap_child(child: &mut tokio::process::Child, binary: &str) {
    if tokio::time::timeout(PROCESS_REAP_TIMEOUT, child.wait())
        .await
        .is_err()
    {
        tracing::warn!(
            binary,
            "version probe: child could not be reaped after kill"
        );
    }
}

async fn drain_output_tasks(
    stdout_task: tokio::task::JoinHandle<Vec<u8>>,
    stderr_task: tokio::task::JoinHandle<Vec<u8>>,
) {
    drop(tokio::join!(
        join_output_task(stdout_task),
        join_output_task(stderr_task)
    ));
}

async fn join_output_task(mut task: tokio::task::JoinHandle<Vec<u8>>) -> Vec<u8> {
    match tokio::time::timeout(PROCESS_REAP_TIMEOUT, &mut task).await {
        Ok(Ok(output)) => output,
        Ok(Err(_)) => Vec::new(),
        Err(_) => {
            task.abort();
            Vec::new()
        }
    }
}

async fn terminate_process_tree(child: &mut tokio::process::Child, binary: &str) {
    if let Some(pid) = child.id()
        && let Ok(pid) = i32::try_from(pid)
    {
        let group = format!("-{pid}");
        let mut command = tokio::process::Command::new("kill");
        command
            .args(["-KILL", group.as_str()])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        let kill = command.status();
        if tokio::time::timeout(PROCESS_REAP_TIMEOUT, kill)
            .await
            .is_err()
        {
            tracing::debug!(binary, "version probe: process-group kill timed out");
        }
    }
    drop(child.start_kill());
}

fn extract_version(stdout: &str) -> Option<String> {
    let line = stdout.lines().map(str::trim).find(|l| !l.is_empty())?;
    line.split_whitespace()
        .map(|t| t.trim_matches(['(', ')', ',', '.', ';', '[', ']']))
        .find(|t| t.starts_with(|c: char| c.is_ascii_digit()) && t.contains('.'))
        .filter(|t| !t.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_claude_code_version_first_token() {
        assert_eq!(
            extract_version("2.1.143 (Claude Code)\n"),
            Some("2.1.143".to_owned())
        );
    }

    #[test]
    fn extracts_copilot_version_last_token() {
        assert_eq!(
            extract_version("GitHub Copilot CLI 1.0.49-1.\n"),
            Some("1.0.49-1".to_owned())
        );
    }

    #[test]
    fn extracts_claude_style_version() {
        assert_eq!(extract_version("claude 2.0.0\n"), Some("2.0.0".to_owned()));
    }

    #[test]
    fn extracts_gh_copilot_style_version() {
        assert_eq!(
            extract_version("gh copilot version 1.5.0\n"),
            Some("1.5.0".to_owned())
        );
    }

    #[test]
    fn empty_output_yields_none() {
        assert_eq!(extract_version(""), None);
        assert_eq!(extract_version("\n\n"), None);
    }

    #[test]
    fn strips_trailing_punctuation() {
        assert_eq!(
            extract_version("version 2.0.0,\n"),
            Some("2.0.0".to_owned())
        );
    }

    #[test]
    fn ignores_non_version_words_without_digits() {
        assert!(
            !extract_version("2.1.143 (Claude Code)")
                .unwrap()
                .contains("Code")
        );
    }

    #[tokio::test]
    async fn missing_binary_yields_none() {
        let v =
            query_cli_version_args("kodosi-this-binary-does-not-exist-xyzzy", &["--version"]).await;
        assert_eq!(v, None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn timed_out_probe_is_killed_and_reaped() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("slow-version");
        let marker = dir.path().join("survived");
        std::fs::write(&script, "#!/bin/sh\nsleep 0.2\nprintf survived > \"$1\"\n").unwrap();
        let mut permissions = std::fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&script, permissions).unwrap();

        assert_eq!(
            spawn_version(
                script.to_str().unwrap(),
                &[marker.to_str().unwrap()],
                Duration::from_millis(20),
            )
            .await,
            None
        );
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(
            !marker.exists(),
            "timed-out child must not survive to execute later output"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn descendant_holding_output_pipe_cannot_extend_probe_timeout() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("tree-version");
        std::fs::write(&script, "#!/bin/sh\n(sleep 5) &\nsleep 5\n").unwrap();
        let mut permissions = std::fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&script, permissions).unwrap();
        let started = Instant::now();
        assert_eq!(
            spawn_version(script.to_str().unwrap(), &[], Duration::from_millis(20),).await,
            None
        );
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "tree kill and bounded reader joins must return promptly"
        );
    }

    #[test]
    fn cache_key_includes_complete_argument_vector() {
        let binary = "__cache_key_includes_complete_argument_vector__";
        insert_cache(binary, &["--version"], Some("2.90.0".to_owned()));
        insert_cache(binary, &["copilot", "--version"], Some("1.5.0".to_owned()));
        assert_eq!(
            look_up_cache(binary, &["--version"]),
            Some(Some("2.90.0".to_owned()))
        );
        assert_eq!(
            look_up_cache(binary, &["copilot", "--version"]),
            Some(Some("1.5.0".to_owned()))
        );
    }

    #[test]
    fn legacy_copilot_fallback_invokes_gh_copilot_version() {
        assert_eq!(
            install_version_probes(kodosi_session::AgentKind::Copilot),
            [
                VersionProbe {
                    binary: "copilot",
                    args: &["--version"],
                },
                VersionProbe {
                    binary: "gh",
                    args: &["copilot", "--version"],
                },
            ]
        );
    }
}
