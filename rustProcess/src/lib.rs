#![recursion_limit = "256"]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::needless_pass_by_value,
        clippy::too_many_lines,
        clippy::unchecked_time_subtraction,
        clippy::redundant_clone,
        clippy::match_wildcard_for_single_variants,
        clippy::field_reassign_with_default,
        clippy::manual_let_else,
        clippy::similar_names,
        unused_qualifications,
        let_underscore_drop,
    )
)]
#![forbid(unsafe_code)]

#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(
        target_os = "linux",
        target_arch = "x86_64",
        target_env = "gnu",
        target_pointer_width = "64"
    )
)))]
compile_error!("kodosi-runtime supports arm64 macOS and x86_64 Linux GNU only");

use std::fmt;

use tokio::{
    sync::{
        mpsc::{self, error::TrySendError},
        oneshot,
    },
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

pub(crate) mod agent_intel;
#[cfg(feature = "cli")]
pub(crate) mod cli;
pub(crate) mod config;
pub(crate) mod discovery;
#[cfg(feature = "cli")]
pub(crate) mod headless_host;
pub(crate) mod host_protocol;
pub(crate) mod identity_core;
mod legacy_supervision;
pub(crate) mod local_sessions;
pub(crate) mod remote_sessions;
pub(crate) mod room_crypto;
pub(crate) mod rooms;
pub(crate) mod runtime;
pub(crate) mod runtime_event_bus;
pub(crate) mod session_integrations;
pub(crate) mod session_runtime;
pub(crate) mod sessions_common;
pub(crate) mod sharing;
pub(crate) mod shutdown;
pub(crate) mod support;
pub mod terminal_transport;

pub use crate::host_protocol::{
    AccessGrantEntry, AccountAgentIntelEvent, AccountContextEvent, AccountDeviceEvent,
    AccountFriendsEvent, AccountRoomEvent, AccountSessionEvent, AccountTrustEvent,
    ActivePendingPermission, AgentGlobalEvent, AgentIntelCommand, AgentIntelEvent,
    AgentIntelFailureKind, AuthCommand, AuthEvent, AuthRequiredReason, CollaborationCleanupHealth,
    CollaborationCleanupState, DeviceCommand, DeviceEvent, DeviceLinkOutcome,
    DeviceLinkRequestEntry, FriendEntry, FriendRequestEntry, FriendsCommand, FriendsEvent,
    HiddenSessionEntry, MyDeviceEntry, PendingPermissionDecisionPhase, PendingPermissionsSnapshot,
    RemotePermissionDecisionPhase, RoomActionStatus, RoomAgentDeliveryState, RoomChatEntry,
    RoomCommand, RoomEntry, RoomEvent, RoomInvitationEntry, RoomMemberEntry, RoomTaskEntry,
    SelfDeviceLinkOutcome, SemanticSendMode, SessionCommand, SessionEvent, SteerDeliveryState,
    SteerQueueEntry, SteerTransition, SystemCommand, SystemEvent, TerminalCommand, TerminalEvent,
    TrustCommand, TrustEvent, TrustPinEntry,
};

pub use crate::agent_intel::risk::ApprovalRisk;
#[cfg(feature = "cli")]
pub(crate) use crate::host_protocol::HostCommand;
#[cfg(feature = "cli")]
pub(crate) use crate::host_protocol::{HostEvent, RuntimeSessionStatus};
pub(crate) use crate::host_protocol::{RelayActionStatus, RoomListEntry, SessionListEntry};
pub use crate::runtime_event_bus::RuntimeEventReceivers;
pub use crate::support::error::{AppError, Result};
pub use kodosi_domain::provider_conversation::{
    ProviderConversationIdentity, ProviderConversationProvider,
};

pub mod protocol_authority {
    pub use crate::host_protocol::authority::{
        AddedRequiredField, CompatibilityReport, PROTOCOL_VERSION, RemovedEntry,
        compatibility_report, generated_frozen_snapshot, render_desktop_runtime_authority_json,
    };
}

pub mod protocol_limits {
    pub use crate::host_protocol::{
        CHAT_BODY_MAX_LEN, CHAT_RECIPIENT_MAX_COUNT, ROOM_LENGTH_UNIT, ROOM_NAME_MAX_LEN,
        ROOM_SLUG_MAX_LEN, ROOM_SLUG_MIN_LEN, ROOM_SLUG_PATTERN, TASK_DESCRIPTION_MAX_LEN,
        TASK_RESULT_MAX_LEN, TASK_STATUSES, TASK_TITLE_MAX_LEN,
    };
    pub const IDENTITY_DEVICE_ID_MAX_UTF16_CODE_UNITS: usize =
        crate::identity_core::IDENTITY_DEVICE_ID_MAX_UTF16_CODE_UNITS;
    pub const IDENTITY_DEVICE_LABEL_MAX_UTF16_CODE_UNITS: usize =
        crate::identity_core::IDENTITY_DEVICE_LABEL_MAX_UTF16_CODE_UNITS;
    pub const IDENTITY_MAX_DEVICE_CERTIFICATE_BODY_LEN: usize =
        crate::identity_core::IDENTITY_MAX_DEVICE_CERTIFICATE_BODY_LEN;
    pub const IDENTITY_MAX_SIGNED_DEVICE_LIST_BODY_LEN: usize =
        crate::identity_core::IDENTITY_MAX_SIGNED_DEVICE_LIST_BODY_LEN;
    pub const IDENTITY_ML_DSA_65_PUBLIC_KEY_LEN: usize =
        crate::identity_core::IDENTITY_ML_DSA_65_PUBLIC_KEY_LEN;
    pub const IDENTITY_ML_DSA_65_SIGNATURE_LEN: usize =
        crate::identity_core::IDENTITY_ML_DSA_65_SIGNATURE_LEN;
    pub const IDENTITY_ML_KEM_768_PUBLIC_KEY_LEN: usize =
        crate::identity_core::IDENTITY_ML_KEM_768_PUBLIC_KEY_LEN;
    pub use crate::identity_core::{
        IDENTITY_MAX_ENTRIES, IDENTITY_MAX_FIELD_LEN, IDENTITY_MAX_UNIX_TIME_MILLISECONDS,
        IDENTITY_NO_EXPIRY_SENTINEL,
    };
}

const TERMINAL_CHANNEL_CAPACITY: usize = 256;
const SYSTEM_CHANNEL_CAPACITY: usize = 16;
const AUTH_CHANNEL_CAPACITY: usize = 32;
const FRIENDS_CHANNEL_CAPACITY: usize = 32;
const DEVICES_CHANNEL_CAPACITY: usize = 32;
const TRUST_CHANNEL_CAPACITY: usize = 16;
const ROOM_CHANNEL_CAPACITY: usize = 16;
const SESSIONS_CHANNEL_CAPACITY: usize = 32;
const AGENT_INTEL_CHANNEL_CAPACITY: usize = 32;
const HUB_SUBSCRIBE_CAPACITY: usize = 32;
#[cfg(feature = "cli")]
const DEVICE_RPC_CAPACITY: usize = 8;
#[cfg(feature = "cli")]
const MAX_FRAME_BYTES: usize = kodosi_domain::terminal::TERMINAL_PRESENTATION_CONTROL_MAX_BYTES;

#[cfg(feature = "cli")]
pub(crate) enum DeviceRpcRequest {
    Revoke {
        expected_account_user_id: String,
        device_id: String,
        reply: oneshot::Sender<Result<runtime::identity::DeviceRpcOutcome>>,
    },
    ApproveLink {
        expected_account_user_id: String,
        user_code: String,
        reply: oneshot::Sender<Result<runtime::identity::DeviceRpcOutcome>>,
    },
    StartSelfLink {
        expected_account_user_id: String,
        label: Option<String>,
        reply: oneshot::Sender<Result<runtime::identity::DeviceRpcOutcome>>,
    },
    CancelSelfLink {
        expected_account_user_id: String,
        reply: oneshot::Sender<Result<runtime::identity::DeviceRpcOutcome>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AccountCommandContext {
    pub(crate) user_id: Option<String>,
    pub(crate) epoch: u64,
}

#[derive(Debug)]
pub(crate) struct AccountScopedCommand<T> {
    pub(crate) expected: Option<AccountCommandContext>,
    pub(crate) command: T,
}

impl<T> AccountScopedCommand<T> {
    pub(crate) fn internal(command: T) -> Self {
        Self {
            expected: None,
            command,
        }
    }

    pub(crate) fn scoped(expected: AccountCommandContext, command: T) -> Self {
        Self {
            expected: Some(expected),
            command,
        }
    }
}

pub fn system_command_is_account_scoped(command: &SystemCommand) -> bool {
    matches!(
        command,
        SystemCommand::QueryPendingPermissions { .. }
            | SystemCommand::AllowPendingPermissionRequest { .. }
            | SystemCommand::DenyPendingPermissionRequest { .. }
            | SystemCommand::SemanticSend { .. }
            | SystemCommand::CancelSteer { .. }
            | SystemCommand::QuerySteer { .. }
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RemoteCommandStatus {
    pub(crate) account_user_id: Option<String>,
    pub(crate) account_epoch: u64,
    #[cfg(feature = "cli")]
    pub(crate) remote_operations_ready: bool,
}

#[derive(Debug)]
#[cfg(feature = "cli")]
pub(crate) struct SnapshotRpcRequest {
    pub(crate) expected: AccountCommandContext,
    pub(crate) reply: oneshot::Sender<SnapshotRpcResponse>,
}

#[derive(Debug, Clone)]
#[cfg(feature = "cli")]
pub(crate) struct SnapshotRpcResponse {
    pub(crate) account_user_id: Option<String>,
    pub(crate) account_epoch: u64,
    pub(crate) remote_operations_ready: bool,
    pub(crate) auth: AuthEvent,
    pub(crate) sessions: Vec<SessionListEntry>,
    pub(crate) rooms: Vec<RoomListEntry>,
}

pub struct EmbeddedRuntime {
    command_sink: RuntimeCommandSink,
    runtime_event_rx: Option<RuntimeEventReceivers>,
    shutdown_token: CancellationToken,
    heartbeat_handle: Option<JoinHandle<()>>,
    runtime_handle: Option<JoinHandle<Result<()>>>,
}

#[derive(Clone)]
pub struct RuntimeCommandSink {
    terminal: mpsc::Sender<TerminalCommand>,
    system: mpsc::Sender<AccountScopedCommand<SystemCommand>>,
    auth: mpsc::Sender<AccountScopedCommand<AuthCommand>>,
    friends: mpsc::Sender<AccountScopedCommand<FriendsCommand>>,
    devices: mpsc::Sender<AccountScopedCommand<DeviceCommand>>,
    trust: mpsc::Sender<AccountScopedCommand<TrustCommand>>,
    room: mpsc::Sender<AccountScopedCommand<RoomCommand>>,
    sessions: mpsc::Sender<AccountScopedCommand<SessionCommand>>,
    agent_intel: mpsc::Sender<AccountScopedCommand<AgentIntelCommand>>,
    hub_subscribe: mpsc::Sender<terminal_transport::TerminalHubCommand>,
    remote_status: tokio::sync::watch::Receiver<RemoteCommandStatus>,
    #[cfg(feature = "cli")]
    snapshot_rpc: mpsc::Sender<SnapshotRpcRequest>,
    #[cfg(feature = "cli")]
    device_rpc: mpsc::Sender<DeviceRpcRequest>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCommandSendError {
    Busy,
    Stopped,
    SessionNotFound,
}

impl fmt::Display for RuntimeCommandSendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => f.write_str("runtime busy"),
            Self::Stopped => f.write_str("runtime stopped"),
            Self::SessionNotFound => f.write_str("terminal session not found"),
        }
    }
}

impl std::error::Error for RuntimeCommandSendError {}

macro_rules! impl_lane {
    ($try_method:ident, $send_method:ident, $field:ident, $cmd:ty) => {
        pub fn $try_method(
            &self,
            command: $cmd,
        ) -> std::result::Result<(), RuntimeCommandSendError> {
            self.$field
                .try_send(command)
                .map_err(|error| map_try_send_error(&error))
        }

        pub async fn $send_method(
            &self,
            command: $cmd,
        ) -> std::result::Result<(), RuntimeCommandSendError> {
            self.$field
                .send(command)
                .await
                .map_err(|_| RuntimeCommandSendError::Stopped)
        }
    };
}

macro_rules! impl_scoped_lane {
    ($try_scoped_method:ident, $field:ident, $cmd:ty) => {
        pub fn $try_scoped_method(
            &self,
            command: $cmd,
        ) -> std::result::Result<(), RuntimeCommandSendError> {
            self.$field
                .try_send(AccountScopedCommand::scoped(
                    self.account_command_context(),
                    command,
                ))
                .map_err(|error| map_try_send_error(&error))
        }
    };
}

impl RuntimeCommandSink {
    impl_lane!(try_send_terminal, send_terminal, terminal, TerminalCommand);

    pub fn try_send_system(
        &self,
        command: SystemCommand,
    ) -> std::result::Result<(), RuntimeCommandSendError> {
        self.system
            .try_send(AccountScopedCommand::internal(command))
            .map_err(|error| map_try_send_error(&error))
    }

    pub async fn send_system(
        &self,
        command: SystemCommand,
    ) -> std::result::Result<(), RuntimeCommandSendError> {
        self.system
            .send(AccountScopedCommand::internal(command))
            .await
            .map_err(|_| RuntimeCommandSendError::Stopped)
    }

    impl_scoped_lane!(try_send_system_scoped, system, SystemCommand);
    impl_scoped_lane!(try_send_auth, auth, AuthCommand);
    impl_scoped_lane!(try_send_friends_scoped, friends, FriendsCommand);
    impl_scoped_lane!(try_send_devices_scoped, devices, DeviceCommand);
    impl_scoped_lane!(try_send_trust_scoped, trust, TrustCommand);
    impl_scoped_lane!(try_send_room_scoped, room, RoomCommand);
    impl_scoped_lane!(try_send_session_scoped, sessions, SessionCommand);

    pub fn try_send_agent_intel_scoped(
        &self,
        command: AgentIntelCommand,
    ) -> std::result::Result<(), RuntimeCommandSendError> {
        self.agent_intel
            .try_send(AccountScopedCommand::scoped(
                self.account_command_context(),
                command,
            ))
            .map_err(|error| map_try_send_error(&error))
    }

    pub(crate) fn account_command_context(&self) -> AccountCommandContext {
        let status = self.remote_status.borrow();
        AccountCommandContext {
            user_id: status.account_user_id.clone(),
            epoch: status.account_epoch,
        }
    }

    #[cfg(feature = "cli")]
    pub(crate) fn remote_command_status(&self) -> RemoteCommandStatus {
        self.remote_status.borrow().clone()
    }

    #[cfg(feature = "cli")]
    pub(crate) fn subscribe_remote_command_status(
        &self,
    ) -> tokio::sync::watch::Receiver<RemoteCommandStatus> {
        self.remote_status.clone()
    }

    #[cfg(feature = "cli")]
    pub(crate) async fn snapshot_rpc(
        &self,
        expected: AccountCommandContext,
    ) -> std::result::Result<SnapshotRpcResponse, RuntimeCommandSendError> {
        let (reply, response) = oneshot::channel();
        self.snapshot_rpc
            .send(SnapshotRpcRequest { expected, reply })
            .await
            .map_err(|_| RuntimeCommandSendError::Stopped)?;
        response.await.map_err(|_| RuntimeCommandSendError::Stopped)
    }

    #[cfg(feature = "cli")]
    pub(crate) async fn send_device_rpc(
        &self,
        request: DeviceRpcRequest,
    ) -> std::result::Result<(), RuntimeCommandSendError> {
        self.device_rpc
            .send(request)
            .await
            .map_err(|_| RuntimeCommandSendError::Stopped)
    }

    #[cfg(feature = "cli")]
    pub(crate) async fn dispatch_host(
        &self,
        command: HostCommand,
        expected: AccountCommandContext,
    ) -> std::result::Result<(), RuntimeCommandSendError> {
        match command {
            HostCommand::Terminal(command) => self.send_terminal(command).await,
            HostCommand::System(command) if system_command_is_account_scoped(&command) => self
                .system
                .send(AccountScopedCommand::scoped(expected, command))
                .await
                .map_err(|_| RuntimeCommandSendError::Stopped),
            HostCommand::System(command) => self.send_system(command).await,
            HostCommand::Auth(command) => self
                .auth
                .send(AccountScopedCommand::scoped(expected, command))
                .await
                .map_err(|_| RuntimeCommandSendError::Stopped),
            HostCommand::Friends(command) => self
                .friends
                .send(AccountScopedCommand::scoped(expected, command))
                .await
                .map_err(|_| RuntimeCommandSendError::Stopped),
            HostCommand::Devices(command) => self
                .devices
                .send(AccountScopedCommand::scoped(expected, command))
                .await
                .map_err(|_| RuntimeCommandSendError::Stopped),
            HostCommand::Trust(command) => self
                .trust
                .send(AccountScopedCommand::scoped(expected, command))
                .await
                .map_err(|_| RuntimeCommandSendError::Stopped),
            HostCommand::Room(command) => self
                .room
                .send(AccountScopedCommand::scoped(expected, command))
                .await
                .map_err(|_| RuntimeCommandSendError::Stopped),
            HostCommand::Session(command) => self
                .sessions
                .send(AccountScopedCommand::scoped(expected, command))
                .await
                .map_err(|_| RuntimeCommandSendError::Stopped),
            HostCommand::AgentIntel(command) => self
                .agent_intel
                .send(AccountScopedCommand::scoped(expected, command))
                .await
                .map_err(|_| RuntimeCommandSendError::Stopped),
        }
    }

    #[must_use]
    pub fn is_runtime_loop_open(&self) -> bool {
        #[cfg(feature = "cli")]
        let headless_open = !self.snapshot_rpc.is_closed() && !self.device_rpc.is_closed();
        #[cfg(not(feature = "cli"))]
        let headless_open = true;

        !self.terminal.is_closed()
            && !self.system.is_closed()
            && !self.auth.is_closed()
            && !self.friends.is_closed()
            && !self.devices.is_closed()
            && !self.trust.is_closed()
            && !self.room.is_closed()
            && !self.sessions.is_closed()
            && !self.agent_intel.is_closed()
            && !self.hub_subscribe.is_closed()
            && self.remote_status.has_changed().is_ok()
            && headless_open
    }

    pub async fn subscribe_terminal_hub(
        &self,
        session_id: kodosi_domain::ids::SessionId,
        surface: terminal_transport::TerminalSurface,
        capability: terminal_transport::TerminalCapability,
    ) -> std::result::Result<terminal_transport::hub::SubscriberHandle, RuntimeCommandSendError>
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.hub_subscribe
            .send(terminal_transport::TerminalHubCommand::Subscribe(
                terminal_transport::TerminalHubRequest {
                    session_id,
                    surface,
                    capability,
                    reply: reply_tx,
                },
            ))
            .await
            .map_err(|_| RuntimeCommandSendError::Stopped)?;
        reply_rx
            .await
            .map_err(|_| RuntimeCommandSendError::Stopped)?
            .ok_or(RuntimeCommandSendError::SessionNotFound)
    }

    pub async fn unsubscribe_terminal_hub(
        &self,
        session_id: kodosi_domain::ids::SessionId,
        connection_id: terminal_transport::TerminalConnectionId,
    ) -> std::result::Result<(), RuntimeCommandSendError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.hub_subscribe
            .send(terminal_transport::TerminalHubCommand::Unsubscribe(
                terminal_transport::TerminalHubUnsubscribeRequest {
                    session_id,
                    connection_id,
                    reply: reply_tx,
                },
            ))
            .await
            .map_err(|_| RuntimeCommandSendError::Stopped)?;
        reply_rx.await.map_err(|_| RuntimeCommandSendError::Stopped)
    }
}

fn map_try_send_error<T>(error: &TrySendError<T>) -> RuntimeCommandSendError {
    match error {
        TrySendError::Full(_) => RuntimeCommandSendError::Busy,
        TrySendError::Closed(_) => RuntimeCommandSendError::Stopped,
    }
}

impl Drop for EmbeddedRuntime {
    fn drop(&mut self) {
        self.shutdown_token.cancel();
        if let Some(handle) = self.heartbeat_handle.take() {
            handle.abort();
        }
        if let Some(handle) = self.runtime_handle.take() {
            handle.abort();
        }
    }
}

impl EmbeddedRuntime {
    #[must_use]
    pub fn command_sink(&self) -> RuntimeCommandSink {
        self.command_sink.clone()
    }

    pub fn take_events(&mut self) -> Option<RuntimeEventReceivers> {
        self.runtime_event_rx.take()
    }

    pub fn shutdown(&self) {
        self.shutdown_token.cancel();
    }

    #[must_use]
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown_token.clone()
    }

    pub async fn join(self) -> Result<()> {
        let deadline = tokio::time::Instant::now() + shutdown::HOST_SHUTDOWN_BUDGET;
        self.join_before(deadline).await
    }

    pub async fn join_before(mut self, deadline: tokio::time::Instant) -> Result<()> {
        self.shutdown_token.cancel();

        if let Some(heartbeat_handle) = self.heartbeat_handle.take() {
            shutdown::join_within("runtime heartbeat", heartbeat_handle, deadline).await;
        }

        if let Some(mut runtime_handle) = self.runtime_handle.take() {
            match tokio::time::timeout_at(deadline, &mut runtime_handle).await {
                Ok(Ok(Ok(()))) => {}
                Ok(Ok(Err(error))) => return Err(error),
                Ok(Err(error)) => return Err(AppError::Join(error)),
                Err(_elapsed) => {
                    runtime_handle.abort();
                    match runtime_handle.await {
                        Err(error) if error.is_cancelled() => {}
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => return Err(error),
                        Err(error) => return Err(AppError::Join(error)),
                    }
                    return Err(AppError::Unsupported {
                        reason: format!(
                            "runtime loop did not stop within its {}s shutdown budget; aborted",
                            shutdown::HOST_SHUTDOWN_BUDGET.as_secs()
                        ),
                    });
                }
            }
        }

        Ok(())
    }
}

#[cfg(feature = "cli")]
#[expect(
    clippy::future_not_send,
    reason = "session attach owns a non-Send Ghostty mirror on the CLI current-thread runtime"
)]
pub async fn run_cli() -> std::process::ExitCode {
    Box::pin(cli::run()).await
}

pub async fn start_embedded_runtime() -> Result<EmbeddedRuntime> {
    start_embedded_runtime_with_config(load_default_config()?, CancellationToken::new()).await
}

fn load_default_config() -> Result<config::AppConfig> {
    let config = config::AppConfig::load(None)?;
    support::telemetry::tracing_setup::install(&config.log_filter)?;
    match legacy_supervision::cleanup_default_home_once() {
        Ok(Some(report)) if !report.changed_paths.is_empty() => {
            tracing::info!(
                removed_entries = report.removed_entries,
                helper_removed = report.helper_removed,
                "removed legacy user-global supervision hooks"
            );
        }
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(%error, "legacy supervision cleanup requires manual repair");
        }
    }
    Ok(config)
}

async fn start_embedded_runtime_with_config(
    config: config::AppConfig,
    shutdown_token: CancellationToken,
) -> Result<EmbeddedRuntime> {
    let authority_lock = support::storage::runtime_lock::RuntimeAuthorityLock::acquire()?;
    let mut app = runtime::Runtime::new(config, shutdown_token.clone())?;
    app.initialize().await?;
    let (terminal_tx, terminal_rx) = mpsc::channel::<TerminalCommand>(TERMINAL_CHANNEL_CAPACITY);
    let (system_tx, system_rx) =
        mpsc::channel::<AccountScopedCommand<SystemCommand>>(SYSTEM_CHANNEL_CAPACITY);
    let (auth_tx, auth_rx) =
        mpsc::channel::<AccountScopedCommand<AuthCommand>>(AUTH_CHANNEL_CAPACITY);
    let (friends_tx, friends_rx) =
        mpsc::channel::<AccountScopedCommand<FriendsCommand>>(FRIENDS_CHANNEL_CAPACITY);
    let (devices_tx, devices_rx) =
        mpsc::channel::<AccountScopedCommand<DeviceCommand>>(DEVICES_CHANNEL_CAPACITY);
    let (trust_tx, trust_rx) =
        mpsc::channel::<AccountScopedCommand<TrustCommand>>(TRUST_CHANNEL_CAPACITY);
    let (room_tx, room_rx) =
        mpsc::channel::<AccountScopedCommand<RoomCommand>>(ROOM_CHANNEL_CAPACITY);
    let (sessions_tx, sessions_rx) =
        mpsc::channel::<AccountScopedCommand<SessionCommand>>(SESSIONS_CHANNEL_CAPACITY);
    let (agent_intel_tx, agent_intel_rx) =
        mpsc::channel::<AccountScopedCommand<AgentIntelCommand>>(AGENT_INTEL_CHANNEL_CAPACITY);
    let (hub_subscribe_tx, hub_subscribe_rx) =
        mpsc::channel::<terminal_transport::TerminalHubCommand>(HUB_SUBSCRIBE_CAPACITY);
    let (initial_account_user_id, initial_account_epoch) = app.state.identity.event_context();
    let initial_status = RemoteCommandStatus {
        account_user_id: initial_account_user_id,
        account_epoch: initial_account_epoch,
        #[cfg(feature = "cli")]
        remote_operations_ready: app.remote_surfaces_ready(),
    };
    let (remote_status_tx, remote_status_rx) = tokio::sync::watch::channel(initial_status);
    #[cfg(feature = "cli")]
    let (snapshot_rpc_tx, snapshot_rpc_rx) = mpsc::channel::<SnapshotRpcRequest>(8);
    #[cfg(feature = "cli")]
    let (device_rpc_tx, device_rpc_rx) = mpsc::channel::<DeviceRpcRequest>(DEVICE_RPC_CAPACITY);
    let (runtime_event_tx, runtime_event_rx) = runtime_event_bus::runtime_event_channels();
    let heartbeat_handle = tokio::spawn(runtime::runtime_loop::heartbeat_loop(
        runtime_event_tx.clone(),
        shutdown_token.clone(),
    ));

    drop(agent_intel::mcp_health_task::spawn(
        runtime_event_tx.clone(),
        shutdown_token.clone(),
        ::agent_intel::runtime::paths::claude_home(),
        ::agent_intel::runtime::paths::copilot_home(),
    ));

    let runtime_shutdown = shutdown_token.clone();
    let runtime_handle = tokio::spawn(async move {
        let _authority_lock = authority_lock;
        Box::pin(runtime::runtime_loop::runtime_loop(
            app,
            runtime::runtime_loop::RuntimeLaneReceivers {
                terminal: terminal_rx,
                system: system_rx,
                auth: auth_rx,
                friends: friends_rx,
                devices: devices_rx,
                trust: trust_rx,
                room: room_rx,
                sessions: sessions_rx,
                agent_intel: agent_intel_rx,
                hub_subscribe: hub_subscribe_rx,
                remote_status: remote_status_tx,
                headless: runtime::runtime_loop::HeadlessRuntimeLaneReceivers {
                    #[cfg(feature = "cli")]
                    snapshot_rpc: snapshot_rpc_rx,
                    #[cfg(feature = "cli")]
                    device_rpc: device_rpc_rx,
                },
            },
            runtime_event_tx,
            runtime_shutdown,
        ))
        .await
    });

    let embedded_runtime = EmbeddedRuntime {
        command_sink: RuntimeCommandSink {
            terminal: terminal_tx,
            system: system_tx,
            auth: auth_tx,
            friends: friends_tx,
            devices: devices_tx,
            trust: trust_tx,
            room: room_tx,
            sessions: sessions_tx,
            agent_intel: agent_intel_tx,
            hub_subscribe: hub_subscribe_tx,
            remote_status: remote_status_rx,
            #[cfg(feature = "cli")]
            snapshot_rpc: snapshot_rpc_tx,
            #[cfg(feature = "cli")]
            device_rpc: device_rpc_tx,
        },
        runtime_event_rx: Some(runtime_event_rx),
        shutdown_token,
        heartbeat_handle: Some(heartbeat_handle),
        runtime_handle: Some(runtime_handle),
    };

    Ok(embedded_runtime)
}

#[cfg(test)]
mod embedded_runtime_tests {
    use super::*;

    fn command_sink_with_scoped_receivers(
        status: RemoteCommandStatus,
    ) -> (
        RuntimeCommandSink,
        tokio::sync::watch::Sender<RemoteCommandStatus>,
        mpsc::Receiver<AccountScopedCommand<RoomCommand>>,
        mpsc::Receiver<AccountScopedCommand<AuthCommand>>,
    ) {
        let (terminal, _) = mpsc::channel(1);
        let (system, _) = mpsc::channel(1);
        let (auth, auth_rx) = mpsc::channel(1);
        let (friends, _) = mpsc::channel(1);
        let (devices, _) = mpsc::channel(1);
        let (trust, _) = mpsc::channel(1);
        let (room, room_rx) = mpsc::channel(1);
        let (sessions, _) = mpsc::channel(1);
        let (agent_intel, _) = mpsc::channel(1);
        let (hub_subscribe, _) = mpsc::channel(1);
        let (remote_status_tx, remote_status) = tokio::sync::watch::channel(status);
        #[cfg(feature = "cli")]
        let (snapshot_rpc, _) = mpsc::channel(1);
        #[cfg(feature = "cli")]
        let (device_rpc, _) = mpsc::channel(1);
        (
            RuntimeCommandSink {
                terminal,
                system,
                auth,
                friends,
                devices,
                trust,
                room,
                sessions,
                agent_intel,
                hub_subscribe,
                remote_status,
                #[cfg(feature = "cli")]
                snapshot_rpc,
                #[cfg(feature = "cli")]
                device_rpc,
            },
            remote_status_tx,
            room_rx,
            auth_rx,
        )
    }

    #[cfg(feature = "cli")]
    #[tokio::test]
    async fn host_dispatch_keeps_hello_account_after_status_changes() {
        let account_a = AccountCommandContext {
            user_id: Some("account-a".to_owned()),
            epoch: 7,
        };
        let (sink, status_tx, mut room_rx, mut auth_rx) =
            command_sink_with_scoped_receivers(RemoteCommandStatus {
                account_user_id: account_a.user_id.clone(),
                account_epoch: account_a.epoch,
                #[cfg(feature = "cli")]
                remote_operations_ready: true,
            });
        status_tx
            .send(RemoteCommandStatus {
                account_user_id: Some("account-b".to_owned()),
                account_epoch: 8,
                #[cfg(feature = "cli")]
                remote_operations_ready: true,
            })
            .expect("advance runtime account");

        sink.dispatch_host(HostCommand::Room(RoomCommand::Refresh), account_a.clone())
            .await
            .expect("queue stale-account command for runtime rejection");

        let queued = room_rx.recv().await.expect("room command");
        assert_eq!(queued.expected, Some(account_a.clone()));

        sink.dispatch_host(HostCommand::Auth(AuthCommand::Logout), account_a.clone())
            .await
            .expect("queue stale-account auth command for runtime rejection");
        let queued = auth_rx.recv().await.expect("auth command");
        assert_eq!(queued.expected, Some(account_a));
        assert!(matches!(queued.command, AuthCommand::Logout));
    }

    #[tokio::test]
    async fn dropping_embedded_runtime_cancels_and_aborts_owned_tasks() {
        let (command_sink, _status_tx, _room_rx, _auth_rx) =
            command_sink_with_scoped_receivers(RemoteCommandStatus {
                account_user_id: None,
                account_epoch: 1,
                #[cfg(feature = "cli")]
                remote_operations_ready: false,
            });
        let shutdown_token = CancellationToken::new();
        let observed_shutdown = shutdown_token.clone();
        let heartbeat_handle = tokio::spawn(std::future::pending::<()>());
        let heartbeat_abort = heartbeat_handle.abort_handle();
        let runtime_handle = tokio::spawn(std::future::pending::<Result<()>>());
        let runtime_abort = runtime_handle.abort_handle();
        let runtime = EmbeddedRuntime {
            command_sink,
            runtime_event_rx: None,
            shutdown_token,
            heartbeat_handle: Some(heartbeat_handle),
            runtime_handle: Some(runtime_handle),
        };

        drop(runtime);

        assert!(observed_shutdown.is_cancelled());
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while !heartbeat_abort.is_finished() || !runtime_abort.is_finished() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("drop must terminate both owned tasks");
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_abort_destroys_task_owned_authority_before_returning() {
        let root = tempfile::tempdir().expect("lock root");
        let lock_path = root.path().join("runtime-authority.lock");
        let authority = support::storage::runtime_lock::RuntimeAuthorityLock::at(&lock_path)
            .expect("first authority");
        let runtime_handle = tokio::spawn(async move {
            let _authority = authority;
            std::future::pending::<Result<()>>().await
        });
        let (terminal, _) = mpsc::channel(1);
        let (system, _) = mpsc::channel(1);
        let (auth, _) = mpsc::channel(1);
        let (friends, _) = mpsc::channel(1);
        let (devices, _) = mpsc::channel(1);
        let (trust, _) = mpsc::channel(1);
        let (room, _) = mpsc::channel(1);
        let (sessions, _) = mpsc::channel(1);
        let (agent_intel, _) = mpsc::channel(1);
        let (hub_subscribe, _) = mpsc::channel(1);
        let (_remote_status_tx, remote_status) = tokio::sync::watch::channel(RemoteCommandStatus {
            account_user_id: None,
            account_epoch: 1,
            #[cfg(feature = "cli")]
            remote_operations_ready: false,
        });
        #[cfg(feature = "cli")]
        let (snapshot_rpc, _) = mpsc::channel(1);
        #[cfg(feature = "cli")]
        let (device_rpc, _) = mpsc::channel(1);
        let runtime = EmbeddedRuntime {
            command_sink: RuntimeCommandSink {
                terminal,
                system,
                auth,
                friends,
                devices,
                trust,
                room,
                sessions,
                agent_intel,
                hub_subscribe,
                remote_status,
                #[cfg(feature = "cli")]
                snapshot_rpc,
                #[cfg(feature = "cli")]
                device_rpc,
            },
            runtime_event_rx: None,
            shutdown_token: CancellationToken::new(),
            heartbeat_handle: None,
            runtime_handle: Some(runtime_handle),
        };

        let error = runtime
            .join_before(tokio::time::Instant::now())
            .await
            .expect_err("zero deadline aborts pending runtime");
        assert!(error.to_string().contains("aborted"));

        support::storage::runtime_lock::RuntimeAuthorityLock::at(&lock_path)
            .expect("successor acquires immediately after join returns");
    }
}
