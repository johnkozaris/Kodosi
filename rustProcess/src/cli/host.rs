use crate::{Result, headless_host};

use super::{
    args::CliHostAction,
    output::OutputMode,
    status::{collect_host_status, render_host_status_human},
};

pub(in crate::cli) async fn run_host_command(
    command: CliHostAction,
    output: OutputMode,
) -> Result<()> {
    match command {
        CliHostAction::Start => {
            let (_, started) = headless_host::ensure_host_running().await?;
            let info = headless_host::host_info().await?;
            if output.json {
                return output.write_json(&serde_json::json!({
                    "running": true,
                    "started": started,
                    "pid": info.as_ref().map(|value| value.pid),
                    "started_at": info.as_ref().map(|value| value.started_at.clone()),
                }));
            }
            if started {
                output.write_line("Headless host started.")?;
            } else {
                output.write_line("Headless host is already running.")?;
            }
            output.write_line("Start a session with `kodosi session start --name <name>`.")?;
            Ok(())
        }
        CliHostAction::Status => {
            let status = collect_host_status().await?;
            if output.json {
                return output.write_json(&status);
            }
            render_host_status_human(output, &status)
        }
        CliHostAction::Stop => {
            let stopped = headless_host::stop_host().await?;
            if output.json {
                return output.write_json(&serde_json::json!({ "stopped": stopped }));
            }
            if stopped {
                output.write_line("Headless host stopped.")?;
            } else {
                output.write_line("Headless host was not running.")?;
            }
            Ok(())
        }
    }
}
