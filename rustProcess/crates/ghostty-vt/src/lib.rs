#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        reason = "tests use panic-on-error assertions for failure clarity"
    )
)]

pub use ghostty_vt_sys::raw::{
    CheckpointLimits, ClipboardContent, ClipboardLocation, ClipboardWriteHandler,
    ClipboardWriteOutcome, CompressionProgress, Effect, Error, Key, Modifiers, Screen,
    TerminalState,
};

use ghostty_vt_sys::raw::{Format, FormatOptions, Terminal as RawTerminal};

const FORMATTER_PROFILE_MAX_BYTES: usize = 8 * 1024 * 1024;

const CLI_REPLAY_PREFIX: &[u8] = b"\x1b]8;;\x1b\\\x1b[0m\x1b[2J\x1b[3J\x1b[H";
const CLI_REPLAY_SUFFIX: &[u8] = b"\x1b]8;;\x1b\\\x1b[0m";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalPolicy {
    pub continuation_max_bytes: usize,
    pub scrollback_max_bytes: usize,
    pub scrollback_max_lines: usize,
    pub clipboard_enabled: bool,
    pub dark: bool,
}

impl Default for TerminalPolicy {
    fn default() -> Self {
        Self {
            continuation_max_bytes: 1024 * 1024,
            scrollback_max_bytes: 64 * 1024 * 1024,
            scrollback_max_lines: 1_024,
            clipboard_enabled: false,
            dark: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticCheckpoint(Vec<u8>);

impl SemanticCheckpoint {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

impl From<Vec<u8>> for SemanticCheckpoint {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

impl AsRef<[u8]> for SemanticCheckpoint {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
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
        Ok(Self {
            raw: RawTerminal::new(
                cols,
                rows,
                policy.continuation_max_bytes,
                policy.scrollback_max_bytes,
                policy.scrollback_max_lines,
                policy.clipboard_enabled,
                policy.dark,
            )?,
            processing_failed: false,
        })
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

    pub fn set_clipboard_enabled(&mut self, enabled: bool) -> Result<(), Error> {
        self.raw.set_clipboard_enabled(enabled)
    }

    pub fn set_clipboard_writer(
        &mut self,
        writer: Option<ClipboardWriteHandler>,
    ) -> Result<(), Error> {
        self.raw.set_clipboard_writer(writer)
    }

    pub fn set_dark(&mut self, dark: bool) -> Result<(), Error> {
        self.raw.set_dark(dark)
    }

    pub fn encode_key(&mut self, key: Key, modifiers: Modifiers) -> Result<Vec<u8>, Error> {
        self.raw.encode_key(key, modifiers)
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

    pub fn semantic_checkpoint(
        &mut self,
        limits: CheckpointLimits,
    ) -> Result<SemanticCheckpoint, Error> {
        let (bytes, _) = self.raw.encode_checkpoint(limits)?;
        Ok(SemanticCheckpoint(bytes))
    }

    pub fn restore_semantic_checkpoint(
        &mut self,
        checkpoint: &SemanticCheckpoint,
        limits: CheckpointLimits,
    ) -> Result<(), Error> {
        self.raw
            .restore_checkpoint(checkpoint.as_bytes(), limits)
            .map(|_| ())
    }

    pub fn format_finite_cli_replay(&mut self) -> Result<Vec<u8>, Error> {
        let active_screen = self.raw.state()?.active_screen;
        let screen_vt = self.raw.format(
            FormatOptions {
                format: Format::Vt,
                unwrap: false,
                trim: false,
                screen: Some(active_screen),
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

    fn assert_checkpoint_suffix_parity(cols: u16, rows: u16, prefix: &[u8], suffix: &[u8]) {
        let mut source = Terminal::new(cols, rows, TerminalPolicy::default()).expect("source");
        source.write(prefix).expect("source prefix");
        let checkpoint = source
            .semantic_checkpoint(CheckpointLimits::default())
            .expect("checkpoint");

        let mut restored = Terminal::new(cols, rows, TerminalPolicy::default()).expect("restored");
        restored
            .restore_semantic_checkpoint(&checkpoint, CheckpointLimits::default())
            .expect("restore");
        source.write(suffix).expect("source suffix");
        restored.write(suffix).expect("restored suffix");

        let source_after = source
            .semantic_checkpoint(CheckpointLimits::default())
            .expect("source after");
        let restored_after = restored
            .semantic_checkpoint(CheckpointLimits::default())
            .expect("restored after");
        assert_eq!(restored_after, source_after);
    }

    #[test]
    fn semantic_checkpoint_is_owned_json() {
        let mut terminal = Terminal::new(20, 4, TerminalPolicy::default()).expect("terminal");
        terminal.write(b"owned").expect("write");

        let checkpoint = terminal
            .semantic_checkpoint(CheckpointLimits::default())
            .expect("checkpoint");

        assert!(checkpoint.as_bytes().starts_with(b"{"));
    }

    #[test]
    fn semantic_checkpoint_preserves_pending_wrap_and_saved_cursor() {
        assert_checkpoint_suffix_parity(8, 3, b"12345678\x1b7", b"\x1b8Z");
    }

    #[test]
    fn semantic_checkpoint_preserves_ss2_and_ss3() {
        assert_checkpoint_suffix_parity(8, 2, b"\x1b*0\x1bN", b"q");
        assert_checkpoint_suffix_parity(8, 2, b"\x1b+0\x1bO", b"q");
    }
}
