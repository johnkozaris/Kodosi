use std::time::Duration;

use tokio::time::Instant;

use crate::{
    Event,
    protocol::{EventBody, SessionEntry},
};

/// An agent's spinner changes the title of its terminal several times a second, and an app reads
/// the whole list of terminals again for each list that it gets.
pub(super) const PACE: Duration = Duration::from_millis(250);

/// The list of terminals that the apps got last, and a newer one that waits for its moment.
#[derive(Default)]
pub(super) struct Catalog {
    sent: Option<Listing>,
    sent_at: Option<Instant>,
    waiting: Option<(Instant, Listing)>,
    asked: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Listing {
    pub(super) user: Option<String>,
    pub(super) epoch: u64,
    pub(super) sessions: Vec<SessionEntry>,
}

impl Catalog {
    /// An app that asks for the list waits for one, also when the list did not change.
    pub(super) fn ask(&mut self) {
        self.asked = true;
    }

    /// The list goes out at once only when no list went out a moment ago.
    pub(super) fn changed(&mut self, listing: Listing, now: Instant) -> Option<Event> {
        if let Some((_, waiting)) = &mut self.waiting {
            *waiting = listing;
            return None;
        }
        if let Some(due) = self.sent_at.map(|at| at + PACE).filter(|due| now < *due) {
            self.waiting = Some((due, listing));
            return None;
        }
        self.send(listing, now)
    }

    pub(super) fn due(&self) -> Option<Instant> {
        self.waiting.as_ref().map(|(due, _)| *due)
    }

    pub(super) fn take_due(&mut self, now: Instant) -> Option<Event> {
        self.due()
            .is_some_and(|due| due <= now)
            .then(|| self.take(now))
            .flatten()
    }

    pub(super) fn take(&mut self, now: Instant) -> Option<Event> {
        let (_, listing) = self.waiting.take()?;
        self.send(listing, now)
    }

    fn send(&mut self, listing: Listing, now: Instant) -> Option<Event> {
        if !self.asked && self.sent.as_ref() == Some(&listing) {
            return None;
        }
        self.asked = false;
        self.sent_at = Some(now);
        self.sent = Some(listing.clone());
        Some(listing.event())
    }
}

impl Listing {
    pub(super) fn event(self) -> Event {
        Event {
            account_user_id: self.user,
            account_epoch: self.epoch,
            event: EventBody::SessionsSnapshot {
                sessions: self.sessions,
            },
            recipient: None,
        }
    }
}
