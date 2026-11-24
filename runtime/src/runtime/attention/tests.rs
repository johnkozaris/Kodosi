use super::*;

const ID: &str = "11111111-1111-4111-8111-111111111111";

fn nobody() -> BTreeSet<String> {
    BTreeSet::new()
}

fn after(start: Instant, millis: u64) -> Instant {
    start + Duration::from_millis(millis)
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
