use std::{
    collections::VecDeque,
    fmt,
    path::Path,
    result::Result,
    time::{Duration, Instant},
};

use kodosi_session::{
    ClientFocus, KodosiPty, SessionTerminalHandle, TerminalEffect, TerminalInput,
};
use tokio::sync::mpsc;

use crate::{
    AppError,
    session_runtime::{
        commands::SessionInput,
        coordinator_events::{emit_capture_metadata, emit_working_dir_changed},
        events::{LocalCoordinatorOrigin, RuntimeSessionEvent},
        handles::{SessionPtyInstruction, SessionScreenInstruction},
    },
};
use kodosi_domain::terminal::{TerminalPixelGeometry, TerminalSize};

use super::{
    LoopOutcome, PtyOperation, SessionRuntimeError, SessionRuntimeResult, SessionScreenOutcome,
    TerminalOperation,
    pty_io::{
        PendingPtyWrite, commit_pending_terminal_messages, enqueue_pending_terminal_messages,
        ensure_pty_queue_capacity,
    },
};

const TERMINAL_NOTIFICATION_WINDOW: Duration = Duration::from_secs(10);
const TERMINAL_NOTIFICATION_BURST: u8 = 3;
const TERMINAL_NOTIFICATION_FIELD_MAX_BYTES: usize = 512;
const TERMINAL_TITLE_MAX_BYTES: usize = 512;

pub(super) enum ProcessedPtyOutput {
    Continue {
        applied_sequence: u64,
        processing_failed: bool,
    },
    Exit(LoopOutcome),
}

pub(super) struct TerminalNotificationPolicy {
    window_started: Instant,
    emitted: u8,
}

impl Default for TerminalNotificationPolicy {
    fn default() -> Self {
        Self {
            window_started: Instant::now(),
            emitted: 0,
        }
    }
}

impl TerminalNotificationPolicy {
    fn admit(&mut self, title: &str, body: &str) -> Option<(Option<String>, Option<String>)> {
        let now = Instant::now();
        if now.duration_since(self.window_started) >= TERMINAL_NOTIFICATION_WINDOW {
            self.window_started = now;
            self.emitted = 0;
        }
        if self.emitted >= TERMINAL_NOTIFICATION_BURST {
            return None;
        }
        self.emitted += 1;
        let title = sanitize_notification_field(title);
        let body = sanitize_notification_field(body);
        Some((
            (!title.is_empty()).then_some(title),
            (!body.is_empty()).then_some(body),
        ))
    }
}

fn sanitize_notification_field(value: &str) -> String {
    sanitize_terminal_text(value, TERMINAL_NOTIFICATION_FIELD_MAX_BYTES)
}

fn sanitize_terminal_title(value: &str) -> Option<String> {
    let title = sanitize_terminal_text(value, TERMINAL_TITLE_MAX_BYTES);
    (!title.is_empty()).then_some(title)
}

fn sanitize_terminal_text(value: &str, max_bytes: usize) -> String {
    let mut output = String::new();
    for character in value.chars() {
        if character.is_control() || is_bidi_control(character) {
            if matches!(character, '\n' | '\r' | '\t')
                && !output.ends_with(' ')
                && !output.is_empty()
            {
                output.push(' ');
            }
            continue;
        }
        if output.len().saturating_add(character.len_utf8()) > max_bytes {
            break;
        }
        output.push(character);
    }
    output.trim().to_owned()
}

const fn is_bidi_control(character: char) -> bool {
    matches!(
        character,
        '\u{061c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
    )
}

pub(super) trait SessionPtyResizer: Sync {
    fn resize_terminal(
        &self,
        rows: u16,
        cols: u16,
        width_pixels: Option<u16>,
        height_pixels: Option<u16>,
    ) -> Result<(), String>;
}

impl SessionPtyResizer for KodosiPty {
    fn resize_terminal(
        &self,
        rows: u16,
        cols: u16,
        width_pixels: Option<u16>,
        height_pixels: Option<u16>,
    ) -> Result<(), String> {
        self.resize(rows, cols, width_pixels, height_pixels)
            .map_err(|error| error.to_string())
    }
}

pub(super) enum ScreenInstructionResult {
    Continue,
    Pty(SessionPtyInstruction),
    Exit(LoopOutcome),
}

pub(super) struct ScreenMutableState<'a> {
    pub(super) pending_pty_writes: &'a mut VecDeque<PendingPtyWrite>,
    pub(super) current_size: &'a mut TerminalSize,
}

#[expect(
    clippy::too_many_lines,
    reason = "one exhaustive screen-instruction match keeps ordered terminal authority auditable"
)]
pub(super) async fn handle_screen_instruction(
    pty: &impl SessionPtyResizer,
    terminal: &SessionTerminalHandle,
    state: ScreenMutableState<'_>,
    session_events: &mpsc::Sender<RuntimeSessionEvent>,
    origin: LocalCoordinatorOrigin,
    instruction: SessionScreenInstruction,
) -> SessionRuntimeResult<ScreenInstructionResult> {
    let ScreenMutableState {
        pending_pty_writes,
        current_size,
    } = state;
    match instruction {
        SessionScreenInstruction::Input(input) => handle_screen_input(terminal, input).await,
        SessionScreenInstruction::ConfirmedInput {
            input,
            reclaim_size,
            completion,
        } => {
            if let Some(size) = reclaim_size {
                match handle_screen_resize(
                    pty,
                    terminal,
                    pending_pty_writes,
                    session_events,
                    origin,
                    current_size,
                    size,
                    None,
                    None,
                )
                .await?
                {
                    SessionScreenOutcome::Continue => {}
                    SessionScreenOutcome::Exit(outcome) => {
                        return Ok(ScreenInstructionResult::Exit(outcome));
                    }
                }
            }
            handle_confirmed_screen_input(terminal, input, completion).await
        }
        SessionScreenInstruction::InputBatch(inputs) => {
            handle_screen_input_batch(terminal, inputs).await
        }
        SessionScreenInstruction::ResizeAndInputBatch { size, inputs } => {
            match handle_screen_resize(
                pty,
                terminal,
                pending_pty_writes,
                session_events,
                origin,
                current_size,
                size,
                None,
                None,
            )
            .await?
            {
                SessionScreenOutcome::Continue => handle_screen_input_batch(terminal, inputs).await,
                SessionScreenOutcome::Exit(outcome) => Ok(ScreenInstructionResult::Exit(outcome)),
            }
        }
        SessionScreenInstruction::Resize {
            size,
            pixel_geometry,
            completion,
        } => match handle_screen_resize(
            pty,
            terminal,
            pending_pty_writes,
            session_events,
            origin,
            current_size,
            size,
            pixel_geometry,
            completion,
        )
        .await?
        {
            SessionScreenOutcome::Continue => Ok(ScreenInstructionResult::Continue),
            SessionScreenOutcome::Exit(outcome) => Ok(ScreenInstructionResult::Exit(outcome)),
        },
        SessionScreenInstruction::Focus { client_id } => {
            let pending_messages = terminal
                .set_client_focus(client_id, ClientFocus::Focused)
                .await
                .map_err(|error| SessionRuntimeError::terminal(TerminalOperation::Focus, error))?;
            enqueue_terminal_messages(pending_pty_writes, pending_messages)?;
            Ok(ScreenInstructionResult::Continue)
        }
        SessionScreenInstruction::Blur { client_id } => {
            let pending_messages = terminal
                .set_client_focus(client_id, ClientFocus::Blurred)
                .await
                .map_err(|error| SessionRuntimeError::terminal(TerminalOperation::Blur, error))?;
            enqueue_terminal_messages(pending_pty_writes, pending_messages)?;
            Ok(ScreenInstructionResult::Continue)
        }
        SessionScreenInstruction::SetClipboardSupport { supported } => {
            terminal
                .set_clipboard_support(supported)
                .await
                .map_err(|error| {
                    SessionRuntimeError::terminal(TerminalOperation::ClipboardSupport, error)
                })?;
            Ok(ScreenInstructionResult::Continue)
        }
        SessionScreenInstruction::ThemeChanged { dark } => {
            terminal.notify_theme_changed(dark).await.map_err(|error| {
                SessionRuntimeError::terminal(TerminalOperation::ThemeChanged, error)
            })?;
            Ok(ScreenInstructionResult::Continue)
        }
        SessionScreenInstruction::CaptureCheckpointData { reply_tx } => {
            let result = terminal
                .checkpoint_data()
                .await
                .map_err(|error| terminal_request_error("capture checkpoint data", error));
            drop(reply_tx.send(result));
            Ok(ScreenInstructionResult::Continue)
        }
        SessionScreenInstruction::CapturePresentationData { reply_tx } => {
            let result = terminal
                .presentation_data()
                .await
                .map_err(|error| terminal_request_error("capture presentation data", error));
            drop(reply_tx.send(result));
            Ok(ScreenInstructionResult::Continue)
        }
    }
}

pub(super) async fn process_pty_output(
    terminal: &SessionTerminalHandle,
    pending_pty_writes: &mut VecDeque<PendingPtyWrite>,
    session_events: &mpsc::Sender<RuntimeSessionEvent>,
    origin: LocalCoordinatorOrigin,
    payload: bytes::Bytes,
    last_working_dir: &mut Option<String>,
    metadata_dirty: &mut bool,
    notification_policy: &mut TerminalNotificationPolicy,
) -> SessionRuntimeResult<ProcessedPtyOutput> {
    let processed = terminal
        .process_output(payload.clone())
        .await
        .map_err(|error| SessionRuntimeError::terminal(TerminalOperation::ProcessOutput, error))?;
    if !dispatch_terminal_effects(
        pending_pty_writes,
        session_events,
        origin,
        processed.effects,
        last_working_dir,
        metadata_dirty,
        notification_policy,
    )
    .await?
    {
        return Ok(ProcessedPtyOutput::Exit(LoopOutcome::Cancelled));
    }
    Ok(ProcessedPtyOutput::Continue {
        applied_sequence: processed.applied_sequence,
        processing_failed: processed.processing_failed,
    })
}

async fn dispatch_terminal_effects(
    pending_pty_writes: &mut VecDeque<PendingPtyWrite>,
    session_events: &mpsc::Sender<RuntimeSessionEvent>,
    origin: LocalCoordinatorOrigin,
    effects: Vec<TerminalEffect>,
    last_working_dir: &mut Option<String>,
    metadata_dirty: &mut bool,
    notification_policy: &mut TerminalNotificationPolicy,
) -> SessionRuntimeResult<bool> {
    let terminal_reply_bytes = effects
        .iter()
        .filter_map(|effect| match effect {
            TerminalEffect::PtyWrite(bytes) => Some(bytes.len()),
            _ => None,
        })
        .sum();
    ensure_pty_queue_capacity(pending_pty_writes, terminal_reply_bytes)
        .map_err(|error| SessionRuntimeError::pty(PtyOperation::QueueInput, error))?;
    for effect in effects {
        match effect {
            TerminalEffect::PtyWrite(bytes) => {
                commit_pending_terminal_messages(pending_pty_writes, vec![bytes]);
            }
            TerminalEffect::ClipboardText(text) => {
                if session_events
                    .send(RuntimeSessionEvent::ClipboardUpdate { origin, text })
                    .await
                    .is_err()
                {
                    return Ok(false);
                }
            }
            TerminalEffect::DesktopNotification { title, body } => {
                let Some((title, body)) = notification_policy.admit(&title, &body) else {
                    tracing::debug!(
                        session_id = %origin.session_id,
                        "dropped terminal notification above per-session rate limit"
                    );
                    continue;
                };
                if title.is_none() && body.is_none() {
                    continue;
                }
                if session_events
                    .send(RuntimeSessionEvent::TerminalNotification {
                        origin,
                        title,
                        body,
                    })
                    .await
                    .is_err()
                {
                    return Ok(false);
                }
            }
            TerminalEffect::Cwd(path) => {
                *metadata_dirty = false;
                if !emit_cwd_update_if_changed(
                    session_events,
                    origin,
                    last_working_dir,
                    path.as_path(),
                )
                .await
                {
                    return Ok(false);
                }
            }
            TerminalEffect::Bell => {
                if session_events
                    .send(RuntimeSessionEvent::TerminalBell { origin })
                    .await
                    .is_err()
                {
                    return Ok(false);
                }
            }
            TerminalEffect::Title(title) => {
                if session_events
                    .send(RuntimeSessionEvent::TerminalTitleChanged {
                        origin,
                        title: sanitize_terminal_title(&title),
                    })
                    .await
                    .is_err()
                {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}

async fn emit_cwd_update_if_changed(
    session_events: &mpsc::Sender<RuntimeSessionEvent>,
    origin: LocalCoordinatorOrigin,
    last_working_dir: &mut Option<String>,
    new_cwd: &Path,
) -> bool {
    let new_cwd_str = new_cwd.to_string_lossy().into_owned();
    if last_working_dir.as_deref() == Some(new_cwd_str.as_str()) {
        return true;
    }

    last_working_dir.replace(new_cwd_str.clone());
    emit_working_dir_changed(session_events, origin, new_cwd_str)
        .await
        .is_ok()
}

async fn handle_screen_input(
    terminal: &SessionTerminalHandle,
    input: SessionInput,
) -> SessionRuntimeResult<ScreenInstructionResult> {
    let bytes = terminal
        .encode_input(session_input_to_terminal_input(&input))
        .await
        .map_err(|error| SessionRuntimeError::terminal(TerminalOperation::EncodeInput, error))?;
    if bytes.is_empty() {
        Ok(ScreenInstructionResult::Continue)
    } else {
        Ok(ScreenInstructionResult::Pty(SessionPtyInstruction::Write(
            bytes,
        )))
    }
}

async fn handle_confirmed_screen_input(
    terminal: &SessionTerminalHandle,
    input: SessionInput,
    completion: tokio::sync::oneshot::Sender<crate::Result<()>>,
) -> SessionRuntimeResult<ScreenInstructionResult> {
    let bytes = match terminal
        .encode_input(session_input_to_terminal_input(&input))
        .await
    {
        Ok(bytes) => bytes,
        Err(error) => {
            let error = SessionRuntimeError::terminal(TerminalOperation::EncodeInput, error);
            drop(completion.send(Err(AppError::Unsupported {
                reason: error.to_string(),
            })));
            return Err(error);
        }
    };
    if bytes.is_empty() {
        drop(completion.send(Ok(())));
        return Ok(ScreenInstructionResult::Continue);
    }
    Ok(ScreenInstructionResult::Pty(
        SessionPtyInstruction::ConfirmedWrite { bytes, completion },
    ))
}

async fn handle_screen_input_batch(
    terminal: &SessionTerminalHandle,
    inputs: Vec<SessionInput>,
) -> SessionRuntimeResult<ScreenInstructionResult> {
    let mut encoded = Vec::new();
    for input in inputs {
        let bytes = terminal
            .encode_input(session_input_to_terminal_input(&input))
            .await
            .map_err(|error| {
                SessionRuntimeError::terminal(TerminalOperation::EncodeBatchedInput, error)
            })?;
        encoded.extend(bytes);
    }

    if encoded.is_empty() {
        Ok(ScreenInstructionResult::Continue)
    } else {
        Ok(ScreenInstructionResult::Pty(SessionPtyInstruction::Write(
            encoded,
        )))
    }
}

fn send_resize_completion(
    completion: &mut Option<tokio::sync::oneshot::Sender<crate::Result<()>>>,
    result: crate::Result<()>,
) {
    if let Some(completion) = completion.take() {
        drop(completion.send(result));
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the screen actor owns PTY, terminal, event, and current-size state for one ordered resize"
)]
pub(super) async fn handle_screen_resize(
    pty: &impl SessionPtyResizer,
    terminal: &SessionTerminalHandle,
    pending_pty_writes: &mut VecDeque<PendingPtyWrite>,
    session_events: &mpsc::Sender<RuntimeSessionEvent>,
    origin: LocalCoordinatorOrigin,
    current_size: &mut TerminalSize,
    size: TerminalSize,
    pixel_geometry: Option<TerminalPixelGeometry>,
    mut completion: Option<tokio::sync::oneshot::Sender<crate::Result<()>>>,
) -> SessionRuntimeResult<SessionScreenOutcome> {
    if *current_size == size && pixel_geometry.is_none() {
        send_resize_completion(&mut completion, Ok(()));
        return Ok(SessionScreenOutcome::Continue);
    }
    let pty_geometry = pixel_geometry.map(|geometry| {
        (
            u16::try_from(geometry.width_pixels()).unwrap_or(u16::MAX),
            u16::try_from(geometry.height_pixels()).unwrap_or(u16::MAX),
        )
    });
    if (*current_size != size || pty_geometry.is_some())
        && let Err(error) = pty.resize_terminal(
            size.rows(),
            size.cols(),
            pty_geometry.map(|geometry| geometry.0),
            pty_geometry.map(|geometry| geometry.1),
        )
    {
        let error = SessionRuntimeError::pty(PtyOperation::Resize, error);
        send_resize_completion(
            &mut completion,
            Err(AppError::Unsupported {
                reason: error.to_string(),
            }),
        );
        return Err(error);
    }
    let pending_messages = match terminal
        .resize(size.rows(), size.cols(), pixel_geometry)
        .await
    {
        Ok(pending_messages) => pending_messages,
        Err(error) => {
            let error = SessionRuntimeError::terminal(TerminalOperation::Resize, error);
            send_resize_completion(
                &mut completion,
                Err(AppError::Unsupported {
                    reason: error.to_string(),
                }),
            );
            return Err(error);
        }
    };
    *current_size = size;
    send_resize_completion(&mut completion, Ok(()));
    enqueue_terminal_messages(pending_pty_writes, pending_messages)?;
    if emit_capture_metadata(session_events, origin, size, None, false)
        .await
        .is_err()
    {
        return Ok(SessionScreenOutcome::Exit(LoopOutcome::Cancelled));
    }
    Ok(SessionScreenOutcome::Continue)
}

fn session_input_to_terminal_input(input: &SessionInput) -> TerminalInput {
    TerminalInput::Raw(input.as_bytes().to_vec())
}

fn terminal_request_error(context: &str, error: impl fmt::Display) -> AppError {
    AppError::Unsupported {
        reason: format!("{context}: {error}"),
    }
}

fn enqueue_terminal_messages(
    pending_pty_writes: &mut VecDeque<PendingPtyWrite>,
    pending_messages: Vec<Vec<u8>>,
) -> SessionRuntimeResult<()> {
    enqueue_pending_terminal_messages(pending_pty_writes, pending_messages)
        .map_err(|error| SessionRuntimeError::pty(PtyOperation::QueueInput, error))
}

#[cfg(test)]
mod tests;
