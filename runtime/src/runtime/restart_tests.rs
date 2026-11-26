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

/// A terminal as its host keeps it: the host fills in `working` only on the copies it publishes.
fn kept(fields: serde_json::Value) -> SessionEntry {
    let mut entry = json!({
        "id": Uuid::nil(), "incarnationId": Uuid::nil(), "kind": "local", "name": "Fix login",
        "isOwner": true, "status": "running", "connectionState": "local", "prompt": false,
        "repository": false, "working": false, "sharedWith": [], "attention": false,
        "connectedUsers": [], "hidden": false, "look": "dark",
    });
    if let (Some(entry), serde_json::Value::Object(fields)) = (entry.as_object_mut(), fields) {
        entry.extend(fields);
    }
    serde_json::from_value(entry).unwrap()
}

#[test]
fn a_restart_is_quiet_only_while_no_task_runs_and_nobody_else_is_in() {
    let me = Some("ana");
    let resting = [
        kept(json!({"prompt": true, "connectedUsers": ["ana"]})),
        kept(json!({"program": "claude", "programStatus": {"state": "idle"}})),
    ];
    assert_eq!(cost(&resting, me), (0, false));
    for used in [
        json!({"program": "claude", "programStatus": {"state": "working"}}),
        json!({"program": "claude", "programStatus": {"state": "blocked", "kind": "permission"}}),
        json!({"program": "claude", "programStatus": {"state": "idle"}, "helpers": 1}),
        json!({"prompt": true, "connectedUsers": ["ana", "ben"]}),
    ] {
        assert_eq!(cost([&kept(used)], me), (0, true));
    }
    assert_eq!(cost([&kept(json!({"program": "sleep"}))], me), (1, false));
}
