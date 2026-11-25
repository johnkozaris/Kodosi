use super::types::Checkpoint;
use crate::{Error, Result};
use ghostty_vt::{Terminal, TerminalPolicy};

/// Test-restores each remote checkpoint into a throwaway emulator so corrupt snapshots are
/// rejected before reaching clients.
pub(super) fn validate_terminal_checkpoint(checkpoint: &Checkpoint) -> Result<()> {
    Terminal::from_snapshot(
        checkpoint.snapshot(),
        TerminalPolicy {
            console: checkpoint.console,
        },
    )
    .map(drop)
    .map_err(|error| {
        Error::Other(format!(
            "Ghostty terminal checkpoint validation failed: {error}"
        ))
    })
}
