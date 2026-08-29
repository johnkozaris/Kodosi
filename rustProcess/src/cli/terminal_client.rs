use std::{
    collections::VecDeque,
    future,
    io::{self, Write},
    time::Duration,
};

use bytes::Bytes;
use ghostty_vt::{CheckpointLimits, SemanticCheckpoint, Terminal, TerminalPolicy};
use kodosi_domain::terminal::TerminalCheckpointV2;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite},
    time::{Instant, sleep_until},
};

use crate::{
    AppError, Result, headless_host,
    support::io::framed_json::{self, FramedJsonReader, FramedJsonWriter},
    terminal_transport::{CaptureBuffer, TerminalDataSequenceDecision, classify_data_frame},
};

use super::args::SessionAttachArgs;

const ENTER_ALTERNATE_SCREEN: &[u8] = b"\x1b[?1049h";
const EXIT_ALTERNATE_SCREEN: &[u8] = b"\x1b[?1049l";
const STDIN_BUFFER_SIZE: usize = 4096;
const RESIZE_DEBOUNCE: Duration = Duration::from_millis(50);
const INITIAL_CHECKPOINT_TIMEOUT: Duration = Duration::from_secs(2);
const RENDER_IDLE: Duration = Duration::from_millis(2);
const RENDER_BUDGET: Duration = Duration::from_millis(16);
const SYNC_BEGIN: &[u8] = b"\x1b[?2026h";
const SYNC_END: &[u8] = b"\x1b[?2026l";

#[expect(
    clippy::future_not_send,
    reason = "the attach command owns its Ghostty mirror on the CLI current-thread runtime"
)]
pub(in crate::cli) async fn run_session_attach(args: SessionAttachArgs) -> Result<()> {
    ensure_remote_session_opened(&args.session_id).await?;
    let connection = headless_host::connect_terminal_capture_lane(args.session_id).await?;
    let can_write = connection.can_write();
    if !can_write {
        io::stderr()
            .write_all(b"Kodosi: attached read-only; input and resize are disabled.\n")
            .map_err(AppError::Io)?;
    }
    run_attach_loop(
        connection.reader,
        connection.writer,
        io::stdout(),
        can_write,
    )
    .await
}

async fn ensure_remote_session_opened(session_id: &str) -> Result<()> {
    let Some(mut host) = headless_host::connect_existing_host().await? else {
        return Ok(());
    };
    let snapshot = host.snapshot().await?;
    let Some(entry) = snapshot
        .sessions
        .iter()
        .find(|entry| entry.id() == session_id)
    else {
        return Ok(());
    };
    if !entry.is_remote() {
        return Ok(());
    }
    host.open_remote_session(session_id).await
}

#[derive(Debug)]
pub(in crate::cli) struct CapturedTerminalOutput {
    pub(in crate::cli) bytes: Vec<u8>,
    pub(in crate::cli) total_bytes_seen: usize,
}

pub(in crate::cli) async fn send_session_input(session_id: String, input: Vec<u8>) -> Result<()> {
    let connection = headless_host::connect_terminal_capture_lane(session_id).await?;
    ensure_terminal_write_capability(connection.can_write(), "session input")?;
    let mut reader = connection.reader;
    let mut writer = connection.writer;

    let _next_sequence = wait_for_initial_checkpoint(&mut reader).await?;
    write_client_frame(
        &mut writer,
        headless_host::terminal_lane::TerminalLaneClientFrame::Input {
            bytes: Bytes::from(input),
        },
    )
    .await
}

pub(in crate::cli) async fn capture_session_command(
    session_id: String,
    input: Vec<u8>,
    timeout: Duration,
    max_bytes: usize,
) -> Result<CapturedTerminalOutput> {
    let connection = headless_host::connect_terminal_capture_lane(session_id).await?;
    ensure_terminal_write_capability(connection.can_write(), "session run")?;
    capture_terminal_window(
        connection.reader,
        connection.writer,
        input,
        timeout,
        max_bytes,
    )
    .await
}

fn enter_terminal_mode(restore_guard: &mut TerminalRestoreGuard) -> Result<()> {
    let mut stdout = io::stdout();
    stdout.write_all(ENTER_ALTERNATE_SCREEN)?;
    stdout.flush()?;
    restore_guard.alternate_screen = true;
    crossterm::terminal::enable_raw_mode().map_err(AppError::Io)?;
    restore_guard.raw_mode = true;
    Ok(())
}

#[derive(Default)]
struct TerminalRestoreGuard {
    alternate_screen: bool,
    raw_mode: bool,
}

impl Drop for TerminalRestoreGuard {
    fn drop(&mut self) {
        if self.raw_mode {
            drop(crossterm::terminal::disable_raw_mode());
        }
        if self.alternate_screen {
            let mut stdout = io::stdout();
            drop(stdout.write_all(EXIT_ALTERNATE_SCREEN));
            drop(stdout.flush());
        }
    }
}

#[expect(
    clippy::future_not_send,
    reason = "one current-thread CLI task owns the non-Send Ghostty mirror"
)]
async fn run_attach_loop<R, W>(
    mut reader: FramedJsonReader<R>,
    mut writer: FramedJsonWriter<W>,
    mut stdout: io::Stdout,
    can_write: bool,
) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let initial = tokio::time::timeout(INITIAL_CHECKPOINT_TIMEOUT, read_server_frame(&mut reader))
        .await
        .map_err(|_| {
            terminal_sequence_error("timed out waiting for terminal bootstrap".to_owned())
        })??
        .ok_or_else(|| {
            terminal_sequence_error("terminal lane closed before bootstrap".to_owned())
        })?;
    let mut stream = AttachTerminalStream::from_initial(initial)?;

    let mut restore_guard = TerminalRestoreGuard::default();
    enter_terminal_mode(&mut restore_guard)?;
    stream.render_now(&mut stdout)?;

    let mut stdin = tokio::io::stdin();
    let mut stdin_buf = [0_u8; STDIN_BUFFER_SIZE];
    let mut detach_parser = DetachGestureParser::new();
    let mut resize_signal = resize_signal()?;
    let mut resize_throttle = ResizeThrottle::new(RESIZE_DEBOUNCE);
    if can_write && resize_throttle.record_resize(Instant::now()) == ResizeDecision::SendNow {
        send_current_resize(&mut writer).await?;
    }

    loop {
        tokio::select! {
            server_frame = read_server_frame(&mut reader) => {
                let Some(frame) = server_frame? else {
                    return Err(terminal_sequence_error(
                        "terminal lane closed without an exact Closed frame".to_owned(),
                    ));
                };
                if !stream.handle(frame)? {
                    stream.render_if_dirty(&mut stdout)?;
                    return Ok(());
                }
            }
            read_result = stdin.read(&mut stdin_buf) => {
                let bytes_read = read_result.map_err(AppError::Io)?;
                if bytes_read == 0 {
                    return Ok(());
                }
                let result = detach_parser.process(&stdin_buf[..bytes_read]);
                if can_write && !result.forwarded.is_empty() {
                    write_client_frame(
                        &mut writer,
                        headless_host::terminal_lane::TerminalLaneClientFrame::Input {
                            bytes: Bytes::from(result.forwarded),
                        },
                    ).await?;
                }
                if result.detach {
                    return Ok(());
                }
            }
            () = recv_resize_signal(&mut resize_signal), if can_write => {
                if resize_throttle.record_resize(Instant::now()) == ResizeDecision::SendNow {
                    send_current_resize(&mut writer).await?;
                }
            }
            () = wait_resize_deadline(resize_throttle.pending_deadline()), if can_write && resize_throttle.has_pending() => {
                if resize_throttle.take_due(Instant::now()) {
                    send_current_resize(&mut writer).await?;
                }
            }
            () = wait_render_deadline(stream.render_deadline()), if stream.render_pending() => {
                stream.render_if_due(&mut stdout)?;
            }
        }
    }
}

fn ensure_terminal_write_capability(can_write: bool, operation: &str) -> Result<()> {
    if can_write {
        Ok(())
    } else {
        Err(AppError::Unsupported {
            reason: format!(
                "{operation} requires writable terminal access; this session is read-only"
            ),
        })
    }
}

async fn capture_terminal_window<R, W>(
    mut reader: FramedJsonReader<R>,
    mut writer: FramedJsonWriter<W>,
    input: Vec<u8>,
    timeout: Duration,
    max_bytes: usize,
) -> Result<CapturedTerminalOutput>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let initial_sequence = wait_for_initial_checkpoint(&mut reader).await?;
    write_client_frame(
        &mut writer,
        headless_host::terminal_lane::TerminalLaneClientFrame::Input {
            bytes: Bytes::from(input),
        },
    )
    .await?;

    let mut capture = CaptureBuffer::new(max_bytes);
    let mut stream = CaptureLaneStream::new(initial_sequence);
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let read_result = tokio::time::timeout(remaining, read_server_frame(&mut reader)).await;
        let Some(message) = (match read_result {
            Ok(result) => result?,
            Err(_) => break,
        }) else {
            return Err(terminal_sequence_error(
                "terminal capture lane closed without an exact Closed frame".to_owned(),
            ));
        };
        match message {
            headless_host::terminal_lane::TerminalLaneServerFrame::Data { sequence, bytes } => {
                stream.accept_data(sequence)?;
                capture.accept(&crate::terminal_transport::TerminalDataFrame::new(
                    sequence, bytes,
                ));
                if capture.truncated() {
                    return Err(AppError::Unsupported {
                        reason: format!(
                            "session.run captured more than {max_bytes} bytes; increase --max-bytes"
                        ),
                    });
                }
            }
            headless_host::terminal_lane::TerminalLaneServerFrame::Closed {
                reason,
                final_sequence,
            } => {
                stream.finish(final_sequence)?;
                return Err(AppError::Unsupported {
                    reason: format!("terminal lane closed: {reason}"),
                });
            }
            headless_host::terminal_lane::TerminalLaneServerFrame::LocalCheckpoint { .. } => {
                return Err(terminal_sequence_error(
                    "terminal capture received a replacement frame after command start".to_owned(),
                ));
            }
            headless_host::terminal_lane::TerminalLaneServerFrame::Resize {
                at_sequence, ..
            } => {
                stream.accept_resize(at_sequence)?;
            }
        }
    }

    stream.finish_timeout()?;
    Ok(CapturedTerminalOutput {
        total_bytes_seen: capture.total_bytes_seen(),
        bytes: capture.into_bytes().to_vec(),
    })
}

async fn wait_for_initial_checkpoint<R>(reader: &mut FramedJsonReader<R>) -> Result<u64>
where
    R: AsyncRead + Unpin,
{
    loop {
        let read_result =
            tokio::time::timeout(INITIAL_CHECKPOINT_TIMEOUT, read_server_frame(reader)).await;
        let Some(message) = (match read_result {
            Ok(result) => result?,
            Err(_) => {
                return Err(AppError::Unsupported {
                    reason: "timed out waiting for terminal checkpoint".to_owned(),
                });
            }
        }) else {
            return Err(AppError::Unsupported {
                reason: "terminal lane closed before checkpoint".to_owned(),
            });
        };
        match message {
            headless_host::terminal_lane::TerminalLaneServerFrame::LocalCheckpoint {
                next_sequence,
                ..
            } => return Ok(next_sequence),
            headless_host::terminal_lane::TerminalLaneServerFrame::Closed { reason, .. } => {
                return Err(AppError::Unsupported {
                    reason: format!("terminal lane closed: {reason}"),
                });
            }
            headless_host::terminal_lane::TerminalLaneServerFrame::Data { .. }
            | headless_host::terminal_lane::TerminalLaneServerFrame::Resize { .. } => {}
        }
    }
}

struct CaptureLaneStream {
    next_sequence: Option<u64>,
    pending_resize_boundaries: VecDeque<u64>,
}

impl CaptureLaneStream {
    fn new(next_sequence: u64) -> Self {
        Self {
            next_sequence: Some(next_sequence),
            pending_resize_boundaries: VecDeque::new(),
        }
    }

    fn accept_resize(&mut self, at_sequence: u64) -> Result<()> {
        let Some(expected) = self.next_sequence else {
            return Err(terminal_sequence_error(
                "terminal capture resize arrived before a snapshot".to_owned(),
            ));
        };
        if at_sequence < expected {
            return Err(terminal_sequence_error(format!(
                "terminal resize boundary {at_sequence} is behind next data sequence {expected}"
            )));
        }
        if let Some(pending) = self.pending_resize_boundaries.back()
            && at_sequence < *pending
        {
            return Err(terminal_sequence_error(format!(
                "terminal resize boundary {at_sequence} regressed behind pending boundary {pending}"
            )));
        }
        self.pending_resize_boundaries.push_back(at_sequence);
        self.consume_boundaries(expected);
        Ok(())
    }

    fn accept_data(&mut self, sequence: u64) -> Result<()> {
        self.reject_crossed_boundary(sequence)?;
        match classify_data_frame(sequence, &mut self.next_sequence) {
            TerminalDataSequenceDecision::Exact => {
                self.consume_boundaries(sequence);
                Ok(())
            }
            TerminalDataSequenceDecision::Stale => Err(terminal_sequence_error(format!(
                "terminal data sequence {sequence} predates the active snapshot"
            ))),
            TerminalDataSequenceDecision::Gap { expected, actual } => Err(terminal_sequence_error(
                format!("terminal data sequence gap: expected {expected}, received {actual}"),
            )),
            TerminalDataSequenceDecision::Exhausted => Err(terminal_sequence_error(
                "terminal data sequence exhausted".to_owned(),
            )),
        }
    }

    fn finish(&self, final_sequence: u64) -> Result<()> {
        self.finish_pending_resize()?;
        let cursor = self.next_sequence.unwrap_or(0);
        if cursor == final_sequence {
            Ok(())
        } else {
            Err(terminal_sequence_error(format!(
                "terminal closed at sequence {final_sequence}, but capture expected {cursor}"
            )))
        }
    }

    fn finish_timeout(&self) -> Result<()> {
        self.finish_pending_resize()
    }

    fn finish_pending_resize(&self) -> Result<()> {
        self.pending_resize_boundaries.front().map_or_else(
            || Ok(()),
            |boundary| {
                Err(terminal_sequence_error(format!(
                    "terminal capture ended before resize boundary {boundary}"
                )))
            },
        )
    }

    fn reject_crossed_boundary(&self, sequence: u64) -> Result<()> {
        if let Some(boundary) = self.pending_resize_boundaries.front()
            && sequence > *boundary
        {
            return Err(terminal_sequence_error(format!(
                "terminal crossed resize boundary {boundary} at data sequence {sequence}"
            )));
        }
        Ok(())
    }

    fn consume_boundaries(&mut self, sequence: u64) {
        while self.pending_resize_boundaries.front() == Some(&sequence) {
            self.pending_resize_boundaries.pop_front();
        }
    }
}

enum AttachMode {
    Mirror(Terminal),
}

struct AttachTerminalStream {
    mode: AttachMode,
    next_sequence: Option<u64>,
    pending_resizes: VecDeque<PendingResize>,
    first_dirty: Option<Instant>,
    idle_deadline: Option<Instant>,
    last_render: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingResize {
    rows: u16,
    cols: u16,
    at_sequence: u64,
}

impl AttachTerminalStream {
    fn from_initial(frame: headless_host::terminal_lane::TerminalLaneServerFrame) -> Result<Self> {
        match frame {
            headless_host::terminal_lane::TerminalLaneServerFrame::LocalCheckpoint {
                checkpoint,
                next_sequence,
            } => {
                let mut terminal = Terminal::new(
                    checkpoint.cols(),
                    checkpoint.rows(),
                    TerminalPolicy {
                        clipboard_enabled: false,
                        ..TerminalPolicy::default()
                    },
                )
                .map_err(terminal_mirror_error)?;
                terminal
                    .restore_semantic_checkpoint(
                        &SemanticCheckpoint::from(checkpoint.semantic_checkpoint.clone()),
                        CheckpointLimits::default(),
                    )
                    .map_err(terminal_mirror_error)?;
                validate_mirror_metadata(&mut terminal, &checkpoint)?;
                Ok(Self {
                    mode: AttachMode::Mirror(terminal),
                    next_sequence: Some(next_sequence),
                    pending_resizes: VecDeque::new(),
                    first_dirty: Some(Instant::now()),
                    idle_deadline: Some(Instant::now()),
                    last_render: Vec::new(),
                })
            }
            headless_host::terminal_lane::TerminalLaneServerFrame::Closed { reason, .. } => Err(
                terminal_sequence_error(format!("terminal lane closed: {reason}")),
            ),
            headless_host::terminal_lane::TerminalLaneServerFrame::Data { .. }
            | headless_host::terminal_lane::TerminalLaneServerFrame::Resize { .. } => Err(
                terminal_sequence_error("terminal data arrived before bootstrap".to_owned()),
            ),
        }
    }

    fn handle(
        &mut self,
        frame: headless_host::terminal_lane::TerminalLaneServerFrame,
    ) -> Result<bool> {
        match frame {
            headless_host::terminal_lane::TerminalLaneServerFrame::LocalCheckpoint { .. } => Err(
                terminal_sequence_error("unexpected replacement local checkpoint".to_owned()),
            ),
            headless_host::terminal_lane::TerminalLaneServerFrame::Data { sequence, bytes } => {
                match classify_data_frame(sequence, &mut self.next_sequence) {
                    TerminalDataSequenceDecision::Stale => return Ok(true),
                    TerminalDataSequenceDecision::Exact => {}
                    TerminalDataSequenceDecision::Gap { expected, actual } => {
                        return Err(terminal_sequence_error(format!(
                            "terminal data sequence gap: expected {expected}, received {actual}"
                        )));
                    }
                    TerminalDataSequenceDecision::Exhausted => {
                        return Err(terminal_sequence_error(
                            "terminal data sequence exhausted".to_owned(),
                        ));
                    }
                }
                self.apply_resize_at(sequence)?;
                let AttachMode::Mirror(terminal) = &mut self.mode;
                drop(terminal.write(&bytes).map_err(terminal_mirror_error)?);
                self.mark_dirty();
                Ok(true)
            }
            headless_host::terminal_lane::TerminalLaneServerFrame::Resize {
                rows,
                cols,
                at_sequence,
            } => {
                self.queue_resize(PendingResize {
                    rows,
                    cols,
                    at_sequence,
                })?;
                self.apply_resize_at_current_boundary()?;
                Ok(true)
            }
            headless_host::terminal_lane::TerminalLaneServerFrame::Closed {
                reason,
                final_sequence,
            } => {
                if let Some(resize) = self.pending_resizes.front() {
                    return Err(terminal_sequence_error(format!(
                        "terminal lane closed before resize boundary {}: {reason}",
                        resize.at_sequence
                    )));
                }
                let cursor = self.next_sequence.unwrap_or(0);
                if cursor != final_sequence {
                    return Err(terminal_sequence_error(format!(
                        "terminal closed at sequence {final_sequence}, but CLI expected {cursor}"
                    )));
                }
                Ok(false)
            }
        }
    }

    fn queue_resize(&mut self, resize: PendingResize) -> Result<()> {
        let Some(expected) = self.next_sequence else {
            return Err(terminal_sequence_error(
                "terminal resize arrived before bootstrap".to_owned(),
            ));
        };
        if resize.at_sequence < expected {
            return Err(terminal_sequence_error(format!(
                "terminal resize boundary {} is behind next data sequence {expected}",
                resize.at_sequence
            )));
        }
        if let Some(pending) = self.pending_resizes.back()
            && resize.at_sequence < pending.at_sequence
        {
            return Err(terminal_sequence_error(format!(
                "terminal resize boundary {} regressed behind pending boundary {}",
                resize.at_sequence, pending.at_sequence
            )));
        }
        self.pending_resizes.push_back(resize);
        Ok(())
    }

    fn apply_resize_at_current_boundary(&mut self) -> Result<()> {
        let Some(sequence) = self.next_sequence else {
            return Ok(());
        };
        self.apply_resize_at(sequence)
    }

    fn apply_resize_at(&mut self, sequence: u64) -> Result<()> {
        while let Some(resize) = self.pending_resizes.front().copied() {
            if sequence < resize.at_sequence {
                return Ok(());
            }
            if sequence > resize.at_sequence {
                return Err(terminal_sequence_error(format!(
                    "terminal crossed resize boundary {} at data sequence {sequence}",
                    resize.at_sequence
                )));
            }
            let AttachMode::Mirror(terminal) = &mut self.mode;
            drop(
                terminal
                    .resize(resize.cols, resize.rows, 0, 0)
                    .map_err(terminal_mirror_error)?,
            );
            self.pending_resizes.pop_front();
            self.mark_dirty();
        }
        Ok(())
    }

    fn mark_dirty(&mut self) {
        let now = Instant::now();
        self.first_dirty.get_or_insert(now);
        self.idle_deadline = Some(now + RENDER_IDLE);
    }

    const fn render_pending(&self) -> bool {
        self.first_dirty.is_some()
    }

    fn render_deadline(&self) -> Option<Instant> {
        Some(std::cmp::min(
            self.idle_deadline?,
            self.first_dirty? + RENDER_BUDGET,
        ))
    }

    fn render_if_due<W: Write>(&mut self, output: &mut W) -> Result<()> {
        if self
            .render_deadline()
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.render_now(output)?;
        }
        Ok(())
    }

    fn render_if_dirty<W: Write>(&mut self, output: &mut W) -> Result<()> {
        if self.render_pending() {
            self.render_now(output)?;
        }
        Ok(())
    }

    fn render_now<W: Write>(&mut self, output: &mut W) -> Result<()> {
        let AttachMode::Mirror(terminal) = &mut self.mode;
        let frame = terminal
            .format_finite_cli_replay()
            .map_err(terminal_mirror_error)?;
        self.first_dirty = None;
        self.idle_deadline = None;
        if frame == self.last_render {
            return Ok(());
        }
        output.write_all(SYNC_BEGIN)?;
        let write_result = output.write_all(&frame);
        let end_result = output.write_all(SYNC_END);
        write_result?;
        end_result?;
        output.flush()?;
        self.last_render = frame;
        Ok(())
    }
}

async fn wait_render_deadline(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => sleep_until(deadline).await,
        None => future::pending().await,
    }
}

fn validate_mirror_metadata(
    terminal: &mut Terminal,
    checkpoint: &TerminalCheckpointV2,
) -> Result<()> {
    let state = terminal.state().map_err(terminal_mirror_error)?;
    let active_screen = match state.active_screen {
        ghostty_vt::Screen::Primary => kodosi_domain::terminal::TerminalScreen::Primary,
        ghostty_vt::Screen::Alternate => kodosi_domain::terminal::TerminalScreen::Alternate,
    };
    if state.rows != checkpoint.rows()
        || state.cols != checkpoint.cols()
        || active_screen != checkpoint.active_screen
        || state.cursor_x != checkpoint.cursor_x()
        || state.cursor_y != checkpoint.cursor_y()
        || state.cursor_visible == checkpoint.cursor_hidden()
    {
        return Err(terminal_sequence_error(
            "terminal checkpoint metadata disagrees with restored mirror".to_owned(),
        ));
    }
    Ok(())
}

fn terminal_mirror_error(error: ghostty_vt::Error) -> AppError {
    AppError::Unsupported {
        reason: format!("Ghostty CLI mirror failed: {error}"),
    }
}

async fn read_server_frame<R>(
    reader: &mut FramedJsonReader<R>,
) -> Result<Option<headless_host::terminal_lane::TerminalLaneServerFrame>>
where
    R: AsyncRead + Unpin,
{
    let Some(bytes) = framed_json::read_frame(reader).await? else {
        return Ok(None);
    };
    headless_host::terminal_lane::TerminalLaneServerFrame::decode(&bytes).map(Some)
}

async fn write_client_frame<W>(
    writer: &mut FramedJsonWriter<W>,
    frame: headless_host::terminal_lane::TerminalLaneClientFrame,
) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    framed_json::write_frame(writer, frame.encode()?).await
}

fn terminal_sequence_error(reason: String) -> AppError {
    AppError::Unsupported { reason }
}

async fn send_current_resize<W>(writer: &mut FramedJsonWriter<W>) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let (cols, rows) = crossterm::terminal::size().map_err(AppError::Io)?;
    write_client_frame(
        writer,
        headless_host::terminal_lane::TerminalLaneClientFrame::Resize { rows, cols },
    )
    .await
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetachState {
    Idle,
    WaitD(u8),
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct DetachGestureResult {
    forwarded: Vec<u8>,
    detach: bool,
}

#[derive(Debug, Clone, Copy)]
struct DetachGestureParser {
    state: DetachState,
}

impl DetachGestureParser {
    fn new() -> Self {
        Self {
            state: DetachState::Idle,
        }
    }

    fn process(&mut self, bytes: &[u8]) -> DetachGestureResult {
        let mut result = DetachGestureResult::default();
        for &byte in bytes {
            match self.state {
                DetachState::Idle => self.handle_idle_byte(byte, &mut result.forwarded),
                DetachState::WaitD(_) if byte == b'd' => {
                    self.state = DetachState::Idle;
                    result.detach = true;
                    break;
                }
                DetachState::WaitD(prefix) => {
                    result.forwarded.push(prefix);
                    self.state = DetachState::Idle;
                    self.handle_idle_byte(byte, &mut result.forwarded);
                }
            }
        }
        result
    }

    fn handle_idle_byte(&mut self, byte: u8, forwarded: &mut Vec<u8>) {
        if is_detach_prefix(byte) {
            self.state = DetachState::WaitD(byte);
        } else {
            forwarded.push(byte);
        }
    }
}

impl Default for DetachGestureParser {
    fn default() -> Self {
        Self::new()
    }
}

fn is_detach_prefix(byte: u8) -> bool {
    matches!(byte, 0x0f | 0x02)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResizeDecision {
    SendNow,
    Pending { deadline: Instant },
}

#[derive(Debug, Clone)]
struct ResizeThrottle {
    debounce: Duration,
    last_sent: Option<Instant>,
    pending_deadline: Option<Instant>,
}

impl ResizeThrottle {
    fn new(debounce: Duration) -> Self {
        Self {
            debounce,
            last_sent: None,
            pending_deadline: None,
        }
    }

    fn record_resize(&mut self, now: Instant) -> ResizeDecision {
        let Some(last_sent) = self.last_sent else {
            self.last_sent = Some(now);
            self.pending_deadline = None;
            return ResizeDecision::SendNow;
        };

        let elapsed = now.saturating_duration_since(last_sent);
        if elapsed >= self.debounce {
            self.last_sent = Some(now);
            self.pending_deadline = None;
            ResizeDecision::SendNow
        } else {
            let deadline = last_sent + self.debounce;
            self.pending_deadline = Some(deadline);
            ResizeDecision::Pending { deadline }
        }
    }

    fn pending_deadline(&self) -> Option<Instant> {
        self.pending_deadline
    }

    fn has_pending(&self) -> bool {
        self.pending_deadline.is_some()
    }

    fn take_due(&mut self, now: Instant) -> bool {
        let Some(deadline) = self.pending_deadline else {
            return false;
        };
        if now < deadline {
            return false;
        }
        self.pending_deadline = None;
        self.last_sent = Some(now);
        true
    }
}

async fn wait_resize_deadline(deadline: Option<Instant>) {
    let Some(deadline) = deadline else {
        future::pending::<()>().await;
        return;
    };
    sleep_until(deadline).await;
}

type ResizeSignal = tokio::signal::unix::Signal;

fn resize_signal() -> Result<ResizeSignal> {
    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::window_change())
        .map_err(AppError::Io)
}

async fn recv_resize_signal(signal: &mut ResizeSignal) {
    let _ = signal.recv().await;
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use kodosi_domain::terminal::{TerminalCheckpointV2, TerminalScreen, TerminalSize};
    use tokio::io::duplex;
    use tokio::time::Duration;

    use super::{
        CaptureLaneStream, DetachGestureParser, RESIZE_DEBOUNCE, ResizeDecision, ResizeThrottle,
        capture_terminal_window, ensure_terminal_write_capability, is_detach_prefix,
    };
    use crate::headless_host::terminal_lane::{TerminalLaneClientFrame, TerminalLaneServerFrame};
    use crate::support::io::framed_json;
    use tokio::time::Instant;

    fn checkpoint() -> TerminalCheckpointV2 {
        TerminalCheckpointV2::new(
            TerminalSize::new(24, 80).expect("size"),
            TerminalScreen::Primary,
            b"checkpoint".to_vec(),
            0,
            0,
            false,
        )
        .expect("checkpoint")
    }

    async fn send_server_frame<W: tokio::io::AsyncWrite + Unpin>(
        writer: &mut framed_json::FramedJsonWriter<W>,
        frame: TerminalLaneServerFrame,
    ) {
        framed_json::write_frame(writer, frame.encode().expect("encode"))
            .await
            .expect("write frame");
    }

    async fn read_client_frame<R: tokio::io::AsyncRead + Unpin>(
        reader: &mut framed_json::FramedJsonReader<R>,
    ) -> TerminalLaneClientFrame {
        let bytes = framed_json::read_frame(reader)
            .await
            .expect("read")
            .expect("frame");
        TerminalLaneClientFrame::decode(&bytes).expect("decode")
    }

    #[test]
    fn detach_parser_passes_regular_bytes() {
        let mut parser = DetachGestureParser::new();
        let result = parser.process(b"hello\r\n");
        assert_eq!(result.forwarded, b"hello\r\n");
        assert!(!result.detach);
    }

    #[test]
    fn detach_parser_detects_ctrl_o_d() {
        let mut parser = DetachGestureParser::new();
        let result = parser.process(&[0x0f, b'd']);
        assert!(result.forwarded.is_empty());
        assert!(result.detach);
    }

    #[test]
    fn detach_parser_detects_ctrl_b_d() {
        let mut parser = DetachGestureParser::new();
        let result = parser.process(&[0x02, b'd']);
        assert!(result.forwarded.is_empty());
        assert!(result.detach);
    }

    #[test]
    fn detach_parser_flushes_prefix_when_next_byte_is_not_d() {
        let mut parser = DetachGestureParser::new();
        let result = parser.process(&[0x0f, b'x']);
        assert_eq!(result.forwarded, [0x0f, b'x']);
        assert!(!result.detach);
    }

    #[test]
    fn detach_parser_handles_prefix_across_chunks() {
        let mut parser = DetachGestureParser::new();
        let first = parser.process(&[0x02]);
        assert!(first.forwarded.is_empty());
        assert!(!first.detach);

        let second = parser.process(b"d");
        assert!(second.forwarded.is_empty());
        assert!(second.detach);
    }

    #[test]
    fn detach_parser_reprocesses_non_d_as_idle_byte() {
        let mut parser = DetachGestureParser::new();
        let result = parser.process(&[0x0f, 0x02, b'd']);
        assert_eq!(result.forwarded, [0x0f]);
        assert!(result.detach);
    }

    #[test]
    fn detach_parser_stops_processing_after_detach() {
        let mut parser = DetachGestureParser::new();
        let result = parser.process(&[b'a', 0x02, b'd', b'b']);
        assert_eq!(result.forwarded, b"a");
        assert!(result.detach);
    }

    #[test]
    fn detach_prefixes_are_ctrl_o_and_ctrl_b() {
        assert!(is_detach_prefix(0x0f));
        assert!(is_detach_prefix(0x02));
        assert!(!is_detach_prefix(b'd'));
    }

    #[test]
    fn mutating_cli_operations_reject_read_only_terminal_lanes() {
        let error = ensure_terminal_write_capability(false, "session input")
            .expect_err("read-only terminal must reject input");
        assert!(error.to_string().contains("read-only"));
        assert!(ensure_terminal_write_capability(true, "session input").is_ok());
    }

    #[test]
    fn capture_stream_applies_resize_at_its_exclusive_data_boundary() {
        let mut stream = CaptureLaneStream::new(5);

        stream
            .accept_resize(6)
            .expect("future resize boundary should queue");
        stream.accept_data(5).expect("pre-resize data should apply");
        assert_eq!(stream.pending_resize_boundaries.front(), Some(&6));
        stream
            .accept_data(6)
            .expect("boundary data should follow the resize");
        assert!(stream.pending_resize_boundaries.is_empty());
        assert_eq!(stream.next_sequence, Some(7));
    }

    #[test]
    fn capture_stream_fails_closed_on_data_gap() {
        let mut stream = CaptureLaneStream::new(5);

        let error = stream
            .accept_data(6)
            .expect_err("a missing sequence must not be captured")
            .to_string();
        assert!(error.contains("expected 5"));
        assert_eq!(stream.next_sequence, Some(5));
    }

    #[test]
    fn capture_stream_fails_closed_when_resize_boundary_never_arrives() {
        let mut stream = CaptureLaneStream::new(5);
        stream
            .accept_resize(7)
            .expect("future resize boundary should queue");
        stream
            .accept_data(5)
            .expect("pre-boundary data should apply");

        let error = stream
            .finish_timeout()
            .expect_err("capture cannot succeed across an unresolved resize")
            .to_string();
        assert!(error.contains("boundary 7"));
    }

    #[test]
    fn capture_stream_requires_the_exact_close_boundary() {
        let mut stream = CaptureLaneStream::new(5);
        stream.accept_data(5).expect("exact data should apply");

        assert!(stream.finish(6).is_ok());
        assert!(stream.finish(7).is_err());
    }

    #[test]
    fn resize_throttle_sends_first_resize_immediately() {
        let mut throttle = ResizeThrottle::new(RESIZE_DEBOUNCE);
        let now = Instant::now();
        assert_eq!(throttle.record_resize(now), ResizeDecision::SendNow);
        assert!(!throttle.has_pending());
    }

    #[test]
    fn resize_throttle_delays_resize_inside_debounce_window() {
        let mut throttle = ResizeThrottle::new(RESIZE_DEBOUNCE);
        let now = Instant::now();
        assert_eq!(throttle.record_resize(now), ResizeDecision::SendNow);

        let decision = throttle.record_resize(now + RESIZE_DEBOUNCE / 2);
        assert_eq!(
            decision,
            ResizeDecision::Pending {
                deadline: now + RESIZE_DEBOUNCE
            }
        );
        assert_eq!(throttle.pending_deadline(), Some(now + RESIZE_DEBOUNCE));
    }

    #[test]
    fn resize_throttle_sends_pending_when_deadline_arrives() {
        let mut throttle = ResizeThrottle::new(RESIZE_DEBOUNCE);
        let now = Instant::now();
        assert_eq!(throttle.record_resize(now), ResizeDecision::SendNow);
        std::assert_matches!(
            throttle.record_resize(now + RESIZE_DEBOUNCE / 2),
            ResizeDecision::Pending { .. }
        );

        assert!(!throttle.take_due(now + RESIZE_DEBOUNCE / 2));
        assert!(throttle.take_due(now + RESIZE_DEBOUNCE));
        assert!(!throttle.has_pending());
    }

    #[test]
    fn resize_throttle_sends_immediately_after_debounce_window() {
        let mut throttle = ResizeThrottle::new(RESIZE_DEBOUNCE);
        let now = Instant::now();
        assert_eq!(throttle.record_resize(now), ResizeDecision::SendNow);
        assert_eq!(
            throttle.record_resize(now + RESIZE_DEBOUNCE),
            ResizeDecision::SendNow
        );
        assert!(!throttle.has_pending());
    }

    #[tokio::test]
    async fn capture_terminal_window_sends_input_after_checkpoint_and_captures_data() {
        let (client_stream, server_stream) = duplex(4096);
        let (client_read, client_write) = tokio::io::split(client_stream);
        let (server_read, server_write) = tokio::io::split(server_stream);
        let mut server_reader = framed_json::reader(server_read);
        let mut server_writer = framed_json::writer(server_write);

        let capture_task = tokio::spawn(capture_terminal_window(
            framed_json::reader(client_read),
            framed_json::writer(client_write),
            b"echo hi\n".to_vec(),
            Duration::from_millis(10),
            64,
        ));

        send_server_frame(
            &mut server_writer,
            TerminalLaneServerFrame::LocalCheckpoint {
                checkpoint: checkpoint(),
                next_sequence: 0,
            },
        )
        .await;
        let input = read_client_frame(&mut server_reader).await;
        match input {
            TerminalLaneClientFrame::Input { bytes } => {
                assert_eq!(bytes.as_ref(), b"echo hi\n");
            }
            TerminalLaneClientFrame::Resize { .. } => panic!("capture should send input first"),
        }
        send_server_frame(
            &mut server_writer,
            TerminalLaneServerFrame::Data {
                sequence: 0,
                bytes: Bytes::from_static(b"ok\n"),
            },
        )
        .await;

        let captured = capture_task.await.unwrap().unwrap();
        assert_eq!(captured.bytes, b"ok\n");
        assert_eq!(captured.total_bytes_seen, 3);
    }

    #[tokio::test]
    async fn capture_terminal_window_fails_when_capture_overflows() {
        let (client_stream, server_stream) = duplex(4096);
        let (client_read, client_write) = tokio::io::split(client_stream);
        let (_, server_write) = tokio::io::split(server_stream);
        let mut server_writer = framed_json::writer(server_write);

        let capture_task = tokio::spawn(capture_terminal_window(
            framed_json::reader(client_read),
            framed_json::writer(client_write),
            b"cmd\n".to_vec(),
            Duration::from_millis(100),
            2,
        ));

        send_server_frame(
            &mut server_writer,
            TerminalLaneServerFrame::LocalCheckpoint {
                checkpoint: checkpoint(),
                next_sequence: 0,
            },
        )
        .await;
        send_server_frame(
            &mut server_writer,
            TerminalLaneServerFrame::Data {
                sequence: 0,
                bytes: Bytes::from_static(b"abc"),
            },
        )
        .await;

        let error = capture_task.await.unwrap().unwrap_err().to_string();
        assert!(error.contains("captured more than 2 bytes"));
    }
}
