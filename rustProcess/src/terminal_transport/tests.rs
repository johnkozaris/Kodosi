use super::*;
use bytes::Bytes;
use kodosi_domain::terminal::{TerminalCheckpointV2, TerminalScreen, TerminalSize};

fn checkpoint() -> TerminalCheckpointV2 {
    TerminalCheckpointV2::new(
        TerminalSize::new(24, 80).expect("size"),
        TerminalScreen::Primary,
        b"checkpoint".to_vec(),
        9,
        4,
        true,
    )
    .expect("checkpoint")
}

#[test]
fn connection_ids_are_distinct() {
    assert_ne!(TerminalConnectionId::new(), TerminalConnectionId::new());
}

#[test]
fn connection_id_display_is_hyphenated_uuid() {
    let value = TerminalConnectionId::new().to_string();
    assert_eq!(value.len(), 36);
    assert!(value.contains('-'));
}

#[test]
fn capability_can_write() {
    assert!(TerminalCapability::Write.can_write());
    assert!(!TerminalCapability::ReadOnly.can_write());
}

#[test]
fn data_frame_carries_raw_bytes_and_sequence() {
    let raw = Bytes::from_static(b"\x1b[2J\x1b[H");
    let frame = TerminalDataFrame::new(42, raw.clone());
    assert_eq!(frame.sequence, 42);
    assert_eq!(frame.bytes, raw);
}

#[test]
fn local_checkpoint_control_carries_next_sequence() {
    let frame = TerminalControlFrame::SemanticCheckpoint {
        checkpoint: checkpoint(),
        next_sequence: 7,
    };
    std::assert_matches!(
        frame,
        TerminalControlFrame::SemanticCheckpoint {
            next_sequence: 7,
            ..
        }
    );
}

#[test]
fn control_resize_carries_dimensions_and_sequence() {
    let frame = TerminalControlFrame::Resize {
        rows: 40,
        cols: 120,
        at_sequence: 100,
    };
    std::assert_matches!(
        frame,
        TerminalControlFrame::Resize {
            rows: 40,
            cols: 120,
            at_sequence: 100
        }
    );
}

#[test]
fn control_closed_carries_reason_and_exclusive_boundary() {
    let frame = TerminalControlFrame::Closed {
        reason: TerminalCloseReason::Detached,
        final_sequence: 9,
    };
    std::assert_matches!(
        frame,
        TerminalControlFrame::Closed {
            reason: TerminalCloseReason::Detached,
            final_sequence: 9,
        }
    );
}

#[test]
fn close_reason_display() {
    assert_eq!(
        TerminalCloseReason::SessionEnded.to_string(),
        "session ended"
    );
    assert_eq!(TerminalCloseReason::Detached.to_string(), "detached");
    assert_eq!(
        TerminalCloseReason::RelayDisconnected.to_string(),
        "relay disconnected"
    );
    assert_eq!(TerminalCloseReason::AuthRevoked.to_string(), "auth revoked");
    assert_eq!(
        TerminalCloseReason::IoError("broken pipe".to_owned()).to_string(),
        "I/O error: broken pipe"
    );
}

#[test]
fn terminal_surface_display() {
    assert_eq!(TerminalSurface::Desktop.to_string(), "desktop");
    assert_eq!(TerminalSurface::Cli.to_string(), "cli");
    assert_eq!(
        TerminalSurface::HeadlessCapture.to_string(),
        "headless-capture"
    );
    assert_eq!(TerminalSurface::RelayHost.to_string(), "relay-host");
}
