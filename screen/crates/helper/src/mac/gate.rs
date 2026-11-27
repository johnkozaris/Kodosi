//! Whom Kodosi Screen serves. A host of Kodosi's team can run with any data folder and sign in to
//! any account, so a Kodosi Screen of the team serves the host's account only after the person at
//! the Mac allowed that account in Kodosi Screen's own alert. Any program can start such a host
//! and have it ask, but no program can click for the person, so the alert shows only when
//! Kodosi's app vouches for their click on its switch. Without a team macOS grants Kodosi Screen
//! nothing of a released helper, so it serves every account.

use screen::link::ToHelper;

pub(super) struct Gate {
    guarded: bool,
    /// The key of the host's account, once the host named it.
    owner: Option<String>,
    allowed: bool,
    /// The host's asks that wait for the alert that shows.
    waiting: usize,
    /// Numbers the alerts, so that the answer to one that the sharing's end overtook allows
    /// nothing.
    alert: u64,
}

/// What the helper does with an ask of the host.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Step {
    Answer(bool),
    Show(u64),
    /// The alert that shows answers this ask too.
    Join,
}

impl Gate {
    pub(super) const fn new(guarded: bool) -> Self {
        Self {
            guarded,
            owner: None,
            allowed: false,
            waiting: 0,
            alert: 0,
        }
    }

    pub(super) const fn serves(&self) -> bool {
        !self.guarded || self.allowed
    }

    /// Until it serves the host, the helper only says what macOS lets it do and whether the
    /// display is away: it lists no screens or windows, captures nothing, plays no input, leaves
    /// the clipboard alone and asks macOS for nothing.
    pub(super) const fn admits(&self, said: &ToHelper) -> bool {
        self.serves()
            || matches!(
                said,
                ToHelper::Owner { .. }
                    | ToHelper::Allow { .. }
                    | ToHelper::Forget
                    | ToHelper::Stop
                    | ToHelper::Quit
            )
    }

    /// Takes the host's account, which `known` says whether the person allowed before, and
    /// returns that answer for the host. A link has one account, so a second one changes nothing.
    pub(super) fn own(&mut self, key: String, known: impl FnOnce(&str) -> bool) -> Option<bool> {
        if self.owner.is_some() {
            return None;
        }
        self.allowed = !self.guarded || known(&key);
        self.owner = Some(key);
        Some(self.allowed)
    }

    /// One alert at a time: an ask while it shows waits for its answer. An alert shows only when
    /// `vouched` took the vouch of Kodosi's app for the person's click; without it the answer is
    /// no. An account that the person allowed before needs none.
    pub(super) fn ask(&mut self, vouched: impl FnOnce() -> bool) -> Step {
        if self.serves() {
            Step::Answer(true)
        } else if self.owner.is_none() {
            Step::Answer(false)
        } else if self.waiting > 0 {
            self.waiting += 1;
            Step::Join
        } else if vouched() {
            self.waiting = 1;
            self.alert += 1;
            Step::Show(self.alert)
        } else {
            Step::Answer(false)
        }
    }

    /// The account that alert `alert` asks about, while its answer still counts.
    pub(super) fn standing(&self, alert: u64) -> Option<&str> {
        self.owner
            .as_deref()
            .filter(|_| alert == self.alert && self.waiting > 0)
    }

    /// Takes the answer to the alert that stands, and returns how many asks it answers.
    pub(super) const fn settle(&mut self, allowed: bool) -> usize {
        self.allowed = allowed || !self.guarded;
        let waiting = self.waiting;
        self.waiting = 0;
        waiting
    }

    /// The sharing went off, so no account is allowed and the asks that wait hear no. Returns how
    /// many asks that answers.
    pub(super) const fn forget(&mut self) -> usize {
        self.allowed = !self.guarded;
        self.alert += 1;
        let waiting = self.waiting;
        self.waiting = 0;
        waiting
    }
}

#[cfg(test)]
mod tests {
    use stream::viewer::Input;

    use super::*;

    fn serving() -> [ToHelper; 4] {
        [
            ToHelper::Input {
                input: Input::Text {
                    text: "rm -rf ~".to_owned(),
                },
            },
            ToHelper::Clipboard {
                text: "secret".to_owned(),
            },
            ToHelper::Ask,
            ToHelper::Notice { notice: None },
        ]
    }

    #[test]
    fn a_guarded_helper_serves_the_host_only_after_the_person_allowed_its_account() {
        let mut gate = Gate::new(true);
        assert!(serving().iter().all(|said| !gate.admits(said)));
        assert_eq!(gate.own("maya".to_owned(), |_| false), Some(false));
        assert!(serving().iter().all(|said| !gate.admits(said)));
        let Step::Show(alert) = gate.ask(|| true) else {
            panic!("no alert");
        };
        assert_eq!(gate.standing(alert), Some("maya"));
        assert_eq!(gate.settle(true), 1);
        assert!(serving().iter().all(|said| gate.admits(said)));
    }

    #[test]
    fn only_the_vouch_of_kodosis_app_for_a_click_shows_the_alert() {
        let mut gate = Gate::new(true);
        gate.own("maya".to_owned(), |_| false);
        assert_eq!(gate.ask(|| false), Step::Answer(false));
        assert!(!gate.serves());
        assert!(matches!(gate.ask(|| true), Step::Show(_)));
        let mut allowed = Gate::new(true);
        allowed.own("maya".to_owned(), |_| true);
        assert_eq!(
            allowed.ask(|| panic!("an allowed account needs no vouch")),
            Step::Answer(true)
        );
    }

    #[test]
    fn an_ask_while_the_alert_shows_joins_it() {
        let mut gate = Gate::new(true);
        gate.own("maya".to_owned(), |_| false);
        assert!(matches!(gate.ask(|| true), Step::Show(_)));
        assert_eq!(gate.ask(|| true), Step::Join);
        assert_eq!(gate.settle(true), 2);
        assert_eq!(gate.ask(|| true), Step::Answer(true));
    }

    #[test]
    fn the_end_of_the_sharing_takes_the_answer_back_also_from_the_alert_that_shows() {
        let mut gate = Gate::new(true);
        gate.own("maya".to_owned(), |key| key == "maya");
        assert!(gate.serves());
        assert_eq!(gate.forget(), 0);
        assert!(!gate.serves());
        let Step::Show(alert) = gate.ask(|| true) else {
            panic!("no alert");
        };
        assert_eq!(gate.forget(), 1);
        assert_eq!(gate.standing(alert), None);
        assert!(!gate.serves());
    }

    #[test]
    fn a_helper_without_a_team_serves_every_account_and_asks_nothing() {
        let mut gate = Gate::new(false);
        assert!(serving().iter().all(|said| gate.admits(said)));
        assert_eq!(gate.ask(|| true), Step::Answer(true));
        assert_eq!(gate.own("ana".to_owned(), |_| false), Some(true));
    }
}
