use kodosi_session::{CheckpointWithSequence, PresentationWithSequence};
use tokio::{
    sync::{mpsc, oneshot},
    time::{self, Duration},
};
use tokio_util::sync::CancellationToken;

use crate::{AppError, Result, session_runtime::commands::SessionInput};
use kodosi_domain::{
    ids::SessionId,
    terminal::{TerminalPixelGeometry, TerminalSize},
};

#[derive(Debug, Clone)]
pub(crate) struct OwnedSessionSpec {
    pub(crate) id: SessionId,
    pub(crate) size: TerminalSize,
    pub(crate) initial_terminal_sequence: u64,
    pub(crate) initial_theme_dark: bool,
    pub(crate) launch: OwnedSessionLaunch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LaunchKind {
    Create,
    Resume,
    Reopen,
}

#[derive(Debug, Clone)]
pub(crate) struct OwnedSessionLaunch {
    pub(crate) kind: LaunchKind,
    pub(crate) initial_working_dir: Option<String>,
    pub(crate) initial_shell: Option<String>,
    pub(crate) resume_source:
        Option<kodosi_domain::provider_conversation::ProviderConversationIdentity>,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionSenders {
    screen: Option<mpsc::Sender<SessionScreenInstruction>>,
    pty: Option<mpsc::Sender<SessionPtyInstruction>>,
}

#[derive(Debug)]
pub(crate) struct OwnedSessionHandle {
    pub(crate) senders: SessionSenders,
    pub(crate) cancellation: CancellationToken,
    pub(crate) join_handle: tokio::task::JoinHandle<()>,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionRuntimeHandle {
    session_id: SessionId,
    senders: SessionSenders,
}

#[derive(Debug)]
pub(crate) enum SessionPtyInstruction {
    Write(Vec<u8>),
    ConfirmedWrite {
        bytes: Vec<u8>,
        completion: oneshot::Sender<Result<()>>,
    },
    Interrupt,
    InterruptThenWrite {
        bytes: Vec<u8>,
        reply: oneshot::Sender<Result<()>>,
    },
    Kill,
}

pub(crate) enum SessionScreenInstruction {
    Input(SessionInput),
    ConfirmedInput {
        input: SessionInput,
        reclaim_size: Option<TerminalSize>,
        completion: oneshot::Sender<Result<()>>,
    },
    InputBatch(Vec<SessionInput>),
    ResizeAndInputBatch {
        size: TerminalSize,
        inputs: Vec<SessionInput>,
    },
    Resize {
        size: TerminalSize,
        pixel_geometry: Option<TerminalPixelGeometry>,
        completion: Option<oneshot::Sender<Result<()>>>,
    },
    Focus {
        client_id: String,
    },
    Blur {
        client_id: String,
    },
    SetClipboardSupport {
        supported: bool,
    },
    ThemeChanged {
        dark: bool,
    },
    CaptureCheckpointData {
        reply_tx: oneshot::Sender<Result<CheckpointWithSequence>>,
    },
    CapturePresentationData {
        reply_tx: oneshot::Sender<Result<PresentationWithSequence>>,
    },
}

impl std::fmt::Debug for SessionScreenInstruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Input(_) => "Input(..)",
            Self::ConfirmedInput { .. } => "ConfirmedInput(..)",
            Self::InputBatch(_) => "InputBatch(..)",
            Self::ResizeAndInputBatch { .. } => "ResizeAndInputBatch(..)",
            Self::Resize { .. } => "Resize(..)",
            Self::Focus { .. } => "Focus { .. }",
            Self::Blur { .. } => "Blur { .. }",
            Self::SetClipboardSupport { .. } => "SetClipboardSupport { .. }",
            Self::ThemeChanged { .. } => "ThemeChanged { .. }",
            Self::CaptureCheckpointData { .. } => "CaptureCheckpointData { .. }",
            Self::CapturePresentationData { .. } => "CapturePresentationData { .. }",
        })
    }
}

const CONTROL_SEND_TIMEOUT: Duration = Duration::from_millis(50);

const SNAPSHOT_REPLY_TIMEOUT: Duration = Duration::from_millis(750);

impl SessionSenders {
    pub(crate) fn new(
        screen: Option<mpsc::Sender<SessionScreenInstruction>>,
        pty: Option<mpsc::Sender<SessionPtyInstruction>>,
    ) -> Self {
        Self { screen, pty }
    }

    pub(crate) fn try_send_to_screen(
        &self,
        session_id: SessionId,
        instruction: SessionScreenInstruction,
    ) -> Result<()> {
        let screen = self.screen.as_ref().ok_or_else(|| AppError::Unsupported {
            reason: format!("session {session_id} does not support screen instructions"),
        })?;
        screen.try_send(instruction).map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => AppError::ChannelFull {
                session: session_id.to_string(),
            },
            mpsc::error::TrySendError::Closed(_) => AppError::ChannelClosed {
                session: session_id.to_string(),
            },
        })
    }

    pub(crate) async fn send_to_screen_with_timeout(
        &self,
        session_id: SessionId,
        instruction: SessionScreenInstruction,
    ) -> Result<()> {
        let screen = self.screen.as_ref().ok_or_else(|| AppError::Unsupported {
            reason: format!("session {session_id} does not support screen instructions"),
        })?;
        send_with_timeout(screen, session_id, instruction).await
    }

    pub(crate) async fn send_to_pty_with_timeout(
        &self,
        session_id: SessionId,
        instruction: SessionPtyInstruction,
    ) -> Result<()> {
        let pty = self.pty.as_ref().ok_or_else(|| AppError::Unsupported {
            reason: format!("session {session_id} does not support PTY instructions"),
        })?;
        send_with_timeout(pty, session_id, instruction).await
    }
}

async fn send_with_timeout<T>(
    sender: &mpsc::Sender<T>,
    session_id: SessionId,
    instruction: T,
) -> Result<()> {
    match time::timeout(CONTROL_SEND_TIMEOUT, sender.send(instruction)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(_)) => Err(AppError::ChannelClosed {
            session: session_id.to_string(),
        }),
        Err(_elapsed) => Err(AppError::ChannelFull {
            session: session_id.to_string(),
        }),
    }
}

impl SessionRuntimeHandle {
    pub(crate) fn new(session_id: SessionId, senders: SessionSenders) -> Self {
        Self {
            session_id,
            senders,
        }
    }

    pub(crate) async fn send_screen_instruction(
        &self,
        instruction: SessionScreenInstruction,
    ) -> Result<()> {
        self.senders
            .send_to_screen_with_timeout(self.session_id, instruction)
            .await
    }

    pub(crate) async fn send_pty_instruction(
        &self,
        instruction: SessionPtyInstruction,
    ) -> Result<()> {
        self.senders
            .send_to_pty_with_timeout(self.session_id, instruction)
            .await
    }

    pub(crate) async fn interrupt_then_write(&self, bytes: Vec<u8>) -> Result<()> {
        let (reply, response) = oneshot::channel();
        self.send_pty_instruction(SessionPtyInstruction::InterruptThenWrite { bytes, reply })
            .await?;
        match time::timeout(CONTROL_SEND_TIMEOUT, response).await {
            Ok(Ok(result)) => result,
            Ok(Err(_closed)) => Err(AppError::ChannelClosed {
                session: self.session_id.to_string(),
            }),
            Err(_elapsed) => Err(AppError::ChannelFull {
                session: self.session_id.to_string(),
            }),
        }
    }

    pub(crate) async fn begin_resize(
        &self,
        size: TerminalSize,
        pixel_geometry: Option<TerminalPixelGeometry>,
    ) -> Result<oneshot::Receiver<Result<()>>> {
        let (completion, applied) = oneshot::channel();
        self.send_screen_instruction(SessionScreenInstruction::Resize {
            size,
            pixel_geometry,
            completion: Some(completion),
        })
        .await?;
        Ok(applied)
    }

    pub(crate) async fn resize_and_wait(
        &self,
        size: TerminalSize,
        pixel_geometry: Option<TerminalPixelGeometry>,
    ) -> Result<()> {
        let applied = self.begin_resize(size, pixel_geometry).await?;
        await_mutation_reply(self.session_id, applied).await
    }

    pub(crate) async fn begin_confirmed_input(
        &self,
        input: SessionInput,
        reclaim_size: Option<TerminalSize>,
    ) -> Result<oneshot::Receiver<Result<()>>> {
        let (completion, admitted) = oneshot::channel();
        self.send_screen_instruction(SessionScreenInstruction::ConfirmedInput {
            input,
            reclaim_size,
            completion,
        })
        .await?;
        Ok(admitted)
    }

    pub(crate) async fn send_inputs_with_reclaim(
        &self,
        inputs: Vec<SessionInput>,
        reclaim_size: Option<TerminalSize>,
    ) -> Result<()> {
        if inputs.is_empty() {
            return Ok(());
        }
        let instruction = match reclaim_size {
            Some(size) => SessionScreenInstruction::ResizeAndInputBatch { size, inputs },
            None => SessionScreenInstruction::InputBatch(inputs),
        };
        self.send_screen_instruction(instruction).await
    }

    pub(crate) async fn capture_checkpoint_data(&self) -> Result<CheckpointWithSequence> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.senders
            .send_to_screen_with_timeout(
                self.session_id,
                SessionScreenInstruction::CaptureCheckpointData { reply_tx },
            )
            .await?;
        await_capture_reply(self.session_id, reply_rx).await
    }

    pub(crate) async fn capture_presentation_data(&self) -> Result<PresentationWithSequence> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.senders
            .send_to_screen_with_timeout(
                self.session_id,
                SessionScreenInstruction::CapturePresentationData { reply_tx },
            )
            .await?;
        await_capture_reply(self.session_id, reply_rx).await
    }

    pub(crate) async fn set_clipboard_support(&self, supported: bool) -> Result<()> {
        self.send_screen_instruction(SessionScreenInstruction::SetClipboardSupport { supported })
            .await
    }
}

async fn await_mutation_reply<T>(
    session_id: SessionId,
    reply_rx: oneshot::Receiver<Result<T>>,
) -> Result<T> {
    match reply_rx.await {
        Ok(result) => result,
        Err(_closed) => Err(AppError::ChannelClosed {
            session: session_id.to_string(),
        }),
    }
}

async fn await_capture_reply<T>(
    session_id: SessionId,
    reply_rx: oneshot::Receiver<Result<T>>,
) -> Result<T> {
    match time::timeout(SNAPSHOT_REPLY_TIMEOUT, reply_rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_recv)) => Err(AppError::ChannelClosed {
            session: session_id.to_string(),
        }),
        Err(_elapsed) => Err(AppError::ChannelFull {
            session: session_id.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SessionPtyInstruction, SessionRuntimeHandle, SessionScreenInstruction, SessionSenders,
    };
    use crate::{AppError, session_runtime::commands::SessionInput};
    use kodosi_domain::{ids::SessionId, terminal::TerminalSize};
    use std::time::Duration;
    use tokio::{sync::mpsc, time::sleep};

    #[test]
    fn screen_instruction_debug_output_redacts_terminal_input() {
        let instruction =
            SessionScreenInstruction::Input(SessionInput::new(b"typed-secret".to_vec()));

        assert_eq!(format!("{instruction:?}"), "Input(..)");
    }

    #[tokio::test]
    async fn send_to_pty_with_timeout_forwards_instruction() {
        let (pty_tx, mut pty_rx) = mpsc::channel::<SessionPtyInstruction>(1);
        let senders = SessionSenders::new(None, Some(pty_tx));
        let session_id = SessionId::new();

        senders
            .send_to_pty_with_timeout(
                session_id,
                SessionPtyInstruction::Write(vec![0x1b, b'[', b'A']),
            )
            .await
            .unwrap_or_else(|error| panic!("expected PTY send to succeed: {error}"));

        let Some(SessionPtyInstruction::Write(bytes)) = pty_rx.recv().await else {
            panic!("expected PTY write instruction");
        };
        assert_eq!(bytes, vec![0x1b, b'[', b'A']);
    }

    #[tokio::test]
    async fn send_to_pty_with_timeout_requires_supported_channel() {
        let senders = SessionSenders::new(None, None);
        let session_id = SessionId::new();

        let result = senders
            .send_to_pty_with_timeout(session_id, SessionPtyInstruction::Interrupt)
            .await;
        std::assert_matches!(result, Err(AppError::Unsupported { .. }));
    }

    #[tokio::test]
    async fn try_send_to_screen_forwards_instruction() {
        let (screen_tx, mut screen_rx) = mpsc::channel::<SessionScreenInstruction>(1);
        let senders = SessionSenders::new(Some(screen_tx), None);
        let session_id = SessionId::new();

        senders
            .try_send_to_screen(
                session_id,
                SessionScreenInstruction::Focus {
                    client_id: "client-1".to_owned(),
                },
            )
            .unwrap_or_else(|error| panic!("expected screen send to succeed: {error}"));

        let Some(SessionScreenInstruction::Focus { client_id }) = screen_rx.recv().await else {
            panic!("expected focus instruction");
        };
        assert_eq!(client_id, "client-1");
    }

    #[tokio::test]
    async fn send_to_screen_with_timeout_waits_for_capacity() {
        let (screen_tx, mut screen_rx) = mpsc::channel::<SessionScreenInstruction>(1);
        let senders = SessionSenders::new(Some(screen_tx), None);
        let session_id = SessionId::new();

        senders
            .try_send_to_screen(
                session_id,
                SessionScreenInstruction::Focus {
                    client_id: "client-1".to_owned(),
                },
            )
            .unwrap_or_else(|error| panic!("expected first screen send to succeed: {error}"));

        let send_task = tokio::spawn({
            let senders = senders.clone();
            async move {
                senders
                    .send_to_screen_with_timeout(
                        session_id,
                        SessionScreenInstruction::Blur {
                            client_id: "client-2".to_owned(),
                        },
                    )
                    .await
            }
        });

        sleep(Duration::from_millis(10)).await;
        let Some(drained) = screen_rx.recv().await else {
            panic!("expected queued screen instruction");
        };
        std::assert_matches!(drained, SessionScreenInstruction::Focus { .. });
        send_task
            .await
            .unwrap_or_else(|error| panic!("screen send task should succeed: {error}"))
            .unwrap_or_else(|error| panic!("timed screen send should succeed: {error}"));

        let Some(SessionScreenInstruction::Blur { client_id }) = screen_rx.recv().await else {
            panic!("expected blur instruction after capacity freed");
        };
        assert_eq!(client_id, "client-2");
    }

    #[tokio::test]
    async fn send_to_screen_with_timeout_fails_when_queue_stays_full() {
        let (screen_tx, _screen_rx) = mpsc::channel::<SessionScreenInstruction>(1);
        let senders = SessionSenders::new(Some(screen_tx), None);
        let session_id = SessionId::new();

        senders
            .try_send_to_screen(
                session_id,
                SessionScreenInstruction::Focus {
                    client_id: "client-1".to_owned(),
                },
            )
            .unwrap_or_else(|error| panic!("expected first screen send to succeed: {error}"));

        let send_task = tokio::spawn({
            let senders = senders.clone();
            async move {
                senders
                    .send_to_screen_with_timeout(
                        session_id,
                        SessionScreenInstruction::Blur {
                            client_id: "client-2".to_owned(),
                        },
                    )
                    .await
            }
        });

        let result = send_task
            .await
            .unwrap_or_else(|error| panic!("screen send task should finish: {error}"));
        std::assert_matches!(result, Err(AppError::ChannelFull { .. }));
    }

    #[tokio::test(start_paused = true)]
    async fn applied_resize_waits_for_definitive_result_beyond_snapshot_timeout() {
        let (screen_tx, mut screen_rx) = mpsc::channel::<SessionScreenInstruction>(4);
        let session_id = SessionId::new();
        let handle =
            SessionRuntimeHandle::new(session_id, SessionSenders::new(Some(screen_tx), None));
        let resize = tokio::spawn(async move {
            handle
                .resize_and_wait(TerminalSize::new(30, 100).expect("size"), None)
                .await
        });
        let instruction = screen_rx.recv().await.expect("resize instruction");
        let SessionScreenInstruction::Resize {
            completion: Some(completion),
            ..
        } = instruction
        else {
            panic!("expected correlated resize")
        };

        tokio::time::advance(Duration::from_secs(1)).await;
        assert!(
            !resize.is_finished(),
            "mutation result must not use snapshot timeout"
        );
        completion.send(Ok(())).expect("resize waiter remains live");

        resize
            .await
            .expect("resize task")
            .expect("definitive resize success");
    }

    #[tokio::test]
    async fn a_checkpoint_reply_that_never_arrives_fails_instead_of_wedging_the_loop() {
        let (screen_tx, mut screen_rx) = mpsc::channel::<SessionScreenInstruction>(4);
        let session_id = SessionId::new();
        let handle =
            SessionRuntimeHandle::new(session_id, SessionSenders::new(Some(screen_tx), None));

        let stalled_coordinator = tokio::spawn(async move {
            let instruction = screen_rx.recv().await;
            std::assert_matches!(
                instruction,
                Some(SessionScreenInstruction::CaptureCheckpointData { .. })
            );

            sleep(Duration::from_secs(30)).await;
        });

        let result = handle.capture_checkpoint_data().await;

        std::assert_matches!(result, Err(AppError::ChannelFull { .. }));
        stalled_coordinator.abort();
    }

    #[tokio::test]
    async fn a_checkpoint_request_does_not_wait_forever_for_queue_capacity() {
        let (screen_tx, _screen_rx) = mpsc::channel::<SessionScreenInstruction>(1);
        let session_id = SessionId::new();
        let handle =
            SessionRuntimeHandle::new(session_id, SessionSenders::new(Some(screen_tx), None));
        handle
            .senders
            .try_send_to_screen(
                session_id,
                SessionScreenInstruction::Focus {
                    client_id: "client-1".to_owned(),
                },
            )
            .unwrap_or_else(|error| panic!("expected the queue to accept one item: {error}"));

        let result = handle.capture_checkpoint_data().await;

        std::assert_matches!(result, Err(AppError::ChannelFull { .. }));
    }

    #[tokio::test]
    async fn a_dropped_coordinator_reports_closed_not_full() {
        let (screen_tx, mut screen_rx) = mpsc::channel::<SessionScreenInstruction>(4);
        let session_id = SessionId::new();
        let handle =
            SessionRuntimeHandle::new(session_id, SessionSenders::new(Some(screen_tx), None));

        let dropping_coordinator = tokio::spawn(async move {
            drop(screen_rx.recv().await);
        });

        let result = handle.capture_checkpoint_data().await;

        std::assert_matches!(result, Err(AppError::ChannelClosed { .. }));
        dropping_coordinator
            .await
            .unwrap_or_else(|error| panic!("coordinator task should finish: {error}"));
    }
}
