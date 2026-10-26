use std::{path::PathBuf, time::Duration};

use tokio::sync::mpsc;
use uuid::Uuid;

use super::*;

async fn runs_start_line(shell: PathBuf, arguments: Vec<String>, line: &str, output: &[u8]) {
    let root = tempfile::tempdir().expect("temp cwd");
    let (changes, _changes) = mpsc::channel(128);
    let session = TerminalSession::spawn(
        Uuid::now_v7(),
        Uuid::now_v7(),
        shell,
        arguments,
        kodosi_os::fs::canonicalize(root.path()).expect("canonical cwd"),
        None,
        None,
        Vec::new(),
        changes,
    )
    .await
    .expect("shell");
    let mut subscription = session.subscribe().await.expect("subscribe");
    session.start(line.to_owned()).expect("start line");
    let mut bytes = Vec::new();
    let outcome = tokio::time::timeout(Duration::from_secs(20), async {
        while !bytes.windows(output.len()).any(|window| window == output) {
            let frame = subscription
                .data
                .recv()
                .await
                .expect("terminal data before the output");
            bytes.extend_from_slice(&frame.bytes);
        }
    })
    .await;
    session.close().await.expect("close");
    drop(session);
    assert!(
        outcome.is_ok(),
        "missing {:?}; received {:?}",
        String::from_utf8_lossy(output),
        String::from_utf8_lossy(&bytes)
    );
}

#[tokio::test]
async fn a_screen_that_a_console_starts_with_stays_in_its_history() {
    let cmd = std::env::var_os("ComSpec").map_or_else(|| PathBuf::from("cmd.exe"), PathBuf::from);
    let root = tempfile::tempdir().expect("temp cwd");
    let (changes, _changes) = mpsc::channel(128);
    let session = TerminalSession::spawn(
        Uuid::now_v7(),
        Uuid::now_v7(),
        cmd,
        Vec::new(),
        kodosi_os::fs::canonicalize(root.path()).expect("canonical cwd"),
        None,
        None,
        b"before quit\r\n".to_vec(),
        changes,
    )
    .await
    .expect("shell");
    session
        .start("set /a 6*7+1000".to_owned())
        .expect("start line");
    let history = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let history = session.read(true).await.expect("history");
            if history.contains("1042") {
                break history;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    let screen = session.read(false).await.expect("screen");
    session.close().await.expect("close");
    let history = history.expect("cmd output");
    let before = history.find("before quit").expect("saved screen");
    assert!(
        before < history.find("1042").expect("cmd output"),
        "{history}"
    );
    assert!(!screen.contains("before quit"), "{screen}");
}

#[tokio::test]
async fn a_start_line_runs_once_cmd_waits_at_its_prompt() {
    let cmd = std::env::var_os("ComSpec").map_or_else(|| PathBuf::from("cmd.exe"), PathBuf::from);
    runs_start_line(cmd, Vec::new(), "set /a 6*7+1000", b"1042").await;
}

#[tokio::test]
async fn a_start_line_runs_once_powershell_waits_at_its_prompt() {
    let powershell = kodosi_os::process::find_executable("pwsh")
        .or_else(|| kodosi_os::process::find_executable("powershell"))
        .expect("PowerShell");
    runs_start_line(
        powershell,
        vec!["-NoLogo".to_owned(), "-NoProfile".to_owned()],
        "'it''s ' + (6*7+1000)",
        b"it's 1042",
    )
    .await;
}
