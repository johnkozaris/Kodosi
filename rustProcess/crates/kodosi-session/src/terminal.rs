use std::{collections::HashSet, path::PathBuf, str, thread, time::Duration};

use ghostty_vt::{
    CheckpointLimits, ClipboardLocation, CompressionProgress, Effect, Key, Modifiers, Screen,
    Terminal, TerminalPolicy,
};
use kodosi_domain::terminal::{
    TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES, TerminalCheckpointV2, TerminalPixelGeometry,
    TerminalPresentationV2, TerminalScreen, TerminalSize,
};
use kodosi_utils::{KodosiError, Result};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientFocus {
    Focused,
    Blurred,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalInput {
    Text(String),
    Raw(Vec<u8>),
    Enter,
    Backspace,
    Tab,
    Escape,
    Home,
    End,
    Arrow(TerminalDirection),
    Ctrl(char),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalEffect {
    PtyWrite(Vec<u8>),
    Bell,
    Title(String),
    ClipboardText(String),
    DesktopNotification { title: String, body: String },
    Cwd(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessedTerminalOutput {
    pub effects: Vec<TerminalEffect>,
    pub applied_sequence: u64,
    pub processing_failed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointWithSequence {
    pub checkpoint: TerminalCheckpointV2,
    pub applied_sequence: u64,
}

pub struct PresentationWithSequence {
    pub presentation: TerminalPresentationV2,
    pub applied_sequence: u64,
}

pub struct SessionTerminalHandle {
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
    PresentationData {
        reply_tx: oneshot::Sender<Result<PresentationWithSequence>>,
    },
    EncodeInput {
        input: TerminalInput,
        reply_tx: oneshot::Sender<Result<Vec<u8>>>,
    },
    SetFocused {
        client_id: String,
        focus: ClientFocus,
        reply_tx: oneshot::Sender<Result<Vec<Vec<u8>>>>,
    },
    SetClipboardSupport {
        supported: bool,
        reply_tx: oneshot::Sender<Result<()>>,
    },
    ThemeChanged {
        dark: bool,
        reply_tx: oneshot::Sender<Result<()>>,
    },
    Shutdown {
        reply_tx: oneshot::Sender<Result<Vec<Vec<u8>>>>,
    },
}

impl SessionTerminalHandle {
    pub fn spawn_with_theme(
        rows: u16,
        cols: u16,
        initial_sequence: u64,
        supports_osc52_clipboard: bool,
        history: TerminalHistoryPolicy,
        initial_theme_dark: bool,
    ) -> Result<Self> {
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
                    rows,
                    cols,
                    initial_sequence,
                    supports_osc52_clipboard,
                    history,
                    initial_theme_dark,
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

    pub async fn process_output(&self, bytes: bytes::Bytes) -> Result<ProcessedTerminalOutput> {
        self.request(|reply_tx| TerminalCommand::ProcessOutput { bytes, reply_tx })
            .await
    }

    pub async fn resize(
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

    pub async fn encode_input(&self, input: TerminalInput) -> Result<Vec<u8>> {
        self.request(|reply_tx| TerminalCommand::EncodeInput { input, reply_tx })
            .await
    }

    pub async fn checkpoint_data(&self) -> Result<CheckpointWithSequence> {
        self.request(|reply_tx| TerminalCommand::CheckpointData { reply_tx })
            .await
    }

    pub async fn presentation_data(&self) -> Result<PresentationWithSequence> {
        self.request(|reply_tx| TerminalCommand::PresentationData { reply_tx })
            .await
    }

    pub async fn set_client_focus(
        &self,
        client_id: String,
        focus: ClientFocus,
    ) -> Result<Vec<Vec<u8>>> {
        self.request(|reply_tx| TerminalCommand::SetFocused {
            client_id,
            focus,
            reply_tx,
        })
        .await
    }

    pub async fn set_clipboard_support(&self, supported: bool) -> Result<()> {
        self.request(|reply_tx| TerminalCommand::SetClipboardSupport {
            supported,
            reply_tx,
        })
        .await
    }

    pub async fn notify_theme_changed(&self, dark: bool) -> Result<()> {
        self.request(|reply_tx| TerminalCommand::ThemeChanged { dark, reply_tx })
            .await
    }

    pub async fn shutdown(self) -> Result<Vec<Vec<u8>>> {
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
    focused_clients: HashSet<String>,
    applied: u64,
}

impl SessionTerminal {
    fn new(
        rows: u16,
        cols: u16,
        initial_sequence: u64,
        supports_osc52_clipboard: bool,
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
                clipboard_enabled: supports_osc52_clipboard,
                dark: initial_theme_dark,
            },
        )
        .map_err(terminal_error)?;
        Ok(Self {
            terminal,
            focused_clients: HashSet::new(),
            applied: initial_sequence,
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

    fn set_clipboard_support(&mut self, supported: bool) -> Result<()> {
        self.terminal
            .set_clipboard_enabled(supported)
            .map_err(terminal_error)
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
        let checkpoint = TerminalCheckpointV2::new(
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

    fn presentation_data(&mut self) -> Result<PresentationWithSequence> {
        let formatted = self.terminal.plain_presentation().map_err(terminal_error)?;
        let presentation = TerminalPresentationV2::new(
            TerminalSize::new(formatted.rows, formatted.cols).map_err(|error| {
                KodosiError::Unsupported(format!("terminal presentation size failed: {error}"))
            })?,
            terminal_screen(formatted.active_screen),
            formatted.plain_lines,
            formatted.cursor_x,
            formatted.cursor_y,
            formatted.cursor_hidden,
        )
        .map_err(|error| {
            KodosiError::Unsupported(format!("terminal presentation validation failed: {error}"))
        })?;
        Ok(PresentationWithSequence {
            presentation,
            applied_sequence: self.applied,
        })
    }

    fn encode_input(&mut self, input: TerminalInput) -> Result<Vec<u8>> {
        match input {
            TerminalInput::Text(text) => Ok(text.into_bytes()),
            TerminalInput::Raw(bytes) => Ok(bytes),
            TerminalInput::Enter => self.encode_key(Key::Enter, Modifiers::default()),
            TerminalInput::Backspace => self.encode_key(Key::Backspace, Modifiers::default()),
            TerminalInput::Tab => self.encode_key(Key::Tab, Modifiers::default()),
            TerminalInput::Escape => self.encode_key(Key::Escape, Modifiers::default()),
            TerminalInput::Home => self.encode_key(Key::Home, Modifiers::default()),
            TerminalInput::End => self.encode_key(Key::End, Modifiers::default()),
            TerminalInput::Arrow(direction) => self.encode_key(
                match direction {
                    TerminalDirection::Up => Key::ArrowUp,
                    TerminalDirection::Down => Key::ArrowDown,
                    TerminalDirection::Left => Key::ArrowLeft,
                    TerminalDirection::Right => Key::ArrowRight,
                },
                Modifiers::default(),
            ),
            TerminalInput::Ctrl(character) => self.encode_key(
                Key::Letter(character),
                Modifiers {
                    control: true,
                    ..Modifiers::default()
                },
            ),
        }
    }

    fn encode_key(&mut self, key: Key, modifiers: Modifiers) -> Result<Vec<u8>> {
        self.terminal
            .encode_key(key, modifiers)
            .map_err(terminal_error)
    }

    fn set_client_focus(&mut self, client_id: String, focus: ClientFocus) -> Result<Vec<Vec<u8>>> {
        let was_focused = !self.focused_clients.is_empty();
        match focus {
            ClientFocus::Focused => {
                self.focused_clients.insert(client_id);
            }
            ClientFocus::Blurred => {
                self.focused_clients.remove(&client_id);
            }
        }
        let is_focused = !self.focused_clients.is_empty();
        if was_focused == is_focused {
            return Ok(Vec::new());
        }
        let message = self
            .terminal
            .encode_focus(is_focused)
            .map_err(terminal_error)?;
        Ok((!message.is_empty())
            .then_some(message)
            .into_iter()
            .collect())
    }

    fn compress_incremental(&mut self) -> Result<CompressionProgress> {
        self.terminal.compress_incremental().map_err(terminal_error)
    }

    fn compression_activity(&mut self) -> Result<u64> {
        self.terminal.compression_activity().map_err(terminal_error)
    }

    fn shutdown_messages(&mut self) -> Result<Vec<Vec<u8>>> {
        if self.focused_clients.is_empty() {
            return Ok(Vec::new());
        }
        self.focused_clients.clear();
        let message = self.terminal.encode_focus(false).map_err(terminal_error)?;
        Ok((!message.is_empty())
            .then_some(message)
            .into_iter()
            .collect())
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
        Effect::ClipboardWrite { location, contents } => {
            if location != ClipboardLocation::Standard {
                return None;
            }
            if contents.is_empty() {
                return Some(TerminalEffect::ClipboardText(String::new()));
            }
            contents.into_iter().find_map(|content| {
                let mime = str::from_utf8(&content.mime).ok()?;
                if mime != "text/plain" && mime != "text/plain;charset=utf-8" {
                    return None;
                }
                String::from_utf8(content.data)
                    .ok()
                    .map(TerminalEffect::ClipboardText)
            })
        }
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
    rows: u16,
    cols: u16,
    initial_sequence: u64,
    supports_osc52_clipboard: bool,
    history: TerminalHistoryPolicy,
    initial_theme_dark: bool,
    init_tx: std::sync::mpsc::SyncSender<Result<()>>,
) {
    let terminal = SessionTerminal::new(
        rows,
        cols,
        initial_sequence,
        supports_osc52_clipboard,
        history,
        initial_theme_dark,
    );
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
                    TerminalCommand::PresentationData { reply_tx } => {
                        let result = terminal.presentation_data();
                        if result.is_ok() {
                            compression.restart_idle();
                        }
                        drop(reply_tx.send(result));
                    }
                    TerminalCommand::EncodeInput { input, reply_tx } => {
                        drop(reply_tx.send(terminal.encode_input(input)));
                    }
                    TerminalCommand::SetFocused {
                        client_id,
                        focus,
                        reply_tx,
                    } => {
                        drop(reply_tx.send(terminal.set_client_focus(client_id, focus)));
                    }
                    TerminalCommand::SetClipboardSupport {
                        supported,
                        reply_tx,
                    } => {
                        drop(reply_tx.send(terminal.set_clipboard_support(supported)));
                    }
                    TerminalCommand::ThemeChanged { dark, reply_tx } => {
                        drop(reply_tx.send(terminal.theme_changed(dark)));
                    }
                    TerminalCommand::Shutdown { reply_tx } => {
                        drop(reply_tx.send(terminal.shutdown_messages()));
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
mod tests {
    use super::*;

    #[tokio::test]
    async fn initial_light_theme_answers_first_color_scheme_query() -> Result<()> {
        let terminal = SessionTerminalHandle::spawn_with_theme(
            4,
            12,
            0,
            false,
            TerminalHistoryPolicy::default(),
            false,
        )?;

        let output = terminal
            .process_output(bytes::Bytes::from_static(b"\x1b[?996n"))
            .await?;
        assert!(output.effects.iter().any(
            |effect| matches!(effect, TerminalEffect::PtyWrite(bytes) if bytes == b"\x1b[?997;2n")
        ));
        Ok(())
    }

    #[tokio::test]
    async fn resize_returns_ghostty_in_band_size_report() -> Result<()> {
        let terminal = SessionTerminalHandle::spawn_with_theme(
            24,
            80,
            0,
            false,
            TerminalHistoryPolicy::default(),
            true,
        )?;
        terminal
            .process_output(bytes::Bytes::from_static(b"\x1b[?2048h"))
            .await?;

        let writes = terminal.resize(30, 100, None).await?;

        assert_eq!(writes, [b"\x1b[48;30;100;0;0t".to_vec()]);
        Ok(())
    }

    #[tokio::test]
    async fn bell_and_title_effects_keep_ghostty_stream_order() -> Result<()> {
        let terminal = SessionTerminalHandle::spawn_with_theme(
            4,
            20,
            0,
            false,
            TerminalHistoryPolicy::default(),
            true,
        )?;

        let processed = terminal
            .process_output(bytes::Bytes::from_static(
                b"\x1b]2;first\x07\x07\x1b]2;second\x07\x07",
            ))
            .await?;

        assert_eq!(
            processed.effects,
            vec![
                TerminalEffect::Title("first".to_owned()),
                TerminalEffect::Bell,
                TerminalEffect::Title("second".to_owned()),
                TerminalEffect::Bell,
            ]
        );
        Ok(())
    }

    #[tokio::test]
    async fn applied_sequence_starts_from_hub_baseline() -> Result<()> {
        let terminal = SessionTerminalHandle::spawn_with_theme(
            4,
            12,
            41,
            false,
            TerminalHistoryPolicy::default(),
            true,
        )?;
        let fresh = terminal.checkpoint_data().await?;
        assert_eq!(fresh.applied_sequence, 41);
        terminal
            .process_output(bytes::Bytes::from_static(b"x"))
            .await?;
        assert_eq!(terminal.checkpoint_data().await?.applied_sequence, 42);
        Ok(())
    }

    #[tokio::test]
    async fn actor_captures_semantic_authority_without_vt_replay_fields() -> Result<()> {
        let terminal = SessionTerminalHandle::spawn_with_theme(
            4,
            12,
            0,
            false,
            TerminalHistoryPolicy::default(),
            true,
        )?;
        terminal
            .process_output(bytes::Bytes::from_static(b"primary\x1b[?1049h\x1b[Halt"))
            .await?;
        let checkpoint = terminal.checkpoint_data().await?.checkpoint;
        assert_eq!(checkpoint.schema_version(), 2);
        assert_eq!(checkpoint.active_screen, TerminalScreen::Alternate);
        assert!(!checkpoint.semantic_checkpoint.is_empty());
        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn compression_activity_restarts_idle_deadline() {
        let idle = Duration::from_secs(2);
        let mut scheduler = CompressionScheduler::new(4, idle);
        assert_eq!(scheduler.deadline(), None);

        scheduler.observe(4);
        assert_eq!(scheduler.deadline(), None);
        scheduler.observe(5);
        assert_eq!(scheduler.deadline(), Some(Instant::now() + idle));

        time::advance(Duration::from_secs(1)).await;
        scheduler.observe(6);
        assert_eq!(scheduler.deadline(), Some(Instant::now() + idle));
    }

    #[tokio::test(start_paused = true)]
    async fn checkpoint_access_restarts_compression_idle_deadline() {
        let idle = Duration::from_secs(2);
        let mut scheduler = CompressionScheduler::new(4, idle);
        scheduler.restart_idle();
        assert_eq!(scheduler.deadline(), Some(Instant::now() + idle));
    }

    #[tokio::test(start_paused = true)]
    async fn compression_progress_controls_continuation() {
        let mut scheduler = CompressionScheduler::new(0, Duration::from_secs(2));
        scheduler.observe(1);

        scheduler.step(CompressionProgress::Pending);
        assert_eq!(scheduler.deadline(), Some(Instant::now()));
        scheduler.step(CompressionProgress::Complete);
        assert_eq!(scheduler.deadline(), None);

        scheduler.observe(2);
        scheduler.step(CompressionProgress::Unsupported);
        assert_eq!(scheduler.deadline(), None);
    }

    #[test]
    fn cwd_normalization_is_engine_neutral() {
        assert_eq!(
            parse_cwd(b"file://localhost/tmp/with%20space"),
            Some(PathBuf::from("/tmp/with space"))
        );
        assert_eq!(
            parse_cwd(b"/tmp/project"),
            Some(PathBuf::from("/tmp/project"))
        );
        assert_eq!(parse_cwd(b"file://host/foo%2"), None);
    }

    #[test]
    fn clipboard_clear_is_preserved_as_empty_text() {
        assert_eq!(
            convert_effect(Effect::ClipboardWrite {
                location: ClipboardLocation::Standard,
                contents: Vec::new(),
            }),
            Some(TerminalEffect::ClipboardText(String::new()))
        );
    }

    #[tokio::test]
    async fn sequence_exhaustion_is_reserved_before_terminal_mutation() -> Result<()> {
        let terminal = SessionTerminalHandle::spawn_with_theme(
            4,
            12,
            u64::MAX,
            false,
            TerminalHistoryPolicy::default(),
            true,
        )?;
        let before = terminal.presentation_data().await?.presentation;
        let error = terminal
            .process_output(bytes::Bytes::from_static(b"must not appear"))
            .await
            .err()
            .unwrap_or_else(|| panic!("exhausted sequence must fail"));
        assert!(error.to_string().contains("sequence exhausted"));
        assert_eq!(terminal.presentation_data().await?.presentation, before);
        Ok(())
    }
}
