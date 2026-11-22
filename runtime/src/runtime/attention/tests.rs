use super::*;

const ID: &str = "11111111-1111-4111-8111-111111111111";

fn nobody() -> BTreeSet<String> {
    BTreeSet::new()
}

fn looking() -> BTreeSet<String> {
    BTreeSet::from([ID.to_owned()])
}

fn after(start: Instant, millis: u64) -> Instant {
    start + Duration::from_millis(millis)
}

#[test]
fn a_new_state_marks_its_terminal_once_it_lasted_two_seconds_without_a_look() {
    let mut attention = Attention::default();
    let start = Instant::now();
    attention.observe(ID, Some(Waiting::Done), true, false, start);
    assert!(!attention.settle(after(start, 5000), &nobody()));

    attention.observe(ID, Some(Waiting::Blocked), true, false, start);
    assert!(!attention.settle(after(start, 1999), &nobody()));
    assert!(attention.settle(after(start, 2000), &nobody()));
    assert!(attention.marked(ID));
    assert_eq!(
        attention.take_started(),
        [(ID.to_owned(), Waiting::Blocked)]
    );
    attention.observe(ID, Some(Waiting::Blocked), true, false, after(start, 3000));
    assert!(attention.marked(ID));
    attention.observe(ID, Some(Waiting::Error), true, false, after(start, 3000));
    assert!(!attention.marked(ID));

    attention.observe(ID, Some(Waiting::Done), true, false, after(start, 4000));
    attention.observe(ID, None, true, false, after(start, 5000));
    assert!(!attention.settle(after(start, 9000), &nobody()));
    assert_eq!(attention.take_started(), Vec::new());
}

#[test]
fn a_look_while_a_state_settles_or_after_its_mark_ends_the_call() {
    let mut attention = Attention::default();
    let start = Instant::now();
    attention.observe(ID, None, true, false, start);
    attention.observe(ID, Some(Waiting::Done), true, false, start);
    assert!(!attention.look(&looking()));
    assert!(!attention.settle(after(start, 3000), &nobody()));

    attention.observe(ID, Some(Waiting::Error), true, false, after(start, 3000));
    assert!(!attention.settle(after(start, 5000), &looking()));
    assert!(!attention.settle(after(start, 6000), &nobody()));

    attention.observe(ID, Some(Waiting::Blocked), true, false, after(start, 6000));
    assert!(attention.settle(after(start, 8000), &nobody()));
    assert!(attention.look(&looking()));
    assert!(!attention.marked(ID));

    attention.observe(ID, Some(Waiting::Done), false, false, after(start, 9000));
    assert!(!attention.settle(after(start, 12000), &nobody()));
}

#[cfg(feature = "host")]
#[test]
fn a_device_of_the_owner_that_saw_the_terminal_ends_its_mark() {
    let mut attention = Attention::default();
    let start = Instant::now();
    attention.observe(ID, None, true, false, start);
    attention.observe(ID, Some(Waiting::Done), true, false, start);
    assert!(attention.settle(after(start, 2000), &nobody()));
    assert_eq!(attention.marked_state(ID), Some(Waiting::Done));
    assert!(attention.seen_elsewhere(ID));
    assert_eq!(attention.marked_state(ID), None);

    attention.observe(ID, Some(Waiting::Blocked), true, false, after(start, 3000));
    assert!(!attention.seen_elsewhere(ID));
    assert!(!attention.settle(after(start, 6000), &nobody()));
}

fn present(attention: &mut Attention, presence: Presence) -> CancellationToken {
    let lifetime = CancellationToken::new();
    attention.present(Uuid::now_v7(), presence, lifetime.clone());
    lifetime
}

#[test]
fn the_device_is_where_its_most_present_app_is_and_away_without_one() {
    let mut attention = Attention::default();
    assert_eq!(attention.changed_presence(), None);
    let attached = Uuid::now_v7();
    assert!(attention.looks(&attached));
    let idle = present(&mut attention, Presence::Idle);
    let here = present(&mut attention, Presence::Here);
    let kodosi = present(&mut attention, Presence::Kodosi);
    let _away = present(&mut attention, Presence::Away);
    assert_eq!(attention.changed_presence(), Some(Presence::Kodosi));
    assert_eq!(attention.changed_presence(), None);
    kodosi.cancel();
    assert_eq!(attention.changed_presence(), Some(Presence::Here));
    here.cancel();
    assert_eq!(attention.changed_presence(), Some(Presence::Idle));
    idle.cancel();
    assert_eq!(attention.changed_presence(), Some(Presence::Away));

    let app = Uuid::now_v7();
    for presence in [Presence::Idle, Presence::Here] {
        attention.present(app, presence, CancellationToken::new());
        assert!(!attention.looks(&app));
    }
    attention.present(app, Presence::Kodosi, CancellationToken::new());
    assert!(attention.looks(&app));
    assert_eq!(attention.changed_presence(), Some(Presence::Kodosi));
}

fn waiting(attention: &mut Attention) {
    let blocked = TerminalStatus {
        state: Some(crate::terminal::ProgramState::Blocked),
        attention: true,
        ..TerminalStatus::default()
    };
    attention.report(ID, Some(&blocked), false);
}

fn calls(presence: Presence, elsewhere: Elsewhere) -> Option<Cue> {
    let mut attention = Attention::default();
    waiting(&mut attention);
    let _app = present(&mut attention, presence);
    attention.hear_elsewhere(elsewhere);
    attention.call(ID, Waiting::Blocked, Instant::now())
}

#[test]
fn a_call_shows_on_a_computer_where_its_person_can_be_and_quietly_where_nobody_is() {
    let nowhere = Elsewhere::default();
    let in_kodosi = Elsewhere {
        kodosi: true,
        here: false,
    };
    let at_a_computer = Elsewhere {
        kodosi: false,
        here: true,
    };
    assert_eq!(calls(Presence::Here, nowhere), Some(Cue::Sound));
    assert_eq!(calls(Presence::Here, at_a_computer), Some(Cue::Sound));
    assert_eq!(calls(Presence::Here, in_kodosi), None);
    assert_eq!(calls(Presence::Idle, nowhere), Some(Cue::Sound));
    assert_eq!(calls(Presence::Idle, at_a_computer), None);
    assert_eq!(calls(Presence::Idle, in_kodosi), None);
    assert_eq!(calls(Presence::Away, nowhere), Some(Cue::List));
    assert_eq!(calls(Presence::Away, at_a_computer), None);
    assert_eq!(calls(Presence::Away, in_kodosi), None);
    assert_eq!(calls(Presence::Kodosi, nowhere), None);
}

#[test]
fn only_a_wait_has_a_sound_and_at_most_once_in_thirty_seconds() {
    let mut attention = Attention::default();
    waiting(&mut attention);
    let app = Uuid::now_v7();
    attention.present(app, Presence::Here, CancellationToken::new());
    let start = Instant::now();
    assert_eq!(attention.call(ID, Waiting::Done, start), Some(Cue::Banner));
    assert_eq!(attention.call(ID, Waiting::Error, start), Some(Cue::Banner));
    assert_eq!(
        attention.call(ID, Waiting::Blocked, start),
        Some(Cue::Sound)
    );
    attention.present(app, Presence::Idle, CancellationToken::new());
    assert_eq!(
        attention.call(ID, Waiting::Blocked, after(start, 29_999)),
        Some(Cue::Banner)
    );
    assert_eq!(
        attention.call(ID, Waiting::Blocked, after(start, 30_000)),
        Some(Cue::Sound)
    );
    attention.report(ID, None, false);
    assert_eq!(
        attention.call(ID, Waiting::Blocked, after(start, 90_000)),
        None
    );
}

#[cfg(feature = "host")]
#[test]
fn a_programs_notification_sounds_where_its_person_can_be_and_only_lists_where_nobody_is() {
    let mut attention = Attention::default();
    let app = Uuid::now_v7();
    let start = Instant::now();
    let mut noted = |presence, elsewhere, millis| {
        attention.present(app, presence, CancellationToken::new());
        attention.hear_elsewhere(elsewhere);
        attention.notification_cue(after(start, millis))
    };
    let nowhere = Elsewhere::default();
    let at_a_computer = Elsewhere {
        kodosi: false,
        here: true,
    };
    assert_eq!(noted(Presence::Kodosi, nowhere, 0), Cue::Sound);
    assert_eq!(noted(Presence::Here, nowhere, 29_999), Cue::Banner);
    assert_eq!(noted(Presence::Here, at_a_computer, 30_000), Cue::Sound);
    assert_eq!(noted(Presence::Idle, nowhere, 60_000), Cue::Sound);
    assert_eq!(noted(Presence::Idle, at_a_computer, 90_000), Cue::List);
    assert_eq!(noted(Presence::Away, nowhere, 90_000), Cue::List);
}

fn starts(attention: &mut Attention, id: &str, start: Instant) {
    attention.observe(id, None, true, false, start);
    attention.observe(id, Some(Waiting::Blocked), true, false, start);
    assert!(attention.settle(after(start, 2000), &nobody()));
    attention.take_started();
}

#[test]
fn a_mark_calls_once_on_a_device_however_often_its_person_comes_and_goes() {
    let mut attention = Attention::default();
    let start = Instant::now();
    let (sign_only, told) = (ID, "told");
    let app = Uuid::now_v7();
    attention.present(app, Presence::Kodosi, CancellationToken::new());
    assert!(!attention.left());
    starts(&mut attention, sign_only, start);
    assert_eq!(attention.call(sign_only, Waiting::Blocked, start), None);
    attention.present(app, Presence::Here, CancellationToken::new());
    starts(&mut attention, told, after(start, 3000));
    assert_eq!(
        attention.call(told, Waiting::Blocked, after(start, 5000)),
        Some(Cue::Sound)
    );

    attention.present(app, Presence::Idle, CancellationToken::new());
    assert!(attention.left());
    assert_eq!(
        attention.call_waits(after(start, 40_000)),
        [(sign_only.to_owned(), Cue::Sound)]
    );
    for (back, away) in [
        (Presence::Here, Presence::Away),
        (Presence::Kodosi, Presence::Idle),
    ] {
        attention.present(app, back, CancellationToken::new());
        assert!(!attention.left());
        attention.present(app, away, CancellationToken::new());
        assert!(attention.left());
        assert_eq!(attention.call_waits(after(start, 80_000)), Vec::new());
    }
    assert!(attention.marked(sign_only) && attention.marked(told));
}

#[test]
fn a_wait_that_called_while_its_person_was_gone_does_not_call_again_when_the_person_leaves() {
    let mut attention = Attention::default();
    let start = Instant::now();
    let blocked = TerminalStatus {
        state: Some(crate::terminal::ProgramState::Blocked),
        attention: true,
        ..TerminalStatus::default()
    };
    attention.report("earlier", Some(&blocked), false);
    let app = Uuid::now_v7();
    attention.present(app, Presence::Here, CancellationToken::new());
    assert!(!attention.left());
    attention.present(app, Presence::Away, CancellationToken::new());
    attention.report(ID, Some(&blocked), true);
    assert_eq!(
        attention.take_started(),
        [(ID.to_owned(), Waiting::Blocked)]
    );
    assert_eq!(attention.call(ID, Waiting::Blocked, start), Some(Cue::List));
    assert!(attention.left());
    assert_eq!(
        attention.call_waits(start),
        [("earlier".to_owned(), Cue::List)]
    );
}

#[test]
fn a_device_that_loses_its_connection_does_not_count_its_person_as_gone() {
    let mut attention = Attention::default();
    attention.hear_elsewhere(Elsewhere {
        kodosi: true,
        here: false,
    });
    assert!(!attention.left());
    attention.lose_elsewhere();
    assert!(!attention.left());
}

#[test]
fn a_bell_or_a_stopped_spinner_marks_a_terminal_for_this_device_alone() {
    let mut attention = Attention::default();
    for (spinning, stopped) in [
        (Some(true), false),
        (None, false),
        (Some(true), false),
        (Some(false), true),
        (Some(false), false),
    ] {
        assert_eq!(attention.spinner_stopped(ID, spinning), stopped);
    }
    attention.mark_here(ID);
    assert!(attention.signed(ID));
    assert!(!attention.marked(ID));
    #[cfg(feature = "host")]
    assert_eq!(attention.marked_state(ID), None);
    assert!(attention.look(&looking()));
    assert!(!attention.signed(ID));
}

#[test]
fn a_mark_that_the_host_reports_calls_only_when_it_is_news() {
    let mut attention = Attention::default();
    let status = |state, attention| TerminalStatus {
        state: Some(state),
        attention,
        ..TerminalStatus::default()
    };
    let done = status(crate::terminal::ProgramState::Done, true);
    attention.report(ID, Some(&done), false);
    assert!(attention.marked(ID));
    assert_eq!(attention.take_started(), Vec::new());
    attention.report(ID, Some(&done), true);
    assert_eq!(attention.take_started(), Vec::new());
    let seen = status(crate::terminal::ProgramState::Done, false);
    attention.report(ID, Some(&seen), true);
    assert!(!attention.marked(ID));
    attention.report(ID, Some(&done), true);
    assert_eq!(attention.take_started(), [(ID.to_owned(), Waiting::Done)]);
    let blocked = status(crate::terminal::ProgramState::Blocked, true);
    attention.report(ID, Some(&blocked), true);
    assert_eq!(
        attention.take_started(),
        [(ID.to_owned(), Waiting::Blocked)]
    );
}

#[test]
fn a_look_at_a_reported_mark_tells_its_host_again_until_the_mark_ends() {
    let mut attention = Attention::default();
    let start = Instant::now();
    let done = TerminalStatus {
        state: Some(crate::terminal::ProgramState::Done),
        attention: true,
        ..TerminalStatus::default()
    };
    attention.report(ID, Some(&done), true);
    assert_eq!(attention.to_tell(&nobody(), start), Vec::<String>::new());
    assert_eq!(attention.to_tell(&looking(), start), [ID]);
    attention.told(ID, start);
    attention.report(ID, Some(&done), true);
    assert_eq!(
        attention.to_tell(&looking(), after(start, 9_999)),
        Vec::<String>::new()
    );
    assert_eq!(attention.to_tell(&looking(), after(start, 10_000)), [ID]);
    attention.report(ID, None, true);
    assert_eq!(
        attention.to_tell(&looking(), after(start, 20_000)),
        Vec::<String>::new()
    );
}
