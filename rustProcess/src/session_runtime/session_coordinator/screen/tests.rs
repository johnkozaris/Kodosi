use super::{
    SessionPtyResizer, TERMINAL_NOTIFICATION_FIELD_MAX_BYTES, TERMINAL_TITLE_MAX_BYTES,
    TerminalNotificationPolicy, dispatch_terminal_effects, enqueue_terminal_messages,
    handle_screen_resize, sanitize_terminal_title,
};
use crate::session_runtime::events::{LocalCoordinatorOrigin, RuntimeSessionEvent};
use kodosi_domain::ids::SessionId;
use kodosi_domain::terminal::{TerminalPixelGeometry, TerminalSize};
use kodosi_session::{SessionTerminalHandle, TerminalEffect, TerminalHistoryPolicy};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc;

type AppliedResize = (u16, u16, Option<u16>, Option<u16>);

fn origin(session_id: SessionId) -> LocalCoordinatorOrigin {
    LocalCoordinatorOrigin {
        session_id,
        local_incarnation_id: uuid::Uuid::now_v7(),
    }
}

#[derive(Default)]
struct RecordingPtyResizer {
    applied: Arc<Mutex<Vec<AppliedResize>>>,
}

impl SessionPtyResizer for RecordingPtyResizer {
    fn resize_terminal(
        &self,
        rows: u16,
        cols: u16,
        width_pixels: Option<u16>,
        height_pixels: Option<u16>,
    ) -> Result<(), String> {
        self.applied
            .lock()
            .map_err(|_| "resize recorder lock poisoned".to_owned())?
            .push((rows, cols, width_pixels, height_pixels));
        Ok(())
    }
}

struct RejectingPtyResizer;

impl SessionPtyResizer for RejectingPtyResizer {
    fn resize_terminal(
        &self,
        _rows: u16,
        _cols: u16,
        _width_pixels: Option<u16>,
        _height_pixels: Option<u16>,
    ) -> Result<(), String> {
        Err("injected PTY resize failure".to_owned())
    }
}

#[tokio::test]
async fn successful_resize_updates_live_terminal_dimensions() {
    let id = SessionId::new();
    let terminal = SessionTerminalHandle::spawn_with_theme(
        24,
        80,
        0,
        false,
        TerminalHistoryPolicy::default(),
        true,
    )
    .expect("terminal");
    let resizer = RecordingPtyResizer::default();
    let mut pending_pty_writes = VecDeque::new();
    let (events_tx, _events_rx) = mpsc::channel(2);
    let mut current = TerminalSize::new(24, 80).expect("size");
    let requested = TerminalSize::new(30, 100).expect("requested");

    handle_screen_resize(
        &resizer,
        &terminal,
        &mut pending_pty_writes,
        &events_tx,
        origin(id),
        &mut current,
        requested,
        None,
        None,
    )
    .await
    .expect("resize");
    assert_eq!(
        *resizer.applied.lock().expect("resize recorder"),
        vec![(30, 100, None, None)]
    );
    let snapshot = terminal
        .checkpoint_data()
        .await
        .expect("live terminal checkpoint")
        .checkpoint;
    assert_eq!((snapshot.rows(), snapshot.cols()), (30, 100));
}

#[tokio::test]
async fn resize_completion_precedes_post_commit_metadata_backpressure() {
    let terminal = SessionTerminalHandle::spawn_with_theme(
        24,
        80,
        0,
        false,
        TerminalHistoryPolicy::default(),
        true,
    )
    .expect("terminal");
    let resizer = RecordingPtyResizer::default();
    let (events_tx, mut events_rx) = mpsc::channel(1);
    events_tx
        .send(RuntimeSessionEvent::TerminalBell {
            origin: origin(SessionId::new()),
        })
        .await
        .expect("seed event lane");
    let id = SessionId::new();
    let requested = TerminalSize::new(30, 100).expect("requested");
    let (completion, applied) = tokio::sync::oneshot::channel();
    let resize = tokio::spawn(async move {
        let mut current = TerminalSize::new(24, 80).expect("size");
        let mut pending_pty_writes = VecDeque::new();
        let outcome = handle_screen_resize(
            &resizer,
            &terminal,
            &mut pending_pty_writes,
            &events_tx,
            origin(id),
            &mut current,
            requested,
            None,
            Some(completion),
        )
        .await;
        (outcome, current, terminal)
    });

    applied
        .await
        .expect("completion sender")
        .expect("resize applied");
    assert!(
        !resize.is_finished(),
        "metadata publication should still be blocked"
    );
    drop(events_rx.recv().await);
    let (outcome, current, terminal) = resize.await.expect("resize task");

    assert!(outcome.is_ok());
    assert_eq!(current, requested);
    terminal.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn geometry_only_resize_updates_pty_pixel_dimensions() {
    let terminal = SessionTerminalHandle::spawn_with_theme(
        24,
        80,
        0,
        false,
        TerminalHistoryPolicy::default(),
        true,
    )
    .expect("terminal");
    let resizer = RecordingPtyResizer::default();
    let mut pending_pty_writes = VecDeque::new();
    let (events_tx, _events_rx) = mpsc::channel(2);
    let id = SessionId::new();
    let mut current = TerminalSize::new(24, 80).expect("size");
    let geometry = TerminalPixelGeometry::new(800, 480, 10, 20).expect("geometry");

    let requested = current;
    handle_screen_resize(
        &resizer,
        &terminal,
        &mut pending_pty_writes,
        &events_tx,
        origin(id),
        &mut current,
        requested,
        Some(geometry),
        None,
    )
    .await
    .expect("geometry resize");

    assert_eq!(
        *resizer.applied.lock().expect("resize recorder"),
        vec![(24, 80, Some(800), Some(480))]
    );
    terminal.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn failed_pty_resize_does_not_advance_terminal_size() {
    let terminal = SessionTerminalHandle::spawn_with_theme(
        24,
        80,
        0,
        false,
        TerminalHistoryPolicy::default(),
        true,
    )
    .expect("terminal");
    let mut pending_pty_writes = VecDeque::new();
    let (events_tx, _events_rx) = mpsc::channel(2);
    let id = SessionId::new();
    let mut current = TerminalSize::new(24, 80).expect("size");
    let requested = TerminalSize::new(30, 100).expect("requested");

    let result = handle_screen_resize(
        &RejectingPtyResizer,
        &terminal,
        &mut pending_pty_writes,
        &events_tx,
        origin(id),
        &mut current,
        requested,
        None,
        None,
    )
    .await;
    let Err(error) = result else {
        panic!("closed PTY lane must fail resize transaction");
    };

    assert!(error.to_string().contains("injected PTY resize failure"));
    assert_eq!(current, TerminalSize::new(24, 80).expect("original"));
    terminal.shutdown().await.expect("shutdown");
}

#[test]
fn terminal_notification_policy_sanitizes_bounds_and_rate_limits() {
    let mut policy = TerminalNotificationPolicy::default();
    let dangerous = format!("title\nspoof\u{202e}{}", "é".repeat(400));
    let (title, body) = policy
        .admit(&dangerous, "body\tline")
        .expect("first notification admitted");
    let title = title.expect("sanitized title");
    assert!(!title.contains('\n'));
    assert!(!title.contains('\u{202e}'));
    assert!(title.len() <= TERMINAL_NOTIFICATION_FIELD_MAX_BYTES);
    assert_eq!(body.as_deref(), Some("body line"));
    assert!(policy.admit("two", "").is_some());
    assert!(policy.admit("three", "").is_some());
    assert!(policy.admit("four", "").is_none());
}

#[test]
fn terminal_title_is_bounded_sanitized_and_can_clear() {
    let dangerous = format!(" title\nspoof\u{202e}{} ", "é".repeat(400));
    let title = sanitize_terminal_title(&dangerous).expect("non-empty title");

    assert!(!title.contains('\n'));
    assert!(!title.contains('\u{202e}'));
    assert!(title.len() <= TERMINAL_TITLE_MAX_BYTES);
    assert_eq!(sanitize_terminal_title("\n\u{202e}"), None);
}

#[tokio::test]
async fn terminal_bell_and_title_effects_keep_stream_order() {
    let mut pending_pty_writes = VecDeque::new();
    let (events_tx, mut events_rx) = mpsc::channel(4);
    let session_id = SessionId::new();
    let origin = origin(session_id);
    let mut cwd = None;
    let mut metadata_dirty = false;
    let mut notification_policy = TerminalNotificationPolicy::default();

    assert!(
        dispatch_terminal_effects(
            &mut pending_pty_writes,
            &events_tx,
            origin,
            vec![
                TerminalEffect::Title("first".to_owned()),
                TerminalEffect::Bell,
                TerminalEffect::Title("second".to_owned()),
                TerminalEffect::Bell,
            ],
            &mut cwd,
            &mut metadata_dirty,
            &mut notification_policy,
        )
        .await
        .expect("dispatch")
    );

    std::assert_matches!(
        events_rx.recv().await,
        Some(RuntimeSessionEvent::TerminalTitleChanged { origin, title })
            if origin.session_id == session_id && title.as_deref() == Some("first")
    );
    std::assert_matches!(
        events_rx.recv().await,
        Some(RuntimeSessionEvent::TerminalBell { origin }) if origin.session_id == session_id
    );
    std::assert_matches!(
        events_rx.recv().await,
        Some(RuntimeSessionEvent::TerminalTitleChanged { origin, title })
            if origin.session_id == session_id && title.as_deref() == Some("second")
    );
    std::assert_matches!(
        events_rx.recv().await,
        Some(RuntimeSessionEvent::TerminalBell { origin }) if origin.session_id == session_id
    );
}

#[test]
fn terminal_replies_bypass_the_external_pty_channel() {
    let mut pending_pty_writes = VecDeque::new();
    let replies = (0_u8..129).map(|index| vec![index]).collect();

    enqueue_terminal_messages(&mut pending_pty_writes, replies).expect("enqueue replies");

    assert_eq!(pending_pty_writes.len(), 129);
    assert_eq!(
        pending_pty_writes
            .front()
            .map(|write| write.bytes.as_slice()),
        Some(&[0][..])
    );
    assert_eq!(
        pending_pty_writes
            .back()
            .map(|write| write.bytes.as_slice()),
        Some(&[128][..])
    );
}

#[test]
fn oversized_terminal_reply_batch_is_rejected_atomically() {
    let mut pending_pty_writes = VecDeque::new();
    enqueue_terminal_messages(&mut pending_pty_writes, vec![b"prior".to_vec()])
        .expect("seed queue");
    let before = pending_pty_writes.len();

    let result =
        enqueue_terminal_messages(&mut pending_pty_writes, vec![vec![0; 10 * 1024 * 1024]]);

    assert!(result.is_err());
    assert_eq!(pending_pty_writes.len(), before);
    assert_eq!(
        pending_pty_writes
            .front()
            .map(|write| write.bytes.as_slice()),
        Some(&b"prior"[..])
    );
}
