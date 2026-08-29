use std::{collections::VecDeque, io};

use kodosi_session::{KodosiPty, RawFdAsyncReader, ShutdownStage};
use tokio::time::{self, Duration};

use kodosi_domain::lifecycle::StopReason;

use super::{LoopOutcome, PtyOperation, SessionRuntimeError, SessionRuntimeResult};
use crate::{AppError, session_runtime::handles::SessionPtyInstruction};

pub(super) const PTY_SHUTDOWN_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) async fn read_pty_batch_within(
    reader: &mut RawFdAsyncReader,
    read_buffer: &mut [u8],
    deadline: time::Instant,
) -> SessionRuntimeResult<Option<bytes::Bytes>> {
    let read_result = time::timeout_at(deadline, read_pty_batch(reader, read_buffer))
        .await
        .map_err(|_| SessionRuntimeError::pty(PtyOperation::DrainToEof, PtyDrainTimedOut))?;
    match read_result {
        Ok(payload) => Ok(payload),
        Err(error) if super::terminal_closed_error(&error) => Ok(None),
        Err(error) => Err(SessionRuntimeError::pty(PtyOperation::DrainToEof, error)),
    }
}

pub(super) async fn read_pty_batch(
    reader: &mut RawFdAsyncReader,
    read_buffer: &mut [u8],
) -> io::Result<Option<bytes::Bytes>> {
    let bytes_read = reader.read(read_buffer).await?;
    if bytes_read == 0 {
        return Ok(None);
    }

    let mut total = bytes_read;
    while total < read_buffer.len() {
        match reader.try_read(&mut read_buffer[total..]) {
            Ok(0) => break,
            Ok(bytes_read) => total += bytes_read,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) if super::terminal_closed_error(&error) => break,
            Err(error) => return Err(error),
        }
    }
    Ok(Some(bytes::Bytes::copy_from_slice(&read_buffer[..total])))
}

#[derive(Debug, thiserror::Error)]
#[error("timed out before PTY reached EOF")]
struct PtyDrainTimedOut;

pub(super) struct PendingPtyWrite {
    pub(super) bytes: Vec<u8>,
    pub(super) offset: usize,
}

pub(super) fn handle_pty_instruction(
    pty: &mut KodosiPty,
    pending_pty_writes: &mut VecDeque<PendingPtyWrite>,
    instruction: SessionPtyInstruction,
) -> SessionRuntimeResult<Option<LoopOutcome>> {
    match instruction {
        SessionPtyInstruction::Write(bytes) => {
            enqueue_pty_write(pending_pty_writes, bytes)
                .map_err(|error| SessionRuntimeError::pty(PtyOperation::QueueInput, error))?;
            drain_pending_pty_writes(pty, pending_pty_writes)?;
            Ok(None)
        }
        SessionPtyInstruction::ConfirmedWrite { bytes, completion } => {
            if let Err(error) = enqueue_pty_write(pending_pty_writes, bytes) {
                drop(completion.send(Err(AppError::Unsupported {
                    reason: SessionRuntimeError::pty(PtyOperation::QueueInput, error).to_string(),
                })));
                return Ok(None);
            }
            match drain_pending_pty_writes(pty, pending_pty_writes) {
                Ok(()) => {
                    drop(completion.send(Ok(())));
                    Ok(None)
                }
                Err(error) => {
                    drop(completion.send(Err(AppError::DeliveryUnknown {
                        reason: error.to_string(),
                    })));
                    Err(error)
                }
            }
        }
        SessionPtyInstruction::Interrupt => pty
            .request_shutdown(ShutdownStage::Interrupt)
            .map(|()| None)
            .map_err(|error| SessionRuntimeError::pty(PtyOperation::Interrupt, error)),
        SessionPtyInstruction::InterruptThenWrite { bytes, reply } => {
            let result = (|| -> crate::Result<()> {
                pty.request_shutdown(ShutdownStage::Interrupt)
                    .map_err(|error| AppError::Unsupported {
                        reason: format!("PTY interrupt failed: {error}"),
                    })?;
                enqueue_pty_write(pending_pty_writes, bytes).map_err(|error| {
                    AppError::Unsupported {
                        reason: format!("PTY input queue failed: {error}"),
                    }
                })?;
                drain_pending_pty_writes(pty, pending_pty_writes).map_err(|error| {
                    AppError::Unsupported {
                        reason: format!("PTY input write failed: {error}"),
                    }
                })
            })();
            drop(reply.send(result));
            Ok(None)
        }
        SessionPtyInstruction::Kill => Ok(Some(LoopOutcome::Stopped(StopReason::UserRequested))),
    }
}

pub(super) fn ensure_pending_terminal_message_capacity(
    pending_pty_writes: &VecDeque<PendingPtyWrite>,
    pending_messages: &[Vec<u8>],
) -> Result<(), PtyQueueFull> {
    ensure_pty_queue_capacity(
        pending_pty_writes,
        pending_messages.iter().map(Vec::len).sum(),
    )
}

pub(super) fn commit_pending_terminal_messages(
    pending_pty_writes: &mut VecDeque<PendingPtyWrite>,
    pending_messages: Vec<Vec<u8>>,
) {
    for message in pending_messages {
        if !message.is_empty() {
            pending_pty_writes.push_back(PendingPtyWrite {
                bytes: message,
                offset: 0,
            });
        }
    }
}

pub(super) fn enqueue_pending_terminal_messages(
    pending_pty_writes: &mut VecDeque<PendingPtyWrite>,
    pending_messages: Vec<Vec<u8>>,
) -> Result<(), PtyQueueFull> {
    ensure_pending_terminal_message_capacity(pending_pty_writes, &pending_messages)?;
    commit_pending_terminal_messages(pending_pty_writes, pending_messages);
    Ok(())
}

const MAX_PENDING_PTY_BYTES: usize = 10 * 1024 * 1024;

fn pending_pty_bytes(pending_pty_writes: &VecDeque<PendingPtyWrite>) -> usize {
    pending_pty_writes
        .iter()
        .map(|write| write.bytes.len().saturating_sub(write.offset))
        .sum()
}

pub(super) fn ensure_pty_queue_capacity(
    pending_pty_writes: &VecDeque<PendingPtyWrite>,
    incoming: usize,
) -> Result<(), PtyQueueFull> {
    let queued = pending_pty_bytes(pending_pty_writes);
    if queued.saturating_add(incoming) > MAX_PENDING_PTY_BYTES {
        return Err(PtyQueueFull {
            pending: queued,
            incoming,
        });
    }
    Ok(())
}

pub(super) fn enqueue_pty_write(
    pending_pty_writes: &mut VecDeque<PendingPtyWrite>,
    bytes: Vec<u8>,
) -> Result<(), PtyQueueFull> {
    if bytes.is_empty() {
        return Ok(());
    }
    ensure_pty_queue_capacity(pending_pty_writes, bytes.len())?;
    pending_pty_writes.push_back(PendingPtyWrite { bytes, offset: 0 });
    Ok(())
}

#[derive(Debug, thiserror::Error)]
#[error("write queue backpressure: {pending} bytes pending, {incoming} incoming")]
pub(super) struct PtyQueueFull {
    pending: usize,
    incoming: usize,
}

pub(super) fn drain_pending_pty_writes(
    pty: &mut KodosiPty,
    pending_pty_writes: &mut VecDeque<PendingPtyWrite>,
) -> SessionRuntimeResult<()> {
    while let Some(front) = pending_pty_writes.front_mut() {
        let remaining = front.bytes.get(front.offset..).unwrap_or_default();
        if remaining.is_empty() {
            pending_pty_writes.pop_front();
            continue;
        }
        match pty.write(remaining) {
            Ok(0) => break,
            Ok(bytes_written) => {
                front.offset = front.offset.saturating_add(bytes_written);
                if front.offset >= front.bytes.len() {
                    pending_pty_writes.pop_front();
                }
            }
            Err(error) => {
                pending_pty_writes.clear();
                return Err(SessionRuntimeError::pty(
                    PtyOperation::WriteQueuedBytes,
                    error,
                ));
            }
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::{
        PTY_SHUTDOWN_DRAIN_TIMEOUT, enqueue_pending_terminal_messages, enqueue_pty_write,
        handle_pty_instruction, read_pty_batch_within,
    };
    use crate::{AppError, session_runtime::handles::SessionPtyInstruction};
    use kodosi_session::{KodosiPty, RawFdAsyncReader, ShutdownStage, WaitOutcome};
    use std::collections::VecDeque;
    use tokio::time::{self, Duration};

    fn drain_deadline() -> time::Instant {
        time::Instant::now() + PTY_SHUTDOWN_DRAIN_TIMEOUT
    }

    fn script_pty(script: &str) -> (KodosiPty, RawFdAsyncReader, tempfile::TempDir) {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().expect("test script directory");
        let path = directory.path().join("session-child");
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).expect("write test script");
        let mut permissions = std::fs::metadata(&path)
            .expect("test script metadata")
            .permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&path, permissions).expect("make test script executable");
        let path = path.to_string_lossy().into_owned();
        let (pty, reader) =
            KodosiPty::spawn_shell(Some(path.as_str()), None, 24, 80, "pty-drain-test")
                .unwrap_or_else(|error| panic!("spawn test child: {error}"));
        (pty, reader, directory)
    }

    async fn collect_to_eof(reader: &mut RawFdAsyncReader, buffer: &mut [u8]) -> Vec<u8> {
        let mut output = Vec::new();
        loop {
            let batch = read_pty_batch_within(reader, buffer, drain_deadline())
                .await
                .unwrap_or_else(|error| panic!("drain PTY: {error}"));
            let Some(batch) = batch else { break };
            output.extend_from_slice(&batch);
        }
        output
    }

    async fn wait_for_marker(reader: &mut RawFdAsyncReader, buffer: &mut [u8], marker: &[u8]) {
        let mut output = Vec::new();
        time::timeout(Duration::from_secs(2), async {
            while !output.windows(marker.len()).any(|window| window == marker) {
                let Some(batch) = read_pty_batch_within(reader, buffer, drain_deadline())
                    .await
                    .unwrap_or_else(|error| panic!("read marker: {error}"))
                else {
                    panic!("PTY reached EOF before marker")
                };
                output.extend_from_slice(&batch);
            }
        })
        .await
        .expect("marker should arrive");
    }

    #[tokio::test]
    async fn shutdown_drain_keeps_exit_trap_bytes() {
        let (mut pty, mut reader, _directory) = script_pty(
            "trap 'printf \\\"EXIT_TRAP_SENTINEL\\\\n\\\"' EXIT\nprintf 'READY\\n'\nsleep 0.2",
        );
        let mut buffer = vec![0_u8; 64 * 1024];
        wait_for_marker(&mut reader, &mut buffer, b"READY").await;

        let mut drain = std::pin::pin!(collect_to_eof(&mut reader, &mut buffer));
        let drained = tokio::select! {
            biased;
            drained = &mut drain => drained,
            exit = pty.wait_within(Duration::from_secs(2)) => {
                assert!(matches!(exit.expect("wait for shell"), WaitOutcome::Reaped(_)));
                drain.await
            }
        };

        assert!(
            drained
                .windows(b"EXIT_TRAP_SENTINEL".len())
                .any(|window| window == b"EXIT_TRAP_SENTINEL"),
            "exit-trap output must survive child reaping: {}",
            String::from_utf8_lossy(&drained)
        );
    }

    #[tokio::test]
    async fn shutdown_drain_keeps_buffered_final_output_in_order() {
        for _ in 0..32 {
            let (mut pty, mut reader, _directory) =
                script_pty("printf 'READY\\n'\nsleep 0.01\nprintf 'FINAL_ONE'\nprintf 'FINAL_TWO'");
            let mut buffer = vec![0_u8; 64 * 1024];
            wait_for_marker(&mut reader, &mut buffer, b"READY").await;
            let mut drain = std::pin::pin!(collect_to_eof(&mut reader, &mut buffer));
            let drained = tokio::select! {
                biased;
                drained = &mut drain => drained,
                exit = pty.wait_within(Duration::from_secs(2)) => {
                    assert!(matches!(exit.expect("wait for shell"), WaitOutcome::Reaped(_)));
                    drain.await
                }
            };

            assert_eq!(
                drained, b"FINAL_ONEFINAL_TWO",
                "drain must preserve exact PTY batch ordering"
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_drain_timeout_is_fatal_and_bounded() {
        let (mut pty, mut reader, _directory) = script_pty("sleep 30");
        let mut buffer = vec![0_u8; 64 * 1024];
        let started = time::Instant::now();

        let error = read_pty_batch_within(&mut reader, &mut buffer, drain_deadline())
            .await
            .expect_err("live PTY must exceed drain bound");

        assert!(
            error
                .to_string()
                .contains("timed out before PTY reached EOF")
        );
        assert_eq!(started.elapsed(), PTY_SHUTDOWN_DRAIN_TIMEOUT);
        pty.request_shutdown(ShutdownStage::Force)
            .expect("force-stop hung test child");
        pty.wait().await.expect("reap force-stopped child");
    }

    #[tokio::test]
    async fn delivered_user_bytes_are_not_filtered_by_terminal_report_grammar() {
        let (mut pty, mut reader, _directory) = script_pty(
            "stty raw -echo\nprintf 'READY\\n'\ndd bs=1 count=26 2>/dev/null\nprintf 'DONE\\n'",
        );
        let mut buffer = vec![0_u8; 64 * 1024];
        wait_for_marker(&mut reader, &mut buffer, b"READY").await;
        let bytes = b"\x1b[I\x1b[O\x1b[?1004;1$y\x1bP1$r0m\x1b\\".to_vec();
        assert_eq!(bytes.len(), 26);
        let mut queue = VecDeque::new();

        handle_pty_instruction(
            &mut pty,
            &mut queue,
            SessionPtyInstruction::Write(bytes.clone()),
        )
        .expect("write arbitrary user bytes");

        let drained = collect_to_eof(&mut reader, &mut buffer).await;
        assert_eq!(drained, [bytes, b"DONE\n".to_vec()].concat());
        pty.wait().await.expect("reap user-byte test child");
    }

    #[tokio::test]
    async fn confirmed_write_rejects_before_admission_when_pending_queue_is_full() {
        let (mut pty, _reader, _directory) = script_pty("sleep 30");
        let mut queue = VecDeque::new();
        enqueue_pty_write(&mut queue, vec![0u8; 9 * 1024 * 1024]).expect("under cap");
        let (completion, admitted) = tokio::sync::oneshot::channel();

        let result = handle_pty_instruction(
            &mut pty,
            &mut queue,
            SessionPtyInstruction::ConfirmedWrite {
                bytes: vec![0u8; 2 * 1024 * 1024],
                completion,
            },
        );

        assert!(
            result.is_ok(),
            "pre-admission rejection keeps the session live"
        );
        std::assert_matches!(
            admitted.await.expect("completion sender"),
            Err(AppError::Unsupported { .. })
        );
        assert_eq!(
            queue.len(),
            1,
            "rejected bytes must not enter the PTY queue"
        );
        pty.request_shutdown(ShutdownStage::Force)
            .expect("force-stop test child");
        pty.wait().await.expect("reap force-stopped test child");
    }

    #[tokio::test]
    async fn instruction_queue_saturation_is_fatal_without_mutating_queued_bytes() {
        let (mut pty, _reader, _directory) = script_pty("sleep 30");
        let mut queue = VecDeque::new();
        let under_cap = vec![0u8; 9 * 1024 * 1024];
        enqueue_pty_write(&mut queue, under_cap).expect("under cap");

        let error = match handle_pty_instruction(
            &mut pty,
            &mut queue,
            SessionPtyInstruction::Write(vec![0u8; 2 * 1024 * 1024]),
        ) {
            Err(error) => error,
            Ok(_) => panic!("accepted input must fail the session rather than disappear"),
        };

        assert!(error.to_string().contains("backpressure"));
        assert_eq!(queue.len(), 1);
        assert_eq!(
            queue.front().map(|write| write.bytes.len()),
            Some(9 * 1024 * 1024)
        );
        pty.request_shutdown(ShutdownStage::Force)
            .expect("force-stop test child");
        pty.wait().await.expect("reap force-stopped test child");
    }

    #[test]
    fn terminal_message_batch_is_admitted_atomically_under_pressure() {
        let mut queue = VecDeque::new();
        enqueue_pty_write(&mut queue, vec![0u8; 9 * 1024 * 1024]).expect("under cap");
        let messages = vec![vec![1u8; 512 * 1024], vec![2u8; 1024 * 1024]];

        enqueue_pending_terminal_messages(&mut queue, messages).expect_err("batch exceeds cap");

        assert_eq!(queue.len(), 1, "no prefix of a rejected batch may survive");
        assert_eq!(queue.front().map(|write| write.bytes[0]), Some(0));
    }

    #[test]
    fn enqueue_pty_write_rejects_incoming_when_cap_exceeded() {
        let mut queue = VecDeque::new();
        let under_cap = vec![0u8; 9 * 1024 * 1024];
        enqueue_pty_write(&mut queue, under_cap).expect("under cap");
        assert_eq!(queue.len(), 1, "9 MiB should fit under the 10 MiB cap");

        let over_cap = vec![0u8; 2 * 1024 * 1024];
        let error = enqueue_pty_write(&mut queue, over_cap).expect_err("over cap");
        assert!(error.to_string().contains("backpressure"));
        assert_eq!(
            queue.len(),
            1,
            "exceeding the cap rejects only the incoming write; queued input survives"
        );
        assert_eq!(
            queue.front().map(|w| w.bytes.len()),
            Some(9 * 1024 * 1024),
            "the preserved entry is the original under-cap write"
        );
    }
}
