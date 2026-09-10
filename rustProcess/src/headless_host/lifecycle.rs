use std::{path::Path, process::Stdio, time::Duration};

use serde::Serialize;
use tokio::io::{ReadHalf, WriteHalf};
use tokio::time;

use crate::{AppError, Result, SystemCommand, shutdown, support::io::framed_json};

use super::{
    client::HeadlessHostClient,
    handshake::{
        ConnectionLane, ControlClientFrame, HOST_PROTOCOL_VERSION, HostHelloRequest,
        HostHelloResponse, TerminalLaneCapability,
    },
    local_endpoint::{
        LocalStream, connect_local, connect_local_retrying, host_lock_is_free,
        is_connection_missing, is_not_a_socket,
    },
    state_file::{
        HostStateFile, cleanup_host_state_file_if_matches, host_runtime_dir,
        load_host_state_file_in,
    },
};

fn require_runtime_dir() -> Result<std::path::PathBuf> {
    host_runtime_dir().ok_or_else(|| AppError::Unsupported {
        reason: "headless host runtime directory is unavailable".to_owned(),
    })
}

const HOST_START_TIMEOUT: Duration = Duration::from_secs(10);
const HOST_START_POLL_INTERVAL: Duration = Duration::from_millis(100);

const HOST_START_SPAWN_ATTEMPTS: usize = 3;

const CONTROL_HELLO_TIMEOUT: Duration = Duration::from_secs(5);

const TERMINAL_HELLO_TIMEOUT: Duration = Duration::from_secs(15);

const HANDSHAKE_ATTEMPTS: usize = 3;

const HOST_STOP_POLL_INTERVAL: Duration = Duration::from_millis(50);

pub(super) const INTERNAL_HOST_SERVE_COMMAND: &str = "__internal-host-serve";

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RunningHostInfo {
    pub(crate) pid: u32,
    pub(crate) started_at: String,
}

pub(crate) struct TerminalLaneConnection {
    pub(crate) reader: framed_json::FramedJsonReader<ReadHalf<LocalStream>>,
    pub(crate) writer: framed_json::FramedJsonWriter<WriteHalf<LocalStream>>,
    capability: TerminalLaneCapability,
}

impl TerminalLaneConnection {
    pub(crate) const fn can_write(&self) -> bool {
        self.capability.can_write()
    }
}

#[derive(Debug)]
pub(in crate::headless_host) enum HostConnectOutcome {
    Connected(Box<HeadlessHostClient>),

    Gone,

    Silent,

    Rejected(String),

    Incompatible { server_version: u8, message: String },
}

pub(in crate::headless_host) fn classify_rejection(
    response: &HostHelloResponse,
) -> HostConnectOutcome {
    let message = response
        .message
        .clone()
        .unwrap_or_else(|| "host rejected the connection".to_owned());
    if response.server_version == HOST_PROTOCOL_VERSION {
        return HostConnectOutcome::Rejected(message);
    }
    HostConnectOutcome::Incompatible {
        server_version: response.server_version,
        message,
    }
}

async fn control_handshake(
    runtime_dir: &Path,
    state: &HostStateFile,
) -> Result<HostConnectOutcome> {
    let socket_path = Path::new(&state.socket_path);
    let stream = match connect_local_retrying(socket_path).await {
        Ok(stream) => stream,
        Err(error) => {
            return classify_control_connect_error(runtime_dir, state, socket_path, error);
        }
    };

    let mut framed = framed_json::framed(stream);
    if let Err(error) = framed_json::send_json(
        &mut framed,
        &HostHelloRequest {
            token: state.token.clone(),
            protocol_version: HOST_PROTOCOL_VERSION,
            lane: ConnectionLane::Control,
        },
    )
    .await
    {
        return classify_control_handshake_error(runtime_dir, state, error);
    }

    let response = match time::timeout(
        CONTROL_HELLO_TIMEOUT,
        framed_json::next_json::<_, HostHelloResponse>(&mut framed),
    )
    .await
    {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => return classify_control_handshake_error(runtime_dir, state, error),
        Err(_elapsed) => return Ok(HostConnectOutcome::Silent),
    };

    match response {
        Some(response @ HostHelloResponse { accepted: true, .. }) => {
            let account_user_id = response
                .account_user_id
                .as_deref()
                .map(kodosi_domain::ids::UserId::try_from)
                .transpose()
                .map_err(|error| AppError::InvalidBackendData {
                    field: "hostHello.accountUserId".to_owned(),
                    reason: error.to_string(),
                })?;
            Ok(HostConnectOutcome::Connected(Box::new(
                HeadlessHostClient {
                    framed,
                    account_user_id,
                    account_epoch: response.account_epoch.unwrap_or(0),
                    remote_operations_ready: response.remote_operations_ready.unwrap_or(false),
                    pending_events: std::collections::VecDeque::new(),
                },
            )))
        }

        Some(ref rejection) => Ok(classify_rejection(rejection)),
        None => {
            cleanup_host_state_file_if_matches(runtime_dir, state);
            Ok(HostConnectOutcome::Gone)
        }
    }
}

fn classify_control_handshake_error(
    runtime_dir: &Path,
    state: &HostStateFile,
    error: AppError,
) -> Result<HostConnectOutcome> {
    let AppError::Io(ref io_error) = error else {
        return Err(error);
    };
    if matches!(
        io_error.kind(),
        std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::NotConnected
            | std::io::ErrorKind::UnexpectedEof
    ) {
        cleanup_host_state_file_if_matches(runtime_dir, state);
        return Ok(HostConnectOutcome::Gone);
    }
    Err(error)
}

pub(in crate::headless_host) fn classify_control_connect_error(
    runtime_dir: &Path,
    state: &HostStateFile,
    socket_path: &Path,
    error: std::io::Error,
) -> Result<HostConnectOutcome> {
    if error.kind() == std::io::ErrorKind::TimedOut {
        return Ok(HostConnectOutcome::Silent);
    }
    if is_connection_missing(&error) || is_not_a_socket(socket_path) {
        cleanup_host_state_file_if_matches(runtime_dir, state);
        return Ok(HostConnectOutcome::Gone);
    }
    Err(AppError::Io(error))
}

pub(in crate::headless_host) fn ensure_matching_config(state: &HostStateFile) -> Result<()> {
    ensure_config_identity_matches(
        state.config_identity.as_deref(),
        &crate::config::configuration_identity()?,
    )
}

fn ensure_config_identity_matches(recorded: Option<&str>, current: &str) -> Result<()> {
    if recorded == Some(current) {
        return Ok(());
    }
    Err(AppError::Unsupported {
        reason: "the running headless host uses a different configuration; run `kodosi host stop` before using a different --config or KODOSI__ override"
            .to_owned(),
    })
}

pub(in crate::headless_host) async fn connect_control_lane(
    runtime_dir: &Path,
) -> Result<HostConnectOutcome> {
    for attempt in 1..=HANDSHAKE_ATTEMPTS {
        let Some(state) = load_host_state_file_in(runtime_dir)? else {
            return Ok(HostConnectOutcome::Gone);
        };

        let outcome = control_handshake(runtime_dir, &state).await?;
        if matches!(outcome, HostConnectOutcome::Connected(_)) {
            ensure_matching_config(&state)?;
        }

        let (HostConnectOutcome::Rejected(ref message)
        | HostConnectOutcome::Incompatible { ref message, .. }) = outcome
        else {
            return Ok(outcome);
        };
        if attempt == HANDSHAKE_ATTEMPTS {
            return Ok(outcome);
        }

        match load_host_state_file_in(runtime_dir)? {
            Some(current) if current.token != state.token => {
                tracing::debug!(
                    message,
                    "host token changed mid-handshake; retrying with the published one"
                );
            }
            _ => return Ok(outcome),
        }
    }

    Ok(HostConnectOutcome::Gone)
}

fn silent_host_error() -> AppError {
    AppError::Unsupported {
        reason: format!(
            "the headless host socket accepted a connection but did not answer the handshake \
             within {}s; the host is shutting down or wedged (run `kodosi host stop`)",
            CONTROL_HELLO_TIMEOUT.as_secs()
        ),
    }
}

fn incompatible_host_error(server_version: u8, message: &str) -> AppError {
    AppError::Unsupported {
        reason: format!(
            "the running headless host speaks protocol v{server_version} and this CLI speaks \
             v{HOST_PROTOCOL_VERSION}, so they cannot talk to each other ({message}); run \
             `kodosi host stop` to replace it"
        ),
    }
}

pub(crate) async fn connect_existing_host() -> Result<Option<HeadlessHostClient>> {
    let runtime_dir = require_runtime_dir()?;
    match connect_control_lane(&runtime_dir).await? {
        HostConnectOutcome::Connected(client) => Ok(Some(*client)),
        HostConnectOutcome::Gone => Ok(None),
        HostConnectOutcome::Silent => Err(silent_host_error()),
        HostConnectOutcome::Rejected(reason) => Err(AppError::Unsupported { reason }),
        HostConnectOutcome::Incompatible {
            server_version,
            message,
        } => Err(incompatible_host_error(server_version, &message)),
    }
}

pub(crate) async fn connect_existing_or_replace_incompatible_host()
-> Result<Option<HeadlessHostClient>> {
    let runtime_dir = require_runtime_dir()?;
    match connect_control_lane(&runtime_dir).await? {
        HostConnectOutcome::Connected(client) => Ok(Some(*client)),
        HostConnectOutcome::Gone => Ok(None),
        HostConnectOutcome::Incompatible { .. } => {
            let (client, _) = ensure_host_running_in(&runtime_dir).await?;
            Ok(Some(client))
        }
        HostConnectOutcome::Silent => Err(silent_host_error()),
        HostConnectOutcome::Rejected(reason) => Err(AppError::Unsupported { reason }),
    }
}

pub(crate) async fn connect_terminal_capture_lane(
    session_id: String,
) -> Result<TerminalLaneConnection> {
    connect_terminal_lane_with_capture(session_id, true).await
}

async fn connect_terminal_lane_with_capture(
    session_id: String,
    capture: bool,
) -> Result<TerminalLaneConnection> {
    let runtime_dir = require_runtime_dir()?;
    let Some(state) = load_host_state_file_in(&runtime_dir)? else {
        return Err(AppError::Unsupported {
            reason: "headless host is not running".to_owned(),
        });
    };
    ensure_matching_config(&state)?;

    let socket_path = Path::new(&state.socket_path);
    match connect_local(socket_path).await {
        Ok(stream) => {
            let (read_half, write_half) = tokio::io::split(stream);
            let mut reader = framed_json::reader(read_half);
            let mut writer = framed_json::writer(write_half);
            framed_json::write_json(
                &mut writer,
                &HostHelloRequest {
                    token: state.token.clone(),
                    protocol_version: HOST_PROTOCOL_VERSION,
                    lane: ConnectionLane::Terminal {
                        session_id,
                        capture,
                    },
                },
            )
            .await?;
            let response = match time::timeout(
                TERMINAL_HELLO_TIMEOUT,
                framed_json::read_json::<_, HostHelloResponse>(&mut reader),
            )
            .await
            {
                Ok(result) => result?,
                Err(_elapsed) => return Err(silent_host_error()),
            };
            match response {
                Some(HostHelloResponse {
                    accepted: true,
                    capability: Some(capability),
                    ..
                }) => Ok(TerminalLaneConnection {
                    reader,
                    writer,
                    capability,
                }),
                Some(HostHelloResponse {
                    accepted: true,
                    capability: None,
                    ..
                }) => Err(AppError::Unsupported {
                    reason:
                        "headless host accepted terminal attach without declaring its capability"
                            .to_owned(),
                }),
                Some(ref rejection) => match classify_rejection(rejection) {
                    HostConnectOutcome::Incompatible {
                        server_version,
                        message,
                    } => Err(incompatible_host_error(server_version, &message)),
                    _ => Err(AppError::Unsupported {
                        reason: rejection
                            .message
                            .clone()
                            .unwrap_or_else(|| "host rejected terminal attach".to_owned()),
                    }),
                },
                None => {
                    cleanup_host_state_file_if_matches(&runtime_dir, &state);
                    Err(AppError::Unsupported {
                        reason: "headless host closed the terminal attach handshake".to_owned(),
                    })
                }
            }
        }
        Err(error) if is_connection_missing(&error) || is_not_a_socket(socket_path) => {
            cleanup_host_state_file_if_matches(&runtime_dir, &state);
            Err(AppError::Unsupported {
                reason: "headless host is not running".to_owned(),
            })
        }
        Err(error) => Err(AppError::Io(error)),
    }
}

fn spawn_detached_host() -> Result<std::process::Child> {
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command.arg(INTERNAL_HOST_SERVE_COMMAND);
    if let Some(path) = crate::config::cli_config_path() {
        command.arg("--config").arg(path);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    configure_detached_process(&mut command);
    command.spawn().map_err(AppError::Io)
}

pub(crate) async fn ensure_host_running() -> Result<(HeadlessHostClient, bool)> {
    ensure_host_running_in(&require_runtime_dir()?).await
}

pub(in crate::headless_host) async fn ensure_host_running_in(
    runtime_dir: &Path,
) -> Result<(HeadlessHostClient, bool)> {
    let mut replaced_incompatible = false;
    match connect_control_lane(runtime_dir).await? {
        HostConnectOutcome::Connected(client) => return Ok((*client, false)),
        HostConnectOutcome::Rejected(reason) => return Err(AppError::Unsupported { reason }),
        HostConnectOutcome::Incompatible {
            server_version,
            message,
        } => {
            tracing::info!(
                server_version,
                client_version = HOST_PROTOCOL_VERSION,
                message,
                "the running headless host speaks a different protocol; stopping it and starting \
                 one that matches this binary"
            );

            stop_host_in(runtime_dir)
                .await
                .map_err(|error| AppError::Unsupported {
                    reason: format!(
                        "could not replace the headless host that speaks protocol \
                         v{server_version} (this CLI speaks v{HOST_PROTOCOL_VERSION}): {error}"
                    ),
                })?;
            replaced_incompatible = true;
        }

        HostConnectOutcome::Gone | HostConnectOutcome::Silent => {}
    }

    let mut child = spawn_detached_host()?;
    let mut spawns = 1;
    let mut last_exit = None;
    let deadline = std::time::Instant::now() + HOST_START_TIMEOUT;
    while std::time::Instant::now() < deadline {
        if let Some(status) = child.try_wait().map_err(AppError::Io)? {
            last_exit = Some(status);
            if spawns >= HOST_START_SPAWN_ATTEMPTS {
                break;
            }
            child = spawn_detached_host()?;
            spawns += 1;
        }
        match connect_control_lane(runtime_dir).await? {
            HostConnectOutcome::Connected(client) => return Ok((*client, true)),
            HostConnectOutcome::Rejected(reason) => return Err(AppError::Unsupported { reason }),

            HostConnectOutcome::Incompatible {
                server_version,
                message,
            } => return Err(incompatible_host_error(server_version, &message)),
            HostConnectOutcome::Gone | HostConnectOutcome::Silent => {}
        }
        time::sleep(HOST_START_POLL_INTERVAL).await;
    }

    Err(AppError::Unsupported {
        reason: last_exit.map_or_else(
            || {
                if replaced_incompatible {
                    "replaced the incompatible headless host but timed out waiting for its \
                     replacement to start"
                        .to_owned()
                } else {
                    "timed out waiting for the headless host to start".to_owned()
                }
            },
            |status| format!("headless host exited before it became ready ({status})"),
        ),
    })
}

pub(crate) async fn host_info() -> Result<Option<RunningHostInfo>> {
    if connect_existing_host().await?.is_none() {
        return Ok(None);
    }
    let Some(state) = load_host_state_file_in(&require_runtime_dir()?)? else {
        return Ok(None);
    };
    Ok(Some(RunningHostInfo {
        pid: state.pid,
        started_at: state.started_at,
    }))
}

async fn request_shutdown_in_version(state: &HostStateFile, server_version: u8) -> bool {
    let socket_path = Path::new(&state.socket_path);
    let Ok(stream) = connect_local(socket_path).await else {
        return false;
    };
    let mut framed = framed_json::framed(stream);
    if framed_json::send_json(
        &mut framed,
        &HostHelloRequest {
            token: state.token.clone(),
            protocol_version: server_version,
            lane: ConnectionLane::Control,
        },
    )
    .await
    .is_err()
    {
        return false;
    }

    let accepted = match time::timeout(
        CONTROL_HELLO_TIMEOUT,
        framed_json::next_json::<_, HostHelloResponse>(&mut framed),
    )
    .await
    {
        Ok(Ok(Some(response))) => response.accepted,

        Ok(Ok(None) | Err(_)) | Err(_) => false,
    };
    if !accepted {
        return false;
    }

    if server_version >= 11 {
        let Ok(command) = serde_json::to_value(SystemCommand::Shutdown) else {
            return false;
        };
        framed_json::send_json(&mut framed, &ControlClientFrame::Command { command })
            .await
            .is_ok()
    } else {
        framed_json::send_json(&mut framed, &SystemCommand::Shutdown)
            .await
            .is_ok()
    }
}

pub(crate) async fn stop_host() -> Result<bool> {
    stop_host_in(&require_runtime_dir()?).await
}

pub(in crate::headless_host) async fn stop_host_in(runtime_dir: &Path) -> Result<bool> {
    let deadline =
        time::Instant::now() + shutdown::HOST_SHUTDOWN_BUDGET + shutdown::HOST_STOP_SLACK;
    let mut stop = StopProgress::default();

    loop {
        if !stop.requested
            && let Some(state) = load_host_state_file_in(runtime_dir)?
        {
            match control_handshake(runtime_dir, &state).await? {
                HostConnectOutcome::Connected(client) => {
                    let mut client = *client;
                    client.send_system(SystemCommand::Shutdown).await?;
                    stop.observed(&state, true);

                    await_control_close(&mut client, deadline).await;
                }

                HostConnectOutcome::Silent => stop.observed(&state, false),

                HostConnectOutcome::Gone => {}

                HostConnectOutcome::Incompatible {
                    server_version,
                    ref message,
                } => {
                    tracing::info!(
                        server_version,
                        client_version = HOST_PROTOCOL_VERSION,
                        message,
                        "the running headless host speaks a different protocol; asking it to shut \
                         down in its own version"
                    );
                    let delivered = request_shutdown_in_version(&state, server_version).await;
                    stop.observed(&state, delivered);
                    stop.incompatible = Some(server_version);
                }
                HostConnectOutcome::Rejected(reason) => {
                    return Err(AppError::Unsupported {
                        reason: format!("headless host refused the stop request: {reason}"),
                    });
                }
            }
        }

        if host_lock_is_free(runtime_dir) {
            if let Some(state) = stop.state.as_ref() {
                cleanup_host_state_file_if_matches(runtime_dir, state);
            }
            return Ok(stop.was_running);
        }
        if time::Instant::now() >= deadline {
            return Err(stop.timed_out());
        }
        time::sleep(HOST_STOP_POLL_INTERVAL).await;
    }
}

#[derive(Default)]
struct StopProgress {
    state: Option<HostStateFile>,
    was_running: bool,
    requested: bool,
    delivered: bool,

    incompatible: Option<u8>,
}

impl StopProgress {
    fn observed(&mut self, state: &HostStateFile, delivered: bool) {
        self.state = Some(state.clone());
        self.was_running = true;
        self.requested = true;
        self.delivered = delivered;
    }

    fn timed_out(&self) -> AppError {
        let waited = (shutdown::HOST_SHUTDOWN_BUDGET + shutdown::HOST_STOP_SLACK).as_secs();
        let pid = self.state.as_ref().map_or(0, |state| state.pid);
        let reason = self.incompatible.map_or_else(
            || {
                if self.delivered {
                    format!(
                        "headless host (pid {pid}) accepted the stop request but still owns its \
                         runtime directory after {waited}s"
                    )
                } else if self.requested {
                    format!(
                        "headless host (pid {pid}) never answered its control socket and still \
                         owns its runtime directory after {waited}s; it is wedged"
                    )
                } else {
                    format!(
                        "a headless host still owns the runtime directory after {waited}s but \
                         published no control socket to stop it"
                    )
                }
            },
            |server_version| {
                format!(
                    "headless host (pid {pid}) speaks protocol v{server_version} and this CLI \
                     speaks v{HOST_PROTOCOL_VERSION}; it did not release its runtime directory \
                     after {waited}s, so stop it by its process instead"
                )
            },
        );
        AppError::Unsupported { reason }
    }
}

async fn await_control_close(client: &mut HeadlessHostClient, deadline: time::Instant) {
    loop {
        match time::timeout_at(deadline, client.recv()).await {
            Ok(Ok(Some(_event))) => {}
            Ok(Ok(None) | Err(_)) => return,
            Err(_elapsed) => return,
        }
    }
}

fn configure_detached_process(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;

    command.process_group(0);
}

#[cfg(test)]
mod tests {
    use super::ensure_config_identity_matches;

    #[test]
    fn matching_config_identity_is_accepted() {
        ensure_config_identity_matches(Some("digest"), "digest").expect("matching identity");
    }

    #[test]
    fn mismatched_or_legacy_config_identity_is_rejected() {
        for recorded in [None, Some("other")] {
            let error = ensure_config_identity_matches(recorded, "current")
                .expect_err("config drift must reject host reuse");
            let message = error.to_string();
            assert!(message.contains("different configuration"));
            assert!(message.contains("host stop"));
        }
    }
}
