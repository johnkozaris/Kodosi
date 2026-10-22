#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        reason = "tests use panic-on-error assertions for failure clarity"
    )
)]

pub use ghostty_vt_sys::raw::{
    CompressionProgress, ContextAction, ContextSignal, Effect, Error, ProgramStatus,
    ProgramStatusKind, ProgramStatusState, ProgressState, Screen, SnapshotHeader, SnapshotLimits,
    TerminalState, snapshot_without_images,
};

use ghostty_vt_sys::raw::{Format, FormatOptions, Limits, Terminal as RawTerminal};

const FORMATTER_PROFILE_MAX_BYTES: usize = 8 * 1024 * 1024;

pub const IMAGE_STORAGE_MAX_BYTES: u64 = 64 * 1024 * 1024;

const CLI_REPLAY_PREFIX: &[u8] = b"\x1b]8;;\x1b\\\x1b[0m\x1b[2J\x1b[H";
const CLI_REPLAY_SUFFIX: &[u8] = b"\x1b]8;;\x1b\\\x1b[0m";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalPolicy {
    pub continuation_max_bytes: usize,
    pub scrollback_max_bytes: usize,
    pub scrollback_max_lines: usize,
    /// Terminals that replay the same output need the same value, or they store different images.
    pub image_storage_max_bytes: u64,
    pub console: bool,
}

impl Default for TerminalPolicy {
    fn default() -> Self {
        Self {
            continuation_max_bytes: 1024 * 1024,
            scrollback_max_bytes: 64 * 1024 * 1024,
            scrollback_max_lines: 1_024,
            image_storage_max_bytes: IMAGE_STORAGE_MAX_BYTES,
            console: false,
        }
    }
}

impl TerminalPolicy {
    const fn limits(self) -> Limits {
        Limits {
            continuation_max_bytes: self.continuation_max_bytes,
            scrollback_max_bytes: self.scrollback_max_bytes,
            scrollback_max_lines: self.scrollback_max_lines,
            image_storage_max_bytes: self.image_storage_max_bytes,
        }
    }
}

#[derive(Debug)]
pub struct WriteOutcome {
    pub effects: Vec<Effect>,
    pub processing_failed: bool,
}

pub struct Terminal {
    raw: RawTerminal,
    processing_failed: bool,
}

impl Terminal {
    pub fn new(cols: u16, rows: u16, policy: TerminalPolicy) -> Result<Self, Error> {
        let mut raw = RawTerminal::new(cols, rows, policy.limits())?;
        if policy.console {
            raw.use_console_profile()?;
        }
        Ok(Self {
            raw,
            processing_failed: false,
        })
    }

    /// Restores a terminal from a Ghostty snapshot. The snapshot holds the console
    /// modes, so the policy adds only the console rule for resizes.
    pub fn from_snapshot(snapshot: &[u8], policy: TerminalPolicy) -> Result<Self, Error> {
        let mut raw = RawTerminal::from_snapshot(snapshot, policy.limits())?;
        if policy.console {
            raw.keep_scrollback_on_resize()?;
        }
        Ok(Self {
            raw,
            processing_failed: false,
        })
    }

    pub fn report_version(&mut self, version: &'static str) -> Result<(), Error> {
        self.raw.report_version(version)
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<Vec<Effect>, Error> {
        let outcome = self.write_outcome(bytes)?;
        if outcome.processing_failed {
            return Err(Error::ProcessingFailed);
        }
        Ok(outcome.effects)
    }

    pub fn write_outcome(&mut self, bytes: &[u8]) -> Result<WriteOutcome, Error> {
        if self.processing_failed {
            return Err(Error::ProcessingFailed);
        }
        let effects = self.raw.write(bytes)?;
        self.processing_failed = self.raw.state()?.vt_processing_error;
        Ok(WriteOutcome {
            effects,
            processing_failed: self.processing_failed,
        })
    }

    pub fn resize(
        &mut self,
        cols: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> Result<Vec<Effect>, Error> {
        self.raw.resize(cols, rows, cell_width_px, cell_height_px)
    }

    pub fn encode_focus(&mut self, focused: bool) -> Result<Vec<u8>, Error> {
        self.raw.encode_focus(focused)
    }

    pub fn compression_activity(&mut self) -> Result<u64, Error> {
        self.raw.compression_activity()
    }

    pub fn compress_incremental(&mut self) -> Result<CompressionProgress, Error> {
        self.raw.compress_incremental()
    }

    pub fn state(&mut self) -> Result<TerminalState, Error> {
        self.raw.state()
    }

    pub fn snapshot(&mut self, limits: SnapshotLimits) -> Result<Vec<u8>, Error> {
        self.raw.encode_snapshot(true, limits)
    }

    /// Ends before the history, after READY and the pixels of the images on screen.
    /// The first page of a screen can also hold some history rows.
    pub fn visible_snapshot(&mut self, limits: SnapshotLimits) -> Result<Vec<u8>, Error> {
        self.raw.encode_snapshot(false, limits)
    }

    pub fn text(&mut self, history: bool) -> Result<String, Error> {
        let text = self.raw.format(
            FormatOptions {
                format: Format::Plain,
                unwrap: true,
                trim: true,
                palette: false,
                modes: false,
                scrolling_region: false,
                tabstops: false,
                pwd: false,
                keyboard: false,
                cursor: false,
                style: false,
                hyperlink: false,
                protection: false,
                kitty_keyboard: false,
                charsets: false,
                screen_only: !history,
            },
            FORMATTER_PROFILE_MAX_BYTES,
        )?;
        Ok(String::from_utf8_lossy(&text).into_owned())
    }

    pub fn format_finite_cli_replay(&mut self) -> Result<Vec<u8>, Error> {
        let screen_vt = self.raw.format(
            FormatOptions {
                format: Format::Vt,
                unwrap: false,
                trim: false,
                palette: false,
                modes: false,
                scrolling_region: false,
                tabstops: false,
                pwd: false,
                keyboard: false,
                cursor: true,
                style: false,
                hyperlink: false,
                protection: false,
                kitty_keyboard: false,
                charsets: false,
                screen_only: true,
            },
            FORMATTER_PROFILE_MAX_BYTES,
        )?;
        let mut output =
            Vec::with_capacity(CLI_REPLAY_PREFIX.len() + screen_vt.len() + CLI_REPLAY_SUFFIX.len());
        output.extend_from_slice(CLI_REPLAY_PREFIX);
        output.extend_from_slice(&screen_vt);
        output.extend_from_slice(CLI_REPLAY_SUFFIX);
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAXIMUM: usize = 8 * 1024 * 1024;
    const LIMITS: SnapshotLimits = SnapshotLimits {
        text_bytes: MAXIMUM,
        total_bytes: MAXIMUM,
        image_data_bytes: u64::MAX,
    };

    /// A restored terminal lays out its pages anew, so tests compare what the
    /// terminals show, not snapshot bytes.
    fn appearance(terminal: &mut Terminal) -> (Vec<u8>, TerminalState) {
        let shown = terminal
            .raw
            .format(
                FormatOptions {
                    format: Format::Vt,
                    unwrap: false,
                    trim: false,
                    palette: true,
                    modes: true,
                    scrolling_region: true,
                    tabstops: true,
                    pwd: true,
                    keyboard: true,
                    cursor: true,
                    style: true,
                    hyperlink: true,
                    protection: true,
                    kitty_keyboard: true,
                    charsets: true,
                    screen_only: false,
                },
                MAXIMUM,
            )
            .expect("format");
        (shown, terminal.state().expect("state"))
    }

    #[test]
    fn console_terminals_keep_grapheme_widths_and_replicas_resize_like_them() {
        let console = TerminalPolicy {
            console: true,
            ..TerminalPolicy::default()
        };
        let mut host = Terminal::new(10, 4, console).expect("host");
        host.write(b"\x1bc").expect("reset");
        assert!(host.raw.mode(2027, false).expect("grapheme mode"));
        for line in 0..12 {
            host.write(format!("line {line}\r\n").as_bytes())
                .expect("output");
        }
        let snapshot = host.snapshot(LIMITS).expect("snapshot");
        let mut replica = Terminal::from_snapshot(&snapshot, console).expect("replica");
        let mut pulling =
            Terminal::from_snapshot(&snapshot, TerminalPolicy::default()).expect("pulling replica");
        let mut reset =
            Terminal::from_snapshot(&snapshot, TerminalPolicy::default()).expect("reset replica");
        reset.write(b"\x1bc").expect("replica reset");
        assert!(reset.raw.mode(2027, false).expect("grapheme default"));

        host.resize(10, 8, 0, 0).expect("host resize");
        replica.resize(10, 8, 0, 0).expect("replica resize");
        pulling.resize(10, 8, 0, 0).expect("pulling resize");
        let host = appearance(&mut host);
        assert_eq!(appearance(&mut replica), host);
        assert_ne!(appearance(&mut pulling), host);
    }

    fn assert_snapshot_suffix_parity(cols: u16, rows: u16, prefix: &[u8], suffix: &[u8]) {
        let mut source = Terminal::new(cols, rows, TerminalPolicy::default()).expect("source");
        source.write(prefix).expect("source prefix");
        let snapshot = source.snapshot(LIMITS).expect("snapshot");

        let mut restored =
            Terminal::from_snapshot(&snapshot, TerminalPolicy::default()).expect("restore");
        source.write(suffix).expect("source suffix");
        restored.write(suffix).expect("restored suffix");

        assert_eq!(appearance(&mut restored), appearance(&mut source));
    }

    /// Twenty columns put the whole history in the first page of the screen.
    fn visible_parity(host_bytes: &[u8], suffix: &[u8]) {
        let policy = TerminalPolicy::default();
        let mut host = Terminal::new(20, 4, policy).expect("host");
        host.write(host_bytes).expect("host bytes");
        let visible = host.visible_snapshot(LIMITS).expect("visible");
        let mut viewer = Terminal::from_snapshot(&visible, policy).expect("viewer");
        host.write(suffix).expect("host suffix");
        viewer.write(suffix).expect("viewer suffix");
        assert_eq!(appearance(&mut viewer), appearance(&mut host));
    }

    #[test]
    fn a_cli_replay_draws_only_the_screen_and_keeps_the_scrollback_that_shows_it() {
        let mut host = Terminal::new(20, 4, TerminalPolicy::default()).expect("host");
        for line in 0..30 {
            host.write(format!("line {line}\r\n").as_bytes())
                .expect("output");
        }

        let replay = host.format_finite_cli_replay().expect("replay");
        let text = String::from_utf8_lossy(&replay);

        assert!(replay.starts_with(CLI_REPLAY_PREFIX) && replay.ends_with(CLI_REPLAY_SUFFIX));
        assert!(!text.contains("\x1b[3J"));
        assert!(text.contains("line 27") && text.contains("line 29"));
        assert!(!text.contains("line 26"));
    }

    #[test]
    fn a_visible_snapshot_gives_the_exact_rows_modes_and_cursor_of_both_screens() {
        let mut history = Vec::new();
        for line in 0..300 {
            history.extend_from_slice(format!("history line {line}\r\n").as_bytes());
        }
        let plain = [&history[..], b"one\r\ntwo \x1b[1;32mgreen\x1b[0m\r\n$ top"].concat();
        let full_screen = [&plain[..], b"\x1b[?1049h\x1b[2;3Hfull \x1b[7mscreen"].concat();
        let wrapped = [
            &history[..],
            &b"0123456789".repeat(9)[..],
            b"\x1b[?2004h\x1b[2;3r",
        ]
        .concat();
        for host in [&plain[..], &full_screen[..], &wrapped[..]] {
            for suffix in [
                &b""[..],
                b"Z\r\nnext \x1b[4mline\x1b[0m\ttab",
                b"\x1b[?1049l",
                b"\x1b[?1049l\r\n$ next\r\nmore\r\nlines\r\nscroll",
            ] {
                visible_parity(host, suffix);
            }
        }
    }

    #[test]
    fn a_visible_snapshot_leaves_out_the_older_history() {
        let mut host = Terminal::new(100, 30, TerminalPolicy::default()).expect("host");
        for line in 0..2000 {
            host.write(format!("history line {line} with some more text in it\r\n").as_bytes())
                .expect("write");
        }
        let visible = host.visible_snapshot(LIMITS).expect("visible");
        let full = host.snapshot(LIMITS).expect("full");
        assert!(visible.len() * 2 < full.len());
        let mut viewer =
            Terminal::from_snapshot(&visible, TerminalPolicy::default()).expect("viewer");
        assert!(viewer.snapshot(LIMITS).expect("viewer").len() < full.len());
        assert_eq!(viewer.state().expect("viewer"), host.state().expect("host"));
    }

    #[test]
    fn snapshot_preserves_pending_wrap_and_saved_cursor() {
        assert_snapshot_suffix_parity(8, 3, b"12345678\x1b7", b"\x1b8Z");
    }

    #[test]
    fn snapshot_preserves_ss2_and_ss3() {
        assert_snapshot_suffix_parity(8, 2, b"\x1b*0\x1bN", b"q");
        assert_snapshot_suffix_parity(8, 2, b"\x1b+0\x1bO", b"q");
    }

    #[test]
    fn snapshot_restores_when_a_wrapped_line_is_longer_than_the_history() {
        assert_snapshot_suffix_parity(100, 30, &vec![b'x'; 110_000], b"\r\nafter");
    }
}
