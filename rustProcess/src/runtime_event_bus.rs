use tokio::sync::mpsc;

use crate::{
    AccountAgentIntelEvent, AccountDeviceEvent, AccountFriendsEvent, AccountRoomEvent,
    AccountSessionEvent, AccountTrustEvent, AgentGlobalEvent, AgentIntelEvent, AppError, AuthEvent,
    DeviceEvent, FriendsEvent, Result, RoomEvent, SessionEvent, SystemEvent, TerminalEvent,
    TrustEvent,
};

pub(crate) const RUNTIME_TERMINAL_CONTROL_CAPACITY: usize = 256;
pub(crate) const RUNTIME_SYSTEM_CAPACITY: usize = 64;
pub(crate) const RUNTIME_FRIENDS_CAPACITY: usize = 64;
pub(crate) const RUNTIME_DEVICES_CAPACITY: usize = 64;
pub(crate) const RUNTIME_TRUST_CAPACITY: usize = 32;
pub(crate) const RUNTIME_ROOM_CAPACITY: usize = 32;
pub(crate) const RUNTIME_AUTH_CAPACITY: usize = 64;
pub(crate) const RUNTIME_SESSIONS_CAPACITY: usize = 256;
pub(crate) const RUNTIME_AGENT_INTEL_CAPACITY: usize = 64;
pub(crate) const RUNTIME_AGENT_GLOBAL_CAPACITY: usize = 16;

#[derive(Clone)]
pub(crate) struct RuntimeEventSender {
    terminal_control: mpsc::Sender<TerminalEvent>,
    system: mpsc::Sender<SystemEvent>,
    friends: mpsc::Sender<AccountFriendsEvent>,
    devices: mpsc::Sender<AccountDeviceEvent>,
    trust: mpsc::Sender<AccountTrustEvent>,
    room: mpsc::Sender<AccountRoomEvent>,
    auth: mpsc::Sender<AuthEvent>,
    sessions: mpsc::Sender<AccountSessionEvent>,
    agent_intel: mpsc::Sender<AccountAgentIntelEvent>,
    agent_global: mpsc::Sender<AgentGlobalEvent>,
}

#[allow(
    clippy::struct_field_names,
    reason = "all fields carry '_rx' suffix matching their sender counterparts"
)]
pub struct RuntimeEventReceivers {
    pub terminal_control_rx: mpsc::Receiver<TerminalEvent>,
    pub system_rx: mpsc::Receiver<SystemEvent>,
    pub friends_rx: mpsc::Receiver<AccountFriendsEvent>,
    pub devices_rx: mpsc::Receiver<AccountDeviceEvent>,
    pub trust_rx: mpsc::Receiver<AccountTrustEvent>,
    pub room_rx: mpsc::Receiver<AccountRoomEvent>,
    pub auth_rx: mpsc::Receiver<AuthEvent>,
    pub sessions_rx: mpsc::Receiver<AccountSessionEvent>,
    pub agent_intel_rx: mpsc::Receiver<AccountAgentIntelEvent>,
    pub agent_global_rx: mpsc::Receiver<AgentGlobalEvent>,
}

#[cfg(any(test, feature = "cli"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeEventLane {
    TerminalControl,
    System,
    Sessions,
    Friends,
    Devices,
    Trust,
    Room,
    Auth,
    AgentIntel,
    AgentGlobal,
}

#[cfg(any(test, feature = "cli"))]
impl RuntimeEventLane {
    const ALL: [Self; 10] = [
        Self::TerminalControl,
        Self::System,
        Self::Sessions,
        Self::Friends,
        Self::Devices,
        Self::Trust,
        Self::Room,
        Self::Auth,
        Self::AgentIntel,
        Self::AgentGlobal,
    ];

    const fn bit(self) -> u16 {
        match self {
            Self::TerminalControl => 1 << 0,
            Self::System => 1 << 1,
            Self::Sessions => 1 << 2,
            Self::Friends => 1 << 3,
            Self::Devices => 1 << 4,
            Self::Trust => 1 << 5,
            Self::Room => 1 << 6,
            Self::Auth => 1 << 7,
            Self::AgentIntel => 1 << 8,
            Self::AgentGlobal => 1 << 9,
        }
    }

    fn all_done_bits() -> u16 {
        Self::ALL
            .into_iter()
            .fold(0, |done, lane| done | lane.bit())
    }
}

#[cfg(any(test, feature = "cli"))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RuntimeEventLaneMask {
    done: u16,
}

#[cfg(any(test, feature = "cli"))]
impl RuntimeEventLaneMask {
    pub(crate) const fn empty() -> Self {
        Self { done: 0 }
    }

    pub(crate) fn mark_done(&mut self, lane: RuntimeEventLane) {
        self.done |= lane.bit();
    }

    pub(crate) const fn contains(self, lane: RuntimeEventLane) -> bool {
        self.done & lane.bit() != 0
    }

    pub(crate) fn is_all_done(self) -> bool {
        self.done == RuntimeEventLane::all_done_bits()
    }
}

pub(crate) fn runtime_event_channels() -> (RuntimeEventSender, RuntimeEventReceivers) {
    let (terminal_control_tx, terminal_control_rx) =
        mpsc::channel::<TerminalEvent>(RUNTIME_TERMINAL_CONTROL_CAPACITY);
    let (system_tx, system_rx) = mpsc::channel::<SystemEvent>(RUNTIME_SYSTEM_CAPACITY);
    let (friends_tx, friends_rx) = mpsc::channel::<AccountFriendsEvent>(RUNTIME_FRIENDS_CAPACITY);
    let (devices_tx, devices_rx) = mpsc::channel::<AccountDeviceEvent>(RUNTIME_DEVICES_CAPACITY);
    let (trust_tx, trust_rx) = mpsc::channel::<AccountTrustEvent>(RUNTIME_TRUST_CAPACITY);
    let (room_tx, room_rx) = mpsc::channel::<AccountRoomEvent>(RUNTIME_ROOM_CAPACITY);
    let (auth_tx, auth_rx) = mpsc::channel::<AuthEvent>(RUNTIME_AUTH_CAPACITY);
    let (sessions_tx, sessions_rx) =
        mpsc::channel::<AccountSessionEvent>(RUNTIME_SESSIONS_CAPACITY);
    let (agent_intel_tx, agent_intel_rx) =
        mpsc::channel::<AccountAgentIntelEvent>(RUNTIME_AGENT_INTEL_CAPACITY);
    let (agent_global_tx, agent_global_rx) =
        mpsc::channel::<AgentGlobalEvent>(RUNTIME_AGENT_GLOBAL_CAPACITY);

    (
        RuntimeEventSender {
            terminal_control: terminal_control_tx,
            system: system_tx,
            friends: friends_tx,
            devices: devices_tx,
            trust: trust_tx,
            room: room_tx,
            auth: auth_tx,
            sessions: sessions_tx,
            agent_intel: agent_intel_tx,
            agent_global: agent_global_tx,
        },
        RuntimeEventReceivers {
            terminal_control_rx,
            system_rx,
            friends_rx,
            devices_rx,
            trust_rx,
            room_rx,
            auth_rx,
            sessions_rx,
            agent_intel_rx,
            agent_global_rx,
        },
    )
}

macro_rules! impl_event_lane {
    ($method:ident, $field:ident, $event:ty, $label:literal) => {
        #[must_use = concat!(
                                            stringify!($method),
                                            " may fail when the event consumer has shut down"
                                        )]
        pub(crate) async fn $method(&self, event: $event) -> Result<()> {
            self.$field
                .send(event)
                .await
                .map_err(|_closed| AppError::ChannelClosed {
                    session: $label.to_owned(),
                })
        }
    };
}

impl RuntimeEventSender {
    pub(crate) async fn send_session(
        &self,
        account_user_id: Option<String>,
        account_epoch: u64,
        event: SessionEvent,
    ) -> Result<()> {
        self.sessions
            .send(AccountSessionEvent::new(
                account_user_id,
                account_epoch,
                event,
            ))
            .await
            .map_err(|_| AppError::ChannelClosed {
                session: "runtime.sessions".to_owned(),
            })
    }

    pub(crate) async fn send_agent_intel(
        &self,
        account_user_id: Option<String>,
        account_epoch: u64,
        event: AgentIntelEvent,
    ) -> Result<()> {
        self.agent_intel
            .send(AccountAgentIntelEvent::new(
                account_user_id,
                account_epoch,
                event,
            ))
            .await
            .map_err(|_| AppError::ChannelClosed {
                session: "runtime.agent_intel".to_owned(),
            })
    }

    pub(crate) fn agent_intel_event_sender(&self) -> &mpsc::Sender<AccountAgentIntelEvent> {
        &self.agent_intel
    }

    pub(crate) fn agent_global_event_sender(&self) -> &mpsc::Sender<AgentGlobalEvent> {
        &self.agent_global
    }

    pub(crate) async fn send_friends(
        &self,
        account_user_id: Option<String>,
        account_epoch: u64,
        event: FriendsEvent,
    ) -> Result<()> {
        self.friends
            .send(AccountFriendsEvent::new(
                account_user_id,
                account_epoch,
                event,
            ))
            .await
            .map_err(|_| AppError::ChannelClosed {
                session: "runtime.friends".to_owned(),
            })
    }

    pub(crate) async fn send_devices(
        &self,
        account_user_id: Option<String>,
        account_epoch: u64,
        event: DeviceEvent,
    ) -> Result<()> {
        self.devices
            .send(AccountDeviceEvent::new(
                account_user_id,
                account_epoch,
                event,
            ))
            .await
            .map_err(|_| AppError::ChannelClosed {
                session: "runtime.devices".to_owned(),
            })
    }

    pub(crate) async fn send_trust(
        &self,
        account_user_id: Option<String>,
        account_epoch: u64,
        event: TrustEvent,
    ) -> Result<()> {
        self.trust
            .send(AccountTrustEvent::new(
                account_user_id,
                account_epoch,
                event,
            ))
            .await
            .map_err(|_| AppError::ChannelClosed {
                session: "runtime.trust".to_owned(),
            })
    }

    pub(crate) async fn send_room(
        &self,
        account_user_id: Option<String>,
        account_epoch: u64,
        event: RoomEvent,
    ) -> Result<()> {
        self.room
            .send(AccountRoomEvent::new(account_user_id, account_epoch, event))
            .await
            .map_err(|_| AppError::ChannelClosed {
                session: "runtime.room".to_owned(),
            })
    }

    impl_event_lane!(
        send_terminal_control,
        terminal_control,
        TerminalEvent,
        "runtime.terminal_control"
    );
    impl_event_lane!(send_system, system, SystemEvent, "runtime.system");
    impl_event_lane!(send_auth, auth, AuthEvent, "runtime.auth");
    impl_event_lane!(
        send_agent_global,
        agent_global,
        AgentGlobalEvent,
        "runtime.agent_global"
    );

    pub(crate) fn try_send_heartbeat(&self) -> bool {
        match self.system.try_send(SystemEvent::Heartbeat) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::debug!("heartbeat skipped — runtime.system full");
                true
            }
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RoomEvent;

    fn claude_global_status() -> ::agent_intel::ops::dto::ClaudeGlobalStatus {
        ::agent_intel::ops::dto::ClaudeGlobalStatus {
            cwd: None,
            claude_code_version: None,
            installed_plugins: Vec::new(),
            loaded_skills: Vec::new(),
            loaded_agents: Vec::new(),
            mcp_servers: Vec::new(),
            notices: None,
        }
    }

    #[cfg(any(test, feature = "cli"))]
    #[test]
    fn lane_mask_completes_only_after_every_runtime_lane_closes() {
        let mut done_lanes = RuntimeEventLaneMask::empty();
        assert!(!done_lanes.is_all_done());

        for lane in RuntimeEventLane::ALL {
            assert!(!done_lanes.contains(lane));
            done_lanes.mark_done(lane);
            assert!(done_lanes.contains(lane));
        }

        assert!(done_lanes.is_all_done());
    }

    #[tokio::test]
    async fn terminal_control_send_lands_on_terminal_control_lane_only() {
        let (tx, mut rx) = runtime_event_channels();
        tx.send_terminal_control(TerminalEvent::Notification {
            session_id: "s".to_owned(),
            title: None,
            body: None,
        })
        .await
        .unwrap_or_else(|error| panic!("terminal control send should succeed: {error}"));
        std::assert_matches!(
            rx.terminal_control_rx.try_recv(),
            Ok(TerminalEvent::Notification { .. })
        );
        assert!(rx.system_rx.try_recv().is_err());
        assert!(rx.auth_rx.try_recv().is_err());
        assert!(rx.agent_intel_rx.try_recv().is_err());
        assert!(rx.agent_global_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn system_send_lands_on_system_lane_only() {
        let (tx, mut rx) = runtime_event_channels();
        tx.send_system(SystemEvent::Error {
            message: "boom".to_owned(),
            context: None,
        })
        .await
        .unwrap_or_else(|error| panic!("system send should succeed: {error}"));
        std::assert_matches!(rx.system_rx.try_recv(), Ok(SystemEvent::Error { .. }));
        assert!(rx.terminal_control_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn friends_send_lands_on_friends_lane_only() {
        let (tx, mut rx) = runtime_event_channels();
        tx.send_friends(
            Some("user-1".to_owned()),
            1,
            FriendsEvent::Error {
                operation: "refresh".to_owned(),
                message: "backend unavailable".to_owned(),
                request_id: None,
            },
        )
        .await
        .unwrap_or_else(|error| panic!("friends send should succeed: {error}"));

        std::assert_matches!(
            rx.friends_rx.try_recv(),
            Ok(AccountFriendsEvent {
                account_user_id: Some(account_user_id),
                account_epoch: 1,
                event: FriendsEvent::Error { .. },
                ..
            }) if account_user_id == "user-1"
        );
        assert!(rx.terminal_control_rx.try_recv().is_err());
        assert!(rx.system_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn agent_global_send_lands_on_agent_global_lane_only() {
        let (tx, mut rx) = runtime_event_channels();
        tx.send_agent_global(AgentGlobalEvent::ClaudeStatus {
            generation: None,
            status: claude_global_status(),
        })
        .await
        .unwrap_or_else(|error| panic!("agent global send should succeed: {error}"));

        std::assert_matches!(
            rx.agent_global_rx.try_recv(),
            Ok(AgentGlobalEvent::ClaudeStatus { .. })
        );
        assert!(rx.terminal_control_rx.try_recv().is_err());
        assert!(rx.system_rx.try_recv().is_err());
        assert!(rx.agent_intel_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn devices_send_lands_on_devices_lane_only() {
        let (tx, mut rx) = runtime_event_channels();
        tx.send_devices(
            Some("user-1".to_owned()),
            1,
            DeviceEvent::Error {
                user_code: None,
                operation: "refresh".to_owned(),
                message: "backend unavailable".to_owned(),
            },
        )
        .await
        .unwrap_or_else(|error| panic!("devices send should succeed: {error}"));

        std::assert_matches!(
            rx.devices_rx.try_recv(),
            Ok(AccountDeviceEvent {
                account_user_id: Some(account_user_id),
                account_epoch: 1,
                event: DeviceEvent::Error { .. },
                ..
            }) if account_user_id == "user-1"
        );
        assert!(rx.terminal_control_rx.try_recv().is_err());
        assert!(rx.system_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn trust_send_lands_on_trust_lane_only() {
        let (tx, mut rx) = runtime_event_channels();
        tx.send_trust(
            Some("user-1".to_owned()),
            1,
            TrustEvent::Error {
                request_id: None,
                user_id: None,
                operation: "refresh".to_owned(),
                message: "store unavailable".to_owned(),
            },
        )
        .await
        .unwrap_or_else(|error| panic!("trust send should succeed: {error}"));

        std::assert_matches!(
            rx.trust_rx.try_recv(),
            Ok(AccountTrustEvent {
                account_user_id: Some(account_user_id),
                account_epoch: 1,
                event: TrustEvent::Error { .. },
                ..
            }) if account_user_id == "user-1"
        );
        assert!(rx.terminal_control_rx.try_recv().is_err());
        assert!(rx.devices_rx.try_recv().is_err());
        assert!(rx.system_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn room_send_lands_on_room_lane_only() {
        let (tx, mut rx) = runtime_event_channels();
        tx.send_room(
            Some("user-1".to_owned()),
            1,
            RoomEvent::Error {
                room_id: None,
                operation: "refresh".to_owned(),
                message: "backend unavailable".to_owned(),
            },
        )
        .await
        .unwrap_or_else(|error| panic!("room send should succeed: {error}"));

        std::assert_matches!(
            rx.room_rx.try_recv(),
            Ok(AccountRoomEvent {
                account_user_id: Some(account_user_id),
                account_epoch: 1,
                event: RoomEvent::Error { .. },
                ..
            }) if account_user_id == "user-1"
        );
        assert!(rx.auth_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn auth_send_lands_on_auth_lane_only() {
        let (tx, mut rx) = runtime_event_channels();
        tx.send_auth(AuthEvent::Ready {
            user_id: Some("user-1".to_owned()),
            account_epoch: 1,
        })
        .await
        .unwrap_or_else(|error| panic!("auth send should succeed: {error}"));

        std::assert_matches!(rx.auth_rx.try_recv(), Ok(AuthEvent::Ready { .. }));
        assert!(rx.terminal_control_rx.try_recv().is_err());
        assert!(rx.system_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn agent_intel_send_lands_on_agent_intel_lane_only() {
        let (tx, mut rx) = runtime_event_channels();
        tx.send_agent_intel(
            Some("account-a".to_owned()),
            7,
            AgentIntelEvent::Cleared {
                session_id: "session-1".to_owned(),
                session_incarnation_id: "incarnation-1".to_owned(),
            },
        )
        .await
        .unwrap_or_else(|error| panic!("agent intel send should succeed: {error}"));

        let envelope = rx.agent_intel_rx.try_recv().expect("agent intel envelope");
        assert_eq!(envelope.account_user_id.as_deref(), Some("account-a"));
        assert_eq!(envelope.account_epoch, 7);
        std::assert_matches!(envelope.event, AgentIntelEvent::Cleared { .. });
        assert!(rx.terminal_control_rx.try_recv().is_err());
        assert!(rx.system_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn closed_receiver_surfaces_channel_closed_on_terminal_control() {
        let (tx, rx) = runtime_event_channels();
        drop(rx);
        let outcome = tx
            .send_terminal_control(TerminalEvent::Notification {
                session_id: "s".to_owned(),
                title: None,
                body: None,
            })
            .await;
        std::assert_matches!(
                outcome,
                Err(AppError::ChannelClosed { ref session })
                    if session == "runtime.terminal_control"
            ,
            "expected ChannelClosed, got {outcome:?}"
        );
    }

    #[test]
    fn try_send_heartbeat_returns_false_after_close() {
        let (tx, rx) = runtime_event_channels();
        assert!(tx.try_send_heartbeat());
        drop(rx);
        assert!(!tx.try_send_heartbeat());
    }
}
