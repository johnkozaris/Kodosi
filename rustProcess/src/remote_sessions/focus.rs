use std::collections::{HashMap, HashSet};

use kodosi_domain::ids::SessionId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusTransition {
    Send { focused: bool },
    Skip,
}

#[derive(Debug, Default)]
pub(crate) struct ClientFocusTracker {
    focused: HashMap<SessionId, HashSet<String>>,
}

impl ClientFocusTracker {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn note_focus(&mut self, session: SessionId, client_id: String) -> FocusTransition {
        let entry = self.focused.entry(session).or_default();
        let was_empty = entry.is_empty();
        let inserted = entry.insert(client_id);
        if was_empty && inserted {
            FocusTransition::Send { focused: true }
        } else {
            FocusTransition::Skip
        }
    }

    pub(crate) fn note_blur(&mut self, session: SessionId, client_id: &str) -> FocusTransition {
        let Some(entry) = self.focused.get_mut(&session) else {
            return FocusTransition::Skip;
        };
        let removed = entry.remove(client_id);
        if removed && entry.is_empty() {
            self.focused.remove(&session);
            FocusTransition::Send { focused: false }
        } else {
            FocusTransition::Skip
        }
    }

    pub(crate) fn clients(&self, session: SessionId) -> Vec<String> {
        self.focused
            .get(&session)
            .map(|clients| clients.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub(crate) fn forget(&mut self, session: SessionId) {
        self.focused.remove(&session);
    }
}

#[cfg(test)]
mod tests {
    use super::{ClientFocusTracker, FocusTransition};
    use kodosi_domain::ids::SessionId;

    #[test]
    fn first_focus_transitions_to_focused() {
        let mut tracker = ClientFocusTracker::new();
        let session = SessionId::new();

        let outcome = tracker.note_focus(session, "client-a".to_owned());

        assert_eq!(outcome, FocusTransition::Send { focused: true });
    }

    #[test]
    fn second_focus_does_not_emit() {
        let mut tracker = ClientFocusTracker::new();
        let session = SessionId::new();

        let _ = tracker.note_focus(session, "client-a".to_owned());
        let outcome = tracker.note_focus(session, "client-b".to_owned());

        assert_eq!(outcome, FocusTransition::Skip);
    }

    #[test]
    fn redundant_focus_is_idempotent() {
        let mut tracker = ClientFocusTracker::new();
        let session = SessionId::new();

        let _ = tracker.note_focus(session, "client-a".to_owned());
        let outcome = tracker.note_focus(session, "client-a".to_owned());

        assert_eq!(outcome, FocusTransition::Skip);
    }

    #[test]
    fn final_blur_transitions_to_blurred() {
        let mut tracker = ClientFocusTracker::new();
        let session = SessionId::new();

        let _ = tracker.note_focus(session, "client-a".to_owned());
        let outcome = tracker.note_blur(session, "client-a");

        assert_eq!(outcome, FocusTransition::Send { focused: false });
    }

    #[test]
    fn blur_with_other_client_focused_does_not_emit() {
        let mut tracker = ClientFocusTracker::new();
        let session = SessionId::new();

        let _ = tracker.note_focus(session, "client-a".to_owned());
        let _ = tracker.note_focus(session, "client-b".to_owned());
        let outcome = tracker.note_blur(session, "client-a");

        assert_eq!(outcome, FocusTransition::Skip);
    }

    #[test]
    fn blur_for_unknown_client_is_skip() {
        let mut tracker = ClientFocusTracker::new();
        let session = SessionId::new();

        let outcome = tracker.note_blur(session, "client-a");

        assert_eq!(outcome, FocusTransition::Skip);
    }

    #[test]
    fn blur_after_forget_is_skip() {
        let mut tracker = ClientFocusTracker::new();
        let session = SessionId::new();

        let _ = tracker.note_focus(session, "client-a".to_owned());
        tracker.forget(session);

        let outcome = tracker.note_blur(session, "client-a");
        assert_eq!(outcome, FocusTransition::Skip);
    }

    #[test]
    fn sessions_are_isolated() {
        let mut tracker = ClientFocusTracker::new();
        let s1 = SessionId::new();
        let s2 = SessionId::new();

        let _ = tracker.note_focus(s1, "client-a".to_owned());
        let outcome = tracker.note_focus(s2, "client-a".to_owned());

        assert_eq!(outcome, FocusTransition::Send { focused: true });
    }
}
