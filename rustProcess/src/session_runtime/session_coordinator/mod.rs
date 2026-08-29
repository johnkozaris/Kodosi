mod process;
mod pty_io;
mod screen;

use std::{collections::VecDeque, fmt, io};

use kodosi_session::{KodosiPty, RawFdAsyncReader, SessionTerminalHandle, TerminalHistoryPolicy};
use tokio::{
    sync::mpsc,
    time::{self, Duration, MissedTickBehavior},
};
use tokio_util::sync::CancellationToken;

use crate::{
    AppError, Result,
    session_runtime::{
        coordinator_events::{emit_capture_metadata, emit_failure_and_stop, emit_stopped},
        events::{LocalCoordinatorOrigin, RuntimeSessionEvent},
        handles::{
            LaunchKind, OwnedSessionHandle, OwnedSessionSpec, SessionPtyInstruction,
            SessionScreenInstruction, SessionSenders,
        },
        metadata::RuntimeMetadataWorker,
    },
};
use kodosi_domain::{lifecycle::StopReason, terminal::TerminalSize};

use process::{resolve_process_exit, shutdown_terminal, wait_for_process_shutdown};
use pty_io::{
    PTY_SHUTDOWN_DRAIN_TIMEOUT, drain_pending_pty_writes, handle_pty_instruction, read_pty_batch,
    read_pty_batch_within,
};
use screen::{
    ProcessedPtyOutput, ScreenInstructionResult, handle_screen_instruction, handle_screen_resize,
    process_pty_output,
};

const SESSION_SCREEN_CAPACITY: usize = 128;
const SESSION_PTY_CAPACITY: usize = 128;
const RUNTIME_METADATA_INTERVAL: Duration = Duration::from_secs(5);
const PENDING_PTY_DRAIN_INTERVAL: Duration = Duration::from_millis(10);
const RESIZE_COALESCE_WINDOW: Duration = Duration::from_millis(16);

#[derive(Debug, Default)]
struct ResizeCoalescer {
    pending: Option<TerminalSize>,
    deadline: Option<time::Instant>,
}

impl ResizeCoalescer {
    fn record(&mut self, size: TerminalSize, now: time::Instant) {
        self.pending = Some(size);
        self.deadline.get_or_insert(now + RESIZE_COALESCE_WINDOW);
    }

    fn deadline(&self) -> Option<time::Instant> {
        self.deadline
    }

    fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    fn take(&mut self) -> Option<TerminalSize> {
        self.deadline = None;
        self.pending.take()
    }
}

async fn wait_resize_deadline(deadline: Option<time::Instant>) {
    match deadline {
        Some(deadline) => time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

fn provider_resume_arguments(
    source: &kodosi_domain::provider_conversation::ProviderConversationIdentity,
) -> Vec<String> {
    match source.provider {
        kodosi_domain::provider_conversation::ProviderConversationProvider::Claude => {
            vec!["--resume".to_owned(), source.native_conversation_id.clone()]
        }
        kodosi_domain::provider_conversation::ProviderConversationProvider::Copilot => {
            vec![format!("--resume={}", source.native_conversation_id)]
        }
    }
}

enum LoopOutcome {
    Cancelled,
    Stopped(StopReason),
    Failed(SessionRuntimeError),
}

enum SessionScreenOutcome {
    Continue,
    Exit(LoopOutcome),
}

type SessionRuntimeResult<T> = std::result::Result<T, SessionRuntimeError>;

#[derive(Debug, thiserror::Error)]
enum SessionRuntimeError {
    #[error("terminal emulator failed while {operation}: {detail}")]
    Terminal {
        operation: TerminalOperation,
        detail: String,
    },
    #[error("PTY {operation} failed: {detail}")]
    Pty {
        operation: PtyOperation,
        detail: String,
    },
}

#[derive(Debug, Clone, Copy)]
enum TerminalOperation {
    StartKernel,
    ProcessOutput,
    EncodeInput,
    EncodeBatchedInput,
    Resize,
    Focus,
    Blur,
    ClipboardSupport,
    ThemeChanged,
    ShutdownKernel,
}

#[derive(Debug, Clone, Copy)]
enum PtyOperation {
    SpawnShell,
    Read,
    Wait,
    WaitAfterForceKill,
    ForceKill,
    Resize,
    Interrupt,
    WriteQueuedBytes,
    QueueInput,
    DrainToEof,
}

impl SessionRuntimeError {
    fn terminal(operation: TerminalOperation, source: impl fmt::Display) -> Self {
        Self::Terminal {
            operation,
            detail: source.to_string(),
        }
    }

    fn pty(operation: PtyOperation, source: impl fmt::Display) -> Self {
        Self::Pty {
            operation,
            detail: source.to_string(),
        }
    }
}

impl fmt::Display for TerminalOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::StartKernel => "starting the terminal kernel",
            Self::ProcessOutput => "processing PTY output",
            Self::EncodeInput => "adjusting input",
            Self::EncodeBatchedInput => "adjusting batched input",
            Self::Resize => "resizing",
            Self::Focus => "updating focus",
            Self::Blur => "updating blur",
            Self::ClipboardSupport => "updating clipboard support",
            Self::ThemeChanged => "notifying theme change",
            Self::ShutdownKernel => "stopping the terminal kernel",
        })
    }
}

impl fmt::Display for PtyOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::SpawnShell => "shell spawn",
            Self::Read => "read",
            Self::Wait => "child wait",
            Self::WaitAfterForceKill => "child wait after force-kill",
            Self::ForceKill => "force-kill",
            Self::Resize => "resize",
            Self::Interrupt => "interrupt",
            Self::WriteQueuedBytes => "queued write",
            Self::QueueInput => "queue input",
            Self::DrainToEof => "shutdown drain to EOF",
        })
    }
}

pub(crate) struct OwnedSessionServices {
    pub(crate) session_events: mpsc::Sender<RuntimeSessionEvent>,
    pub(crate) local_incarnation_id: uuid::Uuid,
    pub(crate) cancellation: CancellationToken,
    pub(crate) supports_osc52_clipboard: bool,
    pub(crate) terminal_history: TerminalHistoryPolicy,
    pub(crate) terminal_hub: crate::terminal_transport::hub::SessionHub,
}

#[expect(
    clippy::too_many_lines,
    reason = "session resource construction and ownership transfer stay in one failure-atomic path"
)]
pub(crate) async fn spawn(
    spec: OwnedSessionSpec,
    services: OwnedSessionServices,
) -> Result<OwnedSessionHandle> {
    let OwnedSessionServices {
        session_events,
        local_incarnation_id,
        cancellation,
        supports_osc52_clipboard,
        terminal_history,
        terminal_hub,
    } = services;
    let launch_kind_str = match spec.launch.kind {
        LaunchKind::Create => "create",
        LaunchKind::Resume => "resume",
        LaunchKind::Reopen => "reopen",
    };
    let pane_identity = spec.id.to_string();

    let telemetry_registration =
        match crate::agent_intel::telemetry::register_session(spec.id, local_incarnation_id) {
            Ok(registration) => Some(registration),
            Err(error) => {
                tracing::warn!(
                    session_id = %spec.id,
                    %error,
                    "session-scoped agent event receiver is unavailable"
                );
                None
            }
        };
    let session_integration = match telemetry_registration
        .as_ref()
        .map_or(Ok(None), |registration| {
            crate::session_integrations::prepare(spec.id, registration)
        }) {
        Ok(integration) => integration,
        Err(error) => {
            tracing::warn!(
                session_id = %spec.id,
                %error,
                "session-scoped Copilot extension is unavailable"
            );
            None
        }
    };
    let mut session_environment = Vec::new();
    if let Some(integration) = &session_integration {
        session_environment.extend_from_slice(integration.environment());
    }
    session_environment.push((
        "KODOSI_SESSION_INCARNATION_ID".to_owned(),
        local_incarnation_id.to_string(),
    ));
    let pty = if let Some(source) = &spec.launch.resume_source {
        let executable = session_integration
            .as_ref()
            .and_then(|integration| integration.provider_executable(source.provider))
            .or_else(|| crate::session_integrations::installed_provider_executable(source.provider))
            .ok_or_else(|| AppError::Unsupported {
                reason: format!(
                    "{} is not installed or executable",
                    source.provider.executable()
                ),
            });
        match executable {
            Err(error) => Err(error),
            Ok(executable) => {
                let launch_permit = if source.provider
                    == kodosi_domain::provider_conversation::ProviderConversationProvider::Claude
                {
                    Some(::agent_intel::claude::launch_coordination::acquire_launch_permit().await)
                } else {
                    None
                };
                let arguments = provider_resume_arguments(source);
                let result = KodosiPty::spawn_program_with_env(
                    &executable,
                    &arguments,
                    spec.launch.initial_working_dir.as_deref(),
                    spec.size.rows(),
                    spec.size.cols(),
                    &pane_identity,
                    &session_environment,
                )
                .map_err(|error| AppError::Unsupported {
                    reason: SessionRuntimeError::pty(PtyOperation::SpawnShell, error).to_string(),
                });
                if result.is_ok()
                    && let Some(launch_permit) = launch_permit
                {
                    launch_permit.record_spawn();
                } else {
                    drop(launch_permit);
                }
                result
            }
        }
    } else {
        KodosiPty::spawn_shell_with_env(
            spec.launch.initial_shell.as_deref(),
            spec.launch.initial_working_dir.as_deref(),
            spec.size.rows(),
            spec.size.cols(),
            &pane_identity,
            &session_environment,
        )
        .map_err(|error| AppError::Unsupported {
            reason: SessionRuntimeError::pty(PtyOperation::SpawnShell, error).to_string(),
        })
    };
    let (pty, reader) = match pty {
        Ok(pty) => pty,
        Err(error) => {
            cancellation.cancel();
            return Err(error);
        }
    };

    tracing::info!(
        session_id = %spec.id,
        runtime = "kodosi",
        launch = launch_kind_str,
        child_pid = pty.child_pid(),
        working_dir = ?spec.launch.initial_working_dir,
        size_rows = spec.size.rows(),
        size_cols = spec.size.cols(),
        "Kodosi session started"
    );

    let (screen_tx, screen_rx) = mpsc::channel(SESSION_SCREEN_CAPACITY);
    let (pty_tx, pty_rx) = mpsc::channel(SESSION_PTY_CAPACITY);
    let senders = SessionSenders::new(Some(screen_tx), Some(pty_tx));
    let initial_cwd = spec.launch.initial_working_dir.clone();
    let origin = LocalCoordinatorOrigin {
        session_id: spec.id,
        local_incarnation_id,
    };
    let coordinator_cancellation = cancellation.clone();
    let join_handle = tokio::spawn(async move {
        let _telemetry_registration = telemetry_registration;
        let _session_integration = session_integration;
        run_owned_session(
            spec,
            pty,
            reader,
            screen_rx,
            pty_rx,
            session_events,
            origin,
            coordinator_cancellation,
            initial_cwd,
            supports_osc52_clipboard,
            terminal_history,
            terminal_hub,
        )
        .await;
    });

    Ok(OwnedSessionHandle {
        senders,
        cancellation,
        join_handle,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "One helper keeps live and shutdown PTY batches on the same parse, sequence-check, and publish pipeline."
)]
async fn admit_pty_batch(
    terminal: &SessionTerminalHandle,
    pending_pty_writes: &mut VecDeque<pty_io::PendingPtyWrite>,
    session_events: &mpsc::Sender<RuntimeSessionEvent>,
    origin: LocalCoordinatorOrigin,
    payload: bytes::Bytes,
    last_working_dir: &mut Option<String>,
    metadata_dirty: &mut bool,
    notification_policy: &mut screen::TerminalNotificationPolicy,
    terminal_hub: &crate::terminal_transport::hub::SessionHub,
) -> SessionRuntimeResult<SessionScreenOutcome> {
    let processed = process_pty_output(
        terminal,
        pending_pty_writes,
        session_events,
        origin,
        payload.clone(),
        last_working_dir,
        metadata_dirty,
        notification_policy,
    )
    .await?;
    let (applied_sequence, processing_failed) = match processed {
        ProcessedPtyOutput::Continue {
            applied_sequence,
            processing_failed,
        } => (applied_sequence, processing_failed),
        ProcessedPtyOutput::Exit(outcome) => return Ok(SessionScreenOutcome::Exit(outcome)),
    };

    let expected_hub_sequence = applied_sequence.saturating_sub(1);
    if terminal_hub.publish_local(origin, payload.clone()) != Some(expected_hub_sequence) {
        return Err(SessionRuntimeError::terminal(
            TerminalOperation::ProcessOutput,
            format!("terminal hub sequence disagreed with applied sequence {applied_sequence}"),
        ));
    }
    if processing_failed {
        return Err(SessionRuntimeError::terminal(
            TerminalOperation::ProcessOutput,
            "terminal VT processor failed after applying the published batch",
        ));
    }

    match session_events.try_send(RuntimeSessionEvent::TerminalOutputObserved {
        origin,
        data: payload,
    }) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(_)) => {
            tracing::debug!(
                session_id = %origin.session_id,
                "runtime state loop lagged terminal output; local hub continued"
            );
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            return Ok(SessionScreenOutcome::Exit(LoopOutcome::Cancelled));
        }
    }
    Ok(SessionScreenOutcome::Continue)
}

fn retain_shutdown_outcome(outcome: &mut LoopOutcome, error: SessionRuntimeError) {
    if matches!(outcome, LoopOutcome::Cancelled | LoopOutcome::Stopped(_)) {
        *outcome = LoopOutcome::Failed(error);
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "The owned-session loop intentionally keeps PTY lifecycle, stream fanout, metadata refresh, and snapshot replies in one place."
)]
#[expect(
    clippy::too_many_arguments,
    reason = "The owned-session loop intentionally receives the explicit screen and PTY lanes instead of hiding them behind a shared mutable wrapper."
)]
async fn run_owned_session(
    spec: OwnedSessionSpec,
    mut pty: KodosiPty,
    mut reader: RawFdAsyncReader,
    mut screen_rx: mpsc::Receiver<SessionScreenInstruction>,
    mut pty_rx: mpsc::Receiver<SessionPtyInstruction>,
    session_events: mpsc::Sender<RuntimeSessionEvent>,
    origin: LocalCoordinatorOrigin,
    cancellation: CancellationToken,
    initial_working_dir: Option<String>,
    supports_osc52_clipboard: bool,
    terminal_history: TerminalHistoryPolicy,
    terminal_hub: crate::terminal_transport::hub::SessionHub,
) {
    let mut current_size = spec.size;
    let mut process_reaped = false;
    let mut read_buffer = vec![0_u8; 64 * 1024];
    let mut pending_pty_writes = VecDeque::new();
    let mut metadata_interval = time::interval(RUNTIME_METADATA_INTERVAL);
    metadata_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut pty_drain_interval = time::interval(PENDING_PTY_DRAIN_INTERVAL);
    pty_drain_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut metadata_dirty = false;
    let mut pending_resize = ResizeCoalescer::default();
    let mut notification_policy = screen::TerminalNotificationPolicy::default();
    let terminal = match SessionTerminalHandle::spawn_with_theme(
        current_size.rows(),
        current_size.cols(),
        spec.initial_terminal_sequence,
        supports_osc52_clipboard,
        terminal_history,
        spec.initial_theme_dark,
    ) {
        Ok(terminal) => terminal,
        Err(error) => {
            let _ = emit_failure_and_stop(
                &session_events,
                origin,
                SessionRuntimeError::terminal(TerminalOperation::StartKernel, error),
            )
            .await;
            return;
        }
    };
    let mut last_working_dir: Option<String> = None;

    if emit_capture_metadata(
        &session_events,
        origin,
        current_size,
        initial_working_dir,
        true,
    )
    .await
    .is_err()
    {
        return;
    }

    let metadata_worker = RuntimeMetadataWorker::spawn(
        origin,
        pty.child_pid(),
        session_events.clone(),
        &cancellation,
    );
    metadata_worker.request(&pty, last_working_dir.as_deref());

    let mut outcome = loop {
        tokio::select! {
            biased;

            () = cancellation.cancelled() => {
                break LoopOutcome::Cancelled;
            }
            () = wait_resize_deadline(pending_resize.deadline()), if pending_resize.is_pending() => {
                if let Some(size) = pending_resize.take() {
                    match handle_screen_resize(
                        &pty,
                        &terminal,
                        &mut pending_pty_writes,
                        &session_events,
                        origin,
                        &mut current_size,
                        size,
                        None,
                        None,
                    ).await {
                        Ok(SessionScreenOutcome::Continue) => {}
                        Ok(SessionScreenOutcome::Exit(outcome)) => break outcome,
                        Err(error) => break LoopOutcome::Failed(error),
                    }
                }
            }
            maybe_instruction = pty_rx.recv() => {
                match maybe_instruction {
                    Some(instruction) => match handle_pty_instruction(
                        &mut pty,
                        &mut pending_pty_writes,
                        instruction,
                    ) {
                        Ok(Some(outcome)) => break outcome,
                        Ok(None) => {}
                        Err(error) => break LoopOutcome::Failed(error),
                    },
                    None => break LoopOutcome::Cancelled,
                }
            }
            _ = pty_drain_interval.tick(), if !pending_pty_writes.is_empty() => {
                if let Err(error) = drain_pending_pty_writes(&mut pty, &mut pending_pty_writes) {
                    break LoopOutcome::Failed(error);
                }
            }
            _ = metadata_interval.tick() => {
                if let Some(size) = pending_resize.take() {
                    match handle_screen_resize(
                        &pty,
                        &terminal,
                        &mut pending_pty_writes,
                        &session_events,
                        origin,
                        &mut current_size,
                        size,
                        None,
                        None,
                    ).await {
                        Ok(SessionScreenOutcome::Continue) => {}
                        Ok(SessionScreenOutcome::Exit(outcome)) => break outcome,
                        Err(error) => break LoopOutcome::Failed(error),
                    }
                }
                if metadata_dirty
                    && metadata_worker.request(&pty, last_working_dir.as_deref())
                {
                    metadata_dirty = false;
                }
            }
            read_result = read_pty_batch(&mut reader, &mut read_buffer) => {
                if let Some(size) = pending_resize.take() {
                    match handle_screen_resize(
                        &pty,
                        &terminal,
                        &mut pending_pty_writes,
                        &session_events,
                        origin,
                        &mut current_size,
                        size,
                        None,
                        None,
                    ).await {
                        Ok(SessionScreenOutcome::Continue) => {}
                        Ok(SessionScreenOutcome::Exit(outcome)) => break outcome,
                        Err(error) => break LoopOutcome::Failed(error),
                    }
                }
                match read_result {
                    Ok(None) => {
                        process_reaped = true;
                        break resolve_process_exit(&mut pty).await;
                    }
                    Ok(Some(payload)) => {
                        metadata_dirty = true;
                        match admit_pty_batch(
                            &terminal,
                            &mut pending_pty_writes,
                            &session_events,
                            origin,
                            payload,
                            &mut last_working_dir,
                            &mut metadata_dirty,
                            &mut notification_policy,
                            &terminal_hub,
                        )
                        .await
                        {
                            Ok(SessionScreenOutcome::Continue) => {}
                            Ok(SessionScreenOutcome::Exit(outcome)) => break outcome,
                            Err(error) => break LoopOutcome::Failed(error),
                        }
                    }
                    Err(error) if terminal_closed_error(&error) => {
                        process_reaped = true;
                        break resolve_process_exit(&mut pty).await;
                    }
                    Err(error) => {
                        break LoopOutcome::Failed(SessionRuntimeError::pty(
                            PtyOperation::Read,
                            error,
                        ));
                    }
                }
            }
            maybe_instruction = screen_rx.recv() => {
                let Some(instruction) = maybe_instruction else {
                    break LoopOutcome::Cancelled;
                };


                if let SessionScreenInstruction::Resize {
                    size,
                    pixel_geometry: None,
                    completion: None,
                } = &instruction
                {
                    pending_resize.record(*size, time::Instant::now());
                    continue;
                }
                if let Some(size) = pending_resize.take() {
                    match handle_screen_resize(
                        &pty,
                        &terminal,
                        &mut pending_pty_writes,
                        &session_events,
                        origin,
                        &mut current_size,
                        size,
                        None,
                        None,
                    ).await {
                        Ok(SessionScreenOutcome::Continue) => {}
                        Ok(SessionScreenOutcome::Exit(outcome)) => break outcome,
                        Err(error) => break LoopOutcome::Failed(error),
                    }
                }
                match handle_screen_instruction(
                    &pty,
                    &terminal,
                    screen::ScreenMutableState {
                        pending_pty_writes: &mut pending_pty_writes,
                        current_size: &mut current_size,
                    },
                    &session_events,
                    origin,
                    instruction,
                )
                .await
                {
                    Ok(ScreenInstructionResult::Continue) => {}
                    Ok(ScreenInstructionResult::Pty(instruction)) => {
                        match handle_pty_instruction(
                            &mut pty,
                            &mut pending_pty_writes,
                            instruction,
                        ) {
                            Ok(Some(outcome)) => break outcome,
                            Ok(None) => {}
                            Err(error) => break LoopOutcome::Failed(error),
                        }
                    }
                    Ok(ScreenInstructionResult::Exit(outcome)) => break outcome,
                    Err(error) => break LoopOutcome::Failed(error),
                }
            }
        }
    };

    metadata_worker.shutdown().await;

    if !process_reaped {
        let shutdown_deadline = time::Instant::now() + PTY_SHUTDOWN_DRAIN_TIMEOUT;
        {
            let shutdown = wait_for_process_shutdown(&mut pty);
            tokio::pin!(shutdown);
            let mut shutdown_complete = false;
            let mut reached_eof = false;
            let mut drain_failed = false;

            while !shutdown_complete || (!reached_eof && !drain_failed) {
                tokio::select! {
                    biased;
                    read_result = read_pty_batch_within(
                        &mut reader,
                        &mut read_buffer,
                        shutdown_deadline,
                    ), if !reached_eof && !drain_failed => {
                        match read_result {
                            Ok(None) => reached_eof = true,
                            Ok(Some(payload)) => {
                                metadata_dirty = true;
                                match admit_pty_batch(
                                    &terminal,
                                    &mut pending_pty_writes,
                                    &session_events,
                                    origin,
                                    payload,
                                    &mut last_working_dir,
                                    &mut metadata_dirty,
                                    &mut notification_policy,
                                    &terminal_hub,
                                )
                                .await
                                {
                                    Ok(SessionScreenOutcome::Continue) => {}
                                    Ok(SessionScreenOutcome::Exit(LoopOutcome::Cancelled)) => {
                                        retain_shutdown_outcome(
                                            &mut outcome,
                                            SessionRuntimeError::pty(
                                                PtyOperation::DrainToEof,
                                                "terminal output pipeline closed during shutdown drain",
                                            ),
                                        );
                                        drain_failed = true;
                                    }
                                    Ok(SessionScreenOutcome::Exit(drain_outcome)) => {
                                        outcome = drain_outcome;
                                        drain_failed = true;
                                    }
                                    Err(error) => {
                                        retain_shutdown_outcome(&mut outcome, error);
                                        drain_failed = true;
                                    }
                                }
                            }
                            Err(error) => {
                                tracing::debug!(session_id = %spec.id, "PTY shutdown drain failed: {error}");
                                retain_shutdown_outcome(&mut outcome, error);
                                drain_failed = true;
                            }
                        }
                    }
                    shutdown_result = &mut shutdown, if !shutdown_complete => {
                        shutdown_complete = true;
                        if let Err(error) = shutdown_result {
                            tracing::debug!(session_id = %spec.id, "PTY shutdown cleanup failed: {error}");
                            retain_shutdown_outcome(&mut outcome, error);
                        }
                    }
                }
            }
        }
        if let Err(error) = drain_pending_pty_writes(&mut pty, &mut pending_pty_writes) {
            retain_shutdown_outcome(&mut outcome, error);
        }
    }

    if let Err(error) = shutdown_terminal(&mut pty, terminal).await {
        tracing::debug!(session_id = %spec.id, "terminal shutdown cleanup failed: {error}");
    }

    match outcome {
        LoopOutcome::Cancelled => {
            let _ = emit_stopped(&session_events, origin, StopReason::Cancelled).await;
        }
        LoopOutcome::Stopped(reason) => {
            let _ = emit_stopped(&session_events, origin, reason).await;
        }
        LoopOutcome::Failed(message) => {
            let _ = emit_failure_and_stop(&session_events, origin, message).await;
        }
    }
}

fn terminal_closed_error(error: &io::Error) -> bool {
    const EIO: i32 = 5;
    const ENXIO: i32 = 6;
    const EBADF: i32 = 9;

    matches!(error.raw_os_error(), Some(EIO | ENXIO | EBADF))
}

#[cfg(test)]
mod tests;
