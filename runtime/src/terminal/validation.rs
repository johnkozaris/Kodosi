use super::types::{Checkpoint, TerminalScreen};
use crate::{Error, Result};
use ghostty_vt::{Screen, Terminal, TerminalPolicy};

pub fn validate_terminal_checkpoint(checkpoint: &Checkpoint) -> Result<()> {
    let mut terminal = Terminal::from_snapshot(
        &checkpoint.snapshot,
        TerminalPolicy {
            console: checkpoint.console,
            ..TerminalPolicy::default()
        },
    )
    .map_err(checkpoint_error)?;
    validate_restored_metadata(&mut terminal, checkpoint)
}

fn validate_restored_metadata(terminal: &mut Terminal, checkpoint: &Checkpoint) -> Result<()> {
    let state = terminal.state().map_err(checkpoint_error)?;
    let active_screen = terminal_screen(state.active_screen);
    let cursor_hidden = !state.cursor_visible;
    if state.rows != checkpoint.rows()
        || state.cols != checkpoint.cols()
        || active_screen != checkpoint.active_screen
        || state.cursor_x != checkpoint.cursor_x()
        || state.cursor_y != checkpoint.cursor_y()
        || cursor_hidden != checkpoint.cursor_hidden()
    {
        return Err(Error::Other(format!(
            "terminal checkpoint metadata disagrees with restored snapshot: metadata={}x{} {:?} cursor={},{},hidden={}; restored={}x{} {:?} cursor={},{},hidden={}",
            checkpoint.cols(),
            checkpoint.rows(),
            checkpoint.active_screen,
            checkpoint.cursor_x(),
            checkpoint.cursor_y(),
            checkpoint.cursor_hidden(),
            state.cols,
            state.rows,
            active_screen,
            state.cursor_x,
            state.cursor_y,
            cursor_hidden,
        )));
    }
    Ok(())
}

fn terminal_screen(screen: Screen) -> TerminalScreen {
    match screen {
        Screen::Primary => TerminalScreen::Primary,
        Screen::Alternate => TerminalScreen::Alternate,
    }
}

fn checkpoint_error(error: ghostty_vt::Error) -> Error {
    Error::Other(format!(
        "Ghostty terminal checkpoint validation failed: {error}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

    fn checkpoint_fixture(terminal: &mut Terminal) -> TestResult<Checkpoint> {
        let snapshot = terminal.snapshot(super::super::types::TERMINAL_SNAPSHOT_MAX_BYTES)?;
        let state = terminal.state()?;
        Ok(Checkpoint::new(
            super::super::types::TerminalSize::new(state.rows, state.cols)?,
            terminal_screen(state.active_screen),
            snapshot,
            state.cursor_x,
            state.cursor_y,
            !state.cursor_visible,
        )?)
    }

    #[test]
    fn validator_accepts_valid_and_rejects_malformed_or_contradictory_checkpoints() -> TestResult {
        let malformed = Checkpoint::new(
            super::super::types::TerminalSize::new(4, 12)?,
            TerminalScreen::Primary,
            b"GHOSTSNP\x01\x00".to_vec(),
            0,
            0,
            false,
        )?;
        assert!(validate_terminal_checkpoint(&malformed).is_err());

        let mut authority = Terminal::new(12, 4, TerminalPolicy::default())?;
        authority.write(b"checkpoint")?;
        let captured = checkpoint_fixture(&mut authority)?;
        validate_terminal_checkpoint(&captured)?;

        let contradictory = Checkpoint::new(
            super::super::types::TerminalSize::new(5, 12)?,
            captured.active_screen,
            captured.snapshot.clone(),
            captured.cursor_x(),
            captured.cursor_y(),
            captured.cursor_hidden(),
        )?;
        assert!(
            validate_terminal_checkpoint(&contradictory)
                .is_err_and(|error| error.to_string().contains("metadata disagrees"))
        );
        Ok(())
    }
}
