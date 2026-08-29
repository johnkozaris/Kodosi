use std::fmt;

use kodosi_domain::{lifecycle::StopReason, terminal::TerminalSize};
use tokio::sync::mpsc;

use crate::session_runtime::events::{LocalCoordinatorOrigin, RuntimeSessionEvent};

pub(super) async fn emit_capture_metadata(
    session_events: &mpsc::Sender<RuntimeSessionEvent>,
    origin: LocalCoordinatorOrigin,
    size: TerminalSize,
    working_dir: Option<String>,
    refresh_working_dir: bool,
) -> Result<(), ()> {
    session_events
        .send(RuntimeSessionEvent::CaptureMetadata {
            origin,
            size,
            working_dir,
            refresh_working_dir,
        })
        .await
        .map_err(|_| ())
}

pub(super) async fn emit_working_dir_changed(
    session_events: &mpsc::Sender<RuntimeSessionEvent>,
    origin: LocalCoordinatorOrigin,
    working_dir: String,
) -> Result<(), ()> {
    session_events
        .send(RuntimeSessionEvent::WorkingDirChanged {
            origin,
            working_dir,
        })
        .await
        .map_err(|_| ())
}

pub(super) async fn emit_stopped(
    session_events: &mpsc::Sender<RuntimeSessionEvent>,
    origin: LocalCoordinatorOrigin,
    reason: StopReason,
) -> Result<(), ()> {
    session_events
        .send(RuntimeSessionEvent::Stopped { origin, reason })
        .await
        .map_err(|_| ())
}

pub(super) async fn emit_failure_and_stop(
    session_events: &mpsc::Sender<RuntimeSessionEvent>,
    origin: LocalCoordinatorOrigin,
    message: impl fmt::Display,
) -> Result<(), ()> {
    session_events
        .send(RuntimeSessionEvent::Failed {
            origin,
            message: message.to_string(),
        })
        .await
        .map_err(|_| ())?;
    emit_stopped(session_events, origin, StopReason::Failed).await
}
