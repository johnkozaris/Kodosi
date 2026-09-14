use std::{path::PathBuf, str, thread, time::Duration};

use super::types::{
    Checkpoint, TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES, TerminalPixelGeometry, TerminalScreen,
    TerminalSize,
};
use ghostty_vt::{CheckpointLimits, CompressionProgress, Effect, Screen, Terminal, TerminalPolicy};
use kodosi_pty::{KodosiError, Result};
use tokio::{
    runtime::Builder as TokioRuntimeBuilder,
    sync::{mpsc as tokio_mpsc, oneshot},
    time::{self, Instant},
};

const TERMINAL_COMMAND_CAPACITY: usize = 64;
const TERMINAL_COMMAND_SEND_TIMEOUT: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalHistoryPolicy {
    pub max_bytes: usize,
    pub max_lines: usize,
    pub continuation_max_bytes: usize,
    pub compression_idle: Duration,
}

impl Default for TerminalHistoryPolicy {
    fn default() -> Self {
        Self {
            max_bytes: 64 * 1024 * 1024,
            max_lines: 1_024,
            continuation_max_bytes: 1024 * 1024,
            compression_idle: Duration::from_secs(2),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TerminalEffect {
    PtyWrite(Vec<u8>),
    Bell,
    Title(String),
    DesktopNotification { title: String, body: String },
    Cwd(PathBuf),
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
        pixel_geometry: Option<TerminalPixelGeometry>,
        reply_tx: oneshot::Sender<Result<Vec<TerminalEffect>>>,
    },
    CheckpointData {
        reply_tx: oneshot::Sender<Result<CheckpointWithSequence>>,
    },
    SetFocused {
        focused: bool,
        reply_tx: oneshot::Sender<Result<Vec<u8>>>,
    },
    ThemeChanged {
        dark: bool,
        reply_tx: oneshot::Sender<Result<()>>,
    },
    Shutdown {
        reply_tx: oneshot::Sender<Result<()>>,
    },
}

struct TerminalActorConfig {
    rows: u16,
    cols: u16,
    history: TerminalHistoryPolicy,
    initial_theme_dark: bool,
}

impl SessionTerminalHandle {
    pub(super) fn spawn(
        size: TerminalSize,
        history: TerminalHistoryPolicy,
        initial_theme_dark: bool,
    ) -> Result<Self> {
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
                    drop(init_tx.send(Err(KodosiError::Unsupported(
                        "failed to initialize terminal authority runtime".to_owned(),
                    ))));
                    return;
                };
                runtime.block_on(terminal_actor_main(
                    command_rx,
                    TerminalActorConfig {
                        rows,
                        cols,
                        history,
                        initial_theme_dark,
                    },
                    init_tx,
                ));
            })
            .map_err(|error| {
                KodosiError::Unsupported(format!(
                    "failed to spawn terminal authority thread: {error}"
                ))
            })?;
        init_rx.recv().map_err(|_| {
            KodosiError::Unsupported(
                "terminal authority thread exited before initialization".to_owned(),
            )
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
        pixel_geometry: Option<TerminalPixelGeometry>,
    ) -> Result<Vec<Vec<u8>>> {
        let effects = self
            .request(|reply_tx| TerminalCommand::Resize {
                rows,
                cols,
                pixel_geometry,
                reply_tx,
            })
            .await?;
        effects
            .into_iter()
            .map(|effect| match effect {
                TerminalEffect::PtyWrite(bytes) => Ok(bytes),
                other => Err(KodosiError::Unsupported(format!(
                    "terminal resize emitted non-PTY effect: {other:?}"
                ))),
            })
            .collect()
    }

    pub(super) async fn checkpoint_data(&self) -> Result<CheckpointWithSequence> {
        self.request(|reply_tx| TerminalCommand::CheckpointData { reply_tx })
            .await
    }

    pub(super) async fn set_focused(&self, focused: bool) -> Result<Vec<u8>> {
        self.request(|reply_tx| TerminalCommand::SetFocused { focused, reply_tx })
            .await
    }

    pub(super) async fn notify_theme_changed(&self, dark: bool) -> Result<()> {
        self.request(|reply_tx| TerminalCommand::ThemeChanged { dark, reply_tx })
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
                tokio_mpsc::error::SendTimeoutError::Timeout(_) => KodosiError::Backpressure(
                    "terminal authority command queue saturated".to_owned(),
                ),
                tokio_mpsc::error::SendTimeoutError::Closed(_) => KodosiError::Unsupported(
                    "terminal authority thread is no longer available".to_owned(),
                ),
            })?;
        reply_rx.await.map_err(|_| {
            KodosiError::Unsupported(
                "terminal authority reply channel closed unexpectedly".to_owned(),
            )
        })?
    }
}

struct SessionTerminal {
    terminal: Terminal,
    applied: u64,
}

impl SessionTerminal {
    fn new(
        rows: u16,
        cols: u16,
        history: TerminalHistoryPolicy,
        initial_theme_dark: bool,
    ) -> Result<Self> {
        let terminal = Terminal::new(
            cols,
            rows,
            TerminalPolicy {
                continuation_max_bytes: history.continuation_max_bytes,
                scrollback_max_bytes: history.max_bytes,
                scrollback_max_lines: history.max_lines,
                dark: initial_theme_dark,
            },
        )
        .map_err(terminal_error)?;
        Ok(Self {
            terminal,
            applied: 0,
        })
    }

    fn process_output(&mut self, bytes: &[u8]) -> Result<ProcessedTerminalOutput> {
        let next_sequence = self.applied.checked_add(1).ok_or_else(|| {
            KodosiError::Unsupported("terminal sequence exhausted u64".to_owned())
        })?;
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

    fn resize(
        &mut self,
        rows: u16,
        cols: u16,
        pixel_geometry: Option<TerminalPixelGeometry>,
    ) -> Result<Vec<TerminalEffect>> {
        let (cell_width, cell_height) = pixel_geometry.map_or((0, 0), |geometry| {
            (geometry.cell_width_pixels(), geometry.cell_height_pixels())
        });
        self.terminal
            .resize(cols, rows, cell_width, cell_height)
            .map_err(terminal_error)
            .map(|effects| effects.into_iter().filter_map(convert_effect).collect())
    }

    fn theme_changed(&mut self, dark: bool) -> Result<()> {
        self.terminal.set_dark(dark).map_err(terminal_error)
    }

    fn checkpoint_data(&mut self) -> Result<CheckpointWithSequence> {
        let checkpoint = self
            .terminal
            .semantic_checkpoint(CheckpointLimits {
                max_json_bytes: TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES,
                ..CheckpointLimits::default()
            })
            .map_err(terminal_error)?;
        let state = self.terminal.state().map_err(terminal_error)?;
        let checkpoint = Checkpoint::new(
            TerminalSize::new(state.rows, state.cols).map_err(|error| {
                KodosiError::Unsupported(format!("terminal checkpoint size failed: {error}"))
            })?,
            terminal_screen(state.active_screen),
            checkpoint.into_bytes(),
            state.cursor_x,
            state.cursor_y,
            !state.cursor_visible,
        )
        .map_err(|error| {
            KodosiError::Unsupported(format!("terminal checkpoint validation failed: {error}"))
        })?;
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

fn terminal_screen(screen: Screen) -> TerminalScreen {
    match screen {
        Screen::Primary => TerminalScreen::Primary,
        Screen::Alternate => TerminalScreen::Alternate,
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
    }
}

fn parse_cwd(raw: &[u8]) -> Option<PathBuf> {
    let text = str::from_utf8(raw).ok()?;
    let Some(rest) = text.strip_prefix("file://") else {
        return (!text.is_empty()).then(|| PathBuf::from(text));
    };
    let slash = rest.find('/')?;
    let encoded = &rest.as_bytes()[slash..];
    let output = percent_decode(encoded)?;

    String::from_utf8(output).ok().map(PathBuf::from)
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

fn terminal_error(error: ghostty_vt::Error) -> KodosiError {
    KodosiError::Unsupported(format!("terminal authority failed: {error}"))
}

struct CompressionScheduler {
    activity: u64,
    deadline: Option<Instant>,
    idle: Duration,
}

impl CompressionScheduler {
    fn new(activity: u64, idle: Duration) -> Self {
        Self {
            activity,
            deadline: None,
            idle,
        }
    }

    fn observe(&mut self, activity: u64) {
        if activity != self.activity {
            self.activity = activity;
            self.deadline = Some(Instant::now() + self.idle);
        }
    }

    fn restart_idle(&mut self) {
        self.deadline = Some(Instant::now() + self.idle);
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
    config: TerminalActorConfig,
    init_tx: std::sync::mpsc::SyncSender<Result<()>>,
) {
    let TerminalActorConfig {
        rows,
        cols,
        history,
        initial_theme_dark,
    } = config;
    let terminal = SessionTerminal::new(rows, cols, history, initial_theme_dark);
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
    let mut compression = CompressionScheduler::new(activity, history.compression_idle);

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
                        pixel_geometry,
                        reply_tx,
                    } => {
                        let result = terminal.resize(rows, cols, pixel_geometry);
                        mutated = result.is_ok();
                        drop(reply_tx.send(result));
                    }
                    TerminalCommand::CheckpointData { reply_tx } => {
                        let result = terminal.checkpoint_data();
                        if result.is_ok() {
                            compression.restart_idle();
                        }
                        drop(reply_tx.send(result));
                    }
                    TerminalCommand::SetFocused { focused, reply_tx } => {
                        drop(reply_tx.send(terminal.terminal.encode_focus(focused).map_err(terminal_error)));
                    }
                    TerminalCommand::ThemeChanged { dark, reply_tx } => {
                        drop(reply_tx.send(terminal.theme_changed(dark)));
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
