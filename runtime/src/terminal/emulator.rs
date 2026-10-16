use std::{path::PathBuf, str, thread, time::Duration};

use super::{
    status::{BlockedKind, ProgramState, ProgramStatusReport},
    types::{CellPixels, Checkpoint, TerminalSize, is_valid_folder, snapshot_limits},
};
use crate::{Error, Result};
use ghostty_vt::{
    CompressionProgress, ContextSignal, Effect, ProgramStatusKind, ProgramStatusState,
    ProgressState, Terminal, TerminalPolicy,
};
use tokio::{
    runtime::Builder as TokioRuntimeBuilder,
    sync::{mpsc as tokio_mpsc, oneshot},
    time::{self, Instant},
};

const TERMINAL_COMMAND_CAPACITY: usize = 64;
const TERMINAL_COMMAND_SEND_TIMEOUT: Duration = Duration::from_millis(50);
const COMPRESSION_IDLE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TerminalEffect {
    PtyWrite(Vec<u8>),
    Bell,
    Title(String),
    DesktopNotification { title: String, body: String },
    Cwd(PathBuf),
    ProgramStatus(ProgramStatusReport),
    Progress(ProgramStatusReport),
    Context(ContextSignal),
    Reset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProcessedTerminalOutput {
    pub effects: Vec<TerminalEffect>,
    pub applied_sequence: u64,
    pub processing_failed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CheckpointWithSequence {
    pub checkpoint: Checkpoint,
    pub applied_sequence: u64,
}

pub(super) struct SessionTerminalHandle {
    command_tx: tokio_mpsc::Sender<TerminalCommand>,
}

enum TerminalCommand {
    ProcessOutput {
        bytes: bytes::Bytes,
        reply_tx: oneshot::Sender<Result<ProcessedTerminalOutput>>,
    },
    Resize {
        rows: u16,
        cols: u16,
        cell: CellPixels,
        reply_tx: oneshot::Sender<Result<Vec<TerminalEffect>>>,
    },
    CheckpointData {
        history: bool,
        image_data_bytes: u64,
        reply_tx: oneshot::Sender<Result<CheckpointWithSequence>>,
    },
    SetFocused {
        focused: bool,
        reply_tx: oneshot::Sender<Result<Vec<u8>>>,
    },
    Shutdown {
        reply_tx: oneshot::Sender<Result<()>>,
    },
}

impl SessionTerminalHandle {
    pub(super) fn spawn(size: TerminalSize) -> Result<Self> {
        let rows = size.rows();
        let cols = size.cols();
        let (command_tx, command_rx) = tokio_mpsc::channel(TERMINAL_COMMAND_CAPACITY);
        let (init_tx, init_rx) = std::sync::mpsc::sync_channel(1);
        thread::Builder::new()
            .name("kodosi-terminal-authority".to_owned())
            .spawn(move || {
                let runtime = TokioRuntimeBuilder::new_current_thread()
                    .enable_time()
                    .build();
                let Ok(runtime) = runtime else {
                    drop(init_tx.send(Err(Error::Other(
                        "failed to initialize terminal authority runtime".to_owned(),
                    ))));
                    return;
                };
                runtime.block_on(terminal_actor_main(command_rx, rows, cols, init_tx));
            })
            .map_err(|error| {
                Error::Other(format!(
                    "failed to spawn terminal authority thread: {error}"
                ))
            })?;
        init_rx.recv().map_err(|_| {
            Error::Other("terminal authority thread exited before initialization".to_owned())
        })??;
        Ok(Self { command_tx })
    }

    pub(super) async fn process_output(
        &self,
        bytes: bytes::Bytes,
    ) -> Result<ProcessedTerminalOutput> {
        self.request(|reply_tx| TerminalCommand::ProcessOutput { bytes, reply_tx })
            .await
    }

    pub(super) async fn resize(
        &self,
        rows: u16,
        cols: u16,
        cell: CellPixels,
    ) -> Result<Vec<Vec<u8>>> {
        let effects = self
            .request(|reply_tx| TerminalCommand::Resize {
                rows,
                cols,
                cell,
                reply_tx,
            })
            .await?;
        effects
            .into_iter()
            .map(|effect| match effect {
                TerminalEffect::PtyWrite(bytes) => Ok(bytes),
                other => Err(Error::Other(format!(
                    "terminal resize emitted non-PTY effect: {other:?}"
                ))),
            })
            .collect()
    }

    pub(super) async fn checkpoint_data(
        &self,
        history: bool,
        image_data_bytes: u64,
    ) -> Result<CheckpointWithSequence> {
        self.request(|reply_tx| TerminalCommand::CheckpointData {
            history,
            image_data_bytes,
            reply_tx,
        })
        .await
    }

    pub(super) async fn set_focused(&self, focused: bool) -> Result<Vec<u8>> {
        self.request(|reply_tx| TerminalCommand::SetFocused { focused, reply_tx })
            .await
    }

    pub(super) async fn shutdown(self) -> Result<()> {
        self.request(|reply_tx| TerminalCommand::Shutdown { reply_tx })
            .await
    }

    async fn request<T>(
        &self,
        build_command: impl FnOnce(oneshot::Sender<Result<T>>) -> TerminalCommand,
    ) -> Result<T> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.command_tx
            .send_timeout(build_command(reply_tx), TERMINAL_COMMAND_SEND_TIMEOUT)
            .await
            .map_err(|error| match error {
                tokio_mpsc::error::SendTimeoutError::Timeout(_) => Error::Busy,
                tokio_mpsc::error::SendTimeoutError::Closed(_) => {
                    Error::Other("terminal authority thread is no longer available".to_owned())
                }
            })?;
        reply_rx.await.map_err(|_| {
            Error::Other("terminal authority reply channel closed unexpectedly".to_owned())
        })?
    }
}

struct SessionTerminal {
    terminal: Terminal,
    applied: u64,
}

impl SessionTerminal {
    fn new(rows: u16, cols: u16) -> Result<Self> {
        let mut terminal = Terminal::new(
            cols,
            rows,
            TerminalPolicy {
                console: kodosi_pty::CONSOLE_HOST,
                ..TerminalPolicy::default()
            },
        )
        .map_err(terminal_error)?;
        terminal
            .report_version(concat!("Kodosi ", env!("KODOSI_VERSION")))
            .map_err(terminal_error)?;
        Ok(Self {
            terminal,
            applied: 0,
        })
    }

    fn process_output(&mut self, bytes: &[u8]) -> Result<ProcessedTerminalOutput> {
        let next_sequence = self
            .applied
            .checked_add(1)
            .ok_or_else(|| Error::Other("terminal sequence exhausted u64".to_owned()))?;
        let outcome = self.terminal.write_outcome(bytes).map_err(terminal_error)?;
        self.applied = next_sequence;
        Ok(ProcessedTerminalOutput {
            effects: outcome
                .effects
                .into_iter()
                .filter_map(convert_effect)
                .collect(),
            applied_sequence: self.applied,
            processing_failed: outcome.processing_failed,
        })
    }

    fn resize(&mut self, rows: u16, cols: u16, cell: CellPixels) -> Result<Vec<TerminalEffect>> {
        self.terminal
            .resize(cols, rows, cell.width, cell.height)
            .map_err(terminal_error)
            .map(|effects| effects.into_iter().filter_map(convert_effect).collect())
    }

    /// A history too large for one snapshot gives the visible part, so a view
    /// always gets the screen.
    fn checkpoint_data(
        &mut self,
        history: bool,
        image_data_bytes: u64,
    ) -> Result<CheckpointWithSequence> {
        let limits = snapshot_limits(image_data_bytes);
        let snapshot = match history.then(|| self.terminal.snapshot(limits)) {
            None | Some(Err(ghostty_vt::Error::LimitExceeded)) => {
                self.terminal.visible_snapshot(limits)
            }
            Some(full) => full,
        }
        .map_err(terminal_error)?;
        let checkpoint = Checkpoint::new(snapshot, kodosi_pty::CONSOLE_HOST)
            .map_err(|error| Error::Other(format!("terminal checkpoint failed: {error}")))?;
        Ok(CheckpointWithSequence {
            checkpoint,
            applied_sequence: self.applied,
        })
    }

    fn compress_incremental(&mut self) -> Result<CompressionProgress> {
        self.terminal.compress_incremental().map_err(terminal_error)
    }

    fn compression_activity(&mut self) -> Result<u64> {
        self.terminal.compression_activity().map_err(terminal_error)
    }
}

fn convert_effect(effect: Effect) -> Option<TerminalEffect> {
    match effect {
        Effect::PtyWrite(bytes) => Some(TerminalEffect::PtyWrite(bytes)),
        Effect::Bell => Some(TerminalEffect::Bell),
        Effect::Title(bytes) => Some(TerminalEffect::Title(
            String::from_utf8_lossy(&bytes).into_owned(),
        )),
        Effect::Pwd(bytes) => parse_cwd(&bytes).map(TerminalEffect::Cwd),
        Effect::DesktopNotification { title, body } => Some(TerminalEffect::DesktopNotification {
            title: String::from_utf8_lossy(&title).into_owned(),
            body: String::from_utf8_lossy(&body).into_owned(),
        }),
        Effect::ProgramStatus(status) => program_status(status).map(TerminalEffect::ProgramStatus),
        Effect::Progress { state, progress } => {
            Some(TerminalEffect::Progress(ProgramStatusReport {
                id: String::new(),
                state: match state {
                    ProgressState::Remove => None,
                    ProgressState::Error => Some(ProgramState::Error),
                    ProgressState::Set | ProgressState::Indeterminate | ProgressState::Pause => {
                        Some(ProgramState::Working)
                    }
                },
                kind: None,
                progress: progress.filter(|_| state != ProgressState::Indeterminate),
                app: None,
                title: None,
                message: None,
            }))
        }
        Effect::Context(signal) => Some(TerminalEffect::Context(signal)),
        Effect::Reset => Some(TerminalEffect::Reset),
    }
}

fn program_status(status: ghostty_vt::ProgramStatus) -> Option<ProgramStatusReport> {
    let text = |bytes: Vec<u8>| String::from_utf8(bytes).ok();
    let optional = |bytes: Vec<u8>| text(bytes).map(|text| (!text.is_empty()).then_some(text));
    Some(ProgramStatusReport {
        id: text(status.id)?,
        state: match status.state {
            ProgramStatusState::Idle => Some(ProgramState::Idle),
            ProgramStatusState::Working => Some(ProgramState::Working),
            ProgramStatusState::Done => Some(ProgramState::Done),
            ProgramStatusState::Blocked => Some(ProgramState::Blocked),
            ProgramStatusState::Error => Some(ProgramState::Error),
            ProgramStatusState::Clear => None,
        },
        kind: status.kind.map(|kind| match kind {
            ProgramStatusKind::Permission => BlockedKind::Permission,
            ProgramStatusKind::Question => BlockedKind::Question,
            ProgramStatusKind::Auth => BlockedKind::Auth,
        }),
        progress: status.progress,
        app: optional(status.app)?,
        title: optional(status.title)?,
        message: optional(status.message)?,
    })
}

fn parse_cwd(raw: &[u8]) -> Option<PathBuf> {
    let text = str::from_utf8(raw).ok()?;
    let path = if let Some(rest) = text.strip_prefix("file://") {
        let slash = rest.find('/')?;
        let decoded = String::from_utf8(percent_decode(&rest.as_bytes()[slash..])?).ok()?;
        kodosi_os::fs::path_from_file_url(&rest[..slash], &decoded)
    } else {
        // Windows Terminal and ConEmu shell integrations quote the OSC 9;9 path.
        PathBuf::from(
            text.strip_prefix('"')
                .and_then(|path| path.strip_suffix('"'))
                .unwrap_or(text),
        )
    };
    (path.is_absolute() && path.to_str().is_some_and(is_valid_folder)).then_some(path)
}

fn percent_decode(encoded: &[u8]) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(encoded.len());
    let mut index = 0;
    while index < encoded.len() {
        if encoded[index] == b'%' {
            let end = index.checked_add(3)?;
            if end > encoded.len() {
                return None;
            }
            output
                .push(u8::from_str_radix(str::from_utf8(&encoded[index + 1..end]).ok()?, 16).ok()?);
            index = end;
        } else {
            output.push(encoded[index]);
            index += 1;
        }
    }
    Some(output)
}

fn terminal_error(error: ghostty_vt::Error) -> Error {
    Error::Other(format!("terminal authority failed: {error}"))
}

struct CompressionScheduler {
    activity: u64,
    deadline: Option<Instant>,
}

impl CompressionScheduler {
    const fn new(activity: u64) -> Self {
        Self {
            activity,
            deadline: None,
        }
    }

    fn observe(&mut self, activity: u64) {
        if activity != self.activity {
            self.activity = activity;
            self.restart_idle();
        }
    }

    fn restart_idle(&mut self) {
        self.deadline = Some(Instant::now() + COMPRESSION_IDLE);
    }

    const fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    fn step(&mut self, progress: CompressionProgress) {
        self.deadline = match progress {
            CompressionProgress::Pending => Some(Instant::now()),
            CompressionProgress::Complete | CompressionProgress::Unsupported => None,
        };
    }
}

async fn wait_for_compression(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

#[allow(
    clippy::future_not_send,
    reason = "the Ghostty authority is intentionally !Send and polled only by its dedicated current-thread runtime"
)]
async fn terminal_actor_main(
    mut command_rx: tokio_mpsc::Receiver<TerminalCommand>,
    rows: u16,
    cols: u16,
    init_tx: std::sync::mpsc::SyncSender<Result<()>>,
) {
    let terminal = SessionTerminal::new(rows, cols);
    let Ok(mut terminal) = terminal else {
        drop(init_tx.send(terminal.map(|_| ())));
        return;
    };
    let activity = terminal.compression_activity();
    let Ok(activity) = activity else {
        drop(init_tx.send(activity.map(|_| ())));
        return;
    };
    if init_tx.send(Ok(())).is_err() {
        return;
    }
    let mut compression = CompressionScheduler::new(activity);

    loop {
        tokio::select! {
            biased;
            command = command_rx.recv() => {
                let Some(command) = command else {
                    break;
                };
                let mut mutated = false;
                match command {
                    TerminalCommand::ProcessOutput { bytes, reply_tx } => {
                        let result = terminal.process_output(&bytes);
                        mutated = result.is_ok();
                        drop(reply_tx.send(result));
                    }
                    TerminalCommand::Resize {
                        rows,
                        cols,
                        cell,
                        reply_tx,
                    } => {
                        let result = terminal.resize(rows, cols, cell);
                        mutated = result.is_ok();
                        drop(reply_tx.send(result));
                    }
                    TerminalCommand::CheckpointData {
                        history,
                        image_data_bytes,
                        reply_tx,
                    } => {
                        let result = terminal.checkpoint_data(history, image_data_bytes);
                        if result.is_ok() {
                            compression.restart_idle();
                        }
                        drop(reply_tx.send(result));
                    }
                    TerminalCommand::SetFocused { focused, reply_tx } => {
                        drop(reply_tx.send(terminal.terminal.encode_focus(focused).map_err(terminal_error)));
                    }
                    TerminalCommand::Shutdown { reply_tx } => {
                        drop(terminal);
                        drop(reply_tx.send(Ok(())));
                        break;
                    }
                }
                if mutated {
                    let Ok(activity) = terminal.compression_activity() else {
                        break;
                    };
                    compression.observe(activity);
                }
            }
            () = wait_for_compression(compression.deadline()) => {
                let Ok(progress) = terminal.compress_incremental() else {
                    break;
                };
                compression.step(progress);
            }
        }
    }
}

#[cfg(test)]
#[path = "emulator_tests.rs"]
mod tests;
