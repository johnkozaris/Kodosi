use std::collections::BTreeSet;

use tokio::{sync::broadcast, time};
use tokio_util::sync::CancellationToken;

use crate::{
    AgentGlobalEvent, AgentIntelEvent, AuthEvent, DeviceEvent, FriendsEvent, HostEvent, RoomEvent,
    SessionEvent, SystemEvent, TerminalEvent, TrustEvent,
    runtime_event_bus::{RuntimeEventLane, RuntimeEventLaneMask},
    shutdown,
};

use super::server::HeadlessHostActivity;

async fn drain_window_closed(deadline: Option<time::Instant>) {
    match deadline {
        Some(deadline) => time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

fn lanes_are_drained(rx: &crate::RuntimeEventReceivers) -> bool {
    rx.terminal_control_rx.is_empty()
        && rx.system_rx.is_empty()
        && rx.friends_rx.is_empty()
        && rx.devices_rx.is_empty()
        && rx.trust_rx.is_empty()
        && rx.room_rx.is_empty()
        && rx.auth_rx.is_empty()
        && rx.sessions_rx.is_empty()
        && rx.agent_intel_rx.is_empty()
        && rx.agent_global_rx.is_empty()
}

#[allow(
    clippy::too_many_lines,
    reason = "single per-lane select dispatch — splitting per lane would obscure the pattern"
)]
pub(in crate::headless_host) async fn dispatch_runtime_events(
    mut runtime_event_rx: crate::RuntimeEventReceivers,
    events_tx: broadcast::Sender<HostEvent>,
    activity_tx: tokio::sync::watch::Sender<HeadlessHostActivity>,
    shutdown_token: CancellationToken,
    runtime_finished: CancellationToken,
) {
    let mut done_lanes = RuntimeEventLaneMask::empty();
    let mut active_local_sessions = BTreeSet::new();
    let mut active_remote_relays = BTreeSet::new();
    let mut drain_deadline: Option<time::Instant> = None;
    let mut runtime_done = false;

    loop {
        if done_lanes.is_all_done() || (runtime_done && lanes_are_drained(&runtime_event_rx)) {
            break;
        }

        tokio::select! {
            biased;





            () = shutdown_token.cancelled(), if drain_deadline.is_none() => {
                drain_deadline = Some(time::Instant::now() + shutdown::HOST_SHUTDOWN_BUDGET);
            },


            () = runtime_finished.cancelled(), if !runtime_done => runtime_done = true,
            () = drain_window_closed(drain_deadline) => break,
            maybe = runtime_event_rx.system_rx.recv(), if !done_lanes.contains(RuntimeEventLane::System) => {
                match maybe {
                    Some(message) => publish_headless_system_event(
                        &mut active_local_sessions,
                        &mut active_remote_relays,
                        &events_tx,
                        &activity_tx,
                        message,
                    ),
                    None => done_lanes.mark_done(RuntimeEventLane::System),
                }
            },
            maybe = runtime_event_rx.terminal_control_rx.recv(), if !done_lanes.contains(RuntimeEventLane::TerminalControl) => {
                match maybe {
                    Some(message) => publish_headless_terminal_event(
                        &mut active_local_sessions,
                        &mut active_remote_relays,
                        &events_tx,
                        &activity_tx,
                        message,
                    ),
                    None => done_lanes.mark_done(RuntimeEventLane::TerminalControl),
                }
            },
            maybe = runtime_event_rx.auth_rx.recv(), if !done_lanes.contains(RuntimeEventLane::Auth) => {
                match maybe {
                    Some(event) => publish_headless_auth_event(&events_tx, &activity_tx, event),
                    None => done_lanes.mark_done(RuntimeEventLane::Auth),
                }
            },
            maybe = runtime_event_rx.sessions_rx.recv(), if !done_lanes.contains(RuntimeEventLane::Sessions) => {
                match maybe {
                    Some(event) => publish_headless_session_event(
                        &mut active_local_sessions,
                        &mut active_remote_relays,
                        &events_tx,
                        &activity_tx,
                        event.event,
                    ),
                    None => done_lanes.mark_done(RuntimeEventLane::Sessions),
                }
            },
            maybe = runtime_event_rx.agent_intel_rx.recv(), if !done_lanes.contains(RuntimeEventLane::AgentIntel) => {
                match maybe {
                    Some(event) => publish_headless_agent_intel_event(&events_tx, event.event),
                    None => done_lanes.mark_done(RuntimeEventLane::AgentIntel),
                }
            },
            maybe = runtime_event_rx.agent_global_rx.recv(), if !done_lanes.contains(RuntimeEventLane::AgentGlobal) => {
                match maybe {
                    Some(event) => publish_headless_agent_global_event(&events_tx, event),
                    None => done_lanes.mark_done(RuntimeEventLane::AgentGlobal),
                }
            },
            maybe = runtime_event_rx.friends_rx.recv(), if !done_lanes.contains(RuntimeEventLane::Friends) => {
                match maybe {
                    Some(event) => publish_headless_friends_event(&events_tx, event.event),
                    None => done_lanes.mark_done(RuntimeEventLane::Friends),
                }
            },
            maybe = runtime_event_rx.devices_rx.recv(), if !done_lanes.contains(RuntimeEventLane::Devices) => {
                match maybe {
                    Some(event) => publish_headless_devices_event(&events_tx, &activity_tx, event.event),
                    None => done_lanes.mark_done(RuntimeEventLane::Devices),
                }
            },
            maybe = runtime_event_rx.trust_rx.recv(), if !done_lanes.contains(RuntimeEventLane::Trust) => {
                match maybe {
                    Some(event) => publish_headless_trust_event(&events_tx, event.event),
                    None => done_lanes.mark_done(RuntimeEventLane::Trust),
                }
            },
            maybe = runtime_event_rx.room_rx.recv(), if !done_lanes.contains(RuntimeEventLane::Room) => {
                match maybe {
                    Some(event) => publish_headless_room_event(&events_tx, event.event),
                    None => done_lanes.mark_done(RuntimeEventLane::Room),
                }
            },
        }
    }

    shutdown_token.cancel();
}

fn publish_headless_terminal_event(
    _active_local_sessions: &mut BTreeSet<String>,
    _active_remote_relays: &mut BTreeSet<String>,
    events_tx: &broadcast::Sender<HostEvent>,
    _activity_tx: &tokio::sync::watch::Sender<HeadlessHostActivity>,
    message: TerminalEvent,
) {
    drop(events_tx.send(HostEvent::Terminal(message)));
}

fn publish_headless_system_event(
    _active_local_sessions: &mut BTreeSet<String>,
    _active_remote_relays: &mut BTreeSet<String>,
    events_tx: &broadcast::Sender<HostEvent>,
    _activity_tx: &tokio::sync::watch::Sender<HeadlessHostActivity>,
    message: SystemEvent,
) {
    drop(events_tx.send(HostEvent::System(message)));
}

fn publish_headless_auth_event(
    events_tx: &broadcast::Sender<HostEvent>,
    activity_tx: &tokio::sync::watch::Sender<HeadlessHostActivity>,
    message: AuthEvent,
) {
    match &message {
        AuthEvent::DeviceCode { .. } | AuthEvent::Finalizing => {
            activity_tx.send_modify(|activity| activity.pending_auth = true);
        }
        AuthEvent::Ready { .. } | AuthEvent::Required { .. } | AuthEvent::Error { .. } => {
            activity_tx.send_modify(|activity| activity.pending_auth = false);
        }
        AuthEvent::Notice { .. } | AuthEvent::IdentityHealth { .. } => {}
    }
    drop(events_tx.send(HostEvent::Auth(message)));
}

fn publish_headless_friends_event(events_tx: &broadcast::Sender<HostEvent>, message: FriendsEvent) {
    drop(events_tx.send(HostEvent::Friends(message)));
}

fn publish_headless_devices_event(
    events_tx: &broadcast::Sender<HostEvent>,
    activity_tx: &tokio::sync::watch::Sender<HeadlessHostActivity>,
    message: DeviceEvent,
) {
    match &message {
        DeviceEvent::LinkSelfPending { .. } => {
            activity_tx.send_modify(|activity| activity.pending_device_link = true);
        }
        DeviceEvent::LinkSelfResolved { .. } => {
            activity_tx.send_modify(|activity| activity.pending_device_link = false);
        }
        DeviceEvent::List { .. }
        | DeviceEvent::LinkSnapshot { .. }
        | DeviceEvent::LinkRequested { .. }
        | DeviceEvent::LinkResolved { .. }
        | DeviceEvent::Error { .. } => {}
    }
    drop(events_tx.send(HostEvent::Devices(message)));
}

fn publish_headless_trust_event(events_tx: &broadcast::Sender<HostEvent>, message: TrustEvent) {
    drop(events_tx.send(HostEvent::Trust(message)));
}

fn publish_headless_room_event(events_tx: &broadcast::Sender<HostEvent>, message: RoomEvent) {
    drop(events_tx.send(HostEvent::Room(message)));
}

fn publish_headless_session_event(
    active_local_sessions: &mut BTreeSet<String>,
    active_remote_relays: &mut BTreeSet<String>,
    events_tx: &broadcast::Sender<HostEvent>,
    activity_tx: &tokio::sync::watch::Sender<HeadlessHostActivity>,
    message: SessionEvent,
) {
    if apply_active_session_event(active_local_sessions, active_remote_relays, &message) {
        activity_tx.send_modify(|activity| {
            activity.active_local_sessions = active_local_sessions.len();
            activity.active_remote_relays = active_remote_relays.len();
        });
    }
    drop(events_tx.send(HostEvent::Session(message)));
}

fn publish_headless_agent_intel_event(
    events_tx: &broadcast::Sender<HostEvent>,
    message: AgentIntelEvent,
) {
    drop(events_tx.send(HostEvent::AgentIntel(message)));
}

fn publish_headless_agent_global_event(
    events_tx: &broadcast::Sender<HostEvent>,
    message: AgentGlobalEvent,
) {
    drop(events_tx.send(HostEvent::AgentGlobal(message)));
}

pub(in crate::headless_host) fn apply_active_session_event(
    active_local_sessions: &mut BTreeSet<String>,
    active_remote_relays: &mut BTreeSet<String>,
    message: &SessionEvent,
) -> bool {
    match message {
        SessionEvent::List { sessions } => {
            active_local_sessions.clear();
            active_remote_relays.clear();
            for session in sessions {
                if session.is_active_local() {
                    active_local_sessions.insert(session.id().to_owned());
                }
                if session.has_active_remote_relay() {
                    active_remote_relays.insert(session.id().to_owned());
                }
            }
        }
        SessionEvent::Upsert { session } => {
            if session.is_local() {
                if session.is_active_local() {
                    active_local_sessions.insert(session.id().to_owned());
                } else {
                    active_local_sessions.remove(session.id());
                }
            } else if session.has_active_remote_relay() {
                active_remote_relays.insert(session.id().to_owned());
            } else {
                active_remote_relays.remove(session.id());
            }
        }
        SessionEvent::Removed { session_id } => {
            active_local_sessions.remove(session_id);
            active_remote_relays.remove(session_id);
        }
        _ => return false,
    }
    true
}
