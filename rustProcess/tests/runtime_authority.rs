#![allow(clippy::expect_used)]

use std::{
    fs,
    io::Write as _,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const WORKER_ROOT: &str = "KODOSI_RUNTIME_AUTHORITY_TEST_ROOT";
const WORKER_ROLE: &str = "KODOSI_RUNTIME_AUTHORITY_TEST_ROLE";
const TEST_NAME: &str = "production_runtime_startup_preserves_future_store_and_fences_processes";

struct HolderGuard {
    child: Option<std::process::Child>,
    stop_path: std::path::PathBuf,
}

impl HolderGuard {
    fn new(child: std::process::Child, stop_path: std::path::PathBuf) -> Self {
        Self {
            child: Some(child),
            stop_path,
        }
    }

    fn child_mut(&mut self) -> &mut std::process::Child {
        self.child.as_mut().expect("holder child")
    }

    fn stop_and_wait(mut self) -> std::process::ExitStatus {
        fs::write(&self.stop_path, b"stop").expect("stop marker");
        let status = wait_or_kill(self.child_mut());
        self.child.take();
        status
    }
}

impl Drop for HolderGuard {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            drop(fs::write(&self.stop_path, b"stop"));
            let _ = wait_or_kill(child);
        }
    }
}

fn wait_or_kill(child: &mut std::process::Child) -> std::process::ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().expect("holder status probe") {
            return status;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    drop(child.kill());
    child.wait().expect("reap holder after kill")
}

#[test]
fn production_runtime_startup_preserves_future_store_and_fences_processes() {
    let Ok(root) = std::env::var(WORKER_ROOT) else {
        run_parent();
        return;
    };
    let role = std::env::var(WORKER_ROLE).expect("worker role");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Tokio runtime");
    runtime.block_on(async move {
        match role.as_str() {
            "holder" => run_holder(Path::new(&root)).await,
            "contender" => run_contender(Path::new(&root)).await,
            other => panic!("unknown worker role {other}"),
        }
    });
}

fn run_parent() {
    let root = tempfile::tempdir().expect("isolated root");
    let canonical_root = root.path().canonicalize().expect("canonical isolated root");
    let core = canonical_root.join("core");
    fs::create_dir_all(&core).expect("core directory");
    let store_path = core.join("collaboration-teardown-obligations.json");
    let future_payload = br#"{"version":999,"records":[{"unknown":"preserve exactly"}]}"#;
    fs::write(&store_path, future_payload).expect("future store");

    let mut holder = HolderGuard::new(
        spawn_worker(&canonical_root, "holder"),
        canonical_root.join("holder-stop"),
    );
    wait_for_file(&canonical_root.join("holder-ready"), holder.child_mut());

    let contender = spawn_worker(&canonical_root, "contender")
        .wait_with_output()
        .expect("contender output");
    assert!(
        contender.status.success(),
        "contender failed unexpectedly: {}",
        String::from_utf8_lossy(&contender.stderr)
    );
    assert!(canonical_root.join("contender-denied").is_file());
    assert_eq!(
        fs::read(&store_path).expect("future store after contender"),
        future_payload
    );

    let status = holder.stop_and_wait();
    assert!(status.success(), "holder failed: {status}");
    assert_eq!(
        fs::read(store_path).expect("future store after shutdown"),
        future_payload
    );
}

fn spawn_worker(root: &Path, role: &str) -> std::process::Child {
    let executable = std::env::current_exe().expect("test executable");
    let runtime_dir = root.join("runtime");
    fs::create_dir_all(&runtime_dir).expect("runtime directory");
    Command::new(executable)
        .args(["--exact", TEST_NAME, "--nocapture"])
        .env(WORKER_ROOT, root)
        .env(WORKER_ROLE, role)
        .env("KODOSI_DATA_ROOT", root)
        .env("KODOSI_PRODUCTION_DATA_ROOT", root.join("production"))
        .env("XDG_RUNTIME_DIR", &runtime_dir)
        .env("KODOSI__BACKEND__API", "http://127.0.0.1:9")
        .env("KODOSI__BACKEND__HOST_RELAY", "ws://127.0.0.1:9")
        .env("KODOSI__BACKEND__VIEWER_RELAY", "ws://127.0.0.1:9")
        .env("KODOSI__BACKEND__USER_EVENTS", "ws://127.0.0.1:9")
        .env("KODOSI__AUTH__ISSUER", "http://127.0.0.1:9")
        .env(
            "KODOSI__AUTH__KEYRING_SERVICE",
            "com.kodosi.runtime-authority-test",
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn worker")
}

fn wait_for_file(path: &Path, child: &mut std::process::Child) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if path.is_file() {
            return;
        }
        if let Some(status) = child.try_wait().expect("holder status probe") {
            let mut stderr = String::new();
            if let Some(mut pipe) = child.stderr.take() {
                use std::io::Read as _;
                pipe.read_to_string(&mut stderr).expect("holder stderr");
            }
            panic!("holder exited before readiness ({status}): {stderr}");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    drop(child.kill());
    drop(child.wait());
    panic!("holder did not prove local runtime readiness");
}

async fn run_holder(root: &Path) {
    let mut runtime = kodosi_runtime::start_embedded_runtime()
        .await
        .expect("production embedded runtime should start with a future cleanup schema");
    let mut events = runtime.take_events().expect("runtime event receivers");
    runtime
        .command_sink()
        .try_send_session_scoped(kodosi_runtime::SessionCommand::SnapshotRefresh)
        .expect("local snapshot command");
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if matches!(
                events.sessions_rx.recv().await,
                Some(kodosi_runtime::AccountContextEvent {
                    event: kodosi_runtime::SessionEvent::List { .. },
                    ..
                })
            ) {
                break;
            }
        }
    })
    .await
    .expect("local session snapshot response");
    fs::write(root.join("holder-ready"), b"ready").expect("ready marker");

    while !root.join("holder-stop").is_file() {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    runtime.shutdown();
    runtime.join().await.expect("graceful runtime shutdown");
}

async fn run_contender(root: &Path) {
    let error = match kodosi_runtime::start_embedded_runtime().await {
        Ok(runtime) => {
            runtime.shutdown();
            runtime.join().await.expect("unexpected runtime shutdown");
            panic!("a second process acquired the same data-root authority")
        }
        Err(error) => error,
    };
    assert!(
        matches!(
            error,
            kodosi_runtime::AppError::Io(ref source)
                if source.kind() == std::io::ErrorKind::AlreadyExists
        ),
        "unexpected contender error: {error:?}"
    );
    fs::write(root.join("contender-denied"), b"denied").expect("denied marker");
    std::io::stderr().flush().expect("flush contender stderr");
}
