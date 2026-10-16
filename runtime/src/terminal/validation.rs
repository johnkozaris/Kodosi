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
            ..TerminalPolicy::default()
        },
    )
    .map(drop)
    .map_err(|error| {
        Error::Other(format!(
            "Ghostty terminal checkpoint validation failed: {error}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::TERMINAL_SNAPSHOT_MAX_BYTES;

    type TestResult<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

    #[test]
    fn a_checkpoint_is_valid_only_when_its_snapshot_restores() -> TestResult {
        let mut host = Terminal::new(12, 4, TerminalPolicy::default())?;
        host.write(b"checkpoint")?;
        let snapshot = host.snapshot(TERMINAL_SNAPSHOT_MAX_BYTES)?;
        validate_terminal_checkpoint(&Checkpoint::new(snapshot.clone(), false)?)?;

        let mut damaged = snapshot;
        let last = damaged.len() - 1;
        damaged[last] ^= 1;
        assert!(validate_terminal_checkpoint(&Checkpoint::new(damaged, false)?).is_err());
        Ok(())
    }
}
