use tokio::process::Command;

use crate::{AppError, Result};

pub(super) fn browser_command(_url: &str) -> Result<Command> {
    Err(AppError::Unsupported {
        reason: "automatic browser launch is not supported on this platform".to_owned(),
    })
}
