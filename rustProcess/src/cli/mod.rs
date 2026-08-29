mod agent;
mod args;
mod auth;
mod client;
mod device;
mod doctor;
mod host;
mod msg;
mod output;
mod repair;
mod rooms;
mod session;
mod status;
mod terminal_client;
mod trust;

use clap::Parser;

use crate::{Result, headless_host};

use self::{
    args::{Cli, CliCommand},
    output::OutputMode,
};

const OWNER_DASHBOARD_URL: &str = "https://kodosi.com/dashboard";
const OWNER_SESSION_URL_PREFIX: &str = "https://kodosi.com/my-sessions";
const BACKEND_API_CONFIG_KEY: &str = "backend.api";

#[expect(
    clippy::future_not_send,
    reason = "session attach owns a non-Send Ghostty mirror on the CLI current-thread runtime"
)]
pub(crate) async fn run() -> std::process::ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => return write_parse_error(&error),
    };
    let protocol_stdout_only = matches!(&cli.command, CliCommand::InternalRoomChannelServe);
    if let Some(path) = cli.config.clone()
        && let Err(error) = crate::config::set_cli_config_path(path)
    {
        let output = OutputMode::from_cli(&cli);
        write_run_error(&error, output, protocol_stdout_only);
        return std::process::ExitCode::FAILURE;
    }
    let output = OutputMode::from_cli(&cli);
    let result = Box::pin(dispatch(cli.command, output)).await;
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            write_run_error(&error, output, protocol_stdout_only);
            std::process::ExitCode::FAILURE
        }
    }
}

fn write_run_error(error: &crate::AppError, output: OutputMode, protocol_stdout_only: bool) {
    if protocol_stdout_only {
        use std::io::Write;

        drop(writeln!(
            std::io::stderr().lock(),
            "Room channel server failed: {error}"
        ));
    } else {
        drop(output.write_error(error));
    }
}

fn write_parse_error(error: &clap::Error) -> std::process::ExitCode {
    use clap::error::ErrorKind;
    use std::io::Write;

    if std::env::args_os().any(|arg| arg == "__internal-room-channel-serve") {
        drop(writeln!(std::io::stderr().lock(), "{error}"));
        return if matches!(
            error.kind(),
            ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
        ) {
            std::process::ExitCode::SUCCESS
        } else {
            std::process::ExitCode::from(2)
        };
    }

    if matches!(
        error.kind(),
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
    ) {
        return match error.print() {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(_) => std::process::ExitCode::FAILURE,
        };
    }

    let json_requested = std::env::args_os().any(|arg| arg == "--json");
    if json_requested {
        let payload = serde_json::json!({
            "ok": false,
            "error": {
                "code": "cli_usage",
                "message": error.to_string(),
            }
        });
        let mut stdout = std::io::stdout().lock();
        if serde_json::to_writer_pretty(&mut stdout, &payload).is_ok()
            && stdout.write_all(b"\n").is_ok()
        {
            return std::process::ExitCode::from(2);
        }
    } else {
        drop(error.print());
    }
    std::process::ExitCode::from(2)
}

#[expect(
    clippy::future_not_send,
    reason = "session attach owns a non-Send Ghostty mirror on the CLI current-thread runtime"
)]
async fn dispatch(command: CliCommand, output: OutputMode) -> Result<()> {
    match command {
        CliCommand::Auth(command) => Box::pin(auth::run_auth_command(command, output)).await,
        CliCommand::Device(command) => device::run_device_command(command, output).await,
        CliCommand::Doctor => Box::pin(doctor::run_doctor(output)).await,
        CliCommand::Repair(command) => repair::run_repair_command(&command, output),
        CliCommand::Host(command) => host::run_host_command(command, output).await,
        CliCommand::Session(command) => session::run_session_command(command, output).await,
        CliCommand::Share(command) => session::run_share_command(command, output).await,
        CliCommand::Trust(command) => trust::run_trust_command(command, output).await,
        CliCommand::Room(command) => rooms::run_room_command(command, output).await,
        CliCommand::Agent(command) => agent::run_agent_command(command, output).await,
        CliCommand::Agents(args) => agent::run_agents_command(args, output).await,
        CliCommand::Msg(command) => msg::run_msg_command(command, output),
        CliCommand::InternalHostServe => headless_host::run_server().await,
        CliCommand::InternalRoomChannelServe => crate::rooms::channel_server::run_server().await,
    }
}
