use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use kodosi_domain::{permissions::ShareScope, session::SessionMode};

#[derive(Debug, Parser)]
#[command(
    name = "kodosi",
    version,
    about = "Control local coding-agent sessions and Kodosi collaboration from the terminal"
)]
pub(in crate::cli) struct Cli {
    #[arg(help = "Layer a TOML file over the embedded defaults before environment overrides.")]
    #[arg(long, global = true, value_name = "PATH")]
    pub(in crate::cli) config: Option<PathBuf>,
    #[arg(help = "Emit machine-readable JSON for data and errors.")]
    #[arg(long, global = true)]
    pub(in crate::cli) json: bool,
    #[arg(help = "Suppress human-readable success and progress output.")]
    #[arg(long, global = true)]
    pub(in crate::cli) quiet: bool,
    #[command(subcommand)]
    pub(in crate::cli) command: CliCommand,
}

#[derive(Debug, Subcommand)]
pub(in crate::cli) enum CliCommand {
    #[command(about = "Sign in, sign out, or inspect account state.")]
    #[command(subcommand)]
    Auth(CliAuthAction),
    #[command(about = "Manage devices enrolled to this account.")]
    #[command(subcommand)]
    Device(CliDeviceAction),
    #[command(about = "Diagnose local configuration, credentials, and host readiness.")]
    Doctor,
    #[command(about = "Resolve durable local operator-attention conditions.")]
    #[command(subcommand)]
    Repair(CliRepairAction),
    #[command(about = "Manage the persistent local runtime used by terminal clients.")]
    #[command(subcommand)]
    Host(CliHostAction),
    #[command(about = "Create and operate local coding-agent sessions.")]
    #[command(subcommand)]
    Session(CliSessionAction),
    #[command(about = "Change who can discover and supervise a session.")]
    #[command(subcommand)]
    Share(CliShareAction),
    #[command(about = "Inspect and reset pinned device-list trust anchors for other users.")]
    #[command(subcommand)]
    Trust(CliTrustAction),
    #[command(about = "Collaborate through encrypted room chat and mission tasks.")]
    #[command(subcommand)]
    Room(CliRoomAction),
    #[command(about = "Agent-room self-description and peer roster.")]
    #[command(subcommand)]
    Agent(CliAgentAction),
    #[command(about = "Send a DM to a peer agent / drain inbox.")]
    #[command(subcommand)]
    Msg(CliMsgAction),
    #[command(about = "List reachable agents in the room.")]
    Agents(CliAgentsArgs),
    #[command(name = "__internal-host-serve", hide = true)]
    InternalHostServe,
    #[command(name = "__internal-room-channel-serve", hide = true)]
    InternalRoomChannelServe,
}

#[derive(Debug, Subcommand)]
pub(in crate::cli) enum CliRepairAction {
    #[command(
        about = "Preserve an unavailable share-transition ledger and install a fresh empty ledger."
    )]
    ResetShareTransitions,
    #[command(
        about = "Accept an unreadable cleanup store as unrecoverable and preserve its evidence."
    )]
    ResetCollaborationCleanup,
    #[command(
        about = "Remove exact legacy Kodosi hooks and their helper without touching foreign settings."
    )]
    CleanupLegacyHooks {
        #[arg(help = "Report the planned cleanup without changing files.")]
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Debug, Subcommand)]
pub(in crate::cli) enum CliAgentAction {
    #[command(about = "Publish a one-line self-description so peers know what you're doing.")]
    Describe(CliAgentDescribeArgs),
}

#[derive(Debug, Args)]
pub(in crate::cli) struct CliAgentDescribeArgs {
    #[arg(help = "One-liner: what this agent is currently working on.")]
    pub(in crate::cli) description: String,
}

#[derive(Debug, Args)]
pub(in crate::cli) struct CliAgentsArgs {
    #[arg(help = "Filter to a room id or slug.")]
    #[arg(long)]
    pub(in crate::cli) room: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(in crate::cli) enum CliMsgAction {
    #[command(about = "DM another agent session by id.")]
    Send(CliMsgSendArgs),
    #[command(about = "Drain unread inbox entries.")]
    Inbox(CliMsgInboxArgs),
    #[command(about = "Commit a cursor returned by `msg inbox --peek`.")]
    Ack(CliMsgAckArgs),
}

#[derive(Debug, Args)]
pub(in crate::cli) struct CliMsgSendArgs {
    pub(in crate::cli) target_session_id: String,
    pub(in crate::cli) body: String,
    #[arg(long)]
    pub(in crate::cli) in_reply_to: Option<String>,
}

#[derive(Debug, Args)]
pub(in crate::cli) struct CliMsgInboxArgs {
    #[arg(help = "Only entries with cursor greater than this value.")]
    #[arg(long)]
    pub(in crate::cli) since: Option<u64>,
    #[arg(help = "Return entries without advancing the durable cursor.")]
    #[arg(long)]
    pub(in crate::cli) peek: bool,
    #[arg(help = "Maximum entries to return.")]
    #[arg(long, default_value_t = 100)]
    pub(in crate::cli) limit: usize,
}

#[derive(Debug, Args)]
pub(in crate::cli) struct CliMsgAckArgs {
    #[arg(help = "Byte cursor returned by `msg inbox --peek`.")]
    pub(in crate::cli) cursor: u64,
}

#[derive(Debug, Subcommand)]
pub(in crate::cli) enum CliDeviceAction {
    #[command(about = "Show the devices enrolled to the authenticated user's identity.")]
    List,
    #[command(about = "Revoke an enrolled device and publish a replacement signed device list.")]
    Revoke(DeviceRevokeArgs),
    #[command(about = "Enroll this machine into the authenticated user's device list.")]
    Link(DeviceLinkArgs),
    #[command(about = "Approve a pending link request initiated from another machine.")]
    ApproveLink(DeviceApproveLinkArgs),
}

#[derive(Debug, Args)]
pub(in crate::cli) struct DeviceRevokeArgs {
    #[arg(help = "Device id to revoke. Use `kodosi device list` to find it.")]
    #[arg(value_name = "DEVICE_ID")]
    pub(in crate::cli) device_id: String,
}

#[derive(Debug, Args)]
pub(in crate::cli) struct DeviceApproveLinkArgs {
    #[arg(help = "User code shown on the new device (for example `ABCD-EFGH`).")]
    #[arg(value_name = "USER_CODE")]
    pub(in crate::cli) user_code: String,
}

#[derive(Debug, Args)]
pub(in crate::cli) struct DeviceLinkArgs {
    #[arg(help = "Label shown to the approver so they can identify this machine.")]
    #[arg(long, value_name = "LABEL")]
    pub(in crate::cli) label: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(in crate::cli) enum CliTrustAction {
    #[command(about = "Show locally cached device-list identity pins.")]
    List,
    #[command(about = "Clear one user's pin; the next explicit contact establishes a new pin.")]
    Reset(TrustResetArgs),
}

#[derive(Debug, Args)]
pub(in crate::cli) struct TrustResetArgs {
    #[arg(help = "The user whose pin should be cleared.")]
    #[arg(long, value_name = "USER_ID")]
    pub(in crate::cli) user: String,
}

#[derive(Debug, Subcommand)]
pub(in crate::cli) enum CliAuthAction {
    #[command(about = "Authenticate this device through the browser/device-code flow.")]
    Login(AuthLoginArgs),
    #[command(about = "Clear durable credentials and disconnect authenticated runtime state.")]
    Logout,
    #[command(about = "Show the current account and backend connection state.")]
    Status,
}

#[derive(Debug, Args)]
pub(in crate::cli) struct AuthLoginArgs {
    #[arg(help = "Print the pending login code and return; poll completion with `auth status`.")]
    #[arg(long)]
    pub(in crate::cli) no_wait: bool,
}

#[derive(Debug, Subcommand)]
pub(in crate::cli) enum CliHostAction {
    #[command(about = "Start the local runtime if it is not already running.")]
    Start,
    #[command(about = "Show the local runtime process and socket state.")]
    Status,
    #[command(about = "Gracefully stop the local runtime.")]
    Stop,
}

#[derive(Debug, Subcommand)]
pub(in crate::cli) enum CliRoomAction {
    #[command(about = "List the rooms you belong to.")]
    List,
    #[command(about = "Read or post room conversation.")]
    #[command(subcommand)]
    Chat(CliRoomChatAction),
    #[command(about = "Manage the room mission queue.")]
    #[command(subcommand)]
    Tasks(CliRoomTaskAction),
}

#[derive(Debug, Subcommand)]
pub(in crate::cli) enum CliRoomChatAction {
    #[command(about = "Read decrypted room messages in sequence order.")]
    List(CliRoomChatListArgs),
    #[command(about = "Post a message. Agent sessions are attributed through `KODOSI_SESSION_ID`.")]
    Post(CliRoomChatPostArgs),
    #[command(about = "List valid typed chat targets in the room.")]
    Peers(CliRoomChatPeersArgs),
}

#[derive(Debug, Subcommand)]
pub(in crate::cli) enum CliRoomTaskAction {
    #[command(about = "List room tasks, optionally filtered by status or assignee.")]
    List(CliRoomTaskListArgs),
    #[command(about = "List tasks assigned to the current `KODOSI_SESSION_ID`.")]
    Mine(CliRoomTasksArgs),
    #[command(about = "Create a task in the room mission queue.")]
    Create(CliRoomTaskCreateArgs),
    #[command(about = "Assign a room task to a live session.")]
    Assign(CliRoomTaskAssignArgs),
    #[command(about = "Remove a task's session assignment.")]
    Unassign(CliRoomTaskTransitionArgs),
    #[command(about = "Claim an assigned task (`Open` → `InProgress`) as the current session.")]
    Claim(CliRoomTaskTransitionArgs),
    #[command(about = "Submit a task for review (`InProgress` → `Review`).")]
    Submit(CliRoomTaskTransitionArgs),
    #[command(about = "Mark a task done with a result note (`InProgress`|`Review` → `Done`).")]
    Done(CliRoomTaskDoneArgs),
    #[command(about = "Archive a completed task.")]
    Archive(CliRoomTaskTransitionArgs),
    #[command(about = "Reopen a task as `Open` (room-owner override).")]
    Reopen(CliRoomTaskTransitionArgs),
}

#[derive(Debug, clap::Args)]
pub(in crate::cli) struct CliRoomChatListArgs {
    #[arg(help = "Room id (uuid) or slug.")]
    pub(in crate::cli) room: String,
    #[arg(help = "Return only messages after this room sequence.")]
    #[arg(long)]
    pub(in crate::cli) since: Option<i64>,
    #[arg(help = "Maximum messages to return.")]
    #[arg(long, default_value_t = 100, value_parser = clap::value_parser!(i32).range(1..=500))]
    pub(in crate::cli) limit: i32,
}

#[derive(Debug, clap::Args)]
pub(in crate::cli) struct CliRoomChatPostArgs {
    #[arg(help = "Room id (uuid) or slug.")]
    pub(in crate::cli) room: String,
    #[arg(help = "Message body.")]
    #[arg(allow_hyphen_values = true)]
    pub(in crate::cli) body: String,
    #[arg(help = "Direct the message to an attached agent session. Repeat for multiple sessions.")]
    #[arg(long = "to-session")]
    pub(in crate::cli) recipient_session_ids: Vec<String>,
    #[arg(help = "Direct the message to a room member. Repeat for multiple people.")]
    #[arg(long = "to-user")]
    pub(in crate::cli) recipient_user_ids: Vec<String>,
    #[arg(help = "Stable mutation identity supplied by a session-scoped delivery adapter.")]
    #[arg(long, hide = true)]
    pub(in crate::cli) request_id: Option<String>,
}

#[derive(Debug, clap::Args)]
pub(in crate::cli) struct CliRoomChatPeersArgs {
    #[arg(help = "Room id (uuid) or slug.")]
    pub(in crate::cli) room: String,
}

#[derive(Debug, clap::Args)]
pub(in crate::cli) struct CliRoomTasksArgs {
    #[arg(help = "Room id (uuid) or slug.")]
    pub(in crate::cli) room: String,
}

#[derive(Debug, clap::Args)]
pub(in crate::cli) struct CliRoomTaskListArgs {
    #[arg(help = "Room id (uuid) or slug.")]
    pub(in crate::cli) room: String,
    #[arg(help = "Filter by `Open`, `InProgress`, `Review`, `Done`, or `Archived`.")]
    #[arg(long)]
    pub(in crate::cli) status: Option<String>,
    #[arg(help = "Filter by assigned session id.")]
    #[arg(long)]
    pub(in crate::cli) assignee: Option<String>,
}

#[derive(Debug, clap::Args)]
pub(in crate::cli) struct CliRoomTaskCreateArgs {
    #[arg(help = "Room id (uuid) or slug.")]
    pub(in crate::cli) room: String,
    #[arg(help = "Task title.")]
    pub(in crate::cli) title: String,
    #[arg(long)]
    pub(in crate::cli) description: Option<String>,
    #[arg(help = "Assign immediately to this live session id.")]
    #[arg(long)]
    pub(in crate::cli) session: Option<String>,
    #[arg(help = "Optional RFC3339 due time.")]
    #[arg(long)]
    pub(in crate::cli) due_at: Option<String>,
}

#[derive(Debug, clap::Args)]
pub(in crate::cli) struct CliRoomTaskAssignArgs {
    #[arg(help = "Room id (uuid) or slug.")]
    pub(in crate::cli) room: String,
    #[arg(help = "Task id (uuid).")]
    pub(in crate::cli) task_id: String,
    #[arg(help = "Live room session id.")]
    pub(in crate::cli) session_id: String,
}

#[derive(Debug, clap::Args)]
pub(in crate::cli) struct CliRoomTaskTransitionArgs {
    #[arg(help = "Room id (uuid) or slug.")]
    pub(in crate::cli) room: String,
    #[arg(help = "Task id (uuid).")]
    pub(in crate::cli) task_id: String,
    #[arg(help = "Exact task revision being acted on.")]
    #[arg(long, value_parser = clap::value_parser!(i64).range(0..))]
    pub(in crate::cli) expected_revision: i64,
    #[arg(help = "Stable mutation identity supplied by a session-scoped delivery adapter.")]
    #[arg(long, hide = true)]
    pub(in crate::cli) request_id: Option<String>,
}

#[derive(Debug, clap::Args)]
pub(in crate::cli) struct CliRoomTaskDoneArgs {
    #[arg(help = "Room id (uuid) or slug.")]
    pub(in crate::cli) room: String,
    #[arg(help = "Task id (uuid).")]
    pub(in crate::cli) task_id: String,
    #[arg(help = "Short result summary.")]
    #[arg(allow_hyphen_values = true)]
    pub(in crate::cli) result: String,
    #[arg(help = "Exact task revision being acted on.")]
    #[arg(long, value_parser = clap::value_parser!(i64).range(0..))]
    pub(in crate::cli) expected_revision: i64,
    #[arg(help = "Stable mutation identity supplied by a session-scoped delivery adapter.")]
    #[arg(long, hide = true)]
    pub(in crate::cli) request_id: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(in crate::cli) enum CliSessionAction {
    #[command(about = "Attach this terminal to a local session. Detach with Ctrl-o d or Ctrl-b d.")]
    Attach(SessionAttachArgs),
    #[command(about = "Start a real local shell session, launching the host when needed.")]
    Start(SessionStartArgs),
    #[command(about = "List active local sessions, stopped history, or owned backend sessions.")]
    List(SessionListArgs),
    #[command(about = "Show lifecycle, project, sharing, and viewer details for one session.")]
    Show(SessionShowArgs),
    #[command(about = "Rename a local or owned session.")]
    Rename(SessionRenameArgs),
    #[command(about = "Change the session's supervision mode.")]
    Mode(SessionModeArgs),
    #[command(about = "Send exact text to an active local session PTY.")]
    Input(SessionInputArgs),
    #[command(
        about = "Run a command in an active local session and capture a short output window."
    )]
    Run(SessionRunArgs),
    #[command(about = "Interrupt the foreground process (equivalent to Ctrl-C).")]
    Interrupt(SessionStopArgs),
    #[command(about = "Stop the terminal process while preserving resumable session state.")]
    Stop(SessionStopArgs),
    #[command(about = "Start a fresh terminal process for a stopped local session.")]
    Reopen(SessionStopArgs),
    #[command(about = "Delete a stopped session's resumable local state.")]
    Delete(SessionStopArgs),
    #[command(about = "Permanently withdraw this account from a session shared by someone else.")]
    Leave(SessionStopArgs),
}

#[derive(Debug, Subcommand)]
pub(in crate::cli) enum CliShareAction {
    #[command(about = "Set a session's audience and optional room.")]
    Set(ShareSetArgs),
    #[command(about = "Return a session to owner-only access.")]
    Off(ShareOffArgs),
}

#[derive(Debug, Args)]
pub(in crate::cli) struct SessionAttachArgs {
    pub(in crate::cli) session_id: String,
}

#[derive(Debug, Args)]
pub(in crate::cli) struct SessionStartArgs {
    #[arg(long)]
    pub(in crate::cli) name: Option<String>,
    #[arg(long, value_name = "DIR")]
    pub(in crate::cli) working_dir: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = ShareScopeArg::JustMe)]
    pub(in crate::cli) scope: ShareScopeArg,
    #[arg(help = "Room id (uuid) or slug — required when `--scope=room`.")]
    #[arg(long)]
    pub(in crate::cli) room: Option<String>,
}

#[derive(Debug, Args)]
pub(in crate::cli) struct SessionListArgs {
    #[arg(help = "Include stopped sessions that can be reopened or deleted.")]
    #[arg(long, conflicts_with = "owned_remote")]
    pub(in crate::cli) all: bool,
    #[arg(help = "List backend sessions owned by this user instead of local host sessions.")]
    #[arg(long)]
    pub(in crate::cli) owned_remote: bool,
}

#[derive(Debug, Args)]
pub(in crate::cli) struct SessionStopArgs {
    pub(in crate::cli) session_id: String,
}

#[derive(Debug, Args)]
pub(in crate::cli) struct SessionInputArgs {
    pub(in crate::cli) session_id: String,
    #[arg(value_name = "TEXT")]
    pub(in crate::cli) text: String,
    #[arg(help = "Append Enter after TEXT. Use `session run` for command-style input.")]
    #[arg(long)]
    pub(in crate::cli) enter: bool,
}

#[derive(Debug, Args)]
pub(in crate::cli) struct SessionRunArgs {
    pub(in crate::cli) session_id: String,
    #[arg(help = "Milliseconds to collect terminal output after sending the command.")]
    #[arg(
        long,
        default_value_t = 2_000,
        value_parser = clap::value_parser!(u64).range(100..=300_000)
    )]
    pub(in crate::cli) timeout_ms: u64,
    #[arg(help = "Maximum decoded terminal bytes to return.")]
    #[arg(long, default_value_t = 65_536)]
    pub(in crate::cli) max_bytes: usize,
    #[arg(help = "Command to send. Multiple arguments are shell-quoted before injection.")]
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    pub(in crate::cli) command: Vec<String>,
}

#[derive(Debug, Args)]
pub(in crate::cli) struct SessionShowArgs {
    pub(in crate::cli) session_id: String,
}

#[derive(Debug, Args)]
pub(in crate::cli) struct SessionRenameArgs {
    pub(in crate::cli) session_id: String,
    pub(in crate::cli) name: String,
}

#[derive(Debug, Args)]
pub(in crate::cli) struct SessionModeArgs {
    pub(in crate::cli) session_id: String,
    #[arg(help = "Mode changes require the runtime to have detected a supported agent.")]
    #[arg(value_enum)]
    pub(in crate::cli) mode: SessionModeArg,
}

#[derive(Debug, Args)]
pub(in crate::cli) struct ShareSetArgs {
    pub(in crate::cli) session_id: String,
    #[arg(long, value_enum)]
    pub(in crate::cli) scope: ShareScopeArg,
    #[arg(help = "Room id (uuid) or slug — required when `--scope=room`.")]
    #[arg(long)]
    pub(in crate::cli) room: Option<String>,
}

#[derive(Debug, Args)]
pub(in crate::cli) struct ShareOffArgs {
    pub(in crate::cli) session_id: String,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(in crate::cli) enum SessionModeArg {
    Normal,
    Plan,
    Autopilot,
}

impl From<SessionModeArg> for SessionMode {
    fn from(value: SessionModeArg) -> Self {
        match value {
            SessionModeArg::Normal => Self::Normal,
            SessionModeArg::Plan => Self::Plan,
            SessionModeArg::Autopilot => Self::Autopilot,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(in crate::cli) enum ShareScopeArg {
    JustMe,
    MyDevices,
    Room,
}

impl From<ShareScopeArg> for ShareScope {
    fn from(value: ShareScopeArg) -> Self {
        match value {
            ShareScopeArg::JustMe => Self::JustMe,
            ShareScopeArg::MyDevices => Self::MyDevices,
            ShareScopeArg::Room => Self::Room,
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser};

    use super::{
        Cli, CliAuthAction, CliCommand, CliRepairAction, CliRoomAction, CliRoomChatAction,
        CliRoomTaskAction, CliSessionAction,
    };

    #[test]
    fn config_path_is_global_and_preserved() {
        let cli = Cli::try_parse_from([
            "kodosi",
            "room",
            "--config",
            "/tmp/kodosi.toml",
            "chat",
            "list",
            "my-room",
        ])
        .expect("global config path should parse after a parent command");

        assert_eq!(
            cli.config.as_deref(),
            Some(std::path::Path::new("/tmp/kodosi.toml"))
        );
    }

    #[test]
    fn share_transition_repair_command_uses_explicit_canonical_spelling() {
        let cli = Cli::try_parse_from(["kodosi", "repair", "reset-share-transitions"])
            .expect("share transition repair command should parse");
        std::assert_matches!(
            cli.command,
            CliCommand::Repair(CliRepairAction::ResetShareTransitions)
        );
    }

    #[test]
    fn room_chat_and_task_commands_are_nested_by_product_concept() {
        let chat =
            Cli::try_parse_from(["kodosi", "room", "chat", "list", "my-room", "--since", "42"])
                .expect("room chat list should parse");
        std::assert_matches!(
            chat.command,
            CliCommand::Room(CliRoomAction::Chat(CliRoomChatAction::List(_)))
        );

        let tasks = Cli::try_parse_from(["kodosi", "room", "tasks", "mine", "my-room"])
            .expect("room tasks mine should parse");
        std::assert_matches!(
            tasks.command,
            CliCommand::Room(CliRoomAction::Tasks(CliRoomTaskAction::Mine(_)))
        );
    }

    #[test]
    fn room_task_transitions_require_a_nonnegative_expected_revision() {
        let claim = Cli::try_parse_from([
            "kodosi",
            "room",
            "tasks",
            "claim",
            "my-room",
            "task-1",
            "--expected-revision",
            "17",
            "--request-id",
            "01900000-0000-7000-8000-000000000004",
        ])
        .expect("task transition with an expected revision should parse");
        std::assert_matches!(
            claim.command,
            CliCommand::Room(CliRoomAction::Tasks(CliRoomTaskAction::Claim(args)))
                if args.expected_revision == 17
                    && args.request_id.as_deref()
                        == Some("01900000-0000-7000-8000-000000000004")
        );
        assert!(
            Cli::try_parse_from(["kodosi", "room", "tasks", "claim", "my-room", "task-1",])
                .is_err()
        );
        let done = Cli::try_parse_from([
            "kodosi",
            "room",
            "tasks",
            "done",
            "my-room",
            "task-1",
            "- fixed the bug",
            "--expected-revision",
            "18",
        ])
        .expect("hyphen-leading task results should parse");
        std::assert_matches!(
            done.command,
            CliCommand::Room(CliRoomAction::Tasks(CliRoomTaskAction::Done(args)))
                if args.result == "- fixed the bug"
        );
        assert!(
            Cli::try_parse_from([
                "kodosi",
                "room",
                "tasks",
                "done",
                "my-room",
                "task-1",
                "finished",
                "--expected-revision",
                "-1",
            ])
            .is_err()
        );
    }

    #[test]
    fn legacy_flat_room_commands_are_rejected() {
        for args in [
            &["kodosi", "room", "chat-post", "my-room", "hello"][..],
            &["kodosi", "room", "tasks-mine", "my-room"],
            &["kodosi", "room", "tasks-claim", "my-room", "task-1"],
            &["kodosi", "room", "tasks-done", "my-room", "task-1", "done"],
            &["kodosi", "room", "tasks-submit", "my-room", "task-1"],
        ] {
            assert!(
                Cli::try_parse_from(args).is_err(),
                "legacy command unexpectedly parsed: {args:?}"
            );
        }
    }

    #[test]
    fn session_start_rejects_removed_flags_and_accepts_canonical_room() {
        assert!(Cli::try_parse_from(["kodosi", "session", "start", "--mode", "normal"]).is_err());
        assert!(
            Cli::try_parse_from([
                "kodosi",
                "session",
                "start",
                "--scope",
                "room",
                "--workspace",
                "mission",
            ])
            .is_err()
        );

        let cli = Cli::try_parse_from([
            "kodosi", "session", "start", "--scope", "room", "--room", "mission",
        ])
        .expect("canonical room option should parse");
        std::assert_matches!(
            cli.command,
            CliCommand::Session(CliSessionAction::Start(args))
                if args.room.as_deref() == Some("mission")
        );
    }

    #[test]
    fn friends_scope_is_not_a_cli_mutation_argument() {
        assert!(
            Cli::try_parse_from([
                "kodosi",
                "share",
                "set",
                "01900000-0000-7000-8000-000000000001",
                "--scope",
                "friends",
            ])
            .is_err()
        );
        assert!(
            Cli::try_parse_from(["kodosi", "session", "start", "--scope", "friends",]).is_err()
        );
    }

    #[test]
    fn room_chat_limit_rejects_unbounded_values() {
        assert!(
            Cli::try_parse_from([
                "kodosi", "room", "chat", "list", "my-room", "--limit", "501",
            ])
            .is_err()
        );
    }

    #[test]
    fn room_chat_post_accepts_repeatable_typed_recipients_and_peers() {
        let post = Cli::try_parse_from([
            "kodosi",
            "room",
            "chat",
            "post",
            "my-room",
            "- hello",
            "--to-session",
            "01900000-0000-7000-8000-000000000001",
            "--to-session",
            "01900000-0000-7000-8000-000000000002",
            "--to-user",
            "01900000-0000-7000-8000-000000000003",
            "--request-id",
            "01900000-0000-7000-8000-000000000004",
        ])
        .expect("typed recipient flags should parse");
        let CliCommand::Room(CliRoomAction::Chat(CliRoomChatAction::Post(args))) = post.command
        else {
            panic!("expected room chat post");
        };
        assert_eq!(args.recipient_session_ids.len(), 2);
        assert_eq!(args.recipient_user_ids.len(), 1);
        assert_eq!(args.body, "- hello");
        assert_eq!(
            args.request_id.as_deref(),
            Some("01900000-0000-7000-8000-000000000004")
        );

        let peers = Cli::try_parse_from(["kodosi", "room", "chat", "peers", "my-room"])
            .expect("room chat peers should parse");
        std::assert_matches!(
            peers.command,
            CliCommand::Room(CliRoomAction::Chat(CliRoomChatAction::Peers(_)))
        );
    }

    #[test]
    fn machine_login_can_return_before_approval() {
        let cli = Cli::try_parse_from(["kodosi", "--json", "auth", "login", "--no-wait"])
            .expect("machine-readable nonblocking login should parse");
        std::assert_matches!(
            cli.command,
            CliCommand::Auth(CliAuthAction::Login(args)) if args.no_wait
        );
    }

    #[test]
    fn room_channel_server_command_is_hidden_but_parseable() {
        let cli = Cli::try_parse_from(["kodosi", "__internal-room-channel-serve"])
            .expect("internal room channel command should parse");
        std::assert_matches!(cli.command, CliCommand::InternalRoomChannelServe);

        let help = Cli::command().render_long_help().to_string();
        assert!(!help.contains("__internal-room-channel-serve"));
    }
}
