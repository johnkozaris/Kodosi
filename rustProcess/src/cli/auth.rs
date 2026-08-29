use serde::Serialize;

use crate::{
    AppError, AuthCommand as RuntimeAuthCommand, AuthEvent, AuthRequiredReason, HostEvent, Result,
    headless_host::{self, HeadlessHostAuthState},
    runtime::{
        one_shot::{LoginStart, LoginSummary, OneShotApp},
        rooms::RoomApplication,
    },
};

use super::{
    args::{AuthLoginArgs, CliAuthAction},
    output::OutputMode,
    status::{
        CliRemoteSessionOutput, CliRoomOutput, CliUserOutput, collect_auth_status, display_name,
        render_auth_status_human, write_signed_in_guidance,
    },
};

#[derive(Debug, Serialize)]
struct LoginOutput {
    user: Option<CliUserOutput>,
    host_running: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rooms: Vec<CliRoomOutput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    owned_remote_sessions: Vec<CliRemoteSessionOutput>,
}

pub(in crate::cli) async fn run_auth_command(
    command: CliAuthAction,
    output: OutputMode,
) -> Result<()> {
    match command {
        CliAuthAction::Login(args) => Box::pin(run_auth_login(args, output)).await,
        CliAuthAction::Logout => run_auth_logout(output).await,
        CliAuthAction::Status => run_auth_status(output).await,
    }
}

async fn run_auth_login(args: AuthLoginArgs, output: OutputMode) -> Result<()> {
    if output.json && !args.no_wait {
        return Err(AppError::Unsupported {
            reason: "`auth login --json` requires --no-wait so the device code can be returned"
                .to_owned(),
        });
    }
    if args.no_wait {
        let (host, _) = headless_host::ensure_host_running().await?;
        return run_auth_login_via_host(host, true, output).await;
    }
    if let Some(host) = headless_host::connect_existing_host().await? {
        return run_auth_login_via_host(host, false, output).await;
    }

    let mut app = OneShotApp::load()?;
    match app.start_login().await? {
        LoginStart::AlreadyReady(summary) => render_login_summary(output, summary, false),
        LoginStart::Pending(prompt) => {
            write_device_login_prompt(output, &prompt.user_code, &prompt.verification_uri, false)?;
            let summary = {
                let _spinner = output.spinner("Waiting for device login approval...");
                app.wait_for_login().await?
            };
            render_login_summary(output, summary, false)
        }
    }
}

async fn run_auth_login_via_host(
    mut host: headless_host::HeadlessHostClient,
    no_wait: bool,
    output: OutputMode,
) -> Result<()> {
    match host.snapshot().await?.auth {
        HeadlessHostAuthState::Ready => {
            return finish_auth_login_from_host(output).await;
        }
        HeadlessHostAuthState::WaitingForApproval {
            user_code,
            verification_uri,
        } => {
            if no_wait {
                return render_pending_login(output, &user_code, &verification_uri, true);
            }
            write_device_login_prompt(output, &user_code, &verification_uri, true)?;
            return wait_for_host_auth_ready(host, output, true).await;
        }
        HeadlessHostAuthState::RequiresLogin { .. } | HeadlessHostAuthState::Unknown => {}
    }

    host.send_auth(RuntimeAuthCommand::LoginStart).await?;
    if no_wait {
        wait_for_host_login_prompt(host, output).await
    } else {
        wait_for_host_auth_ready(host, output, false).await
    }
}

async fn auth_state_after_connection_crossover(operation: &str) -> Result<HeadlessHostAuthState> {
    let Some(mut host) = headless_host::connect_existing_host().await? else {
        return Err(AppError::Unsupported {
            reason: format!("headless host stopped while {operation}"),
        });
    };
    Ok(host.snapshot().await?.auth)
}

async fn wait_for_host_login_prompt(
    mut host: headless_host::HeadlessHostClient,
    output: OutputMode,
) -> Result<()> {
    loop {
        let Some(message) = host.recv().await? else {
            return match auth_state_after_connection_crossover("returning a login code").await? {
                HeadlessHostAuthState::Ready => finish_auth_login_from_host(output).await,
                _ => Err(AppError::Unsupported {
                    reason: "headless host changed account context before returning a login code"
                        .to_owned(),
                }),
            };
        };
        match message {
            HostEvent::Auth(AuthEvent::Ready { .. }) => {
                return finish_auth_login_from_host(output).await;
            }
            HostEvent::Auth(AuthEvent::DeviceCode {
                user_code,
                verification_uri,
            }) => {
                return render_pending_login(output, &user_code, &verification_uri, true);
            }
            HostEvent::Auth(AuthEvent::Error { message, .. }) => {
                return Err(AppError::Unsupported {
                    reason: format!("login failed: {message}"),
                });
            }
            _ => {}
        }
    }
}

async fn wait_for_host_auth_ready(
    mut host: headless_host::HeadlessHostClient,
    output: OutputMode,
    mut showed_device_code: bool,
) -> Result<()> {
    let mut spinner = if showed_device_code {
        output.spinner("Waiting for device login approval...")
    } else {
        None
    };

    loop {
        let message = tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                return Err(AppError::Unsupported {
                    reason: "login cancelled".to_owned(),
                });
            }
            message = host.recv() => message?,
        };

        let Some(message) = message else {
            return match auth_state_after_connection_crossover("login completed").await? {
                HeadlessHostAuthState::Ready => finish_auth_login_from_host(output).await,
                _ => Err(AppError::Unsupported {
                    reason: "headless host changed account context before login completed"
                        .to_owned(),
                }),
            };
        };

        match message {
            HostEvent::Auth(AuthEvent::Ready { .. }) => {
                drop(spinner);
                return finish_auth_login_from_host(output).await;
            }
            HostEvent::Auth(AuthEvent::DeviceCode {
                user_code,
                verification_uri,
            }) if !showed_device_code => {
                drop(spinner.take());
                write_device_login_prompt(output, &user_code, &verification_uri, true)?;
                showed_device_code = true;
                spinner = output.spinner("Waiting for device login approval...");
            }
            HostEvent::Auth(AuthEvent::Error { message, .. }) => {
                return Err(AppError::Unsupported {
                    reason: format!("login failed: {message}"),
                });
            }
            _ => {}
        }
    }
}

fn write_device_login_prompt(
    output: OutputMode,
    user_code: &str,
    verification_uri: &str,
    host_running: bool,
) -> Result<()> {
    output.write_required_line(if host_running {
        "Starting device login flow in the running host."
    } else {
        "Starting device login flow."
    })?;
    output.write_required_line(format!("Verification URL: {verification_uri}"))?;
    output.write_required_line(format!("Device code: {user_code}"))
}

fn render_pending_login(
    output: OutputMode,
    user_code: &str,
    verification_uri: &str,
    host_running: bool,
) -> Result<()> {
    if output.json {
        return output.write_json(&serde_json::json!({
            "status": "waiting_for_approval",
            "user_code": user_code,
            "verification_uri": verification_uri,
            "host_running": host_running,
        }));
    }
    write_device_login_prompt(output, user_code, verification_uri, host_running)?;
    output.write_required_line("Login is pending; run `kodosi auth status` after approving.")
}

async fn finish_auth_login_from_host(output: OutputMode) -> Result<()> {
    let (user, rooms, owned_remote_sessions) = best_effort_auth_enrichment().await;
    render_login_output(output, user, rooms, owned_remote_sessions, true)
}

fn render_login_summary(
    output: OutputMode,
    summary: LoginSummary,
    host_running: bool,
) -> Result<()> {
    let user = summary.user.map(CliUserOutput::from);
    let rooms: Vec<_> = summary.rooms.into_iter().map(CliRoomOutput::from).collect();
    let owned_remote_sessions: Vec<_> = summary
        .owned_remote_sessions
        .into_iter()
        .map(CliRemoteSessionOutput::from)
        .collect();
    render_login_output(output, user, rooms, owned_remote_sessions, host_running)
}

fn render_login_output(
    output: OutputMode,
    user: Option<CliUserOutput>,
    rooms: Vec<CliRoomOutput>,
    owned_remote_sessions: Vec<CliRemoteSessionOutput>,
    host_running: bool,
) -> Result<()> {
    if output.json {
        return output.write_json(&LoginOutput {
            user,
            host_running,
            rooms,
            owned_remote_sessions,
        });
    }

    let user_name = user
        .as_ref()
        .map_or_else(|| "your account".to_owned(), display_name);
    output.write_line(format!("Signed in as {user_name}."))?;
    if !output.quiet {
        write_signed_in_guidance(output, host_running, &rooms, &owned_remote_sessions)?;
    }
    Ok(())
}

pub(in crate::cli) async fn best_effort_auth_enrichment() -> (
    Option<CliUserOutput>,
    Vec<CliRoomOutput>,
    Vec<CliRemoteSessionOutput>,
) {
    let Ok(mut app) = OneShotApp::load() else {
        return (None, Vec::new(), Vec::new());
    };
    if crate::cli::client::ensure_remote_command_access(&mut app, "kodosi auth status")
        .await
        .is_err()
    {
        return (None, Vec::new(), Vec::new());
    }
    let user = app.fetch_current_user().await.ok().map(CliUserOutput::from);
    let rooms = RoomApplication::new(app.runtime())
        .fetch_rooms()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(CliRoomOutput::from)
        .collect();
    let owned_remote_sessions = app
        .fetch_my_sessions()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(CliRemoteSessionOutput::from)
        .collect();
    (user, rooms, owned_remote_sessions)
}

async fn run_auth_logout(output: OutputMode) -> Result<()> {
    if let Some(mut host) = headless_host::connect_existing_host().await? {
        host.send_auth(RuntimeAuthCommand::Logout).await?;
        wait_for_host_signed_out(host, output).await?;
        if output.json {
            return output.write_json(&serde_json::json!({ "signed_out": true }));
        }
        output.write_line("Signed out.")?;
        return Ok(());
    }

    let mut app = OneShotApp::load()?;
    app.logout().await?;

    if output.json {
        return output.write_json(&serde_json::json!({ "signed_out": true }));
    }
    output.write_line("Signed out.")?;
    Ok(())
}

async fn wait_for_host_signed_out(
    mut host: headless_host::HeadlessHostClient,
    output: OutputMode,
) -> Result<()> {
    use std::time::Duration;

    let _spinner = output.spinner("Waiting for running host to sign out...");
    loop {
        let message = tokio::time::timeout(Duration::from_secs(10), host.recv())
            .await
            .map_err(|_| AppError::Unsupported {
                reason: "timed out waiting for the running host to sign out".to_owned(),
            })??;

        let Some(message) = message else {
            return match auth_state_after_connection_crossover("logout completed").await? {
                HeadlessHostAuthState::RequiresLogin {
                    reason: AuthRequiredReason::SignedOut,
                } => Ok(()),
                _ => Err(AppError::Unsupported {
                    reason: "headless host changed account context before logout completed"
                        .to_owned(),
                }),
            };
        };

        match message {
            HostEvent::Auth(AuthEvent::Required {
                reason: AuthRequiredReason::SignedOut,
                ..
            }) => return Ok(()),
            HostEvent::Auth(AuthEvent::Error { operation, message })
                if operation == RuntimeAuthCommand::Logout.operation() =>
            {
                return Err(AppError::Unsupported {
                    reason: format!("logout failed: {message}"),
                });
            }
            _ => {}
        }
    }
}

async fn run_auth_status(output: OutputMode) -> Result<()> {
    let status = collect_auth_status().await?;

    if output.json {
        return output.write_json(&status);
    }

    render_auth_status_human(output, &status)
}
