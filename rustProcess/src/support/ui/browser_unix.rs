use tokio::process::Command;

use crate::Result;

pub(super) fn browser_command(url: &str) -> Result<Command> {
    let mut command = Command::new("xdg-open");
    command.arg(url);
    Ok(command)
}
