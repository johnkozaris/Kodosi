use std::{
    collections::{BTreeSet, HashMap, HashSet},
    time::Duration,
};

use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    network::{Elsewhere, TerminalStatus},
    protocol::{Cue, Presence},
    terminal::Waiting,
};

/// An agent that asks and goes on by itself within this time does not need its person.
const SETTLE: Duration = Duration::from_secs(2);
const QUIET_AFTER_SOUND: Duration = Duration::from_secs(30);
/// A lost frame must not keep a mark that its person saw.
const SEEN_AGAIN: Duration = Duration::from_secs(10);

#[derive(Default)]
pub(super) struct Attention {
    kept: HashMap<String, Kept>,
    reported: HashMap<String, Reported>,
    /// A bell, a title spinner that stops or a program's own notification marks a terminal for
    /// the apps of this device only: the status carries only the mark of a state.
    local: HashSet<String>,
    spinning: HashMap<String, bool>,
    started: Vec<(String, Waiting)>,
    clients: HashMap<Uuid, Client>,
    presence: Presence,
    elsewhere: Elsewhere,
    present: bool,
    sounded: Option<Instant>,
    listed: Option<u64>,
}

/// A mark that this runtime keeps: for the terminals that it hosts, and for friends' terminals.
struct Kept {
    wait: Option<Waiting>,
    since: Instant,
    look: Look,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Look {
    /// The state was there when this runtime first saw the terminal, or came while the terminal
    /// did not call this device.
    Quiet,
    Settling,
    Seen,
    Marked {
        called: bool,
    },
}

impl Kept {
    const fn marked(&self) -> bool {
        matches!(self.look, Look::Marked { .. })
    }

    fn end_by_look(&mut self) -> bool {
        matches!(
            std::mem::replace(&mut self.look, Look::Seen),
            Look::Marked { .. }
        )
    }
}

/// A mark that the host of one of the person's terminals reports.
struct Reported {
    wait: Option<Waiting>,
    marked: bool,
    told: Option<Instant>,
    called: bool,
}

struct Client {
    presence: Presence,
    lifetime: CancellationToken,
}

impl Attention {
    pub(super) fn marked(&self, id: &str) -> bool {
        self.kept.get(id).is_some_and(Kept::marked)
            || self
                .reported
                .get(id)
                .is_some_and(|reported| reported.marked)
    }

    pub(super) fn signed(&self, id: &str) -> bool {
        self.marked(id) || self.local.contains(id)
    }

    #[cfg(feature = "host")]
    pub(super) fn marked_state(&self, id: &str) -> Option<Waiting> {
        self.kept
            .get(id)
            .filter(|kept| kept.marked())
            .and_then(|kept| kept.wait)
    }

    pub(super) fn observe(
        &mut self,
        id: &str,
        wait: Option<Waiting>,
        calls: bool,
        watched: bool,
        now: Instant,
    ) {
        match self.kept.get_mut(id) {
            Some(kept) if kept.wait == wait => {}
            Some(kept) => {
                let look = if wait.is_none() || !calls {
                    Look::Quiet
                } else if watched {
                    Look::Seen
                } else {
                    Look::Settling
                };
                *kept = Kept {
                    wait,
                    since: now,
                    look,
                };
            }
            None => {
                self.kept.insert(
                    id.to_owned(),
                    Kept {
                        wait,
                        since: now,
                        look: Look::Quiet,
                    },
                );
            }
        }
    }

    pub(super) fn settle(&mut self, now: Instant, watched: &BTreeSet<String>) -> bool {
        let mut started = false;
        for (id, kept) in &mut self.kept {
            let Some(wait) = kept.wait.filter(|_| kept.look == Look::Settling) else {
                continue;
            };
            if now.saturating_duration_since(kept.since) < SETTLE {
                continue;
            }
            if watched.contains(id) {
                kept.look = Look::Seen;
            } else {
                kept.look = Look::Marked { called: false };
                self.started.push((id.clone(), wait));
                started = true;
            }
        }
        started
    }

    pub(super) fn look(&mut self, watched: &BTreeSet<String>) -> bool {
        let mut ended = false;
        for id in watched {
            if let Some(kept) = self.kept.get_mut(id) {
                ended |= kept.end_by_look();
            }
            ended |= self.local.remove(id);
        }
        ended
    }

    #[cfg(feature = "host")]
    pub(super) fn seen_elsewhere(&mut self, id: &str) -> bool {
        self.kept.get_mut(id).is_some_and(Kept::end_by_look)
    }

    pub(super) fn mark_here(&mut self, id: &str) {
        self.local.insert(id.to_owned());
    }

    /// `spinning` is absent while the title of a program without a state cannot be read, so a
    /// terminal that comes back with a still title did not stop.
    pub(super) fn spinner_stopped(&mut self, id: &str, spinning: Option<bool>) -> bool {
        let Some(spinning) = spinning else {
            return false;
        };
        let before = self.spinning.insert(id.to_owned(), spinning);
        before == Some(true) && !spinning
    }

    /// A list of the terminals is news only when an earlier list came on the same device
    /// connection: the first list after a start or a reconnect holds the marks that were made
    /// while this runtime did not hear.
    pub(super) fn listed(&mut self, connects: u64) -> bool {
        self.listed.replace(connects) == Some(connects)
    }

    pub(super) fn hears_live(&self, connects: u64) -> bool {
        self.listed == Some(connects)
    }

    /// The caller leaves out the reports of a host that is not connected, so a terminal that comes
    /// back keeps its mark and makes no new one.
    pub(super) fn report(&mut self, id: &str, status: Option<&TerminalStatus>, live: bool) {
        let wait = status.and_then(TerminalStatus::waiting);
        let marked = wait.is_some() && status.is_some_and(|status| status.attention);
        let before = self
            .reported
            .get(id)
            .filter(|before| marked && before.marked && before.wait == wait);
        let (told, called) = before.map_or((None, false), |before| (before.told, before.called));
        if let Some(wait) = wait.filter(|_| marked && before.is_none() && live) {
            self.started.push((id.to_owned(), wait));
        }
        self.reported.insert(
            id.to_owned(),
            Reported {
                wait,
                marked,
                told,
                called,
            },
        );
    }

    pub(super) fn to_tell(&self, watched: &BTreeSet<String>, now: Instant) -> Vec<String> {
        watched
            .iter()
            .filter(|id| {
                self.reported.get(*id).is_some_and(|reported| {
                    reported.marked
                        && reported
                            .told
                            .is_none_or(|at| now.saturating_duration_since(at) >= SEEN_AGAIN)
                })
            })
            .cloned()
            .collect()
    }

    pub(super) fn told(&mut self, id: &str, now: Instant) {
        if let Some(reported) = self.reported.get_mut(id) {
            reported.told = Some(now);
        }
    }

    /// A mark that cannot be told yet waits for its terminal, not for a time.
    pub(super) fn due(
        &self,
        watched: &BTreeSet<String>,
        can_tell: impl Fn(&str) -> bool,
    ) -> Option<Instant> {
        let settles = self
            .kept
            .values()
            .filter(|kept| kept.wait.is_some() && kept.look == Look::Settling)
            .map(|kept| kept.since + SETTLE);
        let told_again = watched
            .iter()
            .filter(|id| can_tell(id))
            .filter_map(|id| self.reported.get(id))
            .filter(|reported| reported.marked)
            .filter_map(|reported| reported.told)
            .map(|told| told + SEEN_AGAIN);
        settles.chain(told_again).min()
    }

    pub(super) fn retain(&mut self, ids: &BTreeSet<String>) {
        self.kept.retain(|id, _| ids.contains(id));
        self.reported.retain(|id, _| ids.contains(id));
        self.local.retain(|id| ids.contains(id));
        self.spinning.retain(|id, _| ids.contains(id));
    }

    pub(super) fn take_started(&mut self) -> Vec<(String, Waiting)> {
        std::mem::take(&mut self.started)
    }

    pub(super) fn present(
        &mut self,
        client: Uuid,
        presence: Presence,
        lifetime: CancellationToken,
    ) {
        self.clients.insert(client, Client { presence, lifetime });
    }

    /// A client that never says how its person uses the device, such as an attached command,
    /// has a person who looks at the terminal.
    pub(super) fn looks(&self, client: &Uuid) -> bool {
        self.clients
            .get(client)
            .is_none_or(|client| client.presence == Presence::Kodosi)
    }

    fn device(&self) -> Presence {
        self.clients
            .values()
            .filter(|client| !client.lifetime.is_cancelled())
            .map(|client| client.presence)
            .max()
            .unwrap_or_default()
    }

    pub(super) fn changed_presence(&mut self) -> Option<Presence> {
        self.clients
            .retain(|_, client| !client.lifetime.is_cancelled());
        let presence = self.device();
        (std::mem::replace(&mut self.presence, presence) != presence).then_some(presence)
    }

    fn person_present(&self) -> bool {
        matches!(self.device(), Presence::Kodosi | Presence::Here)
            || self.elsewhere.kodosi
            || self.elsewhere.here
    }

    /// Whether the person stopped being present since the last call.
    pub(super) fn left(&mut self) -> bool {
        let present = self.person_present();
        let was = std::mem::replace(&mut self.present, present);
        was && !present
    }

    pub(super) fn hear_elsewhere(&mut self, elsewhere: Elsewhere) {
        self.elsewhere = elsewhere;
    }

    /// A device that lost its connection hears nothing about the others, and that is no change
    /// of the devices that the person uses.
    pub(super) fn lose_elsewhere(&mut self) {
        self.elsewhere = Elsewhere::default();
        self.present = self.person_present();
    }

    pub(super) fn forget_account(&mut self) {
        self.lose_elsewhere();
        self.listed = None;
    }

    /// A wait that showed only its sign while its person used Kodosi calls when the person leaves.
    pub(super) fn call_waits(&mut self, now: Instant) -> Vec<(String, Cue)> {
        let Some(can_be_here) = self.shows() else {
            return Vec::new();
        };
        let blocked = Some(Waiting::Blocked);
        let mut waits = Vec::new();
        for (id, kept) in &mut self.kept {
            if kept.wait == blocked && kept.look == (Look::Marked { called: false }) {
                kept.look = Look::Marked { called: true };
                waits.push(id.clone());
            }
        }
        for (id, reported) in &mut self.reported {
            if reported.wait == blocked && reported.marked && !reported.called {
                reported.called = true;
                waits.push(id.clone());
            }
        }
        waits
            .into_iter()
            .map(|id| (id, self.cue(can_be_here, true, now)))
            .collect()
    }

    /// Whether this device shows a notification, and whether its person can be at it. A person
    /// in Kodosi on any device sees the sign there, and a person at another computer gets the
    /// notification there.
    fn shows(&self) -> Option<bool> {
        let nobody_elsewhere = !self.elsewhere.kodosi && !self.elsewhere.here;
        match self.device() {
            Presence::Here if !self.elsewhere.kodosi => Some(true),
            Presence::Idle if nobody_elsewhere => Some(true),
            Presence::Away if nobody_elsewhere => Some(false),
            _ => None,
        }
    }

    pub(super) fn call(&mut self, id: &str, wait: Waiting, now: Instant) -> Option<Cue> {
        if !self.marked(id) {
            return None;
        }
        let can_be_here = self.shows()?;
        if let Some(kept) = self.kept.get_mut(id).filter(|kept| kept.marked()) {
            kept.look = Look::Marked { called: true };
        }
        if let Some(reported) = self.reported.get_mut(id) {
            reported.called = true;
        }
        Some(self.cue(can_be_here, wait == Waiting::Blocked, now))
    }

    /// Only the device of its terminal has a program's own notification, so the devices that the
    /// person uses elsewhere do not take it from this one.
    #[cfg(feature = "host")]
    pub(super) fn notification_cue(&mut self, now: Instant) -> Option<Cue> {
        let can_be_here = match self.device() {
            Presence::Kodosi => return None,
            Presence::Here => true,
            Presence::Idle => !self.elsewhere.kodosi && !self.elsewhere.here,
            Presence::Away => false,
        };
        Some(self.cue(can_be_here, false, now))
    }

    fn cue(&mut self, can_be_here: bool, sounds: bool, now: Instant) -> Cue {
        if !can_be_here {
            return Cue::List;
        }
        let quiet = self
            .sounded
            .is_some_and(|at| now.saturating_duration_since(at) < QUIET_AFTER_SOUND);
        if !sounds || quiet {
            return Cue::Banner;
        }
        self.sounded = Some(now);
        Cue::Sound
    }
}

#[cfg(test)]
mod tests;
