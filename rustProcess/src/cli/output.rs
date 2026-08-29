use std::io::{self, IsTerminal, Write};

use serde::Serialize;

use crate::{AppError, Result};

use super::args::Cli;

#[derive(Debug, Clone, Copy)]
pub(in crate::cli) struct OutputMode {
    pub(in crate::cli) json: bool,
    pub(in crate::cli) quiet: bool,
    stderr_is_terminal: bool,
}

impl OutputMode {
    pub(in crate::cli) fn from_cli(cli: &Cli) -> Self {
        Self {
            json: cli.json,
            quiet: cli.quiet,
            stderr_is_terminal: io::stderr().is_terminal(),
        }
    }

    pub(in crate::cli) fn progress_enabled(self) -> bool {
        !self.json && !self.quiet && self.stderr_is_terminal
    }

    pub(in crate::cli) fn spinner(self, message: &'static str) -> Option<ProgressGuard> {
        if !self.progress_enabled() {
            return None;
        }

        Some(ProgressGuard::new(message))
    }

    #[expect(clippy::unused_self, reason = "API grouping")]
    pub(in crate::cli) fn write_json<T: Serialize>(self, value: &T) -> Result<()> {
        let mut stdout = io::stdout().lock();
        serde_json::to_writer_pretty(&mut stdout, value).map_err(AppError::Json)?;
        stdout.write_all(b"\n").map_err(AppError::Io)
    }

    pub(in crate::cli) fn write_line(self, line: impl AsRef<str>) -> Result<()> {
        if self.quiet {
            return Ok(());
        }
        self.write_required_line(line)
    }

    #[expect(clippy::unused_self, reason = "API grouping")]
    pub(in crate::cli) fn write_required_line(self, line: impl AsRef<str>) -> Result<()> {
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "{}", line.as_ref()).map_err(AppError::Io)
    }

    pub(in crate::cli) fn write_error(self, error: &AppError) -> Result<()> {
        if self.json {
            return self.write_json(&serde_json::json!({
                "ok": false,
                "error": {
                    "code": error.code(),
                    "message": error.to_string(),
                }
            }));
        }
        let mut stderr = io::stderr().lock();
        writeln!(stderr, "Error: {error}").map_err(AppError::Io)
    }
}

pub(in crate::cli) struct ProgressGuard {
    bar: indicatif::ProgressBar,
}

impl ProgressGuard {
    fn new(message: &'static str) -> Self {
        use std::time::Duration;

        use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

        let bar = ProgressBar::new_spinner();
        bar.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
        if let Ok(style) = ProgressStyle::with_template("{spinner} {msg}") {
            bar.set_style(style);
        }
        bar.set_message(message);
        bar.enable_steady_tick(Duration::from_millis(120));
        Self { bar }
    }
}

impl Drop for ProgressGuard {
    fn drop(&mut self) {
        self.bar.finish_and_clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_requires_human_stderr() {
        let output = OutputMode {
            json: false,
            quiet: false,
            stderr_is_terminal: true,
        };
        assert!(output.progress_enabled());
    }

    #[test]
    fn progress_is_disabled_for_machine_and_quiet_modes() {
        for output in [
            OutputMode {
                json: true,
                quiet: false,
                stderr_is_terminal: true,
            },
            OutputMode {
                json: false,
                quiet: true,
                stderr_is_terminal: true,
            },
            OutputMode {
                json: false,
                quiet: false,
                stderr_is_terminal: false,
            },
        ] {
            assert!(!output.progress_enabled());
        }
    }
}
