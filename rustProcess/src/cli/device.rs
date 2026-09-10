use std::io::Write;

use serde::Serialize;

use crate::{Result, headless_host, runtime::one_shot::OneShotApp};

use super::{
    args::{CliDeviceAction, DeviceApproveLinkArgs, DeviceLinkArgs, DeviceRevokeArgs},
    client::{ensure_remote_command_access, ensure_remote_device_command_access},
    output::OutputMode,
};

pub(in crate::cli) async fn run_device_command(
    command: CliDeviceAction,
    output: OutputMode,
) -> Result<()> {
    match command {
        CliDeviceAction::List => run_device_list(output).await,
        CliDeviceAction::Revoke(args) => run_device_revoke(args, output).await,
        CliDeviceAction::Link(args) => run_device_link(args, output).await,
        CliDeviceAction::ApproveLink(args) => run_device_approve_link(args, output).await,
    }
}

#[derive(Debug, Serialize)]
struct DeviceListEntryOutput {
    device_id: String,
    label: String,
    cert_signer_device_id: String,
    cert_issued_at_ms: u64,
    cert_expires_at_ms: Option<u64>,
}

async fn run_device_list(output: OutputMode) -> Result<()> {
    let mut app = OneShotApp::load()?;
    ensure_remote_command_access(&mut app, "kodosi device list").await?;
    let me = app.fetch_current_user().await?;

    let identity = match app.verified_current_identity(&me.id).await {
        Ok(identity) => identity,
        Err(crate::AppError::NotFound) => {
            return render_empty_device_list(output);
        }
        Err(e) => return Err(e),
    };

    let entries: Vec<DeviceListEntryOutput> = identity
        .devices
        .values()
        .map(|device| DeviceListEntryOutput {
            device_id: device.certificate.device_id.clone(),
            label: device.certificate.device_label.clone(),
            cert_signer_device_id: device.certificate.signer_device_id.clone(),
            cert_issued_at_ms: device.certificate.issued_at_ms,
            cert_expires_at_ms: device.certificate.expires_at_ms,
        })
        .collect();

    if output.json {
        return output.write_json(&entries);
    }

    if entries.is_empty() {
        return render_empty_device_list(output);
    }

    output.write_line(format!(
        "Device list generation {}, signed by {}",
        identity.signed_list.generation, identity.signed_list.signer_device_id
    ))?;
    for entry in &entries {
        let signer_label = if entry.cert_signer_device_id == entry.device_id {
            "self-signed".to_owned()
        } else {
            format!("signed by {}", entry.cert_signer_device_id)
        };
        output.write_line(format!(
            "  {}  {}  ({})",
            entry.device_id, entry.label, signer_label
        ))?;
    }
    Ok(())
}

fn render_empty_device_list(output: OutputMode) -> Result<()> {
    if output.json {
        let empty: Vec<DeviceListEntryOutput> = Vec::new();
        output.write_json(&empty)
    } else {
        output.write_line("No enrolled devices.")
    }
}

async fn run_device_revoke(args: DeviceRevokeArgs, output: OutputMode) -> Result<()> {
    let mut app = OneShotApp::load()?;
    let expected_host_account =
        ensure_remote_device_command_access(&mut app, "kodosi device revoke").await?;
    let outcome = if let Some(account_user_id) = expected_host_account {
        headless_host::revoke_device(account_user_id.to_string(), args.device_id.clone()).await?
    } else {
        app.revoke_device(&args.device_id, "kodosi device revoke")
            .await?
    };

    if output.json {
        output.write_json(&serde_json::json!({
            "revoked_device_id": outcome.revoked_device_id,
            "new_generation": outcome.new_generation,
            "history_warning": outcome.history_warning,
        }))
    } else {
        output.write_line(format!(
            "Revoked {}. New device-list generation: {}.",
            outcome.revoked_device_id, outcome.new_generation,
        ))?;
        if let Some(warning) = outcome.history_warning {
            output.write_line(warning)?;
        }
        Ok(())
    }
}

async fn run_device_approve_link(args: DeviceApproveLinkArgs, output: OutputMode) -> Result<()> {
    if !output.json {
        output.write_line("Signing pending device into your device list…")?;
    }
    let mut app = OneShotApp::load()?;
    let expected_host_account =
        ensure_remote_device_command_access(&mut app, "kodosi device approve-link").await?;
    let outcome = if let Some(account_user_id) = expected_host_account {
        headless_host::approve_device_link(account_user_id.to_string(), args.user_code.clone())
            .await?
    } else {
        app.approve_device_link(&args.user_code, "kodosi device approve-link")
            .await?
    };
    let pretty_label = friendly_device_label(&outcome.approved_device_label);
    if output.json {
        output.write_json(&serde_json::json!({
            "approved_user_code": outcome.approved_user_code,
            "approved_device_id": outcome.approved_device_id,
            "new_generation": outcome.new_generation,
        }))
    } else {
        output.write_line(format!(
            "Done. `{pretty_label}` is now enrolled at generation {}.",
            outcome.new_generation
        ))
    }
}

fn friendly_device_label(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        "unnamed device".to_owned()
    } else {
        trimmed.to_owned()
    }
}

async fn run_device_link(args: DeviceLinkArgs, output: OutputMode) -> Result<()> {
    let mut app = OneShotApp::load()?;
    let expected_host_account =
        ensure_remote_device_command_access(&mut app, "kodosi device link").await?;
    let mut host = if expected_host_account.is_some() {
        headless_host::connect_existing_host().await?
    } else {
        None
    };
    let mut one_shot = None;
    let start = if let Some(account_user_id) = expected_host_account {
        headless_host::start_self_device_link(account_user_id.to_string(), args.label.clone())
            .await?
    } else {
        let start = app
            .start_link_this_device(args.label, "kodosi device link")
            .await?;
        one_shot = Some(app);
        start
    };
    print_device_link_instructions(output, &start.user_code, &start.expires_at)?;

    let generation = {
        let _spinner = output.spinner("Waiting for approval on another device...");
        match (host.as_mut(), one_shot.as_mut()) {
            (Some(host), None) => wait_for_host_self_link(host).await,
            (None, Some(app)) => app.wait_for_self_device_link_approval().await,
            _ => Err(crate::AppError::Unsupported {
                reason: "device link lost its runtime authority".to_owned(),
            }),
        }
    };

    match generation {
        Ok(generation) => {
            if output.json {
                output.write_json(&serde_json::json!({
                    "device_id": start.device_id,
                    "new_generation": generation,
                }))
            } else {
                output.write_line("")?;
                output.write_line("Approved. This machine is enrolled and ready.")?;
                if let Some(generation) = generation {
                    output.write_line(format!("    Device list generation: {generation}"))?;
                }
                Ok(())
            }
        }
        Err(error) => Err(error),
    }
}

async fn wait_for_host_self_link(
    host: &mut headless_host::HeadlessHostClient,
) -> Result<Option<u64>> {
    let account_user_id = host
        .account_user_id()
        .ok_or_else(|| crate::AppError::Unsupported {
            reason: "running host lost its resolved account during device linking".to_owned(),
        })?;
    let mut cancellation_requested = false;
    loop {
        let event = tokio::select! {
            _ = tokio::signal::ctrl_c(), if !cancellation_requested => {
                headless_host::cancel_self_device_link(account_user_id.to_string()).await?;
                cancellation_requested = true;
                continue;
            }
            event = host.recv() => event?,
        };
        match event {
            Some(crate::HostEvent::Devices(crate::DeviceEvent::LinkSelfResolved {
                outcome: kodosi_domain::device_link::SelfDeviceLinkOutcome::Approved,
            })) => return Ok(None),
            Some(crate::HostEvent::Devices(crate::DeviceEvent::LinkSelfResolved {
                outcome: kodosi_domain::device_link::SelfDeviceLinkOutcome::Cancelled,
            })) => {
                return Err(crate::AppError::Unsupported {
                    reason: "Link request was cancelled.".to_owned(),
                });
            }
            Some(crate::HostEvent::Devices(crate::DeviceEvent::LinkSelfResolved {
                outcome: kodosi_domain::device_link::SelfDeviceLinkOutcome::Expired,
            })) => {
                return Err(crate::AppError::Unsupported {
                    reason: "Link request expired before approval. Run `kodosi device link` again."
                        .to_owned(),
                });
            }
            Some(crate::HostEvent::Devices(crate::DeviceEvent::LinkSelfResolved {
                outcome: kodosi_domain::device_link::SelfDeviceLinkOutcome::Failed,
            })) => {
                return Err(crate::AppError::Unsupported {
                    reason: "Link request failed.".to_owned(),
                });
            }
            Some(_) => {}
            None => {
                return Err(crate::AppError::Unsupported {
                    reason: "headless host ended before device link resolved".to_owned(),
                });
            }
        }
    }
}

fn print_device_link_instructions(
    output: OutputMode,
    user_code: &str,
    expires_at: &str,
) -> Result<()> {
    if output.json {
        let mut stderr = std::io::stderr().lock();
        serde_json::to_writer(
            &mut stderr,
            &serde_json::json!({
                "event": "device_link_pending",
                "user_code": user_code,
                "expires_at": expires_at,
            }),
        )
        .map_err(crate::AppError::Json)?;
        return stderr.write_all(b"\n").map_err(crate::AppError::Io);
    }

    let expiry_hint = format_remaining_until(expires_at).unwrap_or_else(|| expires_at.to_owned());
    output.write_required_line("")?;
    output.write_required_line(format!("    User code:  {user_code}"))?;
    output.write_required_line(format!("    Expires in: {expiry_hint}"))?;
    output.write_required_line("")?;
    output.write_required_line("On a device you've already used, approve with either:")?;
    output.write_required_line(format!("  · `kodosi device approve-link {user_code}`"))?;
    output.write_required_line("  · desktop app → Settings → Devices → Add a device")?;
    if output.progress_enabled() {
        output.write_required_line("")
    } else {
        output.write_required_line("")?;
        output.write_required_line("Waiting for approval (Ctrl+C to cancel)…")
    }
}

fn format_remaining_until(iso8601: &str) -> Option<String> {
    let expires =
        time::OffsetDateTime::parse(iso8601, &time::format_description::well_known::Rfc3339)
            .ok()?;
    let now = time::OffsetDateTime::now_utc();
    let remaining = expires - now;
    if remaining.is_negative() {
        return Some("expired".to_owned());
    }
    let total_secs = remaining.whole_seconds();
    if total_secs < 60 {
        return Some(format!("{total_secs} s"));
    }
    let minutes = total_secs / 60;
    if minutes < 60 {
        return Some(format!("~{minutes} min"));
    }
    let hours = minutes / 60;
    let trailing = minutes % 60;
    Some(format!("~{hours}h {trailing}m"))
}

#[cfg(test)]
mod tests {
    use super::{format_remaining_until, friendly_device_label};

    #[test]
    fn friendly_device_label_substitutes_empty() {
        assert_eq!(friendly_device_label(""), "unnamed device");
        assert_eq!(friendly_device_label("   "), "unnamed device");
    }

    #[test]
    fn friendly_device_label_trims_whitespace() {
        assert_eq!(friendly_device_label("  mbp  "), "mbp");
    }

    #[test]
    fn format_remaining_until_returns_none_for_garbage() {
        assert!(format_remaining_until("not-a-date").is_none());
        assert!(format_remaining_until("").is_none());
    }

    #[test]
    fn format_remaining_until_returns_expired_for_past() {
        let past = "2020-01-01T00:00:00Z";
        assert_eq!(format_remaining_until(past).as_deref(), Some("expired"));
    }

    #[test]
    fn format_remaining_until_returns_minutes_for_near_future() {
        let future = (time::OffsetDateTime::now_utc() + time::Duration::minutes(14))
            .format(&time::format_description::well_known::Rfc3339)
            .expect("format");
        let result = format_remaining_until(&future).expect("parseable");
        assert!(
            result.starts_with("~13 min") || result.starts_with("~14 min"),
            "expected ~13 or ~14 min, got {result}"
        );
    }
}
