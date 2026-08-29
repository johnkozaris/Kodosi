use std::collections::VecDeque;

use kodosi_session::{KodosiPty, SessionTerminalHandle, ShutdownStage, WaitOutcome};
use tokio::time::Duration;

use kodosi_domain::lifecycle::StopReason;

use super::pty_io::{drain_pending_pty_writes, enqueue_pending_terminal_messages};
use super::{
    LoopOutcome, PtyOperation, SessionRuntimeError, SessionRuntimeResult, TerminalOperation,
};

const PROCESS_HUP_GRACE_PERIOD: Duration = Duration::from_millis(500);

const PROCESS_TERM_GRACE_PERIOD: Duration = Duration::from_secs(2);

pub(super) async fn resolve_process_exit(pty: &mut KodosiPty) -> LoopOutcome {
    match pty.wait().await {
        Ok(_) => LoopOutcome::Stopped(StopReason::ProcessExited),
        Err(error) => LoopOutcome::Failed(SessionRuntimeError::pty(PtyOperation::Wait, error)),
    }
}

pub(super) async fn wait_for_process_shutdown(pty: &mut KodosiPty) -> SessionRuntimeResult<()> {
    if let Err(error) = pty.request_shutdown(ShutdownStage::Hangup) {
        tracing::debug!(
            %error,
            "initial PTY shutdown request failed; proceeding to termination stage"
        );
    }

    if let Err(error) = pty.tcdrain() {
        tracing::debug!("PTY drain before shutdown failed: {error}");
    }

    if matches!(
        pty.wait_within(PROCESS_HUP_GRACE_PERIOD)
            .await
            .map_err(|error| SessionRuntimeError::pty(PtyOperation::Wait, error))?,
        WaitOutcome::Reaped(_)
    ) {
        return Ok(());
    }

    if let Err(error) = pty.request_shutdown(ShutdownStage::Terminate) {
        tracing::debug!(
            %error,
            "PTY termination request failed; proceeding to force-kill"
        );
    }
    if matches!(
        pty.wait_within(PROCESS_TERM_GRACE_PERIOD)
            .await
            .map_err(|error| SessionRuntimeError::pty(PtyOperation::Wait, error))?,
        WaitOutcome::Reaped(_)
    ) {
        return Ok(());
    }

    pty.request_shutdown(ShutdownStage::Force)
        .map_err(|error| SessionRuntimeError::pty(PtyOperation::ForceKill, error))?;
    pty.wait()
        .await
        .map(|_| ())
        .map_err(|error| SessionRuntimeError::pty(PtyOperation::WaitAfterForceKill, error))
}

pub(super) async fn shutdown_terminal(
    pty: &mut KodosiPty,
    terminal: SessionTerminalHandle,
) -> SessionRuntimeResult<()> {
    let pending_messages = terminal
        .shutdown()
        .await
        .map_err(|error| SessionRuntimeError::terminal(TerminalOperation::ShutdownKernel, error))?;
    let mut pending_pty_writes = VecDeque::new();
    enqueue_pending_terminal_messages(&mut pending_pty_writes, pending_messages)
        .map_err(|error| SessionRuntimeError::pty(PtyOperation::QueueInput, error))?;
    drain_pending_pty_writes(pty, &mut pending_pty_writes)
}
