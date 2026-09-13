use super::types::{Checkpoint, TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES, TerminalScreen};
use ghostty_vt::{CheckpointLimits, Screen, SemanticCheckpoint, Terminal, TerminalPolicy};
use kodosi_pty::{KodosiError, Result};

use super::emulator::TerminalHistoryPolicy;

pub fn validate_terminal_checkpoint(
    checkpoint: &Checkpoint,
    history: TerminalHistoryPolicy,
) -> Result<()> {
    let mut terminal = Terminal::new(
        checkpoint.cols(),
        checkpoint.rows(),
        TerminalPolicy {
            continuation_max_bytes: history.continuation_max_bytes,
            scrollback_max_bytes: history.max_bytes,
            scrollback_max_lines: history.max_lines,
            clipboard_enabled: false,
            dark: true,
        },
    )
    .map_err(checkpoint_error)?;
    terminal
        .restore_semantic_checkpoint(
            &SemanticCheckpoint::from(checkpoint.semantic_checkpoint.clone()),
            CheckpointLimits {
                max_json_bytes: TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES,
                ..CheckpointLimits::default()
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
        return Err(KodosiError::Unsupported(format!(
            "terminal checkpoint metadata disagrees with restored semantic checkpoint: metadata={}x{} {:?} cursor={},{},hidden={}; restored={}x{} {:?} cursor={},{},hidden={}",
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

fn checkpoint_error(error: ghostty_vt::Error) -> KodosiError {
    KodosiError::Unsupported(format!(
        "Ghostty terminal checkpoint validation failed: {error}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

    fn checkpoint_fixture(terminal: &mut Terminal) -> TestResult<Checkpoint> {
        let semantic = terminal.semantic_checkpoint(CheckpointLimits {
            max_json_bytes: TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES,
            ..CheckpointLimits::default()
        })?;
        let state = terminal.state()?;
        Ok(Checkpoint::new(
            super::super::types::TerminalSize::new(state.rows, state.cols)?,
            terminal_screen(state.active_screen),
            semantic.into_bytes(),
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
            br#"{"schemaVersion":2}"#.to_vec(),
            0,
            0,
            false,
        )?;
        assert!(
            validate_terminal_checkpoint(&malformed, TerminalHistoryPolicy::default()).is_err()
        );

        let mut authority = Terminal::new(12, 4, TerminalPolicy::default())?;
        authority.write(b"checkpoint")?;
        let captured = checkpoint_fixture(&mut authority)?;
        validate_terminal_checkpoint(&captured, TerminalHistoryPolicy::default())?;

        let contradictory = Checkpoint::new(
            super::super::types::TerminalSize::new(5, 12)?,
            captured.active_screen,
            captured.semantic_checkpoint.clone(),
            captured.cursor_x(),
            captured.cursor_y(),
            captured.cursor_hidden(),
        )?;
        assert!(
            validate_terminal_checkpoint(&contradictory, TerminalHistoryPolicy::default())
                .is_err_and(|error| error.to_string().contains("metadata disagrees"))
        );
        Ok(())
    }
}
