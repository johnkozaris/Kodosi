use std::path::PathBuf;

use super::{SessionTerminal, TerminalEffect, TerminalLook, parse_cwd};

fn reported(raw: &str) -> Option<PathBuf> {
    parse_cwd(raw.as_bytes())
}

#[cfg(windows)]
#[test]
fn a_windows_host_takes_its_folders_from_osc_7_and_quoted_or_bare_osc_9_9() {
    for (raw, folder) in [
        (r"C:\Users\ada\project", r"C:\Users\ada\project"),
        (
            r#""C:\Users\ada\my project ✓""#,
            r"C:\Users\ada\my project ✓",
        ),
        (r#""\\server\share\project""#, r"\\server\share\project"),
        (
            "file://pc/C:/Users/ada/my%20project",
            r"C:\Users\ada\my project",
        ),
        ("file:///d:/", r"d:\"),
        ("file://server/share/x", r"\\server\share\x"),
    ] {
        assert_eq!(reported(raw), Some(PathBuf::from(folder)), "{raw}");
    }
}

#[cfg(unix)]
#[test]
fn a_unix_host_takes_its_folders_from_osc_7_and_quoted_or_bare_osc_9_9() {
    for (raw, folder) in [
        ("/home/ada/project", "/home/ada/project"),
        (r#""/home/ada/my project ✓""#, "/home/ada/my project ✓"),
        (
            "file://localhost/home/ada/my%20project",
            "/home/ada/my project",
        ),
        ("file:///home/ada", "/home/ada"),
    ] {
        assert_eq!(reported(raw), Some(PathBuf::from(folder)), "{raw}");
    }
    assert_eq!(reported("file://another-computer.invalid/home/ada"), None);
}

#[test]
fn a_report_that_is_no_folder_of_this_host_leaves_the_folder_alone() {
    let foreign: &[&str] = if cfg!(windows) {
        &["/home/ada", "file://pc/C%3A%20Users%20ada"]
    } else {
        &[r"C:\Users\ada"]
    };
    for raw in [
        "",
        "\"\"",
        "project",
        r#""C:\Users\ada"#,
        "file://pc/home/ada\u{7}",
        "file://pc/home/%ZZ",
        "file://pc",
    ]
    .iter()
    .chain(foreign)
    {
        assert_eq!(reported(raw), None, "{raw:?}");
    }
    assert_eq!(reported(&format!("/{}", "a".repeat(4096))), None);
}

#[test]
fn a_history_too_large_for_one_snapshot_still_gives_the_screen() {
    let mut terminal = SessionTerminal::new(100, 1024, TerminalLook::Dark).expect("terminal");
    let line = format!("{}\r\n", "\u{754c}".repeat(512));
    for _ in 0..1200 {
        terminal.process_output(line.as_bytes()).expect("output");
    }
    assert_eq!(
        terminal
            .terminal
            .snapshot(crate::terminal::snapshot_limits(u64::MAX)),
        Err(ghostty_vt::Error::LimitExceeded)
    );

    let cut = terminal
        .checkpoint_data(true, u64::MAX)
        .expect("checkpoint");

    assert_eq!(
        cut.checkpoint.snapshot(),
        terminal
            .terminal
            .visible_snapshot(crate::terminal::snapshot_limits(u64::MAX))
            .expect("visible")
    );
}

fn replies(effects: &[TerminalEffect]) -> Vec<&[u8]> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            TerminalEffect::PtyWrite(reply) => Some(reply.as_slice()),
            _ => None,
        })
        .collect()
}

const ASK_COLORS: &[u8] = b"\x1b]11;?\x1b\\\x1b]10;?\x1b\\\x1b]4;1;?\x1b\\\x1b[?996n";

#[test]
fn a_new_look_changes_the_answers_and_reaches_only_a_program_that_follows_the_color_scheme() {
    let mut terminal = SessionTerminal::new(24, 80, TerminalLook::Dark).expect("terminal");

    assert_eq!(
        terminal.change_look(TerminalLook::Light).expect("light"),
        b""
    );
    let output = terminal.process_output(ASK_COLORS).expect("output");
    assert_eq!(
        replies(&output.effects).first().copied(),
        Some(b"\x1b]11;rgb:fafa/f6f6/eeee\x1b\\".as_slice())
    );
    assert_eq!(
        replies(&output.effects).last().copied(),
        Some(b"\x1b[?997;2n".as_slice())
    );

    terminal.process_output(b"\x1b[?2031h").expect("follow");
    assert_eq!(
        terminal.change_look(TerminalLook::Dark).expect("dark"),
        b"\x1b[?997;1n"
    );
    terminal
        .process_output(b"\x1b[?2031l")
        .expect("stop following");
    assert_eq!(
        terminal
            .change_look(TerminalLook::Light)
            .expect("light again"),
        b""
    );
}
