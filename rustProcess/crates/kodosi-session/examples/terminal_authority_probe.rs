use std::{io::Write as _, process::Command, time::Instant};

use kodosi_session::{SessionTerminalHandle, TerminalHistoryPolicy};
use serde::Serialize;

const ROWS: u16 = 50;
const COLS: u16 = 200;

#[derive(Serialize)]
struct ProbeResult {
    schema: &'static str,
    source_revision: String,
    source_dirty: bool,
    host_os: &'static str,
    host_arch: &'static str,
    rss_provider: &'static str,
    profile: &'static str,
    workload: &'static str,
    sessions: usize,
    rows: u16,
    cols: u16,
    population_bytes_per_session: usize,
    latency_bytes_per_session: usize,
    total_applied_bytes_per_session: usize,
    rss_kib_before: u64,
    rss_kib_populated: u64,
    rss_kib_after_idle_compression: u64,
    rss_kib_after_shutdown: u64,
    checkpoint_json_bytes: Vec<usize>,
    presentation_json_bytes: Vec<usize>,
    presentation_plain_lines: Vec<usize>,
    process_output_latency_ns: Option<Latency>,
    checkpoint_latency_ns: Latency,
    presentation_latency_ns: Latency,
}

struct ProbeMeasurements {
    sessions: usize,
    empty_only: bool,
    population_bytes_per_session: usize,
    latency_bytes_per_session: usize,
    total_applied_bytes_per_session: usize,
    rss_kib_before: u64,
    rss_kib_populated: u64,
    rss_kib_after_idle_compression: u64,
    rss_kib_after_shutdown: u64,
    checkpoint_json_bytes: Vec<usize>,
    presentation_json_bytes: Vec<usize>,
    presentation_plain_lines: Vec<usize>,
    process_samples: Vec<u64>,
    checkpoint_samples: Vec<u64>,
    presentation_samples: Vec<u64>,
}

struct SnapshotMeasurements {
    checkpoint_json_bytes: Vec<usize>,
    presentation_json_bytes: Vec<usize>,
    presentation_plain_lines: Vec<usize>,
    checkpoint_samples: Vec<u64>,
    presentation_samples: Vec<u64>,
}

#[derive(Serialize)]
struct Latency {
    samples: Vec<u64>,
    p50: u64,
    p95: u64,
    p99: u64,
    max: u64,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let sessions = std::env::args()
        .nth(1)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1);
    let empty_only = std::env::args().nth(2).as_deref() == Some("empty");
    assert!(
        matches!(sessions, 1 | 4 | 16),
        "sessions must be 1, 4, or 16"
    );
    write_result(&build_result(measure(sessions, empty_only).await));
}

async fn measure(sessions: usize, empty_only: bool) -> ProbeMeasurements {
    let corpus = corpus();
    let rss_kib_before = rss_kib();
    let terminals = spawn_terminals(sessions);

    if !empty_only {
        for terminal in &terminals {
            terminal
                .process_output(bytes::Bytes::copy_from_slice(&corpus))
                .await
                .unwrap_or_else(|error| panic!("populate terminal: {error}"));
        }
    }
    let rss_kib_populated = rss_kib();
    tokio::time::sleep(
        TerminalHistoryPolicy::default().compression_idle + std::time::Duration::from_millis(250),
    )
    .await;
    let rss_kib_after_idle_compression = rss_kib();

    let latency_batch = bytes::Bytes::from(vec![b'x'; 4 * 1024]);
    let process_samples = measure_output_latency(&terminals, &latency_batch, empty_only).await;
    let snapshots = measure_snapshots(&terminals).await;

    for terminal in terminals {
        terminal
            .shutdown()
            .await
            .unwrap_or_else(|error| panic!("shutdown terminal actor: {error}"));
    }
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let population_bytes_per_session = if empty_only { 0 } else { corpus.len() };
    let latency_bytes_per_session = if empty_only {
        0
    } else {
        latency_batch.len() * 128
    };
    ProbeMeasurements {
        sessions,
        empty_only,
        population_bytes_per_session,
        latency_bytes_per_session,
        total_applied_bytes_per_session: population_bytes_per_session + latency_bytes_per_session,
        rss_kib_before,
        rss_kib_populated,
        rss_kib_after_idle_compression,
        rss_kib_after_shutdown: rss_kib(),
        checkpoint_json_bytes: snapshots.checkpoint_json_bytes,
        presentation_json_bytes: snapshots.presentation_json_bytes,
        presentation_plain_lines: snapshots.presentation_plain_lines,
        process_samples,
        checkpoint_samples: snapshots.checkpoint_samples,
        presentation_samples: snapshots.presentation_samples,
    }
}

fn spawn_terminals(sessions: usize) -> Vec<SessionTerminalHandle> {
    (0..sessions)
        .map(|_| {
            SessionTerminalHandle::spawn_with_theme(
                ROWS,
                COLS,
                0,
                false,
                TerminalHistoryPolicy::default(),
                true,
            )
            .unwrap_or_else(|error| panic!("spawn terminal actor: {error}"))
        })
        .collect()
}

async fn measure_output_latency(
    terminals: &[SessionTerminalHandle],
    latency_batch: &bytes::Bytes,
    empty_only: bool,
) -> Vec<u64> {
    let mut samples = Vec::with_capacity(terminals.len() * 128);
    if !empty_only {
        for terminal in terminals {
            for _ in 0..128 {
                let start = Instant::now();
                terminal
                    .process_output(latency_batch.clone())
                    .await
                    .unwrap_or_else(|error| panic!("measure terminal output: {error}"));
                samples.push(nanos(start.elapsed()));
            }
        }
    }
    samples
}

async fn measure_snapshots(terminals: &[SessionTerminalHandle]) -> SnapshotMeasurements {
    let capacity = terminals.len() * 8;
    let mut measurements = SnapshotMeasurements {
        checkpoint_json_bytes: Vec::with_capacity(capacity),
        presentation_json_bytes: Vec::with_capacity(capacity),
        presentation_plain_lines: Vec::with_capacity(capacity),
        checkpoint_samples: Vec::with_capacity(capacity),
        presentation_samples: Vec::with_capacity(capacity),
    };
    for terminal in terminals {
        for _ in 0..8 {
            let start = Instant::now();
            let checkpoint = terminal
                .checkpoint_data()
                .await
                .unwrap_or_else(|error| panic!("capture terminal checkpoint: {error}"));
            measurements.checkpoint_samples.push(nanos(start.elapsed()));
            measurements.checkpoint_json_bytes.push(
                serde_json::to_vec(&checkpoint.checkpoint)
                    .unwrap_or_else(|error| panic!("serialize checkpoint: {error}"))
                    .len(),
            );

            let start = Instant::now();
            let presentation = terminal
                .presentation_data()
                .await
                .unwrap_or_else(|error| panic!("capture terminal presentation: {error}"));
            measurements
                .presentation_samples
                .push(nanos(start.elapsed()));
            measurements.presentation_json_bytes.push(
                serde_json::to_vec(&presentation.presentation)
                    .unwrap_or_else(|error| panic!("serialize presentation: {error}"))
                    .len(),
            );
            measurements
                .presentation_plain_lines
                .push(presentation.presentation.plain_lines.len());
        }
    }
    measurements
}

fn write_result(result: &ProbeResult) {
    let json = serde_json::to_vec_pretty(result)
        .unwrap_or_else(|error| panic!("serialize process probe: {error}"));
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(&json)
        .and_then(|()| stdout.write_all(b"\n"))
        .unwrap_or_else(|error| panic!("write process probe: {error}"));
}

fn build_result(measurements: ProbeMeasurements) -> ProbeResult {
    let ProbeMeasurements {
        sessions,
        empty_only,
        population_bytes_per_session,
        latency_bytes_per_session,
        total_applied_bytes_per_session,
        rss_kib_before,
        rss_kib_populated,
        rss_kib_after_idle_compression,
        rss_kib_after_shutdown,
        checkpoint_json_bytes,
        presentation_json_bytes,
        presentation_plain_lines,
        process_samples,
        checkpoint_samples,
        presentation_samples,
    } = measurements;
    ProbeResult {
        schema: "kodosi-terminal-authority-process-post-v2",
        source_revision: command_output("git", &["rev-parse", "HEAD"]),
        source_dirty: !command_output("git", &["status", "--porcelain"]).is_empty(),
        host_os: std::env::consts::OS,
        host_arch: std::env::consts::ARCH,
        rss_provider: "sysinfo-process-memory-bytes/1024",
        profile: "release",
        workload: if empty_only { "empty" } else { "populated" },
        sessions,
        rows: ROWS,
        cols: COLS,
        population_bytes_per_session,
        latency_bytes_per_session,
        total_applied_bytes_per_session,
        rss_kib_before,
        rss_kib_populated,
        rss_kib_after_idle_compression,
        rss_kib_after_shutdown,
        checkpoint_json_bytes,
        presentation_json_bytes,
        presentation_plain_lines,
        process_output_latency_ns: (!process_samples.is_empty())
            .then(|| Latency::new(process_samples)),
        checkpoint_latency_ns: Latency::new(checkpoint_samples),
        presentation_latency_ns: Latency::new(presentation_samples),
    }
}

impl Latency {
    fn new(mut samples: Vec<u64>) -> Self {
        samples.sort_unstable();
        let p50 = percentile(&samples, 50);
        let p95 = percentile(&samples, 95);
        let p99 = percentile(&samples, 99);
        let max = samples.last().copied().unwrap_or(0);
        Self {
            samples,
            p50,
            p95,
            p99,
            max,
        }
    }
}

fn percentile(samples: &[u64], percentile: usize) -> u64 {
    if samples.is_empty() {
        return 0;
    }
    let index = (samples.len() - 1).saturating_mul(percentile).div_ceil(100);
    samples[index.min(samples.len() - 1)]
}

fn nanos(duration: std::time::Duration) -> u64 {
    duration.as_nanos().try_into().unwrap_or(u64::MAX)
}

fn corpus() -> Vec<u8> {
    let mut output = Vec::with_capacity(512 * 1024);
    let unicode = "e\u{301} 👨‍👩‍👧‍👦 终端 नमस्ते";
    for index in 0..4_000 {
        output.extend_from_slice(
            format!(
                "\x1b[38;2;{};{};{}m[{index:04}] {unicode} terminal authority history payload\x1b[0m\r\n",
                index % 256,
                (index * 7) % 256,
                (index * 13) % 256,
            )
            .as_bytes(),
        );
    }
    output
}

fn rss_kib() -> u64 {
    let mut system = sysinfo::System::new();
    let pid = sysinfo::Pid::from_u32(std::process::id());
    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::Some(&[pid]),
        false,
        sysinfo::ProcessRefreshKind::nothing().with_memory(),
    );
    system.process(pid).map_or_else(
        || panic!("benchmark process {pid} disappeared"),
        sysinfo::Process::memory,
    ) / 1024
}

fn command_output(command: &str, args: &[&str]) -> String {
    let output = Command::new(command)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("run {command}: {error}"));
    assert!(output.status.success(), "{command} failed");
    String::from_utf8(output.stdout)
        .unwrap_or_else(|error| panic!("decode {command} output: {error}"))
        .trim()
        .to_owned()
}
