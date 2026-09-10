use tokio::process::Command;

use crate::Result;

#[expect(
    clippy::unnecessary_wraps,
    reason = "platform browser factories share the unsupported-platform Result contract"
)]
pub(super) fn browser_command(url: &str) -> Result<Command> {
    let mut command = Command::new("xdg-open");
    command.arg(url);
    Ok(command)
}
