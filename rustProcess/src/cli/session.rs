use std::{
    io::{self, Write},
    path::PathBuf,
    time::Duration,
};

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;

use crate::{AppError, Result, SessionListEntry, headless_host};
use kodosi_backend_client::labels;
use kodosi_domain::permissions::ShareScope;

use super::{
    OWNER_SESSION_URL_PREFIX,
    args::{
        CliSessionAction, CliShareAction, SessionInputArgs, SessionListArgs, SessionModeArgs,
        SessionRenameArgs, SessionRunArgs, SessionShowArgs, SessionStartArgs, SessionStopArgs,
        ShareScopeArg, ShareSetArgs,
    },
    client::ensure_remote_command_access,
    output::OutputMode,
    status::write_session_follow_up_guidance,
    terminal_client::{capture_session_command, run_session_attach, send_session_input},
};

const MAX_TERMINAL_INPUT_BYTES: usize = 8 * 1024 * 1024;

fn owner_console_url(session: &SessionListEntry) -> Option<String> {
    session
        .backend_session_id()
        .map(|backend_id| format!("{OWNER_SESSION_URL_PREFIX}/{backend_id}"))
}

#[derive(Debug, Serialize)]
struct SessionStartOutput {
    host_started: bool,
    session: SessionListEntry,

    #[serde(skip_serializing_if = "Option::is_none")]
    owner_console_url: Option<String>,
}

#[derive(Debug, Serialize)]
struct SessionShowOutput {
    session: SessionListEntry,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner_console_url: Option<String>,
}

#[derive(Debug, Serialize)]
struct SessionInputOutput {
    session_id: String,
    bytes_sent: usize,
}

#[derive(Debug, Serialize)]
struct SessionRunOutput {
    session_id: String,
    command: String,
    output_b64: String,
    bytes_seen: usize,
}

#[expect(
    clippy::future_not_send,
    reason = "the attach variant owns its Ghostty mirror on the CLI current-thread runtime"
)]
pub(in crate::cli) async fn run_session_command(
    command: CliSessionAction,
    output: OutputMode,
) -> Result<()> {
    match command {
        CliSessionAction::Attach(args) => run_session_attach(args).await,
        CliSessionAction::Start(args) => run_session_start(args, output).await,
        CliSessionAction::List(args) => run_session_list(args, output).await,
        CliSessionAction::Show(args) => run_session_show(args, output).await,
        CliSessionAction::Rename(args) => run_session_rename(args, output).await,
        CliSessionAction::Mode(args) => run_session_mode(args, output).await,
        CliSessionAction::Input(args) => run_session_input(args, output).await,
        CliSessionAction::Run(args) => run_session_run(args, output).await,
        CliSessionAction::Interrupt(args) => run_session_interrupt(args, output).await,
        CliSessionAction::Stop(args) => run_session_stop(args, output).await,
        CliSessionAction::Reopen(args) => run_session_reopen(args, output).await,
        CliSessionAction::Delete(args) => run_session_delete(args, output).await,
        CliSessionAction::Leave(args) => run_session_leave(args, output).await,
    }
}

async fn run_session_start(args: SessionStartArgs, output: OutputMode) -> Result<()> {
    if matches!(args.scope, ShareScopeArg::Room) && args.room.is_none() {
        return Err(AppError::Unsupported {
            reason: "session start with --scope room also needs --room <id-or-slug>".to_owned(),
        });
    }
    if !matches!(args.scope, ShareScopeArg::Room) && args.room.is_some() {
        return Err(AppError::Unsupported {
            reason: "session start accepts --room only with --scope room".to_owned(),
        });
    }
    let requested_scope: ShareScope = args.scope.into();
    let mut requested_room = args.room.clone();
    if requested_scope != ShareScope::JustMe {
        let mut app = crate::runtime::one_shot::OneShotApp::load()?;
        ensure_remote_command_access(&mut app, "kodosi session start --scope").await?;
        if requested_scope == ShareScope::Room {
            requested_room = Some(
                crate::runtime::rooms::RoomApplication::new(app.runtime())
                    .resolve_room(args.room.as_deref().unwrap_or_default())
                    .await?
                    .id,
            );
        }
    }

    let working_dir = resolve_working_dir(args.working_dir)?;
    let session_name = match args.name {
        Some(name) if name.trim().is_empty() => {
            return Err(AppError::Unsupported {
                reason: "session name cannot be blank".to_owned(),
            });
        }
        Some(name) => name,
        None => default_session_name(working_dir.as_deref()),
    };
    let (mut host, host_started) = headless_host::ensure_host_running().await?;
    let session = host
        .create_session(session_name, working_dir.clone())
        .await?;
    let session_id = session.id().to_owned();

    let session = if session.scope() != requested_scope
        || (requested_scope == ShareScope::Room && session.room_id() != requested_room.as_deref())
    {
        match host
            .update_session_scope(&session_id, requested_scope, requested_room)
            .await
        {
            Ok(session) => session,
            Err(error) => {
                let rollback = roll_back_started_session(&mut host, &session_id).await;
                return Err(share_failure_error(&session_id, &error, &rollback));
            }
        }
    } else {
        session
    };

    let output_payload = SessionStartOutput {
        host_started,
        owner_console_url: owner_console_url(&session),
        session,
    };

    if output.json {
        return output.write_json(&output_payload);
    }

    if host_started {
        output.write_line("Headless host started.")?;
    }
    output.write_line(format!(
        "Started session {} ({})",
        output_payload.session.name(),
        output_payload.session.id()
    ))?;
    output.write_line(format!("Mode: {:?}", output_payload.session.mode()))?;
    output.write_line(format!("Scope: {:?}", output_payload.session.scope()))?;
    if let Some(url) = &output_payload.owner_console_url {
        output.write_line(format!("Owner console: {url}"))?;
    }
    if !output.quiet {
        write_session_follow_up_guidance(output, &output_payload.session)?;
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum StartRollback {
    Removed,

    Retained { cleanup_error: String },
}

async fn roll_back_started_session(
    host: &mut headless_host::HeadlessHostClient,
    session_id: &str,
) -> StartRollback {
    if let Err(cleanup_error) = host.stop_session(session_id).await {
        return StartRollback::Retained {
            cleanup_error: cleanup_error.to_string(),
        };
    }
    if let Err(cleanup_error) = host.delete_session(session_id).await {
        return StartRollback::Retained {
            cleanup_error: cleanup_error.to_string(),
        };
    }
    StartRollback::Removed
}

fn share_failure_error(
    session_id: &str,
    share_error: &AppError,
    rollback: &StartRollback,
) -> AppError {
    let reason = match rollback {
        StartRollback::Removed => format!(
            "failed to apply share scope: {share_error}; session {session_id} was rolled back and no session is running"
        ),
        StartRollback::Retained { cleanup_error } => format!(
            "failed to apply share scope: {share_error}; session {session_id} is still running unshared (just-me) because rollback failed: {cleanup_error} — stop it with `kodosi session stop {session_id}`"
        ),
    };
    AppError::Unsupported { reason }
}

async fn run_session_list(args: SessionListArgs, output: OutputMode) -> Result<()> {
    if args.owned_remote {
        let mut app = crate::runtime::one_shot::OneShotApp::load()?;
        ensure_remote_command_access(&mut app, "kodosi session list --owned-remote").await?;
        let sessions = app.fetch_my_sessions().await?;
        if output.json {
            return output.write_json(&sessions);
        }
        if sessions.is_empty() {
            output.write_line("No owned remote sessions found.")?;
            return Ok(());
        }
        for session in sessions {
            output.write_line(format!(
                "{}  {} [{} / {}]",
                session.id,
                session.title,
                labels::share_scope_label(session.scope),
                labels::session_state_label(session.status)
            ))?;
            output.write_line(format!("  {OWNER_SESSION_URL_PREFIX}/{}", session.id))?;
        }
        return Ok(());
    }

    let Some(mut host) = headless_host::connect_existing_host().await? else {
        if output.json {
            return output.write_json(&Vec::<SessionListEntry>::new());
        }
        output.write_line("Headless host is not running.")?;
        output.write_line("Start one with `kodosi host start` or `kodosi session start ...`.")?;
        return Ok(());
    };

    let snapshot = host.snapshot().await?;
    let local_sessions = snapshot
        .sessions
        .into_iter()
        .filter(|session| session.is_local() && (args.all || session.is_active_local()))
        .collect::<Vec<_>>();
    if output.json {
        return output.write_json(&local_sessions);
    }
    if local_sessions.is_empty() {
        output.write_line(if args.all {
            "No local sessions found."
        } else {
            "No local sessions are running. Use `session list --all` to include stopped history."
        })?;
        return Ok(());
    }
    for session in local_sessions {
        output.write_line(format!(
            "{}  {} [{:?} / {:?}]",
            session.id(),
            session.name(),
            session.status(),
            session.scope()
        ))?;
    }
    Ok(())
}

async fn run_session_show(args: SessionShowArgs, output: OutputMode) -> Result<()> {
    let Some(mut host) = headless_host::connect_existing_host().await? else {
        return Err(AppError::Unsupported {
            reason: "headless host is not running".to_owned(),
        });
    };

    let snapshot = host.snapshot().await?;
    let session = snapshot
        .sessions
        .into_iter()
        .find(|session| session.id() == args.session_id)
        .ok_or_else(|| AppError::Unsupported {
            reason: format!(
                "session {} was not found on the local host",
                args.session_id
            ),
        })?;
    let output_payload = SessionShowOutput {
        owner_console_url: owner_console_url(&session),
        session,
    };

    if output.json {
        return output.write_json(&output_payload);
    }

    output.write_line(format!(
        "{} ({})",
        output_payload.session.name(),
        output_payload.session.id()
    ))?;
    output.write_line(format!("Status: {:?}", output_payload.session.status()))?;
    output.write_line(format!("Mode: {:?}", output_payload.session.mode()))?;
    output.write_line(format!("Scope: {:?}", output_payload.session.scope()))?;
    output.write_line(format!("Project: {}", output_payload.session.project()))?;
    if let Some(meta) = output_payload.session.meta() {
        output.write_line(format!("Working dir: {}", meta.working_dir))?;
        if let Some(running_command) = &meta.running_command {
            output.write_line(format!("Running command: {running_command}"))?;
        }
    }
    if let Some(room_name) = output_payload.session.room_name() {
        let room_id = output_payload.session.room_id().unwrap_or("<unknown>");
        output.write_line(format!("Room: {room_name} ({room_id})"))?;
    }
    output.write_line(format!(
        "Viewer count: {} active / {} entitled",
        output_payload.session.active_count(),
        output_payload.session.entitled_count(),
    ))?;
    if let Some(url) = &output_payload.owner_console_url {
        output.write_line(format!("Owner console: {url}"))?;
    }
    if !output.quiet {
        output.write_line("")?;
        write_session_follow_up_guidance(output, &output_payload.session)?;
    }
    Ok(())
}

async fn run_session_rename(args: SessionRenameArgs, output: OutputMode) -> Result<()> {
    let mut host = require_headless_host().await?;
    let session = host.rename_session(&args.session_id, args.name).await?;
    write_session_mutation(output, "renamed", &session)
}

async fn run_session_mode(args: SessionModeArgs, output: OutputMode) -> Result<()> {
    let mut host = require_headless_host().await?;
    let session = host
        .update_session_mode(&args.session_id, args.mode.into())
        .await?;
    write_session_mutation(output, "updated", &session)
}

async fn run_session_input(args: SessionInputArgs, output: OutputMode) -> Result<()> {
    let keys = session_input_keys(&args.text, args.enter)?;
    let bytes_sent = keys.len();
    let Some(mut host) = headless_host::connect_existing_host().await? else {
        return Err(AppError::Unsupported {
            reason: "headless host is not running".to_owned(),
        });
    };
    ensure_input_capable_session(&mut host, &args.session_id).await?;

    send_session_input(args.session_id.clone(), keys.into_bytes()).await?;

    let payload = SessionInputOutput {
        session_id: args.session_id,
        bytes_sent,
    };
    if output.json {
        return output.write_json(&payload);
    }
    if !output.quiet {
        output.write_line(format!("Sent {} bytes.", payload.bytes_sent))?;
    }
    Ok(())
}

async fn run_session_run(args: SessionRunArgs, output: OutputMode) -> Result<()> {
    if !(1..=MAX_TERMINAL_INPUT_BYTES).contains(&args.max_bytes) {
        return Err(AppError::Unsupported {
            reason: format!("--max-bytes must be between 1 and {MAX_TERMINAL_INPUT_BYTES}"),
        });
    }
    let Some(mut host) = headless_host::connect_existing_host().await? else {
        return Err(AppError::Unsupported {
            reason: "headless host is not running".to_owned(),
        });
    };
    let running_command = input_capable_session_shell(&mut host, &args.session_id).await?;
    let command = joined_command(&args.command, running_command.as_deref())?;
    let capture = capture_session_command(
        args.session_id.clone(),
        format!("{command}\n").into_bytes(),
        Duration::from_millis(args.timeout_ms),
        args.max_bytes,
    )
    .await?;

    if output.json {
        return output.write_json(&SessionRunOutput {
            session_id: args.session_id,
            command,
            output_b64: STANDARD.encode(&capture.bytes),
            bytes_seen: capture.total_bytes_seen,
        });
    }

    if !output.quiet {
        let mut stdout = io::stdout().lock();
        stdout.write_all(&capture.bytes).map_err(AppError::Io)?;
        stdout.flush().map_err(AppError::Io)?;
    }
    Ok(())
}

async fn run_session_stop(args: SessionStopArgs, output: OutputMode) -> Result<()> {
    let Some(mut host) = headless_host::connect_existing_host().await? else {
        return Err(AppError::Unsupported {
            reason: "headless host is not running".to_owned(),
        });
    };

    host.stop_session(&args.session_id).await?;

    if output.json {
        return output.write_json(&serde_json::json!({
            "stopped": true,
            "session_id": args.session_id,
        }));
    }
    output.write_line(format!("Stopped session {}.", args.session_id))?;
    Ok(())
}

async fn run_session_interrupt(args: SessionStopArgs, output: OutputMode) -> Result<()> {
    let mut host = require_headless_host().await?;
    host.interrupt_session(&args.session_id).await?;
    if output.json {
        return output.write_json(&serde_json::json!({
            "interrupted": true,
            "session_id": args.session_id,
        }));
    }
    output.write_line(format!("Interrupted session {}.", args.session_id))
}

async fn run_session_reopen(args: SessionStopArgs, output: OutputMode) -> Result<()> {
    let (mut host, _) = headless_host::ensure_host_running().await?;
    let session = host.reopen_session(&args.session_id).await?;
    write_session_mutation(output, "reopened", &session)
}

async fn run_session_delete(args: SessionStopArgs, output: OutputMode) -> Result<()> {
    let (mut host, _) = headless_host::ensure_host_running().await?;
    host.delete_session(&args.session_id).await?;
    if output.json {
        return output.write_json(&serde_json::json!({
            "deleted": true,
            "session_id": args.session_id,
        }));
    }
    output.write_line(format!("Deleted session {}.", args.session_id))
}

async fn run_session_leave(args: SessionStopArgs, output: OutputMode) -> Result<()> {
    let mut host = require_headless_host().await?;
    host.leave_session(&args.session_id).await?;
    if output.json {
        return output.write_json(&serde_json::json!({
            "left": true,
            "session_id": args.session_id,
        }));
    }
    output.write_line(format!(
        "Left shared session {}. The owner’s session was not stopped or deleted.",
        args.session_id
    ))
}

async fn require_headless_host() -> Result<headless_host::HeadlessHostClient> {
    headless_host::connect_existing_host()
        .await?
        .ok_or_else(|| AppError::Unsupported {
            reason: "headless host is not running".to_owned(),
        })
}

fn write_session_mutation(
    output: OutputMode,
    operation: &str,
    session: &SessionListEntry,
) -> Result<()> {
    if output.json {
        return output.write_json(&serde_json::json!({
            "operation": operation,
            "session": session,
        }));
    }
    output.write_line(format!(
        "{} session {} ({}).",
        operation.to_uppercase(),
        session.name(),
        session.id()
    ))
}

async fn ensure_input_capable_session(
    host: &mut headless_host::HeadlessHostClient,
    session_id: &str,
) -> Result<()> {
    input_capable_session_shell(host, session_id)
        .await
        .map(drop)
}

async fn input_capable_session_shell(
    host: &mut headless_host::HeadlessHostClient,
    session_id: &str,
) -> Result<Option<String>> {
    let snapshot = host.snapshot().await?;
    let Some(session) = snapshot
        .sessions
        .iter()
        .find(|session| session.id() == session_id)
    else {
        return Err(AppError::Unsupported {
            reason: format!("session {session_id} was not found on the local host"),
        });
    };

    if session.is_active_local() {
        return Ok(session
            .meta()
            .and_then(|meta| meta.running_command.as_ref())
            .cloned());
    }

    Err(AppError::Unsupported {
        reason: format!(
            "session {session_id} is not an active local session and cannot receive CLI input"
        ),
    })
}

pub(in crate::cli) async fn run_share_command(
    command: CliShareAction,
    output: OutputMode,
) -> Result<()> {
    match command {
        CliShareAction::Set(args) => run_share_set(args, output).await,
        CliShareAction::Off(args) => {
            run_share_scope_update(
                args.session_id,
                ShareScope::JustMe,
                None,
                output,
                "Sharing disabled.",
            )
            .await
        }
    }
}

async fn run_share_set(args: ShareSetArgs, output: OutputMode) -> Result<()> {
    if matches!(args.scope, ShareScopeArg::Room) && args.room.is_none() {
        return Err(AppError::Unsupported {
            reason: "share set with --scope room also needs --room <id-or-slug>".to_owned(),
        });
    }
    if !matches!(args.scope, ShareScopeArg::Room) && args.room.is_some() {
        return Err(AppError::Unsupported {
            reason: "share set accepts --room only with --scope room".to_owned(),
        });
    }

    let requested_scope: ShareScope = args.scope.into();
    run_share_scope_update(
        args.session_id,
        requested_scope,
        args.room,
        output,
        "Updated share scope.",
    )
    .await
}

async fn run_share_scope_update(
    session_id: String,
    requested_scope: ShareScope,
    room_id: Option<String>,
    output: OutputMode,
    success_line: &str,
) -> Result<()> {
    let Some(mut host) = headless_host::connect_existing_host().await? else {
        return Err(AppError::Unsupported {
            reason: "headless host is not running".to_owned(),
        });
    };

    let session = host
        .update_session_scope(&session_id, requested_scope, room_id)
        .await?;

    if output.json {
        return output.write_json(&session);
    }
    output.write_line(success_line)?;
    output.write_line(format!("Session: {} ({})", session.name(), session.id()))?;
    if let Some(url) = owner_console_url(&session) {
        output.write_line(format!("Owner console: {url}"))?;
    }
    if !output.quiet {
        output.write_line("")?;
        write_session_follow_up_guidance(output, &session)?;
    }
    Ok(())
}

fn resolve_working_dir(working_dir: Option<PathBuf>) -> Result<Option<String>> {
    let directory = match working_dir {
        Some(directory) => directory,
        None => std::env::current_dir().map_err(AppError::Io)?,
    };
    let metadata = std::fs::metadata(&directory).map_err(|error| AppError::Unsupported {
        reason: format!(
            "working directory {} is not accessible: {error}",
            directory.display()
        ),
    })?;
    if !metadata.is_dir() {
        return Err(AppError::Unsupported {
            reason: format!(
                "working directory {} is not a directory",
                directory.display()
            ),
        });
    }
    let path = directory
        .into_os_string()
        .into_string()
        .map_err(|_| AppError::Unsupported {
            reason: "working directory path is not valid Unicode".to_owned(),
        })?;
    Ok(Some(path))
}

fn default_session_name(working_dir: Option<&str>) -> String {
    working_dir
        .and_then(|dir| {
            PathBuf::from(dir)
                .file_name()
                .map(std::ffi::OsStr::to_os_string)
        })
        .and_then(|value| value.into_string().ok())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Kodosi Session".to_owned())
}

fn joined_command(args: &[String], _running_command: Option<&str>) -> Result<String> {
    let command = if let [raw_command] = args {
        raw_command.clone()
    } else {
        args.iter()
            .map(|argument| posix_shell_quote_argument(argument))
            .collect::<Vec<_>>()
            .join(" ")
    };
    if command.trim().is_empty() {
        return Err(AppError::Unsupported {
            reason: "command cannot be blank".to_owned(),
        });
    }
    Ok(command)
}

fn posix_shell_quote_argument(argument: &str) -> String {
    if is_shell_safe_argument(argument) {
        return argument.to_owned();
    }

    let mut quoted = String::with_capacity(argument.len() + 2);
    quoted.push('\'');
    for character in argument.chars() {
        if character == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(character);
        }
    }
    quoted.push('\'');
    quoted
}

fn is_shell_safe_argument(argument: &str) -> bool {
    !argument.is_empty()
        && argument.bytes().all(|byte| {
            matches!(
                byte,
                b'a'..=b'z'
                    | b'A'..=b'Z'
                    | b'0'..=b'9'
                    | b'_'
                    | b'-'
                    | b'.'
                    | b'/'
                    | b':'
                    | b'='
                    | b'+'
                    | b','
                    | b'%'
            )
        })
}

fn session_input_keys(text: &str, enter: bool) -> Result<String> {
    let mut keys = text.to_owned();
    if enter {
        keys.push('\n');
    }
    if keys.is_empty() {
        return Err(AppError::Unsupported {
            reason: "input cannot be empty unless --enter is set".to_owned(),
        });
    }
    if keys.len() > MAX_TERMINAL_INPUT_BYTES {
        return Err(AppError::Unsupported {
            reason: format!("input exceeds the {MAX_TERMINAL_INPUT_BYTES}-byte terminal limit"),
        });
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::{
        StartRollback, default_session_name, joined_command, session_input_keys,
        share_failure_error,
    };
    use crate::AppError;

    fn share_error() -> AppError {
        AppError::Unsupported {
            reason: "backend rejected request (400, code=DOMAIN_ERROR): Default access cannot be Inject.".to_owned(),
        }
    }

    fn reason_of(error: &AppError) -> String {
        match error {
            AppError::Unsupported { reason } => reason.clone(),
            other => panic!("expected an unsupported-operation error, got {other:?}"),
        }
    }

    #[test]
    fn rolled_back_start_reports_that_no_session_survives() {
        let error = share_failure_error("session-1", &share_error(), &StartRollback::Removed);
        let reason = reason_of(&error);
        assert!(
            reason.contains("Default access cannot be Inject."),
            "{reason}"
        );
        assert!(reason.contains("rolled back"), "{reason}");
        assert!(reason.contains("no session is running"), "{reason}");
        assert!(!reason.contains("still running"), "{reason}");
    }

    #[test]
    fn retained_start_names_the_session_the_user_must_clean_up() {
        let error = share_failure_error(
            "session-2",
            &share_error(),
            &StartRollback::Retained {
                cleanup_error: "host is gone".to_owned(),
            },
        );
        let reason = reason_of(&error);
        assert!(
            reason.contains("Default access cannot be Inject."),
            "{reason}"
        );
        assert!(
            reason.contains("still running unshared (just-me)"),
            "{reason}"
        );
        assert!(reason.contains("host is gone"), "{reason}");
        assert!(
            reason.contains("kodosi session stop session-2"),
            "the user needs the exact recovery command: {reason}"
        );
    }

    #[test]
    fn default_session_name_uses_directory_name() {
        assert_eq!(default_session_name(Some("/tmp/kodosi")), "kodosi");
    }

    #[test]
    fn default_session_name_falls_back_when_missing() {
        assert_eq!(default_session_name(None), "Kodosi Session");
    }

    #[test]
    fn joined_command_preserves_user_arguments() {
        let args = vec!["claude".to_owned(), "--version".to_owned()];
        assert_eq!(
            joined_command(&args, Some("/opt/homebrew/bin/fish")).unwrap(),
            "claude --version"
        );
    }

    #[test]
    fn joined_command_quotes_arguments_with_shell_syntax() {
        let args = vec![
            "python".to_owned(),
            "-c".to_owned(),
            "print(1, 2)".to_owned(),
        ];
        assert_eq!(
            joined_command(&args, Some("/bin/zsh")).unwrap(),
            "python -c 'print(1, 2)'"
        );
    }

    #[test]
    fn joined_command_quotes_embedded_single_quotes() {
        let args = vec!["printf".to_owned(), "a'b".to_owned()];
        assert_eq!(
            joined_command(&args, Some("fish")).unwrap(),
            "printf 'a'\\''b'"
        );
    }

    #[test]
    fn joined_command_keeps_single_argument_raw() {
        let args = vec!["printf hi | wc -c".to_owned()];
        assert_eq!(
            joined_command(&args, Some("/bin/zsh")).unwrap(),
            "printf hi | wc -c"
        );
    }

    #[test]
    fn joined_command_rejects_blank_input() {
        let args = vec![" ".to_owned()];
        assert!(joined_command(&args, Some("/bin/sh")).is_err());
    }

    #[test]
    fn session_input_keys_appends_enter_when_requested() {
        assert_eq!(session_input_keys("echo hi", true).unwrap(), "echo hi\n");
        assert_eq!(session_input_keys("", true).unwrap(), "\n");
    }

    #[test]
    fn session_input_keys_rejects_empty_without_enter() {
        assert!(session_input_keys("", false).is_err());
    }
}
