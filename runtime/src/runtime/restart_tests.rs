use super::*;

#[test]
fn an_agent_back_from_an_update_goes_on_only_once_it_rests_and_takes_messages() {
    assert_eq!(next(None, false, false), Next::Wait);
    assert_eq!(next(Some(ProgramState::Blocked), false, false), Next::Wait);
    assert_eq!(next(Some(ProgramState::Idle), false, false), Next::Wait);
    assert_eq!(next(Some(ProgramState::Idle), true, false), Next::GoOn);
    assert_eq!(next(Some(ProgramState::Done), true, false), Next::GoOn);
}

#[test]
fn an_agent_that_works_while_it_starts_still_goes_on_once_it_rests() {
    assert_eq!(next(Some(ProgramState::Working), true, false), Next::Wait);
    assert_eq!(next(Some(ProgramState::Idle), true, false), Next::GoOn);
}

#[test]
fn an_agent_that_goes_on_by_itself_or_fails_keeps_its_own_course() {
    assert_eq!(next(Some(ProgramState::Working), true, true), Next::Forget);
    assert_eq!(next(Some(ProgramState::Error), true, false), Next::Forget);
}

#[test]
fn an_agent_that_never_gets_ready_offers_the_key_instead() {
    assert_eq!(next(None, false, true), Next::Offer);
    assert_eq!(next(Some(ProgramState::Blocked), false, true), Next::Offer);
}
