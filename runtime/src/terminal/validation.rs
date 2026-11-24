use super::types::Checkpoint;
use crate::{Error, Result};
use ghostty_vt::{Terminal, TerminalPolicy};

/// A viewer restores each checkpoint before it uses it, so a snapshot that does
/// not decode never reaches a client.
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
