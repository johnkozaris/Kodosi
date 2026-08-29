use serde::Serialize;

use crate::{Result, runtime::one_shot::OneShotApp};

use super::{
    output::OutputMode,
    status::{
        AuthStatusOutput, AuthStatusState, HostStatusOutput, collect_auth_status,
        collect_host_status, display_name, write_local_only_guidance, write_signed_in_guidance,
    },
};

#[derive(Debug, Serialize)]
struct DoctorOutput {
    backend_api_configured: bool,
    auth_issuer_configured: bool,
    auth: AuthStatusOutput,
    host: HostStatusOutput,
}

pub(in crate::cli) async fn run_doctor(output: OutputMode) -> Result<()> {
    let app = OneShotApp::load()?;
    let backend_api_configured = app.backend_api_configured();
    let auth_issuer_configured = app.auth_issuer_configured();
    drop(app);
    let auth = collect_auth_status().await?;
    let host = collect_host_status().await?;
    let doctor = DoctorOutput {
        backend_api_configured,
        auth_issuer_configured,
        auth,
        host,
    };

    if output.json {
        return output.write_json(&doctor);
    }

    output.write_line(if doctor.backend_api_configured {
        "Backend API: configured"
    } else {
        "Backend API: not configured"
    })?;
    output.write_line(if doctor.auth_issuer_configured {
        "Auth issuer: configured"
    } else {
        "Auth issuer: not configured"
    })?;
    output.write_line(format!("Authentication: {}", doctor.auth.state.as_str()))?;
    if let Some(reason) = &doctor.auth.reason {
        output.write_line(format!("Auth detail: {reason}"))?;
    }
    if let Some(user) = &doctor.auth.user {
        output.write_line(format!("User: {}", display_name(user)))?;
    }
    output.write_line(if doctor.host.running {
        format!(
            "Local host: running ({} active sessions)",
            doctor.host.sessions.len()
        )
    } else {
        "Local host: stopped".to_owned()
    })?;

    if !output.quiet {
        output.write_line("")?;
        if doctor.auth.signed_in {
            write_signed_in_guidance(
                output,
                doctor.auth.host_running,
                &doctor.auth.rooms,
                &doctor.auth.owned_remote_sessions,
            )?;
        } else if doctor.auth.state == AuthStatusState::BackendUnconfigured {
            write_local_only_guidance(output)?;
        } else {
            output.write_line("Sign in: kodosi auth login")?;
            output.write_line("Start a local session: kodosi session start --name <name>")?;
            output.write_line("Inspect local host state: kodosi host status")?;
        }
        if doctor.host.running {
            output.write_line("List local sessions: kodosi session list")?;
        }
    }

    Ok(())
}
