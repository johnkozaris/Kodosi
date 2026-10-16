use super::{
    Checkpoint, TERMINAL_SNAPSHOT_MAX_BYTES, TerminalCheckpointError, TerminalMetadata,
    TerminalSize,
};
use ghostty_vt::{Terminal, TerminalPolicy};

fn in_folder(directory: &str) -> TerminalMetadata {
    TerminalMetadata {
        directory: Some(directory.to_owned()),
        ..TerminalMetadata::default()
    }
}

#[test]
fn folders_from_every_host_platform_are_valid() {
    for folder in [
        "/home/ada/project",
        r"C:\Users\ada\project",
        "C:/Users/ada",
        r"\\server\share\project",
    ] {
        assert!(in_folder(folder).is_valid(), "{folder}");
    }
}

#[test]
fn relative_or_malformed_folders_are_invalid() {
    for folder in [
        "",
        "project",
        "C:project",
        "C:",
        r"\\",
        r"1:\x",
        "/tmp/\u{7}",
    ] {
        assert!(!in_folder(folder).is_valid(), "{folder:?}");
    }
}

#[test]
fn a_checkpoint_takes_its_size_from_the_snapshot() {
    let mut host = Terminal::new(30, 5, TerminalPolicy::default()).expect("terminal");
    let snapshot = host
        .snapshot(TERMINAL_SNAPSHOT_MAX_BYTES)
        .expect("snapshot");
    let checkpoint = Checkpoint::new(snapshot, true).expect("checkpoint");

    assert_eq!(checkpoint.size(), TerminalSize::new(5, 30).expect("size"));
    assert!(checkpoint.console);
}

#[test]
fn a_snapshot_over_the_size_limits_or_unreadable_is_refused_before_it_decodes() {
    let mut wide = Terminal::new(1025, 4, TerminalPolicy::default()).expect("terminal");
    let snapshot = wide
        .snapshot(TERMINAL_SNAPSHOT_MAX_BYTES)
        .expect("snapshot");

    assert!(matches!(
        Checkpoint::new(snapshot, false),
        Err(TerminalCheckpointError::InvalidSize(_))
    ));
    for unreadable in [&b""[..], b"GHOSTSNP\x01\x00", b"not a snapshot"] {
        assert_eq!(
            Checkpoint::new(unreadable.to_vec(), false),
            Err(TerminalCheckpointError::UnreadableSnapshot)
        );
    }
    assert_eq!(
        Checkpoint::new(vec![0; TERMINAL_SNAPSHOT_MAX_BYTES + 1], false),
        Err(TerminalCheckpointError::SnapshotTooLarge {
            actual: TERMINAL_SNAPSHOT_MAX_BYTES + 1,
            maximum: TERMINAL_SNAPSHOT_MAX_BYTES,
        })
    );
}
